use super::*;

#[test]
fn list_tools_schema_exposes_bounded_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "list_tools");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in ["category", "features", "summary_only", "limit"] {
        assert!(
            props.contains_key(field),
            "list_tools input schema must expose {field}"
        );
    }
    assert!(spec.input_schema["required"].as_array().unwrap().is_empty());
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "category",
        "features",
        "limit",
        "categories",
        "recommended_flows",
    ] {
        assert!(
            output.contains_key(field),
            "list_tools output schema must expose {field}"
        );
    }
}

#[test]
fn tool_manifest_schema_exposes_compact_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "tool_manifest");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "category",
        "include_recommended_flows",
        "include_risk_summary",
    ] {
        assert!(
            props.contains_key(field),
            "tool_manifest input schema must expose {field}"
        );
    }
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "schema_version",
        "count",
        "tool_count",
        "filtered_count",
        "category",
        "filtered",
        "categories_requested",
        "limit",
        "truncated",
        "categories",
        "tools",
        "risk_summary",
        "recommended_flows",
    ] {
        assert!(
            output.contains_key(field),
            "tool_manifest output schema must expose {field}"
        );
    }
}

#[test]
fn discovery_output_schemas_cover_runtime_payload_keys() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_GIT;

    let runtime = test_runtime();
    let specs = registered_tool_specs();

    let list_tools_spec = spec_named(&specs, "list_tools");
    let list_tools_payload = runtime.list_tools_payload(ListToolsOptions {
        category: Some(TOOL_CATEGORY_GIT.to_string()),
        features: Some("read".to_string()),
        summary_only: true,
        limit: Some(3),
    });
    assert_payload_keys_declared(
        "list_tools",
        &list_tools_payload,
        output_schema_properties(list_tools_spec),
    );

    let tool_manifest_spec = spec_named(&specs, "tool_manifest");
    let tool_manifest_payload = runtime
        .compact_tool_manifest_payload_bounded(Some(vec![TOOL_CATEGORY_GIT.to_string()]), Some(2));
    assert_payload_keys_declared(
        "tool_manifest",
        &tool_manifest_payload,
        output_schema_properties(tool_manifest_spec),
    );
}

fn output_schema_properties(spec: &ToolSpec) -> &serde_json::Map<String, Value> {
    spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} output schema properties", spec.name))
}

fn assert_payload_keys_declared(
    tool_name: &str,
    payload: &Value,
    output_schema_properties: &serde_json::Map<String, Value>,
) {
    let payload = payload
        .as_object()
        .unwrap_or_else(|| panic!("{tool_name} payload object"));
    for key in payload.keys() {
        assert!(
            output_schema_properties.contains_key(key),
            "{tool_name} runtime output key {key} is missing from output_schema properties"
        );
    }
}

#[test]
fn tool_discovery_groups_drive_tool_categories() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, TOOL_DISCOVERY_GROUPS,
    };

    let categories = registered_tool_categories();
    let category_map = categories.as_object().expect("categories object");

    for group in TOOL_DISCOVERY_GROUPS {
        let tools = group
            .tools
            .iter()
            .map(|name| {
                let definition = lookup_tool_definition(name)
                    .unwrap_or_else(|| panic!("{name} discovery group entry missing definition"));
                assert!(
                    definition.visibility.is_model_visible(),
                    "{name} discovery group entry must be model-visible"
                );
                assert!(
                    is_model_visible_tool_name(name),
                    "{name} discovery group entry must pass visibility facade"
                );
                Value::String((*name).to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            category_map.get(group.name),
            Some(&Value::Array(tools)),
            "{} category must derive from ToolDefinition discovery groups",
            group.name
        );
    }
}

#[test]
fn tool_recommended_flows_reference_visible_defined_tools() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, TOOL_RECOMMENDED_FLOWS,
    };

    let expected_summaries = TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| {
            assert!(
                !flow.name.trim().is_empty(),
                "recommended flow name must be present"
            );
            assert!(
                !flow.manifest_purpose.trim().is_empty(),
                "{} recommended flow purpose must be present",
                flow.name
            );
            assert!(
                flow.summary.chars().count() <= 300,
                "{} recommended flow summary is too long",
                flow.name
            );
            assert!(
                !flow.tools.is_empty(),
                "{} recommended flow must list tools",
                flow.name
            );
            for tool in flow.tools {
                let definition = lookup_tool_definition(tool).unwrap_or_else(|| {
                    panic!(
                        "{} recommended flow references unknown tool {tool}",
                        flow.name
                    )
                });
                assert!(
                    definition.visibility.is_model_visible(),
                    "{} recommended flow references hidden tool {tool}",
                    flow.name
                );
                assert!(
                    is_model_visible_tool_name(tool),
                    "{} recommended flow references non-visible tool {tool}",
                    flow.name
                );
            }
            flow.summary
        })
        .collect::<Vec<_>>();
    assert_eq!(recommended_flows(), expected_summaries);
}

