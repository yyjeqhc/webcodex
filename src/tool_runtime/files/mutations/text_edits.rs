//! Transactional file-change validation, revision binding, and dispatch.

use super::lifecycle::{await_structured_edit_response, structured_edit_not_started_result};
use super::preflight::{read_files_recovery, read_revision_rejection, validate_edit_file_path};
use super::text_edits_result::apply_text_edits_agent_stdout_result;
use super::*;
use crate::tool_runtime::workspace_reads::WorkspaceReadRuntime;

fn apply_text_edit_local_guard_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}. No files were modified. Retry guidance: this Runner generation requires a read revision for local guarded edits; reread the current file and retry with expected_read_revision."
        ),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LOCAL_GUARD_WITHOUT_SHA
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::FixInput)
}

fn apply_text_edit_occurrence_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: the accepted Runner violated the generation-2 apply_text_edit_occurrence baseline; reconnect it or refine the edit to a unique exact match without occurrence."
        ),
        json!({
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE
        }),
    )
}

fn apply_text_edit_line_scope_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: reconnect a Runner that explicitly supports apply_text_edit_line_scope, or remove line_scope only if an unscoped exact edit is safe."
        ),
        json!({
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LINE_SCOPE
        }),
    )
}

fn apply_text_edit_range_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: reconnect a Runner that explicitly supports apply_text_edit_range before retrying this revision-fenced range edit."
        ),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_RANGE
        }),
    )
}
fn validate_apply_text_edit(
    change_index: usize,
    edit_index: usize,
    edit: &ApplyTextEditInput,
) -> Result<(), String> {
    let validate_field = |label: &str, value: &Option<String>| -> Result<(), String> {
        if let Some(value) = value {
            if value.contains('\0') {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): {label} cannot contain NUL bytes",
                    edit.kind.as_str()
                ));
            }
            if value.len() > MAX_APPLY_TEXT_EDIT_FIELD_BYTES {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): {label} exceeds {} bytes",
                    edit.kind.as_str(),
                    MAX_APPLY_TEXT_EDIT_FIELD_BYTES
                ));
            }
        }
        Ok(())
    };
    validate_field("old_text", &edit.old_text)?;
    validate_field("new_text", &edit.new_text)?;
    validate_field("anchor_text", &edit.anchor_text)?;
    if edit.occurrence == Some(0) {
        return Err(format!(
            "change {change_index} edit {edit_index} ({}): occurrence must be at least 1",
            edit.kind.as_str()
        ));
    }
    if let Some(expected) = edit.expected_match_count {
        if expected == 0
            || expected > crate::apply_edits_shared::MAX_APPLY_TEXT_EXPECTED_MATCH_COUNT
        {
            return Err(format!("change {change_index} edit {edit_index} ({}): expected_match_count must be within 1..={}", edit.kind.as_str(), crate::apply_edits_shared::MAX_APPLY_TEXT_EXPECTED_MATCH_COUNT));
        }
        if edit.kind != ApplyTextEditKind::ReplaceExact || edit.occurrence.is_some() {
            return Err(format!("change {change_index} edit {edit_index} ({}): expected_match_count is only allowed for replace_exact without occurrence", edit.kind.as_str()));
        }
    }
    if let Some(line_scope) = edit.line_scope {
        if let Err(reason) = line_scope.validate() {
            return Err(format!(
                "change {change_index} edit {edit_index} ({}): {reason}",
                edit.kind.as_str()
            ));
        }
    }
    match edit.kind {
        ApplyTextEditKind::ReplaceRange => {
            if edit.line_scope.is_none() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_range): start_line/end_line are required"
                ));
            }
            if edit.new_text.is_none() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_range): new_text is required"
                ));
            }
            if edit.old_text.is_some()
                || edit.anchor_text.is_some()
                || edit.occurrence.is_some()
                || edit.expected_match_count.is_some()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_range): old_text, anchor_text, occurrence, and expected_match_count are not allowed"
                ));
            }
        }
        ApplyTextEditKind::ReplaceExact => {
            if edit
                .old_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_exact): old_text must be non-empty"
                ));
            }
            if edit.anchor_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_exact): anchor_text is not allowed"
                ));
            }
        }
        ApplyTextEditKind::DeleteExact => {
            if edit
                .old_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} (delete_exact): old_text must be non-empty"
                ));
            }
            if edit.new_text.is_some() || edit.anchor_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (delete_exact): new_text and anchor_text are not allowed"
                ));
            }
        }
        ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
            if edit
                .anchor_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): anchor_text must be non-empty",
                    edit.kind.as_str()
                ));
            }
            if edit.new_text.is_none() {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): new_text is required",
                    edit.kind.as_str()
                ));
            }
            if edit.old_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): old_text is not allowed",
                    edit.kind.as_str()
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ApplyTextEditsPreflightValidationError {
    message: String,
    edit_index: Option<usize>,
    reread_required: bool,
}

