use super::*;

#[test]
fn required_agent_capability_matches_metadata_risk_table() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk, TOOL_PROVIDER_AGENT};
    use crate::tool_runtime::tool_definition::is_model_visible_tool_name;

    let cases = [
        (
            "run_process",
            ToolRisk::JobRun,
            AgentCapability::StructuredProcess,
        ),
        (
            "run_detached_process",
            ToolRisk::JobRun,
            AgentCapability::DetachedProcess,
        ),
        (
            "run_script",
            ToolRisk::JobRun,
            AgentCapability::StructuredScript,
        ),
        (
            "coding_agent_start",
            ToolRisk::JobRun,
            AgentCapability::CodingAgentRuns,
        ),
        (
            "start_agent_task_coding_run",
            ToolRisk::JobRun,
            AgentCapability::CodingAgentRuns,
        ),
        ("run_shell", ToolRisk::JobRun, AgentCapability::Shell),
        (
            "open_session_shell",
            ToolRisk::JobRun,
            AgentCapability::PersistentShell,
        ),
        (
            "session_shell_exec",
            ToolRisk::JobRun,
            AgentCapability::PersistentShell,
        ),
        (
            "session_shell_status",
            ToolRisk::Read,
            AgentCapability::PersistentShell,
        ),
        (
            "close_session_shell",
            ToolRisk::JobRun,
            AgentCapability::PersistentShell,
        ),
        (
            "apply_patch",
            ToolRisk::ProjectWrite,
            AgentCapability::ApplyPatch,
        ),
        (
            "apply_unified_diff",
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
            AgentCapability::StructuredProcess,
        ),
        (
            "discard_untracked",
            ToolRisk::ProjectWrite,
            AgentCapability::StructuredProcess,
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
            "computer_save_snapshot",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "read_project_artifact_metadata",
            ToolRisk::Read,
            AgentCapability::FileRead,
        ),
        (
            "read_project_artifact",
            ToolRisk::Read,
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
            "apply_text_edits",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        ("git_status", ToolRisk::Read, AgentCapability::GitOrShell),
        ("git_diff", ToolRisk::Read, AgentCapability::GitOrShell),
        (
            "git_diff_hunks",
            ToolRisk::Read,
            AgentCapability::GitOrShell,
        ),
        (
            "git_review_summary",
            ToolRisk::Read,
            AgentCapability::GitOrShell,
        ),
        ("git_log", ToolRisk::Read, AgentCapability::GitOrShell),
        ("cargo_fmt", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_check", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_test", ToolRisk::JobRun, AgentCapability::Shell),
        ("go_test", ToolRisk::JobRun, AgentCapability::OwnerOnly),
        ("read_file", ToolRisk::Read, AgentCapability::FileRead),
        ("read_files", ToolRisk::Read, AgentCapability::FileRead),
        (
            "lsp_status",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "document_symbols",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "document_diagnostics",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "hover",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "workspace_symbols",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "goto_definition",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "find_references",
            ToolRisk::Read,
            AgentCapability::LspReadOnlyNavigation,
        ),
        (
            "call_hierarchy",
            ToolRisk::Read,
            AgentCapability::LspCallHierarchy,
        ),
        ("run_job", ToolRisk::JobRun, AgentCapability::AsyncJobs),
        (
            "project_overview",
            ToolRisk::Read,
            AgentCapability::FileRead,
        ),
        (
            "list_project_files",
            ToolRisk::Read,
            AgentCapability::FileRead,
        ),
        (
            "list_project_tracked_files",
            ToolRisk::Read,
            AgentCapability::Shell,
        ),
        (
            "search_project_text",
            ToolRisk::Read,
            AgentCapability::Shell,
        ),
        (
            "search_project_texts",
            ToolRisk::Read,
            AgentCapability::Shell,
        ),
        (
            "git_diff_summary",
            ToolRisk::Read,
            AgentCapability::GitOrShell,
        ),
        ("show_changes", ToolRisk::Read, AgentCapability::GitOrShell),
        (
            "workspace_hygiene_check",
            ToolRisk::Read,
            AgentCapability::GitOrShell,
        ),
        (
            "workspace_checkpoint_create",
            ToolRisk::CheckpointManage,
            AgentCapability::FileRead,
        ),
        (
            "workspace_checkpoint_restore",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "workspace_checkpoint_list",
            ToolRisk::Read,
            AgentCapability::OwnerOnly,
        ),
        (
            "workspace_checkpoint_show",
            ToolRisk::Read,
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
                || spec.name.starts_with("workspace_checkpoint_")
                || spec.name == "computer_save_snapshot")
                && metadata.requires_project)
                .then_some(spec.name.as_str())
        })
        .collect::<BTreeSet<_>>();
    let table_project_tools = cases
        .iter()
        .map(|(name, _, _)| *name)
        .filter(|name| is_model_visible_tool_name(name))
        .collect::<BTreeSet<_>>();
    assert_eq!(table_project_tools, expected_project_tools);

    for (name, risk, capability) in cases {
        let metadata = lookup_tool_metadata(name).unwrap();
        assert_eq!(metadata.risk, risk, "{name} metadata risk");
        // Hidden tools have no spec, so sample_tool_args cannot build args.
        // Their capability routing still matters (they dispatch); verify it
        // against the ToolDefinition directly. Visible tools go through the
        // full ToolCall path.
        if is_model_visible_tool_name(name) {
            let call = ToolCall::from_tool_name(name, sample_tool_args(name))
                .unwrap_or_else(|e| panic!("{name} should deserialize: {e}"));
            assert_eq!(
                required_agent_capability(&call),
                Some(capability),
                "{name} capability"
            );
        } else {
            assert_eq!(
                crate::tool_runtime::tool_definition::lookup_tool_definition(name)
                    .unwrap()
                    .agent_capability,
                Some(capability),
                "{name} (hidden) capability must match dispatch helper"
            );
        }
    }
}
