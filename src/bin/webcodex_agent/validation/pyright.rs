//! Parse `pyright --outputjson` into the shared, relativized validation model.
//!
//! Pyright emits a JSON object with a `summary` (authoritative error/warning
//! counts) and `generalDiagnostics[]` carrying **absolute** file paths and
//! 0-based LSP positions. This module relativizes paths against the canonical
//! project root (dropping anything outside it), converts to 1-based positions,
//! scrubs and bounds every field, caps the list, and sorts deterministically —
//! producing the same `ValidationDiagnostic` shape every language uses.

use crate::lsp_bridge::redact_absolute_paths;
use crate::validation_bridge::{
    ValidationDiagnostic, MAX_VALIDATION_CODE_CHARS, MAX_VALIDATION_DIAGNOSTICS,
    MAX_VALIDATION_MESSAGE_CHARS,
};
use serde_json::Value;
use std::cmp::Ordering;
use std::path::Path;

const MAX_FILE_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PyrightDiagnostics {
    pub(crate) diagnostics: Vec<ValidationDiagnostic>,
    /// Distinct valid, project-internal diagnostics observed (pre-cap).
    pub(crate) total_diagnostic_count: usize,
    pub(crate) returned_diagnostic_count: usize,
    /// Authoritative counts from pyright's own summary.
    pub(crate) errors_count: usize,
    pub(crate) warnings_count: usize,
    pub(crate) diagnostics_truncated: bool,
    pub(crate) external_results_omitted: usize,
    pub(crate) invalid_results_omitted: usize,
}

