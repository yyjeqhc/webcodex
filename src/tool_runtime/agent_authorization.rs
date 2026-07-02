use super::types::AgentCapability;
use super::{ProjectResolverError, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    /// The capability an agent-backed tool variant requires from the agent
    /// client. Non-agent tools (and tools without a project) require nothing.
    pub(crate) fn required_agent_capability(call: &ToolCall) -> Option<AgentCapability> {
        match call {
            ToolCall::RunShell { .. }
            | ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyPatchChecked { .. }
            | ToolCall::DeleteProjectFiles { .. }
            | ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. } => Some(AgentCapability::Shell),
            // validate_patch runs read-only `git apply --check`/`--stat` via
            // the agent shell path; it requires the same shell capability as
            // apply_patch but never mutates the worktree.
            ToolCall::ValidatePatch { .. } => Some(AgentCapability::Shell),
            // Fixed helper commands over the shell path. Line edits use native
            // agent file ops and require file_write instead.
            ToolCall::ReplaceInFile { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. } => Some(AgentCapability::Shell),
            ToolCall::ReplaceLineRange { .. }
            | ToolCall::InsertAtLine { .. }
            | ToolCall::DeleteLineRange { .. }
            | ToolCall::ApplyTextEdits { .. }
            | ToolCall::ReplaceExactBlock { .. }
            | ToolCall::InsertBeforePattern { .. }
            | ToolCall::InsertAfterPattern { .. } => Some(AgentCapability::FileWrite),
            ToolCall::ReadFile { .. } | ToolCall::ListProjectFiles { .. } => {
                Some(AgentCapability::FileRead)
            }
            // Search runs a bounded `grep` via the agent shell path.
            ToolCall::SearchProjectText { .. } => Some(AgentCapability::Shell),
            ToolCall::GitStatus { .. }
            | ToolCall::GitDiff { .. }
            | ToolCall::GitDiffHunks { .. }
            | ToolCall::GitLog { .. }
            | ToolCall::GitDiffSummary { .. }
            | ToolCall::ShowChanges { .. }
            | ToolCall::WorkspaceHygieneCheck { .. } => Some(AgentCapability::GitOrShell),
            ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. } => Some(AgentCapability::Shell),
            ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. } => Some(AgentCapability::OwnerOnly),
            ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. } => Some(AgentCapability::Shell),
            ToolCall::RunJob { .. } | ToolCall::RunCodex { .. } => Some(AgentCapability::AsyncJobs),
            ToolCall::ListTools
            | ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }
            | ToolCall::SessionHandoffSummary { .. }
            | ToolCall::BindCurrentSession { .. }
            | ToolCall::CurrentSession { .. }
            | ToolCall::UnbindCurrentSession { .. }
            | ToolCall::ListProjects
            | ToolCall::RegisterProject { .. }
            | ToolCall::CreateProject { .. }
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus
            | ToolCall::ToolManifest { .. }
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. } => None,
        }
    }

    /// Enforce the owner boundary and capability requirements for agent-backed
    /// runtime tools before dispatching. This is the single place where the
    /// runtime paths (`/api/tools/call`, `/api/codex/run`, `/api/projects/*`,
    /// `/mcp`) check that the caller is allowed to drive an agent. Legacy
    /// `/api/shell/*` handlers keep their own `assert_shell_client_owner`
    /// checks; this method closes the gap for the runtime paths.
    ///
    /// Returns `Ok(())` for local-executor projects and project-less tools so
    /// they are unaffected.
    pub(crate) async fn authorize_agent_tool(
        &self,
        call: &ToolCall,
        auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let Some(project) = call.project() else {
            return Ok(());
        };
        let required = match Self::required_agent_capability(call) {
            Some(cap) => cap,
            None => return Ok(()),
        };
        let proj = self
            .resolve_project_for_auth(project, auth)
            .await
            .map_err(ProjectResolverError::into_tool_result)?;
        if !proj.is_agent() {
            return Ok(());
        }
        let client_id = proj.agent_client_id().map_err(ToolResult::err)?.to_string();
        if self
            .shell_clients
            .get_client_view_for_auth(&client_id, auth)
            .await
            .is_none()
        {
            return Err(ToolResult::err(format!(
                "unknown shell client: {}",
                client_id
            )));
        }
        self.shell_clients
            .assert_client_access(auth, &client_id)
            .await
            .map_err(ToolResult::err)?;
        if matches!(required, AgentCapability::OwnerOnly) {
            return Ok(());
        }
        // Capability check via the registry helper so the requirement is
        // expressed as a named capability, not a raw struct field access.
        let supported = match required {
            AgentCapability::OwnerOnly => true,
            AgentCapability::Shell => self
                .shell_clients
                .client_supports(&client_id, "shell")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::FileRead => self
                .shell_clients
                .client_supports(&client_id, "file_read")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::FileWrite => self
                .shell_clients
                .client_supports(&client_id, "file_write")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::GitOrShell => {
                self.shell_clients
                    .client_supports(&client_id, "shell")
                    .await
                    .map_err(ToolResult::err)?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "git")
                        .await
                        .map_err(ToolResult::err)?
            }
            AgentCapability::AsyncJobs => {
                self.shell_clients
                    .client_supports(&client_id, "async_jobs")
                    .await
                    .map_err(ToolResult::err)?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "async_shell_jobs")
                        .await
                        .map_err(ToolResult::err)?
            }
        };
        if !supported {
            let label = match required {
                AgentCapability::OwnerOnly => "owner boundary",
                AgentCapability::Shell => "shell",
                AgentCapability::FileRead => "file_read",
                AgentCapability::FileWrite => "file_write",
                AgentCapability::GitOrShell => "shell or git",
                AgentCapability::AsyncJobs => "async shell jobs",
            };
            return Err(ToolResult::err(format!(
                "agent client {} does not support {}",
                client_id, label
            )));
        }
        Ok(())
    }
}
