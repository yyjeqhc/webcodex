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

#[test]
fn mcp_observation_wake_defaults_preserve_explicit_values_and_api_default() {
    for (mut arguments, expected) in [
        (
            json!({"items":[{"job_id":"same-job"}]}),
            json!("meaningful_change"),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"wake_on":"change"}),
            json!("change"),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"wake_on":"meaningful_change"}),
            json!("meaningful_change"),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"wake_on":"terminal"}),
            json!("terminal"),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"wake_on":"all_terminal"}),
            json!("all_terminal"),
        ),
        (
            json!({"items":[{"job_id":"same-job"}],"wake_on":null}),
            Value::Null,
        ),
    ] {
        crate::mcp::tools::project_mcp_model_argument_defaults("observe_jobs", &mut arguments);
        assert_eq!(arguments["wake_on"], expected);
        let call = crate::tool_runtime::ToolCall::from_tool_name("observe_jobs", arguments);
        if expected.is_null() {
            assert!(call.is_err(), "explicit null must remain invalid");
        } else {
            let crate::tool_runtime::ToolCall::ObserveJobs { wake_on, .. } = call.unwrap() else {
                panic!("observe_jobs");
            };
            assert_eq!(serde_json::to_value(wake_on).unwrap(), expected);
        }
    }
    let canonical = crate::tool_runtime::ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items":[{"job_id":"same-job"}]}),
    )
    .unwrap();
    let crate::tool_runtime::ToolCall::ObserveJobs { wake_on, .. } = canonical else {
        panic!("observe_jobs");
    };
    assert_eq!(serde_json::to_value(wake_on).unwrap(), "change");
    let schema = webcodex_tool_contracts::input_schema_for_tool("observe_jobs");
    assert_eq!(schema["properties"]["wake_on"]["default"], "change");
    assert!(schema["properties"]["wake_on"]["description"]
        .as_str()
        .unwrap()
        .contains("change (default)"));
}

#[tokio::test]
async fn mcp_observation_defaults_are_published_by_direct_gateway_and_cli() {
    let runtime = test_runtime();
    let assert_defaults = |schema: &Value| {
        assert_eq!(schema["properties"]["summary_only"]["default"], true);
        let wake_on = &schema["properties"]["wake_on"];
        assert_eq!(wake_on["default"], "meaningful_change");
        let description = wake_on["description"].as_str().unwrap();
        assert!(description.contains("MCP defaults to meaningful_change"));
        assert!(!description.contains("change (default)"));
    };
    for tools in [
        mcp_tools_list_payload_with_compact(false),
        mcp_tools_list_payload_with_compact(true),
        crate::mcp::cli::tools_list(None, false),
    ] {
        let descriptor = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "observe_jobs")
            .unwrap();
        assert_defaults(&descriptor["inputSchema"]);
    }
    for gateway in [false, true] {
        let arguments = json!({"tool_name":"observe_jobs"});
        let params = if gateway {
            adaptive_runtime_gateway_params("read_tool_manifest", arguments)
        } else {
            json!({"name":"read_tool_manifest","arguments":arguments})
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
        assert_defaults(&body["result"]["structuredContent"]["output"]["input_schema"]);
    }
}