impl From<String> for ApplyTextEditsPreflightValidationError {
    fn from(message: String) -> Self {
        Self {
            message,
            edit_index: None,
            reread_required: false,
        }
    }
}

fn validate_apply_file_change(
    index: usize,
    change: &ApplyFileChangeInput,
) -> Result<(), ApplyTextEditsPreflightValidationError> {
    let valid_revision = |required: bool,
                          required_edit_index: Option<usize>|
     -> Result<(), ApplyTextEditsPreflightValidationError> {
        match change.expected_read_revision {
            Some(revision) if (1..=MAX_JSON_SAFE_INTEGER).contains(&revision) => Ok(()),
            Some(_) => Err(format!(
                "change {index} ({}): expected_read_revision must be a positive JSON-safe integer",
                change.kind.as_str()
            )
            .into()),
            None if required => Err(ApplyTextEditsPreflightValidationError {
                message: format!(
                    "change {index} ({}): expected_read_revision is required",
                    change.kind.as_str()
                ),
                edit_index: required_edit_index,
                reread_required: true,
            }),
            None => Ok(()),
        }
    };
    match change.kind {
        ApplyFileChangeKind::Edit => {
            let positional_edit_index = change.edits.iter().position(|edit| {
                edit.occurrence.is_some()
                    || edit.line_scope.is_some()
                    || edit.expected_match_count.is_some()
            });
            if change.to_path.is_some() || change.content.is_some() {
                return Err(
                    format!("change {index} (edit): to_path and content are not allowed").into(),
                );
            }
            if change.edits.is_empty() || change.edits.len() > MAX_APPLY_TEXT_EDITS {
                return Err(format!(
                    "change {index} (edit): edits must contain 1..={MAX_APPLY_TEXT_EDITS} entries"
                )
                .into());
            }
            for (edit_index, edit) in change.edits.iter().enumerate() {
                if let Err(message) = validate_apply_text_edit(index, edit_index, edit) {
                    return Err(ApplyTextEditsPreflightValidationError {
                        message,
                        edit_index: Some(edit_index),
                        reread_required: false,
                    });
                }
            }
            valid_revision(true, positional_edit_index)?;
        }
        ApplyFileChangeKind::Create => {
            if change.to_path.is_some()
                || change.expected_read_revision.is_some()
                || !change.edits.is_empty()
            {
                return Err(format!(
                    "change {index} (create): to_path, expected_read_revision, and edits are not allowed"
                )
                .into());
            }
            let content = change
                .content
                .as_deref()
                .ok_or_else(|| format!("change {index} (create): content is required"))?;
            if content.contains('\0') {
                return Err(
                    format!("change {index} (create): content cannot contain NUL bytes").into(),
                );
            }
        }
        ApplyFileChangeKind::Delete => {
            valid_revision(true, None)?;
            if change.to_path.is_some() || change.content.is_some() || !change.edits.is_empty() {
                return Err(format!(
                    "change {index} (delete): to_path, content, and edits are not allowed"
                )
                .into());
            }
        }
        ApplyFileChangeKind::Rename => {
            valid_revision(true, None)?;
            let to_path = change
                .to_path
                .as_deref()
                .ok_or_else(|| format!("change {index} (rename): to_path is required"))?;
            if to_path == change.path {
                return Err(
                    format!("change {index} (rename): path and to_path must differ").into(),
                );
            }
            if change.content.is_some() || !change.edits.is_empty() {
                return Err(
                    format!("change {index} (rename): content and edits are not allowed").into(),
                );
            }
        }
    }
    Ok(())
}

