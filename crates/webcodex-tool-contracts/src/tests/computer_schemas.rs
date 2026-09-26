use super::*;

fn action_branch<'a>(schema: &'a Value, action: &str) -> &'a Value {
    schema["oneOf"]
        .as_array()
        .expect("Computer gateway oneOf")
        .iter()
        .find(|branch| {
            let discriminator = &branch["properties"]["action"];
            discriminator.get("const").and_then(Value::as_str) == Some(action)
                || discriminator
                    .get("enum")
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.len() == 1 && values[0] == action)
        })
        .unwrap_or_else(|| panic!("missing Computer action branch {action}"))
}

fn action_properties<'a>(schema: &'a Value, action: &str) -> &'a serde_json::Map<String, Value> {
    action_branch(schema, action)["properties"]
        .as_object()
        .expect("Computer action properties")
}

#[test]
fn tool_specs_hide_legacy_sync_wait_from_process_and_script_discovery() {
    let specs = registered_tool_specs();
    for name in ["run_process", "run_script"] {
        let spec = spec_named(&specs, name);
        let props = spec.input_schema["properties"].as_object().unwrap();
        assert!(
            !props.contains_key("sync_wait_secs"),
            "{name} must hide sync_wait_secs from model discovery"
        );
        assert!(!required_fields(spec).contains(&"sync_wait_secs".to_string()));
    }
}

#[test]
fn computer_primary_surface_is_three_canonical_tools() {
    let names = registered_tool_specs()
        .into_iter()
        .filter(|spec| spec.name.starts_with("computer_"))
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "computer_observe".to_string(),
            "computer_control".to_string(),
            "computer_save_snapshot".to_string(),
        ]
    );
}

