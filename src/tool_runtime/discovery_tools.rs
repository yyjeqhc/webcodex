//! Runtime dispatch adapters for discovery and observability tool calls.

use super::kernel::ToolProtocolCapabilities;
use super::runtime_info::ListRunnersOptions;
use super::tool_inputs::ListToolsOptions;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_discovery_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        protocol_capabilities: ToolProtocolCapabilities,
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
            ToolCall::ListRunners {
                client_id,
                client_ids,
                include_projects,
                summary_only,
                query,
                status,
                limit,
            } => {
                self.list_runners_with_options(
                    auth,
                    ListRunnersOptions {
                        client_id,
                        client_ids,
                        include_projects,
                        summary_only,
                        query,
                        status,
                        limit,
                    },
                )
                .await
            }
            ToolCall::RuntimeStatus {
                compact,
                summary_only,
                client_id,
            } => {
                self.runtime_status_with_options(auth, compact, summary_only, client_id)
                    .await
            }
            call @ ToolCall::ReadToolTrace { .. } => {
                self.read_tool_trace_diagnostic(call, auth).await
            }
            ToolCall::ToolManifest {
                query,
                limit,
                tool_name,
                category,
                intent,
                include_recommended_flows,
                include_risk_summary,
            } => {
                self.tool_manifest(
                    tool_name,
                    category,
                    intent,
                    include_recommended_flows,
                    include_risk_summary,
                    query,
                    limit,
                    protocol_capabilities,
                )
                .await
            }
            _ => unreachable!("non-discovery tool routed to discovery dispatcher"),
        }
    }
}
