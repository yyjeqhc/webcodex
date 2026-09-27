use crate::tool_runtime::ToolResult;
use serde_json::{json, Value};

pub(super) const MCP_STATELESS_CACHE_TTL_MS: u64 = 0;
pub(super) const MCP_STATELESS_CACHE_SCOPE: &str = "private";

/// MCP presentation only; canonical ToolResult success and protocol errors are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpToolResultPresentation {
    Standard,
    OpenAiStructuredFailureCompat,
}

impl McpToolResultPresentation {
    pub(super) fn from_request_params(params: &Value) -> Self {
        match params
            .get("_meta")
            .and_then(|meta| meta.get("io.modelcontextprotocol/clientInfo"))
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str)
        {
            Some("openai-mcp") => Self::OpenAiStructuredFailureCompat,
            _ => Self::Standard,
        }
    }

    fn is_error(self, success: bool) -> bool {
        // The current OpenAI Host promotes isError into an exception that hides
        // structuredContent. Preserve the complete WebCodex-owned result as a
        // value, with business failure still authoritative in its success field.
        matches!(self, Self::Standard) && !success
    }
}

pub(super) fn mcp_complete_result(mut result: Value) -> Value {
    let Some(object) = result.as_object_mut() else {
        return result;
    };
    object
        .entry("resultType".to_string())
        .or_insert_with(|| Value::String("complete".to_string()));
    result
}

pub(super) fn mcp_stateless_result(result: Value, cacheable: bool) -> Value {
    let mut result = mcp_complete_result(result);
    let Some(object) = result.as_object_mut() else {
        return result;
    };
    if cacheable {
        object
            .entry("ttlMs".to_string())
            .or_insert_with(|| Value::from(MCP_STATELESS_CACHE_TTL_MS));
        object
            .entry("cacheScope".to_string())
            .or_insert_with(|| Value::String(MCP_STATELESS_CACHE_SCOPE.to_string()));
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

fn mcp_tool_text_content(structured: &Value, concise: String, text_json_compat: bool) -> String {
    if text_json_compat {
        serde_json::to_string(structured).unwrap_or(concise)
    } else {
        concise
    }
}

pub(super) fn mcp_runtime_tool_result_fallback_with_compat(
    result: ToolResult,
    text_json_compat: bool,
    presentation: McpToolResultPresentation,
) -> Value {
    // `structuredContent` is the canonical machine-readable result. Repeating
    // that full JSON object in `content.text` doubles model context, so the
    // compatibility copy is explicit opt-in rather than the default.
    let concise = if result.success {
        "WebCodex tool completed successfully.".to_string()
    } else {
        result
            .error
            .clone()
            .unwrap_or_else(|| "WebCodex tool failed.".to_string())
    };
    let success = result.success;
    let structured = json!({
        "success": success,
        "output": result.output,
        "error": result.error,
    });
    let text = mcp_tool_text_content(&structured, concise, text_json_compat);
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": presentation.is_error(success)
    })
}

pub(super) fn mcp_runtime_tool_result_fallback(
    result: ToolResult,
    text_json_compat: bool,
    presentation: McpToolResultPresentation,
) -> Value {
    mcp_runtime_tool_result_fallback_with_compat(result, text_json_compat, presentation)
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
