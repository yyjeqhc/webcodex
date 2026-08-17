use super::*;

fn build_connector_test_router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
    runtime: Arc<ToolRuntime>,
    project_root: &std::path::Path,
) -> Router {
    const PROJECT_GRANT_ID: &str = "wc_pgrant_3333333333333333";
    const PROJECT_CREDENTIAL: &str =
        "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let state_root = project_root
        .parent()
        .expect("connector test project parent")
        .join("connector-state");
    let connector = crate::connector_runtime::ConnectorRuntime::new(
        runtime.clone(),
        db.clone(),
        crate::connector_runtime::ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "demo".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:demo".to_string(),
            executor_root: project_root.to_string_lossy().to_string(),
            runs_root: state_root.join("runs").to_string_lossy().to_string(),
            results_root: state_root.join("results").to_string_lossy().to_string(),
            projects_dir: state_root
                .join("agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        crate::auth::ProjectCredentialVerifier::new(
            PROJECT_GRANT_ID.to_string(),
            PROJECT_CREDENTIAL,
        )
        .unwrap(),
    )
    .unwrap();
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(runtime))
        .hoop(affix_state::inject(
            crate::connector_runtime::ConnectorRuntimeSlot(Some(Arc::new(connector))),
        ))
        .push(
            Router::with_path("mcp")
                .hoop(crate::AuthMiddleware)
                .get(mcp_info)
                .post(mcp_post),
        )
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(crate::connector_runtime::http::routes())
                .push(Router::with_path("tools/call").post(crate::runtime_http::tools_call)),
        )
        .push(Router::with_path("openapi.json").get(crate::openapi::openapi_json))
}

#[tokio::test]
async fn http_project_connector_lists_and_dispatches_only_canonical_capabilities() {
    // The runtime surface is explicit below and the request path never
    // re-reads WEBCODEX_MCP_MODEL_SURFACE, so no env lock is needed.
    let config = test_config(Some("secret"));
    let (tmp, db) = test_db();
    let project = tmp.path().join("connector-project");
    crate::connector_runtime::tests::init_repo(&project);
    let user_token = "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::CanonicalConnector));
    let service = Service::new(build_connector_test_router(config, db, runtime, &project));

    let mut discovery = TestClient::get("http://localhost/mcp")
        .bearer_auth(user_token)
        .send(&service)
        .await;
    assert_eq!(effective_status(&discovery), StatusCode::OK);
    let discovery_body: Value = discovery.take_json().await.unwrap();
    assert_eq!(
        discovery_body["modelSurface"],
        crate::model_surface::MODEL_SURFACE_CANONICAL_CONNECTOR
    );

    let mut schema = TestClient::get("http://localhost/openapi.json")
        .send(&service)
        .await;
    assert_eq!(effective_status(&schema), StatusCode::OK);
    let schema_body: Value = schema.take_json().await.unwrap();
    assert_eq!(schema_body["paths"].as_object().unwrap().len(), 14);
    assert!(schema_body["paths"]
        .get("/api/connector/task/start")
        .is_some());
    assert!(schema_body["paths"]
        .get("/api/connector/code/navigate")
        .is_some());
    assert!(schema_body["paths"]
        .get("/api/connector/code/impact")
        .is_some());
    assert!(schema_body["paths"].get("/api/tools/call").is_none());
    let action_checks_schema = schema_body["paths"]["/api/connector/checks/run"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();
    let action_navigation_schema = schema_body["paths"]["/api/connector/code/navigate"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();
    let action_impact_schema = schema_body["paths"]["/api/connector/code/impact"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();

    let mut listed = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&listed), StatusCode::OK);
    let listed_body: Value = listed.take_json().await.unwrap();
    let names = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, crate::connector_runtime::surface::CAPABILITY_NAMES);
    let mcp_checks_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "checks_run")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_checks_schema, action_checks_schema);
    let mcp_navigation_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "code_navigate")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_navigation_schema, action_navigation_schema);
    let mcp_impact_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "code_impact")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_impact_schema, action_impact_schema);

    let mut missing_window = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": { "goal": "must not create an anonymous context" }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_window), StatusCode::BAD_REQUEST);
    let missing_window_body: Value = missing_window.take_json().await.unwrap();
    assert_eq!(missing_window_body["error"]["code"], -32600);
    assert!(missing_window_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("initialize"));

    let mut initialized = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&initialized), StatusCode::OK);
    let mcp_session_id = initialized
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("connector initialize session id")
        .to_string();
    let initialized_body: Value = initialized.take_json().await.unwrap();
    assert_eq!(
        initialized_body["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_CANONICAL_CONNECTOR
    );

    let mut action_started = TestClient::post("http://localhost/api/connector/task/start")
        .bearer_auth(user_token)
        .json(&json!({
            "goal": "exercise the Actions adapter",
            "mode": "read_only"
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&action_started), StatusCode::OK);
    let window_cookie = action_started
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("webcodex_window="))
        .and_then(|value| value.split(';').next())
        .expect("first connector call must mint a window cookie")
        .to_string();
    let action_body: Value = action_started.take_json().await.unwrap();
    assert_eq!(action_body["ok"], true);
    assert!(action_body["task_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_task_"));
    assert!(action_body.get("success").is_none());

    let mut action_continued = TestClient::post("http://localhost/api/connector/task/start")
        .bearer_auth(user_token)
        .add_header("cookie", &window_cookie, true)
        .json(&json!({
            "goal": "continue the Actions inspection",
            "mode": "read_only"
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&action_continued), StatusCode::OK);
    let continued_body: Value = action_continued.take_json().await.unwrap();
    assert_eq!(continued_body["task_id"], action_body["task_id"]);
    assert_eq!(continued_body["data"]["continuation"], "continued");

    let action_conversation_request = |conversation_id: &'static str, goal: &'static str| {
        TestClient::post("http://localhost/api/connector/task/start")
            .bearer_auth(user_token)
            .add_header("openai-conversation-id", conversation_id, true)
            .json(&json!({"goal": goal, "mode": "read_only"}))
    };
    let mut conversation_a = action_conversation_request("conversation-a", "conversation A work")
        .send(&service)
        .await;
    let conversation_a_body: Value = conversation_a.take_json().await.unwrap();
    let mut conversation_b = action_conversation_request("conversation-b", "conversation B work")
        .send(&service)
        .await;
    let conversation_b_body: Value = conversation_b.take_json().await.unwrap();
    assert_ne!(
        conversation_a_body["task_id"], conversation_b_body["task_id"],
        "one credential must not merge two hosted conversations"
    );
    let mut conversation_a_again =
        action_conversation_request("conversation-a", "conversation A follow-up")
            .send(&service)
            .await;
    let conversation_a_again_body: Value = conversation_a_again.take_json().await.unwrap();
    assert_eq!(
        conversation_a_again_body["task_id"],
        conversation_a_body["task_id"]
    );
    assert_eq!(
        conversation_a_again_body["data"]["continuation"],
        "continued"
    );

    let mut legacy = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth(user_token)
        .json(&json!({ "name": "runtime_status", "arguments": {} }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&legacy), StatusCode::FORBIDDEN);
    let legacy_body: Value = legacy.take_json().await.unwrap();
    assert!(legacy_body["error"]
        .as_str()
        .unwrap()
        .contains("canonical connector capabilities"));

    let mut started = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            &mcp_session_id,
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": { "goal": "inspect the project", "mode": "read_only" }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&started), StatusCode::OK);
    let started_body: Value = started.take_json().await.unwrap();
    assert_eq!(started_body["result"]["structuredContent"]["ok"], true);
    assert!(started_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_task_"));
    assert!(started_body["result"]["structuredContent"]
        .get("success")
        .is_none());

    let mut continued = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            &mcp_session_id,
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 211,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": {
                    "goal": "continue inspecting the project",
                    "mode": "read_only"
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&continued), StatusCode::OK);
    let continued_body: Value = continued.take_json().await.unwrap();
    assert_eq!(
        continued_body["result"]["structuredContent"]["task_id"],
        started_body["result"]["structuredContent"]["task_id"]
    );
    assert_eq!(
        continued_body["result"]["structuredContent"]["data"]["continuation"],
        "continued"
    );

    let mut hidden = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 22,
            "method": "tools/call",
            "params": { "name": "runtime_status", "arguments": {} }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&hidden), StatusCode::BAD_REQUEST);
    let hidden_body: Value = hidden.take_json().await.unwrap();
    assert_eq!(hidden_body["error"]["code"], -32602);
    assert!(hidden_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not available"));
}

