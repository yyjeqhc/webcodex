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
        match call {
            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExec {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("code_mode_exec requires a resolved Project"),
                };
                let (result, composition) = self
                    .code_mode_exec(
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

            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExecEffectful {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err(
                            "code_mode_exec_effectful requires a resolved Project",
                        )
                    }
                };
                let (result, composition) = self
                    .code_mode_exec_effectful(
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

            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExecMutating {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err(
                            "code_mode_exec_mutating requires a resolved Project",
                        )
                    }
                };
                let (result, composition) = self
                    .code_mode_exec_mutating(
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
            _ => ToolResult::err("tool does not belong to the Code Mode dispatch family"),
        }
    }
}
