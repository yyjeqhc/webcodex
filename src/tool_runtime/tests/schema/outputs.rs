use super::*;

#[test]
fn computer_control_output_schema_has_closed_native_platforms() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("control_computer");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let application_id = "application_iavN7wEjRWeJq83v";
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
                "suggested_call": {
                    "follow_up_kind": "fallback_recovery",
                    "tool": "observe_computer",
                    "arguments": {"action": "applications", "client_id": "msi"}
                }
            }),
        ),
    )
    .unwrap();
    validate(&stale).unwrap();
    let mut legacy_recovery_tool = stale.clone();
    legacy_recovery_tool["output"]["recovery_tool"] = json!("observe_computer");
    assert!(validate(&legacy_recovery_tool).is_err());
    assert!(schema["properties"]["output"]["properties"]
        .get("recovery_tool")
        .is_none());
    let serialized_schema = serde_json::to_string(&schema).unwrap();
    assert!(serialized_schema.contains("suggested_call"));
    assert!(serialized_schema.contains("reconcile_with"));
    assert!(serialized_schema.contains("recovery_tool"));
    assert!(serialized_schema.contains("not"));
    let mut inferred_argument = stale.clone();
    inferred_argument["output"]["suggested_call"]["arguments"]["surface_id"] =
        json!("surface_should_not_be_inferred");
    assert!(validate(&inferred_argument).is_err());
    let mut duplicate_kind = stale.clone();
    duplicate_kind["output"]["recovery_kind"] = json!("reobserve");
    assert!(validate(&duplicate_kind).is_err());

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
fn browser_output_schemas_accept_canonical_results_and_reject_leaked_fields() {
    let observe_schema = crate::tool_runtime::registry::output_schema_for_tool("observe_browser");
    let act_schema = crate::tool_runtime::registry::output_schema_for_tool("control_browser");
    let validate_observe = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            value,
            &observe_schema,
        )
    };
    let validate_act = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &act_schema)
    };

    let snapshot = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": false,
        "browser_id": "browser_abcdefghijklmnop",
        "page_id": "page_abcdefghijklmnop",
        "snapshot_generation": 1,
        "node_count": 1,
        "truncated": false,
        "nodes": [{
            "role": "button",
            "name": "Continue",
            "value": null,
            "element_id": "element_abcdefghijklmnop",
            "actions": ["click"],
            "actionable": true
        }]
    })))
    .unwrap();
    validate_observe(&snapshot).unwrap();
    let mut unknown_action = snapshot.clone();
    unknown_action["output"]["nodes"][0]["actions"] = json!(["spinbutton"]);
    assert!(validate_observe(&unknown_action).is_err());
    let value_control =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "execution_state": "completed",
            "state_changed": false,
            "browser_id": "browser_abcdefghijklmnop",
            "page_id": "page_abcdefghijklmnop",
            "snapshot_generation": 2,
            "node_count": 1,
            "truncated": false,
            "nodes": [{
                "role": "spinbutton",
                "name": "Qty",
                "element_id": "element_abcdefghijklmnop",
                "actions": ["set_value"],
                "actionable": true
            }]
        })))
        .unwrap();
    validate_observe(&value_control).unwrap();

    let screenshot =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "execution_state": "completed",
            "state_changed": false,
            "browser_id": "browser_abcdefghijklmnop",
            "page_id": "page_abcdefghijklmnop",
            "content_base64": "iVBORw0KGgo=",
            "mime_type": "image/png",
            "width": 1024,
            "height": 768,
            "file_bytes": 8,
            "sha256": "a".repeat(64)
        })))
        .unwrap();
    validate_observe(&screenshot).unwrap();

    let console = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": false,
        "retained_count": 200,
        "count": 2,
        "truncated": true,
        "entries": [
            {"level":"warning","text":"warn","source":null,"timestamp":1.0},
            {"level":"error","text":"boom","source":"https://example.test/app.js","timestamp":2.0}
        ]
    })))
    .unwrap();
    validate_observe(&console).unwrap();

    let diagnostics = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": false,
        "console_retained": 200,
        "console_count": 1,
        "console_truncated": true,
        "console": [
            {"level":"exception","text":"boom","source":null,"timestamp":3.0}
        ],
        "network_retained": 300,
        "network_count": 1,
        "network_truncated": false,
        "network": [
            {"method":"GET","url":"https://example.test/api","resource_type":"Fetch","status":500,"failed_reason":null,"timestamp":4.0}
        ]
    })))
    .unwrap();
    validate_observe(&diagnostics).unwrap();

    let launch = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": true,
        "browser_id": "browser_abcdefghijklmnop",
        "page_count": 1
    })))
    .unwrap();
    validate_act(&launch).unwrap();

    let stale = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "stale element",
            json!({
                "execution_state": "not_started",
                "state_changed": false,
                "error_kind": "stale_element",
                "message": "element identity is stale",
                "recovery": {
                    "reason": "re-observe before acting",
                    "suggested_call": {
                        "follow_up_kind": "fallback_recovery",
                        "tool": "observe_browser",
                        "arguments": {
                            "action": "snapshot",
                            "client_id": "msi",
                            "browser_id": "browser_abcdefghijklmnop",
                            "page_id": "page_abcdefghijklmnop"
                        }
                    }
                }
            }),
        ),
    )
    .unwrap();
    validate_act(&stale).unwrap();

    let compact = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": false,
        "browser_id": "browser_abcdefghijklmnop",
        "page_id": "page_abcdefghijklmnop",
        "snapshot_generation": 4,
        "snapshot_mode": "interactive",
        "auto_compacted": true,
        "max_nodes": 256,
        "max_depth": 32,
        "node_count": 2,
        "truncated": false,
        "nodes": [
            {
                "role": "combobox",
                "name": "Fruit",
                "disabled": false,
                "element_id": "element_abcdefghijklmnop",
                "actions": ["select_option"],
                "actionable": true
            },
            {
                "role": "option",
                "name": "Apple",
                "value": "a",
                "group_id": "group_1",
                "group_role": "combobox",
                "group_label": "Fruit",
                "selected": true,
                "disabled": false,
                "read_only": false,
                "actionable": false
            }
        ]
    })))
    .unwrap();
    validate_observe(&compact).unwrap();
    let mut leaked_backend = compact;
    leaked_backend["output"]["nodes"][1]["backend_node_id"] = json!(11);
    assert!(validate_observe(&leaked_backend).is_err());

    let diagnostics_delta =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "execution_state": "completed",
            "state_changed": false,
            "cursor": 5,
            "since_cursor": 3,
            "delta_truncated": false,
            "new_console_errors": 1,
            "new_console_warnings": 0,
            "new_failed_requests": 0,
            "new_4xx": 0,
            "new_5xx": 0,
            "console_retained": 0,
            "console_count": 0,
            "console_truncated": false,
            "console": [],
            "network_retained": 0,
            "network_count": 0,
            "network_truncated": false,
            "network": []
        })))
        .unwrap();
    validate_observe(&diagnostics_delta).unwrap();

    let effect = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": true,
        "stability": {"stable": false, "waited_ms": 250, "reason": "deadline"}
    })))
    .unwrap();
    validate_act(&effect).unwrap();

    let mut leaked = snapshot;
    leaked["output"]["target_id"] = json!("private-cdp-target");
    assert!(validate_observe(&leaked).is_err());
    let mut leaked = launch;
    leaked["output"]["debug_endpoint"] = json!("private-endpoint");
    assert!(validate_act(&leaked).is_err());
}

#[test]
fn read_files_output_schema_rejects_sparse_item_over_default_limit() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let default_limit = webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
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
            &schema,
        )
        .is_err(),
        "complete sparse read_files item cannot claim more lines than the default range can return"
    );
}
