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

pub(crate) fn computer_find_elements_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose Accessibility surface is searched."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Opaque process-local surface_id returned by computer_list_windows."},
            "role": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Optional exact Accessibility role match."},
            "subrole": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Optional exact Accessibility subrole match."},
            "label": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Optional case-sensitive literal substring matched only against title, description, or placeholder; AXValue is never searched."},
            "focused": {"type": "boolean", "description": "Optional exact focused-state match; unknown/null state does not match."},
            "enabled": {"type": "boolean", "description": "Optional exact enabled-state match; unknown/null state does not match."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 32, "description": "Maximum matching elements returned; defaults to 8."}
        },
        "required": ["client_id", "surface_id"]
    })
}

pub(crate) fn computer_element_state_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose observed element is revalidated."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact opaque process-local surface_id that owns the element."},
            "element_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact ephemeral element_id returned by computer_accessibility_tree or computer_find_elements."}
        },
        "required": ["client_id", "surface_id", "element_id"]
    })
}

pub(crate) fn computer_activate_window_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose already-observed window is activated."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact opaque process-local surface_id returned by computer_list_windows."}
        },
        "required": ["client_id", "surface_id"]
    })
}

pub(crate) fn computer_control_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose macOS Accessibility element is controlled."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact opaque surface_id used to obtain the element."},
            "element_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Opaque process-local element_id returned by computer_accessibility_tree or computer_find_elements."},
            "action": {"type": "string", "enum": ["press", "focus"], "description": "Bounded control action. CU-AX2 supports only press and focus."}
        },
        "required": ["client_id", "surface_id", "element_id", "action"]
    })
}

pub(crate) fn computer_input_text_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "client_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact Runner client_id whose already-focused macOS Accessibility text element is mutated."},
            "surface_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Exact opaque surface_id used to obtain the element."},
            "element_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Opaque process-local element_id returned by computer_accessibility_tree or computer_find_elements."},
            "text": {"type": "string", "minLength": 1, "maxLength": 2048, "description": "Caller text written verbatim with AXValue. Runtime enforces a 2048-byte UTF-8 ceiling and rejects NUL; the target must already be focused and empty."}
        },
        "required": ["client_id", "surface_id", "element_id", "text"]
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
