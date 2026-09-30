//! Snapshot capture and create-only project artifact publication.

use super::effects::{bounded_text, computer_error, computer_suggested_recovery};
use super::inputs::effective_snapshot_dimension_bounds;
use super::snapshot_receipts::sha256_hex;
use super::*;
pub(super) fn computer_snapshot_artifact_lifecycle_failure(
    message: &str,
    state: ShellCommandExecutionState,
    project: &str,
    path: &str,
    sha256: &str,
    file_bytes: usize,
    mime_type: &str,
) -> ToolResult {
    if state == ShellCommandExecutionState::NotStarted {
        return ToolResult::err_with_output(
            message.to_string(),
            json!({
                "error_kind": "not_started",
                "message": bounded_text(message),
                "execution_state": "not_started",
                "state_changed": false,
                "project": project,
                "path": path,
                "expected_sha256": sha256,
                "expected_file_bytes": file_bytes,
                "expected_mime_type": mime_type,
            }),
        );
    }
    let message = format!(
        "{message}; the create-only artifact may already exist. Read metadata for this exact project/path and compare SHA-256, byte count, and MIME before deciding whether another attempt is safe"
    );
    let result = ToolResult::err_with_output(
        message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(&message),
            "execution_state": "outcome_unknown",
            "project": project,
            "path": path,
            "expected_sha256": sha256,
            "expected_file_bytes": file_bytes,
            "expected_mime_type": mime_type,
        }),
    );
    computer_suggested_recovery(
        result,
        "read_project_artifact_metadata",
        json!({"project": project, "path": path}),
    )
}

