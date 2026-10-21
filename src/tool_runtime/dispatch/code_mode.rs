//! Experimental Code Mode composition routing after canonical governance.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_code_mode_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
        logical_invocation_id: Option<&str>,
        correlation: &mut crate::tool_runtime::window_activity::ToolCallCorrelation,
    ) -> ToolResult {
        #[cfg(feature = "experimental-code-mode")]
        {
            use crate::tool_runtime::code_mode::CodeModeCallableStage;

            let (stage, session_id, source, timeout_ms) = match call {
                ToolCall::CodeModeExec {
                    project: _,
                    session_id,
                    source,
                    timeout_ms,
                } => (
                    CodeModeCallableStage::ReadOnly,
                    session_id,
                    source,
                    timeout_ms,
                ),
                ToolCall::CodeModeExecEffectful {
                    project: _,
                    session_id,
                    source,
                    timeout_ms,
                } => (
                    CodeModeCallableStage::Validation,
                    session_id,
                    source,
                    timeout_ms,
                ),
                ToolCall::CodeModeExecMutating {
                    project: _,
                    session_id,
                    source,
                    timeout_ms,
                } => (
                    CodeModeCallableStage::GuardedEdit,
                    session_id,
                    source,
                    timeout_ms,
                ),
                _ => {
                    return ToolResult::err("tool does not belong to the Code Mode dispatch family")
                }
            };
            let project = match project_resolution {
                Some(Ok(project)) => project,
                Some(Err(error)) => return error.into_tool_result(),
                None => {
                    return ToolResult::err(format!(
                        "{} requires a resolved Project",
                        stage.entry_tool()
                    ))
                }
            };
            let (result, composition) = self
                .code_mode_exec(
                    stage,
                    project,
                    session_id,
                    source,
                    timeout_ms,
                    auth,
                    transport,
                    logical_invocation_id.map(str::to_string),
                )
                .await;
            correlation.code_mode_composition = Some(composition);
            result
        }
        #[cfg(not(feature = "experimental-code-mode"))]
        {
            let _ = call;
            ToolResult::err("tool does not belong to the Code Mode dispatch family")
        }
    }
}
