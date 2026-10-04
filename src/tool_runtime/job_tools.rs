//! Runtime dispatch adapters for job tool calls.

use super::{ToolCall, ToolCallCorrelation, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    /// Resolve a Project anchor only from exact caller-authorized Job records.
    /// A mixed, projectless, invalid, or hidden set deliberately has no anchor.
    /// This is observability provenance only; it never grants Project authority.
    pub(crate) async fn common_job_activity_project_for_auth(
        &self,
        job_ids: &[String],
        auth: Option<&AuthContext>,
    ) -> Option<String> {
        let access = crate::runner_http::runner_access_from_auth(auth);
        let job_ids = job_ids.iter().map(String::as_str).collect::<Vec<_>>();
        self.runner_registry
            .common_job_project_for_auth(access.as_ref(), &job_ids)
            .await
    }

    fn bind_job_activity_project(&self, correlation: &mut ToolCallCorrelation, project: String) {
        correlation.resolved_project = Some(project.clone());
        if let Some(trace_id) = crate::tool_request_trace::current_active_trace_id() {
            self.window_activity.update(&trace_id, None, Some(&project));
        }
    }

    pub(crate) async fn dispatch_job_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        ssh_resource: Option<&str>,
        correlation: &mut ToolCallCorrelation,
    ) -> ToolResult {
        match call {
            ToolCall::RunJob {
                project,
                command,
                session_id,
                timeout_secs,
                cwd,
                purpose,
                shell,
            } => {
                self.run_job_for_auth_with_contract_with_ssh_resource(
                    project,
                    command,
                    session_id,
                    timeout_secs,
                    cwd,
                    Vec::new(),
                    auth,
                    purpose,
                    shell,
                    ssh_resource,
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
            ToolCall::ObserveJobs {
                items,
                tail_lines,
                wait_secs,
                wake_on,
                summary_only,
            } => {
                let summary_items = summary_only.then(|| items.clone());
                let mut result = self
                    .observe_jobs_for_auth(items, tail_lines, wait_secs, wake_on, auth)
                    .await;
                if let Some(items) = summary_items {
                    super::observe_jobs::summarize_observe_jobs_result(
                        &mut result,
                        &items,
                        tail_lines,
                    );
                }
                self.capture_job_audit_trace(correlation, "observe_jobs", &result, auth);
                result
            }
            ToolCall::WaitForJobReadiness {
                job_ids,
                mode,
                wait_secs,
            } => {
                if let Some(project) = self
                    .common_job_activity_project_for_auth(&job_ids, auth)
                    .await
                {
                    self.bind_job_activity_project(correlation, project);
                }
                let result = self
                    .wait_for_job_readiness(job_ids, mode, wait_secs, auth)
                    .await;
                self.capture_job_audit_trace(correlation, "wait_for_job_readiness", &result, auth);
                result
            }
            ToolCall::WaitForJobTerminal {
                job_id,
                idempotency_key,
            } => {
                self.wait_for_job_terminal(job_id, idempotency_key, auth)
                    .await
            }
            ToolCall::ListJobs {
                limit,
                status,
                project,
                session_id,
            } => {
                self.list_jobs_for_auth_with_filters(limit, status, project, session_id, auth)
                    .await
            }
            ToolCall::JobTail {
                job_id,
                tail_lines,
                after_observation_token,
                wait_secs,
            } => {
                self.job_tail_for_auth(job_id, tail_lines, auth, after_observation_token, wait_secs)
                    .await
            }
            _ => unreachable!("non-job tool routed to job dispatcher"),
        }
    }
}
