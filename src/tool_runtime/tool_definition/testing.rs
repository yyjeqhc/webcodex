use super::AgentCapability::{OwnerOnly, Shell};
use super::ToolVisibility::ModelVisible;
use super::{captures_validation_output, def, ToolDefinition, TOOL_CATEGORY_VALIDATION};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::JobRun, JOB_RUN, TOOL_PROVIDER_AGENT,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    captures_validation_output(def(
        "cargo_fmt",
        ModelVisible,
        TOOL_CATEGORY_VALIDATION,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Execute,
            risk: JobRun,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    )),
    captures_validation_output(def(
        "cargo_check",
        ModelVisible,
        TOOL_CATEGORY_VALIDATION,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Execute,
            risk: JobRun,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    )),
    captures_validation_output(def(
        "cargo_test",
        ModelVisible,
        TOOL_CATEGORY_VALIDATION,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Execute,
            risk: JobRun,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    )),
    captures_validation_output(def(
        "go_test",
        ModelVisible,
        TOOL_CATEGORY_VALIDATION,
        Some(OwnerOnly),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Execute,
            risk: JobRun,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    )),
];
