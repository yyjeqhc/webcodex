//! Authorized job continuation routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_job_continuation_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        match call {
            ToolCall::PresentJobTerminalContinuation { wait_id } => {
                self.present_job_terminal_continuation(auth, wait_id).await
            }

            ToolCall::JobTerminalContinuationBind {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_bind_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationState {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_state_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationPrepare {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_prepare_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationFinish {
                wait_id,
                binding_id,
                attempt_id,
                outcome,
            } => {
                self.job_terminal_continuation_finish_for_window(
                    auth, window, wait_id, binding_id, attempt_id, outcome,
                )
                .await
            }

            ToolCall::JobTerminalContinuationUnbind {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_unbind_for_window(auth, window, wait_id, binding_id)
                    .await
            }
            _ => ToolResult::err("tool does not belong to the job_continuation dispatch family"),
        }
    }
}
