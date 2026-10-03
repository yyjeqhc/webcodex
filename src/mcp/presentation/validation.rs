//! Validation display owns its tool registration and conservative field/budget
//! projection. Canonical validation policy, parsing and evidence remain elsewhere.
use super::registry::PresentationRenderer;
use super::{
    bounded_text, copy_bounded_text, copy_scalar, MAX_MCP_PRESENTATION_ITEMS,
    MCP_PRESENTATION_VERSION,
};
use serde_json::{json, Map, Value};
use webcodex_validation::validation_kind_for_tool;

pub(super) const RUN: PresentationRenderer = PresentationRenderer {
    tools: &["cargo_check", "cargo_test", "go_test"],
    project: validation_run_presentation,
};
pub(super) const SUMMARY: PresentationRenderer = PresentationRenderer {
    tools: &["read_validation_summary"],
    project: |_, output| validation_summary_presentation(output),
};

fn validation_diagnostic_item(value: &Value) -> Option<Value> {
    let severity = value.get("severity").and_then(bounded_text)?;
    let message = value.get("message").and_then(bounded_text)?;
    let mut item = Map::new();
    item.insert("severity".to_string(), Value::String(severity));
    copy_bounded_text(value, &mut item, "code");
    item.insert("message".to_string(), Value::String(message));
    Some(Value::Object(item))
}

fn validation_failed_test_item(value: &Value) -> Option<Value> {
    let name = value.get("name").and_then(bounded_text)?;
    let failure_kind = value.get("failure_kind").and_then(bounded_text)?;
    Some(json!({
        "name": name,
        "failure_kind": failure_kind,
    }))
}

fn validation_diagnostics_presentation(diagnostics: &Value) -> Option<Value> {
    diagnostics.as_object()?;
    let mut output = Map::new();
    for key in [
        "available",
        "diagnostic_count",
        "returned_diagnostic_count",
        "diagnostics_truncated",
        "failed_test_details_truncated",
    ] {
        copy_scalar(diagnostics, &mut output, key);
    }

    let diagnostic_source = diagnostics
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let failed_source = diagnostics
        .get("failed_test_details")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let source_items = diagnostic_source.len().saturating_add(failed_source.len());
    let mut remaining = MAX_MCP_PRESENTATION_ITEMS;
    let mut diagnostic_items = Vec::new();
    for item in diagnostic_source {
        if remaining == 0 {
            break;
        }
        if let Some(item) = validation_diagnostic_item(item) {
            diagnostic_items.push(item);
            remaining -= 1;
        }
    }
    let mut failed_tests = Vec::new();
    for item in failed_source {
        if remaining == 0 {
            break;
        }
        if let Some(item) = validation_failed_test_item(item) {
            failed_tests.push(item);
            remaining -= 1;
        }
    }
    if !diagnostic_items.is_empty() {
        output.insert("items".to_string(), Value::Array(diagnostic_items));
    }
    if !failed_tests.is_empty() {
        output.insert("failed_tests".to_string(), Value::Array(failed_tests));
    }
    output.insert(
        "presentation_items_truncated".to_string(),
        Value::Bool(source_items > MAX_MCP_PRESENTATION_ITEMS),
    );
    Some(Value::Object(output))
}

fn validation_run_presentation(tool_name: &str, output: &Value) -> Option<Value> {
    output.as_object()?;
    let validation_kind = validation_kind_for_tool(tool_name)?;
    let mut presentation = Map::new();
    presentation.insert("version".to_string(), Value::from(MCP_PRESENTATION_VERSION));
    presentation.insert(
        "kind".to_string(),
        Value::String("validation_run".to_string()),
    );
    presentation.insert("tool".to_string(), Value::String(tool_name.to_string()));
    presentation.insert(
        "validation_kind".to_string(),
        Value::String(validation_kind.to_string()),
    );
    for key in ["execution_state", "failure_kind", "job_id", "job_status"] {
        copy_bounded_text(output, &mut presentation, key);
    }
    for key in [
        "terminal",
        "passed",
        "command_started",
        "command_completed",
        "duration_ms",
        "exit_code",
        "promoted_to_job",
        "warnings_count",
        "errors_count",
        "tests_detected",
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        copy_scalar(output, &mut presentation, key);
    }
    if let Some(diagnostics) = output
        .get("diagnostics")
        .and_then(validation_diagnostics_presentation)
    {
        presentation.insert("diagnostics".to_string(), diagnostics);
    }
    Some(Value::Object(presentation))
}

