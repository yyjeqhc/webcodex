use super::*;

fn params(tool: &str, arguments: Value, gateway: bool, compact: bool) -> Value {
    let mut call = if gateway {
        adaptive_runtime_gateway_params(tool, arguments)
    } else {
        json!({"name":tool,"arguments":arguments})
    };
    call["arguments"]["_wc"] = json!({"compact_execution":compact});
    mcp_2026_params(call)
}

#[test]
fn execution_control_input_is_explicit_closed_and_business_args_stay_unchanged() {
    for compact in [false, true] {
        let mut args =
            json!({"project":"p","executable":"echo","_wc":{"compact_execution":compact}});
        let parsed =
            crate::mcp::tools::parse_mcp_invocation_envelope("run_process", &mut args, true)
                .unwrap();
        assert_eq!(parsed.metadata.compact_execution, compact);
        assert_eq!(args, json!({"project":"p","executable":"echo"}));
    }
    for (tool, value, stateless) in [
        ("run_process", json!("true"), true),
        ("run_process", Value::Null, true),
        ("read_files", json!(true), true),
        ("run_process", json!(true), false),
    ] {
        let mut args = json!({"_wc":{"compact_execution":value}});
        assert!(
            crate::mcp::tools::parse_mcp_invocation_envelope(tool, &mut args, stateless).is_err()
        );
    }
}

#[tokio::test]
async fn execution_control_schema_exposes_opt_in_and_validates_both_result_views() {
    let McpOutcome::Ok(body) = crate::mcp::tools::handle_list(None, None, true, false, false).await
    else {
        panic!()
    };
    let tools = body["result"]["tools"].as_array().unwrap();
    for name in ["run_process", "run_script", "run_shell", "observe_jobs"] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            tool["inputSchema"]["properties"]["_wc"]["properties"]["compact_execution"]["type"],
            "boolean"
        );
        let execution = json!({"state":"completed","outcome":"passed","exit_code":0});
        let output = if name == "observe_jobs" {
            json!({"items":[{"execution":execution}]})
        } else {
            json!({"execution":execution,"details":{"stdout_tail":"preserved"}})
        };
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &json!({"success":true,"error":null,"output":output}),
            &tool["outputSchema"],
        )
        .unwrap();
    }
}

