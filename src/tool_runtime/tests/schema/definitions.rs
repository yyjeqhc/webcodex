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
fn model_visible_tool_definitions_have_public_tool_specs() {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let spec_names = registered_tool_specs()
        .iter()
        .map(|spec| spec.name.clone())
        .collect::<BTreeSet<_>>();
    for definition in model_visible_tool_definitions() {
        assert!(
            spec_names.contains(definition.name),
            "{} is model-visible but missing a public ToolSpec row",
            definition.name
        );
    }
}

#[test]
fn public_tool_specs_are_model_visible_tool_definitions() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_visible_tool_definitions,
    };

    let visible_definition_names = model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    for spec in registered_tool_specs() {
        let definition = lookup_tool_definition(&spec.name)
            .unwrap_or_else(|| panic!("{} public ToolSpec is missing ToolDefinition", spec.name));
        assert!(
            visible_definition_names.contains(definition.name),
            "{} public ToolSpec must be model-visible in ToolDefinition",
            spec.name
        );
    }
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
fn hidden_run_codex_surface_contract_is_explicit() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_hidden_tool_names, tool_definitions,
    };

    assert_eq!(tool_definitions().count(), 67, "ToolDefinition count");
    assert!(
        lookup_tool_definition("run_codex").is_some(),
        "run_codex must keep an explicit hidden ToolDefinition"
    );
    assert_eq!(
        model_hidden_tool_names().collect::<Vec<_>>(),
        vec!["run_codex"],
        "run_codex must remain the only model-hidden ToolDefinition"
    );

    let model_visible_names = registered_tool_names();
    assert_eq!(model_visible_names.len(), 66, "tools.count");
    assert!(
        !model_visible_names.iter().any(|name| name == "run_codex"),
        "tools.count/model-facing names must not include run_codex"
    );

    let openapi = crate::openapi::build_openapi_spec();
    let tool_description = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        [TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .expect("ToolCallRequest.tool description");
    assert!(
        !tool_description.contains("run_codex"),
        "callRuntimeTool accepted-name text must not advertise run_codex"
    );
    let operation_ids = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|methods| methods.as_object().unwrap().values())
        .map(|operation| operation["operationId"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        !operation_ids
            .iter()
            .any(|operation_id| operation_id.contains("runCodex")
                || operation_id.contains("RunCodex")),
        "run_codex must not gain a dedicated OpenAPI operation: {operation_ids:?}"
    );
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
    assert_eq!(
        model_hidden_tool_names().collect::<Vec<_>>(),
        vec!["run_codex"],
        "run_codex hidden parser-known behavior must stay explicit"
    );
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
