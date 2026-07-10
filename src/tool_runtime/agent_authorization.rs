use super::tool_definition::{runtime_tool_agent_capability, AgentCapability};
use super::{ProjectResolverError, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

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
        auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let Some(project) = call.project() else {
            return Ok(());
        };
        let required = match required_agent_capability(call) {
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
        if required.is_owner_only() {
            return Ok(());
        }
        // Capability check via the registry helper so the requirement is
        // expressed as a named capability, not a raw struct field access.
        let mut supported = false;
        for capability in required.registry_capabilities() {
            if self
                .shell_clients
                .client_supports(&client_id, capability)
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
            if matches!(required, AgentCapability::LspReadOnlyNavigation) {
                return Err(ToolResult::err(format!(
                    "{}: {}",
                    crate::lsp_bridge::error_codes::AGENT_CAPABILITY_UNAVAILABLE,
                    message
                )));
            }
            return Err(ToolResult::err(message));
        }
        Ok(())
    }
}
