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
    plugin_auth_for("alice", include_scope)
}

fn plugin_auth_for(owner: &str, include_scope: bool) -> crate::auth::AuthContext {
    let mut auth = mcp_export_api_auth("plugin-test-pat", owner);
    if include_scope {
        auth.scopes.extend([
            crate::auth::SCOPE_PLUGIN_INSPECT.to_string(),
            crate::auth::SCOPE_PLUGIN_INVOKE.to_string(),
            crate::auth::SCOPE_PLUGIN_MANAGE.to_string(),
        ]);
    }
    auth
}

fn plugin_auth_with_scopes(scopes: &[&str]) -> crate::auth::AuthContext {
    let mut auth = mcp_export_api_auth("plugin-test-pat", "alice");
    auth.user_id = Some("plugin-test-user-alice".to_string());
    auth.scopes
        .extend(scopes.iter().map(|scope| (*scope).to_string()));
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
                        error_code: match status {
                            "ready" => None,
                            "ready_secondary" => Some("first_class_catalog_too_large".to_string()),
                            _ => Some("plugin_initialize_failed".to_string()),
                        },
                        catalog_tool_count: tools.len(),
                        catalog_digest: None,
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

async fn register_non_plugin_runner_for_owner(
    runtime: &ToolRuntime,
    client_id: &str,
    runner_instance_id: &str,
    owner: &str,
) {
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: Some(format!("Plain {client_id}")),
                owner: Some(owner.to_string()),
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities::default(),
                policy: Some(RunnerPolicySummary::default()),
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

async fn describe_dynamic_binding(
    runtime: &Arc<ToolRuntime>,
    auth: &crate::auth::AuthContext,
    runner_id: &str,
    runner_instance_id: &str,
    provider: PluginProviderView,
    tool: PluginTool,
    rpc_id: u64,
) -> (String, Value) {
    let request_runtime = Arc::clone(runtime);
    let request_auth = auth.clone();
    let runner = runner_id.to_string();
    let plugin = provider.provider_id.clone();
    let tool_name = tool.name.clone();
    let task = tokio::spawn(async move {
        handle_mcp_request(
            &request_runtime,
            rpc(
                "tools/call",
                Some(json!(rpc_id)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {
                        "action":"describe",
                        "runner":runner,
                        "plugin":plugin,
                        "tool":tool_name
                    }
                }),
            ),
            Some(&request_auth),
        )
        .await
    });

    let providers_request =
        wait_for_plugin_request(&runtime.runner_registry, runner_id, runner_instance_id).await;
    assert!(matches!(
        providers_request.plugin_gateway,
        Some(PluginGatewayRequest::ProvidersList)
    ));
    complete_plugin_request(
        runtime,
        providers_request,
        runner_instance_id,
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![provider.clone()],
            first_class_restart_required: true,
        }),
    )
    .await;

    let tools_request =
        wait_for_plugin_request(&runtime.runner_registry, runner_id, runner_instance_id).await;
    assert!(matches!(
        tools_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsList {
            plane: PluginPlane::Effective,
            ref provider_id,
            ref provider_instance_id,
        }) if provider_id == &provider.provider_id
            && provider_instance_id == &provider.provider_instance_id
    ));
    complete_plugin_request(
        runtime,
        tools_request,
        runner_instance_id,
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Tools { tools: vec![tool] }),
    )
    .await;

    let McpOutcome::Ok(result) = task.await.unwrap() else {
        panic!("plugin_tool describe did not complete successfully");
    };
    assert_eq!(result["result"]["isError"], false);
    let binding = result["result"]["structuredContent"]["binding"]
        .as_str()
        .expect("describe must return opaque binding")
        .to_string();
    assert!(binding.starts_with("wc_pbind_"));
    let encoded = serde_json::to_string(&result["result"]["structuredContent"]).unwrap();
    assert!(!encoded.contains(runner_instance_id));
    assert!(!encoded.contains(&provider.provider_instance_id));
    (binding, result)
}

