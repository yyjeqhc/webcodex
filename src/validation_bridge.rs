//! Typed agent validation bridge contract (multi-language validation).
//!
//! Shared by the server runtime and `webcodex-agent`. Like `lsp_bridge`, this
//! module exposes only a fixed, closed set of read-only validation operations —
//! never arbitrary commands, tool flags, or absolute project roots. The agent
//! runs the language's validator, relativizes and sanitizes its output, and
//! returns this typed result; the server never receives raw tool output or
//! absolute paths.
//!
//! Path relativization happens on the agent because structured validators
//! (pyright, ruff, eslint) emit absolute paths that only the agent can resolve
//! against the canonical project root — the same reason LSP navigation
//! relativizes agent-side. See docs/MULTI_LANGUAGE_VALIDATION.md.

use crate::lsp_bridge::bound_error_message;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const AGENT_VALIDATION_RESULT_FORMAT: &str = "webcodex.agent_validation_result.v1";
pub const AGENT_VALIDATION_REQUEST_KIND: &str = "validation";

/// Max diagnostics / failed tests returned in one validation result. Matches
/// the Rust `validation_parser` caps so downstream aggregation is uniform.
pub const MAX_VALIDATION_DIAGNOSTICS: usize = 20;
pub const MAX_VALIDATION_FAILED_TESTS: usize = 20;
pub const MAX_VALIDATION_MESSAGE_CHARS: usize = 240;
pub const MAX_VALIDATION_CODE_CHARS: usize = 64;
pub const MAX_VALIDATION_TEST_NAME_CHARS: usize = 240;

/// Stable error codes for the agent validation bridge and public tools.
pub mod validation_error_codes {
    pub const AGENT_CAPABILITY_UNAVAILABLE: &str = "agent_capability_unavailable";
    pub const VALIDATION_TOOL_UNAVAILABLE: &str = "validation_tool_unavailable";
    pub const VALIDATION_TOOL_FAILED: &str = "validation_tool_failed";
    pub const VALIDATION_TIMEOUT: &str = "validation_timeout";
    pub const UNSUPPORTED_LANGUAGE: &str = "unsupported_language";
    pub const LANGUAGE_REQUIRED: &str = "language_required";
    pub const INVALID_PROJECT_PATH: &str = "invalid_project_path";
    pub const UNKNOWN_PROJECT: &str = "unknown_project";
    pub const MISSING_VALIDATION_PAYLOAD: &str = "missing_validation_payload";
    pub const MALFORMED_AGENT_VALIDATION_RESULT: &str = "malformed_agent_validation_result";
}

pub fn is_known_validation_error_code(code: &str) -> bool {
    use validation_error_codes::*;
    matches!(
        code,
        AGENT_CAPABILITY_UNAVAILABLE
            | VALIDATION_TOOL_UNAVAILABLE
            | VALIDATION_TOOL_FAILED
            | VALIDATION_TIMEOUT
            | UNSUPPORTED_LANGUAGE
            | LANGUAGE_REQUIRED
            | INVALID_PROJECT_PATH
            | UNKNOWN_PROJECT
            | MISSING_VALIDATION_PAYLOAD
            | MALFORMED_AGENT_VALIDATION_RESULT
    )
}

/// One read-only validation operation. Unknown `operation` values fail serde,
/// so old agents and future wire formats cannot treat them as shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum AgentValidationRequest {
    /// Type-check (rust: n/a here; python: pyright; ts: tsc).
    Typecheck {
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// Run the project's tests (python: pytest; ts: vitest/jest).
    Test {
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        filter: Option<String>,
    },
    /// Lint (python: ruff; ts: eslint).
    Lint {
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// Formatting check only, never a write (python: black --check; ts: prettier --check).
    FormatCheck {
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
}

impl AgentValidationRequest {
    /// Public validation-kind label used in results and evidence.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Typecheck { .. } => "typecheck",
            Self::Test { .. } => "test",
            Self::Lint { .. } => "lint",
            Self::FormatCheck { .. } => "format",
        }
    }
}

/// Agent-side validation payload. `project_id` is the agent-local project id
/// already resolved by the server. Absolute roots are never included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentValidationPayload {
    pub project_id: String,
    pub request: AgentValidationRequest,
}

/// One normalized diagnostic. Paths are project-relative; positions are
/// 1-based. Never carries absolute paths, secrets, or raw output bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationDiagnostic {
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationTestSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationFailedTest {
    pub name: String,
    pub failure_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// Result of one validation run. Bounded, relativized, secret-safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRunResult {
    pub project: String,
    pub language: String,
    pub tool: String,
    pub kind: String,
    /// The validator executable resolved and ran.
    pub available: bool,
    /// True when the run reported no failures (no error diagnostics / no failed
    /// tests / no formatting diff).
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub diagnostics: Vec<ValidationDiagnostic>,
    pub total_diagnostic_count: usize,
    pub returned_diagnostic_count: usize,
    pub errors_count: usize,
    pub warnings_count: usize,
    pub diagnostics_truncated: bool,
    /// Diagnostics dropped because their path resolved outside the project.
    pub external_results_omitted: usize,
    /// Diagnostics dropped as malformed/unsafe.
    pub invalid_results_omitted: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_summary: Option<ValidationTestSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_tests: Vec<ValidationFailedTest>,
    #[serde(default)]
    pub failed_tests_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentValidationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentValidationResultEnvelope {
    pub format: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentValidationError>,
}

impl AgentValidationResultEnvelope {
    #[allow(dead_code)] // Constructed by the webcodex-agent production target.
    pub fn ok(result: impl Serialize) -> Self {
        Self {
            format: AGENT_VALIDATION_RESULT_FORMAT.to_string(),
            success: true,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }

    #[allow(dead_code)] // Constructed by the webcodex-agent production target.
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            format: AGENT_VALIDATION_RESULT_FORMAT.to_string(),
            success: false,
            result: None,
            error: Some(AgentValidationError {
                code: code.into(),
                message: bound_error_message(message),
            }),
        }
    }

    #[allow(dead_code)] // Serialized by the webcodex-agent production target.
    pub fn to_stdout_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"format":"webcodex.agent_validation_result.v1","success":false,"error":{"code":"malformed_agent_validation_result","message":"failed to serialize result"}}"#.to_string()
        })
    }
}

