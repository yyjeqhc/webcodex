use super::*;
use crate::runner_protocol::{
    RunnerPolicySummary, RunnerResultPayload, ShellCommandExecutionState,
};
use response::{mcp_runtime_tool_result_fallback, McpToolResultPresentation};
use webcodex_core::mcp_gateway::{
    McpGatewayContent, McpGatewayProvider, McpGatewayRequest, McpGatewayResponse,
    McpGatewayResponsePayload, McpGatewayTool, McpGatewayToolResult,
};

fn client_meta(name: &str, version: &str) -> Value {
    json!({"io.modelcontextprotocol/clientInfo": {"name": name, "version": version}})
}

async fn http_call(
    service: &Service,
    mut params: Value,
    meta: Value,
    modern: bool,
) -> (StatusCode, Value) {
    params["_meta"] = meta;
    let mut request = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        // Neither the User-Agent nor unrelated metadata selects presentation.
        .add_header("user-agent", "openai-mcp", true);
    if modern {
        params["_meta"]["io.modelcontextprotocol/protocolVersion"] =
            json!(MCP_STATELESS_PROTOCOL_VERSION);
        params["_meta"]["io.modelcontextprotocol/clientCapabilities"] = json!({});
        request = request
            .add_header(
                MCP_PROTOCOL_VERSION_HEADER,
                MCP_STATELESS_PROTOCOL_VERSION,
                true,
            )
            .add_header(MCP_METHOD_HEADER, "tools/call", true)
            .add_header(
                MCP_NAME_HEADER,
                params["name"].as_str().unwrap_or("invalid"),
                true,
            );
    }
    let mut response = request
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params,
        }))
        .send(service)
        .await;
    (
        effective_status(&response),
        response.take_json().await.unwrap(),
    )
}

