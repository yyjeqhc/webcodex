//! Bounded multi-Job observation composed from the canonical single-Job path.

use super::{
    ObserveJobsItem, ObserveJobsWakeOn, RecoveryKind, SuggestedToolCall, ToolResult, ToolRuntime,
};
use crate::auth::AuthContext;
use crate::json_measurement::serialized_json_len;
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::Instant;
use webcodex_core::runtime_contract::{
    MAX_JOB_OBSERVATION_WAIT_SECS, MODEL_INSPECTION_MAX_RESULT_BYTES,
};
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;

pub(crate) const MAX_OBSERVE_JOBS_ITEMS: usize = 8;
pub(crate) const MAX_OBSERVE_JOBS_TAIL_LINES: usize = 200;
/// Final serialized model-facing budget for packing multiple already-bounded
/// Job observations. This does not change any single Job stream/tail retention.
const MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES: usize = MODEL_INSPECTION_MAX_RESULT_BYTES;
const MAX_OBSERVE_JOBS_ERROR_CHARS: usize = 512;

#[derive(Debug)]
struct ObservedJob {
    index: usize,
    job_id: String,
    observation_ref: Option<String>,
    result: ToolResult,
}

#[derive(Debug, Clone)]
struct ResolvedObserveJobsItem {
    index: usize,
    item: ObserveJobsItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WakeReason {
    Immediate,
    Updated,
    Terminal,
    ItemError,
    Timeout,
}

impl WakeReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Updated => "updated",
            Self::Terminal => "terminal",
            Self::ItemError => "item_error",
            Self::Timeout => "timeout",
        }
    }
}

fn observed_has_error(observed: &[ObservedJob]) -> bool {
    observed.iter().any(|item| !item.result.success)
}

fn observed_terminal_satisfied(observed: &[ObservedJob], wake_on: ObserveJobsWakeOn) -> bool {
    let mut terminal = observed
        .iter()
        .map(|item| item.result.output["terminal"].as_bool() == Some(true));
    if wake_on == ObserveJobsWakeOn::AllTerminal {
        terminal.all(|terminal| terminal)
    } else {
        terminal.any(|terminal| terminal)
    }
}

fn final_wake_reason(
    observed: &[ObservedJob],
    wake_on: ObserveJobsWakeOn,
    wait_reason: WakeReason,
) -> WakeReason {
    if observed_has_error(observed) || wait_reason == WakeReason::ItemError {
        WakeReason::ItemError
    } else if wake_on == ObserveJobsWakeOn::AllTerminal && wait_reason == WakeReason::Timeout {
        // A final snapshot may race a post-deadline completion;
        // it must not rewrite the expired shared wait as satisfied.
        WakeReason::Timeout
    } else if observed_terminal_satisfied(observed, wake_on) || wait_reason == WakeReason::Terminal
    {
        WakeReason::Terminal
    } else if wake_on == ObserveJobsWakeOn::Change
        && (observed_has_change(observed) || wait_reason == WakeReason::Updated)
    {
        WakeReason::Updated
    } else {
        WakeReason::Timeout
    }
}

fn observed_has_change(observed: &[ObservedJob]) -> bool {
    observed
        .iter()
        .any(|item| item.result.output["changed"].as_bool() == Some(true))
}

fn bounded_error(error: Option<&str>) -> String {
    let error = error.unwrap_or("Job observation failed");
    let mut chars = error.chars();
    let bounded = chars
        .by_ref()
        .take(MAX_OBSERVE_JOBS_ERROR_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

fn observation_error_kind(result: &ToolResult) -> &'static str {
    let error = result.error.as_deref().unwrap_or_default();
    if error.starts_with("unknown observation_ref:") {
        "unknown_observation_ref"
    } else if error.contains("after_observation_token") {
        "invalid_observation_token"
    } else if error.starts_with("unknown job:") {
        "unknown_job"
    } else if error.contains("local_job_observation") {
        "observation_failed"
    } else {
        "job_observation_failed"
    }
}

fn observation_recovery(error_kind: &str) -> RecoveryKind {
    match error_kind {
        "invalid_observation_token" | "unknown_observation_ref" | "output_budget_exceeded" => {
            RecoveryKind::FixInput
        }
        "unknown_job" => RecoveryKind::Reobserve,
        _ => RecoveryKind::NoAction,
    }
}

/// A presentation-only filter. Unrecognized output, warnings and test summaries stay verbatim.
/// Partial final lines stay visible because their complete meaning is not yet proven.
fn compact_validation_log(text: &str, tool: &str, stream: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            if !line.ends_with('\n') {
                return true;
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            let passed_test = stream == "stdout"
                && tool == "cargo_test"
                && trimmed.starts_with("test ")
                && trimmed.ends_with(" ... ok");
            let cargo_progress = stream == "stderr"
                && tool.starts_with("cargo_")
                && [
                    "   Compiling ",
                    "    Checking ",
                    "    Finished ",
                    "     Running ",
                ]
                .iter()
                .any(|prefix| line.starts_with(prefix));
            !(passed_test || cargo_progress)
        })
        .collect()
}

