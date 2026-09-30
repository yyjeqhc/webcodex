//! Shared edit-path policy, read-revision fences, and bounded reread guidance.

use super::*;

pub(super) fn read_files_recovery(project: &str, path: &str) -> Value {
    crate::tool_runtime::SuggestedToolCall::mechanically_followable(
        "read_files",
        json!({"project": project, "items": [{"path": path}]}),
    )
    .to_value()
}
pub(super) fn recoverable_write_rejection(reason: impl AsRef<str>) -> String {
    format!(
        "Rejected before write: {}.\nNo files were modified.\nRetry guidance: read the file again to refresh line numbers/context, then retry with updated guards.",
        reason.as_ref()
    )
}

pub(super) fn read_revision_rejection(
    project: &str,
    path: &str,
    error: ReadRevisionLookupError,
) -> ToolResult {
    let (error_kind, detail) = match error {
        ReadRevisionLookupError::Unknown => (
            "unknown_read_revision",
            "the read revision is unknown, expired, evicted, or belongs to a prior Server runtime",
        ),
        ReadRevisionLookupError::ProjectMismatch => (
            "read_revision_project_mismatch",
            "the read revision belongs to a different Project",
        ),
        ReadRevisionLookupError::PathMismatch => (
            "read_revision_path_mismatch",
            "the read revision belongs to a different project-relative path",
        ),
        ReadRevisionLookupError::OwnerMismatch => (
            "read_revision_owner_mismatch",
            "the read revision belongs to a different Runner process or Project placement",
        ),
    };
    ToolResult::err_with_output(
        format!("Rejected before write: {detail}. No files were modified."),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": error_kind,
            "path": path,
            "recovery": read_files_recovery(project, path),
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::FixInput)
}

pub(super) fn read_revision_target(
    resolved: &ResolvedProject,
    path: &str,
    runner_instance_id: &str,
) -> ReadRevisionTarget {
    ReadRevisionTarget {
        project_id: resolved.resolved_id.clone(),
        path: path.to_string(),
        client_id: resolved.config.client_id.clone(),
        runner_instance_id: runner_instance_id.to_string(),
        project_root: resolved.config.path.clone(),
        root_fingerprint: resolved.root_fingerprint.clone(),
    }
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
    if p.has_root()
        || p.components()
            .any(|component| matches!(component, std::path::Component::Prefix(_)))
    {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_edit_path(path) {
        return Err(format!(
            "refusing sensitive path '{}': touches runner.toml, legacy agent.toml, webcodex.env, \
             .env, project-registry, projects.d, .git, target, or node_modules",
            path
        ));
    }
    Ok(())
}

pub(super) fn write_project_file_preflight_rejection(
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
    .with_recovery(crate::tool_runtime::RecoveryKind::FixInput)
}

pub(super) fn apply_text_edits_preflight_rejection(
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
        "execution_state": "not_started",
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

pub(super) fn apply_text_edits_path_policy_rejection(
    change_index: usize,
    kind: &str,
    path: &str,
    message: String,
) -> ToolResult {
    let mut result =
        crate::tool_runtime::permissions::edit_path_policy_rejected_result(path, message);
    if let Some(output) = result.output.as_object_mut() {
        // The rejected path may itself be absolute or sensitive. Keep exact
        // change provenance without copying that untrusted path into recovery
        // metadata.
        output.remove("path");
        output.remove("error");
    }
    result.output["execution_state"] = json!("not_started");
    result.output["change_index"] = json!(change_index);
    result.output["kind"] = json!(kind);
    result.output["retry_guidance"] =
        json!("correct the rejected project-relative path and retry the whole batch");
    result
}
