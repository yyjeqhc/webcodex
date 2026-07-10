//! Runtime dispatch adapters for coding-task workflow tool calls.

use super::{sessions, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_coding_task_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        match call {
            ToolCall::StartCodingTask {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                include_runtime_status,
                compact_startup,
                include_git,
                include_recent_commits,
                include_rules,
                include_tool_manifest,
                tool_manifest_intent,
                tool_manifest_categories,
                tool_manifest_limit,
                bind_current,
            } => {
                self.start_coding_task(
                    project,
                    title,
                    mode,
                    deny_write_tools,
                    deny_shell_tools,
                    include_runtime_status,
                    compact_startup,
                    include_git,
                    include_recent_commits,
                    include_rules,
                    include_tool_manifest,
                    tool_manifest_intent,
                    tool_manifest_categories,
                    tool_manifest_limit,
                    bind_current,
                    auth,
                    transport,
                )
                .await
            }
            ToolCall::FinishCodingTask {
                project,
                session_id,
                summary_only,
                include_diff,
                include_workspace,
                include_hygiene,
                include_handoff,
                include_validation_summary,
            } => {
                self.finish_coding_task(
                    project,
                    session_id,
                    summary_only,
                    include_diff,
                    include_workspace,
                    include_hygiene,
                    include_handoff,
                    include_validation_summary,
                    auth,
                )
                .await
            }
            _ => unreachable!("non-coding-task tool routed to coding-task dispatcher"),
        }
    }
}
