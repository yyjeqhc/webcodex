use super::*;
use crate::runner_protocol::{
    RunnerPolicySummary, RunnerPollRequest, RunnerRegisterRequest, RunnerResultPayload,
    RunnerResultRequest,
};
use serde_json::json;
use webcodex_core::plugin::{
    PluginDispatchState, PluginGatewayRequest, PluginGatewayResponse, PluginGatewayResponsePayload,
    PluginPlane, PluginSchemaObservation, PluginTool, StartupPluginProvider,
};

fn plugin_tool() -> PluginTool {
    PluginTool {
        name: "echo".to_string(),
        title: None,
        description: Some("Echo a value".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {"value": {"type": "string"}}
        }),
        output_schema: None,
        annotations: None,
    }
}

fn startup_provider(instance_id: &str) -> StartupPluginProvider {
    StartupPluginProvider {
        provider_id: "repo-tools".to_string(),
        provider_instance_id: instance_id.to_string(),
        name: "Repo Tools".to_string(),
        status: "ready".to_string(),
        error_code: None,
        tools: vec![plugin_tool()],
    }
}

fn plugin_registration(
    client_id: &str,
    runner_instance_id: &str,
    providers: Vec<StartupPluginProvider>,
) -> RunnerRegisterRequest {
    let mut capabilities = RunnerCapabilities::default();
    capabilities.native_tool_plugins = true;
    current_runner_registration(RunnerRegisterRequest {
        client_id: client_id.to_string(),
        runner_instance_id: runner_instance_id.to_string(),
        runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        capabilities,
        host_context: None,
        policy: Some(RunnerPolicySummary {
            plugin_providers: Some(providers),
            ..Default::default()
        }),
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
    })
}

async fn register_plugin_runner(registry: &RunnerRegistry) {
    registry
        .register(plugin_registration(
            "plugin-runner",
            "runner-instance",
            vec![startup_provider("startup-provider-instance")],
        ))
        .await
        .unwrap();
}

fn startup_list(provider_instance_id: &str) -> PluginGatewayRequest {
    PluginGatewayRequest::ToolsList {
        plane: PluginPlane::Startup,
        provider_id: "repo-tools".to_string(),
        provider_instance_id: provider_instance_id.to_string(),
    }
}

#[tokio::test]
async fn plugin_registration_catalog_is_exact_immutable_and_required_by_capability() {
    let registry = RunnerRegistry::default();
    registry
        .register(plugin_registration(
            "valid-plugin-runner",
            "valid-instance",
            vec![startup_provider("provider-instance")],
        ))
        .await
        .unwrap();
    let view = registry
        .get_runner_view("valid-plugin-runner")
        .await
        .unwrap();
    assert_eq!(
        view.policy
            .as_ref()
            .and_then(|policy| policy.plugin_providers.as_ref())
            .unwrap(),
        &vec![startup_provider("provider-instance")]
    );

    let changed = registry
        .register(plugin_registration(
            "valid-plugin-runner",
            "valid-instance",
            vec![startup_provider("replacement-provider-instance")],
        ))
        .await
        .unwrap_err();
    assert!(
        changed.contains("cannot change startup Plugin catalog"),
        "{changed}"
    );

    let mut missing_catalog = plugin_registration("missing", "missing-instance", vec![]);
    missing_catalog.policy.as_mut().unwrap().plugin_providers = None;
    assert!(registry
        .register(missing_catalog)
        .await
        .unwrap_err()
        .contains("requires explicit startup Plugin catalog"));

    let mut inventory_without_capability = plugin_registration("no-cap", "no-cap-instance", vec![]);
    inventory_without_capability
        .capabilities
        .native_tool_plugins = false;
    assert!(registry
        .register(inventory_without_capability)
        .await
        .unwrap_err()
        .contains("requires native_tool_plugins capability"));
}

#[tokio::test]
async fn plugin_reload_can_target_exact_runner_with_zero_startup_plugins() {
    let registry = RunnerRegistry::default();
    registry
        .register(plugin_registration(
            "empty-plugin-runner",
            "empty-instance",
            vec![],
        ))
        .await
        .unwrap();
    let alice = auth_context(Some("alice"), false);
    let (_request_id, receiver) = registry
        .enqueue_plugin_gateway(
            "empty-plugin-runner",
            "empty-instance",
            PluginGatewayRequest::Reload,
            Some(&alice),
            "test".to_string(),
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "empty-plugin-runner".to_string(),
            runner_instance_id: "empty-instance".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        request.plugin_gateway,
        Some(PluginGatewayRequest::Reload)
    ));
    registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: "empty-plugin-runner".to_string(),
                runner_instance_id: "empty-instance".to_string(),
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
                PluginGatewayResponsePayload::Reloaded {
                    providers: vec![],
                    failures: vec![],
                    first_class_restart_required: false,
                },
            )),
            coding_agent: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        receiver.await.unwrap().payload,
        Some(PluginGatewayResponsePayload::Reloaded { .. })
    ));
}

