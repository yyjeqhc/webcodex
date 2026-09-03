use serde_json::{json, Value};

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};

fn state_schema() -> Value {
    json!({"type":"string","enum":["starting","running","waiting_permission","completed","failed","cancelled","lost"]})
}

fn execution_state_schema() -> Value {
    json!({"type":"string","enum":["not_started","started","outcome_unknown","completed"]})
}

fn terminal_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type":"object",
                "additionalProperties":false,
                "properties":{
                    "stop_reason": nullable_schema("string", "Correlated stable ACP v1 stop reason when available."),
                    "error_code": nullable_schema("string", "Bounded protocol/provider terminal error code when available."),
                    "message": nullable_schema("string", "Bounded terminal diagnostic; never reasoning or a transcript."),
                    "completed_at": schema_type("integer", "Unix terminal timestamp.")
                },
                "required":["stop_reason","error_code","message","completed_at"]
            },
            {"type":"null"}
        ]
    })
}

fn usage_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "properties":{
            "used_tokens":{"type":"integer","minimum":0},
            "context_window_tokens":{"type":"integer","minimum":0},
            "cost_amount":{"type":"string"},
            "cost_currency":{"type":"string"}
        }
    })
}

fn event_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "properties":{
            "sequence":{"type":"integer","minimum":1},
            "kind":{
                "type":"string",
                "enum":[
                    "agent_message",
                    "reasoning",
                    "plan",
                    "tool_activity",
                    "file_change",
                    "terminal_activity",
                    "usage",
                    "permission_request",
                    "terminal"
                ]
            },
            "text":nullable_schema("string", "Bounded normalized text when this event kind carries text."),
            "label":nullable_schema("string", "Bounded normalized activity label when present."),
            "status":nullable_schema("string", "Bounded normalized activity status when present."),
            "usage":{"anyOf":[usage_schema(),{"type":"null"}]}
        },
        "required":["sequence","kind","text","label","status","usage"]
    })
}

fn common_run_fields() -> Vec<(&'static str, Value)> {
    vec![
        ("run_id", schema_type("string", "Opaque CodingAgentRun id.")),
        (
            "project",
            schema_type("string", "Exact registered runtime Project id."),
        ),
        (
            "provider_id",
            schema_type("string", "Logical operator-configured provider id."),
        ),
        ("state", state_schema()),
        ("execution_state", execution_state_schema()),
        ("terminal", terminal_schema()),
        (
            "error_kind",
            schema_type(
                "string",
                "Bounded failure classification when unsuccessful.",
            ),
        ),
    ]
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    let mut fields = common_run_fields();
    match name {
        "coding_agent_start" => {
            fields.push((
                "observation_token",
                schema_type("string", "Opaque Run-bound observation token."),
            ));
            Some(wrapped_output_schema(fields))
        }
        "coding_agent_observe" => {
            fields.extend([
                ("events", array_schema(event_schema(), "Only-new retained normalized CodingAgentRun events; raw ACP JSON is never exposed.")),
                ("observation_token", schema_type("string", "Opaque Run-bound token for the next observation.")),
                ("has_more", schema_type("boolean", "True when retained newer events remain after this page.")),
                ("history_lost", schema_type("boolean", "True when the requested cursor predates retained history or the Server epoch rebaselined.")),
                ("first_retained_sequence", schema_type("integer", "First currently retained Runner event sequence.")),
            ]);
            Some(wrapped_output_schema(fields))
        }
        "coding_agent_cancel" => {
            fields.push((
                "cancel_requested",
                schema_type(
                    "boolean",
                    "True when cancellation was requested for a nonterminal Run.",
                ),
            ));
            Some(wrapped_output_schema(fields))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coding_agent_event_schema_is_closed_and_terminal_is_nullable() {
        let event = event_schema();
        assert_eq!(event["additionalProperties"], false);
        assert_eq!(
            event["properties"]["kind"]["enum"],
            json!([
                "agent_message",
                "reasoning",
                "plan",
                "tool_activity",
                "file_change",
                "terminal_activity",
                "usage",
                "permission_request",
                "terminal"
            ])
        );
        assert_eq!(
            event["properties"]["usage"]["anyOf"][0]["additionalProperties"],
            false
        );
        assert_eq!(event["properties"]["usage"]["anyOf"][1]["type"], "null");

        let terminal = terminal_schema();
        assert_eq!(terminal["anyOf"][0]["additionalProperties"], false);
        assert_eq!(terminal["anyOf"][1]["type"], "null");

        let observe = output_schema_for_tool("coding_agent_observe").unwrap();
        let serialized = serde_json::to_string(&observe).unwrap();
        assert!(serialized.contains("agent_message"));
        assert!(!serialized.contains("agentmessage"));
    }
}
