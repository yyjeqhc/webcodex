use crate::ToolCall;
use serde_json::{json, Value};

fn inputs(revision: Value) -> Vec<(&'static str, Value)> {
    vec![
        (
            "read_files",
            json!({"project":"agent:r:p","items":[{"path":"x","expected_read_revision":revision}]}),
        ),
        (
            "edit_project_files",
            json!({"project":"agent:r:p","changes":[{"kind":"edit","path":"x","expected_read_revision":revision,"edits":[{"kind":"replace_exact","old_text":"old","new_text":"new"}]}]}),
        ),
        (
            "edit_project_files",
            json!({"project":"agent:r:p","changes":[{"kind":"delete","path":"x","expected_read_revision":revision}]}),
        ),
        (
            "edit_project_files",
            json!({"project":"agent:r:p","changes":[{"kind":"rename","path":"x","to_path":"y","expected_read_revision":revision}]}),
        ),
    ]
}
fn parse(tool: &str, params: Value) -> Result<ToolCall, String> {
    ToolCall::from_tool_name(tool, params)
}

#[test]
fn read_revision_equivalent_json_numbers_have_one_canonical_integer() {
    for n in [1_u64, 123, 8_535_043_794_784_493, 9_007_199_254_740_991] {
        for (encoded, canonical) in
            inputs(serde_json::from_str::<Value>(&format!("{n}.0")).unwrap())
                .into_iter()
                .zip(inputs(json!(n)))
        {
            let actual = serde_json::to_value(parse(encoded.0, encoded.1).unwrap()).unwrap();
            let expected = serde_json::to_value(parse(canonical.0, canonical.1).unwrap()).unwrap();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn read_revision_does_not_coerce_ambiguous_or_out_of_range_fences() {
    for value in [
        json!(0),
        json!(-1),
        json!(0.0),
        json!(1.5),
        json!(9007199254740992_u64),
        json!(9007199254740992.0),
        json!(u64::MAX),
        json!("8535043794784493"),
        Value::Null,
        json!(true),
        json!({}),
    ] {
        for (tool, params) in inputs(value.clone()) {
            assert!(parse(tool, params).is_err(), "accepted {tool}: {value}");
        }
    }
    assert!(parse(
        "read_files",
        json!({"project":"agent:r:p","items":[{"path":"x"}]})
    )
    .is_ok());
    assert!(parse(
        "edit_project_files",
        json!({"project":"agent:r:p","changes":[{"kind":"delete","path":"x"}]})
    )
    .is_err());
}