fn validation_event_presentation(event: &Value) -> Option<Value> {
    event.as_object()?;
    let mut item = Map::new();
    for key in [
        "tool_name",
        "validation_kind",
        "failure_class",
        "failure_kind",
    ] {
        copy_bounded_text(event, &mut item, key);
    }
    for key in [
        "success",
        "validation_passed",
        "expectation_satisfied",
        "unresolved_failure",
        "duration_ms",
        "tests_detected",
        "tests_run_count",
        "zero_tests_run",
    ] {
        copy_scalar(event, &mut item, key);
    }
    if let Some(test_summary) = event.pointer("/diagnostics/test_summary") {
        if let Some(value) = test_summary.get("passed").filter(|value| value.is_number()) {
            item.insert("tests_passed".to_string(), value.clone());
        }
        if let Some(value) = test_summary.get("failed").filter(|value| value.is_number()) {
            item.insert("tests_failed".to_string(), value.clone());
        }
    }
    (!item.is_empty()).then_some(Value::Object(item))
}

fn validation_current_evidence_presentation(current: &Value) -> Option<Value> {
    current.as_object()?;
    let mut output = Map::new();
    for key in ["status", "reason", "latest_status", "boundary_reason"] {
        copy_bounded_text(current, &mut output, key);
    }
    for key in [
        "events_total",
        "successes",
        "failures",
        "expected_results",
        "resolved_failure_count",
        "unresolved_failure_count",
        "evidence_gap_event_count",
        "stale_failure_count",
        "evidence_after_latest_content_change",
    ] {
        copy_scalar(current, &mut output, key);
    }
    (!output.is_empty()).then_some(Value::Object(output))
}

fn validation_failure_set_count(validation: &Value, key: &str) -> Option<Value> {
    let set = validation.get(key)?;
    let count = set.get("count").filter(|value| value.is_number())?;
    Some(json!({"count": count.clone()}))
}

fn validation_historical_failures_presentation(validation: &Value) -> Option<Value> {
    let history = validation.get("historical_failures")?;
    history.as_object()?;
    let mut output = Map::new();
    for key in ["count", "resolved", "unresolved"] {
        copy_scalar(history, &mut output, key);
    }
    (!output.is_empty()).then_some(Value::Object(output))
}

fn validation_summary_presentation(output: &Value) -> Option<Value> {
    let validation = output.get("validation")?;
    validation.as_object()?;
    let mut summary = Map::new();
    for key in ["status", "latest_status", "reason"] {
        copy_bounded_text(validation, &mut summary, key);
    }
    for key in [
        "available",
        "events_total",
        "successes",
        "failures",
        "expected_results",
        "cargo_test_zero_tests_run",
    ] {
        copy_scalar(validation, &mut summary, key);
    }
    if let Some(current) = validation
        .get("current_evidence")
        .and_then(validation_current_evidence_presentation)
    {
        summary.insert("current_evidence".to_string(), current);
    }
    if let Some(history) = validation_historical_failures_presentation(validation) {
        summary.insert("historical_failures".to_string(), history);
    }
    for key in ["resolved_failures", "unresolved_failures", "evidence_gaps"] {
        if let Some(count) = validation_failure_set_count(validation, key) {
            summary.insert(key.to_string(), count);
        }
    }

    if let Some(source_events) = validation.get("events").and_then(Value::as_array) {
        // Canonical validation events are chronological. Keep the most recent
        // bounded subset without changing their canonical relative ordering.
        let skip = source_events
            .len()
            .saturating_sub(MAX_MCP_PRESENTATION_ITEMS);
        let events = source_events
            .iter()
            .skip(skip)
            .filter_map(validation_event_presentation)
            .collect::<Vec<_>>();
        summary.insert("events".to_string(), Value::Array(events));
        summary.insert(
            "events_truncated".to_string(),
            Value::Bool(source_events.len() > MAX_MCP_PRESENTATION_ITEMS),
        );
    }
    Some(json!({
        "version": MCP_PRESENTATION_VERSION,
        "kind": "validation_summary",
        "validation": Value::Object(summary),
    }))
}
