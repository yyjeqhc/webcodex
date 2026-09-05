use super::*;
use std::sync::Arc;
use webcodex_core::ssh_resource::{
    SshResourceInventoryEntry, SshResourceResponse, SshResourceSource,
};

fn ssh_auth() -> crate::auth::AuthContext {
    let mut auth = mcp_export_api_auth("ssh-resource-test-pat", "alice");
    auth.scopes.push(crate::auth::SCOPE_SSH_LOCAL.to_string());
    auth
}

async fn register_managed_runner(runtime: &ToolRuntime, instance: &str) {
    let mut capabilities = RunnerCapabilities::default();
    capabilities.managed_ssh_resources = true;
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: "runner-a".to_string(),
                runner_instance_id: instance.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: Some("SSH Resource Runner".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                host_context: None,
                capabilities,
                policy: None,
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

async fn wait_for_request(
    runtime: &ToolRuntime,
    instance: &str,
) -> crate::runner_protocol::RunnerRequest {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(request) = runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: "runner-a".to_string(),
                runner_instance_id: instance.to_string(),
            })
            .await
            .unwrap()
        {
            return request;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "SSH resource request did not dispatch"
        );
        tokio::task::yield_now().await;
    }
}

async fn complete_response(
    runtime: &ToolRuntime,
    request: crate::runner_protocol::RunnerRequest,
    instance: &str,
    response: SshResourceResponse,
) {
    runtime
        .runner_registry
        .complete(crate::runner_protocol::RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: request.client_id,
                runner_instance_id: instance.to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(serde_json::to_string(&response).unwrap()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            },
            command_execution_state: Some(
                crate::runner_protocol::ShellCommandExecutionState::Completed,
            ),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await
        .unwrap();
}

async fn call_in_task(
    runtime: Arc<ToolRuntime>,
    auth: crate::auth::AuthContext,
    arguments: Value,
    id: u64,
) -> tokio::task::JoinHandle<McpOutcome> {
    tokio::spawn(async move {
        handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(id)),
                json!({
                    "name": crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
                    "arguments": arguments
                }),
            ),
            Some(&auth),
        )
        .await
    })
}

fn tool_result(outcome: McpOutcome) -> Value {
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected normal MCP tool result: {outcome:?}");
    };
    value["result"].clone()
}

async fn list_binding(
    runtime: &Arc<ToolRuntime>,
    auth: &crate::auth::AuthContext,
    revision: u64,
) -> String {
    let task = call_in_task(
        Arc::clone(runtime),
        auth.clone(),
        json!({"action":"list","runner":"runner-a"}),
        801,
    )
    .await;
    let request = wait_for_request(runtime, "instance-a").await;
    assert_eq!(request.kind, "ssh_resource");
    assert!(!request
        .content
        .as_deref()
        .unwrap_or_default()
        .contains("target"));
    complete_response(
        runtime,
        request,
        "instance-a",
        SshResourceResponse::List {
            revision,
            resources: vec![SshResourceInventoryEntry {
                name: "spe".to_string(),
                source: SshResourceSource::Static,
                active: true,
                pending_restart: false,
            }],
        },
    )
    .await;
    let result = tool_result(task.await.unwrap());
    assert_eq!(result["isError"], false);
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("17724@w10"));
    result["structuredContent"]["binding"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn managed_ssh_list_register_keeps_target_out_of_model_result() {
    let runtime = Arc::new(test_runtime());
    let auth = ssh_auth();
    register_managed_runner(&runtime, "instance-a").await;
    let binding = list_binding(&runtime, &auth, 0).await;

    let target = "17724@w10";
    let task = call_in_task(
        Arc::clone(&runtime),
        auth,
        json!({
            "action":"register",
            "binding":binding,
            "name":"w10",
            "target":target
        }),
        802,
    )
    .await;
    let request = wait_for_request(&runtime, "instance-a").await;
    assert_eq!(request.kind, "ssh_resource");
    assert!(request.content.as_deref().unwrap().contains(target));
    assert!(request.command.is_empty());
    assert!(request.path.is_none());
    complete_response(
        &runtime,
        request,
        "instance-a",
        SshResourceResponse::Register {
            revision: 1,
            resource: "w10".to_string(),
            persisted: true,
            active: false,
            restart_required: true,
        },
    )
    .await;
    let result = tool_result(task.await.unwrap());
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains(target));
    assert_eq!(result["structuredContent"]["resource"], "w10");
    assert_eq!(result["structuredContent"]["persisted"], true);
    assert_eq!(result["structuredContent"]["active"], false);
    assert_eq!(result["structuredContent"]["restart_required"], true);
}

