//! Runtime dispatch adapters for discovery and observability tool calls.

use super::tool_inputs::ListToolsOptions;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_discovery_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ListTools {
                category,
                features,
                summary_only,
                limit,
            } => ToolResult::ok(self.list_tools_payload(ListToolsOptions {
                category,
                features,
                summary_only,
                limit,
            })),
            ToolCall::ListAgents => self.list_agents(auth).await,
            ToolCall::RuntimeStatus {
                compact,
                summary_only,
            } => {
                self.runtime_status_with_options(auth, compact, summary_only)
                    .await
            }
            ToolCall::ToolManifest {
                category,
                intent,
                include_recommended_flows,
                include_risk_summary,
            } => {
                self.tool_manifest(
                    category,
                    intent,
                    include_recommended_flows,
                    include_risk_summary,
                )
                .await
            }
            _ => unreachable!("non-discovery tool routed to discovery dispatcher"),
        }
    }
}
