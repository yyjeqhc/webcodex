use super::RunnerCapabilityRequirement::CodingAgentRuns;
use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, require_all_scopes, ToolDefinition, PERMISSION_RISK_JOB,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_CODING_AGENT,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{JobRun, Read},
    CODING_AGENT_RUN, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    permission_risk(
        model_spec(
            require_all_scopes(
                def(
                    "start_coding_agent",
                    super::ToolAuditPolicy::typed_fields(&[
                        super::ToolAuditResultField::value("run_id"),
                        super::ToolAuditResultField::value("project"),
                        super::ToolAuditResultField::value("provider_id"),
                        super::ToolAuditResultField::value("state"),
                        super::ToolAuditResultField::value("execution_state"),
                        super::ToolAuditResultField::value("cancel_requested"),
                        super::ToolAuditResultField::pointer("terminal_stop_reason", "/terminal/stop_reason"),
                        super::ToolAuditResultField::pointer("terminal_error_code", "/terminal/error_code"),
                        super::ToolAuditResultField::pointer("terminal_completed_at", "/terminal/completed_at"),
                        super::ToolAuditResultField::value("error_kind"),
                        super::ToolAuditResultField::value("recovery_kind"),
                    ]),
                    ModelVisible,
                    TOOL_CATEGORY_CODING_AGENT,
                    Some(CodingAgentRuns),
                    TOOL_PROVIDER_RUNNER,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Execute,
                        risk: JobRun,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::Keyed,
                    },
                    Some(CODING_AGENT_RUN),
                    true,
                    NoPath,
                    true,
                    false,
                    super::ToolSessionEvidencePolicy::NONE,
                ),
                &[CODING_AGENT_RUN, webcodex_core::authority::SCOPE_PROJECT_WRITE],
            ),
            "Start one idempotent delegated ACP Run on an exact Project and logical Runner provider; configured model API adapters can perform text reasoning/review. Optional context_session_id quotes an independently authorized bounded handoff and explicit Goal context; no source resume, recorder inference, or authority transfer. Current files/Git are not fetched. The context snapshot is part of the intent: a changed snapshot under the same key conflicts. Autonomous execution may outlive this request; after any uncertain start observe the same Run rather than dispatching a replacement. Choose providers only from the exact Runner inventory; no automatic quota/model switching.",
        ),
        PERMISSION_RISK_JOB,
    ),
    model_spec(
        def(
            "observe_coding_agent",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::CodingAgentObservation,
            ),
            ModelVisible,
            TOOL_CATEGORY_CODING_AGENT,
            None,
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(CODING_AGENT_RUN),
            false,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Observe bounded normalized events and lifecycle for one existing CodingAgentRun. Return the opaque token for only-new follow-ups; history loss/reset is explicit. Observation never starts, retries, or resumes ACP work.",
    ),
    permission_risk(
        model_spec(
            def(
            "cancel_coding_agent",
            super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::value("run_id"),
                super::ToolAuditResultField::value("project"),
                super::ToolAuditResultField::value("provider_id"),
                super::ToolAuditResultField::value("state"),
                super::ToolAuditResultField::value("execution_state"),
                super::ToolAuditResultField::value("cancel_requested"),
                super::ToolAuditResultField::pointer("terminal_stop_reason", "/terminal/stop_reason"),
                super::ToolAuditResultField::pointer("terminal_error_code", "/terminal/error_code"),
                super::ToolAuditResultField::pointer("terminal_completed_at", "/terminal/completed_at"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("recovery_kind"),
            ]),
            ModelVisible,
            TOOL_CATEGORY_CODING_AGENT,
            None,
            TOOL_PROVIDER_RUNNER,
            // Cancel is Run lifecycle control but deliberately not a second
            // WebCodex PermissionEvaluator decision after start admission.
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: super::ToolRisk::RunControl,
                approval: super::ToolApprovalPolicy::InheritFromStart,
                idempotency: super::ToolIdempotency::DesiredState,
            },
            Some(CODING_AGENT_RUN),
            false,
            NoPath,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
            ),
            "Request cancellation of one existing CodingAgentRun. This does not grant permission, retry a prompt, or create a replacement Run; observe the same run_id for authoritative terminal state.",
        ),
        PERMISSION_RISK_WRITE,
    ),
];
