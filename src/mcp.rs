use crate::json_error;
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct McpToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

fn runtime(depot: &Depot) -> Option<Arc<ToolRuntime>> {
    depot.obtain::<Arc<ToolRuntime>>().ok().cloned()
}

/// Outcome of handling a single MCP JSON-RPC request.
///
/// Carries the JSON-RPC response body alongside the HTTP status the HTTP
/// wrapper should render. Keeping this separate from `Response` makes the
/// core protocol logic testable without a live server.
#[derive(Debug)]
enum McpOutcome {
    /// A normal JSON-RPC result. HTTP 200 with the body.
    Ok(Value),
    /// A JSON-RPC protocol error. HTTP 400 with the error body.
    BadRequest(Value),
    /// A JSON-RPC notification (request without an `id` member). Per the
    /// JSON-RPC 2.0 and MCP specs the server MUST NOT reply with a
    /// JSON-RPC response body. The HTTP wrapper acknowledges with 202 and
    /// an empty body.
    Notification,
}

#[handler]
pub async fn mcp_info(depot: &mut Depot, res: &mut Response) {
    let auth_required = crate::auth::get_config(depot)
        .map(|c| c.is_auth_enabled())
        .unwrap_or(false);
    res.render(Json(json!({
        "name": "private-drop",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "mcp",
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "transport": "streamable-http-jsonrpc",
        "endpoint": "/mcp",
        "methods": [
            "initialize",
            "ping",
            "tools/list",
            "tools/call",
            "notifications/initialized"
        ],
        "auth": {
            "type": "bearer",
            "required": auth_required,
            "header": "Authorization: Bearer <DROP_TOKEN>"
        }
    })));
}

#[handler]
pub async fn mcp_post(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let request: JsonRpcRequest = match req.parse_json().await {
        Ok(request) => request,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(rpc_error(None, -32700, format!("Parse error: {}", e))));
            return;
        }
    };
    match handle_mcp_request(&runtime, request).await {
        McpOutcome::Ok(body) => res.render(Json(body)),
        McpOutcome::BadRequest(body) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
        }
        McpOutcome::Notification => {
            // JSON-RPC notifications carry no `id`; the server MUST NOT reply
            // with a JSON-RPC body. Acknowledge with 202 and an empty body.
            res.status_code(StatusCode::ACCEPTED);
        }
    }
}

/// Core MCP JSON-RPC dispatch. Pure (no HTTP types) so it can be unit tested.
///
/// Business logic stays in `ToolRuntime`; this function only frames the
/// JSON-RPC envelope and translates tool results into MCP content blocks.
async fn handle_mcp_request(runtime: &ToolRuntime, request: JsonRpcRequest) -> McpOutcome {
    // A JSON-RPC request without an `id` member is a notification. Per the
    // JSON-RPC 2.0 and MCP specs the server MUST NOT reply with a JSON-RPC
    // response body, even if the method is unknown or malformed. We accept
    // the notification silently. `notifications/initialized` is the common
    // case sent by MCP clients after `initialize` completes.
    if request.id.is_none() {
        return McpOutcome::Notification;
    }

    if request.jsonrpc.as_deref().unwrap_or("2.0") != "2.0" {
        return McpOutcome::BadRequest(rpc_error(request.id, -32600, "jsonrpc must be '2.0'"));
    }

    let id = request.id.clone();
    let response = match request.method.as_str() {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "private-drop",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(
            id,
            json!({
                "tools": runtime.tool_specs()
            }),
        ),
        "tools/call" => {
            let params: McpToolCallParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(e) => {
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!("Invalid params: {}", e),
                    ));
                }
            };
            let call = match ToolCall::from_tool_name(&params.name, params.arguments) {
                Ok(call) => call,
                Err(e) => {
                    return McpOutcome::BadRequest(rpc_error(id, -32602, e));
                }
            };
            let result = runtime.dispatch(call).await;
            let text = serde_json::to_string_pretty(&json!({
                "success": result.success,
                "output": result.output.clone(),
                "error": result.error.clone(),
            }))
            .unwrap_or_else(|_| "{}".to_string());
            rpc_result(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ],
                    "structuredContent": {
                        "success": result.success,
                        "output": result.output,
                        "error": result.error,
                    },
                    "isError": !result.success
                }),
            )
        }
        "notifications/initialized" => rpc_result(id, json!({})),
        _ => {
            return McpOutcome::BadRequest(rpc_error(
                id,
                -32601,
                format!("Method not found: {}", request.method),
            ));
        }
    };
    McpOutcome::Ok(response)
}

