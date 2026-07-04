use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn replace_in_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("old", "string", "Non-empty substring to replace.", true),
        ("new", "string", "Replacement string.", true),
        (
            "expected_replacements",
            "integer",
            "Expected occurrence count (default 1).",
            false,
        ),
        (
            "allow_multiple",
            "boolean",
            "Allow replacing multiple occurrences (default false).",
            false,
        ),
    ]))
}

pub(crate) fn replace_exact_block_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "old_text",
            "string",
            "Non-empty literal block; must match exactly once.",
            true,
        ),
        (
            "new_text",
            "string",
            "Replacement text; may be empty to delete the block.",
            true,
        ),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for current whole-file content.",
            false,
        ),
    ]))
}

pub(crate) fn insert_before_pattern_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "pattern",
            "string",
            "Non-empty literal pattern; must match exactly once.",
            true,
        ),
        (
            "text",
            "string",
            "Non-empty text to insert, including intended newlines.",
            true,
        ),
    ]))
}

pub(crate) fn insert_after_pattern_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "pattern",
            "string",
            "Non-empty literal pattern; must match exactly once.",
            true,
        ),
        (
            "text",
            "string",
            "Non-empty text to insert, including intended newlines.",
            true,
        ),
    ]))
}

pub(crate) fn write_project_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("content", "string", "UTF-8 file content (no NUL).", true),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file (default false).",
            false,
        ),
        (
            "expected_sha256",
            "string",
            "Required sha256 of the current file when overwriting.",
            false,
        ),
        (
            "expected_content_prefix",
            "string",
            "Required prefix of the current file when overwriting.",
            false,
        ),
    ]))
}
