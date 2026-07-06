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
        "search_project_text" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved project id.")),
            ("pattern", schema_type("string", "Search pattern.")),
            (
                "path",
                schema_type("string", "Project-relative search root."),
            ),
            (
                "backend",
                schema_type("string", "Search backend used: rg, grep, or native."),
            ),
            (
                "matches",
                array_schema(search_match_schema(), "Bounded search matches."),
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
        ])),
        _ => None,
    }
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
