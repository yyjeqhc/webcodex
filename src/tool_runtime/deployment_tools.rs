//! Durable preparation and observation for controlled WebPi service deployments.
//!
//! This module deliberately does not execute restart/cutover. It validates a
//! bounded non-secret manifest and persists exact idempotent deployment intent so
//! a later supervisor-backed cutover can survive Server restart.

use super::session_context::runtime_observation_principal;
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use webcodex_store::{
    DeploymentOperation, DeploymentPrincipal, DeploymentReceiptBeginOutcome,
    DeploymentReceiptRecord, DeploymentReceiptStoreError, DeploymentState, NewDeploymentReceipt,
};

const MAX_ARTIFACTS: usize = 16;
const MAX_ARTIFACT_NAME_CHARS: usize = 128;
const MAX_VERSION_CHARS: usize = 64;
const MAX_BACKUP_ID_CHARS: usize = 128;
const MAX_CANDIDATE_ID_CHARS: usize = 64;
const REQUIRED_DEPLOY_ARTIFACTS: [&str; 3] = ["webpi.exe", "webpi-server.exe", "webpi-runner.exe"];

fn safe_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_candidate_id(value: &str) -> bool {
    if value.is_empty() || value.chars().count() > MAX_CANDIDATE_ID_CHARS {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn manifest_string_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max_chars: usize,
) -> Result<&'a str, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("target_manifest.{key} must be a string"))?;
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(format!(
            "target_manifest.{key} must be 1..={max_chars} characters"
        ));
    }
    Ok(value)
}

fn validate_manifest(operation: DeploymentOperation, manifest: &Value) -> Result<String, String> {
    let object = manifest
        .as_object()
        .ok_or_else(|| "target_manifest must be an object".to_string())?;
    if matches!(operation, DeploymentOperation::Rollback) {
        if !only_keys(object, &["backup_id"]) {
            return Err("rollback target_manifest may contain only backup_id".to_string());
        }
        let backup_id = manifest_string_field(object, "backup_id", MAX_BACKUP_ID_CHARS)?;
        if !safe_candidate_id(backup_id) {
            return Err(format!(
                "rollback target_manifest.backup_id must be a safe 1..={MAX_BACKUP_ID_CHARS} character backup id"
            ));
        }
        return serde_json::to_string(manifest)
            .map_err(|error| format!("serialize target_manifest: {error}"));
    }
    if !only_keys(
        object,
        &[
            "version",
            "git_commit",
            "git_dirty",
            "built_at",
            "artifacts",
            "candidate_id",
            "backup_id",
        ],
    ) {
        return Err("target_manifest contains unsupported fields".to_string());
    }
    manifest_string_field(object, "version", MAX_VERSION_CHARS)?;
    let git_commit = manifest_string_field(object, "git_commit", 40)?;
    if !safe_hex(git_commit, 40) {
        return Err("target_manifest.git_commit must be a 40-character hex commit".to_string());
    }
    if let Some(value) = object.get("git_dirty") {
        if !value.is_boolean() {
            return Err("target_manifest.git_dirty must be boolean".to_string());
        }
    }
    if let Some(value) = object.get("built_at") {
        if value.as_i64().is_none() {
            return Err("target_manifest.built_at must be an integer".to_string());
        }
    }
    let artifacts = object
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "target_manifest.artifacts must be an array".to_string())?;
    if artifacts.len() > MAX_ARTIFACTS {
        return Err(format!(
            "target_manifest.artifacts may contain at most {MAX_ARTIFACTS} entries"
        ));
    }
    if matches!(operation, DeploymentOperation::Deploy) && artifacts.is_empty() {
        return Err("deploy target_manifest requires at least one artifact".to_string());
    }
    let candidate_id = object.get("candidate_id").and_then(Value::as_str);
    match operation {
        DeploymentOperation::Deploy => {
            let Some(candidate_id) = candidate_id.filter(|value| safe_candidate_id(value)) else {
                return Err(format!(
                    "deploy target_manifest.candidate_id must be a safe 1..={MAX_CANDIDATE_ID_CHARS} character candidate id"
                ));
            };
            let _ = candidate_id;
        }
        DeploymentOperation::Restart => {}
        DeploymentOperation::Rollback => {
            unreachable!("rollback returns after narrow manifest validation")
        }
    }
    let mut artifact_names = std::collections::BTreeSet::new();
    for artifact in artifacts {
        let artifact = artifact
            .as_object()
            .ok_or_else(|| "target_manifest artifact must be an object".to_string())?;
        if !only_keys(artifact, &["name", "sha256", "size_bytes"]) {
            return Err("target_manifest artifact contains unsupported fields".to_string());
        }
        let name = manifest_string_field(artifact, "name", MAX_ARTIFACT_NAME_CHARS)?;
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err("target_manifest artifact name contains unsafe characters".to_string());
        }
        if !artifact_names.insert(name.to_string()) {
            return Err("target_manifest artifact names must be unique".to_string());
        }
        let digest = manifest_string_field(artifact, "sha256", 64)?;
        if !safe_hex(digest, 64) {
            return Err("target_manifest artifact sha256 must be 64 hex characters".to_string());
        }
        let size = artifact
            .get("size_bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                "target_manifest artifact size_bytes must be a non-negative integer".to_string()
            })?;
        if size == 0 {
            return Err(
                "target_manifest artifact size_bytes must be greater than zero".to_string(),
            );
        }
    }
    if matches!(operation, DeploymentOperation::Deploy) {
        let required: std::collections::BTreeSet<String> = REQUIRED_DEPLOY_ARTIFACTS
            .into_iter()
            .map(str::to_string)
            .collect();
        if artifact_names != required {
            return Err(
                "deploy target_manifest artifacts must be exactly webpi.exe, webpi-server.exe, and webpi-runner.exe"
                    .to_string(),
            );
        }
    }
    match object.get("backup_id") {
        Some(Value::String(value)) if safe_candidate_id(value) => {}
        Some(_) => {
            return Err(format!(
                "target_manifest.backup_id must be 1..={MAX_BACKUP_ID_CHARS} characters"
            ))
        }
        None if matches!(operation, DeploymentOperation::Rollback) => {
            return Err("rollback target_manifest requires backup_id".to_string())
        }
        None => {}
    }
    serde_json::to_string(manifest).map_err(|error| format!("serialize target_manifest: {error}"))
}

