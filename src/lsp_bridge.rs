//! Typed agent LSP navigation bridge contract.
//!
//! Shared by the server runtime and `webcodex-agent`. This module intentionally
//! exposes only four fixed read-only operations — never arbitrary LSP methods,
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

pub const MAX_SYMBOL_NAME_CHARS: usize = 256;
pub const MAX_SYMBOL_DETAIL_CHARS: usize = 512;
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
}

fn default_document_symbols_limit() -> usize {
    DEFAULT_DOCUMENT_SYMBOLS_LIMIT
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspAvailabilityStatus {
    Unavailable,
    Available,
    Initializing,
    Running,
    Crashed,
}

impl LspAvailabilityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Available => "available",
            Self::Initializing => "initializing",
            Self::Running => "running",
            Self::Crashed => "crashed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspCommandSource {
    Configured,
    Environment,
    Path,
}

impl LspCommandSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Environment => "environment",
            Self::Path => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerStatusEntry {
    pub language: String,
    pub server: String,
    pub available: bool,
    pub running: bool,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

pub fn bound_error_message(message: impl Into<String>) -> String {
    let message = message.into();
    // Never ship absolute-looking paths or env-looking fragments in bridge errors.
    let sanitized = message
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

pub fn bound_symbol_name(name: &str) -> String {
    bound_field(name, MAX_SYMBOL_NAME_CHARS)
}

pub fn bound_symbol_detail(detail: &str) -> String {
    bound_field(detail, MAX_SYMBOL_DETAIL_CHARS)
}

fn bound_field(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Map LSP SymbolKind integer to a stable lowercase name.
pub fn symbol_kind_name(kind_code: i64) -> &'static str {
    match kind_code {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "unknown",
    }
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
        let json = r#"{"project_id":"p","request":{"operation":"workspace_symbols","query":"x"}}"#;
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

    #[test]
    fn symbol_kind_mapping_is_stable() {
        assert_eq!(symbol_kind_name(12), "function");
        assert_eq!(symbol_kind_name(23), "struct");
        assert_eq!(symbol_kind_name(999), "unknown");
    }
}
