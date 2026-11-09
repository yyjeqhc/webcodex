use super::RunnerCapabilityRequirement::RunnerConfigControl;
use super::ToolVisibility::ModelVisible;
use super::{
    model_spec, permission_risk, ToolDeclaration, ToolDefinition, PERMISSION_RISK_WRITE,
    TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{Read, RunControl},
    RUNNER_MANAGE, RUNTIME_READ, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
            ToolDeclaration {
                name: "check_runner_config",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                visibility: ModelVisible,
                category: TOOL_CATEGORY_RUNTIME,
                runner_capability: Some(RunnerConfigControl),
                provider_id: TOOL_PROVIDER_RUNNER,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                required_scope: Some(RUNTIME_READ),
                requires_project: false,
                path_hint: NoPath,
                destructive: false,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE,
            }.define(),
            "Read, parse, validate, and classify the candidate at one exact caller-visible Runner's startup-bound runner.toml path. This never mutates active config, accepts no filesystem path, and returns only bounded sanitized validation metadata.",
    ),
    permission_risk(
            model_spec(
                ToolDeclaration {
                    name: "reload_runner_config",
                    audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                    visibility: ModelVisible,
                    category: TOOL_CATEGORY_RUNTIME,
                    runner_capability: Some(RunnerConfigControl),
                    provider_id: TOOL_PROVIDER_RUNNER,
                    semantic: super::ToolSemanticContract {
                        effect: super::ToolEffect::Mutate,
                        risk: RunControl,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::FencedReplay,
                    },
                    required_scope: Some(RUNNER_MANAGE),
                    requires_project: false,
                    path_hint: NoPath,
                    destructive: false,
                    shell_like: false,
                    session_evidence: super::ToolSessionEvidencePolicy::NONE,
                }.define(),
                "Activate the candidate already present at one exact caller-visible Runner's startup-bound runner.toml path. Requires expected_generation, never writes the file, and reports partial activation/restart-required fields without pretending restart-only fields are live.",
            ),
            PERMISSION_RISK_WRITE,
    ),
];
