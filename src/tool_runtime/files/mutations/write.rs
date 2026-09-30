//! Single-file writes with exact read-revision and Runner-owner guards.

use super::lifecycle::{
    await_structured_edit_response, structured_edit_not_started_result,
    structured_edit_outcome_unknown_result,
};
use super::preflight::{
    read_files_recovery, read_revision_rejection, read_revision_target, validate_edit_file_path,
};
use super::*;

fn sanitize_write_project_file_model_recovery(
    mut result: ToolResult,
    project: &str,
    path: &str,
) -> ToolResult {
    if result.success {
        return result;
    }
    let sha_mismatch = result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("expected_sha256 mismatch"));
    if let Some(output) = result.output.as_object_mut() {
        for key in [
            "retry_guidance",
            "recovery_action",
            "expected_read_revision",
            "reread_required",
            "suggested_call",
            "error",
        ] {
            output.remove(key);
        }
    }
    if !sha_mismatch {
        return result;
    }
    result
        .output
        .as_object_mut()
        .map(|output| output.remove("sha256"));
    result.output["error_kind"] = json!("stale_file_revision");
    result.output["path"] = json!(path);
    result.output["recovery"] = read_files_recovery(project, path);
    result.error = Some(format!(
        "Rejected whole-file replacement: the source for '{path}' changed before mutation. No file was overwritten."
    ));
    result
}

pub(super) fn write_project_file_agent_stdout_result(stdout: &str) -> ToolResult {
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

fn compact_write_project_file_preflight_rejection(
    detail: impl Into<String>,
    error_kind: &'static str,
) -> ToolResult {
    let detail = detail.into();
    ToolResult::err_with_output(
        format!("Rejected before write: {detail}. No files were modified."),
        json!({
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error_kind": error_kind,
        }),
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::FixInput)
}
impl ToolRuntime {
    // -------------------------------------------------------------------------
    pub(crate) async fn write_project_file(
        &self,
        project: String,
        path: String,
        content: String,
        overwrite: Option<bool>,
        expected_read_revision: Option<u64>,
    ) -> ToolResult {
        if let Err(e) = validate_edit_file_path(&path) {
            return crate::tool_runtime::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if content.contains('\0') {
            return compact_write_project_file_preflight_rejection(
                "content cannot contain NUL bytes",
                "invalid_content",
            );
        }
        if content.len() > MAX_WRITE_CONTENT_BYTES {
            return compact_write_project_file_preflight_rejection(
                format!("content exceeds {MAX_WRITE_CONTENT_BYTES} bytes"),
                "content_too_large",
            );
        }
        let overwrite = overwrite.unwrap_or(false);
        if expected_read_revision
            .is_some_and(|revision| !(1..=MAX_JSON_SAFE_INTEGER).contains(&revision))
        {
            return compact_write_project_file_preflight_rejection(
                "expected_read_revision must be a positive JSON-safe integer",
                "invalid_expected_read_revision",
            );
        }
        match (overwrite, expected_read_revision.is_some()) {
            (true, false) => {
                return compact_write_project_file_preflight_rejection(
                    "overwrite=true requires expected_read_revision",
                    "missing_expected_read_revision",
                )
            }
            (false, true) => {
                return compact_write_project_file_preflight_rejection(
                    "expected_read_revision is allowed only when overwrite=true",
                    "unexpected_expected_read_revision",
                )
            }
            _ => {}
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
                "write_project_file",
                "the resolved Runner became unavailable before mutation admission",
            );
        };
        let Some(runner_project_id) =
            crate::tool_runtime::runner_local_project_id(&resolved.resolved_id)
        else {
            return structured_edit_not_started_result(
                "write_project_file",
                "the resolved Project identity could not be bound to a Runner-local project id",
            );
        };
        let expected_sha256 = match expected_read_revision {
            Some(revision) => {
                let target = read_revision_target(&resolved, &path, &runner.runner_instance_id);
                match self.read_revisions.resolve(revision, &target) {
                    Ok(sha256) => Some(sha256),
                    Err(error) => {
                        return read_revision_rejection(&resolved.resolved_id, &path, error)
                    }
                }
            }
            None => None,
        };

        let payload = json!({
            "path": path.clone(),
            "content": content,
            "overwrite": overwrite,
            "expected_sha256": expected_sha256,
        });
        let wait_timeout = 60_u64;
        let request = ShellFileOpRequest {
            op: "write_project_file".to_string(),
            client_id: resolved.config.client_id.clone(),
            path: path.clone(),
            cwd: Some(resolved.config.path.clone()),
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
        };
        let (request_id, rx) = match self
            .runner_registry
            .enqueue_project_file_mutation(
                request,
                runner_project_id,
                &resolved.config.path,
                &runner.runner_instance_id,
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) if error.starts_with("stale_runner:") => {
                if expected_read_revision.is_some() {
                    return read_revision_rejection(
                        &resolved.resolved_id,
                        &path,
                        ReadRevisionLookupError::OwnerMismatch,
                    );
                }
                return structured_edit_not_started_result(
                    "write_project_file",
                    "the exact Runner changed before the new-file create could be dispatched; retry after resolving the current Project owner",
                );
            }
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
        sanitize_write_project_file_model_recovery(
            write_project_file_agent_stdout_result(&response.stdout.unwrap_or_default()),
            &resolved.resolved_id,
            &path,
        )
    }
}
