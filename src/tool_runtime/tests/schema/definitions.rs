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
            definition.metadata().authority,
            metadata.authority,
            "{} authority mirror must match definition ToolMetadata",
            definition.name
        );
    }
}

#[test]
fn delete_files_remains_legacy_metadata_only_not_runtime_tool() {
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use crate::tool_runtime::tool_definition::lookup_tool_definition;

    assert!(
        lookup_tool_metadata("delete_files").is_some(),
        "delete_files legacy dedicated route metadata must remain explicit"
    );
    assert!(
        lookup_tool_definition("delete_files").is_none(),
        "delete_files must not become a ToolDefinition"
    );
    assert!(
        !is_known_tool_name("delete_files"),
        "delete_files must not become a known runtime tool"
    );
    assert!(
        ToolCall::from_tool_name(
            "delete_files",
            json!({"project": SAMPLE_PROJECT, "paths": []})
        )
        .is_err(),
        "delete_files must not be accepted by ToolCall"
    );
    assert!(
        !registered_tool_specs()
            .iter()
            .any(|spec| spec.name == "delete_files"),
        "delete_files must not become a public ToolSpec"
    );

    let openapi = crate::openapi::build_openapi_spec();
    let tool_description = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        [TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .expect("ToolCallRequest.tool description");
    assert!(
        !tool_description.contains("delete_files"),
        "callRuntimeTool accepted-name text must not advertise legacy delete_files"
    );
}

#[test]
fn git_diff_hunks_rejects_unknown_legacy_fields_compactly() {
    let error = ToolCall::from_tool_name(
        "git_diff_hunks",
        json!({
            "project": SAMPLE_PROJECT,
            "mode": "worktree",
            "max_lines_per_hunk": 80
        }),
    )
    .unwrap_err();
    assert!(error.contains("unknown field(s)"), "{error}");
    assert!(error.contains("mode"), "{error}");
    assert!(error.contains("max_lines_per_hunk"), "{error}");
    assert!(
        !error.contains("properties"),
        "must not dump JSON Schema: {error}"
    );
    assert!(
        !error.contains("additionalProperties"),
        "must stay compact: {error}"
    );
}

#[test]
fn tool_call_parser_name_gate_matches_tool_definitions() {
    use crate::tool_runtime::tool_definition::{model_hidden_tool_names, tool_definitions};

    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    assert_eq!(
        known_names, definition_names,
        "ToolCall parser accepted-name gate must match ToolDefinition names"
    );

    for name in &definition_names {
        let result = ToolCall::from_tool_name(name, Value::Null);
        if let Err(err) = result {
            assert!(
                !err.contains("unknown tool"),
                "{name} has a ToolDefinition but parser treated it as unknown: {err}"
            );
        }
    }

    let err = ToolCall::from_tool_name("__not_a_webcodex_tool__", Value::Null).unwrap_err();
    assert!(
        err.contains("unknown tool"),
        "unknown tool names must stay rejected by the parser gate: {err}"
    );
    assert!(
        ToolCall::from_tool_name(
            "delete_files",
            json!({"project": SAMPLE_PROJECT, "paths": []})
        )
        .is_err(),
        "delete_files must remain legacy route metadata only, not ToolCall parseable"
    );
    // A ToolDefinition may be `ModelHidden`: kernel-known and dispatchable only
    // through an adapter that explicitly projects it, or retained as compatibility
    // plumbing outside the ordinary model-facing registry. The set is intentionally
    // fixed and documented here so an accidental hide/exposure is caught. The
    // legacy single-purpose edit tools are no longer ToolDefinitions at all, so
    // they are absent here.
    let expected_hidden: BTreeSet<&str> = [
        "start_session",
        "start_coding_task",
        "job_tail",
        "skill_list",
        "skill_read_file",
        "skill_versions",
        "skill_install",
        "skill_activate",
        "skill_remove_revision",
        "memory_search",
        "memory_read",
        "memory_set",
        "memory_delete",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        model_hidden_tool_names().collect::<BTreeSet<_>>(),
        expected_hidden,
        "hidden ToolDefinitions must match the documented compatibility batch"
    );
}

#[test]
fn tool_definitions_match_agent_capability_dispatch_helper() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, tool_definitions,
    };

    for definition in tool_definitions() {
        // ModelHidden tools are dispatched for back-compat but have no
        // model-facing ToolSpec, so sample_tool_args (which reads the spec's
        // required fields) cannot build arguments for them. They are still
        // covered by the parser-name-gate test. Here we assert the full
        // dispatch-helper mirror only for model-visible tools.
        if !is_model_visible_tool_name(definition.name) {
            // Hidden tools have no spec, so we cannot synthesize valid args
            // from required fields. They are still explained by a ToolDefinition
            // and parser-known (covered by the parser-name-gate test); here we
            // only confirm the definition exists and resolves.
            assert!(
                lookup_tool_definition(definition.name).is_some(),
                "{} (hidden) must still resolve to a ToolDefinition",
                definition.name
            );
            continue;
        }
        let args = sample_tool_args(definition.name);
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
