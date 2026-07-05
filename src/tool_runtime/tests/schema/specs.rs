use super::*;

#[test]
fn tool_specs_and_metadata_are_synchronized() {
    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();

    for spec in &specs {
        assert!(
            crate::tool_runtime::metadata::lookup_tool_metadata(&spec.name).is_some(),
            "{} missing metadata",
            spec.name
        );
    }

    for metadata in crate::tool_runtime::metadata::iter_tool_metadata() {
        if metadata.name == "delete_files" {
            // Legacy dedicated HTTP route metadata; not a public runtime
            // ToolSpec name and intentionally not accepted by ToolCall.
            continue;
        }
        if is_model_hidden_tool_name(metadata.name) {
            // Hidden implemented tools keep parser/metadata coverage without
            // being advertised through model-facing specs.
            continue;
        }
        assert!(
            spec_names.contains(metadata.name),
            "{} metadata is not exposed by registry specs",
            metadata.name
        );
    }
}

#[test]
fn tool_specs_names_are_unique() {
    let specs = registered_tool_specs();
    let mut names = specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    names.sort();
    let mut deduped = names.clone();
    deduped.dedup();
    assert_eq!(names, deduped, "tool names must be unique");
}

#[test]
fn tool_specs_names_are_snake_case() {
    for spec in registered_tool_specs() {
        assert!(
            !spec.name.contains('-'),
            "tool name '{}' should use snake_case (no hyphens)",
            spec.name
        );
        assert!(
            spec.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "tool name '{}' should be snake_case",
            spec.name
        );
    }
}

#[test]
fn tool_specs_derive_contract_fields_from_name() {
    for spec in registered_tool_specs() {
        assert_eq!(
            spec.output_schema,
            crate::tool_runtime::registry::output_schema_for_tool(&spec.name),
            "{} output schema must derive from ToolSpec.name",
            spec.name
        );
        assert_eq!(
            spec.annotations,
            crate::tool_runtime::registry::tool_annotations(&spec.name),
            "{} MCP annotations must derive from ToolSpec.name",
            spec.name
        );
    }
}

#[test]
fn tool_specs_input_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.input_schema;
        assert_eq!(
            schema["type"].as_str(),
            Some("object"),
            "tool '{}' input schema must be type object",
            spec.name
        );
        assert!(
            schema["properties"].is_object(),
            "tool '{}' input schema must have properties object",
            spec.name
        );
        assert!(
            schema["required"].is_array(),
            "tool '{}' input schema must have required array",
            spec.name
        );
    }
}

#[test]
fn tool_specs_expose_common_testing_metadata() {
    use crate::tool_runtime::sessions::TOOL_CALL_EXPECTATION_METADATA_FIELDS;

    for spec in registered_tool_specs() {
        let props = spec.input_schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} input schema properties", spec.name));
        for &field in TOOL_CALL_EXPECTATION_METADATA_FIELDS {
            assert!(
                props.contains_key(field),
                "{} input schema should expose common testing metadata field {field}",
                spec.name
            );
            let desc = props[field]["description"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            assert!(
                desc.contains("testing") || desc.contains("smoke"),
                "{}.{field} should be documented as testing/smoke metadata: {desc}",
                spec.name
            );
            assert!(
                desc.contains("does not change"),
                "{}.{field} should document that behavior is unchanged: {desc}",
                spec.name
            );
        }
    }
}

#[test]
fn tool_specs_output_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.output_schema;
        assert_eq!(
            schema["type"].as_str(),
            Some("object"),
            "tool '{}' output schema must be type object",
            spec.name
        );
        assert!(
            schema["properties"].is_object(),
            "tool '{}' output schema must have properties object",
            spec.name
        );
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|v| v == "success")),
            "tool '{}' output schema must require success",
            spec.name
        );
    }
}

