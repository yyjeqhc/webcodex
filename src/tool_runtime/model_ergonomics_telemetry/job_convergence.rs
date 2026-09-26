//! Job ergonomics use existing Action Audit rows, not a second recorder. Exact
//! salted relations and adapter ordering/timing support offline convergence
//! analysis without a global "last Job" guess or durable timing authority.
use crate::auth::{AuthContext, SCOPE_RUNTIME_READ};
use crate::client_window::ClientWindow;
use crate::tool_runtime::{ToolResult, ToolRuntime};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MAX_EVENTS: usize = webcodex_runner_registry::MAX_JOB_TELEMETRY_SNAPSHOTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobEventKind {
    PendingHandoff,
    ExplicitObserve,
    PassiveTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JobEvent {
    pub(crate) kind: JobEventKind,
    /// Salted exact principal/Window/Project/business-Session/Job relation.
    pub(crate) relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) validation_failure: Option<bool>,
    /// Existing first Server terminal observation, with second resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) terminal_observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct JobConvergenceRecord {
    pub(crate) pending_handoff_count: u8,
    pub(crate) passive_terminal_delivery_count: u8,
    pub(crate) passive_failure_delivery_count: u8,
    pub(crate) wait_for_job_terminal_count: u8,
    /// False on missing scope/Window, registry contention, eviction or denial.
    pub(crate) correlation_complete: bool,
    pub(crate) events: Vec<JobEvent>,
}

fn relation(salt: &[u8; 16], fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"webcodex-job-ergonomics-v1");
    hash.update(salt);
    for field in fields {
        hash.update((field.len() as u64).to_be_bytes());
        hash.update(field.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

impl ToolRuntime {
    pub(crate) fn job_convergence_record(
        &self,
        tool: &str,
        result: &ToolResult,
        context: &super::super::window_activity::ToolCallCorrelation,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
    ) -> Option<JobConvergenceRecord> {
        let mut record = JobConvergenceRecord {
            wait_for_job_terminal_count: u8::from(tool == "wait_for_job_terminal"),
            ..Default::default()
        };
        let mut selected = Vec::new();
        let mut observation_complete = tool != "observe_jobs";
        if result.success {
            let passive_eligible =
                webcodex_tool_contracts::runtime_tool_supports_passive_job_attention(tool);
            if let Some(id) = super::super::job_attention::pending_continuation_job_id(result)
                .filter(|_| passive_eligible)
            {
                record.pending_handoff_count = 1;
                selected.push((id, JobEventKind::PendingHandoff, None, None));
            }
            if tool == "observe_jobs" {
                if let Some(items) = result.output["items"].as_array() {
                    observation_complete = !items.is_empty()
                        && items.len() <= super::super::observe_jobs::MAX_OBSERVE_JOBS_ITEMS;
                    for item in items
                        .iter()
                        .take(super::super::observe_jobs::MAX_OBSERVE_JOBS_ITEMS)
                    {
                        // Both the canonical batch and sparse success projection.
                        if item["success"] == true
                            || item.get("terminal").is_some_and(|value| value.is_boolean())
                        {
                            if let Some(id) = item["job_id"].as_str() {
                                selected.push((id, JobEventKind::ExplicitObserve, None, None));
                                continue;
                            }
                        }
                        observation_complete = false;
                    }
                }
            }
            if let Some(items) = result.output["job_attention"]["items"]
                .as_array()
                .filter(|_| passive_eligible)
            {
                for item in items
                    .iter()
                    .take(super::super::observe_jobs::MAX_OBSERVE_JOBS_ITEMS)
                {
                    if item["state"] != "terminal" {
                        continue;
                    }
                    let Some(id) = item["job_id"].as_str() else {
                        continue;
                    };
                    let failed =
                        item["validation"]["passed"] == false || item["command_ok"] == false;
                    record.passive_terminal_delivery_count += 1;
                    record.passive_failure_delivery_count += u8::from(failed);
                    selected.push((
                        id,
                        JobEventKind::PassiveTerminal,
                        Some(failed),
                        Some(item["validation"]["passed"] == false),
                    ));
                }
            }
        }
        if selected.is_empty() && record.wait_for_job_terminal_count == 0 && observation_complete {
            return None;
        }
        selected.truncate(MAX_EVENTS);
        let (Some(auth), Some(window)) = (auth, window) else {
            return Some(record);
        };
        if auth.is_open_anonymous() || !auth.has_scope(SCOPE_RUNTIME_READ) {
            return Some(record);
        }
        let Ok((kind, principal)) =
            super::super::session_context::runtime_observation_principal(Some(auth))
        else {
            return Some(record);
        };
        let ids: Vec<_> = selected.iter().map(|(id, _, _, _)| *id).collect();
        let Some(jobs) = self.runner_registry.try_job_telemetry_snapshots_for_auth(
            crate::runner_http::runner_access_from_auth(Some(auth)).as_ref(),
            &ids,
        ) else {
            return Some(record);
        };
        for (id, event_kind, failure, validation_failure) in &selected {
            let Some(job) = jobs.iter().find(|job| job.job_id == *id) else {
                continue;
            };
            let (Some(project), Some(session)) =
                (job.project_id.as_deref(), job.session_id.as_deref())
            else {
                continue;
            };
            if context
                .resolved_project
                .as_deref()
                .is_some_and(|expected| expected != project)
                || context
                    .business_session_id
                    .as_deref()
                    .is_some_and(|expected| expected != session)
            {
                continue;
            }
            // Explicit observation may select a Job without a business Session
            // argument. Its durable Session is correlation only; it never fills
            // request context, authorizes work, or becomes recorder identity.
            record.events.push(JobEvent {
                kind: *event_kind,
                relation: relation(
                    &self.job_ergonomics_salt,
                    &[&kind, &principal, window.key(), project, session, id],
                ),
                failure: *failure,
                validation_failure: *validation_failure,
                terminal_observed_at_ms: (*event_kind != JobEventKind::PendingHandoff)
                    .then_some(job.terminal_observed_at)
                    .flatten()
                    .and_then(|seconds| seconds.checked_mul(1000)),
            });
        }
        record.correlation_complete = observation_complete && record.events.len() == selected.len();
        Some(record)
    }
}
