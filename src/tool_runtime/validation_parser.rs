//! Minimal parsers for bounded validation output tails.
//!
//! These helpers extract only stable cargo/rustc facts from already-bounded
//! text. They deliberately avoid message bodies, stack traces, root-cause
//! inference, or fix suggestions.

#![allow(dead_code)]

use serde::Serialize;

pub(crate) const PARSER_KIND: &str = "minimal_bounded_tail_parser";
pub(crate) const PARSER_VERSION: u8 = 1;
pub(crate) const PARSER_LIMITATIONS: [&str; 3] = [
    "bounded tails only",
    "no root-cause inference",
    "no full stdout/stderr bodies",
];
pub(crate) const NO_STABLE_DIAGNOSTICS_REASON: &str = "no stable diagnostics found";
pub(crate) const SESSION_LEDGER_UNWIRED_REASON: &str =
    "minimal bounded tail parser is not wired because session ledger events do not retain bounded stdout/stderr tails";

const MAX_SAFE_TEXT_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ValidationDiagnostics {
    pub(crate) available: bool,
    pub(crate) parser: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostic_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_diagnostic: Option<CargoDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) test_summary: Option<CargoTestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_failed_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) truncated: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CargoDiagnostic {
    pub(crate) severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) column: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CargoTestSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) passed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ignored: Option<u64>,
}

pub(crate) fn parse_cargo_check_diagnostics(
    stdout_tail: &str,
    stderr_tail: &str,
    truncated: bool,
) -> ValidationDiagnostics {
    let lines = combined_lines(stdout_tail, stderr_tail);
    let mut diagnostic_count = 0usize;
    let mut first_diagnostic = None;

    for (idx, line) in lines.iter().enumerate() {
        let Some((severity, code)) = parse_diagnostic_header(line) else {
            continue;
        };
        diagnostic_count += 1;
        if first_diagnostic.is_none() {
            let span = lines
                .iter()
                .skip(idx + 1)
                .take(6)
                .find_map(|line| parse_span_line(line));
            let (file, line, column) = span
                .map(|span| (Some(span.file), Some(span.line), Some(span.column)))
                .unwrap_or((None, None, None));
            first_diagnostic = Some(CargoDiagnostic {
                severity,
                code,
                file,
                line,
                column,
            });
        }
    }

    if diagnostic_count == 0 {
        diagnostics_unavailable()
    } else {
        ValidationDiagnostics {
            available: true,
            parser: PARSER_KIND,
            reason: None,
            diagnostic_count: Some(diagnostic_count),
            first_diagnostic,
            test_summary: None,
            first_failed_test: None,
            truncated: Some(truncated),
        }
    }
}

pub(crate) fn parse_cargo_test_diagnostics(
    stdout_tail: &str,
    stderr_tail: &str,
    truncated: bool,
) -> ValidationDiagnostics {
    let lines = combined_lines(stdout_tail, stderr_tail);
    let test_summary = lines.iter().find_map(|line| parse_test_summary_line(line));
    let first_failed_test = lines.iter().find_map(|line| parse_failed_test_line(line));

    if test_summary.is_none() && first_failed_test.is_none() {
        return diagnostics_unavailable();
    }

    let diagnostic_count = test_summary
        .as_ref()
        .and_then(|summary| summary.failed)
        .and_then(|failed| usize::try_from(failed).ok())
        .or_else(|| first_failed_test.as_ref().map(|_| 1))
        .unwrap_or(0);

    ValidationDiagnostics {
        available: true,
        parser: PARSER_KIND,
        reason: None,
        diagnostic_count: Some(diagnostic_count),
        first_diagnostic: None,
        test_summary,
        first_failed_test,
        truncated: Some(truncated),
    }
}

fn diagnostics_unavailable() -> ValidationDiagnostics {
    ValidationDiagnostics {
        available: false,
        parser: PARSER_KIND,
        reason: Some(NO_STABLE_DIAGNOSTICS_REASON),
        diagnostic_count: None,
        first_diagnostic: None,
        test_summary: None,
        first_failed_test: None,
        truncated: None,
    }
}

fn combined_lines<'a>(stdout_tail: &'a str, stderr_tail: &'a str) -> Vec<&'a str> {
    stdout_tail.lines().chain(stderr_tail.lines()).collect()
}

fn parse_diagnostic_header(line: &str) -> Option<(&'static str, Option<String>)> {
    let line = line.trim_start();
    parse_diagnostic_header_for_severity(line, "error")
        .or_else(|| parse_diagnostic_header_for_severity(line, "warning"))
}

fn parse_diagnostic_header_for_severity(
    line: &str,
    severity: &'static str,
) -> Option<(&'static str, Option<String>)> {
    let rest = line.strip_prefix(severity)?;
    if rest.starts_with(':') {
        return Some((severity, None));
    }
    let rest = rest.strip_prefix('[')?;
    let (code, after_code) = rest.split_once(']')?;
    if !after_code.starts_with(':') || !is_rust_error_code(code) {
        return None;
    }
    Some((severity, Some(code.to_string())))
}

fn is_rust_error_code(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('E') && value[1..].chars().all(|ch| ch.is_ascii_digit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    file: String,
    line: u64,
    column: u64,
}

fn parse_span_line(line: &str) -> Option<Span> {
    let location = line
        .trim_start()
        .strip_prefix("-->")?
        .trim_start()
        .split_whitespace()
        .next()?;
    let mut parts = location.rsplitn(3, ':');
    let column = parts.next()?.parse::<u64>().ok()?;
    let line = parts.next()?.parse::<u64>().ok()?;
    let file = sanitize_file_path(parts.next()?)?;
    Some(Span { file, line, column })
}

fn sanitize_file_path(value: &str) -> Option<String> {
    sanitize_bounded_text(value).filter(|value| {
        !value.starts_with('/')
            && !value.contains('\\')
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/'))
    })
}

fn parse_test_summary_line(line: &str) -> Option<CargoTestSummary> {
    if !line.contains("test result:") {
        return None;
    }
    let tokens: Vec<&str> = line
        .split(|ch: char| ch.is_whitespace() || ch == ';' || ch == '.')
        .filter(|token| !token.is_empty())
        .collect();
    let summary = CargoTestSummary {
        passed: count_before_label(&tokens, "passed"),
        failed: count_before_label(&tokens, "failed"),
        ignored: count_before_label(&tokens, "ignored"),
    };
    if summary.passed.is_none() && summary.failed.is_none() && summary.ignored.is_none() {
        None
    } else {
        Some(summary)
    }
}

fn count_before_label(tokens: &[&str], label: &str) -> Option<u64> {
    tokens.windows(2).find_map(|window| {
        (window[1] == label)
            .then(|| window[0].parse::<u64>().ok())
            .flatten()
    })
}

fn parse_failed_test_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("test ")?;
    let name = rest.strip_suffix(" ... FAILED")?;
    sanitize_test_name(name)
}

fn sanitize_test_name(value: &str) -> Option<String> {
    sanitize_bounded_text(value).filter(|value| {
        value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-' | '<' | '>' | '.'))
    })
}

fn sanitize_bounded_text(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_SAFE_TEXT_CHARS
        || value.chars().any(|ch| ch.is_control())
    {
        None
    } else {
        Some(value.to_string())
    }
}
