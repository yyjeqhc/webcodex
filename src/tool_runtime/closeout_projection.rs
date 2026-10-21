//! Adapters for existing subsystem reports and the public closeout result schema.
//! Only this boundary knows report paths; the evaluator consumes typed facts.
use super::closeout_facts::*;
use serde_json::{json, Value};

pub(crate) fn workspace_observation(
    value: &Value,
    requested: bool,
    succeeded: bool,
) -> Observation<WorkspaceFacts> {
    if !requested {
        return Observation::NotChecked;
    }
    if !succeeded
        || !value.is_object()
        || !matches!(value.get("clean"), Some(Value::Bool(_) | Value::Null))
    {
        return Observation::Failed;
    }
    Observation::Observed(WorkspaceFacts {
        clean: value.get("clean").and_then(Value::as_bool),
        conflicts: value.pointer("/counts/conflicted").and_then(Value::as_u64),
        non_git: value.get("non_git_project").and_then(Value::as_bool) == Some(true)
            || value.get("git_available").and_then(Value::as_bool) == Some(false),
    })
}

pub(crate) fn hygiene_observation(
    value: &Value,
    requested: bool,
    succeeded: bool,
) -> Observation<HygieneFacts> {
    if !requested {
        return Observation::NotChecked;
    }
    if !succeeded
        || !value.is_object()
        || !matches!(value.get("clean"), Some(Value::Bool(_) | Value::Null))
    {
        return Observation::Failed;
    }
    // Successful hygiene reports intentionally omit zero counts and false flags.
    // Apply that sparse contract only after success/shape checks, never to a failure.
    Observation::Observed(HygieneFacts {
        clean: value.get("clean").and_then(Value::as_bool),
        secret_like_paths: value
            .pointer("/counts/secret_like_paths")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        truncated: value
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn project_closeout_decision(
    decision: &CloseoutDecision,
    facts: &CloseoutFacts,
    display: &Value,
) -> Value {
    let validation = display.get("validation").unwrap_or(&Value::Null);
    let integrity = json!({"status": decision.integrity_status.as_str(), "error_reasons": decision.integrity_errors, "warning_reasons": decision.integrity_warnings});
    json!({
        "facts": {
            "work_performed": display.get("work_performed").cloned().unwrap_or_else(|| json!([])),
            "changed_paths": display.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
            "executions": validation.get("events").cloned().unwrap_or_else(|| json!([])),
            "validations_passed": facts.validation.successes,
            "validations_failed": facts.validation.failures,
            "validations_skipped": {"count": u64::from(facts.validation.validation_skipped()), "reason": validation.get("reason").cloned().unwrap_or(Value::Null)},
            "resolved_failures": validation.get("resolved_failures").cloned().unwrap_or_else(|| json!({"count": facts.validation.resolved_failure_count, "events": []})),
            "unresolved_failures": validation.get("unresolved_failures").cloned().unwrap_or_else(|| json!({"count": facts.validation.unresolved_failure_count, "events": []})),
            "workspace_state": {
                "checked": facts.workspace.was_checked(), "clean": facts.workspace_clean(), "conflicts": facts.workspace_conflicts().unwrap_or(0),
                "hygiene_checked": facts.hygiene.as_ref().map(Observation::was_checked),
                "hygiene_clean": facts.hygiene_observed().and_then(|h| h.clean).unwrap_or(true),
            },
            "active_jobs": display.get("jobs").cloned().unwrap_or_else(|| json!({})),
            "evidence_integrity": integrity,
        },
        "hard_blockers": decision.blocking_reasons,
        "advisories": decision.advisories,
        "task_outcome": {"status": decision.task_status.as_str(), "blocking": decision.blocking(), "blocking_reasons": decision.blocking_reasons, "warning_reasons": decision.task_warning_reasons},
        "evidence_history": {"status": decision.history_status.as_str()},
        "evidence_integrity": integrity,
        "informational_notes": decision.informational_notes,
        "verdict": {"status": decision.legacy_status.as_str(), "blocking": decision.legacy_blocking(), "blocking_reasons": decision.blocking_reasons, "warning_reasons": decision.advisories, "suggested_next_actions": decision.actions},
    })
}

pub(crate) fn install_closeout_decision(
    target: &mut Value,
    facts: &CloseoutFacts,
    initial_actions: &[String],
) -> CloseoutDecision {
    let decision = evaluate_closeout(facts, initial_actions);
    let projected = project_closeout_decision(&decision, facts, target);
    for (key, value) in projected
        .as_object()
        .expect("closeout projection is an object")
    {
        target[key] = value.clone();
    }
    decision
}

#[cfg(test)]
mod tests;
