//! Runtime tool definitions.
//!
//! This module is the central declaration point for runtime tool names,
//! model-facing visibility, manifest category, runtime metadata, and agent
//! capability. Non-runtime route metadata fallbacks remain in `metadata.rs`
//! while the registry migration proceeds in small steps.

#![allow(dead_code)]

mod artifacts;
mod checkpoints;
mod current_sessions;
mod discovery;
mod edits;
mod files;
mod git;
mod hygiene;
mod jobs;
mod patches;
mod sessions;
mod testing;

use super::metadata::{
    metadata as make_tool_metadata, ToolMetadata, ToolPathHint, ToolRisk, RUNTIME_READ,
    TOOL_PROVIDER_CONTROL,
};
pub(crate) use super::tool_catalog::{TOOL_DISCOVERY_GROUPS, TOOL_RECOMMENDED_FLOWS};
#[cfg(test)]
pub(crate) use super::tool_catalog::{
    TOOL_DISCOVERY_GROUP_CHECKPOINT, TOOL_DISCOVERY_GROUP_CLEANUP, TOOL_DISCOVERY_GROUP_EDIT,
    TOOL_DISCOVERY_GROUP_GIT, TOOL_DISCOVERY_GROUP_INSPECT, TOOL_DISCOVERY_GROUP_JOBS,
    TOOL_DISCOVERY_GROUP_PATCH, TOOL_DISCOVERY_GROUP_PROJECTS, TOOL_DISCOVERY_GROUP_REVIEW,
    TOOL_DISCOVERY_GROUP_RUNTIME, TOOL_DISCOVERY_GROUP_SHELL, TOOL_DISCOVERY_GROUP_VALIDATION,
};
pub use super::tool_policy::is_known_tool_name;
#[allow(unused_imports)]
pub(crate) use super::tool_policy::{
    is_model_hidden_tool_name, is_model_visible_tool_name, known_tool_names,
    lookup_tool_definition, model_hidden_tool_names, model_visible_tool_definitions,
    model_visible_tool_names_csv, runtime_tool_agent_capability,
    runtime_tool_allows_current_session_fallback, runtime_tool_captures_validation_output,
    runtime_tool_category, runtime_tool_creates_or_binds_session, runtime_tool_disabled_message,
    runtime_tool_extra_accepted_flattened_args, runtime_tool_is_change_summary_like,
    runtime_tool_is_current_session_control, runtime_tool_is_git_like, runtime_tool_is_read_like,
    runtime_tool_is_shell_like, runtime_tool_is_write_like, runtime_tool_metadata,
    runtime_tool_permission_risk, runtime_tool_requires_explicit_business_session,
    runtime_tool_requires_permission, runtime_tool_requires_session_project_escape,
    runtime_tool_session_risk_class,
};
use crate::shell_protocol::{
    SHELL_CLIENT_CAPABILITY_ASYNC_JOBS, SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_FILE_READ, SHELL_CLIENT_CAPABILITY_FILE_WRITE,
    SHELL_CLIENT_CAPABILITY_GIT, SHELL_CLIENT_CAPABILITY_SHELL,
};

/// Capability an agent-backed tool requires before dispatch can reach an
/// agent-backed project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentCapability {
    /// Project-scoped native tools that do not require an agent capability but
    /// still need the agent owner boundary when the project is agent-backed.
    OwnerOnly,
    /// `run_shell`, `apply_patch` (agent path runs `git apply` via shell).
    Shell,
    /// `read_file` (agent path uses the file_read request kind).
    FileRead,
    /// Native file mutation requests handled by the agent.
    FileWrite,
    /// `git_status` / `git_diff` (agent path runs git via shell; accept either
    /// an explicit `git` capability or `shell`).
    GitOrShell,
    /// `run_job` / `run_codex` (agent path starts an async job).
    AsyncJobs,
}

