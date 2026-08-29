#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolRisk {
    ReadOnly,
    ProjectWrite,
    SkillManage,
    MemoryManage,
    CommunicationManage,
    ComputerControl,
    JobRun,
    /// Reserved for account-control tools; the current runtime manifest has
    /// no model-facing account mutation tool.
    #[allow(dead_code)]
    AccountManage,
    Unknown,
}

impl ToolRisk {
    pub(crate) fn session_risk_class(self) -> &'static str {
        match self {
            ToolRisk::ReadOnly => "read_only",
            ToolRisk::ProjectWrite => "project_write",
            ToolRisk::SkillManage => "skill_manage",
            ToolRisk::MemoryManage => "memory_manage",
            ToolRisk::CommunicationManage => "communication_manage",
            ToolRisk::ComputerControl => "computer_control",
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

impl ToolPathHint {
    pub(crate) fn manifest_label(self) -> &'static str {
        match self {
            ToolPathHint::None => "none",
            ToolPathHint::SinglePath => "single_path",
            ToolPathHint::PathList => "path_list",
            ToolPathHint::Patch => "patch",
            ToolPathHint::Artifact => "artifact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolAuthorityPolicy {
    Require(&'static str),
    RequireAll(&'static [&'static str]),
    /// Reserved fail-closed policy for tools intentionally restricted to
    /// first-party credentials.
    #[allow(dead_code)]
    FirstPartyOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolMetadata {
    pub(crate) name: &'static str,
    pub(crate) provider_id: &'static str,
    pub(crate) risk: ToolRisk,
    /// Canonical credential authority for this runtime tool. Discovery and
    /// execution must both derive from this policy rather than tool-name switches.
    pub(crate) authority: ToolAuthorityPolicy,
    /// Compatibility-only manifest hint retained for the existing `oauth_scope`
    /// field. It is never consulted for authorization.
    pub(crate) legacy_oauth_scope_hint: Option<&'static str>,
    pub(crate) requires_project: bool,
    pub(crate) path_hint: ToolPathHint,
    pub(crate) read_only: bool,
    pub(crate) destructive: bool,
    pub(crate) shell_like: bool,
}

pub(crate) const RUNTIME_READ: &str = crate::auth::SCOPE_RUNTIME_READ;
pub(crate) const SESSION_COLLABORATE: &str = crate::auth::SCOPE_SESSION_COLLABORATE;
pub(crate) const COMMUNICATION_READ: &str = crate::auth::SCOPE_COMMUNICATION_READ;
pub(crate) const COMMUNICATION_MANAGE: &str = crate::auth::SCOPE_COMMUNICATION_MANAGE;
pub(crate) const PROJECT_READ: &str = crate::auth::SCOPE_PROJECT_READ;
pub(crate) const PROJECT_WRITE: &str = crate::auth::SCOPE_PROJECT_WRITE;
pub(crate) const ADMIN: &str = crate::auth::SCOPE_ADMIN;
pub(crate) const JOB_RUN: &str = crate::auth::SCOPE_JOB_RUN;
pub(crate) const CODING_AGENT_RUN: &str = crate::auth::SCOPE_CODING_AGENT_RUN;
pub(crate) const COMPUTER_READ: &str = crate::auth::SCOPE_COMPUTER_READ;
pub(crate) const COMPUTER_CONTROL: &str = crate::auth::SCOPE_COMPUTER_CONTROL;
pub(crate) const COMPUTER_LAUNCH: &str = crate::auth::SCOPE_COMPUTER_LAUNCH;
pub(crate) const COMPUTER_DISPLAY_READ: &str = crate::auth::SCOPE_COMPUTER_DISPLAY_READ;
pub(crate) const COMPUTER_POINTER_CONTROL: &str = crate::auth::SCOPE_COMPUTER_POINTER_CONTROL;
pub(crate) const COMPUTER_CLIPBOARD_READ: &str = crate::auth::SCOPE_COMPUTER_CLIPBOARD_READ;
pub(crate) const COMPUTER_CLIPBOARD_WRITE: &str = crate::auth::SCOPE_COMPUTER_CLIPBOARD_WRITE;

pub(crate) const TOOL_PROVIDER_AGENT: &str = "agent";
pub(crate) const TOOL_PROVIDER_CONTROL: &str = "control";
pub(crate) const TOOL_PROVIDER_NATIVE: &str = "native";
pub(crate) const TOOL_PROVIDER_UNKNOWN: &str = "unknown";

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
        authority: match oauth_scope {
            Some(scope) => ToolAuthorityPolicy::Require(scope),
            None => ToolAuthorityPolicy::Unknown,
        },
        legacy_oauth_scope_hint: oauth_scope,
        requires_project,
        path_hint,
        read_only: matches!(risk, ToolRisk::ReadOnly),
        destructive,
        shell_like,
    }
}

const LEGACY_ROUTE_METADATA: &[ToolMetadata] = &[metadata(
    "delete_files",
    TOOL_PROVIDER_AGENT,
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
    super::tool_definition::tool_definitions()
        .map(|definition| definition.metadata())
        .chain(LEGACY_ROUTE_METADATA.iter().copied())
}

pub(crate) fn tool_metadata(name: &str) -> ToolMetadata {
    lookup_tool_metadata(name).copied().unwrap_or(ToolMetadata {
        name: "<unknown>",
        provider_id: TOOL_PROVIDER_UNKNOWN,
        risk: ToolRisk::Unknown,
        authority: ToolAuthorityPolicy::Unknown,
        legacy_oauth_scope_hint: None,
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
    use crate::auth::scopes::oauth_scope_policy_for_runtime_tool;
    use crate::auth::{SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ};
    use crate::tool_runtime::{is_known_tool_name, known_tool_names};

    #[test]
    fn tool_metadata_covers_all_known_tools() {
        for name in known_tool_names() {
            assert!(
                lookup_tool_metadata(name).is_some(),
                "{name} missing tool metadata"
            );
        }
    }

    #[test]
    fn runtime_tool_metadata_authority_is_the_runtime_scope_policy() {
        for metadata in iter_tool_metadata().filter(|metadata| is_known_tool_name(metadata.name)) {
            assert_eq!(
                oauth_scope_policy_for_runtime_tool(metadata.name),
                metadata.authority,
                "{} ToolMetadata authority must drive runtime authorization",
                metadata.name
            );
        }
    }

    #[test]
    fn tool_metadata_unknown_is_safe() {
        assert!(lookup_tool_metadata("not_a_tool").is_none());
        let metadata = tool_metadata("not_a_tool");
        assert_eq!(metadata.risk, ToolRisk::Unknown);
        assert_eq!(metadata.legacy_oauth_scope_hint, None);
        assert!(!metadata.read_only);
        assert!(!metadata.destructive);
        assert!(!metadata.shell_like);
    }

    #[test]
    fn tool_metadata_preserves_legacy_delete_files_route() {
        assert!(!is_known_tool_name("delete_files"));
        let metadata = lookup_tool_metadata("delete_files").unwrap();
        assert_eq!(metadata.provider_id, TOOL_PROVIDER_AGENT);
        assert_eq!(metadata.risk, ToolRisk::ProjectWrite);
        assert_eq!(metadata.legacy_oauth_scope_hint, Some(PROJECT_WRITE));
        assert!(metadata.requires_project);
        assert_eq!(metadata.path_hint, ToolPathHint::PathList);
        assert!(metadata.destructive);
        assert!(!metadata.shell_like);
    }

    #[test]
    fn tool_metadata_show_changes_is_project_read_and_read_only() {
        let metadata = lookup_tool_metadata("show_changes").unwrap();
        assert_eq!(metadata.provider_id, TOOL_PROVIDER_AGENT);
        assert_eq!(metadata.risk, ToolRisk::ReadOnly);
        assert_eq!(metadata.legacy_oauth_scope_hint, Some(PROJECT_READ));
        assert!(metadata.requires_project);
        assert!(metadata.read_only);
        assert!(!metadata.destructive);
    }

    #[test]
    fn tool_metadata_start_session_is_runtime_read() {
        let metadata = lookup_tool_metadata("start_session").unwrap();
        assert_eq!(metadata.provider_id, TOOL_PROVIDER_CONTROL);
        assert_eq!(metadata.risk, ToolRisk::ReadOnly);
        assert_eq!(metadata.legacy_oauth_scope_hint, Some(SCOPE_RUNTIME_READ));
        assert!(!metadata.requires_project);
        assert!(metadata.read_only);
    }

    #[test]
    fn checkpoint_metadata_uses_project_read_and_write_scopes() {
        for name in [
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, TOOL_PROVIDER_NATIVE, "{name}");
            assert_eq!(metadata.risk, ToolRisk::ReadOnly, "{name}");
            assert_eq!(
                metadata.legacy_oauth_scope_hint,
                Some(SCOPE_PROJECT_READ),
                "{name}"
            );
            assert!(metadata.requires_project, "{name}");
            assert!(metadata.read_only, "{name}");
        }
        for name in [
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, TOOL_PROVIDER_NATIVE, "{name}");
            assert_eq!(metadata.risk, ToolRisk::ProjectWrite, "{name}");
            assert_eq!(
                metadata.legacy_oauth_scope_hint,
                Some(PROJECT_WRITE),
                "{name}"
            );
            assert!(metadata.requires_project, "{name}");
            assert!(!metadata.read_only, "{name}");
        }
    }

    #[test]
    fn tool_metadata_write_tools_are_project_write() {
        for name in [
            "write_project_file",
            "apply_text_edits",
            "apply_unified_diff",
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
            "unregister_project",
            "create_project",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.risk, ToolRisk::ProjectWrite, "{name}");
            assert_eq!(
                metadata.legacy_oauth_scope_hint,
                Some(SCOPE_PROJECT_WRITE),
                "{name}"
            );
            assert!(!metadata.read_only, "{name}");
        }
    }

    #[test]
    fn tool_metadata_job_tools_are_job_run() {
        for name in [
            "run_shell",
            "run_job",
            "stop_job",
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "go_test",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.risk, ToolRisk::JobRun, "{name}");
            assert_eq!(
                metadata.legacy_oauth_scope_hint,
                Some(SCOPE_JOB_RUN),
                "{name}"
            );
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
