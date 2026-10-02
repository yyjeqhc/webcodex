//! Direct Project-to-Project artifact transfer through Control.
//!
//! Whole binary payloads stay on the internal Control↔Runner artifact transport
//! and never surface in model text or Host attachments.

use super::files::{
    artifact_upload_begin_failure_is_definite, artifact_upload_failure_is_definite,
    validate_artifact_file_path, validate_project_artifact_export_snapshot,
    INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES,
};
use super::sessions::SessionTransport;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::{AuthContext, AuthKind};
use crate::db::{
    ArtifactHandoffAcceptanceClaim, ArtifactHandoffAcceptanceOutcome, ArtifactHandoffGrant,
    ArtifactHandoffImportRequest, ArtifactHandoffPrincipal, ArtifactHandoffSourceSnapshot,
    ArtifactHandoffStoreError,
};
use crate::tool_runtime::project_resolution::ResolvedProject;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
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

fn artifact_handoff_unavailable() -> ToolResult {
    ToolResult::err_with_output(
        "Artifact handoff is unavailable",
        json!({
            "error_kind": "artifact_handoff_unavailable",
            "state_changed": false,
        }),
    )
}

fn artifact_handoff_store_error(error: ArtifactHandoffStoreError) -> ToolResult {
    if error.code() == "artifact_handoff_acceptance_idempotency_conflict" {
        return ToolResult::err_with_output(
            "Artifact handoff idempotency key conflicts with a different import request",
            json!({
                "error_kind": "artifact_handoff_idempotency_conflict",
                "state_changed": false,
            }),
        );
    }
    artifact_handoff_unavailable()
}

fn artifact_handoff_source_stale() -> ToolResult {
    ToolResult::err_with_output(
        "Artifact handoff source snapshot is stale",
        json!({
            "error_kind": "artifact_handoff_source_stale",
            "state_changed": false,
        }),
    )
}

