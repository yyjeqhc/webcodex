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
pub(crate) const VALIDATION_OUTPUT_METADATA_ABSENT_REASON: &str =
    "no validation event contains safe bounded output metadata";

const MAX_SAFE_TEXT_CHARS: usize = 240;
const MAX_FAILED_TESTS: usize = 10;

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
    /// Sanitized failed test names from the bounded tail (max 10, first-seen order).
    pub(crate) failed_tests: Vec<String>,
    /// Equals `failed_tests.first()` when present; retained for compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_failed_test: Option<String>,
    /// True when more than 10 unique safe names were seen, or the tail was
    /// truncated and the summary failed count exceeds captured names.
    pub(crate) failed_tests_truncated: bool,
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
            failed_tests: Vec::new(),
            first_failed_test: None,
            failed_tests_truncated: false,
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
    let test_summary = aggregate_cargo_test_summaries(lines.iter().copied());
    let (failed_tests, unique_failed_count) = collect_failed_tests(&lines);

    if test_summary.is_none() && failed_tests.is_empty() {
        return diagnostics_unavailable();
    }

    let diagnostic_count = test_summary
        .as_ref()
        .and_then(|summary| summary.failed)
        .and_then(|failed| usize::try_from(failed).ok())
        .unwrap_or(failed_tests.len());

    let summary_failed = test_summary
        .as_ref()
        .and_then(|summary| summary.failed)
        .unwrap_or(0);
    let failed_tests_truncated = unique_failed_count > MAX_FAILED_TESTS
        || (truncated && summary_failed > failed_tests.len() as u64);

    let first_failed_test = failed_tests.first().cloned();

    ValidationDiagnostics {
        available: true,
        parser: PARSER_KIND,
        reason: None,
        diagnostic_count: Some(diagnostic_count),
        first_diagnostic: None,
        test_summary,
        failed_tests,
        first_failed_test,
        failed_tests_truncated,
        truncated: Some(truncated),
    }
}

/// Aggregate all parseable `test result:` harness summaries from the given lines.
///
/// Passed/failed/ignored counts are summed with saturating_add. A field stays
/// `None` only when no summary contributed that field. Returns `None` when no
/// summary lines were found.
pub(crate) fn aggregate_cargo_test_summaries<'a, I>(lines: I) -> Option<CargoTestSummary>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut found = false;
    let mut passed: Option<u64> = None;
    let mut failed: Option<u64> = None;
    let mut ignored: Option<u64> = None;

    for line in lines {
        let Some(summary) = parse_test_summary_line(line) else {
            continue;
        };
        found = true;
        if let Some(value) = summary.passed {
            passed = Some(passed.unwrap_or(0).saturating_add(value));
        }
        if let Some(value) = summary.failed {
            failed = Some(failed.unwrap_or(0).saturating_add(value));
        }
        if let Some(value) = summary.ignored {
            ignored = Some(ignored.unwrap_or(0).saturating_add(value));
        }
    }

    if found {
        Some(CargoTestSummary {
            passed,
            failed,
            ignored,
        })
    } else {
        None
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
        failed_tests: Vec::new(),
        first_failed_test: None,
        failed_tests_truncated: false,
        truncated: None,
    }
}

/// Collect unique sanitized failed test names in first-seen order (max 10).
/// Returns `(bounded_names, total_unique_count)`.
fn collect_failed_tests(lines: &[&str]) -> (Vec<String>, usize) {
    let mut failed_tests = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for line in lines {
        let Some(name) = parse_failed_test_line(line) else {
            continue;
        };
        if seen.iter().any(|existing| existing == &name) {
            continue;
        }
        seen.push(name.clone());
        if failed_tests.len() < MAX_FAILED_TESTS {
            failed_tests.push(name);
        }
    }
    (failed_tests, seen.len())
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
