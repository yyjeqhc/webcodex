//! Ledger-derived validation event summaries.
//!
//! This module deliberately records facts already present in the session
//! ledger. It does not expose stdout/stderr, infer root causes, or change tool
//! execution behavior. Diagnostics are parsed only from safe bounded validation
//! output metadata captured by session events.

use serde::Serialize;
use serde_json::{json, Value};

use super::session_context::{
    session_project_mismatch_result, unknown_session_result, SessionProjectMismatch,
};
use super::sessions::{SessionEvent, SessionSummary};
use super::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, ValidationDiagnostics,
    PARSER_KIND, PARSER_LIMITATIONS, PARSER_VERSION, VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;

const DEFAULT_VALIDATION_EVENT_LIMIT: usize = 10;
const VALIDATION_SOURCE: &str = "session_ledger";
const VALIDATION_PARSER_SOURCE: &str = "bounded_validation_metadata";
const DEFAULT_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 20;
const MAX_PUBLIC_VALIDATION_EVENT_LIMIT: usize = 100;
const PUBLIC_VALIDATION_SESSION_EVENT_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationEvent {
    pub(crate) tool_name: String,
    pub(crate) validation_kind: &'static str,
    pub(crate) success: bool,
    pub(crate) failure_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i64>,
    pub(crate) summary: String,
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
    pub(crate) input_summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<ValidationDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_detected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tests_run_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zero_tests_run: Option<bool>,
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
        let mut validation = validation_summary_from_events(&summary.events, limit);
        remove_public_validation_input_summaries(&mut validation);
        ToolResult::ok(json!({
            "project": resolved.resolved_id,
            "session_id": session_id,
            "validation": validation,
        }))
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
    let historical_failures = validation_historical_failures(&validation_events);
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

fn validation_historical_failures(events: &[ValidationEvent]) -> ValidationHistoricalFailures {
    let failures = events.iter().filter(|event| !event.success).count();
    let latest_decisive = events
        .iter()
        .rev()
        .find(|event| validation_event_decides_historical_failure_status(event));
    ValidationHistoricalFailures {
        count: failures,
        resolved: failures > 0 && latest_decisive.is_some_and(|event| event.success),
        unresolved: failures > 0 && latest_decisive.is_some_and(|event| !event.success),
    }
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

pub(crate) fn extract_validation_events(events: &[SessionEvent]) -> Vec<ValidationEvent> {
    let mut started = Vec::new();
    let mut validation_events = Vec::new();

    for event in events {
        match event.kind.as_str() {
            "tool_call_started" if validation_kind_for_tool(&event.tool_name).is_some() => {
                started.push(event.clone());
            }
            "tool_call_finished" => {
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

pub(crate) fn validation_kind_for_tool(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "cargo_fmt" => Some("format"),
        "cargo_check" => Some("check"),
        "cargo_test" => Some("test"),
        "validate_patch" => Some("patch_preflight"),
        "apply_patch_checked" => Some("patch_apply_checked"),
        _ => None,
    }
}

fn validation_event_from_finished(
    finished: &SessionEvent,
    started: Option<&SessionEvent>,
) -> Option<ValidationEvent> {
    let validation_kind = validation_kind_for_tool(&finished.tool_name)?;
    let success = match finished.status.as_deref() {
        Some("succeeded") => true,
        Some("failed") => false,
        _ => return None,
    };
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
    let input_summary = started.and_then(|event| event.input_summary.clone());
    let diagnostics = validation_diagnostics_from_summary(finished);
    let failure_kind = validation_failure_kind(finished, success, diagnostics.as_ref());
    let (tests_detected, tests_run_count, zero_tests_run) = cargo_test_run_metadata(finished);
    let outcome = if success { "succeeded" } else { "failed" };

    Some(ValidationEvent {
        tool_name: finished.tool_name.clone(),
        validation_kind,
        success,
        failure_kind,
        exit_code: finished.exit_code,
        summary: format!("{} {}", finished.tool_name, outcome),
        project,
        session_id: finished.session_id.clone(),
        started_at,
        completed_at,
        duration_ms: finished.duration_ms,
        affected_paths,
        input_summary,
        diagnostics,
        tests_detected,
        tests_run_count,
        zero_tests_run,
    })
}

fn validation_failure_kind(
    finished: &SessionEvent,
    success: bool,
    diagnostics: Option<&ValidationDiagnostics>,
) -> &'static str {
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

    let has_compile_error = diagnostics.is_some_and(|diagnostics| {
        diagnostics
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
    });
    if matches!(finished.tool_name.as_str(), "cargo_check" | "cargo_test") && has_compile_error {
        return "compile_error";
    }

    if finished.tool_name == "cargo_test"
        && diagnostics.is_some_and(|diagnostics| {
            diagnostics
                .test_summary
                .as_ref()
                .and_then(|summary| summary.failed)
                .is_some_and(|failed| failed > 0)
                || !diagnostics.failed_tests.is_empty()
        })
    {
        return "test_failure";
    }

    if finished.tool_name == "cargo_fmt" && cargo_fmt_has_stable_diff_metadata(finished) {
        return "format_diff";
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

fn cargo_fmt_has_stable_diff_metadata(finished: &SessionEvent) -> bool {
    let Some(summary) = finished.validation_output_summary.as_ref() else {
        return false;
    };
    ["stdout_tail_excerpt", "stderr_tail_excerpt"]
        .iter()
        .filter_map(|key| summary.get(*key).and_then(Value::as_str))
        .flat_map(str::lines)
        .map(str::trim_start)
        .filter_map(|line| line.strip_prefix("Diff in "))
        .any(|location| {
            let location = location.trim_end_matches(':');
            location
                .rsplit_once(':')
                .is_some_and(|(_, line)| line.parse::<u64>().is_ok_and(|line| line > 0))
        })
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

fn validation_diagnostics_from_summary(finished: &SessionEvent) -> Option<ValidationDiagnostics> {
    let summary = finished.validation_output_summary.as_ref()?.as_object()?;
    let stdout_tail = summary
        .get("stdout_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stderr_tail = summary
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

    match finished.tool_name.as_str() {
        "cargo_fmt" | "cargo_check" => Some(parse_cargo_check_diagnostics(
            stdout_tail,
            stderr_tail,
            truncated,
        )),
        "cargo_test" => Some(parse_cargo_test_diagnostics(
            stdout_tail,
            stderr_tail,
            truncated,
        )),
        _ => None,
    }
}

fn cargo_test_run_metadata(finished: &SessionEvent) -> (Option<bool>, Option<u64>, Option<bool>) {
    if finished.tool_name != "cargo_test" {
        return (None, None, None);
    }
    let Some(summary) = finished.validation_output_summary.as_ref() else {
        return (None, None, None);
    };
    let tests_detected = summary.get("tests_detected").and_then(Value::as_bool);
    let tests_run_count = summary.get("tests_run_count").and_then(Value::as_u64);
    let zero_tests_run = summary.get("zero_tests_run").and_then(Value::as_bool);
    (tests_detected, tests_run_count, zero_tests_run)
}

fn cargo_test_zero_tests_success(event: &ValidationEvent) -> bool {
    event.tool_name == "cargo_test" && event.success && event.zero_tests_run == Some(true)
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
