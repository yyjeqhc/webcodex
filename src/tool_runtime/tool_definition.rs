//! Runtime tool definitions.
//!
//! This module is the central declaration point for runtime tool names,
//! model-facing visibility, manifest category, runtime metadata, and agent
//! capability. Compatibility snapshots and non-runtime route metadata fallbacks
//! remain while the registry migration proceeds in small steps.

#![allow(dead_code)]

use super::metadata::{
    metadata as make_tool_metadata, tool_metadata as fallback_tool_metadata, ToolMetadata,
    ToolPathHint, ToolRisk, JOB_RUN, PROJECT_READ, PROJECT_WRITE, RUNTIME_READ,
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
    /// Agent capability required before dispatch reaches an agent-backed
    /// project. `None` means the tool is not agent-dispatched or enforces its
    /// ownership boundary inside a specialized handler.
    pub(crate) agent_capability: Option<AgentCapability>,
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

impl ToolDefinition {
    pub(crate) fn metadata(self) -> ToolMetadata {
        self.metadata
    }

    pub(crate) fn oauth_scope(self) -> Option<&'static str> {
        self.metadata.oauth_scope
    }

    pub(crate) fn session_risk_class(self) -> &'static str {
        self.metadata.risk.session_risk_class()
    }

    pub(crate) fn is_read_like(self) -> bool {
        self.metadata.read_only
    }

    pub(crate) fn is_write_like(self) -> bool {
        self.metadata.risk == ToolRisk::ProjectWrite
    }

    pub(crate) fn is_shell_like(self) -> bool {
        self.metadata.shell_like || self.metadata.risk == ToolRisk::JobRun
    }

    pub(crate) fn is_git_like(self) -> bool {
        tool_is_in_discovery_group(self.name, "git")
    }

    pub(crate) fn is_change_summary_like(self) -> bool {
        matches!(
            self.name,
            "show_changes" | "git_diff_summary" | "git_diff_hunks"
        )
    }

    pub(crate) fn captures_validation_output(self) -> bool {
        matches!(self.name, "cargo_fmt" | "cargo_check" | "cargo_test")
    }

    pub(crate) fn is_current_session_control(self) -> bool {
        matches!(
            self.name,
            "bind_current_session" | "current_session" | "unbind_current_session"
        )
    }

    pub(crate) fn requires_explicit_business_session(self) -> bool {
        matches!(
            self.name,
            "finish_coding_task"
                | "session_summary"
                | "post_session_message"
                | "list_session_messages"
                | "resolve_session_message"
                | "session_discussion_summary"
                | "session_handoff_summary"
        )
    }

    pub(crate) fn creates_or_binds_session(self) -> bool {
        matches!(
            self.name,
            "start_session" | "start_coding_task" | "bind_current_session"
        )
    }

    pub(crate) fn allows_current_session_fallback(self) -> bool {
        self.metadata.requires_project
            && !self.is_current_session_control()
            && !self.requires_explicit_business_session()
            && !self.creates_or_binds_session()
    }

    pub(crate) fn requires_session_project_escape(self) -> bool {
        !self.metadata.read_only || self.metadata.destructive || self.metadata.shell_like
    }

    pub(crate) fn requires_permission(self) -> bool {
        !self.metadata.read_only || self.metadata.destructive || self.metadata.shell_like
    }

    pub(crate) fn permission_risk(self) -> &'static str {
        if self.captures_validation_output() {
            return "validation";
        }
        if matches!(self.name, "run_job" | "stop_job" | "run_codex") {
            return "job";
        }
        if self.metadata.shell_like {
            return "shell";
        }
        if self.metadata.destructive {
            return "destructive";
        }
        if self.metadata.path_hint == ToolPathHint::Artifact {
            return "artifact_write";
        }
        if self.metadata.path_hint == ToolPathHint::Patch || self.name.contains("patch") {
            return "patch";
        }
        if matches!(
            self.metadata.risk,
            ToolRisk::ProjectWrite | ToolRisk::AccountManage
        ) {
            return "write";
        }
        "write"
    }
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
        agent_capability,
    }
}

use AgentCapability::{AsyncJobs, FileRead, FileWrite, GitOrShell, OwnerOnly, Shell};
use ToolPathHint::{Artifact, None as NoPath, Patch, PathList, SinglePath};
use ToolRisk::{JobRun, ProjectWrite, ReadOnly};
use ToolVisibility::{ModelHidden, ModelVisible};

