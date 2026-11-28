//! The Workbench's small, typed host adapter. Domain operations still enter
//! the canonical kernel; this is not an arbitrary runtime-tool gateway.
use super::*;
use crate::tool_runtime::ToolResult;
use serde::Deserialize;

pub(super) const CANONICAL_TOOLS: &[&str] = &[
    "list_runners",
    "list_project_files",
    "start_web_preview",
    "get_goal",
    "create_goal",
    "update_goal",
    "read_agent_task",
    "observe_coding_agent",
    "cancel_coding_agent",
    "get_session_assignment",
    "complete_session_message",
    "reconcile_agent_task_coding_run",
];

pub(super) fn is_adapter_tool(name: &str) -> bool {
    matches!(
        name,
        "read_workbench_state" | "update_workbench_settings" | "submit_workbench_work"
    )
}

pub(super) fn descriptors(compact: bool) -> Vec<Value> {
    let text = |max| json!({"type":"string","minLength":1,"maxLength":max});
    let mut result = vec![
        json!({"name":"read_workbench_state","description":"Read bounded personal work, Goals, executors and explicit Project/Session observations. Partial sections retain their errors and paging; no execution or implicit Session selection.","inputSchema":{"type":"object","properties":{"project":text(256),"session_id":text(256),"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":50}},"additionalProperties":false}}),
        json!({"name":"update_workbench_settings","description":"Set this authenticated principal's personal Workbench display name; null clears the override. Does not change server configuration or another principal's label.","inputSchema":{"type":"object","properties":{"display_name":{"type":["string","null"],"minLength":1,"maxLength":80}},"required":["display_name"],"additionalProperties":false}}),
        json!({"name":"submit_workbench_work","description":"Submit explicitly routed work through one durable AgentTask and exact Attempt. Replay the same key and unchanged input after interruption; uncertain execution must be reconciled, never replaced. Project, assignee and executor remain independently authorized.","inputSchema":{"type":"object","properties":{"project":text(256),"title":text(200),"instruction":text(8192),"goal_id":text(64),"idempotency_key":text(128),"execution":{"type":"object","properties":{"kind":{"enum":["chat","runner"]},"agent_id":text(64),"provider_id":text(128),"endpoint_id":text(64),"expected_controller_generation":{"type":"integer","minimum":1}},"required":["kind","agent_id"],"additionalProperties":false}},"required":["project","title","instruction","execution","idempotency_key"],"additionalProperties":false}}),
    ];
    for tool in &mut result {
        let readonly = tool["name"] == "read_workbench_state";
        if !compact {
            // The typed adapter always returns the standard ToolResult envelope;
            // partial section diagnostics remain inside its output object.
            tool["outputSchema"] = json!({"type":"object","properties":{
                "success":{"type":"boolean"},"output":{"type":"object"},"error":{"type":["string","null"]}
            },"required":["success","output"]});
        }
        tool["annotations"] = json!({"readOnlyHint":readonly,"destructiveHint":tool["name"] == "submit_workbench_work","openWorldHint":false,"idempotentHint":true});
        tool["_meta"] = json!({"ui":{"visibility":["app"]}});
    }
    result
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateInput {
    project: Option<String>,
    session_id: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsInput {
    // Value distinguishes explicit null from an omitted required field.
    display_name: Value,
}

pub(super) async fn call(
    runtime: &ToolRuntime,
    name: &str,
    arguments: Value,
    auth: Option<&AuthContext>,
    window: Option<&crate::client_window::ClientWindow>,
) -> ToolResult {
    match name {
        "read_workbench_state" => {
            let input: StateInput = match serde_json::from_value(arguments) {
                Ok(input) => input,
                Err(_) => return invalid(),
            };
            if input.limit.is_some_and(|n| n == 0 || n > 50)
                || input.project.as_ref().is_some_and(|s| !bounded(s, 256))
                || input.session_id.as_ref().is_some_and(|s| !bounded(s, 256))
            {
                return invalid();
            }
            let mut result = runtime
                .read_workbench_state(
                    input.project.clone(),
                    input.session_id,
                    input.offset,
                    input.limit,
                    auth,
                    window,
                )
                .await;
            if result.success {
                runtime
                    .enrich_workbench_execution(
                        &mut result.output,
                        input.project.as_deref(),
                        auth,
                        window,
                    )
                    .await;
            }
            result
        }
        "update_workbench_settings" => {
            if arguments.get("display_name").is_none() {
                return invalid();
            }
            let input: SettingsInput = match serde_json::from_value(arguments) {
                Ok(input) => input,
                Err(_) => return invalid(),
            };
            let value = match input.display_name {
                Value::Null => None,
                Value::String(value) => Some(value),
                _ => return invalid(),
            };
            runtime.update_workbench_settings(value, auth)
        }
        "submit_workbench_work" => runtime.submit_workbench_work(arguments, auth, window).await,
        _ => invalid(),
    }
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn invalid() -> ToolResult {
    ToolResult::err_with_output(
        "Invalid Workbench arguments",
        json!({"error_kind":"invalid_workbench_arguments","state_changed":false}),
    )
}