fn parse_operation(value: &str) -> Result<DeploymentOperation, String> {
    match value {
        "deploy" => Ok(DeploymentOperation::Deploy),
        "restart" => Ok(DeploymentOperation::Restart),
        "rollback" => Ok(DeploymentOperation::Rollback),
        _ => Err("operation must be one of deploy, restart, rollback".to_string()),
    }
}

fn request_hash(operation: DeploymentOperation, client_id: &str, manifest: &Value) -> String {
    let canonical = serde_json::to_vec(&json!({
        "operation": operation.as_str(),
        "client_id": client_id,
        "target_manifest": manifest,
    }))
    .expect("deployment request JSON is serializable");
    let mut hasher = Sha256::new();
    hasher.update(b"webpi.deployment-receipt.request.v1\0");
    hasher.update(canonical);
    format!("{:x}", hasher.finalize())
}

fn restart_manifest() -> Value {
    let build = crate::build_info::current();
    json!({
        "version": build.version,
        "git_commit": build.git_commit,
        "git_dirty": build.git_dirty,
        "built_at": build.built_at,
        "artifacts": [],
    })
}

fn restart_request_hash(client_id: &str, expected_generation: u64, manifest: &Value) -> String {
    let canonical = serde_json::to_vec(&json!({
        "operation": "restart",
        "client_id": client_id,
        "expected_generation": expected_generation,
        "target_manifest": manifest,
    }))
    .expect("restart request JSON is serializable");
    let mut hasher = Sha256::new();
    hasher.update(b"webpi.service-restart.request.v1\0");
    hasher.update(canonical);
    format!("{:x}", hasher.finalize())
}

fn deployment_terminal_diagnostic(
    state: DeploymentState,
) -> Option<(
    webcodex_core::runtime_diagnostics::DiagnosticSeverity,
    &'static str,
)> {
    use webcodex_core::runtime_diagnostics::DiagnosticSeverity;
    match state {
        DeploymentState::Succeeded => Some((DiagnosticSeverity::Info, "succeeded")),
        DeploymentState::RolledBack => Some((DiagnosticSeverity::Warn, "rolled_back")),
        DeploymentState::Failed => Some((DiagnosticSeverity::Error, "failed")),
        DeploymentState::OutcomeUnknown => Some((DiagnosticSeverity::Warn, "outcome_unknown")),
        _ => None,
    }
}

fn record_deployment_terminal_diagnostic(record: &DeploymentReceiptRecord, state: DeploymentState) {
    if let Some((severity, code)) = deployment_terminal_diagnostic(state) {
        webcodex_core::runtime_diagnostics::record(
            severity,
            "service_deployment",
            code,
            Some(&record.receipt_id),
        );
    }
}

fn transition_receipt(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
    state: DeploymentState,
    error_code: Option<&str>,
) -> Result<DeploymentReceiptRecord, DeploymentReceiptStoreError> {
    let updated = db.transition_deployment_receipt(
        principal,
        &record.receipt_id,
        record.revision,
        state,
        chrono::Utc::now().timestamp(),
        error_code,
    )?;
    record_deployment_terminal_diagnostic(&updated, state);
    Ok(updated)
}

fn transition_receipt_with_backup(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
    state: DeploymentState,
    error_code: Option<&str>,
    backup_id: Option<&str>,
) -> Result<DeploymentReceiptRecord, DeploymentReceiptStoreError> {
    let updated = db.transition_deployment_receipt_with_backup(
        principal,
        &record.receipt_id,
        record.revision,
        state,
        chrono::Utc::now().timestamp(),
        error_code,
        backup_id,
    )?;
    record_deployment_terminal_diagnostic(&updated, state);
    Ok(updated)
}

fn reconcile_missing_supervisor_result(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
    action: &str,
    unknown_error_code: &'static str,
) -> Result<DeploymentReceiptRecord, ToolResult> {
    match super::service_supervisor::request_in_flight(
        action,
        &record.receipt_id,
        record.revision,
    ) {
        Ok(true) => Ok(record),
        Ok(false) => transition_receipt(
            db,
            principal,
            record,
            DeploymentState::OutcomeUnknown,
            Some(unknown_error_code),
        )
        .map_err(store_error),
        Err(error) => Err(ToolResult::err_with_output(
            "supervisor execution state could not be verified while reconciling a switching receipt",
            json!({
                "error_kind": error.code(),
                "state_changed": false,
                "deployment_receipt": receipt_json(record),
            }),
        )),
    }
}

fn reconcile_restart_receipt(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
) -> Result<DeploymentReceiptRecord, ToolResult> {
    if record.operation != "restart" || record.state != "switching" {
        return Ok(record);
    }
    let result =
        match super::service_supervisor::read_restart_result(&record.receipt_id, record.revision) {
            Ok(Some(result)) => result,
            Ok(None) => {
                return reconcile_missing_supervisor_result(
                    db,
                    principal,
                    record,
                    "restart",
                    "restart_outcome_unknown",
                );
            }
            Err(error) => {
                return Err(ToolResult::err_with_output(
                    "signed supervisor restart result could not be verified",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ));
            }
        };
    let updated = if result.status == "succeeded" {
        let verifying = transition_receipt(db, principal, record, DeploymentState::Verifying, None)
            .map_err(store_error)?;
        transition_receipt(db, principal, verifying, DeploymentState::Succeeded, None)
            .map_err(store_error)?
    } else {
        transition_receipt(
            db,
            principal,
            record,
            DeploymentState::Failed,
            result.error_code.as_deref().or(Some("restart_failed")),
        )
        .map_err(store_error)?
    };
    super::service_supervisor::remove_restart_result(&updated.receipt_id);
    Ok(updated)
}

fn deploy_request_parts(
    record: &DeploymentReceiptRecord,
) -> Result<
    (
        String,
        Vec<super::service_supervisor::SupervisorDeployArtifact>,
    ),
    ToolResult,
