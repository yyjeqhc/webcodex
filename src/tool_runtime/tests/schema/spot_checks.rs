use super::*;

#[test]
fn tool_specs_git_log_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "git_log");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "git_log input schema",
        present: ["project", "limit", "skip", "session_id"]
    );
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output_props,
        "git_log output schema",
        present: ["project", "limit", "skip", "count", "truncated", "commits"]
    );
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_show_changes_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "show_changes");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "show_changes input schema",
        present: [
            "project",
            "session_id",
            "include_diff",
            "max_hunks",
            "max_hunk_lines",
            "session_event_limit",
        ]
    );
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output_props,
        "show_changes output schema",
        present: [
            "project",
            "branch",
            "head",
            "clean",
            "counts",
            "files",
            "diff_stat",
            "untracked_previews",
            "untracked_previews_truncated",
            "warnings",
            "suggested_next_actions",
            "session",
        ]
    );
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_structured_validation_schema_and_output() {
    let specs = registered_tool_specs();
    for name in ["cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        assert!(spec.input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("cwd"));
        let output_props = spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .unwrap();
        assert_schema_fields!(
            output_props,
            format!("{name} output schema"),
            present: ["exit_code", "duration_ms", "stdout_tail", "stderr_tail", "passed"]
        );
    }
    let go_props = spec_named(&specs, "go_test").input_schema["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        go_props,
        "go_test input schema",
        present: ["project", "cwd", "timeout_secs", "session_id"],
        absent: [
            "filter",
            "package",
            "features",
            "all_targets",
            "all_features",
            "no_default_features",
            "no_run",
            "env",
        ]
    );
}

#[test]
fn tool_specs_computer_find_elements_is_bounded_semantic_observation() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_find_elements");
    assert_eq!(
        required_fields(spec),
        vec!["client_id".to_string(), "surface_id".to_string()]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_find_elements input schema",
        present: ["client_id", "surface_id", "role", "subrole", "label", "focused", "enabled", "limit"]
    );
    assert_eq!(props["limit"]["minimum"], 1);
    assert_eq!(props["limit"]["maximum"], 32);
    assert!(props["label"]["description"]
        .as_str()
        .is_some_and(|description| description.contains("AXValue is never searched")));

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_find_elements output schema",
        present: ["platform", "surface_id", "observation_generation", "elements", "count", "scanned_nodes", "truncated"]
    );
    let element = &output["elements"]["items"];
    assert!(element["properties"].get("value").is_none());
    assert_eq!(output["elements"]["maxItems"], 32);
}

#[test]
fn tool_specs_computer_element_state_is_exact_read_only_normalized_state() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_element_state");
    assert_eq!(
        required_fields(spec),
        vec![
            "client_id".to_string(),
            "surface_id".to_string(),
            "element_id".to_string(),
        ]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_element_state input schema",
        present: ["client_id", "surface_id", "element_id"]
    );

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_element_state output schema",
        present: [
            "platform",
            "surface_id",
            "element_id",
            "observation_generation",
            "enabled",
            "focused",
            "protected",
            "value_empty",
            "can_press",
            "can_focus",
            "can_input_text"
        ],
        absent: ["value", "title", "description", "placeholder"]
    );
    assert_eq!(output["observation_generation"]["minimum"], 1);
}

#[test]
fn tool_specs_computer_snapshot_has_bounded_region_without_format_controls() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_snapshot");
    assert_eq!(
        required_fields(spec),
        vec!["client_id".to_string(), "surface_id".to_string()]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_snapshot input schema",
        present: ["client_id", "surface_id", "region", "max_width", "max_height"],
        absent: ["format", "quality", "save", "display_id"]
    );
    assert_eq!(props["max_width"]["maximum"], 4096);
    assert_eq!(props["max_height"]["maximum"], 4096);
    let region = props["region"]["properties"].as_object().unwrap();
    assert_schema_fields!(
        region,
        "computer_snapshot region schema",
        present: ["x", "y", "width", "height"]
    );
    assert_eq!(props["region"]["additionalProperties"], false);

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_snapshot output schema",
        present: [
            "surface",
            "source_width",
            "source_height",
            "region",
            "width",
            "height",
            "mime_type",
            "file_bytes",
            "sha256",
            "captured_at_unix_ms",
            "content_base64"
        ]
    );
}

