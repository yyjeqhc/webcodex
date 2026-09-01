//! Compact deterministic task handoff projection.
//!
//! `handoff_brief` is a pure, read-only view over snapshots that the calling
//! tool already obtained. It never reads the workspace, executes a command,
//! queries an Agent/Runner, refreshes activity, mutates a Workflow Session,
//! consumes guidance, or invokes an LLM.

use serde_json::{json, Value};
use std::collections::BTreeSet;

use super::continuation_feedback::EXPLORATION_CONTINUITY_ACTION;
use super::sessions::{
    normalize_observed_project_path, redact_and_bound_instruction, SessionSummary,
};

pub(crate) const HANDOFF_BRIEF_HARD_MAX_BYTES: usize = 8 * 1024;
pub(crate) const HANDOFF_INSTRUCTION_MAX_CHARS: usize = 600;
pub(crate) const HANDOFF_CHANGED_PATHS_MAX_ITEMS: usize = 12;
pub(crate) const HANDOFF_RECENT_FILES_MAX_ITEMS: usize = 8;
pub(crate) const HANDOFF_OPEN_FAILURES_MAX_ITEMS: usize = 5;
pub(crate) const HANDOFF_NEXT_ACTIONS_MAX_ITEMS: usize = 5;

const HANDOFF_FAILURE_NAME_MAX_CHARS: usize = 240;
const HANDOFF_BRANCH_MAX_CHARS: usize = 256;

/// Bounded snapshots gathered by a handoff/finish caller before invoking the
/// pure projection.
pub(crate) struct HandoffBriefInput<'a> {
    pub(crate) session_summary: &'a SessionSummary,
    pub(crate) continuation_feedback: &'a Value,
    pub(crate) workspace_requested: bool,
    pub(crate) workspace: Option<&'a Value>,
    pub(crate) validation_requested: bool,
    pub(crate) validation: Option<&'a Value>,
    pub(crate) jobs: Option<&'a Value>,
    /// The Workflow Session summary carries exact open-message counts. This
    /// flag lets callers report a stable gap if that guidance snapshot was not
    /// available instead of silently treating it as empty.
    pub(crate) guidance_available: bool,
    /// Optional existing deterministic action projection. Only fixed known
    /// templates are reused; arbitrary strings are never copied into the brief.
    pub(crate) existing_suggested_actions: Option<&'a Value>,
}

#[derive(Debug)]
struct WorkspaceProjection {
    value: Value,
    status: &'static str,
    dirty: Option<bool>,
    conflicted: Option<bool>,
}

#[derive(Debug)]
struct ValidationProjection {
    value: Value,
    status: &'static str,
}

#[derive(Debug)]
struct JobProjection {
    available: bool,
    blocking: Option<u64>,
    terminal_pending: Option<u64>,
    recovering: Option<u64>,
}

