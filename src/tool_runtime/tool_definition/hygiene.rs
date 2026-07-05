use super::AgentCapability::GitOrShell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[def(
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
)];
