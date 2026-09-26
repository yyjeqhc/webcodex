use super::*;

fn oauth_mcp_service(scopes: &str) -> (tempfile::TempDir, Service, String) {
    let config = test_config_oauth2(Some("secret"));
    let (tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, scopes);
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    (tmp, service, token)
}

fn adaptive_gateway_params(tool: &str, arguments: Value) -> Value {
    json!({
        "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
        "arguments": {"tool": tool, "arguments": arguments}
    })
}

async fn oauth_mcp_service_with_plugin_runner(
    scopes: &str,
) -> (tempfile::TempDir, Service, String) {
    let config = test_config_oauth2(Some("secret"));
    let (tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, scopes);
    let runtime = Arc::new(test_runtime());
    let mut capabilities = RunnerCapabilities::default();
    capabilities.native_tool_plugins = true;
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: "oauth-plugin-runner".to_string(),
                runner_instance_id: "oauth-plugin-runner-instance".to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: Some("OAuth Plugin Runner".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                host_context: None,
                capabilities,
                policy: Some(Default::default()),
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
    let service = Service::new(build_test_router(config, db, runtime));
    (tmp, service, token)
}

fn assert_mcp_oauth_scope_rejected(
    status: StatusCode,
    body: &Value,
    challenge: Option<&str>,
    scope: Option<&str>,
) {
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
    assert_eq!(body["error"], "insufficient_scope");
    let challenge = challenge.unwrap_or("");
    assert!(
        challenge.contains("error=\"insufficient_scope\""),
        "challenge: {}",
        challenge
    );
    if let Some(scope) = scope {
        assert!(
            body["error_description"]
                .as_str()
                .unwrap_or("")
                .contains(scope),
            "body: {:?}",
            body
        );
        assert!(challenge.contains(scope), "challenge: {}", challenge);
    }
}

#[tokio::test]
async fn pat_mcp_tools_list_requires_runtime_read_without_oauth_framing() {
    let runtime = test_runtime();
    let mut auth = mcp_export_api_auth("pat-project-read-only", "alice");
    auth.scopes = vec![crate::auth::SCOPE_PROJECT_READ.to_string()];
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(json!(41)), json!({})),
        Some(&auth),
    )
    .await;

    match outcome {
        McpOutcome::Forbidden {
            body,
            required_scope,
        } => {
            assert_eq!(required_scope, Some(crate::auth::SCOPE_RUNTIME_READ));
            assert_eq!(body["status"], StatusCode::FORBIDDEN.as_u16());
            assert_ne!(body["error"], "insufficient_scope");
            assert!(body["error"]
                .as_str()
                .unwrap_or("")
                .contains(crate::auth::SCOPE_RUNTIME_READ));
        }
        other => panic!("PAT without runtime:read must fail closed, got {other:?}"),
    }
}

#[tokio::test]
async fn oauth2_mcp_tools_list_requires_runtime_read() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) =
        oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_local_gateway_catalog_and_call_require_explicit_scope() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(!body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::mcp_gateway::MCP_TOOL_NAME));

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp_gateway::MCP_TOOL_NAME,
            "arguments": {"action": "list"}
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_MCP_LOCAL),
    );

    let (_tmp, service, token) = oauth_mcp_service("runtime:read mcp:local");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::mcp_gateway::MCP_TOOL_NAME));

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp_gateway::MCP_TOOL_NAME,
            "arguments": {"action": "list"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], false);
    assert_eq!(body["result"]["structuredContent"]["servers"], json!([]));
}

#[tokio::test]
async fn oauth2_native_plugin_catalog_and_call_require_explicit_plugin_scope() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(!body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::plugin_gateway::PLUGIN_TOOL_NAME));

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
            "arguments": {"action": "list"}
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PLUGIN_INSPECT),
    );

    let (_tmp, service, token) = oauth_mcp_service("runtime:read plugin:inspect plugin:invoke");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::plugin_gateway::PLUGIN_TOOL_NAME));

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
            "arguments": {"action": "list"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], false);
    assert_eq!(body["result"]["structuredContent"]["runners"], json!([]));

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
            "arguments": {
                "action": "call",
                "binding": "wc_pbind_AAAAAAAAAAAAAAAAAAAAAA",
                "arguments": {"value": "hello"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], true);
    assert_eq!(
        body["result"]["structuredContent"]["error"]["code"], "describe_required",
        "a syntactically valid call must reach binding resolution after plugin:invoke scope"
    );

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
            "arguments": {"action": "check", "runner": "runner-a", "plugin": "repo-tools"}
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PLUGIN_MANAGE),
    );
}

