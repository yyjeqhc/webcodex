//! Typed agent-side validation bridge contract.
//!
//! Shared by the server runtime and `webcodex-agent`. This module intentionally
//! carries only declarative, project-relative validation requests — never
//! arbitrary shell command strings, absolute project roots, or raw tool JSON
//! bodies. Adapter ids resolve on the agent; the agent builds argv, executes,
//! bounds output, parses, and returns sanitized diagnostics.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire protocol version for request/response envelopes.
pub const VALIDATION_BRIDGE_PROTOCOL_VERSION: u32 = 1;
/// Result envelope format id (versioned independently of request fields).
pub const VALIDATION_BRIDGE_RESULT_FORMAT: &str = "webcodex.validation_bridge_result.v1";
/// Agent request kind for the validation bridge (never falls through to shell).
pub const AGENT_VALIDATION_REQUEST_KIND: &str = "validation";

/// Hard cap on captured tool stdout. Oversized output is not tail-truncated and
/// is never JSON-parsed; the bridge returns a structured `output_too_large`.
pub const MAX_VALIDATION_STDOUT_BYTES: usize = 2 * 1024 * 1024;
/// Hard cap applied while reading validation stderr. Bytes beyond this limit
/// are drained from the pipe without being retained.
pub const MAX_VALIDATION_STDERR_CAPTURE_BYTES: usize = 32 * 1024;
/// Bounded stderr summary across the bridge (or omitted entirely).
pub const MAX_VALIDATION_STDERR_SUMMARY_CHARS: usize = 512;
/// Max diagnostics returned in a bridge response.
pub const MAX_BRIDGE_DIAGNOSTICS: usize = 20;
/// Max characters for a diagnostic message.
pub const MAX_DIAGNOSTIC_MESSAGE_CHARS: usize = 240;
/// Max characters for rule/code identifiers.
pub const MAX_RULE_CHARS: usize = 64;
/// Max characters for a project-relative path in diagnostics.
pub const MAX_PATH_CHARS: usize = 512;
/// Max target paths per request.
pub const MAX_TARGETS: usize = 32;
/// Default synchronous timeout (aligned with agent sync wait contract).
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
pub const MIN_TIMEOUT_SECS: u64 = 1;
pub const MAX_TIMEOUT_SECS: u64 = 120;

/// Stable failure kinds for the validation bridge.
pub mod failure_kinds {
    pub const INVALID_ARGUMENTS: &str = "invalid_arguments";
    pub const ADAPTER_NOT_FOUND: &str = "adapter_not_found";
    pub const LANGUAGE_ADAPTER_MISMATCH: &str = "language_adapter_mismatch";
    pub const TOOL_UNAVAILABLE: &str = "tool_unavailable";
    pub const SPAWN_FAILED: &str = "spawn_failed";
    pub const TIMEOUT: &str = "timeout";
    pub const OUTPUT_TOO_LARGE: &str = "output_too_large";
    pub const MALFORMED_OUTPUT: &str = "malformed_output";
    pub const PROCESS_EXIT: &str = "process_exit";
    pub const COMPILE_ERROR: &str = "compile_error";
    pub const UNKNOWN_PROJECT: &str = "unknown_project";
    pub const INVALID_PROJECT_PATH: &str = "invalid_project_path";
    pub const MISSING_VALIDATION_PAYLOAD: &str = "missing_validation_payload";
    pub const PROTOCOL_ERROR: &str = "protocol_error";
}

pub fn is_known_failure_kind(kind: &str) -> bool {
    matches!(
        kind,
        failure_kinds::INVALID_ARGUMENTS
            | failure_kinds::ADAPTER_NOT_FOUND
            | failure_kinds::LANGUAGE_ADAPTER_MISMATCH
            | failure_kinds::TOOL_UNAVAILABLE
            | failure_kinds::SPAWN_FAILED
            | failure_kinds::TIMEOUT
            | failure_kinds::OUTPUT_TOO_LARGE
            | failure_kinds::MALFORMED_OUTPUT
            | failure_kinds::PROCESS_EXIT
            | failure_kinds::COMPILE_ERROR
            | failure_kinds::UNKNOWN_PROJECT
            | failure_kinds::INVALID_PROJECT_PATH
            | failure_kinds::MISSING_VALIDATION_PAYLOAD
            | failure_kinds::PROTOCOL_ERROR
    )
}

/// Declarative validation request. Contains no shell command strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationBridgeRequest {
    pub protocol_version: u32,
    pub adapter_id: String,
    pub language: String,
    pub validation_kind: String,
    /// Agent-local project id (not a filesystem path).
    pub project_id: String,
    /// Optional project-relative working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional project-relative target paths (files or directories).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub options: ValidationBridgeOptions,
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationBridgeOptions {
    /// Reserved for future bounded options; empty in v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_diagnostics: Option<usize>,
}

