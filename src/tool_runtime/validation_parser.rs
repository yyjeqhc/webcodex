//! Deterministic structured extraction from bounded validation excerpts.
//!
//! These helpers consume only already-bounded, validation-tool metadata. They
//! return stable cargo/rustc facts without retaining output bodies, note/help
//! stacks, panic payloads, assertion values, backtraces, commands, or inferred
//! root causes.

use serde::Serialize;
use std::cmp::Ordering;

pub(crate) const PARSER_KIND: &str = "structured_validation_parser";
pub(crate) const PARSER_VERSION: u8 = 2;
pub(crate) const PARSER_LIMITATIONS: [&str; 3] = [
    "bounded validation excerpts only",
    "deterministic evidence extraction; no root-cause inference",
    "no full stdout/stderr bodies; incomplete excerpts may omit fields or report unknown",
];
pub(crate) const NO_STABLE_DIAGNOSTICS_REASON: &str = "no stable diagnostics found";
pub(crate) const VALIDATION_OUTPUT_METADATA_ABSENT_REASON: &str =
    "no validation event contains safe bounded output metadata";

pub(crate) const MAX_DIAGNOSTICS: usize = 20;
pub(crate) const MAX_FAILED_TESTS: usize = 20;
pub(crate) const MAX_DIAGNOSTIC_MESSAGE_CHARS: usize = 240;
const MAX_CODE_CHARS: usize = 64;
const MAX_FILE_CHARS: usize = 512;
const MAX_TEST_NAME_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ValidationDiagnostics {
    pub(crate) available: bool,
    pub(crate) parser: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<&'static str>,
    /// Total unique, valid diagnostics observed in the bounded excerpt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostic_count: Option<usize>,
    pub(crate) diagnostics: Vec<CargoDiagnostic>,
    pub(crate) returned_diagnostic_count: usize,
    pub(crate) diagnostics_truncated: bool,
    pub(crate) invalid_diagnostics_omitted: usize,
    /// Equals `diagnostics.first()` when present; retained for compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_diagnostic: Option<CargoDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) test_summary: Option<CargoTestSummary>,
    /// Sanitized failed test names from the bounded excerpt (max 20,
    /// deterministic first-seen order).
    pub(crate) failed_tests: Vec<String>,
    pub(crate) failed_test_details: Vec<FailedTestDetail>,
    /// Equals `failed_tests.first()` when present; retained for compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_failed_test: Option<String>,
    /// True when more than 20 unique safe names were seen, or the excerpt was
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
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedTestDetail {
    pub(crate) name: String,
    pub(crate) failure_kind: &'static str,
    pub(crate) file: Option<String>,
    pub(crate) line: Option<u64>,
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
    stdout_excerpt: &str,
    stderr_excerpt: &str,
    truncated: bool,
) -> ValidationDiagnostics {
    let lines = combined_lines(stdout_excerpt, stderr_excerpt);
    let parsed = parse_rust_diagnostics(&lines);
    if parsed.total == 0 {
        return diagnostics_unavailable(parsed.invalid);
    }
    diagnostics_from_rust(parsed, truncated)
}

pub(crate) fn parse_cargo_test_diagnostics(
    stdout_excerpt: &str,
    stderr_excerpt: &str,
    truncated: bool,
) -> ValidationDiagnostics {
    let lines = combined_lines(stdout_excerpt, stderr_excerpt);
    let parsed = parse_rust_diagnostics(&lines);
    let test_summary = aggregate_cargo_test_summaries(lines.iter().copied());
    let (all_failed_tests, unique_failed_count) = collect_failed_tests(&lines);

    if test_summary.is_none() && all_failed_tests.is_empty() && parsed.total == 0 {
        return diagnostics_unavailable(parsed.invalid);
    }

    let failed_tests: Vec<String> = all_failed_tests
        .into_iter()
        .take(MAX_FAILED_TESTS)
        .collect();
    let failed_test_details = failed_test_details(&lines, &failed_tests);
    let summary_failed = test_summary
        .as_ref()
        .and_then(|summary| summary.failed)
        .unwrap_or(0);
    let failed_tests_truncated = unique_failed_count > MAX_FAILED_TESTS
        || (truncated && summary_failed > failed_tests.len() as u64);
    let diagnostic_count = if test_summary.is_some() || !failed_tests.is_empty() {
        test_summary
            .as_ref()
            .and_then(|summary| summary.failed)
            .and_then(|failed| failed.try_into().ok())
            .or(Some(unique_failed_count))
    } else {
        Some(parsed.total)
    };
    let diagnostics_truncated = truncated || parsed.total > MAX_DIAGNOSTICS;
    let diagnostics = parsed.items;
    let returned_diagnostic_count = diagnostics.len();
    let first_diagnostic = diagnostics.first().cloned();
    let first_failed_test = failed_tests.first().cloned();

    ValidationDiagnostics {
        available: true,
        parser: PARSER_KIND,
        reason: None,
        diagnostic_count,
        diagnostics,
        returned_diagnostic_count,
        diagnostics_truncated,
        invalid_diagnostics_omitted: parsed.invalid,
        first_diagnostic,
        test_summary,
        failed_tests,
        failed_test_details,
        first_failed_test,
        failed_tests_truncated,
        truncated: Some(truncated),
    }
}

/// Aggregate all parseable `test result:` harness summaries from the given
/// lines. Counts use saturating addition across multiple test binaries.
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

    found.then_some(CargoTestSummary {
        passed,
        failed,
        ignored,
    })
}

