use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn list_project_files_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "path",
            "string",
            "Optional project-relative directory to list (default: project root).",
            false,
        ),
        (
            "limit",
            "integer",
            "Maximum number of entries to return.",
            false,
        ),
    ]))
}

pub(crate) fn search_project_text_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("pattern", "string", "Text pattern to search for.", true),
        (
            "path",
            "string",
            "Optional project-relative directory to scope the search (default: project root).",
            false,
        ),
        (
            "limit",
            "integer",
            "Maximum number of matches to return.",
            false,
        ),
        (
            "context_before",
            "integer",
            "Optional number of context lines before each match (clamped to 20).",
            false,
        ),
        (
            "context_after",
            "integer",
            "Optional number of context lines after each match (clamped to 20).",
            false,
        ),
    ]))
}

pub(crate) fn read_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("start_line", "integer", "1-based line offset.", false),
        ("limit", "integer", "Maximum line count.", false),
        (
            "with_line_numbers",
            "boolean",
            "When true, include numbered_text and lines with 1-based line numbers.",
            false,
        ),
    ]))
}
