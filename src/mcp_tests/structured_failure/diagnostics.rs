use super::*;

fn command_failure(output: Value, compact: bool) -> ToolResult {
    let error = format!(
        "Command failed.\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: inspect stderr/stdout above.",
        output["stdout_tail"].as_str().unwrap_or_default(),
        output["stderr_tail"].as_str().unwrap_or_default(),
    );
    ToolResult::err_with_output(
        error,
        if compact {
            json!({"execution":{"state":"completed","outcome":"failed","exit_code":1},"details":output})
        } else {
            output
        },
    )
}

#[test]
fn failure_text_is_bounded_without_changing_structured_logs() {
    // Include four-byte scalars to catch byte slicing and prove a UTF-8 bound.
    let limit = crate::tool_runtime::COMMAND_STDIO_TAIL_CHARS;
    for compact in [false, true] {
        for len in [limit - 1, limit, limit + 1, limit * 8] {
            let stdout = format!("{}OUT_END", "🦀".repeat(len - 7));
            let stderr = format!("{}ERR_END", "界".repeat(len - 7));
            let output = json!({
                "tool_failure":false,"failure_kind":"command_exit_nonzero",
                "stdout_tail":stdout,"stderr_tail":stderr,
                "stdout_truncated":false,"stderr_truncated":false,
            });
            let standard = mcp_runtime_tool_result_fallback(
                command_failure(output.clone(), compact),
                false,
                McpToolResultPresentation::Standard,
            );
            let openai = mcp_runtime_tool_result_fallback(
                command_failure(output.clone(), compact),
                false,
                McpToolResultPresentation::OpenAiStructuredFailureCompat,
            );
            assert_eq!(standard["structuredContent"], openai["structuredContent"]);
            assert_eq!(
                standard["structuredContent"]["output"],
                command_failure(output, compact).output
            );
            let text = standard["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("OUT_END"), "missing stdout diagnostic");
            assert!(text.contains("ERR_END"), "missing stderr diagnostic");
            assert_eq!(text.matches("OUT_END").count(), 1);
            assert_eq!(text.matches("ERR_END").count(), 1);
            assert_eq!(text.contains("[truncated]"), len > limit);
            let prose = standard["structuredContent"]["error"].as_str().unwrap();
            assert!(text.chars().count() <= prose.chars().count() + 2 * limit + 100);
            assert!(text.len() <= prose.len() + 2 * limit * 4 + 100);
            assert!(!openai["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("OUT_END"));
        }
    }
}

#[test]
fn failure_text_respects_upstream_truncation_and_missing_logs() {
    for (logs, expected) in [
        (json!({}), "Command failed."),
        (
            json!({"stdout_tail":"","stderr_tail":""}),
            "Command failed.",
        ),
        (
            json!({"stdout_tail":null,"stderr_tail":42}),
            "Command failed.",
        ),
        (
            json!({"stderr_tail":"stderr only"}),
            "Command failed.\n\nstderr_tail:\nstderr only",
        ),
        (
            json!({"stdout_tail":"stdout only"}),
            "Command failed.\n\nstdout_tail:\nstdout only",
        ),
        (
            json!({"stdout_tail":"last line","stdout_truncated":true}),
            "Command failed.\n\nstdout_tail [truncated]:\nlast line",
        ),
        (
            json!({"stderr_tail":"","stderr_truncated":true}),
            "Command failed.\n\nstderr_tail [truncated]:\n",
        ),
    ] {
        for compact in [false, true] {
            let mut output = logs.clone();
            output["tool_failure"] = json!(false);
            output["failure_kind"] = json!("command_exit_nonzero");
            let mut result = command_failure(output, compact);
            result.error = Some("Command failed.".into());
            let expected_structured = serde_json::to_value(&result).unwrap();
            let rendered = mcp_runtime_tool_result_fallback(
                result,
                false,
                McpToolResultPresentation::Standard,
            );
            assert_eq!(rendered["content"][0]["text"], expected);
            assert_eq!(rendered["structuredContent"], expected_structured);
        }
    }
}

#[test]
fn failure_diagnostics_do_not_expand_unrelated_errors_or_json_compat() {
    for compact in [false, true] {
        for (kind, tool_failure) in [
            ("tool_fault", true),
            ("timeout", true),
            ("outcome_unknown", false),
            ("not_started", false),
        ] {
            let mut result = command_failure(
                json!({
                    "failure_kind":kind,"tool_failure":tool_failure,
                    "execution_state":"outcome_unknown","stdout_tail":"unrelated stdout","stderr_tail":"unrelated stderr",
                }),
                compact,
            );
            result.error = Some("Inspect state; do not retry.".into());
            let expected = serde_json::to_value(&result).unwrap();
            let rendered = mcp_runtime_tool_result_fallback(
                result,
                false,
                McpToolResultPresentation::Standard,
            );
            assert_eq!(rendered["structuredContent"], expected);
            assert_eq!(
                rendered["content"][0]["text"],
                "Inspect state; do not retry."
            );
        }
        for presentation in [
            McpToolResultPresentation::Standard,
            McpToolResultPresentation::OpenAiStructuredFailureCompat,
        ] {
            let result = command_failure(
                json!({
                    "failure_kind":"timeout","tool_failure":false,
                    "stdout_tail":"unique stdout","stderr_tail":"unique stderr","stderr_truncated":true,
                }),
                compact,
            );
            let rendered = mcp_runtime_tool_result_fallback(result, true, presentation);
            let text = rendered["content"][0]["text"].as_str().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(text).unwrap(),
                rendered["structuredContent"]
            );
            assert_eq!(text.matches("unique stdout").count(), 1);
            assert_eq!(text.matches("unique stderr").count(), 1);
        }
    }
}

#[test]
fn failure_diagnostics_do_not_require_error_prose_or_expand_success() {
    for success in [false, true] {
        for presentation in [
            McpToolResultPresentation::Standard,
            McpToolResultPresentation::OpenAiStructuredFailureCompat,
        ] {
            let result = ToolResult {
                success,
                error: None,
                output: json!({"tool_failure":false,"failure_kind":"command_exit_nonzero","stdout_tail":"available diagnostic"}),
            };
            let expected = json!({"success":success,"output":result.output,"error":null});
            let rendered = mcp_runtime_tool_result_fallback(result, false, presentation);
            assert_eq!(rendered["structuredContent"], expected);
            let text = rendered["content"][0]["text"].as_str().unwrap();
            if success {
                assert_eq!(text, "WebCodex tool completed successfully.");
            } else {
                assert!(text.starts_with("WebCodex tool failed."));
                assert_eq!(
                    text.contains("available diagnostic"),
                    presentation == McpToolResultPresentation::Standard
                );
            }
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_command_failure_diagnostics_preserve_client_and_execution_contracts() {
    let _env = crate::auth::AuthEnvGuard::new();
    _env.enable_direct_shared_key();
    _env.disable_open_anonymous();
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    register_failure_runner(&runtime).await;
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        runtime.clone(),
    ));
    for tool in ["run_shell", "run_process", "run_script"] {
        for timed_out in [false, true] {
            for compact in [false, true] {
                let mut baseline = None;
                for client in ["generic-test-client", "claude-code", "openai-mcp"] {
                    let arguments = match tool {
                        "run_shell" => json!({"command":"probe"}),
                        "run_process" => json!({"executable":"probe","args":[]}),
                        _ => json!({"language":"python","script":"print('probe')"}),
                    };
                    let mut arguments = arguments;
                    arguments["project"] = json!("agent:failure-runner:probe");
                    arguments["timeout_secs"] = json!(30);
                    arguments["sync_wait_secs"] = json!(30);
                    // Exercise direct and gateway presentation; both must use the same policy.
                    let mut params = if compact {
                        adaptive_runtime_gateway_params(tool, arguments)
                    } else {
                        json!({"name":tool,"arguments":arguments})
                    };
                    if compact {
                        params["arguments"]["_wc"] = json!({"compact_execution":true});
                    }
                    let runner = async {
                        let request = wait_for_failure_request(&runtime).await;
                        runtime
                            .runner_registry
                            .complete(RunnerResultPayload {
                                result: RunnerResultRequest {
                                    client_id: "failure-runner".into(),
                                    runner_instance_id: "inst".into(),
                                    request_id: request.request_id,
                                    exit_code: if timed_out { None } else { Some(1) },
                                    stdout: Some("stdout diagnostic witness".into()),
                                    stderr: Some("stderr diagnostic witness".into()),
                                    stdout_truncated: true,
                                    stderr_truncated: false,
                                    duration_ms: Some(7),
                                    error: None,
                                },
                                command_execution_state: Some(if timed_out {
                                    ShellCommandExecutionState::TimedOut
                                } else {
                                    ShellCommandExecutionState::Completed
                                }),
                                mcp_gateway: None,
                                plugin_gateway: None,
                                coding_agent: None,
                            })
                            .await
                            .unwrap();
                    };
                    let ((status, body), ()) = tokio::join!(
                        http_call(&service, params, client_meta(client, "2"), true),
                        runner
                    );
                    assert_eq!(status, StatusCode::OK, "{body}");
                    let output = assert_failure(&body, client != "openai-mcp");
                    let details = if compact { &output["details"] } else { output };
                    assert_eq!(
                        details["failure_kind"],
                        if timed_out {
                            "timeout"
                        } else {
                            "command_exit_nonzero"
                        }
                    );
                    assert_eq!(details["tool_failure"], false);
                    assert_eq!(details["stdout_tail"], "stdout diagnostic witness");
                    assert_eq!(details["stderr_tail"], "stderr diagnostic witness");
                    let state = if compact {
                        &output["execution"]["state"]
                    } else {
                        &output["execution_state"]
                    };
                    assert_eq!(state, if timed_out { "timed_out" } else { "completed" });
                    let structured = &body["result"]["structuredContent"];
                    let text = body["result"]["content"][0]["text"].as_str().unwrap();
                    assert_eq!(
                        text.contains("stdout diagnostic witness"),
                        client != "openai-mcp"
                    );
                    assert_eq!(
                        text.contains("stderr diagnostic witness"),
                        client != "openai-mcp"
                    );
                    assert!(!structured["error"]
                        .as_str()
                        .unwrap()
                        .contains("diagnostic witness"));
                    if timed_out {
                        assert!(text.contains("cannot prove its side effects ended"));
                        assert!(text.contains("do not blindly retry"));
                    }
                    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                        structured,
                        &crate::tool_runtime::registry::output_schema_for_tool(tool),
                    )
                    .unwrap();
                    if let Some(expected) = &baseline {
                        assert_eq!(structured, expected);
                    } else {
                        baseline = Some(structured.clone());
                    }
                }
            }
        }
    }
}