pub(crate) const TOOL_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "list_tools",
        ModelVisible,
        "runtime",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "start_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "start_coding_task",
        ModelVisible,
        "workflow",
        Some(GitOrShell),
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "finish_coding_task",
        ModelVisible,
        "workflow",
        Some(GitOrShell),
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "session_summary",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "post_session_message",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "list_session_messages",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "resolve_session_message",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "session_discussion_summary",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "session_handoff_summary",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "workspace_hygiene_check",
        ModelVisible,
        "cleanup",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "bind_current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "unbind_current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "workspace_checkpoint_create",
        ModelVisible,
        "checkpoint",
        Some(FileRead),
        "native",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "workspace_checkpoint_list",
        ModelVisible,
        "checkpoint",
        Some(OwnerOnly),
        "native",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "workspace_checkpoint_show",
        ModelVisible,
        "checkpoint",
        Some(OwnerOnly),
        "native",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "workspace_checkpoint_restore",
        ModelVisible,
        "checkpoint",
        Some(FileWrite),
        "native",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Patch,
        false,
        false,
    ),
    def(
        "workspace_checkpoint_delete",
        ModelVisible,
        "checkpoint",
        Some(OwnerOnly),
        "native",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        NoPath,
        true,
        false,
    ),
    def(
        "list_projects",
        ModelVisible,
        "project",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "register_project",
        ModelVisible,
        "project",
        None,
        "control",
        ProjectWrite,
        Some(PROJECT_WRITE),
        false,
        NoPath,
        true,
        false,
    ),
    def(
        "create_project",
        ModelVisible,
        "project",
        None,
        "control",
        ProjectWrite,
        Some(PROJECT_WRITE),
        false,
        NoPath,
        true,
        false,
    ),
    def(
        "list_agents",
        ModelVisible,
        "runtime",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "runtime_status",
        ModelVisible,
        "runtime",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "tool_manifest",
        ModelVisible,
        "runtime",
        None,
        "control",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "run_shell",
        ModelVisible,
        "job",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        true,
        true,
    ),
    def(
        "run_job",
        ModelVisible,
        "job",
        Some(AsyncJobs),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        true,
        true,
    ),
    def(
        "stop_job",
        ModelVisible,
        "job",
        None,
        "native",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        true,
        false,
    ),
    def(
        "run_codex",
        ModelHidden,
        "codex",
        Some(AsyncJobs),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        true,
        true,
    ),
    def(
        "job_status",
        ModelVisible,
        "job",
        None,
        "native",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "job_log",
        ModelVisible,
        "job",
        None,
        "native",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "list_project_files",
        ModelVisible,
        "file",
        Some(FileRead),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "search_project_text",
        ModelVisible,
        "file",
        Some(Shell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "git_diff_summary",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "show_changes",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "list_jobs",
        ModelVisible,
        "job",
        None,
        "native",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "job_tail",
        ModelVisible,
        "job",
        None,
        "native",
        ReadOnly,
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "read_file",
        ModelVisible,
        "file",
        Some(FileRead),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "git_status",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "git_diff",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "git_diff_hunks",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "git_log",
        ModelVisible,
        "git",
        Some(GitOrShell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "cargo_fmt",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "cargo_check",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "cargo_test",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "apply_patch",
        ModelVisible,
        "patch",
        Some(Shell),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Patch,
        false,
        false,
    ),
    def(
        "apply_patch_checked",
        ModelVisible,
        "patch",
        Some(Shell),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Patch,
        false,
        false,
    ),
    def(
        "delete_project_files",
        ModelVisible,
        "cleanup",
        Some(Shell),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    ),
    def(
        "git_restore_paths",
        ModelVisible,
        "cleanup",
        Some(Shell),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    ),
    def(
        "discard_untracked",
        ModelVisible,
        "cleanup",
        Some(Shell),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    ),
    def(
        "validate_patch",
        ModelVisible,
        "patch",
        Some(Shell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        Patch,
        false,
        false,
    ),
    def(
        "replace_in_file",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "replace_exact_block",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "insert_before_pattern",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "insert_after_pattern",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "write_project_file",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "save_project_artifact",
        ModelVisible,
        "artifact",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "read_project_artifact_metadata",
        ModelVisible,
        "artifact",
        Some(FileRead),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "read_project_artifact",
        ModelVisible,
        "artifact",
        Some(FileRead),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "artifact_upload_begin",
        ModelVisible,
        "artifact",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "artifact_upload_chunk",
        ModelVisible,
        "artifact",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "artifact_upload_finish",
        ModelVisible,
        "artifact",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "artifact_upload_abort",
        ModelVisible,
        "artifact",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        Artifact,
        false,
        false,
    ),
    def(
        "replace_line_range",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "insert_at_line",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "delete_line_range",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "apply_text_edits",
        ModelVisible,
        "edit",
        Some(FileWrite),
        "agent",
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
];

pub(crate) const TOOL_DISCOVERY_GROUPS: &[ToolDiscoveryGroup] = &[
    ToolDiscoveryGroup {
        name: "inspect",
        tools: &[
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
            "start_coding_task",
            "read_file",
            "search_project_text",
            "show_changes",
            "list_project_files",
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "git_log",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
        ],
    },
    ToolDiscoveryGroup {
        name: "projects",
        tools: &["list_projects", "register_project", "create_project"],
    },
    ToolDiscoveryGroup {
        name: "git",
        tools: &[
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "git_log",
            "show_changes",
            "git_restore_paths",
            "discard_untracked",
            "workspace_checkpoint_create",
            "workspace_checkpoint_restore",
        ],
    },
    ToolDiscoveryGroup {
        name: "review",
        tools: &[
            "finish_coding_task",
            "show_changes",
            "git_diff_hunks",
            "workspace_hygiene_check",
            "git_diff_summary",
            "git_log",
            "git_status",
            "git_diff",
            "workspace_checkpoint_show",
            "workspace_checkpoint_list",
        ],
    },
    ToolDiscoveryGroup {
        name: "validation",
        tools: &[
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "validate_patch",
            "apply_patch_checked",
        ],
    },
    ToolDiscoveryGroup {
        name: "patch",
        tools: &["apply_patch", "apply_patch_checked", "validate_patch"],
    },
    ToolDiscoveryGroup {
        name: "edit",
        tools: &[
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
            "replace_in_file",
            "replace_exact_block",
            "insert_before_pattern",
            "insert_after_pattern",
            "write_project_file",
            "save_project_artifact",
            "read_project_artifact_metadata",
            "read_project_artifact",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
        ],
    },
    ToolDiscoveryGroup {
        name: "shell",
        tools: &[
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "run_shell",
            "run_job",
        ],
    },
    ToolDiscoveryGroup {
        name: "jobs",
        tools: &[
            "run_job",
            "stop_job",
            "job_status",
            "job_log",
            "list_jobs",
            "job_tail",
        ],
    },
    ToolDiscoveryGroup {
        name: "runtime",
        tools: &[
            "list_tools",
            "start_session",
            "start_coding_task",
            "finish_coding_task",
            "session_summary",
            "post_session_message",
            "list_session_messages",
            "resolve_session_message",
            "session_discussion_summary",
            "session_handoff_summary",
            "bind_current_session",
            "current_session",
            "unbind_current_session",
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
            "list_projects",
            "list_agents",
            "runtime_status",
            "tool_manifest",
        ],
    },
    ToolDiscoveryGroup {
        name: "cleanup",
        tools: &[
            "delete_project_files",
            "git_restore_paths",
            "discard_untracked",
            "workspace_checkpoint_delete",
        ],
    },
    ToolDiscoveryGroup {
        name: "checkpoint",
        tools: &[
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
        ],
    },
];

pub(crate) const TOOL_RECOMMENDED_FLOWS: &[ToolRecommendedFlow] = &[
    ToolRecommendedFlow {
        name: "discovery",
        summary:
            "Discovery: resolve project with list_projects/runtime_status, then load rules/context with read_file before editing.",
        manifest_purpose: "Resolve the project and load rules/context before editing.",
        tools: &[
            "start_coding_task",
            "list_projects",
            "runtime_status",
            "read_file",
        ],
    },
    ToolRecommendedFlow {
        name: "inspect",
        summary: "Inspect: use read_file, search_project_text, and show_changes before editing.",
        manifest_purpose: "Use the default inspect tools before editing.",
        tools: &["read_file", "search_project_text", "show_changes"],
    },
    ToolRecommendedFlow {
        name: "edit",
        summary:
            "Edit: prefer replace_line_range / insert_at_line / delete_line_range for local line edits; use apply_text_edits for batches; use apply_patch_checked for broad diffs.",
        manifest_purpose:
            "Prefer structured line edits, batch text edits, or checked patches for source changes.",
        tools: &[
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
        ],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use cargo_check / cargo_test / validate_patch when applicable. raw run_shell is a bounded escape hatch, not the primary editing or validation path.",
        manifest_purpose:
            "Use structured validation; run_shell is a bounded diagnostics escape hatch, not the primary validation path.",
        tools: &["cargo_check", "cargo_test", "validate_patch", "run_shell"],
    },
    ToolRecommendedFlow {
        name: "review",
        summary: "Review: use show_changes / git_diff_hunks / workspace_hygiene_check before final response.",
        manifest_purpose: "Review diffs and workspace hygiene before the final response.",
        tools: &["show_changes", "git_diff_hunks", "workspace_hygiene_check"],
    },
    ToolRecommendedFlow {
        name: "handoff",
        summary: "Handoff: use session_summary / session_handoff_summary when a task spans multiple steps.",
        manifest_purpose: "Summarize or hand off multi-step session state.",
        tools: &[
            "finish_coding_task",
            "session_summary",
            "session_handoff_summary",
        ],
    },
];

pub(crate) fn lookup_tool_definition(name: &str) -> Option<&'static ToolDefinition> {
    TOOL_DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

/// Returns `true` if `name` is a recognized runtime tool name. Public so the
/// HTTP/MCP adapters can decide whether to emit the rich "unknown tool" error.
pub fn is_known_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some()
}

pub(crate) fn runtime_tool_oauth_scope(name: &str) -> Option<&'static str> {
    lookup_tool_definition(name).and_then(|definition| definition.oauth_scope())
}

pub(crate) fn runtime_tool_metadata(name: &str) -> ToolMetadata {
    lookup_tool_definition(name)
        .map(|definition| definition.metadata())
        .unwrap_or_else(|| fallback_tool_metadata(name))
}

pub(crate) fn runtime_tool_category(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.category)
        .unwrap_or("other")
}

