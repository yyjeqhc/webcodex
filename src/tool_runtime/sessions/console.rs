//! Human-facing, read-only Workflow Session projection for the host Console.
//!
//! This is deliberately a projection of the existing bounded Session ledger.
//! It does not create observation truth, infer execution outcomes, or expose raw
//! arguments/output. Correlation is presentation-only via `call_id`.

use serde::Serialize;
use std::collections::HashMap;

use super::super::helpers::is_safe_job_id;
use super::events::normalize_observed_project_path;
use super::model::{SessionEvent, SessionMessageKind, SessionRecord};
use super::util::{bound_chars, looks_like_secret_string};

pub(crate) const DEFAULT_CONSOLE_SESSION_LIST_LIMIT: usize = 20;
pub(crate) const MAX_CONSOLE_SESSION_LIST_LIMIT: usize = 50;
pub(crate) const DEFAULT_CONSOLE_ACTIVITY_LIMIT: usize = 100;
pub(crate) const MAX_CONSOLE_ACTIVITY_LIMIT: usize = 200;
const MAX_CONSOLE_TEXT_CHARS: usize = 240;
const MAX_CONSOLE_PATHS_PER_ITEM: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowSessionConsoleList {
    pub(crate) sessions: Vec<WorkflowSessionConsoleListItem>,
    pub(crate) total: usize,
    pub(crate) returned: usize,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowSessionConsoleListItem {
    pub(crate) session_id: String,
    pub(crate) title: String,
    pub(crate) lifecycle: String,
    pub(crate) mode: String,
    pub(crate) updated_at: i64,
    pub(crate) running_call: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowSessionConsoleDetail {
    pub(crate) session_id: String,
    pub(crate) title: String,
    pub(crate) lifecycle: String,
    pub(crate) mode: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) running_call: bool,
    pub(crate) activity: Vec<WorkflowSessionConsoleActivity>,
    pub(crate) activity_total: usize,
    pub(crate) activity_returned: usize,
    pub(crate) activity_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowSessionConsoleActivity {
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool: Option<String>,
    pub(crate) state: String,
    pub(crate) started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finished_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) paths: Vec<String>,
}

#[derive(Clone, Copy)]
struct Interaction<'a> {
    start: Option<&'a SessionEvent>,
    finish: Option<&'a SessionEvent>,
    sequence: usize,
}

struct OrderedActivity {
    activity: WorkflowSessionConsoleActivity,
    ledger_sequence: Option<usize>,
    fallback_sequence: usize,
}

pub(super) fn normalize_console_session_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_CONSOLE_SESSION_LIST_LIMIT)
        .clamp(1, MAX_CONSOLE_SESSION_LIST_LIMIT)
}

pub(super) fn normalize_console_activity_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_CONSOLE_ACTIVITY_LIMIT)
        .clamp(1, MAX_CONSOLE_ACTIVITY_LIMIT)
}

pub(super) fn build_list_item(record: &SessionRecord) -> WorkflowSessionConsoleListItem {
    WorkflowSessionConsoleListItem {
        session_id: record.session_id.clone(),
        title: console_title(record.title.as_deref()),
        lifecycle: record.lifecycle.as_str().to_string(),
        mode: record.mode.as_str().to_string(),
        updated_at: record.updated_at,
        running_call: build_interactions(record)
            .into_iter()
            .any(|interaction| interaction.finish.is_none() && interaction.start.is_some()),
    }
}

pub(super) fn build_detail(
    record: &SessionRecord,
    project: &str,
    limit: usize,
) -> WorkflowSessionConsoleDetail {
    let interactions = build_interactions(record);
    let running_call = interactions
        .iter()
        .any(|interaction| interaction.finish.is_none() && interaction.start.is_some());
    let mut ordered_activity = interactions
        .into_iter()
        .map(|interaction| OrderedActivity {
            activity: activity_from_interaction(interaction, project),
            ledger_sequence: Some(interaction.sequence),
            fallback_sequence: interaction.sequence,
        })
        .collect::<Vec<_>>();
    for (fallback_sequence, message) in record
        .messages
        .iter()
        .filter(|message| message.kind == SessionMessageKind::Progress)
        .enumerate()
    {
        ordered_activity.push(OrderedActivity {
            activity: WorkflowSessionConsoleActivity {
                kind: "Progress".to_string(),
                tool: None,
                state: "info".to_string(),
                started_at: message.created_at,
                finished_at: None,
                duration_ms: None,
                exit_code: None,
                job_id: None,
                summary: Some(console_safe_text(&message.message, MAX_CONSOLE_TEXT_CHARS)),
                paths: Vec::new(),
            },
            ledger_sequence: None,
            fallback_sequence,
        });
    }
    sort_ordered_activity(&mut ordered_activity);
    let activity_total = ordered_activity.len();
    if ordered_activity.len() > limit {
        ordered_activity.drain(0..ordered_activity.len() - limit);
    }
    let activity = ordered_activity
        .into_iter()
        .map(|entry| entry.activity)
        .collect::<Vec<_>>();
    let activity_returned = activity.len();
    WorkflowSessionConsoleDetail {
        session_id: record.session_id.clone(),
        title: console_title(record.title.as_deref()),
        lifecycle: record.lifecycle.as_str().to_string(),
        mode: record.mode.as_str().to_string(),
        created_at: record.created_at,
        updated_at: record.updated_at,
        running_call,
        activity,
        activity_total,
        activity_returned,
        activity_truncated: activity_total > activity_returned,
    }
}

