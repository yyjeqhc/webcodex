use super::AgentCapability::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_EDIT};
use crate::tool_runtime::metadata::{
    ToolPathHint::{PathList, SinglePath},
    ToolRisk::ProjectWrite,
    PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "write_project_file",
        ModelVisible,
        TOOL_CATEGORY_EDIT,
        Some(FileWrite),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: ProjectWrite,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(PROJECT_WRITE),
        true,
        SinglePath,
        false,
        false,
    ),
    def(
        "apply_text_edits",
        ModelVisible,
        TOOL_CATEGORY_EDIT,
        Some(FileWrite),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: ProjectWrite,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(PROJECT_WRITE),
        true,
        PathList,
        false,
        false,
    ),
];
