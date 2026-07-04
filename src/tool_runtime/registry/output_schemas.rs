use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod common;
mod discovery;
mod edits;
mod git;
mod jobs;
mod sessions;

use common::{
    array_schema, default_output_schema, nullable_schema, open_object_schema, schema_type,
    search_match_schema, wrapped_output_schema,
};

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

    match name {
        "workspace_hygiene_check" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                nullable_schema("string", "Canonical runtime project id, when resolved."),
            ),
            ("git_available", schema_type("boolean", "True when the project is a git repository.")),
            ("clean", schema_type("boolean", "True when git is available and no findings were reported.")),
            (
                "counts",
                open_object_schema("Bounded finding counts: findings, critical, high, medium, low, untracked, tracked, large_files, secret_like_paths, cache_paths."),
            ),
            (
                "findings",
                array_schema(
                    open_object_schema("Hygiene finding: path, kind, severity, tracked_status, reason, recommendation. Never includes file contents."),
                    "Bounded hygiene findings. Path is project-relative. Secret-like files are identified by name only.",
                ),
            ),
            ("truncated", schema_type("boolean", "True when findings were truncated to max_findings.")),
            (
                "warnings",
                array_schema(schema_type("string", "Warning code."), "Warning codes such as non_git_project."),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Bounded suggested next actions."),
            ),
        ]),
        "delete_project_files" => wrapped_output_schema(vec![
            ("ok", schema_type("boolean", "True when the delete command completed successfully.")),
            (
                "deleted_paths",
                array_schema(schema_type("string", "Deleted project-relative path."), "Requested paths removed with rm -f."),
            ),
            (
                "missing_paths",
                array_schema(schema_type("string", "Missing project-relative path."), "Reserved for future missing-path detail; currently empty for rm -f success."),
            ),
            (
                "refused_paths",
                array_schema(schema_type("string", "Refused project-relative path."), "Reserved for future refused-path detail; cleanup path validation failures still return a failed tool result."),
            ),
            (
                "stdout_present",
                schema_type("boolean", "Whether the underlying command produced stdout. Raw stdout is not exposed by default."),
            ),
            (
                "stderr_present",
                schema_type("boolean", "Whether the underlying command produced stderr. Raw stderr is not exposed by default."),
            ),
        ]),
        "read_file" => wrapped_output_schema(vec![
            ("content", schema_type("string", "File content.")),
            ("path", schema_type("string", "Project-relative path.")),
            (
                "start_line",
                schema_type("integer", "1-based starting line."),
            ),
            ("limit", schema_type("integer", "Maximum requested line count.")),
            (
                "total_lines",
                schema_type("integer", "Total line count, when available."),
            ),
            (
                "numbered_text",
                schema_type(
                    "string",
                    "Optional line-numbered content when with_line_numbers=true.",
                ),
            ),
            (
                "lines",
                array_schema(
                    open_object_schema("Line object with 1-based line and text fields."),
                    "Optional structured lines when with_line_numbers=true.",
                ),
            ),
        ]),
        "search_project_text" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved project id.")),
            ("pattern", schema_type("string", "Search pattern.")),
            ("path", schema_type("string", "Project-relative search root.")),
            (
                "backend",
                schema_type("string", "Search backend used: rg, grep, or native."),
            ),
            (
                "matches",
                array_schema(
                    search_match_schema(),
                    "Bounded search matches.",
                ),
            ),
            ("count", schema_type("integer", "Returned match count.")),
            (
                "truncated",
                schema_type("boolean", "Whether more matches were available."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Search command exit code, when available."),
            ),
            (
                "context_before",
                schema_type("integer", "Effective context lines before each match."),
            ),
            (
                "context_after",
                schema_type("integer", "Effective context lines after each match."),
            ),
        ]),
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
