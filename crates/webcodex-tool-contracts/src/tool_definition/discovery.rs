use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, ToolDefinition, TOOL_CATEGORY_PROJECT,
    TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{ProjectWrite, Read},
    PROJECT_READ, PROJECT_WRITE, RUNTIME_READ, TOOL_PROVIDER_CONTROL,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "list_projects",
            super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                    "client_id",
                    "project",
                    "query",
                ]),
            ),
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            false,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        )
        .with_activity(
            super::ToolActivityPresentation::Support,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        "List caller-visible Projects. Results keep canonical Runtime Project ids and, when stable root identity is available, also issue a short principal-scoped project_ref for later model calls. project_ref is convenience only and grants no authority. When Runner/Project identity is known, pass exact client_id/project filters; use bounded query and summary_only instead of reading the full registry.",
    ),
    model_spec(
        def(
            "register_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            false,
            NoPath,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Register an existing directory, including a non-Git or ad-hoc workspace, as a Project on one Runner. Successful onboarding returns the canonical Runtime Project id and a Server-issued project_ref when a stable root identity is available. Use this when the directory already exists; policy still bounds allowed paths.",
    ),
    model_spec(
        def(
            "unregister_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            false,
            NoPath,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Unregister one exact Runner project using the revision from list_projects. Removes registration only; never deletes source, worktree, or branch. Re-list after an indeterminate outcome.",
    ),
    model_spec(
        def(
            "create_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            false,
            NoPath,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Create a directory on one Runner and register it as a Project. Successful onboarding returns the canonical Runtime Project id and a Server-issued project_ref when a stable root identity is available. Use this for a new workspace; existing directories belong on the registration path.",
    ),
    model_spec(
        def(
            "list_runners",
            super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                super::ToolAuditSessionInputPolicy::OmitTopLevel(&["client_id", "client_ids"]),
            ),
            ModelVisible,
            TOOL_CATEGORY_RUNTIME,
            None,
            TOOL_PROVIDER_CONTROL,
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
        )
        .with_activity(
            super::ToolActivityPresentation::Support,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        "List caller-visible Runners; use exact client_id/client_ids if known, summary_only + include_projects=false for health. Full mode includes shared Job concurrency and host_context advisory metadata; never authority.",
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "runtime_status",
                super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                    super::ToolAuditSessionInputPolicy::OmitTopLevel(&["client_id"]),
                ),
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
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
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            )
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read(),
            ),
            "Read runtime health and protocol/build/source alignment; exact client_id focuses one Runner and its Job concurrency. compact=true or summary_only=true selects sparse counts without inventories. Canonical/API default is full diagnostics with capabilities, build, authority, configuration and connection details; MCP defaults to sparse and accepts compact=false for diagnostics.",
        ).with_gpt_action_description("Read fleet or exact client_id Runner health and protocol/build/source alignment. compact=true returns sparse counts; omit for full diagnostic inventories and configuration."),
        20,
    ),
    model_spec(
            def(
                "current_window_activity",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
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
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            ),
            "Read bounded sanitized ActionAudit activity only for the current Host Window from transport identity. Requires runtime:read; every event retains current Project and principal visibility checks. No window selector, arguments, output, credentials, paths, or Host/model-state diagnosis. response_handed_at_ms means WebCodex constructed the response and handed it to the HTTP framework or returned from the handler; it does not prove downstream receipt. next_call_gap_ms exists only when a later canonical event was observed. Summary gap threshold counts are cumulative and, with totals, are descriptive WebCodex-observed request timing only; short gaps never prove Host cells, model turns, model thinking, frontend/network/user delay, or MCP orchestration. overlapping_call_count counts only persisted window_transition_kind=overlap and is never inferred from gap duration.",
        ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "tool_manifest",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
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
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            ),
            "Global runtime discovery; do not pass project. Filter by category/intent for sparse selection entries, or pass exact tool_name for one compact contract with description, preferred route, input schema, and safety/authority hints but no output schema. availability=direct means the direct callable is the preferred model route; if that callable is unavailable or not loaded, call_runtime_tool may be used as a fallback for an otherwise admitted target. availability never changes behavior, authority, permissions, execution, or verdicts. Unfiltered discovery retains the global category inventory.",
        ).with_gpt_action_description("Discover model-visible runtime tools. Filter by category/intent or pass exact tool_name for one compact contract. availability=direct is preferred; long-tail tools use call_runtime_tool. Discovery never changes authority."),
        30,
    ),
];
