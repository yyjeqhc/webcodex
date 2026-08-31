use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_PATCH};
use crate::tool_runtime::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[def(
    "apply_unified_diff",
    ModelVisible,
    TOOL_CATEGORY_PATCH,
    Some(Shell),
    TOOL_PROVIDER_AGENT,
    super::ToolSemanticContract {
        effect: super::ToolEffect::Mutate,
        risk: ProjectWrite,
        approval: super::ToolApprovalPolicy::Standard,
        idempotency: super::ToolIdempotency::NonIdempotent,
    },
    Some(PROJECT_WRITE),
    true,
    Patch,
    false,
    false,
)];
