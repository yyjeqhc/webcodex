//! Tool-call event helpers: classification, expectations, validation excerpts, path extraction.
use super::super::metadata::{ToolPathHint, ToolRisk};
use super::super::tool_definition::{
    runtime_tool_captures_validation_output, runtime_tool_is_change_summary_like,
    runtime_tool_is_git_like, runtime_tool_is_read_like, runtime_tool_is_shell_like,
    runtime_tool_is_write_like, runtime_tool_metadata, runtime_tool_session_risk_class,
};
use serde_json::{json, Value};

use super::model::{
    SessionEvent, ToolCallExpectation, ToolCallRecorderMetadata, MAX_VALIDATION_EXCERPT_CHARS,
    SESSION_ID_PREFIX, TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_EXPECTATION_METADATA_FIELDS,
    TOOL_EXPECTATION_RESULT_MATCHED, TOOL_EXPECTATION_RESULT_MISMATCH,
    TOOL_EXPECTATION_RESULT_NONE, TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
    TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS, TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD,
};
use super::util::{bound_summary_string, validation_excerpt};

impl ToolCallRecorderMetadata {
    pub(crate) fn from_arguments(arguments: &Value) -> Self {
        Self {
            expectation: tool_call_expectation_from_arguments(arguments),
        }
    }
}

