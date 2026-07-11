use super::*;

#[test]
fn tool_definitions_drive_session_and_permission_policy() {
    use crate::tool_runtime::metadata::ToolRisk;
    use crate::tool_runtime::tool_definition::{
        runtime_tool_allows_current_session_fallback, runtime_tool_captures_validation_output,
        runtime_tool_creates_or_binds_session, runtime_tool_disabled_message,
        runtime_tool_extra_accepted_flattened_args, runtime_tool_is_change_summary_like,
        runtime_tool_is_current_session_control, runtime_tool_is_git_like,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_permission_risk, runtime_tool_requires_explicit_business_session,
        runtime_tool_requires_permission, runtime_tool_requires_session_project_escape,
        runtime_tool_session_risk_class, tool_definitions, PERMISSION_RISK_ARTIFACT_WRITE,
        PERMISSION_RISK_DESTRUCTIVE, PERMISSION_RISK_JOB, PERMISSION_RISK_PATCH,
        PERMISSION_RISK_SHELL, PERMISSION_RISK_VALIDATION, PERMISSION_RISK_WRITE,
        TOOL_DISCOVERY_GROUPS, TOOL_DISCOVERY_GROUP_GIT,
    };

    let git_group = TOOL_DISCOVERY_GROUPS
        .iter()
        .find(|group| group.name == TOOL_DISCOVERY_GROUP_GIT)
        .expect("git discovery group")
        .tools
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for definition in tool_definitions() {
        let metadata = definition.metadata();
        assert_eq!(
            definition.session_risk_class(),
            metadata.risk.session_risk_class(),
            "{} session risk class must derive from metadata risk",
            definition.name
        );
        assert_eq!(
            definition.is_read_like(),
            metadata.read_only,
            "{} read-like policy must derive from metadata",
            definition.name
        );
        assert_eq!(
            definition.is_write_like(),
            metadata.risk == ToolRisk::ProjectWrite,
            "{} write-like policy must derive from metadata",
            definition.name
        );
        assert_eq!(
            definition.is_shell_like(),
            metadata.shell_like || metadata.risk == ToolRisk::JobRun,
            "{} shell-like guard policy must include job-run tools",
            definition.name
        );
        assert_eq!(
            definition.is_git_like(),
            git_group.contains(definition.name),
            "{} git-like ledger policy must mirror the git discovery group",
            definition.name
        );
        assert_eq!(
            definition.requires_session_project_escape(),
            !metadata.read_only || metadata.destructive || metadata.shell_like,
            "{} cross-project session policy must derive from metadata risk flags",
            definition.name
        );
        assert_eq!(
            definition.requires_permission(),
            !metadata.read_only || metadata.destructive || metadata.shell_like,
            "{} permission requirement must derive from metadata risk flags",
            definition.name
        );
        assert_eq!(
            runtime_tool_session_risk_class(definition.name),
            definition.session_risk_class(),
            "{} session risk facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_read_like(definition.name),
            definition.is_read_like(),
            "{} read-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_write_like(definition.name),
            definition.is_write_like(),
            "{} write-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_shell_like(definition.name),
            definition.is_shell_like(),
            "{} shell-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_git_like(definition.name),
            definition.is_git_like(),
            "{} git-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_change_summary_like(definition.name),
            definition.is_change_summary_like(),
            "{} change-summary facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_captures_validation_output(definition.name),
            definition.captures_validation_output(),
            "{} validation-output facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_current_session_control(definition.name),
            definition.is_current_session_control(),
            "{} current-session control facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_explicit_business_session(definition.name),
            definition.requires_explicit_business_session(),
            "{} business-session facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_creates_or_binds_session(definition.name),
            definition.creates_or_binds_session(),
            "{} session creation/bind facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_disabled_message(definition.name),
            definition.disabled_message(),
            "{} disabled facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_extra_accepted_flattened_args(definition.name),
            definition.extra_accepted_flattened_args(),
            "{} extra accepted flattened args facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_allows_current_session_fallback(definition.name),
            definition.allows_current_session_fallback(),
            "{} current-session fallback facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_session_project_escape(definition.name),
            definition.requires_session_project_escape(),
            "{} session-project escape facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_permission(definition.name),
            definition.requires_permission(),
            "{} permission facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_permission_risk(definition.name),
            definition.permission_risk(),
            "{} permission risk facade must use ToolDefinition",
            definition.name
        );
    }

    let change_summary_tools = tool_definitions()
        .filter(|definition| definition.is_change_summary_like())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        change_summary_tools,
        vec!["git_diff_summary", "show_changes", "git_diff_hunks"]
    );

    let validation_output_tools = tool_definitions()
        .filter(|definition| definition.captures_validation_output())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        validation_output_tools,
        vec!["cargo_fmt", "cargo_check", "cargo_test"]
    );

    let current_session_control_tools = tool_definitions()
        .filter(|definition| definition.is_current_session_control())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        current_session_control_tools,
        vec![
            "bind_current_session",
            "current_session",
            "unbind_current_session"
        ]
    );

    let explicit_business_session_tools = tool_definitions()
        .filter(|definition| definition.requires_explicit_business_session())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        explicit_business_session_tools,
        vec![
            "finish_coding_task",
            "session_summary",
            "validation_summary",
            "post_session_message",
            "list_session_messages",
            "resolve_session_message",
            "session_discussion_summary",
            "session_handoff_summary"
        ]
    );

    let creates_or_binds_session_tools = tool_definitions()
        .filter(|definition| definition.creates_or_binds_session())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        creates_or_binds_session_tools,
        vec!["start_session", "start_coding_task", "bind_current_session"]
    );

    let disabled_tools = tool_definitions()
        .filter(|definition| definition.disabled_message().is_some())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(disabled_tools, Vec::<&'static str>::new());

    let extra_accepted_flattened_arg_tools = tool_definitions()
        .filter(|definition| !definition.extra_accepted_flattened_args().is_empty())
        .map(|definition| {
            (
                definition.name,
                definition.extra_accepted_flattened_args().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        extra_accepted_flattened_arg_tools,
        vec![("start_coding_task", vec!["session_id"])]
    );

    let unit_argument_tools = tool_definitions()
        .filter(|definition| definition.uses_unit_arguments())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(unit_argument_tools, vec!["list_projects", "list_agents"]);

    let artifact_upload_path_binding_tools = tool_definitions()
        .filter(|definition| definition.requires_artifact_upload_path_binding())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        artifact_upload_path_binding_tools,
        vec![
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort"
        ]
    );

    let current_session_fallback_tools = tool_definitions()
        .filter(|definition| definition.allows_current_session_fallback())
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert!(current_session_fallback_tools.contains("read_file"));
    assert!(current_session_fallback_tools.contains("run_shell"));
    assert!(current_session_fallback_tools.contains("workspace_hygiene_check"));
    for name in [
        "start_session",
        "start_coding_task",
        "finish_coding_task",
        "session_summary",
        "session_handoff_summary",
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            !current_session_fallback_tools.contains(name),
            "{name} must not implicitly use the current-session binding"
        );
    }

    for (tool, risk) in [
        ("cargo_check", PERMISSION_RISK_VALIDATION),
        ("run_shell", PERMISSION_RISK_SHELL),
        ("run_job", PERMISSION_RISK_JOB),
        ("stop_job", PERMISSION_RISK_JOB),
        ("delete_project_files", PERMISSION_RISK_DESTRUCTIVE),
        ("save_project_artifact", PERMISSION_RISK_ARTIFACT_WRITE),
        ("apply_patch", PERMISSION_RISK_PATCH),
        ("write_project_file", PERMISSION_RISK_WRITE),
    ] {
        assert_eq!(runtime_tool_permission_risk(tool), risk, "{tool}");
    }

    assert_eq!(
        runtime_tool_session_risk_class("__unknown__"),
        ToolRisk::Unknown.session_risk_class()
    );
    assert!(!runtime_tool_is_write_like("__unknown__"));
    assert!(!runtime_tool_is_shell_like("__unknown__"));
    assert!(!runtime_tool_allows_current_session_fallback("__unknown__"));
    assert!(runtime_tool_requires_permission("__unknown__"));
    assert!(runtime_tool_requires_session_project_escape("__unknown__"));
    assert_eq!(
        runtime_tool_permission_risk("__unknown__"),
        PERMISSION_RISK_WRITE
    );
    assert_eq!(
        runtime_tool_permission_risk("compat_patch_like"),
        PERMISSION_RISK_PATCH,
        "unknown compatibility names keep the legacy path/name fallback"
    );
    assert_ne!(
        runtime_tool_permission_risk("compat_patch_like"),
        runtime_tool_permission_risk("unknown_artifact"),
        "name-based patch fallback must not classify unrelated unknown names"
    );
}

