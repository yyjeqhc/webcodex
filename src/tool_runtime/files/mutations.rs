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

fn structured_edit_not_started_result(tool_name: &str, reason: impl AsRef<str>) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "{tool_name} was not dispatched: {}. No files were modified by this request. Restore Runner availability, then retry with the same guards.",
            reason.as_ref()
        ),
        json!({
            "execution_state": "not_started",
            "state_changed": false,
            "error_kind": "not_started",
            "failure_kind": "not_started",
            "tool_failure": true,
            "recovery_action": "retry_same_after_runner_recovery",
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::RetrySame, None)
}

fn structured_edit_outcome_unknown_result(
    tool_name: &str,
    reason: impl AsRef<str>,
    mut output: Value,
) -> ToolResult {
    if !output.is_object() {
        output = json!({});
    }
    let fields = output
        .as_object_mut()
        .expect("structured edit uncertainty output normalized to object");
    for key in [
        "conflict_recovery",
        "retry_guidance",
        "state_changed",
        "execution_state",
        "error_kind",
        "failure_kind",
        "tool_failure",
        "recovery_action",
        "recovery_kind",
        "recovery_tool",
        "error",
    ] {
        fields.remove(key);
    }
    fields.insert("execution_state".to_string(), json!("outcome_unknown"));
    fields.insert("state_changed".to_string(), Value::Null);
    fields.insert("error_kind".to_string(), json!("outcome_unknown"));
    fields.insert("failure_kind".to_string(), json!("outcome_unknown"));
    fields.insert("tool_failure".to_string(), json!(true));
    fields.insert(
        "recovery_action".to_string(),
        json!("inspect_workspace_before_retry"),
    );
    ToolResult::err_with_output(
        format!(
            "{tool_name} outcome is unknown: {}. The Runner may already have changed files. Inspect current workspace state before issuing another write.",
            reason.as_ref()
        ),
        output,
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::Reobserve, None)
}

fn structured_edit_delivery_failure(
    tool_name: &str,
    reason: impl AsRef<str>,
    state: ShellCommandExecutionState,
) -> ToolResult {
    if state == ShellCommandExecutionState::NotStarted {
        structured_edit_not_started_result(tool_name, reason)
    } else {
        structured_edit_outcome_unknown_result(tool_name, reason, json!({}))
    }
}

async fn await_structured_edit_response(
    runtime: &ToolRuntime,
    request_id: &str,
    rx: tokio::sync::oneshot::Receiver<ShellRunResponse>,
    wait_timeout_secs: u64,
    tool_name: &str,
) -> Result<ShellRunResponse, ToolResult> {
    match tokio::time::timeout(Duration::from_secs(wait_timeout_secs + 4), rx).await {
        Ok(Ok(response)) => {
            if response.request_dispatched == Some(false) {
                return Err(structured_edit_not_started_result(
                    tool_name,
                    "the returned lifecycle evidence proves the request never left the queue",
                ));
            }
            if response.error.is_some() {
                return Err(structured_edit_outcome_unknown_result(
                    tool_name,
                    "the Runner returned a transport or execution error after dispatch",
                    json!({}),
                ));
            }
            if response.exit_code != Some(0) {
                return Err(structured_edit_outcome_unknown_result(
                    tool_name,
                    "the Runner returned a non-zero terminal result after dispatch",
                    json!({}),
                ));
            }
            Ok(response)
        }
        Ok(Err(_)) => {
            let state = dispatch_uncertainty_lifecycle(
                runtime
                    .shell_clients
                    .cancel_request_dispatch_state(request_id)
                    .await,
            );
            Err(structured_edit_delivery_failure(
                tool_name,
                "the Runner response channel closed before a trustworthy result was received",
                state,
            ))
        }
        Err(_) => {
            let state = dispatch_uncertainty_lifecycle(
                runtime
                    .shell_clients
                    .cancel_request_dispatch_state(request_id)
                    .await,
            );
            Err(structured_edit_delivery_failure(
                tool_name,
                format!("no trustworthy result arrived within {wait_timeout_secs} seconds"),
                state,
            ))
        }
    }
}

fn write_project_file_preflight_rejection(
    detail: impl Into<String>,
    error_kind: &'static str,
    retry_guidance: &'static str,
) -> ToolResult {
    let detail = detail.into();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {detail}.\nNo files were modified.\nRetry guidance: {retry_guidance}."
        ),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": error_kind,
            "retry_guidance": retry_guidance,
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::FixInput, None)
}

