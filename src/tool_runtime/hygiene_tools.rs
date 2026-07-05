//! Runtime dispatch adapter for workspace hygiene tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_hygiene_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                session_id,
            } => {
                self.workspace_hygiene_check(project, max_findings, include_tracked, session_id)
                    .await
            }
            _ => unreachable!("non-hygiene tool routed to hygiene dispatcher"),
        }
    }
}
