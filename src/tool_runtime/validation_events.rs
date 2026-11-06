//! Root-owned live validation adapter.
//!
//! Canonical validation planning/evidence semantics live in `webcodex-validation`;
//! this module retains only authorization, live Job materialization, SessionStore
//! mutation, and model-facing ToolResult composition.

use serde_json::{json, Value};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runner_protocol::ShellJobInfo;
use webcodex_core::validation_source::{ValidationFreshness, ValidationSourceState};
use webcodex_tool_runtime_contracts::tool_audit::{
    assertion_validation_identity, is_structured_validation_target_identity,
    is_validation_execution_identity,
};
#[cfg(test)]
use webcodex_validation::validation_summary_for_session_events;
use webcodex_validation::{event_is_job_acceptance_only, validation_evidence_profile_for_tool};
use webcodex_workflow_session::{
    canonical_tool_call_finished_events, safe_model_facing_assertion_name, SessionEvent,
    SessionSummary,
};

use super::session_context::{
    absent_workflow_session_result, session_project_mismatch_result, SessionProjectMismatch,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;

#[cfg(test)]
pub(crate) use webcodex_validation::{
    current_validation_evidence_for_session, CurrentValidationEvidenceProjection,
};
pub(crate) use webcodex_validation::{
    event_observes_validation_activity, validation_summary_from_events,
};

const DEFAULT_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 20;
const MAX_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 100;
const PUBLIC_VALIDATION_SESSION_EVENT_LIMIT: usize = 200;

#[cfg(test)]
pub(crate) fn validation_summary_for_session(summary: &SessionSummary) -> Value {
    validation_summary_for_session_events(summary, &summary.events, 10)
}

impl ToolRuntime {
    pub(crate) async fn validation_summary_tool(
        &self,
        project: String,
        session_id: String,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(result) = self
            .authorize_session_target(&session_id, "read_validation_summary", auth)
            .await
        {
            return result;
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let Some(summary) = self
            .sessions
            .summary(&session_id, Some(PUBLIC_VALIDATION_SESSION_EVENT_LIMIT))
        else {
            return absent_workflow_session_result(&self.sessions, &session_id, auth);
        };
        if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
            let mismatch = SessionProjectMismatch {
                session_project: summary.project.unwrap_or_else(|| "<unscoped>".to_string()),
                request_project: resolved.resolved_id,
            };
            return session_project_mismatch_result(
                &session_id,
                "read_validation_summary",
                &mismatch,
            );
        }
        let limit = limit
            .unwrap_or(DEFAULT_PUBLIC_VALIDATION_EVENT_LIMIT)
            .clamp(1, MAX_PUBLIC_VALIDATION_EVENT_LIMIT);
        let mut validation = self
            .validation_summary_for_session_with_jobs(&summary, limit, auth)
            .await;
        remove_public_validation_input_summaries(&mut validation);
        // Pure read-only projection derived only from the ledger validation
        // summary above. Never re-runs validation, mutates the ledger, or
        // changes the verdict; `unavailable` with a stable reason code when the
        // two validation attempts are not proven comparable.
        let validation_delta = super::continuation_feedback::validation_delta_value(&validation);
        ToolResult::ok(json!({
            "project": resolved.resolved_id,
            "session_id": session_id,
            "validation": validation,
            "validation_delta": validation_delta,
        }))
    }

    pub(crate) async fn validation_summary_for_session_with_jobs(
        &self,
        summary: &SessionSummary,
        limit: usize,
        auth: Option<&AuthContext>,
    ) -> Value {
        self.validation_report_for_session_with_jobs(summary, limit, auth)
            .await
            .to_value()
    }

    pub(crate) async fn validation_report_for_session_with_jobs(
        &self,
        summary: &SessionSummary,
        limit: usize,
        auth: Option<&AuthContext>,
    ) -> webcodex_validation::ValidationReport {
        self.materialize_session_validation_job_terminals(summary, auth)
            .await;
        let refreshed = self
            .sessions
            .summary(
                &summary.session_id,
                Some(PUBLIC_VALIDATION_SESSION_EVENT_LIMIT),
            )
            .unwrap_or_else(|| summary.clone());
        let mut events = refreshed.events.clone();
        self.append_terminal_generic_job_validation_events(&refreshed, &mut events, auth)
            .await;
        self.refresh_validation_source_states(&mut events);
        events.sort_by_key(|event| {
            (
                event.timestamp,
                event.finished_at.unwrap_or(event.timestamp),
            )
        });
        let refreshed = self.refresh_validation_source_summary(&refreshed);
        webcodex_validation::ValidationReport::from_events(&refreshed, &events, limit)
    }

    /// Refresh only the in-memory validation-source projection. This never
    /// materializes Jobs, appends Session events, or rewrites durable execution
    /// facts, so read-only presentation surfaces may safely use it too.
    pub(crate) fn refresh_validation_source_states(&self, events: &mut [SessionEvent]) {
        // This also sees canonical writes from other Sessions on this Project.
        for event in events {
            let project = event
                .resolved_project
                .as_deref()
                .or(event.project.as_deref());
            if let (Some(project), Some(output)) =
                (project, event.validation_output_summary.as_mut())
            {
                let source = output
                    .get("source_state")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<ValidationSourceState>(value).ok());
                if let Some(source) = source {
                    if source.freshness != ValidationFreshness::Stale {
                        output["source_state"] = json!(self
                            .validation_sources
                            .observe(project, source.start_fence.as_ref()));
                    }
                }
            }
        }
    }

    pub(crate) fn refresh_validation_source_summary(
        &self,
        summary: &SessionSummary,
    ) -> SessionSummary {
        let mut refreshed = summary.clone();
        self.refresh_validation_source_states(&mut refreshed.events);
        refreshed
    }

    /// Preserve generic `run_job` and promoted `run_shell` validation evidence without mixing
    /// those shell Jobs into the durable structured-validation marker ledger. Structured
    /// validation Jobs carry explicit validation metadata and are materialized above; these
    /// generic executions remain a read-time projection from the retained acceptance event plus
    /// the authoritative terminal Job state.
    async fn append_terminal_generic_job_validation_events(
        &self,
        summary: &SessionSummary,
        events: &mut Vec<SessionEvent>,
        auth: Option<&AuthContext>,
    ) {
        let Some(project) = summary.project.as_deref() else {
            return;
        };
        let accepted = canonical_tool_call_finished_events(&summary.events)
            .into_iter()
            .filter(|event| {
                matches!(event.tool_name.as_str(), "run_job" | "run_shell")
                    && event.job_id.is_some()
                    && event_observes_validation_activity(event)
                    && event_is_job_acceptance_only(event)
            })
            .cloned()
            .collect::<Vec<_>>();

        for mut observed in accepted {
            let Some(job_id) = observed.job_id.as_deref() else {
                continue;
            };
            let Ok(observation) = self.query_job_status_for_auth(job_id, auth).await else {
                continue;
            };
            let status = observation.job;
            let job_lifecycle = RunnerJobLifecycle::from_wire(&status.status).ok();
            if !job_lifecycle.is_some_and(RunnerJobLifecycle::is_terminal)
                || status.session_id.as_deref() != Some(summary.session_id.as_str())
                || status.project_id.as_deref() != Some(project)
            {
                continue;
            }
            // Structured validation belongs to the durable materialization path.
            if observation.validation.is_some() {
                continue;
            }

            let exit_code = status.exit_code.map(i64::from);
            let succeeded =
                job_lifecycle == Some(RunnerJobLifecycle::Completed) && exit_code == Some(0);
            observed.status = Some(if succeeded { "succeeded" } else { "failed" }.to_string());
            observed.exit_code = exit_code;
            observed.started_at = status.started_at;
            observed.finished_at = status.ended_at;
            if let Some(completed_at) = observed.finished_at {
                observed.timestamp = completed_at;
            }
            observed.duration_ms = status.duration_ms;
            observed.failure_kind = (!succeeded).then(|| match job_lifecycle {
                Some(RunnerJobLifecycle::Timeout | RunnerJobLifecycle::TimedOut) => {
                    "timeout".to_string()
                }
                Some(RunnerJobLifecycle::Stopped | RunnerJobLifecycle::Cancelled) => {
                    "cancelled".to_string()
                }
                Some(RunnerJobLifecycle::Lost) => "execution_lost".to_string(),
                _ => "command_exit_nonzero".to_string(),
            });

            let mut output = self.validation_job_log_output(job_id, auth).await;
            for field in ["purpose", "command_summary", "cwd", "shell", "executor"] {
                if output.get(field).is_none_or(Value::is_null) {
                    output[field] = observed
                        .validation_output_summary
                        .as_ref()
                        .and_then(|value| value.get(field))
                        .cloned()
                        .unwrap_or(Value::Null);
                }
            }
            output["execution_state"] = json!(match job_lifecycle {
                Some(RunnerJobLifecycle::Timeout | RunnerJobLifecycle::TimedOut) => "timed_out",
                Some(RunnerJobLifecycle::Stopped | RunnerJobLifecycle::Cancelled) => "cancelled",
                Some(RunnerJobLifecycle::Lost) => "lost",
                _ => "completed",
            });
            output["exit_code"] = json!(status.exit_code);
            observed.validation_output_summary =
                super::sessions::execution_output_summary_for_tool_result(
                    &observed.tool_name,
                    &output,
                );
            if observed.validation_output_summary.is_some() {
                events.push(observed);
            }
        }
    }

    /// Read the same bounded log tail used for Session evidence, without routing
    /// an internal query through the model-facing Job tool and its projections.
    async fn validation_job_log_output(&self, job_id: &str, auth: Option<&AuthContext>) -> Value {
        match self
            .runner_registry
            .job_log_for_auth(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                job_id,
                None,
                None,
                Some(200),
                None,
                None,
            )
            .await
        {
            Ok((job, stdout, stderr, next_stdout_line, next_stderr_line, observation)) => json!({
                "stdout_tail": stdout.unwrap_or_default(),
                "stderr_tail": stderr.unwrap_or_default(),
                "stdout_lines": next_stdout_line.saturating_sub(1),
                "stderr_lines": next_stderr_line.saturating_sub(1),
                "stdout_truncated": observation.stdout_truncated,
                "stderr_truncated": observation.stderr_truncated,
                "purpose": job.purpose.as_deref().unwrap_or("other"),
                "command_summary": job.command_preview,
                "cwd": job.project_cwd,
                "shell": job.shell,
                "executor": "agent",
            }),
            Err(_) => json!({
                "stdout_tail": "", "stderr_tail": "",
                "stdout_truncated": true, "stderr_truncated": true,
            }),
        }
    }

    async fn materialize_session_validation_job_terminals(
        &self,
        summary: &SessionSummary,
        auth: Option<&AuthContext>,
    ) {
        let Some(project) = summary.project.as_deref() else {
            return;
        };
        self.materialize_validation_job_terminals_for_sessions(
            project,
            std::slice::from_ref(&summary.session_id),
            auth,
        )
        .await;
    }

    pub(crate) async fn materialize_validation_job_terminals_for_sessions(
        &self,
        project: &str,
        session_ids: &[String],
        auth: Option<&AuthContext>,
    ) {
        // Absence from `grouped` is eviction authority for the bounded durable
        // materialization marker set. Hold one runtime-shared ordering fence from
        // authoritative snapshot acquisition through every Session mutation so
        // a snapshot acquired later can never materialize first and then have a
        // still-retained marker removed by an older snapshot. The batch lock is
        // deliberately stronger than a per-Session lock because candidate
        // acquisition itself is batched across Sessions.
        #[cfg(test)]
        self.validation_terminal_reconciliation_test_hook
            .before_reconciliation_lock();
        let _reconciliation_guard = self.validation_terminal_reconciliation.lock().await;
        let mut grouped = self
            .validation_job_candidates_for_sessions(project, session_ids, auth)
            .await;
        #[cfg(test)]
        self.validation_terminal_reconciliation_test_hook
            .after_snapshot_acquired()
            .await;
        for session_id in session_ids {
            let Some(jobs) = grouped.remove(session_id) else {
                continue;
            };
            self.materialize_validation_job_candidates(project, session_id, &jobs, auth)
                .await;
        }
    }

    async fn materialize_validation_job_candidates(
        &self,
        project: &str,
        session_id: &str,
        jobs: &[ShellJobInfo],
        auth: Option<&AuthContext>,
    ) {
        let retained_terminal_job_ids = jobs
            .iter()
            .filter(|job| {
                RunnerJobLifecycle::from_wire(&job.status)
                    .is_ok_and(RunnerJobLifecycle::is_terminal)
            })
            .map(|job| job.job_id.as_str())
            .collect::<Vec<_>>();
        for job in jobs {
            let job_id = job.job_id.as_str();
            if !RunnerJobLifecycle::from_wire(&job.status)
                .is_ok_and(RunnerJobLifecycle::is_terminal)
            {
                continue;
            }
            let Ok(observation) = self.query_job_status_for_auth(job_id, auth).await else {
                continue;
            };
            let status = observation.job;
            let terminal_status = status.status.as_str();
            let terminal_lifecycle = RunnerJobLifecycle::from_wire(terminal_status).ok();
            if !terminal_lifecycle.is_some_and(RunnerJobLifecycle::is_terminal)
                || status.session_id.as_deref() != Some(session_id)
                || status.project_id.as_deref() != Some(project)
            {
                continue;
            }
            let passed = observation
                .validation
                .as_ref()
                .and_then(|validation| validation.passed());
            // Only the persisted diagnostic payload remains JSON. Job identity,
            // lifecycle, correlation and verdict are consumed above as typed facts.
            let validation = observation
                .validation
                .map(|validation| validation.into_value());
            let validation = validation.as_ref().and_then(Value::as_object);
            let structured_execution = job.structured_execution.as_ref();
            let (
                tool_name,
                validation_target_id,
                validation_tool,
                validation_passed,
                assertion_name,
            ) = if let Some(metadata) = status.validation.as_ref() {
                let tool_name = if metadata.project_validation.is_some() {
                    metadata.tool.as_str()
                } else {
                    metadata.adapter.as_str()
                };
                if tool_name != "project_validate"
                    && validation_evidence_profile_for_tool(tool_name).is_none()
                {
                    continue;
                }
                // Portable validation retains its wrapper tool identity; the
                // verified Job metadata supplies its read-only adapter separately.
                let adapter = if tool_name == "project_validate" {
                    Some(metadata.adapter.as_str())
                } else {
                    Some(tool_name)
                };
                let Some(adapter) = adapter
                    .filter(|adapter| validation_evidence_profile_for_tool(adapter).is_some())
                else {
                    continue;
                };
                let Some(identity) = metadata
                    .validation_target_id
                    .as_deref()
                    .filter(|value| is_structured_validation_target_identity(value))
                else {
                    continue;
                };
                (tool_name, identity, Some(adapter), passed, None)
            } else {
                let Some(metadata) = structured_execution else {
                    continue;
                };
                let source = metadata.execution_source.as_str();
                if !matches!(source, "run_process" | "run_script") {
                    continue;
                };
                let Some(identity) = metadata
                    .validation_identity
                    .as_deref()
                    .filter(|identity| is_validation_execution_identity(identity))
                else {
                    continue;
                };
                let validation_tool = metadata
                    .validation_tool
                    .as_deref()
                    .filter(|tool| validation_evidence_profile_for_tool(tool).is_some());
                let assertion_name = metadata
                    .assertion_name
                    .as_deref()
                    .and_then(|value| safe_model_facing_assertion_name(source, value))
                    .filter(|value| assertion_validation_identity(value) == identity);
                (source, identity, validation_tool, None, assertion_name)
            };
            let mut output = self.validation_job_log_output(job_id, auth).await;
            if let Some(validation) = validation {
                for field in [
                    "adapter",
                    "backend",
                    "action",
                    "passed",
                    "warnings_count",
                    "errors_count",
                    "tests_detected",
                    "tests_run_count",
                    "tests_passed",
                    "tests_failed",
                    "zero_tests_run",
                    "require_tests",
                    "no_run",
                    "test_count_assertion",
                    "diagnostics",
                    "source_state",
                ] {
                    // A structured terminal count can be independently proven
                    // even when bounded-log component diagnostics are absent.
                    // Do not persist JSON nulls as explicit contradictory test
                    // metadata; absence remains fail-closed in the ledger while
                    // a non-null authoritative tests_run_count stays usable.
                    if let Some(value) = validation.get(field).filter(|value| !value.is_null()) {
                        output[field] = value.clone();
                    }
                }
            } else {
                let detected = super::jobs::detected_job_summary(
                    Some(job.command_preview.as_str()),
                    job.purpose.as_deref(),
                    terminal_status,
                    status.exit_code.map(i64::from),
                    output
                        .get("stdout_tail")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    output
                        .get("stderr_tail")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                for field in [
                    "tests_detected",
                    "tests_run_count",
                    "tests_passed",
                    "tests_failed",
                    "zero_tests_run",
                ] {
                    if let Some(value) = detected.get(field) {
                        output[field] = value.clone();
                    }
                }
            }
            for (field, value) in [
                ("purpose", json!(job.purpose)),
                ("command_summary", json!(job.command_preview)),
                ("cwd", json!(job.project_cwd)),
                ("shell", json!(job.shell)),
                ("executor", json!("agent")),
            ] {
                if output.get(field).is_none_or(Value::is_null) {
                    output[field] = value;
                }
            }
            if output.get("purpose").is_none_or(Value::is_null) {
                output["purpose"] = json!("validation");
            }
            if let Some(validation_tool) = validation_tool {
                output["validation_tool"] = json!(validation_tool);
            }
            output["execution_state"] = json!(match terminal_lifecycle {
                Some(RunnerJobLifecycle::Timeout | RunnerJobLifecycle::TimedOut) => "timed_out",
                Some(RunnerJobLifecycle::Stopped | RunnerJobLifecycle::Cancelled) => "cancelled",
                Some(RunnerJobLifecycle::Lost) => "lost",
                _ => "completed",
            });
            output["exit_code"] = json!(status.exit_code);
            let validation_output_summary =
                super::sessions::execution_output_summary_for_tool_result(tool_name, &output);
            self.sessions.record_validation_job_terminal(
                session_id,
                job_id,
                &retained_terminal_job_ids,
                tool_name,
                super::sessions::session_tool_contract(tool_name),
                Some(project.to_string()),
                validation_target_id,
                assertion_name.as_deref(),
                terminal_status,
                status.exit_code.map(i64::from),
                validation_passed,
                status.started_at,
                status.ended_at,
                status.duration_ms,
                validation_output_summary,
            );
        }
    }
}

fn remove_public_validation_input_summaries(validation: &mut Value) {
    for field in ["latest", "latest_success", "latest_failure"] {
        if let Some(event) = validation.get_mut(field).and_then(Value::as_object_mut) {
            event.remove("input_summary");
        }
    }
    if let Some(events) = validation.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            if let Some(event) = event.as_object_mut() {
                event.remove("input_summary");
            }
        }
    }
}