#[derive(Debug)]
struct ParsedDiagnostics {
    items: Vec<CargoDiagnostic>,
    total: usize,
    invalid: usize,
}

fn parse_rust_diagnostics(lines: &[&str]) -> ParsedDiagnostics {
    let mut items = Vec::new();
    let mut invalid = 0usize;
    for (index, raw_line) in lines.iter().enumerate() {
        let line = sanitize_line(raw_line);
        let header = match parse_diagnostic_header(&line) {
            HeaderParse::None => continue,
            HeaderParse::Invalid => {
                invalid = invalid.saturating_add(1);
                continue;
            }
            HeaderParse::Valid(header) => header,
        };
        let span = lines
            .iter()
            .skip(index + 1)
            .take(6)
            .map(|line| sanitize_line(line))
            .take_while(|line| matches!(parse_diagnostic_header(line), HeaderParse::None))
            .find_map(|line| parse_span_line(&line));
        let (file, line, column) = span
            .map(|span| (Some(span.file), Some(span.line), Some(span.column)))
            .unwrap_or((None, None, None));
        items.push(CargoDiagnostic {
            severity: header.severity,
            code: header.code,
            file,
            line,
            column,
            message: header.message,
        });
    }

    items.sort_by(compare_diagnostics);
    items.dedup();
    let total = items.len();
    items.truncate(MAX_DIAGNOSTICS);
    ParsedDiagnostics {
        items,
        total,
        invalid,
    }
}

fn diagnostics_from_rust(parsed: ParsedDiagnostics, truncated: bool) -> ValidationDiagnostics {
    let diagnostics_truncated = truncated || parsed.total > MAX_DIAGNOSTICS;
    let returned_diagnostic_count = parsed.items.len();
    let first_diagnostic = parsed.items.first().cloned();
    ValidationDiagnostics {
        available: true,
        parser: PARSER_KIND,
        reason: None,
        diagnostic_count: Some(parsed.total),
        diagnostics: parsed.items,
        returned_diagnostic_count,
        diagnostics_truncated,
        invalid_diagnostics_omitted: parsed.invalid,
        first_diagnostic,
        test_summary: None,
        failed_tests: Vec::new(),
        failed_test_details: Vec::new(),
        first_failed_test: None,
        failed_tests_truncated: false,
        truncated: Some(truncated),
    }
}

fn diagnostics_unavailable(invalid: usize) -> ValidationDiagnostics {
    ValidationDiagnostics {
        available: false,
        parser: PARSER_KIND,
        reason: Some(NO_STABLE_DIAGNOSTICS_REASON),
        diagnostic_count: None,
        diagnostics: Vec::new(),
        returned_diagnostic_count: 0,
        diagnostics_truncated: false,
        invalid_diagnostics_omitted: invalid,
        first_diagnostic: None,
        test_summary: None,
        failed_tests: Vec::new(),
        failed_test_details: Vec::new(),
        first_failed_test: None,
        failed_tests_truncated: false,
        truncated: None,
    }
}

