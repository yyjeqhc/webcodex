use serde_json::{json, Value};

use super::common::{nullable_schema, schema_type, wrapped_output_schema};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "cargo_fmt" | "cargo_check" => Some(cargo_output_schema(false)),
        "cargo_test" => Some(cargo_output_schema(true)),
        _ => None,
    }
}

fn cargo_output_schema(include_zero_tests_metadata: bool) -> Value {
    let mut fields = vec![
            ("project", schema_type("string", "Runtime project id.")),
            ("command", schema_type("string", "Cargo command executed.")),
            (
                "cwd",
                schema_type("string", "Project-relative working directory."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Cargo command exit code."),
            ),
            (
                "duration_ms",
                schema_type("integer", "Command duration in milliseconds."),
            ),
            ("stdout_tail", schema_type("string", "Bounded stdout tail.")),
            ("stderr_tail", schema_type("string", "Bounded stderr tail.")),
            (
                "stdout_truncated",
                schema_type("boolean", "Whether stdout was truncated."),
            ),
            (
                "stderr_truncated",
                schema_type("boolean", "Whether stderr was truncated."),
            ),
            (
                "passed",
                schema_type("boolean", "Whether exit_code was zero."),
            ),
            (
                "failure_kind",
                schema_type(
                    "string",
                    "Stable failure kind. Non-zero cargo_fmt, cargo_check, and cargo_test command exits use validation_failed; pre-start rejection, guard denial, timeout, and runtime errors do not.",
                ),
            ),
            (
                "warnings_count",
                nullable_schema("integer", "Heuristic warning count for cargo_check."),
            ),
            (
                "errors_count",
                nullable_schema("integer", "Heuristic error count for cargo_check."),
            ),
            (
                "tests_passed",
                nullable_schema("integer", "Parsed passed test count for cargo_test."),
            ),
            (
                "tests_failed",
                nullable_schema("integer", "Parsed failed test count for cargo_test."),
            ),
    ];
    if include_zero_tests_metadata {
        fields.extend([
            (
                "tests_detected",
                schema_type(
                    "boolean",
                    "Whether cargo_test parsed at least one Rust test harness running N test(s) section.",
                ),
            ),
            (
                "tests_run_count",
                nullable_schema(
                    "integer",
                    "Total tests from all parsed cargo_test Rust test harness running N test(s) sections.",
                ),
            ),
            (
                "zero_tests_run",
                nullable_schema(
                    "boolean",
                    "True when cargo_test parsed test harness sections and their summed tests_run_count is zero.",
                ),
            ),
            (
                "diagnostics",
                cargo_test_diagnostics_schema(
                    "Structured validation parser v3 evidence extracted deterministically from bounded cargo_test stdout/stderr metadata. Includes at most 20 diagnostics, failed_test_details, test_summary counts, and explicit omission/truncation metadata. Never includes panic bodies, assertion values, backtraces, source bodies, absolute paths, tokens, secrets, root-cause inference, or fix recommendations.",
                ),
            ),
        ]);
    }
    wrapped_output_schema(fields)
}

fn cargo_test_diagnostics_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "available": schema_type(
                "boolean",
                "True when a test result summary or at least one safe failed test name was parsed.",
            ),
            "parser": {
                "type": "string",
                "enum": ["structured_validation_parser"],
                "description": "Stable structured validation parser v3 identifier."
            },
            "reason": nullable_schema(
                "string",
                "Why diagnostics are unavailable, when available is false.",
            ),
            "diagnostic_count": nullable_schema(
                "integer",
                "Prefer test_summary.failed; otherwise the number of safely captured failed test names.",
            ),
            "diagnostics": {
                "type": "array",
                "maxItems": 20,
                "items": cargo_diagnostic_schema(),
                "description": "Bounded sorted, deduplicated rustc diagnostics from the captured excerpt."
            },
            "returned_diagnostic_count": {
                "type": "integer",
                "minimum": 0,
                "maximum": 20
            },
            "diagnostics_truncated": schema_type(
                "boolean",
                "True when the diagnostic list or captured validation excerpt is incomplete.",
            ),
            "invalid_diagnostics_omitted": {
                "type": "integer",
                "minimum": 0,
                "description": "Diagnostic headers omitted because they could not be represented safely."
            },
            "test_summary": {
                "type": "object",
                "description": "Aggregated cargo test result summary counts across all harnesses in the bounded tails.",
                "properties": {
                    "passed": nullable_schema("integer", "Aggregated passed test count."),
                    "failed": nullable_schema("integer", "Aggregated failed test count."),
                    "ignored": nullable_schema("integer", "Aggregated ignored test count."),
                },
                "additionalProperties": false,
            },
            "failed_test_details": {
                "type": "array",
                "maxItems": 20,
                "items": failed_test_detail_schema(),
                "description": "Up to 20 unique failed-test details in deterministic first-seen order. Conservative assertion, panic, or unknown evidence without payload bodies."
            },
            "failed_test_details_truncated": schema_type(
                "boolean",
                "True when more than 20 unique safe names were seen, or the bounded excerpt was truncated and the aggregated summary failed count exceeds captured details.",
            ),
            "truncated": schema_type(
                "boolean",
                "Whether the parsed stdout_tail and/or stderr_tail input was truncated.",
            ),
        },
        "additionalProperties": false,
    })
}

fn cargo_diagnostic_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "severity": { "type": "string", "enum": ["error", "warning", "unknown"] },
            "code": { "type": "string", "maxLength": 64 },
            "file": { "type": "string", "maxLength": 512 },
            "line": { "type": "integer", "minimum": 1 },
            "column": { "type": "integer", "minimum": 1 },
            "message": { "type": "string", "maxLength": 240 }
        },
        "required": ["severity", "message"]
    })
}

fn failed_test_detail_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string", "maxLength": 240 },
            "failure_kind": { "type": "string", "enum": ["assertion", "panic", "unknown"] },
            "file": { "anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}] },
            "line": { "anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}] },
            "column": { "anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}] }
        },
        "required": ["name", "failure_kind", "file", "line", "column"]
    })
}
