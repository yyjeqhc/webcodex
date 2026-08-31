use super::AgentCapability::CodingAgentRuns;
use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, require_all_scopes, ToolDefinition, PERMISSION_RISK_JOB,
    TOOL_CATEGORY_CODING_AGENT,
};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{JobRun, Read},
    CODING_AGENT_RUN, TOOL_PROVIDER_AGENT,
};
use crate::tool_runtime::registry::input_schemas::{
    coding_agent_cancel_input_schema, coding_agent_observe_input_schema,
    coding_agent_start_input_schema,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    permission_risk(
        model_spec(
            require_all_scopes(
                def(
                    "coding_agent_start",
                    ModelVisible,
                    TOOL_CATEGORY_CODING_AGENT,
                    Some(CodingAgentRuns),
                    TOOL_PROVIDER_AGENT,
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
                ),
                &[CODING_AGENT_RUN, crate::auth::SCOPE_PROJECT_WRITE],
            ),
            "Start one idempotent delegated ACP coding-agent Run on an exact registered Project and logical Runner provider. Autonomous execution may outlive this request; after any uncertain start, reuse the same idempotency key and observe the same Run rather than dispatching a replacement.",
            coding_agent_start_input_schema,
        ),
        PERMISSION_RISK_JOB,
    ),
    model_spec(
        def(
            "coding_agent_observe",
            ModelVisible,
            TOOL_CATEGORY_CODING_AGENT,
            None,
            TOOL_PROVIDER_AGENT,
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
        ),
        "Observe bounded normalized events and lifecycle for one existing CodingAgentRun. Return the opaque token for only-new follow-ups; history loss/reset is explicit. Observation never starts, retries, or resumes ACP work.",
        coding_agent_observe_input_schema,
    ),
    model_spec(
        def(
            "coding_agent_cancel",
            ModelVisible,
            TOOL_CATEGORY_CODING_AGENT,
            None,
            TOOL_PROVIDER_AGENT,
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
            false,
            false,
        ),
        "Request cancellation of one existing CodingAgentRun. This does not grant permission, retry a prompt, or create a replacement Run; observe the same run_id for authoritative terminal state.",
        coding_agent_cancel_input_schema,
    ),
];