fn compare_diagnostics(left: &CargoDiagnostic, right: &CargoDiagnostic) -> Ordering {
    severity_rank(left.severity)
        .cmp(&severity_rank(right.severity))
        .then_with(|| {
            optional_string_sort_key(left.file.as_deref())
                .cmp(&optional_string_sort_key(right.file.as_deref()))
        })
        .then_with(|| {
            left.line
                .unwrap_or(u64::MAX)
                .cmp(&right.line.unwrap_or(u64::MAX))
        })
        .then_with(|| {
            left.column
                .unwrap_or(u64::MAX)
                .cmp(&right.column.unwrap_or(u64::MAX))
        })
        .then_with(|| {
            optional_string_sort_key(left.code.as_deref())
                .cmp(&optional_string_sort_key(right.code.as_deref()))
        })
        .then_with(|| left.message.cmp(&right.message))
}

fn severity_rank(value: &str) -> u8 {
    match value {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn optional_string_sort_key(value: Option<&str>) -> (bool, &str) {
    (value.is_none(), value.unwrap_or_default())
}

fn combined_lines<'a>(stdout_excerpt: &'a str, stderr_excerpt: &'a str) -> Vec<&'a str> {
    stdout_excerpt
        .lines()
        .chain(stderr_excerpt.lines())
        .collect()
}

#[derive(Debug)]
struct DiagnosticHeader {
    severity: &'static str,
    code: Option<String>,
    message: String,
}

enum HeaderParse {
    None,
    Invalid,
    Valid(DiagnosticHeader),
}

fn parse_diagnostic_header(line: &str) -> HeaderParse {
    let line = line.trim_start();
    parse_diagnostic_header_for_severity(line, "error")
        .or_else(|| parse_diagnostic_header_for_severity(line, "warning"))
        .unwrap_or(HeaderParse::None)
}

fn parse_diagnostic_header_for_severity(line: &str, severity: &'static str) -> Option<HeaderParse> {
    let rest = line.strip_prefix(severity)?;
    let (code, message) = if let Some(message) = rest.strip_prefix(':') {
        (None, message)
    } else if let Some(rest) = rest.strip_prefix('[') {
        let Some((code, after_code)) = rest.split_once(']') else {
            return Some(HeaderParse::Invalid);
        };
        let Some(message) = after_code.strip_prefix(':') else {
            return Some(HeaderParse::Invalid);
        };
        if !is_rust_error_code(code) || code.chars().count() > MAX_CODE_CHARS {
            return Some(HeaderParse::Invalid);
        }
        (Some(code.to_string()), message)
    } else {
        return None;
    };
    let message = sanitize_bounded_value(message, MAX_DIAGNOSTIC_MESSAGE_CHARS);
    match message {
        Some(message) if !looks_sensitive(&message) => Some(HeaderParse::Valid(DiagnosticHeader {
            severity,
            code,
            message,
        })),
        _ => Some(HeaderParse::Invalid),
    }
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
    parse_location(location)
}

fn parse_location(value: &str) -> Option<Span> {
    let value = value.trim_end_matches(':');
    let mut parts = value.rsplitn(3, ':');
    let column = parts
        .next()?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)?;
    let line = parts
        .next()?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)?;
    let file = sanitize_file_path(parts.next()?)?;
    Some(Span { file, line, column })
}

fn sanitize_file_path(value: &str) -> Option<String> {
    let value = sanitize_bounded_value(value, MAX_FILE_CHARS)?;
    if value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.contains("://")
        || value.to_ascii_lowercase().starts_with("file:")
        || value.as_bytes().get(1) == Some(&b':')
        || looks_sensitive(&value)
        || value.split('/').any(|part| part.is_empty() || part == "..")
    {
        return None;
    }
    Some(value)
}

fn collect_failed_tests(lines: &[&str]) -> (Vec<String>, usize) {
    let mut failed_tests = Vec::new();
    for line in lines {
        let line = sanitize_line(line);
        let Some(name) = parse_failed_test_line(&line) else {
            continue;
        };
        if !failed_tests.iter().any(|existing| existing == &name) {
            failed_tests.push(name);
        }
    }
    let total = failed_tests.len();
    (failed_tests, total)
}

