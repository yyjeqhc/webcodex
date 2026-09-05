use super::*;
use crate::runner_protocol::{RunnerPolicySummary, RunnerResultPayload};
use std::sync::Arc;
use webcodex_core::plugin::{
    PluginContent, PluginDispatchState, PluginGatewayRequest, PluginGatewayResponse,
    PluginGatewayResponsePayload, PluginPlane, PluginProviderView, PluginTool, PluginToolResult,
    StartupPluginProvider,
};

async fn wait_for_plugin_request(
    registry: &crate::runner_http::RunnerRegistry,
    client_id: &str,
    runner_instance_id: &str,
) -> crate::runner_protocol::RunnerRequest {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(request) = registry
            .poll(RunnerPollRequest {
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
            })
            .await
            .unwrap()
        {
            return request;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "direct startup Plugin call did not dispatch within 10 seconds"
        );
        tokio::task::yield_now().await;
    }
}

async fn complete_plugin_request(
    runtime: &ToolRuntime,
    request: crate::runner_protocol::RunnerRequest,
    runner_instance_id: &str,
    response: PluginGatewayResponse,
) {
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: request.client_id,
                runner_instance_id: runner_instance_id.to_string(),
                request_id: request.request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: Some(response),
            coding_agent: None,
        })
        .await
        .unwrap();
}

fn plugin_auth(include_scope: bool) -> crate::auth::AuthContext {
    let mut auth = mcp_export_api_auth("plugin-test-pat", "alice");
    if include_scope {
        auth.scopes
            .push(crate::auth::SCOPE_PLUGIN_LOCAL.to_string());
    }
    auth
}

fn plugin_tool(name: &str) -> PluginTool {
    PluginTool {
        name: name.to_string(),
        title: Some("Repository Search".to_string()),
        description: Some("Search repository symbols".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        output_schema: Some(json!({
            "type": "object",
            "properties": {"matches": {"type": "array"}}
        })),
        annotations: Some(json!({"readOnlyHint": true})),
    }
}

async fn register_plugin_runner(
    runtime: &ToolRuntime,
    client_id: &str,
    runner_instance_id: &str,
    provider_id: &str,
    provider_instance_id: &str,
    tools: Vec<PluginTool>,
) {
    register_plugin_runner_for_owner(
        runtime,
        client_id,
        runner_instance_id,
        provider_id,
        provider_instance_id,
        "alice",
        tools,
    )
    .await;
}

async fn register_plugin_runner_for_owner(
    runtime: &ToolRuntime,
    client_id: &str,
    runner_instance_id: &str,
    provider_id: &str,
    provider_instance_id: &str,
    owner: &str,
    tools: Vec<PluginTool>,
) {
    register_plugin_runner_with_status(
        runtime,
        client_id,
        runner_instance_id,
        provider_id,
        provider_instance_id,
        owner,
        "ready",
        tools,
    )
    .await;
}

async fn register_plugin_runner_with_status(
    runtime: &ToolRuntime,
    client_id: &str,
    runner_instance_id: &str,
    provider_id: &str,
    provider_instance_id: &str,
    owner: &str,
    status: &str,
    tools: Vec<PluginTool>,
) {
    let mut capabilities = RunnerCapabilities::default();
    capabilities.native_tool_plugins = true;
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: Some(format!("Plugin {client_id}")),
                owner: Some(owner.to_string()),
                hostname: None,
                host_context: None,
                capabilities,
                policy: Some(RunnerPolicySummary {
                    plugin_providers: Some(vec![StartupPluginProvider {
                        provider_id: provider_id.to_string(),
                        provider_instance_id: provider_instance_id.to_string(),
                        name: "Repo Tools".to_string(),
                        status: status.to_string(),
                        error_code: (status != "ready")
                            .then(|| "plugin_initialize_failed".to_string()),
                        tools,
                    }]),
                    ..Default::default()
                }),
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
            },
        ))
        .await
        .unwrap();
}

async fn tools_list(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    stateless: bool,
) -> Value {
    let params = if stateless {
        mcp_2026_params(json!({}))
    } else {
        json!({})
    };
    match handle_mcp_request(
        runtime,
        rpc("tools/list", Some(json!(710)), params),
        Some(auth),
    )
    .await
    {
        McpOutcome::Ok(value) => value,
        other => panic!("tools/list failed: {other:?}"),
    }
}

#[tokio::test]
async fn non_ready_startup_provider_is_never_exposed_directly() {
    let runtime = test_runtime();
    let auth = plugin_auth(true);
    register_plugin_runner_with_status(
        &runtime,
        "runner-failed",
        "runner-instance-failed",
        "repo-tools",
        "provider-instance-failed",
        "alice",
        "failed",
        vec![plugin_tool("should_not_leak")],
    )
    .await;

    let value = tools_list(&runtime, &auth, false).await;
    assert!(!value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "should_not_leak"));
}

