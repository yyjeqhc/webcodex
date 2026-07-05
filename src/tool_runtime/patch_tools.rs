//! Runtime dispatch adapters for patch tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_patch_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::ApplyPatch {
                project,
                patch,
                session_id: _,
            } => self.apply_patch(project, patch).await,
            ToolCall::ApplyPatchChecked {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.apply_patch_checked(project, patch, deny_sensitive_paths)
                    .await
            }
            ToolCall::ValidatePatch {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.validate_patch(project, patch, deny_sensitive_paths)
                    .await
            }
            _ => unreachable!("non-patch tool routed to patch dispatcher"),
        }
    }
}
