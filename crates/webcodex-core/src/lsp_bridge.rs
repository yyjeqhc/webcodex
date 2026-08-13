//! Typed agent LSP navigation bridge contract.
//!
//! Shared by the server runtime and `webcodex-runner`. This module intentionally
//! exposes only fixed read-only operations — never arbitrary LSP methods,
//! JSON-RPC passthrough, or absolute project roots.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const AGENT_LSP_RESULT_FORMAT: &str = "webcodex.agent_lsp_result.v1";
pub const AGENT_LSP_REQUEST_KIND: &str = "lsp";

pub const DEFAULT_DOCUMENT_SYMBOLS_LIMIT: usize = 100;
pub const MIN_DOCUMENT_SYMBOLS_LIMIT: usize = 1;
pub const MAX_DOCUMENT_SYMBOLS_LIMIT: usize = 500;

pub const DEFAULT_GOTO_DEFINITION_LIMIT: usize = 20;
pub const MIN_GOTO_DEFINITION_LIMIT: usize = 1;
pub const MAX_GOTO_DEFINITION_LIMIT: usize = 100;

pub const DEFAULT_FIND_REFERENCES_LIMIT: usize = 50;
pub const MIN_FIND_REFERENCES_LIMIT: usize = 1;
pub const MAX_FIND_REFERENCES_LIMIT: usize = 200;

pub const DEFAULT_DOCUMENT_DIAGNOSTICS_LIMIT: usize = 100;
pub const MIN_DOCUMENT_DIAGNOSTICS_LIMIT: usize = 1;
pub const MAX_DOCUMENT_DIAGNOSTICS_LIMIT: usize = 200;

pub const DEFAULT_WORKSPACE_SYMBOLS_LIMIT: usize = 50;
pub const MIN_WORKSPACE_SYMBOLS_LIMIT: usize = 1;
pub const MAX_WORKSPACE_SYMBOLS_LIMIT: usize = 200;

pub const DEFAULT_CALL_HIERARCHY_DEPTH: usize = 1;
pub const MIN_CALL_HIERARCHY_DEPTH: usize = 1;
pub const MAX_CALL_HIERARCHY_DEPTH: usize = 2;
pub const DEFAULT_CALL_HIERARCHY_LIMIT: usize = 50;
pub const MIN_CALL_HIERARCHY_LIMIT: usize = 1;
pub const MAX_CALL_HIERARCHY_LIMIT: usize = 100;
pub const MAX_CALL_HIERARCHY_ROOTS: usize = 16;
pub const MAX_CALL_HIERARCHY_CALL_SITES_PER_EDGE: usize = 20;
pub const MAX_CALL_HIERARCHY_PREPARE_ITEMS_INSPECTED: usize = 64;
pub const MAX_CALL_HIERARCHY_CALL_ENTRIES_INSPECTED_PER_RPC: usize = 256;
pub const MAX_CALL_HIERARCHY_RAW_CALL_SITE_RANGES_INSPECTED_PER_ENTRY: usize = 100;

pub const MAX_ERROR_MESSAGE_CHARS: usize = 240;

/// Stable error codes for the agent LSP bridge and public tools.
pub mod error_codes {
    pub const AGENT_CAPABILITY_UNAVAILABLE: &str = "agent_capability_unavailable";
    pub const LSP_SERVER_UNAVAILABLE: &str = "lsp_server_unavailable";
    pub const LSP_SERVER_FAILED: &str = "lsp_server_failed";
    pub const LSP_REQUEST_TIMEOUT: &str = "lsp_request_timeout";
    pub const LSP_PROTOCOL_ERROR: &str = "lsp_protocol_error";
    pub const INVALID_PROJECT_PATH: &str = "invalid_project_path";
    pub const UNSUPPORTED_LANGUAGE: &str = "unsupported_language";
    pub const FILE_NOT_FOUND: &str = "file_not_found";
    pub const INVALID_POSITION: &str = "invalid_position";
    pub const INVALID_ARGUMENTS: &str = "invalid_arguments";
    pub const MALFORMED_AGENT_LSP_RESULT: &str = "malformed_agent_lsp_result";
    pub const UNKNOWN_PROJECT: &str = "unknown_project";
    pub const MISSING_LSP_PAYLOAD: &str = "missing_lsp_payload";
    pub const DOCUMENT_TOO_LARGE: &str = "document_too_large";
    pub const CALL_HIERARCHY_UNSUPPORTED: &str = "call_hierarchy_unsupported";
}

