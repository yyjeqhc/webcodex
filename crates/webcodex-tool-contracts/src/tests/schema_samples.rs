use super::*;
use crate::test_support::{
    sample_schema_value, sample_tool_args_for_spec, validate_schema_instance,
};

#[test]
fn all_registered_samples_validate_and_deserialize() {
    let mut failures = Vec::new();
    for spec in registered_tool_specs() {
        let sample = std::panic::catch_unwind(|| sample_tool_args_for_spec(&spec));
        match sample {
            Ok(value) => {
                if let Err(error) = validate_schema_instance(&value, &spec.input_schema) {
                    failures.push(format!("{} schema: {error}", spec.name));
                }
                if let Err(error) = ToolCall::from_tool_name(&spec.name, value) {
                    failures.push(format!("{} parser: {error}", spec.name));
                }
            }
            Err(_) => failures.push(format!("{} generation failed", spec.name)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn schema_samples_choose_declared_values_and_required_children() {
    let schema = json!({"type":"object", "required":["action", "nested"], "properties": {
        "action": {"type":"string", "enum":["check", "test"]},
        "nested": {"type":"array", "minItems":1, "items": {
            "type":"object", "required":["label","count"], "properties": {
                "label": {"type":"string", "minLength":3},
                "count": {"type":"integer", "minimum":2}
            }
        }}
    }});
    let value = sample_schema_value(&schema).unwrap();
    assert_eq!(
        value,
        json!({"action":"check", "nested":[{"label":"aaa", "count":2}]})
    );
    assert_eq!(sample_schema_value(&schema).unwrap(), value);
    validate_schema_instance(&value, &schema).unwrap();
    assert_eq!(
        sample_schema_value(
            &json!({"type":"string", "const":"fixed", "default":"fallback", "enum":["fixed"]})
        )
        .unwrap(),
        json!("fixed")
    );
    assert_eq!(
        sample_schema_value(&json!({"type":"string", "default":"test", "enum":["check","test"]}))
            .unwrap(),
        json!("test")
    );
    assert_eq!(
        sample_schema_value(&json!({"type":"integer", "enum":[-1,2], "minimum":0})).unwrap(),
        json!(2)
    );
    assert_eq!(
        sample_schema_value(&json!({"type":"string", "enum":["a".repeat(1025), "safe"]})).unwrap(),
        json!("safe")
    );
    assert_eq!(
        sample_schema_value(&json!({"type":["string", "null"], "minLength":1})).unwrap(),
        json!("a")
    );
    assert_eq!(
        sample_schema_value(&json!({"type":"number", "minimum":1.5})).unwrap(),
        json!(1.5)
    );
    assert_eq!(
        sample_schema_value(&json!({"type":"integer", "default":-1, "enum":[2], "minimum":0}))
            .unwrap(),
        json!(2)
    );
    // A rejected expensive branch must not consume the budget needed by a later
    // valid branch. Candidate exploration is bounded, but failed candidates are
    // not part of the generated sample.
    assert_eq!(
        sample_schema_value(&json!({"anyOf":[
            {"type":"array", "minItems":32, "items":{"type":"array", "minItems":32, "items":{"type":"boolean"}}},
            {"type":"string", "const":"safe"}
        ]}))
        .unwrap(),
        json!("safe")
    );
}

#[test]
fn schema_samples_fail_closed_and_bound_resources() {
    for schema in [
        json!({"$ref":"#/recursive"}),
        json!({"type":"string", "minLength":-1}),
        json!({"type":"object", "const":{"child":"bad"}, "properties":{"child":{"type":"string", "pattern":"^good$"}}}),
        json!({"type":"number", "minimum":1.5, "maximum":1.4}),
        json!({"type":"integer", "anyOf":[{"type":"string", "const":"bad"}]}),
        json!({"type":"string", "pattern":"^opaque$"}),
        json!({"type":"string", "minLength":1025}),
        json!({"type":"array", "minItems":33, "items":{"type":"boolean"}}),
        json!({"type":"string", "minLength":3, "maxLength":2}),
        json!({"type":"integer", "minimum":2, "maximum":1}),
        json!({"type":"string", "enum":[]}),
        json!({"type":"object", "required":["missing"]}),
    ] {
        assert!(sample_schema_value(&schema).is_err(), "{schema}");
    }
    let wide = json!({"type":"array", "minItems":32, "items":{"type":"array", "minItems":32, "items":{"type":"boolean"}}});
    assert!(sample_schema_value(&wide).is_err());
    assert!(validate_schema_instance(
        &json!(9_007_199_254_740_993_u64),
        &json!({"type":"integer", "maximum":9_007_199_254_740_992_u64})
    )
    .is_err());
    let mut schema = json!({"type":"boolean"});
    for _ in 0..18 {
        schema = json!({"type":"array", "minItems":1, "items":schema});
    }
    assert!(sample_schema_value(&schema).is_err());
}
