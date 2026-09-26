use super::*;
use serde_json::json;

fn schema_accepts(schema: &Value, value: &Value) -> bool {
    test_support::validate_schema_instance(value, schema).is_ok()
}

#[test]
fn edit_project_files_schema_and_parser_require_closed_revision_fenced_changes() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "edit_project_files").input_schema;
    let variants = schema["properties"]["changes"]["items"]["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 4);
    for variant in variants { assert_eq!(variant["additionalProperties"], false); }
    let valid = [
        json!({"kind":"edit","path":"a.rs","expected_read_revision":123,"edits":[{"kind":"replace_exact","old_text":"old","new_text":"new"}]}),
        json!({"kind":"create","path":"new.rs","content":""}),
        json!({"kind":"delete","path":"old.rs","expected_read_revision":123}),
        json!({"kind":"rename","path":"a.rs","to_path":"b.rs","expected_read_revision":123}),
    ];
    for change in &valid {
        let request = json!({"project":"demo","changes":[change]});
        assert!(schema_accepts(schema, &request), "{request}");
        let parsed = ToolCall::from_tool_name("edit_project_files", request).unwrap();
        let serialized = serde_json::to_value(parsed).unwrap();
        assert!(serialized.to_string().contains("edit_project_files"));
        for field in ["content", "to_path", "edits", "expected_sha256", "old_text", "new_text", "unknown"] {
            if change.get(field).is_some() { continue; }
            let mut invalid = change.clone();
            invalid[field] = json!("unexpected");
            let request = json!({"project":"demo","changes":[invalid]});
            assert!(!schema_accepts(schema, &request), "{request}");
            assert!(ToolCall::from_tool_name("edit_project_files", request).is_err());
        }
        if change["kind"] != "create" {
            let mut invalid = change.clone();
            invalid.as_object_mut().unwrap().remove("expected_read_revision");
            let request = json!({"project":"demo","changes":[invalid]});
            assert!(!schema_accepts(schema, &request));
            assert!(ToolCall::from_tool_name("edit_project_files", request).is_err());
        }
    }
    for revision in [0, 9007199254740992_u64] {
        let request = json!({"project":"demo","changes":[{"kind":"delete","path":"a","expected_read_revision":revision}]});
        assert!(!schema_accepts(schema, &request));
        assert!(ToolCall::from_tool_name("edit_project_files", request).is_err());
    }
    assert!(ToolCall::from_tool_name("apply_text_edits", json!({"project":"demo","changes":[valid[1]]})).is_err());
    let shorthand = json!({"project":"demo","changes":[{"path":"a","old_text":"old","new_text":"new"}]});
    assert!(!schema_accepts(schema, &shorthand));
    assert!(ToolCall::from_tool_name("edit_project_files", shorthand).is_err());
}

#[test]
fn apply_text_edits_model_schema_size_is_bounded() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "edit_project_files");
    let input_schema_bytes = serde_json::to_vec(&spec.input_schema).unwrap().len();
    let description_bytes = spec.description.len();
    let total_tool_projection_bytes = serde_json::to_vec(spec).unwrap().len();

    eprintln!(
        "apply_text_edits surface bytes: input_schema={input_schema_bytes} description={description_bytes} total_tool_projection={total_tool_projection_bytes}"
    );
    assert!(input_schema_bytes <= 20_000);
    assert!(description_bytes <= MODEL_TOOL_DESCRIPTION_MAX_CHARS);
    assert!(total_tool_projection_bytes <= 30_000);
}

#[test]
fn write_project_file_schema_uses_read_revision_for_whole_file_replacement() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "write_project_file").input_schema;
    let revision = 3817291045227_u64;

    assert_eq!(
        schema["properties"]["expected_read_revision"]["type"],
        "integer"
    );
    assert_eq!(
        schema["properties"]["expected_read_revision"]["maximum"],
        9007199254740991_u64
    );
    assert!(schema["properties"].get("expected_sha256").is_none());

    for value in [
        json!({"project":"demo","path":"new.rs","content":"fn main() {}"}),
        json!({"project":"demo","path":"existing.rs","content":"fn main() {}","overwrite":true,"expected_read_revision":revision}),
    ] {
        assert!(
            schema_accepts(schema, &value),
            "valid write shape rejected: {value}"
        );
    }
    // overwrite/revision coupling is a semantic preflight rule in ToolRuntime,
    // not a JSON-Schema conditional. Structural schema admits those combinations
    // so Host compatibility does not depend on allOf/if/then support.
    for value in [
        json!({"project":"demo","path":"existing.rs","content":"x","overwrite":true}),
        json!({"project":"demo","path":"existing.rs","content":"x","expected_read_revision":revision}),
        json!({"project":"demo","path":"existing.rs","content":"x","overwrite":false,"expected_read_revision":revision}),
    ] {
        assert!(
            schema_accepts(schema, &value),
            "structural write shape rejected: {value}"
        );
    }
    for value in [
        json!({"project":"demo","path":"existing.rs","content":"x","overwrite":true,"expected_read_revision":0}),
        json!({"project":"demo","path":"existing.rs","content":"x","overwrite":true,"expected_read_revision":9007199254740992_u64}),
        json!({"project":"demo","path":"existing.rs","content":"x","overwrite":true,"expected_sha256":"a".repeat(64)}),
    ] {
        assert!(
            !schema_accepts(schema, &value),
            "invalid structural write shape accepted: {value}"
        );
    }
}

#[test]
fn edit_project_files_guidance_is_read_native() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "edit_project_files");
    for phrase in ["read_files", "ONE change per file", "expected_read_revision", "preflighted transactionally", "outcome_unknown", "show_changes", "structured validation"] {
        assert!(spec.description.contains(phrase), "{phrase}");
    }
}