pub(super) fn computer_snapshot_artifact_definite_failure(
    message: &str,
    project: &str,
    path: &str,
    sha256: &str,
    file_bytes: usize,
    mime_type: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "artifact_write_failed",
            "message": bounded_text(message),
            "execution_state": "completed",
            "state_changed": false,
            "project": project,
            "path": path,
            "expected_sha256": sha256,
            "expected_file_bytes": file_bytes,
            "expected_mime_type": mime_type,
        }),
    )
}
impl ToolRuntime {
    pub(super) async fn capture_computer_snapshot(
        &self,
        client_id: &str,
        surface_id: &str,
        region: Option<ComputerSnapshotRegion>,
        max_width: Option<u32>,
        max_height: Option<u32>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return computer_error("invalid_surface", "surface_id is invalid");
        }
        if let Some(region) = region.as_ref() {
            if region.width == 0
                || region.height == 0
                || region.x.checked_add(region.width).is_none()
                || region.y.checked_add(region.height).is_none()
            {
                return computer_error("invalid_request", "snapshot region is invalid");
            }
        }
        let (max_width, max_height) =
            match effective_snapshot_dimension_bounds(max_width, max_height) {
                Ok(bounds) => bounds,
                Err(()) => {
                    return computer_error(
                        "invalid_request",
                        "snapshot output dimension bound is invalid",
                    )
                }
            };
        let advanced = region.is_some() || max_width.is_some() || max_height.is_some();
        let (kind, payload) = if advanced {
            (
                "computer_snapshot_region",
                json!({
                    "surface_id": surface_id,
                    "region": region,
                    "max_width": max_width,
                    "max_height": max_height,
                }),
            )
        } else {
            ("computer_snapshot", json!({"surface_id": surface_id}))
        };
        self.dispatch_computer_request(client_id, kind, payload, auth, None, Some(surface_id), None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn save_computer_snapshot_artifact(
        &self,
        project: String,
        path: String,
        client_id: String,
        surface_id: String,
        region: Option<ComputerSnapshotRegion>,
        max_width: Option<u32>,
        max_height: Option<u32>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_artifact_file_path(&path) {
            return ToolResult::err_with_output(
                error.clone(),
                json!({"error_kind": "artifact_policy", "project": project, "path": path, "message": bounded_text(&error)}),
            );
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let target_client_id = resolved.config.client_id.clone();
        let target_cwd = resolved.config.path.clone();
        let project_id = resolved.resolved_id;
        let expected_project_prefix = format!("agent:{target_client_id}:");
        let target_agent_project_id = match project_id.strip_prefix(&expected_project_prefix) {
            Some(project_id) if !project_id.is_empty() => project_id.to_string(),
            _ => {
                return ToolResult::err(
                    "computer_save_snapshot resolved target project identity is invalid",
                )
            }
        };

        let capture = self
            .capture_computer_snapshot(&client_id, &surface_id, region, max_width, max_height, auth)
            .await;
        if !capture.success {
            return capture;
        }
        let snapshot = capture.output;
        let content_base64 = match snapshot.get("content_base64").and_then(Value::as_str) {
            Some(content) => content.to_string(),
            None => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot content is missing",
                )
            }
        };
        let decoded = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_MCP_IMAGE_BYTES => bytes,
            _ => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot content is inconsistent",
                )
            }
        };
        let mime_type = match snapshot.get("mime_type").and_then(Value::as_str) {
            Some(mime) => mime.to_string(),
            None => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot MIME is missing",
                )
            }
        };
        if let Err(error) = validate_artifact_mime_for_path(&path, Some(&mime_type)) {
            return ToolResult::err_with_output(
                error.clone(),
                json!({"error_kind": "artifact_policy", "project": project_id, "path": path, "message": bounded_text(&error)}),
            );
        }
        let file_bytes = decoded.len();
        let sha256 = snapshot
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| sha256_hex(&decoded));
        let Some(surface) = snapshot.get("surface") else {
            return computer_error(
                "invalid_runner_response",
                "validated snapshot surface is missing",
            );
        };
        let source_width = snapshot
            .get("source_width")
            .and_then(Value::as_u64)
            .or_else(|| surface.get("width").and_then(Value::as_u64))
            .unwrap_or_default();
        let source_height = snapshot
            .get("source_height")
            .and_then(Value::as_u64)
            .or_else(|| surface.get("height").and_then(Value::as_u64))
            .unwrap_or_default();
        let width = snapshot
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let height = snapshot
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let captured_region = snapshot.get("region").cloned().unwrap_or_else(
            || json!({"x": 0, "y": 0, "width": source_width, "height": source_height}),
        );

        let payload = json!({
            "path": path.clone(),
            "content_base64": content_base64,
            "mime_type": mime_type,
            "overwrite": false,
            "max_bytes": MAX_MCP_IMAGE_BYTES,
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) => serialized,
            Err(_) => {
                return computer_error(
                    "invalid_request",
                    "could not encode snapshot artifact request",
                )
            }
        };
        let wait_timeout = 60_u64;
        let request = ShellFileOpRequest {
            op: "save_project_artifact".to_string(),
            client_id: target_client_id,
            path: path.clone(),
            cwd: Some(target_cwd.clone()),
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
        let requested_by = crate::runner_http::requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .runner_registry
            .enqueue_computer_snapshot_artifact(
                request,
                &target_agent_project_id,
                &target_cwd,
                requested_by,
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) => {
                return computer_snapshot_artifact_lifecycle_failure(
                    &format!("snapshot artifact write was not dispatched: {error}"),
                    ShellCommandExecutionState::NotStarted,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                )
            }
        };
        let response = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), receiver)
            .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                let state = dispatch_uncertainty_lifecycle(
                    self.runner_registry
                        .cancel_request_dispatch_state(&request_id)
                        .await,
                );
                return computer_snapshot_artifact_lifecycle_failure(
                    "snapshot artifact response channel closed before a terminal result was received",
                    state,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                );
            }
            Err(_) => {
                let state = dispatch_uncertainty_lifecycle(
                    self.runner_registry
                        .cancel_request_dispatch_state(&request_id)
                        .await,
                );
                return computer_snapshot_artifact_lifecycle_failure(
                    "timed out waiting for snapshot artifact write result",
                    state,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                );
            }
        };
        let state = runner_command_lifecycle(&response, wait_timeout);
        if state == ShellCommandExecutionState::NotStarted {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write did not start",
                state,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if state != ShellCommandExecutionState::Completed {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write did not return a definite terminal result",
                ShellCommandExecutionState::OutcomeUnknown,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if let Some(error) = response.error.as_deref() {
            return computer_snapshot_artifact_definite_failure(
                error,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if response.exit_code != Some(0) {
            return computer_snapshot_artifact_definite_failure(
                response
                    .stderr
                    .as_deref()
                    .unwrap_or("snapshot artifact write failed"),
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        let output: Value = match response
            .stdout
            .as_deref()
            .map(str::trim)
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(output)) => output,
            _ => {
                return computer_snapshot_artifact_lifecycle_failure(
                    "snapshot artifact write returned invalid JSON after possible commit",
                    ShellCommandExecutionState::OutcomeUnknown,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                )
            }
        };
        if let Some(error) = output.get("error").and_then(Value::as_str) {
            return computer_snapshot_artifact_definite_failure(
                error,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        let metadata_matches = output.get("path").and_then(Value::as_str) == Some(path.as_str())
            && output.get("bytes_written").and_then(Value::as_u64) == Some(file_bytes as u64)
            && output.get("sha256").and_then(Value::as_str) == Some(sha256.as_str())
            && output.get("mime_type").and_then(Value::as_str) == Some(mime_type.as_str());
        if !metadata_matches {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write returned inconsistent success metadata after possible commit",
                ShellCommandExecutionState::OutcomeUnknown,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }

        ToolResult::ok(json!({
            "project": project_id,
            "path": path,
            "client_id": client_id,
            "surface_id": surface_id,
            "source_width": source_width,
            "source_height": source_height,
            "region": captured_region,
            "width": width,
            "height": height,
            "mime_type": mime_type,
            "file_bytes": file_bytes,
            "sha256": sha256,
            "saved": true,
        }))
    }
}