pub fn is_known_error_code(code: &str) -> bool {
    matches!(
        code,
        error_codes::AGENT_CAPABILITY_UNAVAILABLE
            | error_codes::LSP_SERVER_UNAVAILABLE
            | error_codes::LSP_SERVER_FAILED
            | error_codes::LSP_REQUEST_TIMEOUT
            | error_codes::LSP_PROTOCOL_ERROR
            | error_codes::INVALID_PROJECT_PATH
            | error_codes::UNSUPPORTED_LANGUAGE
            | error_codes::FILE_NOT_FOUND
            | error_codes::INVALID_POSITION
            | error_codes::INVALID_ARGUMENTS
            | error_codes::MALFORMED_AGENT_LSP_RESULT
            | error_codes::UNKNOWN_PROJECT
            | error_codes::MISSING_LSP_PAYLOAD
            | error_codes::DOCUMENT_TOO_LARGE
            | error_codes::CALL_HIERARCHY_UNSUPPORTED
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallHierarchyDirection {
    Incoming,
    Outgoing,
    #[default]
    Both,
}

/// Typed LSP navigation request carried on agent requests.
///
/// Unknown `operation` values fail serde; this is intentional so old agents and
/// future wire formats cannot silently treat unknown ops as shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum AgentLspRequest {
    Status,
    DocumentSymbols {
        path: String,
        #[serde(default = "default_document_symbols_limit")]
        limit: usize,
    },
    DocumentDiagnostics {
        path: String,
        #[serde(default = "default_document_diagnostics_limit")]
        limit: usize,
    },
    Hover {
        path: String,
        line: usize,
        column: usize,
    },
    WorkspaceSymbols {
        query: String,
        #[serde(default = "default_workspace_symbols_limit")]
        limit: usize,
    },
    GotoDefinition {
        path: String,
        line: usize,
        column: usize,
        #[serde(default = "default_goto_definition_limit")]
        limit: usize,
    },
    FindReferences {
        path: String,
        line: usize,
        column: usize,
        #[serde(default = "default_include_declaration")]
        include_declaration: bool,
        #[serde(default = "default_find_references_limit")]
        limit: usize,
    },
    CallHierarchy {
        path: String,
        line: usize,
        column: usize,
        #[serde(default)]
        direction: CallHierarchyDirection,
        #[serde(default = "default_call_hierarchy_depth")]
        depth: usize,
        #[serde(default = "default_call_hierarchy_limit")]
        limit: usize,
    },
}

fn default_document_symbols_limit() -> usize {
    DEFAULT_DOCUMENT_SYMBOLS_LIMIT
}

fn default_document_diagnostics_limit() -> usize {
    DEFAULT_DOCUMENT_DIAGNOSTICS_LIMIT
}

fn default_workspace_symbols_limit() -> usize {
    DEFAULT_WORKSPACE_SYMBOLS_LIMIT
}

fn default_goto_definition_limit() -> usize {
    DEFAULT_GOTO_DEFINITION_LIMIT
}

fn default_find_references_limit() -> usize {
    DEFAULT_FIND_REFERENCES_LIMIT
}

fn default_include_declaration() -> bool {
    true
}

fn default_call_hierarchy_depth() -> usize {
    DEFAULT_CALL_HIERARCHY_DEPTH
}

fn default_call_hierarchy_limit() -> usize {
    DEFAULT_CALL_HIERARCHY_LIMIT
}

/// Agent-side LSP payload. `project_id` is the agent-local project id already
/// resolved by the server from a full runtime project id. Absolute roots and
/// arbitrary LSP methods are never included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLspPayload {
    pub project_id: String,
    pub request: AgentLspRequest,
}

