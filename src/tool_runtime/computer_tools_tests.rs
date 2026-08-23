use super::*;

fn surface(id: &str) -> Value {
    json!({
        "surface_id": id,
        "application": "Example",
        "title": "Window",
        "width": 1280,
        "height": 720,
        "focused": false,
        "active": false
    })
}

fn accessibility_tree() -> Value {
    json!({
        "platform": "macos",
        "surface_id": "surface_test",
        "nodes": [
            {
                "element_id": "element_root",
                "parent_element_id": null,
                "depth": 0,
                "role": "AXWindow",
                "subrole": null,
                "title": "Example",
                "description": null,
                "value": null,
                "placeholder": null,
                "enabled": true,
                "focused": false,
                "child_count": 1
            },
            {
                "element_id": "element_child",
                "parent_element_id": "element_root",
                "depth": 1,
                "role": "AXButton",
                "subrole": null,
                "title": "OK",
                "description": null,
                "value": null,
                "placeholder": null,
                "enabled": true,
                "focused": false,
                "child_count": 0
            }
        ],
        "node_count": 2,
        "truncated": false,
        "max_depth": 2,
        "max_nodes": 8,
        "observation_generation": 7
    })
}

fn application(id: &str, name: &str) -> Value {
    json!({"application_id": id, "display_name": name})
}

const APPLICATION_ID: &str = "application_0123456789abcdef0123456789abcdef";
const APPLICATION_ID_2: &str = "application_fedcba9876543210fedcba9876543210";

#[test]
fn computer_application_id_and_public_argument_shape_are_closed() {
    assert!(valid_application_id(APPLICATION_ID));
    for invalid in [
        "",
        "application_",
        "application_0123456789ABCDEF0123456789ABCDEF",
        "application_0123456789abcdef0123456789abcdeg",
        "surface_0123456789abcdef0123456789abcdef",
    ] {
        assert!(!valid_application_id(invalid), "{invalid}");
    }

    let list = ToolCall::from_tool_name(
        "computer_list_applications",
        json!({"client_id": "msi", "limit": 4}),
    )
    .unwrap();
    assert!(matches!(list, ToolCall::ComputerListApplications { .. }));
    let launch = ToolCall::from_tool_name(
        "computer_launch_application",
        json!({"client_id": "msi", "application_id": APPLICATION_ID}),
    )
    .unwrap();
    assert!(matches!(launch, ToolCall::ComputerLaunchApplication { .. }));
    for forbidden in [
        "path",
        "argv",
        "cwd",
        "environment",
        "command",
        "script",
        "url",
    ] {
        let mut args = json!({"client_id": "msi", "application_id": APPLICATION_ID});
        args.as_object_mut()
            .unwrap()
            .insert(forbidden.to_string(), json!("forbidden"));
        let error = ToolCall::from_tool_name("computer_launch_application", args).unwrap_err();
        assert!(error.contains("unknown field"), "{error}");
    }
}

#[test]
fn computer_application_list_validator_is_bounded_exact_and_private() {
    let valid = validate_application_list(
        json!({
            "applications": [application(APPLICATION_ID, "Editor")],
            "count": 1,
            "truncated": true
        }),
        1,
    );
    assert!(valid.success, "{:?}", valid.output);

    let too_many = validate_application_list(
        json!({
            "applications": [
                application(APPLICATION_ID, "One"),
                application(APPLICATION_ID_2, "Two")
            ],
            "count": 2,
            "truncated": false
        }),
        1,
    );
    assert!(!too_many.success);

    let duplicate = validate_application_list(
        json!({
            "applications": [
                application(APPLICATION_ID, "One"),
                application(APPLICATION_ID, "Two")
            ],
            "count": 2,
            "truncated": false
        }),
        2,
    );
    assert!(!duplicate.success);

    for leak in [
        json!({
            "applications": [{
                "application_id": APPLICATION_ID,
                "display_name": "Editor",
                "path": "C:\\\\secret.exe"
            }],
            "count": 1,
            "truncated": false
        }),
        json!({
            "applications": [{
                "application_id": APPLICATION_ID,
                "display_name": "Editor",
                "native_identity": "AUMID-or-PIDL"
            }],
            "count": 1,
            "truncated": false
        }),
    ] {
        let result = validate_application_list(leak, 1);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_runner_response");
    }
}

