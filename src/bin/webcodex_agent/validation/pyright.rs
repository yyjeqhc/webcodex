//! Internal Pyright adapter: command construction, execution, JSON parse.
//!
//! Command argv is built only on the agent. Structured JSON is captured in full
//! up to a hard byte cap; oversized output is never tail-parsed.

use super::execute::{resolve_executable, run_bounded};
use super::path::relativize_path;
use super::{base_response, registry};
use crate::validation_bridge::{
    bound_error_message, failure_kinds, sanitize_bridge_text, BridgeDiagnostic, BridgeDiagnostics,
    ValidationBridgeRequest, ValidationBridgeResponse, MAX_BRIDGE_DIAGNOSTICS,
    MAX_DIAGNOSTIC_MESSAGE_CHARS, MAX_RULE_CHARS, MAX_VALIDATION_STDOUT_BYTES,
};
use serde_json::Value;
use std::cmp::Ordering;
use std::path::Path;

const ADAPTER_ID: &str = "pyright";

pub(crate) fn run_pyright(
    project_root: &Path,
    request: &ValidationBridgeRequest,
    max_timeout_secs: u64,
) -> ValidationBridgeResponse {
    let meta = registry::lookup_adapter(ADAPTER_ID).expect("pyright adapter registered");
    let timeout = request.timeout_secs.min(max_timeout_secs).max(1);

    let Some(program) = resolve_executable(meta.env_override, meta.executable_name) else {
        let mut response = base_response(request, false);
        response.failure_kind = Some(failure_kinds::TOOL_UNAVAILABLE.to_string());
        response.message = Some("pyright executable is not available".to_string());
        return response;
    };

    let cwd = match request.cwd.as_deref() {
        Some(rel) => match super::path::resolve_cwd_under_project(project_root, rel) {
            Ok(path) => path,
            Err(message) => {
                let mut response = base_response(request, true);
                response.failure_kind = Some(failure_kinds::INVALID_ARGUMENTS.to_string());
                response.message = Some(message);
                return response;
            }
        },
        None => project_root.to_path_buf(),
    };

    let mut args = vec!["--outputjson".to_string()];
    for target in &request.targets {
        match super::path::resolve_under_project(project_root, target) {
            Ok(path) => {
                // Pass project-relative path when possible so pyright output is
                // easier to relativize; fall back to absolute for the process.
                if let Ok(rel) = path.strip_prefix(project_root) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    args.push(if rel.is_empty() { ".".to_string() } else { rel });
                } else {
                    args.push(path.to_string_lossy().to_string());
                }
            }
            Err(message) => {
                let mut response = base_response(request, true);
                response.failure_kind = Some(failure_kinds::INVALID_ARGUMENTS.to_string());
                response.message = Some(message);
                return response;
            }
        }
    }

    let captured = run_bounded(&program, &args, &cwd, timeout);
    let mut response = base_response(request, true);
    response.command_started = captured.spawn_error.is_none();
    response.duration_ms = captured.duration_ms;
    response.exit_code = captured.exit_code;
    response.stdout_bytes = if captured.stdout_capped {
        MAX_VALIDATION_STDOUT_BYTES as u64 + 1
    } else {
        captured.stdout.len() as u64
    };
    response.stdout_capped = captured.stdout_capped;
    response.stderr_capped = captured.stderr_capped;
    response.stderr_summary = captured.stderr_summary.clone();

    if let Some(spawn_error) = captured.spawn_error {
        response.command_started = false;
        response.failure_kind = Some(failure_kinds::SPAWN_FAILED.to_string());
        response.message = Some(bound_error_message(spawn_error));
        return response;
    }

    if captured.timed_out {
        response.failure_kind = Some(failure_kinds::TIMEOUT.to_string());
        response.message = Some(format!("pyright timed out after {timeout} seconds"));
        return response;
    }

    if let Some(wait_error) = captured.wait_error {
        response.failure_kind = Some(failure_kinds::PROCESS_EXIT.to_string());
        response.message = Some(bound_error_message(wait_error));
        return response;
    }

    if captured.stdout_capped {
        response.failure_kind = Some(failure_kinds::OUTPUT_TOO_LARGE.to_string());
        response.message = Some(format!(
            "pyright stdout exceeded {MAX_VALIDATION_STDOUT_BYTES} bytes; complete JSON was not captured and will not be parsed"
        ));
        return response;
    }

    let stdout_text = match std::str::from_utf8(&captured.stdout) {
        Ok(text) => text,
        Err(_) => {
            response.failure_kind = Some(failure_kinds::MALFORMED_OUTPUT.to_string());
            response.message = Some("pyright stdout is not valid UTF-8".to_string());
            return response;
        }
    };

    if stdout_text.trim().is_empty() {
        response.failure_kind = Some(failure_kinds::MALFORMED_OUTPUT.to_string());
        response.message = Some("pyright produced empty stdout instead of JSON".to_string());
        return response;
    }

    match parse_pyright_for_status(project_root, stdout_text) {
        Ok(parsed) => {
            let status =
                classify_pyright_status(captured.exit_code, parsed.effective_error_count());
            response.diagnostics = Some(parsed.into_bridge());
            match status {
                PyrightStatus::Success => {
                    response.success = true;
                    response.failure_kind = None;
                }
                PyrightStatus::CompileError => {
                    response.failure_kind = Some(failure_kinds::COMPILE_ERROR.to_string());
                }
                PyrightStatus::ProcessExit => {
                    response.failure_kind = Some(failure_kinds::PROCESS_EXIT.to_string());
                    response.message = Some(match captured.exit_code {
                        Some(code) => format!(
                            "pyright exited with status code {code} without error diagnostics"
                        ),
                        None => "pyright terminated without an exit code".to_string(),
                    });
                }
            }
            response
        }
        Err(message) => {
            response.failure_kind = Some(failure_kinds::MALFORMED_OUTPUT.to_string());
            response.message = Some(bound_error_message(message));
            // If JSON is unusable, do not pretend validation passed solely on exit.
            response.success = false;
            response
        }
    }
}

