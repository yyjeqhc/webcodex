use serde_json::{json, Value};

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

pub(crate) fn project_overview_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Full agent runtime project id.", true),
        (
            "path",
            "string",
            "Optional project-relative directory scope (default: project root).",
            false,
        ),
        (
            "max_depth",
            "integer",
            "Bounded scan depth; defaults to 2 and is clamped by the runtime to 1..4.",
            false,
        ),
        (
            "limit",
            "integer",
            "Bounded scanned-entry limit; defaults to 200 and is clamped by the runtime to 20..500.",
            false,
        ),
    ]));
    schema["properties"]["max_depth"]["default"] = json!(2);
    schema["properties"]["limit"]["default"] = json!(200);
    schema
}

pub(crate) fn search_project_text_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
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
            "Maximum records to return: matches in matches mode, files in files_with_matches/count modes.",
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
        (
            "include_globs",
            "array",
            "Optional ripgrep include globs. At most 32 entries of 1..256 bytes; negated and protected-path globs are rejected.",
            false,
        ),
        (
            "exclude_globs",
            "array",
            "Optional additive ripgrep exclude globs. Built-in secret/build excludes always remain active.",
            false,
        ),
        (
            "result_mode",
            "string",
            "Result shape: matches (default), files_with_matches, or count.",
            false,
        ),
        (
            "timeout_secs",
            "integer",
            "Optional search timeout in seconds. Server clamps the value to 1..120 (default 30). Out-of-range values are accepted and clamped rather than rejected by schema.",
            false,
        ),
    ]));
    for field in ["include_globs", "exclude_globs"] {
        let description = schema["properties"][field]["description"].clone();
        schema["properties"][field] = json!({
            "type": "array",
            "maxItems": 32,
            "items": {
                "type": "string",
                "minLength": 1,
                "maxLength": 256,
            },
            "description": description,
        });
    }
    schema["properties"]["result_mode"]["enum"] = json!(["matches", "files_with_matches", "count"]);
    schema["properties"]["result_mode"]["default"] = json!("matches");
    // Intentionally no minimum/maximum: strict clients would reject 0/999
    // before send, but runtime clamps any integer to 1..120.
    schema["properties"]["timeout_secs"]["default"] = json!(30);
    schema
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
