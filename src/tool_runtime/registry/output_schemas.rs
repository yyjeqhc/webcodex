use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod common;
mod discovery;
mod edits;
mod files;
mod git;
mod hygiene;
mod jobs;
mod sessions;

use common::{default_output_schema, nullable_schema, schema_type, wrapped_output_schema};

pub(crate) fn output_schema_for_tool(name: &str) -> Value {
    if let Some(schema) = jobs::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = discovery::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = coding_tasks::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = checkpoints::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = artifacts::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = git::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = edits::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = sessions::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = hygiene::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = files::output_schema_for_tool(name) {
        return schema;
    }

    match name {
        "cargo_fmt" | "cargo_check" | "cargo_test" => wrapped_output_schema(vec![
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
        ]),
        _ => default_output_schema(),
    }
}
