use serde_json::{json, Value};

use super::super::super::tool_spec::ToolSpec;

pub(crate) fn with_common_testing_metadata(mut spec: ToolSpec) -> ToolSpec {
    let Some(properties) = spec
        .input_schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return spec;
    };
    properties.entry("expected_failure".to_string()).or_insert_with(|| {
        json!({
            "type": "boolean",
            "description": "Optional testing/smoke metadata only. When true, a failed call is classified as an expected failure in session handoff/finish summaries. Does not change authorization, permission, execution, hard guards, command_started, or the immediate success/error result."
        })
    });
    properties
        .entry("expected_failure_kind".to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Optional testing/smoke metadata only. Expected structured failure_kind or error_kind for an expected failure. Does not change tool behavior or safety decisions."
            })
        });
    properties
        .entry("test_expect_failure_kind".to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Alias for expected_failure_kind for testing/smoke callers. Matches structured failure_kind or error_kind and does not change tool behavior."
            })
        });
    properties.entry("assertion_name".to_string()).or_insert_with(|| {
        json!({
            "type": "string",
            "description": "Optional testing/smoke assertion label recorded in the session ledger. Does not change authorization, permission, execution, or immediate tool output."
        })
    });
    spec
}
