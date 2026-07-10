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
        "intent",
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
        "intent",
        "available_intents",
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
        .compact_tool_manifest_payload_bounded(
            Some(vec![TOOL_CATEGORY_GIT.to_string()]),
            None,
            Some(2),
        )
        .unwrap();
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
    assert_eq!(list_tools["count"], 2);
    assert_eq!(list_tools["returned_count"], 2);
    assert_eq!(list_tools["filtered_count"], list_tools["total_count"]);
    assert!(list_tools["total_count"].as_u64().unwrap() > 2);
    assert!(!serde_json::to_string(&list_tools)
        .unwrap()
        .contains("ResponseTooLarge"));

    let manifest = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![TOOL_CATEGORY_SESSION.to_string()]),
            None,
            Some(2),
        )
        .unwrap();
    assert_eq!(manifest["truncated"], true);
    assert_eq!(manifest["truncation_reason"], "limit");
    assert_eq!(manifest["limit_applied"], true);
    assert_eq!(manifest["requested_limit"], 2);
    assert_eq!(manifest["count"], 2);
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

fn string_array(value: &Value, context: &str) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{context} must be an array"))
        .iter()
        .map(|member| {
            member
                .as_str()
                .unwrap_or_else(|| panic!("{context} member must be a string: {member:?}"))
                .to_string()
        })
        .collect()
}

fn string_set(value: &Value, context: &str) -> BTreeSet<String> {
    string_array(value, context).into_iter().collect()
}

fn category_member_sets(
    categories: &Value,
    context: &str,
) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    categories
        .as_object()
        .unwrap_or_else(|| panic!("{context} must be an object"))
        .iter()
        .map(|(category, members)| {
            (
                category.clone(),
                string_set(members, &format!("{context}.{category}")),
            )
        })
        .collect()
}

fn definition_category_member_sets() -> std::collections::BTreeMap<String, BTreeSet<String>> {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let mut categories = std::collections::BTreeMap::new();
    for definition in model_visible_tool_definitions() {
        categories
            .entry(definition.category.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(definition.name.to_string());
    }
    categories
}

fn tool_entry_names(tools: &Value, context: &str) -> BTreeSet<String> {
    tools
        .as_array()
        .unwrap_or_else(|| panic!("{context} must be an array"))
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{context} entry missing name: {tool:?}"))
                .to_string()
        })
        .collect()
}

fn assert_categories_hide_runtime_only_tools(
    categories: &std::collections::BTreeMap<String, BTreeSet<String>>,
    context: &str,
) {
    for forbidden in ["delete_files", "run_codex"] {
        assert!(
            categories
                .values()
                .all(|members| !members.contains(forbidden)),
            "{context} categories must not expose {forbidden}: {categories:?}"
        );
    }
}

fn assert_no_response_too_large(surface: &str, payload: &Value) {
    assert!(
        !serde_json::to_string(payload)
            .unwrap()
            .contains("ResponseTooLarge"),
        "{surface} bounded discovery must not surface ResponseTooLarge: {payload:?}"
    );
}

fn allowed_tool_definition_categories_for_discovery_group(group: &str) -> &'static [&'static str] {
    match group {
        "checkpoint" => &["checkpoint"],
        "cleanup" => &["checkpoint", "cleanup"],
        "edit" => &["artifact", "edit", "patch"],
        "git" => &["checkpoint", "cleanup", "file", "git"],
        "inspect" => &[
            "checkpoint",
            "file",
            "git",
            "lsp",
            "project",
            "runtime",
            "session",
            "workflow",
        ],
        "jobs" => &["job"],
        "patch" => &["patch"],
        "projects" => &["project"],
        "review" => &["checkpoint", "cleanup", "file", "git", "workflow"],
        "runtime" => &["checkpoint", "project", "runtime", "session", "workflow"],
        "shell" => &["job", "validation"],
        "validation" => &["patch", "validation"],
        other => panic!("missing discovery group category allowlist for {other}"),
    }
}