/// Build the shared `handoff_brief` object from already-obtained bounded
/// snapshots. The returned value is always at most
/// [`HANDOFF_BRIEF_HARD_MAX_BYTES`] when serialized as JSON.
pub(crate) fn build_handoff_brief(input: HandoffBriefInput<'_>) -> Value {
    let continuation = input.continuation_feedback;
    let attempt = continuation
        .get("attempt")
        .filter(|value| value.is_object());
    let continuation_status = continuation.get("status").and_then(Value::as_str);
    let continuation_available =
        attempt.is_some() && matches!(continuation_status, Some("available" | "not_applicable"));

    let root_instruction = instruction_projection(input.session_summary.title.as_deref());
    let latest_instruction = input
        .session_summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "task_instruction")
        .and_then(|event| event.instruction.as_deref());
    let latest_instruction = instruction_projection(latest_instruction);

    let workspace = project_workspace(input.workspace_requested, input.workspace);
    let mut validation = project_validation(input.validation_requested, input.validation);
    let jobs = project_jobs(input.jobs);

    let changes = bounded_path_list(
        attempt.and_then(|value| value.pointer("/changes/changed_paths")),
        attempt
            .and_then(|value| value.pointer("/changes/total_changed_paths"))
            .and_then(Value::as_u64),
        attempt
            .and_then(|value| value.pointer("/changes/truncated"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        HANDOFF_CHANGED_PATHS_MAX_ITEMS,
    );
    let recent_files = bounded_path_list(
        attempt.and_then(|value| value.pointer("/exploration/observed_paths")),
        attempt
            .and_then(|value| value.pointer("/exploration/total_observed_paths"))
            .and_then(Value::as_u64),
        attempt
            .and_then(|value| value.pointer("/exploration/truncated"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        HANDOFF_RECENT_FILES_MAX_ITEMS,
    );
    if input.validation_requested && matches!(validation.status, "passed" | "failed") {
        validation.value["open_failures"] = bounded_failure_list(attempt);
    }
    let meaningful_tool_calls = attempt
        .and_then(|value| value.pointer("/activity/meaningful_tool_calls"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let (open_guidance, open_risks, open_questions, open_todos) = if input.guidance_available {
        (
            Some(input.session_summary.messages.pending_guidance as u64),
            Some(input.session_summary.messages.open_risks as u64),
            Some(input.session_summary.messages.open_questions as u64),
            Some(input.session_summary.messages.open_todos as u64),
        )
    } else {
        (None, None, None, None)
    };

    let attempt_boundary_evicted = attempt.is_some_and(|value| {
        value
            .pointer("/boundary/reason_code")
            .and_then(Value::as_str)
            == Some("attempt_boundary_evicted")
            || value
                .pointer("/event_range/complete")
                .and_then(Value::as_bool)
                == Some(false)
            || value
                .pointer("/exploration/complete")
                .and_then(Value::as_bool)
                == Some(false)
    });

    let mut basis_reasons = BTreeSet::new();
    if !continuation_available {
        basis_reasons.insert("continuation_unavailable");
    }
    if attempt_boundary_evicted {
        basis_reasons.insert("attempt_boundary_evicted");
    }
    match workspace.status {
        "not_requested" => {
            basis_reasons.insert("workspace_not_requested");
        }
        "unavailable" => {
            basis_reasons.insert("workspace_unavailable");
        }
        _ => {}
    }
    match validation.status {
        "not_requested" => {
            basis_reasons.insert("validation_not_requested");
        }
        "unavailable" => {
            basis_reasons.insert("validation_unavailable");
        }
        _ => {}
    }
    if !jobs.available {
        basis_reasons.insert("job_summary_unavailable");
    }
    if !input.guidance_available {
        basis_reasons.insert("guidance_unavailable");
    }

    let progress_state = progress_state(
        input.session_summary,
        continuation_available,
        &workspace,
        &validation,
        &jobs,
        open_risks,
        input.guidance_available,
    );
    let changed_count = changes["returned"].as_u64().unwrap_or(0);
    let recent_count = recent_files["returned"].as_u64().unwrap_or(0);
    let next_actions = next_actions(
        &workspace,
        &validation,
        &jobs,
        open_guidance,
        open_risks,
        open_questions,
        open_todos,
        changed_count,
        recent_count,
        continuation,
        input.existing_suggested_actions,
    );

    let reason_codes = basis_reasons.into_iter().collect::<Vec<_>>();
    let mut brief = json!({
        "version": 1,
        "session": {
            "session_id": input.session_summary.session_id,
            "lifecycle": input.session_summary.lifecycle.as_str(),
            "mode": input.session_summary.mode.as_str(),
        },
        "task": {
            "root_instruction": root_instruction,
            "latest_instruction": latest_instruction,
        },
        "workspace": workspace.value,
        "progress": {
            "state": progress_state,
            "meaningful_tool_calls": meaningful_tool_calls,
            "changes": changes,
            "recent_files": recent_files,
        },
        "validation": validation.value,
        "attention": {
            "workspace_conflict": workspace.conflicted,
            "blocking_jobs": jobs.blocking,
            "terminal_pending_jobs": jobs.terminal_pending,
            "recovering_jobs": jobs.recovering,
            "open_guidance": open_guidance,
            "open_risks": open_risks,
            "open_questions": open_questions,
            "open_todos": open_todos,
        },
        "next_actions": next_actions,
        "basis": {
            "complete": reason_codes.is_empty(),
            "reason_codes": reason_codes,
        },
        "deterministic": true,
        "llm_summary": false,
    });
    enforce_hard_limit(&mut brief);
    brief
}

pub(crate) fn handoff_brief_size(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn instruction_projection(instruction: Option<&str>) -> Value {
    let Some(raw_instruction) = instruction else {
        return json!({
            "excerpt": null,
            "truncated": false,
        });
    };
    let redacted = redact_and_bound_instruction(raw_instruction, usize::MAX);
    let (excerpt, length_truncated) = exact_char_bound(&redacted, HANDOFF_INSTRUCTION_MAX_CHARS);
    let truncated = redacted != raw_instruction || length_truncated;
    json!({
        "excerpt": excerpt,
        "truncated": truncated,
    })
}

fn exact_char_bound(value: &str, max_chars: usize) -> (String, bool) {
    let mut chars = value.chars();
    let bounded = chars.by_ref().take(max_chars).collect::<String>();
    (bounded, chars.next().is_some())
}

fn project_workspace(requested: bool, workspace: Option<&Value>) -> WorkspaceProjection {
    if !requested {
        return WorkspaceProjection {
            value: unavailable_workspace("not_requested", "workspace_not_requested"),
            status: "not_requested",
            dirty: None,
            conflicted: None,
        };
    }
    let Some(workspace) = workspace.filter(|value| value.is_object()) else {
        return WorkspaceProjection {
            value: unavailable_workspace("unavailable", "workspace_unavailable"),
            status: "unavailable",
            dirty: None,
            conflicted: None,
        };
    };
    let git_available = workspace
        .get("git_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let clean = workspace.get("clean").and_then(Value::as_bool);
    let conflicted_count = workspace
        .pointer("/counts/conflicted")
        .and_then(Value::as_u64);
    if !git_available || clean.is_none() || conflicted_count.is_none() {
        return WorkspaceProjection {
            value: unavailable_workspace("unavailable", "workspace_unavailable"),
            status: "unavailable",
            dirty: None,
            conflicted: None,
        };
    }

    let dirty = !clean.unwrap_or(false);
    let conflicted = conflicted_count.unwrap_or(0) > 0;
    let branch = safe_branch(workspace.get("branch").and_then(Value::as_str));
    let head = safe_head(workspace.get("head"));
    let ahead = workspace.get("ahead").and_then(Value::as_u64);
    let behind = workspace.get("behind").and_then(Value::as_u64);
    WorkspaceProjection {
        value: json!({
            "status": "available",
            "reason_code": null,
            "branch": branch,
            "head": head,
            "dirty": dirty,
            "conflicted": conflicted,
            "ahead": ahead,
            "behind": behind,
        }),
        status: "available",
        dirty: Some(dirty),
        conflicted: Some(conflicted),
    }
}

fn unavailable_workspace(status: &str, reason_code: &str) -> Value {
    json!({
        "status": status,
        "reason_code": reason_code,
        "branch": null,
        "head": null,
        "dirty": null,
        "conflicted": null,
        "ahead": null,
        "behind": null,
    })
}

fn safe_branch(branch: Option<&str>) -> Option<String> {
    let branch = branch?.trim();
    if branch.is_empty() || branch.chars().any(char::is_control) {
        return None;
    }
    let redacted = redact_and_bound_instruction(branch, HANDOFF_BRANCH_MAX_CHARS);
    if redacted == "[redacted]" {
        return None;
    }
    Some(exact_char_bound(&redacted, HANDOFF_BRANCH_MAX_CHARS).0)
}

fn safe_head(head: Option<&Value>) -> Option<String> {
    let value = head.and_then(|head| {
        head.as_str().or_else(|| {
            head.get("commit")
                .and_then(Value::as_str)
                .or_else(|| head.get("hash").and_then(Value::as_str))
        })
    })?;
    let value = value.trim();
    (value.len() >= 7 && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn project_validation(requested: bool, validation: Option<&Value>) -> ValidationProjection {
    if !requested {
        return ValidationProjection {
            value: validation_value(
                "not_requested",
                Some("validation_not_requested"),
                json!({
                    "items": [],
                    "total": 0,
                    "returned": 0,
                    "truncated": false,
                }),
            ),
            status: "not_requested",
        };
    }
    let failures = json!({
        "items": [],
        "total": 0,
        "returned": 0,
        "truncated": false,
    });
    let Some(validation) = validation.filter(|value| value.is_object()) else {
        return ValidationProjection {
            value: validation_value("unavailable", Some("validation_unavailable"), failures),
            status: "unavailable",
        };
    };

    let current = validation
        .get("current_evidence")
        .filter(|value| value.is_object());
    let projected = if let Some(status) = current
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
    {
        match status {
            "passed" => "passed",
            "failed" => "failed",
            "stale" => "stale",
            "not_run" => "not_run",
            _ => "unavailable",
        }
    } else {
        // Additive compatibility for callers/tests that still provide the
        // pre-current-evidence validation shape. Production closeout summaries
        // carry current_evidence and therefore never derive a current verdict
        // from these historical fields.
        let available = validation.get("available").and_then(Value::as_bool);
        let status = validation.get("status").and_then(Value::as_str);
        let latest_status = validation.get("latest_status").and_then(Value::as_str);
        let unresolved = validation
            .pointer("/unresolved_failures/count")
            .and_then(Value::as_u64);
        if available == Some(false)
            && (status == Some("not_run") || latest_status == Some("not_run"))
        {
            "not_run"
        } else if unresolved.is_some_and(|count| count > 0)
            || latest_status == Some("failed")
            || status == Some("failed")
        {
            "failed"
        } else if available == Some(true)
            && latest_status == Some("passed")
            && unresolved == Some(0)
        {
            "passed"
        } else if status == Some("not_run") || latest_status == Some("not_run") {
            "not_run"
        } else {
            "unavailable"
        }
    };
    let reason_code = (projected == "unavailable").then_some("validation_unavailable");
    ValidationProjection {
        value: validation_value(projected, reason_code, failures),
        status: projected,
    }
}

fn validation_value(status: &str, reason_code: Option<&str>, open_failures: Value) -> Value {
    json!({
        "status": status,
        "open_failures": open_failures,
        "reason_code": reason_code,
    })
}

fn project_jobs(jobs: Option<&Value>) -> JobProjection {
    let Some(jobs) = jobs.filter(|value| value.is_object()) else {
        return JobProjection {
            available: false,
            blocking: None,
            terminal_pending: None,
            recovering: None,
        };
    };
    let blocking = jobs.get("blocking_active_count").and_then(Value::as_u64);
    let terminal_pending = jobs.get("terminal_pending_count").and_then(Value::as_u64);
    let recovering = jobs.get("recovering_count").and_then(Value::as_u64);
    let available = blocking.is_some() && terminal_pending.is_some() && recovering.is_some();
    JobProjection {
        available,
        blocking: available.then_some(blocking.unwrap_or(0)),
        terminal_pending: available.then_some(terminal_pending.unwrap_or(0)),
        recovering: available.then_some(recovering.unwrap_or(0)),
    }
}

fn bounded_path_list(
    items: Option<&Value>,
    declared_total: Option<u64>,
    source_truncated: bool,
    max_items: usize,
) -> Value {
    let source = items.and_then(Value::as_array);
    let mut seen = BTreeSet::new();
    let mut returned = Vec::new();
    let mut safety_filtered = false;
    if let Some(source) = source {
        for item in source {
            let Some(raw) = item.as_str() else {
                safety_filtered = true;
                continue;
            };
            let Some(path) = normalize_observed_project_path(raw) else {
                safety_filtered = true;
                continue;
            };
            if redact_and_bound_instruction(&path, 512) == "[redacted]" {
                safety_filtered = true;
                continue;
            }
            if seen.insert(path.clone()) && returned.len() < max_items {
                returned.push(path);
            }
        }
    }
    let source_len = source.map_or(0, |values| values.len() as u64);
    let total = declared_total.unwrap_or(source_len).max(source_len);
    let returned_count = returned.len() as u64;
    json!({
        "items": returned,
        "total": total,
        "returned": returned_count,
        "truncated": source_truncated || safety_filtered || total > returned_count,
    })
}

fn bounded_failure_list(attempt: Option<&Value>) -> Value {
    let source = attempt
        .and_then(|value| value.pointer("/validation/open_failures"))
        .and_then(Value::as_array);
    let declared_total = attempt
        .and_then(|value| value.pointer("/validation/total_open_failures"))
        .and_then(Value::as_u64);
    let source_truncated = attempt
        .and_then(|value| value.pointer("/validation/failures_truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut seen = BTreeSet::new();
    let mut returned = Vec::new();
    let mut safety_filtered = false;
    if let Some(source) = source {
        for failure in source {
            let kind = failure.get("kind").and_then(Value::as_str);
            let name = failure.get("name").and_then(Value::as_str);
            let Some(name) = safe_failure_identity(kind, name) else {
                safety_filtered = true;
                continue;
            };
            if seen.insert(name.clone()) && returned.len() < HANDOFF_OPEN_FAILURES_MAX_ITEMS {
                returned.push(name);
            }
        }
    }
    let source_len = source.map_or(0, |values| values.len() as u64);
    let total = declared_total.unwrap_or(source_len).max(source_len);
    let returned_count = returned.len() as u64;
    json!({
        "items": returned,
        "total": total,
        "returned": returned_count,
        "truncated": source_truncated || safety_filtered || total > returned_count,
    })
}

fn safe_failure_identity(kind: Option<&str>, name: Option<&str>) -> Option<String> {
    let kind = kind?;
    let name = name?.trim();
    if name.is_empty()
        || name.starts_with(['/', '\\'])
        || name.chars().any(char::is_control)
        || name
            .chars()
            .any(|ch| !(ch.is_alphanumeric() || matches!(ch, '_' | ':' | '-' | '<' | '>' | '.')))
        || !matches!(kind, "test" | "diagnostic" | "unknown")
    {
        return None;
    }
    let redacted = redact_and_bound_instruction(name, HANDOFF_FAILURE_NAME_MAX_CHARS);
    let (bounded, _) = exact_char_bound(&redacted, HANDOFF_FAILURE_NAME_MAX_CHARS);
    Some(bounded)
}

#[allow(clippy::too_many_arguments)]
fn progress_state(
    session: &SessionSummary,
    continuation_available: bool,
    workspace: &WorkspaceProjection,
    validation: &ValidationProjection,
    jobs: &JobProjection,
    open_risks: Option<u64>,
    guidance_available: bool,
) -> &'static str {
    if !session.lifecycle.allows_mutation() {
        return "closed";
    }
    if workspace.conflicted == Some(true)
        || jobs.blocking.is_some_and(|count| count > 0)
        || jobs.recovering.is_some_and(|count| count > 0)
        || validation.status == "failed"
        || open_risks.is_some_and(|count| count > 0)
    {
        return "blocked";
    }
    if validation.status == "stale"
        || (workspace.dirty == Some(true) && validation.status != "passed")
    {
        return "needs_validation";
    }
    if !continuation_available
        || !jobs.available
        || !guidance_available
        || workspace.status != "available"
        || validation.status == "unavailable"
        || validation.status == "not_requested"
    {
        return "insufficient_evidence";
    }
    "ready_to_continue"
}

#[allow(clippy::too_many_arguments)]
fn next_actions(
    workspace: &WorkspaceProjection,
    validation: &ValidationProjection,
    jobs: &JobProjection,
    open_guidance: Option<u64>,
    open_risks: Option<u64>,
    open_questions: Option<u64>,
    open_todos: Option<u64>,
    changed_count: u64,
    recent_count: u64,
    continuation: &Value,
    existing: Option<&Value>,
) -> Vec<String> {
    let known = known_existing_actions(continuation, existing);
    let mut actions = Vec::new();
    if workspace.conflicted == Some(true) {
        push_unique(
            &mut actions,
            "resolve workspace conflicts before continuing",
        );
    }
    if jobs.recovering.is_some_and(|count| count > 0) {
        push_known_or(
            &mut actions,
            &known,
            "await recovering jobs before relying on their output",
        );
    } else if jobs.blocking.is_some_and(|count| count > 0) {
        push_unique(&mut actions, "stop or await blocking active jobs");
    }
    if validation.status == "failed" {
        push_unique(&mut actions, "resolve the latest validation failure");
    }
    if open_risks.is_some_and(|count| count > 0) {
        push_unique(&mut actions, "review open risk guidance before continuing");
    }
    if matches!(validation.status, "not_run" | "stale") {
        push_known_or(
            &mut actions,
            &known,
            "run validation before proceeding when the task warrants it",
        );
    }
    if open_guidance.is_some_and(|count| count > 0)
        || open_questions.is_some_and(|count| count > 0)
        || open_todos.is_some_and(|count| count > 0)
    {
        push_unique(
            &mut actions,
            "review open questions, todos, or guidance before continuing",
        );
    }
    if jobs.terminal_pending.is_some_and(|count| count > 0) {
        push_known_or(
            &mut actions,
            &known,
            "confirm terminal-pending jobs reached a final state",
        );
    }
    if recent_count > 0 {
        push_known_or(&mut actions, &known, EXPLORATION_CONTINUITY_ACTION);
    }
    if changed_count > 0 {
        push_unique(&mut actions, "inspect current changes and continue");
    }
    if actions.is_empty() {
        push_known_or(&mut actions, &known, "continue with the next task step");
    }
    actions.truncate(HANDOFF_NEXT_ACTIONS_MAX_ITEMS);
    actions
}

fn known_existing_actions(continuation: &Value, existing: Option<&Value>) -> BTreeSet<String> {
    const ALLOWED: &[&str] = &[
        "await recovering jobs before relying on their output",
        "confirm terminal-pending jobs reached a final state",
        "run validation before proceeding when the task warrants it",
        EXPLORATION_CONTINUITY_ACTION,
        "continue with the next task step",
    ];
    let mut known = BTreeSet::new();
    for value in [
        continuation.pointer("/attempt/suggested_next_actions"),
        existing,
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_array)
    .flatten()
    .filter_map(Value::as_str)
    {
        if ALLOWED.contains(&value) {
            known.insert(value.to_string());
        }
    }
    known
}

fn push_known_or(actions: &mut Vec<String>, known: &BTreeSet<String>, fallback: &str) {
    let action = known.get(fallback).map(String::as_str).unwrap_or(fallback);
    push_unique(actions, action);
}

fn push_unique(actions: &mut Vec<String>, action: &str) {
    if actions.len() < HANDOFF_NEXT_ACTIONS_MAX_ITEMS
        && !actions.iter().any(|existing| existing == action)
    {
        actions.push(action.to_string());
    }
}

fn enforce_hard_limit(brief: &mut Value) {
    for pointer in [
        "/progress/recent_files",
        "/progress/changes",
        "/validation/open_failures",
    ] {
        while handoff_brief_size(brief) >= HANDOFF_BRIEF_HARD_MAX_BYTES
            && pop_list_item(brief, pointer)
        {}
    }
    while handoff_brief_size(brief) >= HANDOFF_BRIEF_HARD_MAX_BYTES
        && pop_plain_array_item(brief, "/next_actions")
    {}
    while handoff_brief_size(brief) >= HANDOFF_BRIEF_HARD_MAX_BYTES {
        let root_len = brief
            .pointer("/task/root_instruction/excerpt")
            .and_then(Value::as_str)
            .map(str::len)
            .unwrap_or(0);
        let latest_len = brief
            .pointer("/task/latest_instruction/excerpt")
            .and_then(Value::as_str)
            .map(str::len)
            .unwrap_or(0);
        if root_len == 0 && latest_len == 0 {
            break;
        }
        let pointer = if root_len >= latest_len {
            "/task/root_instruction"
        } else {
            "/task/latest_instruction"
        };
        if !pop_instruction_char(brief, pointer) {
            break;
        }
    }
    debug_assert!(
        handoff_brief_size(brief) < HANDOFF_BRIEF_HARD_MAX_BYTES,
        "handoff brief hard-limit reduction must retain a bounded core"
    );
}

fn pop_list_item(brief: &mut Value, pointer: &str) -> bool {
    let Some(list) = brief.pointer_mut(pointer).and_then(Value::as_object_mut) else {
        return false;
    };
    let popped = list
        .get_mut("items")
        .and_then(Value::as_array_mut)
        .and_then(Vec::pop)
        .is_some();
    if !popped {
        return false;
    }
    let returned = list
        .get("items")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    list.insert("returned".to_string(), json!(returned));
    list.insert("truncated".to_string(), json!(true));
    true
}

fn pop_plain_array_item(brief: &mut Value, pointer: &str) -> bool {
    brief
        .pointer_mut(pointer)
        .and_then(Value::as_array_mut)
        .and_then(Vec::pop)
        .is_some()
}

fn pop_instruction_char(brief: &mut Value, pointer: &str) -> bool {
    let Some(instruction) = brief.pointer_mut(pointer).and_then(Value::as_object_mut) else {
        return false;
    };
    let Some(mut excerpt) = instruction
        .get("excerpt")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return false;
    };
    if excerpt.pop().is_none() {
        return false;
    }
    instruction.insert("excerpt".to_string(), json!(excerpt));
    instruction.insert("truncated".to_string(), json!(true));
    true
}
