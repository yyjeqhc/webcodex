use super::*;
use std::sync::Arc;

const APP_TOOLS: [&str; 8] = [
    "bind_agent_continuation",
    "recover_agent_continuation_endpoint",
    "get_agent_continuation_state",
    "acquire_agent_continuation_wake",
    "prepare_agent_continuation_wake",
    "finish_agent_continuation_wake",
    "unbind_agent_continuation",
    "get_agent_wait_state",
];

fn tool<'a>(payload: &'a Value, name: &str) -> Option<&'a Value> {
    payload["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}

fn mcp_2026_window_params(params: Value, raw_openai_session: &str) -> Value {
    let mut params = mcp_2026_params(params);
    params["_meta"]["openai/session"] = Value::String(raw_openai_session.to_string());
    params
}

fn endpoint_client_window_key(db: &crate::db::Database, endpoint_id: &str) -> Option<String> {
    db.conn_for_tests()
        .query_row(
            "SELECT mcp_app_client_window_key FROM wc_agent_endpoints WHERE endpoint_id = ?1",
            [endpoint_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn schema_type_matches(value: &Value, schema: &Value) -> bool {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("number") => value.is_number(),
        Some("null") => value.is_null(),
        Some(_) | None => true,
    }
}

fn host_project_through_output_schema(value: &Value, schema: &Value) -> Value {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        if let Some(branch) = variants
            .iter()
            .find(|branch| schema_type_matches(value, branch))
        {
            return host_project_through_output_schema(value, branch);
        }
    }
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        let Some(object) = value.as_object() else {
            return value.clone();
        };
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return value.clone();
        };
        let mut projected = serde_json::Map::new();
        for (field, child_schema) in properties {
            if let Some(child) = object.get(field) {
                projected.insert(
                    field.clone(),
                    host_project_through_output_schema(child, child_schema),
                );
            }
        }
        return Value::Object(projected);
    }
    if schema.get("type").and_then(Value::as_str) == Some("array") {
        if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
            return Value::Array(
                values
                    .iter()
                    .map(|child| host_project_through_output_schema(child, items))
                    .collect(),
            );
        }
    }
    value.clone()
}

fn continuation_auth(username: &str) -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    auth.user_id = Some(format!("user-{username}"));
    auth.username = Some(username.to_string());
    auth.api_key_id = Some(format!("key-{username}"));
    auth.role = Some("user".to_string());
    auth.scopes = vec![
        crate::auth::SCOPE_RUNTIME_READ.to_string(),
        crate::auth::SCOPE_COMMUNICATION_READ.to_string(),
        crate::auth::SCOPE_COMMUNICATION_MANAGE.to_string(),
    ];
    auth.token_kind = Some("user".to_string());
    auth
}