fn spawn_binding_call(
    runtime: &Arc<ToolRuntime>,
    auth: &crate::auth::AuthContext,
    binding: String,
    arguments: Value,
    rpc_id: u64,
) -> tokio::task::JoinHandle<McpOutcome> {
    let request_runtime = Arc::clone(runtime);
    let request_auth = auth.clone();
    tokio::spawn(async move {
        handle_mcp_request(
            &request_runtime,
            rpc(
                "tools/call",
                Some(json!(rpc_id)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {
                        "action":"call",
                        "binding":binding,
                        "arguments":arguments
                    }
                }),
            ),
            Some(&request_auth),
        )
        .await
    })
}

fn spawn_plugin_metadata_call(
    runtime: &Arc<ToolRuntime>,
    auth: &crate::auth::AuthContext,
    arguments: Value,
    rpc_id: u64,
) -> tokio::task::JoinHandle<McpOutcome> {
    let request_runtime = Arc::clone(runtime);
    let request_auth = auth.clone();
    tokio::spawn(async move {
        handle_mcp_request(
            &request_runtime,
            rpc(
                "tools/call",
                Some(json!(rpc_id)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": arguments
                }),
            ),
            Some(&request_auth),
        )
        .await
    })
}

#[tokio::test]
async fn plugin_operation_scopes_are_independent_and_fail_closed() {
    let runtime = test_runtime();
    let inspect = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INSPECT]);
    let invoke = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INVOKE]);
    let manage = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_MANAGE]);

    let inspect_list = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(680)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {"action":"list"}
            }),
        ),
        Some(&inspect),
    )
    .await;
    let McpOutcome::Ok(inspect_list) = inspect_list else {
        panic!("plugin:inspect must allow list");
    };
    assert_eq!(inspect_list["result"]["isError"], false);

    for (id, arguments, required) in [
        (
            681,
            json!({"action":"call"}),
            crate::auth::SCOPE_PLUGIN_INVOKE,
        ),
        (
            682,
            json!({"action":"check"}),
            crate::auth::SCOPE_PLUGIN_MANAGE,
        ),
        (
            683,
            json!({"action":"reload"}),
            crate::auth::SCOPE_PLUGIN_MANAGE,
        ),
    ] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(id)),
                json!({"name": crate::plugin_gateway::PLUGIN_TOOL_NAME, "arguments": arguments}),
            ),
            Some(&inspect),
        )
        .await;
        match outcome {
            McpOutcome::Forbidden { required_scope, .. } => {
                assert_eq!(required_scope, Some(required));
            }
            other => panic!("plugin:inspect must not escalate to {required}: {other:?}"),
        }
    }

    let invoke_call = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(684)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {"action":"call"}
            }),
        ),
        Some(&invoke),
    )
    .await;
    let McpOutcome::Ok(invoke_call) = invoke_call else {
        panic!("plugin:invoke must pass scope governance for call");
    };
    assert_eq!(invoke_call["result"]["isError"], true);
    assert_eq!(
        invoke_call["result"]["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
    for (id, action) in [(685, "check"), (686, "reload")] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(id)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": {"action":action}
                }),
            ),
            Some(&invoke),
        )
        .await;
        match outcome {
            McpOutcome::Forbidden { required_scope, .. } => {
                assert_eq!(required_scope, Some(crate::auth::SCOPE_PLUGIN_MANAGE));
            }
            other => panic!("plugin:invoke must not manage Plugins: {other:?}"),
        }
    }

    let manage_call = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(687)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {"action":"call"}
            }),
        ),
        Some(&manage),
    )
    .await;
    match manage_call {
        McpOutcome::Forbidden { required_scope, .. } => {
            assert_eq!(required_scope, Some(crate::auth::SCOPE_PLUGIN_INVOKE));
        }
        other => panic!("plugin:manage must not imply plugin:invoke: {other:?}"),
    }

    let manage_reload = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(688)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {"action":"reload"}
            }),
        ),
        Some(&manage),
    )
    .await;
    let McpOutcome::Ok(manage_reload) = manage_reload else {
        panic!("plugin:manage must pass scope governance for reload");
    };
    assert_eq!(manage_reload["result"]["isError"], true);
    assert_eq!(
        manage_reload["result"]["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
}

#[tokio::test]
async fn read_only_session_allows_plugin_inspect_but_denies_call_before_provider_dispatch() {
    let runtime = test_runtime();
    let auth = plugin_auth_with_scopes(&[
        crate::auth::SCOPE_PLUGIN_INSPECT,
        crate::auth::SCOPE_PLUGIN_INVOKE,
    ]);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "provider-instance-a",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    let session =
        start_authorized_test_session(&runtime, &auth, crate::tool_runtime::SessionMode::ReadOnly);

    let inspect = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(689)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"list",
                    "recording_session_id":session.session_id
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(inspect) = inspect else {
        panic!("read-only Session must allow Plugin inspection");
    };
    assert_eq!(inspect["result"]["isError"], false);

    let denied = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(690)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":"wc_pbind_0123456789abcdef0123456789abcdef",
                    "arguments":{"query":"must-not-run"},
                    "recording_session_id":session.session_id
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(denied) = denied else {
        panic!("Session guard denial must render a normal MCP tool result");
    };
    assert_eq!(denied["result"]["isError"], true);
    assert_eq!(
        denied["result"]["structuredContent"]["output"]["error_kind"],
        "session_guard_denied"
    );
    assert_eq!(
        denied["result"]["structuredContent"]["output"]["dispatch_certainty"],
        "not_started"
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
    let ledger = format!(
        "{:?}",
        runtime.sessions.summary(&session.session_id, Some(100))
    );
    assert!(!ledger.contains("must-not-run"));
    assert!(!ledger.contains("wc_pbind_0123456789abcdef0123456789abcdef"));
}

#[tokio::test]
async fn specialized_recording_session_authority_fails_closed_at_mcp_boundary() {
    let runtime = test_runtime();
    let owner = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INSPECT]);
    let session =
        start_authorized_test_session(&runtime, &owner, crate::tool_runtime::SessionMode::Normal);
    let mut foreign = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INSPECT]);
    foreign.username = Some("bob".to_string());
    foreign.user_id = Some("plugin-test-user-bob".to_string());
    foreign.api_key_id = Some("plugin-test-pat-bob".to_string());

    let denied = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(693)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"list",
                    "recording_session_id":session.session_id
                }
            }),
        ),
        Some(&foreign),
    )
    .await;
    let McpOutcome::Ok(denied) = denied else {
        panic!("Session authority denial must render a normal MCP tool result");
    };
    assert_eq!(denied["result"]["isError"], true);
    assert_eq!(
        denied["result"]["structuredContent"]["output"]["failure_kind"],
        "session_authority_denied"
    );
    assert_eq!(
        denied["result"]["structuredContent"]["output"]["dispatch_certainty"],
        "not_started"
    );
}

