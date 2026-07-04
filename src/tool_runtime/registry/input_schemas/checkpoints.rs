use serde_json::{json, Value};

use super::super::super::tool_inputs::{
    CHECKPOINT_KIND_VALUES, CHECKPOINT_VALIDATION_STATUS_VALUES,
};
use super::common::{
    object_schema, with_optional_session_id, OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION,
};

fn checkpoint_project_input_schema(
    fields: Vec<(&'static str, &'static str, &'static str, bool)>,
) -> Value {
    object_schema(with_optional_session_id(fields))
}

pub(crate) fn checkpoint_list_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        (
            "limit",
            "integer",
            "Maximum checkpoints to return (default 20, max 100).",
            false,
        ),
    ])
}

pub(crate) fn checkpoint_show_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        (
            "checkpoint_id",
            "string",
            "wc_ckpt_* id returned by workspace_checkpoint_create.",
            true,
        ),
        (
            "include_diff_stat",
            "boolean",
            "Include tracked/staged diff stat strings (default false).",
            false,
        ),
    ])
}

pub(crate) fn checkpoint_restore_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        ("checkpoint_id", "string", "wc_ckpt_* id to restore.", true),
        ("confirm", "boolean", "Must be true to restore.", true),
    ])
}

pub(crate) fn checkpoint_delete_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        ("checkpoint_id", "string", "wc_ckpt_* id to delete.", true),
        ("confirm", "boolean", "Must be true to delete.", true),
    ])
}

pub(crate) fn checkpoint_validation_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": CHECKPOINT_VALIDATION_STATUS_VALUES,
                "description": "Validation result supplied by the caller. The runtime records metadata only and never runs these commands."
            },
            "commands": {
                "type": "array",
                "items": { "type": "string", "maxLength": 200 },
                "maxItems": 20,
                "description": "Command summaries supplied by the caller. Stdout/stderr and env values are not stored."
            },
            "summary": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ],
                "maxLength": 500,
                "description": "Short validation summary supplied by the caller."
            }
        },
        "required": [],
    })
}

pub(crate) fn checkpoint_labels_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "string",
            "maxLength": 64,
            "pattern": "^[A-Za-z0-9._-]+$"
        },
        "maxItems": 20,
        "description": description,
    })
}

pub(crate) fn checkpoint_create_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Runtime project id."
            },
            "title": {
                "type": "string",
                "description": "Optional human-readable title."
            },
            "note": {
                "type": "string",
                "description": "Optional note; not used by restore."
            },
            "include_untracked": {
                "type": "boolean",
                "description": "Include small non-secret UTF-8 untracked files (default false)."
            },
            "kind": {
                "type": "string",
                "enum": CHECKPOINT_KIND_VALUES,
                "description": "Optional semantic checkpoint kind. Defaults to snapshot."
            },
            "labels": checkpoint_labels_schema("Optional simple ASCII labels for handoff, filtering, or recovery hints."),
            "validation": checkpoint_validation_schema("Optional bounded validation metadata supplied by the caller."),
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project"],
        "additionalProperties": false,
    })
}
