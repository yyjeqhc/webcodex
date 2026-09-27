use super::common::{suggested_tool_call_schema, wrapped_output_schema};
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
                    "browser_observe": {"type": "boolean"},
                    "browser_control": {"type": "boolean"},
                    "browser_element_action_admission": {"type": "boolean"},
                    "browser_launch": {"type": "boolean"}
                },
                "required": [
                    "browser_observe",
                    "browser_control",
                    "browser_element_action_admission",
                    "browser_launch"
                ]
            }
        },
        "required": ["client_id", "display_name", "connected", "capabilities"]
    })
}

fn browser_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "browser_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "page_count": {"type": "integer", "minimum": 0, "maximum": 16}
        },
        "required": ["browser_id", "page_count"]
    })
}

fn page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "browser_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "page_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "title": {"type": "string", "maxLength": 256},
            "url": {"type": "string", "maxLength": 2048}
        },
        "required": ["browser_id", "page_id", "title", "url"]
    })
}

fn node_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "role": {"type": "string", "maxLength": 64},
            "name": {"anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}]},
            "description": {"anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}]},
            "value": {"anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}]},
            "group_id": {"anyOf": [{"type": "string", "maxLength": 32}, {"type": "null"}]},
            "group_role": {"anyOf": [{"type": "string", "maxLength": 64}, {"type": "null"}]},
            "group_label": {"anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}]},
            "checked": {"anyOf": [{"type": "string", "maxLength": 32}, {"type": "null"}]},
            "selected": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "required": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "disabled": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "read_only": {"anyOf": [{"type": "boolean"}, {"type": "null"}]},
            "element_id": {"anyOf": [{"type": "string", "minLength": 1, "maxLength": 128}, {"type": "null"}]},
            "actions": {
                "type": "array",
                "maxItems": 5,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["click", "input_text", "select_option", "set_value", "upload_file"]
                }
            },
            "actionable": {"type": "boolean"}
        },
        "required": ["role", "actionable"]
    })
}

fn stability_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "stable": {"type": "boolean"},
            "waited_ms": {"type": "integer", "minimum": 0},
            "reason": {"type": "string", "maxLength": 64}
        },
        "required": ["stable", "waited_ms", "reason"]
    })
}

fn bounded_count(maximum: u64) -> Value {
    json!({"type": "integer", "minimum": 0, "maximum": maximum})
}

fn console_entry_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "level": {"type": "string", "maxLength": 64},
            "text": {"type": "string", "maxLength": 2048},
            "source": {"anyOf": [{"type": "string", "maxLength": 8192}, {"type": "null"}]},
            "timestamp": {"anyOf": [{"type": "number"}, {"type": "null"}]}
        },
        "required": ["level", "text", "source", "timestamp"]
    })
}

fn network_entry_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "method": {"type": "string", "maxLength": 128},
            "url": {"type": "string", "maxLength": 8192},
            "resource_type": {"anyOf": [{"type": "string", "maxLength": 64}, {"type": "null"}]},
            "status": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 65535}, {"type": "null"}]},
            "failed_reason": {"anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}]},
            "timestamp": {"anyOf": [{"type": "number"}, {"type": "null"}]}
        },
        "required": ["method", "url", "resource_type", "status", "failed_reason", "timestamp"]
    })
}

fn recovery_schema() -> Value {
    let client = json!({"type": "string", "minLength": 1, "maxLength": 128});
    let browser = json!({"type": "string", "minLength": 1, "maxLength": 128});
    let page = json!({"type": "string", "minLength": 1, "maxLength": 128});
    let arguments = json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "action": {"type": "string", "const": "browsers"},
                    "client_id": client.clone()
                },
                "required": ["action", "client_id"]
            },
            {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "action": {"type": "string", "const": "pages"},
                    "client_id": client.clone(),
                    "browser_id": browser.clone()
                },
                "required": ["action", "client_id", "browser_id"]
            },
            {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "action": {"type": "string", "const": "snapshot"},
                    "client_id": client,
                    "browser_id": browser,
                    "page_id": page
                },
                "required": ["action", "client_id", "browser_id", "page_id"]
            }
        ]
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "reason": {"type": "string", "maxLength": 256},
            "suggested_call": suggested_tool_call_schema(
                "browser_observe",
                arguments,
                "Observation-first reconciliation call. It never retries the uncertain Browser effect."
            )
        },
        "required": ["reason", "suggested_call"]
    })
}

fn common_fields() -> Vec<(&'static str, Value)> {
    vec![
        (
            "execution_state",
            json!({"type": "string", "enum": ["not_started", "completed", "outcome_unknown"]}),
        ),
        ("state_changed", json!({"type": "boolean"})),
        ("error_kind", json!({"type": "string", "maxLength": 128})),
        ("message", json!({"type": "string", "maxLength": 512})),
        ("recovery", recovery_schema()),
    ]
}

