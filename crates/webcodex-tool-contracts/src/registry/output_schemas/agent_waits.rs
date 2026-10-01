use super::common::{schema_type, wrapped_output_schema};
use serde_json::{json, Value};

fn nullable_integer(description: &str) -> Value {
    json!({"anyOf":[{"type":"integer"},{"type":"null"}],"description":description})
}

fn wait_source_schema() -> Value {
    json!({
        "type":"object","additionalProperties":false,
        "properties": {
            "ordinal":{"type":"integer","minimum":0,"maximum":7},
            "kind":{"type":"string","const":"agent_task_terminal"},
            "task_id":{"type":"string","pattern":"^wc_agent_task_[A-Za-z0-9_-]{16}$","description":"Exact source identity only; no inherited Task authority."}
        },
        "required":["ordinal","kind","task_id"]
    })
}

fn wait_match_schema() -> Value {
    json!({
        "type":"object","additionalProperties":false,
        "properties": {
            "sequence":{"type":"integer","minimum":1,"maximum":8},
            "kind":{"type":"string","const":"agent_task_terminal"},
            "task_id":{"type":"string","pattern":"^wc_agent_task_[A-Za-z0-9_-]{16}$"},
            "task_attempt_id":{"type":"string","pattern":"^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"},
            "terminal_task_state":{"type":"string","enum":["succeeded","failed"]},
            "occurred_at_unix_ms":schema_type("integer","Authoritative source terminal transition timestamp. The Wait stores no terminal result/reason body.")
        },
        "required":["sequence","kind","task_id","task_attempt_id","terminal_task_state","occurred_at_unix_ms"]
    })
}

fn wait_schema() -> Value {
    json!({
        "type":"object","additionalProperties":false,
        "properties": {
            "wait_id":{"type":"string","pattern":"^wc_agent_wait_[A-Za-z0-9_-]{16}$"},
            "target_agent_id":{"type":"string","pattern":"^wc_dagent_[A-Za-z0-9_-]{16}$"},
            "goal_id":{"anyOf":[{"type":"string","pattern":"^wc_goal_[A-Za-z0-9_-]{16}$"},{"type":"null"}],"description":"Optional exact Goal correlation reference only; grants no Goal or source authority."},
            "state":{"type":"string","enum":["waiting","triggered","resumed","cancelled"]},
            "mode":{"type":"string","enum":["any","all"]},
            "revision":{"type":"integer","minimum":1},
            "created_at_unix_ms":schema_type("integer","Wait creation time."),
            "updated_at_unix_ms":schema_type("integer","Latest authoritative Wait mutation time."),
            "triggered_at_unix_ms":nullable_integer("Time the selected ANY/ALL trigger condition became true, or null while waiting."),
            "resumed_at_unix_ms":nullable_integer("Exact Wait-origin Wake consume time, or null before resumed."),
            "cancelled_at_unix_ms":nullable_integer("Explicit pre-dispatch cancellation time, or null otherwise."),
            "source_count":{"type":"integer","minimum":1,"maximum":8},
            "match_count":{"type":"integer","minimum":0,"maximum":8},
            "match_sequence":{"type":"integer","minimum":0,"maximum":8},
            "sources":{"type":"array","maxItems":8,"items":wait_source_schema(),"description":"Bounded source references only; references grant no source authority."},
            "matches":{"type":"array","maxItems":8,"items":wait_match_schema(),"description":"Bounded semantic fact references only; no Task instruction/result/reason/log/fence/token payloads."}
        },
        "required":["wait_id","target_agent_id","goal_id","state","mode","revision","created_at_unix_ms","updated_at_unix_ms","triggered_at_unix_ms","resumed_at_unix_ms","cancelled_at_unix_ms","source_count","match_count","match_sequence","sources","matches"]
    })
}

fn wait_model_schema() -> Value {
    let mut schema = wait_schema();
    let properties = schema["properties"].as_object_mut().unwrap();
    properties.retain(|key, _| {
        matches!(
            key.as_str(),
            "wait_id"
                | "goal_id"
                | "state"
                | "mode"
                | "source_count"
                | "match_count"
                | "sources"
                | "matches"
        )
    });
    let sources = properties.get_mut("sources").unwrap();
    sources["items"]["properties"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| matches!(key.as_str(), "kind" | "task_id"));
    sources["items"]["required"] = json!(["kind", "task_id"]);
    let matches = properties.get_mut("matches").unwrap();
    matches["items"]["properties"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| {
            matches!(
                key.as_str(),
                "task_id" | "task_attempt_id" | "terminal_task_state"
            )
        });
    matches["items"]["required"] = json!(["task_id", "task_attempt_id", "terminal_task_state"]);
    schema["required"] = json!([
        "wait_id",
        "goal_id",
        "state",
        "mode",
        "source_count",
        "match_count",
        "sources",
        "matches"
    ]);
    schema
}

pub fn output_schema_for_tool(name: &str) -> Option<Value> {
    let schema = match name {
        "wait_for_agent_events" => wrapped_output_schema(vec![
            ("agent_wait", wait_model_schema()),
            ("agent_continuation", super::communication::agent_continuation_projection_schema()),
            ("replayed", schema_type("boolean","True for exact keyed Wait creation replay.")),
            ("state_changed", schema_type("boolean","Whether this call first created the Wait and any immediate terminal-source matches.")),
        ]),
        "read_agent_wait" => wrapped_output_schema(vec![("agent_wait", wait_model_schema())]),
        "get_agent_wait_state" => wrapped_output_schema(vec![("agent_wait", wait_schema())]),
        "cancel_agent_wait" => wrapped_output_schema(vec![
            ("agent_wait", wait_model_schema()),
            ("replayed", schema_type("boolean","True for exact keyed cancellation replay.")),
            ("state_changed", schema_type("boolean","True only for the first successful pre-dispatch cancellation.")),
        ]),
        _ => return None,
    };
    Some(schema)
}