#[tokio::test]
async fn plugin_enqueue_rechecks_owner_runner_and_startup_provider_identity() {
    let registry = RunnerRegistry::default();
    register_plugin_runner(&registry).await;
    let bob = auth_context(Some("bob"), false);
    assert!(registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "runner-instance",
            startup_list("startup-provider-instance"),
            Some(&bob),
            "bob".to_string(),
        )
        .await
        .unwrap_err()
        .contains("unavailable"));

    let alice = auth_context(Some("alice"), false);
    assert!(registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "stale-runner-instance",
            startup_list("startup-provider-instance"),
            Some(&alice),
            "alice".to_string(),
        )
        .await
        .unwrap_err()
        .contains("stale Runner"));
    assert!(registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "runner-instance",
            startup_list("stale-provider-instance"),
            Some(&alice),
            "alice".to_string(),
        )
        .await
        .unwrap_err()
        .contains("stale startup Plugin provider"));
    let inner = registry.inner.lock().await;
    assert!(inner.pending_by_id.is_empty());
    assert!(inner.plugin_gateway_waiters.is_empty());
}

#[tokio::test]
async fn plugin_dequeue_rechecks_exact_runner_and_startup_provider_before_dispatch() {
    let registry = RunnerRegistry::default();
    register_plugin_runner(&registry).await;
    let alice = auth_context(Some("alice"), false);
    let (_request_id, receiver) = registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "runner-instance",
            startup_list("startup-provider-instance"),
            Some(&alice),
            "test".to_string(),
        )
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        inner
            .runners
            .get_mut("plugin-runner")
            .unwrap()
            .runner_instance_id = "replacement-runner-instance".to_string();
    }
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "plugin-runner".to_string(),
            runner_instance_id: "replacement-runner-instance".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    let response = receiver.await.unwrap();
    assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(response.error.as_ref().unwrap().code, "stale_runner");

    let registry = RunnerRegistry::default();
    register_plugin_runner(&registry).await;
    let (_request_id, receiver) = registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "runner-instance",
            startup_list("startup-provider-instance"),
            Some(&alice),
            "test".to_string(),
        )
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        inner
            .runners
            .get_mut("plugin-runner")
            .unwrap()
            .policy
            .as_mut()
            .unwrap()
            .plugin_providers = Some(vec![startup_provider("replacement-provider-instance")]);
    }
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "plugin-runner".to_string(),
            runner_instance_id: "runner-instance".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    let response = receiver.await.unwrap();
    assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        response.error.as_ref().unwrap().code,
        "stale_plugin_provider"
    );
}

#[tokio::test]
async fn dynamic_effective_provider_is_runner_owned_but_exact_and_never_falls_back_to_startup() {
    let registry = RunnerRegistry::default();
    register_plugin_runner(&registry).await;
    let alice = auth_context(Some("alice"), false);
    let schema = PluginSchemaObservation {
        input_schema: plugin_tool().input_schema,
        output_schema: None,
        annotations: None,
    };
    let (_request_id, receiver) = registry
        .enqueue_plugin_gateway(
            "plugin-runner",
            "runner-instance",
            PluginGatewayRequest::ToolsCall {
                plane: PluginPlane::Effective,
                provider_id: "repo-tools".to_string(),
                provider_instance_id: "dynamic-provider-instance".to_string(),
                name: "echo".to_string(),
                arguments: json!({"value":"dynamic"}),
                expected_schema: schema,
            },
            Some(&alice),
            "test".to_string(),
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "plugin-runner".to_string(),
            runner_instance_id: "runner-instance".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        request.plugin_gateway,
        Some(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Effective,
            ref provider_instance_id,
            ..
        }) if provider_instance_id == "dynamic-provider-instance"
    ));
    registry
        .reconcile_disconnect("plugin-runner", "runner-instance")
        .await;
    let response = receiver.await.unwrap();
    assert_eq!(response.dispatch_state, PluginDispatchState::OutcomeUnknown);
    assert_eq!(response.error.as_ref().unwrap().code, "runner_unavailable");
}
