use super::*;

#[test]
fn missing_nested_edits_has_array_location_without_input_values() {
    let error = ToolCall::from_tool_name(
        "edit_project_files",
        json!({
            "project":"demo", "changes":[{"path":"private-file", "content":"secret-content"}]
        }),
    )
    .unwrap_err();
    assert!(
        error.contains("changes[0].edits: missing required field"),
        "{error}"
    );
    assert!(!error.contains("private-file"));
    assert!(!error.contains("secret-content"));
}

#[test]
fn nested_object_and_array_type_errors_have_safe_paths() {
    for (tool, arguments, path) in [
        (
            "search_file_context",
            json!({"project":"demo","queries":[{"pattern":"x","limit":"secret-token"}]}),
            "queries[0].limit",
        ),
        (
            "search_file_context",
            json!({"project":"demo","query":{"pattern":"x","limit":"secret-token"}}),
            "query.limit",
        ),
        (
            "read_files",
            json!({"project":"demo","items":[{"path":"private-file","start_line":"secret-token"}]}),
            "items[0].start_line",
        ),
    ] {
        let error = ToolCall::from_tool_name(tool, arguments).unwrap_err();
        assert!(error.contains(path), "{error}");
        assert!(!error.contains("secret-token"));
        assert!(!error.contains("private-file"));
    }
}

#[test]
fn unknown_enum_values_and_object_keys_are_never_echoed() {
    for arguments in [
        json!({"project":"demo","queries":[{"pattern":"x","pattern_mode":"secret-token"}]}),
        json!({"project":"demo","queries":[{"pattern":"x","private/path/secret-token":true}]}),
    ] {
        let error = ToolCall::from_tool_name("search_file_context", arguments).unwrap_err();
        assert!(error.contains("queries[0]"), "{error}");
        assert!(!error.contains("secret-token"));
        assert!(!error.contains("private/path"));
    }
}

#[test]
fn required_top_level_field_remains_actionable() {
    let error = ToolCall::from_tool_name("find_definition", json!({})).unwrap_err();
    assert!(error.contains("project: missing required field"), "{error}");
}

#[test]
fn edit_defaults_and_revision_type_errors_keep_exact_safe_field_paths() {
    ToolCall::from_tool_name("edit_project_files", json!({
        "project":"demo", "changes":[{"path":"file.rs", "edits":[{"old_text":"old","new_text":"new"}]}]
    })).unwrap();
    let error = ToolCall::from_tool_name("edit_project_files", json!({
        "project":"demo", "changes":[{"path":"private-file", "expected_read_revision":"secret-token", "edits":[{"old_text":"old","new_text":"new"}]}]
    })).unwrap_err();
    assert!(
        error.contains("changes[0].expected_read_revision"),
        "{error}"
    );
    assert!(!error.contains("secret-token"));
    assert!(!error.contains("private-file"));
}

#[test]
fn context_search_selection_is_validated_before_execution_with_legacy_support() {
    for arguments in [
        json!({"project":"demo"}),
        json!({"project":"demo","queries":[]}),
        json!({"project":"demo","query":{"pattern":"x"},"queries":[{"pattern":"y"}]}),
    ] {
        assert!(ToolCall::from_tool_name("search_file_context", arguments)
            .unwrap_err()
            .contains("queries"));
    }
    for arguments in [
        json!({"project":"demo","query":{"pattern":"x"}}),
        json!({"project":"demo","queries":[{"pattern":"x"}]}),
    ] {
        ToolCall::from_tool_name("search_file_context", arguments).unwrap();
    }
}
