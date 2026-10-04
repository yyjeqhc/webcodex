use super::*;

#[test]
fn retired_delete_files_alias_stays_absent() {
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use crate::tool_runtime::tool_definition::lookup_tool_definition;

    assert!(lookup_tool_metadata("delete_files").is_none());
    assert!(lookup_tool_definition("delete_files").is_none());
    assert!(!is_known_tool_name("delete_files"));
    assert!(ToolCall::from_tool_name("delete_files", json!({})).is_err());
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "delete_files"));
}

#[test]
fn git_diff_hunks_rejects_unknown_legacy_fields_compactly() {
    let error = ToolCall::from_tool_name(
        "read_git_diff_hunks",
        json!({
            "project": SAMPLE_PROJECT,
            "mode": "worktree",
            "max_lines_per_hunk": 80
        }),
    )
    .unwrap_err();
    assert!(error.contains("unknown field"), "{error}");
    assert!(
        error.contains("mode") || error.contains("max_lines_per_hunk"),
        "{error}"
    );
    assert!(error.contains("max_hunk_lines"), "{error}");
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
        "retired delete_files alias must stay absent from ToolCall parsing"
    );
    // A ToolDefinition may be `ModelHidden`: kernel-known and dispatchable only
    // through an adapter that explicitly projects it, or retained as compatibility
    // plumbing outside the ordinary model-facing registry. The set is intentionally
    // fixed and documented here so an accidental hide/exposure is caught. The
    // legacy single-purpose edit tools are no longer ToolDefinitions at all, so
    // they are absent here.
    let expected_hidden: BTreeSet<&str> = [
        "start_session",
        "read_job_tail",
        "sync_goal_plan",
        "get_work_result_state",
        "read_work_result_activity_detail",
        "send_work_result_message",
        "apply_patch",
        "apply_unified_diff",
        "write_project_file",
        "read_changed_file_diff",
        "read_pdf_chunk",
        "record_external_observation",
        "get_session_handoff_state",
        "present_agent_continuation",
        "present_job_terminal_continuation",
        "bind_agent_continuation",
        "recover_agent_continuation_endpoint",
        "get_agent_continuation_state",
        "acquire_agent_continuation_wake",
        "prepare_agent_continuation_wake",
        "finish_agent_continuation_wake",
        "unbind_agent_continuation",
        "get_agent_wait_state",
        "bind_job_terminal_continuation",
        "get_job_terminal_continuation_state",
        "prepare_job_terminal_continuation",
        "finish_job_terminal_continuation",
        "unbind_job_terminal_continuation",
        "read_tool_trace",
        "list_skills",
        "read_skill_file",
        "list_skill_versions",
        "install_skill",
        "activate_skill",
        "remove_skill_revision",
        "search_memory",
        "read_memory",
        "set_memory",
        "delete_memory",
        "list_memory_scopes",
        "purge_memory_scope",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        model_hidden_tool_names().collect::<BTreeSet<_>>(),
        expected_hidden,
        "hidden ToolDefinitions must match the documented App-only, dormant presentation and compatibility inventory"
    );
}

#[test]
fn tool_definitions_match_agent_capability_dispatch_helper() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, tool_definitions,
    };

    for definition in tool_definitions() {
        // ModelHidden tools retain explicit protocol/domain paths but have no
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
            required_runner_capability(&call),
            definition.runner_capability,
            "{} Runner capability mirror must match dispatch helper",
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
