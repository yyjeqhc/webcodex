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
    assert!(
        spec.description.chars().count() <= crate::tool_runtime::MODEL_TOOL_DESCRIPTION_MAX_CHARS
    );
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
            "diff_review_handoff",
            "untracked_previews",
            "untracked_previews_truncated",
            "warnings",
            "suggested_next_actions",
            "session",
        ]
    );
    assert!(
        spec.description.chars().count() <= crate::tool_runtime::MODEL_TOOL_DESCRIPTION_MAX_CHARS
    );
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
    let cargo_test = spec_named(&specs, "cargo_test");
    let cargo_test_input = cargo_test.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        cargo_test_input,
        "cargo_test input schema",
        present: ["require_tests", "min_tests", "no_run"]
    );
    assert_eq!(cargo_test_input["min_tests"]["minimum"], 1);
    assert_eq!(
        cargo_test_input["min_tests"]["maximum"],
        crate::shell_protocol::CARGO_TEST_MIN_TESTS_MAX
    );
    for valid in [
        serde_json::json!({"project": "agent:demo:repo"}),
        serde_json::json!({"project": "agent:demo:repo", "no_run": true}),
        serde_json::json!({"project": "agent:demo:repo", "require_tests": true}),
        serde_json::json!({"project": "agent:demo:repo", "require_tests": false, "min_tests": 6}),
    ] {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &valid,
            &cargo_test.input_schema,
        )
        .unwrap_or_else(|error| panic!("valid cargo_test input rejected: {valid}: {error}"));
    }
    for invalid in [
        serde_json::json!({"project": "agent:demo:repo", "min_tests": 0}),
        serde_json::json!({"project": "agent:demo:repo", "min_tests": -1}),
        serde_json::json!({"project": "agent:demo:repo", "min_tests": 1.5}),
        serde_json::json!({"project": "agent:demo:repo", "min_tests": crate::shell_protocol::CARGO_TEST_MIN_TESTS_MAX + 1}),
        serde_json::json!({"project": "agent:demo:repo", "no_run": true, "require_tests": true}),
        serde_json::json!({"project": "agent:demo:repo", "no_run": true, "min_tests": 1}),
    ] {
        assert!(
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                &invalid,
                &cargo_test.input_schema
            )
            .is_err(),
            "invalid cargo_test input passed schema: {invalid}"
        );
    }
    let cargo_test_output = cargo_test.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(cargo_test_output.contains_key("test_count_assertion"));
    assert_eq!(
        cargo_test_output["test_count_assertion"]["properties"]["reason_code"]["enum"],
        serde_json::json!([
            "minimum_satisfied",
            "minimum_not_met",
            "test_count_unproven"
        ])
    );
    assert_eq!(
        cargo_test_output["test_count_assertion"]["properties"]["evidence_reason_code"]["enum"],
        serde_json::json!([
            "complete_summary",
            "output_truncated",
            "partial_harness_summary",
            "no_complete_summary"
        ])
    );
    for name in ["job_status", "job_log"] {
        let output = &spec_named(&specs, name).output_schema["properties"]["output"]["properties"];
        assert_eq!(
            output["validation"]["properties"]["test_count_assertion"]["properties"]["reason_code"]
                ["enum"],
            serde_json::json!([
                "minimum_satisfied",
                "minimum_not_met",
                "test_count_unproven"
            ]),
            "{name} must expose the same durable assertion projection"
        );
    }
    let openapi = crate::openapi::build_openapi_spec();
    let flattened = &openapi["components"]["schemas"]["ToolCallRequest"]["properties"];
    assert_eq!(flattened["require_tests"]["type"], "boolean");
    assert_eq!(flattened["min_tests"]["type"], "integer");
    assert_eq!(flattened["min_tests"]["minimum"], 1);
    assert_eq!(
        flattened["min_tests"]["maximum"],
        crate::shell_protocol::CARGO_TEST_MIN_TESTS_MAX
    );
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
fn tool_specs_schema_spot_checks() {
    let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
        (
            "apply_patch",
            vec!["project", "patch"],
            vec!["dry_run", "strict_matching", "session_id"],
        ),
        (
            "apply_unified_diff",
            vec!["project", "diff"],
            vec!["deny_sensitive_paths", "session_id"],
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
        assert_eq!(actual_sorted, expected_sorted, "{name}: required fields mismatch (expected exactly {expected_sorted:?}, got {required:?})");
        for field in expected_forbidden {
            assert!(
                !required.contains(&field.to_string()),
                "{name}: field '{field}' should not be required"
            );
        }
        assert!(
            spec.description.chars().count()
                <= crate::tool_runtime::MODEL_TOOL_DESCRIPTION_MAX_CHARS,
            "{name}: description too long"
        );
    }

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