fn expected_cross_listed_discovery_groups(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "apply_patch_checked" => Some(&["edit", "patch", "validation"]),
        "cargo_check" => Some(&["shell", "validation"]),
        "cargo_fmt" => Some(&["shell", "validation"]),
        "cargo_test" => Some(&["shell", "validation"]),
        "discard_untracked" => Some(&["cleanup", "git"]),
        "finish_coding_task" => Some(&["review", "runtime"]),
        "git_diff" => Some(&["git", "inspect", "review"]),
        "git_diff_hunks" => Some(&["git", "inspect", "review"]),
        "git_diff_summary" => Some(&["git", "inspect", "review"]),
        "git_log" => Some(&["git", "inspect", "review"]),
        "git_restore_paths" => Some(&["cleanup", "git"]),
        "git_status" => Some(&["git", "inspect", "review"]),
        "list_agents" => Some(&["inspect", "runtime"]),
        "list_projects" => Some(&["inspect", "projects", "runtime"]),
        "list_tools" => Some(&["inspect", "runtime"]),
        "run_job" => Some(&["jobs", "shell"]),
        "runtime_status" => Some(&["inspect", "runtime"]),
        "show_changes" => Some(&["git", "inspect", "review"]),
        "start_coding_task" => Some(&["inspect", "runtime"]),
        "validate_patch" => Some(&["patch", "validation"]),
        "workspace_checkpoint_create" => Some(&["checkpoint", "git", "runtime"]),
        "workspace_checkpoint_delete" => Some(&["checkpoint", "cleanup", "runtime"]),
        "workspace_checkpoint_list" => Some(&["checkpoint", "inspect", "review", "runtime"]),
        "workspace_checkpoint_restore" => Some(&["checkpoint", "git", "runtime"]),
        "workspace_checkpoint_show" => Some(&["checkpoint", "inspect", "review", "runtime"]),
        _ => None,
    }
}

