//! Job display owns progress allowlists and observation formatting, not lifecycle truth.
use super::registry::PresentationRenderer;
use super::{
    bounded_text, copy_bounded_text, copy_scalar, MAX_MCP_PRESENTATION_ITEMS,
    MCP_PRESENTATION_VERSION,
};
use serde_json::{json, Map, Value};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

pub(super) const LIST: PresentationRenderer = PresentationRenderer {
    tools: &["list_jobs"],
    project: |_, output| list_jobs_presentation(output),
};
pub(super) const OBSERVE: PresentationRenderer = PresentationRenderer {
    tools: &["observe_jobs"],
    project: |_, output| observe_jobs_presentation(output),
};

fn safe_job_progress(state: &str, reason_code: &str) -> Option<Value> {
    if !matches!(state, "working" | "waiting") {
        return None;
    }
    let summary = match reason_code {
        "process_running" => "Process execution in progress",
        "validation_format" => "Formatting validation in progress",
        "validation_check" => "Check validation in progress",
        "validation_test" => "Test validation in progress",
        "cargo_waiting_for_build_lock" | "cargo_build_lock" => "Waiting for Cargo build lock",
        "cargo_compiling" => "Cargo compilation in progress",
        "cargo_checking" => "Cargo checking in progress",
        _ => return None,
    };
    Some(json!({
        "state": state,
        "reason_code": reason_code,
        "summary": summary,
    }))
}

fn job_progress_presentation(source: &Value) -> Option<Value> {
    if let Some(progress) = source
        .pointer("/detected_summary/progress")
        .and_then(Value::as_object)
    {
        if let (Some(state), Some(reason_code)) = (
            progress.get("state").and_then(Value::as_str),
            progress.get("reason_code").and_then(Value::as_str),
        ) {
            if let Some(projected) = safe_job_progress(state, reason_code) {
                return Some(projected);
            }
        }
    }

    let activity = source.get("activity")?.as_object()?;
    let state = activity.get("state")?.as_str()?;
    let reason_code = activity.get("phase")?.as_str()?;
    safe_job_progress(state, reason_code)
}

fn job_work_presentation(source: &Value) -> Option<Value> {
    let detected = source.get("detected_summary")?.as_object()?;
    let mut output = Map::new();
    if let Some(kind) = detected.get("kind").and_then(Value::as_str).filter(|kind| {
        matches!(
            *kind,
            "test"
                | "check"
                | "format"
                | "build"
                | "validation"
                | "release"
                | "diagnostic"
                | "operation"
        )
    }) {
        output.insert("kind".to_string(), Value::String(kind.to_string()));
    }
    if let Some(outcome) = detected
        .get("outcome")
        .and_then(Value::as_str)
        .filter(|outcome| {
            matches!(
                *outcome,
                "in_progress" | "passed" | "failed" | "timed_out" | "cancelled"
            )
        })
    {
        output.insert("outcome".to_string(), Value::String(outcome.to_string()));
    }
    let detected = Value::Object(detected.clone());
    for key in [
        "tests_detected",
        "tests_run_count",
        "zero_tests_run",
        "tests_passed",
        "tests_failed",
    ] {
        copy_scalar(&detected, &mut output, key);
    }
    (!output.is_empty()).then(|| Value::Object(output))
}

fn apply_job_lifecycle(item: &mut Map<String, Value>, status: &str) {
    let active = webcodex_runner_registry::job_status_is_active(status);
    let lifecycle = RunnerJobLifecycle::from_wire(status).ok();
    let terminal_pending = lifecycle == Some(RunnerJobLifecycle::StopRequested);
    let terminal = lifecycle.is_some_and(RunnerJobLifecycle::is_terminal);
    item.insert("active".to_string(), Value::Bool(active));
    item.insert("terminal".to_string(), Value::Bool(terminal));
    item.insert(
        "terminal_pending".to_string(),
        Value::Bool(terminal_pending),
    );
    item.insert(
        "blocking_active".to_string(),
        Value::Bool(active && !terminal_pending),
    );
}

fn job_item_needs_attention(item: &Value) -> bool {
    matches!(
        item.get("status").and_then(Value::as_str),
        Some("failed" | "lost" | "timeout" | "timed_out")
    ) || matches!(
        item.get("recovery_state").and_then(Value::as_str),
        Some("recovering" | "lost_after_reconcile")
    ) || item.get("command_execution_state").and_then(Value::as_str) == Some("outcome_unknown")
        || item.get("error_kind").is_some()
}