/// Sanitized diagnostic returned across the bridge (project-relative paths only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeDiagnostic {
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u64>,
    pub message: String,
}

/// Bounded, already-sanitized diagnostics payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeDiagnostics {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub diagnostics: Vec<BridgeDiagnostic>,
    pub diagnostic_count: usize,
    pub returned_diagnostic_count: usize,
    pub diagnostics_truncated: bool,
    pub invalid_diagnostics_omitted: usize,
    pub external_results_omitted: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_error_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_warning_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_information_count: Option<u64>,
}

/// Full validation bridge response (never includes absolute paths or raw JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationBridgeResponse {
    pub protocol_version: u32,
    pub adapter_id: String,
    pub language: String,
    pub validation_kind: String,
    pub success: bool,
    pub command_started: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<BridgeDiagnostics>,
    pub tool_available: bool,
    pub stdout_bytes: u64,
    pub stdout_capped: bool,
    pub stderr_capped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Versioned envelope returned on agent stdout for validation bridge requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationBridgeResultEnvelope {
    pub format: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ValidationBridgeResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ValidationBridgeError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationBridgeError {
    pub code: String,
    pub message: String,
}

impl ValidationBridgeResultEnvelope {
    pub fn ok(mut result: ValidationBridgeResponse) -> Self {
        sanitize_response_free_text(&mut result);
        Self {
            format: VALIDATION_BRIDGE_RESULT_FORMAT.to_string(),
            success: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            format: VALIDATION_BRIDGE_RESULT_FORMAT.to_string(),
            success: false,
            result: None,
            error: Some(ValidationBridgeError {
                code: code.into(),
                message: bound_error_message(message),
            }),
        }
    }

    pub fn to_stdout_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"format":"webcodex.validation_bridge_result.v1","success":false,"error":{"code":"protocol_error","message":"failed to serialize result"}}"#
                .to_string()
        })
    }
}

pub fn bound_error_message(message: impl Into<String>) -> String {
    let message = sanitize_bridge_text(&message.into());
    let mut out: String = message
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(240)
        .collect();
    if message.chars().count() > 240 {
        out.push('…');
    }
    out
}

/// Redact absolute filesystem paths from free text before it crosses the
/// validation bridge. Relative paths and ordinary slash-separated prose are
/// preserved. Unix, Windows drive, and UNC forms are replaced uniformly.
pub fn sanitize_bridge_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied_until = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let boundary = index == 0 || is_path_start_boundary(bytes[index - 1]);
        let file_uri = bytes
            .get(index..index.saturating_add(7))
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"file://"))
            && boundary;
        let unix = bytes[index] == b'/' && bytes.get(index + 1) != Some(&b'/') && boundary;
        let drive = bytes
            .get(index..index.saturating_add(3))
            .is_some_and(|prefix| {
                prefix[0].is_ascii_alphabetic()
                    && prefix[1] == b':'
                    && matches!(prefix[2], b'/' | b'\\')
                    && boundary
            });
        let unc = matches!(
            bytes.get(index..index.saturating_add(2)),
            Some(prefix) if prefix == b"\\\\" || prefix == b"//"
        ) && boundary;

        if file_uri || unix || drive || unc {
            let end = absolute_path_token_end(bytes, index);
            out.push_str(&text[copied_until..index]);
            out.push_str("<path>");
            copied_until = end;
            index = end;
        } else {
            index += 1;
        }
    }
    out.push_str(&text[copied_until..]);
    out
}

fn is_path_start_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'\'' | b'"' | b'`' | b'(' | b'[' | b'{' | b'<' | b'=' | b',' | b';' | b'!'
        )
}

fn absolute_path_token_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len()
        && !bytes[end].is_ascii_whitespace()
        && !matches!(
            bytes[end],
            b'\'' | b'"' | b'`' | b')' | b']' | b'}' | b'>' | b',' | b';' | b'|'
        )
    {
        end += 1;
    }
    end
}

fn sanitize_response_free_text(response: &mut ValidationBridgeResponse) {
    response.adapter_id = sanitize_bridge_text(&response.adapter_id);
    response.language = sanitize_bridge_text(&response.language);
    response.validation_kind = sanitize_bridge_text(&response.validation_kind);
    response.failure_kind = response
        .failure_kind
        .take()
        .map(|text| sanitize_bridge_text(&text));
    response.stderr_summary = response
        .stderr_summary
        .take()
        .map(|text| sanitize_bridge_text(&text));
    response.message = response.message.take().map(bound_error_message);
    if let Some(diagnostics) = response.diagnostics.as_mut() {
        diagnostics.reason = diagnostics
            .reason
            .take()
            .map(|text| bound_error_message(text));
        for diagnostic in &mut diagnostics.diagnostics {
            diagnostic.message = sanitize_bridge_text(&diagnostic.message);
            diagnostic.code = diagnostic
                .code
                .take()
                .map(|text| sanitize_bridge_text(&text));
            diagnostic.rule = diagnostic
                .rule
                .take()
                .map(|text| sanitize_bridge_text(&text));
        }
    }
}

