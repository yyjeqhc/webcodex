use super::*;
use serde_json::json;

fn apply_text_edits_schema_accepts(schema: &Value, value: &Value) -> bool {
    test_support::validate_schema_instance(value, schema).is_ok()
}

fn schema_accepts(schema: &Value, value: &Value) -> bool {
    test_support::validate_schema_instance(value, schema).is_ok()
}

#[test]
fn apply_text_edits_input_schema_encodes_structural_wire_contracts() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    let schema = &spec.input_schema;
    let changes = &schema["properties"]["changes"];

    assert_eq!(changes["type"], "array");
    assert_eq!(changes["minItems"], 1);
    assert_eq!(changes["maxItems"], 16);
    let wire_variants = changes["items"]["anyOf"]
        .as_array()
        .expect("apply_text_edits change wire union");
    assert_eq!(wire_variants.len(), 2);

    let canonical = wire_variants
        .iter()
        .find(|variant| variant["properties"].get("kind").is_some())
        .expect("canonical apply-file-change wire variant");
    let shorthand = wire_variants
        .iter()
        .find(|variant| variant["properties"].get("old_text").is_some())
        .expect("exact-replace shorthand wire variant");

    assert_eq!(canonical["additionalProperties"], false);
    assert_eq!(canonical["required"], json!(["kind", "path"]));
    assert_eq!(
        canonical["properties"]["kind"]["enum"],
        json!(["edit", "create", "delete", "rename"])
    );
    assert_eq!(canonical["properties"]["path"]["minLength"], 1);
    assert_eq!(canonical["properties"]["to_path"]["minLength"], 1);
    assert_eq!(canonical["properties"]["edits"]["maxItems"], 20);
    assert_eq!(
        canonical["properties"]["expected_read_revision"]["maximum"],
        9007199254740991_u64
    );

    let edit = &canonical["properties"]["edits"]["items"];
    assert_eq!(edit["type"], "object");
    assert_eq!(edit["additionalProperties"], false);
    assert_eq!(edit["required"], json!(["kind"]));
    assert_eq!(
        edit["properties"]["kind"]["enum"],
        json!([
            "replace_exact",
            "insert_after",
            "insert_before",
            "delete_exact"
        ])
    );
    assert_eq!(edit["properties"]["old_text"]["minLength"], 1);
    assert_eq!(edit["properties"]["old_text"]["maxLength"], 512 * 1024);
    assert_eq!(edit["properties"]["anchor_text"]["minLength"], 1);
    assert_eq!(edit["properties"]["anchor_text"]["maxLength"], 512 * 1024);
    assert_eq!(edit["properties"]["new_text"]["maxLength"], 512 * 1024);
    assert_eq!(edit["properties"]["occurrence"]["minimum"], 1);
    assert_eq!(edit["properties"]["expected_match_count"]["minimum"], 1);
    assert_eq!(edit["properties"]["expected_match_count"]["maximum"], 1024);
    let line_scope = &edit["properties"]["line_scope"];
    assert_eq!(line_scope["additionalProperties"], false);
    assert_eq!(line_scope["required"], json!(["start_line", "end_line"]));
    assert_eq!(line_scope["properties"]["start_line"]["minimum"], 1);
    assert_eq!(line_scope["properties"]["end_line"]["minimum"], 1);

    assert_eq!(shorthand["additionalProperties"], false);
    assert_eq!(
        shorthand["required"],
        json!(["path", "old_text", "new_text"])
    );
    assert_eq!(shorthand["properties"]["path"]["minLength"], 1);
    assert_eq!(shorthand["properties"]["old_text"]["minLength"], 1);
    assert!(shorthand["properties"].get("kind").is_none());
    assert!(shorthand["properties"].get("occurrence").is_none());
    assert!(shorthand["properties"].get("line_scope").is_none());

    // Kind-dependent field combinations remain semantic Runtime validation rather
    // than Host-sensitive JSON-Schema conditionals. Structural schema accepts a
    // parseable canonical DTO even when later preflight must reject its semantics.
    for structurally_valid in [
        json!({"project":"demo","changes":[{"kind":"delete","path":"old.txt"}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact"}]}]}),
    ] {
        assert!(
            apply_text_edits_schema_accepts(schema, &structurally_valid),
            "structurally valid canonical DTO rejected: {structurally_valid}"
        );
    }

    let revision = 3817291045227_u64;
    let shorthand_request = json!({
        "project":"demo",
        "changes":[{"path":"a.rs","old_text":"old","new_text":"new","expected_read_revision":revision}]
    });
    assert!(apply_text_edits_schema_accepts(schema, &shorthand_request));
    let parsed = ToolCall::from_tool_name("apply_text_edits", shorthand_request)
        .expect("shorthand accepted by canonical typed parser");
    assert!(matches!(
        parsed,
        ToolCall::ApplyTextEdits { changes, .. }
            if changes.len() == 1
                && changes[0].kind == webcodex_core::apply_edits_shared::ApplyFileChangeKind::Edit
                && changes[0].edits.len() == 1
                && changes[0].edits[0].kind == webcodex_core::apply_edits_shared::ApplyTextEditKind::ReplaceExact
                && changes[0].expected_read_revision == Some(revision)
    ));

    for invalid in [
        json!({"project":"demo","changes":[]}),
        json!({"project":"demo","changes":[{"path":"a.rs","old_text":"old"}]}),
        json!({"project":"demo","changes":[{"path":"a.rs","old_text":"","new_text":"new"}]}),
        json!({"project":"demo","changes":[{"path":"a.rs","old_text":"old","new_text":"new","unknown":true}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","expected_read_revision":0}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","expected_read_revision":9007199254740992_u64}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","occurrence":0}]}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","expected_match_count":0}]}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","expected_match_count":"many"}]}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","expected_match_count":1025}]}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","line_scope":{"start_line":0,"end_line":2}}]}]}),
        json!({"project":"demo","changes":[{"kind":"edit","path":"a.rs","edits":[{"kind":"replace_exact","old_text":"old","unknown":true}]}]}),
    ] {
        assert!(
            !apply_text_edits_schema_accepts(schema, &invalid),
            "invalid structural apply_text_edits shape accepted: {invalid}"
        );
    }
}

