use super::common::default_output_schema;
use serde_json::{json, Value};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    if name != "present_docx" {
        return None;
    }
    let mut schema = default_output_schema();
    let payload = json!({
        "type":"object",
        "properties":{
            "project":{"type":"string","minLength":1,"maxLength":512},
            "path":{"type":"string","minLength":1,"maxLength":512},
            "sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},
            "name":{"type":"string","maxLength":255},
            "bytes":{"type":"integer","minimum":4,"maximum":10485760}
        },
        "required":["project","path","sha256","name","bytes"],
        "additionalProperties":false
    });
    schema["properties"]["output"]["properties"] = json!({
        "docx_document": payload,
        "error_kind":{"type":"string"},
        "state_changed":{"type":"boolean"}
    });
    Some(schema)
}