fn static_guidance(
    status: Option<&str>,
    execution_state: Option<&str>,
    recovery: Option<&str>,
) -> Option<&'static str> {
    if execution_state == Some("outcome_unknown") {
        return Some("Outcome is uncertain. Observe current Job state before retrying.");
    }
    if status == Some("lost") {
        return Some(
            "Job state is lost. Re-observe runtime state before deciding whether retry is safe.",
        );
    }
    match recovery {
        Some("recovering") => {
            Some("Job recovery is in progress; this card does not poll or retry automatically.")
        }
        Some("lost_after_reconcile") => Some(
            "Job remained lost after reconciliation; inspect current runtime state before retrying.",
        ),
        _ => None,
    }
}

fn job_summary_presentation(job: &Value) -> Option<Value> {
    let job_id = job.get("job_id").and_then(bounded_text)?;
    let status = job.get("status").and_then(bounded_text)?;
    let mut item = Map::new();
    item.insert("job_id".to_string(), Value::String(job_id));
    item.insert("status".to_string(), Value::String(status.clone()));
    copy_bounded_text(job, &mut item, "project");
    copy_bounded_text(job, &mut item, "command_execution_state");
    copy_bounded_text(job, &mut item, "recovery_state");
    copy_bounded_text(job, &mut item, "recovery_reason_code");
    copy_bounded_text(job, &mut item, "recovery_reason");
    for key in ["duration_ms", "elapsed_secs", "exit_code"] {
        copy_scalar(job, &mut item, key);
    }
    apply_job_lifecycle(&mut item, &status);
    if let Some(progress) = job_progress_presentation(job) {
        item.insert("progress".to_string(), progress);
    }
    if let Some(work) = job_work_presentation(job) {
        item.insert("work".to_string(), work);
    }
    let execution_state = job.get("command_execution_state").and_then(Value::as_str);
    let recovery_state = job.get("recovery_state").and_then(Value::as_str);
    if let Some(guidance) = static_guidance(Some(&status), execution_state, recovery_state) {
        item.insert("guidance".to_string(), Value::String(guidance.to_string()));
    }
    Some(Value::Object(item))
}

fn list_jobs_presentation(output: &Value) -> Option<Value> {
    let jobs = output.get("jobs")?.as_array()?;
    let projected = jobs
        .iter()
        .filter_map(job_summary_presentation)
        .collect::<Vec<_>>();
    let foreground_count = projected
        .iter()
        .filter(|item| {
            item.get("active").and_then(Value::as_bool) == Some(true)
                || job_item_needs_attention(item)
        })
        .count();
    let routine_omitted_count = projected.len().saturating_sub(foreground_count);
    let items = projected
        .into_iter()
        .filter(|item| {
            item.get("active").and_then(Value::as_bool) == Some(true)
                || job_item_needs_attention(item)
        })
        .take(MAX_MCP_PRESENTATION_ITEMS)
        .collect::<Vec<_>>();
    let shown_active_count = items
        .iter()
        .filter(|item| item.get("active").and_then(Value::as_bool) == Some(true))
        .count();
    let shown_terminal_count = items
        .iter()
        .filter(|item| item.get("terminal").and_then(Value::as_bool) == Some(true))
        .count();
    let shown_attention_count = items
        .iter()
        .filter(|item| job_item_needs_attention(item))
        .count();
    Some(json!({
        "version": MCP_PRESENTATION_VERSION,
        "kind": "job_list",
        "count": output.get("count").and_then(Value::as_u64)?,
        "matched_count": output.get("matched_count").and_then(Value::as_u64)?,
        "truncated": output.get("truncated").and_then(Value::as_bool)?,
        "presented_count": items.len(),
        "routine_omitted_count": routine_omitted_count,
        "items_truncated": foreground_count > MAX_MCP_PRESENTATION_ITEMS,
        "shown_active_count": shown_active_count,
        "shown_terminal_count": shown_terminal_count,
        "shown_attention_count": shown_attention_count,
        "items": items,
    }))
}

fn observation_guidance(item: &Value) -> Option<&'static str> {
    let status = item.get("status").and_then(Value::as_str);
    let execution_state = item.get("command_execution_state").and_then(Value::as_str);
    let recovery_state = item.get("recovery_state").and_then(Value::as_str);
    static_guidance(status, execution_state, recovery_state)
}

