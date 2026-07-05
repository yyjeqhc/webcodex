use super::*;

#[test]
fn tool_definitions_cover_known_names_and_public_specs() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_hidden_tool_names, model_visible_tool_definitions,
        tool_definitions,
    };

    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let definition_order = tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    let hidden_names = model_hidden_tool_names().collect::<BTreeSet<_>>();
    let definition_hidden_names = tool_definitions()
        .filter(|definition| definition.visibility.is_model_hidden())
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    for name in known_tool_names() {
        assert!(
            lookup_tool_definition(name).is_some(),
            "{name} missing ToolDefinition lookup"
        );
    }
    assert_eq!(
        definition_names, known_names,
        "ToolDefinition mirror must cover every ToolCall name exactly"
    );
    assert_eq!(
        definition_order,
        known_tool_names().collect::<Vec<_>>(),
        "known-tool iterator must mirror canonical ToolDefinition order"
    );
    assert_eq!(
        hidden_names, definition_hidden_names,
        "hidden-name iterator must match ToolDefinition visibility"
    );

    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    let visible_definition_names = model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let spec_order = specs
        .iter()
        .map(|spec| spec.name.clone())
        .collect::<Vec<_>>();
    let visible_definition_order = model_visible_tool_definitions()
        .map(|definition| definition.name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        spec_names, visible_definition_names,
        "model-visible ToolDefinitions must match public ToolSpecs"
    );

    assert_eq!(
        visible_definition_order, spec_order,
        "canonical ToolDefinition order must preserve public ToolSpec order"
    );
    assert_eq!(
        registered_tool_names(),
        visible_definition_order,
        "public tool_names must derive from canonical model-visible ToolDefinition order"
    );
}

#[test]
fn tool_definitions_drive_metadata_visibility_and_categories() {
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use crate::tool_runtime::tool_definition::tool_definitions;

    for definition in tool_definitions() {
        let metadata = definition.metadata();
        let facade_metadata = lookup_tool_metadata(definition.name)
            .copied()
            .unwrap_or_else(|| panic!("{} missing metadata facade entry", definition.name));
        assert_eq!(
            metadata, facade_metadata,
            "{} metadata facade must return ToolDefinition metadata",
            definition.name
        );
        assert_eq!(metadata.name, definition.name);
        assert_eq!(
            definition.visibility.is_model_hidden(),
            is_model_hidden_tool_name(definition.name),
            "{} visibility mirror must match model-hidden filter",
            definition.name
        );
        assert_eq!(
            definition.category,
            tool_manifest_category(definition.name),
            "{} category mirror must match tool_manifest",
            definition.name
        );
        assert_eq!(
            definition.metadata().oauth_scope,
            metadata.oauth_scope,
            "{} OAuth scope mirror must match definition ToolMetadata",
            definition.name
        );
    }
}

#[test]
fn tool_definitions_match_agent_capability_dispatch_helper() {
    use crate::tool_runtime::tool_definition::tool_definitions;

    for definition in tool_definitions() {
        let args = if definition.name == "run_codex" {
            json!({
                "project": SAMPLE_PROJECT,
                "prompt": "summarize",
            })
        } else {
            sample_tool_args(definition.name)
        };
        let call = ToolCall::from_tool_name(definition.name, args)
            .unwrap_or_else(|e| panic!("{} should deserialize: {e}", definition.name));
        assert_eq!(
            call.tool_name(),
            definition.name,
            "{} ToolCall::tool_name() mirror must match definition",
            definition.name
        );
        assert_eq!(
            required_agent_capability(&call),
            definition.agent_capability,
            "{} agent capability mirror must match dispatch helper",
            definition.name
        );
        assert_eq!(
            call.project().is_some(),
            definition.metadata().requires_project,
            "{} project accessor must match metadata.requires_project",
            definition.name
        );
    }
}
