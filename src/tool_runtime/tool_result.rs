//! Runtime tool execution result envelope.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub success: bool,
    /// Main payload - always a JSON object so both MCP and GPT Actions
    /// can forward it verbatim.
    pub output: Value,
    /// Optional human-readable error when success == false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(msg.into()),
        }
    }

    pub fn err_with_output(msg: impl Into<String>, output: Value) -> Self {
        Self {
            success: false,
            output,
            error: Some(msg.into()),
        }
    }
}
