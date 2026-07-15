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
            "changes": {
                "type": "array",
                "minItems": 1,
                "maxItems": 16,
                "description": "Transactional list of 1..16 file changes. Existing files require expected_sha256; the whole batch is preflighted before mutation.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["edit", "create", "delete", "rename"],
                            "description": "File change kind."
                        },
                        "path": {
                            "type": "string",
                            "description": "Project-relative source or target path."
                        },
                        "to_path": {
                            "type": "string",
                            "description": "Project-relative destination path required by rename."
                        },
                        "content": {
                            "type": "string",
                            "description": "Complete UTF-8 content required by create."
                        },
                        "expected_sha256": {
                            "type": "string",
                            "pattern": "^[a-f0-9]{64}$",
                            "description": "Required current-file sha256 for edit, delete, and rename."
                        },
                        "edits": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 20,
                            "description": "Exact edits required by kind=edit.",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {
                                    "kind": {
                                        "type": "string",
                                        "enum": ["replace_exact", "insert_after", "insert_before", "delete_exact"]
                                    },
                                    "old_text": { "type": "string" },
                                    "new_text": { "type": "string" },
                                    "anchor_text": { "type": "string" }
                                },
                                "required": ["kind"]
                            }
                        }
                    },
                    "required": ["kind", "path"]
                }
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true, compute the plan without writing."
            },
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project", "changes"],
        "additionalProperties": false
    })
}
