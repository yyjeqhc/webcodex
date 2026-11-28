//! Admin removal of one stale Runner registration.
//!
//! This is the control-plane counterpart to a Runner leaving the fleet by
//! itself. It only forgets a Server-side registration record that is provably
//! idle; the Runner machine is never contacted, and its local credentials,
//! configuration, projects and files are untouched. The operation converges on
//! the target state: a registration that is already absent reports
//! `already_unregistered` instead of failing, so retries are replay-safe by
//! construction and no idempotency journal is needed.

use crate::admin_project_lifecycle::{api_error, subject_id, ServiceResponse};
use crate::auth::AuthContext;
use crate::db::AdminRunnerAudit;
use crate::tool_runtime::ToolRuntime;
use crate::Database;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use webcodex_runner_registry::RunnerRemovalRefusal;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunnerUnregisterRequest {
    pub client_id: String,
    /// Explicit destructive-action confirmation, matching the project
    /// lifecycle envelope.
    pub confirm: bool,
}

pub(crate) struct AdminRunnerLifecycleService {
    runtime: Arc<ToolRuntime>,
    db: Arc<Database>,
}

impl AdminRunnerLifecycleService {
    pub(crate) fn new(runtime: Arc<ToolRuntime>, db: Arc<Database>) -> Self {
        Self { runtime, db }
    }

    pub(crate) async fn unregister(
        &self,
        auth: &AuthContext,
        request: RunnerUnregisterRequest,
    ) -> ServiceResponse {
        let client_id = request.client_id;
        let response = if !request.confirm || !valid_client_id(&client_id) {
            api_error(400, "invalid_request")
        } else {
            let access = crate::runner_http::runner_access_from_auth(Some(auth));
            let removal = self
                .runtime
                .runner_registry
                .remove_stale_runner_for_auth(access.as_ref(), &client_id)
                .await;
            removal_response(&client_id, removal)
        };
        self.audit(auth, &client_id, &response);
        response
    }

    fn audit(&self, auth: &AuthContext, client_id: &str, response: &ServiceResponse) {
        let outcome = response
            .body
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("failed");
        let projects_removed = response
            .body
            .get("projects_removed")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let reason_code = response.body.pointer("/error/code").and_then(Value::as_str);
        let subject_id = subject_id(auth);
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let _ = self
            .db
            .insert_admin_runner_lifecycle_audit(&AdminRunnerAudit {
                correlation_id: &correlation_id,
                subject_type: if auth.is_bootstrap() {
                    "bootstrap"
                } else {
                    "admin_pat"
                },
                subject_id: &subject_id,
                operation: "unregister",
                runner: client_id,
                outcome,
                changed: outcome == "unregistered",
                projects_removed,
                reason_code,
            });
    }
}

fn removal_response(
    client_id: &str,
    removal: Result<webcodex_runner_registry::RunnerRemovalOutcome, RunnerRemovalRefusal>,
) -> ServiceResponse {
    match removal {
        Ok(outcome) => {
            tracing::info!(
                runner = %outcome.client_id,
                projects_removed = outcome.projects_removed,
                "admin removed a stale Runner registration"
            );
            ServiceResponse {
                status: 200,
                body: json!({
                    "operation": "unregister",
                    "runner": outcome.client_id,
                    "outcome": "unregistered",
                    "changed": true,
                    "projects_removed": outcome.projects_removed,
                    "warnings": []
                }),
            }
        }
        // Absence is the target state, so a repeated or racing removal still
        // reports the desired end state instead of an error.
        Err(RunnerRemovalRefusal::UnknownRunner) => ServiceResponse {
            status: 200,
            body: json!({
                "operation": "unregister",
                "runner": client_id,
                "outcome": "already_unregistered",
                "changed": false,
                "projects_removed": 0,
                "warnings": []
            }),
        },
        Err(RunnerRemovalRefusal::Connected) => api_error(409, "runner_connected"),
        Err(RunnerRemovalRefusal::StillOnline { .. }) => api_error(409, "runner_online"),
        Err(RunnerRemovalRefusal::ActiveJobs { active }) => ServiceResponse {
            status: 409,
            body: json!({"error":{"code":"active_jobs_conflict"},"active_jobs":active}),
        },
        Err(RunnerRemovalRefusal::PendingRequests { pending }) => ServiceResponse {
            status: 409,
            body: json!({"error":{"code":"runner_busy"},"pending_requests":pending}),
        },
        Err(RunnerRemovalRefusal::Maintenance) => api_error(409, "runner_maintenance"),
    }
}

/// The same `client_id` shape accepted at registration ingress.
fn valid_client_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_shape_matches_registration_ingress() {
        assert!(valid_client_id("gpu21-7ff4f11c8ef74468"));
        assert!(valid_client_id("cu31vm"));
        assert!(!valid_client_id(""));
        assert!(!valid_client_id("has space"));
        assert!(!valid_client_id(&"a".repeat(129)));
        assert!(!valid_client_id("agent:cu31vm:project"));
    }

    #[test]
    fn absence_converges_and_refusals_are_typed_and_safe() {
        let missing = removal_response("gone", Err(RunnerRemovalRefusal::UnknownRunner));
        assert_eq!(missing.status, 200);
        assert_eq!(missing.body["outcome"], "already_unregistered");
        assert_eq!(missing.body["changed"], false);

        let connected = removal_response("live", Err(RunnerRemovalRefusal::Connected));
        assert_eq!(connected.status, 409);
        assert_eq!(connected.body["error"]["code"], "runner_connected");

        let online = removal_response(
            "recent",
            Err(RunnerRemovalRefusal::StillOnline { last_seen: 7 }),
        );
        assert_eq!(online.status, 409);
        assert_eq!(online.body["error"]["code"], "runner_online");

        let jobs = removal_response("busy", Err(RunnerRemovalRefusal::ActiveJobs { active: 3 }));
        assert_eq!(jobs.status, 409);
        assert_eq!(jobs.body["error"]["code"], "active_jobs_conflict");
        assert_eq!(jobs.body["active_jobs"], 3);

        let queued = removal_response(
            "busy",
            Err(RunnerRemovalRefusal::PendingRequests { pending: 2 }),
        );
        assert_eq!(queued.status, 409);
        assert_eq!(queued.body["error"]["code"], "runner_busy");

        let maintenance = removal_response("upgrading", Err(RunnerRemovalRefusal::Maintenance));
        assert_eq!(maintenance.status, 409);
        assert_eq!(maintenance.body["error"]["code"], "runner_maintenance");
    }

    #[test]
    fn removed_registration_reports_exact_counts() {
        let removed = removal_response(
            "stale",
            Ok(webcodex_runner_registry::RunnerRemovalOutcome {
                client_id: "stale".to_string(),
                runner_instance_id: "instance".to_string(),
                projects_removed: 2,
            }),
        );
        assert_eq!(removed.status, 200);
        assert_eq!(removed.body["outcome"], "unregistered");
        assert_eq!(removed.body["changed"], true);
        assert_eq!(removed.body["projects_removed"], 2);
    }
}
