use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use super::tool_inputs::{
    is_checkpoint_kind, is_checkpoint_validation_status, CheckpointValidationInput,
};
use super::tool_result::ToolResult;
use super::ToolRuntime;
use crate::action_sessions::secret_like_value;
use crate::projects::ProjectConfig;
use crate::workspace_checkpoint::{create_workspace_checkpoint, restore_workspace_checkpoint};

const CHECKPOINT_VERSION: u32 = 1;
const CHECKPOINT_ID_PREFIX: &str = "wc_ckpt_";
const DEFAULT_LIST_LIMIT: usize = 20;
const MAX_LIST_LIMIT: usize = 100;
const DEFAULT_CHECKPOINT_KIND: &str = "snapshot";
const DEFAULT_VALIDATION_STATUS: &str = "unknown";
const MAX_CHECKPOINT_LABELS: usize = 20;
const MAX_CHECKPOINT_LABEL_LEN: usize = 64;
const MAX_VALIDATION_COMMANDS: usize = 20;
const MAX_VALIDATION_COMMAND_LEN: usize = 200;
const MAX_VALIDATION_SUMMARY_LEN: usize = 500;

#[derive(Debug, Clone)]
pub(crate) struct CheckpointStore {
    state_dir: PathBuf,
}

impl CheckpointStore {
    pub(crate) fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn project_dir(&self, resolved_project: &str) -> PathBuf {
        self.state_dir
            .join("checkpoints")
            .join(safe_project_id(resolved_project))
    }

    fn checkpoint_path(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
    ) -> Result<PathBuf, String> {
        validate_checkpoint_id(checkpoint_id)?;
        Ok(self
            .project_dir(resolved_project)
            .join(format!("{checkpoint_id}.json")))
    }

    fn write(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
        checkpoint: &Value,
    ) -> Result<PathBuf, String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        let parent = path
            .parent()
            .ok_or_else(|| "checkpoint path has no parent".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create checkpoint dir: {err}"))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("checkpoint.json");
        let tmp = path.with_file_name(format!(
            ".{file_name}.tmp-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let data = serde_json::to_vec_pretty(checkpoint)
            .map_err(|err| format!("failed to serialize checkpoint: {err}"))?;
        fs::write(&tmp, data)
            .and_then(|_| fs::rename(&tmp, &path))
            .map_err(|err| {
                let _ = fs::remove_file(&tmp);
                format!("failed to write checkpoint: {err}")
            })?;
        Ok(path)
    }

    fn load(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
    ) -> Result<(Value, PathBuf), String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        let content =
            fs::read_to_string(&path).map_err(|err| format!("failed to read checkpoint: {err}"))?;
        let value: Value = serde_json::from_str(&content)
            .map_err(|err| format!("invalid checkpoint JSON: {err}"))?;
        Ok((value, path))
    }

    fn delete(&self, resolved_project: &str, checkpoint_id: &str) -> Result<PathBuf, String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        fs::remove_file(&path).map_err(|err| format!("failed to delete checkpoint: {err}"))?;
        Ok(path)
    }

    fn list(&self, resolved_project: &str, limit: usize) -> Result<Vec<(Value, PathBuf)>, String> {
        let dir = self.project_dir(resolved_project);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("failed to list checkpoints: {err}")),
        };
        let mut checkpoints = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if validate_checkpoint_id(stem).is_err() {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            checkpoints.push((value, path));
        }
        checkpoints.sort_by(|(a, _), (b, _)| {
            let a_time = a
                .get("created_at")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let b_time = b
                .get("created_at")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            b_time
                .cmp(&a_time)
                .then_with(|| checkpoint_id_of(b).cmp(&checkpoint_id_of(a)))
        });
        checkpoints.truncate(limit);
        Ok(checkpoints)
    }
}

impl Default for CheckpointStore {
    fn default() -> Self {
        Self::new(crate::config::runtime_state_dir())
    }
}

impl ToolRuntime {
    pub fn with_checkpoint_state_dir(mut self, state_dir: impl Into<PathBuf>) -> Self {
        self.checkpoint_store = CheckpointStore::new(state_dir);
        self
    }

    #[cfg(test)]
    pub(crate) fn checkpoint_state_dir(&self) -> &Path {
        self.checkpoint_store.state_dir()
    }