/// Parse pyright JSON. Returns `None` when the excerpt is not valid pyright
/// JSON (e.g. truncated), which the caller treats as a tool/protocol failure.
pub(crate) fn parse_pyright_output(json: &str, project_root: &Path) -> Option<PyrightDiagnostics> {
    let value: Value = serde_json::from_str(json.trim()).ok()?;
    let object = value.as_object()?;
    // A valid pyright report always carries a summary object.
    let summary = object.get("summary")?.as_object()?;
    let errors_count = summary
        .get("errorCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let warnings_count = summary
        .get("warningCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;

    let mut diagnostics = Vec::new();
    let mut external = 0usize;
    let mut invalid = 0usize;
    if let Some(items) = object.get("generalDiagnostics").and_then(Value::as_array) {
        for item in items {
            match normalize_pyright_diagnostic(item, project_root) {
                DiagnosticNormalize::Ok(diagnostic) => diagnostics.push(diagnostic),
                DiagnosticNormalize::External => external += 1,
                DiagnosticNormalize::Invalid => invalid += 1,
            }
        }
    }

    diagnostics.sort_by(compare_diagnostics);
    diagnostics.dedup();
    let total_diagnostic_count = diagnostics.len();
    let diagnostics_truncated = total_diagnostic_count > MAX_VALIDATION_DIAGNOSTICS;
    diagnostics.truncate(MAX_VALIDATION_DIAGNOSTICS);

    Some(PyrightDiagnostics {
        returned_diagnostic_count: diagnostics.len(),
        diagnostics,
        total_diagnostic_count,
        errors_count,
        warnings_count,
        diagnostics_truncated,
        external_results_omitted: external,
        invalid_results_omitted: invalid,
    })
}

enum DiagnosticNormalize {
    Ok(ValidationDiagnostic),
    External,
    Invalid,
}

fn normalize_pyright_diagnostic(value: &Value, project_root: &Path) -> DiagnosticNormalize {
    let Some(object) = value.as_object() else {
        return DiagnosticNormalize::Invalid;
    };
    let severity = match object.get("severity").and_then(Value::as_str) {
        Some("error") => "error",
        Some("warning") => "warning",
        Some("information") => "information",
        // `hint`/unknown severities are not surfaced as validation findings.
        _ => return DiagnosticNormalize::Invalid,
    };
    let Some(raw_message) = object.get("message").and_then(Value::as_str) else {
        return DiagnosticNormalize::Invalid;
    };
    let Some(message) = bound_field(raw_message, MAX_VALIDATION_MESSAGE_CHARS) else {
        return DiagnosticNormalize::Invalid;
    };
    if looks_sensitive(&message) {
        return DiagnosticNormalize::Invalid;
    }

    // `file` is absolute; relativize against the canonical project root.
    let file = match object.get("file").and_then(Value::as_str) {
        Some(path) => match relativize(path, project_root) {
            PathClass::Inside(relative) => Some(relative),
            PathClass::Outside => return DiagnosticNormalize::External,
            PathClass::Invalid => return DiagnosticNormalize::Invalid,
        },
        None => None,
    };

    // pyright ranges are 0-based; convert start to 1-based.
    let (line, column) = match object.get("range").and_then(Value::as_object) {
        Some(range) => {
            let start = range.get("start").and_then(Value::as_object);
            let line = start
                .and_then(|s| s.get("line"))
                .and_then(Value::as_u64)
                .and_then(|l| u32::try_from(l.saturating_add(1)).ok());
            let column = start
                .and_then(|s| s.get("character"))
                .and_then(Value::as_u64)
                .and_then(|c| u32::try_from(c.saturating_add(1)).ok());
            (line, column)
        }
        None => (None, None),
    };

    let code = object
        .get("rule")
        .and_then(Value::as_str)
        .and_then(|rule| bound_field(rule, MAX_VALIDATION_CODE_CHARS))
        .filter(|code| !code.is_empty());

    DiagnosticNormalize::Ok(ValidationDiagnostic {
        severity: severity.to_string(),
        code,
        file,
        line,
        column,
        message,
    })
}

enum PathClass {
    Inside(String),
    Outside,
    Invalid,
}

/// Lexically relativize `absolute` against the canonical `project_root`.
/// pyright emits canonical absolute paths, so `strip_prefix` is sufficient and
/// filesystem-free; the residue is re-checked to reject any escape.
fn relativize(absolute: &str, project_root: &Path) -> PathClass {
    let path = Path::new(absolute);
    if !path.is_absolute() {
        return PathClass::Invalid;
    }
    let Ok(relative) = path.strip_prefix(project_root) else {
        return PathClass::Outside;
    };
    let Some(text) = relative.to_str() else {
        return PathClass::Invalid;
    };
    let text = text.replace('\\', "/");
    if text.is_empty()
        || text.starts_with('/')
        || text.split('/').any(|part| part.is_empty() || part == "..")
        || text.chars().count() > MAX_FILE_CHARS
    {
        return PathClass::Invalid;
    }
    PathClass::Inside(text)
}

/// Redact absolute-path material, collapse control characters, trim, and
/// truncate by Unicode scalar count with an ellipsis marker.
fn bound_field(value: &str, max_chars: usize) -> Option<String> {
    let sanitized = redact_absolute_paths(value)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        return None;
    }
    if sanitized.chars().count() <= max_chars {
        return Some(sanitized.to_string());
    }
    Some(
        sanitized
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
            + "…",
    )
}

/// Drop diagnostics whose text looks like it carries a credential. Mirrors the
/// Rust validation parser's conservative secret guard.
fn looks_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !matches!(*ch, '_' | '-') && !ch.is_whitespace())
        .collect();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || compact.contains("apikey")
        || compact.contains("accesskey")
        || compact.contains("privatekey")
}

fn compare_diagnostics(left: &ValidationDiagnostic, right: &ValidationDiagnostic) -> Ordering {
    severity_rank(&left.severity)
        .cmp(&severity_rank(&right.severity))
        .then_with(|| optional_key(left.file.as_deref()).cmp(&optional_key(right.file.as_deref())))
        .then_with(|| left.line.unwrap_or(u32::MAX).cmp(&right.line.unwrap_or(u32::MAX)))
        .then_with(|| left.column.unwrap_or(u32::MAX).cmp(&right.column.unwrap_or(u32::MAX)))
        .then_with(|| optional_key(left.code.as_deref()).cmp(&optional_key(right.code.as_deref())))
        .then_with(|| left.message.cmp(&right.message))
}

