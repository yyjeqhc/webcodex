//! Runtime dispatch adapter for the canonical unified-diff mutation tool.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_patch_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::ApplyUnifiedDiff {
                project,
                diff,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.apply_unified_diff(project, diff, deny_sensitive_paths)
                    .await
            }
            _ => unreachable!("non-patch tool routed to patch dispatcher"),
        }
    }
}
