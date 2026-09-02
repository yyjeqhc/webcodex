//! Ledger-derived validation event summaries.
//!
//! This module deliberately records facts already present in the session
//! ledger. It does not expose stdout/stderr, infer root causes, or change tool
//! execution behavior. Diagnostics are parsed only from safe bounded validation
//! output metadata captured by session events.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use super::session_context::{
    session_project_mismatch_result, unknown_session_result, SessionProjectMismatch,
};
use super::sessions::{
    canonical_tool_call_finished_events, current_attempt_event_view,
    safe_model_facing_assertion_name, tool_supports_model_facing_assertion_name, SessionEvent,
    SessionSummary,
};
use super::tool_audit::{
    assertion_validation_identity, is_structured_validation_target_identity,
    is_validation_execution_identity, structured_validation_target_identity,
};
use super::validation_parser::{
    ValidationDiagnostics, PARSER_KIND, PARSER_LIMITATIONS, PARSER_VERSION,
    VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
};
use super::validation_profile::{
    validation_adapter_for_tool, ValidationAdapter, ValidationFailureEvidence,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;

#[cfg(test)]
const DEFAULT_VALIDATION_EVENT_LIMIT: usize = 10;
const VALIDATION_SOURCE: &str = "session_ledger";
const VALIDATION_PARSER_SOURCE: &str = "bounded_validation_metadata";
const DEFAULT_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 20;
const MAX_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 100;
const PUBLIC_VALIDATION_SESSION_EVENT_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationEvent {
    pub(crate) tool_name: String,
    pub(crate) execution_source: String,
    pub(crate) identity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) assertion_name: Option<String>,
    pub(crate) purpose: String,
    pub(crate) validation_kind: String,
    pub(crate) success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) execution_success: Option<bool>,
    pub(crate) failure_kind: &'static str,
    pub(crate) failure_category: &'static str,
    pub(crate) unresolved_failure: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i64>,
    pub(crate) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command_summary: Option<String>,
    pub(crate) cwd: String,
    pub(crate) shell: String,
    pub(crate) execution_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    pub(crate) session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) affected_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<ValidationDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detected_summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_detected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_run_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_passed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_failed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zero_tests_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) test_count_assertion: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stdout_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stderr_lines: Option<u64>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stdout_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stderr_evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationParserSummary {
    available: bool,
    kind: &'static str,
    version: u8,
    source: &'static str,
    raw_output_exposed: bool,
    limitations: [&'static str; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationSummary {
    available: bool,
    status: &'static str,
    reason: Option<&'static str>,
    latest: Option<ValidationEvent>,
    latest_status: &'static str,
    historical_failures: ValidationHistoricalFailures,
    resolved_failures: ValidationFailureSet,
    unresolved_failures: ValidationFailureSet,
    source: &'static str,
    events_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    successes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failures: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_success: Option<ValidationEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_failure: Option<ValidationEvent>,
    events: Vec<ValidationEvent>,
    parser: ValidationParserSummary,
    cargo_test_zero_tests_run: bool,
    #[serde(skip_serializing_if = "is_false")]
    skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationHistoricalFailures {
    count: usize,
    resolved: bool,
    unresolved: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationFailureSet {
    count: usize,
    events: Vec<ValidationEvent>,
}

#[derive(Debug, Clone)]
struct ExtractedValidationEvent {
    source_event_id: String,
    event: ValidationEvent,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentValidationEvidenceProjection {
    pub(crate) evidence: Value,
    pub(crate) current_validation: Value,
    pub(crate) non_current_failure_event_ids: HashSet<String>,
}

#[cfg(test)]
pub(crate) fn validation_summary_for_session(summary: &SessionSummary) -> Value {
    validation_summary_for_session_events(summary, &summary.events, DEFAULT_VALIDATION_EVENT_LIMIT)
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
            .authorize_session_target(&session_id, "validation_summary", auth)
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
            return unknown_session_result(&session_id);
        };
        if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
            let mismatch = SessionProjectMismatch {
                session_project: summary.project.unwrap_or_else(|| "<unscoped>".to_string()),
                request_project: resolved.resolved_id,
            };
            return session_project_mismatch_result(&session_id, "validation_summary", &mismatch);
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
        events.sort_by_key(|event| {
            (
                event.timestamp,
                event.finished_at.unwrap_or(event.timestamp),
            )
        });
        validation_summary_for_session_events(&refreshed, &events, limit)
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
                    && execution_purpose(event).is_some()
                    && job_acceptance_only(event)
            })
            .cloned()
            .collect::<Vec<_>>();

        for mut observed in accepted {
            let Some(job_id) = observed.job_id.as_deref() else {
                continue;
            };
            let status = self
                .job_status_for_auth(job_id.to_string(), false, auth)
                .await;
            if !status.success
                || status.output.get("terminal").and_then(Value::as_bool) != Some(true)
                || status.output.get("session_id").and_then(Value::as_str)
                    != Some(summary.session_id.as_str())
                || status.output.get("project").and_then(Value::as_str) != Some(project)
            {
                continue;
            }
            // A generic execution carrying structured validation metadata belongs to the durable
            // materialization path and must not be synthesized a second time here.
            if status
                .output
                .get("validation")
                .is_some_and(Value::is_object)
            {
                continue;
            }

            let log = self
                .job_log_for_auth(job_id.to_string(), None, Some(200), auth, None, None)
                .await;
            let job_status = status
                .output
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let exit_code = status.output.get("exit_code").and_then(Value::as_i64);
            let succeeded = job_status == "completed" && exit_code == Some(0);
            observed.status = Some(if succeeded { "succeeded" } else { "failed" }.to_string());
            observed.exit_code = exit_code;
            observed.started_at = status.output.get("started_at").and_then(Value::as_i64);
            observed.finished_at = status.output.get("ended_at").and_then(Value::as_i64);
            if let Some(completed_at) = observed.finished_at {
                observed.timestamp = completed_at;
            }
            observed.duration_ms = status.output.get("duration_ms").and_then(Value::as_u64);
            observed.failure_kind = (!succeeded).then(|| match job_status {
                "timeout" | "timed_out" => "timeout".to_string(),
                "stopped" | "cancelled" => "cancelled".to_string(),
                "lost" => "execution_lost".to_string(),
                _ => "command_exit_nonzero".to_string(),
            });

            let mut output = if log.success {
                log.output
            } else {
                json!({
                    "stdout_tail": "",
                    "stderr_tail": "",
                    "stdout_truncated": true,
                    "stderr_truncated": true,
                })
            };
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
            output["execution_state"] = json!(match job_status {
                "timeout" | "timed_out" => "timed_out",
                "stopped" | "cancelled" => "cancelled",
                "lost" => "lost",
                _ => "completed",
            });
            output["exit_code"] = status
                .output
                .get("exit_code")
                .cloned()
                .unwrap_or(Value::Null);
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
        jobs: &[Value],
        auth: Option<&AuthContext>,
    ) {
        let retained_terminal_job_ids = jobs
            .iter()
            .filter(|job| {
                matches!(
                    job.get("status").and_then(Value::as_str),
                    Some(
                        "completed"
                            | "failed"
                            | "timeout"
                            | "timed_out"
                            | "stopped"
                            | "cancelled"
                            | "lost"
                    )
                )
            })
            .filter_map(|job| job.get("job_id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        for job in jobs {
            let Some(job_id) = job.get("job_id").and_then(Value::as_str) else {
                continue;
            };
            let status_name = job
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(
                status_name,
                "completed" | "failed" | "timeout" | "timed_out" | "stopped" | "cancelled" | "lost"
            ) {
                continue;
            }
            let status = self
                .job_status_for_auth(job_id.to_string(), false, auth)
                .await;
            if !status.success
                || status.output.get("terminal").and_then(Value::as_bool) != Some(true)
                || status.output.get("session_id").and_then(Value::as_str) != Some(session_id)
                || status.output.get("project").and_then(Value::as_str) != Some(project)
            {
                continue;
            }
            let validation = status.output.get("validation").and_then(Value::as_object);
            let structured_execution = job.get("structured_execution").and_then(Value::as_object);
            let terminal_status = status
                .output
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(status_name);
            let (
                tool_name,
                validation_target_id,
                validation_tool,
                validation_passed,
                assertion_name,
            ) = if let Some(validation) = validation {
                let Some(tool_name) = validation
                    .get("tool")
                    .and_then(Value::as_str)
                    .filter(|tool| validation_adapter_for_tool(tool).is_some())
                else {
                    continue;
                };
                let Some(identity) = validation
                    .get("validation_target_id")
                    .and_then(Value::as_str)
                    .filter(|value| is_structured_validation_target_identity(value))
                else {
                    continue;
                };
                (
                    tool_name,
                    identity,
                    Some(tool_name),
                    validation.get("passed").and_then(Value::as_bool),
                    None,
                )
            } else {
                let Some(metadata) = structured_execution else {
                    continue;
                };
                let Some(source) = metadata
                    .get("execution_source")
                    .and_then(Value::as_str)
                    .filter(|source| matches!(*source, "run_process" | "run_script"))
                else {
                    continue;
                };
                let Some(identity) = metadata
                    .get("validation_identity")
                    .and_then(Value::as_str)
                    .filter(|identity| is_validation_execution_identity(identity))
                else {
                    continue;
                };
                let validation_tool = metadata
                    .get("validation_tool")
                    .and_then(Value::as_str)
                    .filter(|tool| validation_adapter_for_tool(tool).is_some());
                let assertion_name = metadata
                    .get("assertion_name")
                    .and_then(Value::as_str)
                    .and_then(|value| safe_model_facing_assertion_name(source, value))
                    .filter(|value| assertion_validation_identity(value) == identity);
                (source, identity, validation_tool, None, assertion_name)
            };
            let log = self
                .job_log_for_auth(job_id.to_string(), None, Some(200), auth, None, None)
                .await;
            let mut output = if log.success {
                log.output
            } else {
                json!({
                    "stdout_tail": "",
                    "stderr_tail": "",
                    "stdout_truncated": true,
                    "stderr_truncated": true,
                })
            };
            if let Some(validation) = validation {
                for field in [
                    "passed",
                    "warnings_count",
                    "errors_count",
                    "tests_detected",
                    "tests_run_count",
                    "tests_passed",
                    "tests_failed",
                    "zero_tests_run",
                    "test_count_assertion",
                    "diagnostics",
                ] {
                    if let Some(value) = validation.get(field) {
                        output[field] = value.clone();
                    }
                }
            } else {
                let detected = super::jobs::detected_job_summary(
                    job.get("command_summary").and_then(Value::as_str),
                    job.get("purpose").and_then(Value::as_str),
                    terminal_status,
                    status.output.get("exit_code").and_then(Value::as_i64),
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
            for field in ["purpose", "command_summary", "cwd", "shell", "executor"] {
                if output.get(field).is_none_or(Value::is_null) {
                    output[field] = job.get(field).cloned().unwrap_or(Value::Null);
                }
            }
            if output.get("purpose").is_none_or(Value::is_null) {
                output["purpose"] = json!("validation");
            }
            if let Some(validation_tool) = validation_tool {
                output["validation_tool"] = json!(validation_tool);
            }
            output["execution_state"] = json!(match terminal_status {
                "timeout" | "timed_out" => "timed_out",
                "stopped" | "cancelled" => "cancelled",
                "lost" => "lost",
                _ => "completed",
            });
            output["exit_code"] = status
                .output
                .get("exit_code")
                .cloned()
                .unwrap_or(Value::Null);
            let validation_output_summary =
                super::sessions::execution_output_summary_for_tool_result(tool_name, &output);
            self.sessions.record_validation_job_terminal(
                session_id,
                job_id,
                &retained_terminal_job_ids,
                tool_name,
                Some(project.to_string()),
                validation_target_id,
                assertion_name.as_deref(),
                terminal_status,
                status.output.get("exit_code").and_then(Value::as_i64),
                validation_passed,
                status.output.get("started_at").and_then(Value::as_i64),
                status.output.get("ended_at").and_then(Value::as_i64),
                status.output.get("duration_ms").and_then(Value::as_u64),
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

pub(crate) fn skipped_validation_summary() -> Value {
    let mut validation = to_value(ValidationSummary {
        available: false,
        status: "unknown",
        reason: Some("validation_summary_not_requested"),
        latest: None,
        latest_status: "unknown",
        historical_failures: no_historical_failures(),
        resolved_failures: no_failures(),
        unresolved_failures: no_failures(),
        source: VALIDATION_SOURCE,
        events_total: 0,
        successes: None,
        failures: None,
        latest_success: None,
        latest_failure: None,
        events: Vec::new(),
        parser: parser_unavailable(),
        cargo_test_zero_tests_run: false,
        skipped: true,
    });
    validation["current_evidence"] = json!({
        "status": "unknown",
        "reason": "validation_summary_not_requested",
        "latest_status": "unknown",
        "events_total": 0,
        "successes": 0,
        "failures": 0,
        "resolved_failure_count": 0,
        "unresolved_failure_count": 0,
        "stale_failure_count": 0,
        "evidence_after_latest_content_change": false,
        "boundary_reason": "attempt_boundary_unavailable",
    });
    validation
}

fn validation_summary_for_session_events(
    summary: &SessionSummary,
    events: &[SessionEvent],
    limit: usize,
) -> Value {
    let mut historical = validation_summary_from_events(events, limit);
    let current = current_validation_evidence_for_events(summary, events, limit);
    historical["current_evidence"] = current.evidence;
    historical
}

pub(crate) fn current_validation_evidence_for_session(
    summary: &SessionSummary,
    limit: usize,
) -> CurrentValidationEvidenceProjection {
    current_validation_evidence_for_events(summary, &summary.events, limit)
}

fn current_validation_evidence_for_events(
    summary: &SessionSummary,
    events: &[SessionEvent],
    limit: usize,
) -> CurrentValidationEvidenceProjection {
    let attempt = current_attempt_event_view(summary);
    if !attempt.complete {
        return CurrentValidationEvidenceProjection {
            evidence: json!({
                "status": "unknown",
                "reason": "attempt_boundary_unavailable",
                "latest_status": "unknown",
                "events_total": 0,
                "successes": 0,
                "failures": 0,
                "resolved_failure_count": 0,
                "unresolved_failure_count": 0,
                "stale_failure_count": 0,
                "evidence_after_latest_content_change": false,
                "boundary_reason": "attempt_boundary_unavailable",
            }),
            current_validation: validation_summary_from_events(&[], limit),
            non_current_failure_event_ids: HashSet::new(),
        };
    }

    // Attempt and mutation boundaries are durable ledger-order fences. A
    // validation can prove the current workspace only when its exact execution
    // start is after the effective fence; completing after the fence is not
    // sufficient because the execution may have overlapped a content change.
    let canonical_finished_ids = canonical_tool_call_finished_events(&summary.events)
        .into_iter()
        .map(|event| event.event_id.as_str())
        .collect::<HashSet<_>>();
    let reset_index = summary
        .events
        .iter()
        .enumerate()
        .skip(attempt.attempt_start)
        .filter(|(_, event)| {
            event.kind != "tool_call_finished"
                || canonical_finished_ids.contains(event.event_id.as_str())
        })
        .filter(|(_, event)| material_workspace_content_change(event))
        .map(|(index, _)| index)
        .last();
    let effective_boundary_index = reset_index.or(attempt.boundary_event_index);

    let validation_records = extract_validation_event_records(events);
    let mut current_source_event_ids = HashSet::new();
    let mut current_failure_ids = HashSet::new();
    let mut stale_failure_count = 0usize;
    let mut stale_validation_count = 0usize;

    for record in &validation_records {
        let start_index = authoritative_validation_start_event_index(&summary.events, record);
        let started_in_attempt = start_index.is_some_and(|index| index >= attempt.attempt_start);
        let started_after_boundary = start_index.is_some_and(|index| {
            started_in_attempt && effective_boundary_index.is_none_or(|boundary| index > boundary)
        });
        if started_after_boundary {
            current_source_event_ids.insert(record.source_event_id.clone());
            if !record.event.success {
                current_failure_ids.insert(record.source_event_id.clone());
            }
        } else if reset_index.is_some_and(|boundary| {
            start_index.is_some_and(|index| index >= attempt.attempt_start && index <= boundary)
        }) {
            stale_validation_count += 1;
            if !record.event.success {
                stale_failure_count += 1;
            }
        }
    }

    // Keep all starts so exact call_id correlation and validation identity
    // extraction remain unchanged, but admit only validation outcomes whose
    // source execution was proven to start after the effective boundary.
    let current_events = events
        .iter()
        .filter(|event| {
            event.kind == "tool_call_started"
                || current_source_event_ids.contains(event.event_id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let current_validation = validation_summary_from_events(&current_events, limit);
    let current_events_total = current_validation
        .get("events_total")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let successes = current_validation
        .get("successes")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let failures = current_validation
        .get("failures")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let resolved_failure_count = current_validation
        .pointer("/resolved_failures/count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let unresolved_failure_count = current_validation
        .pointer("/unresolved_failures/count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let (status, reason) = if current_events_total > 0 && unresolved_failure_count > 0 {
        ("failed", Some("current_validation_failures"))
    } else if current_events_total > 0 && successes > 0 {
        ("passed", None)
    } else if reset_index.is_some() && stale_validation_count > 0 {
        ("stale", Some("validation_stale_after_changes"))
    } else if current_events_total == 0 {
        ("not_run", Some("no_validation_in_current_attempt"))
    } else {
        ("unknown", Some("current_validation_evidence_unknown"))
    };
    let latest_status = current_validation
        .get("latest_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let boundary_reason = if reset_index.is_some() {
        "workspace_content_changed"
    } else {
        "attempt_start"
    };

    let non_current_failure_event_ids = validation_records
        .into_iter()
        .filter(|record| !record.event.success)
        .map(|record| record.source_event_id)
        .filter(|event_id| !current_failure_ids.contains(event_id))
        .collect();

    CurrentValidationEvidenceProjection {
        evidence: json!({
            "status": status,
            "reason": reason,
            "latest_status": latest_status,
            "events_total": current_events_total,
            "successes": successes,
            "failures": failures,
            "resolved_failure_count": resolved_failure_count,
            "unresolved_failure_count": unresolved_failure_count,
            "stale_failure_count": stale_failure_count,
            "evidence_after_latest_content_change": reset_index.is_some() && current_events_total > 0,
            "boundary_reason": boundary_reason,
        }),
        current_validation,
        non_current_failure_event_ids,
    }
}

fn authoritative_validation_start_event_index(
    ledger_events: &[SessionEvent],
    record: &ExtractedValidationEvent,
) -> Option<usize> {
    let (source_index, source) = ledger_events
        .iter()
        .enumerate()
        .find(|(_, event)| event.event_id == record.source_event_id)?;
    match source.kind.as_str() {
        "tool_call_finished" => exact_tool_start_event_index(ledger_events, source_index, source),
        "validation_job_terminal" => {
            let job_id = source.job_id.as_deref()?;
            let (acceptance_index, acceptance) = ledger_events[..source_index]
                .iter()
                .enumerate()
                .rev()
                .find(|(_, event)| {
                    event.kind == "tool_call_finished"
                        && event.job_id.as_deref() == Some(job_id)
                        && job_acceptance_only(event)
                })?;
            exact_tool_start_event_index(ledger_events, acceptance_index, acceptance)
        }
        _ => None,
    }
}

fn exact_tool_start_event_index(
    ledger_events: &[SessionEvent],
    finish_index: usize,
    finished: &SessionEvent,
) -> Option<usize> {
    let call_id = finished.call_id.as_deref()?;
    ledger_events[..finish_index]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, event)| {
            event.kind == "tool_call_started"
                && event.call_id.as_deref() == Some(call_id)
                && event.session_id == finished.session_id
                && event.tool_name == finished.tool_name
        })
        .map(|(index, _)| index)
}

fn material_workspace_content_change(event: &SessionEvent) -> bool {
    event.kind == "tool_call_finished"
        && event
            .effect_evidence
            .as_ref()
            .and_then(|evidence| evidence.state_changed)
            == Some(true)
        && !event.changed_paths.is_empty()
}

pub(crate) fn validation_summary_from_events(events: &[SessionEvent], limit: usize) -> Value {
    let validation_events = extract_validation_events(events);
    let events_total = validation_events.len();
    if events_total == 0 {
        return to_value(ValidationSummary {
            available: false,
            status: "not_run",
            reason: Some("no_validation_tool_invoked"),
            latest: None,
            latest_status: "not_run",
            historical_failures: no_historical_failures(),
            resolved_failures: no_failures(),
            unresolved_failures: no_failures(),
            source: VALIDATION_SOURCE,
            events_total,
            successes: None,
            failures: None,
            latest_success: None,
            latest_failure: None,
            events: Vec::new(),
            parser: parser_unavailable(),
            cargo_test_zero_tests_run: false,
            skipped: false,
        });
    }

    let mut validation_events = validation_events;
    let (historical_failures, resolved_failures, unresolved_failures) =
        classify_validation_failures(&mut validation_events);
    let successes = validation_events
        .iter()
        .filter(|event| event.success)
        .count();
    let failures = events_total.saturating_sub(successes);
    let status = validation_status(successes, failures);
    let parser = parser_summary_for_events(&validation_events);
    let cargo_test_zero_tests_run = validation_events.iter().any(cargo_test_zero_tests_success);
    let latest = validation_events.last().cloned();
    let latest_status = validation_latest_status(latest.as_ref());
    let latest_success = validation_events
        .iter()
        .rev()
        .find(|event| event.success)
        .cloned();
    let latest_failure = validation_events
        .iter()
        .rev()
        .find(|event| !event.success)
        .cloned();
    let skip = events_total.saturating_sub(limit);
    let events = validation_events.into_iter().skip(skip).collect();

    to_value(ValidationSummary {
        available: true,
        status,
        reason: None,
        latest,
        latest_status,
        historical_failures,
        resolved_failures,
        unresolved_failures,
        source: VALIDATION_SOURCE,
        events_total,
        successes: Some(successes),
        failures: Some(failures),
        latest_success,
        latest_failure,
        events,
        parser,
        cargo_test_zero_tests_run,
        skipped: false,
    })
}

fn validation_status(successes: usize, failures: usize) -> &'static str {
    match (successes > 0, failures > 0) {
        (true, true) => "mixed",
        (true, false) => "passed",
        (false, true) => "failed",
        (false, false) => "unknown",
    }
}

fn validation_latest_status(latest: Option<&ValidationEvent>) -> &'static str {
    match latest {
        Some(event) if event.success => "passed",
        Some(_) => "failed",
        None => "not_run",
    }
}

fn classify_validation_failures(
    events: &mut [ValidationEvent],
) -> (
    ValidationHistoricalFailures,
    ValidationFailureSet,
    ValidationFailureSet,
) {
    // Identity is meaningful only inside the exact resolved project. Workflow
    // Sessions can explicitly record cross-project tool calls, so a successful
    // validation in project B must never resolve a same-shaped failure from
    // project A merely because cwd/package/filter/features match.
    let mut latest_success_by_identity = HashMap::<(Option<String>, String, String), usize>::new();
    for (index, event) in events.iter().enumerate() {
        if event.success && validation_event_decides_historical_failure_status(event) {
            latest_success_by_identity.insert(validation_reconciliation_key(event), index);
        }
    }
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for (index, event) in events.iter_mut().enumerate() {
        if event.success {
            continue;
        }
        let is_resolved = latest_success_by_identity
            .get(&validation_reconciliation_key(event))
            .is_some_and(|success_index| *success_index > index);
        event.unresolved_failure = !is_resolved;
        if is_resolved {
            resolved.push(event.clone());
        } else {
            unresolved.push(event.clone());
        }
    }
    let failures = resolved.len() + unresolved.len();
    (
        ValidationHistoricalFailures {
            count: failures,
            resolved: !resolved.is_empty() && unresolved.is_empty(),
            unresolved: !unresolved.is_empty(),
        },
        ValidationFailureSet {
            count: resolved.len(),
            events: resolved,
        },
        ValidationFailureSet {
            count: unresolved.len(),
            events: unresolved,
        },
    )
}

fn validation_reconciliation_key(event: &ValidationEvent) -> (Option<String>, String, String) {
    let assertion_domain = if event.identity.starts_with("assertion:") {
        if tool_supports_model_facing_assertion_name(&event.tool_name) {
            // P1 intentionally keeps one assertion namespace across the four
            // public generic execution tools so a logical check can move between
            // run_process/run_script/run_shell/run_job after a fix.
            "generic_execution".to_string()
        } else {
            // Hidden recorder assertion metadata predates the public P1 contract.
            // Preserve same-tool internal reconciliation without allowing a
            // generic model-facing assertion (or another structured tool) to
            // become proof for this tool's validation failure.
            format!("tool:{}", event.tool_name)
        }
    } else {
        // Non-assertion identities already carry their existing command/target
        // domain and keep the exact pre-P1 reconciliation semantics.
        String::new()
    };
    (
        event.project.clone(),
        event.identity.clone(),
        assertion_domain,
    )
}

fn validation_event_decides_historical_failure_status(event: &ValidationEvent) -> bool {
    !cargo_test_zero_tests_success(event) && !cargo_test_unproven_execution_success(event)
}

// `validation_kind = test` is only an intent/category for generic execution.
// Only an actual first-class structured test tool opts into count/zero-test proof.
fn structured_test_requires_execution_proof(event: &ValidationEvent) -> bool {
    validation_adapter_for_tool(&event.tool_name).is_some_and(|adapter| {
        adapter.validation_kind() == "test" && adapter.reports_test_run_metadata()
    })
}

fn cargo_test_unproven_execution_success(event: &ValidationEvent) -> bool {
    structured_test_requires_execution_proof(event)
        && event.success
        && (event.tests_run_count.is_none() || event.zero_tests_run.is_none())
}

fn no_historical_failures() -> ValidationHistoricalFailures {
    ValidationHistoricalFailures {
        count: 0,
        resolved: false,
        unresolved: false,
    }
}

fn no_failures() -> ValidationFailureSet {
    ValidationFailureSet {
        count: 0,
        events: Vec::new(),
    }
}

pub(crate) fn extract_validation_events(events: &[SessionEvent]) -> Vec<ValidationEvent> {
    extract_validation_event_records(events)
        .into_iter()
        .map(|record| record.event)
        .collect()
}

fn extract_validation_event_records(events: &[SessionEvent]) -> Vec<ExtractedValidationEvent> {
    let mut started = Vec::new();
    let mut validation_events = Vec::new();
    let mut terminal_jobs = HashSet::new();
    let canonical_finished_ids = canonical_tool_call_finished_events(events)
        .into_iter()
        .map(|event| event.event_id.as_str())
        .collect::<HashSet<_>>();

    for event in events {
        match event.kind.as_str() {
            "tool_call_started" if execution_purpose(event).is_some() => {
                started.push(event.clone());
            }
            "tool_call_finished" => {
                if !canonical_finished_ids.contains(event.event_id.as_str()) {
                    continue;
                }
                // A `run_job` acceptance or a promoted structured validation
                // handoff (still queued/running) is not a terminal validation
                // outcome. The Job's terminal status feeds the summary
                // separately through `validation_summary_for_session_with_jobs`.
                if job_acceptance_only(event) {
                    continue;
                }
                let start = matching_start(&mut started, event);
                if let Some(validation_event) =
                    validation_event_from_finished(event, start.as_ref())
                {
                    validation_events.push(ExtractedValidationEvent {
                        source_event_id: event.event_id.clone(),
                        event: validation_event,
                    });
                }
            }
            "validation_job_terminal" => {
                let Some(job_id) = event.job_id.as_deref() else {
                    continue;
                };
                if !terminal_jobs.insert(job_id) {
                    continue;
                }
                if let Some(validation_event) = validation_event_from_finished(event, Some(event)) {
                    validation_events.push(ExtractedValidationEvent {
                        source_event_id: event.event_id.clone(),
                        event: validation_event,
                    });
                }
            }
            _ => {}
        }
    }

    validation_events
}

/// True for a finished tool event that merely accepted a Job (or promoted a
/// validation to a Job) without a terminal result. Such events carry a
/// `job_id` and a non-terminal `execution_state`, and never contribute a
/// pass/fail verdict by themselves.
fn job_acceptance_only(event: &SessionEvent) -> bool {
    if event.exit_code.is_some() {
        return false;
    }
    if event.job_id.as_deref().is_none_or(str::is_empty) {
        return false;
    }
    let execution_state = event
        .validation_output_summary
        .as_ref()
        .and_then(|summary| summary.get("execution_state"))
        .and_then(Value::as_str);
    matches!(
        execution_state,
        Some("started") | Some("queued") | Some("running")
    )
}

pub(crate) fn validation_kind_for_tool(tool_name: &str) -> Option<&'static str> {
    if let Some(adapter) = validation_adapter_for_tool(tool_name) {
        return Some(adapter.validation_kind());
    }
    None
}

fn execution_purpose(event: &SessionEvent) -> Option<String> {
    if let Some(kind) = validation_kind_for_tool(&event.tool_name) {
        return Some(
            match kind {
                "test" => "test",
                "check" => "validation",
                "format" => "format",
                _ => "validation",
            }
            .to_string(),
        );
    }
    if !matches!(
        event.tool_name.as_str(),
        "run_process" | "run_script" | "run_shell" | "run_job"
    ) {
        return None;
    }
    let purpose = event
        .input_summary
        .as_ref()
        .and_then(|input| input.get("purpose"))
        .and_then(Value::as_str)
        .or_else(|| {
            event
                .validation_output_summary
                .as_ref()
                .and_then(|summary| summary.get("purpose"))
                .and_then(Value::as_str)
        })?;
    matches!(
        purpose,
        "validation" | "test" | "build" | "format" | "release"
    )
    .then(|| purpose.to_string())
}

pub(crate) fn event_observes_validation_activity(event: &SessionEvent) -> bool {
    execution_purpose(event).is_some()
}

fn validation_event_from_finished(
    finished: &SessionEvent,
    started: Option<&SessionEvent>,
) -> Option<ValidationEvent> {
    let adapter = validation_adapter_for_execution(finished, started);
    let purpose = started
        .and_then(execution_purpose)
        .or_else(|| execution_purpose(finished))?;
    let validation_kind = adapter
        .map(|adapter| adapter.validation_kind().to_string())
        .or_else(|| validation_kind_for_tool(&finished.tool_name).map(str::to_string))
        .unwrap_or_else(|| purpose.clone());
    let execution_success = match finished.status.as_deref() {
        Some("succeeded") => true,
        Some("failed") => false,
        _ => return None,
    };
    // Pre-execution rejections (invalid arguments, offline agent, scope/guard
    // denial, schema reject) must not become validation evidence. Only calls
    // that actually entered validation execution — exit code and/or bounded
    // validation output metadata — feed the ledger summary.
    if !validation_execution_started(finished)
        && (!execution_success || finished.tool_name == "run_job")
    {
        return None;
    }
    let started_at = finished
        .started_at
        .or_else(|| started.and_then(|event| event.started_at));
    let completed_at = finished.finished_at;
    let project = finished
        .resolved_project
        .clone()
        .or_else(|| finished.project.clone())
        .or_else(|| started.and_then(|event| event.resolved_project.clone()))
        .or_else(|| started.and_then(|event| event.project.clone()));
    let affected_paths = if finished.changed_paths.is_empty() {
        started
            .map(|event| event.changed_paths.clone())
            .unwrap_or_default()
    } else {
        finished.changed_paths.clone()
    };
    let diagnostics =
        adapter.and_then(|adapter| validation_diagnostics_from_summary(finished, adapter));
    let failure_kind =
        validation_failure_kind(finished, execution_success, diagnostics.as_ref(), adapter);
    let public_result_expectation =
        finished.result_expectation.is_some() || !finished.accepted_exit_codes.is_empty();
    let expectation_matched = !execution_success
        && public_result_expectation
        && matches!(
            finished.failure_expectation_result.as_deref(),
            Some("matched_expected_failure" | "matched_expected_result")
        );
    // Validation success describes whether the declared validation assertion
    // was satisfied, not whether the underlying process exited zero. The raw
    // process outcome remains available as execution_success/exit_code.
    let success = execution_success || expectation_matched;
    let command_summary = execution_string(started, finished, "command_summary").or_else(|| {
        started
            .and_then(|event| event.input_summary.as_ref())
            .and_then(|input| input.get("command"))
            .and_then(Value::as_str)
            .map(first_line_summary)
    });
    let cwd = execution_string(started, finished, "cwd").unwrap_or_else(|| ".".to_string());
    let shell =
        execution_string(started, finished, "shell").unwrap_or_else(|| "configured".to_string());
    let execution_state = execution_string(started, finished, "execution_state")
        .unwrap_or_else(|| "completed".to_string());
    let identity = execution_identity(
        started,
        finished,
        &purpose,
        command_summary.as_deref(),
        &finished.tool_name,
    );
    let assertion_name = public_validation_assertion_name(started, finished, &identity);
    let (
        stdout_evidence,
        stderr_evidence,
        stdout_truncated,
        stderr_truncated,
        stdout_lines,
        stderr_lines,
    ) = execution_output_evidence(finished);
    let (tests_detected, tests_run_count, tests_passed, tests_failed, zero_tests_run) =
        validation_test_run_metadata(finished, adapter, diagnostics.as_ref());
    let test_count_assertion = finished
        .validation_output_summary
        .as_ref()
        .and_then(|summary| summary.get("test_count_assertion"))
        .cloned();
    let detected_summary = adapter.map(|adapter| {
        json!({
            "kind": adapter.validation_kind(),
            "parser_available": diagnostics.as_ref().is_some_and(|value| value.available),
            "tests_detected": tests_detected,
            "tests_run_count": tests_run_count,
            "tests_passed": tests_passed,
            "tests_failed": tests_failed,
            "zero_tests_run": zero_tests_run,
        })
    });
    let outcome = if success { "succeeded" } else { "failed" };

    Some(ValidationEvent {
        tool_name: finished.tool_name.clone(),
        execution_source: finished.tool_name.clone(),
        identity,
        assertion_name,
        purpose,
        validation_kind,
        success,
        execution_success: (success != execution_success).then_some(execution_success),
        failure_kind,
        failure_category: failure_kind,
        unresolved_failure: !success,
        exit_code: finished.exit_code,
        summary: format!("{} {}", finished.tool_name, outcome),
        command_summary,
        cwd,
        shell,
        execution_state,
        project,
        session_id: finished.session_id.clone(),
        started_at,
        completed_at,
        duration_ms: finished.duration_ms,
        affected_paths,
        diagnostics,
        detected_summary,
        tests_detected,
        tests_run_count,
        tests_passed,
        tests_failed,
        zero_tests_run,
        test_count_assertion,
        stdout_lines,
        stderr_lines,
        stdout_truncated,
        stderr_truncated,
        stdout_evidence,
        stderr_evidence,
    })
}

fn validation_adapter_for_execution(
    finished: &SessionEvent,
    started: Option<&SessionEvent>,
) -> Option<&'static dyn ValidationAdapter> {
    validation_adapter_for_tool(&finished.tool_name)
        .or_else(|| {
            started
                .and_then(|event| event.input_summary.as_ref())
                .and_then(|input| input.get("validation_tool"))
                .and_then(Value::as_str)
                .or_else(|| {
                    finished
                        .input_summary
                        .as_ref()
                        .and_then(|input| input.get("validation_tool"))
                        .and_then(Value::as_str)
                })
                .or_else(|| {
                    finished
                        .validation_output_summary
                        .as_ref()
                        .and_then(|summary| summary.get("validation_tool"))
                        .and_then(Value::as_str)
                })
                .and_then(validation_adapter_for_tool)
        })
        .or_else(|| {
            let command = started
                .and_then(|event| event.input_summary.as_ref())
                .and_then(|input| {
                    input
                        .get("command_summary")
                        .or_else(|| input.get("command"))
                })
                .and_then(Value::as_str)
                .or_else(|| {
                    finished
                        .validation_output_summary
                        .as_ref()
                        .and_then(|summary| summary.get("command_summary"))
                        .and_then(Value::as_str)
                })?;
            let mut words = command.split_whitespace();
            match (words.next(), words.next()) {
                (Some("cargo"), Some("fmt")) => validation_adapter_for_tool("cargo_fmt"),
                (Some("cargo"), Some("check")) => validation_adapter_for_tool("cargo_check"),
                (Some("cargo"), Some("test")) => validation_adapter_for_tool("cargo_test"),
                _ => None,
            }
        })
}

fn execution_string(
    started: Option<&SessionEvent>,
    finished: &SessionEvent,
    field: &str,
) -> Option<String> {
    finished
        .validation_output_summary
        .as_ref()
        .and_then(|summary| summary.get(field))
        .and_then(Value::as_str)
        .or_else(|| {
            started
                .and_then(|event| event.input_summary.as_ref())
                .and_then(|input| input.get(field))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn public_validation_assertion_name(
    started: Option<&SessionEvent>,
    finished: &SessionEvent,
    identity: &str,
) -> Option<String> {
    for raw in [
        started.and_then(|event| event.assertion_name.as_deref()),
        finished.assertion_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let Some(assertion_name) = safe_model_facing_assertion_name(&finished.tool_name, raw)
        else {
            continue;
        };
        if assertion_validation_identity(&assertion_name) == identity {
            return Some(assertion_name);
        }
    }
    None
}

fn execution_identity(
    started: Option<&SessionEvent>,
    finished: &SessionEvent,
    purpose: &str,
    command_summary: Option<&str>,
    tool_name: &str,
) -> String {
    if let Some(assertion) = started
        .and_then(|event| event.assertion_name.as_deref())
        .or(finished.assertion_name.as_deref())
        .filter(|value| !value.is_empty())
    {
        return assertion_validation_identity(assertion);
    }
    if let Some(identity) = started
        .and_then(|event| event.input_summary.as_ref())
        .and_then(|input| input.get("execution_identity"))
        .and_then(Value::as_str)
        .or_else(|| {
            finished
                .input_summary
                .as_ref()
                .and_then(|input| input.get("execution_identity"))
                .and_then(Value::as_str)
        })
        .filter(|value| is_validation_execution_identity(value))
    {
        return identity.to_string();
    }
    if validation_adapter_for_tool(tool_name).is_some() {
        if let Some(input) = started
            .and_then(|event| event.input_summary.as_ref())
            .or(finished.input_summary.as_ref())
        {
            if let Some(identity) = input
                .get("validation_target_id")
                .and_then(Value::as_str)
                .filter(|value| is_structured_validation_target_identity(value))
            {
                return identity.to_string();
            }
            if let Some(identity) = structured_validation_target_identity(tool_name, input) {
                return identity;
            }
        }
    }
    let command = started
        .and_then(|event| event.input_summary.as_ref())
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str)
        .or(command_summary)
        .unwrap_or(tool_name);
    let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{purpose}\0{normalized}").as_bytes())
    );
    format!("command:{}", &digest[..24])
}

fn first_line_summary(command: &str) -> String {
    crate::shell_client::command_preview(command)
}

type OutputEvidence = (
    Option<String>,
    Option<String>,
    bool,
    bool,
    Option<u64>,
    Option<u64>,
);

fn execution_output_evidence(finished: &SessionEvent) -> OutputEvidence {
    let Some(summary) = finished.validation_output_summary.as_ref() else {
        return (None, None, false, false, None, None);
    };
    (
        summary
            .get("stdout_tail_excerpt")
            .and_then(Value::as_str)
            .map(str::to_string),
        summary
            .get("stderr_tail_excerpt")
            .and_then(Value::as_str)
            .map(str::to_string),
        summary
            .get("stdout_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        summary
            .get("stderr_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        summary.get("stdout_lines").and_then(Value::as_u64),
        summary.get("stderr_lines").and_then(Value::as_u64),
    )
}

/// True when the finished tool call actually entered validation execution.
/// Parameter / schema / permission / enqueue rejections leave neither an exit
/// code nor validation output metadata and must not pollute the ledger.
fn validation_execution_started(finished: &SessionEvent) -> bool {
    finished.exit_code.is_some() || finished.validation_output_summary.is_some()
}

fn validation_failure_kind(
    finished: &SessionEvent,
    success: bool,
    diagnostics: Option<&ValidationDiagnostics>,
    adapter: Option<&dyn ValidationAdapter>,
) -> &'static str {
    if let Some(adapter) = adapter {
        let (stdout_excerpt, stderr_excerpt, _) =
            validation_output_excerpts(finished).unwrap_or_default();
        return adapter.map_failure_kind(ValidationFailureEvidence {
            success,
            reported_failure_kind: finished
                .failure_kind
                .as_deref()
                .or(finished.error_kind.as_deref()),
            exit_code: finished.exit_code,
            diagnostics,
            stdout_excerpt,
            stderr_excerpt,
        });
    }

    if success {
        return "unknown";
    }
    if matches!(
        finished
            .failure_kind
            .as_deref()
            .or(finished.error_kind.as_deref()),
        Some("timeout" | "timed_out" | "command_timeout")
    ) {
        return "timeout";
    }

    if finished.exit_code.is_some_and(|exit_code| exit_code != 0)
        || matches!(
            finished
                .failure_kind
                .as_deref()
                .or(finished.error_kind.as_deref()),
            Some(
                "command_exit_nonzero"
                    | "command_spawn_failed"
                    | "command_wait_failed"
                    | "command_output_failed"
            )
        )
    {
        return "process_exit";
    }
    "unknown"
}

fn matching_start(
    started: &mut Vec<SessionEvent>,
    finished: &SessionEvent,
) -> Option<SessionEvent> {
    let pos = if let Some(call_id) = finished.call_id.as_deref() {
        // call_id is the exact pair-level correlation. In particular, a
        // same-Session logical invocation has recorder and business starts with
        // the same tool/timestamp but distinct call ids; falling back in that
        // case could attach recorder input to the canonical business finish.
        started
            .iter()
            .position(|event| event.call_id.as_deref() == Some(call_id))?
    } else {
        // Legacy ledger rows predate call_id. Preserve their conservative
        // historical matcher without inventing correlation.
        started.iter().position(|event| {
            event.call_id.is_none()
                && event.session_id == finished.session_id
                && event.tool_name == finished.tool_name
                && event.started_at == finished.started_at
        })?
    };
    Some(started.remove(pos))
}

fn parser_unavailable() -> ValidationParserSummary {
    ValidationParserSummary {
        available: false,
        kind: PARSER_KIND,
        version: PARSER_VERSION,
        source: VALIDATION_PARSER_SOURCE,
        raw_output_exposed: false,
        limitations: PARSER_LIMITATIONS,
        reason: Some(VALIDATION_OUTPUT_METADATA_ABSENT_REASON),
    }
}

fn parser_available() -> ValidationParserSummary {
    ValidationParserSummary {
        available: true,
        kind: PARSER_KIND,
        version: PARSER_VERSION,
        source: VALIDATION_PARSER_SOURCE,
        raw_output_exposed: false,
        limitations: PARSER_LIMITATIONS,
        reason: None,
    }
}

fn parser_summary_for_events(events: &[ValidationEvent]) -> ValidationParserSummary {
    if events.iter().any(|event| event.diagnostics.is_some()) {
        parser_available()
    } else {
        parser_unavailable()
    }
}

fn validation_diagnostics_from_summary(
    finished: &SessionEvent,
    adapter: &dyn ValidationAdapter,
) -> Option<ValidationDiagnostics> {
    let (stdout_excerpt, stderr_excerpt, truncated) = validation_output_excerpts(finished)?;
    Some(adapter.parse(stdout_excerpt, stderr_excerpt, truncated))
}

fn validation_output_excerpts(finished: &SessionEvent) -> Option<(&str, &str, bool)> {
    let summary = finished.validation_output_summary.as_ref()?.as_object()?;
    let stdout_excerpt = summary
        .get("stdout_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stderr_excerpt = summary
        .get("stderr_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let truncated = summary
        .get("stdout_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || summary
            .get("stderr_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Some((stdout_excerpt, stderr_excerpt, truncated))
}

fn validation_test_run_metadata(
    finished: &SessionEvent,
    adapter: Option<&dyn ValidationAdapter>,
    diagnostics: Option<&ValidationDiagnostics>,
) -> (
    Option<bool>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<bool>,
) {
    let summary = finished.validation_output_summary.as_ref();
    let explicit_test_metadata = summary.is_some_and(|value| {
        [
            "tests_detected",
            "tests_run_count",
            "tests_passed",
            "tests_failed",
            "zero_tests_run",
        ]
        .into_iter()
        .any(|field| value.get(field).is_some())
    });
    if !adapter.is_some_and(ValidationAdapter::reports_test_run_metadata) && !explicit_test_metadata
    {
        return (None, None, None, None, None);
    }
    let parsed_test_summary = diagnostics.and_then(|value| value.test_summary.as_ref());
    let truncated = summary
        .and_then(|value| value.get("stdout_truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || summary
            .and_then(|value| value.get("stderr_truncated"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let parsed_passed = (!truncated)
        .then(|| parsed_test_summary.and_then(|value| value.passed))
        .flatten();
    let parsed_failed = (!truncated)
        .then(|| parsed_test_summary.and_then(|value| value.failed))
        .flatten();
    let parsed_ignored = (!truncated)
        .then(|| parsed_test_summary.and_then(|value| value.ignored))
        .flatten();
    // Go's structured test count includes skipped tests, while Cargo's canonical
    // count intentionally remains passed + failed. Reconcile each tool against
    // the same semantics used by its structured Job projection.
    let include_ignored = finished.tool_name == "go_test";
    let component_tests_run = |passed: Option<u64>, failed: Option<u64>| match (passed, failed) {
        (Some(passed), Some(failed)) => {
            let run = passed.saturating_add(failed);
            if include_ignored {
                parsed_ignored.map(|ignored| run.saturating_add(ignored))
            } else {
                Some(run)
            }
        }
        _ => None,
    };
    let parsed_tests_run = component_tests_run(parsed_passed, parsed_failed);
    let tests_detected = summary
        .and_then(|value| value.get("tests_detected"))
        .and_then(Value::as_bool)
        .or_else(|| Some(parsed_test_summary.is_some()));

    let explicit_tests_run_field = summary.and_then(|value| value.get("tests_run_count"));
    let explicit_tests_passed_field = summary.and_then(|value| value.get("tests_passed"));
    let explicit_tests_failed_field = summary.and_then(|value| value.get("tests_failed"));
    let explicit_zero_tests_field = summary.and_then(|value| value.get("zero_tests_run"));
    if [
        explicit_tests_run_field,
        explicit_tests_passed_field,
        explicit_tests_failed_field,
        explicit_zero_tests_field,
    ]
    .into_iter()
    .flatten()
    .any(Value::is_null)
    {
        return (tests_detected, None, None, None, None);
    }

    let explicit_tests_run = explicit_tests_run_field.and_then(Value::as_u64);
    let explicit_tests_passed = explicit_tests_passed_field.and_then(Value::as_u64);
    let explicit_tests_failed = explicit_tests_failed_field.and_then(Value::as_u64);
    let explicit_zero_tests = explicit_zero_tests_field.and_then(Value::as_bool);
    let explicit_component_run = component_tests_run(explicit_tests_passed, explicit_tests_failed);
    let explicit_coherent = match (explicit_tests_run, explicit_component_run) {
        (Some(run), Some(component_run)) => run == component_run,
        _ => true,
    } && match (
        explicit_tests_run.or(explicit_component_run),
        explicit_zero_tests,
    ) {
        (Some(run), Some(zero)) => zero == (run == 0),
        _ => true,
    } && !(explicit_zero_tests == Some(true)
        && (explicit_tests_passed.is_some_and(|count| count > 0)
            || explicit_tests_failed.is_some_and(|count| count > 0)));

    let sources_agree = [
        (explicit_tests_run, parsed_tests_run),
        (explicit_tests_passed, parsed_passed),
        (explicit_tests_failed, parsed_failed),
    ]
    .into_iter()
    .all(|(explicit, parsed)| match (explicit, parsed) {
        (Some(explicit), Some(parsed)) => explicit == parsed,
        _ => true,
    });
    if !explicit_coherent || !sources_agree {
        return (tests_detected, None, None, None, None);
    }

    let tests_passed = explicit_tests_passed.or(parsed_passed);
    let tests_failed = explicit_tests_failed.or(parsed_failed);
    let tests_run_count = explicit_tests_run
        .or(explicit_component_run)
        .or(parsed_tests_run);
    let zero_tests_run = explicit_zero_tests.or_else(|| tests_run_count.map(|count| count == 0));
    let component_run = component_tests_run(tests_passed, tests_failed);
    if matches!((tests_run_count, component_run), (Some(run), Some(component_run)) if run != component_run)
        || (zero_tests_run == Some(true)
            && (tests_run_count.is_some_and(|count| count > 0)
                || tests_passed.is_some_and(|count| count > 0)
                || tests_failed.is_some_and(|count| count > 0)))
    {
        return (tests_detected, None, None, None, None);
    }
    (
        tests_detected,
        tests_run_count,
        tests_passed,
        tests_failed,
        zero_tests_run,
    )
}

fn cargo_test_zero_tests_success(event: &ValidationEvent) -> bool {
    structured_test_requires_execution_proof(event)
        && event.success
        && event.zero_tests_run == Some(true)
}

fn to_value(summary: ValidationSummary) -> Value {
    serde_json::to_value(summary).unwrap_or_else(|_| {
        json!({
            "available": false,
            "status": "unknown",
            "reason": "validation_summary_unavailable",
            "latest": null,
            "latest_status": "unknown",
            "historical_failures": {
                "count": 0,
                "resolved": false,
                "unresolved": false,
            },
            "resolved_failures": {"count": 0, "events": []},
            "unresolved_failures": {"count": 0, "events": []},
            "source": VALIDATION_SOURCE,
            "events_total": 0,
            "events": [],
            "parser": {
                "available": false,
                "kind": PARSER_KIND,
                "version": PARSER_VERSION,
                "source": VALIDATION_PARSER_SOURCE,
                "raw_output_exposed": false,
                "limitations": PARSER_LIMITATIONS,
                "reason": VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
            },
            "cargo_test_zero_tests_run": false
        })
    })
}

fn is_false(value: &bool) -> bool {
    !*value
}
