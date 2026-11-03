use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerPollRequest, RunnerRequest, RunnerResultRequest,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::{json, Value};

const CLIENT: &str = "post-observation";
const ACTIONS: [&str; 3] = ["press", "input_text", "scroll"];

fn capabilities() -> RunnerCapabilities {
    RunnerCapabilities {
        computer_observe: true,
        computer_control: true,
        computer_text_input: true,
        computer_scroll: true,
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

fn call(action: &str) -> ToolCall {
    let mut args = json!({"action":action,"client_id":CLIENT,"surface_id":"surface_test",
        "element_id":"element_test","observe_after":{"action":"accessibility_tree","max_depth":3,"max_nodes":16}});
    if action == "input_text" {
        args["text"] = json!("hello");
    }
    if action == "scroll" {
        args["direction"] = json!("down");
    }
    ToolCall::from_tool_name("control_computer", args).unwrap()
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

fn action_result(action: &str) -> Value {
    let mut result = json!({"platform":"windows","surface_id":"surface_test","element_id":"element_test","success":true});
    match action {
        "press" => result["action"] = json!("press"),
        "input_text" => result["text_bytes"] = json!(5),
        "scroll" => {
            result["direction"] = json!("down");
            result["amount"] = json!("small");
            result["position_before"] = json!(0);
            result["position_after"] = json!(10);
        }
        _ => unreachable!(),
    }
    result
}

fn tree() -> Value {
    json!({"platform":"windows","surface_id":"surface_test","observation_generation":2,
        "max_depth":3,"max_nodes":16,"truncated":false,"node_count":1,
        "nodes":[{"element_id":"element_new","parent_element_id":null,"depth":0,"role":"AXWindow",
            "subrole":null,"title":"changed state","description":null,"placeholder":null,"value":null,
            "enabled":true,"focused":true,"child_count":0}]})
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

async fn set_control_epoch(runtime: &ToolRuntime, epoch: &str) {
    runtime
        .runner_registry
        .update_computer_control_epoch(CLIENT, &format!("inst-{CLIENT}"), None, Some(epoch.into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn computer_post_observation_orders_actions_and_returns_fresh_tree() {
    for action in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let request = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        assert_eq!(
            request.kind,
            match action {
                "press" => "computer_control",
                "input_text" => "computer_input_text",
                "scroll" => "computer_scroll",
                _ => unreachable!(),
            }
        );
        let wire: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        assert!(wire.get("observe_after").is_none());
        if action == "scroll" {
            assert_eq!(
                wire,
                json!({"surface_id":"surface_test","element_id":"element_test","direction":"down","amount":"small"})
            );
        }
        no_more_requests(&runtime).await;
        complete(&runtime, request, Some(action_result(action)), None).await;
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let observation = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        assert_eq!(observation.kind, "computer_accessibility_tree");
        let payload: Value = serde_json::from_str(observation.stdin.as_deref().unwrap()).unwrap();
        assert_eq!(
            payload,
            json!({"surface_id":"surface_test","max_depth":3,"max_nodes":16})
        );
        complete(&runtime, observation, Some(tree()), None).await;
        let result = tool.await;
        assert!(result.success, "{:?}", result);
        assert_eq!(result.output["success"], true);
        if action == "scroll" {
            assert_eq!(result.output["position_after"], 10);
        }
        assert_eq!(result.output["post_observation"]["success"], true);
        assert_eq!(
            result.output["post_observation"]["output"]["observation_generation"],
            2
        );
        assert_eq!(
            result.output["post_observation"]["output"]["nodes"][0]["element_id"],
            "element_new"
        );
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_post_observation_preserves_action_success_when_read_fails() {
    for action_name in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(&runtime, action, Some(action_result(action_name)), None).await;
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let read = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(
            &runtime,
            read,
            None,
            Some("accessibility_failed: provider unavailable"),
        )
        .await;
        let result = tool.await;
        assert!(result.success);
        assert_eq!(result.output["success"], true);
        assert_eq!(result.output["post_observation"]["success"], false);
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_post_observation_unknown_action_never_observes_or_replays() {
    for action_name in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(
            &runtime,
            action,
            None,
            Some("outcome_unknown: native result lost"),
        )
        .await;
        let result = tool.await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(result.output.get("post_observation").is_none());
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_post_observation_local_control_rejection_never_observes_or_replays() {
    for action_name in ACTIONS {
        for error_kind in [
            "computer_control_paused",
            "computer_control_changed",
            "computer_control_unavailable",
        ] {
            let runtime = setup(capabilities()).await;
            let auth = bootstrap_auth_context();
            let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
            assert!(futures_util::poll!(tool.as_mut()).is_pending());
            let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
            complete(&runtime, action, None, Some(error_kind)).await;
            let result = tool.await;
            assert!(!result.success, "{action_name}: {result:?}");
            assert_eq!(result.output["error_kind"], error_kind);
            assert_eq!(result.output["execution_state"], "not_started");
            assert!(result.output.get("post_observation").is_none());
            no_more_requests(&runtime).await;
        }
    }
}

#[tokio::test]
async fn computer_scroll_post_observation_rejects_retired_control_epoch_before_dispatch() {
    let mut caps = capabilities();
    caps.computer_local_control = true;
    let runtime = setup(caps).await;
    set_control_epoch(&runtime, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").await;
    let auth = bootstrap_auth_context();
    let mut tool = Box::pin(runtime.dispatch_computer_tool(call("scroll"), Some(&auth)));
    assert!(futures_util::poll!(tool.as_mut()).is_pending());
    set_control_epoch(&runtime, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").await;
    no_more_requests(&runtime).await;
    let result = tool.await;
    assert!(!result.success, "{result:?}");
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("computer_control_changed:"));
    assert!(result.output.get("post_observation").is_none());
    no_more_requests(&runtime).await;
}

#[tokio::test]
async fn computer_scroll_post_observation_reads_after_completed_action_and_control_epoch_change() {
    let mut caps = capabilities();
    caps.computer_local_control = true;
    let runtime = setup(caps).await;
    set_control_epoch(&runtime, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").await;
    let auth = bootstrap_auth_context();
    let mut tool = Box::pin(runtime.dispatch_computer_tool(call("scroll"), Some(&auth)));
    assert!(futures_util::poll!(tool.as_mut()).is_pending());
    let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
    assert_eq!(
        action.computer_control_epoch.as_deref(),
        Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
    );
    complete(&runtime, action, Some(action_result("scroll")), None).await;
    set_control_epoch(&runtime, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").await;
    assert!(futures_util::poll!(tool.as_mut()).is_pending());
    let observation = wait_for_runner_request_for_client(&runtime, CLIENT).await;
    assert_eq!(observation.kind, "computer_accessibility_tree");
    assert!(observation.computer_control_epoch.is_none());
    complete(&runtime, observation, Some(tree()), None).await;
    let result = tool.await;
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["position_after"], 10);
    assert_eq!(result.output["post_observation"]["success"], true);
    no_more_requests(&runtime).await;
}

#[tokio::test]
async fn computer_post_observation_preflights_read_scope_and_capability_before_action() {
    for action_name in ACTIONS {
        for missing_scope in [true, false] {
            let mut caps = capabilities();
            caps.computer_accessibility_observe = missing_scope;
            let runtime = setup(caps).await;
            let mut auth = auth_context(Some("alice"), false);
            auth.scopes = vec![crate::auth::SCOPE_COMPUTER_CONTROL.into()];
            let auth = if missing_scope {
                auth
            } else {
                bootstrap_auth_context()
            };
            let result = runtime
                .dispatch_computer_tool(call(action_name), Some(&auth))
                .await;
            assert!(!result.success);
            assert_eq!(result.output["execution_state"], "not_started");
            no_more_requests(&runtime).await;
        }
    }
}

#[tokio::test]
async fn computer_post_observation_invalid_action_is_not_started() {
    let runtime = setup(capabilities()).await;
    let auth = bootstrap_auth_context();
    let mut invalid = call("input_text");
    let ToolCall::ComputerControl(
        crate::tool_runtime::tool_call::ComputerControlToolCall::InputText { text, .. },
    ) = &mut invalid
    else {
        unreachable!()
    };
    text.clear();
    let result = runtime.dispatch_computer_tool(invalid, Some(&auth)).await;
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(result.output.get("post_observation").is_none());
    no_more_requests(&runtime).await;
}

#[tokio::test]
async fn computer_post_observation_does_not_retarget_replacement_runner() {
    for action_name in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(&runtime, action, Some(action_result(action_name)), None).await;
        runtime
            .runner_registry
            .register(crate::test_support::runner_registration(
                CLIENT,
                "replacement",
                capabilities(),
            ))
            .await
            .unwrap();
        let result = tool.await;
        assert!(result.success);
        assert_eq!(result.output["post_observation"]["success"], false);
        assert!(runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: CLIENT.into(),
                runner_instance_id: "replacement".into()
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn computer_post_observation_timeout_preserves_completed_action() {
    for action_name in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(&runtime, action, Some(action_result(action_name)), None).await;
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        tokio::time::advance(std::time::Duration::from_secs(64)).await;
        let result = tool.await;
        assert!(result.success);
        assert_eq!(result.output["post_observation"]["success"], false);
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_post_observation_cancelled_waiter_skips_queued_read() {
    for action_name in ACTIONS {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call(action_name), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        complete(&runtime, action, Some(action_result(action_name)), None).await;
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        drop(tool);
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_scroll_post_observation_requires_every_action_capability() {
    for missing in ["control", "scroll"] {
        let mut caps = capabilities();
        if missing == "control" {
            caps.computer_control = false;
        } else {
            caps.computer_scroll = false;
        }
        let runtime = setup(caps).await;
        let result = runtime
            .dispatch_computer_tool(call("scroll"), Some(&bootstrap_auth_context()))
            .await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        assert!(result.output.get("post_observation").is_none());
        no_more_requests(&runtime).await;
    }
}

#[tokio::test]
async fn computer_scroll_post_observation_reads_after_boundary_and_keeps_read_failure_separate() {
    for lose_read_capability in [false, true] {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call("scroll"), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        let mut output = action_result("scroll");
        output["position_before"] = json!(100);
        output["position_after"] = json!(100);
        complete(&runtime, action, Some(output), None).await;
        if lose_read_capability {
            let mut caps = capabilities();
            caps.computer_accessibility_observe = false;
            runtime
                .runner_registry
                .register(crate::test_support::runner_registration(
                    CLIENT,
                    &format!("inst-{CLIENT}"),
                    caps,
                ))
                .await
                .unwrap();
        } else {
            assert!(futures_util::poll!(tool.as_mut()).is_pending());
            let read = wait_for_runner_request_for_client(&runtime, CLIENT).await;
            assert_eq!(read.kind, "computer_accessibility_tree");
            complete(&runtime, read, Some(tree()), None).await;
        }
        let result = tool.await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["position_before"], 100);
        assert_eq!(result.output["position_after"], 100);
        assert_eq!(
            result.output["post_observation"]["success"],
            !lose_read_capability
        );
        no_more_requests(&runtime).await;
    }
}

#[tokio::test(start_paused = true)]
async fn computer_scroll_post_observation_lost_or_inconsistent_result_never_reads_or_replays() {
    for lost_response in [false, true] {
        let runtime = setup(capabilities()).await;
        let auth = bootstrap_auth_context();
        let mut tool = Box::pin(runtime.dispatch_computer_tool(call("scroll"), Some(&auth)));
        assert!(futures_util::poll!(tool.as_mut()).is_pending());
        let action = wait_for_runner_request_for_client(&runtime, CLIENT).await;
        if lost_response {
            tokio::time::advance(std::time::Duration::from_secs(64)).await;
        } else {
            let mut output = action_result("scroll");
            output["direction"] = json!("up");
            complete(&runtime, action, Some(output), None).await;
        }
        let result = tool.await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(result.output.get("post_observation").is_none());
        no_more_requests(&runtime).await;
    }
}
