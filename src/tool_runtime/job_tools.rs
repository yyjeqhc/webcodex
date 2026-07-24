//! Runtime dispatch adapters for job tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_job_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::RunJob {
                project,
                command,
                session_id,
                timeout_secs,
                cwd,
            } => {
                self.run_job_for_auth(
                    project,
                    command,
                    session_id,
                    timeout_secs,
                    cwd,
                    Vec::new(),
                    auth,
                )
                .await
            }
            ToolCall::StopJob {
                project,
                job_id,
                session_id,
                confirm,
            } => {
                self.stop_job_model_facing(project, job_id, session_id, confirm, auth)
                    .await
            }
            ToolCall::JobStatus {
                job_id,
                include_command_preview,
            } => {
                self.job_status_for_auth(job_id, include_command_preview, auth)
                    .await
            }
            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => {
                self.job_log_for_auth(job_id, offset, tail_lines, auth)
                    .await
            }
            ToolCall::ListJobs { limit, status } => {
                self.list_jobs_for_auth(limit, status, auth).await
            }
            ToolCall::JobTail { job_id, tail_lines } => {
                self.job_tail_for_auth(job_id, tail_lines, auth).await
            }
            _ => unreachable!("non-job tool routed to job dispatcher"),
        }
    }
}
