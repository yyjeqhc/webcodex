use super::common::wrapped_output_schema;
use serde_json::{json, Value};

fn target_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "display_name": {"anyOf": [{"type": "string", "maxLength": 200}, {"type": "null"}]},
            "connected": {"type": "boolean"},
            "capabilities": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "computer_observe": {"type": "boolean"},
                    "computer_accessibility_observe": {"type": "boolean"}
                },
                "required": ["computer_observe", "computer_accessibility_observe"]
            }
        },
        "required": ["client_id", "display_name", "connected", "capabilities"]
    })
}

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

fn accessibility_node_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "element_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "parent_element_id": {"anyOf": [{"type": "string", "minLength": 1, "maxLength": 128}, {"type": "null"}]},
            "depth": {"type": "integer", "minimum": 0, "maximum": 8},
            "role": {"type": "string", "maxLength": 256},
            "subrole": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "title": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "description": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "value": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "placeholder": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "enabled": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "focused": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "child_count": {"type": "integer", "minimum": 0}
        },
        "required": ["element_id", "parent_element_id", "depth", "role", "subrole", "title", "description", "value", "placeholder", "enabled", "focused", "child_count"]
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "computer_list_targets" => Some(wrapped_output_schema(vec![
            (
                "targets",
                json!({"type": "array", "maxItems": 64, "items": target_schema()}),
            ),
            (
                "count",
                json!({"type": "integer", "minimum": 0, "maximum": 64}),
            ),
            ("total_count", json!({"type": "integer", "minimum": 0})),
            ("truncated", json!({"type": "boolean"})),
        ])),
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
        "computer_accessibility_status" => Some(wrapped_output_schema(vec![
            ("platform", json!({"type": "string", "enum": ["macos"]})),
            ("trusted", json!({"type": "boolean"})),
        ])),
        "computer_accessibility_tree" => Some(wrapped_output_schema(vec![
            ("platform", json!({"type": "string", "enum": ["macos"]})),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "nodes",
                json!({"type": "array", "maxItems": 256, "items": accessibility_node_schema()}),
            ),
            (
                "node_count",
                json!({"type": "integer", "minimum": 0, "maximum": 256}),
            ),
            ("truncated", json!({"type": "boolean"})),
            (
                "max_depth",
                json!({"type": "integer", "minimum": 0, "maximum": 8}),
            ),
            (
                "max_nodes",
                json!({"type": "integer", "minimum": 1, "maximum": 256}),
            ),
        ])),
        "computer_control" => Some(wrapped_output_schema(vec![
            ("platform", json!({"type": "string", "enum": ["macos"]})),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "element_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "action",
                json!({"type": "string", "enum": ["press", "focus"]}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
        ])),
        "computer_input_text" => Some(wrapped_output_schema(vec![
            ("platform", json!({"type": "string", "enum": ["macos"]})),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "element_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "text_bytes",
                json!({"type": "integer", "minimum": 1, "maximum": 2048}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
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
