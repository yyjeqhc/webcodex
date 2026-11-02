use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerPollRequest, RunnerRequest, RunnerResultRequest,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::{json, Value};

const CLIENT: &str = "bounded-scroll";

fn capabilities() -> RunnerCapabilities {
    RunnerCapabilities {
        computer_observe: true,
        computer_control: true,
        computer_scroll: true,
        computer_text_input: true,
        computer_accessibility_observe: true,
        ..Default::default()
    }
}

async fn setup(caps: RunnerCapabilities) -> ToolRuntime {
    let runtime = runtime_with_agent_project(CLIENT);
    register_agent_projects_for_auth(
        &runtime,
        CLIENT,
        &bootstrap_auth_context(),
        caps,
        vec![registered_project("agent-proj", "/tmp/agent-proj")],
    )
    .await;
    runtime
}

fn call() -> ToolCall {
    ToolCall::from_tool_name(
        "control_computer",
        json!({"action":"scroll", "direction":"down", "client_id":CLIENT,
        "surface_id":"surface_test", "element_id":"element_test"}),
    )
    .unwrap()
}

async fn complete(
    runtime: &ToolRuntime,
    request: RunnerRequest,
    output: Option<Value>,
    error: Option<&str>,
) {
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: CLIENT.into(),
            runner_instance_id: format!("inst-{CLIENT}"),
            request_id: request.request_id,
            exit_code: Some(if error.is_none() { 0 } else { 1 }),
            stdout: output.map(|v| v.to_string()),
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: error.map(str::to_string),
        })
        .await
        .unwrap();
}