#[test]
fn required_agent_capability_matches_metadata_risk_table() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk, TOOL_PROVIDER_AGENT};

    let cases = [
        ("run_shell", ToolRisk::JobRun, AgentCapability::Shell),
        (
            "apply_patch",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "apply_patch_checked",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "delete_project_files",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "git_restore_paths",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "discard_untracked",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        // Read-only dry run, but implemented through the agent shell path.
        ("validate_patch", ToolRisk::ReadOnly, AgentCapability::Shell),
        (
            "replace_in_file",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "replace_exact_block",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_before_pattern",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_after_pattern",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "write_project_file",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "save_project_artifact",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "read_project_artifact_metadata",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "read_project_artifact",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "artifact_upload_begin",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_chunk",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_finish",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_abort",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "replace_line_range",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_at_line",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "delete_line_range",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "apply_text_edits",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "git_status",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        ("git_diff", ToolRisk::ReadOnly, AgentCapability::GitOrShell),
        (
            "git_diff_hunks",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        ("git_log", ToolRisk::ReadOnly, AgentCapability::GitOrShell),
        ("cargo_fmt", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_check", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_test", ToolRisk::JobRun, AgentCapability::Shell),
        ("read_file", ToolRisk::ReadOnly, AgentCapability::FileRead),
        (
            "lsp_status",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "document_symbols",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "document_diagnostics",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "hover",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "workspace_symbols",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "goto_definition",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "find_references",
            ToolRisk::ReadOnly,
            AgentCapability::LspReadOnlyNavigation,
        ),
        ("run_job", ToolRisk::JobRun, AgentCapability::AsyncJobs),
        (
            "project_overview",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "list_project_files",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "search_project_text",
            ToolRisk::ReadOnly,
            AgentCapability::Shell,
        ),
        (
            "git_diff_summary",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "show_changes",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "workspace_hygiene_check",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "workspace_checkpoint_create",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "workspace_checkpoint_restore",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "workspace_checkpoint_list",
            ToolRisk::ReadOnly,
            AgentCapability::OwnerOnly,
        ),
        (
            "workspace_checkpoint_show",
            ToolRisk::ReadOnly,
            AgentCapability::OwnerOnly,
        ),
        (
            "workspace_checkpoint_delete",
            ToolRisk::ProjectWrite,
            AgentCapability::OwnerOnly,
        ),
    ];

    let specs = registered_tool_specs();
    let expected_project_tools = specs
        .iter()
        .filter_map(|spec| {
            let metadata = lookup_tool_metadata(&spec.name).unwrap();
            ((metadata.provider_id == TOOL_PROVIDER_AGENT
                || spec.name.starts_with("workspace_checkpoint_"))
                && metadata.requires_project)
                .then_some(spec.name.as_str())
        })
        .collect::<BTreeSet<_>>();
    let table_project_tools = cases
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<BTreeSet<_>>();
    assert_eq!(table_project_tools, expected_project_tools);

    for (name, risk, capability) in cases {
        let metadata = lookup_tool_metadata(name).unwrap();
        assert_eq!(metadata.risk, risk, "{name} metadata risk");
        let call = ToolCall::from_tool_name(name, sample_tool_args(name))
            .unwrap_or_else(|e| panic!("{name} should deserialize: {e}"));
        assert_eq!(
            required_agent_capability(&call),
            Some(capability),
            "{name} capability"
        );
    }
}

#[test]
fn policy_helpers_keep_non_runtime_names_on_fallback_boundary() {
    use crate::tool_runtime::metadata::{
        lookup_tool_metadata, ToolPathHint, ToolRisk, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
        TOOL_PROVIDER_UNKNOWN,
    };
    use crate::tool_runtime::tool_definition::{
        is_model_hidden_tool_name, is_model_visible_tool_name, lookup_tool_definition,
        runtime_tool_allows_current_session_fallback, runtime_tool_category,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_metadata, runtime_tool_permission_risk, runtime_tool_requires_permission,
        runtime_tool_requires_session_project_escape, runtime_tool_session_risk_class,
        PERMISSION_RISK_DESTRUCTIVE, PERMISSION_RISK_WRITE,
    };

    let delete_files = runtime_tool_metadata("delete_files");
    assert_eq!(delete_files.name, "delete_files");
    assert_eq!(delete_files.provider_id, TOOL_PROVIDER_AGENT);
    assert_eq!(delete_files.risk, ToolRisk::ProjectWrite);
    assert_eq!(delete_files.oauth_scope, Some(PROJECT_WRITE));
    assert!(delete_files.requires_project);
    assert_eq!(delete_files.path_hint, ToolPathHint::PathList);
    assert!(!delete_files.read_only);
    assert!(delete_files.destructive);
    assert!(!delete_files.shell_like);
    assert_eq!(
        lookup_tool_metadata("delete_files").copied(),
        Some(delete_files)
    );
    assert!(lookup_tool_definition("delete_files").is_none());
    assert!(!is_known_tool_name("delete_files"));
    assert!(!is_model_visible_tool_name("delete_files"));
    assert!(!is_model_hidden_tool_name("delete_files"));
    assert_eq!(runtime_tool_category("delete_files"), "other");
    assert_eq!(
        runtime_tool_session_risk_class("delete_files"),
        ToolRisk::ProjectWrite.session_risk_class()
    );
    assert!(!runtime_tool_is_read_like("delete_files"));
    assert!(runtime_tool_is_write_like("delete_files"));
    assert!(!runtime_tool_is_shell_like("delete_files"));
    assert!(!runtime_tool_allows_current_session_fallback(
        "delete_files"
    ));
    assert!(runtime_tool_requires_permission("delete_files"));
    assert!(runtime_tool_requires_session_project_escape("delete_files"));
    assert_eq!(
        runtime_tool_permission_risk("delete_files"),
        PERMISSION_RISK_DESTRUCTIVE
    );
    assert!(
        ToolCall::from_tool_name(
            "delete_files",
            json!({"project": SAMPLE_PROJECT, "paths": ["old.txt"]})
        )
        .is_err(),
        "delete_files must remain metadata-only, not a runtime ToolCall"
    );
    assert_agent_capability_lookup_rejects_non_runtime_name("delete_files");

    for name in ["__unknown_tool_for_policy_test__", "not_a_tool"] {
        let unknown = runtime_tool_metadata(name);
        assert_eq!(unknown.name, "<unknown>", "{name}");
        assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN, "{name}");
        assert_eq!(unknown.risk, ToolRisk::Unknown, "{name}");
        assert_eq!(unknown.oauth_scope, None, "{name}");
        assert!(!unknown.requires_project, "{name}");
        assert_eq!(unknown.path_hint, ToolPathHint::None, "{name}");
        assert!(!unknown.read_only, "{name}");
        assert!(!unknown.destructive, "{name}");
        assert!(!unknown.shell_like, "{name}");
        assert!(lookup_tool_metadata(name).is_none(), "{name}");
        assert!(lookup_tool_definition(name).is_none(), "{name}");
        assert!(!is_known_tool_name(name), "{name}");
        assert!(!is_model_visible_tool_name(name), "{name}");
        assert!(!is_model_hidden_tool_name(name), "{name}");
        assert_eq!(runtime_tool_category(name), "other", "{name}");
        assert_eq!(
            runtime_tool_session_risk_class(name),
            ToolRisk::Unknown.session_risk_class(),
            "{name}"
        );
        assert!(!runtime_tool_is_read_like(name), "{name}");
        assert!(!runtime_tool_is_write_like(name), "{name}");
        assert!(!runtime_tool_is_shell_like(name), "{name}");
        assert!(
            !runtime_tool_allows_current_session_fallback(name),
            "{name}"
        );
        assert!(runtime_tool_requires_permission(name), "{name}");
        assert!(runtime_tool_requires_session_project_escape(name), "{name}");
        assert_eq!(
            runtime_tool_permission_risk(name),
            PERMISSION_RISK_WRITE,
            "{name}"
        );
        assert!(
            ToolCall::from_tool_name(name, json!({})).is_err(),
            "{name} must remain non-callable"
        );
        assert_agent_capability_lookup_rejects_non_runtime_name(name);
    }
}

fn assert_agent_capability_lookup_rejects_non_runtime_name(name: &str) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        let _ = crate::tool_runtime::tool_definition::runtime_tool_agent_capability(name);
    });
    std::panic::set_hook(previous_hook);
    assert!(
        result.is_err(),
        "{name} must not resolve agent capability through metadata fallback"
    );
}