#[test]
fn computer_application_launch_lifecycle_is_exact_and_never_blindly_retryable() {
    assert!(computer_request_is_effect("computer_launch_application"));
    assert!(!computer_request_is_effect("computer_list_applications"));

    for platform in ["windows", "macos"] {
        let valid = validate_computer_launch_application(
            json!({"platform": platform, "application_id": APPLICATION_ID, "success": true}),
            APPLICATION_ID,
        );
        assert!(valid.success, "{platform}: {:?}", valid.output);
    }

    let unsupported = validate_computer_launch_application(
        json!({"platform": "linux", "application_id": APPLICATION_ID, "success": true}),
        APPLICATION_ID,
    );
    assert!(!unsupported.success);

    let invalid = validate_computer_launch_application(
        json!({
            "platform": "macos",
            "application_id": APPLICATION_ID_2,
            "success": true,
            "native_identity": "MUST_NOT_SURVIVE"
        }),
        APPLICATION_ID,
    );
    assert!(!invalid.success);
    let unknown = computer_application_effect_outcome_unknown(
        "Runner returned inconsistent successful launch metadata",
        APPLICATION_ID,
    );
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert_eq!(unknown.output["reconcile_with"], "computer_list_windows");
    assert_eq!(unknown.output["recovery_kind"], "reobserve");
    assert_eq!(unknown.output["recovery_tool"], "computer_list_windows");
    assert!(unknown.output.get("state_changed").is_none());
    assert!(!serde_json::to_string(&unknown.output)
        .unwrap()
        .contains("MUST_NOT_SURVIVE"));

    for (dispatched, expected) in [
        (Some(false), "not_started"),
        (Some(true), "outcome_unknown"),
        (None, "outcome_unknown"),
    ] {
        let result = computer_application_effect_delivery_failure(
            "launch transport lost",
            dispatched,
            APPLICATION_ID,
        );
        assert_eq!(result.output["error_kind"], expected);
        if expected == "not_started" {
            assert_eq!(result.output["state_changed"], false);
        } else {
            assert_eq!(result.output["reconcile_with"], "computer_list_windows");
        }
    }

    for error in [
        "stale_application: PRIVATE_NATIVE_ID",
        "application_failed: PRIVATE_NATIVE_ID",
    ] {
        let result = computer_application_launch_runner_error(error, Some(true), APPLICATION_ID);
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["state_changed"], false);
        let serialized = serde_json::to_string(&result.output).unwrap();
        if error.starts_with("stale_application") {
            assert_eq!(result.output["recovery_kind"], "reobserve");
            assert_eq!(result.output["recovery_tool"], "computer_list_applications");
        } else {
            assert!(result.output.get("recovery_kind").is_none());
        }
        assert!(!serialized.contains("PRIVATE_NATIVE_ID"));
        assert!(!result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("PRIVATE_NATIVE_ID"));
    }
    let malformed = computer_application_effect_not_started(
        "invalid_application",
        "application_id is invalid",
        &"x".repeat(512),
    );
    assert!(malformed.output["application_id"].is_null());
    assert_eq!(malformed.output["execution_state"], "not_started");
}

const DISPLAY_ID: &str = "display_0123456789abcdef0123456789abcdef";