#[tokio::test]
async fn oauth2_managed_ssh_resource_surface_requires_explicit_ssh_local_scope() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read project:write job:run");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(!body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME));

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
            json!({"action": "list", "runner": "runner-a"}),
        ),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_SSH_LOCAL),
    );

    let (_tmp, service, token) = oauth_mcp_service("runtime:read ssh:local");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert!(!body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME));

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
            json!({"action": "list", "runner": "runner-a"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], true);
    assert_eq!(
        body["result"]["structuredContent"]["error"]["code"],
        "ssh_resource_registry_unavailable"
    );
}

#[tokio::test]
async fn oauth2_plugin_gateway_visibility_uses_any_plugin_scope_and_provider_names_stay_hidden() {
    let tool_name = "oauth_plugin_echo";
    let (_tmp, service, token) = oauth_mcp_service_with_plugin_runner("runtime:read").await;
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let names = listed_tool_names(&body);
    assert!(!names.contains(crate::plugin_gateway::PLUGIN_TOOL_NAME));
    assert!(!names.contains(tool_name));

    for scope in [
        crate::auth::SCOPE_PLUGIN_INSPECT,
        crate::auth::SCOPE_PLUGIN_INVOKE,
        crate::auth::SCOPE_PLUGIN_MANAGE,
    ] {
        let scopes = format!("runtime:read {scope}");
        let (_tmp, service, token) = oauth_mcp_service_with_plugin_runner(&scopes).await;
        let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
        assert_eq!(status, StatusCode::OK, "scope={scope}, body={body:?}");
        let names = listed_tool_names(&body);
        assert!(
            names.contains(crate::plugin_gateway::PLUGIN_TOOL_NAME),
            "scope={scope} must make the stable Plugin gateway visible"
        );
        assert!(
            !names.contains(tool_name),
            "provider-local names must never become outer MCP tools"
        );
    }
}

#[tokio::test]
async fn oauth2_adaptive_gateway_preserves_canonical_target_scope_errors() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            "arguments": {
                "tool": "list_tools",
                "arguments": {"summary_only": true, "limit": 1}
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorized gateway dispatch: {body:?}"
    );
    assert_eq!(body["result"]["isError"], false);
    assert_eq!(
        body["result"]["structuredContent"]["output"]["returned_count"],
        1
    );

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            "arguments": {
                "tool": "read_files",
                "arguments": {"project": "demo", "items": [{"path": "README.md"}]}
            }
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_READ),
    );

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            "arguments": {
                "tool": crate::mcp_gateway::MCP_TOOL_NAME,
                "arguments": {"action": "list"}
            }
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_MCP_LOCAL),
    );

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::plugin_gateway::PLUGIN_TOOL_NAME,
            "arguments": {"action": "list"}
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PLUGIN_INSPECT),
    );

    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "adaptive tools/list: {body:?}");
    let names = listed_tool_names(&body);
    assert!(names.contains(crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
    assert!(!names.contains(crate::mcp_gateway::MCP_TOOL_NAME));

    let (_tmp, service, token) = oauth_mcp_service("runtime:read mcp:local");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorized adaptive tools/list: {body:?}"
    );
    let names = listed_tool_names(&body);
    assert!(names.contains(crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
    assert!(names.contains(crate::mcp_gateway::MCP_TOOL_NAME));

    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            "arguments": {
                "tool": crate::mcp_gateway::MCP_TOOL_NAME,
                "arguments": {"action": "list"}
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorized adaptive mcp_tool: {body:?}"
    );
    assert_eq!(body["result"]["isError"], false);
    assert_eq!(body["result"]["structuredContent"]["servers"], json!([]));
}

#[tokio::test]
async fn oauth2_mcp_computer_app_resources_require_runtime_read() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "resources/list",
        mcp_2026_ui_params(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(
        body["result"]["resources"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "resources/list",
        mcp_2026_ui_params(json!({})),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_computer_observe_snapshot_keeps_computer_read_scope() {
    let arguments = json!({
        "action": "snapshot_window",
        "client_id": "missing-runner",
        "surface_id": "surface_test"
    });
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params("computer_observe", arguments.clone()),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_COMPUTER_READ),
    );

    let (_tmp, service, token) = oauth_mcp_service("computer:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params("computer_observe", arguments),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {body:?}");
}

#[tokio::test]
async fn oauth2_mcp_server_discover_requires_runtime_read_and_advertises_both_versions() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "server/discover",
        json!({
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(
        body["result"]["supportedVersions"],
        json!([
            MCP_STATELESS_PROTOCOL_VERSION,
            MCP_CHATGPT_PROTOCOL_VERSION,
            MCP_PROTOCOL_VERSION
        ])
    );
    assert_eq!(body["result"]["resultType"], "complete");

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "server/discover",
        mcp_2026_params(json!({})),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_unknown_method_keeps_legacy_fail_closed_but_modern_returns_404() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");

    let (legacy_status, legacy_body, legacy_challenge) =
        oauth_mcp_request(&service, &token, "prompts/list", json!({})).await;
    assert_mcp_oauth_scope_rejected(
        legacy_status,
        &legacy_body,
        legacy_challenge.as_deref(),
        None,
    );

    let (modern_status, modern_body, _) =
        oauth_mcp_request(&service, &token, "prompts/list", mcp_2026_params(json!({}))).await;
    assert_eq!(
        modern_status,
        StatusCode::NOT_FOUND,
        "body: {modern_body:?}"
    );
    assert_eq!(modern_body["error"]["code"], -32601);
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_project_read_for_read_files() {
    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "read_files", "arguments": {"project": "demo", "items": [{"path": "README.md"}]}}),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "read_files", "arguments": {"project": "demo", "items": [{"path": "README.md"}]}}),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_project_write_for_edit_tools() {
    // Edit tools require the project:write scope. Select the explicit full
    // canonical Adaptive Runtime so the scope gate decides this call.
    let (_tmp, service, token) = oauth_mcp_service("project:write");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            "write_project_file",
            json!({
                "project": "demo",
                "path": "README.md",
                "content": "new"
            }),
        ),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            "write_project_file",
            json!({
                "project": "demo",
                "path": "README.md",
                "content": "new"
            }),
        ),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_WRITE),
    );
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_job_run_for_run_shell() {
    let (_tmp, service, token) = oauth_mcp_service("job:run");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "run_shell", "arguments": {"project": "demo", "command": "echo hi"}}),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "run_shell", "arguments": {"project": "demo", "command": "echo hi"}}),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_JOB_RUN),
    );
}