fn successful_validation_for_summary(output: &Value) -> Option<&str> {
    if output["status"] != "completed"
        || output["terminal"] != true
        || output["exit_code"].as_i64() != Some(0)
        // Structured validation Jobs may omit this supplementary process field.
        // Canonical terminal status, exit code and validation evidence below prove success;
        // an explicit contradictory or unknown process state still fails closed.
        || output.get("command_execution_state").is_some_and(|state| {
            !state.is_null() && state.as_str() != Some("completed")
        })
        || ["recovery_state", "recovery_reason_code"]
            .iter()
            .any(|key| output.get(*key).is_some_and(|value| !value.is_null()))
    {
        return None;
    }
    let validation = output.get("validation")?;
    let tool = validation.get("tool")?.as_str()?;
    if !matches!(tool, "cargo_test" | "cargo_check" | "cargo_fmt" | "go_test")
        || validation["state"] != "completed"
        || validation["passed"] != true
        || validation["truncated"] != false
        || validation["no_run"] == true
        || validation
            .get("test_count_assertion")
            .is_some_and(|assertion| !assertion.is_null() && assertion["status"] != "passed")
    {
        return None;
    }
    if matches!(tool, "cargo_test" | "go_test")
        && (validation["tests_detected"] != true
            || !validation["tests_run_count"]
                .as_u64()
                .is_some_and(|count| count > 0)
            || validation["zero_tests_run"] != false
            || validation["tests_failed"].as_u64() != Some(0))
    {
        return None;
    }
    if tool == "cargo_check" && validation["errors_count"].as_u64() != Some(0) {
        return None;
    }
    Some(tool)
}

/// Keeps canonical Job storage and observation cursors unchanged. Detail recovery deliberately
/// repeats the caller's original selection, so an advanced token cannot skip omitted log lines.
/// Apply after canonical observation/packing; never retry, wait, authorize or mutate a Job here.
pub(crate) fn summarize_observe_jobs_result(
    result: &mut ToolResult,
    originals: &[ObserveJobsItem],
    tail_lines: usize,
) {
    if !result.success {
        return;
    }
    let tail_lines = tail_lines.clamp(1, MAX_OBSERVE_JOBS_TAIL_LINES);
    let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        if item["success"] != true {
            continue;
        }
        let Some(original) = item["index"]
            .as_u64()
            .and_then(|index| originals.get(index as usize))
        else {
            continue;
        };
        if !original.job_id.is_empty() && item["job_id"].as_str() != Some(original.job_id.as_str())
        {
            continue;
        }
        let output = &item["output"];
        let Some(tool) = successful_validation_for_summary(output) else {
            continue;
        };
        let mut compact = output.clone();
        let mut omitted = Vec::new();
        for stream in ["stdout", "stderr"] {
            let key = format!("{stream}_tail");
            let Some(text) = output.get(&key).and_then(Value::as_str) else {
                continue;
            };
            let projected = compact_validation_log(text, tool, stream);
            if projected != text {
                compact[format!("{stream}_returned_lines")] = json!(projected.lines().count());
                compact[key] = json!(projected);
                omitted.push(stream);
            }
        }
        if omitted.is_empty() {
            continue;
        }
        compact["logs_omitted"] = json!(omitted);
        compact["suggested_call"] = SuggestedToolCall::new(
            "observe_jobs",
            json!({
                "items": [observe_jobs_item_argument_value(original)],
                "tail_lines": tail_lines,
                "summary_only": false,
            }),
        )
        .to_value();
        // Do not make tiny results larger merely to call them summaries.
        if serialized_json_len(&compact).unwrap_or(usize::MAX)
            < serialized_json_len(output).unwrap_or(0)
        {
            item["output"] = compact;
        }
    }
    if let Some(arguments) = result.output.pointer_mut("/suggested_call/arguments") {
        arguments["summary_only"] = json!(true);
    }
}

fn batch_item(observed: ObservedJob) -> Value {
    if observed.result.success {
        let mut output = observed.result.output;
        if let Some(output) = output.as_object_mut() {
            // observe_jobs owns one shared wait for the batch. The final item
            // refreshes are snapshots only; leaking job_log's internal
            // non-waiting metadata would present a second, contradictory wait
            // fact to the model.
            output.remove("wait_outcome");
            output.remove("waited_ms");
            output.remove("continuation_semantics");
        }
        let mut item = json!({
            "index": observed.index,
            "job_id": observed.job_id,
            "success": true,
            "output": output,
            "error_kind": null,
            "error": null,
        });
        if let Some(observation_ref) = observed.observation_ref {
            item["observation_ref"] = json!(observation_ref);
        }
        item
    } else {
        let error_kind = observation_error_kind(&observed.result);
        let recovery_kind = observation_recovery(error_kind);
        let mut item = json!({
            "index": observed.index,
            "success": false,
            "output": null,
            "error_kind": error_kind,
            "error": bounded_error(observed.result.error.as_deref()),
        });
        item["job_id"] = if observed.job_id.is_empty() {
            Value::Null
        } else {
            json!(observed.job_id)
        };
        if let Some(observation_ref) = observed.observation_ref {
            item["observation_ref"] = json!(observation_ref);
        }
        if error_kind == "unknown_job" {
            item["suggested_call"] = SuggestedToolCall::new("list_jobs", json!({})).to_value();
        } else {
            item["recovery_kind"] = json!(recovery_kind.as_str());
        }
        item
    }
}

