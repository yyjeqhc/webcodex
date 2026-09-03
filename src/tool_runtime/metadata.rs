#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolRisk {
    /// Data exposure / observation risk. This says nothing about whether the
    /// tool has side effects; `ToolEffect` is authoritative for that question.
    Read,
    ProjectWrite,
    SkillManage,
    MemoryManage,
    CommunicationManage,
    SessionCollaborate,
    WorkflowManage,
    CheckpointManage,
    RunControl,
    ComputerControl,
    JobRun,
    Unknown,
}

impl ToolRisk {
    pub(crate) fn session_risk_class(self) -> &'static str {
        match self {
            // Preserve the existing external risk label while separating
            // read-only effect semantics from risk classification internally.
            ToolRisk::Read => "read_only",
            ToolRisk::ProjectWrite => "project_write",
            ToolRisk::SkillManage => "skill_manage",
            ToolRisk::MemoryManage => "memory_manage",
            ToolRisk::CommunicationManage => "communication_manage",
            ToolRisk::SessionCollaborate => "session_collaborate",
            ToolRisk::WorkflowManage => "workflow_manage",
            ToolRisk::CheckpointManage => "checkpoint_manage",
            ToolRisk::RunControl => "run_control",
            ToolRisk::ComputerControl => "computer_control",
            ToolRisk::JobRun => "job_run",
            ToolRisk::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolEffect {
    Observe,
    Mutate,
    Execute,
    Unknown,
}

impl ToolEffect {
    pub(crate) const fn read_only_hint(self) -> bool {
        matches!(self, Self::Observe)
    }

    pub(crate) const fn manifest_label(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Mutate => "mutate",
            Self::Execute => "execute",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolApprovalPolicy {
    None,
    Standard,
    InheritFromStart,
    Unknown,
}

impl ToolApprovalPolicy {
    pub(crate) const fn requires_permission(self) -> bool {
        matches!(self, Self::Standard | Self::Unknown)
    }

    pub(crate) const fn manifest_label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Standard => "standard",
            Self::InheritFromStart => "inherit_from_start",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolIdempotency {
    PureRead,
    DesiredState,
    Keyed,
    FencedReplay,
    NonIdempotent,
    Unknown,
}

impl ToolIdempotency {
    pub(crate) const fn mcp_hint(self) -> bool {
        matches!(
            self,
            Self::PureRead | Self::DesiredState | Self::Keyed | Self::FencedReplay
        )
    }

    pub(crate) const fn manifest_label(self) -> &'static str {
        match self {
            Self::PureRead => "pure_read",
            Self::DesiredState => "desired_state",
            Self::Keyed => "keyed",
            Self::FencedReplay => "fenced_replay",
            Self::NonIdempotent => "non_idempotent",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolSemanticContract {
    pub(crate) effect: ToolEffect,
    pub(crate) risk: ToolRisk,
    pub(crate) approval: ToolApprovalPolicy,
    pub(crate) idempotency: ToolIdempotency,
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

pub(crate) use webcodex_core::authority::ToolAuthorityPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolMetadata {
    pub(crate) name: &'static str,
    pub(crate) provider_id: &'static str,
    pub(crate) effect: ToolEffect,
    pub(crate) risk: ToolRisk,
    pub(crate) approval: ToolApprovalPolicy,
    pub(crate) idempotency: ToolIdempotency,
    /// Canonical credential authority for this runtime tool. Discovery and
    /// execution must both derive from this policy rather than tool-name switches.
    pub(crate) authority: ToolAuthorityPolicy,
    pub(crate) requires_project: bool,
    pub(crate) path_hint: ToolPathHint,
    pub(crate) destructive: bool,
    pub(crate) shell_like: bool,
}

pub(crate) const RUNTIME_READ: &str = webcodex_core::authority::SCOPE_RUNTIME_READ;
pub(crate) const SESSION_COLLABORATE: &str = webcodex_core::authority::SCOPE_SESSION_COLLABORATE;
pub(crate) const COMMUNICATION_READ: &str = webcodex_core::authority::SCOPE_COMMUNICATION_READ;
pub(crate) const COMMUNICATION_MANAGE: &str = webcodex_core::authority::SCOPE_COMMUNICATION_MANAGE;
pub(crate) const PROJECT_READ: &str = webcodex_core::authority::SCOPE_PROJECT_READ;
pub(crate) const PROJECT_WRITE: &str = webcodex_core::authority::SCOPE_PROJECT_WRITE;
pub(crate) const ADMIN: &str = webcodex_core::authority::SCOPE_ADMIN;
pub(crate) const JOB_RUN: &str = webcodex_core::authority::SCOPE_JOB_RUN;
pub(crate) const CODING_AGENT_RUN: &str = webcodex_core::authority::SCOPE_CODING_AGENT_RUN;
pub(crate) const COMPUTER_READ: &str = webcodex_core::authority::SCOPE_COMPUTER_READ;
pub(crate) const COMPUTER_CONTROL: &str = webcodex_core::authority::SCOPE_COMPUTER_CONTROL;
pub(crate) const COMPUTER_LAUNCH: &str = webcodex_core::authority::SCOPE_COMPUTER_LAUNCH;
pub(crate) const COMPUTER_DISPLAY_READ: &str =
    webcodex_core::authority::SCOPE_COMPUTER_DISPLAY_READ;
pub(crate) const COMPUTER_POINTER_CONTROL: &str =
    webcodex_core::authority::SCOPE_COMPUTER_POINTER_CONTROL;
pub(crate) const COMPUTER_CLIPBOARD_READ: &str =
    webcodex_core::authority::SCOPE_COMPUTER_CLIPBOARD_READ;
pub(crate) const COMPUTER_CLIPBOARD_WRITE: &str =
    webcodex_core::authority::SCOPE_COMPUTER_CLIPBOARD_WRITE;

pub(crate) const TOOL_PROVIDER_AGENT: &str = "agent";
pub(crate) const TOOL_PROVIDER_CONTROL: &str = "control";
pub(crate) const TOOL_PROVIDER_NATIVE: &str = "native";
pub(crate) const TOOL_PROVIDER_UNKNOWN: &str = "unknown";

pub(crate) const fn metadata(
    name: &'static str,
    provider_id: &'static str,
    semantic: ToolSemanticContract,
    required_scope: Option<&'static str>,
    requires_project: bool,
    path_hint: ToolPathHint,
    destructive: bool,
    shell_like: bool,
) -> ToolMetadata {
    ToolMetadata {
        name,
        provider_id,
        effect: semantic.effect,
        risk: semantic.risk,
        approval: semantic.approval,
        idempotency: semantic.idempotency,
        authority: match required_scope {
            Some(scope) => ToolAuthorityPolicy::Require(scope),
            None => ToolAuthorityPolicy::Unknown,
        },
        requires_project,
        path_hint,
        destructive,
        shell_like,
    }
}

pub(crate) fn lookup_tool_metadata(name: &str) -> Option<&'static ToolMetadata> {
    super::tool_definition::lookup_tool_definition(name).map(|definition| &definition.metadata)
}

#[cfg(test)]
pub(crate) fn iter_tool_metadata() -> impl Iterator<Item = ToolMetadata> {
    super::tool_definition::tool_definitions().map(|definition| definition.metadata())
}

pub(crate) fn tool_metadata(name: &str) -> ToolMetadata {
    lookup_tool_metadata(name).copied().unwrap_or(ToolMetadata {
        name: "<unknown>",
        provider_id: TOOL_PROVIDER_UNKNOWN,
        effect: ToolEffect::Unknown,
        risk: ToolRisk::Unknown,
        approval: ToolApprovalPolicy::Unknown,
        idempotency: ToolIdempotency::Unknown,
        authority: ToolAuthorityPolicy::Unknown,
        requires_project: false,
        path_hint: ToolPathHint::None,
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
        assert_eq!(metadata.effect, ToolEffect::Unknown);
        assert_eq!(metadata.risk, ToolRisk::Unknown);
        assert_eq!(metadata.approval, ToolApprovalPolicy::Unknown);
        assert_eq!(metadata.idempotency, ToolIdempotency::Unknown);
        assert_eq!(metadata.authority, ToolAuthorityPolicy::Unknown);
        assert!(!metadata.destructive);
        assert!(!metadata.shell_like);
    }

    #[test]
    fn tool_metadata_show_changes_is_project_read_observation() {
        let metadata = lookup_tool_metadata("show_changes").unwrap();
        assert_eq!(metadata.provider_id, TOOL_PROVIDER_AGENT);
        assert_eq!(metadata.effect, ToolEffect::Observe);
        assert_eq!(metadata.risk, ToolRisk::Read);
        assert_eq!(metadata.approval, ToolApprovalPolicy::None);
        assert_eq!(metadata.idempotency, ToolIdempotency::PureRead);
        assert_eq!(
            metadata.authority,
            ToolAuthorityPolicy::Require(PROJECT_READ)
        );
        assert!(metadata.requires_project);
        assert!(!metadata.destructive);
    }

    #[test]
    fn tool_metadata_start_session_is_workflow_mutation_without_new_approval() {
        let metadata = lookup_tool_metadata("start_session").unwrap();
        assert_eq!(metadata.provider_id, TOOL_PROVIDER_CONTROL);
        assert_eq!(metadata.effect, ToolEffect::Mutate);
        assert_eq!(metadata.risk, ToolRisk::WorkflowManage);
        assert_eq!(metadata.approval, ToolApprovalPolicy::None);
        assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
        assert_eq!(
            metadata.authority,
            ToolAuthorityPolicy::Require(SCOPE_RUNTIME_READ)
        );
        assert!(!metadata.requires_project);
    }

    #[test]
    fn checkpoint_metadata_separates_effect_from_existing_authority() {
        let create = lookup_tool_metadata("workspace_checkpoint_create").unwrap();
        assert_eq!(create.provider_id, TOOL_PROVIDER_NATIVE);
        assert_eq!(create.effect, ToolEffect::Mutate);
        assert_eq!(create.risk, ToolRisk::CheckpointManage);
        assert_eq!(create.approval, ToolApprovalPolicy::None);
        assert_eq!(create.idempotency, ToolIdempotency::NonIdempotent);
        assert_eq!(
            create.authority,
            ToolAuthorityPolicy::Require(SCOPE_PROJECT_READ)
        );
        assert!(create.requires_project);

        for name in ["workspace_checkpoint_list", "workspace_checkpoint_show"] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, TOOL_PROVIDER_NATIVE, "{name}");
            assert_eq!(metadata.effect, ToolEffect::Observe, "{name}");
            assert_eq!(metadata.risk, ToolRisk::Read, "{name}");
            assert_eq!(metadata.approval, ToolApprovalPolicy::None, "{name}");
            assert_eq!(metadata.idempotency, ToolIdempotency::PureRead, "{name}");
            assert_eq!(
                metadata.authority,
                ToolAuthorityPolicy::Require(SCOPE_PROJECT_READ),
                "{name}"
            );
            assert!(metadata.requires_project, "{name}");
        }
        for name in [
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
        ] {
            let metadata = lookup_tool_metadata(name).unwrap();
            assert_eq!(metadata.provider_id, TOOL_PROVIDER_NATIVE, "{name}");
            assert_eq!(metadata.effect, ToolEffect::Mutate, "{name}");
            assert_eq!(metadata.risk, ToolRisk::ProjectWrite, "{name}");
            assert_eq!(metadata.approval, ToolApprovalPolicy::Standard, "{name}");
            assert_eq!(
                metadata.idempotency,
                ToolIdempotency::NonIdempotent,
                "{name}"
            );
            assert_eq!(
                metadata.authority,
                ToolAuthorityPolicy::Require(PROJECT_WRITE),
                "{name}"
            );
            assert!(metadata.requires_project, "{name}");
        }
    }

    #[test]
    fn tool_metadata_write_tools_are_project_write() {
        for name in [
            "write_project_file",
            "apply_text_edits",
            "apply_patch",
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
                metadata.authority,
                ToolAuthorityPolicy::Require(SCOPE_PROJECT_WRITE),
                "{name}"
            );
            assert_eq!(metadata.effect, ToolEffect::Mutate, "{name}");
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
                metadata.authority,
                ToolAuthorityPolicy::Require(SCOPE_JOB_RUN),
                "{name}"
            );
        }
    }
}