/// Validate project-relative path strings in a request (cwd/targets).
/// Rejects absolute paths, `..`, NUL/control characters, and empty segments.
pub fn validate_project_relative_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if trimmed.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("path cannot contain control characters".to_string());
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err("path must be project-relative".to_string());
    }
    // Windows drive / UNC
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        return Err("path must be project-relative".to_string());
    }
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        return Err("path must be project-relative".to_string());
    }
    if trimmed.contains("://") {
        return Err("path must be project-relative".to_string());
    }
    for component in trimmed.split(['/', '\\']) {
        if component == ".." {
            return Err("path cannot contain '..'".to_string());
        }
    }
    if trimmed.chars().count() > MAX_PATH_CHARS {
        return Err(format!("path exceeds {MAX_PATH_CHARS} characters"));
    }
    Ok(())
}

pub fn validate_bridge_request(request: &ValidationBridgeRequest) -> Result<(), String> {
    if request.protocol_version != VALIDATION_BRIDGE_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported protocol_version {}; expected {}",
            request.protocol_version, VALIDATION_BRIDGE_PROTOCOL_VERSION
        ));
    }
    if request.adapter_id.trim().is_empty() {
        return Err("adapter_id cannot be empty".to_string());
    }
    if request.language.trim().is_empty() {
        return Err("language cannot be empty".to_string());
    }
    if request.validation_kind.trim().is_empty() {
        return Err("validation_kind cannot be empty".to_string());
    }
    if request.project_id.trim().is_empty() {
        return Err("project_id cannot be empty".to_string());
    }
    if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&request.timeout_secs) {
        return Err(format!(
            "timeout_secs must be between {MIN_TIMEOUT_SECS} and {MAX_TIMEOUT_SECS}"
        ));
    }
    if request.targets.len() > MAX_TARGETS {
        return Err(format!("at most {MAX_TARGETS} targets are allowed"));
    }
    if let Some(cwd) = request.cwd.as_deref() {
        validate_project_relative_path(cwd)?;
    }
    for target in &request.targets {
        validate_project_relative_path(target)?;
    }
    Ok(())
}

pub fn parse_validation_bridge_result_envelope(
    stdout: &str,
) -> Result<ValidationBridgeResultEnvelope, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "{}: empty validation bridge result",
            failure_kinds::PROTOCOL_ERROR
        ));
    }
    let envelope: ValidationBridgeResultEnvelope =
        serde_json::from_str(trimmed).map_err(|error| {
            format!(
                "{}: {}",
                failure_kinds::PROTOCOL_ERROR,
                bound_error_message(error.to_string())
            )
        })?;
    if envelope.format != VALIDATION_BRIDGE_RESULT_FORMAT {
        return Err(bound_error_message(format!(
            "{}: unexpected format {:?}",
            failure_kinds::PROTOCOL_ERROR,
            envelope.format
        )));
    }
    if envelope.success {
        if envelope.result.is_none() {
            return Err(format!(
                "{}: success envelope missing result",
                failure_kinds::PROTOCOL_ERROR
            ));
        }
    } else if envelope.error.is_none() {
        return Err(format!(
            "{}: failure envelope missing error",
            failure_kinds::PROTOCOL_ERROR
        ));
    }
    Ok(envelope)
}

