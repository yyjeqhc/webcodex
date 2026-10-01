use super::*;

#[test]
fn mcp_observation_summary_defaults_preserve_explicit_values_and_api_default() {
    for (mut arguments, expected) in [
        (json!({"items":[{"job_id":"same-job"}]}), json!(true)),
        (
            json!({"items":[{"job_id":"same-job"}],"summary_only":false}),
            json!(false),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"summary_only":true}),
            json!(true),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"summary_only":null}),
            Value::Null,
        ),
    ] {
        crate::mcp::tools::project_mcp_model_argument_defaults("observe_jobs", &mut arguments);
        assert_eq!(arguments["summary_only"], expected);
        if expected.is_null() {
            assert!(
                crate::tool_runtime::ToolCall::from_tool_name("observe_jobs", arguments).is_err()
            );
        } else {
            let call =
                crate::tool_runtime::ToolCall::from_tool_name("observe_jobs", arguments).unwrap();
            assert!(
                matches!(call, crate::tool_runtime::ToolCall::ObserveJobs { summary_only, .. } if summary_only == expected.as_bool().unwrap())
            );
        }
    }
    let canonical = crate::tool_runtime::ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items":[{"job_id":"same-job"}]}),
    )
    .unwrap();
    assert!(matches!(
        canonical,
        crate::tool_runtime::ToolCall::ObserveJobs {
            summary_only: false,
            ..
        }
    ));
    assert_eq!(
        webcodex_tool_contracts::input_schema_for_tool("observe_jobs")["properties"]
            ["summary_only"]["default"],
        false
    );
}

#[tokio::test]
async fn mcp_observation_summary_default_is_published_by_direct_and_gateway_manifest() {
    let runtime = test_runtime();
    let tools = mcp_tools_list_payload_with_compact(false);
    let descriptor = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "observe_jobs")
        .unwrap();
    assert_eq!(
        descriptor["inputSchema"]["properties"]["summary_only"]["default"],
        true
    );
    for gateway in [false, true] {
        let arguments = json!({"tool_name":"observe_jobs"});
        let params = if gateway {
            adaptive_runtime_gateway_params("tool_manifest", arguments)
        } else {
            json!({"name":"tool_manifest","arguments":arguments})
        };
        let McpOutcome::Ok(body) = handle_mcp_request(
            &runtime,
            rpc("tools/call", Some(json!(1)), mcp_2026_params(params)),
            None,
        )
        .await
        else {
            panic!("observation manifest");
        };
        let schema = &body["result"]["structuredContent"]["output"]["input_schema"];
        assert_eq!(schema["properties"]["summary_only"]["default"], true);
    }
}