#[test]
fn apply_text_edits_model_schema_size_is_bounded() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
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
fn apply_text_edits_composition_guidance_and_union_stay_unambiguous() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    for phrase in [
        "transactional structured option",
        "use ONE change per file",
        "expected_match_count=N",
        "explicit bounded file",
        "exact cardinality is known",
        "optional dry_run",
        "dry_run is not ritual",
        "change_summary",
        "mechanical scope",
        "not semantic review",
        "show_changes",
        "git_diff_hunks",
        "git_review_summary",
    ] {
        assert!(
            spec.description.contains(phrase),
            "missing guidance: {phrase}"
        );
    }
    let changes = &spec.input_schema["properties"]["changes"];
    assert!(changes["description"].as_str().unwrap().contains("ONE"));
    let variants = changes["items"]["anyOf"].as_array().unwrap();
    assert!(variants.iter().all(|variant| {
        let description = variant["description"].as_str().unwrap();
        description.contains("occurrence") && description.contains("line_scope")
    }));
    for change in [
        json!({"path":"a.rs","old_text":"old","new_text":"new","occurrence":2}),
        json!({"path":"a.rs","old_text":"old","new_text":"new","line_scope":{"start_line":1,"end_line":2}}),
        json!({"path":"a.rs","old_text":"old","new_text":"new","edits":[]}),
        json!({"kind":"edit","path":"a.rs","occurrence":2,"edits":[]}),
        json!({"kind":"edit","path":"a.rs","line_scope":{"start_line":1,"end_line":2},"edits":[]}),
    ] {
        let request = json!({"project":"demo","changes":[change]});
        assert!(!schema_accepts(&spec.input_schema, &request), "{request}");
        assert!(ToolCall::from_tool_name("apply_text_edits", request).is_err());
    }
    let request = json!({"project":"demo","changes":[{
        "kind":"edit","path":"a.rs","expected_read_revision":123,
        "edits":[
            {"kind":"replace_exact","old_text":"old","new_text":"new","occurrence":2},
            {"kind":"delete_exact","old_text":"other","line_scope":{"start_line":10,"end_line":20}}
        ]
    }]});
    assert!(schema_accepts(&spec.input_schema, &request));
    assert!(ToolCall::from_tool_name("apply_text_edits", request).is_ok());
}