fn output_budget_failure_item(index: usize, job_id: String) -> Value {
    json!({
        "index": index,
        "job_id": job_id,
        "success": false,
        "output": null,
        "error_kind": "output_budget_exceeded",
        "recovery_kind": RecoveryKind::FixInput.as_str(),
        "error": "The bounded Job observation cannot fit in one model result; resubmit this Job with a smaller tail_lines value.",
    })
}

fn batch_output(
    requested_count: usize,
    items: Vec<Value>,
    wake_reason: WakeReason,
    waited_ms: u64,
    output_truncated: bool,
    next_index: Option<usize>,
) -> Value {
    let succeeded_count = items
        .iter()
        .filter(|item| item["success"].as_bool() == Some(true))
        .count();
    let returned_count = items.len();
    let changed_count = items
        .iter()
        .filter(|item| {
            item["success"].as_bool() == Some(true)
                && item["output"]["changed"].as_bool() == Some(true)
        })
        .count();
    let terminal_count = items
        .iter()
        .filter(|item| {
            item["success"].as_bool() == Some(true)
                && item["output"]["terminal"].as_bool() == Some(true)
        })
        .count();
    let mut output = json!({
        "requested_count": requested_count,
        "returned_count": returned_count,
        "succeeded_count": succeeded_count,
        "failed_count": returned_count - succeeded_count,
        "items": items,
        "wait": {
            "outcome": wake_reason.as_str(),
            "waited_ms": waited_ms,
        },
        "changed_count": changed_count,
        "terminal_count": terminal_count,
        "output_truncated": output_truncated,
    });
    if let Some(next_index) = next_index {
        output["next_index"] = json!(next_index);
    }
    output
}

fn observe_jobs_item_argument_value(item: &ObserveJobsItem) -> Value {
    if let Some(observation_ref) = item.observation_ref.as_deref() {
        return json!({"observation_ref": observation_ref});
    }
    let mut value = json!({"job_id": item.job_id});
    if let Some(token) = item.after_observation_token.as_deref() {
        value["after_observation_token"] = json!(token);
    }
    value
}

fn add_actionable_batch_continuation(
    output: &mut Value,
    original_items: &[ObserveJobsItem],
    tail_lines: usize,
) {
    let Some(root) = output.as_object_mut() else {
        return;
    };
    let Some(next_index) = root.get("next_index").and_then(Value::as_u64) else {
        return;
    };
    let Some(remaining) = original_items
        .get(next_index as usize..)
        .filter(|items| !items.is_empty())
    else {
        return;
    };
    root.insert(
        "suggested_call".to_string(),
        SuggestedToolCall::new(
            "observe_jobs",
            json!({
                "items": remaining.iter().map(observe_jobs_item_argument_value).collect::<Vec<_>>(),
                "tail_lines": tail_lines,
            }),
        )
        .to_value(),
    );
    root.remove("next_index");
}

fn serialized_batch_fits(output: &Value) -> bool {
    serialized_json_len(&ToolResult::ok(output.clone()))
        .map(|bytes| {
            bytes
                <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
                    .saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES)
        })
        .unwrap_or(false)
}