#[test]
fn tool_specs_full_display_observation_is_closed_and_bounded() {
    let specs = registered_tool_specs();
    let list = spec_named(&specs, "computer_list_displays");
    assert_eq!(list.input_schema["additionalProperties"], false);
    assert_eq!(list.input_schema["properties"]["limit"]["maximum"], 16);
    let display = &list.output_schema["properties"]["output"]["properties"]["displays"]["items"];
    assert_eq!(display["additionalProperties"], false);
    assert_schema_fields!(
        display["properties"].as_object().unwrap(),
        "computer display item schema",
        present: ["display_id", "width", "height", "primary"],
        absent: ["native_identity", "device_path", "x", "y", "scale_factor"]
    );
    assert_eq!(
        list.output_schema["properties"]["output"]["additionalProperties"],
        false
    );

    let snapshot = spec_named(&specs, "computer_snapshot_display");
    let props = snapshot.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_snapshot_display input schema",
        present: ["client_id", "display_id", "max_width", "max_height"],
        absent: ["region", "x", "y", "global_x", "pointer", "click", "monitor_id"]
    );
    assert_eq!(props["max_width"]["maximum"], 4096);
    assert_eq!(props["max_height"]["maximum"], 4096);
    let output = snapshot.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_snapshot_display output schema",
        present: [
            "client_id",
            "display_id",
            "snapshot_generation",
            "source_width",
            "source_height",
            "width",
            "height",
            "mime_type",
            "file_bytes",
            "sha256",
            "captured_at_unix_ms",
            "content_base64"
        ],
        absent: ["native_identity", "device_path", "global_x", "global_y", "scale_factor", "region"]
    );
    assert_eq!(
        snapshot.output_schema["properties"]["output"]["additionalProperties"],
        false
    );
    assert_eq!(output["snapshot_generation"]["minimum"], 1);
}

#[test]
fn tool_specs_clipboard_text_is_strict_bounded_and_private() {
    let specs = registered_tool_specs();
    let read = spec_named(&specs, "computer_read_clipboard");
    assert_eq!(read.input_schema["additionalProperties"], false);
    assert_eq!(required_fields(read), vec!["client_id".to_string()]);
    let read_input = read.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        read_input,
        "clipboard read input schema",
        present: ["client_id"],
        absent: ["text", "surface_id", "format", "hwnd", "sequence", "clipboard_generation"]
    );
    assert_eq!(
        read.output_schema["properties"]["output"]["additionalProperties"],
        false
    );
    let read_output = read.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        read_output,
        "clipboard read output schema",
        present: ["platform", "available", "text", "text_bytes"],
        absent: ["format", "hwnd", "native_owner", "hglobal", "sequence", "sha256"]
    );
    assert_eq!(read_output["platform"]["enum"], json!(["windows", "macos"]));
    assert!(read.description.contains("NSPasteboardTypeString"));
    assert!(read.description.contains("CF_UNICODETEXT"));

    let write = spec_named(&specs, "computer_write_clipboard");
    assert_eq!(write.input_schema["additionalProperties"], false);
    assert_eq!(
        required_fields(write),
        vec!["client_id".to_string(), "text".to_string()]
    );
    let write_input = write.input_schema["properties"].as_object().unwrap();
    assert_eq!(write_input["text"]["minLength"], 1);
    assert_eq!(write_input["text"]["maxLength"], 16384);
    assert_schema_fields!(
        write_input,
        "clipboard write input schema",
        present: ["client_id", "text"],
        absent: ["surface_id", "element_id", "paste", "format", "mime_type", "hwnd", "restore", "append", "clipboard_generation"]
    );
    assert_eq!(
        write.output_schema["properties"]["output"]["additionalProperties"],
        false
    );
    let write_output = write.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        write_output,
        "clipboard write output schema",
        present: ["platform", "text_bytes", "success", "execution_state", "state_changed", "error_kind"],
        absent: ["text", "sha256", "hglobal", "hwnd", "native_owner", "sequence"]
    );
    assert_eq!(
        write_output["platform"]["enum"],
        json!(["windows", "macos"])
    );
    assert!(write.description.contains("NSPasteboardTypeString"));
    assert!(write.description.contains("CF_UNICODETEXT"));
}

