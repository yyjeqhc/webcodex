use super::*;

#[test]
fn computer_post_observation_contract_is_explicit_and_narrow() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "control_computer");
    assert!(action_properties(&spec.input_schema, "press").contains_key("observe_after"));
    assert!(action_properties(&spec.input_schema, "input_text").contains_key("observe_after"));
    assert!(!action_properties(&spec.input_schema, "focus").contains_key("observe_after"));
    assert_eq!(
        spec.output_schema["properties"]["output"]["properties"]["post_observation"]["properties"]
            ["success"]["type"],
        "boolean"
    );
    test_support::validate_schema_instance(
        &json!({"success":true,"output":{"platform":"windows","surface_id":"surface_test",
            "element_id":"element_test","action":"press","success":true,
            "post_observation":{"success":false,"output":{"error_kind":"timeout","message":"read timed out"},"error":"read timed out"}}}),
        &spec.output_schema,
    ).unwrap();
    for action in ["press", "input_text"] {
        let mut args = json!({"action":action,"client_id":"runner","surface_id":"surface_test",
            "element_id":"element_test", "observe_after":{"action":"accessibility_tree","max_depth":3,"max_nodes":32}});
        if action == "input_text" {
            args["text"] = json!("hello");
        }
        let call = ToolCall::from_tool_name("control_computer", args.clone()).unwrap();
        let ToolCall::ComputerControl(control) = call else {
            panic!("Computer control call")
        };
        assert_eq!(
            serde_json::to_value(control).unwrap()["observe_after"],
            args["observe_after"]
        );
        args["observe_after"]["action"] = json!("snapshot_display");
        assert!(ToolCall::from_tool_name("control_computer", args).is_err());
    }
    assert!(ToolCall::from_tool_name(
        "control_computer",
        json!({"action":"focus",
        "client_id":"runner","surface_id":"surface_test","element_id":"element_test",
        "observe_after":{"action":"accessibility_tree"}})
    )
    .is_err());
}

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
        .filter(|spec| {
            crate::lookup_tool_definition(&spec.name)
                .is_some_and(|d| d.category == crate::TOOL_CATEGORY_COMPUTER)
        })
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "observe_computer".to_string(),
            "control_computer".to_string(),
            "save_computer_snapshot".to_string(),
        ]
    );
}

#[test]
fn computer_observe_schema_is_closed_read_only_action_union() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "observe_computer");
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
fn computer_element_state_discovery_guides_current_semantic_action_selection() {
    let specs = registered_tool_specs();
    for name in ["observe_computer", "control_computer"] {
        let spec = spec_named(&specs, name);
        for term in ["element_state", "can_press", "can_focus", "can_input_text"] {
            assert!(spec.description.contains(term), "{name} omits {term}");
        }
    }

    let observe = spec_named(&specs, "observe_computer");
    let state = action_branch(&observe.input_schema, "element_state");
    let description = state["description"].as_str().expect("state guidance");
    for term in [
        "can_press",
        "can_focus",
        "can_input_text",
        "observation_generation",
        "stale",
    ] {
        assert!(description.contains(term), "state schema omits {term}");
    }
    assert_eq!(state["properties"].as_object().unwrap().len(), 4);
    let input = json!({
        "action": "element_state", "client_id": "observed-client",
        "surface_id": "observed-surface", "element_id": "observed-element"
    });
    test_support::validate_schema_instance(&input, &observe.input_schema).unwrap();
    let mut unsupported = input;
    unsupported["refresh"] = json!(true);
    assert!(test_support::validate_schema_instance(&unsupported, &observe.input_schema).is_err());
}