fn failed_test_details(lines: &[&str], failed_tests: &[String]) -> Vec<FailedTestDetail> {
    let panics = collect_associated_panics(lines);
    failed_tests
        .iter()
        .map(|name| {
            let panic = panics.iter().find(|panic| &panic.name == name);
            FailedTestDetail {
                name: name.clone(),
                failure_kind: panic.map(|panic| panic.failure_kind).unwrap_or("unknown"),
                file: panic.and_then(|panic| panic.span.as_ref().map(|span| span.file.clone())),
                line: panic.and_then(|panic| panic.span.as_ref().map(|span| span.line)),
                column: panic.and_then(|panic| panic.span.as_ref().map(|span| span.column)),
            }
        })
        .collect()
}

struct AssociatedPanic {
    name: String,
    failure_kind: &'static str,
    span: Option<Span>,
}

fn collect_associated_panics(lines: &[&str]) -> Vec<AssociatedPanic> {
    let sanitized: Vec<String> = lines.iter().map(|line| sanitize_line(line)).collect();
    let mut panics = Vec::new();
    for (index, line) in sanitized.iter().enumerate() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("thread '") else {
            continue;
        };
        let Some((name, location)) = rest.split_once("' panicked at ") else {
            continue;
        };
        let Some(name) = sanitize_test_name(name) else {
            continue;
        };
        let location_token = location.split_whitespace().next().unwrap_or_default();
        let span = parse_location(location_token);
        let assertion = sanitized
            .iter()
            .skip(index + 1)
            .take(8)
            .map(|line| line.trim())
            .take_while(|line| {
                !line.starts_with("thread '")
                    && !line.starts_with("test result:")
                    && parse_failed_test_line(line).is_none()
            })
            .any(is_stable_assertion_signature);
        if !panics
            .iter()
            .any(|panic: &AssociatedPanic| panic.name == name)
        {
            panics.push(AssociatedPanic {
                name,
                failure_kind: if assertion { "assertion" } else { "panic" },
                span,
            });
        }
    }
    panics
}

fn is_stable_assertion_signature(line: &str) -> bool {
    line == "assertion failed" || (line.starts_with("assertion ") && line.ends_with(" failed"))
}

fn parse_test_summary_line(line: &str) -> Option<CargoTestSummary> {
    let line = sanitize_line(line);
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
    (summary.passed.is_some() || summary.failed.is_some() || summary.ignored.is_some())
        .then_some(summary)
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
    let value = sanitize_bounded_value(value, MAX_TEST_NAME_CHARS)?;
    if looks_sensitive(&value)
        || !value
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ':' | '-' | '<' | '>' | '.'))
    {
        return None;
    }
    Some(value)
}

/// Strip terminal escapes and controls, collapse whitespace, preserve Unicode,
/// and truncate by Unicode scalar count.
fn sanitize_bounded_value(value: &str, max_chars: usize) -> Option<String> {
    let value = sanitize_line(value);
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(max_chars).collect())
}

fn sanitize_line(value: &str) -> String {
    let stripped = strip_ansi(value);
    let mut out = String::new();
    let mut pending_space = false;
    for ch in stripped.chars() {
        if ch.is_control() || ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

fn strip_ansi(value: &str) -> String {
    let mut chars = value.chars().peekable();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                let mut previous_escape = false;
                for next in chars.by_ref() {
                    if next == '\u{7}' || (previous_escape && next == '\\') {
                        break;
                    }
                    previous_escape = next == '\u{1b}';
                }
            }
            Some(_) | None => {}
        }
    }
    out
}

fn looks_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !matches!(*ch, '_' | '-') && !ch.is_whitespace())
        .collect();
    if lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || compact.contains("apikey")
        || compact.contains("accesskey")
        || compact.contains("privatekey")
    {
        return true;
    }
    value.split_whitespace().any(|word| {
        let Some((key, assigned)) = word.split_once('=') else {
            return false;
        };
        !assigned.is_empty()
            && key.len() >= 2
            && key
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    })
}
