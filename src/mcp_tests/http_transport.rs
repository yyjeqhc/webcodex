use super::*;

#[tokio::test]
async fn mcp_tools_call_writes_a_summary_action_audit_row() {
    // list_tools is a full-operator-only tool; select that surface so the
    // call dispatches and lands an action audit row.
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // End the connection guard structurally inside the block: the awaited
    // requests below must not overlap it.
    let (endpoint, action, operation, status, summary) = {
        let conn = db.conn_for_tests();
        let (endpoint, action, operation, status): (String, String, String, String) = conn
            .query_row(
                "SELECT endpoint, action_name, operation, status FROM action_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        // Summary-level discipline: no tool output is persisted for MCP rows.
        let summary: String = conn
            .query_row("SELECT summary_json FROM action_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        (endpoint, action, operation, status, summary)
    };
    assert_eq!(endpoint, "/mcp");
    assert_eq!(action, "toolsCall");
    assert_eq!(operation, "list_tools");
    assert_eq!(status, "success");
    assert!(
        !summary.contains("tools"),
        "summary must not embed output: {summary}"
    );

    // Non-tool methods stay out of the audit.
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // A no-id tools/call is a JSON-RPC notification. The core dispatcher
    // intentionally ignores every notification, so it must not create a
    // successful action row for work that never ran.
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::ACCEPTED));

    let count: i64 = {
        let conn = db.conn_for_tests();
        conn.query_row("SELECT COUNT(*) FROM action_events", [], |row| row.get(0))
            .unwrap()
    };
    assert_eq!(count, 1);
}

fn seed_action_audit_pat(db: &crate::Database, user: &crate::models::UserRecord) -> String {
    let plaintext = crate::auth::generate_api_token();
    let record = crate::models::ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: "action-audit-test".to_string(),
        key_prefix: crate::auth::token_prefix(&plaintext),
        created_at: chrono::Utc::now().timestamp(),
        last_used_at: None,
        revoked_at: None,
        scopes: "runtime:read project:read project:write job:run".to_string(),
        expires_at: None,
        kind: crate::models::TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &crate::auth::hash_token(&plaintext))
        .unwrap();
    plaintext
}