/// Public 1-based Unicode scalar position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPosition {
    pub line: usize,
    pub column: usize,
}

/// Half-open range using public positions (same semantics as LSP Range).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRange {
    pub start: PublicPosition,
    pub end: PublicPosition,
}

/// Project-relative location returned to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicLocation {
    pub path: String,
    pub range: PublicRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_range: Option<PublicRange>,
}

/// Normalized document symbol node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicSymbol {
    pub name: String,
    pub kind: String,
    pub kind_code: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub range: PublicRange,
    pub selection_range: PublicRange,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PublicSymbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspAvailabilityStatus {
    Unavailable,
    Available,
    Initializing,
    Running,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspCommandSource {
    Configured,
    Environment,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerStatusEntry {
    pub language: String,
    pub server: String,
    pub available: bool,
    pub running: bool,
    pub status: LspAvailabilityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<LspCommandSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_encoding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspStatusResult {
    pub project: String,
    pub detected_languages: Vec<String>,
    pub servers: Vec<LspServerStatusEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbolsResult {
    pub project: String,
    pub path: String,
    pub language: String,
    pub symbols: Vec<PublicSymbol>,
    pub total_count: usize,
    pub returned_count: usize,
    pub truncated: bool,
    pub external_results_omitted: usize,
    pub invalid_results_omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicDiagnostic {
    pub range: PublicRange,
    pub severity: String,
    #[serde(default)]
    pub severity_code: Option<i64>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub message: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentDiagnosticsStatus {
    Complete,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDiagnosticsResult {
    pub project: String,
    pub path: String,
    pub language: String,
    pub diagnostics: Vec<PublicDiagnostic>,
    pub total_count: usize,
    pub returned_count: usize,
    pub truncated: bool,
    pub status: DocumentDiagnosticsStatus,
    pub clean: Option<bool>,
    #[serde(default)]
    pub published_version: Option<i32>,
    pub invalid_results_omitted: usize,
    pub related_information_omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicHover {
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub range: Option<PublicRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverResult {
    pub project: String,
    pub path: String,
    pub position: PublicPosition,
    #[serde(default)]
    pub hover: Option<PublicHover>,
    pub truncated: bool,
    pub range_omitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicWorkspaceSymbol {
    pub name: String,
    pub kind: String,
    pub kind_code: i64,
    #[serde(default)]
    pub container_name: Option<String>,
    pub path: String,
    #[serde(default)]
    pub range: Option<PublicRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSymbolsResult {
    pub project: String,
    pub query: String,
    pub symbols: Vec<PublicWorkspaceSymbol>,
    pub total_results: usize,
    pub returned_count: usize,
    pub truncated: bool,
    pub external_results_omitted: usize,
    pub invalid_results_omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationsResult {
    pub project: String,
    pub path: String,
    pub query_position: PublicPosition,
    pub locations: Vec<PublicLocation>,
    pub total_results: usize,
    pub returned_count: usize,
    pub truncated: bool,
    pub external_results_omitted: usize,
    pub invalid_results_omitted: usize,
}

/// Public project-local symbol used by call-hierarchy results.
///
/// The opaque LSP `data` field and the source URI intentionally have no
/// representation here; they remain request-local inside the Runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCallHierarchySymbol {
    pub name: String,
    pub kind: String,
    pub kind_code: i64,
    pub path: String,
    pub range: PublicRange,
    pub selection_range: PublicRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallHierarchyEdgeDirection {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCallHierarchyEdge {
    pub direction: CallHierarchyEdgeDirection,
    pub depth: usize,
    pub from: PublicCallHierarchySymbol,
    pub to: PublicCallHierarchySymbol,
    pub call_sites: Vec<PublicRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallHierarchyResult {
    pub project: String,
    pub path: String,
    pub language: String,
    pub query_position: PublicPosition,
    pub direction: CallHierarchyDirection,
    pub depth: usize,
    pub roots: Vec<PublicCallHierarchySymbol>,
    pub root_total_count: usize,
    pub root_returned_count: usize,
    pub edges: Vec<PublicCallHierarchyEdge>,
    pub returned_count: usize,
    pub truncated: bool,
    pub external_results_omitted: usize,
    pub invalid_results_omitted: usize,
    pub call_site_ranges_omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLspError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLspResultEnvelope {
    pub format: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentLspError>,
}

impl AgentLspResultEnvelope {
    pub fn ok(result: impl Serialize) -> Self {
        Self {
            format: AGENT_LSP_RESULT_FORMAT.to_string(),
            success: true,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            format: AGENT_LSP_RESULT_FORMAT.to_string(),
            success: false,
            result: None,
            error: Some(AgentLspError {
                code: code.into(),
                message: bound_error_message(message),
            }),
        }
    }

    pub fn to_stdout_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"format":"webcodex.agent_lsp_result.v1","success":false,"error":{"code":"lsp_protocol_error","message":"failed to serialize result"}}"#.to_string()
        })
    }
}

/// Parse a strict versioned agent LSP result envelope from stdout.
pub fn parse_agent_lsp_result_envelope(stdout: &str) -> Result<AgentLspResultEnvelope, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "{}: empty agent LSP result",
            error_codes::MALFORMED_AGENT_LSP_RESULT
        ));
    }
    let envelope: AgentLspResultEnvelope = serde_json::from_str(trimmed).map_err(|error| {
        format!(
            "{}: {}",
            error_codes::MALFORMED_AGENT_LSP_RESULT,
            bound_error_message(error.to_string())
        )
    })?;
    if envelope.format != AGENT_LSP_RESULT_FORMAT {
        return Err(format!(
            "{}: unexpected format {:?}",
            error_codes::MALFORMED_AGENT_LSP_RESULT,
            envelope.format
        ));
    }
    if envelope.success {
        if envelope.result.is_none() {
            return Err(format!(
                "{}: success envelope missing result",
                error_codes::MALFORMED_AGENT_LSP_RESULT
            ));
        }
    } else if envelope.error.is_none() {
        return Err(format!(
            "{}: failure envelope missing error",
            error_codes::MALFORMED_AGENT_LSP_RESULT
        ));
    }
    Ok(envelope)
}

pub fn clamp_document_symbols_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_DOCUMENT_SYMBOLS_LIMIT)
        .clamp(MIN_DOCUMENT_SYMBOLS_LIMIT, MAX_DOCUMENT_SYMBOLS_LIMIT)
}

pub fn clamp_goto_definition_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_GOTO_DEFINITION_LIMIT)
        .clamp(MIN_GOTO_DEFINITION_LIMIT, MAX_GOTO_DEFINITION_LIMIT)
}

pub fn clamp_find_references_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_FIND_REFERENCES_LIMIT)
        .clamp(MIN_FIND_REFERENCES_LIMIT, MAX_FIND_REFERENCES_LIMIT)
}

pub fn clamp_document_diagnostics_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_DOCUMENT_DIAGNOSTICS_LIMIT).clamp(
        MIN_DOCUMENT_DIAGNOSTICS_LIMIT,
        MAX_DOCUMENT_DIAGNOSTICS_LIMIT,
    )
}

