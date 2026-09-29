use super::ToolVisibility::{ModelHidden, ModelVisible};
use super::{
    adaptive_runtime_direct, def, model_spec, permission_risk, require_all_scopes, ToolDefinition,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_AGENT_WAIT,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{Read, WorkflowManage},
    COMMUNICATION_MANAGE, COMMUNICATION_READ, TOOL_PROVIDER_CONTROL,
};
use webcodex_core::authority::{COMMUNICATION_MANAGE_SCOPES, COMMUNICATION_READ_SCOPES};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(
        adaptive_runtime_direct(
            permission_risk(
                model_spec(
                    def(
                        "wait_for_agent_events",
                    super::ToolAuditPolicy::typed_fields(&[
                        super::ToolAuditResultField::pointer("wait_id", "/agent_wait/wait_id"),
                        super::ToolAuditResultField::pointer("state", "/agent_wait/state"),
                        super::ToolAuditResultField::pointer("mode", "/agent_wait/mode"),
                        super::ToolAuditResultField::pointer("goal_id", "/agent_wait/goal_id"),
                        super::ToolAuditResultField::pointer("source_count", "/agent_wait/source_count"),
                        super::ToolAuditResultField::pointer("revision", "/agent_wait/revision"),
                        super::ToolAuditResultField::pointer("match_count", "/agent_wait/match_count"),
                        super::ToolAuditResultField::value("replayed"),
                        super::ToolAuditResultField::value("state_changed"),
                        super::ToolAuditResultField::value("error_kind"),
                    ]),
                    ModelVisible,
                    TOOL_CATEGORY_AGENT_WAIT,
                    None,
                    TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Mutate,
                        risk: WorkflowManage,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::Keyed,
                    },
                    Some(COMMUNICATION_MANAGE),
                    false,
                    NoPath,
                    false,
                    false,
                    super::ToolSessionEvidencePolicy::NONE,
                    ),
                    "Create one caller-owned durable one-shot AgentWait over 1..8 exact agent_task_terminal selectors with closed mode any|all and optional exact goal_id correlation context. Mode defaults to any. Under any, the first match triggers and later matches follow the existing one-shot coalescing rules. Under all, partial matches remain durable without creating a Wake; only the final required source match triggers one Wake. Generic Waits may snapshot already-terminal sources and never affect Goal attention. A goal_id-scoped Wait instead requires an active same-owner Goal whose explicit controller equals the target Agent, requires every exact source Task to be explicitly correlated to that Goal, and fails closed if any selected source is already terminal; while active it owns terminal attention for only those exact Goal/Task sources. The Endpoint is presentation only, not durable Wait ownership. Exact keyed replay returns the same Wait.",
                ).with_gpt_action_description("Create one bounded one-shot AgentWait over exact AgentTask terminal selectors. mode=any (default) triggers on the first match; mode=all triggers only after every registered source matches. Exact idempotency replay returns the same Wait."),
                PERMISSION_RISK_WRITE,
            ),
            21,
            super::ToolDirectReason::Continuation,
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            def(
                "read_agent_wait",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("wait_id", "/agent_wait/wait_id"),
                    super::ToolAuditResultField::pointer("state", "/agent_wait/state"),
                    super::ToolAuditResultField::pointer("mode", "/agent_wait/mode"),
                    super::ToolAuditResultField::pointer("source_count", "/agent_wait/source_count"),
                    super::ToolAuditResultField::pointer("revision", "/agent_wait/revision"),
                    super::ToolAuditResultField::pointer("match_count", "/agent_wait/match_count"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_AGENT_WAIT,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read, approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
                Some(COMMUNICATION_READ), false, NoPath, false, false, super::ToolSessionEvidencePolicy::NONE,
            ),
            "Read one exact caller-owned AgentWait with bounded self-contained rendezvous truth: wait_id, optional exact goal_id, state, mode, source_count, match_count, all registered source Task identities, and matching Task/Attempt identities with terminal states. Source references contain only kind/task_id. goal_id is only correlation context. The result contains no Goal objective/title/lifecycle/controller metadata, source Task instruction/result/reason/log/fence/token, Project reference, Endpoint, or credentials and grants no Task, Project, Goal, Session, or execution authority; re-read the Goal and source Tasks independently.",
        ),
        COMMUNICATION_READ_SCOPES,
    ),
    require_all_scopes(
        permission_risk(
            model_spec(
                def(
                    "cancel_agent_wait",
                    super::ToolAuditPolicy::typed_fields(&[
                        super::ToolAuditResultField::pointer("wait_id", "/agent_wait/wait_id"),
                        super::ToolAuditResultField::pointer("state", "/agent_wait/state"),
                        super::ToolAuditResultField::value("replayed"),
                        super::ToolAuditResultField::value("state_changed"),
                        super::ToolAuditResultField::value("error_kind"),
                    ]),
                    ModelVisible,
                    TOOL_CATEGORY_AGENT_WAIT,
                    None,
                    TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract { effect: super::ToolEffect::Mutate, risk: WorkflowManage, approval: super::ToolApprovalPolicy::Standard, idempotency: super::ToolIdempotency::Keyed },
                    Some(COMMUNICATION_MANAGE), false, NoPath, false, false, super::ToolSessionEvidencePolicy::NONE,
                ),
                "Cancel an exact one-shot AgentWait only while waiting or while its Wait-origin Wake remains pre-dispatch pending/claimed. A claimed Wake is safely revoked/retired. Once Host dispatch preparation has occurred, cancellation fails closed because delivery cannot truthfully be withdrawn. Resumed/cancelled Waits are immutable.",
            ),
            PERMISSION_RISK_WRITE,
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        def(
            "agent_wait_state",
            super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::pointer("wait_id", "/agent_wait/wait_id"),
                super::ToolAuditResultField::pointer("state", "/agent_wait/state"),
                super::ToolAuditResultField::pointer("revision", "/agent_wait/revision"),
                super::ToolAuditResultField::pointer("match_count", "/agent_wait/match_count"),
                super::ToolAuditResultField::value("error_kind"),
            ]),
            ModelHidden,
            TOOL_CATEGORY_AGENT_WAIT,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read, approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
            Some(COMMUNICATION_READ), false, NoPath, false, false, super::ToolSessionEvidencePolicy::NONE,
        )
        .with_activity(
            super::ToolActivityPresentation::Transport,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        COMMUNICATION_READ_SCOPES,
    ),
];
