//! Runtime dispatch adapter for model-generated Codex patches and raw unified diffs.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_patch_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::ApplyPatch {
                project,
                patch,
                dry_run,
                matching_mode,
                session_id: _,
            } => {
                self.apply_patch(project, patch, dry_run, matching_mode)
                    .await
            }
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
