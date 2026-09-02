use crate::connector_runtime::ConnectorCallOutcome;
use crate::tool_runtime::ToolResult;
use serde_json::{json, Value};

pub(super) fn mcp_stateless_result(mut result: Value, cacheable: bool) -> Value {
    let Some(object) = result.as_object_mut() else {
        return result;
    };
    object
        .entry("resultType".to_string())
        .or_insert_with(|| Value::String("complete".to_string()));
    if cacheable {
        object
            .entry("ttlMs".to_string())
            .or_insert_with(|| Value::from(0));
        object
            .entry("cacheScope".to_string())
            .or_insert_with(|| Value::String("private".to_string()));
    }
    let meta = object
        .entry("_meta".to_string())
        .or_insert_with(|| json!({}));
    if let Some(meta_object) = meta.as_object_mut() {
        meta_object
            .entry("io.modelcontextprotocol/serverInfo".to_string())
            .or_insert_with(|| {
                json!({
                    "name": "webcodex",
                    "version": env!("CARGO_PKG_VERSION")
                })
            });
    }
    result
}

pub(super) fn connector_call_tool_result(outcome: ConnectorCallOutcome) -> Value {
    // Connector output follows the same MCP layering as Runtime tools: the body
    // is canonical in structuredContent and content.text is only a tiny fallback.
    let text = if outcome.ok {
        "WebCodex connector tool completed successfully.".to_string()
    } else {
        outcome
            .body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("WebCodex connector tool failed.")
            .to_string()
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": outcome.body,
        "isError": !outcome.ok
    })
}

pub(super) fn mcp_runtime_tool_result_fallback(result: ToolResult) -> Value {
    // `structuredContent` is the canonical machine-readable result. Repeating
    // that full JSON object in `content.text` doubles model context and echoes
    // metadata the caller already has. Keep text as a tiny fallback instead.
    let text = if result.success {
        "WebCodex tool completed successfully.".to_string()
    } else {
        result
            .error
            .clone()
            .unwrap_or_else(|| "WebCodex tool failed.".to_string())
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": {
            "success": result.success,
            "output": result.output,
            "error": result.error,
        },
        "isError": !result.success
    })
}

pub(super) fn rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

pub(super) fn rpc_error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

pub(super) fn rpc_error_with_data(
    id: Option<Value>,
    code: i64,
    message: impl Into<String>,
    data: Value,
) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
            "data": data,
        }
    })
}
