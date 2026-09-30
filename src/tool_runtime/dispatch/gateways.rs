//! Specialized plugin and named-SSH governance entry points.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_plugin_gateway(
        &self,
        plugin: crate::tool_runtime::PluginToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        match crate::plugin_gateway::invoke(self, plugin, recording_session_id, auth, transport)
            .await
        {
            Ok(invocation) => invocation.to_tool_result(),
            Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Scope {
                required_scope,
                description,
            }) => ToolResult::err_with_output(
                description,
                serde_json::json!({
                    "failure_kind": "insufficient_scope",
                    "required_scope": required_scope,
                    "dispatch_certainty": "not_started",
                }),
            ),
            Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Tool(result)) => {
                result
            }
        }
    }
    pub(super) async fn dispatch_ssh_resource_gateway(
        &self,
        ssh_resource: crate::tool_runtime::SshResourceToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        match crate::ssh_resource_gateway::invoke(
            self,
            ssh_resource,
            recording_session_id,
            auth,
            transport,
        )
        .await
        {
            Ok(invocation) => invocation.to_tool_result(),
            Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Scope {
                required_scope,
                description,
            }) => ToolResult::err_with_output(
                description,
                serde_json::json!({
                    "failure_kind": "insufficient_scope",
                    "required_scope": required_scope,
                    "dispatch_certainty": "not_started",
                }),
            ),
            Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Tool(result)) => {
                result
            }
        }
    }
}
