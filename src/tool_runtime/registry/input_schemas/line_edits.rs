use serde_json::{json, Value};

use super::common::{
    object_schema, with_optional_session_id, OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION,
};

pub(crate) fn replace_line_range_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "start_line",
            "integer",
            "1-based inclusive start line.",
            true,
        ),
        ("end_line", "integer", "1-based inclusive end line.", true),
        (
            "new_text",
            "string",
            "Replacement text; empty deletes the range.",
            true,
        ),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for the original range text.",
            false,
        ),
        (
            "expected_old_prefix",
            "string",
            "Optional prefix guard for the original range text.",
            false,
        ),
    ]))
}

pub(crate) fn insert_at_line_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "line",
            "integer",
            "1-based insertion line; total_lines+1 appends at EOF.",
            true,
        ),
        ("text", "string", "Text to insert.", true),
        (
            "expected_anchor_sha256",
            "string",
            "Optional sha256 guard for anchor line or empty EOF anchor.",
            false,
        ),
        (
            "expected_anchor_prefix",
            "string",
            "Optional prefix guard for anchor line or empty EOF anchor.",
            false,
        ),
    ]))
}

pub(crate) fn delete_line_range_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "start_line",
            "integer",
            "1-based inclusive start line.",
            true,
        ),
        ("end_line", "integer", "1-based inclusive end line.", true),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for the original range text.",
            false,
        ),
        (
            "expected_old_prefix",
            "string",
            "Optional prefix guard for the original range text.",
            false,
        ),
    ]))
}

pub(crate) fn apply_text_edits_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Agent-registered project id."
            },
            "path": {
                "type": "string",
                "description": "Project-relative file path."
            },
            "edits": {
                "type": "array",
                "minItems": 1,
                "maxItems": 20,
                "description": "Ordered list of 1..20 atomic edits.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": [
                                "replace_exact",
                                "insert_after",
                                "insert_before",
                                "delete_exact"
                            ],
                            "description": "Atomic edit kind."
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Exact text to replace or delete, required by replace_exact/delete_exact."
                        },
                        "new_text": {
                            "type": "string",
                            "description": "Replacement or inserted text, required by replace_exact/insert_before/insert_after."
                        },
                        "anchor_text": {
                            "type": "string",
                            "description": "Unique anchor text required by insert_before/insert_after."
                        }
                    },
                    "required": ["kind"]
                }
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true, compute the plan without writing."
            },
            "expected_file_sha256": {
                "type": "string",
                "description": "Optional sha256 guard for the whole original file."
            },
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project", "path", "edits"],
        "additionalProperties": false
    })
}
