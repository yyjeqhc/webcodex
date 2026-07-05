use serde_json::{json, Value};

use super::super::super::tool_spec::ToolSpec;
use crate::tool_runtime::sessions::{
    TOOL_ASSERTION_NAME_FIELD, TOOL_EXPECTED_FAILURE_FIELD, TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
};

pub(crate) fn with_common_testing_metadata(mut spec: ToolSpec) -> ToolSpec {
    let Some(properties) = spec
        .input_schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return spec;
    };
    properties
        .entry(TOOL_EXPECTED_FAILURE_FIELD.to_string())
        .or_insert_with(|| {
        json!({
            "type": "boolean",
            "description": "Optional testing/smoke metadata only. When true, a failed call is classified as an expected failure in session handoff/finish summaries. Does not change authorization, permission, execution, hard guards, command_started, or the immediate success/error result."
        })
    });
    properties
        .entry(TOOL_EXPECTED_FAILURE_KIND_FIELD.to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Optional testing/smoke metadata only. Expected structured failure_kind or error_kind for an expected failure. Does not change tool behavior or safety decisions."
            })
        });
    properties
        .entry(TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD.to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Alias for expected_failure_kind for testing/smoke callers. Matches structured failure_kind or error_kind and does not change tool behavior."
            })
        });
    properties
        .entry(TOOL_ASSERTION_NAME_FIELD.to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Optional testing/smoke assertion label recorded in the session ledger. Does not change authorization, permission, execution, or immediate tool output."
            })
        });
    spec
}
