use serde_json::Value;

use super::common::{object_schema, with_optional_session_id, PATCH_FIELD_DESCRIPTION};

pub(crate) fn apply_patch_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
    ]))
}

pub(crate) fn apply_patch_checked_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
        (
            "deny_sensitive_paths",
            "boolean",
            "Block sensitive path warnings before applying.",
            false,
        ),
    ]))
}