#[tokio::test]
async fn restricted_permission_denies_plugin_call_and_direct_tool_before_provider_dispatch() {
    let runtime = test_runtime().with_permission_evaluator(
        crate::tool_runtime::PermissionEvaluator::with_mode(
            crate::tool_runtime::AuthorityMode::Restricted,
        ),
    );
    let auth = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INVOKE]);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "provider-instance-a",
        vec![plugin_tool("permission_search")],
    )
    .await;

    let call = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(691)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":"wc_pbind_0123456789abcdef0123456789abcdef",
                    "arguments":{"query":"must-not-run"}
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(call) = call else {
        panic!("permission denial must render a normal MCP tool result");
    };
    assert_eq!(call["result"]["isError"], true);
    assert_eq!(
        call["result"]["structuredContent"]["output"]["failure_kind"],
        "permission_denied"
    );
    assert_eq!(
        call["result"]["structuredContent"]["output"]["dispatch_certainty"],
        "not_started"
    );

    let direct = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(692)),
            json!({
                "name":"permission_search",
                "arguments":{"query":"must-not-run"}
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(direct) = direct else {
        panic!("direct Plugin permission denial must render a normal MCP tool result");
    };
    assert_eq!(direct["result"]["isError"], true);
    assert_eq!(
        direct["result"]["structuredContent"]["output"]["failure_kind"],
        "permission_denied"
    );
    assert_eq!(
        direct["result"]["structuredContent"]["output"]["dispatch_certainty"],
        "not_started"
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

#[tokio::test]
async fn plugin_tool_list_discovers_only_visible_plugin_capable_runners() {
    let runtime = test_runtime();
    let auth = plugin_auth(true);
    register_plugin_runner(
        &runtime,
        "runner-visible",
        "runner-visible-instance",
        "repo-tools",
        "visible-provider-instance",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    register_plugin_runner_for_owner(
        &runtime,
        "runner-private",
        "runner-private-instance",
        "private-tools",
        "private-provider-instance",
        "bob",
        vec![plugin_tool("private_search")],
    )
    .await;
    register_non_plugin_runner_for_owner(
        &runtime,
        "runner-plain",
        "runner-plain-instance",
        "alice",
    )
    .await;

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(760)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {"action":"list"}
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("plugin_tool list failed: {outcome:?}");
    };
    let runners = value["result"]["structuredContent"]["runners"]
        .as_array()
        .expect("runner discovery array");
    assert_eq!(runners.len(), 1);
    assert_eq!(runners[0]["runner"], "runner-visible");
    let encoded = serde_json::to_string(&value["result"]["structuredContent"]).unwrap();
    for forbidden in [
        "runner-visible-instance",
        "visible-provider-instance",
        "runner-private",
        "runner-plain",
        "runner_instance_id",
        "provider_instance_id",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "runner list leaked {forbidden}"
        );
    }
}

#[tokio::test]
async fn plugin_tool_list_provider_and_tools_is_bounded_and_binding_free() {
    let runtime = Arc::new(test_runtime());
    let auth = plugin_auth(true);
    register_plugin_runner_with_status(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "startup-provider-instance",
        "alice",
        "ready_secondary",
        vec![],
    )
    .await;
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "dynamic-provider-instance".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 0,
    };

    let provider_task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({"action":"list","runner":"runner-a"}),
        761,
    );
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
            providers: vec![provider.clone()],
            first_class_restart_required: true,
        }),
    )
    .await;
    let McpOutcome::Ok(provider_result) = provider_task.await.unwrap() else {
        panic!("provider discovery failed");
    };
    let provider_view = &provider_result["result"]["structuredContent"]["plugins"][0];
    assert_eq!(provider_view["plugin"], "repo-tools");
    assert_eq!(provider_view["status"], "ready");
    assert_eq!(provider_view["source"], "dynamic");
    assert_eq!(provider_view["startupAdmission"], "secondary");
    assert_eq!(
        provider_view["startupAdmissionCode"],
        "first_class_catalog_too_large"
    );
    assert_eq!(provider_view["startupDirectToolCount"], 0);

    let bindings_before = runtime.plugin_gateway.binding_count();
    let tools_task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({
            "action":"list",
            "runner":"runner-a",
            "plugin":"repo-tools"
        }),
        762,
    );
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
            providers: vec![provider.clone()],
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
    let McpOutcome::Ok(tools_result) = tools_task.await.unwrap() else {
        panic!("tool discovery failed");
    };
    let discovery = &tools_result["result"]["structuredContent"];
    assert_eq!(discovery["runner"], "runner-a");
    assert_eq!(discovery["plugin"], "repo-tools");
    assert_eq!(discovery["name"], "Repo Tools");
    assert_eq!(discovery["status"], "ready");
    assert_eq!(discovery["source"], "dynamic");
    assert_eq!(discovery["startupAdmission"], "secondary");
    assert_eq!(discovery["toolCount"], 1);
    assert_eq!(discovery["tools"][0]["name"], "search_symbol");
    assert_eq!(discovery["tools"][0]["title"], "Repository Search");
    assert!(discovery.get("firstClassRestartRequired").is_none());
    assert_eq!(runtime.plugin_gateway.binding_count(), bindings_before);
    let encoded = serde_json::to_string(discovery).unwrap();
    for forbidden in [
        "inputSchema",
        "outputSchema",
        "annotations",
        "description",
        "binding",
        "dynamic-provider-instance",
        "startup-provider-instance",
        "runner-instance-a",
        "provider_instance_id",
        "runner_instance_id",
        "command",
        "argv",
        "cwd",
        "env",
        "stderr",
    ] {
        assert!(!encoded.contains(forbidden), "tool list leaked {forbidden}");
    }
}

