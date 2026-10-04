use super::common::default_output_schema;
use serde_json::{json, Value};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    if !matches!(name, "present_docx" | "read_docx_chunk") {
        return None;
    }
    let mut schema = default_output_schema();
    let identity = json!({
        "project": {"type":"string", "minLength":1, "maxLength":512},
        "path": {"type":"string", "minLength":1, "maxLength":512},
        "sha256": {"type":"string", "pattern":"^[0-9a-f]{64}$"}
    });
    let mut properties = identity.as_object().unwrap().clone();
    let key = if name == "present_docx" {
        properties.insert("name".into(), json!({"type":"string","maxLength":255}));
        properties.insert(
            "bytes".into(),
            json!({"type":"integer","minimum":4,"maximum":10485760}),
        );
        "docx_document"
    } else {
        properties.insert(
            "bytes_total".into(),
            json!({"type":"integer","minimum":4,"maximum":10485760}),
        );
        properties.insert(
            "byte_offset".into(),
            json!({"type":"integer","minimum":0,"maximum":10485759}),
        );
        properties.insert(
            "next_byte_offset".into(),
            json!({"anyOf":[{"type":"integer","minimum":1,"maximum":10485759},{"type":"null"}]}),
        );
        properties.insert("complete".into(), json!({"type":"boolean"}));
        "docx_chunk"
    };
    let required: Vec<_> = properties.keys().cloned().collect();
    schema["properties"]["output"]["properties"] = json!({
        key: {"type":"object","properties":properties,"required":required,"additionalProperties":false},
        "error_kind":{"type":"string"}, "state_changed":{"type":"boolean"}
    });
    Some(schema)
}
