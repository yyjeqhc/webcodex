use crate::auth::AuthContext;
use crate::json_error;
use crate::tool_request_trace::{
    estimate_json_bytes, jsonrpc_id_safe, new_trace_id, ToolRequestLifecycle,
};
use crate::tool_runtime::kernel::{
    ToolCallContext, ToolCallErrorStatus, ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::{registered_tool_specs, ToolRuntime, ToolSpec};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_RESERVED_SESSION_ID_FIELD: &str = "_session_id";

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

fn tool_name_from_params(params: &Value) -> Option<String> {
    params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// MCP tools/list payload. When `WEBCODEX_MCP_COMPACT_SCHEMAS=true`, omit
/// `outputSchema` only (name/description/inputSchema/annotations retained).
/// This is an A/B compatibility experiment — not a permanent API change.
fn mcp_tools_list_payload() -> Value {
    let compact = crate::config::mcp_compact_schemas_enabled();
    let tools: Vec<Value> = registered_tool_specs()
        .into_iter()
        .map(|spec| mcp_tool_spec_json(spec, compact))
        .collect();
    json!({ "tools": tools })
}

fn mcp_tool_spec_json(spec: ToolSpec, compact: bool) -> Value {
    if compact {
        json!({
            "name": spec.name,
            "description": spec.description,
            "inputSchema": spec.input_schema,
            "annotations": spec.annotations,
        })
    } else {
        // Match ToolSpec's camelCase serde so default behavior is unchanged.
        serde_json::to_value(spec).unwrap_or_else(|_| json!({}))
    }
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
    /// The HTTP request authenticated, but the OAuth2 bearer token lacks the
    /// delegated scope needed by this JSON-RPC method or tool.
    Forbidden {
        body: Value,
        required_scope: Option<&'static str>,
    },
}

#[handler]
pub async fn mcp_info(depot: &mut Depot, res: &mut Response) {
    let auth_required = crate::auth::get_config(depot)
        .map(|c| c.is_auth_enabled())
        .unwrap_or(false);
    res.render(Json(json!({
        "name": "webcodex",
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
            "header": "Authorization: Bearer <shared_key_or_wc_pat>"
        }
    })));
}

#[handler]
pub async fn mcp_post(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let mut guard = ToolRequestLifecycle::new("mcp", new_trace_id(), "-", "POST /mcp", None);
    guard.received();

    let Some(runtime) = runtime(depot) else {
        // Size unknown without building the json_error body for measurement.
        guard.response_serialized(500, None, Some(false), None, "error_runtime_missing");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        guard.handler_returned(500, None, Some(false), None, "error_runtime_missing");
        return;
    };
    let request: JsonRpcRequest = match req.parse_json().await {
        Ok(request) => request,
        Err(e) => {
            guard.set_jsonrpc_id("none");
            guard.parsed("parse_error");
            let body = rpc_error(None, -32700, format!("Parse error: {}", e));
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "parse_error");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "parse_error");
            return;
        }
    };

    guard.set_jsonrpc_id(jsonrpc_id_safe(request.id.as_ref()));
    guard.set_method(request.method.clone());
    let tool_name = if request.method == "tools/call" {
        tool_name_from_params(&request.params)
    } else {
        None
    };
    guard.set_tool_name(tool_name);
    guard.parsed("ok");

    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let outcome =
        handle_mcp_request_with_lifecycle(&runtime, request, auth.as_ref(), Some(&mut guard)).await;

    match outcome {
        McpOutcome::Ok(body) => {
            // Protocol success: valid JSON-RPC result envelope.
            // Tool success: only meaningful for tools/call (isError / structuredContent.success).
            let tool_success = body
                .get("result")
                .and_then(|r| r.get("structuredContent"))
                .and_then(|s| s.get("success"))
                .and_then(|v| v.as_bool());
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(200, estimated, Some(true), tool_success, "ok");
            res.render(Json(body));
            guard.handler_returned(200, estimated, Some(true), tool_success, "ok");
        }
        McpOutcome::BadRequest(body) => {
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "bad_request");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "bad_request");
        }
        McpOutcome::Forbidden {
            body,
            required_scope,
        } => {
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(403, estimated, Some(false), None, "forbidden");
            res.status_code(StatusCode::FORBIDDEN);
            let challenge = crate::auth::oauth_insufficient_scope_challenge(required_scope);
            if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                res.headers_mut().insert("www-authenticate", val);
            }
            res.render(Json(body));
            guard.handler_returned(403, estimated, Some(false), None, "forbidden");
        }
        McpOutcome::Notification => {
            // JSON-RPC notifications carry no `id`; the server MUST NOT reply
            // with a JSON-RPC body. Acknowledge with 202 and an empty body.
            // Empty body size is known (0) without JSON serialization.
            guard.response_serialized(202, Some(0), Some(true), None, "notification");
            res.status_code(StatusCode::ACCEPTED);
            guard.handler_returned(202, Some(0), Some(true), None, "notification");
        }
    }
}

