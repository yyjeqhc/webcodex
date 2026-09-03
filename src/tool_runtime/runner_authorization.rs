use super::project_resolution::ResolvedProject;
use super::tool_definition::{runtime_tool_runner_capability, RunnerCapabilityRequirement};
use super::{ProjectResolverError, RecoveryKind, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::{
    SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL, SHELL_CLIENT_CAPABILITY_SSH_SHELL,
};
use serde_json::json;

/// The Runner capability or owner-boundary requirement a Runner-backed tool
/// needs before dispatch. Non-Runner tools (and tools without a Project) require nothing.
pub(crate) fn required_runner_capability(call: &ToolCall) -> Option<RunnerCapabilityRequirement> {
    runtime_tool_runner_capability(call.tool_name())
}

fn runner_capability_unavailable_result(message: impl Into<String>) -> ToolResult {
    // Stable pre-0.4 ToolResult/Session compatibility identity. Current Rust
    // policy terminology is Runner-facing, but replay must retain this wire code.
    ToolResult::err_with_output(
        message,
        json!({"error_kind": "agent_capability_unavailable"}),
    )
    .with_recovery(RecoveryKind::NoAction, None)
}

impl ToolRuntime {
    /// Enforce the owner boundary and capability requirements for Runner-backed
    /// runtime tools before dispatching. This is the single place where the
    /// runtime paths (`/api/tools/call`, `/api/projects/*`, `/mcp`) check that
    /// the caller is allowed to drive a Runner.
    /// `/api/shell/*` handlers keep their own `assert_shell_client_owner`
    /// checks; this method closes the gap for the runtime paths.
    ///
    /// Returns `Ok(())` for project-less tools so they are unaffected.
    pub(crate) async fn authorize_runner_tool(
        &self,
        call: &ToolCall,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
        project_resolution: Option<&Result<ResolvedProject, ProjectResolverError>>,
    ) -> Result<(), ToolResult> {
        let Some(project) = call.project() else {
            return Ok(());
        };
        let required = required_runner_capability(call);
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
                    "error_kind": "unsupported_resource",
                    "command_started": false,
                    "command_completed": false,
                    "command_ok": false,
                    "exit_code": null,
                    "execution_state": "not_started",
                    "failure_kind": "unsupported_resource",
                    "tool_failure": true,
                }),
            )
            .with_recovery(RecoveryKind::FixInput, None));
        }
        let client_id = proj.client_id.clone();
        let access = crate::shell_client::runner_access_from_auth(auth);
        if self
            .shell_clients
            .get_client_view_for_auth(&client_id, access.as_ref())
            .await
            .is_none()
        {
            return Err(ToolResult::err(format!(
                "unknown shell client: {}",
                client_id
            )));
        }
        self.shell_clients
            .assert_client_access(access.as_ref(), &client_id)
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
                        .client_supports_for_auth(&client_id, capability, access.as_ref())
                        .await
                        .map_err(ToolResult::err)?
                    {
                        supported = true;
                        break;
                    }
                }
                if !supported {
                    let message =
                        format!("Runner {} does not support {}", client_id, required.label());
                    if matches!(
                        required,
                        RunnerCapabilityRequirement::LspReadOnlyNavigation
                            | RunnerCapabilityRequirement::LspCallHierarchy
                    ) {
                        return Err(runner_capability_unavailable_result(format!(
                            "{}: {}",
                            crate::lsp_bridge::error_codes::AGENT_CAPABILITY_UNAVAILABLE,
                            message
                        )));
                    }
                    if matches!(required, RunnerCapabilityRequirement::PersistentShell) {
                        return Err(runner_capability_unavailable_result(format!(
                            "agent_capability_unavailable: {}",
                            message
                        )));
                    }
                    if matches!(
                        required,
                        RunnerCapabilityRequirement::StructuredProcess
                            | RunnerCapabilityRequirement::DetachedProcess
                            | RunnerCapabilityRequirement::StructuredScript
                    ) {
                        let noun =
                            if matches!(required, RunnerCapabilityRequirement::StructuredScript) {
                                "script"
                            } else if matches!(
                                required,
                                RunnerCapabilityRequirement::DetachedProcess
                            ) {
                                "detached process"
                            } else {
                                "process"
                            };
                        return Err(ToolResult::err_with_output(
                            format!(
                                "capability_unavailable: Runner {} does not support {}; no {noun} was started and no shell fallback was attempted",
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
                    return Err(runner_capability_unavailable_result(message));
                }
            }
        }
        if ssh_resource.is_some() {
            if matches!(call, ToolCall::OpenSessionShell { .. }) {
                // `OpenSessionShell` already requires `persistent_shell` through
                // the tool definition. A named persistent SSH shell has its own
                // additive capability and deliberately does not require the
                // one-shot/background `ssh_shell` capability. Later
                // exec/status/close route by the resource saved in the shell
                // record rather than inheriting Session context here.
                if !self
                    .shell_clients
                    .client_supports_for_auth(
                        &client_id,
                        SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL,
                        access.as_ref(),
                    )
                    .await
                    .map_err(ToolResult::err)?
                {
                    return Err(runner_capability_unavailable_result(format!(
                        "agent_capability_unavailable: Runner {} does not support {}",
                        client_id, SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL
                    )));
                }
            } else {
                // Accepted one-shot/background SSH-resource execution still
                // requires the historical `ssh_shell` capability. Structured
                // process/script resource calls have already failed closed above.
                if !self
                    .shell_clients
                    .client_supports_for_auth(
                        &client_id,
                        SHELL_CLIENT_CAPABILITY_SSH_SHELL,
                        access.as_ref(),
                    )
                    .await
                    .map_err(ToolResult::err)?
                {
                    return Err(runner_capability_unavailable_result(format!(
                        "agent_capability_unavailable: Runner {} does not support {}",
                        client_id, SHELL_CLIENT_CAPABILITY_SSH_SHELL
                    )));
                }
            }
        }
        Ok(())
    }
}
