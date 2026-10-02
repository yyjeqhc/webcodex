//! External reports never enter native execution/validation projections.
use super::{RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use webcodex_store::{ExternalObservation, ExternalObservationError};

const MAX_HANDOFF_EXTERNAL_OBSERVATIONS: usize = 5;

fn external_observation_coverage(reason: &str) -> Value {
    json!({
        "complete": false,
        "reason": reason,
        "ordering": "server_recorded_at_then_identity",
    })
}

fn project_observation(value: ExternalObservation) -> Value {
    let status = match value.exit_code {
        Some(0) => "reported_success",
        Some(_) => "reported_failure",
        None => "unknown",
    };
    let mut result = serde_json::to_value(value).expect("observation serializes");
    result["status"] = json!(status);
    result
}

impl ToolRuntime {
    /// Read the exact authorized Session Project's retained external reports
    /// for a handoff. This is a separate, bounded claim projection: it never
    /// materializes native Session events or changes closeout decisions.
    pub(super) fn handoff_external_observations(
        &self,
        session_id: &str,
        session_project: Option<&str>,
    ) -> Value {
        let unavailable = |reason_code: &str| {
            json!({
                "status": "unavailable",
                "reason_code": reason_code,
                "provenance": "external_report",
                "coverage": external_observation_coverage("read_unavailable"),
                "total": null,
                "returned": null,
                "truncated": null,
                "unknown_count": null,
                "observations": null,
            })
        };
        let Some(project) = session_project else {
            return unavailable("session_project_unavailable");
        };
        let Some(db) = self.communication_db.as_ref() else {
            return unavailable("store_unavailable");
        };
        let Ok(rows) = db.list_external_observations(session_id, project) else {
            return unavailable("store_unavailable");
        };
        let total = rows.len();
        let unknown_count = rows.iter().filter(|row| row.exit_code.is_none()).count();
        // The store sorts by server timestamp and identity, not source order.
        // Keep the last few in that order while retaining the whole count.
        let mut observations = rows
            .into_iter()
            .rev()
            .take(MAX_HANDOFF_EXTERNAL_OBSERVATIONS)
            .map(project_observation)
            .collect::<Vec<_>>();
        observations.reverse();
        let returned = observations.len();
        json!({
            "status": "available",
            "reason_code": null,
            "provenance": "external_report",
            "coverage": external_observation_coverage("source_sequence_unavailable"),
            "total": total,
            "returned": returned,
            "truncated": returned < total,
            "unknown_count": unknown_count,
            "observations": observations,
        })
    }

    pub(crate) async fn external_observation_tool(
        &self,
        project: String,
        session_id: String,
        input: Option<ExternalObservation>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let is_record = input.is_some();
        let name = if is_record {
            "record_external_observation"
        } else {
            "list_external_observations"
        };
        if let Err(result) = self.authorize_session_target(&session_id, name, auth).await {
            return result;
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(value) => value,
            Err(error) => return error.into_tool_result(),
        };
        let Some(summary) = self.sessions.summary(&session_id, Some(0)) else {
            return super::session_context::absent_workflow_session_result(
                &self.sessions,
                &session_id,
                auth,
            );
        };
        if project != resolved.resolved_id || summary.project.as_deref() != Some(project.as_str()) {
            return ToolResult::err_with_output(
                "External observations require the exact Session Project",
                json!({
                    "error_kind":"session_project_mismatch", "state_changed":false,
                }),
            );
        }
        if input.is_some() && !summary.lifecycle.allows_mutation() {
            return ToolResult::err_with_output(
                "Session is closed",
                json!({"error_kind":"session_closed", "state_changed":false}),
            );
        }
        let Some(db) = self.communication_db.as_ref() else {
            return if is_record {
                ToolResult::err_with_output(
                    "external_observation_store_unavailable",
                    json!({
                        "error_kind": "external_observation_store_unavailable",
                        "state_changed": false,
                        "retry_same_event_identity": true,
                    }),
                )
                .with_recovery(RecoveryKind::RetrySame)
            } else {
                ToolResult::err_with_output(
                    "external_observation_store_unavailable",
                    json!({
                        "error_kind": "external_observation_store_unavailable",
                        "state_changed": false,
                    }),
                )
                .with_recovery(RecoveryKind::Reobserve)
            };
        };
        let result = if let Some(input) = input {
            db.record_external_observation(&session_id, &project, input)
                .map(|(record, inserted)| {
                    json!({
                        "session_id":session_id, "project":project, "provenance":"external_report",
                        "inserted":inserted, "observation":project_observation(record),
                    })
                })
        } else {
            db.list_external_observations(&session_id, &project).map(|rows| json!({
                "session_id":session_id, "project":project, "provenance":"external_report",
                "coverage": external_observation_coverage("source_sequence_unavailable"),
                "observations": rows.into_iter().map(project_observation).collect::<Vec<_>>(),
            }))
        };
        match result {
            Ok(output) => ToolResult::ok(output),
            Err(ExternalObservationError::InvalidInput) => ToolResult::err_with_output(
                "invalid_external_observation",
                json!({
                    "error_kind": "invalid_external_observation",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput),
            Err(ExternalObservationError::Conflict) => ToolResult::err_with_output(
                "external_observation_conflict",
                json!({
                    "error_kind": "external_observation_conflict",
                    "failure_kind": "conflict",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput),
            Err(ExternalObservationError::Capacity) => ToolResult::err_with_output(
                "external_observation_capacity",
                json!({
                    "error_kind": "external_observation_capacity",
                    "state_changed": false,
                    "recovery": "Retained external-observation capacity is full; do not mint a new event identity to bypass the bound.",
                }),
            )
            .with_recovery(RecoveryKind::UserAction),
            Err(ExternalObservationError::Storage) if is_record => {
                // SQLite commit failure may mean the exact keyed report committed
                // but its acknowledgement was lost. The only safe retry is the
                // same Session + adapter_id + event_id + canonical payload.
                ToolResult::err_with_output(
                    "external_observation_storage_uncertain",
                    json!({
                        "error_kind": "external_observation_storage_uncertain",
                        "failure_kind": "outcome_unknown",
                        "state_changed": Value::Null,
                        "retry_same_event_identity": true,
                        "recovery": "Retry the same Session, adapter_id, event_id and payload; never replay the observed business operation.",
                    }),
                )
                .with_recovery(RecoveryKind::RetrySame)
            }
            Err(ExternalObservationError::Storage) => ToolResult::err_with_output(
                "external_observation_store_unavailable",
                json!({
                    "error_kind": "external_observation_store_unavailable",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::Reobserve),
        }
    }
}
