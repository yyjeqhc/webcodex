#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ToolRisk {
    ReadOnly,
    ProjectWrite,
    JobRun,
    AccountManage,
    Unknown,
}

impl ToolRisk {
    pub(crate) fn session_risk_class(self) -> &'static str {
        match self {
            ToolRisk::ReadOnly => "read_only",
            ToolRisk::ProjectWrite => "project_write",
            ToolRisk::JobRun => "job_run",
            ToolRisk::AccountManage => "account_manage",
            ToolRisk::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolPathHint {
    None,
    SinglePath,
    PathList,
    Patch,
    Artifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolMetadata {
    pub(crate) name: &'static str,
    pub(crate) provider_id: &'static str,
    pub(crate) risk: ToolRisk,
    pub(crate) oauth_scope: Option<&'static str>,
    pub(crate) requires_project: bool,
    pub(crate) path_hint: ToolPathHint,
    pub(crate) read_only: bool,
    pub(crate) destructive: bool,
    pub(crate) shell_like: bool,
}

const RUNTIME_READ: &str = "runtime:read";
const PROJECT_READ: &str = "project:read";
const PROJECT_WRITE: &str = "project:write";
const JOB_RUN: &str = "job:run";

const fn metadata(
    name: &'static str,
    provider_id: &'static str,
    risk: ToolRisk,
    oauth_scope: Option<&'static str>,
    requires_project: bool,
    path_hint: ToolPathHint,
    destructive: bool,
    shell_like: bool,
) -> ToolMetadata {
    ToolMetadata {
        name,
        provider_id,
        risk,
        oauth_scope,
        requires_project,
        path_hint,
        read_only: matches!(risk, ToolRisk::ReadOnly),
        destructive,
        shell_like,
    }
}

pub(crate) const TOOL_METADATA: &[ToolMetadata] = &[
    metadata(
        "list_tools",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "start_session",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "session_summary",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "post_session_message",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "list_session_messages",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "resolve_session_message",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "session_discussion_summary",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "session_handoff_summary",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "bind_current_session",
        "control",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "current_session",
        "control",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "unbind_current_session",
        "control",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "workspace_checkpoint_create",
        "native",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "workspace_checkpoint_list",
        "native",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "workspace_checkpoint_show",
        "native",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "workspace_checkpoint_restore",
        "native",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::Patch,
        false,
        false,
    ),
    metadata(
        "workspace_checkpoint_delete",
        "native",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::None,
        true,
        false,
    ),
    metadata(
        "run_shell",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        true,
        true,
    ),
    metadata(
        "apply_patch",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::Patch,
        false,
        false,
    ),
    metadata(
        "apply_patch_checked",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::Patch,
        false,
        false,
    ),
    metadata(
        "delete_project_files",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::PathList,
        true,
        false,
    ),
    metadata(
        "delete_files",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::PathList,
        true,
        false,
    ),
    metadata(
        "git_restore_paths",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::PathList,
        true,
        false,
    ),
    metadata(
        "discard_untracked",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::PathList,
        true,
        false,
    ),
    metadata(
        "validate_patch",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::Patch,
        false,
        false,
    ),
    metadata(
        "replace_in_file",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "replace_exact_block",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "insert_before_pattern",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "insert_after_pattern",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "write_project_file",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "save_project_artifact",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::Artifact,
        false,
        false,
    ),
    metadata(
        "read_project_artifact_metadata",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::Artifact,
        false,
        false,
    ),
    metadata(
        "read_project_artifact",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::Artifact,
        false,
        false,
    ),
    metadata(
        "replace_line_range",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "insert_at_line",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "delete_line_range",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "apply_text_edits",
        "agent",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "git_status",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "git_diff",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "git_diff_hunks",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "git_log",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "cargo_fmt",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "cargo_check",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "cargo_test",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "read_file",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::SinglePath,
        false,
        false,
    ),
    metadata(
        "run_job",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        true,
        true,
    ),
    metadata(
        "run_codex",
        "agent",
        ToolRisk::JobRun,
        Some(JOB_RUN),
        true,
        ToolPathHint::None,
        true,
        true,
    ),
    metadata(
        "job_status",
        "native",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "job_log",
        "native",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "list_project_files",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "search_project_text",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "git_diff_summary",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "show_changes",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "workspace_hygiene_check",
        "agent",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        true,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "list_jobs",
        "native",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "job_tail",
        "native",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "list_projects",
        "control",
        ToolRisk::ReadOnly,
        Some(PROJECT_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "register_project",
        "control",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        false,
        ToolPathHint::None,
        true,
        false,
    ),
    metadata(
        "create_project",
        "control",
        ToolRisk::ProjectWrite,
        Some(PROJECT_WRITE),
        false,
        ToolPathHint::None,
        true,
        false,
    ),
    metadata(
        "list_agents",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "runtime_status",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
    metadata(
        "tool_manifest",
        "control",
        ToolRisk::ReadOnly,
        Some(RUNTIME_READ),
        false,
        ToolPathHint::None,
        false,
        false,
    ),
];

pub(crate) fn lookup_tool_metadata(name: &str) -> Option<&'static ToolMetadata> {
    TOOL_METADATA.iter().find(|metadata| metadata.name == name)
}

pub(crate) fn tool_metadata(name: &str) -> ToolMetadata {
    lookup_tool_metadata(name).copied().unwrap_or(ToolMetadata {
        name: "<unknown>",
        provider_id: "unknown",
        risk: ToolRisk::Unknown,
        oauth_scope: None,
        requires_project: false,
        path_hint: ToolPathHint::None,
        read_only: false,
        destructive: false,
        shell_like: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::scopes::{oauth_scope_policy_for_runtime_tool, OAuthToolScopePolicy};
    use crate::auth::scopes::{
        SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
    };
    use crate::tool_runtime::types::KNOWN_TOOL_NAMES;

    #[test]
    fn tool_metadata_covers_all_known_tools() {
        for name in KNOWN_TOOL_NAMES {
            assert!(
                lookup_tool_metadata(name).is_some(),
                "{name} missing tool metadata"
            );
        }
    }

    #[test]
    fn runtime_tool_metadata_oauth_scopes_are_known_to_scope_policy() {
        for metadata in TOOL_METADATA
            .iter()
            .filter(|metadata| KNOWN_TOOL_NAMES.contains(&metadata.name))
        {
            let Some(scope) = metadata.oauth_scope else {
                continue;
            };
            assert_eq!(
                oauth_scope_policy_for_runtime_tool(metadata.name),
                OAuthToolScopePolicy::Require(scope),
                "{} metadata scope should drive runtime tool OAuth policy",
                metadata.name
            );
        }
    }

    #[test]
    fn tool_metadata_unknown_is_safe() {
        assert!(lookup_tool_metadata("not_a_tool").is_none());
        let metadata = tool_metadata("not_a_tool");
        assert_eq!(metadata.risk, ToolRisk::Unknown);
        assert_eq!(metadata.oauth_scope, None);
        assert!(!metadata.read_only);
        assert!(!metadata.destructive);
        assert!(!metadata.shell_like);
    }

    #[test]
    fn tool_metadata_show_changes_is_project_read_and_read_only() {
        let metadata = lookup_tool_metadata("show_changes").unwrap();
        assert_eq!(metadata.provider_id, "agent");
        assert_eq!(metadata.risk, ToolRisk::ReadOnly);
        assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_READ));
        assert!(metadata.requires_project);
        assert!(metadata.read_only);
        assert!(!metadata.destructive);
    }

    #[test]
    fn tool_metadata_start_session_is_runtime_read() {
        let metadata = lookup_tool_metadata("start_session").unwrap();
        assert_eq!(metadata.provider_id, "control");
        assert_eq!(metadata.risk, ToolRisk::ReadOnly);
        assert_eq!(metadata.oauth_scope, Some(SCOPE_RUNTIME_READ));
        assert!(!metadata.requires_project);
        assert!(metadata.read_only);
    }

    #[test]
    fn current_session_tools_are_project_read_control_tools() {
        for name in [
            "bind_current_session",
            "current_session",
            "unbind_current_session",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, "control", "{name}");
            assert_eq!(metadata.risk, ToolRisk::ReadOnly, "{name}");
            assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_READ), "{name}");
            assert!(metadata.requires_project, "{name}");
            assert!(metadata.read_only, "{name}");
            assert!(!metadata.destructive, "{name}");
            assert!(!metadata.shell_like, "{name}");
        }
    }

    #[test]
    fn checkpoint_metadata_uses_project_read_and_write_scopes() {
        for name in [
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, "native", "{name}");
            assert_eq!(metadata.risk, ToolRisk::ReadOnly, "{name}");
            assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_READ), "{name}");
            assert!(metadata.requires_project, "{name}");
            assert!(metadata.read_only, "{name}");
        }
        for name in [
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, "native", "{name}");
            assert_eq!(metadata.risk, ToolRisk::ProjectWrite, "{name}");
            assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_WRITE), "{name}");
            assert!(metadata.requires_project, "{name}");
            assert!(!metadata.read_only, "{name}");
        }
    }

    #[test]
    fn tool_metadata_write_tools_are_project_write() {
        for name in [
            "write_project_file",
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "replace_in_file",
            "replace_exact_block",
            "insert_before_pattern",
            "insert_after_pattern",
            "apply_patch",
            "apply_patch_checked",
            "delete_project_files",
            "save_project_artifact",
            "git_restore_paths",
            "discard_untracked",
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
            "register_project",
            "create_project",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.risk, ToolRisk::ProjectWrite, "{name}");
            assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_WRITE), "{name}");
            assert!(!metadata.read_only, "{name}");
        }
    }

    #[test]
    fn tool_metadata_job_tools_are_job_run() {
        for name in [
            "run_shell",
            "run_job",
            "run_codex",
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.risk, ToolRisk::JobRun, "{name}");
            assert_eq!(metadata.oauth_scope, Some(SCOPE_JOB_RUN), "{name}");
        }
    }

    #[test]
    fn tool_metadata_keeps_account_manage_class_available() {
        assert_eq!(
            ToolRisk::AccountManage.session_risk_class(),
            "account_manage"
        );
    }
}
