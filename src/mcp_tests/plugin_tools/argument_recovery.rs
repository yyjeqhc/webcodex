use super::*;

#[tokio::test]
async fn plugin_argument_recovery_rejects_retargeting_before_mcp_or_api_dispatch() {
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
    let binding = format!("wc_pbind_{}", webcodex_core::compact::random_suffix::<16>());
    let arguments = json!({"action":"call", "binding":binding,
        "project":"agent:runner-a:private-project", "arguments":{"query":"private-query"}});
    let McpOutcome::BadRequest(result) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(790)),
            json!({"name":"plugin_tool", "arguments":arguments}),
        ),
        Some(&auth),
    )
    .await
    else {
        panic!("call-time Project must fail at the canonical parser");
    };
    assert_eq!(result["error"]["code"], -32602);
    let message = result["error"]["message"].as_str().unwrap();
    assert!(message.contains("same intended Project"), "{message}");
    assert!(message.contains("another Project"), "{message}");
    assert!(message.contains("unknown outcome"), "{message}");
    assert!(!message.contains("private-project"));
    assert!(!message.contains("private-query"));
    assert!(!message.contains(&binding));
    let api = spawn_generic_plugin_call(&runtime, &auth, arguments, None)
        .await
        .unwrap();
    assert!(!api.success);
    assert!(api.result.is_none());
    assert!(matches!(api.error_status,
        Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments { message: api_message })
        if api_message == message));
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-a".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    assert_eq!(runtime.plugin_gateway.binding_count(), 0);
}

#[tokio::test]
async fn plugin_argument_recovery_missing_project_describe_never_invokes_tool_or_creates_binding() {
    let runtime = Arc::new(test_runtime());
    let auth = plugin_auth(true);
    let mut tool = plugin_tool("search_symbol");
    tool.project_bound = true;
    register_plugin_runner(
        &runtime,
        "runner-a",
        "runner-instance-a",
        "repo-tools",
        "provider-instance-a",
        vec![tool.clone()],
    )
    .await;
    let provider = PluginProviderView {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: "provider-instance-a".to_string(),
        name: "Repo Tools".to_string(),
        status: "ready".to_string(),
        error_code: None,
    };
    let arguments = json!({"action":"describe", "runner":"runner-a",
        "plugin":"repo-tools", "tool":"search_symbol"});
    let mut recoveries = Vec::new();
    for mcp in [true, false] {
        let runtime_task = Arc::clone(&runtime);
        let auth_task = auth.clone();
        let arguments_task = arguments.clone();
        let task = tokio::spawn(async move {
            if mcp {
                let McpOutcome::Ok(result) = handle_mcp_request(
                    &runtime_task,
                    rpc(
                        "tools/call",
                        Some(json!(791)),
                        json!({"name":"plugin_tool", "arguments":arguments_task}),
                    ),
                    Some(&auth_task),
                )
                .await
                else {
                    panic!("describe metadata request must reach governed gateway");
                };
                assert_eq!(result["result"]["isError"], true);
                let output = &result["result"]["structuredContent"];
                assert_eq!(output["error"]["code"], "plugin_project_required");
                assert_eq!(output["dispatchState"], "not_started");
                assert!(output.get("binding").is_none());
                let recovery = output["recovery"].as_str().unwrap();
                let fallback = result["result"]["content"][0]["text"].as_str().unwrap();
                assert!(
                    fallback.contains(recovery),
                    "text-only Hosts must receive the same recovery"
                );
                assert!(fallback.contains(output["error"]["message"].as_str().unwrap()));
                assert!(fallback.len() <= 1024);
                output["recovery"].clone()
            } else {
                let result =
                    spawn_generic_plugin_call(&runtime_task, &auth_task, arguments_task, None)
                        .await
                        .unwrap();
                assert!(!result.success);
                let output = result.result.unwrap().output;
                assert_eq!(output["failure_kind"], "plugin_project_required");
                assert_eq!(output["dispatch_certainty"], "not_started");
                assert!(output.get("binding").is_none());
                output["recovery"].clone()
            }
        });
        let providers =
            wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a")
                .await;
        assert!(matches!(
            providers.plugin_gateway,
            Some(PluginGatewayRequest::ProvidersList)
        ));
        complete_plugin_request(
            &runtime,
            providers,
            "runner-instance-a",
            PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
                providers: vec![provider.clone()],
            }),
        )
        .await;
        let tools =
            wait_for_plugin_request(&runtime.runner_registry, "runner-a", "runner-instance-a")
                .await;
        assert!(matches!(
            tools.plugin_gateway,
            Some(PluginGatewayRequest::ToolsList { .. })
        ));
        complete_plugin_request(
            &runtime,
            tools,
            "runner-instance-a",
            PluginGatewayResponse::success(PluginGatewayResponsePayload::Tools {
                tools: vec![tool.clone()],
            }),
        )
        .await;
        let recovery = task.await.unwrap();
        let text = recovery
            .as_str()
            .expect("missing-Project failure must offer a recovery action");
        for phrase in [
            "action=describe",
            "same runner, plugin, and tool",
            "intended explicit project",
            "only binding and arguments",
            "uncertain effect",
        ] {
            assert!(text.contains(phrase), "missing {phrase}: {text}");
        }
        assert!(text.len() <= 512);
        assert!(!text.contains("runner-a"));
        assert!(!text.contains("provider-instance-a"));
        recoveries.push(recovery);
        assert_eq!(runtime.plugin_gateway.binding_count(), 0);
        assert!(
            runtime
                .runner_registry
                .poll(RunnerPollRequest {
                    client_id: "runner-a".to_string(),
                    runner_instance_id: "runner-instance-a".to_string(),
                })
                .await
                .unwrap()
                .is_none(),
            "guidance must not invoke or automatically retry the provider tool"
        );
    }
    assert_eq!(recoveries[0], recoveries[1]);
}
