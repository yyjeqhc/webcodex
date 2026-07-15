use super::*;
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn apply_text_edits_input_schema_matches_runtime_edit_objects() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    let changes = &spec.input_schema["properties"]["changes"];

    assert_eq!(changes["type"], "array");
    assert_eq!(changes["items"]["type"], "object");
    assert_eq!(changes["items"]["required"], json!(["kind", "path"]));

    let kind_enum = changes["items"]["properties"]["kind"]["enum"]
        .as_array()
        .expect("apply_text_edits file change kind enum should be listed")
        .iter()
        .map(|value| value.as_str().expect("kind enum value should be a string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        kind_enum,
        BTreeSet::from(["edit", "create", "delete", "rename"])
    );
    let edits = &changes["items"]["properties"]["edits"];
    assert_eq!(edits["items"]["required"], json!(["kind"]));

    let object_args = json!({
        "project": "agent:oe:private-drop",
        "changes": [{
            "kind": "edit",
            "path": "src/lib.rs",
            "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "edits": [{
                    "kind": "insert_after",
                    "anchor_text": "fn main() {}",
                    "new_text": "\n"
            }]
        }]
    });
    ToolCall::from_tool_name("apply_text_edits", object_args)
        .expect("apply_text_edits should deserialize object edit inputs");

    let string_args = json!({
        "project": "agent:oe:private-drop",
        "changes": ["{\"kind\":\"edit\",\"path\":\"src/lib.rs\"}"]
    });
    assert!(
        ToolCall::from_tool_name("apply_text_edits", string_args).is_err(),
        "apply_text_edits should reject stringified edit objects"
    );
}