pub fn clamp_workspace_symbols_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_WORKSPACE_SYMBOLS_LIMIT)
        .clamp(MIN_WORKSPACE_SYMBOLS_LIMIT, MAX_WORKSPACE_SYMBOLS_LIMIT)
}

pub fn validate_call_hierarchy_bounds(depth: usize, limit: usize) -> Result<(), &'static str> {
    if !(MIN_CALL_HIERARCHY_DEPTH..=MAX_CALL_HIERARCHY_DEPTH).contains(&depth) {
        return Err("call hierarchy depth must be 1..=2");
    }
    if !(MIN_CALL_HIERARCHY_LIMIT..=MAX_CALL_HIERARCHY_LIMIT).contains(&limit) {
        return Err("call hierarchy limit must be 1..=100");
    }
    Ok(())
}

/// Best-effort redaction of absolute-path-looking material in error text.
///
/// Replaces `file:` URIs, absolute POSIX paths (including quoted, bracketed,
/// or `key=/...` embedded forms), Windows drive paths, and UNC prefixes with
/// `<path>`, while leaving relative paths like `src/main.rs` intact. This is
/// layered defense for the error channel only; result payloads are bounded by
/// the typed result contract and boundary classification instead.
pub fn redact_absolute_paths(message: &str) -> String {
    const PLACEHOLDER: &str = "<path>";
    fn is_path_stop(c: char) -> bool {
        c.is_whitespace() || matches!(c, '\'' | '"' | '`' | ')' | ']' | '}' | ',' | ';')
    }
    fn is_token_interior(c: char) -> bool {
        c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')
    }
    fn starts_with_file_uri(chars: &[char]) -> bool {
        chars.len() >= 6
            && chars[..5]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case("file:")
            && chars[5] == '/'
    }
    let chars: Vec<char> = message.chars().collect();
    let mut out = String::with_capacity(message.len());
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let file_uri = starts_with_file_uri(&chars[i..])
            && !prev.is_some_and(|p| p.is_alphanumeric() || matches!(p, '.' | '_' | '-'));
        // A '/' opens an absolute path only at a token start; `src/main.rs`
        // keeps its interior separator.
        let absolute_posix = c == '/'
            && !prev.is_some_and(is_token_interior)
            && chars
                .get(i + 1)
                .is_some_and(|next| next.is_alphanumeric() || matches!(next, '.' | '_' | '-'));
        let windows_drive = c.is_ascii_alphabetic()
            && !prev.is_some_and(|p| p.is_alphanumeric() || matches!(p, '.' | '_' | '-'))
            && chars.get(i + 1) == Some(&':')
            && matches!(chars.get(i + 2), Some('/') | Some('\\'));
        let unc = c == '\\' && chars.get(i + 1) == Some(&'\\');
        if file_uri || absolute_posix || windows_drive || unc {
            out.push_str(PLACEHOLDER);
            while i < chars.len() && !is_path_stop(chars[i]) {
                i += 1;
            }
            prev = Some('>');
            continue;
        }
        out.push(c);
        prev = Some(c);
        i += 1;
    }
    out
}