#[tokio::test]
async fn http_project_connector_2026_uses_explicit_task_ids_without_transport_window_state() {
    // The runtime surface is explicit below and the request path never
    // re-reads WEBCODEX_MCP_MODEL_SURFACE, so no env lock is needed.
    let config = test_config(Some("secret"));
    let (tmp, db) = test_db();
    let project = tmp.path().join("connector-2026-project");
    crate::connector_runtime::tests::init_repo(&project);
    let user_token = "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::CanonicalConnector));
    let service = Service::new(build_connector_test_router(config, db, runtime, &project));

    let start_params = |goal: &str| {
        mcp_2026_params(json!({
            "name": "task_start",
            "arguments": { "goal": goal, "mode": "read_only" }
        }))
    };

    let mut first = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_start", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-not-bind-2026",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 220,
            "method": "tools/call",
            "params": start_params("inspect the stateless connector path")
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&first), StatusCode::OK);
    assert!(first
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .is_none());
    let first_body: Value = first.take_json().await.unwrap();
    let first_task_id = first_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .expect("2026 task_start must return task_id")
        .to_string();
    assert!(first_task_id.starts_with("wc_task_"));
    assert_eq!(first_body["result"]["resultType"], "complete");

    let mut second = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_start", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-not-bind-2026",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 221,
            "method": "tools/call",
            "params": start_params("start independent stateless work")
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&second), StatusCode::OK);
    let second_body: Value = second.take_json().await.unwrap();
    let second_task_id = second_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .expect("second 2026 task_start must return task_id");
    assert_ne!(
        second_task_id, first_task_id,
        "2026 must not derive hidden continuity from Mcp-Session-Id"
    );

    let mut resumed = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_resume", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 222,
            "method": "tools/call",
            "params": mcp_2026_params(json!({
                "name": "task_resume",
                "arguments": { "task_id": first_task_id }
            }))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resumed), StatusCode::OK);
    let resumed_body: Value = resumed.take_json().await.unwrap();
    assert_eq!(
        resumed_body["result"]["structuredContent"]["task_id"],
        first_task_id
    );
    assert_eq!(
        resumed_body["result"]["structuredContent"]["data"]["continuity"]["window_rebound"],
        false
    );
}