impl AgentCapability {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OwnerOnly => "owner boundary",
            Self::Shell => SHELL_CLIENT_CAPABILITY_SHELL,
            Self::FileRead => SHELL_CLIENT_CAPABILITY_FILE_READ,
            Self::FileWrite => SHELL_CLIENT_CAPABILITY_FILE_WRITE,
            Self::GitOrShell => "shell or git",
            Self::AsyncJobs => "async shell jobs",
        }
    }

    pub(crate) fn registry_capabilities(self) -> &'static [&'static str] {
        match self {
            Self::OwnerOnly => &[],
            Self::Shell => &[SHELL_CLIENT_CAPABILITY_SHELL],
            Self::FileRead => &[SHELL_CLIENT_CAPABILITY_FILE_READ],
            Self::FileWrite => &[SHELL_CLIENT_CAPABILITY_FILE_WRITE],
            Self::GitOrShell => &[SHELL_CLIENT_CAPABILITY_SHELL, SHELL_CLIENT_CAPABILITY_GIT],
            Self::AsyncJobs => &[
                SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
                SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
            ],
        }
    }

    pub(crate) fn is_owner_only(self) -> bool {
        matches!(self, Self::OwnerOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolVisibility {
    ModelVisible,
    ModelHidden,
}

impl ToolVisibility {
    pub(crate) fn is_model_hidden(self) -> bool {
        matches!(self, Self::ModelHidden)
    }

    pub(crate) fn is_model_visible(self) -> bool {
        matches!(self, Self::ModelVisible)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolDefinition {
    pub(crate) name: &'static str,
    pub(crate) visibility: ToolVisibility,
    pub(crate) category: &'static str,
    pub(crate) metadata: ToolMetadata,
    pub(crate) policy: ToolDefinitionPolicy,
    /// Agent capability required before dispatch reaches an agent-backed
    /// project. `None` means the tool is not agent-dispatched or enforces its
    /// ownership boundary inside a specialized handler.
    pub(crate) agent_capability: Option<AgentCapability>,
}

pub(crate) const TOOL_CATEGORY_ARTIFACT: &str = "artifact";
pub(crate) const TOOL_CATEGORY_CHECKPOINT: &str = "checkpoint";
pub(crate) const TOOL_CATEGORY_CLEANUP: &str = "cleanup";
pub(crate) const TOOL_CATEGORY_CODEX: &str = "codex";
pub(crate) const TOOL_CATEGORY_EDIT: &str = "edit";
pub(crate) const TOOL_CATEGORY_FILE: &str = "file";
pub(crate) const TOOL_CATEGORY_GIT: &str = "git";
pub(crate) const TOOL_CATEGORY_JOB: &str = "job";
pub(crate) const TOOL_CATEGORY_PATCH: &str = "patch";
pub(crate) const TOOL_CATEGORY_PROJECT: &str = "project";
pub(crate) const TOOL_CATEGORY_RUNTIME: &str = "runtime";
pub(crate) const TOOL_CATEGORY_SESSION: &str = "session";
pub(crate) const TOOL_CATEGORY_VALIDATION: &str = "validation";

pub(crate) const PERMISSION_RISK_ARTIFACT_WRITE: &str = "artifact_write";
pub(crate) const PERMISSION_RISK_DESTRUCTIVE: &str = "destructive";
pub(crate) const PERMISSION_RISK_JOB: &str = "job";
pub(crate) const PERMISSION_RISK_PATCH: &str = "patch";
pub(crate) const PERMISSION_RISK_SHELL: &str = "shell";
pub(crate) const PERMISSION_RISK_VALIDATION: &str = "validation";
pub(crate) const PERMISSION_RISK_WRITE: &str = "write";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolDefinitionPolicy {
    pub(crate) change_summary_like: bool,
    pub(crate) captures_validation_output: bool,
    pub(crate) current_session_control: bool,
    pub(crate) creates_or_binds_session: bool,
    pub(crate) disabled_message: Option<&'static str>,
    pub(crate) extra_accepted_flattened_args: &'static [&'static str],
    pub(crate) git_like: bool,
    pub(crate) permission_risk: Option<&'static str>,
    pub(crate) requires_artifact_upload_path_binding: bool,
    pub(crate) requires_explicit_business_session: bool,
    pub(crate) unit_arguments: bool,
}

impl ToolDefinitionPolicy {
    const DEFAULT: Self = Self {
        change_summary_like: false,
        captures_validation_output: false,
        current_session_control: false,
        creates_or_binds_session: false,
        disabled_message: None,
        extra_accepted_flattened_args: &[],
        git_like: false,
        permission_risk: None,
        requires_artifact_upload_path_binding: false,
        requires_explicit_business_session: false,
        unit_arguments: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolDiscoveryGroup {
    pub(crate) name: &'static str,
    pub(crate) tools: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolRecommendedFlow {
    pub(crate) name: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) manifest_purpose: &'static str,
    pub(crate) tools: &'static [&'static str],
}

const fn def(
    name: &'static str,
    visibility: ToolVisibility,
    category: &'static str,
    agent_capability: Option<AgentCapability>,
    provider_id: &'static str,
    risk: ToolRisk,
    oauth_scope: Option<&'static str>,
    requires_project: bool,
    path_hint: ToolPathHint,
    destructive: bool,
    shell_like: bool,
) -> ToolDefinition {
    ToolDefinition {
        name,
        visibility,
        category,
        metadata: make_tool_metadata(
            name,
            provider_id,
            risk,
            oauth_scope,
            requires_project,
            path_hint,
            destructive,
            shell_like,
        ),
        policy: ToolDefinitionPolicy::DEFAULT,
        agent_capability,
    }
}

const fn captures_validation_output(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            captures_validation_output: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn change_summary_like(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            change_summary_like: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn current_session_control(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            current_session_control: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn git_like(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            git_like: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn creates_or_binds_session(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            creates_or_binds_session: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn disabled(definition: ToolDefinition, message: &'static str) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            disabled_message: Some(message),
            ..definition.policy
        },
        ..definition
    }
}

const fn extra_accepted_flattened_args(
    definition: ToolDefinition,
    fields: &'static [&'static str],
) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            extra_accepted_flattened_args: fields,
            ..definition.policy
        },
        ..definition
    }
}

const fn permission_risk(
    definition: ToolDefinition,
    permission_risk: &'static str,
) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            permission_risk: Some(permission_risk),
            ..definition.policy
        },
        ..definition
    }
}

