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
        "returned_count",
        "total_count",
        "filtered_count",
        "limit_applied",
        "requested_limit",
        "truncation_reason",
        "truncated",
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
        "returned_count",
        "total_count",
        "limit_applied",
        "requested_limit",
        "truncation_reason",
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

#[test]
fn tool_manifest_and_list_tools_limit_truncation_reports_limit_reason() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_SESSION;

    let runtime = test_runtime();
    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: Some(2),
    });
    assert_eq!(list_tools["truncated"], true);
    assert_eq!(list_tools["truncation_reason"], "limit");
    assert_eq!(list_tools["limit_applied"], true);
    assert_eq!(list_tools["requested_limit"], 2);
    assert_eq!(list_tools["returned_count"], 2);
    assert!(list_tools["total_count"].as_u64().unwrap() > 2);
    assert!(!serde_json::to_string(&list_tools)
        .unwrap()
        .contains("ResponseTooLarge"));

    let manifest = runtime.compact_tool_manifest_payload_bounded(
        Some(vec![TOOL_CATEGORY_SESSION.to_string()]),
        Some(2),
    );
    assert_eq!(manifest["truncated"], true);
    assert_eq!(manifest["truncation_reason"], "limit");
    assert_eq!(manifest["limit_applied"], true);
    assert_eq!(manifest["requested_limit"], 2);
    assert_eq!(manifest["returned_count"], 2);
    assert!(manifest["filtered_count"].as_u64().unwrap() > 2);
    assert!(
        manifest["total_count"].as_u64().unwrap() >= manifest["filtered_count"].as_u64().unwrap()
    );
    assert!(!serde_json::to_string(&manifest)
        .unwrap()
        .contains("ResponseTooLarge"));
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
fn tool_manifest_categories_cover_every_model_visible_definition() {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    assert_eq!(
        manifest["tool_count"],
        registered_tool_specs().len() as i64,
        "tool_manifest tool_count must mirror model-facing ToolSpec count"
    );
    let categories = manifest["categories"]
        .as_object()
        .expect("tool_manifest categories");

    for definition in model_visible_tool_definitions() {
        let members = categories
            .get(definition.category)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("missing tool_manifest category {}", definition.category));
        assert!(
            members.iter().any(|member| member == definition.name),
            "{} ToolDefinition category {} must include the tool in tool_manifest",
            definition.name,
            definition.category
        );
    }
}

#[test]
fn tool_manifest_compact_categories_match_single_tool_definition_category() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_visible_tool_definitions,
    };
    use std::collections::BTreeMap;

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    let categories = manifest["categories"]
        .as_object()
        .expect("tool_manifest categories");
    let visible_names = model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let mut memberships: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (category, members) in categories {
        for member in members
            .as_array()
            .unwrap_or_else(|| panic!("{category} members must be an array"))
        {
            let name = member
                .as_str()
                .unwrap_or_else(|| panic!("{category} member must be a string"));
            let definition = lookup_tool_definition(name)
                .unwrap_or_else(|| panic!("{category} member {name} missing ToolDefinition"));
            assert!(
                definition.visibility.is_model_visible(),
                "{category} member {name} must be model-visible"
            );
            assert_eq!(
                definition.category, category,
                "{name} compact manifest category must match ToolDefinition category"
            );
            memberships
                .entry(name.to_string())
                .or_default()
                .push(category.clone());
        }
    }

    assert_eq!(
        memberships.len(),
        visible_names.len(),
        "compact tool_manifest categories must cover every model-visible tool exactly once"
    );
    for definition in model_visible_tool_definitions() {
        let member_categories = memberships
            .get(definition.name)
            .unwrap_or_else(|| panic!("{} missing compact manifest category", definition.name));
        assert_eq!(
            member_categories,
            &vec![definition.category.to_string()],
            "{} must have exactly one compact manifest category",
            definition.name
        );
    }
    for forbidden in ["delete_files", "run_codex"] {
        assert!(
            !memberships.contains_key(forbidden),
            "{forbidden} must not appear in model-facing tool_manifest categories"
        );
    }

    let tools = manifest["tools"].as_array().expect("tool_manifest tools");
    assert_eq!(
        tools.len(),
        visible_names.len(),
        "unfiltered compact tool_manifest must list every model-visible tool"
    );
    for tool in tools {
        let name = tool["name"]
            .as_str()
            .expect("tool_manifest tool name must be a string");
        let definition = lookup_tool_definition(name)
            .unwrap_or_else(|| panic!("{name} compact manifest entry missing ToolDefinition"));
        assert!(
            visible_names.contains(name),
            "{name} compact manifest entry must be model-visible"
        );
        assert_eq!(
            tool["category"].as_str(),
            Some(definition.category),
            "{name} compact manifest entry category must match ToolDefinition"
        );
    }
}

#[test]
fn tool_manifest_recommended_flows_reference_visible_defined_tools() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, TOOL_RECOMMENDED_FLOWS,
    };

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    let flows = manifest["recommended_flows"]
        .as_array()
        .expect("tool_manifest recommended_flows");
    assert_eq!(flows.len(), TOOL_RECOMMENDED_FLOWS.len());

    for (actual, expected) in flows.iter().zip(TOOL_RECOMMENDED_FLOWS) {
        assert_eq!(actual["name"], expected.name);
        assert_eq!(actual["purpose"], expected.manifest_purpose);
        let tools = actual["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{} recommended flow tools", expected.name));
        assert_eq!(tools.len(), expected.tools.len());
        for (actual_tool, expected_tool) in tools.iter().zip(expected.tools) {
            assert_eq!(actual_tool, expected_tool);
            let definition = lookup_tool_definition(expected_tool).unwrap_or_else(|| {
                panic!(
                    "{} recommended flow references unknown tool {expected_tool}",
                    expected.name
                )
            });
            assert!(
                definition.visibility.is_model_visible(),
                "{} recommended flow references hidden tool {expected_tool}",
                expected.name
            );
            assert!(
                is_model_visible_tool_name(expected_tool),
                "{} recommended flow references non-visible tool {expected_tool}",
                expected.name
            );
        }
    }
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
