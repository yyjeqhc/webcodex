//! Bounded recovery pointers and advisory repeated-failure diagnostics.
//! No workspace reads, execution, scheduling, or authority is owned here.
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use webcodex_core::validation_identity::is_validation_execution_identity;
use webcodex_core::workflow_session_contract::is_safe_job_id;
use webcodex_tool_contracts::{lookup_tool_definition, ToolActivityPresentation};

use crate::continuation::ContinuationToolFailureSnapshot;
use crate::{
    canonical_tool_call_finished_events, current_attempt_event_view,
    normalize_observed_project_path, redact_and_bound_instruction, SessionDiscussionSummary,
    SessionEvent, SessionSummary,
};

pub(crate) const TASK_CONTEXT_MAX_BYTES: usize = 4096;
pub(crate) const RECONCILE_ACTION: &str =
    "reconcile the exact Job or execution outcome before considering another execution";
pub(crate) const STALL_ACTION: &str =
    "inspect the repeated failure evidence and change the approach before another attempt";

pub(crate) fn task_context_value(
    summary: &SessionSummary,
    discussion: Option<&SessionDiscussionSummary>,
    failures: &Value,
    actionable: Option<ContinuationToolFailureSnapshot<'_>>,
    failure_evidence: Option<fn(&SessionEvent) -> Option<Value>>,
) -> Value {
    let view = current_attempt_event_view(summary);
    let events = canonical_tool_call_finished_events(&view.semantic_events);
    let session_id = safe_ref(&summary.session_id).then_some(summary.session_id.as_str());
    let project = summary.project.as_deref().filter(|project| {
        project.len() <= 512
            && !project.chars().any(char::is_control)
            && redact_and_bound_instruction(project, 512) == *project
    });
    let boundary = view
        .boundary_event_index
        .map(|index| summary.events[index].event_id.as_str())
        .or_else(|| {
            summary
                .current_instruction
                .as_ref()
                .map(|event| event.event_id.as_str())
        });
    let safe_boundary = boundary.filter(|id| safe_ref(id));
    let scope_valid = session_id.is_some() && project.is_some() && boundary == safe_boundary;
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut files_filtered = false;
    for event in events.iter().rev().filter(|event| {
        scope_valid && same_scope(summary, event) && event.status.as_deref() == Some("succeeded")
    }) {
        for (kind, paths) in [
            ("changed_path", &event.changed_paths),
            ("observed_path", &event.observed_paths),
        ] {
            for raw in paths {
                let Some(path) = normalize_observed_project_path(raw) else {
                    files_filtered = true;
                    continue;
                };
                if redact_and_bound_instruction(&path, 512) != path || !safe_ref(&event.event_id) {
                    files_filtered = true;
                    continue;
                }
                if seen.insert(path.clone()) && files.len() < 6 {
                    files.push(json!({"path": path, "event_id": event.event_id, "source": kind}));
                }
            }
        }
    }
    let mut locations = Vec::new();
    let mut locations_filtered = false;
    let location_total = failures.as_array().map_or(0, Vec::len);
    for (index, failure) in failures.as_array().into_iter().flatten().enumerate() {
        let Some(name) = crate::handoff_brief::safe_failure_identity(
            failure.get("kind").and_then(Value::as_str),
            failure.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let path = failure
            .get("file")
            .and_then(Value::as_str)
            .and_then(normalize_observed_project_path)
            .filter(|path| redact_and_bound_instruction(path, 512) == *path);
        locations_filtered |= failure.get("file").is_some_and(|file| !file.is_null())
            && path.is_none()
            || failure
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|raw| raw.trim() != name);
        // Test/symbol names without a file are still useful explicit failure locations.
        if locations.len() < 3 {
            locations.push(json!({"name": name, "path": path,
                "line": failure.get("line").and_then(Value::as_u64).filter(|line| *line > 0),
                "source_ref": format!("continuation_feedback#/attempt/validation/open_failures/{index}")}));
        }
    }
    let mut value = json!({
        "scope": {"session_id": session_id, "project": project,
            "instruction_event_id": safe_boundary, "complete": view.complete && scope_valid},
        "relevant_files": list(files, seen.len()),
        "failure_locations": list(locations, location_total),
        "decisions": crate::handoff_brief::project_message_notes(&summary.session_id,
            discussion.map(|value| (value.counts.decision, value.recent_decisions.as_slice()))),
        "recent_progress": crate::handoff_brief::project_message_notes(&summary.session_id,
            discussion.map(|value| (value.counts.progress, value.recent_progress.as_slice()))),
        "stall": stall_diagnostic(summary, &events, &view.semantic_events, view.complete && scope_valid, actionable, failure_evidence),
        "continuation_entry": {"tool": "read_session_handoff", "session_id": session_id},
    });
    let (action, source_refs) = if let Some(action) = value
        .pointer("/stall/recommendation")
        .and_then(Value::as_str)
    {
        (
            action.to_string(),
            value["stall"]["event_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| format!("event:{id}"))
                .take(3)
                .collect::<Vec<_>>(),
        )
    } else if let Some(source) = value
        .pointer("/failure_locations/items/0/source_ref")
        .and_then(Value::as_str)
    {
        (
            "inspect the retained validation failure before choosing the next change".to_string(),
            vec![source.to_string()],
        )
    } else if scope_valid {
        let id = session_id.expect("validated scope");
        let reference = safe_boundary
            .map(|event| format!("event:{event}"))
            .unwrap_or_else(|| format!("session:{id}"));
        (
            "continue from the recorded task and current Session evidence".to_string(),
            vec![reference],
        )
    } else {
        (
            "recover the exact Session and Project scope before choosing a next step".to_string(),
            Vec::new(),
        )
    };
    value["next_steps"] = list(vec![json!({"action":action,"source_refs":source_refs})], 1);
    if locations_filtered {
        value["failure_locations"]["truncated"] = json!(true);
    }
    if files_filtered {
        value["relevant_files"]["truncated"] = json!(true);
    }
    // New recovery context has its own envelope, inside the existing startup and
    // 8 KiB handoff budgets. No list may silently become complete after trimming.
    for field in [
        "recent_progress",
        "decisions",
        "relevant_files",
        "failure_locations",
    ] {
        while crate::handoff_brief_size(&value) > TASK_CONTEXT_MAX_BYTES {
            let items = value[field]["items"]
                .as_array_mut()
                .expect("bounded context list");
            if items.pop().is_none() {
                break;
            }
            let returned = items.len();
            value[field]["returned"] = json!(returned);
            value[field]["truncated"] = json!(true);
        }
    }
    debug_assert!(crate::handoff_brief_size(&value) <= TASK_CONTEXT_MAX_BYTES);
    value
}

fn list(items: Vec<Value>, total: usize) -> Value {
    json!({"returned": items.len(), "truncated": total > items.len(), "total": total, "items": items})
}

fn safe_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn same_scope(summary: &SessionSummary, event: &SessionEvent) -> bool {
    event.session_id == summary.session_id
        && summary.project.as_deref().is_some_and(|project| {
            event
                .resolved_project
                .as_deref()
                .or(event.project.as_deref())
                == Some(project)
        })
}

fn is_housekeeping(event: &SessionEvent, start: Option<&SessionEvent>) -> bool {
    // Polling, heartbeat and pagination are observation mechanics, never
    // repeated execution failures or evidence of progress. Ambiguous paged
    // requests are conservatively excluded rather than counted as new evidence.
    // Canonical support/transport reads and the closeout projection summarize
    // existing evidence. Recording them must not reset a failure run or change
    // the brief they just returned. Actual review/file observations remain Work.
    lookup_tool_definition(&event.tool_name).is_some_and(|definition| {
        definition.activity_semantics().presentation != ToolActivityPresentation::Work
    }) || event.tool_name == "finish_coding_task"
        || event.tool_name.starts_with("get_")
        || event.tool_name.starts_with("list_")
        || event.tool_name.starts_with("observe_")
        || event.tool_name.starts_with("sync_")
        || event.tool_name.contains("heartbeat")
        || event.tool_name.contains("continuation")
        || event
            .input_summary
            .as_ref()
            .or_else(|| start.and_then(|event| event.input_summary.as_ref()))
            .is_some_and(|input| has_pagination(input, 0))
}

fn has_pagination(value: &Value, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    match value {
        Value::Object(object) => object.iter().take(16).any(|(key, value)| {
            ([
                "offset",
                "cursor",
                "page",
                "after",
                "after_message_id",
                "after_observation_token",
            ]
            .contains(&key.as_str())
                && !value.is_null())
                || has_pagination(value, depth + 1)
        }),
        Value::Array(items) => items
            .iter()
            .take(8)
            .any(|item| has_pagination(item, depth + 1)),
        _ => false,
    }
}

fn unknown_execution(event: &SessionEvent) -> bool {
    let states = [
        event
            .effect_evidence
            .as_ref()
            .and_then(|e| e.execution_state.as_deref()),
        event
            .persistent_shell
            .as_ref()
            .and_then(|e| e.execution_state.as_deref()),
        event
            .validation_output_summary
            .as_ref()
            .and_then(|value| value.get("execution_state"))
            .and_then(Value::as_str),
    ];
    // Producers can retain uncertainty as a structured failure kind without
    // effect evidence. An accepted Job must never mask explicit uncertainty.
    if [
        event.failure_kind.as_deref(),
        event.error_kind.as_deref(),
        event.actual_failure_kind.as_deref(),
    ]
    .contains(&Some("outcome_unknown"))
        || states
            .iter()
            .any(|state| matches!(state, Some("outcome_unknown" | "unknown" | "recovering")))
    {
        return true;
    }
    let pending =
        |state: Option<&str>| matches!(state, Some("queued" | "pending" | "started" | "running"));
    if event.status.as_deref() == Some("succeeded")
        && event.job_id.as_deref().is_some_and(is_safe_job_id)
        && states.iter().copied().any(pending)
    {
        // This immutable event records successful asynchronous acceptance, not
        // an unknown outcome. Active/recovering/terminal Jobs are already owned
        // by the canonical Job projection; do not create a permanent recovery
        // requirement from its historical command_completed=false receipt.
        return false;
    }
    event.effect_evidence.as_ref().is_some_and(|evidence| {
        pending(evidence.execution_state.as_deref())
            || (evidence.command_started == Some(true) && evidence.command_completed != Some(true))
    }) || event.persistent_shell.as_ref().is_some_and(|evidence| {
        pending(evidence.execution_state.as_deref())
            || (evidence.command_started == Some(true) && evidence.command_completed != Some(true))
    }) || event
        .validation_output_summary
        .as_ref()
        .is_some_and(|value| pending(value.get("execution_state").and_then(Value::as_str)))
}

fn retained_operation_identity(event: &SessionEvent) -> Option<&str> {
    // These hashes are supplied by the canonical typed execution/validation
    // audit path. Never hash argv/command summaries: they are lossy/redacted.
    let input = event.input_summary.as_ref()?;
    let identity = input
        .get("execution_identity")
        .or_else(|| input.get("validation_target_id"))?
        .as_str()?;
    if !is_validation_execution_identity(identity) || identity.starts_with("assertion:") {
        return None;
    }
    Some(identity)
}

fn exact_prior_start<'a>(
    summary: &SessionSummary,
    finished: &SessionEvent,
    attempt: &'a [SessionEvent],
) -> Option<&'a SessionEvent> {
    // Validation's legacy matcher intentionally permits missing call IDs and
    // selects a single match. Recovery needs a stricter proof: one earlier
    // exact start within this attempt, never a legacy/timestamp fallback.
    let call_id = finished.call_id.as_deref().filter(|id| safe_ref(id))?;
    let finish_index = attempt
        .iter()
        .position(|event| event.event_id == finished.event_id)?;
    let mut starts = attempt[..finish_index].iter().filter(|event| {
        event.kind == "tool_call_started" && event.call_id.as_deref() == Some(call_id)
    });
    let start = starts.next()?;
    if starts.next().is_some()
        || !same_scope(summary, start)
        || start.tool_name != finished.tool_name
        || start.logical_invocation_id != finished.logical_invocation_id
        || start.logical_invocation_role != finished.logical_invocation_role
    {
        return None;
    }
    Some(start)
}

