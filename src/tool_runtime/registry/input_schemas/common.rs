use serde_json::{json, Value};

pub(super) const OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION: &str = "Optional explicit wc_sess_* id returned by start_session. When provided, this tool call is recorded in that session ledger and wins over any current-session binding.";
const ALLOW_CROSS_PROJECT_SESSION_DESCRIPTION: &str = "Advanced/debug escape hatch. When true, allow recording a project tool call into a session whose associated project differs from the request project; the runtime still emits session_project_mismatch warning metadata.";

pub(super) const PATCH_FIELD_DESCRIPTION: &str = "raw standard unified diff only. Do not include Codex apply_patch wrapper syntax, shell heredocs, \"*** Begin Patch\", \"*** Update File\", or \"*** End Patch\". The first non-empty line should be \"diff --git ...\", \"--- ...\", or another git-apply-compatible unified diff header.";

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
    fields.push((
        "allow_cross_project_session",
        "boolean",
        ALLOW_CROSS_PROJECT_SESSION_DESCRIPTION,
        false,
    ));
    fields
}