#[tokio::test]
async fn plugin_tool_list_unknown_provider_fails_without_tools_request() {
    let runtime = Arc::new(test_runtime());
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
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "provider-instance-a".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Startup,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({"action":"list","runner":"runner-a","plugin":"missing-tools"}),
        763,
    );
    let request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        request.plugin_gateway,
        Some(PluginGatewayRequest::ProvidersList)
    ));
    complete_plugin_request(
        &runtime,
        request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![provider],
            first_class_restart_required: false,
        }),
    )
    .await;
    let McpOutcome::Ok(result) = task.await.unwrap() else {
        panic!("unknown provider should render a Plugin tool error");
    };
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["structuredContent"]["error"]["code"],
        "plugin_unavailable"
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

#[tokio::test]
async fn plugin_tool_list_provider_replacement_fails_closed_without_reresolve_or_replay() {
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
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "provider-instance-a".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({"action":"list","runner":"runner-a","plugin":"repo-tools"}),
        764,
    );
    let providers_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    complete_plugin_request(
        &runtime,
        providers_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![provider],
            first_class_restart_required: true,
        }),
    )
    .await;
    let tools_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        tools_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsList {
            ref provider_instance_id,
            ..
        }) if provider_instance_id == "provider-instance-a"
    ));
    complete_plugin_request(
        &runtime,
        tools_request,
        "runner-instance-a",
        PluginGatewayResponse::error(
            PluginDispatchState::NotStarted,
            "stale_plugin_provider",
            "provider A was replaced",
        ),
    )
    .await;
    let McpOutcome::Ok(result) = task.await.unwrap() else {
        panic!("provider replacement should render a Plugin tool error");
    };
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["structuredContent"]["error"]["code"],
        "plugin_replaced"
    );
    assert_eq!(
        result["result"]["structuredContent"]["dispatchState"],
        "not_started"
    );
    assert!(result["result"]["structuredContent"]["recovery"]
        .as_str()
        .unwrap_or_default()
        .contains("Re-list the Plugin"));
    assert!(result["result"]["structuredContent"]["recovery"]
        .as_str()
        .unwrap_or_default()
        .contains("did not retarget or replay"));
    assert_eq!(runtime.plugin_gateway.binding_count(), 0);
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

