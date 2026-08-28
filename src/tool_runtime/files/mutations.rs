use super::*;

pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum serialized batch payload sent to the owning agent. Host-only (the
/// agent enforces a per-file cap instead), so it stays local.
pub(crate) const MAX_APPLY_FILE_CHANGES_BYTES: usize = 1024 * 1024;

fn recoverable_write_rejection(reason: impl AsRef<str>) -> String {
    format!(
        "Rejected before write: {}.\nNo files were modified.\nRetry guidance: read the file again to refresh line numbers/context, then retry with updated guards.",
        reason.as_ref()
    )
}

fn apply_text_edit_occurrence_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: upgrade the Runner or refine the edit to a unique exact match without occurrence."
        ),
        json!({
            "state_changed": false,
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::shell_protocol::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
            "retry_guidance": "upgrade the Runner or refine the edit to a unique exact match without occurrence"
        }),
    )
}

/// Maximum decoded size for whole-payload/model-facing artifact operations.
/// These paths aggregate content or return it as base64/JSON, so they remain at
/// 10 MiB even though data-plane upload/export paths admit larger files.
/// Validate a project-relative file path for the structured edit tools
/// (`write_project_file`, `apply_text_edits`). Unlike the patch preflight
/// path validator, this HARD-rejects sensitive path components (the task spec
/// for these tools says "拒绝敏感路径", not "warn"). Absolute paths, `..`
/// traversal, empty paths, NUL bytes, and sensitive components are all rejected
/// so the helper never touches secrets, version control, or build output.
pub(crate) fn validate_edit_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_edit_path(path) {
        return Err(format!(
            "refusing sensitive path '{}': touches agent.toml, webcodex.env, \
             .env, projects.d, .git, target, or node_modules",
            path
        ));
    }
    Ok(())
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
    match edit.kind {
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
            if edit
                .new_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): new_text must be non-empty",
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
}

impl From<String> for ApplyTextEditsPreflightValidationError {
    fn from(message: String) -> Self {
        Self {
            message,
            edit_index: None,
        }
    }
}

