use super::*;

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