#[test]
fn tool_discovery_groups_drive_tool_categories() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, model_visible_tool_definitions,
        TOOL_DISCOVERY_GROUPS,
    };
    use std::collections::{BTreeMap, BTreeSet};

    let categories = registered_tool_categories();
    let category_map = categories.as_object().expect("categories object");
    let actual_category_names = category_map
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected_group_names = TOOL_DISCOVERY_GROUPS
        .iter()
        .map(|group| group.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_category_names, expected_group_names,
        "registered_tool_categories keys must come only from TOOL_DISCOVERY_GROUPS"
    );

    let mut memberships: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for group in TOOL_DISCOVERY_GROUPS {
        let actual_tools = string_array(
            category_map
                .get(group.name)
                .unwrap_or_else(|| panic!("{} discovery category missing", group.name)),
            group.name,
        );
        let tools = group
            .tools
            .iter()
            .map(|name| {
                assert_ne!(
                    *name, "run_codex",
                    "discovery group {} must not include removed runtime tool run_codex",
                    group.name
                );
                assert_ne!(
                    *name, "delete_files",
                    "discovery group {} must not include legacy route metadata delete_files",
                    group.name
                );
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
                assert!(
                    allowed_tool_definition_categories_for_discovery_group(group.name)
                        .contains(&definition.category),
                    "{} discovery group entry {} has ToolDefinition category {}, which is not in the explicit allowlist",
                    group.name,
                    name,
                    definition.category
                );
                memberships.entry(name).or_default().push(group.name);
                Value::String((*name).to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            category_map.get(group.name),
            Some(&Value::Array(tools)),
            "{} category must derive from ToolDefinition discovery groups",
            group.name
        );
        assert_eq!(
            actual_tools,
            group
                .tools
                .iter()
                .map(|tool| (*tool).to_string())
                .collect::<Vec<_>>(),
            "{} registered category order must match TOOL_DISCOVERY_GROUPS",
            group.name
        );
    }

    for definition in model_visible_tool_definitions() {
        let groups = memberships
            .get(definition.name)
            .unwrap_or_else(|| panic!("{} missing from discovery groups", definition.name));
        if groups.len() == 1 {
            continue;
        }
        let expected =
            expected_cross_listed_discovery_groups(definition.name).unwrap_or_else(|| {
                panic!(
                    "{} appears in multiple discovery groups without an explicit allowlist: {:?}",
                    definition.name, groups
                )
            });
        let actual_groups = groups.iter().copied().collect::<BTreeSet<_>>();
        let expected_groups = expected.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            actual_groups, expected_groups,
            "{} discovery cross-listing changed",
            definition.name
        );
    }

    for allowed in [
        "apply_patch_checked",
        "cargo_check",
        "cargo_fmt",
        "cargo_test",
        "discard_untracked",
        "finish_coding_task",
        "git_diff",
        "git_diff_hunks",
        "git_diff_summary",
        "git_log",
        "git_restore_paths",
        "git_status",
        "list_agents",
        "list_projects",
        "list_tools",
        "run_job",
        "runtime_status",
        "show_changes",
        "start_coding_task",
        "validate_patch",
        "workspace_checkpoint_create",
        "workspace_checkpoint_delete",
        "workspace_checkpoint_list",
        "workspace_checkpoint_restore",
        "workspace_checkpoint_show",
    ] {
        assert!(
            memberships
                .get(allowed)
                .is_some_and(|groups| groups.len() > 1),
            "{allowed} discovery cross-list allowlist must stay tied to an actual duplicate"
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
fn compact_tool_manifest_categories_match_bounded_list_tools_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let expected_count: usize = expected_categories
        .values()
        .map(|members| members.len())
        .sum();

    let manifest = runtime.compact_tool_manifest_payload();
    let manifest_categories = category_member_sets(&manifest["categories"], "tool_manifest");
    assert_eq!(
        manifest_categories, expected_categories,
        "compact tool_manifest categories must be grouped by ToolDefinition category"
    );
    assert_categories_hide_runtime_only_tools(&manifest_categories, "tool_manifest");

    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: None,
    });
    let list_categories = category_member_sets(&list_tools["categories"], "list_tools");
    assert_eq!(
        list_categories, manifest_categories,
        "bounded list_tools categories must match compact tool_manifest categories"
    );
    assert_categories_hide_runtime_only_tools(&list_categories, "list_tools");
    assert_eq!(manifest["tool_count"].as_u64(), Some(expected_count as u64));
    assert_eq!(
        manifest["returned_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(
        list_tools["total_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(
        list_tools["returned_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(list_tools["truncated"], false);
}

#[test]
fn tool_manifest_category_filter_matches_tool_definition_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let all_manifest_categories = category_member_sets(
        &runtime.compact_tool_manifest_payload()["categories"],
        "unfiltered tool_manifest",
    );

    for (category, expected_tools) in expected_categories {
        let manifest = runtime
            .compact_tool_manifest_payload_bounded(Some(vec![category.clone()]), None, None)
            .unwrap();
        assert_eq!(manifest["filtered"], true);
        assert_eq!(manifest["category"].as_str(), Some(category.as_str()));
        assert_eq!(
            string_array(&manifest["categories_requested"], "categories_requested"),
            vec![category.clone()]
        );
        assert_eq!(
            manifest["filtered_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            manifest["returned_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            manifest["count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(manifest["truncated"], false);
        assert_eq!(manifest["limit_applied"], false);
        assert!(manifest["total_count"].as_u64().unwrap() >= expected_tools.len() as u64);
        assert_no_response_too_large("tool_manifest", &manifest);

        let filtered_categories = category_member_sets(
            &manifest["categories"],
            &format!("tool_manifest filtered {category} categories"),
        );
        assert_eq!(
            filtered_categories, all_manifest_categories,
            "filtered compact tool_manifest currently preserves the full categories map"
        );
        assert_categories_hide_runtime_only_tools(&filtered_categories, "filtered tool_manifest");

        let returned_tools = tool_entry_names(
            &manifest["tools"],
            &format!("tool_manifest filtered {category} tools"),
        );
        assert_eq!(
            returned_tools, expected_tools,
            "tool_manifest category filter must return exactly the ToolDefinition category members"
        );
        for tool in manifest["tools"].as_array().expect("tool_manifest tools") {
            assert_eq!(
                tool["category"].as_str(),
                Some(category.as_str()),
                "filtered tool_manifest must not mix categories: {tool:?}"
            );
            assert_ne!(tool["name"].as_str(), Some("run_codex"));
            assert_ne!(tool["name"].as_str(), Some("delete_files"));
        }
    }
}

#[test]
fn list_tools_category_filter_matches_tool_definition_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let all_list_categories = category_member_sets(
        &runtime.list_tools_payload(ListToolsOptions {
            category: None,
            features: None,
            summary_only: true,
            limit: None,
        })["categories"],
        "unfiltered list_tools",
    );

    for (category, expected_tools) in expected_categories {
        let list_tools = runtime.list_tools_payload(ListToolsOptions {
            category: Some(category.clone()),
            features: None,
            summary_only: true,
            limit: None,
        });
        assert_eq!(list_tools["category"].as_str(), Some(category.as_str()));
        assert_eq!(list_tools["features"], Value::Null);
        assert_eq!(
            list_tools["filtered_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            list_tools["returned_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            list_tools["count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(list_tools["truncated"], false);
        assert_eq!(list_tools["limit_applied"], false);
        assert!(list_tools["total_count"].as_u64().unwrap() >= expected_tools.len() as u64);
        assert_no_response_too_large("list_tools", &list_tools);

        let filtered_categories = category_member_sets(
            &list_tools["categories"],
            &format!("list_tools filtered {category} categories"),
        );
        assert_eq!(
            filtered_categories, all_list_categories,
            "filtered list_tools currently preserves the full ToolDefinition category map"
        );
        assert_categories_hide_runtime_only_tools(&filtered_categories, "filtered list_tools");

        let names = string_set(
            &list_tools["names"],
            &format!("list_tools {category} names"),
        );
        assert_eq!(
            names, expected_tools,
            "list_tools category filter names must match ToolDefinition category members"
        );
        let returned_tools = tool_entry_names(
            &list_tools["tools"],
            &format!("list_tools filtered {category} tools"),
        );
        assert_eq!(
            returned_tools, expected_tools,
            "list_tools category filter tools must match ToolDefinition category members"
        );
        for tool in list_tools["tools"].as_array().expect("list_tools tools") {
            assert_eq!(
                tool["category"].as_str(),
                Some(category.as_str()),
                "filtered list_tools must not mix categories: {tool:?}"
            );
            assert_ne!(tool["name"].as_str(), Some("run_codex"));
            assert_ne!(tool["name"].as_str(), Some("delete_files"));
        }
    }
}

#[test]
fn tool_manifest_recommended_flows_reference_visible_defined_tools() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, TOOL_RECOMMENDED_FLOWS,
    };

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    let manifest_categories = category_member_sets(&manifest["categories"], "tool_manifest");
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
            assert!(
                manifest_categories
                    .values()
                    .any(|members| members.contains(*expected_tool)),
                "{} recommended flow references {expected_tool}, which is missing from compact manifest categories",
                expected.name
            );
            assert_ne!(
                *expected_tool, "run_codex",
                "{} recommended flow must not expose run_codex",
                expected.name
            );
            assert_ne!(
                *expected_tool, "delete_files",
                "{} recommended flow must not expose delete_files",
                expected.name
            );
        }
    }
}

#[tokio::test]
async fn tool_manifest_omits_recommended_flows_when_disabled() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("recommended_flows").is_none(),
        "include_recommended_flows=false currently omits recommended_flows: {:?}",
        result.output
    );
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

#[test]
fn tool_manifest_intents_reference_only_known_model_visible_tools() {
    use crate::tool_runtime::tool_definition::{
        available_tool_manifest_intent_names, is_known_tool_name, is_model_visible_tool_name,
        resolve_tool_manifest_intent, TOOL_MANIFEST_INTENTS,
    };
    use std::collections::BTreeSet;

    let expected = ["coding", "audit", "exploration", "release", "discovery"];
    let names: Vec<&str> = TOOL_MANIFEST_INTENTS
        .iter()
        .map(|intent| intent.name)
        .collect();
    assert_eq!(
        names, expected,
        "intent set must stay stable for this release"
    );
    assert_eq!(available_tool_manifest_intent_names(), names);
    for name in &names {
        let resolved = resolve_tool_manifest_intent(name)
            .unwrap_or_else(|unknown| panic!("available intent {unknown} must resolve"))
            .unwrap_or_else(|| panic!("available intent {name} must not resolve as empty"));
        assert_eq!(resolved.name, *name);
    }

    let mut seen = BTreeSet::new();
    for intent in TOOL_MANIFEST_INTENTS {
        assert!(!intent.tools.is_empty(), "{} must list tools", intent.name);
        assert!(
            seen.insert(intent.name),
            "duplicate tool_manifest intent {}",
            intent.name
        );
        for tool in intent.tools {
            assert!(
                is_known_tool_name(tool),
                "{} intent references unknown tool {}",
                intent.name,
                tool
            );
            assert!(
                is_model_visible_tool_name(tool),
                "{} intent references hidden tool {}",
                intent.name,
                tool
            );
            assert_ne!(
                *tool, "run_shell",
                "{} intent must not default to run_shell",
                intent.name
            );
            if matches!(intent.name, "audit" | "exploration" | "release") {
                assert_ne!(
                    *tool, "run_job",
                    "{} intent must not default to run_job",
                    intent.name
                );
            }
            if intent.name == "release" {
                assert_ne!(
                    *tool, "run_shell",
                    "release intent must not default to run_shell"
                );
                assert_ne!(
                    *tool, "run_job",
                    "release intent must not default to run_job"
                );
            }
        }
    }
}

#[tokio::test]
async fn tool_manifest_without_intent_keeps_compat_shape_and_lists_available_intents() {
    let runtime = test_runtime();
    let call = ToolCall::from_tool_name("tool_manifest", json!({})).unwrap();
    let result = runtime.dispatch(call).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["schema_version"], 1);
    assert_eq!(result.output["intent"], Value::Null);
    assert_eq!(result.output["filtered"], false);
    assert!(
        result.output["count"].as_u64().unwrap() > 20,
        "unfiltered tool_manifest should still return the broad compact tool set"
    );
    let available = string_array(&result.output["available_intents"], "available_intents");
    assert_eq!(
        available,
        vec![
            "coding".to_string(),
            "audit".to_string(),
            "exploration".to_string(),
            "release".to_string(),
            "discovery".to_string(),
        ]
    );
    assert_payload_keys_declared(
        "tool_manifest",
        &result.output,
        output_schema_properties(spec_named(&registered_tool_specs(), "tool_manifest")),
    );
}

#[tokio::test]
async fn tool_manifest_all_available_intents_parse_and_filter_through_tool_call() {
    let runtime = test_runtime();
    for intent in ["coding", "audit", "exploration", "release", "discovery"] {
        let call = ToolCall::from_tool_name(
            "tool_manifest",
            json!({
                "intent": intent,
                "include_recommended_flows": false,
                "include_risk_summary": false,
            }),
        )
        .unwrap_or_else(|error| panic!("{intent} must parse: {error}"));
        let result = runtime.dispatch(call).await;

        assert!(result.success, "{intent}: {:?}", result.error);
        assert_eq!(result.output["intent"], intent);
        assert_eq!(result.output["filtered"], true);
        assert!(
            result.output["returned_count"].as_u64().unwrap()
                < result.output["total_count"].as_u64().unwrap(),
            "{intent} must return a bounded manifest: {:?}",
            result.output
        );
    }
}

#[tokio::test]
async fn tool_manifest_intent_coding_returns_ranked_compact_tools() {
    use crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS;

    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "coding");
    assert_eq!(result.output["filtered"], true);
    assert_eq!(result.output["schema_version"], 1);

    let expected: Vec<&str> = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .unwrap()
        .tools
        .to_vec();
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names, expected,
        "coding intent tools must be ranked in table order"
    );
    assert_eq!(result.output["count"], expected.len() as u64);
    assert_eq!(result.output["filtered_count"], expected.len() as u64);
    assert!(
        result.output["total_count"].as_u64().unwrap() > expected.len() as u64,
        "total_count should remain the full runtime tool count"
    );
    assert!(result.output.get("recommended_flows").is_none());
    assert!(result.output["risk_summary"].is_object());
    for forbidden in ["run_shell", "run_job"] {
        assert!(
            !names.contains(&forbidden),
            "coding intent should not include {forbidden}"
        );
    }
}