#[tokio::test]
async fn mcp_observation_summary_default_compacts_same_job_on_both_routes() {
    use crate::runner_protocol::{
        RunnerJobUpdateRequest, ShellJobOpRequest, ShellJobValidationMetadata,
        ShellJobValidationStep,
    };
    use webcodex_runner_registry::ShellJobStartMetadata;
    let runtime = test_runtime();
    let registration: RunnerRegisterRequest = serde_json::from_value(json!({
        "client_id":"summary-runner", "agent_instance_id":"inst",
        "agent_protocol_generation":crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        "capabilities":{"shell":true,"jobs":true,"async_jobs":true,"async_shell_jobs":true,"structured_validation_argv":true}
    })).unwrap();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            registration,
        ))
        .await
        .unwrap();
    let step = ShellJobValidationStep {
        name: "check".into(),
        program: "cargo".into(),
        args: vec!["check".into()],
        env: vec![],
    };
    let metadata: ShellJobValidationMetadata = serde_json::from_value(json!({
        "tool":"cargo_check", "kind":"check", "adapter":"cargo_check",
        "steps":[step], "effective_timeout_secs":60, "sync_wait_secs":1
    }))
    .unwrap();
    let job = runtime.runner_registry.start_job_with_metadata(
        serde_json::from_value::<ShellJobOpRequest>(json!({"op":"start","client_id":"summary-runner","command":"cargo check","timeout_secs":60})).unwrap(),
        "fixture".into(),
        ShellJobStartMetadata {validation_steps:vec![step], validation:Some(metadata), ..Default::default()},
    ).await.unwrap();
    let request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "summary-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    let baseline = runtime
        .runner_registry
        .job_log_for_auth(None, &job.job_id, None, None, Some(200), None, None)
        .await
        .unwrap();
    let original_token = baseline
        .0
        .observation_token
        .expect("initial observation token");
    let progress = (0..120)
        .map(|index| format!("    Checking fixture_{index:03} v0.1.0\n"))
        .collect::<String>();
    let stderr = progress + "    Finished `dev` profile in 1.00s\nwarning: retained diagnostic\n";
    runtime.runner_registry.update_job(serde_json::from_value::<RunnerJobUpdateRequest>(json!({
        "client_id":"summary-runner", "agent_instance_id":"inst", "job_id":job.job_id,
        "request_id":request.request_id, "status":"completed", "exit_code":0,
        "stdout_chunk":"", "stderr_chunk":stderr, "duration_ms":1000, "finished":true,
        "command_execution_state":"completed", "validation_progress":{"completed":1,"current_step":null,"failed_step":null}
    })).unwrap()).await.unwrap();
    let original_item = json!({"job_id":job.job_id,"after_observation_token":original_token});
    for gateway in [false, true] {
        let mut full_bytes = None;
        let mut summary_bytes = None;
        for summary in [Some(false), None, Some(true)] {
            let mut arguments = json!({"items":[original_item],"tail_lines":200});
            if let Some(summary) = summary {
                arguments["summary_only"] = json!(summary);
            }
            let params = if gateway {
                adaptive_runtime_gateway_params("observe_jobs", arguments)
            } else {
                json!({"name":"observe_jobs","arguments":arguments})
            };
            let McpOutcome::Ok(body) =
                handle_mcp_request(&runtime, rpc("tools/call", Some(json!(1)), params), None).await
            else {
                panic!("observe same Job");
            };
            assert_eq!(
                body["result"]["structuredContent"]["success"], true,
                "{body}"
            );
            let observation = &body["result"]["structuredContent"]["output"]["items"][0];
            assert_eq!(observation["job_id"], job.job_id);
            assert_eq!(observation["validation"]["passed"], true);
            assert!(observation["stderr_tail"]
                .as_str()
                .unwrap()
                .contains("retained diagnostic"));
            let bytes = serde_json::to_vec(&body).unwrap().len();
            if summary == Some(false) {
                assert!(observation["stderr_tail"]
                    .as_str()
                    .unwrap()
                    .contains("fixture_000"));
                full_bytes = Some(bytes);
            } else {
                assert!(!observation["stderr_tail"]
                    .as_str()
                    .unwrap()
                    .contains("fixture_000"));
                assert_eq!(
                    observation["suggested_call"]["arguments"]["summary_only"], false,
                    "gateway={gateway}: {body}"
                );
                assert_eq!(
                    observation["suggested_call"]["arguments"]["items"][0],
                    original_item
                );
                assert_ne!(observation["observation_token"], original_token);
                assert!(observation["suggested_call"]["arguments"]
                    .get("wait_secs")
                    .is_none());
                summary_bytes = Some(bytes);
            }
        }
        assert!(summary_bytes.unwrap() * 2 < full_bytes.unwrap());
    }
    let canonical = runtime
        .dispatch(
            crate::tool_runtime::ToolCall::from_tool_name(
                "observe_jobs",
                json!({"items":[original_item],"tail_lines":200}),
            )
            .unwrap(),
        )
        .await;
    assert!(canonical.success, "{:?}", canonical.error);
    let item = &canonical.output["items"][0];
    let observation = item.get("output").unwrap_or(item);
    assert!(
        observation["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("fixture_000"),
        "HTTP/canonical omission must preserve full logs"
    );
}

#[test]
fn mcp_observation_summary_schema_alternatives_preserve_failure_recovery_and_reject_invalid_edges()
{
    for (tool, posture, retained) in [
        ("list_jobs", "fallback_recovery", true),
        ("observe_jobs", "fallback_recovery", false),
        ("list_jobs", "mechanically_followable", false),
    ] {
        let mut result = crate::tool_runtime::ToolResult::err_with_output(
            "unknown Job",
            json!({
                "requested_count":1,"returned_count":1,"succeeded_count":0,"failed_count":1,
                "items":[{"index":0,"job_id":"missing","success":false,"output":null,
                    "error_kind":"unknown_job","error":"unknown Job",
                    "suggested_call":{"follow_up_kind":posture,"tool":tool,"arguments":{}}}],
                "wait":{"outcome":"item_error","waited_ms":0},"changed_count":0,"terminal_count":0,"output_truncated":false
            }),
        );
        crate::model_surface::project_tool_result_suggested_calls(
            "observe_jobs",
            &mut result,
            &|_| crate::model_surface::SuggestedToolCallRoute::Gateway("call_runtime_tool"),
        );
        let call = &result.output["items"][0]["suggested_call"];
        if retained {
            assert_eq!(call["tool"], "call_runtime_tool");
            assert_eq!(call["arguments"]["tool"], "list_jobs");
        } else {
            assert!(call.is_null(), "invalid edge retained: {call}");
        }
    }
}
