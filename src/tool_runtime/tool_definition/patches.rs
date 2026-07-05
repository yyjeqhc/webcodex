use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_PATCH};
use crate::tool_runtime::metadata::{
    ToolPathHint::Patch,
    ToolRisk::{ProjectWrite, ReadOnly},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};

pub(super) const APPLY_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "apply_patch",
        ModelVisible,
        TOOL_CATEGORY_PATCH,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
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
        TOOL_CATEGORY_PATCH,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
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
    TOOL_CATEGORY_PATCH,
    Some(Shell),
    TOOL_PROVIDER_AGENT,
    ReadOnly,
    Some(PROJECT_READ),
    true,
    Patch,
    false,
    false,
)];
