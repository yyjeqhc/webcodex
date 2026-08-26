//! Runtime dispatch for read-only agent-side LSP navigation tools.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::lsp_bridge::{
    clamp_document_diagnostics_limit, clamp_document_symbols_limit, clamp_find_references_limit,
    clamp_goto_definition_limit, clamp_workspace_symbols_limit, error_codes, is_known_error_code,
    parse_agent_lsp_result_envelope, redact_absolute_paths, validate_call_hierarchy_bounds,
    AgentLspPayload, AgentLspRequest, CallHierarchyResult, DocumentDiagnosticsResult,
    DocumentDiagnosticsStatus, DocumentSymbolsResult, HoverResult, LocationsResult,
    LspStatusResult, WorkspaceSymbolsResult, MAX_CALL_HIERARCHY_CALL_SITES_PER_EDGE,
    MAX_CALL_HIERARCHY_ROOTS,
};
use crate::shell_client::{EnqueueLspError, RunnerFeature};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Component, Path};
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
            ToolCall::DocumentDiagnostics {
                project,
                path,
                limit,
                session_id: _,
            } => {
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::DocumentDiagnostics {
                        path,
                        limit: clamp_document_diagnostics_limit(limit),
                    },
                )
                .await
            }
            ToolCall::Hover {
                project,
                path,
                line,
                column,
                session_id: _,
            } => {
                if line < 1 || column < 1 {
                    return ToolResult::err(format!(
                        "{}: line and column must be >= 1",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                self.call_agent_lsp(project, AgentLspRequest::Hover { path, line, column })
                    .await
            }
            ToolCall::WorkspaceSymbols {
                project,
                query,
                limit,
                session_id: _,
            } => {
                let query = query.trim().to_string();
                if query.is_empty() || query.chars().count() > 200 {
                    return ToolResult::err(format!(
                        "{}: query must contain 1..200 non-whitespace characters",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                if redact_absolute_paths(&query) != query {
                    return ToolResult::err(format!(
                        "{}: query must not contain absolute path material",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::WorkspaceSymbols {
                        query,
                        limit: clamp_workspace_symbols_limit(limit),
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
            ToolCall::CallHierarchy {
                project,
                path,
                line,
                column,
                direction,
                depth,
                limit,
                session_id: _,
            } => {
                if line < 1 || column < 1 {
                    return ToolResult::err(format!(
                        "{}: line and column must be >= 1",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                if let Err(message) = validate_call_hierarchy_bounds(depth, limit) {
                    return ToolResult::err(format!(
                        "{}: {message}",
                        error_codes::INVALID_ARGUMENTS
                    ));
                }
                self.call_agent_lsp(
                    project,
                    AgentLspRequest::CallHierarchy {
                        path,
                        line,
                        column,
                        direction,
                        depth,
                        limit,
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
        let Some(client) = self
            .shell_clients
            .get_client_semantic_view(&client_id)
            .await
        else {
            return ToolResult::err(format!(
                "{}: agent is not connected",
                error_codes::AGENT_CAPABILITY_UNAVAILABLE
            ));
        };
        if !client.view.connected {
            return ToolResult::err(format!(
                "{}: agent is not connected",
                error_codes::AGENT_CAPABILITY_UNAVAILABLE
            ));
        }
        let call_hierarchy = matches!(&request, AgentLspRequest::CallHierarchy { .. });
        if call_hierarchy && !client.supports(RunnerFeature::LspCallHierarchy) {
            return ToolResult::err(format!(
                "{}: agent does not support lsp_call_hierarchy",
                error_codes::AGENT_CAPABILITY_UNAVAILABLE
            ));
        }
        if !call_hierarchy && !client.supports(RunnerFeature::LspReadOnlyNavigation) {
            return ToolResult::err(format!(
                "{}: agent does not support lsp_read_only_navigation",
                error_codes::AGENT_CAPABILITY_UNAVAILABLE
            ));
        }
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
        let expected_result = request.clone();
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
            Err(error @ EnqueueLspError::UnsupportedCapability { .. }) => {
                return ToolResult::err(format!(
                    "{}: {}",
                    error_codes::AGENT_CAPABILITY_UNAVAILABLE,
                    error
                ));
            }
            Err(error) => return ToolResult::err(error.to_string()),
        };
        match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
            Ok(Ok(resp)) => {
                if let Some(error) = resp.error {
                    return map_agent_transport_error(error);
                }
                let stdout = resp.stdout.unwrap_or_default();
                match parse_agent_lsp_result_envelope(&stdout) {
                    Ok(envelope) if envelope.success => {
                        let result = envelope.result.unwrap_or(Value::Null);
                        let mut result = match validate_agent_lsp_result(&expected_result, result) {
                            Ok(result) => result,
                            Err(error) => return ToolResult::err(error),
                        };
                        if let Some(obj) = result.as_object_mut() {
                            obj.insert("project".to_string(), json!(resolved.resolved_id));
                        }
                        match expected_result {
                            AgentLspRequest::DocumentDiagnostics { .. } => {
                                let status = result
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown");
                                if status == "complete" {
                                    ToolResult::ok(result)
                                } else {
                                    let code = if status == "timeout" {
                                        error_codes::LSP_REQUEST_TIMEOUT
                                    } else {
                                        error_codes::LSP_PROTOCOL_ERROR
                                    };
                                    ToolResult::err_with_output(
                                        format!(
                                            "{code}: diagnostics are {status}; no fresh clean/diagnostic conclusion is available"
                                        ),
                                        result,
                                    )
                                }
                            }
                            _ => ToolResult::ok(result),
                        }
                    }
                    Ok(envelope) => {
                        let err =
                            envelope
                                .error
                                .unwrap_or_else(|| crate::lsp_bridge::AgentLspError {
                                    code: error_codes::LSP_SERVER_FAILED.to_string(),
                                    message: "LSP request failed".to_string(),
                                });
                        if !is_known_error_code(&err.code) {
                            return ToolResult::err(format!(
                                "{}: agent result contained an unknown error code",
                                error_codes::MALFORMED_AGENT_LSP_RESULT
                            ));
                        }
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

fn validate_agent_lsp_result(request: &AgentLspRequest, result: Value) -> Result<Value, String> {
    let result = match request {
        AgentLspRequest::Status => roundtrip_typed_result::<LspStatusResult>(result),
        AgentLspRequest::DocumentSymbols { .. } => {
            roundtrip_typed_result::<DocumentSymbolsResult>(result)
        }
        AgentLspRequest::DocumentDiagnostics { .. } => {
            serde_json::from_value::<DocumentDiagnosticsResult>(result).and_then(|typed| {
                if validate_document_diagnostics_status(&typed).is_err() {
                    return Err(serde_json::Error::io(std::io::Error::other(
                        "inconsistent diagnostics status",
                    )));
                }
                serde_json::to_value(typed)
            })
        }
        AgentLspRequest::Hover { .. } => roundtrip_typed_result::<HoverResult>(result),
        AgentLspRequest::WorkspaceSymbols { .. } => {
            roundtrip_typed_result::<WorkspaceSymbolsResult>(result)
        }
        AgentLspRequest::GotoDefinition { .. } | AgentLspRequest::FindReferences { .. } => {
            roundtrip_typed_result::<LocationsResult>(result)
        }
        AgentLspRequest::CallHierarchy {
            path,
            direction,
            line,
            column,
            depth,
            limit,
        } => serde_json::from_value::<CallHierarchyResult>(result).and_then(|typed| {
            validate_call_hierarchy_result(
                &typed, path, *direction, *line, *column, *depth, *limit,
            )
            .map_err(|_| {
                serde_json::Error::io(std::io::Error::other("inconsistent call hierarchy result"))
            })?;
            serde_json::to_value(typed)
        }),
    }
    .map_err(|_| {
        format!(
            "{}: agent result did not match the expected LSP result shape",
            error_codes::MALFORMED_AGENT_LSP_RESULT
        )
    })?;
    if contains_forbidden_path_material(&result) {
        return Err(format!(
            "{}: agent result contained forbidden path material",
            error_codes::MALFORMED_AGENT_LSP_RESULT
        ));
    }
    Ok(result)
}

fn validate_call_hierarchy_result(
    result: &CallHierarchyResult,
    requested_path: &str,
    requested_direction: crate::lsp_bridge::CallHierarchyDirection,
    requested_line: usize,
    requested_column: usize,
    requested_depth: usize,
    requested_limit: usize,
) -> Result<(), ()> {
    use crate::lsp_bridge::{CallHierarchyDirection, CallHierarchyEdgeDirection};

    validate_call_hierarchy_bounds(requested_depth, requested_limit).map_err(|_| ())?;
    let normalized_requested_path =
        normalize_requested_call_hierarchy_path(requested_path).ok_or(())?;
    if result.path != normalized_requested_path
        || result.direction != requested_direction
        || result.depth != requested_depth
        || result.returned_count != result.edges.len()
        || result.edges.len() > requested_limit
        || result.root_returned_count != result.roots.len()
        || result.root_total_count < result.root_returned_count
        || result.roots.len() > MAX_CALL_HIERARCHY_ROOTS
        || (result.call_site_ranges_omitted > 0 && !result.truncated)
        || result.query_position.line != requested_line
        || result.query_position.column != requested_column
        || !is_safe_project_relative_path(&result.path)
        || result.language.is_empty()
        || result.language.chars().count() > 64
    {
        return Err(());
    }
    for symbol in &result.roots {
        validate_call_hierarchy_symbol(symbol)?;
    }
    for edge in &result.edges {
        let direction_allowed = match requested_direction {
            CallHierarchyDirection::Incoming => {
                edge.direction == CallHierarchyEdgeDirection::Incoming
            }
            CallHierarchyDirection::Outgoing => {
                edge.direction == CallHierarchyEdgeDirection::Outgoing
            }
            CallHierarchyDirection::Both => true,
        };
        if !direction_allowed
            || !(1..=requested_depth).contains(&edge.depth)
            || edge.call_sites.len() > MAX_CALL_HIERARCHY_CALL_SITES_PER_EDGE
        {
            return Err(());
        }
        validate_call_hierarchy_symbol(&edge.from)?;
        validate_call_hierarchy_symbol(&edge.to)?;
        for range in &edge.call_sites {
            validate_public_range(range)?;
        }
    }
    Ok(())
}

fn validate_call_hierarchy_symbol(
    symbol: &crate::lsp_bridge::PublicCallHierarchySymbol,
) -> Result<(), ()> {
    if symbol.name.is_empty()
        || symbol.name.chars().count() > 256
        || symbol.kind.is_empty()
        || symbol.kind.chars().count() > 64
        || !is_safe_project_relative_path(&symbol.path)
    {
        return Err(());
    }
    validate_public_range(&symbol.range)?;
    validate_public_range(&symbol.selection_range)
}

fn validate_public_range(range: &crate::lsp_bridge::PublicRange) -> Result<(), ()> {
    let start = (range.start.line, range.start.column);
    let end = (range.end.line, range.end.column);
    if range.start.line < 1
        || range.start.column < 1
        || range.end.line < 1
        || range.end.column < 1
        || end < start
    {
        return Err(());
    }
    Ok(())
}

fn normalize_requested_call_hierarchy_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    let mut components = Vec::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(value) => components.push(value.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!components.is_empty()).then(|| components.join("/"))
}

fn is_safe_project_relative_path(path: &str) -> bool {
    if path.is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || path.to_ascii_lowercase().starts_with("file:")
        || string_contains_forbidden_path_material(path)
    {
        return false;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    normalize_requested_call_hierarchy_path(path).as_deref() == Some(path)
}

fn validate_document_diagnostics_status(result: &DocumentDiagnosticsResult) -> Result<(), ()> {
    let clean =
        (result.status == DocumentDiagnosticsStatus::Complete).then_some(result.total_count == 0);
    if result.clean != clean {
        return Err(());
    }
    Ok(())
}

fn roundtrip_typed_result<T>(result: Value) -> Result<Value, serde_json::Error>
where
    T: DeserializeOwned + Serialize,
{
    serde_json::from_value::<T>(result).and_then(serde_json::to_value)
}

fn contains_forbidden_path_material(value: &Value) -> bool {
    match value {
        Value::String(value) => string_contains_forbidden_path_material(value),
        Value::Array(values) => values.iter().any(contains_forbidden_path_material),
        Value::Object(values) => values.values().any(contains_forbidden_path_material),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn string_contains_forbidden_path_material(value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if lower.contains("file://")
        || value.starts_with('/')
        || value.starts_with(r"\\")
        || redact_absolute_paths(value) != value
    {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// Derive the agent-local project id from a server-resolved runtime id.
/// Shared with the coding-startup semantic-navigation probe; never derived
/// from model-supplied free-form ids.
pub(crate) fn agent_local_project_id(resolved_id: &str) -> Option<&str> {
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