#[test]
fn tool_specs_coordinate_pointer_is_snapshot_fenced_and_private() {
    let specs = registered_tool_specs();
    for name in ["computer_pointer_move", "computer_pointer_click"] {
        let spec = spec_named(&specs, name);
        assert_eq!(spec.input_schema["additionalProperties"], false);
        assert_eq!(
            required_fields(spec),
            vec![
                "client_id".to_string(),
                "display_id".to_string(),
                "snapshot_generation".to_string(),
                "x".to_string(),
                "y".to_string(),
            ]
        );
        let input = spec.input_schema["properties"].as_object().unwrap();
        assert_schema_fields!(
            input,
            "computer pointer input schema",
            present: ["client_id", "display_id", "snapshot_generation", "x", "y"],
            absent: ["global_x", "global_y", "button", "double_click", "region", "surface_id", "dpi", "monitor_id"]
        );
        assert_eq!(input["snapshot_generation"]["minimum"], 1);
        assert_eq!(input["x"]["minimum"], 0);
        assert_eq!(input["y"]["minimum"], 0);
        assert_eq!(
            spec.output_schema["properties"]["output"]["additionalProperties"],
            false
        );
        let output = spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .unwrap();
        assert_schema_fields!(
            output,
            "computer pointer output schema",
            present: ["platform", "display_id", "snapshot_generation", "x", "y", "success", "execution_state", "state_changed", "error_kind", "reconcile_with"],
            absent: ["native_identity", "device_path", "global_x", "global_y", "virtual_left", "virtual_top", "dpi", "scale_factor", "content_base64"]
        );
    }
}

#[test]
fn tool_specs_computer_save_snapshot_is_create_only_and_returns_metadata_only() {
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
        present: [
            "project",
            "path",
            "client_id",
            "surface_id",
            "region",
            "max_width",
            "max_height",
            "session_id"
        ],
        absent: ["overwrite", "format", "quality", "mime_type", "content_base64", "save"]
    );
    assert_eq!(props["region"]["additionalProperties"], false);

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_save_snapshot output schema",
        present: [
            "project",
            "path",
            "client_id",
            "surface_id",
            "source_width",
            "source_height",
            "region",
            "width",
            "height",
            "mime_type",
            "file_bytes",
            "sha256",
            "saved"
        ],
        absent: ["content_base64", "surface", "captured_at_unix_ms"]
    );
}

#[test]
fn tool_specs_computer_activate_window_is_exact_surface_only() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_activate_window");
    assert_eq!(
        required_fields(spec),
        vec!["client_id".to_string(), "surface_id".to_string()]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_activate_window input schema",
        present: ["client_id", "surface_id"]
    );
    for forbidden in [
        "application",
        "pid",
        "path",
        "bundle_id",
        "command",
        "action",
    ] {
        assert!(
            props.get(forbidden).is_none(),
            "forbidden activation field: {forbidden}"
        );
    }

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_activate_window output schema",
        present: ["platform", "surface_id", "success"]
    );
}

#[test]
fn tool_specs_computer_scroll_to_element_is_semantic_and_exact() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_scroll_to_element");
    assert_eq!(
        required_fields(spec),
        vec![
            "client_id".to_string(),
            "surface_id".to_string(),
            "element_id".to_string(),
        ]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_scroll_to_element input schema",
        present: ["client_id", "surface_id", "element_id"],
        absent: ["action", "delta", "distance", "x", "y", "wheel", "direction"]
    );
    assert_eq!(spec.input_schema["additionalProperties"], false);

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_scroll_to_element output schema",
        present: ["platform", "surface_id", "element_id", "success"],
        absent: ["action", "title", "value", "coordinates"]
    );
}

