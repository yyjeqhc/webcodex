//! Runtime dispatch adapters for coding-task workflow tool calls.

use super::{sessions, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_coding_task_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        match call {
            ToolCall::StartCodingTask {
                project,
                client_id,
                path,
                temporary_project_name,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                detail,
                resume_session_id,
                bind_current,
                new_session,
                execution_context,
            } => {
                self.start_coding_task(
                    project,
                    client_id,
                    path,
                    temporary_project_name,
                    title,
                    mode,
                    deny_write_tools,
                    deny_shell_tools,
                    detail,
                    resume_session_id,
                    bind_current,
                    new_session,
                    execution_context,
                    auth,
                    transport,
                    window,
                )
                .await
            }
            ToolCall::WorkOnProject {
                project,
                client_id,
                path,
                instruction,
                include_project_instructions,
                session_id,
            } => {
                self.work_on_project(
                    project,
                    client_id,
                    path,
                    instruction,
                    session_id,
                    include_project_instructions,
                    auth,
                    transport,
                    window,
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
