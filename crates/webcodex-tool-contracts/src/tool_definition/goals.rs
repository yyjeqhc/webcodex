use super::ToolVisibility::{ModelHidden, ModelVisible};
use super::{
    adaptive_runtime_direct, control_def, model_spec, require_all_scopes, ToolDefinition,
    TOOL_CATEGORY_GOAL,
};
use crate::metadata::{
    ToolRisk::{Read, WorkflowManage},
    COMMUNICATION_MANAGE, COMMUNICATION_READ,
};
use webcodex_core::authority::{
    COMMUNICATION_MANAGE_SCOPES, COMMUNICATION_READ_SCOPES, SCOPE_COMMUNICATION_MANAGE,
    SCOPE_COMMUNICATION_READ, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
};

const GOAL_SESSION_ASSOCIATE_SCOPES: &[&str] = &[
    SCOPE_COMMUNICATION_READ,
    SCOPE_COMMUNICATION_MANAGE,
    SCOPE_SESSION_COLLABORATE,
];

const GOAL_MUTATION_AUDIT: super::ToolAuditPolicy = super::ToolAuditPolicy::typed_fields(&[
    super::ToolAuditResultField::pointer("goal_id", "/goal/summary/goal_id"),
    super::ToolAuditResultField::pointer("lifecycle", "/goal/summary/lifecycle"),
    super::ToolAuditResultField::pointer("revision", "/goal/summary/revision"),
    super::ToolAuditResultField::value("created"),
    super::ToolAuditResultField::value("replayed"),
    super::ToolAuditResultField::value("state_changed"),
    super::ToolAuditResultField::value("error_kind"),
]);

const GOAL_PLAN_AUDIT: super::ToolAuditPolicy = super::ToolAuditPolicy::typed_fields(&[
    super::ToolAuditResultField::pointer("goal_id", "/goal_plan/goal_id"),
    super::ToolAuditResultField::pointer("lifecycle", "/goal_plan/lifecycle"),
    super::ToolAuditResultField::pointer("revision", "/goal_plan/revision"),
    super::ToolAuditResultField::pointer("agent_task_count", "/goal_plan/agent_task_count"),
    super::ToolAuditResultField::pointer(
        "workflow_session_count",
        "/goal_plan/workflow_session_count",
    ),
    super::ToolAuditResultField::value("error_kind"),
]);