fn validate_apply_file_change(
    index: usize,
    change: &ApplyFileChangeInput,
) -> Result<(), ApplyTextEditsPreflightValidationError> {
    let expected_hash = || -> Result<(), ApplyTextEditsPreflightValidationError> {
        match change.expected_sha256.as_deref() {
            Some(hash) if is_hex_sha256(hash) => Ok(()),
            _ => Err(format!(
                "change {index} ({}): expected_sha256 is required and must be 64 lowercase hexadecimal characters",
                change.kind.as_str()
            )
            .into()),
        }
    };
    match change.kind {
        ApplyFileChangeKind::Edit => {
            expected_hash()?;
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
                    });
                }
            }
        }
        ApplyFileChangeKind::Create => {
            if change.to_path.is_some()
                || change.expected_sha256.is_some()
                || !change.edits.is_empty()
            {
                return Err(format!(
                    "change {index} (create): to_path, expected_sha256, and edits are not allowed"
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
            expected_hash()?;
            if change.to_path.is_some() || change.content.is_some() || !change.edits.is_empty() {
                return Err(format!(
                    "change {index} (delete): to_path, content, and edits are not allowed"
                )
                .into());
            }
        }
        ApplyFileChangeKind::Rename => {
            expected_hash()?;
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

fn apply_text_edits_agent_stdout_result(stdout: &str) -> ToolResult {
    let stdout = stdout.trim();
    let mut obj: Value = match serde_json::from_str(stdout) {
        Ok(value) => value,
        Err(error) => {
            return ToolResult::err(format!(
                "agent apply_text_edits returned invalid JSON: {} (got: {})",
                error,
                &stdout[..stdout.len().min(200)]
            ))
        }
    };
    if let Some(error) = obj.get("error").and_then(Value::as_str).map(str::to_string) {
        let uncertain = obj.get("rollback_complete").and_then(Value::as_bool) == Some(false)
            || obj.get("changed").and_then(Value::as_bool) == Some(true);
        let message = if uncertain {
            if let Some(output) = obj.as_object_mut() {
                // A Runner response that admits incomplete rollback or a
                // changed worktree cannot retain deterministic no-mutation
                // retry authority from an earlier planning conflict.
                output.remove("conflict_recovery");
                output.remove("retry_guidance");
                output.remove("state_changed");
            }
            format!(
                "Edit outcome is uncertain: {error}. Inspect the affected files before issuing another write."
            )
        } else if obj.get("conflict_recovery").is_some_and(Value::is_object) {
            error.clone()
        } else {
            recoverable_write_rejection(&error)
        };
        return ToolResult::err_with_output(message, obj);
    }
    ToolResult::ok(obj)
}

fn apply_text_edits_preflight_rejection(
    message: impl Into<String>,
    error_kind: &'static str,
    change_index: Option<usize>,
    edit_index: Option<usize>,
    kind: Option<&str>,
    path: Option<&str>,
    retry_guidance: &'static str,
) -> ToolResult {
    let detail = message.into();
    let mut output = json!({
        "state_changed": false,
        "error_kind": error_kind,
        "retry_guidance": retry_guidance,
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
        format!(
            "Rejected before write: {detail}.\nNo files were modified.\nRetry guidance: {retry_guidance}."
        ),
        output,
    )
}

fn apply_text_edits_path_policy_rejection(
    change_index: usize,
    kind: &str,
    path: &str,
    message: String,
) -> ToolResult {
    let mut result = super::permissions::edit_path_policy_rejected_result(path, message);
    if let Some(output) = result.output.as_object_mut() {
        // The rejected path may itself be absolute or sensitive. Keep exact
        // change provenance without copying that untrusted path into recovery
        // metadata.
        output.remove("path");
        output.remove("error");
    }
    result.output["change_index"] = json!(change_index);
    result.output["kind"] = json!(kind);
    result.output["retry_guidance"] =
        json!("correct the rejected project-relative path and retry the whole batch");
    result
}

/// Pure, allocation-only computation of an `apply_text_edits` plan against
/// `original` UTF-8 content. Performs every semantic validation (unique
/// match, no overlap, whole-file sha guard) and returns the new content plus
/// a structured summary. Never touches the filesystem — the runtime/agent
/// layer decides whether to write. Used directly by unit tests; the agent
/// handler mirrors these exact semantics for the production write path.
#[cfg(test)]
pub(crate) fn apply_text_edits_to_string(
    original: &str,
    path: &str,
    edits: &[ApplyTextEditInput],
    expected_file_sha256: Option<&str>,
    dry_run: bool,
) -> Result<(String, Value), String> {
    if edits.is_empty() {
        return Err("edits must contain at least one edit".to_string());
    }
    if edits.len() > MAX_APPLY_TEXT_EDITS {
        return Err(format!(
            "too many edits; maximum is {}",
            MAX_APPLY_TEXT_EDITS
        ));
    }
    let old_sha256 = sha256_hex_bytes(original.as_bytes());
    if let Some(expected) = expected_file_sha256 {
        if old_sha256 != expected {
            return Err(recoverable_write_rejection("expected_file_sha256 mismatch"));
        }
    }

    let raw_original = original;
    let line_ending =
        detect_apply_text_line_ending(original).map_err(recoverable_write_rejection)?;
    let canonical_original = canonicalize_apply_text_line_endings(original, line_ending)
        .map_err(recoverable_write_rejection)?;
    let original = canonical_original.as_ref();

    // Resolve each edit to a (start, end, replacement, index) op against the
    // original content. start/end are byte offsets; inserts are zero-width.
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let kind = edit.kind;
        if edit.occurrence == Some(0) {
            return Err(edit_field_error(
                index,
                kind,
                "occurrence must be at least 1",
            ));
        }
        let (needle, replacement): (&str, String) = match kind {
            ApplyTextEditKind::ReplaceExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                let new = edit.new_text.clone().unwrap_or_default();
                (old, new)
            }
            ApplyTextEditKind::DeleteExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                (old, String::new())
            }
            ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
                let anchor = edit
                    .anchor_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        edit_field_error(index, kind, "anchor_text must be non-empty")
                    })?;
                let new = edit
                    .new_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "new_text must be non-empty"))?;
                (anchor, new.to_string())
            }
        };
        if needle.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "match text cannot contain NUL bytes",
            ));
        }
        if replacement.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "replacement text cannot contain NUL bytes",
            ));
        }
        let needle = canonicalize_apply_text_line_endings(needle, line_ending)
            .map_err(|error| edit_match_error(index, kind, error))?;
        let replacement = canonicalize_apply_text_line_endings(&replacement, line_ending)
            .map_err(|error| edit_field_error(index, kind, error))?
            .into_owned();
        let needle = needle.as_ref();
        let (start, end) =
            crate::apply_edits_shared::resolve_apply_text_match(original, needle, edit.occurrence)
                .map_err(|conflict| {
                    use crate::apply_edits_shared::ApplyTextMatchConflictKind;
                    let message = match conflict.kind {
                        ApplyTextMatchConflictKind::MatchNotFound => {
                            "match text was not found".to_string()
                        }
                        ApplyTextMatchConflictKind::MultipleMatches => format!(
                            "match text matched {} times; refusing ambiguous edit",
                            conflict.match_count
                        ),
                        ApplyTextMatchConflictKind::OccurrenceOutOfRange => format!(
                            "requested occurrence {} is out of range for {} exact matches",
                            conflict.requested_occurrence.unwrap_or(0),
                            conflict.match_count
                        ),
                    };
                    edit_match_error(index, kind, &message)
                })?;
        let (range_start, range_end) = match kind {
            ApplyTextEditKind::InsertBefore => (start, start),
            ApplyTextEditKind::InsertAfter => (end, end),
            _ => (start, end),
        };
        ops.push((range_start, range_end, replacement, index));
    }

    // Stable sort by (start, end, original index) so the slice build is
    // deterministic and ties (e.g. multiple inserts at one point) keep caller
    // order.
    ops.sort_by_key(|&(s, e, _, i)| (s, e, i));

    // Reject overlapping edits: a later op must not start before an earlier
    // op ends. Zero-width ops (inserts) never trigger this because their
    // start == end.
    for w in ops.windows(2) {
        let (_, e1, _, _) = w[0];
        let (s2, _, _, _) = w[1];
        if s2 < e1 {
            return Err(recoverable_write_rejection(
                "edits overlap; refusing ambiguous atomic edit batch",
            ));
        }
    }

    // Build the new content by slicing the original at op boundaries.
    let mut new_content = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut edit_summaries: Vec<Value> = Vec::with_capacity(ops.len());
    for &(start, end, ref replacement, index) in &ops {
        new_content.push_str(&original[cursor..start]);
        new_content.push_str(replacement);
        cursor = end;
        let edit = &edits[index];
        let old_start_line = 1 + original[..start].matches('\n').count();
        let mut old_end_line = 1 + original[..end].matches('\n').count();
        if end > start && end <= original.len() && original.as_bytes()[end - 1] == b'\n' {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end == start {
            old_end_line = old_start_line;
        }
        let new_line_count = if replacement.is_empty() {
            0
        } else {
            replacement.lines().count()
        };
        edit_summaries.push(json!({
            "index": index,
            "kind": edit.kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": new_line_count,
        }));
    }
    new_content.push_str(&original[cursor..]);
    let new_content = restore_apply_text_line_endings(new_content, line_ending);

    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let changed = new_content != raw_original;
    let output = json!({
        "path": path,
        "dry_run": dry_run,
        "applied_count": edits.len(),
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "changed": changed,
        "would_change": changed,
        "edits": edit_summaries,
        "changed_paths": [path],
    });
    Ok((new_content, output))
}

