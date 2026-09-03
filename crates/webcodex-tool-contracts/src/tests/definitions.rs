use super::*;

#[test]
fn tool_definitions_cover_known_names_and_public_specs() {
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
    assert_eq!(definition_names, known_names);
    assert_eq!(definition_order, known_tool_names().collect::<Vec<_>>());
    assert_eq!(hidden_names, definition_hidden_names);

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
    assert_eq!(spec_names, visible_definition_names);
    assert_eq!(visible_definition_order, spec_order);
    assert_eq!(registered_tool_names(), visible_definition_order);
}

#[test]
fn adaptive_runtime_direct_declarations_are_visible_ranked_and_unique() {
    let mut seen_ranks = std::collections::BTreeMap::new();
    for definition in tool_definitions() {
        let Some(rank) = definition.adaptive_runtime_direct_rank() else {
            continue;
        };
        assert!(definition.visibility.is_model_visible());
        assert!(seen_ranks.insert(rank, definition.name).is_none());
    }

    let derived = adaptive_runtime_direct_tool_definitions();
    assert!(!derived.is_empty());
    for pair in derived.windows(2) {
        assert!(pair[0].adaptive_runtime_direct_rank() < pair[1].adaptive_runtime_direct_rank());
    }
    assert_eq!(derived.len(), seen_ranks.len());
    let apply_patch = derived
        .iter()
        .find(|definition| definition.name == "apply_patch")
        .expect("apply_patch must be adaptive-direct");
    let apply_text_edits = derived
        .iter()
        .find(|definition| definition.name == "apply_text_edits")
        .expect("apply_text_edits must be adaptive-direct");
    assert!(
        apply_patch.adaptive_runtime_direct_rank()
            < apply_text_edits.adaptive_runtime_direct_rank()
    );
}

#[test]
fn tool_definitions_drive_metadata_visibility_and_categories() {
    for definition in tool_definitions() {
        let metadata = definition.metadata();
        let facade_metadata = lookup_tool_metadata(definition.name)
            .copied()
            .unwrap_or_else(|| panic!("{} missing metadata facade entry", definition.name));
        assert_eq!(metadata, facade_metadata);
        assert_eq!(metadata.name, definition.name);
        assert_eq!(
            definition.visibility.is_model_hidden(),
            is_model_hidden_tool_name(definition.name)
        );
        assert_eq!(definition.category, runtime_tool_category(definition.name));
        assert_eq!(definition.metadata().authority, metadata.authority);
    }
}