#[test]
fn computer_pointer_public_shape_and_effect_lifecycle_are_closed() {
    let context = PointerRequestContext {
        display_id: DISPLAY_ID.to_string(),
        snapshot_generation: 7,
        x: 123,
        y: 456,
    };
    for tool in ["computer_pointer_move", "computer_pointer_click"] {
        assert!(computer_request_is_effect(tool));
        let call = ToolCall::from_tool_name(
            tool,
            json!({
                "client_id": "msi",
                "display_id": DISPLAY_ID,
                "snapshot_generation": 7,
                "x": 123,
                "y": 456
            }),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::ComputerPointerMove { .. } | ToolCall::ComputerPointerClick { .. }
        ));
        for forbidden in ["global_x", "global_y", "button", "region", "surface_id"] {
            let mut args = json!({
                "client_id": "msi",
                "display_id": DISPLAY_ID,
                "snapshot_generation": 7,
                "x": 123,
                "y": 456
            });
            args.as_object_mut()
                .unwrap()
                .insert(forbidden.to_string(), json!(1));
            assert!(ToolCall::from_tool_name(tool, args)
                .unwrap_err()
                .contains("unknown field"));
        }
    }

    let not_started =
        computer_pointer_effect_delivery_failure("no dispatch", Some(false), &context);
    assert!(!not_started.success);
    assert_eq!(not_started.output["execution_state"], "not_started");
    assert_eq!(not_started.output["state_changed"], false);
    let pre_spend_not_started = computer_pointer_runner_error(
        "pointer_input_failed: native pointer preflight rejected before generation spend",
        Some(true),
        &context,
    );
    assert!(!pre_spend_not_started.success);
    assert_eq!(
        pre_spend_not_started.output["execution_state"],
        "not_started"
    );
    assert_eq!(pre_spend_not_started.output["state_changed"], false);
    assert!(pre_spend_not_started.output.get("reconcile_with").is_none());

    let spent_not_started = computer_pointer_runner_error(
        "not_started: native pointer final preflight failed after generation spend before post",
        Some(true),
        &context,
    );
    assert!(!spent_not_started.success);
    assert_eq!(spent_not_started.output["execution_state"], "not_started");
    assert_eq!(spent_not_started.output["state_changed"], false);
    assert_eq!(
        spent_not_started.output["reconcile_with"],
        "computer_snapshot_display"
    );
    assert!(spent_not_started
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("snapshot_generation is spent"));
    let runner_unknown = computer_pointer_runner_error(
        "outcome_unknown: native pointer post occurred but exact cursor proof failed",
        Some(true),
        &context,
    );
    assert!(!runner_unknown.success);
    assert_eq!(runner_unknown.output["execution_state"], "outcome_unknown");
    assert_eq!(
        runner_unknown.output["reconcile_with"],
        "computer_snapshot_display"
    );

    let unknown =
        computer_pointer_effect_delivery_failure("maybe dispatched", Some(true), &context);
    assert!(!unknown.success);
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert_eq!(
        unknown.output["reconcile_with"],
        "computer_snapshot_display"
    );
    assert!(unknown
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("do not blindly retry"));

    for platform in ["windows", "macos"] {
        let valid = validate_computer_pointer(
            json!({
                "platform": platform,
                "display_id": DISPLAY_ID,
                "snapshot_generation": 7,
                "x": 123,
                "y": 456,
                "success": true
            }),
            &context,
        );
        assert!(valid.success, "{platform}");
        assert_eq!(valid.output["execution_state"], "completed");
        assert_eq!(valid.output["state_changed"], true);
    }

    for platform in ["linux", "darwin", "", "MACOS"] {
        let invalid = validate_computer_pointer(
            json!({
                "platform": platform,
                "display_id": DISPLAY_ID,
                "snapshot_generation": 7,
                "x": 123,
                "y": 456,
                "success": true
            }),
            &context,
        );
        assert!(!invalid.success, "{platform}");
        assert_eq!(invalid.output["error_kind"], "invalid_runner_response");
    }

    let missing_platform = validate_computer_pointer(
        json!({
            "display_id": DISPLAY_ID,
            "snapshot_generation": 7,
            "x": 123,
            "y": 456,
            "success": true
        }),
        &context,
    );
    assert!(!missing_platform.success);
    assert_eq!(
        missing_platform.output["error_kind"],
        "invalid_runner_response"
    );

    for invalid_output in [
        json!({
            "platform": "macos",
            "display_id": "display_ffffffffffffffffffffffffffffffff",
            "snapshot_generation": 7,
            "x": 123,
            "y": 456,
            "success": true
        }),
        json!({
            "platform": "macos",
            "display_id": DISPLAY_ID,
            "snapshot_generation": 8,
            "x": 123,
            "y": 456,
            "success": true
        }),
        json!({
            "platform": "macos",
            "display_id": DISPLAY_ID,
            "snapshot_generation": 7,
            "x": 124,
            "y": 456,
            "success": true
        }),
        json!({
            "platform": "macos",
            "display_id": DISPLAY_ID,
            "snapshot_generation": 7,
            "x": 123,
            "y": 457,
            "success": true
        }),
        json!({
            "platform": "macos",
            "display_id": DISPLAY_ID,
            "snapshot_generation": 7,
            "x": 123,
            "y": 456,
            "success": false
        }),
    ] {
        let invalid = validate_computer_pointer(invalid_output, &context);
        assert!(!invalid.success);
        assert_eq!(invalid.output["error_kind"], "invalid_runner_response");
    }

    let leaked = validate_computer_pointer(
        json!({
            "platform": "windows",
            "display_id": DISPLAY_ID,
            "snapshot_generation": 7,
            "x": 123,
            "y": 456,
            "success": true,
            "global_x": -1797
        }),
        &context,
    );
    assert!(!leaked.success);
    assert_eq!(leaked.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_display_public_shape_and_read_only_semantics_are_closed() {
    assert!(valid_display_id(DISPLAY_ID));
    assert!(!computer_request_is_effect("computer_list_displays"));
    assert!(!computer_request_is_effect("computer_snapshot_display"));
    let list = ToolCall::from_tool_name(
        "computer_list_displays",
        json!({"client_id": "msi", "limit": 2}),
    )
    .unwrap();
    assert!(matches!(list, ToolCall::ComputerListDisplays { .. }));
    let snapshot = ToolCall::from_tool_name(
        "computer_snapshot_display",
        json!({"client_id": "msi", "display_id": DISPLAY_ID, "max_width": 960}),
    )
    .unwrap();
    assert!(matches!(snapshot, ToolCall::ComputerSnapshotDisplay { .. }));
    for forbidden in [
        "region",
        "x",
        "y",
        "global_x",
        "pointer",
        "click",
        "monitor_id",
    ] {
        let mut args = json!({"client_id": "msi", "display_id": DISPLAY_ID});
        args.as_object_mut()
            .unwrap()
            .insert(forbidden.to_string(), json!(1));
        let error = ToolCall::from_tool_name("computer_snapshot_display", args).unwrap_err();
        assert!(error.contains("unknown field"), "{error}");
    }
}

#[test]
fn computer_display_list_validator_is_bounded_exact_and_private() {
    let valid = validate_display_list(
        json!({
            "displays": [{
                "display_id": DISPLAY_ID,
                "width": 1920,
                "height": 1080,
                "primary": true
            }],
            "count": 1,
            "truncated": false
        }),
        1,
    );
    assert!(valid.success, "{:?}", valid.output);

    for output in [
        json!({
            "displays": [{
                "display_id": DISPLAY_ID,
                "width": 1920,
                "height": 1080,
                "primary": true,
                "device_path": "PRIVATE"
            }],
            "count": 1,
            "truncated": false
        }),
        json!({
            "displays": [{
                "display_id": DISPLAY_ID,
                "width": 1920,
                "height": 1080,
                "primary": true
            }],
            "count": 1,
            "truncated": false,
            "global_origin": {"x": 0, "y": 0}
        }),
    ] {
        let result = validate_display_list(output, 1);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_runner_response");
    }
}

#[test]
fn computer_display_snapshot_validator_enforces_identity_geometry_and_privacy() {
    let image = [0xff, 0xd8, 0xff, 0xe0];
    let output = json!({
        "display_id": DISPLAY_ID,
        "snapshot_generation": 7,
        "source_width": 1920,
        "source_height": 1080,
        "width": 960,
        "height": 540,
        "mime_type": "image/jpeg",
        "file_bytes": image.len(),
        "sha256": sha256_hex(&image),
        "captured_at_unix_ms": 1_700_000_000_000u64,
        "content_base64": general_purpose::STANDARD.encode(image)
    });
    let valid = validate_display_snapshot(output.clone(), DISPLAY_ID, "msi", Some(960), None);
    assert!(valid.success, "{:?}", valid.output);
    assert_eq!(valid.output["client_id"], "msi");

    for (field, value) in [
        ("native_identity", json!("PRIVATE")),
        ("device_path", json!("PRIVATE")),
        ("global_x", json!(0)),
        ("scale_factor", json!(1.25)),
    ] {
        let mut leaked = output.clone();
        leaked
            .as_object_mut()
            .unwrap()
            .insert(field.to_string(), value);
        let result = validate_display_snapshot(leaked, DISPLAY_ID, "msi", Some(960), None);
        assert!(!result.success, "field {field}");
        assert_eq!(result.output["error_kind"], "invalid_runner_response");
    }

    let mut wrong_generation = output.clone();
    wrong_generation["snapshot_generation"] = json!(0);
    assert!(
        !validate_display_snapshot(wrong_generation, DISPLAY_ID, "msi", Some(960), None,).success
    );

    let mut wrong_dimensions = output.clone();
    wrong_dimensions["height"] = json!(541);
    assert!(
        !validate_display_snapshot(wrong_dimensions, DISPLAY_ID, "msi", Some(960), None,).success
    );

    let mut oversized_source = output;
    oversized_source["source_width"] = json!(10_000);
    oversized_source["source_height"] = json!(10_000);
    let result = validate_display_snapshot(oversized_source, DISPLAY_ID, "msi", Some(960), None);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_accessibility_tree_validator_accepts_bounded_parent_first_tree() {
    let result = validate_accessibility_tree(accessibility_tree(), "surface_test", 2, 8);
    assert!(result.success, "{:?}", result.output);
}

#[test]
fn computer_accessibility_read_validators_accept_windows_platform() {
    let status = validate_accessibility_status(json!({
        "platform": "windows",
        "trusted": true
    }));
    assert!(status.success, "{:?}", status.output);

    let mut tree = accessibility_tree();
    tree["platform"] = json!("windows");
    let validated = validate_accessibility_tree(tree.clone(), "surface_test", 2, 8);
    assert!(validated.success, "{:?}", validated.output);

    let found = filter_accessibility_tree(
        tree,
        "surface_test",
        Some("AXButton"),
        None,
        None,
        None,
        None,
        4,
    );
    assert!(found.success, "{:?}", found.output);
    assert_eq!(found.output["platform"], "windows");

    let state = validate_computer_element_state(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "observation_generation": 7,
            "enabled": true,
            "focused": false,
            "protected": false,
            "value_empty": true,
            "can_press": false,
            "can_focus": false,
            "can_input_text": false
        }),
        "surface_test",
        "element_child",
    );
    assert!(state.success, "{:?}", state.output);
}