pub(crate) fn is_valid_session_id(session_id: &str) -> bool {
    session_id.starts_with(SESSION_ID_PREFIX)
        && session_id.len() > SESSION_ID_PREFIX.len()
        && session_id
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub(crate) fn extract_project(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|obj| obj.get("project"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn tool_call_expectation_from_arguments(arguments: &Value) -> ToolCallExpectation {
    let Some(obj) = arguments.as_object() else {
        return ToolCallExpectation::default();
    };
    let expected_failure = obj
        .get(TOOL_EXPECTED_FAILURE_FIELD)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_failure_kind = obj
        .get(TOOL_EXPECTED_FAILURE_KIND_FIELD)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);
    let assertion_name = obj
        .get(TOOL_ASSERTION_NAME_FIELD)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);

    ToolCallExpectation {
        expected_failure,
        expected_failure_kind,
        assertion_name,
    }
}

pub(crate) fn strip_tool_call_expectation_metadata(arguments: Value) -> Value {
    let Value::Object(mut obj) = arguments else {
        return arguments;
    };
    for &key in TOOL_CALL_EXPECTATION_METADATA_FIELDS {
        obj.remove(key);
    }
    Value::Object(obj)
}

pub(crate) fn tool_failure_summary_from_events(events: &[SessionEvent], limit: usize) -> Value {
    let limit = limit.min(20);
    let mut expected_count = 0usize;
    let mut unexpected_count = 0usize;
    let mut expectation_mismatch_count = 0usize;
    let mut unexpected_success_count = 0usize;
    let mut recent_expected = Vec::new();
    let mut recent_unexpected = Vec::new();
    let mut recent_mismatches = Vec::new();
    let mut recent_unexpected_successes = Vec::new();

    for event in events
        .iter()
        .rev()
        .filter(|event| event.kind == "tool_call_finished")
    {
        match event
            .failure_expectation_result
            .as_deref()
            .unwrap_or_else(|| legacy_failure_expectation_result(event))
        {
            TOOL_EXPECTATION_RESULT_MATCHED => {
                expected_count += 1;
                if recent_expected.len() < limit {
                    recent_expected.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE => {
                unexpected_count += 1;
                if recent_unexpected.len() < limit {
                    recent_unexpected.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_MISMATCH => {
                expectation_mismatch_count += 1;
                if recent_mismatches.len() < limit {
                    recent_mismatches.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS => {
                unexpected_success_count += 1;
                if recent_unexpected_successes.len() < limit {
                    recent_unexpected_successes.push(tool_failure_event_summary(event));
                }
            }
            _ => {}
        }
    }

    json!({
        "expected_count": expected_count,
        "unexpected_count": unexpected_count,
        "expectation_mismatch_count": expectation_mismatch_count,
        "unexpected_success_count": unexpected_success_count,
        "recent_expected": recent_expected,
        "recent_unexpected": recent_unexpected,
        "recent_mismatches": recent_mismatches,
        "recent_unexpected_successes": recent_unexpected_successes,
    })
}

pub(super) fn actual_failure_kind_for_tool_result(
    output: &Value,
    error: Option<&str>,
    error_kind: Option<&str>,
) -> Option<String> {
    let structured_kind = output
        .get("failure_kind")
        .and_then(Value::as_str)
        .or_else(|| output.get("error_kind").and_then(Value::as_str))
        .or_else(|| error_kind.filter(|kind| *kind != "runtime_error"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);
    structured_kind
        .or_else(|| error.map(classify_error_message))
        .or_else(|| error_kind.map(bound_summary_string))
}

pub(super) fn classify_failure_expectation(
    success: bool,
    expectation: &ToolCallExpectation,
    actual_failure_kind: Option<&str>,
) -> &'static str {
    if expectation.expected_failure {
        if success {
            return TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS;
        }
        let Some(expected_kind) = expectation.expected_failure_kind.as_deref() else {
            return TOOL_EXPECTATION_RESULT_MATCHED;
        };
        if Some(expected_kind) == actual_failure_kind {
            TOOL_EXPECTATION_RESULT_MATCHED
        } else {
            TOOL_EXPECTATION_RESULT_MISMATCH
        }
    } else if success {
        TOOL_EXPECTATION_RESULT_NONE
    } else {
        TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE
    }
}

pub(super) fn classify_error_message(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("session_project_mismatch") {
        "session_project_mismatch"
    } else if lower.contains("unknown_session_id") {
        "unknown_session_id"
    } else if lower.contains("confirmation_required")
        || (lower.contains("confirm") && lower.contains("required"))
    {
        "confirmation_required"
    } else if lower.contains("invalid arguments") || lower.contains("missing field") {
        "invalid_arguments"
    } else if lower.contains("insufficient scope") || lower.contains("missing required scope") {
        "insufficient_scope"
    } else if lower.contains("policy_rejected") || lower.contains("policy rejected") {
        "policy_rejected"
    } else if lower.contains("job_not_found")
        || lower.contains("unknown job")
        || (lower.contains("job") && lower.contains("not found"))
    {
        "job_not_found"
    } else {
        "runtime_error"
    };
    kind.to_string()
}

pub(super) fn sanitize_failure_expectation_result(value: &str) -> String {
    match value {
        TOOL_EXPECTATION_RESULT_MATCHED
        | TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE
        | TOOL_EXPECTATION_RESULT_MISMATCH
        | TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS
        | TOOL_EXPECTATION_RESULT_NONE => value.to_string(),
        _ => TOOL_EXPECTATION_RESULT_NONE.to_string(),
    }
}

pub(super) fn legacy_failure_expectation_result(event: &SessionEvent) -> &'static str {
    match event.status.as_deref() {
        Some("failed") => TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
        _ => TOOL_EXPECTATION_RESULT_NONE,
    }
}

pub(super) fn tool_failure_event_summary(event: &SessionEvent) -> Value {
    let success = event.status.as_deref() == Some("succeeded");
    json!({
        "event_id": event.event_id.clone(),
        "tool_name": event.tool_name.clone(),
        "project": event.resolved_project.as_ref().or(event.project.as_ref()).cloned(),
        "assertion_name": event.assertion_name.clone(),
        "expected_failure_kind": event.expected_failure_kind.clone(),
        "actual_failure_kind": event.actual_failure_kind.clone(),
        "status": event.status.clone(),
        "success": success,
        "created_at": event.timestamp,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionToolClassification {
    pub(crate) risk_class: &'static str,
    pub(crate) read_like: bool,
    pub(crate) write_like: bool,
    pub(crate) shell_like: bool,
    pub(crate) git_like: bool,
    pub(crate) change_summary_like: bool,
}

impl SessionToolClassification {
    pub(crate) fn for_tool(tool_name: &str) -> Self {
        Self {
            risk_class: runtime_tool_session_risk_class(tool_name),
            read_like: runtime_tool_is_read_like(tool_name),
            write_like: runtime_tool_is_write_like(tool_name),
            shell_like: runtime_tool_is_shell_like(tool_name),
            git_like: runtime_tool_is_git_like(tool_name),
            change_summary_like: runtime_tool_is_change_summary_like(tool_name),
        }
    }
}

pub(crate) fn changed_paths_for_tool(tool_name: &str, arguments: &Value) -> Vec<String> {
    let metadata = runtime_tool_metadata(tool_name);
    if metadata.risk != ToolRisk::ProjectWrite {
        return Vec::new();
    }
    let Some(obj) = arguments.as_object() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    match metadata.path_hint {
        ToolPathHint::SinglePath => {
            if let Some(path) = obj.get("path").and_then(Value::as_str) {
                push_path(&mut paths, path);
            }
        }
        ToolPathHint::PathList => {
            if let Some(values) = obj.get("paths").and_then(Value::as_array) {
                for path in values.iter().filter_map(Value::as_str) {
                    push_path(&mut paths, path);
                }
            }
            if let Some(changes) = obj.get("changes").and_then(Value::as_array) {
                for change in changes.iter().filter_map(Value::as_object) {
                    for key in ["path", "to_path"] {
                        if let Some(path) = change.get(key).and_then(Value::as_str) {
                            push_path(&mut paths, path);
                        }
                    }
                }
            }
        }
        ToolPathHint::Artifact => {
            for key in ["path", "output_path", "target_path"] {
                if let Some(path) = obj.get(key).and_then(Value::as_str) {
                    push_path(&mut paths, path);
                }
            }
        }
        ToolPathHint::Patch | ToolPathHint::None => {}
    }
    paths
}

pub(super) fn push_path(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() || paths.iter().any(|p| p == path) {
        return;
    }
    paths.push(path.to_string());
}

/// Compute whether a tool call should contribute to `diff_review_count`.
///
/// Only reads a safe boolean (`include_diff`) from arguments for `show_changes`.
/// Does not store raw input, command text, or diff content.
pub(super) fn diff_review_like_for_tool(tool_name: &str, arguments: &Value) -> bool {
    match tool_name {
        "git_diff" | "git_diff_summary" | "git_diff_hunks" => true,
        "show_changes" => arguments
            .get("include_diff")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

pub(super) fn extract_job_id(output: &Value) -> Option<String> {
    output
        .as_object()
        .and_then(|obj| obj.get("job_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
pub(super) fn validation_output_summary_for_tool_result(
    tool_name: &str,
    output: &Value,
) -> Option<Value> {
    if !is_cargo_validation_tool(tool_name) {
        return None;
    }
    let stdout_value = output.get("stdout_tail")?;
    let stderr_value = output.get("stderr_tail")?;
    let stdout = stdout_value.as_str()?;
    let stderr = stderr_value.as_str()?;
    let stdout_excerpt = validation_excerpt(stdout);
    let stderr_excerpt = validation_excerpt(stderr);
    let stdout_truncated = output
        .get("stdout_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stdout_excerpt.filtered;
    let stderr_truncated = output
        .get("stderr_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stderr_excerpt.filtered;

    let mut summary = json!({
        "tool_name": tool_name,
        "stdout_tail_excerpt": stdout_excerpt.text,
        "stderr_tail_excerpt": stderr_excerpt.text,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "max_excerpt_chars": MAX_VALIDATION_EXCERPT_CHARS,
    });
    if tool_name == "cargo_test" {
        summary["tests_detected"] = cargo_test_tests_detected(output);
        summary["tests_run_count"] = cargo_test_tests_run_count(output);
        summary["zero_tests_run"] = cargo_test_zero_tests_run(output);
    }
    Some(summary)
}

pub(super) fn sanitize_persisted_validation_output_summary(
    tool_name: &str,
    value: &Value,
) -> Option<Value> {
    if !is_cargo_validation_tool(tool_name) {
        return None;
    }
    let object = value.as_object()?;
    let stdout = object
        .get("stdout_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stderr = object
        .get("stderr_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stdout_excerpt = validation_excerpt(stdout);
    let stderr_excerpt = validation_excerpt(stderr);
    let stdout_truncated = object
        .get("stdout_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stdout_excerpt.filtered;
    let stderr_truncated = object
        .get("stderr_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stderr_excerpt.filtered;

    let mut summary = json!({
        "tool_name": tool_name,
        "stdout_tail_excerpt": stdout_excerpt.text,
        "stderr_tail_excerpt": stderr_excerpt.text,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "max_excerpt_chars": MAX_VALIDATION_EXCERPT_CHARS,
    });
    if tool_name == "cargo_test" {
        summary["tests_detected"] = persisted_cargo_test_tests_detected(object);
        summary["tests_run_count"] = persisted_cargo_test_tests_run_count(object);
        summary["zero_tests_run"] = persisted_cargo_test_zero_tests_run(object);
    }
    Some(summary)
}

pub(super) fn is_cargo_validation_tool(tool_name: &str) -> bool {
    runtime_tool_captures_validation_output(tool_name)
}

pub(super) fn cargo_test_tests_detected(output: &Value) -> Value {
    output
        .get("tests_detected")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

pub(super) fn cargo_test_tests_run_count(output: &Value) -> Value {
    output
        .get("tests_run_count")
        .and_then(Value::as_u64)
        .map_or(Value::Null, |count| json!(count))
}

pub(super) fn cargo_test_zero_tests_run(output: &Value) -> Value {
    output
        .get("zero_tests_run")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

pub(super) fn persisted_cargo_test_tests_detected(
    object: &serde_json::Map<String, Value>,
) -> Value {
    object
        .get("tests_detected")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

pub(super) fn persisted_cargo_test_tests_run_count(
    object: &serde_json::Map<String, Value>,
) -> Value {
    object
        .get("tests_run_count")
        .and_then(Value::as_u64)
        .map_or(Value::Null, |count| json!(count))
}

pub(super) fn persisted_cargo_test_zero_tests_run(
    object: &serde_json::Map<String, Value>,
) -> Value {
    object
        .get("zero_tests_run")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}
