//! Runtime dispatch adapter for Codex tool calls.

use super::{tool_disabled_result_from_definition, ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_codex_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::RunCodex { .. } => tool_disabled_result_from_definition("run_codex")
                .expect("run_codex must be disabled by ToolDefinition policy"),
            _ => unreachable!("non-codex tool routed to codex dispatcher"),
        }
    }
}