#[test]
fn tool_specs_computer_key_input_is_closed_and_exact() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "computer_key_input");
    assert_eq!(
        required_fields(spec),
        vec![
            "client_id".to_string(),
            "surface_id".to_string(),
            "key".to_string(),
        ]
    );
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "computer_key_input input schema",
        present: ["client_id", "surface_id", "key", "modifiers"],
        absent: ["element_id", "text", "keycode", "repeat", "held", "action", "x", "y"]
    );
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(props["modifiers"]["maxItems"], 4);
    assert_eq!(props["modifiers"]["uniqueItems"], true);
    assert_eq!(
        props["key"]["enum"],
        json!([
            "enter",
            "escape",
            "tab",
            "arrow_up",
            "arrow_down",
            "arrow_left",
            "arrow_right",
            "page_up",
            "page_down",
            "home",
            "end"
        ])
    );

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "computer_key_input output schema",
        present: ["platform", "surface_id", "key", "modifiers", "success"],
        absent: ["element_id", "text", "keycode", "repeat", "held", "title", "value"]
    );
}

#[test]
fn tool_specs_schema_spot_checks() {
    // Table-driven: (tool_name, required_fields, forbidden_fields).
    // Required fields are checked via exact equality to catch unexpected additions.
    let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
        (
            "apply_patch_checked",
            vec!["project", "patch"],
            vec!["deny_sensitive_paths"],
        ),
        (
            "validate_patch",
            vec!["project", "patch"],
            vec!["deny_sensitive_paths"],
        ),
        ("git_diff_summary", vec!["project"], vec![]),
        ("delete_project_files", vec!["project", "paths"], vec![]),
        ("git_restore_paths", vec!["project", "paths"], vec![]),
        ("discard_untracked", vec!["project", "paths"], vec![]),
        (
            "project_overview",
            vec!["project"],
            vec!["path", "max_depth", "limit"],
        ),
        ("list_project_files", vec!["project"], vec!["path", "limit"]),
        (
            "search_project_text",
            vec!["project", "pattern"],
            vec!["path", "limit", "context_before", "context_after"],
        ),
        (
            "read_file",
            vec!["project", "path"],
            vec!["with_line_numbers"],
        ),
        (
            "read_files",
            vec!["project", "items"],
            vec!["with_line_numbers"],
        ),
        ("list_jobs", vec![], vec![]),
        (
            "stop_job",
            vec!["project", "job_id"],
            vec!["confirm", "session_id"],
        ),
        (
            "job_status",
            vec!["job_id"],
            vec!["include_command_preview"],
        ),
        ("job_log", vec!["job_id"], vec![]),
    ];
    let specs = registered_tool_specs();
    for (name, expected_required, expected_forbidden) in &cases {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        let mut expected_sorted: Vec<String> =
            expected_required.iter().map(|s| s.to_string()).collect();
        expected_sorted.sort();
        let mut actual_sorted = required.clone();
        actual_sorted.sort();
        assert_eq!(
                actual_sorted, expected_sorted,
                "{name}: required fields mismatch (expected exactly {expected_sorted:?}, got {required:?})"
            );
        for field in expected_forbidden {
            assert!(
                !required.contains(&field.to_string()),
                "{name}: field '{field}' should not be required"
            );
        }
        assert!(
            spec.description.chars().count() <= 300,
            "{name}: description too long"
        );
    }

    // Extra property checks for tools with richer schemas.
    let spec = spec_named(&specs, "search_project_text");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("context_before"));
    assert!(props.contains_key("context_after"));

    let spec = spec_named(&specs, "job_status");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("include_command_preview"));

    let spec = spec_named(&specs, "read_file");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("with_line_numbers"));

    let spec = spec_named(&specs, "read_files");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("items"));
    assert!(props.contains_key("with_line_numbers"));
}
