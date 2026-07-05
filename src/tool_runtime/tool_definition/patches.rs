use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::Patch,
    ToolRisk::{ProjectWrite, ReadOnly},
    PROJECT_READ, PROJECT_WRITE,
};

pub(super) const APPLY_DEFINITIONS: &[ToolDefinition] = &[
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
];

pub(super) const VALIDATION_DEFINITIONS: &[ToolDefinition] = &[def(
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
)];
