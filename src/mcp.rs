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

#[handler]
pub async fn mcp_info(res: &mut Response) {
    res.render(Json(json!({
        "name": "private-drop",
        "protocol": "mcp",
        "transport": "streamable-http-jsonrpc",
        "endpoint": "/mcp",
        "methods": ["initialize", "ping", "tools/list", "tools/call"],
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
    if request.jsonrpc.as_deref().unwrap_or("2.0") != "2.0" {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(rpc_error(request.id, -32600, "jsonrpc must be '2.0'")));
        return;
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
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(rpc_error(
                        id,
                        -32602,
                        format!("Invalid params: {}", e),
                    )));
                    return;
                }
            };
            let call = match ToolCall::from_tool_name(&params.name, params.arguments) {
                Ok(call) => call,
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(rpc_error(id, -32602, e)));
                    return;
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
            res.status_code(StatusCode::BAD_REQUEST);
            rpc_error(id, -32601, format!("Method not found: {}", request.method))
        }
    };
    res.render(Json(response));
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