fn severity_rank(value: &str) -> u8 {
    match value {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn optional_key(value: Option<&str>) -> (bool, &str) {
    (value.is_none(), value.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/home/user/proj")
    }

    fn pyright_json(diagnostics: &str, errors: u64, warnings: u64) -> String {
        format!(
            r#"{{"version":"1.1.411","summary":{{"filesAnalyzed":1,"errorCount":{errors},"warningCount":{warnings},"informationCount":0,"timeInSec":0.2}},"generalDiagnostics":[{diagnostics}]}}"#
        )
    }

    #[test]
    fn parses_relativizes_and_converts_positions() {
        let json = pyright_json(
            r#"{"file":"/home/user/proj/src/app.py","severity":"error","message":"\"c\" is not defined","range":{"start":{"line":1,"character":15},"end":{"line":1,"character":16}},"rule":"reportUndefinedVariable"}"#,
            1,
            0,
        );
        let parsed = parse_pyright_output(&json, &root()).unwrap();
        assert_eq!(parsed.errors_count, 1);
        assert_eq!(parsed.warnings_count, 0);
        assert_eq!(parsed.returned_diagnostic_count, 1);
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.severity, "error");
        assert_eq!(diagnostic.file.as_deref(), Some("src/app.py"));
        // 0-based (1,15) -> 1-based (2,16)
        assert_eq!(diagnostic.line, Some(2));
        assert_eq!(diagnostic.column, Some(16));
        assert_eq!(diagnostic.code.as_deref(), Some("reportUndefinedVariable"));
        assert_eq!(diagnostic.message, "\"c\" is not defined");
    }

    #[test]
    fn drops_paths_outside_the_project_as_external() {
        let json = pyright_json(
            r#"{"file":"/usr/lib/python3/typeshed/builtins.pyi","severity":"error","message":"stdlib issue","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}"#,
            1,
            0,
        );
        let parsed = parse_pyright_output(&json, &root()).unwrap();
        assert_eq!(parsed.external_results_omitted, 1);
        assert_eq!(parsed.returned_diagnostic_count, 0);
        // The summary count stays authoritative even when the diagnostic is external.
        assert_eq!(parsed.errors_count, 1);
    }

    #[test]
    fn sorts_dedups_and_truncates_at_the_cap() {
        let mut entries = Vec::new();
        for i in 0..(MAX_VALIDATION_DIAGNOSTICS + 5) {
            entries.push(format!(
                r#"{{"file":"/home/user/proj/src/f{i}.py","severity":"warning","message":"unused import {i}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"rule":"reportUnusedImport"}}"#
            ));
        }
        // Add an exact duplicate that must collapse.
        entries.push(entries[0].clone());
        let json = pyright_json(&entries.join(","), 0, MAX_VALIDATION_DIAGNOSTICS as u64 + 5);
        let parsed = parse_pyright_output(&json, &root()).unwrap();
        assert_eq!(parsed.returned_diagnostic_count, MAX_VALIDATION_DIAGNOSTICS);
        assert!(parsed.diagnostics_truncated);
        // Errors sort before warnings; here all are warnings, sorted by file.
        assert!(parsed.diagnostics.windows(2).all(|w| w[0].file <= w[1].file));
    }

    #[test]
    fn scrubs_secret_looking_diagnostics_and_bounds_message() {
        let json = pyright_json(
            r#"{"file":"/home/user/proj/a.py","severity":"error","message":"leaked api_key=sk-livesecret","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}"#,
            1,
            0,
        );
        let parsed = parse_pyright_output(&json, &root()).unwrap();
        assert_eq!(parsed.invalid_results_omitted, 1);
        assert_eq!(parsed.returned_diagnostic_count, 0);
    }

    #[test]
    fn truncated_or_non_pyright_json_returns_none() {
        assert!(parse_pyright_output("{\"summary\":{\"errorCoun", &root()).is_none());
        assert!(parse_pyright_output("not json", &root()).is_none());
        // Missing summary -> not a pyright report.
        assert!(parse_pyright_output(r#"{"generalDiagnostics":[]}"#, &root()).is_none());
    }
}
