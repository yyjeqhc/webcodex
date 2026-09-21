use super::*;

use crate::tool_runtime::metadata::{
    ToolAuthorityPolicy, ToolPathHint, ToolRisk, TOOL_PROVIDER_UNKNOWN,
};

#[test]
fn tool_definition_explains_all_tool_call_runtime_names() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, tool_definitions,
    };

    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    assert_eq!(
        definition_names, known_names,
        "Every ToolCall-reachable runtime name must be explained by ToolDefinition"
    );

    for name in known_tool_names() {
        // ModelHidden tools have no model-facing ToolSpec, so sample_tool_args
        // (which reads a spec's required fields) cannot build args for them.
        // They still must be parser-known: confirm the name is accepted (the
        // only allowed failure is a missing-field argument error, never
        // "unknown tool"). Visible tools get full arg validation.
        if !is_model_visible_tool_name(name) {
            match ToolCall::from_tool_name(name, Value::Null) {
                Ok(_) => {}
                Err(err) => assert!(
                    !err.contains("unknown tool"),
                    "{name} (hidden) must be parser-known, not unknown: {err}"
                ),
            }
            assert!(
                lookup_tool_definition(name).is_some(),
                "{name} (hidden) must resolve to a ToolDefinition"
            );
            continue;
        }
        let args = if name == "run_codex" {
            json!({"project": SAMPLE_PROJECT, "prompt": "summarize"})
        } else {
            sample_tool_args(name)
        };
        let call = ToolCall::from_tool_name(name, args)
            .unwrap_or_else(|err| panic!("{name} should parse through ToolDefinition: {err}"));
        assert_eq!(call.tool_name(), name);
        assert!(
            lookup_tool_definition(call.tool_name()).is_some(),
            "{} ToolCall::tool_name must resolve to ToolDefinition",
            call.tool_name()
        );
    }
}

#[test]
fn plugin_tool_call_parser_is_typed_bounded_and_closed() {
    let list = ToolCall::from_tool_name("plugin_tool", json!({"action":"list"})).unwrap();
    assert_eq!(list.tool_name(), "plugin_tool");
    assert!(matches!(list, ToolCall::PluginTool(_)));
    for invalid in [
        json!({"action":"list","unknown":true}),
        json!({"action":"list","plugin":"repo-tools"}),
        json!({"action":"call","binding":"wc_pbind_bad","arguments":{}}),
        json!({"action":"call","binding":"wc_pbind_AAAAAAAAAAAAAAAAAAAAAA"}),
        json!({"action":"describe","runner":"runner-a","plugin":"repo-tools"}),
    ] {
        assert!(
            ToolCall::from_tool_name("plugin_tool", invalid).is_err(),
            "invalid Plugin gateway arguments must fail closed"
        );
    }
    let call = ToolCall::from_tool_name(
        "plugin_tool",
        json!({
            "action":"call",
            "binding":"wc_pbind_ASNFZ4mrze8BI0VniavN7w",
            "arguments":{}
        }),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::PluginTool(_)));
}

#[test]
fn tool_definition_metadata_fallback_facade_is_unknown_only() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, tool_metadata};
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, runtime_tool_category,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_metadata, runtime_tool_permission_risk, runtime_tool_requires_permission,
        runtime_tool_session_risk_class, PERMISSION_RISK_WRITE,
    };

    for name in [
        "delete_files",
        "__unknown_non_runtime__",
        "__unknown_tool_for_metadata_test__",
        "not_a_tool",
    ] {
        let unknown = tool_metadata(name);
        assert!(lookup_tool_metadata(name).is_none(), "{name}");
        assert!(lookup_tool_definition(name).is_none(), "{name}");
        assert!(!is_known_tool_name(name), "{name}");
        assert!(!is_model_visible_tool_name(name), "{name}");
        assert_eq!(unknown.name, "<unknown>", "{name}");
        assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN, "{name}");
        assert_eq!(unknown.risk, ToolRisk::Unknown, "{name}");
        assert_eq!(unknown.authority, ToolAuthorityPolicy::Unknown, "{name}");
        assert!(!unknown.requires_project, "{name}");
        assert_eq!(unknown.path_hint, ToolPathHint::None, "{name}");
        assert!(!unknown.destructive, "{name}");
        assert!(!unknown.shell_like, "{name}");
        assert_eq!(runtime_tool_metadata(name), unknown, "{name}");
        assert_eq!(runtime_tool_category(name), "other", "{name}");
        assert_eq!(
            runtime_tool_session_risk_class(name),
            ToolRisk::Unknown.session_risk_class(),
            "{name}"
        );
        assert!(!runtime_tool_is_read_like(name), "{name}");
        assert!(!runtime_tool_is_write_like(name), "{name}");
        assert!(!runtime_tool_is_shell_like(name), "{name}");
        assert!(runtime_tool_requires_permission(name), "{name}");
        assert_eq!(
            runtime_tool_permission_risk(name),
            PERMISSION_RISK_WRITE,
            "{name}"
        );
        assert!(ToolCall::from_tool_name(name, json!({})).is_err(), "{name}");
        assert_model_facing_surfaces_do_not_list_name(name);
        assert_runner_capability_lookup_rejects_non_runtime_name(name);
    }
}