#[tokio::test]
async fn oauth2_mcp_detached_process_requires_job_run_and_job_detach() {
    for (scopes, missing) in [
        ("job:run", crate::auth::SCOPE_JOB_DETACH),
        ("job:detach", crate::auth::SCOPE_JOB_RUN),
    ] {
        let (_tmp, service, token) = oauth_mcp_service(scopes);
        let (status, body, challenge) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            adaptive_runtime_gateway_params(
                "run_detached_process",
                json!({
                    "project": "demo",
                    "idempotency_key": "oauth-detached-scope",
                    "executable": "argv-helper",
                    "args": []
                }),
            ),
        )
        .await;
        assert_mcp_oauth_scope_rejected(status, &body, challenge.as_deref(), Some(missing));
    }

    let (_tmp, service, token) = oauth_mcp_service("job:run job:detach");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_runtime_gateway_params(
            "run_detached_process",
            json!({
                "project": "demo",
                "idempotency_key": "oauth-detached-both",
                "executable": "argv-helper",
                "args": []
            }),
        ),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
}

#[tokio::test]
async fn oauth2_mcp_unknown_tool_fails_closed() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "no_such_tool", "arguments": {}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body:?}");
    assert_eq!(body["error"]["code"], -32602);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("call_runtime_tool"));
    assert!(challenge.is_none());
}