fn continuation_runtime() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db =
        Arc::new(crate::db::Database::open(&temp.path().join("agent-continuation.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    (temp, db, runtime)
}

async fn handle_with_app_policy(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&crate::auth::AuthContext>,
    enabled: bool,
) -> McpOutcome {
    let protocol_era = super::super::inferred_protocol_era(&request);
    let window = crate::client_window::stateless_mcp_window(&request.params);
    super::super::handle_mcp_request_with_lifecycle(
        runtime,
        request,
        auth,
        protocol_era,
        super::super::HostFileImportTrust::Untrusted,
        window.identity.as_ref(),
        None,
        None,
        crate::model_surface::effective_mcp_compact_schemas(
            crate::config::mcp_compact_schemas_override(),
        ),
        enabled,
        None,
    )
    .await
}

fn create_agent(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    handle: &str,
    display_name: &str,
    key: &str,
) -> String {
    let result = runtime.create_agent_identity(
        Some(auth),
        handle.to_string(),
        display_name.to_string(),
        Some("PRIVATE Agent description".to_string()),
        vec!["PRIVATE-specialty-label".to_string()],
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn attach(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    agent_id: &str,
    key: &str,
) -> (String, i64) {
    let result = runtime.attach_agent_endpoint(
        Some(auth),
        agent_id.to_string(),
        "ChatGPT".to_string(),
        Some(format!("attachment-{key}")),
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    (
        result.output["endpoint"]["endpoint_id"]
            .as_str()
            .unwrap()
            .to_string(),
        result.output["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap(),
    )
}

fn create_conversation(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    a: &str,
    b: &str,
    key: &str,
) -> String {
    let result = runtime.create_conversation(
        Some(auth),
        Some("PRIVATE conversation title".to_string()),
        vec![a.to_string(), b.to_string()],
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["conversation"]["conversation"]["conversation_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[allow(clippy::too_many_arguments)]
fn post_message(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    conversation_id: &str,
    sender: &str,
    sender_endpoint: &str,
    sender_generation: i64,
    receiver: &str,
    body: &str,
    key: &str,
) {
    let result = runtime.post_conversation_message(
        Some(auth),
        conversation_id.to_string(),
        body.to_string(),
        Some(sender.to_string()),
        Some(sender_endpoint.to_string()),
        Some(sender_generation),
        Some(vec![receiver.to_string()]),
        None,
        Some(key.to_string()),
        None,
        None,
    );
    assert!(result.success, "{:?}", result.output);
}

#[tokio::test]
async fn agent_continuation_app_surface_is_sparse_app_only_without_rendering_bindings() {
    assert_eq!(
        MCP_AGENT_CONTINUATION_UI_RESOURCE_URI,
        "ui://webcodex/agent-continuation/v18"
    );
    let (_temp, _db, adaptive) = continuation_runtime();
    let auth = continuation_auth("continuation-surface");
    let ui = handle_with_app_policy(
        &adaptive,
        rpc(
            "tools/list",
            Some(json!(5101)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(ui) = ui else {
        panic!("expected UI tools/list")
    };
    assert!(tool(&ui["result"], "present_agent_continuation").is_none());
    assert!(tool(&ui["result"], "wait_for_agent_events").is_none());
    let bound_tools: Vec<_> = ui["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|tool| {
            tool.pointer("/_meta/ui/resourceUri")
                .and_then(Value::as_str)
                == Some(MCP_AGENT_CONTINUATION_UI_RESOURCE_URI)
        })
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(bound_tools.is_empty());
    for name in APP_TOOLS {
        let descriptor = tool(&ui["result"], name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            descriptor.pointer("/_meta/ui/visibility"),
            Some(&json!(["app"]))
        );
        assert!(
            descriptor.pointer("/_meta/ui/resourceUri").is_none(),
            "{name} is a bridge helper, not a rendering entrypoint"
        );
        assert_eq!(
            descriptor.pointer("/inputSchema/properties/app_call_id/pattern"),
            Some(&json!("^wc_app_call_[0-9a-f]{16}_[1-9][0-9]{0,5}$")),
            "{name} must advertise only the bounded adapter diagnostic id"
        );
        assert!(!descriptor["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "app_call_id"));
    }
    assert_eq!(
        tool(&ui["result"], "bind_agent_continuation").unwrap()["inputSchema"]["required"],
        json!([
            "agent_id",
            "endpoint_id",
            "expected_controller_generation",
            "binding_id"
        ])
    );
    assert_eq!(
        tool(&ui["result"], "prepare_agent_continuation_wake").unwrap()["inputSchema"]["required"],
        json!([
            "agent_id",
            "endpoint_id",
            "expected_controller_generation",
            "binding_id",
            "wake_id",
            "attempt_id"
        ])
    );

    let plain = handle_with_app_policy(
        &adaptive,
        rpc("tools/list", Some(json!(5102)), mcp_2026_params(json!({}))),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(plain) = plain else {
        panic!("expected plain tools/list")
    };
    assert!(tool(&plain["result"], "present_agent_continuation").is_none());
    assert!(tool(&plain["result"], "wait_for_agent_events").is_none());
    for name in APP_TOOLS {
        assert!(tool(&plain["result"], name).is_none());
    }

    let disabled = handle_with_app_policy(
        &adaptive,
        rpc(
            "tools/list",
            Some(json!(5103)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        false,
    )
    .await;
    let McpOutcome::Ok(disabled) = disabled else {
        panic!("expected disabled tools/list")
    };
    assert!(tool(&disabled["result"], "present_agent_continuation").is_none());
    assert!(tool(&disabled["result"], "wait_for_agent_events").is_none());
    for name in APP_TOOLS {
        assert!(tool(&disabled["result"], name).is_none());
    }

    let legacy = handle_with_app_policy(
        &adaptive,
        rpc("tools/list", Some(json!(5104)), json!({})),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(legacy) = legacy else {
        panic!("expected legacy tools/list")
    };
    for name in APP_TOOLS {
        assert!(tool(&legacy["result"], name).is_none());
    }

    for name in APP_TOOLS {
        assert!(
            !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true),
            "{name} must not be an Adaptive generic gateway target"
        );
    }

    let resources = handle_with_app_policy(
        &adaptive,
        rpc(
            "resources/list",
            Some(json!(5106)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(resources) = resources else {
        panic!("expected resources/list")
    };
    let resource = resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["uri"] == MCP_AGENT_CONTINUATION_UI_RESOURCE_URI)
        .expect("Agent Continuation resource");
    assert_eq!(resource["mimeType"], MCP_UI_RESOURCE_MIME_TYPE);
    for uri in [MCP_AGENT_CONTINUATION_UI_RESOURCE_URI] {
        let read = handle_with_app_policy(
            &adaptive,
            rpc(
                "resources/read",
                Some(json!(5107)),
                mcp_2026_ui_params(json!({"uri": uri})),
            ),
            Some(&auth),
            true,
        )
        .await;
        let McpOutcome::Ok(read) = read else {
            panic!("expected resource read")
        };
        assert_eq!(read["result"]["contents"][0]["uri"], uri);
        assert_eq!(
            read["result"]["contents"][0]["text"],
            MCP_AGENT_CONTINUATION_APP_HTML
        );
    }
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("^wc_dagent_[A-Za-z0-9_-]{16}$"),
        "App must validate the canonical durable Agent id prefix"
    );
    assert!(
        !MCP_AGENT_CONTINUATION_APP_HTML.contains("^wc_agent_[0-9a-f]{32}$"),
        "App must not accept the obsolete/nonexistent wc_agent_ prefix"
    );
    for required in [
        "ui/initialize",
        "ui/notifications/tool-input",
        "bind_agent_continuation",
        "recover_agent_continuation_endpoint",
        "get_agent_continuation_state",
        "acquire_agent_continuation_wake",
        "prepare_agent_continuation_wake",
        "ui/message",
        "finish_agent_continuation_wake",
        "visibilitychange",
        "pagehide",
        "beforeunload",
        "ui/resource-teardown",
        "delivery_unknown",
    ] {
        assert!(
            MCP_AGENT_CONTINUATION_APP_HTML.contains(required),
            "missing {required}"
        );
    }
    for forbidden in [
        "localStorage",
        "console.log",
        "claim_fence",
        "Message body:",
    ] {
        assert!(
            !MCP_AGENT_CONTINUATION_APP_HTML.contains(forbidden),
            "App source contains forbidden marker {forbidden}"
        );
    }
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("host_binding_missing_in_process"),
        "App recovery must key only on the explicit process-local binding-loss contract"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("MAX_RECOVERY_REBINDS = 1"),
        "App restart recovery must remain bounded"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("MAX_ENDPOINT_RECOVERY_ATTEMPTS = 2"),
        "expired-endpoint replacement retries must remain bounded and replay-safe"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("MAX_ENDPOINT_SUCCESSOR_HOPS = 8"),
        "sequential expired-successor recovery must remain explicitly bounded"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("function markCurrentEndpointHealthy()"),
        "a healthy exact controller must reopen future expired-endpoint recovery eligibility"
    );
    let normalized_app_html = MCP_AGENT_CONTINUATION_APP_HTML.replace("\r\n", "\n");
    assert!(
        normalized_app_html.contains(
            "bindingId = viewBindingId;\n    markCurrentEndpointHealthy();\n    renderWithWait(projection);"
        ),
        "a successful exact bind must end the current recovery-probe episode"
    );
    assert!(
        normalized_app_html.contains(
            "markCurrentEndpointHealthy();\n    renderWithWait(projection);\n    return projection;"
        ),
        "a successful exact heartbeat must allow a later lease expiry to probe again"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("function acceptReplacementIdentity(result, stale)"),
        "only the dedicated replacement envelope may retarget a live card"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("candidate.controller_generation < identity.controller_generation"),
        "strictly delayed old-generation responses must be inert after replacement"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("if (tornDown || !snapshotIsCurrent(selector)) return false;"),
        "a delayed old bind response must be discarded after replacement"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("if (tornDown || !snapshotIsCurrent(selector)) return null;"),
        "a delayed old state response must be discarded after replacement"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("if (scheduledEpoch !== identityEpoch) return scheduleNext();"),
        "old timer callbacks must not coordinate the replacement generation"
    );
    assert!(
        !MCP_AGENT_CONTINUATION_APP_HTML.contains("rpcCode === -32000"),
        "generic Host -32000 must never be classified as endpoint expiry"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("accepted = acceptReplacementIdentity(response, stale);"),
        "every successor identity replacement must be gated by the dedicated recovery ToolResult"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML
            .contains("for (let hop = 0; hop < MAX_ENDPOINT_SUCCESSOR_HOPS; hop++)"),
        "v17 successor recovery must remain explicitly bounded"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("version: \"17.0.0\""),
        "App protocol version must advance with the v17 resource"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("const DEBUG_DIAGNOSTICS = false;"),
        "transport diagnostics must stay disabled in the normal product card"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("id=\"diagnostics\" class=\"meta\" hidden"),
        "technical continuation fields must be hidden by default"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("app_call_id"),
        "server-side App call correlation must remain available while UI diagnostics are hidden"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("get_agent_wait_state"),
        "v17 Wait presentation must poll only the exact durable Wait read surface"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("id=\"waitSummary\""),
        "v17 card must render the one-shot Wait lifecycle without a second dispatcher"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("function restartRecoveryOf(projection)"),
        "App restart recovery must consume only a successful validated projection"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains("function toolCallSucceeded(result)"),
        "App must distinguish a successful JSON-RPC exchange from a failed ToolResult"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains(
            "if (!toolCallSucceeded(response)) throw new Error(\"Continuation finish was not accepted\")"
        ),
        "post-fence finish must remain pending when the tool returns business failure"
    );
    assert!(
        MCP_AGENT_CONTINUATION_APP_HTML.contains(
            "if (!toolCallSucceeded(response)) throw new Error(\"Continuation acquire was not accepted\")"
        ),
        "acquire must not reinterpret a business failure as no pending Wake"
    );
}

#[tokio::test]
async fn agent_continuation_app_uses_hashed_openai_session_as_client_window_fence() {
    let (_temp, db, runtime) = continuation_runtime();
    let owner = continuation_auth("continuation-window-owner");
    let foreign = continuation_auth("continuation-window-foreign");
    let agent = create_agent(
        &runtime,
        &owner,
        "continuation-window-agent",
        "Window Agent",
        "continuation-window-create",
    );
    let (endpoint, generation) = attach(&runtime, &owner, &agent, "continuation-window-endpoint");
    let raw_session = "production-openai-session-window-a";
    let binding_id = "wc_host_binding_d3d3d3d3d3d3d3d3d3d3dw".to_string();
    let bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5151)),
            mcp_2026_window_params(
                json!({
                    "name": "bind_agent_continuation",
                    "arguments": {
                        "agent_id": agent,
                        "endpoint_id": endpoint,
                        "expected_controller_generation": generation,
                        "binding_id": binding_id
                    }
                }),
                raw_session,
            ),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(bind) = bind else {
        panic!("window-bound continuation bind failed")
    };
    assert_eq!(bind["result"]["structuredContent"]["success"], true);

    let expected_window =
        crate::client_window::ClientWindow::from_opaque("openai-session", raw_session).unwrap();
    let persisted = endpoint_client_window_key(&db, &endpoint).expect("persisted ClientWindow key");
    assert_eq!(persisted, expected_window.key());
    assert_ne!(
        persisted, raw_session,
        "raw OpenAI session must never be durable"
    );
    assert_eq!(persisted.len(), 64);

    let wrong_window = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5152)),
            mcp_2026_window_params(
                json!({
                    "name": "get_agent_continuation_state",
                    "arguments": {
                        "agent_id": agent,
                        "endpoint_id": endpoint,
                        "expected_controller_generation": generation,
                        "binding_id": binding_id
                    }
                }),
                "production-openai-session-window-b",
            ),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(wrong_window) = wrong_window else {
        panic!("wrong-window state must be a tool failure")
    };
    assert_eq!(
        wrong_window["result"]["structuredContent"]["success"],
        false
    );
    assert_eq!(
        wrong_window["result"]["structuredContent"]["output"]["error_kind"],
        "host_binding_stale"
    );

    let foreign_binding = "wc_host_binding_iIiIiIiIiIiIiIiIiIiIiA".to_string();
    let foreign_bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5153)),
            mcp_2026_window_params(
                json!({
                    "name": "bind_agent_continuation",
                    "arguments": {
                        "agent_id": agent,
                        "endpoint_id": endpoint,
                        "expected_controller_generation": generation,
                        "binding_id": foreign_binding
                    }
                }),
                raw_session,
            ),
        ),
        Some(&foreign),
        true,
    )
    .await;
    let McpOutcome::Ok(foreign_bind) = foreign_bind else {
        panic!("foreign principal must be represented as a tool failure")
    };
    assert_eq!(
        foreign_bind["result"]["structuredContent"]["success"],
        false
    );

    let owner_state = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5154)),
            mcp_2026_window_params(
                json!({
                    "name": "get_agent_continuation_state",
                    "arguments": {
                        "agent_id": agent,
                        "endpoint_id": endpoint,
                        "expected_controller_generation": generation,
                        "binding_id": binding_id
                    }
                }),
                raw_session,
            ),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(owner_state) = owner_state else {
        panic!("owner window state failed")
    };
    assert_eq!(owner_state["result"]["structuredContent"]["success"], true);

    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_endpoints SET lease_expires_at_unix_ms = 0 WHERE endpoint_id = ?1",
            [&endpoint],
        )
        .unwrap();
    let recovered = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5155)),
            mcp_2026_window_params(
                json!({
                    "name": "recover_agent_continuation_endpoint",
                    "arguments": {
                        "agent_id": agent,
                        "endpoint_id": endpoint,
                        "expected_controller_generation": generation,
                        "binding_id": binding_id
                    }
                }),
                raw_session,
            ),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(recovered) = recovered else {
        panic!("same-window expired Endpoint recovery failed")
    };
    assert_eq!(recovered["result"]["structuredContent"]["success"], true);
    let recovery_output = &recovered["result"]["structuredContent"]["output"];
    assert_eq!(
        recovery_output["endpoint_recovery"]["kind"],
        "endpoint_replaced"
    );
    assert_eq!(
        recovery_output["endpoint_recovery"]["replacement"]["from_endpoint_id"],
        endpoint
    );
    assert_eq!(
        recovery_output["endpoint_recovery"]["replacement"]["from_controller_generation"],
        generation
    );
    assert_eq!(
        recovery_output["endpoint_recovery"]["replacement"]["controller_generation"],
        generation + 1
    );
    let replacement_endpoint = recovery_output["endpoint_recovery"]["replacement"]["endpoint_id"]
        .as_str()
        .unwrap();
    assert_eq!(
        endpoint_client_window_key(&db, replacement_endpoint).as_deref(),
        Some(expected_window.key())
    );
}

#[test]
fn restart_recovery_survives_published_projection_output_schema() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-continuation-schema-restart.db");
    let db = Arc::new(crate::db::Database::open(&path).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let owner = continuation_auth("continuation-schema-restart");
    let agent = create_agent(
        &runtime,
        &owner,
        "continuation-schema-restart-agent",
        "Schema Restart Agent",
        "continuation-schema-restart-create",
    );
    let (endpoint, generation) = attach(
        &runtime,
        &owner,
        &agent,
        "continuation-schema-restart-endpoint",
    );
    let binding_id = "wc_host_binding_u7u7u7u7u7u7u7u7u7u7uw".to_string();
    let bind = runtime.agent_continuation_bind(
        Some(&owner),
        agent.clone(),
        endpoint.clone(),
        generation,
        binding_id.clone(),
    );
    assert!(bind.success, "{:?}", bind.output);
    let ordinary = runtime.agent_continuation_state(
        Some(&owner),
        agent.clone(),
        endpoint.clone(),
        generation,
        binding_id.clone(),
    );
    assert!(ordinary.success, "{:?}", ordinary.output);
    assert_eq!(
        ordinary.output["agent_continuation"]["recovery"],
        Value::Null
    );

    drop(runtime);
    drop(db);
    let reopened = Arc::new(crate::db::Database::open(&path).unwrap());
    let ownership = crate::ServerInstanceGuard::acquire(&reopened).unwrap();
    reopened
        .recover_agent_wakes_for_server_takeover(&ownership, chrono::Utc::now().timestamp_millis())
        .unwrap();
    let runtime = ToolRuntime::new_for_tests().with_communication_database(reopened);
    let restart =
        runtime.agent_continuation_state(Some(&owner), agent, endpoint, generation, binding_id);
    assert!(restart.success, "{:?}", restart.output);
    let runtime_projection = restart.output["agent_continuation"].clone();
    assert_eq!(
        runtime_projection["recovery"]["kind"],
        "host_binding_missing_in_process"
    );

    let published = webcodex_tool_contracts::output_schema_for_tool("present_agent_continuation");
    let projection_schema = &published["properties"]["output"]["properties"]["agent_continuation"];
    let host_projection =
        host_project_through_output_schema(&runtime_projection, projection_schema);
    assert_eq!(
        host_projection, runtime_projection,
        "published continuation outputSchema must preserve every runtime projection field"
    );
    assert_eq!(
        host_projection["recovery"]["kind"],
        "host_binding_missing_in_process"
    );
}

#[test]
fn expired_successor_replay_survives_published_recovery_output_schema() {
    let (_temp, db, runtime) = continuation_runtime();
    let owner = continuation_auth("continuation-schema-successor");
    let agent = create_agent(
        &runtime,
        &owner,
        "continuation-schema-successor-agent",
        "Schema Successor Agent",
        "continuation-schema-successor-create",
    );
    let (e1, g1) = attach(
        &runtime,
        &owner,
        &agent,
        "continuation-schema-successor-endpoint",
    );
    let window =
        crate::client_window::ClientWindow::for_test("continuation-schema-successor-window");
    let first_binding = "wc_host_binding_zMzMzMzMzMzMzMzMzMzMzA".to_string();
    let bound = runtime.agent_continuation_bind_for_window(
        Some(&owner),
        Some(&window),
        agent.clone(),
        e1.clone(),
        g1,
        first_binding.clone(),
    );
    assert!(bound.success, "{:?}", bound.output);
    let unbound = runtime.agent_continuation_unbind_for_window(
        Some(&owner),
        Some(&window),
        agent.clone(),
        e1.clone(),
        g1,
        first_binding,
    );
    assert!(unbound.success, "{:?}", unbound.output);
    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_endpoints SET lease_expires_at_unix_ms = 0 WHERE endpoint_id = ?1",
            [&e1],
        )
        .unwrap();

    let first = runtime.agent_continuation_recover_endpoint_for_window(
        Some(&owner),
        Some(&window),
        agent.clone(),
        e1.clone(),
        g1,
        "wc_host_binding_3d3d3d3d3d3d3d3d3d3d3Q".to_string(),
    );
    assert!(first.success, "{:?}", first.output);
    let e2 = first.output["endpoint_recovery"]["replacement"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let g2 = first.output["endpoint_recovery"]["replacement"]["controller_generation"]
        .as_i64()
        .unwrap();
    assert_eq!(g2, g1 + 1);
    assert_eq!(
        first.output["endpoint_recovery"]["successor_needs_recovery"],
        false
    );

    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_endpoints SET lease_expires_at_unix_ms = 0 WHERE endpoint_id = ?1",
            [&e2],
        )
        .unwrap();
    let second = runtime.agent_continuation_recover_endpoint_for_window(
        Some(&owner),
        Some(&window),
        agent.clone(),
        e2.clone(),
        g2,
        "wc_host_binding_7u7u7u7u7u7u7u7u7u7u7g".to_string(),
    );
    assert!(second.success, "{:?}", second.output);
    assert_eq!(
        second.output["endpoint_recovery"]["replacement"]["controller_generation"],
        g2 + 1
    );

    let old_selector_replay = runtime.agent_continuation_recover_endpoint_for_window(
        Some(&owner),
        Some(&window),
        agent,
        e1.clone(),
        g1,
        "wc_host_binding______________________w".to_string(),
    );
    assert!(
        old_selector_replay.success,
        "{:?}",
        old_selector_replay.output
    );
    assert!(old_selector_replay.output["agent_continuation"].is_null());
    assert_eq!(old_selector_replay.output["replayed"], true);
    assert_eq!(old_selector_replay.output["state_changed"], false);
    assert_eq!(
        old_selector_replay.output["endpoint_recovery"]["successor_needs_recovery"],
        true
    );
    assert_eq!(
        old_selector_replay.output["endpoint_recovery"]["replacement"]["from_endpoint_id"],
        e1
    );
    assert_eq!(
        old_selector_replay.output["endpoint_recovery"]["replacement"]["endpoint_id"],
        e2
    );
    assert_eq!(
        old_selector_replay.output["endpoint_recovery"]["replacement"]["controller_generation"],
        g2
    );

    let published =
        webcodex_tool_contracts::output_schema_for_tool("recover_agent_continuation_endpoint");
    let output_schema = &published["properties"]["output"];
    let host_projection =
        host_project_through_output_schema(&old_selector_replay.output, output_schema);
    assert_eq!(
        host_projection, old_selector_replay.output,
        "published recovery outputSchema must preserve the intermediate one-hop successor proof"
    );
    assert_eq!(
        host_projection["endpoint_recovery"]["successor_needs_recovery"],
        true
    );
}

#[test]
fn task_origin_wake_survives_published_bootstrap_output_schema() {
    let wake = json!({
        "wake_id": "wc_wake_qqqqqqqqqqqqqqqq".to_string(),
        "state": "pending",
        "revision": 1,
        "trigger_kind": "agent_task_attempt",
        "conversation_id": null,
        "latest_message_id": null,
        "queued_delivery_count": null,
        "inbox_high_watermark": null,
        "task_id": "wc_agent_task_u7u7u7u7u7u7u7u7".to_string(),
        "task_attempt_id": "wc_agent_task_attempt_zMzMzMzMzMzMzMzM".to_string(),
    });
    let published = webcodex_tool_contracts::output_schema_for_tool("bootstrap_agent_conversation");
    let wake_schema = &published["properties"]["output"]["properties"]["wake"];
    let host_projection = host_project_through_output_schema(&wake, wake_schema);
    assert_eq!(
        host_projection, wake,
        "published bootstrap outputSchema must preserve exact AgentTask-origin Wake identity and null Inbox fields"
    );
}

#[tokio::test]
async fn agent_continuation_app_protocol_uses_standard_result_without_model_projection_leaks() {
    let binding_id = "wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg".to_string();
    let (_temp, _db, runtime) = continuation_runtime();
    let owner = continuation_auth("continuation-owner");
    let foreign = continuation_auth("continuation-foreign");
    let sender = create_agent(
        &runtime,
        &owner,
        "continuation-sender",
        "Sender",
        "continuation-sender-create",
    );
    let receiver = create_agent(
        &runtime,
        &owner,
        "continuation-receiver",
        "Receiver",
        "continuation-receiver-create",
    );
    let (sender_endpoint, sender_generation) =
        attach(&runtime, &owner, &sender, "continuation-sender-endpoint");
    let (receiver_endpoint, receiver_generation) = attach(
        &runtime,
        &owner,
        &receiver,
        "continuation-receiver-endpoint",
    );
    let conversation = create_conversation(
        &runtime,
        &owner,
        &sender,
        &receiver,
        "continuation-conversation",
    );

    // Exercise the retained domain projection without advertising a new model
    // presentation entry. Existing App protocol calls below remain real MCP.
    let present = runtime.present_agent_continuation(
        Some(&owner),
        receiver.clone(),
        receiver_endpoint.clone(),
        receiver_generation,
    );
    let present = json!({"result": super::super::tools::mcp_runtime_tool_result(
        "present_agent_continuation", false, present,
    )});
    assert_eq!(present["result"]["structuredContent"]["success"], true);
    assert_eq!(
        present["result"]["structuredContent"]["output"]["agent_continuation"]["display_name"],
        "Receiver"
    );
    let present_text = present["result"]["structuredContent"].to_string();
    assert!(!present_text.contains("PRIVATE Agent description"));
    assert!(!present_text.contains("PRIVATE-specialty-label"));

    let foreign_bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5202)),
            mcp_2026_params(json!({
                "name": "bind_agent_continuation",
                "arguments": {
                    "agent_id": receiver,
                    "endpoint_id": receiver_endpoint,
                    "expected_controller_generation": receiver_generation,
                    "binding_id": binding_id
                }
            })),
        ),
        Some(&foreign),
        true,
    )
    .await;
    let McpOutcome::Ok(foreign_bind) = foreign_bind else {
        panic!("foreign call should be a tool failure")
    };
    assert_eq!(
        foreign_bind["result"]["structuredContent"]["success"],
        false
    );
    assert!(foreign_bind["result"]["_meta"]
        .get("webcodex/agentContinuation")
        .is_none());

    let bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5203)),
            mcp_2026_params(json!({
                "name": "bind_agent_continuation",
                "arguments": {
                    "agent_id": receiver,
                    "endpoint_id": receiver_endpoint,
                    "expected_controller_generation": receiver_generation,
                    "binding_id": binding_id,
                    "app_call_id": "wc_app_call_0123456789abcdef_1"
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(bind) = bind else {
        panic!("bind failed")
    };
    assert_eq!(bind["result"]["structuredContent"]["success"], true);
    assert!(bind["result"]["_meta"]
        .get("webcodex/agentContinuation")
        .is_none());
    assert_eq!(
        bind["result"]["structuredContent"]["output"]["agent_continuation"]["host_binding"]
            ["bound"],
        true
    );
    let bind_content: Value = serde_json::from_str(
        bind["result"]["content"][0]["text"]
            .as_str()
            .expect("app-only bind must carry a standard text compatibility envelope"),
    )
    .expect("app-only bind text compatibility envelope must be JSON");
    assert_eq!(bind_content, bind["result"]["structuredContent"]);
    let bind_structured = bind["result"]["structuredContent"].to_string();
    assert!(!bind_structured.contains("wc_host_binding_"));
    assert!(!bind_structured.contains("_app_private"));

    let private_body = "PRIVATE durable business message for MCP carrier";
    post_message(
        &runtime,
        &owner,
        &conversation,
        &sender,
        &sender_endpoint,
        sender_generation,
        &receiver,
        private_body,
        "continuation-private-message",
    );
    let acquire = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5204)),
            mcp_2026_params(json!({
                "name": "acquire_agent_continuation_wake",
                "arguments": {
                    "agent_id": receiver,
                    "endpoint_id": receiver_endpoint,
                    "expected_controller_generation": receiver_generation,
                    "binding_id": binding_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(acquire) = acquire else {
        panic!("acquire failed")
    };
    let wake_id = acquire["result"]["structuredContent"]["output"]["wake"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_id = acquire["result"]["structuredContent"]["output"]["wake"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let acquire_text = acquire["result"]["structuredContent"].to_string();
    assert!(!acquire_text.contains("claim_fence"));
    assert!(!acquire_text.contains("consume_token"));
    assert!(!acquire_text.contains(private_body));

    let prepare = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5205)),
            mcp_2026_params(json!({
                "name": "prepare_agent_continuation_wake",
                "arguments": {
                    "agent_id": receiver,
                    "endpoint_id": receiver_endpoint,
                    "expected_controller_generation": receiver_generation,
                    "binding_id": binding_id,
                    "wake_id": wake_id,
                    "attempt_id": attempt_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(prepare) = prepare else {
        panic!("prepare failed")
    };
    assert_eq!(prepare["result"]["structuredContent"]["success"], true);
    let structured = prepare["result"]["structuredContent"].to_string();
    for forbidden in [
        "wc_host_binding_",
        "claim_fence",
        private_body,
        "PRIVATE Agent description",
        "PRIVATE-specialty-label",
        "_app_private",
    ] {
        assert!(
            !structured.contains(forbidden),
            "structuredContent leaked {forbidden}"
        );
    }
    assert!(prepare["result"]["_meta"]
        .get("webcodex/agentContinuation")
        .is_none());
    let automatic_message = prepare["result"]["structuredContent"]["output"]["app_protocol"]
        ["automatic_message"]
        .as_str()
        .expect("App-private exact continuation envelope");
    assert!(automatic_message.contains("consume_token=wc_wake_consume_"));
    assert!(automatic_message.contains(&format!("agent_id={receiver}")));
    assert!(automatic_message.contains(&format!("endpoint_id={receiver_endpoint}")));
    assert!(automatic_message.contains(&format!("wake_id={wake_id}")));
    assert!(!automatic_message.contains(private_body));
    assert!(!automatic_message.contains("PRIVATE Agent description"));
    assert!(!automatic_message.contains("PRIVATE-specialty-label"));
    assert!(automatic_message.chars().count() <= 1536);
    let prepare_content: Value = serde_json::from_str(
        prepare["result"]["content"][0]["text"]
            .as_str()
            .expect("app-only prepare must carry a standard text compatibility envelope"),
    )
    .expect("app-only prepare text compatibility envelope must be JSON");
    assert_eq!(prepare_content, prepare["result"]["structuredContent"]);
    for forbidden in [
        "wc_host_binding_",
        "claim_fence",
        private_body,
        "_app_private",
    ] {
        assert!(!prepare_content.to_string().contains(forbidden));
    }

    // Knowing the current binding and exact Attempt never grants authority.
    let mut read_only_owner = owner.clone();
    read_only_owner
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_COMMUNICATION_MANAGE);
    for name in APP_TOOLS {
        if name == "get_agent_wait_state" {
            // Agent Wait state is a separate read-only App projection keyed by wait_id;
            // Store/runtime tests own exact Wait authority and existence-hiding coverage.
            continue;
        }
        let mut args = json!({
            "agent_id": receiver, "endpoint_id": receiver_endpoint,
            "expected_controller_generation": receiver_generation, "binding_id": binding_id,
        });
        if matches!(
            name,
            "prepare_agent_continuation_wake" | "finish_agent_continuation_wake"
        ) {
            args["wake_id"] = json!(wake_id);
            args["attempt_id"] = json!(attempt_id);
        }
        if name == "finish_agent_continuation_wake" {
            args["outcome"] = json!("dispatch_accepted");
        }
        let request = || {
            rpc(
                "tools/call",
                Some(json!(5206)),
                mcp_2026_params(json!({"name": name, "arguments": args})),
            )
        };
        let denied =
            handle_with_app_policy(&runtime, request(), Some(&foreign), true).await;
        let McpOutcome::Ok(denied) = denied else {
            panic!("foreign {name} must fail as a business result")
        };
        assert_eq!(denied["result"]["structuredContent"]["success"], false);
        let denied_text = denied.to_string();
        assert!(!denied_text.contains("consume_token"));
        assert!(!denied_text.contains(&binding_id));
        let unscoped =
            handle_with_app_policy(&runtime, request(), Some(&read_only_owner), true)
                .await;
        assert!(
            matches!(unscoped, McpOutcome::Forbidden { .. }),
            "{name} requires communication:manage even with the exact fence"
        );
    }

    // The model-visible read stays sparse even while a prepared envelope exists.
    let present_after_prepare = runtime.present_agent_continuation(
        Some(&owner),
        receiver,
        receiver_endpoint,
        receiver_generation,
    );
    assert!(present_after_prepare.success);
    for projection in [present_text, present_after_prepare.output.to_string()] {
        for secret in [
            "binding_id",
            "wc_host_binding_",
            "consume_token",
            "automatic_message",
            "app_protocol",
            "claim_fence",
        ] {
            assert!(
                !projection.contains(secret),
                "model projection leaked {secret}"
            );
        }
    }
}

#[tokio::test]
async fn all_agent_wait_mcp_automatic_message_is_compact_and_guides_authoritative_rereads() {
    use crate::tool_runtime::{AgentWaitEventSelectorCall, AgentWaitModeCall};

    let binding_id = "wc_host_binding_u7u7u7u7u7u7u7u7u7u7uw".to_string();
    let (_temp, _db, runtime) = continuation_runtime();
    let owner = continuation_auth("continuation-all-wait-owner");
    let watcher = create_agent(
        &runtime,
        &owner,
        "continuation-all-wait-watcher",
        "ALL Wait Watcher",
        "continuation-all-wait-watcher-create",
    );
    let worker = create_agent(
        &runtime,
        &owner,
        "continuation-all-wait-worker",
        "ALL Wait Worker",
        "continuation-all-wait-worker-create",
    );
    let (watcher_endpoint, watcher_generation) =
        attach(&runtime, &owner, &watcher, "continuation-all-wait-endpoint");

    let present = runtime.present_agent_continuation(
        Some(&owner),
        watcher.clone(),
        watcher_endpoint.clone(),
        watcher_generation,
    );
    assert!(present.success, "retained ALL Wait projection: {present:?}");

    let bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5302)),
            mcp_2026_params(json!({
                "name": "bind_agent_continuation",
                "arguments": {
                    "agent_id": watcher,
                    "endpoint_id": watcher_endpoint,
                    "expected_controller_generation": watcher_generation,
                    "binding_id": binding_id,
                    "app_call_id": "wc_app_call_0123456789abcdef_2"
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(bind) = bind else {
        panic!("ALL Wait continuation bind failed")
    };
    assert_eq!(bind["result"]["structuredContent"]["success"], true);

    let mut task_ids = Vec::new();
    for index in 0..2 {
        let task = runtime.create_agent_task(
            Some(&owner),
            format!("ALL Wait source {index}"),
            format!("PRIVATE ALL Wait instruction {index}"),
            Some(worker.clone()),
            None,
            None,
            Some(format!("agent:special:private-all-wait-{index}")),
            format!("continuation-all-wait-task-{index}"),
        );
        assert!(task.success, "{:?}", task.output);
        let task_id = task.output["task"]["summary"]["task_id"]
            .as_str()
            .unwrap()
            .to_string();
        let started = runtime.start_agent_task_attempt(
            Some(&owner),
            task_id.clone(),
            worker.clone(),
            format!("continuation-all-wait-start-{index}"),
        );
        assert!(started.success, "{:?}", started.output);
        let completed = runtime.complete_agent_task_attempt(
            Some(&owner),
            task_id.clone(),
            started.output["attempt"]["attempt_id"]
                .as_str()
                .unwrap()
                .to_string(),
            worker.clone(),
            started.output["attempt_fence"]
                .as_str()
                .unwrap()
                .to_string(),
            started.output["attempt"]["attempt_controller_generation"]
                .as_i64()
                .unwrap(),
            "succeeded".to_string(),
            Some(format!("PRIVATE ALL Wait terminal result {index}")),
            Some(format!("PRIVATE ALL Wait terminal reason {index}")),
            format!("continuation-all-wait-complete-{index}"),
        );
        assert!(completed.success, "{:?}", completed.output);
        task_ids.push(task_id);
    }

    let waited = runtime.wait_for_agent_events(
        Some(&owner),
        watcher.clone(),
        watcher_endpoint.clone(),
        watcher_generation,
        AgentWaitModeCall::All,
        None,
        task_ids
            .iter()
            .map(|task_id| AgentWaitEventSelectorCall {
                kind: "agent_task_terminal".to_string(),
                task_id: task_id.clone(),
            })
            .collect(),
        "continuation-all-wait-create".to_string(),
    );
    assert!(waited.success, "{:?}", waited.output);
    assert_eq!(waited.output["agent_wait"]["state"], "triggered");
    assert_eq!(waited.output["agent_wait"]["mode"], "all");
    assert_eq!(waited.output["agent_wait"]["source_count"], 2);
    assert_eq!(waited.output["agent_wait"]["match_count"], 2);
    let wait_id = waited.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();

    let acquire = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5303)),
            mcp_2026_params(json!({
                "name": "acquire_agent_continuation_wake",
                "arguments": {
                    "agent_id": watcher,
                    "endpoint_id": watcher_endpoint,
                    "expected_controller_generation": watcher_generation,
                    "binding_id": binding_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(acquire) = acquire else {
        panic!("ALL Wait Wake acquire failed")
    };
    assert_eq!(acquire["result"]["structuredContent"]["success"], true);
    let wake_id = acquire["result"]["structuredContent"]["output"]["wake"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_id = acquire["result"]["structuredContent"]["output"]["wake"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();

    let prepare = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5304)),
            mcp_2026_params(json!({
                "name": "prepare_agent_continuation_wake",
                "arguments": {
                    "agent_id": watcher,
                    "endpoint_id": watcher_endpoint,
                    "expected_controller_generation": watcher_generation,
                    "binding_id": binding_id,
                    "wake_id": wake_id,
                    "attempt_id": attempt_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(prepare) = prepare else {
        panic!("ALL Wait Wake prepare failed")
    };
    assert_eq!(prepare["result"]["structuredContent"]["success"], true);
    let automatic_message = prepare["result"]["structuredContent"]["output"]["app_protocol"]
        ["automatic_message"]
        .as_str()
        .expect("ALL Wait exact continuation envelope");
    for required in [
        &format!("wait_id={wait_id}"),
        "mode=all",
        "matched=2/2",
        "bootstrap_agent_conversation",
        "consume_agent_wake",
        "read_agent_wait(wait_id)",
        "authoritative source AgentTasks",
    ] {
        assert!(
            automatic_message.contains(required),
            "missing ALL Wait continuation detail {required}"
        );
    }
    assert!(
        automatic_message.chars().count() <= 1_000,
        "ALL Wait automatic message too long: {}",
        automatic_message.chars().count()
    );
    for forbidden in [
        "PRIVATE ALL Wait instruction",
        "PRIVATE ALL Wait terminal result",
        "PRIVATE ALL Wait terminal reason",
        "private-all-wait",
        "grants no Task, Project, Goal",
        "already dispatched by the Endpoint continuation carrier",
    ] {
        assert!(
            !automatic_message.contains(forbidden),
            "ALL Wait continuation leaked or re-expanded {forbidden}"
        );
    }
}

#[tokio::test]
async fn goal_scoped_agent_wait_mcp_message_names_goal_and_authoritative_rereads_compactly() {
    use crate::tool_runtime::{AgentWaitEventSelectorCall, AgentWaitModeCall};

    let binding_id = "wc_host_binding_GoGoGoGoGoGoGoGoGoGoGw".to_string();
    let (_temp, _db, runtime) = continuation_runtime();
    let owner = continuation_auth("continuation-goal-scoped-wait-owner");
    let controller = create_agent(
        &runtime,
        &owner,
        "continuation-goal-scoped-controller",
        "Goal Scoped Controller",
        "continuation-goal-scoped-controller-create",
    );
    let worker = create_agent(
        &runtime,
        &owner,
        "continuation-goal-scoped-worker",
        "Goal Scoped Worker",
        "continuation-goal-scoped-worker-create",
    );
    let (controller_endpoint, controller_generation) = attach(
        &runtime,
        &owner,
        &controller,
        "continuation-goal-scoped-endpoint",
    );

    let present = runtime.present_agent_continuation(
        Some(&owner),
        controller.clone(),
        controller_endpoint.clone(),
        controller_generation,
    );
    assert!(
        present.success,
        "retained Goal-scoped Wait projection: {present:?}"
    );

    let bind = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5402)),
            mcp_2026_params(json!({
                "name": "bind_agent_continuation",
                "arguments": {
                    "agent_id": controller,
                    "endpoint_id": controller_endpoint,
                    "expected_controller_generation": controller_generation,
                    "binding_id": binding_id,
                    "app_call_id": "wc_app_call_0123456789abcdef_3"
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(bind) = bind else {
        panic!("Goal-scoped Wait continuation bind failed")
    };
    assert_eq!(bind["result"]["structuredContent"]["success"], true);

    let goal = runtime.create_goal_with_controller(
        Some(&owner),
        "PRIVATE GOAL TITLE MUST NOT ENTER AUTOMATIC MESSAGE".to_string(),
        "PRIVATE GOAL OBJECTIVE MUST NOT ENTER AUTOMATIC MESSAGE".to_string(),
        Some(controller.clone()),
        "continuation-goal-scoped-goal".to_string(),
    );
    assert!(goal.success, "{:?}", goal.output);
    let goal_id = goal.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let task = runtime.create_agent_task(
        Some(&owner),
        "Goal-scoped ALL source".to_string(),
        "PRIVATE GOAL SCOPED TASK INSTRUCTION".to_string(),
        Some(worker.clone()),
        None,
        None,
        Some("agent:special:PRIVATE_GOAL_SCOPED_PROJECT".to_string()),
        "continuation-goal-scoped-task".to_string(),
    );
    assert!(task.success, "{:?}", task.output);
    let task_id = task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let linked = runtime.associate_goal_agent_task(
        Some(&owner),
        goal_id.clone(),
        task_id.clone(),
        "continuation-goal-scoped-link".to_string(),
    );
    assert!(linked.success, "{:?}", linked.output);

    let started = runtime.start_agent_task_attempt(
        Some(&owner),
        task_id.clone(),
        worker.clone(),
        "continuation-goal-scoped-start".to_string(),
    );
    assert!(started.success, "{:?}", started.output);

    let waited = runtime.wait_for_agent_events(
        Some(&owner),
        controller.clone(),
        controller_endpoint.clone(),
        controller_generation,
        AgentWaitModeCall::All,
        Some(goal_id.clone()),
        vec![AgentWaitEventSelectorCall {
            kind: "agent_task_terminal".to_string(),
            task_id: task_id.clone(),
        }],
        "continuation-goal-scoped-wait".to_string(),
    );
    assert!(waited.success, "{:?}", waited.output);
    assert_eq!(waited.output["agent_wait"]["goal_id"], goal_id);
    assert_eq!(waited.output["agent_wait"]["state"], "waiting");
    assert_eq!(waited.output["agent_wait"]["match_count"], 0);

    let completed = runtime.complete_agent_task_attempt(
        Some(&owner),
        task_id,
        started.output["attempt"]["attempt_id"]
            .as_str()
            .unwrap()
            .to_string(),
        worker,
        started.output["attempt_fence"]
            .as_str()
            .unwrap()
            .to_string(),
        started.output["attempt"]["attempt_controller_generation"]
            .as_i64()
            .unwrap(),
        "succeeded".to_string(),
        Some("PRIVATE GOAL SCOPED TERMINAL RESULT".to_string()),
        Some("PRIVATE GOAL SCOPED TERMINAL REASON".to_string()),
        "continuation-goal-scoped-complete".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);
    let wait_id = waited.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();

    let acquire = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5403)),
            mcp_2026_params(json!({
                "name": "acquire_agent_continuation_wake",
                "arguments": {
                    "agent_id": controller,
                    "endpoint_id": controller_endpoint,
                    "expected_controller_generation": controller_generation,
                    "binding_id": binding_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(acquire) = acquire else {
        panic!("Goal-scoped Wait Wake acquire failed")
    };
    assert_eq!(acquire["result"]["structuredContent"]["success"], true);
    let wake_id = acquire["result"]["structuredContent"]["output"]["wake"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_id = acquire["result"]["structuredContent"]["output"]["wake"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();

    let prepare = handle_with_app_policy(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5404)),
            mcp_2026_params(json!({
                "name": "prepare_agent_continuation_wake",
                "arguments": {
                    "agent_id": controller,
                    "endpoint_id": controller_endpoint,
                    "expected_controller_generation": controller_generation,
                    "binding_id": binding_id,
                    "wake_id": wake_id,
                    "attempt_id": attempt_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(prepare) = prepare else {
        panic!("Goal-scoped Wait Wake prepare failed")
    };
    assert_eq!(prepare["result"]["structuredContent"]["success"], true);
    let automatic_message = prepare["result"]["structuredContent"]["output"]["app_protocol"]
        ["automatic_message"]
        .as_str()
        .expect("Goal-scoped Wait exact continuation envelope");
    for required in [
        &format!("wait_id={wait_id}"),
        &format!("goal_id={goal_id}"),
        "mode=all",
        "matched=1/1",
        "bootstrap_agent_conversation",
        "consume_agent_wake",
        "read_agent_wait(wait_id)",
        "get_goal(goal_id)",
        "every authoritative source AgentTask",
        "explicitly decide and update the Goal from current durable state",
    ] {
        assert!(
            automatic_message.contains(required),
            "missing Goal-scoped Wait continuation detail {required}"
        );
    }
    assert!(
        automatic_message.chars().count() <= 1_000,
        "Goal-scoped Wait automatic message too long: {}",
        automatic_message.chars().count()
    );
    for forbidden in [
        "PRIVATE GOAL TITLE",
        "PRIVATE GOAL OBJECTIVE",
        "PRIVATE GOAL SCOPED TASK INSTRUCTION",
        "PRIVATE_GOAL_SCOPED_PROJECT",
        "PRIVATE GOAL SCOPED TERMINAL RESULT",
        "PRIVATE GOAL SCOPED TERMINAL REASON",
        "goal lifecycle",
        "controller Endpoint",
        "grants no Task, Project, Goal",
    ] {
        assert!(
            !automatic_message.contains(forbidden),
            "Goal-scoped Wait continuation leaked or re-expanded {forbidden}"
        );
    }
}

#[tokio::test]
async fn agent_continuation_hidden_kernel_entry_is_fail_closed_without_protocol_capability() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallErrorStatus, ToolCallRequest,
        ToolProtocolCapabilities, ToolTransport,
    };

    let runtime = ToolRuntime::new_for_tests();
    let auth = continuation_auth("continuation-kernel-gate");
    for transport in [ToolTransport::Mcp, ToolTransport::Api] {
        for name in APP_TOOLS {
            let outcome = runtime
                .call_tool_with_protocol_capabilities(
                    ToolCallRequest {
                        tool_name: name.to_string(),
                        arguments: json!({
                            "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq".to_string(),
                            "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7".to_string(),
                            "expected_controller_generation": 1,
                            "binding_id": "wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg".to_string()
                        }),
                    },
                    ToolCallContext {
                        transport,
                        session_id: None,
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    ToolProtocolCapabilities::default(),
                )
                .await;
            assert!(matches!(
                outcome.error_status,
                Some(ToolCallErrorStatus::InvalidArguments { ref message })
                    if message.contains("Agent continuation App coordination")
            ));
            assert!(outcome.result.is_none());
        }
    }
}
