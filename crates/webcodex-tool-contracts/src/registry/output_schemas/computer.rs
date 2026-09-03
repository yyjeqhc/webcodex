use super::common::wrapped_output_schema;
use serde_json::{json, Value};

fn strict_computer_output_schema(output_properties: Vec<(&str, Value)>) -> Value {
    let mut schema = wrapped_output_schema(output_properties);
    let properties = schema["properties"]["output"]["properties"]
        .as_object_mut()
        .expect("wrapped Computer output properties");
    properties
        .entry("error_kind".to_string())
        .or_insert_with(|| json!({"type": "string", "maxLength": 128}));
    properties
        .entry("failure_kind".to_string())
        .or_insert_with(|| json!({"type": "string", "maxLength": 128}));
    properties
        .entry("message".to_string())
        .or_insert_with(|| json!({"type": "string", "maxLength": 256}));
    properties
        .entry("execution_state".to_string())
        .or_insert_with(
            || json!({"type": "string", "enum": ["not_started", "completed", "outcome_unknown"]}),
        );
    properties
        .entry("state_changed".to_string())
        .or_insert_with(|| json!({"type": "boolean"}));
    properties
        .entry("reconcile_with".to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "enum": [
                    "computer_list_windows",
                    "computer_snapshot_display",
                    "read_project_artifact_metadata"
                ]
            })
        });
    schema["properties"]["output"]["additionalProperties"] = json!(false);
    schema
}

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
                    "computer_application_discovery": {"type": "boolean"},
                    "computer_application_launch": {"type": "boolean"},
                    "computer_display_observe": {"type": "boolean"},
                    "computer_pointer_control": {"type": "boolean"},
                    "computer_clipboard_read": {"type": "boolean"},
                    "computer_clipboard_write": {"type": "boolean"},
                    "computer_snapshot_region": {"type": "boolean"},
                    "computer_accessibility_observe": {"type": "boolean"}
                },
                "required": ["computer_observe", "computer_application_discovery", "computer_application_launch", "computer_display_observe", "computer_pointer_control", "computer_clipboard_read", "computer_clipboard_write", "computer_snapshot_region", "computer_accessibility_observe"]
            }
        },
        "required": ["client_id", "display_name", "connected", "capabilities"]
    })
}

fn application_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "application_id": {"type": "string", "pattern": "^application_[0-9a-f]{32}$", "maxLength": 128},
            "display_name": {"type": "string", "minLength": 1, "maxLength": 256}
        },
        "required": ["application_id", "display_name"]
    })
}

fn display_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "display_id": {"type": "string", "pattern": "^display_[0-9a-f]{32}$", "maxLength": 128},
            "width": {"type": "integer", "minimum": 1, "maximum": 4294967295u64},
            "height": {"type": "integer", "minimum": 1, "maximum": 4294967295u64},
            "primary": {"type": "boolean"}
        },
        "required": ["display_id", "width", "height", "primary"]
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

fn accessibility_match_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "element_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "role": {"type": "string", "minLength": 1, "maxLength": 256},
            "subrole": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "title": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "description": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "placeholder": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}]},
            "enabled": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "focused": {"anyOf": [{"type": "boolean"}, {"type": "null"}]}
        },
        "required": ["element_id", "role", "subrole", "title", "description", "placeholder", "enabled", "focused"]
    })
}