fn observed_success_presentation(observation: &Value) -> Option<Value> {
    let job_id = observation.get("job_id").and_then(bounded_text)?;
    let status = observation.get("status").and_then(bounded_text)?;
    let mut item = Map::new();
    item.insert("job_id".to_string(), Value::String(job_id));
    item.insert("status".to_string(), Value::String(status.clone()));
    for key in [
        "command_execution_state",
        "recovery_state",
        "recovery_reason_code",
        "recovery_reason",
        "log_delta_status",
    ] {
        copy_bounded_text(observation, &mut item, key);
    }
    for key in [
        "changed",
        "exit_code",
        "stdout_lines",
        "stderr_lines",
        "stdout_returned_lines",
        "stderr_returned_lines",
        "stdout_truncated",
        "stderr_truncated",
        "stdout_delta_reset",
        "stderr_delta_reset",
        "earlier_stdout_unavailable",
        "earlier_stderr_unavailable",
    ] {
        copy_scalar(observation, &mut item, key);
    }
    apply_job_lifecycle(&mut item, &status);
    if let Some(progress) = job_progress_presentation(observation) {
        item.insert("progress".to_string(), progress);
    }
    if let Some(work) = job_work_presentation(observation) {
        item.insert("work".to_string(), work);
    }
    if let Some(guidance) = observation_guidance(observation) {
        item.insert("guidance".to_string(), Value::String(guidance.to_string()));
    }
    Some(Value::Object(item))
}

fn observed_failure_presentation(item: &Value) -> Option<Value> {
    let job_id = item.get("job_id").and_then(bounded_text)?;
    let mut output = Map::new();
    output.insert("job_id".to_string(), Value::String(job_id));
    for key in ["error_kind", "recovery_kind"] {
        copy_bounded_text(item, &mut output, key);
    }
    // Preserve only the exact bounded identity-recovery call. Current model
    // results carry the Adaptive gateway route; cached legacy projections may
    // still carry the canonical call. follow_up_kind is preserved because it is
    // the Host execution posture, not arbitrary presentation metadata.
    if let Some(call) = item.get("suggested_call").filter(|call| {
        **call
            == json!({
                "follow_up_kind": "fallback_recovery",
                "tool": "list_jobs",
                "arguments": {}
            })
            || **call
                == json!({
                    "follow_up_kind": "fallback_recovery",
                    "tool": "call_runtime_tool",
                    "arguments": {"tool": "list_jobs", "arguments": {}}
                })
    }) {
        output.insert("suggested_call".to_string(), call.clone());
    }
    if item.get("error_kind").and_then(Value::as_str) == Some("unknown_job") {
        output.insert(
            "guidance".to_string(),
            Value::String(
                "Job is not directly observable here. Re-observe caller-visible Jobs before retrying."
                    .to_string(),
            ),
        );
    }
    Some(Value::Object(output))
}

fn observe_jobs_presentation(output: &Value) -> Option<Value> {
    let source_items = output.get("items")?.as_array()?;
    let items = source_items
        .iter()
        .take(MAX_MCP_PRESENTATION_ITEMS)
        .filter_map(|item| {
            if item.get("success").is_some() {
                if item.get("success").and_then(Value::as_bool) == Some(true) {
                    item.get("output").and_then(observed_success_presentation)
                } else {
                    observed_failure_presentation(item)
                }
            } else {
                observed_success_presentation(item)
            }
        })
        .collect::<Vec<_>>();

    let mut presentation = Map::new();
    presentation.insert("version".to_string(), Value::from(MCP_PRESENTATION_VERSION));
    presentation.insert(
        "kind".to_string(),
        Value::String("job_observation".to_string()),
    );
    presentation.insert("items".to_string(), Value::Array(items));
    presentation.insert(
        "items_truncated".to_string(),
        Value::Bool(source_items.len() > MAX_MCP_PRESENTATION_ITEMS),
    );
    if let Some(wait) = output.get("wait").and_then(Value::as_object) {
        let mut bounded_wait = Map::new();
        if let Some(outcome) = wait.get("outcome").and_then(bounded_text) {
            bounded_wait.insert("outcome".to_string(), Value::String(outcome));
        }
        if let Some(waited_ms) = wait.get("waited_ms").filter(|value| value.is_number()) {
            bounded_wait.insert("waited_ms".to_string(), waited_ms.clone());
        }
        if !bounded_wait.is_empty() {
            presentation.insert("wait".to_string(), Value::Object(bounded_wait));
        }
    }
    for key in [
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "changed_count",
        "terminal_count",
        "output_truncated",
        "next_index",
    ] {
        if let Some(value) = output.get(key) {
            if value.is_boolean() || value.is_number() || value.is_null() {
                presentation.insert(key.to_string(), value.clone());
            }
        }
    }
    Some(Value::Object(presentation))
}

#[cfg(test)]
mod tests;
