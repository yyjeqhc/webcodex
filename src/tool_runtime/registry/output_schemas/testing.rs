use serde_json::Value;

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
        ]);
    }
    wrapped_output_schema(fields)
}
