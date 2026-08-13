//! Tool-call event helpers: classification, expectations, validation excerpts, path extraction.
use super::super::metadata::{ToolPathHint, ToolRisk};
use super::super::tool_definition::{
    runtime_tool_captures_validation_output, runtime_tool_category,
    runtime_tool_is_change_summary_like, runtime_tool_is_git_like, runtime_tool_is_read_like,
    runtime_tool_is_shell_like, runtime_tool_is_write_like, runtime_tool_metadata,
    runtime_tool_session_risk_class, TOOL_CATEGORY_LSP,
};
use crate::lsp_bridge::{
    CallHierarchyResult, DocumentDiagnosticsResult, DocumentSymbolsResult, HoverResult,
    LocationsResult, WorkspaceSymbolsResult,
};
use serde_json::{json, Value};

use super::model::{
    PersistentShellEventEvidence, SessionEvent, ToolCallExpectation, ToolCallRecorderMetadata,
    MAX_OBSERVED_PATHS_PER_EVENT, MAX_VALIDATION_EXCERPT_CHARS, SESSION_ID_PREFIX,
    TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_EXPECTATION_METADATA_FIELDS,
    TOOL_EXPECTATION_RESULT_MATCHED, TOOL_EXPECTATION_RESULT_MISMATCH,
    TOOL_EXPECTATION_RESULT_NONE, TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
    TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS, TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD,
};
use super::util::redact_and_bound_value;
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

/// Internal exploration classification derived from the canonical
/// ToolDefinition category/path metadata. This is deliberately not exposed in
/// the public tool metadata surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorationToolKind {
    Read,
    Search,
    Navigation,
}

pub(crate) const EXPLORATION_TOOL_NAMES: &[&str] = &[
    "read_file",
    "read_files",
    "search_project_text",
    "search_project_texts",
    "document_symbols",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
    "goto_definition",
    "find_references",
    "call_hierarchy",
];

pub(crate) fn exploration_tool_kind(tool_name: &str) -> Option<ExplorationToolKind> {
    if !EXPLORATION_TOOL_NAMES.contains(&tool_name) {
        return None;
    }
    match tool_name {
        "read_file" | "read_files" => Some(ExplorationToolKind::Read),
        "search_project_text" | "search_project_texts" => Some(ExplorationToolKind::Search),
        _ if runtime_tool_category(tool_name) == TOOL_CATEGORY_LSP => {
            Some(ExplorationToolKind::Navigation)
        }
        _ => None,
    }
}

/// Extract only explicit input paths for tools that establish focused
/// exploration evidence. Search roots are intentionally excluded: search
/// evidence comes from successful structured result records instead.
pub(crate) fn observed_input_paths_for_tool(tool_name: &str, arguments: &Value) -> Vec<String> {
    let Some(kind) = exploration_tool_kind(tool_name) else {
        return Vec::new();
    };
    if kind == ExplorationToolKind::Search {
        return Vec::new();
    }

    let mut paths = Vec::new();
    if tool_name == "read_files" {
        if let Some(items) = arguments.get("items").and_then(Value::as_array) {
            for item in items.iter().filter_map(Value::as_object) {
                if let Some(path) = item.get("path").and_then(Value::as_str) {
                    push_observed_path(&mut paths, path);
                }
            }
        }
        return paths;
    }

    let metadata = runtime_tool_metadata(tool_name);
    if metadata.path_hint != ToolPathHint::SinglePath {
        return Vec::new();
    }
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        push_observed_path(&mut paths, path);
    }
    paths
}

/// Build the bounded audit input retained on a session event while ensuring
/// exploration queries and shell command summaries never enter the ledger,
/// even when an internal caller bypasses `ToolCall::session_log_arguments`.
pub(crate) fn session_input_summary_for_tool(tool_name: &str, arguments: &Value) -> Value {
    let mut summary = redact_and_bound_value(arguments);
    let Some(object) = summary.as_object_mut() else {
        return summary;
    };
    match tool_name {
        "search_project_text" => {
            object.remove("pattern");
        }
        "search_project_texts" => {
            if let Some(queries) = object.get_mut("queries").and_then(Value::as_array_mut) {
                for query in queries.iter_mut().filter_map(Value::as_object_mut) {
                    query.remove("pattern");
                }
            }
        }
        "workspace_symbols" => {
            object.remove("query");
        }
        "run_process" => {
            object.remove("executable");
            object.remove("args");
            object.remove("stdin");
            object.remove("process_summary");
        }
        "run_script" => {
            object.remove("script");
            object.remove("args");
            object.remove("stdin");
            object.remove("script_summary");
        }
        "run_shell" | "run_job" | "session_shell_exec" => {
            object.remove("command");
            object.remove("command_summary");
        }
        "observe_jobs" => {
            if let Some(items) = object.get_mut("items").and_then(Value::as_array_mut) {
                for item in items.iter_mut().filter_map(Value::as_object_mut) {
                    item.remove("after_observation_token");
                }
            }
        }
        _ => {}
    }
    summary
}