> {
    if record.operation != "deploy" {
        return Err(ToolResult::err_with_output(
            "deployment receipt is not a deploy operation",
            json!({
                "error_kind": "deployment_receipt_operation_mismatch",
                "state_changed": false,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        ));
    }
    let manifest: Value = serde_json::from_str(&record.target_manifest_json).map_err(|_| {
        ToolResult::err_with_output(
            "stored deployment manifest is invalid JSON",
            json!({
                "error_kind": "stored_deployment_manifest_invalid",
                "state_changed": false,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        )
    })?;
    validate_manifest(DeploymentOperation::Deploy, &manifest).map_err(|reason| {
        ToolResult::err_with_output(
            "stored deployment manifest no longer satisfies the deploy contract",
            json!({
                "error_kind": "stored_deployment_manifest_invalid",
                "state_changed": false,
                "reason": reason,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        )
    })?;
    let object = manifest
        .as_object()
        .expect("validated deploy manifest object");
    let candidate_id = object
        .get("candidate_id")
        .and_then(Value::as_str)
        .expect("validated deploy candidate id")
        .to_string();
    let artifacts = object
        .get("artifacts")
        .and_then(Value::as_array)
        .expect("validated deploy artifacts")
        .iter()
        .map(|value| {
            let item = value.as_object().expect("validated deploy artifact");
            super::service_supervisor::SupervisorDeployArtifact {
                name: item["name"]
                    .as_str()
                    .expect("validated artifact name")
                    .to_string(),
                sha256: item["sha256"]
                    .as_str()
                    .expect("validated artifact digest")
                    .to_ascii_lowercase(),
                size_bytes: item["size_bytes"]
                    .as_u64()
                    .expect("validated artifact size"),
            }
        })
        .collect();
    Ok((candidate_id, artifacts))
}

fn reconcile_deploy_receipt(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
) -> Result<DeploymentReceiptRecord, ToolResult> {
    if record.operation != "deploy" || record.state != "switching" {
        return Ok(record);
    }
    let result =
        match super::service_supervisor::read_deploy_result(&record.receipt_id, record.revision) {
            Ok(Some(result)) => result,
            Ok(None) => {
                return reconcile_missing_supervisor_result(
                    db,
                    principal,
                    record,
                    "deploy",
                    "deploy_outcome_unknown",
                );
            }
            Err(error) => {
                return Err(ToolResult::err_with_output(
                    "signed supervisor deploy result could not be verified",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ));
            }
        };
    let backup = result.backup_id.as_deref();
    let updated = match result.status.as_str() {
        "succeeded" => {
            let verifying = transition_receipt_with_backup(
                db,
                principal,
                record,
                DeploymentState::Verifying,
                None,
                backup,
            )
            .map_err(store_error)?;
            transition_receipt_with_backup(
                db,
                principal,
                verifying,
                DeploymentState::Succeeded,
                None,
                backup,
            )
            .map_err(store_error)?
        }
        "rolled_back" => transition_receipt_with_backup(
            db,
            principal,
            record,
            DeploymentState::RolledBack,
            result
                .error_code
                .as_deref()
                .or(Some("candidate_start_failed")),
            backup,
        )
        .map_err(store_error)?,
        "outcome_unknown" => transition_receipt_with_backup(
            db,
            principal,
            record,
            DeploymentState::OutcomeUnknown,
            result
                .error_code
                .as_deref()
                .or(Some("deploy_outcome_unknown")),
            backup,
        )
        .map_err(store_error)?,
        _ => transition_receipt_with_backup(
            db,
            principal,
            record,
            DeploymentState::Failed,
            result.error_code.as_deref().or(Some("deploy_failed")),
            backup,
        )
        .map_err(store_error)?,
    };
    super::service_supervisor::remove_deploy_result(&updated.receipt_id);
    Ok(updated)
}

fn rollback_backup_id(record: &DeploymentReceiptRecord) -> Result<String, ToolResult> {
    if record.operation != "rollback" {
        return Err(ToolResult::err_with_output(
            "deployment receipt is not a rollback operation",
            json!({
                "error_kind": "deployment_receipt_operation_mismatch",
                "state_changed": false,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        ));
    }
    let manifest: Value = serde_json::from_str(&record.target_manifest_json).map_err(|_| {
        ToolResult::err_with_output(
            "stored rollback manifest is invalid JSON",
            json!({
                "error_kind": "stored_rollback_manifest_invalid",
                "state_changed": false,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        )
    })?;
    validate_manifest(DeploymentOperation::Rollback, &manifest).map_err(|reason| {
        ToolResult::err_with_output(
            "stored rollback manifest no longer satisfies the rollback contract",
            json!({
                "error_kind": "stored_rollback_manifest_invalid",
                "state_changed": false,
                "reason": reason,
                "deployment_receipt": receipt_json(record.clone()),
            }),
        )
    })?;
    Ok(manifest
        .get("backup_id")
        .and_then(Value::as_str)
        .expect("validated rollback backup id")
        .to_string())
}

fn reconcile_rollback_receipt(
    db: &crate::Database,
    principal: &DeploymentPrincipal,
    record: DeploymentReceiptRecord,
) -> Result<DeploymentReceiptRecord, ToolResult> {
    if record.operation != "rollback" || record.state != "switching" {
        return Ok(record);
    }
    let result = match super::service_supervisor::read_rollback_result(
        &record.receipt_id,
        record.revision,
    ) {
        Ok(Some(result)) => result,
        Ok(None) => {
            return reconcile_missing_supervisor_result(
                db,
                principal,
                record,
                "rollback",
                "rollback_outcome_unknown",
            );
        }
        Err(error) => {
            return Err(ToolResult::err_with_output(
                "signed supervisor rollback result could not be verified",
                json!({
                    "error_kind": error.code(),
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            ));
        }
    };
    let safety_backup = result.backup_id.as_deref();
    let updated = match result.status.as_str() {
        "succeeded" => {
            let verifying = transition_receipt_with_backup(
                db,
                principal,
                record,
                DeploymentState::Verifying,
                None,
                safety_backup,
            )
            .map_err(store_error)?;
            transition_receipt_with_backup(
                db,
                principal,
                verifying,
                DeploymentState::Succeeded,
                None,
                safety_backup,
            )
            .map_err(store_error)?
        }
        "outcome_unknown" => transition_receipt_with_backup(
            db,
            principal,
            record,
            DeploymentState::OutcomeUnknown,
            result
                .error_code
                .as_deref()
                .or(Some("rollback_outcome_unknown")),
            safety_backup,
        )
        .map_err(store_error)?,
        _ => transition_receipt_with_backup(
            db,
            principal,
            record,
            DeploymentState::Failed,
            result.error_code.as_deref().or(Some("rollback_failed")),
            safety_backup,
        )
        .map_err(store_error)?,
    };
    super::service_supervisor::remove_rollback_result(&updated.receipt_id);
    Ok(updated)
}

fn deployment_principal(auth: Option<&AuthContext>) -> Result<DeploymentPrincipal, String> {
    runtime_observation_principal(auth).map(|(kind, id)| DeploymentPrincipal { kind, id })
}

fn receipt_json(record: DeploymentReceiptRecord) -> Value {
    let target_manifest = serde_json::from_str::<Value>(&record.target_manifest_json)
        .unwrap_or_else(|_| json!({"status": "invalid_stored_manifest"}));
    let operation_phase =
        webcodex_core::operation_phase::OperationPhase::from_deployment_receipt_state(
            &record.state,
        );
    json!({
        "receipt_id": record.receipt_id,
        "owner_kind": record.owner_kind,
        "operation": record.operation,
        "client_id": record.client_id,
        "request_hash": record.request_hash,
        "target_manifest": target_manifest,
        "state": record.state,
        "operation_phase": operation_phase,
        "revision": record.revision,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "terminal_at": record.terminal_at,
        "last_error_code": record.last_error_code,
        "backup_id": record.backup_id,
    })
}

fn store_error(error: DeploymentReceiptStoreError) -> ToolResult {
    let (kind, message) = match error {
        DeploymentReceiptStoreError::InvalidInput(reason) => ("invalid_deployment_receipt", reason),
        DeploymentReceiptStoreError::IdempotencyConflict => (
            "deployment_idempotency_conflict",
            "deployment idempotency key was already used with a different request".to_string(),
        ),
        DeploymentReceiptStoreError::NotFound => (
            "deployment_receipt_not_found",
            "deployment receipt was not found for the current principal".to_string(),
        ),
        DeploymentReceiptStoreError::RevisionConflict { actual_revision } => (
            "deployment_revision_conflict",
            format!("deployment receipt revision changed (actual_revision={actual_revision})"),
        ),
        DeploymentReceiptStoreError::InvalidTransition { from, to } => (
            "deployment_invalid_transition",
            format!("deployment receipt transition is not allowed: {from} -> {to}"),
        ),
        DeploymentReceiptStoreError::Store(_) => (
            "deployment_store_error",
            "durable deployment receipt storage failed".to_string(),
        ),
    };
    ToolResult::err_with_output(
        message,
        json!({
            "error_kind": kind,
            "state_changed": false,
        }),
    )
}

impl ToolRuntime {
    pub(crate) async fn prepare_service_deployment(
        &self,
        auth: Option<&AuthContext>,
        client_id: String,
        operation: String,
        idempotency_key: String,
        target_manifest: Value,
    ) -> ToolResult {
        let client_id = client_id.trim().to_string();
        let operation = match parse_operation(operation.trim()) {
            Ok(DeploymentOperation::Restart) => {
                return ToolResult::err_with_output(
                    "restart uses the narrower service_restart workflow and does not require service:deploy",
                    json!({
                        "error_kind": "restart_requires_service_restart_workflow",
                        "state_changed": false,
                    }),
                )
            }
            Ok(operation) => operation,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "invalid_deployment_operation", "state_changed": false}),
                )
            }
        };
        let preflight = self
            .deployment_preflight(auth, client_id.clone(), operation.as_str().to_string())
            .await;
        if !preflight.success {
            return preflight;
        }
        if preflight
            .output
            .get("ready_to_begin")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return ToolResult::err_with_output(
                "deployment preflight is blocked; no durable deployment receipt was created",
                json!({
                    "error_kind": "deployment_preflight_blocked",
                    "state_changed": false,
                    "preflight": preflight.output,
                }),
            );
        }
        let target_manifest_json = match validate_manifest(operation, &target_manifest) {
            Ok(value) => value,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "invalid_deployment_manifest", "state_changed": false}),
                )
            }
        };
        let principal = match deployment_principal(auth) {
            Ok(principal) => principal,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "deployment_principal_unavailable", "state_changed": false}),
                )
            }
        };
        let Some(db) = self.deployment_db.as_ref() else {
            return ToolResult::err_with_output(
                "durable deployment receipt store is unavailable",
                json!({"error_kind": "deployment_store_unavailable", "state_changed": false}),
            );
        };
        let request_hash = request_hash(operation, &client_id, &target_manifest);
        let input = NewDeploymentReceipt {
            principal,
            operation,
            client_id,
            idempotency_key,
            request_hash,
            target_manifest_json,
            created_at: chrono::Utc::now().timestamp(),
        };
        match db.begin_deployment_receipt(&input) {
            Ok(DeploymentReceiptBeginOutcome::Created(record)) => ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "replayed": false,
                "state_changed": true,
                "preflight": preflight.output,
            })),
            Ok(DeploymentReceiptBeginOutcome::Replayed(record)) => ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "replayed": true,
                "state_changed": false,
                "preflight": preflight.output,
            })),
            Err(error) => store_error(error),
        }
    }

    pub(crate) async fn service_rollback(
        &self,
        auth: Option<&AuthContext>,
        receipt_id: String,
        expected_revision: u64,
        expected_generation: u64,
    ) -> ToolResult {
        if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
            || receipt_id.len() > 96
        {
            return ToolResult::err_with_output(
                "invalid deployment receipt id",
                json!({"error_kind": "invalid_deployment_receipt_id", "state_changed": false}),
            );
        }
        let principal = match deployment_principal(auth) {
            Ok(principal) => principal,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "deployment_principal_unavailable", "state_changed": false}),
                )
            }
        };
        let Some(db) = self.deployment_db.as_ref() else {
            return ToolResult::err_with_output(
                "durable deployment receipt store is unavailable",
                json!({"error_kind": "deployment_store_unavailable", "state_changed": false}),
            );
        };
        let mut record = match db.read_deployment_receipt(&principal, &receipt_id) {
            Ok(record) => record,
            Err(error) => return store_error(error),
        };
        if record.operation != "rollback" {
            return ToolResult::err_with_output(
                "service_rollback requires a rollback receipt",
                json!({
                    "error_kind": "deployment_receipt_operation_mismatch",
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        if matches!(
            record.state.as_str(),
            "succeeded" | "failed" | "rolled_back" | "outcome_unknown"
        ) {
            return ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "scheduled": false,
                "state_changed": false,
            }));
        }
        if record.state == "switching" {
            let before_revision = record.revision;
            record = match reconcile_rollback_receipt(db, &principal, record) {
                Ok(record) => record,
                Err(result) => return result,
            };
            if record.state != "switching" {
                return ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record.clone()),
                    "scheduled": false,
                    "state_changed": record.revision != before_revision,
                }));
            }
            return match super::service_supervisor::request_in_flight(
                "rollback",
                &record.receipt_id,
                record.revision,
            ) {
                Ok(true) => ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record),
                    "scheduled": true,
                    "state_changed": false,
                })),
                Ok(false) => ToolResult::err_with_output(
                    "rollback receipt is switching but neither a signed result nor the exact supervisor request lock is present; automatic redispatch is forbidden",
                    json!({
                        "error_kind": "rollback_execution_unresolved",
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ),
                Err(error) => ToolResult::err_with_output(
                    "rollback receipt is switching but supervisor execution state could not be verified",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ),
            };
        }
        if record.revision != expected_revision {
            return ToolResult::err_with_output(
                "deployment receipt revision changed; re-read the receipt before rollback",
                json!({
                    "error_kind": "deployment_revision_conflict",
                    "state_changed": false,
                    "expected_revision": expected_revision,
                    "actual_revision": record.revision,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let lifecycle = self.service_lifecycle.snapshot();
        if lifecycle.generation != expected_generation {
            return ToolResult::err_with_output(
                "service lifecycle generation changed; re-read runtime_status before rollback",
                json!({
                    "error_kind": "service_lifecycle_generation_conflict",
                    "state_changed": false,
                    "expected_generation": expected_generation,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !lifecycle.draining {
            return ToolResult::err_with_output(
                "service_rollback requires drain mode before cutover",
                json!({
                    "error_kind": "service_not_draining",
                    "state_changed": false,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !super::service_supervisor::available_for_client(&record.client_id) {
            return ToolResult::err_with_output(
                "standalone service supervisor does not manage the rollback receipt target",
                json!({
                    "error_kind": "supervisor_target_mismatch",
                    "state_changed": false,
                    "managed_client_id": super::service_supervisor::managed_client_id(),
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let preflight = self
            .deployment_preflight(auth, record.client_id.clone(), "rollback".to_string())
            .await;
        if !preflight.success {
            return preflight;
        }
        if preflight
            .output
            .get("ready_for_cutover")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return ToolResult::err_with_output(
                "rollback preflight is not ready for cutover; no supervisor request was scheduled",
                json!({
                    "error_kind": "rollback_preflight_not_ready",
                    "state_changed": false,
                    "preflight": preflight.output,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let target_backup_id = match rollback_backup_id(&record) {
            Ok(backup_id) => backup_id,
            Err(result) => return result,
        };
        let rollback_environment =
            match super::service_supervisor::rollback_environment_probe(&target_backup_id) {
                Ok(value) => value,
                Err(error) => {
                    return ToolResult::err_with_output(
                        "rollback backup/runtime environment failed pre-cutover validation",
                        json!({
                            "error_kind": error.code(),
                            "state_changed": false,
                            "preflight": preflight.output,
                            "deployment_receipt": receipt_json(record),
                        }),
                    );
                }
            };
        if rollback_environment
            .get("installed_runtime_present")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return ToolResult::err_with_output(
                "rollback environment is incomplete; installed WebPi runtime is not ready",
                json!({
                    "error_kind": "rollback_environment_not_ready",
                    "state_changed": false,
                    "preflight": preflight.output,
                    "rollback_environment": rollback_environment,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        if record.state == "planned" {
            record = match transition_receipt(db, &principal, record, DeploymentState::Ready, None)
            {
                Ok(record) => record,
                Err(error) => return store_error(error),
            };
        }
        if record.state != "ready" {
            return ToolResult::err_with_output(
                "rollback receipt is not in a schedulable state",
                json!({
                    "error_kind": "rollback_receipt_not_schedulable",
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        record = match transition_receipt(db, &principal, record, DeploymentState::Switching, None)
        {
            Ok(record) => record,
            Err(error) => return store_error(error),
        };
        match super::service_supervisor::schedule_rollback(
            &record.client_id,
            &record.receipt_id,
            record.revision,
            &target_backup_id,
        ) {
            Ok(super::service_supervisor::SupervisorScheduleOutcome::Scheduled)
            | Ok(super::service_supervisor::SupervisorScheduleOutcome::AlreadyScheduled) => {
                ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record),
                    "scheduled": true,
                    "state_changed": true,
                    "preflight": preflight.output,
                }))
            }
            Err(error) => {
                let failed = transition_receipt(
                    db,
                    &principal,
                    record,
                    DeploymentState::Failed,
                    Some(error.code()),
                );
                match failed {
                    Ok(failed) => ToolResult::err_with_output(
                        "standalone supervisor rejected the rollback request",
                        json!({
                            "error_kind": error.code(),
                            "state_changed": true,
                            "deployment_receipt": receipt_json(failed),
                        }),
                    ),
                    Err(store) => store_error(store),
                }
            }
        }
    }

    pub(crate) async fn service_deploy(
        &self,
        auth: Option<&AuthContext>,
        receipt_id: String,
        expected_revision: u64,
        expected_generation: u64,
    ) -> ToolResult {
        if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
            || receipt_id.len() > 96
        {
            return ToolResult::err_with_output(
                "invalid deployment receipt id",
                json!({"error_kind": "invalid_deployment_receipt_id", "state_changed": false}),
            );
        }
        let principal = match deployment_principal(auth) {
            Ok(principal) => principal,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "deployment_principal_unavailable", "state_changed": false}),
                )
            }
        };
        let Some(db) = self.deployment_db.as_ref() else {
            return ToolResult::err_with_output(
                "durable deployment receipt store is unavailable",
                json!({"error_kind": "deployment_store_unavailable", "state_changed": false}),
            );
        };
        let mut record = match db.read_deployment_receipt(&principal, &receipt_id) {
            Ok(record) => record,
            Err(error) => return store_error(error),
        };
        if record.operation != "deploy" {
            return ToolResult::err_with_output(
                "service_deploy requires a deploy receipt",
                json!({
                    "error_kind": "deployment_receipt_operation_mismatch",
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }

        if matches!(
            record.state.as_str(),
            "succeeded" | "failed" | "rolled_back" | "outcome_unknown"
        ) {
            return ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "scheduled": false,
                "state_changed": false,
            }));
        }

        if record.state == "switching" {
            let before_revision = record.revision;
            record = match reconcile_deploy_receipt(db, &principal, record) {
                Ok(record) => record,
                Err(result) => return result,
            };
            if record.state != "switching" {
                return ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record.clone()),
                    "scheduled": false,
                    "state_changed": record.revision != before_revision,
                }));
            }
            return match super::service_supervisor::request_in_flight(
                "deploy",
                &record.receipt_id,
                record.revision,
            ) {
                Ok(true) => ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record),
                    "scheduled": true,
                    "state_changed": false,
                })),
                Ok(false) => ToolResult::err_with_output(
                    "deploy receipt is switching but neither a signed result nor the exact supervisor request lock is present; automatic redispatch is forbidden",
                    json!({
                        "error_kind": "deploy_execution_unresolved",
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ),
                Err(error) => ToolResult::err_with_output(
                    "deploy receipt is switching but supervisor execution state could not be verified",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": false,
                        "deployment_receipt": receipt_json(record),
                    }),
                ),
            };
        }

        if record.revision != expected_revision {
            return ToolResult::err_with_output(
                "deployment receipt revision changed; re-read the receipt before cutover",
                json!({
                    "error_kind": "deployment_revision_conflict",
                    "state_changed": false,
                    "expected_revision": expected_revision,
                    "actual_revision": record.revision,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let lifecycle = self.service_lifecycle.snapshot();
        if lifecycle.generation != expected_generation {
            return ToolResult::err_with_output(
                "service lifecycle generation changed; re-read runtime_status before cutover",
                json!({
                    "error_kind": "service_lifecycle_generation_conflict",
                    "state_changed": false,
                    "expected_generation": expected_generation,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !lifecycle.draining {
            return ToolResult::err_with_output(
                "service_deploy requires drain mode before cutover",
                json!({
                    "error_kind": "service_not_draining",
                    "state_changed": false,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !super::service_supervisor::available_for_client(&record.client_id) {
            return ToolResult::err_with_output(
                "standalone service supervisor does not manage the deploy receipt target",
                json!({
                    "error_kind": "supervisor_target_mismatch",
                    "state_changed": false,
                    "managed_client_id": super::service_supervisor::managed_client_id(),
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let mut preflight = self
            .deployment_preflight(auth, record.client_id.clone(), "deploy".to_string())
            .await;
        if !preflight.success {
            return preflight;
        }
        if preflight
            .output
            .get("ready_for_cutover")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return ToolResult::err_with_output(
                "deploy preflight is not ready for cutover; no supervisor request was scheduled",
                json!({
                    "error_kind": "deploy_preflight_not_ready",
                    "state_changed": false,
                    "preflight": preflight.output,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let (candidate_id, artifacts) = match deploy_request_parts(&record) {
            Ok(parts) => parts,
            Err(result) => return result,
        };
        if db.deployment_receipt_store_probe().is_err() {
            return ToolResult::err_with_output(
                "deployment receipt store is not currently writable; cutover was not scheduled",
                json!({
                    "error_kind": "deployment_store_probe_failed",
                    "state_changed": false,
                    "preflight": preflight.output,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        let candidate_preflight = match super::service_supervisor::deployment_environment_probe(
            &candidate_id,
            &artifacts,
        ) {
            Ok(value) => value,
            Err(error) => {
                return ToolResult::err_with_output(
                    "deployment candidate/runtime environment failed pre-cutover validation",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": false,
                        "preflight": preflight.output,
                        "deployment_receipt": receipt_json(record),
                    }),
                );
            }
        };
        let environment_ready = candidate_preflight
            .get("installed_runtime_present")
            .and_then(Value::as_bool)
            == Some(true)
            && candidate_preflight
                .get("staging_parent_present")
                .and_then(Value::as_bool)
                == Some(true)
            && candidate_preflight
                .get("backup_parent_present")
                .and_then(Value::as_bool)
                == Some(true);
        if !environment_ready {
            return ToolResult::err_with_output(
                "deployment filesystem roots are not ready for staging/backup",
                json!({
                    "error_kind": "deployment_filesystem_not_ready",
                    "state_changed": false,
                    "candidate_preflight": candidate_preflight,
                    "preflight": preflight.output,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        if let Some(object) = preflight.output.as_object_mut() {
            object.insert("candidate_preflight".to_string(), candidate_preflight);
            object.insert("receipt_store_writable".to_string(), Value::Bool(true));
        }
        if record.state == "planned" {
            record = match transition_receipt(db, &principal, record, DeploymentState::Ready, None)
            {
                Ok(record) => record,
                Err(error) => return store_error(error),
            };
        }
        if record.state != "ready" {
            return ToolResult::err_with_output(
                "deploy receipt is not in a schedulable state",
                json!({
                    "error_kind": "deploy_receipt_not_schedulable",
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        record = match transition_receipt(db, &principal, record, DeploymentState::Switching, None)
        {
            Ok(record) => record,
            Err(error) => return store_error(error),
        };
        match super::service_supervisor::schedule_deploy(
            &record.client_id,
            &record.receipt_id,
            record.revision,
            &candidate_id,
            &artifacts,
        ) {
            Ok(super::service_supervisor::SupervisorScheduleOutcome::Scheduled)
            | Ok(super::service_supervisor::SupervisorScheduleOutcome::AlreadyScheduled) => {
                ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record),
                    "scheduled": true,
                    "state_changed": true,
                    "preflight": preflight.output,
                }))
            }
            Err(error) => {
                let failed = transition_receipt(
                    db,
                    &principal,
                    record,
                    DeploymentState::Failed,
                    Some(error.code()),
                );
                match failed {
                    Ok(failed) => ToolResult::err_with_output(
                        "standalone supervisor rejected the deploy request",
                        json!({
                            "error_kind": error.code(),
                            "state_changed": true,
                            "deployment_receipt": receipt_json(failed),
                        }),
                    ),
                    Err(store) => store_error(store),
                }
            }
        }
    }

    pub(crate) async fn service_restart(
        &self,
        auth: Option<&AuthContext>,
        client_id: String,
        idempotency_key: String,
        expected_generation: u64,
    ) -> ToolResult {
        let client_id = client_id.trim().to_string();
        let lifecycle = self.service_lifecycle.snapshot();
        if lifecycle.generation != expected_generation {
            return ToolResult::err_with_output(
                "service lifecycle generation changed; re-read runtime_status before retrying",
                json!({
                    "error_kind": "service_lifecycle_generation_conflict",
                    "state_changed": false,
                    "expected_generation": expected_generation,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !lifecycle.draining {
            return ToolResult::err_with_output(
                "service_restart requires drain mode before scheduling restart",
                json!({
                    "error_kind": "service_not_draining",
                    "state_changed": false,
                    "service_lifecycle": lifecycle.as_json(),
                }),
            );
        }
        if !super::service_supervisor::available() {
            return ToolResult::err_with_output(
                "standalone service supervisor is unavailable; restart was not scheduled",
                json!({
                    "error_kind": "supervisor_unavailable",
                    "state_changed": false,
                }),
            );
        }
        if !super::service_supervisor::available_for_client(&client_id) {
            return ToolResult::err_with_output(
                "standalone service supervisor does not manage the requested Runner",
                json!({
                    "error_kind": "supervisor_target_mismatch",
                    "state_changed": false,
                    "managed_client_id": super::service_supervisor::managed_client_id(),
                }),
            );
        }
        let preflight = self
            .deployment_preflight(auth, client_id.clone(), "restart".to_string())
            .await;
        if !preflight.success {
            return preflight;
        }
        if preflight
            .output
            .get("ready_for_cutover")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return ToolResult::err_with_output(
                "restart preflight is not ready for cutover; no restart was scheduled",
                json!({
                    "error_kind": "restart_preflight_not_ready",
                    "state_changed": false,
                    "preflight": preflight.output,
                }),
            );
        }
        let principal = match deployment_principal(auth) {
            Ok(principal) => principal,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "deployment_principal_unavailable", "state_changed": false}),
                )
            }
        };
        let Some(db) = self.deployment_db.as_ref() else {
            return ToolResult::err_with_output(
                "durable deployment receipt store is unavailable",
                json!({"error_kind": "deployment_store_unavailable", "state_changed": false}),
            );
        };
        let manifest = restart_manifest();
        let target_manifest_json = match serde_json::to_string(&manifest) {
            Ok(value) => value,
            Err(_) => {
                return ToolResult::err_with_output(
                    "current build identity could not be serialized",
                    json!({"error_kind": "restart_manifest_error", "state_changed": false}),
                )
            }
        };
        let input = NewDeploymentReceipt {
            principal: principal.clone(),
            operation: DeploymentOperation::Restart,
            client_id: client_id.clone(),
            idempotency_key,
            request_hash: restart_request_hash(&client_id, expected_generation, &manifest),
            target_manifest_json,
            created_at: chrono::Utc::now().timestamp(),
        };
        let (mut record, replayed) = match db.begin_deployment_receipt(&input) {
            Ok(DeploymentReceiptBeginOutcome::Created(record)) => (record, false),
            Ok(DeploymentReceiptBeginOutcome::Replayed(record)) => (record, true),
            Err(error) => return store_error(error),
        };

        if record.state == "switching" {
            let before = record.revision;
            record = match reconcile_restart_receipt(db, &principal, record) {
                Ok(record) => record,
                Err(result) => return result,
            };
            if record.state != "switching" {
                return ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record.clone()),
                    "scheduled": false,
                    "replayed": true,
                    "state_changed": record.revision != before,
                    "preflight": preflight.output,
                }));
            }
            let scheduled = match super::service_supervisor::schedule_restart(
                &client_id,
                &record.receipt_id,
                record.revision,
            ) {
                Ok(super::service_supervisor::SupervisorScheduleOutcome::Scheduled)
                | Ok(super::service_supervisor::SupervisorScheduleOutcome::AlreadyScheduled) => true,
                Err(error) => {
                    return ToolResult::err_with_output(
                        "restart receipt is switching but the supervisor request could not be recovered",
                        json!({
                            "error_kind": error.code(),
                            "state_changed": false,
                            "deployment_receipt": receipt_json(record),
                            "preflight": preflight.output,
                        }),
                    )
                }
            };
            return ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "scheduled": scheduled,
                "replayed": true,
                "state_changed": false,
                "preflight": preflight.output,
            }));
        }
        if matches!(
            record.state.as_str(),
            "succeeded" | "failed" | "rolled_back" | "outcome_unknown"
        ) {
            return ToolResult::ok(json!({
                "deployment_receipt": receipt_json(record),
                "scheduled": false,
                "replayed": true,
                "state_changed": false,
                "preflight": preflight.output,
            }));
        }
        if record.state == "planned" {
            record = match transition_receipt(db, &principal, record, DeploymentState::Ready, None)
            {
                Ok(record) => record,
                Err(error) => return store_error(error),
            };
        }
        if record.state != "ready" {
            return ToolResult::err_with_output(
                "restart receipt is not in a schedulable state",
                json!({
                    "error_kind": "restart_receipt_not_schedulable",
                    "state_changed": false,
                    "deployment_receipt": receipt_json(record),
                }),
            );
        }
        record = match transition_receipt(db, &principal, record, DeploymentState::Switching, None)
        {
            Ok(record) => record,
            Err(error) => return store_error(error),
        };
        if let Err(error) = super::service_supervisor::schedule_restart(
            &client_id,
            &record.receipt_id,
            record.revision,
        ) {
            let failed = transition_receipt(
                db,
                &principal,
                record,
                DeploymentState::Failed,
                Some(error.code()),
            );
            return match failed {
                Ok(failed) => ToolResult::err_with_output(
                    "standalone supervisor rejected the restart request",
                    json!({
                        "error_kind": error.code(),
                        "state_changed": true,
                        "deployment_receipt": receipt_json(failed),
                    }),
                ),
                Err(store) => store_error(store),
            };
        }
        ToolResult::ok(json!({
            "deployment_receipt": receipt_json(record),
            "scheduled": true,
            "replayed": replayed,
            "state_changed": true,
            "preflight": preflight.output,
        }))
    }

    pub(crate) fn read_deployment_receipt(
        &self,
        auth: Option<&AuthContext>,
        receipt_id: String,
    ) -> ToolResult {
        if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
            || receipt_id.len() > 96
        {
            return ToolResult::err_with_output(
                "invalid deployment receipt id",
                json!({"error_kind": "invalid_deployment_receipt_id", "state_changed": false}),
            );
        }
        let principal = match deployment_principal(auth) {
            Ok(principal) => principal,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({"error_kind": "deployment_principal_unavailable", "state_changed": false}),
                )
            }
        };
        let Some(db) = self.deployment_db.as_ref() else {
            return ToolResult::err_with_output(
                "durable deployment receipt store is unavailable",
                json!({"error_kind": "deployment_store_unavailable", "state_changed": false}),
            );
        };
        match db.read_deployment_receipt(&principal, &receipt_id) {
            Ok(record) => {
                let before_revision = record.revision;
                let record = match reconcile_restart_receipt(db, &principal, record) {
                    Ok(record) => record,
                    Err(result) => return result,
                };
                let record = match reconcile_deploy_receipt(db, &principal, record) {
                    Ok(record) => record,
                    Err(result) => return result,
                };
                let record = match reconcile_rollback_receipt(db, &principal, record) {
                    Ok(record) => record,
                    Err(result) => return result,
                };
                ToolResult::ok(json!({
                    "deployment_receipt": receipt_json(record.clone()),
                    "state_changed": record.revision != before_revision,
                }))
            }
            Err(error) => store_error(error),
        }
    }
}

#[cfg(test)]
mod deployment_diagnostic_tests {
    use super::*;
    use webcodex_core::runtime_diagnostics::DiagnosticSeverity;

    #[test]
    fn only_terminal_deployment_states_emit_runtime_diagnostics() {
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::Planned),
            None
        );
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::Switching),
            None
        );
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::Succeeded),
            Some((DiagnosticSeverity::Info, "succeeded"))
        );
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::RolledBack),
            Some((DiagnosticSeverity::Warn, "rolled_back"))
        );
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::Failed),
            Some((DiagnosticSeverity::Error, "failed"))
        );
        assert_eq!(
            deployment_terminal_diagnostic(DeploymentState::OutcomeUnknown),
            Some((DiagnosticSeverity::Warn, "outcome_unknown"))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_idempotency_hash_is_bound_to_lifecycle_generation() {
        let manifest = json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "artifacts": []
        });
        let generation_one = restart_request_hash("webpi-local", 1, &manifest);
        let generation_two = restart_request_hash("webpi-local", 2, &manifest);
        assert_ne!(generation_one, generation_two);
        assert_eq!(
            generation_one,
            restart_request_hash("webpi-local", 1, &manifest)
        );
    }

    #[test]
    fn manifest_rejects_unknown_and_secret_shaped_fields() {
        let manifest = json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "artifacts": [],
            "authorization": "Bearer secret"
        });
        assert!(validate_manifest(DeploymentOperation::Restart, &manifest).is_err());
    }

    #[test]
    fn deploy_manifest_requires_bounded_artifact_identity() {
        let manifest = json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "candidate_id": "candidate-20260922",
            "artifacts": [
                {"name": "webpi.exe", "sha256": "b".repeat(64), "size_bytes": 1024},
                {"name": "webpi-server.exe", "sha256": "c".repeat(64), "size_bytes": 2048},
                {"name": "webpi-runner.exe", "sha256": "d".repeat(64), "size_bytes": 4096}
            ]
        });
        assert!(validate_manifest(DeploymentOperation::Deploy, &manifest).is_ok());
    }

    #[test]
    fn rollback_manifest_is_backup_only_and_rejects_path_like_identity() {
        let valid = json!({"backup_id": "backup-source-r4"});
        assert!(validate_manifest(DeploymentOperation::Rollback, &valid).is_ok());

        let path_like = json!({"backup_id": "../backup"});
        assert!(validate_manifest(DeploymentOperation::Rollback, &path_like).is_err());

        let extra = json!({
            "backup_id": "backup-source-r4",
            "candidate_id": "candidate-1"
        });
        assert!(validate_manifest(DeploymentOperation::Rollback, &extra).is_err());
    }

    #[test]
    fn deploy_manifest_rejects_path_like_candidate_and_incomplete_artifacts() {
        let path_like = json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "candidate_id": "../candidate",
            "artifacts": [
                {"name": "webpi.exe", "sha256": "b".repeat(64), "size_bytes": 1024},
                {"name": "webpi-server.exe", "sha256": "c".repeat(64), "size_bytes": 2048},
                {"name": "webpi-runner.exe", "sha256": "d".repeat(64), "size_bytes": 4096}
            ]
        });
        assert!(validate_manifest(DeploymentOperation::Deploy, &path_like).is_err());

        let incomplete = json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "candidate_id": "candidate-20260922",
            "artifacts": [
                {"name": "webpi.exe", "sha256": "b".repeat(64), "size_bytes": 1024}
            ]
        });
        assert!(validate_manifest(DeploymentOperation::Deploy, &incomplete).is_err());
    }
}