#[tokio::test]
async fn execution_control_observe_direct_gateway_preserves_job_and_diagnostics() {
    use crate::runner_protocol::{RunnerJobUpdateRequest, ShellJobOpRequest};
    let runtime = test_runtime();
    let registration: RunnerRegisterRequest = serde_json::from_value(json!({
        "client_id":"control-runner","agent_instance_id":"inst",
        "agent_protocol_generation":crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        "capabilities":{"shell":true,"jobs":true,"async_jobs":true,"async_shell_jobs":true}
    }))
    .unwrap();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            registration,
        ))
        .await
        .unwrap();
    let job = runtime
        .runner_registry
        .start_job(
            serde_json::from_value::<ShellJobOpRequest>(json!({
                "op":"start","client_id":"control-runner","command":"fixture","timeout_secs":60
            }))
            .unwrap(),
            "fixture".into(),
        )
        .await
        .unwrap();
    let request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "control-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    runtime
        .runner_registry
        .update_job(
            serde_json::from_value::<RunnerJobUpdateRequest>(json!({
                "client_id":"control-runner","agent_instance_id":"inst","job_id":job.job_id,
                "request_id":request.request_id,"status":"completed","exit_code":0,
                "stdout_chunk":"retained result\n","stderr_chunk":"warning\n","duration_ms":1,
                "finished":true,"command_execution_state":"completed"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    for gateway in [false, true] {
        for compact in [false, true] {
            let request = params(
                "observe_jobs",
                json!({"items":[{"job_id":job.job_id}],"summary_only":false}),
                gateway,
                compact,
            );
            let McpOutcome::Ok(body) =
                handle_mcp_request(&runtime, rpc("tools/call", Some(json!(1)), request), None)
                    .await
            else {
                panic!("observation")
            };
            let result = &body["result"]["structuredContent"];
            assert_eq!(result["success"], true, "{body}");
            let item = &result["output"]["items"][0];
            if compact {
                assert_eq!(item["execution"]["outcome"], "passed", "{body}");
                assert_eq!(item["execution"]["job_id"], job.job_id);
                assert_eq!(item["details"]["stdout_tail"], "retained result\n");
                assert_eq!(item["details"]["stderr_tail"], "warning\n");
            } else {
                assert!(item.get("execution").is_none());
                assert_eq!(item["job_id"], job.job_id);
                assert_eq!(item["stdout_tail"], "retained result\n");
            }
        }
    }
    // Batch acceptance can succeed with item errors; only per-Job outcomes are
    // completion evidence. Keep the missing Job's exact recovery edge.
    let request = params(
        "observe_jobs",
        json!({"items":[{"job_id":job.job_id},
        {"job_id":"wc_job_missing_123"}],"summary_only":false}),
        true,
        true,
    );
    let McpOutcome::Ok(body) =
        handle_mcp_request(&runtime, rpc("tools/call", Some(json!(2)), request), None).await
    else {
        panic!("mixed observation")
    };
    let result = &body["result"]["structuredContent"];
    assert_eq!(result["success"], true, "{body}");
    assert_eq!(result["output"]["failed_count"], 1);
    let items = &result["output"]["items"];
    assert_eq!(items[0]["execution"]["outcome"], "passed");
    assert_eq!(items[1]["execution"]["outcome"], "unknown");
    assert_eq!(items[1]["details"]["error_kind"], "unknown_job");
    assert_eq!(items[1]["details"]["suggested_call"]["tool"], "list_jobs");
    assert_eq!(
        items[1]["details"]["suggested_call"]["follow_up_kind"],
        "fallback_recovery"
    );
    assert_eq!(
        items[1]["details"]["suggested_call"]["arguments"],
        json!({})
    );
    // Presentation never starts a second command.
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "control-runner".into(),
            runner_instance_id: "inst".into()
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn execution_control_gateway_cannot_apply_to_unrelated_or_hidden_tools() {
    let runtime = test_runtime();
    for tool in ["list_projects", "sync_goal_plan"] {
        let result = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(1)),
                params(tool, json!({}), true, true),
            ),
            None,
        )
        .await;
        if tool == "sync_goal_plan" {
            let McpOutcome::Ok(body) = result else {
                panic!("hidden target response")
            };
            let result = &body["result"]["structuredContent"];
            assert_eq!(result["success"], false);
            assert_eq!(result["output"]["error_kind"], "unknown_tool");
            assert_eq!(result["output"]["state_changed"], false);
            assert!(result["output"].get("execution").is_none());
        } else {
            assert!(matches!(result, McpOutcome::BadRequest(_)), "{tool}");
        }
    }
}

#[test]
fn execution_control_continuation_schema_and_audit_keep_identity_without_cursors() {
    let mut result = crate::tool_runtime::ToolResult::ok(json!({
        "execution":{"state":"running","outcome":"pending","job_id":"wc_job_original_123"},
        "execution_state":"pending",
        "continuation":{"tool":"observe_jobs","arguments":{"items":[{
            "job_id":"wc_job_original_123","after_observation_token":"opaque-cursor"
        }],"wait_secs":5,"wake_on":"terminal"},"follow_up_kind":"fallback_recovery"}
    }));
    crate::mcp::execution_control::project_result(&mut result);
    let next = &result.output["execution"]["next"];
    assert_eq!(next["follow_up_kind"], "fallback_recovery");
    assert_eq!(
        next["arguments"]["items"][0]["after_observation_token"],
        "opaque-cursor"
    );
    let mut args = next["arguments"].clone();
    let parsed =
        crate::mcp::tools::parse_mcp_invocation_envelope("observe_jobs", &mut args, true).unwrap();
    assert!(parsed.metadata.compact_execution);
    crate::tool_runtime::ToolCall::from_tool_name("observe_jobs", args).unwrap();
    let body = json!({"result":{"structuredContent":{"output":result.output}}});
    let audit = mcp_tool_job_audit_correlation(Some("run_process"), &body);
    assert_eq!(audit.async_job_id.as_deref(), Some("wc_job_original_123"));
    let uncertain = json!({"result":{"structuredContent":{"output":{
        "execution":{"state":"outcome_unknown","outcome":"unknown","job_id":"wc_job_original_123"},
        "details":{"promoted_to_job":true,"direct_retry_safe":false}
    }}}});
    let audit = mcp_tool_job_audit_correlation(Some("run_process"), &uncertain);
    assert_eq!(audit.async_job_id.as_deref(), Some("wc_job_original_123"));
    let observed = json!({"result":{"structuredContent":{"output":{"items":[
        {"execution":{"job_id":"wc_job_original_123"},"details":{"project":"agent:runner:repo"}},
        {"execution":{"job_id":"wc_job_original_123"},"details":{"project":"agent:runner:repo"}},
        {"execution":{"job_id":"../unsafe"},"details":{"project":"other"}}
    ]}}}});
    let audit = mcp_tool_job_audit_correlation(Some("observe_jobs"), &observed);
    assert_eq!(audit.observed_job_ids, vec!["wc_job_original_123"]);
    assert_eq!(audit.resolved_project.as_deref(), Some("agent:runner:repo"));
}

#[tokio::test]
async fn execution_control_manifest_keeps_business_arguments_canonical() {
    let runtime = test_runtime();
    for name in [
        "run_process",
        "run_script",
        "run_shell",
        "project_build",
        "run_skill_resource",
        "observe_jobs",
    ] {
        let request = mcp_2026_params(adaptive_runtime_gateway_params(
            "read_tool_manifest",
            json!({"tool_name":name}),
        ));
        let McpOutcome::Ok(body) =
            handle_mcp_request(&runtime, rpc("tools/call", Some(json!(1)), request), None).await
        else {
            panic!("manifest")
        };
        let result = &body["result"]["structuredContent"];
        assert_eq!(result["success"], true, "{body}");
        assert!(
            result["output"]["input_schema"]["properties"]
                .get("_wc")
                .is_none(),
            "{name}"
        );
        assert!(result["output"].get("output_schema").is_none(), "{name}");
        let mut args = json!({"_wc":{"compact_execution":true}});
        assert!(
            crate::mcp::tools::parse_mcp_invocation_envelope(name, &mut args, true)
                .unwrap()
                .metadata
                .compact_execution
        );
    }
}

#[test]
fn execution_control_preserves_bounded_app_metadata_before_split() {
    let mut result = crate::tool_runtime::ToolResult::ok(json!({"items":[{
        "job_id":"job-1","status":"completed","terminal":true,"exit_code":0,
        "stdout_tail":"PRIVATE LOG","execution":{"state":"completed","outcome":"passed","exit_code":0,"job_id":"job-1"}
    }]}));
    let projection =
        crate::mcp::presentation::result_app_presentation("observe_jobs", &result.output).unwrap();
    crate::mcp::execution_control::project_result(&mut result);
    let mut rendered = json!({"structuredContent":{"output":result.output},"_meta":{"keep":true}});
    crate::mcp::presentation::attach_presentation(&mut rendered, projection);
    let app = &rendered["_meta"]["webcodex/presentation"];
    assert_eq!(app["items"][0]["job_id"], "job-1");
    assert_eq!(app["items"][0]["status"], "completed");
    assert!(!app.to_string().contains("PRIVATE"));
    assert_eq!(rendered["_meta"]["keep"], true);
}

#[test]
fn execution_control_budget_falls_back_without_losing_synchronous_output() {
    use crate::json_measurement::serialized_json_len;
    let limit = webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES;
    let mut ordinary = crate::tool_runtime::ToolResult::err_with_output(
        "failed",
        json!({
            "execution_state":"completed","exit_code":1,"stdout_tail":""
        }),
    );
    let remaining = limit - serialized_json_len(&ordinary).unwrap();
    ordinary.output["stdout_tail"] = json!("x".repeat(remaining));
    assert_eq!(serialized_json_len(&ordinary).unwrap(), limit);
    let mut projected =
        crate::tool_runtime::ToolResult::err_with_output("failed", ordinary.output.clone());
    projected.output["execution"] = json!({"state":"completed","outcome":"failed","exit_code":1});
    crate::mcp::execution_control::project_result(&mut projected);
    assert_eq!(projected.output, ordinary.output);
    assert_eq!(serialized_json_len(&projected).unwrap(), limit);
}
