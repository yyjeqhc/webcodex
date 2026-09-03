use super::AgentCapability::{FileRead, SkillStoreManage};
use super::ToolVisibility::ModelHidden;
use super::{def, ToolDefinition, TOOL_CATEGORY_RUNTIME};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{Read, SkillManage},
    ADMIN, PROJECT_READ, TOOL_PROVIDER_AGENT,
};

/// Fixed Phase-3 project Skill runtime tools. They are known to the kernel but
/// intentionally hidden from the generic registry; Stateless MCP 2026 Full
/// Operator projects them explicitly, and the kernel capability gate remains
/// authoritative for execution.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "skill_list",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(FileRead),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "skill_read_file",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(FileRead),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "skill_versions",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(SkillStoreManage),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(ADMIN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "skill_install",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(SkillStoreManage),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: SkillManage,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(ADMIN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "skill_activate",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(SkillStoreManage),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: SkillManage,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(ADMIN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "skill_remove_revision",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        Some(SkillStoreManage),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: SkillManage,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(ADMIN),
        true,
        NoPath,
        true,
        false,
    ),
];
