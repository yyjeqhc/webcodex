//! Opt-in Host control view, captured before lossy model projection.
//! This is presentation only: no dispatch, retry, waiting, or authority changes.

use super::{execution_outcome::ExecutionOutcomeFacts, ToolResult};
use serde_json::{json, Map, Value};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

pub(crate) fn supported(tool: &str) -> bool {
    matches!(
        tool,
        "run_process"
            | "run_script"
            | "run_shell"
            | "project_build"
            | "run_skill_resource"
            | "observe_jobs"
    )
}

pub(super) enum ExecutionControlProjection {
    Single(Value),
    Jobs(Vec<Value>),
}

fn nonempty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

fn recovering(output: &Value) -> bool {
    ["recovery_state", "recovery_reason_code"]
        .iter()
        .any(|key| output.get(*key).is_some_and(|v| !v.is_null()))
}

fn control(state: &str, outcome: &str, output: &Value) -> Value {
    let mut value = json!({"state": state, "outcome": outcome});
    if let Some(code) = output["exit_code"].as_i64() {
        value["exit_code"] = json!(code);
    }
    if let Some(id) = nonempty(&output["job_id"]) {
        value["job_id"] = json!(id);
    }
    value
}

fn command_control(result: &ToolResult) -> Value {
    let facts = ExecutionOutcomeFacts::from_result(result);
    let state = facts.execution_state_name().unwrap_or("unknown");
    let output = &result.output;
    let outcome = if facts.is_outcome_unknown() || recovering(output) {
        "unknown"
    } else if facts.is_pending() {
        if facts.has_job_handoff() && facts.terminal() != Some(true) {
            "pending"
        } else {
            "unknown"
        }
    } else if facts.is_terminal_command_success()
        && output["exit_code"].as_i64() == Some(0)
        && result.error.is_none()
        && output.get("failure_kind").is_none_or(Value::is_null)
        && output.get("error_kind").is_none_or(Value::is_null)
        && output.get("tool_failure").is_none_or(|v| v == false)
        && output.get("passed").is_none_or(|v| v == true)
    {
        "passed"
    } else if !result.success
        && (facts.is_definitely_not_started()
            || (matches!(state, "completed" | "timed_out") && facts.terminal() == Some(true)))
    {
        "failed"
    } else {
        "unknown"
    };
    control(state, outcome, output)
}

fn job_control(item: &Value) -> Value {
    let output = &item["output"];
    let state = output["status"].as_str().unwrap_or("unknown");
    let lifecycle = RunnerJobLifecycle::from_wire(state).ok();
    let command_state = output.get("command_execution_state");
    let uncertain = command_state.is_some_and(|v| !v.is_null() && v != "completed");
    let outcome = if item["success"] != true || recovering(output) {
        "unknown" // An observation failure says nothing about the process outcome.
    } else if command_state.is_some_and(|v| v == "outcome_unknown") {
        "unknown"
    } else if lifecycle.is_some_and(RunnerJobLifecycle::is_active) && output["terminal"] == false {
        "pending"
    } else if output["terminal"] == true
        && matches!(
            lifecycle,
            Some(
                RunnerJobLifecycle::Failed
                    | RunnerJobLifecycle::Stopped
                    | RunnerJobLifecycle::Timeout
                    | RunnerJobLifecycle::TimedOut
                    | RunnerJobLifecycle::Cancelled
            )
        )
    {
        "failed"
    } else if uncertain {
        "unknown"
    } else if lifecycle == Some(RunnerJobLifecycle::Completed)
        && output["terminal"] == true
        && output["exit_code"].as_i64() == Some(0)
        && output
            .get("validation")
            .is_none_or(|v| v.is_null() || v["passed"] == true)
    {
        "passed"
    } else if output["terminal"] == true
        && lifecycle == Some(RunnerJobLifecycle::Completed)
        && (output["exit_code"].as_i64().is_some_and(|code| code != 0)
            || output["validation"]["passed"] == false)
    {
        "failed"
    } else {
        "unknown"
    };
    let mut value = control(state, outcome, output);
    if let Some(id) = nonempty(&item["job_id"]) {
        value["job_id"] = json!(id);
    }
    // Observation cursors remain exact and scoped. They do not authorize retries.
    if let Some(reference) = nonempty(&item["observation_ref"]) {
        value["observation_ref"] = json!(reference);
    } else if let Some(token) = nonempty(&output["observation_token"]) {
        value["observation_token"] = json!(token);
    }
    value
}

impl ExecutionControlProjection {
    pub(super) fn capture(tool: &str, result: &ToolResult) -> Option<Self> {
        if !supported(tool) {
            return None;
        }
        if tool == "observe_jobs" {
            Some(Self::Jobs(
                result.output["items"]
                    .as_array()?
                    .iter()
                    .map(job_control)
                    .collect(),
            ))
        } else {
            Some(Self::Single(command_control(result)))
        }
    }

    pub(super) fn project(self, result: &mut ToolResult) {
        match self {
            Self::Single(execution) => result.output["execution"] = execution,
            Self::Jobs(controls) => {
                let Some(items) = result.output["items"].as_array_mut() else {
                    return;
                };
                // Projection may flatten observations but must never reorder/drop them.
                if controls.len() != items.len() {
                    return;
                }
                for (item, execution) in items.iter_mut().zip(controls) {
                    if item["job_id"] != execution["job_id"] {
                        continue;
                    }
                    item["execution"] = execution;
                }
            }
        }
    }
}

/// Run only after the adapter has projected canonical suggested-call routes.
/// Keep the flat canonical locations intact until that point, including calls
/// inside failed observations and log expansion guidance.
pub(crate) fn split_result(result: &mut ToolResult) {
    if result.output.get("execution").is_some() {
        split_output(&mut result.output);
    } else if let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if item.get("execution").is_some() {
                split_output(item);
            }
        }
    }
}

fn split_output(output: &mut Value) {
    let Some(details) = output.as_object_mut() else {
        return;
    };
    let mut details = std::mem::take(details);
    let Some(mut execution) = details.remove("execution") else {
        return;
    };
    // This is now the adapter-admitted exact call, not a guessed/rebuilt edge.
    if let Some(next) = details.remove("continuation") {
        execution["next"] = next;
    }
    let mut view = Map::new();
    // These are independent obligations, not optional process diagnostics.
    for key in [
        "context_projection",
        "control",
        "session_attention",
        "operator_messages",
        "peer_messages",
        "job_attention",
        "workflow_recording_attention",
        "window_reply",
    ] {
        if let Some(value) = details.remove(key) {
            view.insert(key.to_string(), value);
        }
    }
    if execution["outcome"] == "passed" || execution["outcome"] == "pending" {
        for key in [
            "execution_state",
            "command_execution_state",
            "terminal",
            "command_started",
            "command_completed",
            "command_ok",
            "promoted_to_job",
            "job_status",
            "status",
        ] {
            details.remove(key);
        }
    }
    for (detail, projected) in [
        ("exit_code", "exit_code"),
        ("job_id", "job_id"),
        ("observation_token", "observation_token"),
        ("observation_ref", "observation_ref"),
        ("continuation", "next"),
    ] {
        if execution.get(projected).is_some() {
            details.remove(detail);
        }
    }
    view.insert("execution".to_string(), execution);
    // Never discard synchronous stdout: without a Job it cannot be re-read.
    if !details.is_empty() {
        view.insert("details".to_string(), Value::Object(details));
    }
    *output = Value::Object(view);
}

#[cfg(test)]
#[path = "tests/execution_control.rs"]
mod tests;
