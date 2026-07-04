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

pub(crate) const RUNTIME_READ: &str = "runtime:read";
pub(crate) const PROJECT_READ: &str = "project:read";
pub(crate) const PROJECT_WRITE: &str = "project:write";
pub(crate) const JOB_RUN: &str = "job:run";

pub(crate) const fn metadata(
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

const LEGACY_ROUTE_METADATA: &[ToolMetadata] = &[metadata(
    "delete_files",
    "agent",
    ToolRisk::ProjectWrite,
    Some(PROJECT_WRITE),
    true,
    ToolPathHint::PathList,
    true,
    false,
)];

pub(crate) fn lookup_tool_metadata(name: &str) -> Option<&'static ToolMetadata> {
    super::tool_definition::lookup_tool_definition(name)
        .map(|definition| &definition.metadata)
        .or_else(|| {
            LEGACY_ROUTE_METADATA
                .iter()
                .find(|metadata| metadata.name == name)
        })
}

#[cfg(test)]
pub(crate) fn iter_tool_metadata() -> impl Iterator<Item = ToolMetadata> {
    super::tool_definition::TOOL_DEFINITIONS
        .iter()
        .map(|definition| definition.metadata())
        .chain(LEGACY_ROUTE_METADATA.iter().copied())
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
    use crate::tool_runtime::KNOWN_TOOL_NAMES;

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
        for metadata in
            iter_tool_metadata().filter(|metadata| KNOWN_TOOL_NAMES.contains(&metadata.name))
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
    fn tool_metadata_preserves_legacy_delete_files_route() {
        assert!(!KNOWN_TOOL_NAMES.contains(&"delete_files"));
        let metadata = lookup_tool_metadata("delete_files").unwrap();
        assert_eq!(metadata.provider_id, "agent");
        assert_eq!(metadata.risk, ToolRisk::ProjectWrite);
        assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_WRITE));
        assert!(metadata.requires_project);
        assert_eq!(metadata.path_hint, ToolPathHint::PathList);
        assert!(metadata.destructive);
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
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
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
            "stop_job",
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