fn sort_ordered_activity(activity: &mut [OrderedActivity]) {
    activity.sort_by(|left, right| {
        left.activity
            .started_at
            .cmp(&right.activity.started_at)
            .then_with(|| match (left.ledger_sequence, right.ledger_sequence) {
                (Some(left), Some(right)) => left.cmp(&right),
                // Progress messages have no sequence comparable to Session events.
                // Keep same-second tool activity in authoritative ledger order and
                // place progress after it without claiming a finer causal order.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.fallback_sequence.cmp(&right.fallback_sequence),
            })
    });
}

fn build_interactions(record: &SessionRecord) -> Vec<Interaction<'_>> {
    let mut correlated: HashMap<&str, Interaction<'_>> = HashMap::new();
    let mut legacy_finished = Vec::new();

    for (sequence, event) in record.events.iter().enumerate() {
        let event = event.as_ref();
        match event.kind.as_str() {
            "tool_call_started" => {
                let Some(call_id) = event.call_id.as_deref() else {
                    // A legacy start cannot safely be labelled "running": its
                    // matching finish predates call correlation. Do not guess.
                    continue;
                };
                correlated.entry(call_id).or_insert(Interaction {
                    start: Some(event),
                    finish: None,
                    sequence,
                });
            }
            "tool_call_finished" => {
                let Some(call_id) = event.call_id.as_deref() else {
                    legacy_finished.push(Interaction {
                        start: None,
                        finish: Some(event),
                        sequence,
                    });
                    continue;
                };
                correlated
                    .entry(call_id)
                    .and_modify(|interaction| interaction.finish = Some(event))
                    .or_insert(Interaction {
                        start: None,
                        finish: Some(event),
                        sequence,
                    });
            }
            _ => {}
        }
    }

    let mut interactions: Vec<_> = correlated.into_values().collect();
    interactions.extend(legacy_finished);
    interactions.sort_by_key(|interaction| interaction.sequence);
    interactions
}

fn activity_from_interaction(
    interaction: Interaction<'_>,
    project: &str,
) -> WorkflowSessionConsoleActivity {
    let evidence = interaction
        .finish
        .or(interaction.start)
        .expect("interaction evidence");
    let started_at = interaction
        .start
        .and_then(|event| event.started_at)
        .or_else(|| interaction.finish.and_then(|event| event.started_at))
        .unwrap_or(evidence.timestamp);
    let finish = interaction.finish;
    let same_project = interaction
        .start
        .into_iter()
        .chain(finish)
        .all(|event| event_is_project_safe(event, project));
    let state = console_activity_state(interaction.start, finish);
    let paths = if same_project {
        safe_paths(interaction.start, finish)
    } else {
        Vec::new()
    };
    let summary = if same_project {
        console_interaction_summary(interaction.start, finish, &paths)
    } else {
        Some("cross-project details omitted".to_string())
    };
    WorkflowSessionConsoleActivity {
        kind: semantic_kind(evidence).to_string(),
        tool: Some(bound_chars(&evidence.tool_name, 80)),
        state,
        started_at,
        finished_at: finish.and_then(|event| event.finished_at),
        duration_ms: finish.and_then(|event| event.duration_ms),
        exit_code: finish.and_then(|event| event.exit_code),
        job_id: same_project
            .then(|| {
                finish
                    .and_then(|event| event.job_id.as_deref())
                    .and_then(safe_job_id)
            })
            .flatten(),
        summary,
        paths,
    }
}