struct ParsedPyright {
    diagnostics: Vec<BridgeDiagnostic>,
    diagnostic_count: usize,
    returned_diagnostic_count: usize,
    diagnostics_truncated: bool,
    invalid_diagnostics_omitted: usize,
    external_results_omitted: usize,
    summary_error_count: Option<u64>,
    summary_warning_count: Option<u64>,
    summary_information_count: Option<u64>,
    observed_error_count: u64,
}

impl ParsedPyright {
    fn effective_error_count(&self) -> u64 {
        self.summary_error_count
            .unwrap_or(0)
            .max(self.observed_error_count)
    }

    fn into_bridge(self) -> BridgeDiagnostics {
        BridgeDiagnostics {
            available: true,
            reason: None,
            diagnostics: self.diagnostics,
            diagnostic_count: self.diagnostic_count,
            returned_diagnostic_count: self.returned_diagnostic_count,
            diagnostics_truncated: self.diagnostics_truncated,
            invalid_diagnostics_omitted: self.invalid_diagnostics_omitted,
            external_results_omitted: self.external_results_omitted,
            summary_error_count: self.summary_error_count,
            summary_warning_count: self.summary_warning_count,
            summary_information_count: self.summary_information_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PyrightStatus {
    Success,
    CompileError,
    ProcessExit,
}

fn classify_pyright_status(exit_code: Option<i32>, error_count: u64) -> PyrightStatus {
    match (exit_code, error_count > 0) {
        (None, _) => PyrightStatus::ProcessExit,
        (Some(_), true) => PyrightStatus::CompileError,
        (Some(0), false) => PyrightStatus::Success,
        (Some(_), false) => PyrightStatus::ProcessExit,
    }
}

/// Parse complete Pyright `--outputjson` body. Never call with truncated JSON.
#[cfg(test)]
pub(crate) fn parse_pyright_json(
    project_root: &Path,
    stdout: &str,
) -> Result<BridgeDiagnostics, String> {
    parse_pyright_for_status(project_root, stdout).map(ParsedPyright::into_bridge)
}

fn parse_pyright_for_status(project_root: &Path, stdout: &str) -> Result<ParsedPyright, String> {
    let value: Value = serde_json::from_str(stdout)
        .map_err(|error| format!("pyright JSON parse failed: {error}"))?;
    parse_pyright_value(project_root, &value)
}

fn parse_pyright_value(project_root: &Path, value: &Value) -> Result<ParsedPyright, String> {
    let general = value
        .get("generalDiagnostics")
        .and_then(Value::as_array)
        .ok_or_else(|| "pyright JSON missing generalDiagnostics array".to_string())?;

    let summary = value.get("summary");
    let summary_error_count = summary
        .and_then(|s| s.get("errorCount"))
        .and_then(Value::as_u64);
    let summary_warning_count = summary
        .and_then(|s| s.get("warningCount"))
        .and_then(Value::as_u64);
    let summary_information_count = summary
        .and_then(|s| s.get("informationCount"))
        .and_then(Value::as_u64);

    let mut diagnostics = Vec::new();
    let mut invalid = 0usize;
    let mut external = 0usize;
    let mut observed_error_count = 0u64;
    let mut seen = std::collections::BTreeSet::new();

    for item in general {
        if item
            .get("severity")
            .and_then(Value::as_str)
            .is_some_and(|severity| severity.eq_ignore_ascii_case("error"))
        {
            observed_error_count = observed_error_count.saturating_add(1);
        }
        match parse_one_diagnostic(project_root, item) {
            Ok(diag) => {
                let key = (
                    diag.file.clone().unwrap_or_default(),
                    diag.line.unwrap_or(0),
                    diag.column.unwrap_or(0),
                    diag.end_line.unwrap_or(0),
                    diag.end_column.unwrap_or(0),
                    diag.severity.clone(),
                    diag.rule.clone().unwrap_or_default(),
                    diag.message.clone(),
                );
                if seen.insert(key) {
                    diagnostics.push(diag);
                }
            }
            Err(OmitReason::External) => external += 1,
            Err(OmitReason::Invalid) => invalid += 1,
        }
    }

    diagnostics.sort_by(cmp_diagnostics);
    let diagnostic_count = diagnostics.len();
    let diagnostics_truncated = diagnostic_count > MAX_BRIDGE_DIAGNOSTICS;
    diagnostics.truncate(MAX_BRIDGE_DIAGNOSTICS);
    let returned = diagnostics.len();

    Ok(ParsedPyright {
        diagnostics,
        diagnostic_count,
        returned_diagnostic_count: returned,
        diagnostics_truncated,
        invalid_diagnostics_omitted: invalid,
        external_results_omitted: external,
        summary_error_count,
        summary_warning_count,
        summary_information_count,
        observed_error_count,
    })
}

enum OmitReason {
    External,
    Invalid,
}

fn parse_one_diagnostic(project_root: &Path, item: &Value) -> Result<BridgeDiagnostic, OmitReason> {
    let file_raw = item.get("file").and_then(Value::as_str).unwrap_or("");
    if file_raw.is_empty() {
        return Err(OmitReason::Invalid);
    }
    let file = match relativize_path(project_root, file_raw) {
        Some(rel) => Some(rel),
        None => {
            // Absolute outside project, or unresolvable.
            if Path::new(file_raw).is_absolute() {
                return Err(OmitReason::External);
            }
            return Err(OmitReason::Invalid);
        }
    };

    let severity = map_severity(item.get("severity").and_then(Value::as_str).unwrap_or(""));
    let message = item
        .get("message")
        .and_then(Value::as_str)
        .map(bound_message)
        .filter(|m| !m.is_empty())
        .ok_or(OmitReason::Invalid)?;

    let rule = item
        .get("rule")
        .and_then(Value::as_str)
        .map(bound_rule)
        .filter(|r| !r.is_empty());

    let range = item.get("range");
    let (line, column, end_line, end_column) = match range {
        Some(range) => {
            let start = range.get("start");
            let end = range.get("end");
            let line = zero_to_one(start.and_then(|s| s.get("line")).and_then(Value::as_u64));
            let column = zero_to_one(
                start
                    .and_then(|s| s.get("character"))
                    .and_then(Value::as_u64),
            );
            let end_line = zero_to_one(end.and_then(|s| s.get("line")).and_then(Value::as_u64));
            let end_column =
                zero_to_one(end.and_then(|s| s.get("character")).and_then(Value::as_u64));
            // Invalid range: missing start line/col → omit positions but keep diag if message ok.
            if line.is_none() || column.is_none() {
                (None, None, None, None)
            } else {
                (line, column, end_line, end_column)
            }
        }
        None => (None, None, None, None),
    };

    Ok(BridgeDiagnostic {
        severity: severity.to_string(),
        code: rule.clone(),
        rule,
        file,
        line,
        column,
        end_line,
        end_column,
        message,
    })
}

fn map_severity(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "error" => "error",
        "warning" => "warning",
        "information" | "info" => "information",
        // Pyright may emit "unused" etc.; degrade safely.
        _ => "warning",
    }
}

fn zero_to_one(value: Option<u64>) -> Option<u64> {
    value.map(|v| v.saturating_add(1))
}

fn bound_message(value: &str) -> String {
    let cleaned: String = sanitize_bridge_text(value)
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(MAX_DIAGNOSTIC_MESSAGE_CHARS)
        .collect();
    cleaned
}

fn bound_rule(value: &str) -> String {
    sanitize_bridge_text(value)
        .chars()
        .take(MAX_RULE_CHARS)
        .collect()
}

fn cmp_diagnostics(a: &BridgeDiagnostic, b: &BridgeDiagnostic) -> Ordering {
    a.file
        .cmp(&b.file)
        .then(a.line.cmp(&b.line))
        .then(a.column.cmp(&b.column))
        .then(a.severity.cmp(&b.severity))
        .then(a.rule.cmp(&b.rule))
        .then(a.message.cmp(&b.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_json(file: &str) -> String {
        format!(
            r#"{{
  "version": "1.1.382",
  "generalDiagnostics": [
    {{
      "file": "{file}",
      "severity": "error",
      "message": "Type \"str\" is not assignable to declared type \"int\"",
      "rule": "reportAssignmentType",
      "range": {{
        "start": {{ "line": 2, "character": 4 }},
        "end": {{ "line": 2, "character": 9 }}
      }}
    }},
    {{
      "file": "{file}",
      "severity": "warning",
      "message": "Unused variable",
      "rule": "reportUnusedVariable",
      "range": {{
        "start": {{ "line": 5, "character": 0 }},
        "end": {{ "line": 5, "character": 1 }}
      }}
    }}
  ],
  "summary": {{
    "filesAnalyzed": 1,
    "errorCount": 1,
    "warningCount": 1,
    "informationCount": 0,
    "timeInSec": 0.01
  }}
}}"#
        )
    }

    #[test]
    fn status_machine_never_succeeds_without_an_exit_code() {
        assert_eq!(classify_pyright_status(None, 0), PyrightStatus::ProcessExit);
        assert_eq!(classify_pyright_status(None, 2), PyrightStatus::ProcessExit);
    }

    #[test]
    fn parses_fixture_and_relativizes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("pkg")).unwrap();
        let file = root.join("pkg/mod.py");
        fs::write(&file, "x: int = 'a'\n").unwrap();
        let abs = fs::canonicalize(&file).unwrap();
        let json = sample_json(&abs.to_string_lossy().replace('\\', "\\\\"));
        let diags = parse_pyright_json(root, &json).unwrap();
        assert!(diags.available);
        assert_eq!(diags.diagnostic_count, 2);
        assert_eq!(diags.diagnostics[0].file.as_deref(), Some("pkg/mod.py"));
        assert_eq!(diags.diagnostics[0].line, Some(3)); // 0-based 2 → 1-based 3
        assert_eq!(diags.diagnostics[0].column, Some(5));
        assert_eq!(diags.diagnostics[0].severity, "error");
        assert_eq!(
            diags.diagnostics[0].rule.as_deref(),
            Some("reportAssignmentType")
        );
        assert_eq!(diags.summary_error_count, Some(1));
        assert!(!serde_json::to_string(&diags)
            .unwrap()
            .contains(abs.to_str().unwrap()));
    }

    #[test]
    fn omits_external_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.py");
        fs::write(&secret, "x\n").unwrap();
        let abs = fs::canonicalize(&secret).unwrap();
        let json = sample_json(&abs.to_string_lossy().replace('\\', "\\\\"));
        let diags = parse_pyright_json(root, &json).unwrap();
        assert_eq!(diags.diagnostic_count, 0);
        assert_eq!(diags.external_results_omitted, 2);
    }

    #[test]
    fn rejects_malformed_json() {
        let tmp = tempfile::tempdir().unwrap();
        let err = parse_pyright_json(tmp.path(), "{not json").unwrap_err();
        assert!(err.contains("parse failed"));
    }

    #[test]
    fn rejects_truncated_looking_json() {
        let tmp = tempfile::tempdir().unwrap();
        let err =
            parse_pyright_json(tmp.path(), r#"{"generalDiagnostics":[{"file":""}"#).unwrap_err();
        assert!(err.contains("parse failed"));
    }
}