#[test]
fn computer_observe_schema_is_closed_read_only_action_union() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_observe");
    assert_eq!(spec.annotations["readOnlyHint"], true);
    assert_eq!(spec.annotations["destructiveHint"], false);
    let expected = [
        "targets",
        "windows",
        "displays",
        "applications",
        "accessibility_status",
        "accessibility_tree",
        "find_elements",
        "element_state",
        "snapshot_window",
        "snapshot_display",
        "read_clipboard",
    ];
    let branches = spec.input_schema["oneOf"].as_array().unwrap();
    assert_eq!(branches.len(), expected.len());
    for action in expected {
        let branch = action_branch(&spec.input_schema, action);
        assert_eq!(branch["additionalProperties"], false, "{action}");
        assert!(branch["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "action"));
    }

    let find = action_properties(&spec.input_schema, "find_elements");
    assert_schema_fields!(
        find,
        "find_elements action",
        present: ["action", "client_id", "surface_id", "role", "subrole", "label", "focused", "enabled", "limit"],
        absent: ["text", "application_id", "display_id"]
    );
    assert_eq!(find["limit"]["minimum"], 1);

    let snapshot = action_properties(&spec.input_schema, "snapshot_window");
    assert_schema_fields!(
        snapshot,
        "snapshot_window action",
        present: ["action", "client_id", "surface_id", "region", "max_width", "max_height"],
        absent: ["display_id", "format", "quality", "save"]
    );
    assert_eq!(snapshot["region"]["additionalProperties"], false);

    let display = action_properties(&spec.input_schema, "snapshot_display");
    assert_schema_fields!(
        display,
        "snapshot_display action",
        present: ["action", "client_id", "display_id", "max_width", "max_height"],
        absent: ["surface_id", "region", "x", "y", "pointer", "click"]
    );

    for value in [
        json!({"action":"targets"}),
        json!({"action":"windows","client_id":"special","limit":9999}),
        json!({"action":"snapshot_window","client_id":"special","surface_id":"surface_test","max_width":10000}),
        json!({"action":"snapshot_display","client_id":"special","display_id":"display_iavN7wEjRWeJq83v","max_height":u32::MAX}),
        json!({"action":"read_clipboard","client_id":"special"}),
    ] {
        test_support::validate_schema_instance(&value, &spec.input_schema).unwrap();
    }
    for invalid in [
        json!({"action":"unknown","client_id":"special"}),
        json!({"action":"windows"}),
        json!({"action":"windows","client_id":"special","text":"nope"}),
        json!({"action":"snapshot_display","client_id":"special","display_id":"display_iavN7wEjRWeJq83v","surface_id":"surface_nope"}),
    ] {
        assert!(
            test_support::validate_schema_instance(&invalid, &spec.input_schema).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn computer_control_schema_is_closed_action_union() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_control");
    assert_eq!(spec.annotations["readOnlyHint"], false);
    assert_eq!(spec.annotations["destructiveHint"], true);
    let expected = [
        "launch_application",
        "activate_window",
        "press",
        "focus",
        "scroll_to_element",
        "key",
        "input_text",
        "pointer_move",
        "pointer_click",
        "write_clipboard",
    ];
    let branches = spec.input_schema["oneOf"].as_array().unwrap();
    assert_eq!(branches.len(), expected.len());
    for action in expected {
        let branch = action_branch(&spec.input_schema, action);
        assert_eq!(branch["additionalProperties"], false, "{action}");
    }

    let launch = action_properties(&spec.input_schema, "launch_application");
    assert_schema_fields!(
        launch,
        "launch_application action",
        present: ["action", "client_id", "application_id"],
        absent: ["path", "argv", "cwd", "environment", "command", "script", "url"]
    );
    let pointer = action_properties(&spec.input_schema, "pointer_click");
    assert_schema_fields!(
        pointer,
        "pointer_click action",
        present: ["action", "client_id", "display_id", "snapshot_generation", "x", "y"],
        absent: ["surface_id", "global_x", "global_y", "button", "double_click"]
    );
    assert_eq!(pointer["snapshot_generation"]["minimum"], 1);
    let write = action_properties(&spec.input_schema, "write_clipboard");
    assert_eq!(write["text"]["minLength"], 1);
    assert_eq!(write["text"]["maxLength"], 16384);

    for value in [
        json!({"action":"launch_application","client_id":"special","application_id":"application_iavN7wEjRWeJq83v"}),
        json!({"action":"press","client_id":"special","surface_id":"surface_test","element_id":"element_test"}),
        json!({"action":"key","client_id":"special","surface_id":"surface_test","key":"enter"}),
        json!({"action":"pointer_click","client_id":"special","display_id":"display_iavN7wEjRWeJq83v","snapshot_generation":3,"x":400,"y":220}),
        json!({"action":"write_clipboard","client_id":"special","text":"hello"}),
    ] {
        test_support::validate_schema_instance(&value, &spec.input_schema).unwrap();
    }
    for invalid in [
        json!({"action":"pointer_click","client_id":"special","display_id":"display_iavN7wEjRWeJq83v","snapshot_generation":0,"x":1,"y":1}),
        json!({"action":"launch_application","client_id":"special","application_id":"application_iavN7wEjRWeJq83v","argv":["--unsafe"]}),
        json!({"action":"write_clipboard","client_id":"special"}),
        json!({"action":"targets"}),
    ] {
        assert!(
            test_support::validate_schema_instance(&invalid, &spec.input_schema).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn computer_gateway_outputs_cover_preserved_observation_and_control_shapes() {
    let specs = registered_tool_specs();
    let observe = spec_named(&specs, "computer_observe");
    let observe_output = observe.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "targets",
        "windows",
        "displays",
        "applications",
        "nodes",
        "elements",
        "observation_generation",
        "available",
        "text",
        "snapshot_generation",
        "content_base64",
        "suggested_call",
        "reconcile_with",
    ] {
        assert!(
            observe_output.contains_key(field),
            "observe output missing {field}"
        );
    }
    assert_eq!(
        observe.output_schema["properties"]["output"]["additionalProperties"],
        false
    );

    let control = spec_named(&specs, "computer_control");
    let control_output = control.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "application_id",
        "surface_id",
        "element_id",
        "display_id",
        "snapshot_generation",
        "x",
        "y",
        "text_bytes",
        "success",
        "execution_state",
        "state_changed",
        "suggested_call",
        "reconcile_with",
    ] {
        assert!(
            control_output.contains_key(field),
            "control output missing {field}"
        );
    }
    assert_eq!(
        control.output_schema["properties"]["output"]["additionalProperties"],
        false
    );
}

#[test]
fn computer_save_snapshot_remains_separate_create_only_project_write() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_save_snapshot");
    assert_eq!(
        required_fields(spec),
        vec![
            "project".to_string(),
            "path".to_string(),
            "client_id".to_string(),
            "surface_id".to_string(),
        ]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_save_snapshot input schema",
        present: ["project", "path", "client_id", "surface_id", "region", "max_width", "max_height", "session_id"],
        absent: ["overwrite", "format", "quality", "mime_type", "content_base64", "save"]
    );
    assert_eq!(props["region"]["additionalProperties"], false);
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_save_snapshot output schema",
        present: ["project", "path", "client_id", "surface_id", "source_width", "source_height", "region", "width", "height", "mime_type", "file_bytes", "sha256", "saved"],
        absent: ["content_base64", "captured_at_unix_ms"]
    );
}
