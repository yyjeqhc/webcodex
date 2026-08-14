use super::common::wrapped_output_schema;
use serde_json::{json, Value};

fn surface_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "surface_id": {"type": "string"},
            "application": {"type": "string"},
            "title": {"type": "string"},
            "width": {"type": "integer", "minimum": 1},
            "height": {"type": "integer", "minimum": 1},
            "focused": {
                "description": "Exact window focus when reliably known; null when the platform cannot distinguish exact focused-window state (currently macOS).",
                "anyOf": [{"type": "boolean"}, {"type": "null"}]
            },
            "active": {
                "description": "Reliable platform active/frontmost signal; on macOS this is application-level and multiple windows from the frontmost application may be true.",
                "anyOf": [{"type": "boolean"}, {"type": "null"}]
            }
        },
        "required": ["surface_id", "application", "title", "width", "height", "focused", "active"]
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "computer_list_windows" => Some(wrapped_output_schema(vec![
            (
                "windows",
                json!({"type": "array", "maxItems": 64, "items": surface_schema()}),
            ),
            (
                "count",
                json!({"type": "integer", "minimum": 0, "maximum": 64}),
            ),
            ("truncated", json!({"type": "boolean"})),
        ])),
        "computer_snapshot" => Some(wrapped_output_schema(vec![
            (
                "client_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            ("surface", surface_schema()),
            (
                "width",
                json!({"type": "integer", "minimum": 1, "maximum": 4096}),
            ),
            (
                "height",
                json!({"type": "integer", "minimum": 1, "maximum": 4096}),
            ),
            (
                "mime_type",
                json!({"type": "string", "enum": ["image/jpeg"]}),
            ),
            (
                "file_bytes",
                json!({"type": "integer", "minimum": 1, "maximum": 1048576}),
            ),
            ("content_base64", json!({"type": "string"})),
        ])),
        _ => None,
    }
}