#[test]
fn computer_accessibility_tree_validator_rejects_forward_parent_reference() {
    let mut tree = accessibility_tree();
    tree["nodes"][0]["parent_element_id"] = json!("element_child");
    tree["nodes"][0]["depth"] = json!(1);
    let result = validate_accessibility_tree(tree, "surface_test", 2, 8);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_find_elements_matches_closed_semantic_fields_without_value_search() {
    let mut node = accessibility_tree()["nodes"][1].clone();
    node["subrole"] = json!("AXSearchField");
    node["description"] = json!("Find messages");
    node["placeholder"] = json!("Search conversations");
    node["value"] = json!("SUPER_SECRET_VALUE");
    node["focused"] = Value::Null;

    assert!(node_matches_find_query(
        &node,
        Some("AXButton"),
        Some("AXSearchField"),
        Some("Search"),
        None,
        Some(true),
    ));
    assert!(node_matches_find_query(
        &node,
        None,
        None,
        Some("messages"),
        None,
        None,
    ));
    assert!(!node_matches_find_query(
        &node,
        None,
        None,
        Some("SUPER_SECRET_VALUE"),
        None,
        None,
    ));
    assert!(!node_matches_find_query(
        &node,
        None,
        None,
        None,
        Some(false),
        None,
    ));
    assert!(!node_matches_find_query(
        &node,
        Some("AXTextField"),
        None,
        None,
        None,
        None,
    ));
}

#[test]
fn computer_find_elements_is_ordered_bounded_and_omits_ax_value() {
    let result = filter_accessibility_tree(
        accessibility_tree(),
        "surface_test",
        None,
        None,
        None,
        None,
        Some(true),
        1,
    );
    assert!(result.success, "{:?}", result.output);
    assert_eq!(result.output["surface_id"], "surface_test");
    assert_eq!(result.output["observation_generation"], 7);
    assert_eq!(result.output["scanned_nodes"], 2);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["elements"][0]["element_id"], "element_root");
    assert!(result.output["elements"][0].get("value").is_none());
}

#[test]
fn computer_element_state_validator_enforces_normalized_privacy_and_affordances() {
    let valid = validate_computer_element_state(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "observation_generation": 7,
            "enabled": true,
            "focused": true,
            "protected": false,
            "value_empty": true,
            "can_press": false,
            "can_focus": true,
            "can_input_text": true
        }),
        "surface_test",
        "element_child",
    );
    assert!(valid.success, "{:?}", valid.output);

    let protected_leak = validate_computer_element_state(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "observation_generation": 7,
            "enabled": true,
            "focused": true,
            "protected": true,
            "value_empty": true,
            "can_press": false,
            "can_focus": false,
            "can_input_text": false
        }),
        "surface_test",
        "element_child",
    );
    assert!(!protected_leak.success);
    assert_eq!(
        protected_leak.output["error_kind"],
        "invalid_runner_response"
    );

    let disabled_action = validate_computer_element_state(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "observation_generation": 7,
            "enabled": false,
            "focused": false,
            "protected": false,
            "value_empty": null,
            "can_press": true,
            "can_focus": false,
            "can_input_text": false
        }),
        "surface_test",
        "element_child",
    );
    assert!(!disabled_action.success);
    assert_eq!(
        disabled_action.output["error_kind"],
        "invalid_runner_response"
    );
}

