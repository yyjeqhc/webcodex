use super::*;

#[test]
fn missing_nested_kind_has_array_location_without_input_values() {
    let error = ToolCall::from_tool_name(
        "edit_project_files",
        json!({
            "project":"demo", "changes":[{"path":"private-file", "content":"secret-content"}]
        }),
    )
    .unwrap_err();
    assert!(
        error.contains("changes[0].kind: missing required field"),
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
fn buffered_enum_errors_never_label_an_existing_parent_field_as_missing() {
    let error = ToolCall::from_tool_name("edit_project_files", json!({
        "project":"demo", "changes":[{"kind":"edit","path":"file.rs", "expected_read_revision":1,
            "edits":[{"old_text":"private","new_text":"secret"}]}]
    })).unwrap_err();
    assert!(
        error.contains("changes[0]: nested object is missing required field kind"),
        "{error}"
    );
    assert!(!error.contains("changes[0].kind:"), "{error}");
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
