//! Canonical Runner mutation delivery certainty and transactional effect receipts.

use super::preflight::recoverable_write_rejection;
use super::*;
use crate::tool_runtime::edit_outcome::EditOutcomeFacts;

fn compact_model_edit_surface(tool_name: &str) -> bool {
    matches!(tool_name, "edit_project_files" | "write_project_file")
}

pub(super) fn structured_edit_not_started_result(
    tool_name: &str,
    reason: impl AsRef<str>,
) -> ToolResult {
    let mut output = json!({
        "execution_state": "not_started",
        "state_changed": false,
        "error_kind": "not_started",
        "failure_kind": "not_started",
        "tool_failure": true,
    });
    if !compact_model_edit_surface(tool_name) {
        output["recovery_action"] = json!("retry_same_after_runner_recovery");
    }
    ToolResult::err_with_output(
        format!(
            "{tool_name} was not dispatched: {}. No files were modified by this request.",
            reason.as_ref()
        ),
        output,
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::RetrySame)
}

pub(super) fn structured_edit_outcome_unknown_result(
    tool_name: &str,
    reason: impl AsRef<str>,
    mut output: Value,
) -> ToolResult {
    if !output.is_object() {
        output = json!({});
    }
    let fields = output
        .as_object_mut()
        .expect("structured edit uncertainty output normalized to object");
    for key in [
        "conflict_recovery",
        "retry_guidance",
        "state_changed",
        "execution_state",
        "error_kind",
        "failure_kind",
        "tool_failure",
        "recovery_action",
        "recovery_kind",
        "recovery_tool",
        "error",
    ] {
        fields.remove(key);
    }
    fields.insert("execution_state".to_string(), json!("outcome_unknown"));
    fields.insert("state_changed".to_string(), Value::Null);
    fields.insert("error_kind".to_string(), json!("outcome_unknown"));
    fields.insert("failure_kind".to_string(), json!("outcome_unknown"));
    fields.insert("tool_failure".to_string(), json!(true));
    if !compact_model_edit_surface(tool_name) {
        fields.insert(
            "recovery_action".to_string(),
            json!("inspect_workspace_before_retry"),
        );
    }
    ToolResult::err_with_output(
        format!(
            "{tool_name} outcome is unknown: {}. The Runner may already have changed files. Inspect current workspace state before issuing another write.",
            reason.as_ref()
        ),
        output,
    )
    .with_recovery(crate::tool_runtime::RecoveryKind::Reobserve)
}

fn structured_edit_delivery_failure(
    tool_name: &str,
    reason: impl AsRef<str>,
    state: ShellCommandExecutionState,
) -> ToolResult {
    if state == ShellCommandExecutionState::NotStarted {
        structured_edit_not_started_result(tool_name, reason)
    } else {
        structured_edit_outcome_unknown_result(tool_name, reason, json!({}))
    }
}

pub(super) async fn await_structured_edit_response(
    runtime: &ToolRuntime,
    request_id: &str,
    rx: tokio::sync::oneshot::Receiver<ShellRunResponse>,
    wait_timeout_secs: u64,
    tool_name: &str,
) -> Result<ShellRunResponse, ToolResult> {
    match tokio::time::timeout(Duration::from_secs(wait_timeout_secs + 4), rx).await {
        Ok(Ok(response)) => {
            if response.request_dispatched == Some(false) {
                return Err(structured_edit_not_started_result(
                    tool_name,
                    "the returned lifecycle evidence proves the request never left the queue",
                ));
            }
            if response.error.is_some() {
                return Err(structured_edit_outcome_unknown_result(
                    tool_name,
                    "the Runner returned a transport or execution error after dispatch",
                    json!({}),
                ));
            }
            if response.exit_code != Some(0) {
                return Err(structured_edit_outcome_unknown_result(
                    tool_name,
                    "the Runner returned a non-zero terminal result after dispatch",
                    json!({}),
                ));
            }
            Ok(response)
        }
        Ok(Err(_)) => {
            let state = dispatch_uncertainty_lifecycle(
                runtime
                    .runner_registry
                    .cancel_request_dispatch_state(request_id)
                    .await,
            );
            Err(structured_edit_delivery_failure(
                tool_name,
                "the Runner response channel closed before a trustworthy result was received",
                state,
            ))
        }
        Err(_) => {
            let state = dispatch_uncertainty_lifecycle(
                runtime
                    .runner_registry
                    .cancel_request_dispatch_state(request_id)
                    .await,
            );
            Err(structured_edit_delivery_failure(
                tool_name,
                format!("no trustworthy result arrived within {wait_timeout_secs} seconds"),
                state,
            ))
        }
    }
}

pub(super) fn transactional_edit_agent_stdout_result(
    tool_name: &str,
    stdout: &str,
    expected_change_count: usize,
    expected_dry_run: bool,
) -> ToolResult {
    let stdout = stdout.trim();
    let mut obj: Value = match serde_json::from_str::<Value>(stdout) {
        Ok(value) if value.is_object() => value,
        _ => {
            return structured_edit_outcome_unknown_result(
                tool_name,
                "the Runner returned malformed or non-object JSON after dispatch",
                json!({}),
            )
        }
    };
    if let Some(error) = obj.get("error").and_then(Value::as_str).map(str::to_string) {
        let facts = EditOutcomeFacts::from_output(&obj);
        let rollback_complete = facts.rollback_complete();
        if !facts.proves_no_effect_failure() {
            return structured_edit_outcome_unknown_result(tool_name, error, obj);
        }
        obj["changed"] = json!(false);
        obj["state_changed"] = json!(false);
        obj["execution_state"] = if rollback_complete == Some(true) {
            json!("completed")
        } else {
            json!("not_started")
        };
        let message = if obj.get("conflict_recovery").is_some_and(Value::is_object)
            || error.contains("No files were modified")
            || error.contains("was rolled back")
        {
            error
        } else {
            recoverable_write_rejection(&error)
        };
        return ToolResult::err_with_output(message, obj);
    }

    let Some(changed) = EditOutcomeFacts::from_output(&obj).confirmed_runner_change(
        tool_name,
        expected_change_count,
        expected_dry_run,
    ) else {
        return structured_edit_outcome_unknown_result(
            tool_name,
            "the Runner success payload omitted or contradicted authoritative edit-effect fields",
            obj,
        );
    };
    obj["state_changed"] = json!(changed);
    obj["execution_state"] = json!("completed");
    ToolResult::ok(obj)
}