#[test]
fn tool_specs_required_fields_match_deserialization() {
    // For every tool spec, building arguments with only the required
    // fields must deserialize successfully, and omitting any required
    // field must fail.
    for spec in registered_tool_specs() {
        let required: Vec<String> = spec.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        // Build a minimal valid args object using a placeholder for each
        // required field based on its declared type.
        let mut minimal = serde_json::Map::new();
        let properties = spec.input_schema["properties"].as_object().unwrap();
        for field in &required {
            let prop = &properties[field.as_str()];
            let placeholder = placeholder_from_prop(prop);
            minimal.insert(field.clone(), placeholder);
        }
        let args = Value::Object(minimal);
        ToolCall::from_tool_name(&spec.name, args)
            .unwrap_or_else(|e| panic!("tool '{}' minimal args failed: {}", spec.name, e));

        // Omitting each required field should fail.
        for field in &required {
            let mut partial = serde_json::Map::new();
            for f in &required {
                if f != field {
                    let prop = &properties[f.as_str()];
                    let placeholder = placeholder_from_prop(prop);
                    partial.insert(f.clone(), placeholder);
                }
            }
            let err = ToolCall::from_tool_name(&spec.name, Value::Object(partial))
                .err()
                .unwrap_or_else(|| {
                    panic!(
                        "tool '{}' should reject missing required field '{}'",
                        spec.name, field
                    )
                });
            assert!(
                err.contains(field),
                "tool '{}' error for missing '{}' should mention field: {}",
                spec.name,
                field,
                err
            );
        }
    }
}

#[test]
fn tool_specs_optional_fields_are_not_required() {
    // Optional fields (e.g. timeout_secs, cwd) must not appear in required.
    let specs = registered_tool_specs();
    let run_shell = specs.iter().find(|s| s.name == "run_shell").unwrap();
    let required: Vec<String> = run_shell.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"command".to_string()));
    assert!(!required.contains(&"timeout_secs".to_string()));
    assert!(!required.contains(&"cwd".to_string()));

    let read_file = specs.iter().find(|s| s.name == "read_file").unwrap();
    let required = required_fields(read_file);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"path".to_string()));
    assert!(!required.contains(&"with_line_numbers".to_string()));

    let search_project_text = specs
        .iter()
        .find(|s| s.name == "search_project_text")
        .unwrap();
    let required = required_fields(search_project_text);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"pattern".to_string()));
    assert!(!required.contains(&"context_before".to_string()));
    assert!(!required.contains(&"context_after".to_string()));
}

#[test]
fn tool_specs_covers_expected_tool_set() {
    let names = registered_tool_names();
    assert_eq!(
        names.len(),
        66,
        "model-facing runtime/MCP tool count should be 66 after exposing stop_job"
    );
    for expected in [
        "list_tools",
        "list_projects",
        "list_agents",
        "runtime_status",
        "run_shell",
        "run_job",
        "stop_job",
        "job_status",
        "job_log",
        "read_file",
        "git_status",
        "git_diff",
        "git_diff_summary",
        "git_diff_hunks",
        "git_log",
        "show_changes",
        "workspace_hygiene_check",
        "workspace_checkpoint_create",
        "workspace_checkpoint_list",
        "workspace_checkpoint_show",
        "workspace_checkpoint_restore",
        "workspace_checkpoint_delete",
        "apply_patch",
        "apply_patch_checked",
        "validate_patch",
        "delete_project_files",
        "git_restore_paths",
        "discard_untracked",
        // Phase A: read-only console tools
        "list_project_files",
        "search_project_text",
        "list_jobs",
        "job_tail",
        // Phase 4: file edit tools
        "replace_in_file",
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "write_project_file",
        "save_project_artifact",
        "read_project_artifact_metadata",
        "read_project_artifact",
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        // Project management
        "register_project",
        "create_project",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "expected tool '{}' in specs: {:?}",
            expected,
            names
        );
    }
    assert!(
        !names.iter().any(|n| n == "run_codex"),
        "run_codex must stay hidden from model-facing tool_specs: {:?}",
        names
    );
}

#[test]
fn tool_specs_descriptions_fit_gpt_action_limit() {
    for spec in registered_tool_specs() {
        assert!(
            spec.description.chars().count() <= 300,
            "{} description is too long: {} chars",
            spec.name,
            spec.description.chars().count()
        );
    }
}