#[test]
fn tool_definition_surface_counts_and_action_projection_stay_canonical() {
    use crate::tool_runtime::tool_definition::{lookup_tool_definition, model_hidden_tool_names};

    let openapi = crate::openapi::build_openapi_spec();
    let operation_ids = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|methods| methods.as_object().unwrap().values())
        .map(|operation| operation["operationId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let expected = webcodex_tool_contracts::gpt_action_direct_tool_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .chain(std::iter::once(
            crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
        ))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        operation_ids, expected,
        "generic OpenAPI must derive from Adaptive Direct plus gateway"
    );
    assert!(
        operation_ids.len() < 30,
        "GPT Action operation budget exceeded"
    );
    assert_eq!(openapi["components"]["schemas"], serde_json::json!({}));
    assert!(openapi["components"]["schemas"].is_object());
    for forbidden in [
        "runCodex",
        "RunCodex",
        "sessionHandoffSummary",
        "SessionHandoff",
        "applyTextEdits",
        "ApplyTextEdits",
        "artifactUpload",
        "ArtifactUpload",
        "callRuntimeTool",
    ] {
        assert!(
            !operation_ids
                .iter()
                .any(|operation_id| operation_id.contains(forbidden)),
            "retired GPT Action vocabulary leaked: {forbidden}: {operation_ids:?}"
        );
    }

    let model_facing_names = registered_tool_names();
    assert!(
        lookup_tool_definition("run_codex").is_none(),
        "run_codex must not keep an explicit ToolDefinition"
    );
    // `ModelHidden` is a stable, documented back-compat surface: dispatched
    // but withheld from the model. `run_codex` is different — it must be fully
    // gone (no ToolDefinition at all). The hidden set is asserted to a fixed
    // batch elsewhere; here we only confirm run_codex is not hiding in it.
    assert!(
        !model_hidden_tool_names().any(|name| name == "run_codex"),
        "run_codex must remain fully removed, not hidden"
    );
    assert!(
        ToolCall::from_tool_name(
            "run_codex",
            json!({"project": SAMPLE_PROJECT, "prompt": "summarize"})
        )
        .is_err(),
        "run_codex must not remain parser-known"
    );
    assert!(
        !model_facing_names.iter().any(|name| name == "run_codex"),
        "run_codex must remain removed from model-facing tools: {model_facing_names:?}"
    );
    assert_eq!(
        crate::tool_runtime::tool_definition::model_visible_tool_definitions().count(),
        model_facing_names.len(),
        "model-visible ToolDefinition count must match model-facing tool count (ModelHidden tools are dispatched but not listed)"
    );
    assert_model_facing_surfaces_do_not_list_name("run_codex");
}

#[test]
fn current_session_tools_are_absent_from_all_discovery_surfaces() {
    use crate::tool_runtime::tool_definition::lookup_tool_definition;

    for name in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            lookup_tool_definition(name).is_none(),
            "{name} must not have a ToolDefinition"
        );
        assert!(ToolCall::from_tool_name(name, json!({"project": SAMPLE_PROJECT})).is_err());
        assert_model_facing_surfaces_do_not_list_name(name);
    }
}

fn assert_model_facing_surfaces_do_not_list_name(name: &str) {
    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        !spec_names.contains(name),
        "{name} must not appear in registered ToolSpecs"
    );
    assert!(
        !registered_tool_names().iter().any(|tool| tool == name),
        "{name} must not appear in model-facing tool names"
    );

    let mcp_payload = json!({ "tools": specs });
    let mcp_names = mcp_payload["tools"]
        .as_array()
        .expect("MCP tools/list payload tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("MCP tool name"))
        .collect::<BTreeSet<_>>();
    assert!(
        !mcp_names.contains(name),
        "{name} must not appear in MCP tools/list names"
    );

    let openapi = crate::openapi::build_openapi_spec();
    let action_ids = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|methods| methods.as_object().unwrap().values())
        .filter_map(|operation| operation["operationId"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        !action_ids.contains(name),
        "{name} must not appear as a direct GPT Action operation"
    );

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    assert!(
        !serde_json::to_string(&manifest).unwrap().contains(name),
        "{name} must not appear in compact tool_manifest"
    );
    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: None,
    });
    assert!(
        !serde_json::to_string(&list_tools).unwrap().contains(name),
        "{name} must not appear in bounded list_tools discovery"
    );
    let full_list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: false,
        limit: None,
    });
    assert!(
        !serde_json::to_string(&full_list_tools)
            .unwrap()
            .contains(name),
        "{name} must not appear in full list_tools discovery"
    );

    // Static discovery surfaces: category groups and recommended flows are
    // compiled straight into the model-facing catalog.
    for group in crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUPS {
        assert!(
            !group.tools.contains(&name),
            "{name} must not appear in discovery group {}",
            group.name
        );
    }
    for flow in crate::tool_runtime::tool_catalog::TOOL_RECOMMENDED_FLOWS {
        assert!(
            !flow.tools.contains(&name),
            "{name} must not appear in recommended flow {}",
            flow.name
        );
        assert!(
            !flow.summary.contains(name) && !flow.manifest_purpose.contains(name),
            "{name} must not appear in recommended flow text for {}",
            flow.name
        );
    }
}

fn assert_runner_capability_lookup_rejects_non_runtime_name(name: &str) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        let _ = crate::tool_runtime::tool_definition::runtime_tool_runner_capability(name);
    });
    std::panic::set_hook(previous_hook);
    assert!(
        result.is_err(),
        "{name} must not resolve Runner capability through metadata fallback"
    );
}