fn compact_apply_text_edits_preflight_rejection(
    message: impl Into<String>,
    error_kind: &'static str,
    change_index: Option<usize>,
    edit_index: Option<usize>,
    kind: Option<&str>,
    path: Option<&str>,
) -> ToolResult {
    let detail = message.into();
    let mut output = json!({
        "state_changed": false,
        "execution_state": "not_started",
        "error_kind": error_kind,
    });
    if let Some(change_index) = change_index {
        output["change_index"] = json!(change_index);
    }
    if let Some(edit_index) = edit_index {
        output["edit_index"] = json!(edit_index);
    }
    if let Some(kind) = kind {
        output["kind"] = json!(kind);
    }
    if let Some(path) = path {
        output["path"] = json!(path);
    }
    ToolResult::err_with_output(
        format!("Rejected before write: {detail}. No files were modified."),
        output,
    )
}

fn apply_text_edits_path_overlap(
    first_change_index: usize,
    change_index: usize,
    kind: &str,
    path: &str,
) -> ToolResult {
    let mut result = compact_apply_text_edits_preflight_rejection(
        format!(
            "change {change_index} reuses path '{path}' first occupied by change {first_change_index}; each source/destination path may appear only once"
        ),
        "path_overlap",
        Some(change_index),
        None,
        Some(kind),
        Some(path),
    );
    // Proven by Server preflight, not Runner-supplied recovery. This identifies
    // the conflict; it does not imply sequential changes can be coalesced safely.
    result.output["path_conflict_change_indices"] = json!([first_change_index, change_index]);
    result
}

pub(super) fn compact_apply_text_edits_path_policy_rejection(
    change_index: usize,
    kind: &str,
    path: &str,
    message: String,
) -> ToolResult {
    let mut result =
        crate::tool_runtime::permissions::edit_path_policy_rejected_result(path, message);
    if let Some(output) = result.output.as_object_mut() {
        output.remove("path");
        output.remove("error");
    }
    result.output["execution_state"] = json!("not_started");
    result.output["change_index"] = json!(change_index);
    result.output["kind"] = json!(kind);
    result
}
impl ToolRuntime {
    pub(crate) async fn apply_text_edits(
        &self,
        project: String,
        changes: Vec<ApplyFileChangeInput>,
        dry_run: Option<bool>,
    ) -> ToolResult {
        if changes.is_empty() {
            return compact_apply_text_edits_preflight_rejection(
                "changes must contain at least one file change",
                "empty_batch",
                None,
                None,
                None,
                None,
            );
        }
        if changes.len() > MAX_APPLY_FILE_CHANGES {
            return compact_apply_text_edits_preflight_rejection(
                format!(
                    "too many file changes; maximum is {}",
                    MAX_APPLY_FILE_CHANGES
                ),
                "batch_too_large",
                None,
                None,
                None,
                None,
            );
        }
        let mut touched_paths = std::collections::HashMap::new();
        for (change_index, change) in changes.iter().enumerate() {
            if let Err(error) = validate_edit_file_path(&change.path) {
                return compact_apply_text_edits_path_policy_rejection(
                    change_index,
                    change.kind.as_str(),
                    &change.path,
                    error,
                );
            }
            if let Some(first_index) = touched_paths.insert(change.path.as_str(), change_index) {
                return apply_text_edits_path_overlap(
                    first_index,
                    change_index,
                    change.kind.as_str(),
                    &change.path,
                );
            }
            if let Some(to_path) = change.to_path.as_deref() {
                if let Err(error) = validate_edit_file_path(to_path) {
                    return compact_apply_text_edits_path_policy_rejection(
                        change_index,
                        change.kind.as_str(),
                        to_path,
                        error,
                    );
                }
                if let Some(first_index) = touched_paths.insert(to_path, change_index) {
                    return apply_text_edits_path_overlap(
                        first_index,
                        change_index,
                        change.kind.as_str(),
                        to_path,
                    );
                }
            }
            if let Err(validation_error) = validate_apply_file_change(change_index, change) {
                let failed_kind = validation_error
                    .edit_index
                    .and_then(|edit_index| change.edits.get(edit_index))
                    .map(|edit| edit.kind.as_str())
                    .unwrap_or_else(|| change.kind.as_str());
                let mut result = compact_apply_text_edits_preflight_rejection(
                    validation_error.message,
                    if validation_error.reread_required {
                        "missing_read_revision"
                    } else if validation_error.edit_index.is_some() {
                        "invalid_edit"
                    } else {
                        "invalid_change"
                    },
                    Some(change_index),
                    validation_error.edit_index,
                    Some(failed_kind),
                    Some(&change.path),
                );
                if validation_error.reread_required {
                    result.output["recovery"] = read_files_recovery(&project, &change.path);
                }
                return result;
            }
        }

        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return ToolResult::err(error),
        };
        let Some(runner) = self
            .runner_registry
            .get_runner_view(&resolved.config.client_id)
            .await
        else {
            return structured_edit_not_started_result(
                "edit_project_files",
                "the resolved Runner became unavailable before mutation admission",
            );
        };
        let Some(runner_project_id) =
            crate::tool_runtime::runner_local_project_id(&resolved.resolved_id)
        else {
            return structured_edit_not_started_result(
                "edit_project_files",
                "the resolved Project identity could not be bound to a Runner-local project id",
            );
        };