pub(crate) fn persistent_shell_event_evidence_for_tool_result(
    tool_name: &str,
    output: &Value,
) -> Option<PersistentShellEventEvidence> {
    let action = persistent_shell_action(tool_name)?;
    sanitize_persistent_shell_event_evidence(
        tool_name,
        PersistentShellEventEvidence {
            action: action.to_string(),
            shell_id: output
                .get("shell_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            shell_state: output
                .get("shell_state")
                .and_then(Value::as_str)
                .map(str::to_string),
            execution_state: output
                .get("execution_state")
                .and_then(Value::as_str)
                .map(str::to_string),
            error_code: output
                .get("error_code")
                .and_then(Value::as_str)
                .map(str::to_string),
            command_started: output.get("command_started").and_then(Value::as_bool),
            command_completed: output.get("command_completed").and_then(Value::as_bool),
            already_closed: output.get("already_closed").and_then(Value::as_bool),
        },
    )
}

pub(crate) fn sanitize_persistent_shell_event_evidence(
    tool_name: &str,
    mut evidence: PersistentShellEventEvidence,
) -> Option<PersistentShellEventEvidence> {
    evidence.action = persistent_shell_action(tool_name)?.to_string();
    evidence.shell_id = evidence.shell_id.filter(|value| {
        value.starts_with("wc_shell_")
            && value.len() <= 96
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    });
    evidence.shell_state = evidence.shell_state.and_then(sanitize_shell_evidence_atom);
    evidence.execution_state = evidence
        .execution_state
        .and_then(sanitize_shell_evidence_atom);
    evidence.error_code = evidence.error_code.and_then(sanitize_shell_evidence_atom);
    Some(evidence)
}

fn persistent_shell_action(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "open_session_shell" => Some("open"),
        "session_shell_exec" => Some("exec"),
        "session_shell_status" => Some("status"),
        "close_session_shell" => Some("close"),
        "close_session" => Some("close"),
        _ => None,
    }
}

fn sanitize_shell_evidence_atom(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        None
    } else {
        Some(value.to_string())
    }
}

/// Add paths from a successful structured tool result. Every branch follows a
/// known output schema; this never recursively searches arbitrary JSON for
/// fields named `path`.
pub(crate) fn observed_paths_for_successful_result(
    tool_name: &str,
    input_paths: Vec<String>,
    output: &Value,
) -> Vec<String> {
    let Some(kind) = exploration_tool_kind(tool_name) else {
        return Vec::new();
    };
    let mut paths = sanitize_observed_paths(input_paths);
    match kind {
        ExplorationToolKind::Read => {}
        ExplorationToolKind::Search => {
            let search_outputs: Vec<&Value> = if tool_name == "search_project_texts" {
                output
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|item| item.get("success").and_then(Value::as_bool) == Some(true))
                    .filter_map(|item| item.get("output"))
                    .collect()
            } else {
                vec![output]
            };
            for search_output in search_outputs {
                for key in ["matches", "files"] {
                    for record in search_output
                        .get(key)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(path) = record.get("path").and_then(Value::as_str) {
                            push_observed_path(&mut paths, path);
                        }
                    }
                }
            }
        }
        ExplorationToolKind::Navigation => {
            push_lsp_result_paths(tool_name, output, &mut paths);
        }
    }
    paths
}

