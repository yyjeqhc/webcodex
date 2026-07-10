use super::*;

#[test]
fn tool_specs_schema_spot_checks() {
    // Table-driven: each entry is (tool_name, required_fields, forbidden_fields).
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
}

#[test]
fn tool_specs_git_log_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "git_log");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in ["project", "limit", "skip", "session_id"] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in ["project", "limit", "skip", "count", "truncated", "commits"] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_show_changes_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "show_changes");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "project",
        "session_id",
        "include_diff",
        "max_hunks",
        "max_hunk_lines",
        "session_event_limit",
    ] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
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
    ] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_cargo_tools_schema_and_output() {
    let specs = registered_tool_specs();
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        assert!(spec.input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("cwd"));
        for field in [
            "exit_code",
            "duration_ms",
            "stdout_tail",
            "stderr_tail",
            "passed",
        ] {
            assert!(
                spec.output_schema["properties"]["output"]["properties"]
                    .as_object()
                    .unwrap()
                    .contains_key(field),
                "{} missing output field {}",
                name,
                field
            );
        }
    }
}

#[test]
fn tool_specs_schema_spot_checks_extended() {
    // Table-driven: (tool_name, required_fields, forbidden_fields).
    // Required fields are checked via exact equality to catch unexpected additions.
    let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
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
        ("job_tail", vec!["job_id"], vec!["tail_lines"]),
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
}