        // Resolve every strong model-facing guard before request construction.
        // Any invalid/stale/mismatched revision rejects the entire batch without
        // dispatch, preserving transactionality across mixed guarded/unguarded changes.
        let mut wire_changes = Vec::with_capacity(changes.len());
        for (change_index, change) in changes.iter().enumerate() {
            let expected_sha256 = match change.expected_read_revision {
                Some(revision) => {
                    let target = WorkspaceReadRuntime::revision_target(
                        &resolved,
                        &change.path,
                        &runner.runner_instance_id,
                    );
                    match self.reads.resolve_revision(revision, &target) {
                        Ok(sha256) => Some(sha256),
                        Err(error) => {
                            let mut result =
                                read_revision_rejection(&resolved.resolved_id, &change.path, error);
                            result.output["change_index"] = json!(change_index);
                            return result;
                        }
                    }
                }
                None => None,
            };
            wire_changes.push(crate::apply_edits_shared::ApplyFileChangeInput {
                kind: change.kind,
                path: change.path.clone(),
                to_path: change.to_path.clone(),
                content: change.content.clone(),
                edits: change.edits.clone(),
                expected_sha256,
            });
        }

        let expected_change_count = changes.len();
        let expected_dry_run = dry_run.unwrap_or(false);
        let payload = json!({
            "changes": wire_changes,
            "dry_run": expected_dry_run,
            "recovery_metadata_version": 1,
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) if serialized.len() <= MAX_APPLY_FILE_CHANGES_BYTES => serialized,
            Ok(_) => {
                return compact_apply_text_edits_preflight_rejection(
                    format!(
                        "serialized file changes exceed {} bytes",
                        MAX_APPLY_FILE_CHANGES_BYTES
                    ),
                    "payload_too_large",
                    None,
                    None,
                    None,
                    None,
                )
            }
            Err(error) => {
                return compact_apply_text_edits_preflight_rejection(
                    format!("failed to serialize file changes payload: {error}"),
                    "serialization_failed",
                    None,
                    None,
                    None,
                    None,
                )
            }
        };

