//! Runtime dispatch for read-only agent-side LSP navigation tools.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::lsp_bridge::{
    clamp_document_symbols_limit, clamp_find_references_limit, clamp_goto_definition_limit,
    error_codes, parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspRequest,
};
use serde_json::{json, Value};
use std::time::Duration;

impl ToolRuntime {
    pub(crate) async fn dispatch_lsp_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::LspStatus {
                project,
                session_id: _,
            } => self.call_agent_lsp(project, AgentLspRequest::Status).await,
            ToolCall::DocumentSymbols {
                project,
                path,
                limit,
                session_id: _,
            } => {
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::DocumentSymbols {
                        path,
                        limit: clamp_document_symbols_limit(limit),
                    },
                )
                .await
            }
            ToolCall::GotoDefinition {
                project,
                path,
                line,
                column,
                limit,
                session_id: _,
            } => {
                if line < 1 || column < 1 {
                    return ToolResult::err(format!(
                        "{}: line and column must be >= 1",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::GotoDefinition {
                        path,
                        line,
                        column,
                        limit: clamp_goto_definition_limit(limit),
                    },
                )
                .await
            }
            ToolCall::FindReferences {
                project,
                path,
                line,
                column,
                include_declaration,
                limit,
                session_id: _,
            } => {
                if line < 1 || column < 1 {
                    return ToolResult::err(format!(
                        "{}: line and column must be >= 1",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::FindReferences {
                        path,
                        line,
                        column,
                        include_declaration,
                        limit: clamp_find_references_limit(limit),
                    },
                )
                .await
            }
            other => ToolResult::err(format!("not an LSP tool: {}", other.tool_name())),
        }
    }

    async fn call_agent_lsp(&self, project: String, request: AgentLspRequest) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(p) => p,
            Err(e) => return e.into_tool_result(),
        };
        let proj = &resolved.config;
        if !proj.is_agent() {
            return ToolResult::err(format!(
                "{}: LSP tools require an agent-backed project",
                error_codes::AGENT_CAPABILITY_UNAVAILABLE
            ));
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        // Server-resolved agent-local project id only — never trust a
        // model-supplied free-form agent project id for bridge dispatch.
        let agent_project_id = match agent_local_project_id(&resolved.resolved_id) {
            Some(id) => id.to_string(),
            None => {
                return ToolResult::err(format!(
                    "{}: could not derive agent project id from runtime id",
                    error_codes::UNKNOWN_PROJECT
                ))
            }
        };
        let payload = AgentLspPayload {
            project_id: agent_project_id,
            request,
        };
        let wait_timeout = 30u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_lsp(client_id, payload, "tool_runtime".to_string(), wait_timeout)
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                if e.contains("does not support") {
                    return ToolResult::err(format!(
                        "{}: {}",
                        error_codes::AGENT_CAPABILITY_UNAVAILABLE,
                        e
                    ));
                }
                return ToolResult::err(e);
            }
        };
        match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
            Ok(Ok(resp)) => {
                if let Some(error) = resp.error {
                    return map_agent_transport_error(error);
                }
                let stdout = resp.stdout.unwrap_or_default();
                match parse_agent_lsp_result_envelope(&stdout) {
                    Ok(envelope) if envelope.success => {
                        let mut result = envelope.result.unwrap_or(Value::Null);
                        if let Some(obj) = result.as_object_mut() {
                            obj.insert("project".to_string(), json!(resolved.resolved_id));
                        }
                        let serialized = result.to_string();
                        if serialized.contains("file://") {
                            return ToolResult::err(format!(
                                "{}: agent result contained forbidden path material",
                                error_codes::MALFORMED_AGENT_LSP_RESULT
                            ));
                        }
                        ToolResult::ok(result)
                    }
                    Ok(envelope) => {
                        let err =
                            envelope
                                .error
                                .unwrap_or_else(|| crate::lsp_bridge::AgentLspError {
                                    code: error_codes::LSP_SERVER_FAILED.to_string(),
                                    message: "LSP request failed".to_string(),
                                });
                        ToolResult::err_with_output(
                            format!("{}: {}", err.code, err.message),
                            json!({
                                "code": err.code,
                                "message": err.message,
                            }),
                        )
                    }
                    Err(e) => ToolResult::err(e),
                }
            }
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                ToolResult::err("agent LSP waiter was dropped")
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                ToolResult::err(format!(
                    "{}: timed out waiting for agent LSP result",
                    error_codes::LSP_REQUEST_TIMEOUT
                ))
            }
        }
    }
}

fn agent_local_project_id(resolved_id: &str) -> Option<&str> {
    let rest = resolved_id.strip_prefix("agent:")?;
    let (_client, project_id) = rest.split_once(':')?;
    if project_id.is_empty() {
        None
    } else {
        Some(project_id)
    }
}

fn map_agent_transport_error(error: String) -> ToolResult {
    let lower = error.to_ascii_lowercase();
    if lower.contains("unknown shell client") || lower.contains("not connected") {
        return ToolResult::err(format!("agent unavailable: {error}"));
    }
    ToolResult::err(error)
}