#[tokio::test]
async fn mcp_pat_tools_call_persists_user_attribution() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_action_audit_pat(&db, &user);
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));

    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth(&token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let attrs: (Option<String>, Option<String>, Option<String>) = db
        .conn_for_tests()
        .query_row(
            "SELECT principal_kind, principal_user_id, oauth_client_id FROM action_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(attrs.0.as_deref(), Some("user"));
    assert_eq!(attrs.1.as_deref(), Some(user.id.as_str()));
    assert_eq!(attrs.2, None);
}

#[tokio::test]
async fn mcp_oauth_tools_call_persists_user_and_client_attribution() {
    let config = test_config_oauth2(Some("secret"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, "runtime:read");
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));

    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth(&token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let attrs: (Option<String>, Option<String>, Option<String>) = db
        .conn_for_tests()
        .query_row(
            "SELECT principal_kind, principal_user_id, oauth_client_id FROM action_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(attrs.0.as_deref(), Some("oauth2"));
    assert_eq!(attrs.1.as_deref(), Some(user.id.as_str()));
    assert_eq!(attrs.2.as_deref(), Some(client.client_id.as_str()));
}
#[tokio::test]
async fn http_mcp_initialize_success() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let session_id = resp
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("initialize must mint an MCP session id");
    assert!(session_id.starts_with("wc_mcp_"));
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["result"]["serverInfo"]["name"], "webcodex");
    assert_eq!(
        body["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
    assert!(body["result"]["protocolVersion"].is_string());
    assert_eq!(
        body["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
}

// The asserted outputSchema presence is the default (non-compact) product
// behavior: `WEBCODEX_MCP_COMPACT_SCHEMAS` must stay unset (and serialized
// against other env-mutating tests) for the whole HTTP request.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_tools_list_success() {
    // Default (non-compact) HTTP tools/list: full schema fields present.
    // Compact-mode shape is covered by mcp_tools_list_compact_*.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
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
        assert!(
            tool["outputSchema"].is_object(),
            "default HTTP tools/list must include outputSchema for {}",
            tool["name"]
        );
    }
}

#[tokio::test]
async fn http_mcp_2026_validates_headers_and_ignores_legacy_session_id() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({}));

    let mut missing_headers = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 200,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_headers), StatusCode::BAD_REQUEST);
    let missing_body: Value = missing_headers.take_json().await.unwrap();
    assert_eq!(missing_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut ok = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-be-ignored",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 201,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&ok), StatusCode::OK);
    assert!(ok
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .is_none());
    let ok_body: Value = ok.take_json().await.unwrap();
    assert_eq!(ok_body["result"]["resultType"], "complete");
    assert_eq!(ok_body["result"]["cacheScope"], "private");

    let mut method_mismatch = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "ping", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 202,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&method_mismatch), StatusCode::BAD_REQUEST);
    let mismatch_body: Value = method_mismatch.take_json().await.unwrap();
    assert_eq!(mismatch_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut missing_capabilities = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2021,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&missing_capabilities),
        StatusCode::BAD_REQUEST
    );
    let missing_capabilities_body: Value = missing_capabilities.take_json().await.unwrap();
    assert_eq!(missing_capabilities_body["error"]["code"], -32602);

    let mut version_mismatch = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2022,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&version_mismatch), StatusCode::BAD_REQUEST);
    let version_mismatch_body: Value = version_mismatch.take_json().await.unwrap();
    assert_eq!(version_mismatch_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut malformed_client_info = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2023,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {"name": "missing-version"}
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&malformed_client_info),
        StatusCode::BAD_REQUEST
    );
    let malformed_client_info_body: Value = malformed_client_info.take_json().await.unwrap();
    assert_eq!(malformed_client_info_body["error"]["code"], -32602);

    let mut missing_jsonrpc = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "id": 2024,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_jsonrpc), StatusCode::BAD_REQUEST);
    let missing_jsonrpc_body: Value = missing_jsonrpc.take_json().await.unwrap();
    assert_eq!(missing_jsonrpc_body["error"]["code"], -32600);

    let unsupported_params = json!({
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2099-01-01",
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    });
    let mut unsupported = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 203,
            "method": "tools/list",
            "params": unsupported_params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&unsupported), StatusCode::BAD_REQUEST);
    let unsupported_body: Value = unsupported.take_json().await.unwrap();
    assert_eq!(
        unsupported_body["error"]["code"],
        MCP_UNSUPPORTED_PROTOCOL_VERSION
    );
    assert_eq!(unsupported_body["error"]["data"]["requested"], "2099-01-01");
    assert_eq!(
        unsupported_body["error"]["data"]["supported"],
        json!(MCP_SUPPORTED_PROTOCOL_VERSIONS)
    );
}

#[tokio::test]
async fn http_mcp_2026_tools_call_requires_matching_name_and_accepts_base64_sentinel() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({"name": "list_projects", "arguments": {}}));

    let mut missing_name = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 204,
            "method": "tools/call",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_name), StatusCode::BAD_REQUEST);
    let missing_name_body: Value = missing_name.take_json().await.unwrap();
    assert_eq!(missing_name_body["error"]["code"], MCP_HEADER_MISMATCH);

    let encoded = general_purpose::STANDARD.encode("list_projects");
    let encoded = format!("=?base64?{encoded}?=");
    let mut ok = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, &encoded, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 205,
            "method": "tools/call",
            "params": params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&ok), StatusCode::OK);
    let ok_body: Value = ok.take_json().await.unwrap();
    assert_eq!(ok_body["result"]["resultType"], "complete");
}