#[test]
fn tool_categories_and_recommended_flows_are_well_formed() {
    use crate::tool_runtime::tool_definition::{
        TOOL_DISCOVERY_GROUP_CHECKPOINT, TOOL_DISCOVERY_GROUP_CLEANUP, TOOL_DISCOVERY_GROUP_EDIT,
        TOOL_DISCOVERY_GROUP_GIT, TOOL_DISCOVERY_GROUP_INSPECT, TOOL_DISCOVERY_GROUP_JOBS,
        TOOL_DISCOVERY_GROUP_PATCH, TOOL_DISCOVERY_GROUP_REVIEW, TOOL_DISCOVERY_GROUP_RUNTIME,
        TOOL_DISCOVERY_GROUP_SHELL, TOOL_DISCOVERY_GROUP_VALIDATION,
    };

    let categories = registered_tool_categories();
    // Every declared category is a non-empty array of known tool names.
    let names = registered_tool_names();
    for (cat, members) in categories.as_object().unwrap() {
        let arr = members.as_array().unwrap();
        assert!(!arr.is_empty(), "category '{}' must not be empty", cat);
        for m in arr {
            let name = m.as_str().unwrap();
            assert!(
                names.iter().any(|n| n == name),
                "category '{}' lists unknown tool '{}'",
                cat,
                name
            );
        }
    }
    // Each expected category is present.
    for cat in [
        TOOL_DISCOVERY_GROUP_INSPECT,
        TOOL_DISCOVERY_GROUP_GIT,
        TOOL_DISCOVERY_GROUP_REVIEW,
        TOOL_DISCOVERY_GROUP_VALIDATION,
        TOOL_DISCOVERY_GROUP_PATCH,
        TOOL_DISCOVERY_GROUP_SHELL,
        TOOL_DISCOVERY_GROUP_JOBS,
        TOOL_DISCOVERY_GROUP_RUNTIME,
        TOOL_DISCOVERY_GROUP_CLEANUP,
        TOOL_DISCOVERY_GROUP_CHECKPOINT,
    ] {
        assert!(
            categories.as_object().unwrap().contains_key(cat),
            "missing category {}",
            cat
        );
    }
    let validation = categories[TOOL_DISCOVERY_GROUP_VALIDATION]
        .as_array()
        .unwrap();
    for name in [
        "cargo_fmt",
        "cargo_check",
        "cargo_test",
        "validate_patch",
        "apply_patch_checked",
    ] {
        assert!(validation.iter().any(|v| v == name));
    }
    let review = categories[TOOL_DISCOVERY_GROUP_REVIEW].as_array().unwrap();
    assert!(review.iter().any(|v| v == "git_diff_hunks"));
    assert!(review.iter().any(|v| v == "workspace_hygiene_check"));
    assert!(review.iter().any(|v| v == "git_log"));
    let inspect = categories[TOOL_DISCOVERY_GROUP_INSPECT].as_array().unwrap();
    for name in ["read_file", "search_project_text", "show_changes"] {
        assert!(
            inspect.iter().any(|v| v == name),
            "inspect category should include default inspect tool {name}"
        );
    }
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT].as_array().unwrap();
    let edit_prefix: Vec<&str> = edit
        .iter()
        .take(5)
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        edit_prefix,
        vec![
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
        ],
        "preferred edit tools should lead the edit category"
    );
    // recommended_flows are short and non-empty.
    let flows = recommended_flows();
    assert!(!flows.is_empty());
    for flow in &flows {
        assert!(flow.chars().count() <= 300, "flow too long: {}", flow);
    }
    let joined_flows = flows.join("\n").to_lowercase();
    for phrase in [
        "inspect: use read_file, search_project_text, and show_changes before editing",
        "edit: prefer replace_line_range / insert_at_line / delete_line_range",
        "apply_text_edits for batches",
        "apply_patch_checked for broad diffs",
        "validate: use cargo_check / cargo_test / validate_patch",
        "raw run_shell is a bounded escape hatch",
        "not the primary editing or validation path",
        "review: use show_changes / git_diff_hunks / workspace_hygiene_check",
        "handoff: use session_summary / session_handoff_summary",
    ] {
        assert!(
            joined_flows.contains(phrase),
            "recommended flows should mention {phrase}: {joined_flows}"
        );
    }
}

#[test]
fn tool_categories_include_edit_group() {
    use crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUP_EDIT;

    let cats = registered_tool_categories();
    let edit = cats[TOOL_DISCOVERY_GROUP_EDIT]
        .as_array()
        .expect("edit category present");
    assert!(edit.iter().any(|v| v == "replace_in_file"));
    assert!(edit.iter().any(|v| v == "write_project_file"));
    assert!(edit.iter().any(|v| v == "replace_line_range"));
    assert!(edit.iter().any(|v| v == "insert_at_line"));
    assert!(edit.iter().any(|v| v == "delete_line_range"));
    assert!(edit.iter().any(|v| v == "apply_text_edits"));
}

#[test]
fn tool_categories_include_projects_with_management_tools() {
    use crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUP_PROJECTS;

    let cats = registered_tool_categories();
    let projects = cats[TOOL_DISCOVERY_GROUP_PROJECTS]
        .as_array()
        .expect("projects category present");
    assert!(
        projects.iter().any(|v| v == "register_project"),
        "projects category must include register_project"
    );
    assert!(
        projects.iter().any(|v| v == "create_project"),
        "projects category must include create_project"
    );
}