#[tokio::test]
async fn tool_manifest_accepts_hyphenated_intent_alias() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: Some("Discovery".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "discovery");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "tool_manifest",
            "list_tools",
            "runtime_status",
            "list_agents",
            "list_projects",
            "project_overview",
        ]
    );
}

#[tokio::test]
async fn tool_manifest_unknown_intent_returns_structured_error() {
    let runtime = test_runtime();
    let call =
        ToolCall::from_tool_name("tool_manifest", json!({"intent": "not_a_real_intent"})).unwrap();
    let result = runtime.dispatch(call).await;
    assert!(!result.success, "unknown intent must fail");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("unknown tool_manifest intent"),
        "{:?}",
        result.error
    );
    assert_eq!(result.output["code"], "unknown_tool_manifest_intent");
    assert_eq!(result.output["intent"], "not_a_real_intent");
    let available = string_array(&result.output["available_intents"], "available_intents");
    assert!(available.contains(&"coding".to_string()));
    assert!(
        result.output.get("tools").is_none(),
        "unknown intent must not return a silent empty tool list: {:?}",
        result.output
    );
}

#[tokio::test]
async fn tool_manifest_intent_can_combine_with_category_filter() {
    // category is a strict ToolDefinition.category filter, not a flow/group filter.
    // validate_patch and apply_patch_checked remain patch-category tools, even
    // though they can participate in validation flows. Intent only ranks/filters
    // discovery output and does not change tool behavior.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: Some("validation".to_string()),
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "coding");
    assert_eq!(result.output["category"], "validation");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    // cargo_* are validation; validate_patch/apply_patch_checked are patch.
    assert_eq!(names, vec!["cargo_fmt", "cargo_check", "cargo_test"]);
}