fn console_activity_state(start: Option<&SessionEvent>, finish: Option<&SessionEvent>) -> String {
    let Some(finish) = finish else {
        return if start.is_some() {
            "running"
        } else {
            "completed"
        }
        .to_string();
    };
    if let Some(execution_state) = console_execution_state(finish) {
        match execution_state {
            "queued" => return "queued".to_string(),
            "running" | "started" => return "running".to_string(),
            "outcome_unknown" => return "outcome_unknown".to_string(),
            "timed_out" => return "timed_out".to_string(),
            "cancelled" => return "cancelled".to_string(),
            "not_started" => return "not_started".to_string(),
            // Completion is execution truth, but success/failure remains the
            // ToolResult outcome already retained on the Session event.
            "completed" => {}
            _ => unreachable!("console_execution_state returns a closed allowlist"),
        }
    }
    match finish.status.as_deref() {
        Some("succeeded") => "succeeded",
        Some("failed") => "failed",
        Some(_) => "completed",
        None => "completed",
    }
    .to_string()
}

fn console_execution_state(event: &SessionEvent) -> Option<&str> {
    event
        .effect_evidence
        .as_ref()
        .and_then(|evidence| evidence.execution_state.as_deref())
        .or_else(|| {
            event
                .validation_output_summary
                .as_ref()
                .and_then(|summary| summary.get("execution_state"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::trim)
        .filter(|state| {
            matches!(
                *state,
                "queued"
                    | "running"
                    | "started"
                    | "outcome_unknown"
                    | "completed"
                    | "cancelled"
                    | "timed_out"
                    | "not_started"
            )
        })
}

fn event_is_project_safe(event: &SessionEvent, project: &str) -> bool {
    event
        .resolved_project
        .as_deref()
        .or(event.project.as_deref())
        .is_none_or(|event_project| event_project == project)
}

fn safe_paths(start: Option<&SessionEvent>, finish: Option<&SessionEvent>) -> Vec<String> {
    let mut paths = Vec::new();
    for raw in start.into_iter().chain(finish).flat_map(|event| {
        event
            .observed_paths
            .iter()
            .chain(event.changed_paths.iter())
    }) {
        let Some(path) = normalize_observed_project_path(raw) else {
            continue;
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
        if paths.len() >= MAX_CONSOLE_PATHS_PER_ITEM {
            break;
        }
    }
    paths
}

fn console_interaction_summary(
    start: Option<&SessionEvent>,
    finish: Option<&SessionEvent>,
    paths: &[String],
) -> Option<String> {
    let evidence = finish.or(start)?;
    let mut parts = Vec::new();
    if let Some(input) = start.and_then(|event| event.input_summary.as_ref()) {
        if let Some(count) = input_array_count(input, "items") {
            parts.push(format!("{count} items"));
        }
        if let Some(count) = input_array_count(input, "queries") {
            parts.push(format!("{count} searches"));
        }
        if let Some(count) = input_array_count(input, "changes") {
            parts.push(format!("{count} changes"));
        }
        if let Some(count) = input_array_count(input, "packages") {
            parts.push(format!("{count} package scopes"));
        }
    }
    if !paths.is_empty() {
        parts.push(format!(
            "{} path{}",
            paths.len(),
            if paths.len() == 1 { "" } else { "s" }
        ));
    }
    if let Some(validation) = finish.and_then(|event| event.validation_output_summary.as_ref()) {
        if let Some(count) = validation
            .get("tests_run_count")
            .and_then(|value| value.as_u64())
        {
            parts.push(format!("{count} tests"));
        } else if validation
            .get("zero_tests_run")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            parts.push("0 tests".to_string());
        }
        if let Some(state) = validation
            .get("execution_state")
            .and_then(|value| value.as_str())
            .and_then(safe_atom)
        {
            parts.push(state);
        }
        let output_lines = validation
            .get("stdout_lines")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            .saturating_add(
                validation
                    .get("stderr_lines")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
            );
        if output_lines > 0 {
            parts.push(format!("{output_lines} output lines (content omitted)"));
        }
    }
    if finish
        .and_then(|event| event.job_id.as_deref())
        .and_then(safe_job_id)
        .is_some()
    {
        parts.push("job-backed".to_string());
    }
    if finish.and_then(|event| event.exit_code).is_some() {
        parts.push(format!(
            "exit {}",
            finish.and_then(|event| event.exit_code).unwrap()
        ));
    }
    if parts.is_empty() && interaction_is_progress_metadata(evidence) {
        parts.push("session metadata".to_string());
    }
    (!parts.is_empty()).then(|| bound_chars(&parts.join(" · "), MAX_CONSOLE_TEXT_CHARS))
}

fn input_array_count(value: &serde_json::Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(|value| value.as_array())
        .map(Vec::len)
        .filter(|count| *count > 0)
}

fn safe_atom(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    .then(|| value.to_string())
}

fn safe_job_id(value: &str) -> Option<String> {
    let value = value.trim();
    is_safe_job_id(value).then(|| value.to_string())
}

fn console_title(title: Option<&str>) -> String {
    title
        .map(|title| console_safe_text(title, MAX_CONSOLE_TEXT_CHARS))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Untitled workflow session".to_string())
}

fn console_safe_text(value: &str, max_chars: usize) -> String {
    if looks_like_secret_string(value) || contains_secret_assignment(value) {
        return "[redacted]".to_string();
    }
    let sanitized = value
        .split_whitespace()
        .map(|part| {
            if looks_like_private_path(part) {
                "[private path]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    bound_chars(&sanitized, max_chars)
}

fn contains_secret_assignment(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token=",
        "secret=",
        "password=",
        "authorization=",
        "authorization:",
        "api_key=",
        "apikey=",
        "access_key=",
        "private_key=",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn looks_like_private_path(value: &str) -> bool {
    let token = trim_path_wrappers(value);
    if looks_like_absolute_path(token) {
        return true;
    }
    token
        .rsplit_once('=')
        .map(|(_, candidate)| trim_path_wrappers(candidate))
        .is_some_and(looks_like_absolute_path)
}

fn trim_path_wrappers(value: &str) -> &str {
    value.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
        )
    })
}

fn looks_like_absolute_path(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("file://") || value.starts_with('/') || value.starts_with("\\\\") {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && bytes
            .get(2)
            .is_some_and(|byte| matches!(*byte, b'\\' | b'/'))
}

fn semantic_kind(event: &SessionEvent) -> &'static str {
    match event.tool_name.as_str() {
        "read_file"
        | "read_files"
        | "list_project_files"
        | "list_project_tracked_files"
        | "project_overview"
        | "read_project_artifact"
        | "read_project_artifact_metadata" => "Read",
        "search_project_text" | "search_project_texts" => "Searched",
        "lsp_status"
        | "document_symbols"
        | "document_diagnostics"
        | "hover"
        | "workspace_symbols"
        | "goto_definition"
        | "find_references"
        | "call_hierarchy" => "Navigated",
        "apply_text_edits"
        | "apply_patch"
        | "apply_patch_checked"
        | "write_project_file"
        | "delete_project_files"
        | "git_restore_paths"
        | "discard_untracked"
        | "workspace_checkpoint_restore" => "Edited",
        "git_status"
        | "git_diff"
        | "git_diff_hunks"
        | "git_log"
        | "show_changes"
        | "workspace_hygiene_check"
        | "finish_coding_task" => "Reviewed",
        "cargo_test" | "cargo_check" | "cargo_fmt" | "go_test" | "validate_patch" => "Tested",
        "run_process"
        | "run_script"
        | "run_shell"
        | "run_job"
        | "open_session_shell"
        | "session_shell_exec"
        | "session_shell_status"
        | "close_session_shell"
        | "job_status"
        | "job_log"
        | "observe_jobs"
        | "stop_job" => "Ran",
        _ if event.write_like => "Edited",
        _ if event.git_like || event.change_summary_like => "Reviewed",
        _ if event.shell_like => "Ran",
        _ if event.read_like => "Read",
        _ => "Used",
    }
}

fn interaction_is_progress_metadata(event: &SessionEvent) -> bool {
    matches!(
        event.tool_name.as_str(),
        "post_session_message" | "resolve_session_message" | "update_session_context"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordered(
        kind: &str,
        ledger_sequence: Option<usize>,
        fallback_sequence: usize,
    ) -> OrderedActivity {
        OrderedActivity {
            activity: WorkflowSessionConsoleActivity {
                kind: kind.to_string(),
                tool: None,
                state: "succeeded".to_string(),
                started_at: 1_234,
                finished_at: Some(1_234),
                duration_ms: Some(0),
                exit_code: None,
                job_id: None,
                summary: None,
                paths: Vec::new(),
            },
            ledger_sequence,
            fallback_sequence,
        }
    }

    #[test]
    fn same_second_tool_activity_uses_ledger_sequence_and_progress_is_conservative() {
        let mut activity = vec![
            ordered("Read", Some(4), 4),
            ordered("Progress", None, 1),
            ordered("Searched", Some(1), 1),
            ordered("Progress later", None, 2),
        ];
        sort_ordered_activity(&mut activity);
        let kinds = activity
            .iter()
            .map(|entry| entry.activity.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec!["Searched", "Read", "Progress", "Progress later"]
        );
    }
}
