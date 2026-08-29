use serde_json::{json, Value};

pub(super) const OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION: &str = "Optional explicit wc_sess_* id returned by work_on_project or another compatible Session bootstrap. When provided, this tool call is recorded in that exact Session ledger; omission leaves the call unlinked to Workflow Session state.";

pub(super) const UNIFIED_DIFF_FIELD_DESCRIPTION: &str = "Raw standard unified diff only. Do not include shell heredocs or Codex apply_patch wrapper syntax such as *** Begin Patch / *** Update File / *** End Patch. The first non-empty line should be diff --git ..., --- ..., or another git-apply-compatible unified diff header.";

pub(super) fn object_schema(fields: Vec<(&str, &str, &str, bool)>) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, kind, description, is_required) in fields {
        let schema = if kind == "array" {
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": description,
            })
        } else {
            json!({
                "type": kind,
                "description": description,
            })
        };
        properties.insert(name.to_string(), schema);
        if is_required {
            required.push(Value::String(name.to_string()));
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

pub(super) fn with_optional_session_id(
    mut fields: Vec<(&'static str, &'static str, &'static str, bool)>,
) -> Vec<(&'static str, &'static str, &'static str, bool)> {
    fields.push((
        "session_id",
        "string",
        OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION,
        false,
    ));
    fields
}
