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
        assert!(
            schema.is_object(),
            "tool '{}' must have an input_schema object",
            spec.name
        );
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
        assert_eq!(
            schema["additionalProperties"], false,
            "tool '{}' input schema must reject undeclared top-level properties",
            spec.name
        );
    }
}

#[test]
fn search_project_text_schema_declares_bounded_advanced_inputs() {
    let specs = registered_tool_specs();
    let search = specs
        .iter()
        .find(|spec| spec.name == "search_project_text")
        .expect("search_project_text spec");
    let properties = search.input_schema["properties"].as_object().unwrap();

    for field in [
        "include_globs",
        "exclude_globs",
        "result_mode",
        "timeout_secs",
    ] {
        assert!(properties.contains_key(field), "missing {field}");
    }
    for field in ["include_globs", "exclude_globs"] {
        assert_eq!(properties[field]["type"], "array");
        assert_eq!(properties[field]["maxItems"], 32);
        assert_eq!(properties[field]["items"]["type"], "string");
        assert_eq!(properties[field]["items"]["minLength"], 1);
        assert_eq!(properties[field]["items"]["maxLength"], 256);
    }
    // Timeout is integer without minimum/maximum so strict clients can send
    // out-of-range values that the server clamps to 1..120.
    assert!(properties["timeout_secs"].get("minimum").is_none());
    assert!(properties["timeout_secs"].get("maximum").is_none());
    assert_eq!(properties["timeout_secs"]["type"], "integer");
    assert_eq!(properties["timeout_secs"]["default"], 30);
    let timeout_desc = properties["timeout_secs"]["description"]
        .as_str()
        .unwrap_or("");
    assert!(
        timeout_desc.to_ascii_lowercase().contains("clamp"),
        "timeout_secs description should document server clamp: {timeout_desc}"
    );
    assert_eq!(
        properties["result_mode"]["enum"],
        json!(["matches", "files_with_matches", "count"])
    );
}

#[test]
fn sync_validation_and_run_shell_timeout_schema_bounds() {
    let specs = registered_tool_specs();
    for (name, default) in [
        ("cargo_fmt", 120),
        ("cargo_check", 120),
        ("cargo_test", 120),
        ("run_shell", 60),
    ] {
        let spec = specs.iter().find(|s| s.name == name).expect(name);
        let timeout = &spec.input_schema["properties"]["timeout_secs"];
        assert_eq!(timeout["type"], "integer", "{name}");
        assert_eq!(timeout["minimum"], 1, "{name}");
        assert_eq!(timeout["maximum"], 120, "{name}");
        assert_eq!(timeout["default"], default, "{name}");
        let desc = timeout["description"].as_str().unwrap_or("");
        assert!(
            desc.contains("120") && desc.to_ascii_lowercase().contains("reject"),
            "{name} timeout description should document reject contract: {desc}"
        );
    }

    // search_project_text keeps its own 1..120 clamp semantics without schema max.
    let search = specs
        .iter()
        .find(|s| s.name == "search_project_text")
        .expect("search_project_text");
    assert!(search.input_schema["properties"]["timeout_secs"]
        .get("maximum")
        .is_none());
}

#[test]
fn tool_specs_input_schema_fields_are_declared_and_safe() {
    for spec in registered_tool_specs() {
        let properties = input_schema_properties(&spec);
        let required = spec.input_schema["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{} input schema required array", spec.name));
        let mut seen_required = BTreeSet::new();

        for field in required {
            let field = field
                .as_str()
                .unwrap_or_else(|| panic!("{} required entries must be strings", spec.name));
            assert!(
                properties.contains_key(field),
                "{} required field '{}' must be declared in input_schema.properties",
                spec.name,
                field
            );
            assert!(
                seen_required.insert(field),
                "{} required field '{}' must not be duplicated",
                spec.name,
                field
            );
        }

        assert_schema_property_names_are_safe(&spec.name, &spec.input_schema, "input_schema");
    }
}