#[tokio::test]
async fn plugin_tool_list_provider_busy_is_not_started_and_not_replayed() {
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
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "provider-instance-a".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({"action":"list","runner":"runner-a","plugin":"repo-tools"}),
        765,
    );
    let providers_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    complete_plugin_request(
        &runtime,
        providers_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![provider],
            first_class_restart_required: false,
        }),
    )
    .await;
    let tools_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    complete_plugin_request(
        &runtime,
        tools_request,
        "runner-instance-a",
        PluginGatewayResponse::error(
            PluginDispatchState::NotStarted,
            "plugin_provider_busy",
            "provider is already serving one request",
        ),
    )
    .await;
    let McpOutcome::Ok(result) = task.await.unwrap() else {
        panic!("busy provider should render a Plugin tool error");
    };
    assert_eq!(
        result["result"]["structuredContent"]["error"]["code"],
        "plugin_provider_busy"
    );
    assert_eq!(
        result["result"]["structuredContent"]["dispatchState"],
        "not_started"
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

#[tokio::test]
async fn plugin_tool_list_runner_replacement_fails_closed_and_never_replays_on_replacement() {
    let runtime = Arc::new(test_runtime());
    let auth = plugin_auth(true);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "startup-provider-a",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "provider-instance-a".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let task = spawn_plugin_metadata_call(
        &runtime,
        &auth,
        json!({"action":"list","runner":"runner-a","plugin":"repo-tools"}),
        766,
    );
    let providers_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    complete_plugin_request(
        &runtime,
        providers_request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
            providers: vec![provider],
            first_class_restart_required: false,
        }),
    )
    .await;
    let tools_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        tools_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsList {
            ref provider_instance_id,
            ..
        }) if provider_instance_id == "provider-instance-a"
    ));

    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-b",
        "repo-tools",
        "startup-provider-b",
        vec![plugin_tool("replacement_tool")],
    )
    .await;
    drop(tools_request);

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("Runner replacement must resolve the exact discovery promptly")
        .unwrap();
    let McpOutcome::Ok(result) = outcome else {
        panic!("Runner replacement should render a Plugin tool error");
    };
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["structuredContent"]["error"]["code"],
        "plugin_replaced"
    );
    assert!(result["result"]["structuredContent"]["recovery"]
        .as_str()
        .unwrap_or_default()
        .contains("did not retarget or replay"));
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-b".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    assert_eq!(runtime.plugin_gateway.binding_count(), 0);
}

