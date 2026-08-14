use serde_json::{json, Value};

pub(crate) fn computer_list_windows_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose desktop is observed."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 64, "description": "Optional bounded window count; clamped to 64."}
        },
        "required": ["client_id"]
    })
}

pub(crate) fn computer_accessibility_status_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose Accessibility trust is queried without prompting."}
        },
        "required": ["client_id"]
    })
}

pub(crate) fn computer_accessibility_tree_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose desktop is inspected."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Opaque process-local surface_id returned by computer_list_windows."},
            "max_depth": {"type": "integer", "minimum": 0, "maximum": 8, "description": "Maximum AX descendant depth; clamped to 8."},
            "max_nodes": {"type": "integer", "minimum": 1, "maximum": 256, "description": "Maximum semantic AX elements returned; clamped to 256."}
        },
        "required": ["client_id", "surface_id"]
    })
}

pub(crate) fn computer_snapshot_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose desktop is observed."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Opaque process-local surface_id returned by computer_list_windows."}
        },
        "required": ["client_id", "surface_id"]
    })
}