#[tokio::test]
async fn audit_and_exploration_intents_exclude_shell_and_jobs() {
    for intent in ["audit", "exploration"] {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                category: None,
                intent: Some(intent.to_string()),
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{intent}: {:?}", result.error);
        assert_eq!(result.output["intent"], intent);
        assert_eq!(result.output["filtered"], true);
        assert!(
            result.output["returned_count"].as_u64().unwrap()
                < result.output["total_count"].as_u64().unwrap(),
            "{intent} must not return the full manifest"
        );
        let names: Vec<&str> = result.output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for forbidden in ["run_shell", "run_job"] {
            assert!(
                !names.contains(&forbidden),
                "{intent} must not include {forbidden}: {names:?}"
            );
        }
        if intent == "audit" {
            for required in [
                "start_coding_task",
                "project_overview",
                "read_file",
                "search_project_text",
                "list_project_files",
                "git_status",
                "git_diff_summary",
                "git_diff_hunks",
                "git_log",
                "show_changes",
                "workspace_hygiene_check",
                "session_handoff_summary",
                "finish_coding_task",
                "tool_manifest",
            ] {
                assert!(
                    names.contains(&required),
                    "audit intent must include {required}: {names:?}"
                );
            }
            for tool in result.output["tools"].as_array().unwrap() {
                assert_eq!(
                    tool["risk"], "read_only",
                    "audit intent must exclude project_write/job_run tools: {tool:?}"
                );
                assert_eq!(
                    tool["shell_like"], false,
                    "audit intent must exclude shell-like tools: {tool:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn release_intent_includes_list_jobs_but_not_run_shell_or_run_job() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: Some("release".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "release");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"project_overview"),
        "release intent must not include project_overview: {names:?}"
    );
    assert!(
        names.contains(&"list_jobs"),
        "release intent should keep read-only list_jobs: {names:?}"
    );
    for forbidden in ["run_shell", "run_job"] {
        assert!(
            !names.contains(&forbidden),
            "release intent must not include {forbidden}: {names:?}"
        );
    }
}

#[test]
fn project_overview_manifest_profiles_match_intended_workflows() {
    use crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS;

    for intent in ["coding", "audit", "exploration", "discovery"] {
        let profile = TOOL_MANIFEST_INTENTS
            .iter()
            .find(|profile| profile.name == intent)
            .unwrap_or_else(|| panic!("missing {intent} intent"));
        assert!(
            profile.tools.contains(&"project_overview"),
            "{intent} must include project_overview"
        );
    }
    let release = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|profile| profile.name == "release")
        .expect("release intent");
    assert!(!release.tools.contains(&"project_overview"));
}

#[test]
fn coding_intent_includes_line_edit_and_cleanup_tools() {
    use crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS;

    let coding = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .expect("coding intent");
    for required in [
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        "apply_text_edits",
        "apply_patch_checked",
        "git_status",
        "git_restore_paths",
        "discard_untracked",
        "finish_coding_task",
    ] {
        assert!(
            coding.tools.contains(&required),
            "coding intent must include {required}: {:?}",
            coding.tools
        );
    }
    for forbidden in ["run_shell", "run_job"] {
        assert!(
            !coding.tools.contains(&forbidden),
            "coding intent must exclude {forbidden}"
        );
    }
}

fn assert_recommended_flows_subset_of_manifest_tools(manifest: &Value, context: &str) {
    let tool_names: std::collections::BTreeSet<&str> = manifest["tools"]
        .as_array()
        .expect("manifest tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let flows = manifest["recommended_flows"]
        .as_array()
        .expect("recommended_flows");
    for flow in flows {
        let flow_name = flow["name"].as_str().unwrap_or("<unnamed>");
        let tools = flow["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{context}: flow {flow_name} tools"));
        assert!(
            !tools.is_empty(),
            "{context}: projected flow {flow_name} must not be empty"
        );
        for tool in tools {
            let tool = tool.as_str().expect("flow tool name");
            assert!(
                tool_names.contains(tool),
                "{context}: recommended_flows[{flow_name}] references invisible tool {tool}; visible={tool_names:?}"
            );
        }
    }
}

#[tokio::test]
async fn filtered_tool_manifest_recommended_flows_only_reference_returned_tools() {
    let runtime = test_runtime();

    // intent=coding
    let coding = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: Some("coding".to_string()),
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(coding.success, "{:?}", coding.error);
    assert_eq!(coding.output["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&coding.output, "intent=coding");
    let coding_names: Vec<&str> = coding.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        coding_names.contains(&"replace_line_range"),
        "coding intent tools should expose line edits: {coding_names:?}"
    );

    // single category filter
    let file_only = runtime
        .dispatch(ToolCall::ToolManifest {
            category: Some("file".to_string()),
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(file_only.success, "{:?}", file_only.error);
    assert_eq!(file_only.output["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&file_only.output, "category=file");

    // multi-category startup-style filter without patch
    let no_patch = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![
                "workflow".to_string(),
                "file".to_string(),
                "edit".to_string(),
                "validation".to_string(),
                "git".to_string(),
                "cleanup".to_string(),
            ]),
            Some("coding".to_string()),
            None,
        )
        .expect("startup-style no-patch manifest");
    assert_eq!(no_patch["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&no_patch, "startup no-patch");
    let no_patch_tools = serde_json::to_string(&no_patch["tools"]).unwrap();
    let no_patch_flows = serde_json::to_string(&no_patch["recommended_flows"]).unwrap();
    assert!(
        !no_patch_tools.contains("apply_patch_checked"),
        "without patch category, tools must not include apply_patch_checked"
    );
    assert!(
        !no_patch_flows.contains("apply_patch_checked"),
        "without patch category, recommended_flows must not include apply_patch_checked"
    );

    // same filter with patch
    let with_patch = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![
                "workflow".to_string(),
                "file".to_string(),
                "edit".to_string(),
                "patch".to_string(),
                "validation".to_string(),
                "git".to_string(),
                "cleanup".to_string(),
            ]),
            Some("coding".to_string()),
            None,
        )
        .expect("startup-style with-patch manifest");
    assert_recommended_flows_subset_of_manifest_tools(&with_patch, "startup with-patch");
    let with_patch_tools: Vec<&str> = with_patch["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        with_patch_tools.contains(&"apply_patch_checked"),
        "with patch category, tools should include apply_patch_checked: {with_patch_tools:?}"
    );
    let edit_flow = with_patch["recommended_flows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flow| flow["name"] == "edit")
        .expect("edit flow");
    assert!(
        edit_flow["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "apply_patch_checked"),
        "with patch category, edit flow may include apply_patch_checked: {edit_flow}"
    );

    // limit truncation after intent ordering
    let limited = runtime
        .compact_tool_manifest_payload_bounded(None, Some("coding".to_string()), Some(3))
        .expect("limit-truncated coding manifest");
    assert_eq!(limited["returned_count"], 3);
    assert_eq!(limited["truncated"], true);
    assert_recommended_flows_subset_of_manifest_tools(&limited, "intent=coding limit=3");
}

#[tokio::test]
async fn unfiltered_tool_manifest_keeps_full_recommended_flows() {
    use crate::tool_runtime::tool_definition::TOOL_RECOMMENDED_FLOWS;

    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["filtered"], false);
    let flows = result.output["recommended_flows"]
        .as_array()
        .expect("unfiltered recommended_flows");
    assert_eq!(
        flows.len(),
        TOOL_RECOMMENDED_FLOWS.len(),
        "unfiltered recommended_flows must keep full global set"
    );
    let serialized = result.output["recommended_flows"]
        .to_string()
        .to_lowercase();
    assert!(
        serialized.contains("run_shell")
            && serialized.contains("escape hatch")
            && serialized.contains("not the primary validation path"),
        "unfiltered flows must keep run_shell escape-hatch guidance: {serialized}"
    );
}
