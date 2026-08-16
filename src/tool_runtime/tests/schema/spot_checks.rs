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