#[test]
fn computer_activate_window_validator_is_exact_and_post_dispatch_mismatch_is_unknown() {
    let valid = validate_computer_activate_window(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "success": true
        }),
        "surface_test",
    );
    assert!(valid.success, "{:?}", valid.output);

    let windows = validate_computer_activate_window(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "success": true
        }),
        "surface_test",
    );
    assert!(windows.success, "{:?}", windows.output);

    let wrong_platform = validate_computer_activate_window(
        json!({
            "platform": "linux",
            "surface_id": "surface_test",
            "success": true
        }),
        "surface_test",
    );
    assert!(!wrong_platform.success);

    let invalid = validate_computer_activate_window(
        json!({
            "platform": "macos",
            "surface_id": "surface_other",
            "success": true,
            "title": "MUST_NOT_SURVIVE"
        }),
        "surface_test",
    );
    assert!(!invalid.success);
    let unknown = computer_effect_validated_result(
        invalid,
        "inconsistent window activation result; observe before retrying",
    );
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert!(!serde_json::to_string(&unknown.output)
        .unwrap()
        .contains("MUST_NOT_SURVIVE"));
}

#[test]
fn computer_control_validator_accepts_exact_metadata_only_success() {
    let result = validate_computer_control(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "action": "focus",
            "success": true
        }),
        "surface_test",
        "element_child",
        "focus",
    );
    assert!(result.success, "{:?}", result.output);

    let windows = validate_computer_control(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "action": "press",
            "success": true
        }),
        "surface_test",
        "element_child",
        "press",
    );
    assert!(windows.success, "{:?}", windows.output);
}

#[test]
fn computer_control_validator_rejects_mismatch_or_semantic_extra_fields() {
    // CU-AX2 remains closed to press/focus metadata; CU-AX3 does not widen it.
    let mismatched = validate_computer_control(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_other",
            "action": "focus",
            "success": true
        }),
        "surface_test",
        "element_child",
        "focus",
    );
    assert!(!mismatched.success);
    assert_eq!(mismatched.output["error_kind"], "invalid_runner_response");

    let semantic_extra = validate_computer_control(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "action": "press",
            "success": true,
            "title": "SECRET BUTTON"
        }),
        "surface_test",
        "element_child",
        "press",
    );
    assert!(!semantic_extra.success);
    assert_eq!(
        semantic_extra.output["error_kind"],
        "invalid_runner_response"
    );
}

#[test]
fn computer_scroll_validator_is_exact_and_post_dispatch_mismatch_is_unknown() {
    let valid = validate_computer_scroll_to_element(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "success": true
        }),
        "surface_test",
        "element_child",
    );
    assert!(valid.success, "{:?}", valid.output);

    let windows = validate_computer_scroll_to_element(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "success": true
        }),
        "surface_test",
        "element_child",
    );
    assert!(windows.success, "{:?}", windows.output);

    let invalid = validate_computer_scroll_to_element(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_other",
            "success": true,
            "title": "MUST_NOT_SURVIVE"
        }),
        "surface_test",
        "element_child",
    );
    assert!(!invalid.success);
    let unknown = computer_effect_validated_result(
        invalid,
        "inconsistent scroll result; observe before retrying",
    );
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert!(!serde_json::to_string(&unknown.output)
        .unwrap()
        .contains("MUST_NOT_SURVIVE"));
}

#[test]
fn computer_key_input_normalizes_closed_vocabulary_and_modifiers() {
    assert_eq!(
        normalize_computer_key_input(
            "tab",
            Some(vec!["command".to_string(), "shift".to_string()]),
        )
        .unwrap(),
        vec!["shift".to_string(), "command".to_string()]
    );
    assert!(normalize_computer_key_input("a", None).is_err());
    assert!(normalize_computer_key_input(
        "enter",
        Some(vec!["shift".to_string(), "shift".to_string()]),
    )
    .is_err());
    assert!(normalize_computer_key_input("enter", Some(vec!["caps_lock".to_string()]),).is_err());
}

#[test]
fn computer_key_input_validator_is_exact_and_post_dispatch_mismatch_is_unknown() {
    let expected_modifiers = json!(["shift", "command"]);
    let valid = validate_computer_key_input(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "key": "tab",
            "modifiers": ["shift", "command"],
            "success": true
        }),
        "surface_test",
        "tab",
        &expected_modifiers,
    );
    assert!(valid.success, "{:?}", valid.output);

    let invalid = validate_computer_key_input(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "key": "tab",
            "modifiers": ["command", "shift"],
            "success": true,
            "text": "MUST_NOT_SURVIVE"
        }),
        "surface_test",
        "tab",
        &expected_modifiers,
    );
    assert!(!invalid.success);
    let unknown = computer_effect_validated_result(
        invalid,
        "inconsistent key input result; observe before retrying",
    );
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert!(!serde_json::to_string(&unknown.output)
        .unwrap()
        .contains("MUST_NOT_SURVIVE"));
}

#[test]
fn computer_key_input_validator_accepts_closed_windows_metadata() {
    let expected_modifiers = json!(["shift"]);
    let valid = validate_computer_key_input(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "key": "tab",
            "modifiers": ["shift"],
            "success": true
        }),
        "surface_test",
        "tab",
        &expected_modifiers,
    );
    assert!(valid.success, "{:?}", valid.output);
}

