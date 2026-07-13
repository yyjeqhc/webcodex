use super::*;
use std::collections::{BTreeMap, BTreeSet};

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

#[test]
fn critical_call_runtime_tool_flattened_args_remain_accepted() {
    let mut accepted_fields = BTreeSet::new();
    for spec in registered_tool_specs() {
        accepted_fields
            .extend(crate::tool_runtime::registry::accepted_flattened_args_for_spec(&spec));
    }

    for field in [
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
        "summary_only",
        "include_command_preview",
        "compact_startup",
        "compact",
    ] {
        assert!(
            accepted_fields.contains(field),
            "critical callRuntimeTool flattened arg {field} must remain accepted"
        );
    }
    assert!(!accepted_fields.contains("test_expect_failure_kind"));
}

#[test]
fn accepted_flattened_args_cover_each_tool_spec_input_property() {
    let openapi = crate::openapi::build_openapi_spec();
    let tool_call_properties = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .expect("ToolCallRequest properties");

    for spec in registered_tool_specs() {
        let input_properties = spec.input_schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} input schema properties", spec.name));
        let accepted = crate::tool_runtime::registry::accepted_flattened_args_for_spec(&spec)
            .into_iter()
            .collect::<BTreeSet<_>>();

        for field in input_properties.keys() {
            assert!(
                accepted.contains(field),
                "{} input_schema.properties.{field} must be accepted as a flattened GPT Action arg",
                spec.name
            );
            assert!(
                tool_call_properties.contains_key(field),
                "{} input_schema.properties.{field} must appear in ToolCallRequest.properties",
                spec.name
            );
        }
    }
}

#[test]
fn tool_call_request_flattened_properties_have_explicit_sources() {
    let openapi = crate::openapi::build_openapi_spec();
    let tool_call_properties = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .expect("ToolCallRequest properties");
    let accepted_fields = accepted_flattened_action_fields();
    let spec_input_fields = tool_spec_input_property_fields();
    let explicit_extras = explicit_tool_call_request_extra_fields();

    for field in &accepted_fields {
        assert!(
            tool_call_properties.contains_key(field),
            "ToolCallRequest.properties must expose accepted flattened arg {field}"
        );
    }

    for field in tool_call_properties.keys() {
        if spec_input_fields.contains(field) {
            continue;
        }
        assert!(
            explicit_extras.contains_key(field.as_str()),
            "ToolCallRequest.properties.{field} must come from a ToolSpec input field or the explicit generic-wrapper extra allowlist"
        );
    }

    for (field, reason) in explicit_extras {
        assert!(
            tool_call_properties.contains_key(field),
            "ToolCallRequest.properties must keep explicit extra field {field}: {reason}"
        );
    }
}

#[test]
fn openapi_generic_call_runtime_tool_schema_remains_strict_model_visible_surface() {
    use crate::tool_runtime::tool_definition::{model_visible_tool_definitions, tool_definitions};

    let openapi = crate::openapi::build_openapi_spec();
    let operation_count: usize = openapi["paths"]
        .as_object()
        .expect("OpenAPI paths")
        .values()
        .map(|methods| methods.as_object().expect("path methods").len())
        .sum();
    assert_eq!(operation_count, 25, "OpenAPI operation count");

    assert_eq!(
        registered_tool_specs().len(),
        tool_definitions().count(),
        "model-visible specs must match ToolDefinition count"
    );

    let tool_call = &openapi["components"]["schemas"]["ToolCallRequest"];
    assert_eq!(tool_call["type"], "object");
    assert_eq!(
        tool_call["additionalProperties"], false,
        "ToolCallRequest must not loosen additionalProperties"
    );
    assert_eq!(
        tool_call["required"],
        json!([TOOL_CALL_TOOL_FIELD]),
        "callRuntimeTool must require only the generic tool selector"
    );

    let properties = tool_call["properties"]
        .as_object()
        .expect("ToolCallRequest properties");
    assert!(
        properties.contains_key(TOOL_CALL_TOOL_FIELD),
        "ToolCallRequest.properties must contain tool"
    );
    assert!(
        properties.contains_key(TOOL_CALL_PARAMS_FIELD),
        "ToolCallRequest.properties must keep params for non-Action clients"
    );
    assert!(
        properties.contains_key(TOOL_CALL_ARGUMENTS_FIELD),
        "ToolCallRequest.properties must keep arguments alias"
    );

    let tool_property = &properties[TOOL_CALL_TOOL_FIELD];
    let tool_description = tool_property["description"]
        .as_str()
        .expect("ToolCallRequest.tool description");
    let visible_names = model_visible_tool_definitions()
        .map(|definition| definition.name.to_string())
        .collect::<BTreeSet<_>>();
    for name in &visible_names {
        assert!(
            tool_description.contains(name),
            "ToolCallRequest.tool description must list model-visible tool {name}"
        );
    }
    for forbidden in ["run_codex", "delete_files"] {
        assert!(
            !tool_description.contains(forbidden),
            "ToolCallRequest.tool description must not advertise hidden/runtime-only name {forbidden}"
        );
    }
    if let Some(values) = tool_property.get("enum").and_then(Value::as_array) {
        let enum_names = values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .unwrap_or_else(|| panic!("ToolCallRequest.tool enum values must be strings"))
                    .to_string()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            enum_names, visible_names,
            "ToolCallRequest.tool enum, if added, must match model-visible ToolDefinition names exactly"
        );
    }
}

fn accepted_flattened_action_fields() -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    for spec in registered_tool_specs() {
        fields.extend(crate::tool_runtime::registry::accepted_flattened_args_for_spec(&spec));
    }
    fields
}

fn tool_spec_input_property_fields() -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    for spec in registered_tool_specs() {
        let properties = spec.input_schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} input schema properties", spec.name));
        fields.extend(properties.keys().cloned());
    }
    fields
}

fn explicit_tool_call_request_extra_fields() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (TOOL_CALL_TOOL_FIELD, "generic runtime tool selector"),
        (
            TOOL_CALL_PARAMS_FIELD,
            "non-Action object argument envelope with arbitrary tool-specific keys",
        ),
        (
            TOOL_CALL_ARGUMENTS_FIELD,
            "params compatibility alias for non-Action clients",
        ),
        (
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
            "generic wrapper recorder metadata stripped before concrete tool dispatch",
        ),
    ])
}