const GOAL_ASSOCIATION_AUDIT: super::ToolAuditPolicy = super::ToolAuditPolicy::typed_fields(&[
    super::ToolAuditResultField::pointer("goal_id", "/goal/summary/goal_id"),
    super::ToolAuditResultField::pointer("revision", "/goal/summary/revision"),
    super::ToolAuditResultField::value("replayed"),
    super::ToolAuditResultField::value("state_changed"),
    super::ToolAuditResultField::value("error_kind"),
]);

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(
        model_spec(
            control_def(
                "prepare_goal_workflow",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("goal_id", "/goal/summary/goal_id"),
                    super::ToolAuditResultField::pointer("lifecycle", "/goal/summary/lifecycle"),
                    super::ToolAuditResultField::pointer("revision", "/goal/summary/revision"),
                    super::ToolAuditResultField::pointer(
                        "workflow_session_count",
                        "/goal/summary/workflow_session_count",
                    ),
                    super::ToolAuditResultField::value("created"),
                    super::ToolAuditResultField::value("replayed"),
                    super::ToolAuditResultField::value("state_changed"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Atomically admit one new durable Goal together with one exact independently authorized Workflow Session correlation and an optional explicit owned controller Agent. The Store commits Goal + correlation + keyed replay identity in one transaction at revision 1. This operation is Host-neutral: it never infers identity from a Window, creates/rotates Endpoints, mounts MCP Apps, establishes Host bindings, creates Wakes, or proves continuation readiness. Use present_goal_plan separately; if automatic continuation is desired, independently establish or verify the controller through the agent_continuation_setup flow.",
        ),
        GOAL_SESSION_ASSOCIATE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "create_goal",
                GOAL_MUTATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Create a durable Goal with bounded completion intent and initial plan for substantial multi-step or cross-turn work; reuse an existing Goal when appropriate. Tiny lookups/trivial edits do not need one. Explicitly associate the current Workflow Session, then checkpoint recovery-worthy milestones. For automatic continuation supply an exact owned controller Agent and use the existing continuation carrier. Reuse an already-callable Agent from explicit durable setup or exact Wake context, never from Window inference. Controller identity routes attention only and grants no execution authority.",
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "get_goal",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("goal_id", "/goal/summary/goal_id"),
                    super::ToolAuditResultField::pointer("lifecycle", "/goal/summary/lifecycle"),
                    super::ToolAuditResultField::pointer("revision", "/goal/summary/revision"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(COMMUNICATION_READ),
                false,
            ),
            "Read one exact caller-owned durable Goal, including its optional durable controller Agent identity. Unauthorized and nonexistent Goal ids are existence-hidden. Controller/correlation identities are routing or correlation only and never target-domain authority, Endpoint/window bindings, fences, tokens, credentials, Job state, or Workflow Session ledgers.",
        ),
        COMMUNICATION_READ_SCOPES,
    ),
    require_all_scopes(
        adaptive_runtime_direct(
            model_spec(
                control_def(
                    "present_goal_plan",
                    GOAL_PLAN_AUDIT,
                    ModelVisible,
                    TOOL_CATEGORY_GOAL,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Observe,
                        risk: Read,
                        approval: super::ToolApprovalPolicy::None,
                        idempotency: super::ToolIdempotency::PureRead,
                    },
                    Some(COMMUNICATION_READ),
                    false,
                ),
                "Present one exact caller-owned durable Goal as a sparse read-only Goal Plan MCP App card, including only the optional durable controller Agent identity and never its Endpoint/window bindings. Requires explicit goal_id and never infers Goal or controller identity from Project, Workflow Session, Conversation, credential, ClientWindow, or recent activity. Presentation creates no work, grants no execution authority, and does not modify Goal lifecycle.",
            ),
            17,
            super::ToolDirectReason::Presentation,
        ),
        COMMUNICATION_READ_SCOPES,
    ),
    require_all_scopes(
        control_def(
            "sync_goal_plan",
            GOAL_PLAN_AUDIT,
            ModelHidden,
            TOOL_CATEGORY_GOAL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: WorkflowManage,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::DesiredState,
            },
            Some(COMMUNICATION_MANAGE),
            false,
        )
        .with_activity(
            super::ToolActivityPresentation::Transport,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        &[SCOPE_COMMUNICATION_READ, SCOPE_COMMUNICATION_MANAGE, SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE, SCOPE_PROJECT_READ],
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "list_goals",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("total_count"),
                    super::ToolAuditResultField::array_len("returned_count", "goals"),
                    super::ToolAuditResultField::value("offset"),
                    super::ToolAuditResultField::value("next_offset"),
                    super::ToolAuditResultField::value("truncated"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(COMMUNICATION_READ),
                false,
            ),
            "List bounded Goals visible to the current owner principal, optionally filtered by authoritative lifecycle. List projection omits objective, terminal reason, and exact correlation identities.",
        ),
        COMMUNICATION_READ_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "checkpoint_goal",
                GOAL_MUTATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Checkpoint one exact owned active Goal at a recovery-worthy milestone. Atomically complete selected stable step ids, optionally select one current step, and record a bounded summary using the exact revision and idempotency key. The whole batch validates before mutation; completed steps never regress, at most one step is in_progress, and each new checkpoint increments revision once. Exact replay is read-only; changed replay conflicts. Terminal Goals are immutable. This is progress truth, not effect replay, execution authority, or a routine read_session_handoff requirement.",
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "revise_goal_plan",
                GOAL_MUTATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Revise one exact owned active Goal's full ordered plan using the exact current revision and a fresh idempotency key, with a bounded reason. Retain every completed step with its exact id/title; completed progress never regresses. Retained unfinished ids preserve their statuses; they may be renamed or reordered, removed, or supplemented with new pending steps. Removing the current step leaves no current step. Use checkpoint_goal separately to change progress. Completion conditions and objective are unchanged. Retain the latest eight prior plan snapshots with reasons; get_goal exposes bounded history with explicit total/returned/truncation. The entire candidate, history and replay identity commit atomically with one revision increment. Exact keyed replay is read-only, changed reuse conflicts, and terminal Goals are immutable. On a stale revision read get_goal and reconsider the replacement; never blindly retry with a new revision. This grants no execution, Task, Session, Project or carrier authority.",
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "update_goal",
                GOAL_MUTATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Update bounded Goal metadata or completion_conditions, explicitly replace its exact independently authorized durable controller Agent, or transition active to completed/cancelled using an exact revision and idempotency key. Omitted completion_conditions preserves the current intent; an empty list clears it. Conditions use the creation bounds and never change steps or recorded progress. Omitted controller preserves the current routing identity; terminal Goal state is immutable. Controller routing grants no execution authority and no execution domain is inferred or mutated. Complete every plan step with checkpoint_goal and freshly verify/review before explicitly completing a Goal; the Server cannot judge natural-language completion conditions.",
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "associate_goal_agent_task",
                GOAL_ASSOCIATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Explicitly correlate one owned active Goal with one exact owned AgentTask after independently re-authorizing that AgentTask. The link is identity-only and grants no TaskAttempt, CodingAgentRun, Project, Runner, filesystem, or Job authority.",
        ),
        COMMUNICATION_MANAGE_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            control_def(
                "associate_goal_workflow_session",
                GOAL_ASSOCIATION_AUDIT,
                ModelVisible,
                TOOL_CATEGORY_GOAL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: WorkflowManage,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(COMMUNICATION_MANAGE),
                false,
            ),
            "Explicitly correlate one owned active Goal with one exact Workflow Session after independently re-authorizing the Session through its existing authority fingerprint and any bound Project authorization. The Goal link is never a Session or Project credential.",
        ),
        GOAL_SESSION_ASSOCIATE_SCOPES,
    ),
];