fn snapshot_region_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "x": {"type": "integer", "minimum": 0, "maximum": 4294967295u64},
            "y": {"type": "integer", "minimum": 0, "maximum": 4294967295u64},
            "width": {"type": "integer", "minimum": 1, "maximum": 4294967295u64},
            "height": {"type": "integer", "minimum": 1, "maximum": 4294967295u64}
        },
        "required": ["x", "y", "width", "height"]
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
        "computer_list_displays" => Some(strict_computer_output_schema(vec![
            (
                "displays",
                json!({"type": "array", "maxItems": 16, "items": display_schema()}),
            ),
            (
                "count",
                json!({"type": "integer", "minimum": 0, "maximum": 16}),
            ),
            ("truncated", json!({"type": "boolean"})),
        ])),
        "computer_list_applications" => Some(wrapped_output_schema(vec![
            (
                "applications",
                json!({"type": "array", "maxItems": 64, "items": application_schema()}),
            ),
            (
                "count",
                json!({"type": "integer", "minimum": 0, "maximum": 64}),
            ),
            ("truncated", json!({"type": "boolean"})),
        ])),
        "computer_launch_application" => Some(strict_computer_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["windows", "macos"]}),
            ),
            (
                "application_id",
                json!({"type": "string", "pattern": "^application_[0-9a-f]{32}$", "maxLength": 128}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
        ])),
        "computer_accessibility_status" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            ("trusted", json!({"type": "boolean"})),
        ])),
        "computer_accessibility_tree" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "observation_generation",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
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
        "computer_find_elements" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "observation_generation",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "elements",
                json!({"type": "array", "maxItems": 32, "items": accessibility_match_schema()}),
            ),
            (
                "count",
                json!({"type": "integer", "minimum": 0, "maximum": 32}),
            ),
            (
                "scanned_nodes",
                json!({"type": "integer", "minimum": 1, "maximum": 256}),
            ),
            ("truncated", json!({"type": "boolean"})),
        ])),
        "computer_element_state" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "element_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "observation_generation",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "enabled",
                json!({"anyOf": [{"type": "boolean"}, {"type": "null"}]}),
            ),
            (
                "focused",
                json!({"anyOf": [{"type": "boolean"}, {"type": "null"}]}),
            ),
            ("protected", json!({"type": "boolean"})),
            (
                "value_empty",
                json!({"anyOf": [{"type": "boolean"}, {"type": "null"}]}),
            ),
            ("can_press", json!({"type": "boolean"})),
            ("can_focus", json!({"type": "boolean"})),
            ("can_input_text", json!({"type": "boolean"})),
        ])),
        "computer_activate_window" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
        ])),
        "computer_control" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
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
        "computer_scroll_to_element" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "element_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
        ])),
        "computer_key_input" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "key",
                json!({"type": "string", "enum": ["enter", "escape", "tab", "arrow_up", "arrow_down", "arrow_left", "arrow_right", "page_up", "page_down", "home", "end"]}),
            ),
            (
                "modifiers",
                json!({"type": "array", "maxItems": 4, "uniqueItems": true, "items": {"type": "string", "enum": ["shift", "control", "option", "command"]}}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
        ])),
        "computer_input_text" => Some(wrapped_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["macos", "windows"]}),
            ),
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
                "source_width",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "source_height",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            ("region", snapshot_region_schema()),
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
            (
                "sha256",
                json!({"type": "string", "pattern": "^[0-9a-f]{64}$"}),
            ),
            (
                "captured_at_unix_ms",
                json!({"type": "integer", "minimum": 1, "maximum": 9007199254740991u64}),
            ),
            ("content_base64", json!({"type": "string"})),
        ])),
        "computer_snapshot_display" => Some(strict_computer_output_schema(vec![
            (
                "client_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "display_id",
                json!({"type": "string", "pattern": "^display_[0-9a-f]{32}$", "maxLength": 128}),
            ),
            (
                "snapshot_generation",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "source_width",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "source_height",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
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
                json!({"type": "string", "const": "image/jpeg"}),
            ),
            (
                "file_bytes",
                json!({"type": "integer", "minimum": 1, "maximum": 1048576}),
            ),
            (
                "sha256",
                json!({"type": "string", "pattern": "^[0-9a-f]{64}$"}),
            ),
            (
                "captured_at_unix_ms",
                json!({"type": "integer", "minimum": 1, "maximum": 9007199254740991u64}),
            ),
            ("content_base64", json!({"type": "string"})),
        ])),
        "computer_save_snapshot" => Some(wrapped_output_schema(vec![
            ("project", json!({"type": "string", "minLength": 1})),
            (
                "path",
                json!({"type": "string", "minLength": 1, "maxLength": 4096}),
            ),
            (
                "client_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "surface_id",
                json!({"type": "string", "minLength": 1, "maxLength": 128}),
            ),
            (
                "source_width",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            (
                "source_height",
                json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
            ),
            ("region", snapshot_region_schema()),
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
                json!({"type": "string", "enum": ["image/png", "image/jpeg", "image/webp"]}),
            ),
            (
                "file_bytes",
                json!({"type": "integer", "minimum": 1, "maximum": 1048576}),
            ),
            (
                "sha256",
                json!({"type": "string", "pattern": "^[0-9a-f]{64}$"}),
            ),
            ("saved", json!({"type": "boolean", "const": true})),
        ])),
        "computer_read_clipboard" => {
            let mut schema = strict_computer_output_schema(vec![
                (
                    "platform",
                    json!({"type": "string", "enum": ["windows", "macos"]}),
                ),
                ("available", json!({"type": "boolean"})),
                ("text", json!({"type": "string", "maxLength": 16384})),
                (
                    "text_bytes",
                    json!({"type": "integer", "minimum": 0, "maximum": 16384}),
                ),
            ]);
            schema["properties"]["output"]["allOf"] = json!([
                {
                    "if": {"properties": {"available": {"const": true}}, "required": ["available"]},
                    "then": {"required": ["platform", "available", "text", "text_bytes"]}
                },
                {
                    "if": {"properties": {"available": {"const": false}}, "required": ["available"]},
                    "then": {"required": ["platform", "available", "text_bytes"], "not": {"required": ["text"]}}
                }
            ]);
            Some(schema)
        }
        "computer_write_clipboard" => Some(strict_computer_output_schema(vec![
            (
                "platform",
                json!({"type": "string", "enum": ["windows", "macos"]}),
            ),
            (
                "text_bytes",
                json!({"type": "integer", "minimum": 1, "maximum": 16384}),
            ),
            ("success", json!({"type": "boolean", "const": true})),
            (
                "execution_state",
                json!({"type": "string", "enum": ["not_started", "completed", "outcome_unknown"]}),
            ),
            ("state_changed", json!({"type": "boolean"})),
            ("error_kind", json!({"type": "string", "maxLength": 128})),
        ])),
        "computer_pointer_move" | "computer_pointer_click" => {
            Some(strict_computer_output_schema(vec![
                (
                    "platform",
                    json!({"type": "string", "enum": ["windows", "macos"]}),
                ),
                (
                    "display_id",
                    json!({"type": "string", "pattern": "^display_[0-9a-f]{32}$", "maxLength": 128}),
                ),
                (
                    "snapshot_generation",
                    json!({"type": "integer", "minimum": 1, "maximum": 4294967295u64}),
                ),
                (
                    "x",
                    json!({"type": "integer", "minimum": 0, "maximum": 4294967295u64}),
                ),
                (
                    "y",
                    json!({"type": "integer", "minimum": 0, "maximum": 4294967295u64}),
                ),
                ("success", json!({"type": "boolean", "const": true})),
                (
                    "execution_state",
                    json!({"type": "string", "enum": ["not_started", "completed", "outcome_unknown"]}),
                ),
                ("state_changed", json!({"type": "boolean"})),
                ("error_kind", json!({"type": "string", "maxLength": 128})),
                (
                    "reconcile_with",
                    json!({"type": "string", "const": "computer_snapshot_display"}),
                ),
            ]))
        }
        _ => None,
    }
}