fn rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn rpc_error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        let shell_clients = Arc::new(ShellClientRegistry::default());
        ToolRuntime::new(
            projects,
            shell_clients,
            Arc::new(crate::config::CodexConfig::default()),
        )
    }

    fn rpc(method: &str, id: Option<Value>, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            method: method.to_string(),
            params,
            id,
        }
    }

    #[test]
    fn rpc_result_envelope_is_valid() {
        let value = rpc_result(Some(Value::from(1)), json!({"ok": true}));
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["ok"], true);
    }

    #[test]
    fn rpc_error_envelope_carries_code_and_message() {
        let value = rpc_error(Some(Value::from("a")), -32601, "missing");
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], "a");
        assert_eq!(value["error"]["code"], -32601);
        assert_eq!(value["error"]["message"], "missing");
    }

    #[tokio::test]
    async fn mcp_initialize_returns_protocol_and_server_info() {
        let runtime = test_runtime();
        let outcome =
            handle_mcp_request(&runtime, rpc("initialize", Some(Value::from(1)), json!({}))).await;
        match outcome {
            McpOutcome::Ok(value) => {
                assert_eq!(value["jsonrpc"], "2.0");
                assert_eq!(value["id"], 1);
                assert_eq!(value["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
                assert_eq!(value["result"]["serverInfo"]["name"], "private-drop");
                assert!(value["result"]["serverInfo"]["version"].is_string());
                assert_eq!(
                    value["result"]["capabilities"]["tools"]["listChanged"],
                    false
                );
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_ping_returns_empty_result() {
        let runtime = test_runtime();
        let outcome =
            handle_mcp_request(&runtime, rpc("ping", Some(Value::from(2)), json!({}))).await;
        match outcome {
            McpOutcome::Ok(value) => {
                assert_eq!(value["id"], 2);
                assert!(value["result"].is_object());
                assert!(value["result"].as_object().unwrap().is_empty());
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_returns_same_names_as_runtime() {
        let runtime = test_runtime();
        let outcome =
            handle_mcp_request(&runtime, rpc("tools/list", Some(Value::from(3)), json!({}))).await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        let runtime_names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, runtime_names);
        // Each tool entry must carry MCP-required fields.
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
        }
    }

    #[tokio::test]
    async fn mcp_tools_call_list_projects_returns_content_blocks() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(4)),
                json!({"name": "list_projects", "arguments": {}}),
            ),
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        assert_eq!(value["id"], 4);
        assert!(value["result"]["content"].is_array());
        assert_eq!(value["result"]["content"][0]["type"], "text");
        assert!(value["result"]["content"][0]["text"].is_string());
        assert!(value["result"]["structuredContent"].is_object());
        // list_projects with no config -> success false, isError true.
        assert_eq!(value["result"]["isError"], true);
    }

    #[tokio::test]
    async fn mcp_tools_call_unknown_tool_is_bad_request() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(5)),
                json!({"name": "no_such_tool", "arguments": {}}),
            ),
        )
        .await;
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32602);
                assert!(value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("no_such_tool"));
            }
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_unknown_method_is_bad_request() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("resources/list", Some(Value::from(6)), json!({})),
        )
        .await;
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32601);
                assert!(value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("resources/list"));
            }
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_rejects_non_2_0_jsonrpc() {
        let runtime = test_runtime();
        let request = JsonRpcRequest {
            jsonrpc: Some("1.0".to_string()),
            method: "initialize".to_string(),
            params: json!({}),
            id: Some(Value::from(7)),
        };
        let outcome = handle_mcp_request(&runtime, request).await;
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32600);
                assert_eq!(value["id"], 7);
            }
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_notification_without_id_yields_no_response_body() {
        // A notification (request without an `id` member) must not produce a
        // JSON-RPC response body. This covers `notifications/initialized`
        // which MCP clients send after `initialize` completes.
        let runtime = test_runtime();
        let request = JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            method: "notifications/initialized".to_string(),
            params: json!({}),
            id: None,
        };
        let outcome = handle_mcp_request(&runtime, request).await;
        assert!(
            matches!(outcome, McpOutcome::Notification),
            "expected Notification, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn mcp_notification_unknown_method_also_silent() {
        // Any id-less request is a notification and is accepted silently,
        // even if the method is not recognized.
        let runtime = test_runtime();
        let request = JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            method: "notifications/cancelled".to_string(),
            params: json!({}),
            id: None,
        };
        let outcome = handle_mcp_request(&runtime, request).await;
        assert!(matches!(outcome, McpOutcome::Notification));
    }

    #[tokio::test]
    async fn mcp_notifications_initialized_with_id_returns_result() {
        // If a client (incorrectly) sends notifications/initialized with an
        // id, we still treat it as a normal request and return a result.
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("notifications/initialized", Some(Value::from(9)), json!({})),
        )
        .await;
        match outcome {
            McpOutcome::Ok(value) => {
                assert_eq!(value["id"], 9);
                assert!(value["result"].is_object());
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_parity_with_rest_tools_list() {
        // MCP tools/list and REST /api/tools/list both expose the exact same
        // tool names because both call ToolRuntime::tool_specs().
        let runtime = test_runtime();
        let mcp_outcome =
            handle_mcp_request(&runtime, rpc("tools/list", Some(Value::from(8)), json!({}))).await;
        let mcp_value = match mcp_outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let mcp_names: Vec<String> = mcp_value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        let rest_names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(mcp_names, rest_names);
    }

    // =========================================================================
    // HTTP integration tests — exercise the real Salvo router + AuthMiddleware.
    // These do not start a real server; they build a Router, wrap it in a
    // Service, and dispatch TestClient requests through it.
    // =========================================================================

    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use std::path::PathBuf;

    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        })
    }

    /// Create an empty Database in a temp dir. The TempDir must be kept alive
    /// for the lifetime of the returned Database so the sqlite file is not
    /// deleted mid-test.
    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    /// Build a minimal Router matching the production /mcp wiring: Config,
    /// Database, and ToolRuntime are injected so AuthMiddleware and mcp_post
    /// resolve state exactly as in `main.rs`.
    fn build_test_router(
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        runtime: Arc<ToolRuntime>,
    ) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("mcp")
                    .hoop(crate::AuthMiddleware)
                    .get(mcp_info)
                    .post(mcp_post),
            )
    }

    /// Effective HTTP status: the explicitly set status_code, or OK when the
    /// handler only rendered a body (Salvo defaults Json bodies to 200).
    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    #[tokio::test]
    async fn http_mcp_initialize_success() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let runtime = Arc::new(test_runtime());
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
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], 1);
        assert_eq!(body["result"]["serverInfo"]["name"], "private-drop");
        assert!(body["result"]["protocolVersion"].is_string());
        assert_eq!(
            body["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }

    #[tokio::test]
    async fn http_mcp_tools_list_success() {
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
        }
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
        let runtime = Arc::new(test_runtime());
        let service = Service::new(build_test_router(config, db, runtime));
        let mut resp = TestClient::get("http://localhost/mcp")
            .bearer_auth("secret")
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["name"], "private-drop");
        assert!(body["version"].is_string());
        assert_eq!(body["protocol"], "mcp");
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
    }
}
