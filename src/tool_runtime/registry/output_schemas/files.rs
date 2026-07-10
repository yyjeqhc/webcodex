use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, search_match_schema,
    wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "list_project_files" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved project id.")),
            (
                "path",
                schema_type("string", "Project-relative listed directory path."),
            ),
            (
                "entries",
                array_schema(
                    file_list_entry_schema(),
                    "Bounded project-relative file and directory entries.",
                ),
            ),
            (
                "truncated",
                schema_type(
                    "boolean",
                    "Whether more entries were available than returned.",
                ),
            ),
        ])),
        "read_file" => Some(wrapped_output_schema(vec![
            ("content", schema_type("string", "File content.")),
            ("path", schema_type("string", "Project-relative path.")),
            (
                "start_line",
                schema_type("integer", "1-based starting line."),
            ),
            (
                "limit",
                schema_type("integer", "Maximum requested line count."),
            ),
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
        ])),
        "search_project_text" => {
            Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved project id.")),
            ("pattern", schema_type("string", "Search pattern.")),
            (
                "path",
                schema_type("string", "Project-relative search root."),
            ),
            (
                "backend",
                nullable_schema(
                    "string",
                    "Search backend used: rg, grep, or native. Null/omitted when unknown (for example outer wait timeout before backend selection).",
                ),
            ),
            (
                "result_mode",
                json!({
                    "type": "string",
                    "enum": ["matches", "files_with_matches", "count"],
                    "description": "Effective result mode."
                }),
            ),
            (
                "effective_timeout_secs",
                schema_type("integer", "Effective clamped timeout in seconds."),
            ),
            (
                "matches",
                array_schema(
                    search_match_schema(),
                    "Bounded search matches; present in matches mode.",
                ),
            ),
            ("count", schema_type("integer", "Returned match count.")),
            (
                "files",
                array_schema(
                    search_file_result_schema(),
                    "Bounded file records for files_with_matches or count mode.",
                ),
            ),
            (
                "returned_file_count",
                schema_type("integer", "Number of returned file records."),
            ),
            (
                "returned_match_count",
                schema_type(
                    "integer",
                    "Sum of match_count values in returned count-mode file records.",
                ),
            ),
            (
                "count_complete",
                schema_type(
                    "boolean",
                    "True only when count mode completed without limit or transport truncation.",
                ),
            ),
            (
                "total_matches",
                nullable_schema(
                    "integer",
                    "Global matching-line total only when count_complete=true; otherwise null.",
                ),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether more mode-specific records were available."),
            ),
            (
                "truncation_reason",
                nullable_schema(
                    "string",
                    "Truncation reason: limit or transport; null when complete.",
                ),
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
            (
                "code",
                schema_type(
                    "string",
                    "Stable structured error code on validation, backend capability, execution, timeout, or request-drop failure.",
                ),
            ),
            (
                "field",
                schema_type(
                    "string",
                    "Input field name for invalid_search_request failures.",
                ),
            ),
            (
                "index",
                schema_type(
                    "integer",
                    "Optional zero-based index for invalid glob list entries.",
                ),
            ),
            (
                "reason",
                schema_type(
                    "string",
                    "Optional stable validation reason (empty, too_long, control_char, negated, protected_path, too_many, nul_byte, invalid_path).",
                ),
            ),
            (
                "requested_features",
                array_schema(
                    schema_type("string", "Requested advanced feature."),
                    "Advanced features that require ripgrep.",
                ),
            ),
            ("message", schema_type("string", "Structured failure message.")),
        ]))
        }
        _ => None,
    }
}

fn search_file_result_schema() -> Value {
    json!({
        "type": "object",
        "description": "Unique project-relative matching file, with match_count in count mode.",
        "properties": {
            "path": schema_type("string", "Project-relative file path."),
            "match_count": schema_type("integer", "Matching-line count for this file in count mode."),
        },
        "required": ["path"],
        "additionalProperties": false,
    })
}

fn file_list_entry_schema() -> Value {
    json!({
        "type": "object",
        "description": "One bounded file-list entry.",
        "properties": {
            "path": schema_type("string", "Project-relative file or directory path."),
            "kind": {
                "type": "string",
                "enum": ["file", "dir"],
                "description": "Entry kind."
            }
        },
        "required": ["path", "kind"],
        "additionalProperties": true
    })
}
