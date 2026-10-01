//! Authorized execution routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_execution_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        ssh_resource: Option<&str>,
        validation_assertion_name: Option<&str>,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
        structured_handoff_max_secs: Option<u64>,
    ) -> ToolResult {
        match call {
            ToolCall::JobWriteInput {
                project,
                job_id,
                input_id,
                data,
                close,
            } => {
                self.job_write_input(project, job_id, input_id, data, close, auth)
                    .await
            }
            ToolCall::CodingAgentStart {
                project,
                provider_id,
                idempotency_key,
                instruction,
                context_session_id,
                config,
                timeout_secs,
                recording_session_id,
            } => {
                Box::pin(self.coding_agent_start(
                    project,
                    provider_id,
                    idempotency_key,
                    instruction,
                    config,
                    timeout_secs,
                    recording_session_id,
                    context_session_id,
                    auth,
                ))
                .await
            }
            ToolCall::CodingAgentObserve {
                run_id,
                after_observation_token,
                wait_secs,
            } => {
                self.coding_agent_observe(run_id, after_observation_token, wait_secs, auth)
                    .await
            }
            ToolCall::CodingAgentCancel { run_id } => self.coding_agent_cancel(run_id, auth).await,

            call @ (ToolCall::RunProcess { .. }
            | ToolCall::RunDetachedProcess { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }) => {
                self.dispatch_shell_tool(call, ssh_resource, validation_assertion_name, auth)
                    .await
            }

            ToolCall::ProjectBuild {
                project,
                session_id,
                cwd,
                adapter,
                scope,
                dependency_policy,
                timeout_secs,
            } => {
                self.project_build(
                    project,
                    session_id,
                    cwd,
                    adapter,
                    scope,
                    dependency_policy,
                    timeout_secs,
                    structured_handoff_max_secs,
                    ssh_resource.as_deref(),
                    auth,
                )
                .await
            }

            call @ (ToolCall::OpenSessionShell { .. }
            | ToolCall::SessionShellExec { .. }
            | ToolCall::SessionShellStatus { .. }
            | ToolCall::CloseSessionShell { .. }) => {
                self.dispatch_session_shell_tool(call, ssh_resource).await
            }

            call @ (ToolCall::ApplyPatch { .. } | ToolCall::ApplyUnifiedDiff { .. }) => {
                self.dispatch_patch_tool(call).await
            }

            ToolCall::RunSkillResource {
                skill_id,
                path,
                expected_definition_revision,
                expected_package_revision,
                args,
                session_id,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err("run_skill_resource requires a resolved Project")
                    }
                };
                self.run_skill_resource(
                    &project,
                    skill_id,
                    path,
                    expected_definition_revision,
                    expected_package_revision,
                    args,
                    cwd,
                    timeout_secs,
                    sync_wait_secs,
                    purpose,
                    session_id,
                    auth,
                )
                .await
            }
            _ => ToolResult::err("tool does not belong to the execution dispatch family"),
        }
    }
}