fn push_lsp_result_paths(tool_name: &str, output: &Value, paths: &mut Vec<String>) {
    match tool_name {
        "document_symbols" => {
            if let Ok(result) = serde_json::from_value::<DocumentSymbolsResult>(output.clone()) {
                push_observed_path(paths, &result.path);
            }
        }
        "document_diagnostics" => {
            if let Ok(result) = serde_json::from_value::<DocumentDiagnosticsResult>(output.clone())
            {
                push_observed_path(paths, &result.path);
            }
        }
        "hover" => {
            if let Ok(result) = serde_json::from_value::<HoverResult>(output.clone()) {
                push_observed_path(paths, &result.path);
            }
        }
        "workspace_symbols" => {
            if let Ok(result) = serde_json::from_value::<WorkspaceSymbolsResult>(output.clone()) {
                for symbol in result.symbols {
                    push_observed_path(paths, &symbol.path);
                }
            }
        }
        "goto_definition" | "find_references" => {
            if let Ok(result) = serde_json::from_value::<LocationsResult>(output.clone()) {
                push_observed_path(paths, &result.path);
                for location in result.locations {
                    push_observed_path(paths, &location.path);
                }
            }
        }
        "call_hierarchy" => {
            if let Ok(result) = serde_json::from_value::<CallHierarchyResult>(output.clone()) {
                push_observed_path(paths, &result.path);
                for root in result.roots {
                    push_observed_path(paths, &root.path);
                }
                for edge in result.edges {
                    push_observed_path(paths, &edge.from.path);
                    push_observed_path(paths, &edge.to.path);
                }
            }
        }
        _ => {}
    }
}

/// Normalize one untrusted path into the only representation allowed in
/// exploration evidence. Validation is lexical and never touches the
/// filesystem, resolves symlinks, or reveals the repository root.
pub(crate) fn normalize_observed_project_path(path: &str) -> Option<String> {
    const MAX_OBSERVED_PATH_BYTES: usize = 512;

    let path = path.trim();
    if path.is_empty()
        || path.len() > MAX_OBSERVED_PATH_BYTES
        || path.starts_with('\\')
        || path.chars().any(char::is_control)
    {
        return None;
    }
    if starts_with_uri_scheme(path)
        || crate::validation_bridge::validate_project_relative_path(path).is_err()
    {
        return None;
    }
    let normalized = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty()
        || normalized.len() > MAX_OBSERVED_PATH_BYTES
        || crate::validation_bridge::validate_project_relative_path(&normalized).is_err()
    {
        return None;
    }
    Some(normalized)
}

fn starts_with_uri_scheme(path: &str) -> bool {
    let Some((scheme, _rest)) = path.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

pub(crate) fn sanitize_observed_paths<I>(paths: I) -> Vec<String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut sanitized = Vec::new();
    for path in paths {
        push_observed_path(&mut sanitized, path.as_ref());
    }
    sanitized
}

fn push_observed_path(paths: &mut Vec<String>, path: &str) {
    if paths.len() >= MAX_OBSERVED_PATHS_PER_EVENT {
        return;
    }
    let Some(path) = normalize_observed_project_path(path) else {
        return;
    };
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
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
pub(crate) fn validation_output_summary_for_tool_result(
    tool_name: &str,
    output: &Value,
) -> Option<Value> {
    if !is_cargo_validation_tool(tool_name)
        && !matches!(
            tool_name,
            "run_process" | "run_script" | "run_shell" | "run_job"
        )
    {
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
        "stdout_lines": output.get("stdout_lines").and_then(Value::as_u64),
        "stderr_lines": output.get("stderr_lines").and_then(Value::as_u64),
        "purpose": output.get("purpose").cloned().unwrap_or(Value::Null),
        "command_summary": output
            .get("command_summary")
            .or_else(|| output.get("process_summary"))
            .or_else(|| output.get("script_summary"))
            .cloned()
            .unwrap_or(Value::Null),
        "cwd": output.get("cwd").cloned().unwrap_or(Value::Null),
        "shell": output
            .get("shell")
            .cloned()
            .unwrap_or_else(|| {
                if tool_name == "run_process" {
                    Value::String("direct_argv".to_string())
                } else if tool_name == "run_script" {
                    output.get("language").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }),
        "executor": output.get("executor").cloned().unwrap_or(Value::Null),
        "execution_state": output.get("execution_state").cloned().unwrap_or(Value::Null),
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
    if !is_cargo_validation_tool(tool_name)
        && !matches!(
            tool_name,
            "run_process" | "run_script" | "run_shell" | "run_job"
        )
    {
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
        "stdout_lines": object.get("stdout_lines").and_then(Value::as_u64),
        "stderr_lines": object.get("stderr_lines").and_then(Value::as_u64),
        "purpose": object.get("purpose").and_then(Value::as_str),
        "command_summary": object.get("command_summary").and_then(Value::as_str),
        "cwd": object.get("cwd").and_then(Value::as_str),
        "shell": object.get("shell").and_then(Value::as_str),
        "executor": object.get("executor").and_then(Value::as_str),
        "execution_state": object.get("execution_state").and_then(Value::as_str),
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
