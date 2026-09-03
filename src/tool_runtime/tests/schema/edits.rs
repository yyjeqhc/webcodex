use super::*;

#[test]
fn apply_text_edits_tool_call_parser_accepts_object_edits_and_rejects_stringified_edits() {
    let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let object_args = json!({
        "project": "agent:oe:private-drop",
        "changes": [{
            "kind": "edit",
            "path": "src/lib.rs",
            "expected_sha256": hash,
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
