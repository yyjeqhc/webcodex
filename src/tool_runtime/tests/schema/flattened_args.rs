use super::*;

#[test]
fn accepted_flattened_arg_preferred_order_is_unique_and_declared() {
    use crate::tool_runtime::registry::ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER;

    let mut seen = BTreeSet::new();
    for field in ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER {
        assert!(seen.insert(*field), "duplicate preferred field {field}");
    }

    let mut schema_fields = BTreeSet::new();
    for spec in registered_tool_specs() {
        let Some(properties) = spec.input_schema["properties"].as_object() else {
            continue;
        };
        schema_fields.extend(properties.keys().cloned());
    }

    for field in ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER {
        assert!(
            schema_fields.contains(*field),
            "preferred flattened field {field} is not declared by any ToolSpec input schema"
        );
    }
}

#[test]
fn accepted_flattened_args_appends_recorder_field_once() {
    use crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD;

    let spec = ToolSpec {
        name: "synthetic_tool".to_string(),
        description: "Synthetic test tool".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": {"type": "string"},
                TOOL_CALL_RECORDING_SESSION_ID_FIELD: {"type": "string"}
            },
            "required": [],
            "additionalProperties": false
        }),
        output_schema: json!({"type": "object", "additionalProperties": true}),
        annotations: json!({}),
    };

    let accepted = crate::tool_runtime::registry::accepted_flattened_args_for_spec(&spec);
    let recorder_count = accepted
        .iter()
        .filter(|field| field.as_str() == TOOL_CALL_RECORDING_SESSION_ID_FIELD)
        .count();
    assert_eq!(
        recorder_count, 1,
        "accepted_flattened_args must not duplicate recorder metadata: {accepted:?}"
    );
}
