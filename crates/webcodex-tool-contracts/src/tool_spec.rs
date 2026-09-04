//! MCP-compatible runtime tool specification type.

use serde::Serialize;
use serde_json::Value;

/// Hard repository ceiling for model-facing ToolSpec and OpenAPI operation descriptions.
/// Descriptions may use the full budget when selection, authority, retry, continuation,
/// uncertainty, safety, or recovery semantics require it.
pub const MODEL_TOOL_DESCRIPTION_MAX_CHARS: usize = 600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub annotations: Value,
}
