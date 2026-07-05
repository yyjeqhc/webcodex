use super::AgentCapability::{GitOrShell, Shell};
use super::ToolVisibility::ModelVisible;
use super::{def, git_like, ToolDefinition, TOOL_CATEGORY_CLEANUP};
use crate::tool_runtime::metadata::{
    ToolPathHint::{None as NoPath, PathList},
    ToolRisk::{ProjectWrite, ReadOnly},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[def(
    "workspace_hygiene_check",
    ModelVisible,
    TOOL_CATEGORY_CLEANUP,
    Some(GitOrShell),
    TOOL_PROVIDER_AGENT,
    ReadOnly,
    Some(PROJECT_READ),
    true,
    NoPath,
    false,
    false,
)];

pub(super) const CLEANUP_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "delete_project_files",
        ModelVisible,
        TOOL_CATEGORY_CLEANUP,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    ),
    git_like(def(
        "git_restore_paths",
        ModelVisible,
        TOOL_CATEGORY_CLEANUP,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    )),
    git_like(def(
        "discard_untracked",
        ModelVisible,
        TOOL_CATEGORY_CLEANUP,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        ProjectWrite,
        Some(PROJECT_WRITE),
        true,
        PathList,
        true,
        false,
    )),
];