#[tokio::test]
async fn startup_plugin_direct_inventory_respects_exact_runner_owner_visibility() {
    let runtime = test_runtime();
    let auth = plugin_auth(true);
    register_plugin_runner_for_owner(
        &runtime,
        "runner-bob",
        "runner-instance-bob",
        "repo-tools",
        "provider-instance-bob",
        "bob",
        vec![plugin_tool("private_search")],
    )
    .await;

    let value = tools_list(&runtime, &auth, false).await;
    assert!(!value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "private_search"));

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(709)),
            json!({"name":"private_search","arguments":{"query":"x"}}),
        ),
        Some(&auth),
    )
    .await;
    assert!(matches!(outcome, McpOutcome::BadRequest(_)));
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-bob".to_string(),
            runner_instance_id: "runner-instance-bob".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn startup_plugin_direct_tools_are_scoped_unique_and_keep_exact_schema() {
    let auth = plugin_auth(true);
    for surface in [
        ModelSurface::LocalCoding,
        ModelSurface::AdaptiveRuntime,
        ModelSurface::FullOperatorRuntime,
    ] {
        let runtime = test_runtime_with_surface(surface);
        register_plugin_runner(
            &runtime,
            "runner-a",
            "runner-instance-a",
            "repo-tools",
            "provider-instance-a",
            vec![plugin_tool("search_symbol")],
        )
        .await;
        let value = tools_list(&runtime, &auth, true).await;
        let tools = value["result"]["tools"].as_array().unwrap();
        let direct = tools
            .iter()
            .find(|tool| tool["name"] == "search_symbol")
            .unwrap_or_else(|| panic!("missing direct Plugin tool on {surface:?}"));
        assert_eq!(
            direct["inputSchema"],
            plugin_tool("search_symbol").input_schema
        );
        let properties = direct["inputSchema"]["properties"].as_object().unwrap();
        for sidecar in [
            "recording_session_id",
            "ack_session_context_revision",
            "ack_session_message_ids",
            "context_request",
        ] {
            assert!(
                !properties.contains_key(sidecar),
                "Plugin schema received WebCodex sidecar {sidecar} on {surface:?}"
            );
        }
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == crate::plugin_gateway::PLUGIN_TOOL_NAME));
    }

    assert!(registered_tool_specs()
        .iter()
        .all(|spec| spec.name != "search_symbol"));

    let runtime = test_runtime();
    register_plugin_runner(
        &runtime,
        "runner-b",
        "runner-instance-b",
        "repo-tools",
        "provider-instance-b",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    let without_scope = tools_list(&runtime, &plugin_auth(false), false).await;
    assert!(!without_scope["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "search_symbol"));
}

#[tokio::test]
async fn startup_plugin_reserved_and_duplicate_names_are_not_directly_exposed() {
    let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
    let auth = plugin_auth(true);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools-a",
        "provider-instance-a",
        vec![plugin_tool("runtime_status"), plugin_tool("search")],
    )
    .await;
    register_plugin_runner(
        &runtime,
        "runner-b",
        "runner-instance-b",
        "repo-tools-b",
        "provider-instance-b",
        vec![plugin_tool("search")],
    )
    .await;

    let value = tools_list(&runtime, &auth, false).await;
    let names = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    assert!(
        !names.contains(&"search"),
        "duplicate Plugin name must be omitted"
    );
    // runtime_status is a globally reserved WebCodex name even though the
    // local_coding surface does not itself advertise that built-in tool.
    assert!(!names.contains(&"runtime_status"));
    assert!(names.contains(&crate::plugin_gateway::PLUGIN_TOOL_NAME));
}

#[tokio::test]
async fn direct_startup_plugin_call_routes_exact_startup_provider_and_renders_result() {
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::LocalCoding));
    let auth = plugin_auth(true);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "provider-instance-a",
        vec![plugin_tool("search_symbol")],
    )
    .await;

    let request_runtime = Arc::clone(&runtime);
    let request_auth = auth.clone();
    let task = tokio::spawn(async move {
        handle_mcp_request(
            &request_runtime,
            rpc(
                "tools/call",
                Some(json!(711)),
                json!({
                    "name": "search_symbol",
                    "arguments": {"query": "RunnerRegistry"}
                }),
            ),
            Some(&request_auth),
        )
        .await
    });

    let request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    let Some(PluginGatewayRequest::ToolsCall {
        plane,
        provider_id,
        provider_instance_id,
        name,
        arguments,
        expected_schema,
    }) = request.plugin_gateway.clone()
    else {
        panic!("direct Plugin call did not use typed plugin_gateway: {request:?}");
    };
    assert_eq!(request.kind, "plugin_gateway");
    assert_eq!(plane, PluginPlane::Startup);
    assert_eq!(provider_id, "repo-tools");
    assert_eq!(provider_instance_id, "provider-instance-a");
    assert_eq!(name, "search_symbol");
    assert_eq!(arguments, json!({"query":"RunnerRegistry"}));
    assert_eq!(
        expected_schema,
        plugin_tool("search_symbol").schema_observation()
    );

    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: "runner-a".to_string(),
                runner_instance_id: "runner-instance-a".to_string(),
                request_id: request.request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: Some(PluginGatewayResponse::success(
                PluginGatewayResponsePayload::ToolResult {
                    result: PluginToolResult {
                        content: vec![PluginContent::Text {
                            text: "found RunnerRegistry".to_string(),
                        }],
                        structured_content: Some(json!({"matches":["RunnerRegistry"]})),
                        is_error: false,
                    },
                },
            )),
            coding_agent: None,
        })
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    let McpOutcome::Ok(value) = outcome else {
        panic!("direct Plugin call failed: {outcome:?}");
    };
    assert_eq!(
        value["result"]["content"][0]["text"],
        "found RunnerRegistry"
    );
    assert_eq!(
        value["result"]["structuredContent"],
        json!({"matches":["RunnerRegistry"]})
    );
    assert_eq!(value["result"]["isError"], false);
}

