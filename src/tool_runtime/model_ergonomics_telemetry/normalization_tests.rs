use super::*;
use serde_json::json;

#[test]
fn model_ergonomics_normalization_is_success_only_closed_and_payload_free() {
    for code in ToolInputNormalizationCode::all() {
        let completion = ModelErgonomicsTimer::start_with_arguments(
            "run_process",
            &json!({
                "executable":"PRIVATE_COMMAND", "argv":["PRIVATE_ARG"], "cwd":"PRIVATE_PATH"
            }),
        )
        .unwrap()
        .finish();
        let output = json!({
            "input_normalization":{"code":code.as_str(), "hint":"PRIVATE_HINT"},
            "stdout":"PRIVATE_OUTPUT", "command":"PRIVATE_COMMAND"
        });
        let result = ToolResult::ok(output.clone());
        let api = completion.record_for_tool_result(&result).unwrap();
        let mcp = completion
            .record_for_structured_content(&serde_json::to_value(&result).unwrap())
            .unwrap();
        assert_eq!(api, mcp);
        assert_eq!(api.schema_version, 13);
        assert_eq!(api.input_normalization_code, Some(*code));
        let serialized = serde_json::to_string(&api).unwrap();
        assert!(!serialized.contains("PRIVATE_"));
        assert!(!serialized.contains("hint"));
        assert_eq!(
            serde_json::to_value(&api).unwrap()["input_normalization_code"],
            code.as_str()
        );
        let failed = completion
            .record_for_tool_result(&ToolResult::err_with_output("PRIVATE_PARSER_ERROR", output))
            .unwrap();
        assert!(failed.input_normalization_code.is_none());
        assert!(!serde_json::to_string(&failed).unwrap().contains("PRIVATE_"));
    }
}

#[test]
fn model_ergonomics_normalization_omits_missing_unknown_and_malformed_codes() {
    let completion = ModelErgonomicsTimer::start("run_process").unwrap().finish();
    for output in [
        json!({}),
        json!({"input_normalization":null}),
        json!({"input_normalization":{"code":"arbitrary_PRIVATE_CODE", "hint":"PRIVATE_HINT"}}),
        json!({"input_normalization":{"code":" argv_to_args"}}),
        json!({"input_normalization":{"code":["argv_to_args"]}}),
        json!({"input_normalization":{"code":13}}),
    ] {
        let record = completion
            .record_for_tool_result(&ToolResult::ok(output))
            .unwrap();
        assert!(record.input_normalization_code.is_none());
        let value = serde_json::to_value(record).unwrap();
        assert!(value.get("input_normalization_code").is_none());
        assert!(!value.to_string().contains("PRIVATE_"));
    }
    let pre_result = completion.record_for_pre_result_failure("invalid_arguments");
    assert_eq!(pre_result.error_kind.as_deref(), Some("invalid_arguments"));
    assert!(pre_result.input_normalization_code.is_none());
    assert!(pre_result.serialized_result_bytes.is_none());
}

#[test]
fn model_ergonomics_normalization_survives_late_projection_but_not_failure() {
    let mut timer = ModelErgonomicsTimer::start("run_process").unwrap();
    timer.capture_canonical_result(&ToolResult::ok(json!({
        "input_normalization":{"code":"argv_to_args", "hint":"PRIVATE_HINT"}
    })));
    let completion = timer.finish();
    let compact = ToolResult::ok(json!({"execution_state":"pending"}));
    let api = completion.record_for_tool_result(&compact).unwrap();
    let mcp = completion
        .record_for_structured_content(&serde_json::to_value(&compact).unwrap())
        .unwrap();
    assert_eq!(api, mcp);
    assert_eq!(
        api.input_normalization_code,
        Some(ToolInputNormalizationCode::ArgvToArgs)
    );
    assert!(completion
        .record_for_tool_result(&ToolResult::err("PRIVATE_FAILURE"))
        .unwrap()
        .input_normalization_code
        .is_none());
    assert!(completion
        .record_for_pre_result_failure("invalid_arguments")
        .input_normalization_code
        .is_none());
}