fn operation_identity(
    summary: &SessionSummary,
    finished: &SessionEvent,
    attempt: &[SessionEvent],
) -> Option<String> {
    let start = exact_prior_start(summary, finished, attempt)?;
    let identity = retained_operation_identity(start)?;
    if retained_operation_identity(finished).is_some_and(|retained| retained != identity) {
        return None;
    }
    Some(format!("{}:{identity}", finished.tool_name))
}

fn stall_diagnostic(
    summary: &SessionSummary,
    events: &[&SessionEvent],
    attempt: &[SessionEvent],
    complete: bool,
    actionable: Option<ContinuationToolFailureSnapshot<'_>>,
    failure_evidence: Option<fn(&SessionEvent) -> Option<Value>>,
) -> Value {
    let relevant = events
        .iter()
        .copied()
        .rev()
        .filter(|event| {
            same_scope(summary, event)
                && !is_housekeeping(event, exact_prior_start(summary, event, attempt))
        })
        .take(20)
        .collect::<Vec<_>>();
    let mut ids = Vec::new();
    let mut distinct_events = HashSet::new();
    let mut distinct_calls = HashSet::new();
    let mut distinct_invocations = HashSet::new();
    let mut identity = None;
    let mut signature = None;
    let mut unavailable = !complete || actionable.is_none();
    let unknown = relevant.iter().position(|event| {
        unknown_execution(event)
            && (event.status.as_deref() != Some("failed")
                || actionable.is_none_or(|snapshot| snapshot.is_actionable(event)))
    });
    if let Some(index) = unknown {
        return json!({"status":"reconcile_required","reason_code":"execution_outcome_unknown",
            "scanned_events":relevant.len(),"event_ids":relevant.get(index).filter(|event|safe_ref(&event.event_id)).map(|event|vec![&event.event_id]).unwrap_or_default(),
            "recommendation":RECONCILE_ACTION,"advisory_only":true});
    }
    for event in &relevant {
        if event.status.as_deref() != Some("failed") {
            // A fresh relevant observation, mutation or success changes the
            // evidence basis, even if its bounded summary looks unchanged.
            break;
        }
        if matches!(
            event.failure_expectation_result.as_deref(),
            Some("matched_expected_failure" | "matched_expected_result")
        ) || actionable.is_some_and(|snapshot| !snapshot.is_actionable(event))
        {
            break;
        }
        if event.expected_failure == Some(true) && event.failure_expectation_result.is_none() {
            unavailable = true;
            break;
        }
        let Some(operation) = operation_identity(summary, event, attempt) else {
            unavailable = true;
            break;
        };
        let Some(evidence) = failure_evidence.and_then(|identity| identity(event)) else {
            unavailable = true;
            break;
        };
        let failure = json!([
            event.exit_code,
            event.failure_kind,
            event.error_kind,
            event.actual_failure_kind,
            event.result_expectation,
            event.accepted_exit_codes,
            event.assertion_name,
            event.expected_failure_kind,
            event.failure_expectation_result,
            evidence
        ]);
        if identity
            .as_ref()
            .is_some_and(|previous| previous != &operation)
            || signature
                .as_ref()
                .is_some_and(|previous| previous != &failure)
        {
            break;
        }
        if !safe_ref(&event.event_id)
            || !distinct_events.insert(event.event_id.as_str())
            || event
                .call_id
                .as_deref()
                .is_some_and(|id| !distinct_calls.insert(id))
            || event
                .logical_invocation_id
                .as_deref()
                .is_some_and(|id| !distinct_invocations.insert(id))
        {
            unavailable = true;
            break;
        }
        identity = Some(operation);
        signature = Some(failure);
        ids.push(event.event_id.clone());
        if ids.len() == 3 {
            break;
        }
    }
    let (status, reason, action) = if unavailable {
        ("unavailable", "operation_identity_or_scope_unproven", None)
    } else if ids.len() >= 3 {
        (
            "suspected",
            "repeated_unexpected_failure_without_new_evidence",
            Some(STALL_ACTION),
        )
    } else {
        ("not_observed", "threshold_not_met", None)
    };
    json!({"status": status, "reason_code": reason, "scanned_events": relevant.len(),
        "event_ids": ids,
        "recommendation": action, "advisory_only": true})
}
