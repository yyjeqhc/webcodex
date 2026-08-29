use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id, UNIFIED_DIFF_FIELD_DESCRIPTION};

pub(crate) fn apply_unified_diff_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("diff", "string", UNIFIED_DIFF_FIELD_DESCRIPTION, true),
        (
            "deny_sensitive_paths",
            "boolean",
            "Optional fail-safe sensitive-path policy. Defaults to true; when true, any sensitive-path warning blocks mutation before git apply --check is dispatched.",
            false,
        ),
    ]));
    schema["properties"]["diff"]["maxLength"] =
        json!(crate::tool_runtime::patch::MAX_UNIFIED_DIFF_BYTES);
    schema["properties"]["deny_sensitive_paths"]["default"] = json!(true);
    schema
}