#[test]
fn mcp_tools_list_exposes_registered_tool_spec_input_schemas() {
    let mcp_source = include_str!("../../../mcp.rs");
    assert!(
        mcp_source.contains("\"tools\": registered_tool_specs()"),
        "MCP tools/list should expose registered ToolSpec rows directly so inputSchema stays in parity"
    );

    let specs = registered_tool_specs();
    let payload = json!({ "tools": &specs });
    let tools = payload["tools"].as_array().expect("MCP tools/list tools");
    assert_eq!(
        tools.len(),
        specs.len(),
        "MCP tools/list count must match registered ToolSpec count"
    );

    for (tool, spec) in tools.iter().zip(specs.iter()) {
        assert_eq!(
            tool["name"].as_str(),
            Some(spec.name.as_str()),
            "MCP tool order/name must match registered ToolSpec"
        );
        assert_eq!(
            tool["inputSchema"]["required"], spec.input_schema["required"],
            "{} MCP inputSchema required fields must match ToolSpec input_schema",
            spec.name
        );
        assert_eq!(
            tool["inputSchema"]["additionalProperties"], spec.input_schema["additionalProperties"],
            "{} MCP inputSchema additionalProperties must match ToolSpec input_schema",
            spec.name
        );

        let mcp_property_names = tool["inputSchema"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} MCP inputSchema properties", spec.name))
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let spec_property_names = input_schema_properties(spec)
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            mcp_property_names, spec_property_names,
            "{} MCP inputSchema property names must match ToolSpec input_schema",
            spec.name
        );
    }

    let mcp_names = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("MCP tool name"))
        .collect::<BTreeSet<_>>();
    assert!(
        !mcp_names.contains("run_codex"),
        "MCP tools/list must not expose removed run_codex"
    );
    assert!(
        !mcp_names.contains("delete_files"),
        "MCP tools/list must not expose legacy delete_files metadata"
    );
}

#[test]
fn tool_specs_required_fields_match_declared_properties() {
    for spec in registered_tool_specs() {
        let properties = input_schema_properties(&spec);
        let required = spec.input_schema["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{} input schema required array", spec.name));

        for field in required {
            let field = field
                .as_str()
                .unwrap_or_else(|| panic!("{} required entry must be string", spec.name));
            assert!(
                properties.contains_key(field),
                "{} required field '{}' must exist in properties",
                spec.name,
                field
            );
        }
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
    assert_eq!(
        run_shell.input_schema["properties"]["timeout_secs"]["minimum"],
        1
    );
    assert_eq!(
        run_shell.input_schema["properties"]["timeout_secs"]["maximum"],
        120
    );
    assert_eq!(
        run_shell.input_schema["properties"]["timeout_secs"]["default"],
        60
    );

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
        "project_overview",
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
        "run_codex must stay removed from registered model-facing tool specs: {:?}",
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

const SENSITIVE_INPUT_FIELD_NAMES: &[&str] = &[
    "token",
    "secret",
    "env",
    "environment",
    "credential",
    "password",
];

fn input_schema_properties(spec: &ToolSpec) -> &serde_json::Map<String, Value> {
    spec.input_schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} input schema properties object", spec.name))
}

fn assert_schema_property_names_are_safe(tool_name: &str, schema: &Value, path: &str) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, property_schema) in properties {
            assert!(
                !field.is_empty(),
                "{tool_name} {path} property names must be non-empty"
            );
            assert_ne!(
                field, TOOL_CALL_TOOL_FIELD,
                "{tool_name} {path}.{field} must not expose the generic callRuntimeTool wrapper field"
            );
            let lower = field.to_ascii_lowercase();
            assert!(
                !SENSITIVE_INPUT_FIELD_NAMES.contains(&lower.as_str()),
                "{tool_name} {path}.{field} looks like a sensitive input field; stop and review before exposing it"
            );
            if field.starts_with("test_")
                || matches!(
                    field.as_str(),
                    "expected_failure" | "expected_failure_kind" | "assertion_name"
                )
            {
                assert!(
                    crate::tool_runtime::sessions::TOOL_CALL_EXPECTATION_METADATA_FIELDS
                        .contains(&field.as_str()),
                    "{tool_name} {path}.{field} must use the shared testing metadata field allowlist"
                );
            }

            let nested_path = format!("{path}.properties.{field}");
            assert_schema_property_names_are_safe(tool_name, property_schema, &nested_path);
        }
    }

    if let Some(items) = schema.get("items") {
        let nested_path = format!("{path}.items");
        assert_schema_property_names_are_safe(tool_name, items, &nested_path);
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(schemas) = schema.get(key).and_then(Value::as_array) {
            for (idx, nested) in schemas.iter().enumerate() {
                let nested_path = format!("{path}.{key}[{idx}]");
                assert_schema_property_names_are_safe(tool_name, nested, &nested_path);
            }
        }
    }
}