pub(crate) fn runtime_tool_session_risk_class(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.session_risk_class())
        .unwrap_or_else(|| fallback_tool_metadata(name).risk.session_risk_class())
}

pub(crate) fn runtime_tool_is_read_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_read_like())
        .unwrap_or_else(|| fallback_tool_metadata(name).read_only)
}

pub(crate) fn runtime_tool_is_write_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_write_like())
        .unwrap_or_else(|| fallback_tool_metadata(name).risk == ToolRisk::ProjectWrite)
}

pub(crate) fn runtime_tool_is_shell_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_shell_like())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            metadata.shell_like || metadata.risk == ToolRisk::JobRun
        })
}

pub(crate) fn runtime_tool_is_git_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_git_like())
}

pub(crate) fn runtime_tool_is_change_summary_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_change_summary_like())
}

pub(crate) fn runtime_tool_captures_validation_output(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.captures_validation_output())
}

pub(crate) fn runtime_tool_is_current_session_control(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_current_session_control())
}

pub(crate) fn runtime_tool_requires_explicit_business_session(name: &str) -> bool {
    lookup_tool_definition(name)
        .is_some_and(|definition| definition.requires_explicit_business_session())
}

pub(crate) fn runtime_tool_creates_or_binds_session(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.creates_or_binds_session())
}

