use super::*;

#[test]
fn computer_launch_application_output_schema_has_closed_native_platforms() {
    let schema =
        crate::tool_runtime::registry::output_schema_for_tool("computer_launch_application");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let application_id = "application_0123456789abcdef0123456789abcdef";
    for platform in ["windows", "macos"] {
        let output =
            serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
                "platform": platform,
                "application_id": application_id,
                "success": true,
            })))
            .unwrap();
        validate(&output).unwrap_or_else(|error| panic!("{platform}: {error}"));
    }

    let stale = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "stale application",
            json!({
                "error_kind": "stale_application",
                "message": "application identity is stale",
                "application_id": application_id,
                "state_changed": false,
                "execution_state": "not_started",
                "recovery_kind": "reobserve",
                "recovery_tool": "computer_list_applications"
            }),
        ),
    )
    .unwrap();
    validate(&stale).unwrap();
    let mut invalid_recovery = stale.clone();
    invalid_recovery["output"]["recovery_kind"] = json!("blind_retry");
    assert!(validate(&invalid_recovery).is_err());
    let mut invalid_tool_class = stale.clone();
    invalid_tool_class["output"]["recovery_kind"] = json!("fix_input");
    assert!(validate(&invalid_tool_class).is_err());

    let mut recovery_on_success =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "platform": "macos",
            "application_id": application_id,
            "success": true,
        })))
        .unwrap();
    recovery_on_success["output"]["recovery_kind"] = json!("none");
    assert!(validate(&recovery_on_success).is_err());

    let unsupported =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "platform": "linux",
            "application_id": application_id,
            "success": true,
        })))
        .unwrap();
    assert!(validate(&unsupported).is_err());

    let extra = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "platform": "macos",
        "application_id": application_id,
        "success": true,
        "bundle_url": "PRIVATE",
    })))
    .unwrap();
    assert!(validate(&extra).is_err());
}

#[test]
fn read_file_output_schema_matches_real_results_and_strict_tool_payloads() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_file");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let success = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "text": "hello",
        "format": "plain",
        "path": "src/lib.rs",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "start_line": 1,
        "limit": 2,
        "total_lines": 3,
        "returned_lines": 1,
        "end_line": 1,
        "has_more": true,
        "next_start_line": 2
    })))
    .unwrap();
    assert!(
        success.get("error").is_none(),
        "successful ToolResult omits error"
    );
    validate(&success).unwrap();
    assert!(
        success["output"]["returned_lines"].as_u64().unwrap()
            <= success["output"]["limit"].as_u64().unwrap()
    );

    let sparse = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "text": "hello",
        "path": "src/lib.rs",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "total_lines": 1
    })))
    .unwrap();
    validate(&sparse).unwrap();
    let mut sparse_numbered = sparse.clone();
    sparse_numbered["output"]["format"] = json!("numbered");
    validate(&sparse_numbered).unwrap();

    let mut sparse_plain = sparse.clone();
    sparse_plain["output"]["format"] = json!("plain");
    assert!(
        validate(&sparse_plain).is_err(),
        "complete sparse plain output must omit redundant format"
    );
    let default_limit = webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
    let mut sparse_over_default_limit = sparse.clone();
    sparse_over_default_limit["output"]["total_lines"] = json!(default_limit + 1);
    assert!(
        validate(&sparse_over_default_limit).is_err(),
        "complete sparse read_file output cannot claim more lines than the default range can return"
    );

    let read_files_schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let sparse_batch_over_default_limit = json!({
        "success": true,
        "output": {
            "items": [{
                "index": 0,
                "path": "src/lib.rs",
                "success": true,
                "output": {
                    "text": "hello",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "total_lines": default_limit + 1
                },
                "error": null
            }]
        },
        "error": null
    });
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &sparse_batch_over_default_limit,
            &read_files_schema,
        )
        .is_err(),
        "complete sparse read_files item cannot claim more lines than the default range can return"
    );
    let mut sparse_missing_sha = sparse.clone();
    sparse_missing_sha["output"]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    assert!(validate(&sparse_missing_sha).is_err());
    let mut sparse_fake_continuation = sparse.clone();
    sparse_fake_continuation["output"]["has_more"] = json!(true);
    assert!(
        validate(&sparse_fake_continuation).is_err(),
        "sparse complete form must not admit partial-read continuation fields"
    );

    let failure = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "read_file failed: not_found",
            json!({
                "error_kind": "read_file_failed",
                "reason_code": "not_found",
                "path": "missing.rs",
                "state_changed": false
            }),
        ),
    )
    .unwrap();
    validate(&failure).unwrap();

    // Failures produced before the read implementation runs retain the runtime's
    // generic null/object payloads and must still match the advertised envelope.
    let generic_null = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::err(
        "unknown project",
    ))
    .unwrap();
    validate(&generic_null).unwrap();
    let generic_object = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "session guard denied",
            json!({"error_kind": "session_guard_denied", "state_changed": false}),
        ),
    )
    .unwrap();
    validate(&generic_object).unwrap();

    for recorder_only in ["session_recorded", "session_event_id", "session_id"] {
        let mut telemetry = success.clone();
        telemetry["output"][recorder_only] = match recorder_only {
            "session_recorded" => json!(true),
            "session_event_id" => json!("evt_test"),
            "session_id" => json!("wc_sess_test"),
            _ => unreachable!(),
        };
        assert!(
            validate(&telemetry).is_err(),
            "read_file schema admitted recorder-only field {recorder_only}"
        );
    }

    for missing in ["next_start_line", "sha256"] {
        let mut value = success.clone();
        value["output"].as_object_mut().unwrap().remove(missing);
        assert!(validate(&value).is_err(), "missing {missing} was accepted");
    }

    let mut bad_sha = success.clone();
    bad_sha["output"]["sha256"] = json!("ABC");
    assert!(validate(&bad_sha).is_err());

    let mut zero_limit = success.clone();
    zero_limit["output"]["limit"] = json!(0);
    assert!(validate(&zero_limit).is_err());

    let mut impossible_count = success.clone();
    impossible_count["output"]["returned_lines"] = json!(2001);
    assert!(validate(&impossible_count).is_err());

    let mut missing_reason = failure.clone();
    missing_reason["output"]
        .as_object_mut()
        .unwrap()
        .remove("reason_code");
    assert!(validate(&missing_reason).is_err());

    let mut unknown_failure = failure.clone();
    unknown_failure["output"]["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_failure).is_err());

    let mut unknown_output = success.clone();
    unknown_output["output"]["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_output).is_err());

    let mut unknown_top = success;
    unknown_top["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_top).is_err());
}
