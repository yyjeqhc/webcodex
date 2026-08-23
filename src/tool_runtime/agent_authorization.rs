use super::project_resolution::ResolvedProject;
use super::tool_definition::{runtime_tool_agent_capability, AgentCapability};
use super::{ProjectResolverError, RecoveryKind, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::{
    SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL, SHELL_CLIENT_CAPABILITY_SSH_SHELL,
};
use serde_json::json;

/// The capability an agent-backed tool variant requires from the agent
/// client. Non-agent tools (and tools without a project) require nothing.
pub(crate) fn required_agent_capability(call: &ToolCall) -> Option<AgentCapability> {
    runtime_tool_agent_capability(call.tool_name())
}

impl ToolRuntime {
    /// Enforce the owner boundary and capability requirements for agent-backed
    /// runtime tools before dispatching. This is the single place where the
    /// runtime paths (`/api/tools/call`, `/api/projects/*`, `/mcp`) check that
    /// the caller is allowed to drive an agent.
    /// `/api/shell/*` handlers keep their own `assert_shell_client_owner`
    /// checks; this method closes the gap for the runtime paths.
    ///
    /// Returns `Ok(())` for project-less tools so they are unaffected.
    pub(crate) async fn authorize_agent_tool(
        &self,
        call: &ToolCall,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
        project_resolution: Option<&Result<ResolvedProject, ProjectResolverError>>,
    ) -> Result<(), ToolResult> {
        let Some(project) = call.project() else {
            return Ok(());
        };
        let required = required_agent_capability(call);
        if required.is_none() && ssh_resource.is_none() {
            return Ok(());
        }
        let resolved;
        let proj = match project_resolution {
            Some(Ok(project)) => &project.config,
            Some(Err(error)) => return Err(error.clone().into_tool_result()),
            None => {
                resolved = self
                    .resolve_project_for_auth(project, auth)
                    .await
                    .map_err(ProjectResolverError::into_tool_result)?;
                &resolved
            }
        };
        if matches!(
            call,
            ToolCall::RunProcess { .. }
                | ToolCall::RunDetachedProcess { .. }
                | ToolCall::RunScript { .. }
        ) && ssh_resource.is_some()
        {
            let (tool, representation) = if matches!(call, ToolCall::RunScript { .. }) {
                ("run_script", "typed script payloads")
            } else if matches!(call, ToolCall::RunDetachedProcess { .. }) {
                ("run_detached_process", "detached native argv ownership")
            } else {
                ("run_process", "native argv boundaries")
            };
            return Err(ToolResult::err_with_output(
                format!(
                    "{tool} is unavailable for named Session SSH resources because the current SSH transport cannot preserve {representation}; execution was not started. Use run_shell explicitly for remote shell semantics."
                ),
                json!({
                    "command_started": false,
                    "command_completed": false,
                    "command_ok": false,
                    "exit_code": null,
                    "execution_state": "not_started",
                    "failure_kind": "unsupported_resource",
                    "tool_failure": true,
                }),
            ));
        }
        if !proj.is_agent() {
            if ssh_resource.is_some() {
                return Err(ToolResult::err(
                    "ssh_resource_requires_agent_project: SSH resources require a project owned by a connected Runner"
                        .to_string(),
                ));
            }
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
        if let Some(required) = required {
            if !required.is_owner_only() {
                // Capability check via the registry helper so the requirement is
                // expressed as a named capability, not a raw struct field access.
                let mut supported = false;
                for capability in required.registry_capabilities() {
                    if self
                        .shell_clients
                        .client_supports_for_auth(&client_id, capability, auth)
                        .await
                        .map_err(ToolResult::err)?
                    {
                        supported = true;
                        break;
                    }
                }
                if !supported {
                    let message = format!(
                        "agent client {} does not support {}",
                        client_id,
                        required.label()
                    );
                    if matches!(
                        required,
                        AgentCapability::LspReadOnlyNavigation | AgentCapability::LspCallHierarchy
                    ) {
                        return Err(ToolResult::err(format!(
                            "{}: {}",
                            crate::lsp_bridge::error_codes::AGENT_CAPABILITY_UNAVAILABLE,
                            message
                        )));
                    }
                    if matches!(required, AgentCapability::PersistentShell) {
                        return Err(ToolResult::err(format!(
                            "agent_capability_unavailable: {}",
                            message
                        )));
                    }
                    if matches!(
                        required,
                        AgentCapability::StructuredProcess
                            | AgentCapability::DetachedProcess
                            | AgentCapability::StructuredScript
                    ) {
                        let noun = if matches!(required, AgentCapability::StructuredScript) {
                            "script"
                        } else if matches!(required, AgentCapability::DetachedProcess) {
                            "detached process"
                        } else {
                            "process"
                        };
                        return Err(ToolResult::err_with_output(
                            format!(
                                "capability_unavailable: agent client {} does not support {}; no {noun} was started and no shell fallback was attempted",
                                client_id,
                                required.label()
                            ),
                            json!({
                                "error_kind": "capability_unavailable",
                                "command_started": false,
                                "command_completed": false,
                                "command_ok": false,
                                "exit_code": null,
                                "execution_state": "not_started",
                                "failure_kind": "capability_unavailable",
                                "tool_failure": true,
                            }),
                        )
                        .with_recovery(RecoveryKind::NoAction, None));
                    }
                    return Err(ToolResult::err(message));
                }
            }
        }
        if ssh_resource.is_some() {
            // Every SSH-resource request routes to a Runner-local SSH
            // connection pool (`run_ssh_shell` / `start_ssh_shell_job`).
            // `ssh_shell` was introduced together with SSH resource routing in
            // the same change, so it is both necessary and sufficient to
            // guarantee a legacy Runner understands SSH resources instead of
            // silently executing the command on the Runner host.
            if !self
                .shell_clients
                .client_supports_for_auth(&client_id, SHELL_CLIENT_CAPABILITY_SSH_SHELL, auth)
                .await
                .map_err(ToolResult::err)?
            {
                return Err(ToolResult::err(format!(
                    "agent_capability_unavailable: agent client {} does not support {}",
                    client_id, SHELL_CLIENT_CAPABILITY_SSH_SHELL
                )));
            }
            // Only `OpenSessionShell` inherits the Session SSH resource at this
            // pre-dispatch authorization point. Its ordinary `persistent_shell`
            // capability is already enforced through the tool definition; the
            // SSH-specific capability is the additional fail-closed gate for a
            // Runner predating SSH persistent shells. Later exec/status/close
            // operations route by the shell record created during open.
            if matches!(call, ToolCall::OpenSessionShell { .. })
                && !self
                    .shell_clients
                    .client_supports_for_auth(
                        &client_id,
                        SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL,
                        auth,
                    )
                    .await
                    .map_err(ToolResult::err)?
            {
                return Err(ToolResult::err(format!(
                    "agent_capability_unavailable: agent client {} does not support {}",
                    client_id, SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL
                )));
            }
        }
        Ok(())
    }
}