#[test]
fn computer_input_text_validator_is_exact_and_post_dispatch_mismatch_is_unknown() {
    let valid = validate_computer_input_text(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "text_bytes": "你好🙂".len(),
            "success": true
        }),
        "surface_test",
        "element_child",
        "你好🙂".len(),
    );
    assert!(valid.success, "{:?}", valid.output);

    let windows = validate_computer_input_text(
        json!({
            "platform": "windows",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "text_bytes": 5,
            "success": true
        }),
        "surface_test",
        "element_child",
        5,
    );
    assert!(windows.success, "{:?}", windows.output);

    let invalid = validate_computer_input_text(
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "element_id": "element_child",
            "text_bytes": 1,
            "success": true,
            "text": "MUST_NOT_SURVIVE"
        }),
        "surface_test",
        "element_child",
        4,
    );
    assert!(!invalid.success);
    let unknown = computer_effect_validated_result(
        invalid,
        "inconsistent text input result; observe before retrying",
    );
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert!(!serde_json::to_string(&unknown.output)
        .unwrap()
        .contains("MUST_NOT_SURVIVE"));
}

#[test]
fn computer_input_text_runner_errors_never_echo_text() {
    let secret = "RUNNER_MUST_NOT_ECHO_隐私🙂";
    for (error, dispatched, expected_kind) in [
        (
            format!("input_failed: {secret}"),
            Some(true),
            "input_failed",
        ),
        (
            format!("outcome_unknown: {secret}"),
            Some(true),
            "outcome_unknown",
        ),
        (
            format!("unstructured: {secret}"),
            Some(false),
            "not_started",
        ),
        (
            format!("unstructured: {secret}"),
            Some(true),
            "outcome_unknown",
        ),
    ] {
        let result = computer_text_input_runner_error(&error, dispatched);
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert_eq!(result.output["error_kind"], expected_kind);
        assert!(!serialized.contains(secret));
        assert!(!result.error.as_deref().unwrap_or_default().contains(secret));
    }
}

#[test]
fn computer_input_text_utf8_byte_bound_rejects_empty_nul_and_oversize() {
    let valid = "你好🙂";
    assert_eq!(validate_input_text(valid).unwrap(), valid.len());
    let encoded = serde_json::to_value(ToolCall::ComputerInputText {
        client_id: "mini".to_string(),
        surface_id: "surface_test".to_string(),
        element_id: "element_test".to_string(),
        text: valid.to_string(),
    })
    .unwrap();
    let decoded: ToolCall = serde_json::from_value(encoded).unwrap();
    match decoded {
        ToolCall::ComputerInputText { text, .. } => assert_eq!(text, valid),
        other => panic!(
            "expected computer input text call, got {}",
            other.tool_name()
        ),
    }
    assert_eq!(
        validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES)).unwrap(),
        MAX_INPUT_TEXT_BYTES
    );
    assert!(validate_input_text("").is_err());
    assert!(validate_input_text("a\0b").is_err());
    assert!(validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES + 1)).is_err());
    assert!(validate_input_text(&"🙂".repeat((MAX_INPUT_TEXT_BYTES / 4) + 1)).is_err());
}

#[test]
fn computer_activate_window_uses_effect_delivery_semantics() {
    assert!(computer_request_is_effect("computer_launch_application"));
    assert!(computer_request_is_effect("computer_activate_window"));
    assert!(computer_request_is_effect("computer_control"));
    assert!(computer_request_is_effect("computer_scroll_to_element"));
    assert!(computer_request_is_effect("computer_key_input"));
    assert!(computer_request_is_effect("computer_input_text"));
    for read_only in [
        "computer_list_windows",
        "computer_list_applications",
        "computer_accessibility_tree",
        "computer_snapshot",
        "computer_snapshot_region",
    ] {
        assert!(!computer_request_is_effect(read_only), "{read_only}");
    }
}

#[test]
fn computer_control_transport_failure_is_retryable_only_when_undispatched() {
    // The same narrow delivery fence is shared by activate-window, scroll-to-element, key input, and text input.
    let not_started = computer_effect_delivery_failure("transport lost", Some(false));
    assert!(!not_started.success);
    assert_eq!(not_started.output["error_kind"], "not_started");
    assert_eq!(not_started.output["state_changed"], false);
    assert_eq!(not_started.output["execution_state"], "not_started");

    for dispatched in [Some(true), None] {
        let unknown = computer_effect_delivery_failure("transport lost", dispatched);
        assert!(!unknown.success);
        assert_eq!(unknown.output["error_kind"], "outcome_unknown");
        assert_eq!(unknown.output["execution_state"], "outcome_unknown");
        assert!(unknown.output.get("state_changed").is_none());
    }
}

#[test]
fn read_only_computer_transport_errors_keep_existing_classification() {
    let disconnected = computer_error("runner_disconnected", "Runner response channel closed");
    let timed_out = computer_error("runner_timeout", "Runner did not return computer request");
    assert_eq!(disconnected.output["error_kind"], "runner_disconnected");
    assert_eq!(timed_out.output["error_kind"], "runner_timeout");
    assert!(disconnected.output.get("execution_state").is_none());
    assert!(timed_out.output.get("execution_state").is_none());
}