#[tokio::test]
async fn plugin_tool_list_argument_matrix_rejects_ambiguous_inputs_before_dispatch() {
    let runtime = test_runtime();
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

    for (rpc_id, arguments) in [
        (767, json!({"action":"list","plugin":"repo-tools"})),
        (
            768,
            json!({"action":"list","runner":"runner-a","tool":"search_symbol"}),
        ),
        (
            769,
            json!({"action":"list","runner":"runner-a","binding":"wc_pbind_00000000000000000000000000000000"}),
        ),
        (
            770,
            json!({"action":"list","runner":"runner-a","arguments":{}}),
        ),
        (
            771,
            json!({"action":"list","runner":"runner-a","plugin":"repo-tools","tool":"search_symbol"}),
        ),
    ] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(rpc_id)),
                json!({
                    "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                    "arguments": arguments
                }),
            ),
            Some(&auth),
        )
        .await;
        let McpOutcome::Ok(result) = outcome else {
            panic!("invalid list arguments should render a Plugin tool error");
        };
        assert_eq!(
            result["result"]["structuredContent"]["error"]["code"],
            "invalid_arguments"
        );
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
async fn startup_plugin_direct_inventory_requires_invoke_not_inspect_or_manage() {
    for (id, scope) in [
        (704, crate::auth::SCOPE_PLUGIN_INSPECT),
        (705, crate::auth::SCOPE_PLUGIN_MANAGE),
    ] {
        let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
        register_plugin_runner(
            &runtime,
            "runner-a",
            "runner-instance-a",
            "repo-tools",
            "provider-instance-a",
            vec![plugin_tool("invoke_only_direct")],
        )
        .await;
        let auth = plugin_auth_with_scopes(&[scope]);
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(json!(id)), json!({})),
            Some(&auth),
        )
        .await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("tools/list must succeed for {scope}");
        };
        let names = value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&crate::plugin_gateway::PLUGIN_TOOL_NAME));
        assert!(!names.contains(&"invoke_only_direct"));

        let spoof = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(id + 10)),
                json!({"name":"invoke_only_direct","arguments":{"query":"x"}}),
            ),
            Some(&auth),
        )
        .await;
        match spoof {
            McpOutcome::Forbidden { required_scope, .. } => {
                assert_eq!(required_scope, Some(crate::auth::SCOPE_PLUGIN_INVOKE));
            }
            other => panic!("direct Plugin spoof must require invoke: {other:?}"),
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
    }
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
    let auth = plugin_auth_with_scopes(&[crate::auth::SCOPE_PLUGIN_INVOKE]);
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
            assert_eq!(required_scope, Some(crate::auth::SCOPE_PLUGIN_INVOKE));
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
    let binding = describe_result["result"]["structuredContent"]["binding"]
        .as_str()
        .expect("describe binding")
        .to_string();
    assert!(binding.starts_with("wc_pbind_"));

    let call_task = spawn_binding_call(
        &runtime,
        &auth,
        binding.clone(),
        json!({"query":"PluginManager"}),
        722,
    );
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

    let replacement_task = spawn_binding_call(
        &runtime,
        &auth,
        binding.clone(),
        json!({"query":"stale"}),
        723,
    );
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
                    "binding":binding,
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