fn artifact_handoff_source_auth() -> AuthContext {
    let mut auth = AuthContext::new(AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    auth
}

#[derive(Clone, Copy)]
struct ArtifactHandoffTransferFence<'a> {
    principal: &'a ArtifactHandoffPrincipal,
    claim: &'a ArtifactHandoffAcceptanceClaim,
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

    fn artifact_handoff_success(
        &self,
        claim: ArtifactHandoffAcceptanceClaim,
        replayed: bool,
    ) -> ToolResult {
        let Some(outcome) = claim.acceptance.outcome.as_ref() else {
            return artifact_handoff_unavailable();
        };
        ToolResult::ok(json!({
            "acceptance_id": claim.acceptance.acceptance_id,
            "grant_id": claim.grant.grant_id,
            "replayed": replayed,
            "destination_project": claim.acceptance.destination_project,
            "destination_path": outcome.destination_path,
            "bytes": outcome.destination_bytes,
            "sha256": outcome.destination_sha256,
            "mime_type": claim.grant.source_snapshot.mime_type,
            "provenance": {
                "grant_id": claim.grant.grant_id,
                "source_project": claim.grant.source_project,
                "source_path": claim.grant.source_snapshot.path,
                "source_bytes": claim.grant.source_snapshot.bytes,
                "source_sha256": claim.grant.source_snapshot.sha256,
                "source_mime_type": claim.grant.source_snapshot.mime_type,
                "source_name": claim.grant.source_snapshot.name,
            },
        }))
    }

    fn artifact_handoff_transfer_failure(
        &self,
        result: ToolResult,
        claim: &ArtifactHandoffAcceptanceClaim,
    ) -> ToolResult {
        let committed_without_confirmation =
            result.output.get("error_kind").and_then(Value::as_str)
                == Some("destination_integrity_mismatch_after_commit")
                && result.output.get("committed").and_then(Value::as_bool) == Some(true);
        let outcome_unknown = result
            .output
            .get("outcome_unknown")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || committed_without_confirmation;
        let error_kind = match result.output.get("error_kind").and_then(Value::as_str) {
            Some("artifact_handoff_source_stale" | "source_snapshot_changed") => {
                "artifact_handoff_source_stale"
            }
            Some("source_read_failed") => "artifact_handoff_source_unavailable",
            _ if outcome_unknown => "artifact_handoff_outcome_unknown",
            _ => "artifact_handoff_transfer_failed",
        };
        ToolResult::err_with_output(
            if error_kind == "artifact_handoff_source_stale" {
                "Artifact handoff source snapshot is stale"
            } else {
                "Artifact handoff import did not complete"
            },
            json!({
                "error_kind": error_kind,
                "grant_id": claim.grant.grant_id,
                "acceptance_id": claim.acceptance.acceptance_id,
                "outcome_unknown": outcome_unknown,
                "state_changed": outcome_unknown,
            }),
        )
    }

    async fn revalidate_handoff_source_snapshot(
        &self,
        source: &ResolvedProject,
        grant: &ArtifactHandoffGrant,
        source_auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let metadata = self
            .export_project_artifact_metadata_resolved(
                source,
                grant.source_snapshot.path.clone(),
                source_auth,
            )
            .await;
        if !metadata.success {
            return Err(artifact_handoff_unavailable());
        }
        let snapshot = validate_project_artifact_export_snapshot(
            &grant.source_snapshot.path,
            &metadata.output,
        )
        .map_err(|_| artifact_handoff_source_stale())?;
        let matches = grant.source_snapshot.path == snapshot.path
            && grant.source_snapshot.bytes == snapshot.bytes as u64
            && grant.source_snapshot.sha256 == snapshot.sha256
            && grant.source_snapshot.mime_type == snapshot.mime_type
            && grant.source_snapshot.name == snapshot.name;
        if !matches {
            return Err(artifact_handoff_source_stale());
        }
        Ok(())
    }

    async fn reconcile_handoff_destination(
        &self,
        destination: &ResolvedProject,
        destination_path: &str,
        snapshot: &ArtifactHandoffSourceSnapshot,
        auth: Option<&AuthContext>,
    ) -> Option<ArtifactHandoffAcceptanceOutcome> {
        let metadata = self
            .export_project_artifact_metadata_resolved(
                destination,
                destination_path.to_string(),
                auth,
            )
            .await;
        if !metadata.success
            || metadata.output.get("bytes").and_then(Value::as_u64) != Some(snapshot.bytes)
            || metadata.output.get("sha256").and_then(Value::as_str)
                != Some(snapshot.sha256.as_str())
        {
            return None;
        }
        Some(ArtifactHandoffAcceptanceOutcome {
            destination_path: destination_path.to_string(),
            destination_bytes: snapshot.bytes,
            destination_sha256: snapshot.sha256.clone(),
        })
    }

    pub(crate) async fn accept_artifact_handoff(
        &self,
        grant_id: String,
        destination_project: String,
        destination_path: String,
        overwrite: Option<bool>,
        idempotency_key: String,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        if let Err(error) = validate_artifact_file_path(&destination_path) {
            return ToolResult::err_with_output(
                error,
                json!({
                    "error_kind": "invalid_artifact_handoff_destination_path",
                    "destination_path": destination_path,
                    "state_changed": false,
                }),
            );
        }
        let destination = match self
            .resolve_project_input_for_auth(&destination_project, auth)
            .await
        {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        let principal =
            match super::communication::communication_principal(auth).and_then(|principal| {
                ArtifactHandoffPrincipal::try_from(principal)
                    .map_err(|_| artifact_handoff_unavailable())
            }) {
                Ok(principal) => principal,
                Err(error) => return error,
            };
        let Some(db) = self.communication_db.as_ref() else {
            return artifact_handoff_unavailable();
        };
        let overwrite = overwrite.unwrap_or(false);
        let request = ArtifactHandoffImportRequest {
            grant_id,
            destination_project: destination.resolved_id.clone(),
            destination_path: destination_path.clone(),
            overwrite,
        };
        let now = Utc::now().timestamp_millis();
        let begin_claim =
            match db.begin_artifact_handoff_import(&principal, &request, &idempotency_key, now) {
                Ok(claim) => claim,
                Err(error) => return artifact_handoff_store_error(error),
            };
        if begin_claim.acceptance.state == crate::db::ArtifactHandoffAcceptanceState::Completed {
            return self.artifact_handoff_success(begin_claim, true);
        }
        let replayed = begin_claim.replayed;
        let claim = match db.revalidate_artifact_handoff_acceptance(
            &principal,
            &destination.resolved_id,
            &request.grant_id,
            &begin_claim.acceptance.acceptance_id,
            now,
        ) {
            Ok(claim) => claim,
            Err(error) => return artifact_handoff_store_error(error),
        };
        if claim.acceptance.state == crate::db::ArtifactHandoffAcceptanceState::Completed {
            return self.artifact_handoff_success(claim, true);
        }
        if claim.grant.destination_project != destination.resolved_id {
            return artifact_handoff_unavailable();
        }
        let source_auth = artifact_handoff_source_auth();
        let source = match self
            .resolve_project_input_for_auth(&claim.grant.source_project, Some(&source_auth))
            .await
        {
            Ok(project) => project,
            Err(_) => return artifact_handoff_unavailable(),
        };

        if replayed && claim.acceptance.destination_reconcile_allowed {
            if let Err(error) = self
                .revalidate_handoff_source_snapshot(&source, &claim.grant, Some(&source_auth))
                .await
            {
                return error;
            }
            if let Some(outcome) = self
                .reconcile_handoff_destination(
                    &destination,
                    &destination_path,
                    &claim.grant.source_snapshot,
                    auth,
                )
                .await
            {
                return match db.complete_artifact_handoff_acceptance(
                    &principal,
                    &destination.resolved_id,
                    &claim.grant.grant_id,
                    &claim.acceptance.acceptance_id,
                    outcome,
                    now,
                ) {
                    Ok(acceptance) => self.artifact_handoff_success(
                        ArtifactHandoffAcceptanceClaim {
                            grant: claim.grant,
                            acceptance,
                            replayed: true,
                        },
                        true,
                    ),
                    Err(_) => ToolResult::err_with_output(
                        "Artifact handoff import result could not be reconciled",
                        json!({
                            "error_kind": "artifact_handoff_outcome_unknown",
                            "grant_id": claim.grant.grant_id,
                            "acceptance_id": claim.acceptance.acceptance_id,
                            "outcome_unknown": true,
                            "state_changed": true,
                        }),
                    ),
                };
            }
        }

        let transfer = self
            .transfer_resolved_project_artifact(
                &source,
                claim.grant.source_snapshot.path.clone(),
                &destination,
                destination_path.clone(),
                Some(overwrite),
                Some(&source_auth),
                auth,
                transport,
                Some(&claim.grant.source_snapshot),
                "artifact_handoff_source_stale",
                Some(ArtifactHandoffTransferFence {
                    principal: &principal,
                    claim: &claim,
                }),
            )
            .await;
        if !transfer.success {
            return self.artifact_handoff_transfer_failure(transfer, &claim);
        }
        if transfer.output.get("replayed").and_then(Value::as_bool) == Some(true) {
            // A concurrent same-key acceptance completed after our durable
            // recheck. The helper already returned the canonical replay result.
            return transfer;
        }
        let Some(destination_bytes) = transfer.output.get("bytes").and_then(Value::as_u64) else {
            return self.artifact_handoff_transfer_failure(transfer, &claim);
        };
        let Some(destination_sha256) = transfer
            .output
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return self.artifact_handoff_transfer_failure(transfer, &claim);
        };
        let outcome = ArtifactHandoffAcceptanceOutcome {
            destination_path,
            destination_bytes,
            destination_sha256,
        };
        match db.complete_artifact_handoff_acceptance(
            &principal,
            &destination.resolved_id,
            &claim.grant.grant_id,
            &claim.acceptance.acceptance_id,
            outcome,
            now,
        ) {
            Ok(acceptance) => self.artifact_handoff_success(
                ArtifactHandoffAcceptanceClaim {
                    grant: claim.grant,
                    acceptance,
                    replayed,
                },
                replayed,
            ),
            Err(_) => ToolResult::err_with_output(
                "Artifact handoff import outcome is unknown",
                json!({
                    "error_kind": "artifact_handoff_outcome_unknown",
                    "grant_id": claim.grant.grant_id,
                    "acceptance_id": claim.acceptance.acceptance_id,
                    "outcome_unknown": true,
                    "state_changed": true,
                }),
            ),
        }
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
        self.transfer_resolved_project_artifact(
            &source,
            source_path,
            &destination,
            destination_path,
            overwrite,
            auth,
            auth,
            transport,
            None,
            "invalid_source_snapshot",
            None,
        )
        .await
    }

    async fn transfer_resolved_project_artifact(
        &self,
        source: &ResolvedProject,
        source_path: String,
        destination: &ResolvedProject,
        destination_path: String,
        overwrite: Option<bool>,
        source_auth: Option<&AuthContext>,
        destination_auth: Option<&AuthContext>,
        transport: SessionTransport,
        expected_snapshot: Option<&ArtifactHandoffSourceSnapshot>,
        stale_error_kind: &str,
        handoff_fence: Option<ArtifactHandoffTransferFence<'_>>,
    ) -> ToolResult {
        let source_project = source.resolved_id.clone();
        let destination_project = destination.resolved_id.clone();

        let metadata = self
            .export_project_artifact_metadata_resolved(source, source_path.clone(), source_auth)
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
        if let Some(expected) = expected_snapshot {
            let matches = expected.path == snapshot.path
                && expected.bytes == snapshot.bytes as u64
                && expected.sha256 == snapshot.sha256
                && expected.mime_type == snapshot.mime_type
                && expected.name == snapshot.name;
            if !matches {
                return transfer_error(
                    stale_error_kind,
                    "artifact handoff source snapshot is stale",
                    &source_project,
                    &source_path,
                    &destination_project,
                    &destination_path,
                );
            }
        }
        if let Some(fence) = handoff_fence {
            let Some(db) = self.communication_db.as_ref() else {
                return artifact_handoff_unavailable();
            };
            match db.revalidate_artifact_handoff_acceptance(
                fence.principal,
                &destination_project,
                &fence.claim.grant.grant_id,
                &fence.claim.acceptance.acceptance_id,
                Utc::now().timestamp_millis(),
            ) {
                Ok(claim)
                    if claim.acceptance.state
                        == crate::db::ArtifactHandoffAcceptanceState::Completed =>
                {
                    return self.artifact_handoff_success(claim, true);
                }
                Ok(_) => {}
                Err(_) => return artifact_handoff_unavailable(),
            }
        }

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
                destination_auth,
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
                        destination_auth,
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
                    source_auth,
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
                            destination_auth,
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
                        destination_auth,
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
                            destination_auth,
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
                            destination_auth,
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
                        destination_auth,
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
                    destination_auth,
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
                        destination_auth,
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
                        destination_auth,
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
                destination_auth,
                transport,
            )
            .await;
        if let Some(fence) = handoff_fence {
            let should_enable_reconciliation =
                finish.success || !artifact_upload_failure_is_definite(&finish, &upload_id);
            if should_enable_reconciliation {
                let Some(db) = self.communication_db.as_ref() else {
                    return ToolResult::err_with_output(
                        "Artifact handoff destination commit outcome is unknown",
                        json!({
                            "error_kind": "destination_finish_outcome_unknown",
                            "source_project": source_project,
                            "source_path": source_path,
                            "destination_project": destination_project,
                            "destination_path": destination_path,
                            "outcome_unknown": true,
                        }),
                    );
                };
                match db.mark_artifact_handoff_acceptance_destination_reconcile_allowed(
                    fence.principal,
                    &destination_project,
                    &fence.claim.grant.grant_id,
                    &fence.claim.acceptance.acceptance_id,
                    Utc::now().timestamp_millis(),
                ) {
                    Ok(claim)
                        if claim.acceptance.state
                            == crate::db::ArtifactHandoffAcceptanceState::Completed =>
                    {
                        return self.artifact_handoff_success(claim, true);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        return ToolResult::err_with_output(
                            "Artifact handoff destination commit outcome is unknown",
                            json!({
                                "error_kind": "destination_finish_outcome_unknown",
                                "source_project": source_project,
                                "source_path": source_path,
                                "destination_project": destination_project,
                                "destination_path": destination_path,
                                "outcome_unknown": true,
                            }),
                        );
                    }
                }
            }
        }
        if !finish.success {
            let definite = artifact_upload_failure_is_definite(&finish, &upload_id);
            let cleaned = if definite {
                self.abort_transfer_upload(
                    &destination_project,
                    &destination_path,
                    &upload_id,
                    destination_auth,
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