/// Parse a strict versioned agent validation result envelope from stdout.
pub fn parse_agent_validation_result_envelope(
    stdout: &str,
) -> Result<AgentValidationResultEnvelope, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "{}: empty agent validation result",
            validation_error_codes::MALFORMED_AGENT_VALIDATION_RESULT
        ));
    }
    let envelope: AgentValidationResultEnvelope = serde_json::from_str(trimmed).map_err(|error| {
        format!(
            "{}: {}",
            validation_error_codes::MALFORMED_AGENT_VALIDATION_RESULT,
            bound_error_message(error.to_string())
        )
    })?;
    if envelope.format != AGENT_VALIDATION_RESULT_FORMAT {
        return Err(format!(
            "{}: unexpected format {:?}",
            validation_error_codes::MALFORMED_AGENT_VALIDATION_RESULT,
            envelope.format
        ));
    }
    if envelope.success {
        if envelope.result.is_none() {
            return Err(format!(
                "{}: success envelope missing result",
                validation_error_codes::MALFORMED_AGENT_VALIDATION_RESULT
            ));
        }
    } else if envelope.error.is_none() {
        return Err(format!(
            "{}: failure envelope missing error",
            validation_error_codes::MALFORMED_AGENT_VALIDATION_RESULT
        ));
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serde_roundtrip_and_unknown_operation_rejected() {
        let cases = vec![
            AgentValidationRequest::Typecheck {
                language: Some("python".to_string()),
                cwd: None,
            },
            AgentValidationRequest::Test {
                language: None,
                cwd: Some("backend".to_string()),
                filter: Some("test_login".to_string()),
            },
            AgentValidationRequest::Lint {
                language: Some("python".to_string()),
                cwd: None,
            },
            AgentValidationRequest::FormatCheck {
                language: None,
                cwd: None,
            },
        ];
        for request in cases {
            let payload = AgentValidationPayload {
                project_id: "demo".to_string(),
                request: request.clone(),
            };
            let json = serde_json::to_string(&payload).unwrap();
            let back: AgentValidationPayload = serde_json::from_str(&json).unwrap();
            assert_eq!(back, payload);
        }

        let bad = r#"{"project_id":"p","request":{"operation":"arbitrary","command":"rm -rf /"}}"#;
        assert!(serde_json::from_str::<AgentValidationPayload>(bad).is_err());
    }

    #[test]
    fn kind_labels_are_stable() {
        assert_eq!(
            AgentValidationRequest::Typecheck {
                language: None,
                cwd: None
            }
            .kind_label(),
            "typecheck"
        );
        assert_eq!(
            AgentValidationRequest::Test {
                language: None,
                cwd: None,
                filter: None
            }
            .kind_label(),
            "test"
        );
        assert_eq!(
            AgentValidationRequest::Lint {
                language: None,
                cwd: None
            }
            .kind_label(),
            "lint"
        );
        assert_eq!(
            AgentValidationRequest::FormatCheck {
                language: None,
                cwd: None
            }
            .kind_label(),
            "format"
        );
    }

    #[test]
    fn envelope_roundtrip_and_strict_parse() {
        let ok = AgentValidationResultEnvelope::ok(ValidationRunResult {
            project: "demo".to_string(),
            language: "python".to_string(),
            tool: "pyright".to_string(),
            kind: "typecheck".to_string(),
            available: true,
            passed: true,
            exit_code: Some(0),
            duration_ms: Some(120),
            diagnostics: Vec::new(),
            total_diagnostic_count: 0,
            returned_diagnostic_count: 0,
            errors_count: 0,
            warnings_count: 0,
            diagnostics_truncated: false,
            external_results_omitted: 0,
            invalid_results_omitted: 0,
            test_summary: None,
            failed_tests: Vec::new(),
            failed_tests_truncated: false,
        });
        let stdout = ok.to_stdout_json();
        let parsed = parse_agent_validation_result_envelope(&stdout).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.format, AGENT_VALIDATION_RESULT_FORMAT);

        assert!(parse_agent_validation_result_envelope("not-json").is_err());
        assert!(parse_agent_validation_result_envelope(
            r#"{"format":"other","success":true,"result":{}}"#
        )
        .is_err());
        assert!(parse_agent_validation_result_envelope(
            r#"{"format":"webcodex.agent_validation_result.v1","success":true}"#
        )
        .is_err());
        assert!(is_known_validation_error_code("validation_tool_unavailable"));
        assert!(!is_known_validation_error_code("nope"));
    }
}