/// Core MCP JSON-RPC dispatch. Pure (no HTTP types) so it can be unit tested.
///
/// Business logic stays in `ToolRuntime`; this function only frames the
/// JSON-RPC envelope and translates tool results into MCP content blocks.
/// Test-friendly wrapper: no lifecycle hooks.
#[cfg_attr(not(test), allow(dead_code))]
async fn handle_mcp_request(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&AuthContext>,
) -> McpOutcome {
    handle_mcp_request_with_lifecycle(runtime, request, auth, None).await
}

async fn handle_mcp_request_with_lifecycle(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&AuthContext>,
    mut lifecycle: Option<&mut ToolRequestLifecycle>,
) -> McpOutcome {
    let is_oauth2 = auth.is_some_and(|ctx| ctx.is_oauth_token());

    if is_oauth2
        && matches!(
            request.method.as_str(),
            "initialize" | "ping" | "tools/list" | "notifications/initialized"
        )
    {
        if let Some(outcome) = require_mcp_oauth_scope(auth, crate::auth::SCOPE_RUNTIME_READ) {
            return outcome;
        }
    }

    if is_oauth2
        && !matches!(
            request.method.as_str(),
            "initialize" | "ping" | "tools/list" | "tools/call" | "notifications/initialized"
        )
    {
        return oauth_forbidden(None, "OAuth2 access tokens cannot call unknown MCP methods");
    }

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
                    "name": "webcodex",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, mcp_tools_list_payload()),
        "tools/call" => {
            let mut params: McpToolCallParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(e) => {
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!("Invalid params: {}", e),
                    ));
                }
            };
            let session_id = strip_reserved_session_id(&mut params.arguments);
            // Emit dispatch_started only after params parse succeeds and before
            // ToolRuntime work begins.
            if let Some(lc) = lifecycle.as_deref_mut() {
                lc.set_tool_name(Some(params.name.clone()));
                lc.dispatch_started();
            }
            let outcome = runtime
                .call_tool_with_context(
                    KernelToolCallRequest {
                        tool_name: params.name.clone(),
                        arguments: params.arguments,
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: session_id.as_deref(),
                        auth,
                        record_oauth_scope_denials: false,
                    },
                )
                .await;
            let result = match outcome.error_status {
                Some(ToolCallErrorStatus::InsufficientScope {
                    required_scope,
                    description,
                }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    return oauth_forbidden(required_scope, description);
                }
                Some(ToolCallErrorStatus::InvalidArguments { message }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
                None => outcome
                    .result
                    .expect("tool kernel outcome without error must include result"),
            };
            debug_assert_eq!(outcome.success, result.success);
            if let Some(lc) = lifecycle.as_deref() {
                // Protocol layer produced a JSON-RPC result (not -32xxx).
                // Tool kernel success is independent (isError / structuredContent).
                let category = if result.success {
                    "success"
                } else {
                    "tool_error"
                };
                if result.success {
                    lc.dispatch_finished(true, Some(true), category);
                } else {
                    lc.dispatch_finished(true, Some(false), category);
                }
            }
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

fn require_mcp_oauth_scope(auth: Option<&AuthContext>, scope: &'static str) -> Option<McpOutcome> {
    let auth = auth?;
    if !auth.is_oauth_token() || auth.has_scope(scope) {
        return None;
    }
    Some(oauth_forbidden(
        Some(scope),
        format!("missing required scope: {}", scope),
    ))
}

fn strip_reserved_session_id(arguments: &mut Value) -> Option<String> {
    arguments
        .as_object_mut()
        .and_then(|obj| obj.remove(MCP_RESERVED_SESSION_ID_FIELD))
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn oauth_forbidden(
    required_scope: Option<&'static str>,
    description: impl Into<String>,
) -> McpOutcome {
    McpOutcome::Forbidden {
        body: crate::auth::oauth_insufficient_scope_body(description),
        required_scope,
    }
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

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new_for_tests()
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
        let outcome = handle_mcp_request(
            &runtime,
            rpc("initialize", Some(Value::from(1)), json!({})),
            None,
        )
        .await;
        match outcome {
            McpOutcome::Ok(value) => {
                assert_eq!(value["jsonrpc"], "2.0");
                assert_eq!(value["id"], 1);
                assert_eq!(value["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
                assert_eq!(value["result"]["serverInfo"]["name"], "webcodex");
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
            handle_mcp_request(&runtime, rpc("ping", Some(Value::from(2)), json!({})), None).await;
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
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(3)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        let runtime_names: Vec<String> = registered_tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, runtime_names);
        // Each tool entry must carry MCP-required fields.
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
            assert!(tool["outputSchema"].is_object());
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_default_retains_output_schema() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        let runtime = test_runtime();
        let outcome =
            handle_mcp_request(&runtime, rpc("tools/list", Some(json!(1)), json!({})), None).await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("expected Ok");
        };
        let tools = value["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
            assert!(
                tool["outputSchema"].is_object(),
                "default mode must keep outputSchema for {}",
                tool["name"]
            );
            assert!(tool["annotations"].is_object() || tool.get("annotations").is_some());
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_compact_omits_output_schema_only() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        let runtime = test_runtime();
        let outcome =
            handle_mcp_request(&runtime, rpc("tools/list", Some(json!(2)), json!({})), None).await;
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        let McpOutcome::Ok(value) = outcome else {
            panic!("expected Ok");
        };
        let tools = value["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        for tool in tools {
            assert!(tool["name"].is_string(), "{tool:?}");
            assert!(tool["description"].is_string(), "{tool:?}");
            assert!(tool["inputSchema"].is_object(), "{tool:?}");
            assert!(
                tool.get("outputSchema").is_none(),
                "compact mode must omit outputSchema for {}",
                tool["name"]
            );
            // First-version experiment keeps annotations to reduce variables.
            assert!(
                tool.get("annotations").is_some(),
                "compact mode keeps annotations for {}",
                tool["name"]
            );
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_compact_is_smaller_than_full_serialized() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        let full = serde_json::to_vec(&mcp_tools_list_payload()).expect("full serialize");
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        let compact = serde_json::to_vec(&mcp_tools_list_payload()).expect("compact serialize");
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        assert!(
            compact.len() < full.len(),
            "compact={} full={}",
            compact.len(),
            full.len()
        );
        // Guard against accidental total collapse (must still list many tools).
        assert!(
            compact.len() > 10_000,
            "compact unexpectedly tiny: {}",
            compact.len()
        );
    }

    #[tokio::test]
    async fn mcp_tools_call_still_returns_structured_content_under_compact_flag() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(3)),
                json!({"name": "list_projects", "arguments": {}}),
            ),
            None,
        )
        .await;
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        let McpOutcome::Ok(value) = outcome else {
            panic!("expected Ok, got {outcome:?}");
        };
        assert!(value["result"]["content"].is_array());
        assert!(value["result"]["structuredContent"].is_object());
        assert!(value["result"]["structuredContent"]["success"].is_boolean());
    }

    #[tokio::test]
    async fn session_tools_exposed_in_registry_and_mcp() {
        // tools/list outputSchema depends on WEBCODEX_MCP_COMPACT_SCHEMAS; take
        // the shared env lock so parallel compact-schema tests cannot strip it.
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        let runtime = test_runtime();
        let specs = registered_tool_specs();
        let registry_names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        assert!(registry_names.contains(&"start_session"));
        assert!(registry_names.contains(&"session_summary"));
        assert!(registry_names.contains(&"validation_summary"));
        assert!(registry_names.contains(&"bind_current_session"));
        assert!(registry_names.contains(&"current_session"));
        assert!(registry_names.contains(&"unbind_current_session"));

        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(31)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let names: Vec<String> = value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.iter().any(|name| name == "start_session"));
        assert!(names.iter().any(|name| name == "session_summary"));
        assert!(names.iter().any(|name| name == "validation_summary"));
        assert!(names.iter().any(|name| name == "bind_current_session"));
        assert!(names.iter().any(|name| name == "current_session"));
        assert!(names.iter().any(|name| name == "unbind_current_session"));
        let tools = value["result"]["tools"].as_array().unwrap();
        let start_session = tools
            .iter()
            .find(|tool| tool["name"] == "start_session")
            .expect("missing MCP start_session tool");
        assert_eq!(
            start_session["inputSchema"]["properties"]["mode"]["enum"],
            json!(["normal", "read_only"])
        );
        assert!(start_session["inputSchema"]["properties"]
            .get("deny_write_tools")
            .is_some());
        assert!(start_session["inputSchema"]["properties"]
            .get("deny_shell_tools")
            .is_some());
        assert!(
            start_session["outputSchema"]["properties"]["output"]["properties"]
                .get("guards")
                .is_some()
        );
        let tool_description = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing MCP tool {name}"))["description"]
                .as_str()
                .unwrap()
                .to_lowercase()
        };
        assert!(tool_description("start_session").contains("explicit wc_sess_* session_id"));
        assert!(tool_description("session_summary").contains("session ledger"));
        assert!(tool_description("validation_summary").contains("does not run cargo"));
        assert!(tool_description("session_handoff_summary")
            .contains("does not depend on current-session binding"));
        for name in [
            "bind_current_session",
            "current_session",
            "unbind_current_session",
        ] {
            let description = tool_description(name);
            assert!(
                description.contains("process-local in-memory")
                    && description.contains("not the durable session ledger"),
                "MCP {name} description should distinguish current binding from ledger: {description}"
            );
        }
        let bind_current = tools
            .iter()
            .find(|tool| tool["name"] == "bind_current_session")
            .expect("missing MCP bind_current_session tool");
        assert!(bind_current["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "project"));
        assert!(bind_current["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "session_id"));
        let validation_summary = tools
            .iter()
            .find(|tool| tool["name"] == "validation_summary")
            .expect("missing MCP validation_summary tool");
        assert_eq!(
            validation_summary["inputSchema"]["required"],
            json!(["project", "session_id"])
        );
        assert_eq!(
            validation_summary["inputSchema"]["additionalProperties"],
            false
        );
        for name in ["read_file", "run_shell", "write_project_file"] {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing MCP tool {name}"));
            assert!(
                tool["inputSchema"]["properties"]
                    .get("session_id")
                    .is_some(),
                "MCP tools/list schema missing session_id for {name}"
            );
            assert!(
                !tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|field| field == "session_id"),
                "MCP tools/list must not require session_id for {name}"
            );
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
            None,
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
        // No server-side project config is normal; without registered agents,
        // list_projects succeeds with an empty project array.
        assert_eq!(value["result"]["isError"], false);
    }

    #[tokio::test]
    async fn mcp_tools_call_strips_reserved_session_id_before_dispatch() {
        let runtime = test_runtime();
        let session = runtime
            .sessions
            .start_session(Some("demo".to_string()), Some("mcp strip".to_string()));
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(32)),
                json!({
                    "name": "list_projects",
                    "arguments": {
                        MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                    }
                }),
            ),
            None,
        )
        .await;
        match outcome {
            McpOutcome::Ok(_) => {}
            other => panic!("expected Ok, got {:?}", other),
        }
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        let started = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_started")
            .unwrap();
        assert_eq!(started.transport, "mcp");
        assert_eq!(started.tool_name, "list_projects");
        assert!(
            !serde_json::to_string(&started.input_summary)
                .unwrap()
                .contains(MCP_RESERVED_SESSION_ID_FIELD),
            "_session_id must be stripped before recording/dispatch"
        );
    }

    #[tokio::test]
    async fn mcp_tools_call_records_event_with_session_id() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(33)),
                json!({
                    "name": "list_projects",
                    "arguments": {
                        MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                    }
                }),
            ),
            None,
        )
        .await;
        match outcome {
            McpOutcome::Ok(value) => {
                assert_eq!(value["result"]["structuredContent"]["success"], true);
            }
            other => panic!("expected Ok, got {:?}", other),
        }
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.succeeded, 1);
        let finished = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_finished")
            .unwrap();
        assert_eq!(finished.transport, "mcp");
        assert_eq!(finished.status.as_deref(), Some("succeeded"));
        assert_eq!(finished.risk_class, "read_only");
    }

    #[tokio::test]
    async fn mcp_show_changes_distinguishes_reserved_session_id_from_query_session_id() {
        use crate::shell_protocol::{
            ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
            ShellClientRegisterRequest,
        };

        let runtime = test_runtime();
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: "mcp-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "demo".to_string(),
                    name: Some("Demo".to_string()),
                    path: "/tmp/demo".to_string(),
                    allow_patch: true,
                    kind: None,
                    description: None,
                    hooks: vec![],
                    disabled: false,
                    git_branch: None,
                    git_head: None,
                    git_dirty: None,
                    updated_at: 0,
                    shell_profile: None,
                }]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let project = "agent:mcp-client:demo";
        let tracking_session = runtime
            .sessions
            .start_session(Some(project.to_string()), Some("track call".to_string()));
        let query_session = runtime
            .sessions
            .start_session(Some(project.to_string()), Some("query session".to_string()));
        let write_args = json!({"project": project, "path": "src/query.rs"});
        let start = runtime.sessions.record_tool_call_started(
            Some(&query_session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Mcp,
            "write_project_file",
            &write_args,
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
        let auth = AuthContext {
            kind: crate::auth::AuthKind::Bootstrap,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: Some("admin".to_string()),
            scopes: vec!["admin".to_string()],
            is_bootstrap: true,
            token_kind: None,
            allowed_client_id: None,
            shared_key_hash: None,
        };

        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(34)),
                json!({
                    "name": "show_changes",
                    "arguments": {
                        MCP_RESERVED_SESSION_ID_FIELD: &tracking_session.session_id,
                        "project": project,
                        "session_id": &query_session.session_id,
                        "include_diff": false
                    }
                }),
            ),
            Some(&auth),
        );
        let complete = async {
            let mut req = None;
            for _ in 0..50 {
                req = runtime
                    .shell_clients
                    .poll(ShellAgentPollRequest {
                        client_id: "mcp-client".to_string(),
                        agent_instance_id: "inst".to_string(),
                        projects: None,
                    })
                    .await
                    .unwrap();
                if req.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            let req = req.expect("show_changes should enqueue an agent shell request");
            let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
            runtime
                .shell_clients
                .complete(ShellAgentResultRequest {
                    client_id: "mcp-client".to_string(),
                    agent_instance_id: "inst".to_string(),
                    request_id: req.request_id,
                    exit_code: Some(0),
                    stdout: Some(stdout.to_string()),
                    stderr: Some(String::new()),
                    duration_ms: Some(1),
                    error: None,
                })
                .await
                .unwrap();
        };
        let (outcome, _) = tokio::join!(outcome, complete);
        let value = match outcome {
            McpOutcome::Ok(value) => value,
            other => panic!("expected Ok, got {:?}", other),
        };
        let output = &value["result"]["structuredContent"]["output"];
        assert_eq!(output["session"]["found"], true);
        assert_eq!(output["session"]["session_id"], query_session.session_id);
        assert_eq!(output["session"]["changed_paths"], json!(["src/query.rs"]));

        let tracking_summary = runtime
            .sessions
            .summary(&tracking_session.session_id, Some(10))
            .unwrap();
        assert!(tracking_summary
            .events
            .iter()
            .any(|event| event.tool_name == "show_changes"));
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
            None,
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
            None,
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
        let outcome = handle_mcp_request(&runtime, request, None).await;
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
        let outcome = handle_mcp_request(&runtime, request, None).await;
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
        let outcome = handle_mcp_request(&runtime, request, None).await;
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
            None,
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
        // registry-backed tool names.
        let runtime = test_runtime();
        let mcp_outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(8)), json!({})),
            None,
        )
        .await;
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
        assert!(
            !mcp_names.iter().any(|name| name == "run_codex"),
            "MCP tools/list must not include run_codex: {:?}",
            mcp_names
        );
        let rest_names: Vec<String> = registered_tool_specs()
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
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        })
    }

    fn test_config_oauth2(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config {
                enabled: true,
                access_token_ttl_secs: 3600,
                refresh_token_ttl_secs: 2_592_000,
                ..crate::OAuth2Config::default()
            },
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

    fn seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn seed_oauth_client(
        db: &crate::Database,
        user: &crate::models::UserRecord,
    ) -> crate::models::OAuthClientRecord {
        let now = chrono::Utc::now().timestamp();
        let secret = crate::auth::generate_oauth_client_secret();
        let record = crate::models::OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: crate::auth::hash_token(&secret),
            name: "Test App".to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read project:write job:run account:manage"
                .to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        record
    }

    fn seed_oauth_access_token(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        user: &crate::models::UserRecord,
        scopes: &str,
    ) -> String {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_access_token();
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: scopes.to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        plaintext
    }

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
        assert_eq!(body["result"]["serverInfo"]["name"], "webcodex");
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
            assert!(tool["outputSchema"].is_object());
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

    async fn oauth_mcp_request(
        service: &Service,
        token: &str,
        method: &str,
        params: Value,
    ) -> (StatusCode, Value, Option<String>) {
        let mut resp = TestClient::post("http://localhost/mcp")
            .bearer_auth(token)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 42,
                "method": method,
                "params": params,
            }))
            .send(service)
            .await;
        let status = effective_status(&resp);
        let challenge = resp
            .headers()
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.take_json::<Value>().await.unwrap();
        (status, body, challenge)
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
    async fn oauth2_mcp_tool_call_requires_project_read_for_read_file() {
        let (_tmp, service, token) = oauth_mcp_service("project:read");
        let (status, body, _) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            json!({"name": "read_file", "arguments": {"project": "demo", "path": "README.md"}}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = oauth_mcp_service("runtime:read");
        let (status, body, challenge) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            json!({"name": "read_file", "arguments": {"project": "demo", "path": "README.md"}}),
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
    async fn oauth2_mcp_tool_call_requires_project_write_for_anchor_edit_tools() {
        let (_tmp, service, token) = oauth_mcp_service("project:write");
        let (status, body, _) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            json!({
                "name": "replace_exact_block",
                "arguments": {
                    "project": "demo",
                    "path": "README.md",
                    "old_text": "old",
                    "new_text": "new"
                }
            }),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = oauth_mcp_service("project:read");
        let (status, body, challenge) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            json!({
                "name": "insert_after_pattern",
                "arguments": {
                    "project": "demo",
                    "path": "README.md",
                    "pattern": "anchor",
                    "text": "inserted\n"
                }
            }),
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
    async fn oauth2_mcp_unknown_tool_fails_closed() {
        let (_tmp, service, token) = oauth_mcp_service("runtime:read project:read");
        let (status, body, challenge) = oauth_mcp_request(
            &service,
            &token,
            "tools/call",
            json!({"name": "no_such_tool", "arguments": {}}),
        )
        .await;
        assert_mcp_oauth_scope_rejected(status, &body, challenge.as_deref(), None);
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
        assert_eq!(body["name"], "webcodex");
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

    // =========================================================================
    // runtime_status via MCP tools/list and tools/call
    // =========================================================================

    #[tokio::test]
    async fn mcp_tools_list_includes_runtime_status() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(10)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "runtime_status"),
            "MCP tools/list must include runtime_status: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn mcp_tools_list_exposes_coding_task_and_runtime_status_ux_flags() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(10)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let tool = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing MCP tool {name}"))
        };

        let finish_props = tool("finish_coding_task")["inputSchema"]["properties"]
            .as_object()
            .expect("finish_coding_task inputSchema properties");
        assert!(
            finish_props.contains_key("include_workspace"),
            "MCP finish_coding_task schema should expose include_workspace"
        );
        let finish_required = tool("finish_coding_task")["inputSchema"]["required"]
            .as_array()
            .expect("finish_coding_task required fields");
        assert!(
            !finish_required
                .iter()
                .any(|field| field.as_str() == Some("include_workspace")),
            "include_workspace must not be required in MCP schema"
        );

        let start_props = tool("start_coding_task")["inputSchema"]["properties"]
            .as_object()
            .expect("start_coding_task inputSchema properties");
        assert_eq!(start_props["tool_manifest_intent"]["type"], "string");

        let runtime_props = tool("runtime_status")["inputSchema"]["properties"]
            .as_object()
            .expect("runtime_status inputSchema properties");
        for field in ["compact", "summary_only"] {
            assert!(
                runtime_props.contains_key(field),
                "MCP runtime_status schema should expose {field}"
            );
            assert_eq!(runtime_props[field]["type"], "boolean");
        }

        let overview = tool("project_overview");
        let overview_props = overview["inputSchema"]["properties"]
            .as_object()
            .expect("project_overview inputSchema properties");
        for field in ["project", "path", "max_depth", "limit"] {
            assert!(
                overview_props.contains_key(field),
                "MCP project_overview schema should expose {field}"
            );
        }
        let overview_output = overview["outputSchema"]["properties"]["output"]["properties"]
            .as_object()
            .expect("project_overview outputSchema properties");
        for field in ["project_types", "key_files", "top_level", "scan"] {
            assert!(
                overview_output.contains_key(field),
                "MCP project_overview output schema should expose {field}"
            );
        }
    }

    #[tokio::test]
    async fn mcp_tools_list_includes_validate_patch() {
        // validate_patch is a patch preflight / dry-run tool exposed via MCP
        // tools/list (and a thin REST wrapper), but NOT via GPT Actions.
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(12)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "validate_patch"),
            "MCP tools/list must include validate_patch: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn mcp_tools_list_includes_show_changes() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(13)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "show_changes"),
            "MCP tools/list must include show_changes: {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n == "git_log"),
            "MCP tools/list must include git_log: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn mcp_tools_call_runtime_status_returns_content() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(11)),
                json!({"name": "runtime_status", "arguments": {}}),
            ),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        assert_eq!(value["id"], 11);
        // content blocks
        assert!(value["result"]["content"].is_array());
        assert_eq!(value["result"]["content"][0]["type"], "text");
        assert!(value["result"]["content"][0]["text"].is_string());
        // structuredContent carries the ToolResult shape
        assert!(value["result"]["structuredContent"].is_object());
        assert_eq!(value["result"]["structuredContent"]["success"], true);
        let out = &value["result"]["structuredContent"]["output"];
        assert_eq!(out["service"], "webcodex");
        assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
        // runtime_status never errors on a failed-projects runtime — it
        // reports configured=false instead.
        assert_eq!(value["result"]["isError"], false);
    }

    #[tokio::test]
    async fn mcp_tools_call_show_changes_returns_structured_tool_error() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(14)),
                json!({
                    "name": "show_changes",
                    "arguments": {"project": "agent:nope:nope"}
                }),
            ),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        assert_eq!(value["id"], 14);
        assert_eq!(value["result"]["isError"], true);
        assert_eq!(value["result"]["structuredContent"]["success"], false);
        assert_eq!(
            value["result"]["structuredContent"]["output"]["error_kind"],
            "unknown_project"
        );
    }

    #[tokio::test]
    async fn mcp_tools_list_includes_project_management_tools() {
        let runtime = test_runtime();
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(99)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok, got {:?}", other),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(
            names.contains(&"register_project"),
            "MCP tools/list must include register_project: {:?}",
            names
        );
        assert!(
            names.contains(&"create_project"),
            "MCP tools/list must include create_project: {:?}",
            names
        );
    }
}
