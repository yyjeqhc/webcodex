use serde_json::{json, Value};

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};

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
                    "Bounded cargo_test diagnostics parsed only from stdout_tail/stderr_tail. Includes sanitized failed test names (max 10), test_summary counts, and truncation metadata. Never includes panic messages, assertion diffs, stack traces, source bodies, absolute paths, tokens, secrets, root-cause inference, or fix recommendations.",
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
            "parser": schema_type(
                "string",
                "Stable parser identifier (minimal_bounded_tail_parser).",
            ),
            "reason": nullable_schema(
                "string",
                "Why diagnostics are unavailable, when available is false.",
            ),
            "diagnostic_count": nullable_schema(
                "integer",
                "Prefer test_summary.failed; otherwise the number of safely captured failed test names.",
            ),
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
            "failed_tests": array_schema(
                schema_type(
                    "string",
                    "Sanitized failed test name (A-Z a-z 0-9 _ : - < > . only).",
                ),
                "Up to 10 unique sanitized failed test names in first-seen order.",
            ),
            "first_failed_test": nullable_schema(
                "string",
                "Compatibility alias for failed_tests[0].",
            ),
            "failed_tests_truncated": schema_type(
                "boolean",
                "True when more than 10 unique safe names were seen, or the bounded tail was truncated and the aggregated summary failed count exceeds captured names.",
            ),
            "truncated": schema_type(
                "boolean",
                "Whether the parsed stdout_tail and/or stderr_tail input was truncated.",
            ),
        },
        "additionalProperties": false,
    })
}