        let wait_timeout = 60_u64;
        let routing_path = changes
            .first()
            .map(|change| change.path.clone())
            .expect("non-empty changes validated above");
        let request = ShellFileOpRequest {
            op: "apply_text_edits".to_string(),
            client_id: resolved.config.client_id.clone(),
            path: routing_path,
            cwd: Some(resolved.config.path.clone()),
            content: Some(serialized),
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            wait_timeout_secs: wait_timeout,
        };
        let enqueue_result = self
            .runner_registry
            .enqueue_project_file_mutation(
                request,
                runner_project_id,
                &resolved.config.path,
                &runner.runner_instance_id,
                "tool_runtime".to_string(),
            )
            .await;
        let (request_id, rx) = match enqueue_result {
            Ok(request) => request,
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_EXPECTED_MATCH_COUNT,
                    ) =>
            {
                let mut result = ToolResult::err_with_output(
                    format!("Rejected before write: {error}. No files were modified."),
                    json!({"changed":false,"state_changed":false,"execution_state":"not_started","error_kind":"agent_capability_unavailable","failure_kind":"capability_unavailable","capability":crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_EXPECTED_MATCH_COUNT}),
                );
                if let Some((change_index, edit_index, path)) = changes.iter().enumerate().find_map(|(change_index, change)| {
                    change.edits.iter().position(|edit| edit.expected_match_count.is_some())
                        .map(|edit_index| (change_index, edit_index, change.path.as_str()))
                }) {
                    result.output["change_index"] = json!(change_index);
                    result.output["edit_index"] = json!(edit_index);
                    result.output["path"] = json!(path);
                }
                return result;
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LOCAL_GUARD_WITHOUT_SHA,
                    ) =>
            {
                return apply_text_edit_local_guard_capability_rejection(error)
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LINE_SCOPE,
                    ) =>
            {
                return apply_text_edit_line_scope_capability_rejection(error)
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_RANGE,
                    ) =>
            {
                return apply_text_edit_range_capability_rejection(error)
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
                    ) =>
            {
                return apply_text_edit_occurrence_capability_rejection(error)
            }
            Err(error) if error.starts_with("stale_runner:") => {
                if let Some((change_index, path)) = changes.iter().enumerate().find_map(|(index, change)| {
                    change.expected_read_revision.map(|_| (index, change.path.as_str()))
                }) {
                    let mut result = read_revision_rejection(
                        &resolved.resolved_id,
                        path,
                        ReadRevisionLookupError::OwnerMismatch,
                    );
                    result.output["change_index"] = json!(change_index);
                    return result;
                }
                return structured_edit_not_started_result(
                    "edit_project_files",
                    "the exact Runner changed before the local edit could be dispatched; reread before retrying",
                );
            }
            Err(_) => {
                return structured_edit_not_started_result(
                    "edit_project_files",
                    "the Runner queue rejected the request before dispatch",
                )
            }
        };
        let response = match await_structured_edit_response(
            self,
            &request_id,
            rx,
            wait_timeout,
            "edit_project_files",
        )
        .await
        {
            Ok(response) => response,
            Err(result) => return result,
        };
        let mut result = apply_text_edits_agent_stdout_result(
            &response.stdout.unwrap_or_default(),
            expected_change_count,
            expected_dry_run,
            &resolved.resolved_id,
            &changes,
        );
        if !result.success {
            return result;
        }

        let files = result
            .output
            .get_mut("files")
            .and_then(Value::as_array_mut)
            .expect("validated apply_text_edits success files");
        for (file, change) in files.iter_mut().zip(&changes) {
            let new_sha256 = file
                .get("new_sha256")
                .and_then(Value::as_str)
                .map(str::to_string);
            let read_revision = if expected_dry_run || change.kind == ApplyFileChangeKind::Delete {
                None
            } else {
                let final_path = match change.kind {
                    ApplyFileChangeKind::Rename => change
                        .to_path
                        .as_deref()
                        .expect("validated rename destination"),
                    _ => change.path.as_str(),
                };
                let target = WorkspaceReadRuntime::revision_target(
                    &resolved,
                    final_path,
                    &runner.runner_instance_id,
                );
                Some(self.reads.observe_revision(
                    target,
                    new_sha256.expect("validated final apply_text_edits sha256"),
                ))
            };
            let file = file
                .as_object_mut()
                .expect("validated apply_text_edits file result object");
            file.insert("read_revision".to_string(), json!(read_revision));
        }
        result
    }
}
