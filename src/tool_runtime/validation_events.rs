//! Ledger-derived validation event summaries.
//!
//! This module deliberately records facts already present in the session
//! ledger. It does not expose stdout/stderr, infer root causes, or change tool
//! execution behavior. Diagnostics are parsed only from safe bounded validation
//! output metadata captured by session events.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::session_context::{
    session_project_mismatch_result, unknown_session_result, SessionProjectMismatch,
};
use super::sessions::{SessionEvent, SessionSummary};
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
    pub(crate) purpose: String,
    pub(crate) validation_kind: String,
    pub(crate) success: bool,
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
    pub(crate) zero_tests_run: Option<bool>,
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

#[cfg(test)]
pub(crate) fn validation_summary_for_session(summary: &SessionSummary) -> Value {
    validation_summary_from_events(&summary.events, DEFAULT_VALIDATION_EVENT_LIMIT)
}

impl ToolRuntime {
    pub(crate) async fn validation_summary_tool(
        &self,
        project: String,
        session_id: String,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
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
        let mut events = summary.events.clone();
        let accepted_jobs = summary
            .events
            .iter()
            .filter(|event| {
                event.kind == "tool_call_finished"
                    && event.job_id.is_some()
                    && (event.tool_name == "run_job"
                        || validation_adapter_for_tool(&event.tool_name).is_some())
                    && job_acceptance_only(event)
            })
            .filter_map(|event| event.job_id.clone())
            .collect::<Vec<_>>();
        for job_id in accepted_jobs {
            let status = self.job_status_for_auth(job_id.clone(), false, auth).await;
            if !status.success
                || !status
                    .output
                    .get("terminal")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                continue;
            }
            let log = self
                .job_log_for_auth(job_id.clone(), None, Some(200), auth, None, None)
                .await;
            let Some(accepted) = summary.events.iter().find(|event| {
                event.kind == "tool_call_finished"
                    && event.job_id.as_deref() == Some(job_id.as_str())
                    && (event.tool_name == "run_job"
                        || validation_adapter_for_tool(&event.tool_name).is_some())
            }) else {
                continue;
            };
            let mut observed = accepted.clone();
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
                    "stdout_truncated": false,
                    "stderr_truncated": false,
                })
            };
            if let Some(validation) = status.output.get("validation").and_then(Value::as_object) {
                for field in [
                    "passed",
                    "warnings_count",
                    "errors_count",
                    "tests_detected",
                    "tests_run_count",
                    "tests_passed",
                    "tests_failed",
                    "zero_tests_run",
                    "diagnostics",
                ] {
                    if let Some(value) = validation.get(field) {
                        output[field] = value.clone();
                    }
                }
            }
            for field in ["purpose", "command_summary", "cwd", "shell", "executor"] {
                if output.get(field).is_none_or(Value::is_null) {
                    output[field] = accepted
                        .validation_output_summary
                        .as_ref()
                        .and_then(|value| value.get(field))
                        .cloned()
                        .unwrap_or(Value::Null);
                }
            }
            if output.get("purpose").is_none_or(Value::is_null) {
                output["purpose"] = json!("other");
            }
            output["execution_state"] = json!(match job_status {
                "timeout" | "timed_out" => "timed_out",
                "stopped" | "cancelled" => "cancelled",
                _ => "completed",
            });
            output["exit_code"] = status
                .output
                .get("exit_code")
                .cloned()
                .unwrap_or(Value::Null);
            observed.validation_output_summary =
                super::sessions::execution_output_summary_for_tool_result(
                    &accepted.tool_name,
                    &output,
                );
            events.push(observed);
        }
        events.sort_by_key(|event| {
            (
                event.timestamp,
                event.finished_at.unwrap_or(event.timestamp),
            )
        });
        validation_summary_from_events(&events, limit)
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
    to_value(ValidationSummary {
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
    })
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
    let mut latest_success_by_identity = HashMap::<String, usize>::new();
    for (index, event) in events.iter().enumerate() {
        if event.success && validation_event_decides_historical_failure_status(event) {
            latest_success_by_identity.insert(event.identity.clone(), index);
        }
    }
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for (index, event) in events.iter_mut().enumerate() {
        if event.success {
            continue;
        }
        let is_resolved = latest_success_by_identity
            .get(&event.identity)
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

fn validation_event_decides_historical_failure_status(event: &ValidationEvent) -> bool {
    !cargo_test_zero_tests_success(event)
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
    let mut started = Vec::new();
    let mut validation_events = Vec::new();

    for event in events {
        match event.kind.as_str() {
            "tool_call_started" if execution_purpose(event).is_some() => {
                started.push(event.clone());
            }
            "tool_call_finished" => {
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
                    validation_events.push(validation_event);
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
    match tool_name {
        "validate_patch" => Some("patch_preflight"),
        "apply_patch_checked" => Some("patch_apply_checked"),
        _ => None,
    }
}

fn execution_purpose(event: &SessionEvent) -> Option<String> {
    if let Some(kind) = validation_kind_for_tool(&event.tool_name) {
        return Some(
            match kind {
                "test" => "test",
                "check" | "patch_preflight" | "patch_apply_checked" => "validation",
                "format" => "format",
                _ => "validation",
            }
            .to_string(),
        );
    }
    if !matches!(
        event.tool_name.as_str(),
        "run_process" | "run_shell" | "run_job"
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
    let success = match finished.status.as_deref() {
        Some("succeeded") => true,
        Some("failed") => false,
        _ => return None,
    };
    // Pre-execution rejections (invalid arguments, offline agent, scope/guard
    // denial, schema reject) must not become validation evidence. Only calls
    // that actually entered validation execution — exit code and/or bounded
    // validation output metadata — feed the ledger summary.
    if !validation_execution_started(finished) && (!success || finished.tool_name == "run_job") {
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
    let failure_kind = validation_failure_kind(finished, success, diagnostics.as_ref(), adapter);
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
    let (
        stdout_evidence,
        stderr_evidence,
        stdout_truncated,
        stderr_truncated,
        stdout_lines,
        stderr_lines,
    ) = execution_output_evidence(finished);
    let (tests_detected, tests_run_count, zero_tests_run) =
        validation_test_run_metadata(finished, adapter, diagnostics.as_ref());
    let detected_summary = adapter.map(|adapter| {
        json!({
            "kind": adapter.validation_kind(),
            "parser_available": diagnostics.as_ref().is_some_and(|value| value.available),
            "tests_detected": tests_detected,
            "tests_run_count": tests_run_count,
            "zero_tests_run": zero_tests_run,
        })
    });
    let outcome = if success { "succeeded" } else { "failed" };

    Some(ValidationEvent {
        tool_name: finished.tool_name.clone(),
        execution_source: finished.tool_name.clone(),
        identity,
        purpose,
        validation_kind,
        success,
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
        zero_tests_run,
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
    validation_adapter_for_tool(&finished.tool_name).or_else(|| {
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
        return format!("assertion:{assertion}");
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
    let pos = started.iter().position(|event| {
        event.session_id == finished.session_id
            && event.tool_name == finished.tool_name
            && event.started_at == finished.started_at
    })?;
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
) -> (Option<bool>, Option<u64>, Option<bool>) {
    if !adapter.is_some_and(ValidationAdapter::reports_test_run_metadata) {
        return (None, None, None);
    }
    let summary = finished.validation_output_summary.as_ref();
    let parsed_test_summary = diagnostics.and_then(|value| value.test_summary.as_ref());
    let truncated = summary
        .and_then(|value| value.get("stdout_truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || summary
            .and_then(|value| value.get("stderr_truncated"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let parsed_tests_run = (!truncated)
        .then(|| {
            parsed_test_summary.map(|value| {
                value
                    .passed
                    .unwrap_or(0)
                    .saturating_add(value.failed.unwrap_or(0))
            })
        })
        .flatten();
    let tests_detected = summary
        .and_then(|value| value.get("tests_detected"))
        .and_then(Value::as_bool)
        .or_else(|| Some(parsed_test_summary.is_some()));
    let tests_run_count = summary
        .and_then(|value| value.get("tests_run_count"))
        .and_then(Value::as_u64)
        .or(parsed_tests_run);
    let zero_tests_run = summary
        .and_then(|value| value.get("zero_tests_run"))
        .and_then(Value::as_bool)
        .or_else(|| parsed_tests_run.map(|count| count == 0));
    (tests_detected, tests_run_count, zero_tests_run)
}

fn cargo_test_zero_tests_success(event: &ValidationEvent) -> bool {
    event.validation_kind == "test" && event.success && event.zero_tests_run == Some(true)
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