#[test]
fn computer_control_runner_errors_preserve_structured_error_kinds() {
    for error in [
        "stale_element: handle expired",
        "control_failed: AXPress was rejected",
    ] {
        let result = computer_error(classify_runner_error(error), error);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], classify_runner_error(error));
    }
    let unknown = "outcome_unknown: AXPress messaging failed after dispatch";
    assert_eq!(classify_runner_error(unknown), "outcome_unknown");
    assert_eq!(
        classify_runner_error("key_input_failed: exact surface is not focused"),
        "key_input_failed"
    );
    let result = computer_effect_outcome_unknown(unknown);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "outcome_unknown");
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["recovery_kind"], "reobserve");
    assert!(result.output.get("recovery_tool").is_none());
}

#[test]
fn stale_computer_identities_expose_bounded_reobserve_targets() {
    for (error_kind, recovery_tool) in [
        ("stale_element", "computer_find_elements"),
        ("stale_surface", "computer_list_windows"),
        ("stale_application", "computer_list_applications"),
        ("stale_display", "computer_list_displays"),
    ] {
        let result = computer_error(error_kind, "stale observed identity");
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], error_kind);
        assert_eq!(result.output["recovery_kind"], "reobserve");
        assert_eq!(result.output["recovery_tool"], recovery_tool);
        assert!(matches!(
            result.output["recovery_tool"].as_str().unwrap(),
            "computer_find_elements"
                | "computer_list_windows"
                | "computer_list_applications"
                | "computer_list_displays"
        ));
    }
}