/// Bound an error message for transport: redact absolute-path material, scrub
/// control characters, and truncate to `MAX_ERROR_MESSAGE_CHARS`.
pub fn bound_error_message(message: impl Into<String>) -> String {
    let message = message.into();
    let sanitized = redact_absolute_paths(&message)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let sanitized = sanitized.trim();
    if sanitized.chars().count() <= MAX_ERROR_MESSAGE_CHARS {
        return sanitized.to_string();
    }
    sanitized
        .chars()
        .take(MAX_ERROR_MESSAGE_CHARS.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_lsp_request_serde_roundtrip() {
        let cases = vec![
            AgentLspRequest::Status,
            AgentLspRequest::DocumentSymbols {
                path: "src/main.rs".to_string(),
                limit: 50,
            },
            AgentLspRequest::DocumentDiagnostics {
                path: "src/main.rs".to_string(),
                limit: 100,
            },
            AgentLspRequest::Hover {
                path: "src/main.rs".to_string(),
                line: 10,
                column: 4,
            },
            AgentLspRequest::WorkspaceSymbols {
                query: "ToolRuntime".to_string(),
                limit: 50,
            },
            AgentLspRequest::GotoDefinition {
                path: "src/main.rs".to_string(),
                line: 10,
                column: 4,
                limit: 20,
            },
            AgentLspRequest::FindReferences {
                path: "src/lib.rs".to_string(),
                line: 3,
                column: 8,
                include_declaration: false,
                limit: 40,
            },
            AgentLspRequest::CallHierarchy {
                path: "src/lib.rs".to_string(),
                line: 3,
                column: 8,
                direction: CallHierarchyDirection::Both,
                depth: 2,
                limit: 40,
            },
        ];
        for request in cases {
            let payload = AgentLspPayload {
                project_id: "private-drop".to_string(),
                request: request.clone(),
            };
            let json = serde_json::to_string(&payload).unwrap();
            let back: AgentLspPayload = serde_json::from_str(&json).unwrap();
            assert_eq!(back, payload);
            assert!(!json.contains("operation\":\"unknown"));
        }
    }

    #[test]
    fn arbitrary_operation_is_rejected() {
        let json = r#"{"project_id":"p","request":{"operation":"arbitrary_passthrough","method":"workspace/symbol"}}"#;
        let err = serde_json::from_str::<AgentLspPayload>(json).unwrap_err();
        assert!(
            err.to_string().contains("unknown variant") || err.to_string().contains("operation")
        );
    }

    #[test]
    fn defaults_fill_optional_limits() {
        let json = r#"{"project_id":"p","request":{"operation":"document_symbols","path":"a.rs"}}"#;
        let payload: AgentLspPayload = serde_json::from_str(json).unwrap();
        match payload.request {
            AgentLspRequest::DocumentSymbols { limit, .. } => {
                assert_eq!(limit, DEFAULT_DOCUMENT_SYMBOLS_LIMIT);
            }
            other => panic!("unexpected {other:?}"),
        }
        let workspace = r#"{"project_id":"p","request":{"operation":"workspace_symbols","query":"ToolRuntime"}}"#;
        let payload: AgentLspPayload = serde_json::from_str(workspace).unwrap();
        match payload.request {
            AgentLspRequest::WorkspaceSymbols { limit, .. } => {
                assert_eq!(limit, DEFAULT_WORKSPACE_SYMBOLS_LIMIT);
            }
            other => panic!("unexpected {other:?}"),
        }
        let refs = r#"{"project_id":"p","request":{"operation":"find_references","path":"a.rs","line":1,"column":1}}"#;
        let payload: AgentLspPayload = serde_json::from_str(refs).unwrap();
        match payload.request {
            AgentLspRequest::FindReferences {
                include_declaration,
                limit,
                ..
            } => {
                assert!(include_declaration);
                assert_eq!(limit, DEFAULT_FIND_REFERENCES_LIMIT);
            }
            other => panic!("unexpected {other:?}"),
        }
        let diagnostics =
            r#"{"project_id":"p","request":{"operation":"document_diagnostics","path":"a.rs"}}"#;
        let payload: AgentLspPayload = serde_json::from_str(diagnostics).unwrap();
        match payload.request {
            AgentLspRequest::DocumentDiagnostics { limit, .. } => {
                assert_eq!(limit, DEFAULT_DOCUMENT_DIAGNOSTICS_LIMIT);
            }
            other => panic!("unexpected {other:?}"),
        }
        let hierarchy = r#"{"project_id":"p","request":{"operation":"call_hierarchy","path":"a.rs","line":1,"column":1}}"#;
        let payload: AgentLspPayload = serde_json::from_str(hierarchy).unwrap();
        match payload.request {
            AgentLspRequest::CallHierarchy {
                direction,
                depth,
                limit,
                ..
            } => {
                assert_eq!(direction, CallHierarchyDirection::Both);
                assert_eq!(depth, DEFAULT_CALL_HIERARCHY_DEPTH);
                assert_eq!(limit, DEFAULT_CALL_HIERARCHY_LIMIT);
            }
            other => panic!("unexpected {other:?}"),
        }
        for field in ["direction", "depth", "limit"] {
            let json = format!(
                r#"{{"project_id":"p","request":{{"operation":"call_hierarchy","path":"a.rs","line":1,"column":1,"{field}":null}}}}"#
            );
            assert!(
                serde_json::from_str::<AgentLspPayload>(&json).is_err(),
                "explicit null must be rejected for {field}"
            );
        }
    }

    #[test]
    fn call_hierarchy_bounds_are_stable() {
        assert!(validate_call_hierarchy_bounds(1, 1).is_ok());
        assert!(validate_call_hierarchy_bounds(2, 100).is_ok());
        assert_eq!(
            validate_call_hierarchy_bounds(0, 50),
            Err("call hierarchy depth must be 1..=2")
        );
        assert_eq!(
            validate_call_hierarchy_bounds(1, 101),
            Err("call hierarchy limit must be 1..=100")
        );
        assert_eq!(MAX_CALL_HIERARCHY_PREPARE_ITEMS_INSPECTED, 64);
        assert_eq!(MAX_CALL_HIERARCHY_CALL_ENTRIES_INSPECTED_PER_RPC, 256);
        assert_eq!(
            MAX_CALL_HIERARCHY_RAW_CALL_SITE_RANGES_INSPECTED_PER_ENTRY,
            100
        );
    }

    #[test]
    fn call_hierarchy_result_serde_is_normalized_and_typed() {
        let range = PublicRange {
            start: PublicPosition { line: 2, column: 3 },
            end: PublicPosition { line: 2, column: 7 },
        };
        let symbol = PublicCallHierarchySymbol {
            name: "caller".to_string(),
            kind: "function".to_string(),
            kind_code: 12,
            path: "src/lib.rs".to_string(),
            range: range.clone(),
            selection_range: range,
        };
        let result = CallHierarchyResult {
            project: "demo".to_string(),
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            query_position: PublicPosition { line: 2, column: 3 },
            direction: CallHierarchyDirection::Incoming,
            depth: 1,
            roots: vec![symbol],
            root_total_count: 1,
            root_returned_count: 1,
            edges: Vec::new(),
            returned_count: 0,
            truncated: false,
            external_results_omitted: 0,
            invalid_results_omitted: 0,
            call_site_ranges_omitted: 0,
        };
        let value = serde_json::to_value(&result).unwrap();
        assert!(value.pointer("/roots/0/data").is_none());
        assert!(value.pointer("/roots/0/uri").is_none());
        assert_eq!(
            serde_json::from_value::<CallHierarchyResult>(value).unwrap(),
            result
        );
    }

    #[test]
    fn redact_absolute_paths_covers_uri_quoted_embedded_and_windows_forms() {
        let cases = [
            (
                "failed to load file:///home/user/secret.rs now",
                "failed to load <path> now",
            ),
            ("cannot open /home/user/x.rs", "cannot open <path>"),
            ("cwd '/srv/repo' not allowed", "cwd '<path>' not allowed"),
            ("path=/etc/passwd denied", "path=<path> denied"),
            ("bad (C:\\Users\\x\\y.rs)", "bad (<path>)"),
            ("share \\\\host\\repo failed", "share <path> failed"),
            (
                "manifest at /workspace: broken",
                "manifest at <path> broken",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(redact_absolute_paths(input), expected, "input: {input}");
        }
    }

    #[test]
    fn redact_absolute_paths_preserves_relative_paths_and_plain_text() {
        let cases = [
            "expected item in src/main.rs at line 3",
            "unresolved import a::b::c",
            "1/2 requests failed",
            "duration 3ms w/ retry",
        ];
        for input in cases {
            assert_eq!(redact_absolute_paths(input), input, "input: {input}");
        }
    }

    #[test]
    fn bound_error_message_redacts_before_truncation() {
        let message = format!("rust-analyzer said file://{} exploded", "/a".repeat(400));
        let bounded = bound_error_message(message);
        assert!(!bounded.contains("file://"));
        assert!(!bounded.contains("/a/"));
        assert!(bounded.contains("<path>"));
        assert!(bounded.chars().count() <= MAX_ERROR_MESSAGE_CHARS);
    }

    #[test]
    fn result_envelope_roundtrip_and_strict_parse() {
        let ok = AgentLspResultEnvelope::ok(LspStatusResult {
            project: "p".to_string(),
            detected_languages: vec!["rust".to_string()],
            servers: vec![],
            warnings: vec![],
        });
        let stdout = ok.to_stdout_json();
        let parsed = parse_agent_lsp_result_envelope(&stdout).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.format, AGENT_LSP_RESULT_FORMAT);

        assert!(parse_agent_lsp_result_envelope("not-json").is_err());
        assert!(parse_agent_lsp_result_envelope(
            r#"{"format":"other","success":true,"result":{}}"#
        )
        .is_err());
        assert!(parse_agent_lsp_result_envelope(
            r#"{"format":"webcodex.agent_lsp_result.v1","success":true}"#
        )
        .is_err());
    }
}
