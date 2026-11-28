use super::common::wrapped_output_schema;
use serde_json::{json, Value};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "start_web_preview" => Some(wrapped_output_schema(vec![
            (
                "preview_url",
                json!({"type":["string","null"],"maxLength":2048}),
            ),
            ("job_id", json!({"type":["string","null"],"maxLength":128})),
            (
                "expires_at",
                json!({"type":["string","null"],"maxLength":64}),
            ),
            ("project", json!({"type":"string","maxLength":512})),
            (
                "port",
                json!({"type":"integer","minimum":1,"maximum":65535}),
            ),
            ("status", json!({"enum":["ready","failed"]})),
            (
                "failure_kind",
                json!({"type":["string","null"],"maxLength":64}),
            ),
            ("retry_with_new_key", json!({"type":"boolean"})),
        ])),
        "open_webcodex_workbench" => Some(wrapped_output_schema(vec![
            ("client_id", json!({"type":["string","null"]})),
            ("runners", json!({"type":"object"})),
            ("project", json!({"type":["string","null"]})),
            ("session_id", json!({"type":["string","null"]})),
            (
                "preview_url",
                json!({"type":["string","null"],"maxLength":2048}),
            ),
            ("projects", json!({"type":"object"})),
            (
                "selection",
                json!({"const":"caller_must_choose_runner_project_and_session"}),
            ),
        ])),
        "search_webcodex_resources" => Some(wrapped_output_schema(vec![
            (
                "items",
                json!({"type":"array","maxItems":100,"items":{"type":"object","required":["type","uri","name"],"properties":{
                "type":{"const":"resource_link"},"uri":{"type":"string","maxLength":8192},"name":{"type":"string","maxLength":200},
                "title":{"type":"string","maxLength":200},"description":{"type":"string","maxLength":300},"mimeType":{"type":"string"},"_meta":{"type":"object"}
            },"additionalProperties":false}}),
            ),
            ("total", json!({"type":"integer","minimum":0})),
            ("offset", json!({"type":"integer","minimum":0})),
            ("limit", json!({"type":"integer","minimum":1,"maximum":100})),
            (
                "next_offset",
                json!({"type":["integer","null"],"minimum":0}),
            ),
            (
                "list_truncated",
                json!({"type":"boolean","description":"Source is incomplete; absence of matches does not prove absence of resources."}),
            ),
            ("incomplete", json!({"type":"string"})),
        ])),
        "read_webcodex_resource" => Some(wrapped_output_schema(vec![
            ("uri", json!({"type":"string"})),
            ("kind", json!({"enum":["project","file","goal"]})),
            ("version", json!({"type":["string","integer","null"]})),
            (
                "data",
                json!({"type":"object","description":"Bounded current domain content; binary files return metadata. Historical output observations are never current file truth."}),
            ),
            ("truncated", json!({"type":"boolean"})),
            ("content_policy", json!({"const":"latest_at_read"})),
        ])),
        _ => None,
    }
}