fn apply_output_budget(
    requested_count: usize,
    completed: Vec<Value>,
    wake_reason: WakeReason,
    waited_ms: u64,
) -> Result<Value, String> {
    let mut returned = Vec::with_capacity(completed.len());
    let mut next_index = None;

    for item in completed {
        let index = item["index"].as_u64().unwrap_or(returned.len() as u64) as usize;
        let mut candidate_items = returned.clone();
        candidate_items.push(item.clone());
        let candidate = batch_output(
            requested_count,
            candidate_items,
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if serialized_batch_fits(&candidate) {
            returned.push(item);
            continue;
        }

        let single = batch_output(
            requested_count,
            vec![item.clone()],
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if serialized_batch_fits(&single) {
            next_index = Some(index);
            break;
        }

        let budget_failure =
            output_budget_failure_item(index, item["job_id"].as_str().unwrap_or_default().into());
        let mut candidate_items = returned.clone();
        candidate_items.push(budget_failure.clone());
        let candidate = batch_output(
            requested_count,
            candidate_items,
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if !serialized_batch_fits(&candidate) {
            if returned.is_empty() {
                return Err(
                    "observe_jobs could not encode a bounded output-budget failure item".into(),
                );
            }
            next_index = Some(index);
            break;
        }
        returned.push(budget_failure);
    }

    Ok(batch_output(
        requested_count,
        returned,
        wake_reason,
        waited_ms,
        next_index.is_some(),
        next_index,
    ))
}

fn copy_non_null(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key).filter(|value| !value.is_null()) {
        target.insert(key.to_string(), value.clone());
    }
}

fn copy_present(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

fn sparse_success_item(item: &Value) -> Option<Value> {
    let item = item.as_object()?;
    if item.get("success").and_then(Value::as_bool) != Some(true)
        || !item.get("error_kind").is_some_and(Value::is_null)
        || !item.get("error").is_some_and(Value::is_null)
        || item.get("recovery_kind").is_some()
        || item.get("suggested_call").is_some()
    {
        return None;
    }
    let job_id = item.get("job_id")?.as_str()?.to_string();
    let observation = item.get("output")?.as_object()?;
    if observation.get("job_id")?.as_str()? != job_id {
        return None;
    }
    let status = observation.get("status")?.as_str()?.to_string();
    let terminal = observation.get("terminal")?.as_bool()?;
    let changed = observation.get("changed")?.as_bool()?;
    let log_delta_status = observation.get("log_delta_status")?.as_str()?;
    if !matches!(
        log_delta_status,
        "baseline" | "delta" | "unchanged" | "reset"
    ) {
        return None;
    }
    let observation_token = observation.get("observation_token")?.as_str()?;
    if observation_token.is_empty() {
        return None;
    }
    let stdout_tail = observation.get("stdout_tail")?.as_str()?;
    let stderr_tail = observation.get("stderr_tail")?.as_str()?;
    let stdout_truncated = observation.get("stdout_truncated")?.as_bool()?;
    let stderr_truncated = observation.get("stderr_truncated")?.as_bool()?;
    let stdout_delta_reset = observation.get("stdout_delta_reset")?.as_bool()?;
    let stderr_delta_reset = observation.get("stderr_delta_reset")?.as_bool()?;
    let earlier_stdout_unavailable = observation
        .get("earlier_stdout_unavailable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let earlier_stderr_unavailable = observation
        .get("earlier_stderr_unavailable")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut sparse = serde_json::Map::new();
    sparse.insert("job_id".to_string(), json!(job_id));
    sparse.insert("status".to_string(), json!(status));
    sparse.insert("terminal".to_string(), json!(terminal));
    sparse.insert("changed".to_string(), json!(changed));
    sparse.insert("log_delta_status".to_string(), json!(log_delta_status));
    sparse.insert("observation_token".to_string(), json!(observation_token));
    if let Some(observation_ref) = item.get("observation_ref").and_then(Value::as_str) {
        sparse.insert("observation_ref".to_string(), json!(observation_ref));
    }

    for key in [
        "exit_code",
        "command_execution_state",
        "activity",
        "detected_summary",
        "validation",
        "ssh_resource",
        "logs_omitted",
        "suggested_call",
    ] {
        copy_non_null(observation, &mut sparse, key);
    }

    if log_delta_status != "unchanged" {
        copy_present(observation, &mut sparse, "stdout_tail");
        copy_present(observation, &mut sparse, "stderr_tail");
    } else {
        if !stdout_tail.is_empty() {
            copy_present(observation, &mut sparse, "stdout_tail");
        }
        if !stderr_tail.is_empty() {
            copy_present(observation, &mut sparse, "stderr_tail");
        }
    }

    for (key, present) in [
        ("stdout_truncated", stdout_truncated),
        ("stderr_truncated", stderr_truncated),
        ("stdout_delta_reset", stdout_delta_reset),
        ("stderr_delta_reset", stderr_delta_reset),
        ("earlier_stdout_unavailable", earlier_stdout_unavailable),
        ("earlier_stderr_unavailable", earlier_stderr_unavailable),
    ] {
        if present {
            copy_present(observation, &mut sparse, key);
        }
    }
    for key in ["recovery_state", "recovery_reason_code", "recovery_reason"] {
        copy_non_null(observation, &mut sparse, key);
    }

    let exceptional_log_evidence = log_delta_status == "reset"
        || stdout_truncated
        || stderr_truncated
        || stdout_delta_reset
        || stderr_delta_reset
        || earlier_stdout_unavailable
        || earlier_stderr_unavailable
        || observation
            .get("recovery_state")
            .is_some_and(|value| !value.is_null())
        || observation
            .get("recovery_reason_code")
            .is_some_and(|value| !value.is_null())
        || observation
            .get("recovery_reason")
            .is_some_and(|value| !value.is_null());
    if exceptional_log_evidence {
        for key in [
            "stdout_lines",
            "stderr_lines",
            "stdout_returned_lines",
            "stderr_returned_lines",
            "stdout_retained_from_line",
            "stderr_retained_from_line",
            "cursor",
            "last_update_seq",
        ] {
            copy_non_null(observation, &mut sparse, key);
        }
        if log_delta_status == "reset" {
            for key in [
                "stdout_truncated",
                "stderr_truncated",
                "stdout_delta_reset",
                "stderr_delta_reset",
                "earlier_stdout_unavailable",
                "earlier_stderr_unavailable",
            ] {
                copy_present(observation, &mut sparse, key);
            }
        }
    }

    if matches!(log_delta_status, "baseline" | "reset") {
        for key in ["purpose", "command_summary"] {
            copy_non_null(observation, &mut sparse, key);
        }
    }
    Some(Value::Object(sparse))
}

/// Final model-facing projection for ordinary successful Job observations.
/// The canonical batch and canonical single-Job snapshots remain unchanged for
/// budgeting, Session/audit recording, operator diagnostics, and internal use.
pub(crate) fn sparsify_observe_jobs_model_result(result: &mut ToolResult) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    if output.get("output_truncated").and_then(Value::as_bool) == Some(true) {
        if output.get("suggested_call").is_some() {
            output.remove("next_index");
            output.remove("continuation_semantics");
        }
        return;
    }
    if output.get("output_truncated").and_then(Value::as_bool) != Some(false)
        || output
            .get("next_index")
            .is_some_and(|value| !value.is_null())
    {
        return;
    }
    let Some(items) = output.get("items").and_then(Value::as_array) else {
        return;
    };
    if items.is_empty() || items.len() > MAX_OBSERVE_JOBS_ITEMS {
        return;
    }
    let count = items.len() as u64;
    if output.get("requested_count").and_then(Value::as_u64) != Some(count)
        || output.get("returned_count").and_then(Value::as_u64) != Some(count)
        || output.get("succeeded_count").and_then(Value::as_u64) != Some(count)
        || output.get("failed_count").and_then(Value::as_u64) != Some(0)
    {
        return;
    }
    let Some(wait) = output.get("wait").and_then(Value::as_object) else {
        return;
    };
    let Some(wait_outcome) = wait.get("outcome").and_then(Value::as_str) else {
        return;
    };
    if !matches!(
        wait_outcome,
        "immediate" | "updated" | "terminal" | "timeout"
    ) {
        // item_error is intentionally kept in the canonical batch shape.
        return;
    }
    let Some(waited_ms) = wait.get("waited_ms").and_then(Value::as_u64) else {
        return;
    };

    let mut sparse_items = Vec::with_capacity(items.len());
    let mut changed_count = 0u64;
    let mut terminal_count = 0u64;
    for item in items {
        let Some(sparse) = sparse_success_item(item) else {
            return;
        };
        changed_count += u64::from(sparse.get("changed").and_then(Value::as_bool) == Some(true));
        terminal_count += u64::from(sparse.get("terminal").and_then(Value::as_bool) == Some(true));
        sparse_items.push(sparse);
    }
    if output.get("changed_count").and_then(Value::as_u64) != Some(changed_count)
        || output.get("terminal_count").and_then(Value::as_u64) != Some(terminal_count)
    {
        return;
    }

    output.insert("items".to_string(), Value::Array(sparse_items));
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
        output.remove(key);
    }
    if waited_ms == 0 {
        if let Some(wait) = output.get_mut("wait").and_then(Value::as_object_mut) {
            wait.remove("waited_ms");
        }
    }
}

fn normalize_observe_jobs_preferences(
    tail_lines: usize,
    wait_secs: Option<u64>,
) -> (usize, Option<u64>) {
    (
        tail_lines.min(MAX_OBSERVE_JOBS_TAIL_LINES),
        wait_secs.map(|wait_secs| wait_secs.min(MAX_JOB_OBSERVATION_WAIT_SECS)),
    )
}

impl ToolRuntime {
    fn validate_observe_jobs_input(
        items: &[ObserveJobsItem],
        tail_lines: usize,
        wait_secs: Option<u64>,
    ) -> Result<(), String> {
        if items.len() > MAX_OBSERVE_JOBS_ITEMS {
            return Err("observe_jobs accepts at most 8 resolved items".into());
        }
        if items
            .iter()
            .any(|item| item.job_id.trim().is_empty() || item.observation_ref.is_some())
        {
            return Err(
                "observe_jobs requires every resolved item to have only a non-empty job_id".into(),
            );
        }
        if let Some(item) = items.iter().find(|item| {
            item.after_observation_token.as_ref().is_some_and(|token| {
                token.len() > crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
            })
        }) {
            return Err(format!(
                "observe_jobs token for job_id {} exceeds 192 bytes",
                item.job_id
            ));
        }
        if tail_lines == 0 {
            return Err("observe_jobs tail_lines must be at least 1".into());
        }
        if wait_secs == Some(0) {
            return Err("observe_jobs wait_secs must be at least 1".into());
        }
        let mut seen = HashSet::with_capacity(items.len());
        if let Some(duplicate) = items
            .iter()
            .map(|item| item.job_id.as_str())
            .find(|job_id| !seen.insert(*job_id))
        {
            return Err(format!(
                "observe_jobs rejects duplicate job_id values: {duplicate}"
            ));
        }
        Ok(())
    }

    async fn observe_jobs_pass(
        &self,
        items: &[ResolvedObserveJobsItem],
        tail_lines: usize,
        auth: Option<&AuthContext>,
    ) -> Vec<ObservedJob> {
        let mut observed: Vec<ObservedJob> =
            stream::iter(items.iter().cloned().map(|resolved| async move {
                let item = resolved.item;
                let result = self
                    .job_log_for_auth(
                        item.job_id.clone(),
                        None,
                        Some(tail_lines),
                        auth,
                        item.after_observation_token,
                        None,
                    )
                    .await;
                ObservedJob {
                    index: resolved.index,
                    job_id: item.job_id,
                    observation_ref: None,
                    result,
                }
            }))
            .buffer_unordered(MAX_OBSERVE_JOBS_ITEMS)
            .collect()
            .await;
        observed.sort_by_key(|item| item.index);
        observed
    }

    async fn wait_for_observed_jobs(
        &self,
        items: &[ResolvedObserveJobsItem],
        auth: Option<&AuthContext>,
        wait_secs: u64,
        wake_on: ObserveJobsWakeOn,
        deadline: Instant,
    ) -> Result<WakeReason, String> {
        // Each Job keeps its own waiter across other Jobs' updates. The
        // canonical Notify + revision recheck covers updates both before and
        // during wait registration; no polling heartbeat is needed here.
        let mut waits = stream::iter(items.iter().cloned().map(|resolved| async move {
            let mut item = resolved.item;
            loop {
                if Instant::now() >= deadline {
                    return Ok::<WakeReason, String>(WakeReason::Timeout);
                }
                let result = self
                    .job_log_for_auth(
                        item.job_id.clone(),
                        None,
                        Some(1),
                        auth,
                        item.after_observation_token.clone(),
                        Some(wait_secs),
                    )
                    .await;
                if !result.success {
                    return Ok(WakeReason::ItemError);
                }
                if result.output["terminal"].as_bool() == Some(true) {
                    return Ok(WakeReason::Terminal);
                }
                if result.output["changed"].as_bool() == Some(true) {
                    if wake_on == ObserveJobsWakeOn::Change {
                        return Ok(WakeReason::Updated);
                    }
                    // Private wait cursor only: final requested-tail refresh
                    // still uses the caller's original token for every delta.
                    let token = result.output["observation_token"]
                        .as_str()
                        .filter(|token| !token.is_empty())
                        .ok_or("observe_jobs canonical wait returned no observation token")?;
                    if item.after_observation_token.as_deref() == Some(token) {
                        return Err("observe_jobs canonical wait did not advance its token".into());
                    }
                    item.after_observation_token = Some(token.to_string());
                } else if result.output["wait_outcome"].as_str() == Some("timeout") {
                    return Ok::<WakeReason, String>(WakeReason::Timeout);
                } else {
                    return Err(
                        "observe_jobs canonical wait returned an invalid wait outcome".into(),
                    );
                }
            }
        }))
        .buffer_unordered(MAX_OBSERVE_JOBS_ITEMS);
        // This one absolute deadline also bounds every re-entered canonical
        // wait. Non-terminal updates never reset or extend the batch duration.
        let wait = async {
            while let Some(reason) = waits.next().await {
                let reason = reason?;
                if wake_on != ObserveJobsWakeOn::AllTerminal || reason != WakeReason::Terminal {
                    return Ok(reason);
                }
                // A terminal Job leaves the pending set; the other canonical
                // waiters retain their private cursors and registrations.
            }
            Ok(WakeReason::Terminal)
        };
        match tokio::time::timeout_at(deadline, wait).await {
            Ok(reason) => reason,
            Err(_) => Ok(WakeReason::Timeout),
        }
    }

    pub(crate) async fn observe_jobs_for_auth(
        &self,
        items: Vec<ObserveJobsItem>,
        tail_lines: usize,
        wait_secs: Option<u64>,
        wake_on: ObserveJobsWakeOn,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let (tail_lines, wait_secs) = normalize_observe_jobs_preferences(tail_lines, wait_secs);
        if !(1..=MAX_OBSERVE_JOBS_ITEMS).contains(&items.len()) {
            return ToolResult::err("observe_jobs requires between 1 and 8 items");
        }
        if tail_lines == 0 {
            return ToolResult::err("observe_jobs tail_lines must be at least 1");
        }
        if wait_secs == Some(0) {
            return ToolResult::err("observe_jobs wait_secs must be at least 1");
        }

        let requested_count = items.len();
        let original_items = items.clone();
        let has_ref_selector = items.iter().any(|item| item.observation_ref.is_some());
        let principal_key =
            match crate::tool_runtime::session_context::runtime_observation_principal(auth) {
                Ok((kind, id)) => Some(format!("{kind}:{id}")),
                Err(error) if has_ref_selector => return ToolResult::err(error),
                Err(_) => None,
            };

        let mut resolved = Vec::with_capacity(items.len());
        let mut unresolved_refs = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            if let Some(observation_ref) = item.observation_ref.as_deref() {
                let Some(principal_key) = principal_key.as_deref() else {
                    return ToolResult::err(
                        "runtime_observation_unavailable: observation_ref requires a stable principal",
                    );
                };
                match self
                    .observation_ref_registry
                    .resolve(principal_key, observation_ref)
                {
                    Some((job_id, token)) => resolved.push(ResolvedObserveJobsItem {
                        index,
                        item: ObserveJobsItem::resolved(
                            job_id,
                            Some(token).filter(|token| !token.is_empty()),
                        ),
                    }),
                    None => unresolved_refs.push((index, observation_ref.to_string())),
                }
            } else {
                if item.job_id.trim().is_empty() {
                    return ToolResult::err(
                        "observe_jobs requires each item to supply job_id or observation_ref",
                    );
                }
                resolved.push(ResolvedObserveJobsItem { index, item });
            }
        }

        let canonical_items = resolved
            .iter()
            .map(|resolved| resolved.item.clone())
            .collect::<Vec<_>>();
        if let Err(error) =
            Self::validate_observe_jobs_input(&canonical_items, tail_lines, wait_secs)
        {
            return ToolResult::err(error);
        }

        let unresolved_observations = || {
            unresolved_refs
                .iter()
                .map(|(index, observation_ref)| ObservedJob {
                    index: *index,
                    job_id: String::new(),
                    observation_ref: Some(observation_ref.clone()),
                    result: ToolResult::err(format!(
                        "unknown observation_ref: {observation_ref}; use the retained raw job_id + observation_token pair"
                    )),
                })
                .collect::<Vec<_>>()
        };

        let mut initial = self.observe_jobs_pass(&resolved, tail_lines, auth).await;
        initial.extend(unresolved_observations());
        initial.sort_by_key(|item| item.index);

        let missing_baseline = resolved
            .iter()
            .any(|resolved| resolved.item.after_observation_token.is_none());
        let immediate_reason =
            if wake_on == ObserveJobsWakeOn::AllTerminal && observed_has_error(&initial) {
                Some(WakeReason::ItemError)
            } else if wait_secs.is_none() || missing_baseline {
                Some(WakeReason::Immediate)
            } else if observed_has_error(&initial) {
                Some(WakeReason::ItemError)
            } else if observed_terminal_satisfied(&initial, wake_on) {
                Some(WakeReason::Terminal)
            } else if wake_on == ObserveJobsWakeOn::Change && observed_has_change(&initial) {
                Some(WakeReason::Updated)
            } else {
                None
            };

        let (observed, wake_reason, waited_ms) = if let Some(reason) = immediate_reason {
            (initial, reason, 0)
        } else {
            let wait_secs = wait_secs.expect("shared wait requires validated wait_secs");
            let wait_started = Instant::now();
            let pending: Vec<_> = resolved
                .iter()
                .filter(|resolved| {
                    wake_on != ObserveJobsWakeOn::AllTerminal
                        || initial
                            .iter()
                            .find(|observed| observed.index == resolved.index)
                            .is_none_or(|observed| {
                                observed.result.output["terminal"].as_bool() != Some(true)
                            })
                })
                .cloned()
                .collect();
            let wait_reason = match self
                .wait_for_observed_jobs(
                    &pending,
                    auth,
                    wait_secs,
                    wake_on,
                    wait_started + Duration::from_secs(wait_secs),
                )
                .await
            {
                Ok(reason) => reason,
                Err(error) => return ToolResult::err(error),
            };
            let waited_ms = wait_started.elapsed().as_millis() as u64;
            let mut refreshed = self.observe_jobs_pass(&resolved, tail_lines, auth).await;
            refreshed.extend(unresolved_observations());
            refreshed.sort_by_key(|item| item.index);
            let final_reason = final_wake_reason(&refreshed, wake_on, wait_reason);
            (refreshed, final_reason, waited_ms)
        };

        let mut completed = observed.into_iter().map(batch_item).collect::<Vec<_>>();
        if let Some(principal_key) = principal_key.as_deref() {
            for item in &mut completed {
                if item["success"] != true {
                    continue;
                }
                let Some(job_id) = item.get("job_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(token) = item
                    .get("output")
                    .and_then(|output| output.get("observation_token"))
                    .and_then(Value::as_str)
                    .filter(|token| !token.is_empty())
                else {
                    continue;
                };
                let observation_ref =
                    self.observation_ref_registry
                        .mint(principal_key, job_id, token);
                item["observation_ref"] = json!(observation_ref);
            }
        }

        match apply_output_budget(requested_count, completed, wake_reason, waited_ms) {
            Ok(mut output) => {
                add_actionable_batch_continuation(&mut output, &original_items, tail_lines);
                ToolResult::ok(output)
            }
            Err(error) => ToolResult::err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_terminal_deadline_is_not_rewritten_by_a_racing_final_snapshot() {
        let completed = vec![ObservedJob {
            index: 0,
            job_id: "job".into(),
            observation_ref: None,
            result: ToolResult::ok(json!({"terminal":true,"changed":true})),
        }];
        assert_eq!(
            final_wake_reason(
                &completed,
                ObserveJobsWakeOn::AllTerminal,
                WakeReason::Timeout
            ),
            WakeReason::Timeout
        );
        assert_eq!(
            final_wake_reason(
                &completed,
                ObserveJobsWakeOn::AllTerminal,
                WakeReason::Terminal
            ),
            WakeReason::Terminal
        );
        assert_eq!(
            final_wake_reason(
                &completed,
                ObserveJobsWakeOn::AllTerminal,
                WakeReason::ItemError
            ),
            WakeReason::ItemError
        );
    }

    #[test]
    fn bounded_error_limits_character_count() {
        let error = "雪".repeat(MAX_OBSERVE_JOBS_ERROR_CHARS + 10);
        let bounded = bounded_error(Some(&error));
        assert_eq!(bounded.chars().count(), MAX_OBSERVE_JOBS_ERROR_CHARS + 1);
        assert!(bounded.ends_with('…'));
    }

    #[test]
    fn oversized_observation_preferences_are_clamped() {
        assert_eq!(
            normalize_observe_jobs_preferences(500, Some(120)),
            (
                MAX_OBSERVE_JOBS_TAIL_LINES,
                Some(MAX_JOB_OBSERVATION_WAIT_SECS)
            )
        );
        assert_eq!(
            normalize_observe_jobs_preferences(40, Some(5)),
            (40, Some(5))
        );
    }

    #[test]
    fn batch_item_failures_expose_bounded_recovery_and_success_omits_it() {
        let missing = batch_item(ObservedJob {
            index: 0,
            job_id: "job-missing".to_string(),
            observation_ref: None,
            result: ToolResult::err("unknown job: job-missing"),
        });
        assert_eq!(missing["error_kind"], "unknown_job");
        assert!(missing.get("recovery_kind").is_none());
        assert!(missing.get("recovery_tool").is_none());
        let suggested = &missing["suggested_call"];
        assert_eq!(suggested, &json!({"tool": "list_jobs", "arguments": {}}));
        let parsed = crate::tool_runtime::ToolCall::from_tool_name(
            suggested["tool"].as_str().unwrap(),
            suggested["arguments"].clone(),
        )
        .expect("unknown Job recovery must be parser-ready");
        match parsed {
            crate::tool_runtime::ToolCall::ListJobs {
                project,
                session_id,
                ..
            } => {
                assert!(project.is_none());
                assert!(session_id.is_none());
            }
            other => panic!("unexpected recovery call: {}", other.tool_name()),
        }
        assert!(
            crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(
                suggested["tool"].as_str().unwrap()
            )
        );

        let invalid_token = batch_item(ObservedJob {
            index: 1,
            job_id: "job-token".to_string(),
            observation_ref: None,
            result: ToolResult::err("invalid after_observation_token"),
        });
        assert_eq!(invalid_token["recovery_kind"], "fix_input");
        assert!(invalid_token.get("recovery_tool").is_none());
        assert!(invalid_token.get("suggested_call").is_none());

        let success = batch_item(ObservedJob {
            index: 2,
            job_id: "job-ok".to_string(),
            observation_ref: None,
            result: ToolResult::ok(json!({"changed": false})),
        });
        assert!(success.get("recovery_kind").is_none());
        assert!(success.get("recovery_tool").is_none());
        assert!(success.get("suggested_call").is_none());
    }

    #[test]
    fn batch_continuation_is_parser_ready_and_omits_absent_tokens() {
        let originals = vec![
            ObserveJobsItem {
                job_id: "job-0".to_string(),
                after_observation_token: Some("token-0".to_string()),
                observation_ref: None,
            },
            ObserveJobsItem {
                job_id: "job-1".to_string(),
                after_observation_token: None,
                observation_ref: None,
            },
            ObserveJobsItem {
                job_id: "job-2".to_string(),
                after_observation_token: Some("token-2".to_string()),
                observation_ref: None,
            },
        ];
        let mut output = json!({
            "output_truncated": true,
            "next_index": 1,
        });
        add_actionable_batch_continuation(&mut output, &originals, 17);
        assert!(output.get("next_index").is_none());
        let suggested = &output["suggested_call"];
        assert_eq!(suggested["tool"], "observe_jobs");
        assert_eq!(suggested["arguments"]["tail_lines"], 17);
        assert!(suggested["arguments"].get("wait_secs").is_none());
        assert!(suggested["arguments"].get("wake_on").is_none());
        let items = suggested["arguments"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["job_id"], "job-1");
        assert!(items[0].get("after_observation_token").is_none());
        assert_eq!(items[1]["after_observation_token"], "token-2");
        crate::tool_runtime::ToolCall::from_tool_name(
            suggested["tool"].as_str().unwrap(),
            suggested["arguments"].clone(),
        )
        .expect("aggregate Job follow-up must be parser-ready");
    }

    #[test]
    fn output_budget_replaces_one_oversized_item_without_partial_json() {
        let item = json!({
            "index": 0,
            "job_id": "job-0",
            "success": true,
            "output": {
                "changed": false,
                "terminal": false,
                "stdout_tail": "x".repeat(MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES),
            },
            "error_kind": null,
            "error": null,
        });
        let output = apply_output_budget(1, vec![item], WakeReason::Immediate, 0).unwrap();
        assert_eq!(output["wait"]["outcome"], "immediate");
        assert_eq!(output["wait"]["waited_ms"], 0);
        assert_eq!(output["returned_count"], 1);
        assert_eq!(output["items"][0]["success"], false);
        assert_eq!(output["items"][0]["error_kind"], "output_budget_exceeded");
        assert_eq!(output["items"][0]["recovery_kind"], "fix_input");
        assert_eq!(output["output_truncated"], false);
        assert!(
            serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
                <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
        );
    }

    #[test]
    fn output_budget_keeps_whole_prefix_and_points_at_first_omitted_index() {
        let item = |index| {
            json!({
                "index": index,
                "job_id": format!("job-{index}"),
                "success": true,
                "output": {
                    "changed": false,
                    "terminal": false,
                    "stdout_tail": "x".repeat(90_000),
                },
                "error_kind": null,
                "error": null,
            })
        };
        let output = apply_output_budget(
            4,
            vec![item(0), item(1), item(2), item(3)],
            WakeReason::Immediate,
            0,
        )
        .unwrap();
        // Four ~90 KiB observations straddle the old 256 KiB aggregate budget
        // but fit comfortably inside the explicit 512 KiB model-facing packer.
        assert_eq!(output["returned_count"], 4);
        assert_eq!(output["output_truncated"], false);
        assert!(output.get("next_index").is_none());
        assert_eq!(output["wait"]["outcome"], "immediate");
        assert!(
            serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
                <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
        );

        let output =
            apply_output_budget(8, (0..8).map(item).collect(), WakeReason::Immediate, 0).unwrap();
        assert!(output["returned_count"].as_u64().unwrap() > 2);
        assert!(output["returned_count"].as_u64().unwrap() < 8);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(
            output["next_index"], output["returned_count"],
            "next_index must identify the first whole observation omitted by aggregate packing"
        );
        assert!(
            serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
                <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
        );
    }

    #[test]
    fn aggregate_ceiling_does_not_expand_single_job_snapshot_or_tail_contracts() {
        assert_eq!(MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES, 512 * 1024);
        assert_eq!(MAX_OBSERVE_JOBS_TAIL_LINES, 200);
        assert_eq!(
            webcodex_core::runtime_contract::DEFAULT_OBSERVE_JOBS_TAIL_LINES,
            40
        );
        assert_eq!(
            webcodex_core::runner_protocol::JOB_SNAPSHOT_STREAM_MAX_BYTES,
            64 * 1024
        );
    }
}