#[tokio::test]
async fn direct_plugin_scope_and_ambiguity_fail_before_runner_dispatch() {
    let runtime = test_runtime();
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools-a",
        "provider-instance-a",
        vec![plugin_tool("search")],
    )
    .await;

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(712)),
            json!({"name":"search","arguments":{"query":"x"}}),
        ),
        Some(&plugin_auth(false)),
    )
    .await;
    match outcome {
        McpOutcome::Forbidden { required_scope, .. } => {
            assert_eq!(required_scope, Some(crate::auth::SCOPE_PLUGIN_LOCAL));
        }
        other => panic!("missing Plugin scope must be forbidden, got {other:?}"),
    }
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-a".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    register_plugin_runner(
        &runtime,
        "runner-b",
        "runner-instance-b",
        "repo-tools-b",
        "provider-instance-b",
        vec![plugin_tool("search")],
    )
    .await;
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(713)),
            json!({"name":"search","arguments":{"query":"x"}}),
        ),
        Some(&plugin_auth(true)),
    )
    .await;
    match outcome {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert!(value["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("ambiguous"));
        }
        other => panic!("ambiguous Plugin name must fail closed, got {other:?}"),
    }
    for (client_id, instance) in [
        ("runner-a", "runner-instance-a"),
        ("runner-b", "runner-instance-b"),
    ] {
        assert!(runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: client_id.to_string(),
                runner_instance_id: instance.to_string(),
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn plugin_tool_reload_describe_call_binds_exact_dynamic_provider_and_forgets_replacement() {
    let runtime = Arc::new(test_runtime());
    let auth = plugin_auth(true);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "startup-provider-instance",
        vec![plugin_tool("search_symbol")],
    )
    .await;

    let dynamic_provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "dynamic-provider-instance".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };

    let reload_runtime = Arc::clone(&runtime);
    let reload_auth = auth.clone();
    let reload_task = tokio::spawn(async move {
        handle_mcp_request(
            &reload_runtime,
            rpc(
                "tools/call",
                Some(json!(720)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {"action":"reload","runner":"runner-a"}
                }),
            ),
            Some(&reload_auth),
        )
        .await
    });
    let reload_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        reload_request.plugin_gateway,
        Some(PluginGatewayRequest::Reload)
    ));
    complete_plugin_request(
        &runtime,
        reload_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Reloaded {
            providers: vec![dynamic_provider.clone()],
            failures: vec![],
            first_class_restart_required: true,
        }),
    )
    .await;
    let McpOutcome::Ok(reload_result) = reload_task.await.unwrap() else {
        panic!("plugin_tool reload did not complete successfully");
    };
    assert_eq!(reload_result["result"]["isError"], false);
    assert_eq!(
        reload_result["result"]["structuredContent"]["firstClassRestartRequired"],
        true
    );

    let describe_runtime = Arc::clone(&runtime);
    let describe_auth = auth.clone();
    let describe_task = tokio::spawn(async move {
        handle_mcp_request(
            &describe_runtime,
            rpc(
                "tools/call",
                Some(json!(721)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {
                        "action":"describe",
                        "runner":"runner-a",
                        "plugin":"repo-tools",
                        "tool":"search_symbol"
                    }
                }),
            ),
            Some(&describe_auth),
        )
        .await
    });
    let providers_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        providers_request.plugin_gateway,
        Some(PluginGatewayRequest::ProvidersList)
    ));
    complete_plugin_request(
        &runtime,
        providers_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![dynamic_provider.clone()],
            first_class_restart_required: true,
        }),
    )
    .await;
    let tools_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        tools_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsList {
            plane: PluginPlane::Effective,
            ref provider_id,
            ref provider_instance_id,
        }) if provider_id == "repo-tools" && provider_instance_id == "dynamic-provider-instance"
    ));
    complete_plugin_request(
        &runtime,
        tools_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Tools {
            tools: vec![plugin_tool("search_symbol")],
        }),
    )
    .await;
    let McpOutcome::Ok(describe_result) = describe_task.await.unwrap() else {
        panic!("plugin_tool describe did not complete successfully");
    };
    assert_eq!(describe_result["result"]["isError"], false);
    assert_eq!(
        describe_result["result"]["structuredContent"]["plugin"],
        "repo-tools"
    );

    let call_runtime = Arc::clone(&runtime);
    let call_auth = auth.clone();
    let call_task = tokio::spawn(async move {
        handle_mcp_request(
            &call_runtime,
            rpc(
                "tools/call",
                Some(json!(722)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {
                        "action":"call",
                        "runner":"runner-a",
                        "plugin":"repo-tools",
                        "tool":"search_symbol",
                        "arguments":{"query":"PluginManager"}
                    }
                }),
            ),
            Some(&call_auth),
        )
        .await
    });
    let call_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    let Some(PluginGatewayRequest::ToolsCall {
        plane,
        provider_instance_id,
        name,
        arguments,
        expected_schema,
        ..
    }) = call_request.plugin_gateway.clone()
    else {
        panic!("plugin_tool call did not use typed Plugin gateway");
    };
    assert_eq!(plane, PluginPlane::Effective);
    assert_eq!(provider_instance_id, "dynamic-provider-instance");
    assert_eq!(name, "search_symbol");
    assert_eq!(arguments, json!({"query":"PluginManager"}));
    assert_eq!(
        expected_schema,
        plugin_tool("search_symbol").schema_observation()
    );
    complete_plugin_request(
        &runtime,
        call_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::ToolResult {
            result: PluginToolResult {
                content: vec![PluginContent::Text {
                    text: "found PluginManager".to_string(),
                }],
                structured_content: Some(json!({"matches":["PluginManager"]})),
                is_error: false,
            },
        }),
    )
    .await;
    let McpOutcome::Ok(call_result) = call_task.await.unwrap() else {
        panic!("plugin_tool call did not complete successfully");
    };
    assert_eq!(
        call_result["result"]["content"][0]["text"],
        "found PluginManager"
    );

    let replacement_runtime = Arc::clone(&runtime);
    let replacement_auth = auth.clone();
    let replacement_task = tokio::spawn(async move {
        handle_mcp_request(
            &replacement_runtime,
            rpc(
                "tools/call",
                Some(json!(723)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {
                        "action":"call",
                        "runner":"runner-a",
                        "plugin":"repo-tools",
                        "tool":"search_symbol",
                        "arguments":{"query":"stale"}
                    }
                }),
            ),
            Some(&replacement_auth),
        )
        .await
    });
    let replacement_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        replacement_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Effective,
            ref provider_instance_id,
            ..
        }) if provider_instance_id == "dynamic-provider-instance"
    ));
    complete_plugin_request(
        &runtime,
        replacement_request,
        "runner-instance-a",
        PluginGatewayResponse::error(
            PluginDispatchState::NotStarted,
            "stale_plugin_provider",
            "Exact Plugin provider instance is no longer available",
        ),
    )
    .await;
    let McpOutcome::Ok(replacement_result) = replacement_task.await.unwrap() else {
        panic!("replacement response was not rendered as Plugin tool result");
    };
    assert_eq!(replacement_result["result"]["isError"], true);
    assert_eq!(
        replacement_result["result"]["structuredContent"]["error"]["code"],
        "plugin_replaced"
    );

    let no_redispatch = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(724)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "runner":"runner-a",
                    "plugin":"repo-tools",
                    "tool":"search_symbol",
                    "arguments":{"query":"must-describe-again"}
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(no_redispatch) = no_redispatch else {
        panic!("describe-required response should be a normal MCP tool result");
    };
    assert_eq!(no_redispatch["result"]["isError"], true);
    assert_eq!(
        no_redispatch["result"]["structuredContent"]["error"]["code"],
        "describe_required"
    );
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-a".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}
