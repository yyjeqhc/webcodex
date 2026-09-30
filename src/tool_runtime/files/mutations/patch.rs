//! Patch admission and dispatch; untrusted receipts are checked separately.

use super::lifecycle::{await_structured_edit_response, structured_edit_not_started_result};
use super::patch_result::apply_patch_agent_stdout_result;
use super::preflight::{
    apply_text_edits_path_policy_rejection, apply_text_edits_preflight_rejection,
    validate_edit_file_path, write_project_file_preflight_rejection,
};
use super::*;

pub(super) fn apply_patch_capability_rejection(
    reason: impl AsRef<str>,
    capability: &'static str,
) -> ToolResult {
    let reason = reason.as_ref();
    ToolResult::err_with_output(
        format!(
            "Rejected before write: {reason}.\nNo files were modified.\nRetry guidance: reconnect a current Runner that explicitly supports {capability}."
        ),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": "agent_capability_unavailable",
            "failure_kind": "capability_unavailable",
            "capability": capability,
            "recovery_action": "upgrade_or_reconnect_runner",
            "retry_guidance": format!("reconnect or upgrade the Runner so it explicitly advertises {capability}")
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::RetrySame)
}
impl ToolRuntime {
    pub(crate) async fn apply_patch(
        &self,
        project: String,
        patch: String,
        dry_run: Option<bool>,
        matching_mode: Option<crate::apply_patch_shared::ApplyPatchMatchingMode>,
    ) -> ToolResult {
        let parsed = match crate::apply_patch_shared::parse_codex_patch(&patch) {
            Ok(parsed) => parsed,
            Err(error) => {
                return write_project_file_preflight_rejection(
                    error.to_string(),
                    error.kind,
                    "regenerate a valid Codex *** Begin Patch payload and retry",
                )
            }
        };
        let mut touched_paths = HashSet::new();
        for (change_index, hunk) in parsed.hunks.iter().enumerate() {
            let kind = hunk.kind();
            let path = hunk.path();
            if let Err(error) = validate_edit_file_path(path) {
                return apply_text_edits_path_policy_rejection(change_index, kind, path, error);
            }
            if !touched_paths.insert(path) {
                return apply_text_edits_preflight_rejection(
                    format!("change {change_index} reuses path '{path}'; each source/destination path may appear only once"),
                    "path_overlap",
                    Some(change_index),
                    None,
                    Some(kind),
                    Some(path),
                    "correct the duplicate source/destination path and retry the whole patch",
                );
            }
            if let Some(to_path) = hunk.move_path().filter(|to_path| *to_path != path) {
                if let Err(error) = validate_edit_file_path(to_path) {
                    return apply_text_edits_path_policy_rejection(
                        change_index,
                        kind,
                        to_path,
                        error,
                    );
                }
                if !touched_paths.insert(to_path) {
                    return apply_text_edits_preflight_rejection(
                        format!("change {change_index} reuses destination path '{to_path}'; each source/destination path may appear only once"),
                        "path_overlap",
                        Some(change_index),
                        None,
                        Some(kind),
                        Some(to_path),
                        "correct the duplicate source/destination path and retry the whole patch",
                    );
                }
            }
        }

        let expected_dry_run = dry_run.unwrap_or(false);
        let expected_matching_mode = matching_mode.unwrap_or_default();
        let payload = json!({
            "patch": patch,
            "dry_run": expected_dry_run,
            "matching_mode": expected_matching_mode.as_str(),
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) if serialized.len() <= MAX_APPLY_FILE_CHANGES_BYTES => serialized,
            Ok(_) => {
                return apply_text_edits_preflight_rejection(
                    format!(
                        "serialized patch payload exceeds {MAX_APPLY_FILE_CHANGES_BYTES} bytes"
                    ),
                    "payload_too_large",
                    None,
                    None,
                    None,
                    None,
                    "reduce the patch payload size and retry",
                )
            }
            Err(error) => {
                return apply_text_edits_preflight_rejection(
                    format!("failed to serialize patch payload: {error}"),
                    "serialization_failed",
                    None,
                    None,
                    None,
                    None,
                    "regenerate the patch and retry",
                )
            }
        };

        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        let client_id = proj.client_id.clone();
        let routing_path = parsed
            .hunks
            .first()
            .map(|hunk| hunk.path().to_string())
            .expect("non-empty Codex patch validated above");
        let wait_timeout = 60_u64;
        let request = ShellFileOpRequest {
            op: "apply_patch".to_string(),
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
        let (request_id, rx) = match self
            .runner_registry
            .enqueue_apply_patch(request, "tool_runtime".to_string())
            .await
        {
            Ok(request) => request,
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCHING_MODE,
                    ) =>
            {
                return apply_patch_capability_rejection(
                    error,
                    crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCHING_MODE,
                )
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(
                        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA,
                    ) =>
            {
                return apply_patch_capability_rejection(
                    error,
                    crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA,
                )
            }
            Err(error)
                if error.starts_with("capability_unavailable:")
                    && error.contains(crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH) =>
            {
                return apply_patch_capability_rejection(
                    error,
                    crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH,
                )
            }
            Err(_) => {
                return structured_edit_not_started_result(
                    "apply_patch",
                    "the Runner queue rejected the request before dispatch",
                )
            }
        };
        let response = match await_structured_edit_response(
            self,
            &request_id,
            rx,
            wait_timeout,
            "apply_patch",
        )
        .await
        {
            Ok(response) => response,
            Err(result) => return result,
        };
        apply_patch_agent_stdout_result(
            &response.stdout.unwrap_or_default(),
            &parsed,
            expected_dry_run,
            expected_matching_mode,
        )
    }
}