fn apply_text_edit_occurrence_capability_rejection(reason: impl AsRef<str>) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: the accepted Runner violated the generation-2 apply_text_edit_occurrence baseline; reconnect it or refine the edit to a unique exact match without occurrence."
        ),
        json!({
            "state_changed": false,
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::shell_protocol::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
            "retry_guidance": "reconnect the Runner or refine the edit to a unique exact match without occurrence"
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
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": crate::shell_protocol::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_LINE_SCOPE,
            "retry_guidance": "reconnect a Runner with apply_text_edit_line_scope support; never silently downgrade a scoped edit to an unscoped edit"
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
    if let Some(line_scope) = edit.line_scope {
        if let Err(reason) = line_scope.validate() {
            return Err(format!(
                "change {change_index} edit {edit_index} ({}): {reason}",
                edit.kind.as_str()
            ));
        }
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

fn apply_text_edits_agent_stdout_result(
    stdout: &str,
    expected_change_count: usize,
    expected_dry_run: bool,
) -> ToolResult {
    let stdout = stdout.trim();
    let mut obj: Value = match serde_json::from_str::<Value>(stdout) {
        Ok(value) if value.is_object() => value,
        _ => {
            return structured_edit_outcome_unknown_result(
                "apply_text_edits",
                "the Runner returned malformed or non-object JSON after dispatch",
                json!({}),
            )
        }
    };
    if let Some(error) = obj.get("error").and_then(Value::as_str).map(str::to_string) {
        let rollback_complete = obj.get("rollback_complete").and_then(Value::as_bool);
        let changed = obj.get("changed").and_then(Value::as_bool);
        let state_changed = obj.get("state_changed").and_then(Value::as_bool);
        let uncertain = rollback_complete == Some(false)
            || changed == Some(true)
            || state_changed == Some(true);
        let no_effect_proven = !uncertain
            && (rollback_complete == Some(true)
                || changed == Some(false)
                || state_changed == Some(false));
        if !no_effect_proven {
            return structured_edit_outcome_unknown_result("apply_text_edits", error, obj);
        }
        obj["changed"] = json!(false);
        obj["state_changed"] = json!(false);
        obj["execution_state"] = if rollback_complete == Some(true) {
            json!("completed")
        } else {
            json!("not_started")
        };
        let message = if obj.get("conflict_recovery").is_some_and(Value::is_object)
            || error.contains("No files were modified")
            || error.contains("was rolled back")
        {
            error
        } else {
            recoverable_write_rejection(&error)
        };
        return ToolResult::err_with_output(message, obj);
    }

    let dry_run = obj.get("dry_run").and_then(Value::as_bool);
    let applied_count = obj.get("applied_count").and_then(Value::as_u64);
    let changed = obj.get("changed").and_then(Value::as_bool);
    let would_change = obj.get("would_change").and_then(Value::as_bool);
    let valid = dry_run == Some(expected_dry_run)
        && applied_count == Some(expected_change_count as u64)
        && changed.is_some()
        && would_change.is_some()
        && !(expected_dry_run && changed == Some(true))
        && (expected_dry_run || changed == would_change);
    if !valid {
        return structured_edit_outcome_unknown_result(
            "apply_text_edits",
            "the Runner success payload omitted or contradicted authoritative edit-effect fields",
            obj,
        );
    }
    obj["state_changed"] = json!(changed.expect("validated changed field"));
    obj["execution_state"] = json!("completed");
    ToolResult::ok(obj)
}

fn write_project_file_agent_stdout_result(stdout: &str) -> ToolResult {
    let stdout = stdout.trim();
    let mut obj: Value = match serde_json::from_str::<Value>(stdout) {
        Ok(value) if value.is_object() => value,
        _ => {
            return structured_edit_outcome_unknown_result(
                "write_project_file",
                "the Runner returned malformed or non-object JSON after dispatch",
                json!({}),
            )
        }
    };
    if let Some(error) = obj.get("error").and_then(Value::as_str).map(str::to_string) {
        let changed = obj.get("changed").and_then(Value::as_bool);
        let state_changed = obj.get("state_changed").and_then(Value::as_bool);
        let execution_state = obj.get("execution_state").and_then(Value::as_str);
        if changed != Some(false) || state_changed != Some(false) {
            return structured_edit_outcome_unknown_result("write_project_file", error, obj);
        }
        match execution_state {
            Some("not_started") => {
                obj["changed"] = json!(false);
                obj["state_changed"] = json!(false);
                obj["execution_state"] = json!("not_started");
                return ToolResult::err_with_output(error, obj);
            }
            Some("completed") => {
                obj["changed"] = json!(false);
                obj["state_changed"] = json!(false);
                obj["execution_state"] = json!("completed");
                return ToolResult::err_with_output(error, obj);
            }
            _ => return structured_edit_outcome_unknown_result("write_project_file", error, obj),
        }
    }
    let Some(changed) = obj.get("changed").and_then(Value::as_bool) else {
        return structured_edit_outcome_unknown_result(
            "write_project_file",
            "the Runner success payload omitted the authoritative changed field",
            obj,
        );
    };
    let state_changed = obj.get("state_changed").and_then(Value::as_bool);
    let execution_state = obj.get("execution_state").and_then(Value::as_str);
    if state_changed != Some(changed) || execution_state != Some("completed") {
        return structured_edit_outcome_unknown_result(
            "write_project_file",
            "the Runner success payload omitted or contradicted authoritative write-effect fields",
            obj,
        );
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
        if let Some(line_scope) = edit.line_scope {
            line_scope
                .validate()
                .map_err(|reason| edit_field_error(index, kind, reason))?;
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
        let (start, end) = crate::apply_edits_shared::resolve_apply_text_match(
            original,
            needle,
            edit.occurrence,
            edit.line_scope.as_ref(),
        )
        .map_err(|conflict| {
            use crate::apply_edits_shared::ApplyTextMatchConflictKind;
            let message = match conflict.kind {
                ApplyTextMatchConflictKind::MatchNotFound if conflict.line_scope.is_some() => {
                    "match text was not found within line_scope".to_string()
                }
                ApplyTextMatchConflictKind::MatchNotFound => "match text was not found".to_string(),
                ApplyTextMatchConflictKind::MultipleMatches => format!(
                    "match text matched {} times{}; refusing ambiguous edit",
                    conflict
                        .line_scope_match_count
                        .unwrap_or(conflict.match_count),
                    if conflict.line_scope.is_some() {
                        " within line_scope"
                    } else {
                        ""
                    }
                ),
                ApplyTextMatchConflictKind::OccurrenceOutOfRange => format!(
                    "requested occurrence {} is out of range for {} exact matches",
                    conflict.requested_occurrence.unwrap_or(0),
                    conflict.match_count
                ),
                ApplyTextMatchConflictKind::OccurrenceOutsideLineScope => format!(
                    "requested occurrence {} is outside line_scope",
                    conflict.requested_occurrence.unwrap_or(0)
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
            return self
                .delete_project_files_structured_agent(&proj, client_id, paths, 30)
                .await;
        }

        self.delete_project_files_local(&proj, paths)
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
                return Self::delete_project_files_lifecycle_failure(
                    format!("agent delete_project_files generation-2 capability invariant failed: {error}"),
                    ShellCommandExecutionState::NotStarted,
                );
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
        if output.get("deleted_paths") != Some(&expected) {
            return Self::delete_project_files_lifecycle_failure(
                Self::delete_project_files_outcome_unknown_message(),
                ShellCommandExecutionState::OutcomeUnknown,
            );
        }
        ToolResult::ok(json!({
            "ok": true,
            "state_changed": !paths.is_empty(),
            "deleted_paths": paths,
            "stdout_present": false,
            "stderr_present": false,
        }))
    }

    fn delete_project_files_local(&self, proj: &ProjectConfig, paths: Vec<String>) -> ToolResult {
        let canonical_root = match proj.root().canonicalize() {
            Ok(root) => root,
            Err(_) => return ToolResult::err("project root is unavailable"),
        };
        let mut state_changed = false;
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
                        Ok(()) => state_changed = true,
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
            "state_changed": state_changed,
            "deleted_paths": paths,
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
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if content.contains('\0') {
            return write_project_file_preflight_rejection(
                "content cannot contain NUL bytes",
                "invalid_content",
                "remove the NUL byte and retry",
            );
        }
        if content.len() > MAX_WRITE_CONTENT_BYTES {
            return write_project_file_preflight_rejection(
                format!("content exceeds {MAX_WRITE_CONTENT_BYTES} bytes"),
                "content_too_large",
                "use apply_text_edits for a smaller local change or reduce the full rewrite",
            );
        }
        let overwrite = overwrite.unwrap_or(false);
        if let Some(hash) = expected_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return write_project_file_preflight_rejection(
                    "expected_sha256 must be a lowercase 64-character hex digest",
                    "invalid_expected_sha256",
                    "reread the file and provide its exact current sha256",
                );
            }
        }
        match (overwrite, expected_sha256.is_some()) {
            (true, false) => {
                return write_project_file_preflight_rejection(
                    "overwrite=true requires expected_sha256",
                    "missing_expected_sha256",
                    "reread the existing file and retry with its exact current sha256",
                )
            }
            (false, true) => {
                return write_project_file_preflight_rejection(
                    "expected_sha256 is allowed only when overwrite=true",
                    "unexpected_expected_sha256",
                    "omit expected_sha256 for a new-file create, or set overwrite=true for an existing-file rewrite",
                )
            }
            _ => {}
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
            "overwrite": overwrite,
            "expected_sha256": expected_sha256,
        });
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "write_project_file".to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(proj.path.clone()),
                    content: Some(payload.to_string()),
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
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(_) => {
                return structured_edit_not_started_result(
                    "write_project_file",
                    "the Runner queue rejected the request before dispatch",
                )
            }
        };
        let response = match await_structured_edit_response(
            self,
            &request_id,
            rx,
            wait_timeout,
            "write_project_file",
        )
        .await
        {
            Ok(response) => response,
            Err(result) => return result,
        };
        write_project_file_agent_stdout_result(&response.stdout.unwrap_or_default())
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
        let requires_line_scope_capability = changes
            .iter()
            .flat_map(|change| change.edits.iter())
            .any(|edit| edit.line_scope.is_some());

        let expected_change_count = changes.len();
        let expected_dry_run = dry_run.unwrap_or(false);
        let payload = json!({
            "changes": changes,
            "dry_run": expected_dry_run,
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
        let enqueue_result = if requires_line_scope_capability {
            self.shell_clients
                .enqueue_apply_text_edits_with_line_scope(
                    request,
                    "tool_runtime".to_string(),
                    requires_occurrence_capability,
                )
                .await
        } else if requires_occurrence_capability {
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
                if e.starts_with("capability_unavailable:")
                    && e.contains(
                        crate::shell_protocol::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_LINE_SCOPE,
                    ) =>
            {
                return apply_text_edit_line_scope_capability_rejection(e)
            }
            Err(e)
                if e.starts_with("capability_unavailable:")
                    && e.contains(
                        crate::shell_protocol::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
                    ) =>
            {
                return apply_text_edit_occurrence_capability_rejection(e)
            }
            Err(_) => {
                return structured_edit_not_started_result(
                    "apply_text_edits",
                    "the Runner queue rejected the request before dispatch",
                )
            }
        };
        let response = match await_structured_edit_response(
            self,
            &request_id,
            rx,
            wait_timeout,
            "apply_text_edits",
        )
        .await
        {
            Ok(response) => response,
            Err(result) => return result,
        };
        apply_text_edits_agent_stdout_result(
            &response.stdout.unwrap_or_default(),
            expected_change_count,
            expected_dry_run,
        )
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
            1,
            false,
        );

        assert!(!result.success);
        assert_eq!(result.output["rollback_complete"], false);
        assert!(result.output.get("conflict_recovery").is_none());
        assert!(result.output.get("retry_guidance").is_none());
        assert!(result.output.get("error").is_none());
        assert!(result.output["state_changed"].is_null());
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        let error = result.error.unwrap();
        assert!(error.contains("outcome is unknown"));
        assert!(!error.contains("No files were modified"));
    }

    #[test]
    fn write_project_file_effect_payload_requires_complete_consistent_state() {
        for payload in [
            r#"{"changed":false,"execution_state":"completed","error":"missing state_changed"}"#,
            r#"{"changed":false,"state_changed":true,"execution_state":"completed","error":"contradictory effect"}"#,
            r#"{"changed":false,"state_changed":false,"execution_state":"outcome_unknown","error":"uncertain"}"#,
            r#"{"changed":false,"execution_state":"completed"}"#,
        ] {
            let result = write_project_file_agent_stdout_result(payload);
            assert!(!result.success);
            assert_eq!(result.output["execution_state"], "outcome_unknown");
            assert!(result.output["state_changed"].is_null());
            assert!(result.output.get("error").is_none());
        }

        let rolled_back = write_project_file_agent_stdout_result(
            r#"{"changed":false,"state_changed":false,"execution_state":"completed","error":"write failed and parent creation was rolled back"}"#,
        );
        assert!(!rolled_back.success);
        assert_eq!(rolled_back.output["changed"], false);
        assert_eq!(rolled_back.output["state_changed"], false);
        assert_eq!(rolled_back.output["execution_state"], "completed");
    }
}