const fn requires_artifact_upload_path_binding(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            requires_artifact_upload_path_binding: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn unit_arguments(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            unit_arguments: true,
            ..definition.policy
        },
        ..definition
    }
}

const fn requires_explicit_business_session(definition: ToolDefinition) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            requires_explicit_business_session: true,
            ..definition.policy
        },
        ..definition
    }
}

use ToolPathHint::None as NoPath;
use ToolRisk::ReadOnly;
use ToolVisibility::ModelVisible;

pub(crate) fn tool_definitions() -> impl Iterator<Item = &'static ToolDefinition> {
    TOOL_DEFINITION_GROUPS
        .iter()
        .flat_map(|definitions| definitions.iter())
}

const TOOL_DEFINITION_GROUPS: &[&[ToolDefinition]] = &[
    TOOL_DEFINITION_HEAD,
    sessions::DEFINITIONS,
    hygiene::DEFINITIONS,
    current_sessions::DEFINITIONS,
    checkpoints::DEFINITIONS,
    discovery::DEFINITIONS,
    jobs::EXECUTION_DEFINITIONS,
    files::SEARCH_DEFINITIONS,
    git::SUMMARY_DEFINITIONS,
    jobs::LISTING_DEFINITIONS,
    files::READ_DEFINITIONS,
    git::DETAIL_DEFINITIONS,
    testing::DEFINITIONS,
    patches::APPLY_DEFINITIONS,
    hygiene::CLEANUP_DEFINITIONS,
    patches::VALIDATION_DEFINITIONS,
    edits::COMPATIBILITY_DEFINITIONS,
    artifacts::DEFINITIONS,
    edits::LINE_DEFINITIONS,
];

const TOOL_DEFINITION_HEAD: &[ToolDefinition] = &[def(
    "list_tools",
    ModelVisible,
    TOOL_CATEGORY_RUNTIME,
    None,
    TOOL_PROVIDER_CONTROL,
    ReadOnly,
    Some(RUNTIME_READ),
    false,
    NoPath,
    false,
    false,
)];