#[test]
fn computer_control_schema_is_closed_action_union() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "control_computer");
    assert_eq!(spec.annotations["readOnlyHint"], false);
    assert_eq!(spec.annotations["destructiveHint"], true);
    let expected = [
        "launch_application",
        "activate_window",
        "press",
        "select",
        "focus",
        "scroll_to_element",
        "scroll",
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
fn computer_key_schema_discloses_closed_vocabulary_and_modifier_constraints() {
    use webcodex_core::computer_input_contract::{
        COMPUTER_KEY_INPUT_KEYS, COMPUTER_KEY_INPUT_MODIFIERS,
    };

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "control_computer");
    let key = action_properties(&spec.input_schema, "key");
    let keys: Vec<_> = webcodex_core::computer_input_contract::computer_key_input_keys().collect();
    assert_eq!(key["key"]["enum"], json!(keys));
    let modifiers = key["modifiers"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["type"] == "array")
        .unwrap();
    assert_eq!(
        modifiers["items"]["enum"],
        json!(COMPUTER_KEY_INPUT_MODIFIERS)
    );
    assert_eq!(modifiers["maxItems"], COMPUTER_KEY_INPUT_MODIFIERS.len());
    assert_eq!(modifiers["uniqueItems"], true);

    for name in COMPUTER_KEY_INPUT_KEYS {
        for modifiers in [
            None,
            Some(Value::Null),
            Some(json!([])),
            Some(json!(COMPUTER_KEY_INPUT_MODIFIERS)),
        ] {
            let mut input = json!({"action":"key","client_id":"special","surface_id":"surface_test","key":name});
            if let Some(modifiers) = modifiers {
                input["modifiers"] = modifiers;
            }
            test_support::validate_schema_instance(&input, &spec.input_schema).unwrap();
        }
    }
    for (name, modifiers) in [
        ("unknown", json!([])),
        ("Ctrl+P", json!([])),
        ("Enter", json!([])),
        ("tab", json!(["ctrl"])),
        ("tab", json!(["alt"])),
        ("tab", json!(["Shift"])),
        ("tab", json!("shift")),
        ("tab", json!(true)),
        ("tab", json!(0)),
        ("tab", json!({})),
        ("tab", json!([null])),
        ("tab", json!([1])),
        ("tab", json!(["shift", "shift"])),
        (
            "tab",
            json!(["shift", "control", "option", "command", "shift"]),
        ),
    ] {
        let input = json!({"action":"key","client_id":"special","surface_id":"surface_test","key":name,"modifiers":modifiers});
        assert!(
            test_support::validate_schema_instance(&input, &spec.input_schema).is_err(),
            "{input}"
        );
    }
    for text in [
        "Ctrl+P",
        "Shift+Tab",
        "Windows",
        "macOS",
        "option+Tab",
        "control+Escape",
    ] {
        assert!(
            spec.description.contains(text),
            "missing model guidance: {text}"
        );
    }
}

#[test]
fn computer_key_chords_accept_canonical_keys_and_reject_malformed_modifiers() {
    let schema = crate::input_schema_for_tool("control_computer");
    for key in ["a", "j", "p", "0", "f24", "delete", "slash", "numpad_enter"] {
        let input =
            json!({"action":"key", "client_id":"client", "surface_id":"surface_test", "key":key});
        for modifiers in [
            None,
            Some(Value::Null),
            Some(json!([])),
            Some(json!(["shift"])),
            Some(json!(["control"])),
            Some(json!(["control", "shift", "option"])),
        ] {
            let mut valid = input.clone();
            if let Some(modifiers) = modifiers {
                valid["modifiers"] = modifiers;
            }
            test_support::validate_schema_instance(&valid, &schema).unwrap();
            ToolCall::from_tool_name("control_computer", valid).unwrap();
        }
        for modifiers in [
            Some(json!(["ctrl"])),
            Some(json!(["control", "control"])),
            Some(json!(["shift", "control", "option", "command", "shift"])),
            Some(json!("control")),
        ] {
            let mut invalid = input.clone();
            invalid.as_object_mut().unwrap().remove("modifiers");
            if let Some(modifiers) = modifiers {
                invalid["modifiers"] = modifiers;
            }
            assert!(
                test_support::validate_schema_instance(&invalid, &schema).is_err(),
                "{invalid}"
            );
        }
    }
}