#[tokio::test]
async fn plugin_binding_a_never_retargets_across_reload_and_binding_b_still_calls_v2() {
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

    let provider_v1 = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "dynamic-provider-v1".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let provider_v2 = PluginProviderView {
        provider_instance_id: "dynamic-provider-v2".to_string(),
        ..provider_v1.clone()
    };

    let (binding_a, _) = describe_dynamic_binding(
        &runtime,
        &auth,
        "runner-a",
        "runner-instance-a",
        provider_v1.clone(),
        plugin_tool("search_symbol"),
        730,
    )
    .await;

    let reload_runtime = Arc::clone(&runtime);
    let reload_auth = auth.clone();
    let reload_task = tokio::spawn(async move {
        handle_mcp_request(
            &reload_runtime,
            rpc(
                "tools/call",
                Some(json!(731)),
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
            providers: vec![provider_v2.clone()],
            failures: vec![],
            first_class_restart_required: true,
        }),
    )
    .await;
    let McpOutcome::Ok(reload_result) = reload_task.await.unwrap() else {
        panic!("reload failed");
    };
    assert_eq!(reload_result["result"]["isError"], false);

    let (binding_b, _) = describe_dynamic_binding(
        &runtime,
        &auth,
        "runner-a",
        "runner-instance-a",
        provider_v2.clone(),
        plugin_tool("search_symbol"),
        732,
    )
    .await;
    assert_ne!(binding_a, binding_b);

    let stale_a = spawn_binding_call(&runtime, &auth, binding_a, json!({"query":"stale-a"}), 733);
    let request_a =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        request_a.plugin_gateway,
        Some(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Effective,
            ref provider_instance_id,
            ref name,
            ..
        }) if provider_instance_id == "dynamic-provider-v1" && name == "search_symbol"
    ));
    complete_plugin_request(
        &runtime,
        request_a,
        "runner-instance-a",
        PluginGatewayResponse::error(
            PluginDispatchState::NotStarted,
            "stale_plugin_provider",
            "v1 was replaced",
        ),
    )
    .await;
    let McpOutcome::Ok(stale_result) = stale_a.await.unwrap() else {
        panic!("stale binding should render a normal Plugin tool error");
    };
    assert_eq!(
        stale_result["result"]["structuredContent"]["error"]["code"],
        "plugin_replaced"
    );
    assert_eq!(
        stale_result["result"]["structuredContent"]["dispatchState"],
        "not_started"
    );

    let live_b = spawn_binding_call(&runtime, &auth, binding_b, json!({"query":"live-b"}), 734);
    let request_b =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    let Some(PluginGatewayRequest::ToolsCall {
        plane,
        provider_instance_id,
        name,
        arguments,
        ..
    }) = request_b.plugin_gateway.clone()
    else {
        panic!("binding B did not dispatch a typed call");
    };
    assert_eq!(plane, PluginPlane::Effective);
    assert_eq!(provider_instance_id, "dynamic-provider-v2");
    assert_eq!(name, "search_symbol");
    assert_eq!(arguments, json!({"query":"live-b"}));
    complete_plugin_request(
        &runtime,
        request_b,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::ToolResult {
            result: PluginToolResult {
                content: vec![PluginContent::Text {
                    text: "v2-ok".to_string(),
                }],
                structured_content: Some(json!({"version":2})),
                is_error: false,
            },
        }),
    )
    .await;
    let McpOutcome::Ok(live_result) = live_b.await.unwrap() else {
        panic!("binding B call failed");
    };
    assert_eq!(live_result["result"]["content"][0]["text"], "v2-ok");
}