#[cfg(test)]
fn edit_field_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with corrected edit fields.",
        index,
        kind.as_str(),
        msg
    )
}

#[cfg(test)]
fn edit_match_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact match text.",
        index,
        kind.as_str(),
        msg
    )
}

impl ToolRuntime {
    pub(crate) async fn delete_project_files(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };

        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(client_id) => client_id.to_string(),
                Err(error) => return ToolResult::err(error),
            };
            // Pre-check is a non-authoritative optimization only: the
            // authoritative capability decision happens under the registry lock
            // at enqueue time, so a client that re-registers without
            // structured_file_delete between here and the enqueue falls back to
            // the legacy shell path instead of receiving an unknown file op.
            let supports_structured_delete = self
                .shell_clients
                .get_client_feature_set(&client_id)
                .await
                .map(|features| {
                    features.supports(crate::shell_client::RunnerFeature::StructuredFileDelete)
                })
                .unwrap_or(false);
            if supports_structured_delete {
                return self
                    .delete_project_files_structured_agent(&proj, client_id, paths, project, 30)
                    .await;
            }
            return self.delete_project_files_legacy_shell(project, paths).await;
        }

        self.delete_project_files_local(&proj, paths)
    }

    /// Rolling-upgrade compatibility for Runner binaries that predate
    /// `structured_file_delete`. This deliberately preserves the historical
    /// POSIX `rm -f -- ...` path and is therefore not a new cross-platform
    /// execution contract. Retirement condition: once the supported Runner
    /// fleet requires `structured_file_delete`, remove this fallback and its
    /// compatibility tests rather than extending shell quoting to new platforms.
    async fn delete_project_files_legacy_shell(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        // Admission reaches this path only while mixed-version Runner support is
        // still required. Never retry here after a possibly-dispatched structured
        // delete; that uncertainty is handled by the structured lifecycle path.
        let command = format!("rm -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            let stdout_present = result
                .output
                .get("stdout_tail")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            let stderr_present = result
                .output
                .get("stderr_tail")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            ToolResult::ok(json!({
                "ok": true,
                "deleted_paths": paths,
                "missing_paths": [],
                "refused_paths": [],
                "stdout_present": stdout_present,
                "stderr_present": stderr_present,
            }))
        } else {
            result
        }
    }

    /// Bounded structured-delete failure result. Projects the shared
    /// `ShellCommandExecutionState` to the two authoritative effect states for
    /// a mutation: `not_started` (registry evidence proves the request was
    /// never dispatched) and `outcome_unknown` (dispatched, or dispatch cannot
    /// be proven false — the Runner may already have deleted files). No
    /// shell-command facts (`command_started` / `command_completed` /
    /// `command_ok`) are emitted: a structured file operation has no command
    /// lifecycle, and emitting command fields merely for symmetry would lie.
    fn delete_project_files_lifecycle_failure(
        message: impl Into<String>,
        state: ShellCommandExecutionState,
    ) -> ToolResult {
        let (execution_state, failure_kind) = match state {
            ShellCommandExecutionState::NotStarted => ("not_started", "not_started"),
            _ => ("outcome_unknown", "outcome_unknown"),
        };
        ToolResult::err_with_output(
            message.into(),
            json!({
                "execution_state": execution_state,
                "failure_kind": failure_kind,
                "tool_failure": true,
            }),
        )
    }

    /// Structured-delete effect-boundary failure message: the request may
    /// already have executed, so the model must inspect workspace state rather
    /// than blindly retrying.
    fn delete_project_files_outcome_unknown_message() -> String {
        "agent delete_project_files outcome is unknown; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
            .to_string()
    }

    /// Structured-delete not-started failure message: registry evidence proved
    /// the request was never handed to the Runner, so nothing was executed.
    fn delete_project_files_not_started_message() -> String {
        "agent delete_project_files was not dispatched; the structured delete did not start and the Runner executed no deletion from this request."
            .to_string()
    }

    pub(crate) async fn delete_project_files_structured_agent(
        &self,
        proj: &ProjectConfig,
        client_id: String,
        paths: Vec<String>,
        project: String,
        wait_timeout_secs: u64,
    ) -> ToolResult {
        let payload = match serde_json::to_string(&json!({"paths": paths})) {
            Ok(payload) => payload,
            Err(_) => return ToolResult::err("failed to encode delete_project_files request"),
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_structured_file_delete(
                ShellFileOpRequest {
                    op: "delete_project_files".to_string(),
                    client_id,
                    path: ".".to_string(),
                    cwd: Some(proj.path.clone()),
                    content: Some(payload),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout_secs,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) if error.starts_with("capability_unavailable:") => {
                // The client re-registered without structured_file_delete
                // between the pre-check and this authoritative enqueue; the
                // registry queued nothing and no mutation can have happened.
                // The legacy shell path is the rolling-upgrade fallback every
                // Runner generation supports. This is the ONLY case that may
                // fall back after a structured attempt.
                return self.delete_project_files_legacy_shell(project, paths).await;
            }
            Err(_) => return ToolResult::err("agent delete_project_files is unavailable"),
        };
        let response = tokio::time::timeout(Duration::from_secs(wait_timeout_secs + 2), rx).await;
        let response = match response {
            Ok(Ok(response)) => {
                // Authoritative effect-boundary evidence: only a definite
                // terminal success enters the success path; every other
                // response classifies as not_started (registry-proven
                // undispatch, e.g. runner replacement before poll) or
                // outcome_unknown (dispatched, or dispatch cannot be proven
                // false — the Runner may already have deleted files).
                let state = agent_command_lifecycle(&response, wait_timeout_secs);
                match state {
                    ShellCommandExecutionState::Completed
                        if response.error.is_none() && response.exit_code == Some(0) =>
                    {
                        response
                    }
                    ShellCommandExecutionState::NotStarted => {
                        return Self::delete_project_files_lifecycle_failure(
                            Self::delete_project_files_not_started_message(),
                            state,
                        )
                    }
                    _ => {
                        return Self::delete_project_files_lifecycle_failure(
                            Self::delete_project_files_outcome_unknown_message(),
                            state,
                        )
                    }
                }
            }
            Ok(Err(_)) => {
                // Waiter channel closed. Atomically remove the pending request
                // and classify from recovered dispatch truth; a missing record
                // cannot prove undispatch, so only explicit `Some(false)` is
                // not_started.
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let state = dispatch_uncertainty_lifecycle(dispatch);
                if state == ShellCommandExecutionState::NotStarted {
                    return Self::delete_project_files_lifecycle_failure(
                        Self::delete_project_files_not_started_message(),
                        state,
                    );
                }
                return Self::delete_project_files_lifecycle_failure(
                    "agent delete_project_files waiter was dropped and dispatch cannot be proven false; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
                        .to_string(),
                    state,
                );
            }
            Err(_) => {
                // Wait timeout: cancel with dispatch truth instead of erasing
                // it. A timed-out mutation that may have dispatched must never
                // be presented as definitely not started.
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let state = dispatch_uncertainty_lifecycle(dispatch);
                if state == ShellCommandExecutionState::NotStarted {
                    return Self::delete_project_files_lifecycle_failure(
                        format!(
                            "timed out waiting {wait_timeout_secs} seconds for agent delete_project_files before dispatch; the structured delete did not start and the Runner executed no deletion from this request."
                        ),
                        state,
                    );
                }
                return Self::delete_project_files_lifecycle_failure(
                    format!(
                        "timed out waiting {wait_timeout_secs} seconds for agent delete_project_files; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
                    ),
                    state,
                );
            }
        };
        let output: Value =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(output) => output,
                Err(_) => {
                    return Self::delete_project_files_lifecycle_failure(
                        Self::delete_project_files_outcome_unknown_message(),
                        ShellCommandExecutionState::OutcomeUnknown,
                    )
                }
            };
        let expected = json!(paths);
        if output.get("deleted_paths") != Some(&expected)
            || output.get("missing_paths") != Some(&json!([]))
            || output.get("refused_paths") != Some(&json!([]))
        {
            return Self::delete_project_files_lifecycle_failure(
                Self::delete_project_files_outcome_unknown_message(),
                ShellCommandExecutionState::OutcomeUnknown,
            );
        }
        ToolResult::ok(json!({
            "ok": true,
            "deleted_paths": paths,
            "missing_paths": [],
            "refused_paths": [],
            "stdout_present": false,
            "stderr_present": false,
        }))
    }

    fn delete_project_files_local(&self, proj: &ProjectConfig, paths: Vec<String>) -> ToolResult {
        let canonical_root = match proj.root().canonicalize() {
            Ok(root) => root,
            Err(_) => return ToolResult::err("project root is unavailable"),
        };
        for path in &paths {
            let target = canonical_root.join(path);
            match std::fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if metadata.file_type().is_dir() {
                        return ToolResult::err("delete_project_files refuses directory targets");
                    }
                    let containment = if metadata.file_type().is_symlink() {
                        target
                            .parent()
                            .and_then(|parent| parent.canonicalize().ok())
                    } else {
                        target.canonicalize().ok()
                    };
                    if !containment.as_ref().is_some_and(|candidate| {
                        webcodex_runner_config::paths::path_is_within(candidate, &canonical_root)
                    }) {
                        return ToolResult::err(
                            "delete_project_files target is outside the project",
                        );
                    }
                    match std::fs::remove_file(&target) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => return ToolResult::err("delete_project_files failed"),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut ancestor = target.parent();
                    let mut contained = false;
                    while let Some(candidate) = ancestor {
                        match candidate.canonicalize() {
                            Ok(candidate) => {
                                contained = webcodex_runner_config::paths::path_is_within(
                                    &candidate,
                                    &canonical_root,
                                );
                                break;
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                ancestor = candidate.parent();
                            }
                            Err(_) => {
                                return ToolResult::err(
                                    "delete_project_files parent is unavailable",
                                )
                            }
                        }
                    }
                    if !contained {
                        return ToolResult::err(
                            "delete_project_files target is outside the project",
                        );
                    }
                }
                Err(_) => return ToolResult::err("delete_project_files failed"),
            }
        }
        ToolResult::ok(json!({
            "ok": true,
            "deleted_paths": paths,
            "missing_paths": [],
            "refused_paths": [],
            "stdout_present": false,
            "stderr_present": false,
        }))
    }

    // -------------------------------------------------------------------------
    pub(crate) async fn write_project_file(
        &self,
        project: String,
        path: String,
        content: String,
        overwrite: Option<bool>,
        expected_sha256: Option<String>,
        expected_content_prefix: Option<String>,
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if content.contains('\0') {
            return ToolResult::err("content cannot contain NUL bytes");
        }
        if content.len() > MAX_WRITE_CONTENT_BYTES {
            return ToolResult::err(format!(
                "content too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        if let Some(hash) = expected_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "write_project_file requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path.clone(),
            "content": content,
            "overwrite": overwrite.unwrap_or(false),
            "expected_sha256": expected_sha256,
            "expected_content_prefix": expected_content_prefix,
        });
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "write_project_file",
                payload,
                "write_project_file",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn apply_text_edits(
        &self,
        project: String,
        changes: Vec<ApplyFileChangeInput>,
        dry_run: Option<bool>,
    ) -> ToolResult {
        if changes.is_empty() {
            return apply_text_edits_preflight_rejection(
                "changes must contain at least one file change",
                "empty_batch",
                None,
                None,
                None,
                None,
                "add at least one valid file change and retry",
            );
        }
        if changes.len() > MAX_APPLY_FILE_CHANGES {
            return apply_text_edits_preflight_rejection(
                format!(
                    "too many file changes; maximum is {}",
                    MAX_APPLY_FILE_CHANGES
                ),
                "batch_too_large",
                None,
                None,
                None,
                None,
                "reduce the batch to the supported file-change limit and retry",
            );
        }
        let mut touched_paths = HashSet::new();
        for (change_index, change) in changes.iter().enumerate() {
            if let Err(error) = validate_edit_file_path(&change.path) {
                return apply_text_edits_path_policy_rejection(
                    change_index,
                    change.kind.as_str(),
                    &change.path,
                    error,
                );
            }
            if !touched_paths.insert(change.path.as_str()) {
                return apply_text_edits_preflight_rejection(
                    format!(
                        "change {change_index} reuses path '{}'; each source/destination path may appear only once",
                        change.path
                    ),
                    "path_overlap",
                    Some(change_index),
                    None,
                    Some(change.kind.as_str()),
                    Some(&change.path),
                    "correct the duplicate source/destination path and retry the whole batch",
                );
            }
            if let Some(to_path) = change.to_path.as_deref() {
                if let Err(error) = validate_edit_file_path(to_path) {
                    return apply_text_edits_path_policy_rejection(
                        change_index,
                        change.kind.as_str(),
                        to_path,
                        error,
                    );
                }
                if !touched_paths.insert(to_path) {
                    return apply_text_edits_preflight_rejection(
                        format!(
                            "change {change_index} reuses destination path '{to_path}'; each source/destination path may appear only once"
                        ),
                        "path_overlap",
                        Some(change_index),
                        None,
                        Some(change.kind.as_str()),
                        Some(to_path),
                        "correct the duplicate source/destination path and retry the whole batch",
                    );
                }
            }
            if let Err(validation_error) = validate_apply_file_change(change_index, change) {
                let failed_kind = validation_error
                    .edit_index
                    .and_then(|edit_index| change.edits.get(edit_index))
                    .map(|edit| edit.kind.as_str())
                    .unwrap_or_else(|| change.kind.as_str());
                return apply_text_edits_preflight_rejection(
                    validation_error.message,
                    if validation_error.edit_index.is_some() {
                        "invalid_edit"
                    } else {
                        "invalid_change"
                    },
                    Some(change_index),
                    validation_error.edit_index,
                    Some(failed_kind),
                    Some(&change.path),
                    "correct the rejected change or edit and retry the whole batch",
                );
            }
        }
        let requires_occurrence_capability = changes
            .iter()
            .flat_map(|change| change.edits.iter())
            .any(|edit| edit.occurrence.is_some());

        let payload = json!({
            "changes": changes,
            "dry_run": dry_run.unwrap_or(false),
            "recovery_metadata_version": 1,
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) if serialized.len() <= MAX_APPLY_FILE_CHANGES_BYTES => serialized,
            Ok(_) => {
                return apply_text_edits_preflight_rejection(
                    format!(
                        "serialized file changes exceed {} bytes",
                        MAX_APPLY_FILE_CHANGES_BYTES
                    ),
                    "payload_too_large",
                    None,
                    None,
                    None,
                    None,
                    "reduce the batch payload size and retry",
                )
            }
            Err(error) => {
                return apply_text_edits_preflight_rejection(
                    format!("failed to serialize file changes payload: {error}"),
                    "serialization_failed",
                    None,
                    None,
                    None,
                    None,
                    "correct the request payload and retry",
                )
            }
        };

        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "apply_text_edits requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let wait_timeout = 60_u64;
        let routing_path = changes
            .first()
            .map(|change| change.path.clone())
            .expect("non-empty changes validated above");
        let request = ShellFileOpRequest {
            op: "apply_text_edits".to_string(),
            client_id,
            path: routing_path,
            cwd: Some(proj.path.clone()),
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
        let enqueue_result = if requires_occurrence_capability {
            self.shell_clients
                .enqueue_apply_text_edits_with_occurrence(request, "tool_runtime".to_string())
                .await
        } else {
            self.shell_clients
                .enqueue_file_op(request, "tool_runtime".to_string())
                .await
        };
        let (request_id, rx) = match enqueue_result {
            Ok(r) => r,
            Err(e)
                if requires_occurrence_capability && e.starts_with("capability_unavailable:") =>
            {
                return apply_text_edit_occurrence_capability_rejection(e)
            }
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("agent apply_text_edits request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent apply_text_edits");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || {
                    format!(
                        "agent apply_text_edits failed with code {:?}",
                        resp.exit_code
                    )
                },
            )));
        }
        apply_text_edits_agent_stdout_result(&resp.stdout.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_text_edits_path_policy_recovery_omits_untrusted_path_metadata() {
        let result = apply_text_edits_path_policy_rejection(
            2,
            "edit",
            "/private/secret.txt",
            "path must be project-relative".to_string(),
        );

        assert!(!result.success);
        assert_eq!(result.output["state_changed"], false);
        assert_eq!(result.output["error_kind"], "policy_rejected");
        assert_eq!(result.output["change_index"], 2);
        assert_eq!(result.output["kind"], "edit");
        assert!(result.output.get("path").is_none());
        assert!(result.output.get("error").is_none());
        assert!(!serde_json::to_string(&result.output)
            .unwrap()
            .contains("/private/secret.txt"));
    }

    #[test]
    fn incomplete_apply_text_edits_rollback_is_not_reported_as_no_write() {
        let result = apply_text_edits_agent_stdout_result(
            r#"{"changed":true,"state_changed":false,"rollback_complete":false,"retry_guidance":"retry directly","conflict_recovery":{"schema_version":1,"conflict_kind":"multiple_matches","occurrence_selector_supported":true,"direct_retry_safe":true,"reread_required":false,"recovery_action":"select_occurrence_or_refine_match"},"error":"rollback failed"}"#,
        );

        assert!(!result.success);
        assert_eq!(result.output["rollback_complete"], false);
        assert!(result.output.get("conflict_recovery").is_none());
        assert!(result.output.get("retry_guidance").is_none());
        assert!(result.output.get("state_changed").is_none());
        let error = result.error.unwrap();
        assert!(error.contains("uncertain"));
        assert!(!error.contains("No files were modified"));
    }
}
