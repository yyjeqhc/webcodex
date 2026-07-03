//! Ledger-derived validation event summaries.
//!
//! This module deliberately records facts already present in the session
//! ledger. It does not expose stdout/stderr, infer root causes, or change tool
//! execution behavior. Diagnostics are parsed only from safe bounded validation
//! output metadata captured by session events.

use serde::Serialize;
use serde_json::{json, Value};

use super::sessions::{SessionEvent, SessionSummary};
use super::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, ValidationDiagnostics,
    PARSER_KIND, PARSER_LIMITATIONS, PARSER_VERSION, VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
};

const DEFAULT_VALIDATION_EVENT_LIMIT: usize = 10;
const VALIDATION_SOURCE: &str = "session_ledger";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationEvent {
    pub(crate) tool_name: String,
    pub(crate) validation_kind: &'static str,
    pub(crate) success: bool,
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
}

#[derive(Debug, Clone, Serialize)]
struct ValidationParserSummary {
    available: bool,
    kind: &'static str,
    version: u8,
    limitations: [&'static str; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationSummary {
    available: bool,
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
    #[serde(skip_serializing_if = "is_false")]
    skipped: bool,
}

pub(crate) fn validation_summary_for_session(summary: &SessionSummary) -> Value {
    validation_summary_from_events(&summary.events, DEFAULT_VALIDATION_EVENT_LIMIT)
}

pub(crate) fn skipped_validation_summary() -> Value {
    to_value(ValidationSummary {
        available: false,
        source: VALIDATION_SOURCE,
        events_total: 0,
        successes: None,
        failures: None,
        latest_success: None,
        latest_failure: None,
        events: Vec::new(),
        parser: parser_unavailable(),
        skipped: true,
    })
}

pub(crate) fn validation_summary_from_events(events: &[SessionEvent], limit: usize) -> Value {
    let validation_events = extract_validation_events(events);
    let events_total = validation_events.len();
    if events_total == 0 {
        return to_value(ValidationSummary {
            available: false,
            source: VALIDATION_SOURCE,
            events_total,
            successes: None,
            failures: None,
            latest_success: None,
            latest_failure: None,
            events: Vec::new(),
            parser: parser_unavailable(),
            skipped: false,
        });
    }

    let successes = validation_events
        .iter()
        .filter(|event| event.success)
        .count();
    let failures = events_total.saturating_sub(successes);
    let parser = parser_summary_for_events(&validation_events);
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
        source: VALIDATION_SOURCE,
        events_total,
        successes: Some(successes),
        failures: Some(failures),
        latest_success,
        latest_failure,
        events,
        parser,
        skipped: false,
    })
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
    let outcome = if success { "succeeded" } else { "failed" };

    Some(ValidationEvent {
        tool_name: finished.tool_name.clone(),
        validation_kind,
        success,
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
        limitations: PARSER_LIMITATIONS,
        reason: Some(VALIDATION_OUTPUT_METADATA_ABSENT_REASON),
    }
}

fn parser_available() -> ValidationParserSummary {
    ValidationParserSummary {
        available: true,
        kind: PARSER_KIND,
        version: PARSER_VERSION,
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

fn to_value(summary: ValidationSummary) -> Value {
    serde_json::to_value(summary).unwrap_or_else(|_| {
        json!({
            "available": false,
            "source": VALIDATION_SOURCE,
            "events_total": 0,
            "events": [],
            "parser": {
                "available": false,
                "kind": PARSER_KIND,
                "version": PARSER_VERSION,
                "limitations": PARSER_LIMITATIONS,
                "reason": VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
            }
        })
    })
}

fn is_false(value: &bool) -> bool {
    !*value
}
