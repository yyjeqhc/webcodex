use crate::tool_runtime::{bounded_tail, ToolResult, COMMAND_STDIO_TAIL_CHARS};
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

fn execution_failure_output(success: bool, output: &Value) -> Option<&Value> {
    let output = if output.get("execution").is_some() {
        &output["details"]
    } else {
        output
    };
    (!success
        && output["tool_failure"] == false
        && matches!(
            output["failure_kind"].as_str(),
            Some("command_exit_nonzero" | "timeout")
        ))
    .then_some(output)
}

/// Keep command logs in the structured output only. Match the complete known
/// tail block, not arbitrary error prose, so tool faults and recovery guidance
/// retain their original meaning. HTTP/API and retained runtime results are untouched.
fn project_execution_failure_logs(result: &mut ToolResult) {
    let compact = result.output.get("execution").is_some();
    let Some(output) = execution_failure_output(result.success, &result.output) else {
        return;
    };
    let (Some(stdout), Some(stderr), Some(error)) = (
        output["stdout_tail"].as_str(),
        output["stderr_tail"].as_str(),
        result.error.as_mut(),
    ) else {
        return;
    };
    let repeated = format!("stdout_tail:\n{stdout}\nstderr_tail:\n{stderr}\n");
    if let Some(start) = error.find(&repeated) {
        error.replace_range(
            start..start + repeated.len(),
            if compact {
                "Logs: see output.details.stdout_tail and output.details.stderr_tail.\n"
            } else {
                "Logs: see output.stdout_tail and output.stderr_tail.\n"
            },
        );
        // The canonical nonzero template points at the removed log block.
        *error = error.replace("inspect stderr/stdout above", "inspect the output logs");
    }
}

/// Some standard Hosts expose only content[0].text for isError results. Add
/// bounded diagnostics there without changing structured facts or recovery prose.
fn append_execution_failure_logs(text: &mut String, result: &ToolResult) {
    let Some(output) = execution_failure_output(result.success, &result.output) else {
        return;
    };
    for (name, truncated_name) in [
        ("stdout_tail", "stdout_truncated"),
        ("stderr_tail", "stderr_truncated"),
    ] {
        let Some(log) = output[name].as_str() else {
            continue;
        };
        let (tail, truncated) = bounded_tail(log, COMMAND_STDIO_TAIL_CHARS);
        let truncated = truncated || output[truncated_name] == true;
        if tail.is_empty() && !truncated {
            continue;
        }
        text.push_str("\n\n");
        text.push_str(name);
        if truncated {
            text.push_str(" [truncated]");
        }
        text.push_str(":\n");
        text.push_str(&tail);
    }
}

pub(super) fn mcp_runtime_tool_result_fallback_with_compat(
    mut result: ToolResult,
    text_json_compat: bool,
    presentation: McpToolResultPresentation,
) -> Value {
    project_execution_failure_logs(&mut result);
    // `structuredContent` is the canonical machine-readable result. Repeating
    // that full JSON object in `content.text` doubles model context, so the
    // compatibility copy is explicit opt-in rather than the default.
    let mut concise = if result.success {
        "WebCodex tool completed successfully.".to_string()
    } else {
        result
            .error
            .clone()
            .unwrap_or_else(|| "WebCodex tool failed.".to_string())
    };
    let success = result.success;
    let is_error = presentation.is_error(success);
    // JSON compatibility already carries the logs; the OpenAI presentation
    // exposes structuredContent without raising a Host exception.
    if is_error && !text_json_compat {
        append_execution_failure_logs(&mut concise, &result);
    }
    let structured = json!({
        "success": success,
        "output": result.output,
        "error": result.error,
    });
    let text = mcp_tool_text_content(&structured, concise, text_json_compat);
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": is_error
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