#[tokio::test]
async fn plugin_binding_rechecks_scope_and_runner_owner_without_invalidating_owner_binding() {
    let runtime = Arc::new(test_runtime());
    let alice = plugin_auth_for("alice", true);
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "startup-provider-instance",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "dynamic-provider-instance".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };
    let (binding, _) = describe_dynamic_binding(
        &runtime,
        &alice,
        "runner-a",
        "runner-instance-a",
        provider,
        plugin_tool("search_symbol"),
        740,
    )
    .await;

    let no_scope = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(741)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":binding.clone(),
                    "arguments":{"query":"no-scope"}
                }
            }),
        ),
        Some(&plugin_auth_for("alice", false)),
    )
    .await;
    assert!(matches!(no_scope, McpOutcome::Forbidden { .. }));

    let bob = plugin_auth_for("bob", true);
    let bob_result = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(742)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":binding.clone(),
                    "arguments":{"query":"private"}
                }
            }),
        ),
        Some(&bob),
    )
    .await;
    let McpOutcome::Ok(bob_result) = bob_result else {
        panic!("owner isolation should render a Plugin tool error");
    };
    assert_eq!(bob_result["result"]["isError"], true);
    assert_eq!(
        bob_result["result"]["structuredContent"]["error"]["code"],
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

    let alice_call = spawn_binding_call(
        &runtime,
        &alice,
        binding,
        json!({"query":"still-alice"}),
        743,
    );
    let request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a").await;
    assert!(matches!(
        request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsCall { .. })
    ));
    complete_plugin_request(
        &runtime,
        request,
        "runner-instance-a",
        PluginGatewayResponse::success(PluginGatewayResponsePayload::ToolResult {
            result: PluginToolResult {
                content: vec![PluginContent::Text {
                    text: "alice-ok".to_string(),
                }],
                structured_content: None,
                is_error: false,
            },
        }),
    )
    .await;
    let McpOutcome::Ok(alice_result) = alice_call.await.unwrap() else {
        panic!("Alice's binding was incorrectly invalidated by Bob");
    };
    assert_eq!(alice_result["result"]["content"][0]["text"], "alice-ok");
}

#[tokio::test]
async fn plugin_binding_runner_replacement_and_schema_change_fail_closed() {
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
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "dynamic-provider-instance".to_string(),
        name: "Repo Tools".to_string(),
        plane: PluginPlane::Effective,
        status: "ready".to_string(),
        error_code: None,
        startup_direct_tool_count: 1,
    };

    let (runner_binding, _) = describe_dynamic_binding(
        &runtime,
        &auth,
        "runner-a",
        "runner-instance-a",
        provider.clone(),
        plugin_tool("search_symbol"),
        750,
    )
    .await;
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-b",
        "repo-tools",
        "replacement-startup-provider",
        vec![plugin_tool("search_symbol")],
    )
    .await;
    let replaced = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(751)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":runner_binding,
                    "arguments":{"query":"old-runner"}
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(replaced) = replaced else {
        panic!("runner replacement should render a Plugin tool error");
    };
    assert_eq!(
        replaced["result"]["structuredContent"]["error"]["code"],
        "plugin_replaced"
    );
    assert_eq!(
        replaced["result"]["structuredContent"]["dispatchState"],
        "not_started"
    );
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-b".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    let (schema_binding, _) = describe_dynamic_binding(
        &runtime,
        &auth,
        "runner-a",
        "runner-instance-b",
        provider,
        plugin_tool("search_symbol"),
        752,
    )
    .await;
    let schema_call = spawn_binding_call(
        &runtime,
        &auth,
        schema_binding.clone(),
        json!({"query":"schema-change"}),
        753,
    );
    let schema_request =
        wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-b").await;
    assert!(matches!(
        schema_request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsCall {
            ref provider_instance_id,
            ..
        }) if provider_instance_id == "dynamic-provider-instance"
    ));
    complete_plugin_request(
        &runtime,
        schema_request,
        "runner-instance-b",
        PluginGatewayResponse::error(
            PluginDispatchState::NotStarted,
            "plugin_schema_changed",
            "schema changed",
        ),
    )
    .await;
    let McpOutcome::Ok(schema_result) = schema_call.await.unwrap() else {
        panic!("schema change should render a Plugin tool error");
    };
    assert_eq!(
        schema_result["result"]["structuredContent"]["error"]["code"],
        "plugin_schema_changed"
    );
    assert_eq!(
        schema_result["result"]["structuredContent"]["dispatchState"],
        "not_started"
    );

    let second = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(754)),
            json!({
                "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
                "arguments": {
                    "action":"call",
                    "binding":schema_binding,
                    "arguments":{"query":"must-describe-again"}
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(second) = second else {
        panic!("evicted stale binding should render a normal Plugin tool error");
    };
    assert_eq!(
        second["result"]["structuredContent"]["error"]["code"],
        "describe_required"
    );
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-b".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}