fn listed_tool_names(body: &Value) -> std::collections::HashSet<String> {
    body["result"]["tools"]
        .as_array()
        .expect("tools/list result")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn oauth2_memory_tools_require_canonical_project_and_memory_scopes() {
    for extra_scopes in [
        "project:read",
        "memory:read",
        "project:read memory:read",
        "project:write memory:manage",
        "project:write",
        "memory:manage",
        "project:read memory:read project:write memory:manage",
        "admin project:read memory:read project:write memory:manage",
    ] {
        let scopes = format!("runtime:read {extra_scopes}");
        let (_tmp, service, token) = oauth_mcp_service(&scopes);
        let (status, body, _) =
            oauth_mcp_request(&service, &token, "tools/list", mcp_2026_params(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{scopes}: {body:?}");
        let names = listed_tool_names(&body);
        let actual = [
            "memory_search",
            "memory_read",
            "memory_set",
            "memory_delete",
            "memory_scope_list",
            "memory_scope_purge",
        ]
        .into_iter()
        .filter(|name| names.contains(*name))
        .collect::<Vec<_>>();
        assert!(actual.is_empty(), "{scopes}: {actual:?}");
    }

    for (scopes, tool, arguments, missing_scope) in [
        (
            "runtime:read project:read",
            "memory_search",
            json!({"project": "demo"}),
            crate::auth::SCOPE_MEMORY_READ,
        ),
        (
            "runtime:read memory:read",
            "memory_search",
            json!({"project": "demo"}),
            crate::auth::SCOPE_PROJECT_READ,
        ),
        (
            "runtime:read project:write",
            "memory_set",
            json!({"project":"demo","memory_key":"policy","summary":"summary"}),
            crate::auth::SCOPE_MEMORY_MANAGE,
        ),
        (
            "runtime:read memory:manage",
            "memory_set",
            json!({"project":"demo","memory_key":"policy","summary":"summary"}),
            crate::auth::SCOPE_PROJECT_WRITE,
        ),
    ] {
        let (_tmp, service, token) = oauth_mcp_service(scopes);
        for params in [
            json!({"name": tool, "arguments": arguments}),
            adaptive_gateway_params(tool, arguments),
        ] {
            let (status, body, challenge) =
                oauth_mcp_request(&service, &token, "tools/call", mcp_2026_params(params)).await;
            assert_mcp_oauth_scope_rejected(
                status,
                &body,
                challenge.as_deref(),
                Some(missing_scope),
            );
        }
    }
}

#[tokio::test]
async fn oauth2_tools_list_keeps_computer_tools_long_tail_across_outer_scopes() {
    let baseline = "runtime:read project:read project:write job:run computer:read computer:control";
    for extra_scopes in [
        "",
        "computer:launch",
        "computer:display_read",
        "computer:display_read computer:pointer_control",
        "computer:clipboard_read",
        "computer:clipboard_write",
    ] {
        let scopes = format!("{baseline} {extra_scopes}");
        let (_tmp, service, token) = oauth_mcp_service(scopes.trim());
        let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{scopes}: {body:?}");
        let names = listed_tool_names(&body);
        assert!(names.contains(crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
        for long_tail in [
            "computer_observe",
            "computer_control",
            "computer_save_snapshot",
        ] {
            assert!(
                !names.contains(long_tail),
                "OAuth scopes must not promote long-tail {long_tail} into direct tools/list: {scopes}"
            );
        }
        for retired in [
            "computer_launch_application",
            "computer_list_displays",
            "computer_snapshot_display",
            "computer_pointer_move",
            "computer_pointer_click",
            "computer_read_clipboard",
            "computer_write_clipboard",
        ] {
            assert!(
                !names.contains(retired),
                "tools/list leaked retired {retired}: {scopes}"
            );
        }
    }
}

#[tokio::test]
async fn oauth2_coding_agent_tools_require_independent_scope_in_catalog_and_direct_call() {
    let insufficient = "runtime:read project:read project:write job:run mcp:local";
    let (_tmp, service, token) = oauth_mcp_service(insufficient);
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let names = listed_tool_names(&body);
    for name in [
        "coding_agent_start",
        "coding_agent_observe",
        "coding_agent_cancel",
    ] {
        assert!(!names.contains(name), "insufficient scopes leaked {name}");
    }

    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            "coding_agent_cancel",
            json!({"run_id": "wc_agent_run_scopeprobe0001"}),
        ),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_CODING_AGENT_RUN),
    );

    let coding_agent_without_write = "runtime:read project:read coding_agent:run mcp:local";
    let (_tmp, service, token) = oauth_mcp_service(coding_agent_without_write);
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let names = listed_tool_names(&body);
    for name in [
        "coding_agent_start",
        "coding_agent_observe",
        "coding_agent_cancel",
    ] {
        assert!(
            !names.contains(name),
            "long-tail {name} must stay behind call_runtime_tool"
        );
    }
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            "coding_agent_start",
            json!({
                "project": "agent:missing:demo",
                "provider_id": "codex",
                "idempotency_key": "scope-probe",
                "instruction": "inspect"
            }),
        ),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_WRITE),
    );

    let allowed = format!("{insufficient} coding_agent:run");
    let (_tmp, service, token) = oauth_mcp_service(&allowed);
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let names = listed_tool_names(&body);
    for name in [
        "coding_agent_start",
        "coding_agent_observe",
        "coding_agent_cancel",
    ] {
        assert!(
            !names.contains(name),
            "long-tail {name} must stay behind call_runtime_tool"
        );
    }
}

#[tokio::test]
async fn oauth2_pointer_tool_call_still_requires_display_scope_even_if_invoked_directly() {
    let scopes = "runtime:read computer:read computer:control computer:pointer_control";
    let (_tmp, service, token) = oauth_mcp_service(scopes);
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        adaptive_gateway_params(
            "computer_control",
            json!({
                "action": "pointer_move",
                "client_id": "missing-runner",
                "display_id": "display_AAAAAAAAAAAAAAAA",
                "snapshot_generation": 1,
                "x": 0,
                "y": 0
            }),
        ),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_COMPUTER_DISPLAY_READ),
    );
}

#[tokio::test]
async fn api_token_mcp_behavior_unchanged() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 43,
            "method": "tools/call",
            "params": {"name": "no_such_tool", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("no_such_tool"));
}
