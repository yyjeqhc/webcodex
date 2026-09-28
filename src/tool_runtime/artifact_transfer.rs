//! Direct Project-to-Project artifact transfer through Control.
//!
//! Whole binary payloads stay on the internal Control↔Runner artifact transport
//! and never surface in model text or Host attachments.

use super::files::{
    artifact_upload_begin_failure_is_definite, artifact_upload_failure_is_definite,
    validate_project_artifact_export_snapshot, INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES,
};
use super::sessions::SessionTransport;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

fn transfer_error(
    kind: &str,
    message: impl Into<String>,
    source_project: &str,
    source_path: &str,
    destination_project: &str,
    destination_path: &str,
) -> ToolResult {
    let message = message.into();
    ToolResult::err_with_output(
        message,
        json!({
            "error_kind": kind,
            "source_project": source_project,
            "source_path": source_path,
            "destination_project": destination_project,
            "destination_path": destination_path,
        }),
    )
}

impl ToolRuntime {
    async fn dispatch_transfer_artifact_write(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        Box::pin(self.dispatch_with_auth_transport_options(call, auth, transport)).await
    }

    async fn abort_transfer_upload(
        &self,
        destination_project: &str,
        destination_path: &str,
        upload_id: &str,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> bool {
        self.dispatch_transfer_artifact_write(
            ToolCall::ArtifactUploadAbort {
                project: destination_project.to_string(),
                path: destination_path.to_string(),
                upload_id: upload_id.to_string(),
                session_id: None,
            },
            auth,
            transport,
        )
        .await
        .success
    }

    pub(crate) async fn transfer_project_artifact(
        &self,
        source_project: String,
        source_path: String,
        destination_project: String,
        destination_path: String,
        overwrite: Option<bool>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        // Resolve both authorities independently before creating destination state.
        let source = match self
            .resolve_project_input_for_auth(&source_project, auth)
            .await
        {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        let destination = match self
            .resolve_project_input_for_auth(&destination_project, auth)
            .await
        {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        let source_project = source.resolved_id.clone();
        let destination_project = destination.resolved_id.clone();

        let metadata = self
            .export_project_artifact_metadata_resolved(&source, source_path.clone(), auth)
            .await;
        if !metadata.success {
            return ToolResult::err_with_output(
                metadata
                    .error
                    .unwrap_or_else(|| "source artifact metadata read failed".to_string()),
                json!({
                    "error_kind": "source_read_failed",
                    "source_project": source_project,
                    "source_path": source_path,
                    "destination_project": destination_project,
                    "destination_path": destination_path,
                    "source_failure": metadata.output,
                }),
            );
        }
        let snapshot =
            match validate_project_artifact_export_snapshot(&source_path, &metadata.output) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    return transfer_error(
                        "invalid_source_snapshot",
                        error,
                        &source_project,
                        &source_path,
                        &destination_project,
                        &destination_path,
                    )
                }
            };

        let begin = self
            .dispatch_transfer_artifact_write(
                ToolCall::ArtifactUploadBegin {
                    project: destination_project.clone(),
                    path: destination_path.clone(),
                    session_id: None,
                    expected_bytes: Some(snapshot.bytes),
                    expected_sha256: Some(snapshot.sha256.clone()),
                    mime_type: Some(snapshot.mime_type.clone()),
                    overwrite,
                },
                auth,
                transport,
            )
            .await;
        if !begin.success {
            let definite = artifact_upload_begin_failure_is_definite(&begin);
            let known_upload_id = begin
                .output
                .get("upload_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let cleaned = if definite {
                if let Some(upload_id) = known_upload_id.as_deref() {
                    self.abort_transfer_upload(
                        &destination_project,
                        &destination_path,
                        upload_id,
                        auth,
                        transport,
                    )
                    .await
                } else {
                    false
                }
            } else {
                false
            };
            return ToolResult::err_with_output(
                begin
                    .error
                    .unwrap_or_else(|| "destination artifact upload begin failed".to_string()),
                json!({
                    "error_kind": if definite {
                        "destination_begin_failed"
                    } else {
                        "destination_begin_outcome_unknown"
                    },
                    "source_project": source_project,
                    "source_path": source_path,
                    "destination_project": destination_project,
                    "destination_path": destination_path,
                    "outcome_unknown": !definite,
                    "destination_upload_aborted": cleaned,
                    "destination_failure": begin.output,
                }),
            );
        }
        let Some(upload_id) = begin
            .output
            .get("upload_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return ToolResult::err_with_output(
                "artifact upload begin returned no upload_id",
                json!({
                    "error_kind": "invalid_destination_begin_response",
                    "source_project": source_project,
                    "source_path": source_path,
                    "destination_project": destination_project,
                    "destination_path": destination_path,
                    "outcome_unknown": true,
                }),
            );
        };

        let mut offset = 0usize;
        while offset < snapshot.bytes {
            let length = (snapshot.bytes - offset).min(INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES);
            let chunk = match self
                .read_project_artifact_export_chunk_internal(
                    &source_project,
                    &source_path,
                    snapshot.bytes,
                    &snapshot.sha256,
                    offset,
                    length,
                    auth,
                )
                .await
            {
                Ok(output) => output,
                Err(error) => {
                    let cleaned = self
                        .abort_transfer_upload(
                            &destination_project,
                            &destination_path,
                            &upload_id,
                            auth,
                            transport,
                        )
                        .await;
                    return ToolResult::err_with_output(
                        error,
                        json!({
                            "error_kind": "source_read_failed",
                            "source_project": source_project,
                            "source_path": source_path,
                            "destination_project": destination_project,
                            "destination_path": destination_path,
                            "destination_upload_aborted": cleaned,
                        }),
                    );
                }
            };
            if let Some(error) = chunk.get("error").and_then(Value::as_str) {
                let kind = if chunk.get("error_kind").and_then(Value::as_str)
                    == Some("snapshot_changed")
                {
                    "source_snapshot_changed"
                } else {
                    "source_read_failed"
                };
                let cleaned = self
                    .abort_transfer_upload(
                        &destination_project,
                        &destination_path,
                        &upload_id,
                        auth,
                        transport,
                    )
                    .await;
                return ToolResult::err_with_output(
                    error.to_string(),
                    json!({
                        "error_kind": kind,
                        "source_project": source_project,
                        "source_path": source_path,
                        "destination_project": destination_project,
                        "destination_path": destination_path,
                        "destination_upload_aborted": cleaned,
                        "source_failure": chunk,
                    }),
                );
            }

            let encoded = match chunk.get("content_base64").and_then(Value::as_str) {
                Some(encoded) => encoded,
                None => {
                    let cleaned = self
                        .abort_transfer_upload(
                            &destination_project,
                            &destination_path,
                            &upload_id,
                            auth,
                            transport,
                        )
                        .await;
                    return ToolResult::err_with_output(
                        "source artifact chunk returned no content_base64",
                        json!({
                            "error_kind": "invalid_source_chunk",
                            "source_project": source_project,
                            "source_path": source_path,
                            "destination_project": destination_project,
                            "destination_path": destination_path,
                            "destination_upload_aborted": cleaned,
                        }),
                    );
                }
            };
            let decoded = match general_purpose::STANDARD.decode(encoded.as_bytes()) {
                Ok(decoded) => decoded,
                Err(error) => {
                    let cleaned = self
                        .abort_transfer_upload(
                            &destination_project,
                            &destination_path,
                            &upload_id,
                            auth,
                            transport,
                        )
                        .await;
                    return ToolResult::err_with_output(
                        format!("source artifact chunk returned invalid base64: {error}"),
                        json!({
                            "error_kind": "invalid_source_chunk",
                            "source_project": source_project,
                            "source_path": source_path,
                            "destination_project": destination_project,
                            "destination_path": destination_path,
                            "destination_upload_aborted": cleaned,
                        }),
                    );
                }
            };
            let expected_next = match offset.checked_add(decoded.len()) {
                Some(next) => next,
                None => {
                    return transfer_error(
                        "invalid_source_chunk",
                        "source artifact chunk offset overflow",
                        &source_project,
                        &source_path,
                        &destination_project,
                        &destination_path,
                    )
                }
            };
            let valid_source_chunk = chunk.get("path").and_then(Value::as_str)
                == Some(source_path.as_str())
                && chunk.get("file_bytes").and_then(Value::as_u64) == Some(snapshot.bytes as u64)
                && chunk.get("offset").and_then(Value::as_u64) == Some(offset as u64)
                && chunk.get("bytes_returned").and_then(Value::as_u64)
                    == Some(decoded.len() as u64)
                && chunk.get("next_offset").and_then(Value::as_u64) == Some(expected_next as u64)
                && !decoded.is_empty()
                && decoded.len() <= length
                && expected_next <= snapshot.bytes;
            if !valid_source_chunk {
                let cleaned = self
                    .abort_transfer_upload(
                        &destination_project,
                        &destination_path,
                        &upload_id,
                        auth,
                        transport,
                    )
                    .await;
                return ToolResult::err_with_output(
                    "source artifact chunk response violated the internal transfer contract",
                    json!({
                        "error_kind": "invalid_source_chunk",
                        "source_project": source_project,
                        "source_path": source_path,
                        "destination_project": destination_project,
                        "destination_path": destination_path,
                        "destination_upload_aborted": cleaned,
                    }),
                );
            }

            let write = self
                .dispatch_transfer_artifact_write(
                    ToolCall::ArtifactUploadChunk {
                        project: destination_project.clone(),
                        path: destination_path.clone(),
                        upload_id: upload_id.clone(),
                        offset,
                        content_base64: encoded.to_string(),
                        session_id: None,
                    },
                    auth,
                    transport,
                )
                .await;
            if !write.success {
                let definite = artifact_upload_failure_is_definite(&write, &upload_id);
                let cleaned = if definite {
                    self.abort_transfer_upload(
                        &destination_project,
                        &destination_path,
                        &upload_id,
                        auth,
                        transport,
                    )
                    .await
                } else {
                    false
                };
                return ToolResult::err_with_output(
                    write
                        .error
                        .unwrap_or_else(|| "destination artifact chunk write failed".to_string()),
                    json!({
                        "error_kind": if definite {
                            "destination_write_failed"
                        } else {
                            "destination_write_outcome_unknown"
                        },
                        "source_project": source_project,
                        "source_path": source_path,
                        "destination_project": destination_project,
                        "destination_path": destination_path,
                        "outcome_unknown": !definite,
                        "destination_upload_aborted": cleaned,
                        "destination_failure": write.output,
                    }),
                );
            }
            if write.output.get("next_offset").and_then(Value::as_u64) != Some(expected_next as u64)
            {
                let cleaned = self
                    .abort_transfer_upload(
                        &destination_project,
                        &destination_path,
                        &upload_id,
                        auth,
                        transport,
                    )
                    .await;
                return ToolResult::err_with_output(
                    "destination artifact chunk returned inconsistent next_offset",
                    json!({
                        "error_kind": "invalid_destination_chunk_response",
                        "source_project": source_project,
                        "source_path": source_path,
                        "destination_project": destination_project,
                        "destination_path": destination_path,
                        "destination_upload_aborted": cleaned,
                    }),
                );
            }
            offset = expected_next;
        }

        let finish = self
            .dispatch_transfer_artifact_write(
                ToolCall::ArtifactUploadFinish {
                    project: destination_project.clone(),
                    path: destination_path.clone(),
                    upload_id: upload_id.clone(),
                    session_id: None,
                },
                auth,
                transport,
            )
            .await;
        if !finish.success {
            let definite = artifact_upload_failure_is_definite(&finish, &upload_id);
            let cleaned = if definite {
                self.abort_transfer_upload(
                    &destination_project,
                    &destination_path,
                    &upload_id,
                    auth,
                    transport,
                )
                .await
            } else {
                false
            };
            return ToolResult::err_with_output(
                finish
                    .error
                    .unwrap_or_else(|| "destination artifact upload finish failed".to_string()),
                json!({
                    "error_kind": if definite {
                        "destination_finish_failed"
                    } else {
                        "destination_finish_outcome_unknown"
                    },
                    "source_project": source_project,
                    "source_path": source_path,
                    "destination_project": destination_project,
                    "destination_path": destination_path,
                    "outcome_unknown": !definite,
                    "destination_upload_aborted": cleaned,
                    "destination_failure": finish.output,
                }),
            );
        }

        let committed_bytes = finish.output.get("bytes").and_then(Value::as_u64);
        let committed_sha256 = finish.output.get("sha256").and_then(Value::as_str);
        if committed_bytes != Some(snapshot.bytes as u64)
            || committed_sha256 != Some(snapshot.sha256.as_str())
            || finish.output.get("committed").and_then(Value::as_bool) != Some(true)
        {
            return ToolResult::err_with_output(
                "destination commit did not confirm the source snapshot bytes and sha256",
                json!({
                    "error_kind": "destination_integrity_mismatch_after_commit",
                    "source_project": source_project,
                    "source_path": source_path,
                    "destination_project": destination_project,
                    "destination_path": destination_path,
                    "expected_bytes": snapshot.bytes,
                    "expected_sha256": snapshot.sha256,
                    "destination_result": finish.output,
                    "committed": true,
                }),
            );
        }

        ToolResult::ok(json!({
            "source_project": source_project,
            "source_path": source_path,
            "destination_project": destination_project,
            "destination_path": destination_path,
            "bytes": snapshot.bytes,
            "sha256": snapshot.sha256,
            "mime_type": snapshot.mime_type,
        }))
    }
}
