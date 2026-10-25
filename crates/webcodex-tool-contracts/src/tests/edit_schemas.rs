use super::*;
use serde_json::json;

fn schema_accepts(schema: &Value, value: &Value) -> bool {
    test_support::validate_schema_instance(value, schema).is_ok()
}

#[test]
fn edit_project_files_schema_and_parser_require_closed_revision_fenced_changes() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "edit_project_files").input_schema;
    let variants = schema["properties"]["changes"]["items"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(variants.len(), 4);
    for variant in variants {
        assert_eq!(variant["additionalProperties"], false);
    }
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
        for field in [
            "content",
            "to_path",
            "edits",
            "expected_sha256",
            "old_text",
            "new_text",
            "unknown",
        ] {
            if change.get(field).is_some() {
                continue;
            }
            let mut invalid = change.clone();
            invalid[field] = json!("unexpected");
            let request = json!({"project":"demo","changes":[invalid]});
            assert!(!schema_accepts(schema, &request), "{request}");
            assert!(ToolCall::from_tool_name("edit_project_files", request).is_err());
        }
        if change["kind"] != "create" {
            let mut invalid = change.clone();
            invalid
                .as_object_mut()
                .unwrap()
                .remove("expected_read_revision");
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
    assert!(ToolCall::from_tool_name(
        "apply_text_edits",
        json!({"project":"demo","changes":[valid[1]]})
    )
    .is_err());
    let shorthand =
        json!({"project":"demo","changes":[{"path":"a","old_text":"old","new_text":"new"}]});
    assert!(!schema_accepts(schema, &shorthand));
    assert!(ToolCall::from_tool_name("edit_project_files", shorthand).is_err());
}

#[test]
fn edit_project_files_range_edit_schema_is_closed_and_revision_fenced() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "edit_project_files").input_schema;
    let valid = json!({
        "project": "demo",
        "changes": [{
            "kind": "edit",
            "path": "a.rs",
            "expected_read_revision": 123,
            "edits": [{
                "kind": "replace_range",
                "start_line": 2,
                "end_line": 4,
                "new_text": "replacement\n"
            }]
        }]
    });
    assert!(schema_accepts(schema, &valid), "{valid}");
    assert!(ToolCall::from_tool_name("edit_project_files", valid.clone()).is_ok());

    for (field, value) in [
        ("old_text", json!("old")),
        ("anchor_text", json!("anchor")),
        ("occurrence", json!(1)),
        ("expected_match_count", json!(1)),
        ("line_scope", json!({"start_line": 2, "end_line": 4})),
        ("unknown", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid["changes"][0]["edits"][0][field] = value;
        assert!(!schema_accepts(schema, &invalid), "{invalid}");
        assert!(ToolCall::from_tool_name("edit_project_files", invalid).is_err());
    }

    let mut zero_start = valid.clone();
    zero_start["changes"][0]["edits"][0]["start_line"] = json!(0);
    assert!(!schema_accepts(schema, &zero_start), "{zero_start}");
    assert!(ToolCall::from_tool_name("edit_project_files", zero_start).is_err());

    // JSON Schema can express positive endpoints but not the cross-field
    // start_line <= end_line invariant. The parser enforces it before runtime.
    let mut reversed = valid.clone();
    reversed["changes"][0]["edits"][0]["start_line"] = json!(3);
    reversed["changes"][0]["edits"][0]["end_line"] = json!(2);
    assert!(schema_accepts(schema, &reversed), "{reversed}");
    assert!(ToolCall::from_tool_name("edit_project_files", reversed).is_err());

    let mut missing_revision = valid;
    missing_revision["changes"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected_read_revision");
    assert!(!schema_accepts(schema, &missing_revision));
    assert!(ToolCall::from_tool_name("edit_project_files", missing_revision).is_err());
}

#[test]
fn edit_project_files_replace_selectors_match_schema_and_parser() {
    let schema = input_schema_for_tool("edit_project_files");
    // Published Host inputs already require omission rather than explicit null.
    // Serde's existing Option compatibility still treats null as absence.
    let choices = [None, Some(Value::Null), Some(json!(1))];
    for occurrence in &choices {
        for count in &choices {
            let mut edit = json!({"kind":"replace_exact","old_text":"old","new_text":"new"});
            for (key, value) in [("occurrence", occurrence), ("expected_match_count", count)] {
                if let Some(value) = value {
                    edit[key] = value.clone();
                }
            }
            let request = json!({"project":"demo","changes":[{
                "kind":"edit","path":"a.rs","expected_read_revision":123,"edits":[edit]
            }]});
            let conflict = occurrence.as_ref().is_some_and(Value::is_number)
                && count.as_ref().is_some_and(Value::is_number);
            let has_null = occurrence.as_ref().is_some_and(Value::is_null)
                || count.as_ref().is_some_and(Value::is_null);
            assert_eq!(
                schema_accepts(&schema, &request),
                !conflict && !has_null,
                "{request}"
            );
            assert_eq!(
                ToolCall::from_tool_name("edit_project_files", request.clone()).is_ok(),
                !conflict,
                "{request}"
            );
        }
    }

    let mut request = json!({"project":"demo","changes":[{
        "kind":"edit","path":"a.rs","expected_read_revision":123,"edits":[
            {"kind":"insert_before","anchor_text":"anchor","new_text":"prefix","occurrence":1},
            {"kind":"replace_exact","old_text":"old","new_text":"new","expected_match_count":2}
        ]
    }]});
    assert!(schema_accepts(&schema, &request));
    assert!(ToolCall::from_tool_name("edit_project_files", request.clone()).is_ok());
    request["changes"][0]["edits"][1]["occurrence"] = json!(1);
    assert!(
        !schema_accepts(&schema, &request),
        "second edit conflict: {request}"
    );
    assert!(ToolCall::from_tool_name("edit_project_files", request.clone()).is_err());
    request["changes"][0]["edits"][1]
        .as_object_mut()
        .unwrap()
        .remove("occurrence");
    request["changes"][0]["edits"][1]["unknown"] = json!(true);
    assert!(!schema_accepts(&schema, &request));
    assert!(ToolCall::from_tool_name("edit_project_files", request).is_err());
}

#[test]
fn apply_text_edits_model_schema_size_is_bounded() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "edit_project_files");
    let input_schema_bytes = serde_json::to_vec(&spec.input_schema).unwrap().len();
    let description_bytes = spec.description.len();
    let total_tool_projection_bytes = serde_json::to_vec(spec).unwrap().len();
    // Size assertions are entirely in memory. Do not require a shared Unix
    // /tmp directory or emit global diagnostic files from an ordinary test.

    eprintln!(
        "apply_text_edits surface bytes: input_schema={input_schema_bytes} description={description_bytes} total_tool_projection={total_tool_projection_bytes}"
    );
    assert!(input_schema_bytes <= 20_000);
    assert!(description_bytes <= MODEL_TOOL_DESCRIPTION_MAX_CHARS);
    assert!(total_tool_projection_bytes <= 30_000);
}

#[test]
fn write_project_file_schema_uses_read_revision_for_whole_file_replacement() {
    let specs = exact_manifest_specialist_tool_specs();
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
    for phrase in [
        "read_files",
        "One change per file",
        "expected_read_revision",
        "preflight transactionally",
        "outcome_unknown",
        "review_changes when Git review is useful",
        "task-appropriate validation",
    ] {
        assert!(spec.description.contains(phrase), "{phrase}");
    }
}