#[tokio::test]
async fn managed_ssh_stale_revision_invalidates_binding_without_lost_update() {
    let runtime = Arc::new(test_runtime());
    let auth = ssh_auth();
    register_managed_runner(&runtime, "instance-a").await;
    let binding = list_binding(&runtime, &auth, 7).await;

    let task = call_in_task(
        Arc::clone(&runtime),
        auth.clone(),
        json!({
            "action":"remove",
            "binding":binding,
            "name":"old"
        }),
        803,
    )
    .await;
    let request = wait_for_request(&runtime, "instance-a").await;
    complete_response(
        &runtime,
        request,
        "instance-a",
        SshResourceResponse::Error {
            code: "ssh_resource_registry_stale".to_string(),
            message: "safe".to_string(),
        },
    )
    .await;
    let result = tool_result(task.await.unwrap());
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "ssh_resource_registry_stale"
    );

    // The stale binding was discarded. A second use fails before enqueue, so
    // no lost update can be replayed against a newer registry revision.
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(804)),
            json!({
                "name": crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
                "arguments": {"action":"remove","binding":binding,"name":"old"}
            }),
        ),
        Some(&auth),
    )
    .await;
    let result = tool_result(outcome);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "ssh_resource_binding_required"
    );
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "instance-a".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn managed_ssh_binding_rejects_runner_instance_replacement() {
    let runtime = Arc::new(test_runtime());
    let auth = ssh_auth();
    register_managed_runner(&runtime, "instance-a").await;
    let binding = list_binding(&runtime, &auth, 1).await;

    register_managed_runner(&runtime, "instance-b").await;
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(805)),
            json!({
                "name": crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
                "arguments": {
                    "action":"register",
                    "binding":binding,
                    "name":"w10",
                    "target":"17724@w10"
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let result = tool_result(outcome);
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "runner_replaced"
    );
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "runner-a".to_string(),
            runner_instance_id: "instance-b".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn managed_ssh_invalid_post_dispatch_response_is_outcome_unknown_and_binding_is_retired() {
    let runtime = Arc::new(test_runtime());
    let auth = ssh_auth();
    register_managed_runner(&runtime, "instance-a").await;
    let binding = list_binding(&runtime, &auth, 3).await;

    let task = call_in_task(
        Arc::clone(&runtime),
        auth.clone(),
        json!({
            "action":"register",
            "binding":binding,
            "name":"w10",
            "target":"17724@w10"
        }),
        806,
    )
    .await;
    let request = wait_for_request(&runtime, "instance-a").await;
    complete_response(
        &runtime,
        request,
        "instance-a",
        SshResourceResponse::Register {
            revision: 4,
            resource: "different-name".to_string(),
            persisted: true,
            active: false,
            restart_required: true,
        },
    )
    .await;
    let result = tool_result(task.await.unwrap());
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "ssh_resource_outcome_unknown"
    );

    let retry = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(807)),
            json!({
                "name": crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
                "arguments": {
                    "action":"register",
                    "binding":binding,
                    "name":"w10",
                    "target":"17724@w10"
                }
            }),
        ),
        Some(&auth),
    )
    .await;
    let result = tool_result(retry);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "ssh_resource_binding_required"
    );
}