fn assert_failure(body: &Value, is_error: bool) -> &Value {
    assert!(body.get("error").is_none(), "{body}");
    assert_eq!(body["result"]["isError"], is_error, "{body}");
    assert_eq!(
        body["result"]["structuredContent"]["success"], false,
        "{body}"
    );
    &body["result"]["structuredContent"]["output"]
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_client_presentation_matrix_preserves_native_failure() {
    let _env = crate::auth::AuthEnvGuard::new();
    _env.enable_direct_shared_key();
    _env.disable_open_anonymous();
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for modern in [false, true] {
        let mut baseline = None;
        for (meta, compat) in [
            (client_meta("generic-test-client", "1.0.0"), false),
            (json!({}), false),
            (
                json!({"openai/session": "test", "user": "openai-mcp", "organization": "openai-mcp"}),
                false,
            ),
            (client_meta("OpenAI-MCP", "1.0.0"), false),
            (client_meta("openai-mcp-preview", "1.0.0"), false),
            (client_meta("openai-mcp ", "1.0.0"), false),
            (client_meta("openai-mcp", "1.0.0"), true),
            (client_meta("openai-mcp", "9.8.7"), true),
        ] {
            let (status, mut body) = http_call(
                &service,
                json!({
                    "name": "show_changes", "arguments": {"project": "agent:missing:missing"},
                }),
                meta,
                modern,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            let output = assert_failure(&body, !compat);
            assert_eq!(output["error_kind"], "unknown_project");
            body["result"]["isError"] = json!(true);
            if let Some(expected) = &baseline {
                assert_eq!(&body, expected, "only isError may differ");
            } else {
                baseline = Some(body);
            }
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_openai_protocol_errors_remain_jsonrpc_errors() {
    let _env = crate::auth::AuthEnvGuard::new();
    _env.enable_direct_shared_key();
    _env.disable_open_anonymous();
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for modern in [false, true] {
        for params in [
            json!({"name": "show_changes", "arguments": {"project": 42}}),
            json!({"name": "show_changes", "arguments": []}),
            // A malformed envelope must still fail McpToolCallParams deserialization.
            json!({"name": 42, "arguments": {}}),
        ] {
            let invalid_name = !params["name"].is_string();
            let (status, body) =
                http_call(&service, params, client_meta("openai-mcp", "2"), modern).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(
                body["error"]["code"],
                if modern && invalid_name {
                    MCP_HEADER_MISMATCH
                } else {
                    -32602
                }
            );
            assert!(body.get("result").is_none(), "{body}");
        }
    }
    for info in [
        json!({"name": "openai-mcp"}),
        json!({"name": "openai-mcp", "version": 1}),
        json!("openai-mcp"),
    ] {
        let (status, body) = http_call(
            &service,
            json!({"name": "show_changes", "arguments": {"project": "missing"}}),
            json!({"io.modelcontextprotocol/clientInfo": info}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], -32602);
        assert!(body.get("result").is_none());
    }
}

#[test]
fn resource_failure_fallbacks_use_canonical_presentation_policy() {
    for policy in [
        McpToolResultPresentation::Standard,
        McpToolResultPresentation::OpenAiStructuredFailureCompat,
    ] {
        let failed = || {
            ToolResult::err_with_output(
                "snapshot unavailable",
                json!({"execution_state": "outcome_unknown", "recovery": "inspect state"}),
            )
        };
        let expected = mcp_runtime_tool_result_fallback(failed(), policy);
        assert_eq!(
            mcp_artifact_export_tool_result(
                failed(),
                McpArtifactExportCallerBinding::Bootstrap,
                policy
            ),
            expected
        );
        for tool in [
            "computer_observe",
            "browser_observe",
            "read_project_artifact",
        ] {
            assert_eq!(
                mcp_runtime_tool_result_with_snapshot_resource(tool, true, failed(), None, policy),
                expected
            );
            let invalid_image = mcp_runtime_tool_result_with_snapshot_resource(
                tool,
                true,
                ToolResult::ok(json!({})),
                None,
                policy,
            );
            assert_eq!(invalid_image["structuredContent"]["success"], false);
            assert_eq!(
                invalid_image["isError"],
                policy == McpToolResultPresentation::Standard
            );
        }
        let invalid_export = mcp_artifact_export_tool_result(
            ToolResult::ok(json!({})),
            McpArtifactExportCallerBinding::Bootstrap,
            policy,
        );
        assert_eq!(invalid_export["structuredContent"]["success"], false);
        assert_eq!(
            invalid_export["isError"],
            policy == McpToolResultPresentation::Standard
        );
    }
}

#[tokio::test]
async fn gateway_and_app_canonical_failures_use_request_presentation() {
    let (_tmp, db) = test_db();
    let runtime = test_runtime().with_communication_database(db);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    for call_params in [
        adaptive_runtime_gateway_params(
            "show_changes",
            json!({"project": "agent:missing:missing"}),
        ),
        adaptive_runtime_gateway_params("unknown_target", json!({})),
        json!({"name": "goal_plan_sync", "arguments": {"goal_id": "wc_goal_AAAAAAAAAAAAAAAA"}}),
        json!({"name": "agent_continuation_state", "arguments": {
            "agent_id": "wc_dagent_AAAAAAAAAAAAAAAA", "endpoint_id": "wc_endpoint_AAAAAAAAAAAAAAAA",
            "expected_controller_generation": 1, "binding_id": "wc_host_binding_AAAAAAAAAAAAAAAAAAAAAA"
        }}),
    ] {
        let mut baseline = None;
        for client in ["generic-test-client", "openai-mcp"] {
            let mut params = mcp_2026_ui_params(call_params.clone());
            params["_meta"]["io.modelcontextprotocol/clientInfo"] =
                json!({"name": client, "version": "2"});
            let outcome = handle_mcp_request_with_lifecycle(
                &runtime,
                rpc("tools/call", Some(json!(1)), params),
                Some(&auth),
                McpProtocolEra::Stateless2026,
                HostFileImportTrust::Untrusted,
                None,
                None,
                None,
                true,
                true,
                None,
            )
            .await;
            let McpOutcome::Ok(mut body) = outcome else {
                panic!("expected canonical failure: {outcome:?}");
            };
            assert_failure(&body, client != "openai-mcp");
            body["result"]["isError"] = json!(true);
            if let Some(expected) = &baseline {
                assert_eq!(&body, expected);
            } else {
                baseline = Some(body);
            }
        }
    }
}

async fn register_failure_runner(runtime: &ToolRuntime) {
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: "failure-runner".into(),
                runner_instance_id: "inst".into(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                policy: Some(RunnerPolicySummary {
                    mcp_gateway_providers: Some(vec![McpGatewayProvider {
                        provider_id: "probe-provider".into(),
                        provider_instance_id: "provider-inst".into(),
                        name: "Probe".into(),
                    }]),
                    ..Default::default()
                }),
                capabilities: RunnerCapabilities {
                    file_write: true,
                    shell: true,
                    structured_process_argv: true,
                    apply_text_edit_local_guard_without_sha: true,
                    apply_text_edit_occurrence: true,
                    ..Default::default()
                },
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "failure-runner",
        "inst",
        vec![RunnerProjectSummary {
            id: "probe".into(),
            name: None,
            path: "/tmp/structured-failure-probe".into(),
            allow_patch: true,
            kind: Some("repo".into()),
            registration_source: None,
            description: None,
            hooks: vec![],
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
}

async fn wait_for_failure_request(runtime: &ToolRuntime) -> crate::runner_protocol::RunnerRequest {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "fixture request did not dispatch"
        );
        if let Some(request) = runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: "failure-runner".into(),
                runner_instance_id: "inst".into(),
            })
            .await
            .unwrap()
        {
            return request;
        }
        tokio::task::yield_now().await;
    }
}

async fn complete_failure_request(runtime: &ToolRuntime, tool: &str) {
    let request = wait_for_failure_request(runtime).await;
    let edit = tool == "apply_text_edits";
    assert_eq!(
        request.kind,
        if edit {
            "file_apply_text_edits"
        } else {
            "run_process"
        }
    );
    let stdout = if edit {
        json!({
            "changed": false, "error_kind": "edit_conflict", "state_changed": false,
            "change_index": 0, "edit_index": 0, "kind": "replace_exact", "path": "probe.txt",
            "conflict_recovery": {
                "schema_version": 1, "conflict_kind": "multiple_matches", "match_count": 2,
                "occurrence_selector_supported": true, "direct_retry_safe": true, "reread_required": false,
                "candidate_ranges": [{"occurrence": 1, "start_line": 10, "end_line": 10}, {"occurrence": 2, "start_line": 20, "end_line": 20}],
                "candidates_truncated": false, "recovery_action": "select_occurrence_or_refine_match"
            }, "error": "exact target matched multiple locations"
        }).to_string()
    } else {
        "observed output".into()
    };
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: "failure-runner".into(),
                runner_instance_id: "inst".into(),
                request_id: request.request_id,
                exit_code: Some(if edit { 0 } else { 1 }),
                stdout: Some(stdout),
                stderr: Some(if edit { "" } else { "process diagnostics" }.into()),
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(7),
                error: None,
            },
            command_execution_state: (!edit).then_some(ShellCommandExecutionState::Completed),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await
        .unwrap();
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_openai_mcp_passthrough_preserves_provider_error() {
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
    let provider_result = McpGatewayToolResult {
        content: vec![McpGatewayContent::Text {
            text: "provider rejected the request".into(),
        }],
        structured_content: Some(
            json!({"success": false, "output": {"provider_code": "REJECTED"}, "error": "provider failure"}),
        ),
        is_error: true,
    };
    for (action, client) in [
        ("describe", "openai-mcp"),
        ("call", "generic-test-client"),
        ("call", "openai-mcp"),
    ] {
        let mut arguments = json!({"action": action, "server": "probe-provider", "tool": "probe"});
        if action == "call" {
            arguments["arguments"] = json!({});
        }
        let complete = async {
            let request = wait_for_failure_request(&runtime).await;
            let payload = if action == "describe" {
                assert!(matches!(
                    request.mcp_gateway,
                    Some(McpGatewayRequest::ToolsList { .. })
                ));
                McpGatewayResponsePayload::Tools {
                    tools: vec![McpGatewayTool {
                        name: "probe".into(),
                        title: None,
                        description: None,
                        input_schema: json!({"type": "object"}),
                        output_schema: None,
                        annotations: None,
                        meta: None,
                    }],
                }
            } else {
                assert!(matches!(
                    request.mcp_gateway,
                    Some(McpGatewayRequest::ToolsCall { .. })
                ));
                McpGatewayResponsePayload::ToolResult {
                    result: provider_result.clone(),
                }
            };
            runtime
                .runner_registry
                .complete(RunnerResultPayload {
                    result: RunnerResultRequest {
                        client_id: "failure-runner".into(),
                        runner_instance_id: "inst".into(),
                        request_id: request.request_id,
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        stdout_truncated: false,
                        stderr_truncated: false,
                        duration_ms: None,
                        error: None,
                    },
                    command_execution_state: None,
                    mcp_gateway: Some(McpGatewayResponse::success(payload)),
                    plugin_gateway: None,
                    coding_agent: None,
                })
                .await
                .unwrap();
        };
        let ((status, body), ()) = tokio::join!(
            http_call(
                &service,
                json!({"name": "mcp_tool", "arguments": arguments}),
                client_meta(client, "2"),
                false
            ),
            complete,
        );
        assert_eq!(status, StatusCode::OK, "{body}");
        if action == "call" {
            assert_eq!(
                body["result"],
                serde_json::to_value(&provider_result).unwrap()
            );
        } else {
            assert_eq!(body["result"]["isError"], false, "{body}");
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_passthrough_preserves_mixed_image_content_order() {
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
    let provider_result = McpGatewayToolResult {
        content: vec![
            McpGatewayContent::Text {
                text: "before".into(),
            },
            McpGatewayContent::Image {
                data: "iVBORw0KGgo=".into(),
                mime_type: "image/png".into(),
            },
            McpGatewayContent::Text {
                text: "after".into(),
            },
        ],
        structured_content: Some(json!({"kind": "mixed"})),
        is_error: false,
    };

    for action in ["describe", "call"] {
        let mut arguments = json!({"action": action, "server": "probe-provider", "tool": "probe"});
        if action == "call" {
            arguments["arguments"] = json!({});
        }
        let complete = async {
            let request = wait_for_failure_request(&runtime).await;
            let payload = if action == "describe" {
                assert!(matches!(
                    request.mcp_gateway,
                    Some(McpGatewayRequest::ToolsList { .. })
                ));
                McpGatewayResponsePayload::Tools {
                    tools: vec![McpGatewayTool {
                        name: "probe".into(),
                        title: None,
                        description: None,
                        input_schema: json!({"type": "object"}),
                        output_schema: None,
                        annotations: None,
                        meta: None,
                    }],
                }
            } else {
                assert!(matches!(
                    request.mcp_gateway,
                    Some(McpGatewayRequest::ToolsCall { .. })
                ));
                McpGatewayResponsePayload::ToolResult {
                    result: provider_result.clone(),
                }
            };
            runtime
                .runner_registry
                .complete(RunnerResultPayload {
                    result: RunnerResultRequest {
                        client_id: "failure-runner".into(),
                        runner_instance_id: "inst".into(),
                        request_id: request.request_id,
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        stdout_truncated: false,
                        stderr_truncated: false,
                        duration_ms: None,
                        error: None,
                    },
                    command_execution_state: None,
                    mcp_gateway: Some(McpGatewayResponse::success(payload)),
                    plugin_gateway: None,
                    coding_agent: None,
                })
                .await
                .unwrap();
        };
        let ((status, body), ()) = tokio::join!(
            http_call(
                &service,
                json!({"name": "mcp_tool", "arguments": arguments}),
                client_meta("openai-mcp", "2"),
                false
            ),
            complete,
        );
        assert_eq!(status, StatusCode::OK, "{body}");
        if action == "call" {
            assert_eq!(
                body["result"],
                serde_json::to_value(&provider_result).unwrap()
            );
            assert_eq!(body["result"]["content"][0]["text"], "before");
            assert_eq!(body["result"]["content"][1]["type"], "image");
            assert_eq!(body["result"]["content"][1]["data"], "iVBORw0KGgo=");
            assert_eq!(body["result"]["content"][1]["mimeType"], "image/png");
            assert_eq!(body["result"]["content"][2]["text"], "after");
            assert_eq!(
                body["result"]["structuredContent"],
                json!({"kind": "mixed"})
            );
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_apply_text_edits_and_process_failures_preserve_canonical_output() {
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
    for (tool, arguments) in [
        (
            "apply_text_edits",
            json!({"project": "agent:failure-runner:probe", "changes": [{"kind": "edit", "path": "probe.txt", "edits": [{"kind": "replace_exact", "old_text": "dup", "new_text": "replacement"}]}]}),
        ),
        (
            "run_process",
            json!({"project": "agent:failure-runner:probe", "executable": "probe", "args": [], "purpose": "diagnostic", "timeout_secs": 30, "sync_wait_secs": 30, "accepted_exit_codes": [0, 1], "result_expectation": "observe"}),
        ),
    ] {
        let mut baseline = None;
        for client in ["generic-test-client", "openai-mcp"] {
            let (response, ()) = tokio::join!(
                http_call(
                    &service,
                    json!({"name": tool, "arguments": arguments}),
                    client_meta(client, "2"),
                    true
                ),
                complete_failure_request(&runtime, tool),
            );
            let (status, mut body) = response;
            assert_eq!(status, StatusCode::OK, "{body}");
            let output = assert_failure(&body, client != "openai-mcp");
            if tool == "apply_text_edits" {
                assert_eq!(output["error_kind"], "multiple_matches");
                assert_eq!(output["state_changed"], false);
                assert_eq!(output["execution_state"], "not_started");
                assert_eq!(output["match_count"], 2);
                assert_eq!(
                    output["candidate_ranges"],
                    json!([{"start_line": 10, "end_line": 10}, {"start_line": 20, "end_line": 20}])
                );
                assert_eq!(output["recovery"]["tool"], "read_files");
                assert_eq!(
                    output["recovery"]["arguments"]["items"][0]["path"],
                    "probe.txt"
                );
            } else {
                assert_eq!(output["execution_state"], "completed");
                assert_eq!(output["exit_code"], 1);
                assert_eq!(output["expectation_satisfied"], true);
                assert_eq!(output["stdout_tail"], "observed output");
                assert_eq!(output["stderr_tail"], "process diagnostics");
                assert!(output.get("accepted_exit_codes").is_none());
                assert!(output.get("result_expectation").is_none());
            }
            body["result"]["isError"] = json!(true);
            if let Some(expected) = &baseline {
                assert_eq!(&body, expected);
            } else {
                baseline = Some(body);
            }
        }
    }
}