#[tokio::test]
async fn http_mcp_2026_unknown_method_is_404_jsonrpc_method_not_found() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "prompts/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 206,
            "method": "prompts/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::NOT_FOUND);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn http_mcp_2026_reads_computer_app_template_with_cache_contract() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let params = mcp_2026_ui_params(json!({"uri": MCP_COMPUTER_UI_RESOURCE_URI}));

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, MCP_COMPUTER_UI_RESOURCE_URI, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2060,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["result"]["resultType"], "complete");
    assert_eq!(
        body["result"]["ttlMs"],
        Value::from(MCP_COMPUTER_UI_RESOURCE_TTL_MS)
    );
    assert_eq!(body["result"]["cacheScope"], "private");
    assert_eq!(
        body["result"]["contents"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );
    assert_eq!(
        body["result"]["contents"][0]["mimeType"],
        MCP_UI_RESOURCE_MIME_TYPE
    );
    assert!(body["result"]["contents"][0]["_meta"]
        .get("openai/widgetDomain")
        .is_none());
    let html = body["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(html.starts_with("<div id=\"app\""));
    assert!(html.contains("HTML loaded"));
    assert!(html.contains("ui/initialize"));
    assert!(html.contains("ui/notifications/initialized"));
    assert!(html.contains("ui/notifications/tool-result"));
    assert!(!html.contains("tools/call"));
    assert!(!html.contains("ui/request-display-mode"));

    let (endpoint, action, operation, status, http_status, summary): (
        String,
        String,
        String,
        String,
        i64,
        String,
    ) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT endpoint, action_name, operation, status, http_status, summary_json FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap()
    };
    assert_eq!(endpoint, "/mcp");
    assert_eq!(action, "resourcesRead");
    assert_eq!(operation, "computer_app_resource_read");
    assert_eq!(status, "success");
    assert_eq!(http_status, 200);
    let summary: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(
        summary,
        json!({
            "transport": "mcp",
            "resource_uri": MCP_COMPUTER_UI_RESOURCE_URI,
            "resource_version": "v11",
            "protocol_era": "stateless_2026",
            "ui_capability_present": true,
            "mcp_error_code": Value::Null,
        })
    );
    let durable = summary.to_string();
    for forbidden in ["HTML loaded", "content_base64", "surface_", "Waiting for"] {
        assert!(!durable.contains(forbidden));
    }
}

#[tokio::test]
async fn http_mcp_computer_app_resource_protocol_failure_is_audited_without_content() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let mut params = mcp_2026_ui_params(json!({"uri": MCP_COMPUTER_UI_RESOURCE_URI}));
    params["_meta"]["io.modelcontextprotocol/protocolVersion"] = Value::from("2099-01-01");

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, MCP_COMPUTER_UI_RESOURCE_URI, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 20601,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&response), StatusCode::BAD_REQUEST);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], MCP_UNSUPPORTED_PROTOCOL_VERSION);

    let (action, operation, status, http_status, summary): (String, String, String, i64, String) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT action_name, operation, status, http_status, summary_json FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap()
    };
    assert_eq!(action, "resourcesRead");
    assert_eq!(operation, "computer_app_resource_read");
    assert_eq!(status, "failed");
    assert_eq!(http_status, 400);
    let summary: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(summary["resource_uri"], MCP_COMPUTER_UI_RESOURCE_URI);
    assert_eq!(summary["resource_version"], "v11");
    assert_eq!(summary["protocol_era"], "validation_failed");
    assert_eq!(summary["ui_capability_present"], true);
    assert_eq!(
        summary["mcp_error_code"],
        Value::from(MCP_UNSUPPORTED_PROTOCOL_VERSION)
    );
    let durable = summary.to_string();
    for forbidden in ["HTML loaded", "content_base64", "surface_", "Waiting for"] {
        assert!(!durable.contains(forbidden));
    }
}

#[tokio::test]
async fn http_mcp_2026_validates_resource_name_header_before_resource_contract() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({"uri": "file:///demo.txt"}));

    let mut missing_name = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2061,
            "method": "resources/read",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_name), StatusCode::BAD_REQUEST);
    let missing_body: Value = missing_name.take_json().await.unwrap();
    assert_eq!(missing_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut unsupported = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, "file:///demo.txt", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2062,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&unsupported), StatusCode::BAD_REQUEST);
    let unsupported_body: Value = unsupported.take_json().await.unwrap();
    assert_eq!(unsupported_body["error"]["code"], -32602);
}

