//! Authorized workflows routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn dispatch_workflows_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
        correlation: &mut crate::tool_runtime::window_activity::ToolCallCorrelation,
        bootstrap_context: &mut Option<crate::tool_runtime::coding_task::BootstrapContext>,
    ) -> ToolResult {
        match call {
            call @ (ToolCall::StartSession { .. }
            | ToolCall::ListSessions { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::UpdateSessionContext { .. }
            | ToolCall::CloseSession { .. }
            | ToolCall::ValidationSummary { .. }
            | ToolCall::RecordExternalObservation { .. }
            | ToolCall::ListExternalObservations { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::GetSessionAssignment { .. }
            | ToolCall::ObserveSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::CompleteSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }) => {
                self.dispatch_session_tool(
                    call,
                    auth,
                    transport,
                    window,
                    trusted_recording_session_id,
                )
                .await
            }

            call @ (ToolCall::WorkOnProject { .. } | ToolCall::FinishCodingTask { .. }) => {
                // Startup/closeout aggregation retains relatively large typed workflow state.
                // Keep that future off the shared dispatch future so unrelated tool calls do
                // not inherit its stack cost as the coding startup contract evolves.
                Box::pin(self.dispatch_coding_task_tool(
                    call,
                    auth,
                    transport,
                    trusted_recording_session_id,
                    trusted_recording_session_project,
                    correlation,
                    bootstrap_context,
                ))
                .await
            }

            ToolCall::PresentDocx { project, path } => self.present_docx(project, path, auth).await,
            ToolCall::ReadDocxChunk {
                project,
                path,
                sha256,
                bytes,
                byte_offset,
            } => {
                self.read_docx_chunk(project, path, sha256, bytes, byte_offset, auth)
                    .await
            }

            ToolCall::PresentWorkResult {
                project,
                session_id,
            } => {
                self.present_work_result_for_window(project, session_id, auth, window)
                    .await
            }

            ToolCall::WorkResultState {
                project,
                session_id,
                files,
                automatic,
            } => {
                if let Some(files) = files {
                    self.work_result_files(project, session_id, files, auth)
                        .await
                } else {
                    self.work_result_state_for_window_with_refresh(
                        project, session_id, auth, window, automatic,
                    )
                    .await
                }
            }

            ToolCall::WorkResultActivityDetail {
                project,
                server_trace_id,
            } => {
                self.work_result_activity_detail(project, server_trace_id, auth, window)
                    .await
            }

            ToolCall::WorkResultSendMessage {
                project,
                session_id,
                message,
                delivery_key,
            } => {
                self.work_result_send_message(
                    project,
                    session_id,
                    message,
                    delivery_key,
                    auth,
                    window,
                )
                .await
            }

            ToolCall::ChangesFileDiff {
                project,
                session_id,
                snapshot_id,
                path,
            } => {
                self.changes_file_diff(project, session_id, snapshot_id, path, auth)
                    .await
            }

            call @ (ToolCall::SessionHandoffSummary { .. }
            | ToolCall::SessionHandoffState { .. }) => self.dispatch_handoff_tool(call, auth).await,

            call @ (ToolCall::ResolveWorkspace { .. }
            | ToolCall::UnregisterProjects { .. }
            | ToolCall::ListProjects { .. }
            | ToolCall::RegisterProject { .. }
            | ToolCall::UnregisterProject { .. }
            | ToolCall::CreateProject { .. }) => self.dispatch_project_tool(call, auth).await,
            _ => ToolResult::err("tool does not belong to the workflows dispatch family"),
        }
    }
}
