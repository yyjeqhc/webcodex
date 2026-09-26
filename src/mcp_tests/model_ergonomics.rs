use super::*;

// Durable model-ergonomics and MCP tool-surface measurement integration tests.
// Keep these separate from the general HTTP transport lifecycle coverage.

// Explicit compact-schema=false keeps the full outputSchema projection. Keep the
// env serialized against other compact-schema tests for the whole HTTP request.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_tools_list_explicit_full_projection_audits_effective_policy() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_MCP_COMPACT_SCHEMAS", "false");
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "tools-list-audit", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 2);
    assert!(body["result"]["tools"].is_array());
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        if tool["name"].as_str() == Some(crate::mcp_gateway::MCP_TOOL_NAME) {
            assert!(
                tool.get("outputSchema").is_none(),
                "provider-defined MCP gateway must not claim a fixed structuredContent schema"
            );
        } else {
            assert!(
                tool["outputSchema"].is_object(),
                "explicit full tools/list must include outputSchema for {}",
                tool["name"]
            );
        }
    }
    let event = db.list_action_events("tools-list-audit", 10).unwrap();
    assert_eq!(event.len(), 1);
    assert_eq!(event[0].endpoint, "/mcp");
    assert_eq!(event[0].action_name, "toolsList");
    assert_eq!(event[0].operation.as_deref(), Some("mcp_tools_list"));
    assert_eq!(event[0].status, "success");
    let summary: Value = serde_json::from_str(&event[0].summary_json).unwrap();
    let surface = &summary["tool_surface"];
    assert_eq!(summary["transport"], "mcp");
    assert_eq!(surface["schema_version"], 1);
    assert_eq!(surface["protocol_era"], "legacy");
    assert!(surface.get("runtime_exposure").is_none());
    assert_eq!(surface["compact_schemas"], false);
    assert_eq!(surface["tool_count"].as_u64().unwrap(), tools.len() as u64);
    assert_eq!(
        surface["serialized_tools_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body["result"]["tools"]).unwrap().len() as u64
    );
    assert_eq!(
        surface["serialized_result_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body["result"]).unwrap().len() as u64
    );
    assert_eq!(
        surface["gateway_tool_included"],
        tools
            .iter()
            .any(|tool| tool["name"] == crate::mcp_gateway::MCP_TOOL_NAME)
    );
    let durable = serde_json::to_string(&summary).unwrap();
    for forbidden in ["\"tools\"", "inputSchema", "outputSchema", "description"] {
        assert!(
            !durable.contains(forbidden),
            "tools/list audit leaked schema content: {durable}"
        );
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_adaptive_tools_list_unset_defaults_to_compact_and_reports_effective_policy() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.remove("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "adaptive-tools-list-audit", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2030,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    assert!(tools.iter().all(|tool| tool.get("outputSchema").is_none()));
    assert!(tools
        .iter()
        .any(|tool| { tool["name"] == crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME }));

    let events = db
        .list_action_events("adaptive-tools-list-audit", 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    let surface = &summary["tool_surface"];
    assert!(surface.get("runtime_exposure").is_none());
    assert_eq!(surface["compact_schemas"], true);
    assert_eq!(surface["tool_count"].as_u64().unwrap(), tools.len() as u64);
    assert_eq!(
        surface["serialized_tools_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body["result"]["tools"]).unwrap().len() as u64
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_tools_list_stateless_audit_measures_final_compact_result_and_skips_notifications()
{
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_MCP_COMPACT_SCHEMAS", "1");
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header("x-action-session-id", "tools-list-stateless", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2026,
            "method": "tools/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["result"]["resultType"], "complete");
    assert!(body["result"].get("ttlMs").is_some());
    assert!(body["result"].get("_meta").is_some());
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().all(|tool| tool.get("outputSchema").is_none()));

    let events = db.list_action_events("tools-list-stateless", 10).unwrap();
    assert_eq!(events.len(), 1);
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    let surface = &summary["tool_surface"];
    assert_eq!(surface["protocol_era"], "stateless_2026");
    assert!(surface.get("runtime_exposure").is_none());
    assert_eq!(surface["compact_schemas"], true);
    assert_eq!(surface["tool_count"].as_u64().unwrap(), tools.len() as u64);
    assert_eq!(
        surface["serialized_tools_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body["result"]["tools"]).unwrap().len() as u64
    );
    assert_eq!(
        surface["serialized_result_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body["result"]).unwrap().len() as u64
    );

    let response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header("x-action-session-id", "tools-list-notification", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::ACCEPTED);
    assert!(db
        .list_action_events("tools-list-notification", 10)
        .unwrap()
        .is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_direct_gateway_fallback_is_queryable_without_wrong_route_telemetry() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));

    let mut fallback = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(
            MCP_NAME_HEADER,
            crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            true,
        )
        .add_header("x-action-session-id", "direct-gateway-fallback", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 301,
            "method": "tools/call",
            "params": mcp_2026_params(json!({
                "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                "arguments": {"tool": "runtime_status", "arguments": {"summary_only": true}}
            }))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&fallback), StatusCode::OK);
    let fallback_body: Value = fallback.take_json().await.unwrap();
    assert_eq!(
        fallback_body["result"]["structuredContent"]["success"],
        true
    );
    assert!(!fallback_body.to_string().contains("wrong_invocation_route"));

    let events = db
        .list_action_events("direct-gateway-fallback", 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    let telemetry = events
        .iter()
        .filter_map(|event| serde_json::from_str::<Value>(&event.summary_json).ok())
        .filter_map(|summary| summary.get("model_ergonomics").cloned())
        .collect::<Vec<_>>();
    assert_eq!(telemetry.len(), 1);
    let fallback = &telemetry[0];
    assert_eq!(fallback["tool_name"], "runtime_status");
    assert_eq!(fallback["success"], true);
    assert!(fallback["error_kind"].is_null());
    assert_ne!(fallback["error_kind"], "wrong_invocation_route");
    assert!(fallback["serialized_result_bytes"].as_u64().is_some());
}

#[tokio::test]
async fn http_mcp_work_on_project_preferences_persist_without_private_request_values() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let private_instruction = "PRIVATE_MCP_INSTRUCTION_SENTINEL";
    let private_project = "PRIVATE_MCP_PROJECT_SENTINEL";
    let private_client = "PRIVATE_MCP_CLIENT_SENTINEL";
    let private_path = "/PRIVATE_MCP_PATH_SENTINEL";
    let private_session = "wc_sess_PRIVATE_MCP_SESSION_SENTINEL";
    let private_base_ref = "PRIVATE_MCP_BASE_REF_SENTINEL";

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            "x-action-session-id",
            "mcp-work-on-project-ergonomics",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 302,
            "method": "tools/call",
            "params": {
                "name": "work_on_project",
                "arguments": {
                    "project": private_project,
                    "client_id": private_client,
                    "path": private_path,
                    "mode": "worktree",
                    "base_ref": private_base_ref,
                    "instruction": private_instruction,
                    "session_id": private_session,
                    "guidance_profile": "host_code_mode",
                    "include_extension_catalog": false
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::BAD_REQUEST);
    let _body: Value = response.take_json().await.unwrap();

    let events = db
        .list_action_events("mcp-work-on-project-ergonomics", 10)
        .unwrap();
    assert_eq!(
        events.len(),
        1,
        "one outer tool call must create one ActionAudit row"
    );
    assert_eq!(events[0].operation.as_deref(), Some("work_on_project"));
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    let telemetry = &summary["model_ergonomics"];
    assert_eq!(telemetry["schema_version"], 10);
    let facts = &telemetry["work_on_project"];
    assert_eq!(facts["resume_requested"], true);
    assert_eq!(facts["source"], "invalid");
    assert_eq!(facts["mode"], "worktree");
    assert_eq!(facts["mode_explicit"], true);
    assert_eq!(facts["base_ref_present"], true);
    assert_eq!(facts["guidance_profile"], "host_code_mode");
    assert_eq!(facts["guidance_profile_explicit"], true);
    assert_eq!(facts["include_extension_catalog"], false);
    assert_eq!(facts["include_extension_catalog_explicit"], true);
    let persisted = serde_json::to_string(&summary).unwrap();
    for forbidden in [
        private_instruction,
        private_project,
        private_client,
        private_path,
        private_session,
        private_base_ref,
    ] {
        assert!(
            !persisted.contains(forbidden),
            "persisted MCP model ergonomics leaked {forbidden}: {persisted}"
        );
    }
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_mcp_code_mode_persists_only_bounded_composition_telemetry() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runner_registry = Arc::new(crate::runner_http::RunnerRegistry::default());
    runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "code-mode-audit".to_string(),
                runner_instance_id: "inst-code-mode-audit".to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities::default(),
                policy: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runner_registry,
        "code-mode-audit",
        "inst-code-mode-audit",
        vec![RunnerProjectSummary {
            id: "demo".to_string(),
            name: Some("Code Mode audit".to_string()),
            path: "/tmp/code-mode-audit".to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
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
    let runtime = Arc::new(ToolRuntime::new(
        runner_registry,
        Arc::new(crate::tool_runtime::RuntimeInfo::default()),
    ));
    let exact_project = "agent:code-mode-audit:demo";
    let auth = crate::auth::AuthContext {
        role: Some("admin".to_string()),
        scopes: vec![crate::auth::SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap)
    };
    let fingerprint = crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth))
        .expect("bootstrap test authority");
    let session = runtime
        .sessions
        .start_session_with_options(
            crate::tool_runtime::SessionCreateOptions::new(
                Some(exact_project.to_string()),
                Some("code mode ActionAudit privacy".to_string()),
                crate::tool_runtime::SessionMode::ReadOnly,
                crate::tool_runtime::SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(fingerprint)),
        )
        .unwrap();
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let private_source =
        "const PRIVATE_SOURCE_SENTINEL = 'PRIVATE_OUTPUT_SENTINEL'; text(PRIVATE_SOURCE_SENTINEL);";

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "code-mode-composition-audit", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 401,
            "method": "tools/call",
            "params": {
                "name": "code_mode_exec",
                "arguments": {
                    "project": exact_project,
                    "session_id": session.session_id,
                    "source": private_source
                }
            }
        }))
        .send(&service)
        .await;
    let status = effective_status(&response);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["structuredContent"]["success"], true,
        "{body}"
    );

    let events = db
        .list_action_events("code-mode-composition-audit", 10)
        .unwrap();
    assert_eq!(events.len(), 1, "one outer call must create one audit row");
    assert_eq!(events[0].operation.as_deref(), Some("code_mode_exec"));
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    let composition = &summary["code_mode_composition"];
    assert_eq!(composition["nested_calls"], 0);
    assert_eq!(composition["nested_successes"], 0);
    assert_eq!(composition["nested_failures"], 0);
    assert_eq!(composition["consequential_calls"], 0);
    assert_eq!(composition["known_results"], 0);
    assert_eq!(composition["job_handoffs"], 0);
    assert_eq!(composition["outcome_unknown"], 0);
    assert_eq!(composition["max_in_flight"], 0);
    assert_eq!(composition["nested_tool_counts"], json!({}));
    assert!(composition["duration_ms"].is_u64());
    assert!(composition["slot_wait_ms"].is_u64());
    assert_eq!(composition["input_bytes"], private_source.len());
    assert!(composition["returned_bytes"].is_u64());
    assert!(composition["nested_raw_result_bytes_total"].is_u64());
    let mut keys = composition
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "consequential_calls",
            "duration_ms",
            "input_bytes",
            "job_handoffs",
            "known_results",
            "max_in_flight",
            "nested_calls",
            "nested_failures",
            "nested_raw_result_bytes_total",
            "nested_successes",
            "nested_tool_counts",
            "outcome_unknown",
            "returned_bytes",
            "slot_wait_ms",
        ]
    );
    let persisted = serde_json::to_string(&summary).unwrap();
    for forbidden in [
        "PRIVATE_SOURCE_SENTINEL",
        "PRIVATE_OUTPUT_SENTINEL",
        private_source,
        "source",
        "content",
        "arguments",
    ] {
        assert!(
            !persisted.contains(forbidden),
            "durable Code Mode audit leaked private field/text {forbidden}: {persisted}"
        );
    }
}

#[tokio::test]
async fn http_mcp_tools_list_audit_sink_failure_is_non_blocking() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    db.conn_for_tests()
        .execute("DROP TABLE action_events", [])
        .unwrap();

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 77,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    assert!(body["result"]["tools"].is_array());
}