#[tokio::test]
async fn http_mcp_2026_rejects_legacy_lifecycle_and_cross_origin_transport() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));

    let mut initialize = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "initialize", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 207,
            "method": "initialize",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&initialize), StatusCode::NOT_FOUND);
    let initialize_body: Value = initialize.take_json().await.unwrap();
    assert_eq!(initialize_body["error"]["code"], -32601);

    let cross_origin = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header("origin", "https://attacker.example", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 208,
            "method": "tools/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&cross_origin), StatusCode::FORBIDDEN);

    let legacy_cross_origin = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("origin", "https://attacker.example", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2081,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&legacy_cross_origin),
        StatusCode::FORBIDDEN
    );

    let legacy_cross_origin_get = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("origin", "https://attacker.example", true)
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&legacy_cross_origin_get),
        StatusCode::FORBIDDEN
    );

    let modern_get = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&modern_get),
        StatusCode::METHOD_NOT_ALLOWED
    );

    let modern_delete = TestClient::delete("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&modern_delete),
        StatusCode::METHOD_NOT_ALLOWED
    );
}
#[tokio::test]
async fn http_mcp_tools_call_list_projects_returns_mcp_content() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "list_projects", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 3);
    assert_eq!(body["result"]["content"][0]["type"], "text");
    assert!(body["result"]["content"][0]["text"].is_string());
    assert!(body["result"]["structuredContent"].is_object());
    assert!(
        body["result"]["structuredContent"]["success"].is_boolean(),
        "structuredContent.success must be a bool"
    );
    assert!(
        body["result"]["isError"].is_boolean(),
        "isError must be a bool"
    );
    // A business failure (no projects configured) is an MCP tool error,
    // not a JSON-RPC protocol error: the envelope is still a result.
    assert!(body["result"].get("error").is_none());
    assert!(body.get("error").is_none(), "no top-level JSON-RPC error");
}

#[tokio::test]
async fn http_mcp_tools_call_unknown_tool_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
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
        .unwrap()
        .contains("no_such_tool"));
}

#[tokio::test]
async fn http_mcp_unknown_method_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("resources/list"));
}

#[tokio::test]
async fn http_mcp_invalid_jsonrpc_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "1.0",
            "id": 6,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32600);
    assert_eq!(body["id"], 6);
}

#[tokio::test]
async fn http_mcp_without_bearer_is_unauthorized() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_mcp_with_wrong_bearer_is_unauthorized() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("wrong-token")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_mcp_with_correct_bearer_succeeds() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "ping",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 9);
    assert!(body["result"].is_object());
}
#[tokio::test]
async fn http_mcp_notification_returns_accepted_with_empty_body() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::ACCEPTED);
    let text = resp.take_string().await.unwrap();
    assert!(text.is_empty(), "notification response body must be empty");
}

#[tokio::test]
async fn http_mcp_get_discovery_returns_metadata() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["name"], "webcodex");
    assert!(body["version"].is_string());
    assert_eq!(body["protocol"], "mcp");
    assert_eq!(
        body["modelSurface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
    assert!(body["protocolVersion"].is_string());
    assert_eq!(body["endpoint"], "/mcp");
    let methods = body["methods"].as_array().unwrap();
    let method_names: Vec<String> = methods
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    assert!(method_names.contains(&"initialize".to_string()));
    assert!(method_names.contains(&"tools/list".to_string()));
    assert!(method_names.contains(&"tools/call".to_string()));
    assert!(method_names.contains(&"notifications/initialized".to_string()));
    assert_eq!(body["auth"]["type"], "bearer");
    assert_eq!(body["auth"]["required"], true);
    assert_eq!(
        body["auth"]["header"],
        "Authorization: Bearer <shared_key_or_wc_pat>"
    );
    let auth_json = body["auth"].to_string();
    assert!(
        auth_json.contains("shared_key_or_wc_pat"),
        "MCP auth metadata must advertise shared key or wc_pat bearer use: {auth_json}"
    );
    assert!(
        !auth_json.contains("wc_pat_user_api_token"),
        "MCP auth metadata must not regress to PAT-only placeholder: {auth_json}"
    );
}