async fn no_more_requests(runtime: &ToolRuntime) {
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: CLIENT.into(),
            runner_instance_id: format!("inst-{CLIENT}")
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn computer_scroll_requires_both_capabilities_before_dispatch() {
    for (control, scroll) in [(false, true), (true, false), (false, false)] {
        let mut caps = capabilities();
        caps.computer_control = control;
        caps.computer_scroll = scroll;
        let runtime = setup(caps).await;
        let auth = bootstrap_auth_context();
        let result = runtime.dispatch_computer_tool(call(), Some(&auth)).await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        no_more_requests(&runtime).await;
    }
}

fn receipt() -> Value {
    json!({"platform":"windows", "surface_id":"surface_test", "element_id":"element_test",
        "direction":"down", "amount":"small", "position_before":0.0, "position_after":10.0, "success":true})
}

#[tokio::test]
async fn computer_scroll_normalizes_amount_and_dispatches_exactly_once() {
    for amount in [
        None,
        Some(Value::Null),
        Some(json!("small")),
        Some(json!("large")),
    ] {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut args = json!({"action":"scroll", "client_id":CLIENT, "surface_id":"surface_test", "element_id":"element_test", "direction":"down"});
        if let Some(value) = amount.clone() {
            args["amount"] = value;
        }
        let mut tool = Box::pin(runtime.dispatch_computer_tool(
            ToolCall::from_tool_name("control_computer", args).unwrap(),
            Some(&auth),
        ));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        assert_eq!(request.kind, "computer_scroll");
        let wire: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        let expected = if amount == Some(json!("large")) {
            "large"
        } else {
            "small"
        };
        assert_eq!(
            wire,
            json!({"surface_id":"surface_test", "element_id":"element_test", "direction":"down", "amount":expected})
        );
        let mut output = receipt();
        output["amount"] = json!(expected);
        complete(&runtime, request, Some(output), None).await;
        let result = tool.await;
        assert!(result.success, "{result:?}");
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_scroll_rejects_inconsistent_success_without_replay() {
    for (field, value) in [
        ("direction", json!("up")),
        ("amount", json!("large")),
        ("surface_id", json!("surface_other")),
        ("element_id", json!("element_other")),
        ("position_after", json!(101)),
        ("platform", json!("macos")),
        ("private_extra", json!("must-not-escape")),
    ] {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        let mut output = receipt();
        output[field] = value;
        complete(&runtime, request, Some(output), None).await;
        let result = tool.await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(!result.output.to_string().contains("must-not-escape"));
        no_more_requests(&runtime).await;
    }
}

#[tokio::test(start_paused = true)]
async fn computer_scroll_lost_response_and_native_uncertainty_do_not_replay() {
    for native_error in [false, true] {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        if native_error {
            complete(
                &runtime,
                request,
                None,
                Some("outcome_unknown: scroll confirmation failed"),
            )
            .await;
        } else {
            tokio::time::advance(std::time::Duration::from_secs(64)).await;
        }
        let result = tool.await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_scroll_state_extension_is_negotiated_independently() {
    for selection in [false, true] {
        for scroll in [false, true] {
            let mut caps = capabilities();
            caps.computer_element_state = true;
            caps.computer_selection = selection;
            caps.computer_scroll = scroll;
            let runtime = setup(caps).await;
            let auth = bootstrap_auth_context();
            let args = json!({"action":"element_state","client_id":CLIENT,"surface_id":"surface_test","element_id":"element_test"});
            let mut tool = Box::pin(runtime.dispatch_computer_tool(
                ToolCall::from_tool_name("observe_computer", args).unwrap(),
                Some(&auth),
            ));
            assert!(futures_util::poll!(tool.as_mut()).is_pending());
            let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
            let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
            assert_eq!(
                payload.get("include_scroll"),
                scroll.then_some(&Value::Bool(true))
            );
            assert_eq!(
                payload.get("include_selection"),
                selection.then_some(&Value::Bool(true))
            );
            let mut output = json!({"platform":"windows","surface_id":"surface_test","element_id":"element_test",
            "observation_generation":1,"enabled":true,"focused":false,"protected":false,"value_empty":null,"can_press":false,"can_focus":false,"can_input_text":false});
            if selection {
                output["can_select"] = json!(false);
                output["selected"] = Value::Null;
            }
            if scroll {
                output["scroll"] = json!({"horizontal_percent":null,"vertical_percent":15});
            }
            complete(&runtime, request, Some(output), None).await;
            let result = tool.await;
            assert!(result.success, "{result:?}");
            assert_eq!(result.output.get("scroll").is_some(), scroll);
        }
    }
}

#[tokio::test]
async fn computer_scroll_state_rejects_private_or_inconsistent_axis_facts() {
    for (state, protected, platform) in [
        (
            json!({"horizontal_percent":null,"vertical_percent":-1}),
            false,
            "windows",
        ),
        (json!({"horizontal_percent":null}), false, "windows"),
        (
            json!({"horizontal_percent":null,"vertical_percent":1,"title":"PRIVATE SCROLL VALUE"}),
            false,
            "windows",
        ),
        (
            json!({"horizontal_percent":null,"vertical_percent":1}),
            true,
            "windows",
        ),
        (
            json!({"horizontal_percent":null,"vertical_percent":1}),
            false,
            "macos",
        ),
    ] {
        let mut caps = capabilities();
        caps.computer_element_state = true;
        let runtime = setup(caps).await;
        let auth = bootstrap_auth_context();
        let args = json!({"action":"element_state","client_id":CLIENT,"surface_id":"surface_test","element_id":"element_test"});
        let mut tool = Box::pin(runtime.dispatch_computer_tool(
            ToolCall::from_tool_name("observe_computer", args).unwrap(),
            Some(&auth),
        ));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        let output = json!({"platform":platform,"surface_id":"surface_test","element_id":"element_test",
            "observation_generation":1,"enabled":true,"focused":false,"protected":protected,"value_empty":null,"can_press":false,"can_focus":false,"can_input_text":false,"scroll":state});
        complete(&runtime, request, Some(output), None).await;
        let result = tool.await;
        assert!(!result.success, "{result:?}");
        assert!(!result.output.to_string().contains("PRIVATE SCROLL"));
        no_more_requests(&runtime).await;
    }
}
