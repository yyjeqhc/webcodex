use serde_json::{json, Value};

use super::super::tool_definition::runtime_tool_effect_annotations;

pub fn tool_annotations(name: &str) -> Value {
    let effects = runtime_tool_effect_annotations(name);
    json!({
        "readOnlyHint": effects.read_only_hint,
        "destructiveHint": effects.destructive_hint,
        "idempotentHint": effects.idempotent_hint,
        "openWorldHint": effects.open_world_hint,
    })
}
