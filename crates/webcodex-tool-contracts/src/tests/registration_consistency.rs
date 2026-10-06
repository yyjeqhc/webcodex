//! Exhaustive identity checks across independently maintained contract surfaces.
//! Included under request_schema to inspect the actual derived schema cache,
//! including model-hidden variants, without adding a production registry API.
use super::tool_input_schemas;
use crate::test_support::{sample_tool_args_for_spec, validate_schema_instance};
use crate::{known_tool_names, registered_tool_specs, tool_definitions, ToolCall, ToolSpec};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[test]
fn canonical_registration_sets_are_exact_including_hidden_variants() {
    let schemas = tool_input_schemas();
    let schema_names = schemas.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let definitions = tool_definitions().collect::<Vec<_>>();
    let definition_names = definitions.iter().map(|d| d.name).collect::<BTreeSet<_>>();
    assert_eq!(
        definitions.len(),
        definition_names.len(),
        "duplicate definition"
    );
    assert_eq!(
        schema_names, definition_names,
        "ToolCall/schema and ToolDefinition drift"
    );
    assert_eq!(schema_names, known_tool_names().collect::<BTreeSet<_>>());

    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|s| s.name.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(specs.len(), spec_names.len(), "duplicate model ToolSpec");
    let visible = definitions
        .iter()
        .filter(|d| d.visibility.is_model_visible())
        .map(|d| d.name)
        .collect::<BTreeSet<_>>();
    let hidden = definitions
        .iter()
        .filter(|d| d.visibility.is_model_hidden())
        .map(|d| d.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(spec_names, visible, "visibility must be Definition-owned");
    assert!(
        spec_names.is_disjoint(&hidden),
        "hidden variants became public"
    );
    for spec in &specs {
        assert_eq!(
            spec.input_schema, schemas[&spec.name],
            "{} schema drift",
            spec.name
        );
    }
}

#[test]
fn optional_read_fence_round_trips_without_admitting_explicit_null() {
    use crate::ReadFilesItem;
    let unfenced: ReadFilesItem = serde_json::from_value(json!({"path":"src/lib.rs"})).unwrap();
    let encoded = serde_json::to_value(&unfenced).unwrap();
    assert!(encoded.get("expected_read_revision").is_none());
    assert!(serde_json::from_value::<ReadFilesItem>(encoded)
        .unwrap()
        .expected_read_revision
        .is_none());
    for revision in [1u64, 9_007_199_254_740_991] {
        let fenced: ReadFilesItem =
            serde_json::from_value(json!({"path":"src/lib.rs","expected_read_revision":revision}))
                .unwrap();
        let encoded = serde_json::to_value(fenced).unwrap();
        assert_eq!(encoded["expected_read_revision"], revision);
        assert_eq!(
            serde_json::from_value::<ReadFilesItem>(encoded)
                .unwrap()
                .expected_read_revision,
            Some(revision)
        );
    }
    for revision in [
        Value::Null,
        json!(0),
        json!(-1),
        json!(9_007_199_254_740_992u64),
        json!("1"),
    ] {
        assert!(serde_json::from_value::<ReadFilesItem>(
            json!({"path":"src/lib.rs","expected_read_revision":revision})
        )
        .is_err());
    }
}

#[test]
fn every_canonical_variant_name_round_trips_without_aliasing() {
    let mut failures = Vec::new();
    for (name, schema) in tool_input_schemas() {
        // Hidden tools have no model spec. This local carrier only reuses the
        // existing schema sample generator; it never registers or exposes them.
        let fixture = ToolSpec {
            name: name.clone(),
            description: String::new(),
            input_schema: schema.clone(),
            output_schema: Value::Null,
            annotations: Value::Null,
        };
        let sample = std::panic::catch_unwind(|| sample_tool_args_for_spec(&fixture));
        let args = match sample {
            Ok(args) => args,
            Err(_) => {
                failures.push(format!("{name}: no schema fixture"));
                continue;
            }
        };
        if let Err(error) = validate_schema_instance(&args, schema) {
            failures.push(format!("{name}: fixture schema: {error}"));
        }
        match ToolCall::from_tool_name(name, args) {
            Ok(call) => {
                if call.tool_name() != name {
                    failures.push(format!("{name}: tool_name returned {}", call.tool_name()));
                }
                let wire = serde_json::to_value(&call).expect("serialize canonical ToolCall");
                if wire["tool"] != json!(name) {
                    failures.push(format!("{name}: serialized as {}", wire["tool"]));
                }
                match serde_json::from_value::<ToolCall>(wire) {
                    Ok(decoded) if decoded.tool_name() == name => {}
                    other => failures.push(format!("{name}: canonical round trip: {other:?}")),
                }
            }
            Err(error) => failures.push(format!("{name}: typed parser: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
