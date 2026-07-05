use super::AgentCapability::GitOrShell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ,
};

pub(super) const SUMMARY_DEFINITIONS: &[ToolDefinition] = &[
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
];