#[tokio::test]
async fn mcp_observation_wake_default_ignores_heartbeats_on_direct_gateway_and_cli() {
    use crate::runner_protocol::RunnerJobUpdateRequest;
    use std::time::Duration;
    use webcodex_runner_registry::JobStartRequest;

    const RUNNER_FAILURE: &str = "runner fixture: command exited with failure";

    async fn observe(
        runtime: &ToolRuntime,
        route: &str,
        arguments: Value,
        auth: &crate::auth::AuthContext,
    ) -> Value {
        if route == "cli" {
            return crate::mcp::cli_call_for_test(runtime, "observe_jobs", arguments, auth).await;
        }
        let params = if route == "gateway" {
            adaptive_runtime_gateway_params("observe_jobs", arguments)
        } else {
            json!({"name":"observe_jobs","arguments":arguments})
        };
        let McpOutcome::Ok(body) = handle_mcp_request(
            runtime,
            rpc("tools/call", Some(json!(1)), params),
            Some(auth),
        )
        .await
        else {
            panic!("observe_jobs route={route}");
        };
        body
    }

    async fn update(
        runtime: &ToolRuntime,
        job_id: &str,
        request_id: &str,
        sequence: u64,
        stdout: Option<&str>,
        finished: bool,
    ) {
        runtime
            .runner_registry
            .update_job(
                serde_json::from_value::<RunnerJobUpdateRequest>(json!({
                    "client_id":"mcp-wake-runner", "agent_instance_id":"inst",
                    "job_id":job_id, "request_id":request_id, "update_seq":sequence,
                    "status":if finished { "failed" } else { "running" },
                    "stdout_chunk":stdout, "finished":finished,
                    "exit_code":finished.then_some(23),
                    "error":finished.then_some(RUNNER_FAILURE)
                }))
                .unwrap(),
            )
            .await
            .unwrap();
    }

    async fn token(runtime: &ToolRuntime, job_id: &str) -> String {
        runtime
            .runner_registry
            .job_log_for_auth(None, job_id, None, None, Some(40), None, None)
            .await
            .unwrap()
            .0
            .observation_token
            .expect("observation token")
    }

    fn assert_reply(
        route: &str,
        body: &Value,
        job_id: &str,
        outcome: &str,
        update_kind: Option<&str>,
        stdout: Option<&str>,
    ) {
        assert_ne!(body["result"]["isError"], true, "{route}: {body}");
        if route == "cli" {
            let text = body["result"]["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains(job_id), "{text}");
            if let Some(stdout) = stdout {
                assert!(text.contains(stdout), "{text}");
            }
            if outcome == "terminal" {
                assert!(text.contains("failed"), "{text}");
                assert!(text.contains(RUNNER_FAILURE), "{text}");
            }
            assert!(!text.contains("wait deadline reached"), "{text}");
        } else {
            let result = &body["result"]["structuredContent"];
            assert_eq!(result["success"], true, "{route}: {body}");
            let output = &result["output"];
            assert_eq!(output["wait"]["outcome"], outcome, "{route}: {body}");
            if let Some(update_kind) = update_kind {
                assert_eq!(output["wait"]["update_kind"], update_kind);
            }
            let item = &output["items"][0];
            let observation = item.get("output").unwrap_or(item);
            assert_eq!(observation["job_id"], job_id);
            if let Some(stdout) = stdout {
                assert!(observation["stdout_tail"]
                    .as_str()
                    .unwrap()
                    .contains(stdout));
            }
            if outcome == "terminal" {
                assert_eq!(observation["status"], "failed");
                assert_eq!(observation["error"], RUNNER_FAILURE);
            }
        }
    }

    let runtime = test_runtime();
    let registration: RunnerRegisterRequest = serde_json::from_value(json!({
        "client_id":"mcp-wake-runner", "agent_instance_id":"inst",
        "agent_protocol_generation":crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        "capabilities":{
            "shell":true, "jobs":true, "async_jobs":true, "async_shell_jobs":true,
            "job_state_reconciliation":true
        },
        "job_inventory":{"active_complete":true,"jobs":[]}
    }))
    .unwrap();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            registration,
        ))
        .await
        .unwrap();
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;

    // One in-memory registry and one Job per adapter; no extra Server or Runner process.
    for route in ["direct", "gateway", "cli"] {
        let job = runtime
            .runner_registry
            .start_job(
                JobStartRequest {
                    timeout_secs: 60,
                    ..JobStartRequest::shell("mcp-wake-runner", "echo fixture")
                },
                "fixture".into(),
                None,
                None,
            )
            .await
            .unwrap();
        let request = runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: "mcp-wake-runner".into(),
                runner_instance_id: "inst".into(),
            })
            .await
            .unwrap()
            .unwrap();
        update(&runtime, &job.job_id, &request.request_id, 1, None, false).await;

        let baseline = tokio::time::timeout(
            Duration::from_secs(1),
            observe(
                &runtime,
                route,
                json!({"items":[{"job_id":job.job_id}],"wait_secs":5}),
                &auth,
            ),
        )
        .await
        .expect("a missing cursor must return its baseline immediately");
        assert_reply(route, &baseline, &job.job_id, "immediate", None, None);

        let cursor = token(&runtime, &job.job_id).await;
        let meaningful = observe(
            &runtime,
            route,
            json!({"items":[{"job_id":job.job_id,"after_observation_token":cursor}],"wait_secs":5}),
            &auth,
        );
        tokio::pin!(meaningful);
        // Enter the observation wait before publishing the next accepted revision.
        assert!(futures_util::poll!(&mut meaningful).is_pending());
        update(&runtime, &job.job_id, &request.request_id, 2, None, false).await;
        // Drive the same future during the negative assertion; do not sleep
        // while leaving an unpolled observer that could falsely appear blocked.
        assert!(
            tokio::time::timeout(Duration::from_millis(75), meaningful.as_mut())
                .await
                .is_err(),
            "{route}: a pure heartbeat must not satisfy the MCP default"
        );
        update(
            &runtime,
            &job.job_id,
            &request.request_id,
            3,
            Some("mcp wake output\n"),
            false,
        )
        .await;
        let body = tokio::time::timeout(Duration::from_secs(1), meaningful)
            .await
            .expect("new log output must wake the default wait");
        assert_reply(
            route,
            &body,
            &job.job_id,
            "updated",
            Some("meaningful_change"),
            Some("mcp wake output"),
        );

        let cursor = token(&runtime, &job.job_id).await;
        let change = observe(
            &runtime,
            route,
            json!({"items":[{"job_id":job.job_id,"after_observation_token":cursor}],"wait_secs":5,"wake_on":"change"}),
            &auth,
        );
        tokio::pin!(change);
        assert!(futures_util::poll!(&mut change).is_pending());
        update(&runtime, &job.job_id, &request.request_id, 4, None, false).await;
        let body = tokio::time::timeout(Duration::from_secs(1), change)
            .await
            .expect("explicit change must preserve heartbeat observation");
        assert_reply(
            route,
            &body,
            &job.job_id,
            "updated",
            Some("heartbeat"),
            None,
        );

        let cursor = token(&runtime, &job.job_id).await;
        let terminal = observe(
            &runtime,
            route,
            json!({"items":[{"job_id":job.job_id,"after_observation_token":cursor}],"wait_secs":5}),
            &auth,
        );
        tokio::pin!(terminal);
        assert!(futures_util::poll!(&mut terminal).is_pending());
        // Finish the same Job with a Runner-provided failure. summary_only stays
        // omitted, so every adapter must preserve its reason through the sparse default.
        update(&runtime, &job.job_id, &request.request_id, 5, None, true).await;
        let body = tokio::time::timeout(Duration::from_secs(1), terminal)
            .await
            .expect("terminal state must wake the default wait");
        assert_reply(route, &body, &job.job_id, "terminal", None, None);
    }
}

#[tokio::test]
async fn mcp_observation_summary_default_compacts_same_job_on_both_routes() {
    use crate::runner_protocol::{
        RunnerJobUpdateRequest, ShellJobValidationMetadata, ShellJobValidationStep,
    };
    use webcodex_runner_registry::{JobStartExecution, JobStartRequest, JobValidationPlan};

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
    let job = runtime
        .runner_registry
        .start_job(
            JobStartRequest {
                timeout_secs: 60,
                ..JobStartRequest::new(
                    "summary-runner",
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                )
            },
            "fixture".into(),
            None,
            None,
        )
        .await
        .unwrap();
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
