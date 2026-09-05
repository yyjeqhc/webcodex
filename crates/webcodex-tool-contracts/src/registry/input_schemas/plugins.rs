use serde_json::{json, Value};
use webcodex_core::plugin::{
    PLUGIN_MAX_ARGUMENT_BYTES, PLUGIN_MAX_PROVIDER_ID_BYTES, PLUGIN_MAX_TOOL_NAME_BYTES,
};

pub fn plugin_tool_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "check", "reload", "describe", "call"],
                "description": "Gateway operation. Discovery starts from an exact caller-visible Runner; call uses only an opaque binding from describe."
            },
            "runner": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Exact caller-visible Runner client_id. Required for check/reload/describe and for Runner-scoped list operations."
            },
            "plugin": {
                "type": "string",
                "minLength": 1,
                "maxLength": PLUGIN_MAX_PROVIDER_ID_BYTES,
                "description": "Logical Plugin provider id on the selected exact Runner."
            },
            "tool": {
                "type": "string",
                "minLength": 1,
                "maxLength": PLUGIN_MAX_TOOL_NAME_BYTES,
                "description": "Logical provider-local Plugin tool name. Provider tools never become outer WebCodex MCP tool names."
            },
            "binding": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "pattern": "^wc_pbind_[0-9a-f]{32}$",
                "description": "Opaque exact Runner/provider/tool/schema binding returned by describe. It is observation identity, not authority."
            },
            "arguments": {
                "type": "object",
                "description": format!("Plugin tool arguments matching the schema observed by describe; encoded payload is bounded to {PLUGIN_MAX_ARGUMENT_BYTES} bytes.")
            }
        },
        "required": ["action"],
        "additionalProperties": false,
        "allOf": [
            {
                "if": {"properties": {"action": {"const": "check"}}, "required": ["action"]},
                "then": {"required": ["runner", "plugin"]}
            },
            {
                "if": {"properties": {"action": {"const": "reload"}}, "required": ["action"]},
                "then": {
                    "required": ["runner"],
                    "not": {"required": ["plugin"]}
                }
            },
            {
                "if": {"properties": {"action": {"const": "describe"}}, "required": ["action"]},
                "then": {"required": ["runner", "plugin", "tool"]}
            },
            {
                "if": {"properties": {"action": {"const": "call"}}, "required": ["action"]},
                "then": {
                    "required": ["binding", "arguments"],
                    "not": {"anyOf": [
                        {"required": ["runner"]},
                        {"required": ["plugin"]},
                        {"required": ["tool"]}
                    ]}
                }
            },
            {
                "if": {"required": ["plugin"]},
                "then": {"required": ["runner"]}
            },
            {
                "if": {"required": ["tool"]},
                "then": {"properties": {"action": {"const": "describe"}}}
            },
            {
                "if": {"not": {"properties": {"action": {"const": "call"}}, "required": ["action"]}},
                "then": {"not": {"anyOf": [
                    {"required": ["binding"]},
                    {"required": ["arguments"]}
                ]}}
            }
        ]
    })
}