#[test]
fn computer_key_success_receipts_fit_primitive_and_gateway_output_schemas() {
    let specs = registered_tool_specs();
    let control = spec_named(&specs, "control_computer");
    let primitive = output_schema_for_tool("computer_key_input");
    let output_schemas = [
        ("computer_key_input", &primitive),
        ("control_computer", &control.output_schema),
    ];
    let mut failures = Vec::new();
    for key in webcodex_core::computer_input_contract::computer_key_input_keys() {
        let modifiers = if matches!(key, "j" | "p") {
            json!(["control"])
        } else {
            json!([])
        };
        let input = json!({"action":"key", "client_id":"client", "surface_id":"surface_test",
            "key":key, "modifiers":modifiers});
        test_support::validate_schema_instance(&input, &control.input_schema).unwrap();
        let receipt = json!({"success":true, "output":{"platform":"windows",
            "surface_id":"surface_test", "key":key, "modifiers":modifiers, "success":true}});
        for (name, schema) in output_schemas {
            if let Err(error) = test_support::validate_schema_instance(&receipt, schema) {
                failures.push(format!("{name} {key}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for key in ["unknown", "Ctrl+P", "P"] {
        let receipt = json!({"success":true, "output":{"platform":"windows",
            "surface_id":"surface_test", "key":key, "modifiers":["control"], "success":true}});
        for (name, schema) in output_schemas {
            assert!(
                test_support::validate_schema_instance(&receipt, schema).is_err(),
                "{name} accepted {receipt}"
            );
        }
    }
}

#[test]
fn computer_key_wire_keeps_optional_modifiers_and_canonical_strings() {
    let schema = crate::input_schema_for_tool("control_computer");
    for modifiers in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!(["shift"])),
    ] {
        let mut input =
            json!({"action":"key","client_id":"special","surface_id":"surface_test","key":"tab"});
        if let Some(modifiers) = modifiers {
            input["modifiers"] = modifiers;
        }
        let expected_modifiers = input
            .get("modifiers")
            .filter(|value| !value.is_null())
            .cloned()
            .unwrap_or(json!([]));
        test_support::validate_schema_instance(&input, &schema).unwrap();
        let call = ToolCall::from_tool_name("control_computer", input).unwrap();
        let ToolCall::ComputerControl(crate::tool_call::ComputerControlToolCall::Key {
            key,
            modifiers,
            ..
        }) = call
        else {
            panic!("expected canonical Computer key input");
        };
        assert_eq!(key, "tab");
        assert_eq!(json!(modifiers.unwrap_or_default()), expected_modifiers);
    }
}

#[test]
fn computer_gateway_outputs_cover_preserved_observation_and_control_shapes() {
    let specs = registered_tool_specs();
    let observe = spec_named(&specs, "observe_computer");
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

    let control = spec_named(&specs, "control_computer");
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
    let spec = spec_named(&specs, "save_computer_snapshot");
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
        "save_computer_snapshot input schema",
        present: ["project", "path", "client_id", "surface_id", "region", "max_width", "max_height", "session_id"],
        absent: ["overwrite", "format", "quality", "mime_type", "content_base64", "save"]
    );
    assert_eq!(props["region"]["additionalProperties"], false);
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "save_computer_snapshot output schema",
        present: ["project", "path", "client_id", "surface_id", "source_width", "source_height", "region", "width", "height", "mime_type", "file_bytes", "sha256", "saved"],
        absent: ["content_base64", "captured_at_unix_ms"]
    );
}

#[test]
fn computer_selection_has_one_exact_target_and_no_implicit_effects() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "control_computer");
    let args = json!({"action":"select","client_id":"runner","surface_id":"surface_test","element_id":"element_test"});
    test_support::validate_schema_instance(&args, &spec.input_schema).unwrap();
    let call = ToolCall::from_tool_name("control_computer", args.clone()).unwrap();
    let ToolCall::ComputerControl(control) = call else {
        panic!("selection control")
    };
    assert_eq!(control.action_name(), "select");
    assert_eq!(serde_json::to_value(control).unwrap(), args);
    for field in ["observe_after", "index", "label", "add", "toggle"] {
        let mut extra = args.clone();
        extra[field] = json!(true);
        assert!(ToolCall::from_tool_name("control_computer", extra.clone()).is_err());
        assert!(test_support::validate_schema_instance(&extra, &spec.input_schema).is_err());
    }
    for field in ["client_id", "surface_id", "element_id"] {
        let mut missing = args.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(ToolCall::from_tool_name("control_computer", missing).is_err());
    }
}

#[test]
fn computer_scroll_schema_matches_closed_parser_and_bounded_receipts() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "control_computer");
    for direction in ["up", "down", "left", "right"] {
        for amount in [
            None,
            Some(json!(null)),
            Some(json!("small")),
            Some(json!("large")),
        ] {
            let mut args = json!({"action":"scroll","client_id":"runner","surface_id":"surface_test","element_id":"element_test","direction":direction});
            if let Some(value) = amount {
                args["amount"] = value;
            }
            test_support::validate_schema_instance(&args, &spec.input_schema).unwrap();
            assert!(ToolCall::from_tool_name("control_computer", args).is_ok());
        }
    }
    for (field, value) in [
        ("direction", json!("diagonal")),
        ("amount", json!("pixels")),
        ("steps", json!(2)),
        ("observe_after", json!({"action":"accessibility_tree"})),
    ] {
        let mut args = json!({"action":"scroll","client_id":"runner","surface_id":"surface_test","element_id":"element_test","direction":"down"});
        args[field] = value;
        assert!(test_support::validate_schema_instance(&args, &spec.input_schema).is_err());
        assert!(ToolCall::from_tool_name("control_computer", args).is_err());
    }
    let mut result = json!({"success":true,"output":{"platform":"windows","surface_id":"surface_test","element_id":"element_test","direction":"down","amount":"small","position_before":0.0,"position_after":10.0,"success":true}});
    test_support::validate_schema_instance(&result, &spec.output_schema).unwrap();
    result["output"]["position_after"] = json!(101.0);
    assert!(test_support::validate_schema_instance(&result, &spec.output_schema).is_err());
}