pub(crate) fn runtime_tool_allows_current_session_fallback(name: &str) -> bool {
    lookup_tool_definition(name)
        .is_some_and(|definition| definition.allows_current_session_fallback())
}

pub(crate) fn runtime_tool_requires_session_project_escape(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.requires_session_project_escape())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            !metadata.read_only || metadata.destructive || metadata.shell_like
        })
}

pub(crate) fn runtime_tool_requires_permission(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.requires_permission())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            !metadata.read_only || metadata.destructive || metadata.shell_like
        })
}

pub(crate) fn runtime_tool_permission_risk(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.permission_risk())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            if metadata.shell_like {
                return "shell";
            }
            if metadata.destructive {
                return "destructive";
            }
            if metadata.path_hint == ToolPathHint::Artifact {
                return "artifact_write";
            }
            if metadata.path_hint == ToolPathHint::Patch || name.contains("patch") {
                return "patch";
            }
            if matches!(
                metadata.risk,
                ToolRisk::ProjectWrite | ToolRisk::AccountManage
            ) {
                return "write";
            }
            "write"
        })
}

pub(crate) fn is_model_visible_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_visible())
}

pub(crate) fn is_model_hidden_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_hidden())
}

pub(crate) fn model_visible_tool_definitions() -> impl Iterator<Item = &'static ToolDefinition> {
    TOOL_DEFINITIONS
        .iter()
        .filter(|definition| definition.visibility.is_model_visible())
}

pub(super) fn model_visible_tool_names_csv() -> String {
    model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn tool_is_in_discovery_group(tool_name: &str, group_name: &str) -> bool {
    TOOL_DISCOVERY_GROUPS
        .iter()
        .find(|group| group.name == group_name)
        .is_some_and(|group| group.tools.contains(&tool_name))
}