    pub(crate) async fn workspace_checkpoint_create(
        &self,
        project: String,
        title: Option<String>,
        note: Option<String>,
        include_untracked: Option<bool>,
        kind: Option<String>,
        labels: Vec<String>,
        validation: Option<CheckpointValidationInput>,
    ) -> ToolResult {
        let metadata = match normalize_checkpoint_metadata(kind, labels, validation) {
            Ok(metadata) => metadata,
            Err(err) => {
                return ToolResult::err_with_output(
                    err,
                    json!({"error_kind": "invalid_checkpoint_metadata"}),
                );
            }
        };
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let checkpoint_id = format!("{CHECKPOINT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let include_untracked = include_untracked.unwrap_or(false);
        let helper_output = match self
            .run_checkpoint_create(&resolved.config, include_untracked)
            .await
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if helper_output.get("error").is_some() {
            return ToolResult {
                success: false,
                error: helper_output
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output: helper_output,
            };
        }
        let mut checkpoint = helper_output;
        checkpoint["version"] = json!(CHECKPOINT_VERSION);
        checkpoint["checkpoint_id"] = json!(checkpoint_id);
        checkpoint["project"] = json!(project);
        checkpoint["project_input"] = json!(resolved.input);
        checkpoint["resolved_project"] = json!(resolved.resolved_id);
        checkpoint["title"] = json!(title);
        checkpoint["note"] = json!(note);
        checkpoint["include_untracked"] = json!(include_untracked);
        checkpoint["kind"] = json!(metadata.kind);
        checkpoint["labels"] = json!(metadata.labels);
        checkpoint["validation"] = metadata.validation;
        checkpoint["created_at"] = json!(chrono::Utc::now().timestamp());

        let storage_path = match self.checkpoint_store.write(
            checkpoint["resolved_project"].as_str().unwrap_or_default(),
            checkpoint["checkpoint_id"].as_str().unwrap_or_default(),
            &checkpoint,
        ) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
        let mut output = checkpoint_summary(&checkpoint, SummaryMode::Create);
        output["storage_path"] = json!(storage_path);
        ToolResult::ok(output)
    }

    pub(crate) async fn workspace_checkpoint_list(
        &self,
        project: String,
        limit: Option<usize>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
        let checkpoints = match self.checkpoint_store.list(&resolved.resolved_id, limit) {
            Ok(values) => values,
            Err(err) => return ToolResult::err(err),
        };
        let items: Vec<Value> = checkpoints
            .iter()
            .map(|(checkpoint, _)| checkpoint_summary(checkpoint, SummaryMode::List))
            .collect();
        ToolResult::ok(json!({
            "project": project,
            "resolved_project": resolved.resolved_id,
            "limit": limit,
            "checkpoints": items,
        }))
    }

    pub(crate) async fn workspace_checkpoint_show(
        &self,
        project: String,
        checkpoint_id: String,
        include_diff_stat: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let (checkpoint, path) = match self
            .checkpoint_store
            .load(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        let mut output = checkpoint_summary(&checkpoint, SummaryMode::Show);
        output["storage_path"] = json!(path);
        if include_diff_stat.unwrap_or(false) {
            output["diff_stat"] = json!({
                "tracked": checkpoint.get("diff_stat").cloned().unwrap_or(Value::String(String::new())),
                "staged": checkpoint.get("staged_diff_stat").cloned().unwrap_or(Value::String(String::new())),
            });
        }
        ToolResult::ok(output)
    }

    pub(crate) async fn workspace_checkpoint_restore(
        &self,
        project: String,
        checkpoint_id: String,
        confirm: bool,
    ) -> ToolResult {
        if !confirm {
            return ToolResult::err_with_output(
                "confirm must be true to restore a workspace checkpoint",
                json!({
                    "error_kind": "confirm_required",
                    "checkpoint_id": checkpoint_id,
                    "restored": false,
                }),
            );
        }
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let (checkpoint, _path) = match self
            .checkpoint_store
            .load(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if checkpoint.get("version").and_then(Value::as_u64) != Some(CHECKPOINT_VERSION as u64) {
            return ToolResult::err("unsupported checkpoint version");
        }
        let helper_output = match self
            .run_checkpoint_restore(&resolved.config, checkpoint)
            .await
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if helper_output.get("error").is_some() {
            return ToolResult {
                success: false,
                error: helper_output
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output: helper_output,
            };
        }
        ToolResult::ok(json!({
            "restored": true,
            "checkpoint_id": checkpoint_id,
            "project": project,
            "resolved_project": resolved.resolved_id,
            "changed_paths": helper_output.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
            "warnings": helper_output.get("warnings").cloned().unwrap_or_else(|| json!([])),
        }))
    }

    pub(crate) async fn workspace_checkpoint_delete(
        &self,
        project: String,
        checkpoint_id: String,
        confirm: bool,
    ) -> ToolResult {
        if !confirm {
            return ToolResult::err_with_output(
                "confirm must be true to delete a workspace checkpoint",
                json!({
                    "error_kind": "confirm_required",
                    "checkpoint_id": checkpoint_id,
                    "deleted": false,
                }),
            );
        }
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let path = match self
            .checkpoint_store
            .delete(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
        ToolResult::ok(json!({
            "deleted": true,
            "checkpoint_id": checkpoint_id,
            "project": project,
            "resolved_project": resolved.resolved_id,
            "storage_path": path,
        }))
    }

    async fn run_checkpoint_create(
        &self,
        config: &ProjectConfig,
        include_untracked: bool,
    ) -> Result<Value, String> {
        if config.is_agent() {
            let client_id = config.agent_client_id()?.to_string();
            return self
                .run_agent_json_file_op(
                    client_id,
                    config.path.clone(),
                    ".".to_string(),
                    "checkpoint_create",
                    json!({ "include_untracked": include_untracked }),
                    "checkpoint_create",
                )
                .await;
        }
        let root = config
            .root()
            .canonicalize()
            .map_err(|err| format!("Project root does not exist: {err}"))?;
        Ok(tokio::task::spawn_blocking(move || {
            create_workspace_checkpoint(&root, include_untracked)
        })
        .await
        .map_err(|err| format!("task join error: {err}"))?)
    }

    async fn run_checkpoint_restore(
        &self,
        config: &ProjectConfig,
        checkpoint: Value,
    ) -> Result<Value, String> {
        if config.is_agent() {
            let client_id = config.agent_client_id()?.to_string();
            return self
                .run_agent_json_file_op(
                    client_id,
                    config.path.clone(),
                    ".".to_string(),
                    "checkpoint_restore",
                    json!({ "checkpoint": checkpoint }),
                    "checkpoint_restore",
                )
                .await;
        }
        let root = config
            .root()
            .canonicalize()
            .map_err(|err| format!("Project root does not exist: {err}"))?;
        Ok(
            tokio::task::spawn_blocking(move || restore_workspace_checkpoint(&root, &checkpoint))
                .await
                .map_err(|err| format!("task join error: {err}"))?,
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum SummaryMode {
    Create,
    List,
    Show,
}

#[derive(Debug)]
struct CheckpointMetadata {
    kind: String,
    labels: Vec<String>,
    validation: Value,
}

fn normalize_checkpoint_metadata(
    kind: Option<String>,
    labels: Vec<String>,
    validation: Option<CheckpointValidationInput>,
) -> Result<CheckpointMetadata, String> {
    let kind = match kind {
        Some(value) => {
            let value = value.trim();
            if !is_checkpoint_kind(value) {
                return Err("kind must be one of snapshot, baseline, before_refactor, after_refactor, last_known_good, rollback_candidate".to_string());
            }
            value.to_string()
        }
        None => DEFAULT_CHECKPOINT_KIND.to_string(),
    };

    if labels.len() > MAX_CHECKPOINT_LABELS {
        return Err(format!(
            "labels must contain at most {MAX_CHECKPOINT_LABELS} entries"
        ));
    }
    let mut normalized_labels = Vec::with_capacity(labels.len());
    for (idx, label) in labels.into_iter().enumerate() {
        validate_checkpoint_label(&label).map_err(|err| format!("labels[{idx}] {err}"))?;
        normalized_labels.push(label);
    }

    let validation = normalize_validation_input(validation)?;
    Ok(CheckpointMetadata {
        kind,
        labels: normalized_labels,
        validation,
    })
}

fn normalize_validation_input(
    validation: Option<CheckpointValidationInput>,
) -> Result<Value, String> {
    let Some(validation) = validation else {
        return Ok(default_validation_metadata());
    };

    let status = match validation.status {
        Some(value) => {
            let value = value.trim();
            if !is_checkpoint_validation_status(value) {
                return Err(
                    "validation.status must be one of unknown, not_run, passed, failed".to_string(),
                );
            }
            value.to_string()
        }
        None => DEFAULT_VALIDATION_STATUS.to_string(),
    };

    if validation.commands.len() > MAX_VALIDATION_COMMANDS {
        return Err(format!(
            "validation.commands must contain at most {MAX_VALIDATION_COMMANDS} entries"
        ));
    }
    let mut commands = Vec::with_capacity(validation.commands.len());
    for (idx, command) in validation.commands.into_iter().enumerate() {
        let command = command.trim();
        if command.is_empty() {
            return Err(format!("validation.commands[{idx}] cannot be empty"));
        }
        if command.contains('\0') {
            return Err(format!("validation.commands[{idx}] contains NUL"));
        }
        if command.chars().count() > MAX_VALIDATION_COMMAND_LEN {
            return Err(format!(
                "validation.commands[{idx}] exceeds {MAX_VALIDATION_COMMAND_LEN} characters"
            ));
        }
        if checkpoint_metadata_secret_like_text(command) {
            return Err(format!(
                "validation.commands[{idx}] contains secret-like text"
            ));
        }
        commands.push(command.to_string());
    }

    let summary = match validation.summary {
        Some(value) => {
            let value = value.trim();
            if value.contains('\0') {
                return Err("validation.summary contains NUL".to_string());
            }
            if value.chars().count() > MAX_VALIDATION_SUMMARY_LEN {
                return Err(format!(
                    "validation.summary exceeds {MAX_VALIDATION_SUMMARY_LEN} characters"
                ));
            }
            if checkpoint_metadata_secret_like_text(value) {
                return Err("validation.summary contains secret-like text".to_string());
            }
            if value.is_empty() {
                Value::Null
            } else {
                json!(value)
            }
        }
        None => Value::Null,
    };

    Ok(json!({
        "status": status,
        "commands": commands,
        "summary": summary,
    }))
}

fn validate_checkpoint_label(label: &str) -> Result<(), String> {
    if label.is_empty() {
        return Err("cannot be empty".to_string());
    }
    if label.len() > MAX_CHECKPOINT_LABEL_LEN {
        return Err(format!("exceeds {MAX_CHECKPOINT_LABEL_LEN} characters"));
    }
    if !label
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("may only contain ASCII letters, digits, '.', '_', and '-'".to_string());
    }
    Ok(())
}

fn checkpoint_metadata_secret_like_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    secret_like_value(value)
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("authorization=")
        || lower.contains("client_secret=")
}

fn checkpoint_summary(checkpoint: &Value, mode: SummaryMode) -> Value {
    let untracked_files = checkpoint
        .get("untracked_files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| {
                    let path = file.get("path").and_then(Value::as_str)?;
                    Some(json!({
                        "path": path,
                        "byte_count": file.get("byte_count").cloned().unwrap_or(Value::Null),
                        "sha256": file.get("sha256").cloned().unwrap_or(Value::Null),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let files = checkpoint_file_list(checkpoint);
    let validation = checkpoint_validation_metadata(checkpoint);
    let mut output = json!({
        "checkpoint_id": checkpoint_id_of(checkpoint),
        "project": checkpoint.get("project").cloned().unwrap_or(Value::Null),
        "resolved_project": checkpoint.get("resolved_project").cloned().unwrap_or(Value::Null),
        "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
        "kind": checkpoint_kind(checkpoint),
        "labels": checkpoint_labels(checkpoint),
        "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
        "head": checkpoint.get("head").cloned().unwrap_or(Value::Null),
        "branch": checkpoint.get("branch").cloned().unwrap_or(Value::Null),
        "complete": checkpoint.get("complete").cloned().unwrap_or(Value::Bool(false)),
        "tracked_diff_bytes": checkpoint.get("tracked_diff_bytes").cloned().unwrap_or(Value::Number(0.into())),
        "staged_diff_bytes": checkpoint.get("staged_diff_bytes").cloned().unwrap_or(Value::Number(0.into())),
        "untracked_files": untracked_files,
        "untracked_file_count": checkpoint.get("untracked_files").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "skipped_files": checkpoint.get("skipped_files").cloned().unwrap_or_else(|| json!([])),
        "status_summary": checkpoint.get("status_summary").cloned().unwrap_or_else(|| json!({})),
    });
    match mode {
        SummaryMode::Create => {
            output["note"] = checkpoint.get("note").cloned().unwrap_or(Value::Null);
            output["include_untracked"] = checkpoint
                .get("include_untracked")
                .cloned()
                .unwrap_or(Value::Bool(false));
            output["validation"] = validation;
        }
        SummaryMode::List => {
            output["validation_status"] = validation
                .get("status")
                .cloned()
                .unwrap_or_else(|| json!(DEFAULT_VALIDATION_STATUS));
        }
        SummaryMode::Show => {
            output["note"] = checkpoint.get("note").cloned().unwrap_or(Value::Null);
            output["validation"] = validation;
            output["files"] = json!(files);
            output["limitations"] = checkpoint
                .get("limitations")
                .cloned()
                .unwrap_or_else(|| json!([]));
        }
    }
    output
}

fn checkpoint_kind(checkpoint: &Value) -> String {
    checkpoint
        .get("kind")
        .and_then(Value::as_str)
        .filter(|kind| is_checkpoint_kind(kind))
        .unwrap_or(DEFAULT_CHECKPOINT_KIND)
        .to_string()
}

fn checkpoint_labels(checkpoint: &Value) -> Vec<String> {
    checkpoint
        .get("labels")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|label| {
                    validate_checkpoint_label(label).ok()?;
                    Some(label.to_string())
                })
                .take(MAX_CHECKPOINT_LABELS)
                .collect()
        })
        .unwrap_or_default()
}

fn default_validation_metadata() -> Value {
    json!({
        "status": DEFAULT_VALIDATION_STATUS,
        "commands": [],
        "summary": Value::Null,
    })
}

fn checkpoint_validation_metadata(checkpoint: &Value) -> Value {
    let Some(validation) = checkpoint.get("validation").and_then(Value::as_object) else {
        return default_validation_metadata();
    };
    let status = validation
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| is_checkpoint_validation_status(status))
        .unwrap_or(DEFAULT_VALIDATION_STATUS);
    let commands = validation
        .get("commands")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|command| {
                    let command = command.trim();
                    if command.is_empty()
                        || command.contains('\0')
                        || command.chars().count() > MAX_VALIDATION_COMMAND_LEN
                        || checkpoint_metadata_secret_like_text(command)
                    {
                        return None;
                    }
                    Some(command.to_string())
                })
                .take(MAX_VALIDATION_COMMANDS)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let summary = validation
        .get("summary")
        .and_then(Value::as_str)
        .and_then(|summary| {
            let summary = summary.trim();
            if summary.is_empty()
                || summary.contains('\0')
                || summary.chars().count() > MAX_VALIDATION_SUMMARY_LEN
                || checkpoint_metadata_secret_like_text(summary)
            {
                return None;
            }
            Some(json!(summary))
        })
        .unwrap_or(Value::Null);
    json!({
        "status": status,
        "commands": commands,
        "summary": summary,
    })
}

fn checkpoint_file_list(checkpoint: &Value) -> Vec<Value> {
    let mut files = Vec::new();
    for diff_key in ["staged_diff", "tracked_diff"] {
        if let Some(diff) = checkpoint.get(diff_key).and_then(Value::as_str) {
            for path in changed_paths_from_diff(diff) {
                if !files.iter().any(|file: &Value| {
                    file.get("path").and_then(Value::as_str) == Some(path.as_str())
                }) {
                    files.push(json!({
                        "path": path,
                        "kind": "tracked",
                    }));
                }
            }
        }
    }
    if let Some(untracked) = checkpoint.get("untracked_files").and_then(Value::as_array) {
        for item in untracked {
            let Some(path) = item.get("path").and_then(Value::as_str) else {
                continue;
            };
            if !files
                .iter()
                .any(|file| file.get("path").and_then(Value::as_str) == Some(path))
            {
                files.push(json!({
                    "path": path,
                    "kind": "untracked",
                }));
            }
        }
    }
    files
}

fn changed_paths_from_diff(diff: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(pos) = rest.rfind(" b/") {
                let path = &rest[pos + 3..];
                push_unique(&mut paths, path);
            }
            continue;
        }
        for prefix in ["+++ b/", "--- a/"] {
            if let Some(path) = line.strip_prefix(prefix) {
                if path != "/dev/null" {
                    push_unique(&mut paths, path);
                }
            }
        }
    }
    paths
}

fn push_unique(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() || paths.iter().any(|existing| existing == path) {
        return;
    }
    paths.push(path.to_string());
}

fn checkpoint_id_of(checkpoint: &Value) -> String {
    checkpoint
        .get("checkpoint_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn safe_project_id(project: &str) -> String {
    let mut safe = project
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() {
        safe.push_str("project");
    }
    safe.truncate(96);
    let mut hasher = Sha256::new();
    hasher.update(project.as_bytes());
    let digest = hasher.finalize();
    safe.push('_');
    for byte in digest.iter().take(6) {
        safe.push_str(&format!("{byte:02x}"));
    }
    safe
}

fn validate_checkpoint_id(checkpoint_id: &str) -> Result<(), String> {
    let Some(rest) = checkpoint_id.strip_prefix(CHECKPOINT_ID_PREFIX) else {
        return Err("checkpoint_id must start with wc_ckpt_".to_string());
    };
    if rest.is_empty() || rest.len() > 64 {
        return Err("checkpoint_id has invalid length".to_string());
    }
    if !rest
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Err("checkpoint_id contains invalid characters".to_string());
    }
    Ok(())
}