pub fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "browser_observe" => {
            let mut fields = common_fields();
            fields.extend([
                (
                    "targets",
                    json!({"type": "array", "maxItems": 64, "items": target_schema()}),
                ),
                (
                    "browsers",
                    json!({"type": "array", "maxItems": 4, "items": browser_schema()}),
                ),
                (
                    "pages",
                    json!({"type": "array", "maxItems": 32, "items": page_schema()}),
                ),
                (
                    "count",
                    json!({"type": "integer", "minimum": 0, "maximum": 300}),
                ),
                ("total_count", json!({"type": "integer", "minimum": 0})),
                ("truncated", json!({"type": "boolean"})),
                (
                    "snapshot_mode",
                    json!({"type": "string", "enum": ["full", "interactive"]}),
                ),
                ("auto_compacted", json!({"type": "boolean"})),
                (
                    "max_nodes",
                    json!({"type": "integer", "minimum": 1, "maximum": 256}),
                ),
                (
                    "max_depth",
                    json!({"type": "integer", "minimum": 1, "maximum": 32}),
                ),
                ("cursor", json!({"type": "integer", "minimum": 0})),
                ("since_cursor", json!({"type": "integer", "minimum": 0})),
                ("delta_truncated", json!({"type": "boolean"})),
                ("new_console_errors", bounded_count(200)),
                ("new_console_warnings", bounded_count(200)),
                ("new_failed_requests", bounded_count(300)),
                ("new_4xx", bounded_count(300)),
                ("new_5xx", bounded_count(300)),
                (
                    "retained_count",
                    json!({"type": "integer", "minimum": 0, "maximum": 300}),
                ),
                (
                    "entries",
                    json!({
                        "type": "array",
                        "maxItems": 300,
                        "items": {"oneOf": [console_entry_schema(), network_entry_schema()]}
                    }),
                ),
                (
                    "console_retained",
                    json!({"type": "integer", "minimum": 0, "maximum": 200}),
                ),
                (
                    "console_count",
                    json!({"type": "integer", "minimum": 0, "maximum": 200}),
                ),
                ("console_truncated", json!({"type": "boolean"})),
                (
                    "console",
                    json!({"type": "array", "maxItems": 200, "items": console_entry_schema()}),
                ),
                (
                    "network_retained",
                    json!({"type": "integer", "minimum": 0, "maximum": 300}),
                ),
                (
                    "network_count",
                    json!({"type": "integer", "minimum": 0, "maximum": 300}),
                ),
                ("network_truncated", json!({"type": "boolean"})),
                (
                    "network",
                    json!({"type": "array", "maxItems": 300, "items": network_entry_schema()}),
                ),
                (
                    "browser_id",
                    json!({"type": "string", "minLength": 1, "maxLength": 128}),
                ),
                (
                    "page_id",
                    json!({"type": "string", "minLength": 1, "maxLength": 128}),
                ),
                (
                    "snapshot_generation",
                    json!({"type": "integer", "minimum": 1}),
                ),
                (
                    "node_count",
                    json!({"type": "integer", "minimum": 0, "maximum": 256}),
                ),
                (
                    "nodes",
                    json!({"type": "array", "maxItems": 256, "items": node_schema()}),
                ),
                (
                    "content_base64",
                    json!({"type": "string", "maxLength": 1398104}),
                ),
                (
                    "mime_type",
                    json!({"type": "string", "enum": ["image/png"]}),
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
                    "file_bytes",
                    json!({"type": "integer", "minimum": 1, "maximum": 1048576}),
                ),
                (
                    "sha256",
                    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"}),
                ),
            ]);
            let mut schema = wrapped_output_schema(fields);
            schema["properties"]["output"]["additionalProperties"] = json!(false);
            Some(schema)
        }
        "browser_act" => {
            let mut fields = common_fields();
            fields.extend([
                (
                    "browser_id",
                    json!({"type": "string", "minLength": 1, "maxLength": 128}),
                ),
                (
                    "page_id",
                    json!({"type": "string", "minLength": 1, "maxLength": 128}),
                ),
                (
                    "page_count",
                    json!({"type": "integer", "minimum": 0, "maximum": 16}),
                ),
                ("title", json!({"type": "string", "maxLength": 256})),
                ("url", json!({"type": "string", "maxLength": 2048})),
                ("stability", stability_schema()),
            ]);
            let mut schema = wrapped_output_schema(fields);
            schema["properties"]["output"]["additionalProperties"] = json!(false);
            Some(schema)
        }
        _ => None,
    }
}