/// Helper for tests / future server mapping: ensure a Value has no absolute path strings.
pub fn value_contains_absolute_path_leak(value: &Value) -> bool {
    match value {
        Value::String(s) => {
            sanitize_bridge_text(s) != *s
                || s.starts_with('/')
                || (s.len() >= 2
                    && s.as_bytes()[1] == b':'
                    && s.as_bytes()[0].is_ascii_alphabetic())
                || s.contains("file://")
        }
        Value::Array(items) => items.iter().any(value_contains_absolute_path_leak),
        Value::Object(map) => map.values().any(value_contains_absolute_path_leak),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn successful_response_fixture() -> ValidationBridgeResponse {
        ValidationBridgeResponse {
            protocol_version: VALIDATION_BRIDGE_PROTOCOL_VERSION,
            adapter_id: "pyright".into(),
            language: "python".into(),
            validation_kind: "typecheck".into(),
            success: true,
            command_started: true,
            exit_code: Some(0),
            duration_ms: 12,
            failure_kind: None,
            diagnostics: Some(BridgeDiagnostics {
                available: true,
                reason: None,
                diagnostics: vec![],
                diagnostic_count: 0,
                returned_diagnostic_count: 0,
                diagnostics_truncated: false,
                invalid_diagnostics_omitted: 0,
                external_results_omitted: 0,
                summary_error_count: Some(0),
                summary_warning_count: Some(0),
                summary_information_count: Some(0),
            }),
            tool_available: true,
            stdout_bytes: 10,
            stdout_capped: false,
            stderr_capped: false,
            stderr_summary: None,
            message: None,
        }
    }

    #[test]
    fn bridge_text_sanitizer_redacts_absolute_paths_without_damaging_normal_text() {
        let cases = [
            "/root/git/private-drop/src/app.py",
            "/etc/passwd",
            "/tmp/private-file",
            "file:///root/git/private-drop/src/app.py",
            r"C:\Users\alice\project\app.py",
            "D:/work/project/app.py",
            r"\\server\share\secret.py",
        ];
        for path in cases {
            let sanitized = sanitize_bridge_text(&format!("validation failed at {path}: denied"));
            assert!(!sanitized.contains(path), "path leaked: {sanitized}");
            assert!(sanitized.contains("<path>"), "path not marked: {sanitized}");
            assert!(sanitized.contains("validation failed at"));
        }

        let ordinary = "type mismatch: expected int/string; retry 1/2";
        assert_eq!(sanitize_bridge_text(ordinary), ordinary);
    }

    #[test]
    fn bridge_error_sanitizes_spawn_path_before_serialization() {
        let injected = "/root/git/private-drop/bin/pyright";
        let envelope = ValidationBridgeResultEnvelope::err(
            failure_kinds::SPAWN_FAILED,
            format!("spawn failed for {injected}: permission denied"),
        );
        assert!(!envelope.error.as_ref().unwrap().message.contains(injected));
        assert!(!envelope.to_stdout_json().contains(injected));
    }

    #[test]
    fn envelope_success_is_transport_success_not_validation_verdict() {
        let mut response = successful_response_fixture();
        response.success = false;
        response.failure_kind = Some(failure_kinds::COMPILE_ERROR.to_string());
        let envelope = ValidationBridgeResultEnvelope::ok(response);
        assert!(envelope.success);
        assert!(!envelope.result.as_ref().unwrap().success);
        assert_eq!(
            envelope.result.as_ref().unwrap().failure_kind.as_deref(),
            Some(failure_kinds::COMPILE_ERROR)
        );
    }

    #[test]
    fn result_parser_sanitizes_unexpected_format_text() {
        let injected = "/root/git/private-drop/private-format";
        let stdout = format!(
            r#"{{"format":"{injected}","success":false,"error":{{"code":"x","message":"x"}}}}"#
        );
        let error = parse_validation_bridge_result_envelope(&stdout).unwrap_err();
        assert!(!error.contains(injected));
        assert!(error.contains("<path>"));
    }

    #[test]
    fn validate_project_relative_path_rejects_escapes() {
        assert!(validate_project_relative_path("src/app.py").is_ok());
        assert!(validate_project_relative_path("/abs/path").is_err());
        assert!(validate_project_relative_path("../secret").is_err());
        assert!(validate_project_relative_path("a/../b").is_err());
        assert!(validate_project_relative_path("a\0b").is_err());
        assert!(validate_project_relative_path("C:\\windows").is_err());
        assert!(validate_project_relative_path("file://x").is_err());
    }

    #[test]
    fn request_validation_requires_protocol_and_timeout_bounds() {
        let mut req = ValidationBridgeRequest {
            protocol_version: VALIDATION_BRIDGE_PROTOCOL_VERSION,
            adapter_id: "pyright".into(),
            language: "python".into(),
            validation_kind: "typecheck".into(),
            project_id: "demo".into(),
            cwd: None,
            targets: vec![],
            timeout_secs: 60,
            options: ValidationBridgeOptions::default(),
        };
        assert!(validate_bridge_request(&req).is_ok());
        req.timeout_secs = 300;
        assert!(validate_bridge_request(&req).is_err());
        req.timeout_secs = 60;
        req.protocol_version = 99;
        assert!(validate_bridge_request(&req).is_err());
        req.protocol_version = VALIDATION_BRIDGE_PROTOCOL_VERSION;
        req.cwd = Some("/abs".into());
        assert!(validate_bridge_request(&req).is_err());
    }

    #[test]
    fn envelope_roundtrip() {
        let response = successful_response_fixture();
        let env = ValidationBridgeResultEnvelope::ok(response);
        let parsed = parse_validation_bridge_result_envelope(&env.to_stdout_json()).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.result.as_ref().unwrap().adapter_id, "pyright");
    }
}
