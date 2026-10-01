use super::RunnerCapabilityRequirement::RunnerConfigControl;
use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, ToolDefinition, PERMISSION_RISK_WRITE, TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{Read, RunControl},
    RUNNER_MANAGE, RUNTIME_READ, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
            def(
                "check_runner_config",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                Some(RunnerConfigControl),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Read, parse, validate, and classify the candidate at one exact caller-visible Runner's startup-bound runner.toml path. This never mutates active config, accepts no filesystem path, and returns only bounded sanitized validation metadata.",
    ),
    permission_risk(
            model_spec(
                def(
                    "reload_runner_config",
                    super::ToolAuditPolicy::TYPED_CANONICAL,
                    ModelVisible,
                    TOOL_CATEGORY_RUNTIME,
                    Some(RunnerConfigControl),
                    TOOL_PROVIDER_RUNNER,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Mutate,
                        risk: RunControl,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::FencedReplay,
                    },
                    Some(RUNNER_MANAGE),
                    false,
                    NoPath,
                    false,
                    false,
                    super::ToolSessionEvidencePolicy::NONE,
                ),
                "Activate the candidate already present at one exact caller-visible Runner's startup-bound runner.toml path. Requires expected_generation, never writes the file, and reports partial activation/restart-required fields without pretending restart-only fields are live.",
            ),
            PERMISSION_RISK_WRITE,
    ),
];
