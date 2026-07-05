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
    assert_eq!(disabled_tools, vec!["run_codex"]);

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
    assert_eq!(
        unit_argument_tools,
        vec!["list_projects", "list_agents", "runtime_status"]
    );

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
        ("run_codex", PERMISSION_RISK_JOB),
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