#[test]
fn computer_window_list_validator_rejects_more_than_requested_limit() {
    let result = validate_window_list(
        json!({
            "windows": [surface("surface_1"), surface("surface_2")],
            "count": 2,
            "truncated": false
        }),
        1,
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_runner_image_too_large_preserves_structured_error_kind() {
    let error = "image_too_large: raw RGBA capture exceeds bound";
    let result = computer_error(classify_runner_error(error), error);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "image_too_large");
}

#[test]
fn computer_snapshot_validator_accepts_advanced_region_metadata_and_rejects_mismatch() {
    let image = [0xff, 0xd8, 0xff, 0xe0];
    let region = json!({"x": 10, "y": 20, "width": 100, "height": 80});
    let output = json!({
        "surface": surface("surface_test"),
        "source_width": 1280,
        "source_height": 720,
        "region": region.clone(),
        "width": 50,
        "height": 40,
        "mime_type": "image/jpeg",
        "file_bytes": image.len(),
        "sha256": sha256_hex(&image),
        "captured_at_unix_ms": 1_700_000_000_000u64,
        "content_base64": general_purpose::STANDARD.encode(image)
    });
    let result = validate_snapshot(
        output.clone(),
        "surface_test",
        "mini",
        true,
        Some(&region),
        Some(60),
        Some(50),
    );
    assert!(result.success, "{:?}", result.output);
    assert_eq!(result.output["client_id"], "mini");

    let wrong_region = json!({"x": 11, "y": 20, "width": 100, "height": 80});
    let result = validate_snapshot(
        output,
        "surface_test",
        "mini",
        true,
        Some(&wrong_region),
        Some(60),
        Some(50),
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_snapshot_validator_requires_complete_advanced_metadata() {
    let image = [0xff, 0xd8, 0xff, 0xe0];
    let result = validate_snapshot(
        json!({
            "surface": surface("surface_test"),
            "width": 40,
            "height": 30,
            "mime_type": "image/jpeg",
            "file_bytes": image.len(),
            "content_base64": general_purpose::STANDARD.encode(image)
        }),
        "surface_test",
        "mini",
        true,
        None,
        Some(40),
        Some(30),
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_runner_response");
}

#[test]
fn computer_snapshot_validator_rejects_decoded_image_over_mcp_bound() {
    let encoded = "AAAA".repeat((MAX_MCP_IMAGE_BYTES / 3) + 1);
    let result = validate_snapshot(
        json!({
            "surface": surface("surface_test"),
            "width": 1280,
            "height": 720,
            "mime_type": "image/jpeg",
            "file_bytes": MAX_MCP_IMAGE_BYTES + 2,
            "content_base64": encoded
        }),
        "surface_test",
        "msi",
        false,
        None,
        None,
        None,
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "image_too_large");
}

#[test]
fn computer_clipboard_public_validator_is_strict_and_read_is_not_an_effect() {
    assert!(!computer_request_is_effect("computer_read_clipboard"));
    assert!(computer_request_is_effect("computer_write_clipboard"));
    assert_eq!(validate_clipboard_write_text("hello").unwrap(), 5);
    assert!(validate_clipboard_write_text("").is_err());
    assert!(validate_clipboard_write_text("bad\0text").is_err());
    assert!(validate_clipboard_write_text(&"a".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1)).is_err());

    let unavailable = validate_computer_read_clipboard(json!({
        "platform":"windows","available":false,"text_bytes":0
    }));
    assert!(unavailable.success);
    assert!(unavailable.output.get("text").is_none());

    let text = String::from_utf16(&[0x0041, 0x4E2D, 0xD83D, 0xDE00]).unwrap();
    let available = validate_computer_read_clipboard(json!({
        "platform":"windows","available":true,"text":text,"text_bytes":text.len()
    }));
    assert!(available.success);

    let macos_available = validate_computer_read_clipboard(json!({
        "platform":"macos","available":true,"text":"","text_bytes":0
    }));
    assert!(macos_available.success);

    let leaked = validate_computer_read_clipboard(json!({
        "platform":"windows","available":true,"text":"safe","text_bytes":4,
        "native_owner":"PRIVATE_OWNER"
    }));
    assert!(!leaked.success);
    assert_eq!(leaked.output["error_kind"], "invalid_runner_response");
    for platform in ["linux", "darwin", "MACOS"] {
        let unsupported = validate_computer_read_clipboard(json!({
            "platform":platform,"available":false,"text_bytes":0
        }));
        assert!(!unsupported.success, "platform {platform}");
    }

    let context = ClipboardWriteContext {
        text_bytes: Some(5),
    };
    let written = validate_computer_write_clipboard(
        json!({"platform":"windows","text_bytes":5,"success":true}),
        &context,
    );
    assert!(written.success);
    let macos_written = validate_computer_write_clipboard(
        json!({"platform":"macos","text_bytes":5,"success":true}),
        &context,
    );
    assert!(macos_written.success);
    let unsupported_write = validate_computer_write_clipboard(
        json!({"platform":"linux","text_bytes":5,"success":true}),
        &context,
    );
    assert!(!unsupported_write.success);
    let leaked_write = validate_computer_write_clipboard(
        json!({
            "platform":"windows","text_bytes":5,"success":true,
            "hglobal":"PRIVATE_HGLOBAL"
        }),
        &context,
    );
    assert!(!leaked_write.success);
}

#[test]
fn computer_clipboard_write_lifecycle_preserves_not_started_and_unknown() {
    let context = ClipboardWriteContext {
        text_bytes: Some(5),
    };
    let not_started =
        computer_clipboard_write_delivery_failure("not dispatched", Some(false), &context);
    assert!(!not_started.success);
    assert_eq!(not_started.output["execution_state"], "not_started");
    assert_eq!(not_started.output["state_changed"], false);
    assert_eq!(not_started.output["text_bytes"], 5);

    let unknown = computer_clipboard_write_delivery_failure("response lost", None, &context);
    assert!(!unknown.success);
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert!(unknown.output.get("state_changed").is_none());
    assert!(unknown
        .error
        .as_deref()
        .unwrap()
        .contains("do not blindly retry"));
    assert!(unknown
        .error
        .as_deref()
        .unwrap()
        .contains("computer:clipboard_read"));

    let native_unknown = computer_clipboard_write_runner_error(
        "outcome_unknown: EmptyClipboard changed state before SetClipboardData failed",
        Some(true),
        &context,
    );
    assert_eq!(native_unknown.output["execution_state"], "outcome_unknown");
    assert_eq!(native_unknown.output["state_changed"], true);

    let native_not_started = computer_clipboard_write_runner_error(
        "not_started: OpenClipboard failed",
        Some(true),
        &context,
    );
    assert_eq!(native_not_started.output["execution_state"], "not_started");
    assert_eq!(native_not_started.output["state_changed"], false);
}

#[test]
fn computer_save_snapshot_lifecycle_distinguishes_not_started_from_unknown() {
    let not_started = computer_snapshot_artifact_lifecycle_failure(
        "not dispatched",
        ShellCommandExecutionState::NotStarted,
        "agent:target:demo",
        "artifacts/ui.jpg",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1234,
        "image/jpeg",
    );
    assert!(!not_started.success);
    assert_eq!(not_started.output["error_kind"], "not_started");
    assert_eq!(not_started.output["execution_state"], "not_started");
    assert_eq!(not_started.output["state_changed"], false);
    assert!(not_started.output.get("reconcile_with").is_none());
    assert!(not_started.output.get("recovery_kind").is_none());
    assert!(not_started.output.get("recovery_tool").is_none());

    let unknown = computer_snapshot_artifact_lifecycle_failure(
        "response lost",
        ShellCommandExecutionState::OutcomeUnknown,
        "agent:target:demo",
        "artifacts/ui.jpg",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        4321,
        "image/jpeg",
    );
    assert!(!unknown.success);
    assert_eq!(unknown.output["error_kind"], "outcome_unknown");
    assert_eq!(unknown.output["execution_state"], "outcome_unknown");
    assert_eq!(
        unknown.output["reconcile_with"],
        "read_project_artifact_metadata"
    );
    assert_eq!(unknown.output["recovery_kind"], "reconcile");
    assert_eq!(
        unknown.output["recovery_tool"],
        "read_project_artifact_metadata"
    );
    assert_eq!(unknown.output["project"], "agent:target:demo");
    assert_eq!(unknown.output["path"], "artifacts/ui.jpg");
    assert_eq!(unknown.output["expected_file_bytes"], 4321);
    assert_eq!(unknown.output["expected_mime_type"], "image/jpeg");
}

#[test]
fn computer_save_snapshot_definite_write_failure_is_not_retry_uncertainty() {
    let result = computer_snapshot_artifact_definite_failure(
        "file exists and overwrite is false",
        "agent:target:demo",
        "artifacts/ui.jpg",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        99,
        "image/jpeg",
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "artifact_write_failed");
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["state_changed"], false);
    assert!(result.output.get("reconcile_with").is_none());
    assert!(result.output.get("recovery_kind").is_none());
    assert!(result.output.get("recovery_tool").is_none());
}

#[test]
fn computer_snapshot_validator_attaches_exact_client_id() {
    let image = [0xff, 0xd8, 0xff, 0xe0];
    let result = validate_snapshot(
        json!({
            "surface": surface("surface_test"),
            "width": 1280,
            "height": 720,
            "mime_type": "image/jpeg",
            "file_bytes": image.len(),
            "content_base64": general_purpose::STANDARD.encode(image)
        }),
        "surface_test",
        "msi",
        false,
        None,
        None,
        None,
    );
    assert!(result.success);
    assert_eq!(result.output["client_id"], "msi");
}
