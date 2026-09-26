use super::ToolVisibility::{ModelHidden, ModelVisible};
use super::{
    adaptive_runtime_direct, def, model_spec, require_all_scopes, ToolDefinition,
    TOOL_CATEGORY_PROJECT, TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{ProjectWrite, Read, RunControl},
    DIAGNOSTICS_READ, PROJECT_READ, PROJECT_WRITE, RUNTIME_READ, SERVICE_DEPLOY, SERVICE_RESTART,
    TOOL_PROVIDER_CONTROL,
};
use webcodex_core::authority::{SCOPE_SERVICE_DEPLOY, SCOPE_SERVICE_RESTART};

const SERVICE_DEPLOY_SCOPES: &[&str] = &[SCOPE_SERVICE_RESTART, SCOPE_SERVICE_DEPLOY];

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
        "List caller-visible Projects. When Runner/Project identity is known, pass exact client_id/project; use bounded query and summary_only instead of reading the full registry.",
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
        "Register an existing directory, including a non-Git or ad-hoc workspace, as a Project on one Runner. Use this when the directory already exists; policy still bounds allowed paths.",
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
        "Create a directory on one Runner and register it as a Project. Use this for a new workspace; existing directories belong on the registration path.",
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
    model_spec(
        def(
            "deployment_preflight",
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
            ),
            "Preflight one explicit restart/deploy/rollback operation for one exact Runner. Read-only: restart requires only service:restart, while deploy/rollback additionally require service:deploy. Reports active/recovering Jobs, drain state, source alignment, blockers and warnings; never changes service state.",
        ).with_gpt_action_description("Check one exact Runner for a controlled restart/deploy/rollback. Read-only; operation determines the narrow required service scopes. Reports active/recovering Jobs, alignment, blockers and drain requirements; never changes service state."),
    require_all_scopes(
        model_spec(
            def(
                "prepare_service_deployment",
                    super::ToolAuditPolicy::typed_fields(&[
                        super::ToolAuditResultField::pointer("receipt_id", "/deployment_receipt/receipt_id"),
                        super::ToolAuditResultField::pointer("state", "/deployment_receipt/state"),
                        super::ToolAuditResultField::pointer("revision", "/deployment_receipt/revision"),
                        super::ToolAuditResultField::value("replayed"),
                        super::ToolAuditResultField::value("state_changed"),
                        super::ToolAuditResultField::value("error_kind"),
                    ]),
                    ModelVisible,
                    TOOL_CATEGORY_RUNTIME,
                    None,
                    TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Mutate,
                        risk: RunControl,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::Keyed,
                    },
                    Some(SERVICE_DEPLOY),
                    false,
                    NoPath,
                    false,
                    false,
                    super::ToolSessionEvidencePolicy::NONE,
                )
                .with_activity(
                    super::ToolActivityPresentation::Support,
                    super::ToolActivityInteraction::Meaningful,
                ),
                "Create or exactly replay one durable deployment receipt after validating a bounded non-secret release manifest. This is preparation only: it does not drain, restart, replace binaries, launch a process, or cut over traffic. Exact idempotency-key replay returns the same receipt; changed reuse fails closed.",
            ).with_gpt_action_description("Prepare a controlled WebPi deploy/restart/rollback by creating an exact durable receipt. Requires service:restart + service:deploy. It validates a bounded non-secret manifest but performs no drain, restart or binary replacement."),
        SERVICE_DEPLOY_SCOPES,
    ),
    model_spec(
        def(
            "read_deployment_receipt",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("receipt_id", "/deployment_receipt/receipt_id"),
                    super::ToolAuditResultField::pointer("state", "/deployment_receipt/state"),
                    super::ToolAuditResultField::pointer("revision", "/deployment_receipt/revision"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
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
                Some(SERVICE_RESTART),
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
            "Read one exact durable deployment receipt owned by the current authenticated principal. service:restart is sufficient because deploy/rollback authority already implies service:restart; receipt ownership remains enforced. This survives Server restart and never changes deployment state.",
        ).with_gpt_action_description("Read one exact durable WebPi deployment receipt owned by the current principal. Survives Server restart; read-only."),
    require_all_scopes(
        model_spec(
            def(
                "service_rollback",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("receipt_id", "/deployment_receipt/receipt_id"),
                    super::ToolAuditResultField::pointer("state", "/deployment_receipt/state"),
                    super::ToolAuditResultField::pointer("revision", "/deployment_receipt/revision"),
                    super::ToolAuditResultField::pointer("backup_id", "/deployment_receipt/backup_id"),
                    super::ToolAuditResultField::value("scheduled"),
                    super::ToolAuditResultField::value("state_changed"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: RunControl,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::DesiredState,
                },
                Some(SERVICE_DEPLOY),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Execute one already-prepared WebPi rollback receipt after drain and zero active Jobs. Requires service:restart + service:deploy and exact receipt/lifecycle generations. The target backup is consumed only from the durable receipt and resolved under the supervisor backup root; the tool accepts no path, executable, or argv. Before restoring, the supervisor snapshots the current runtime into a new safety backup so rollback itself is reversible. Switching rollback receipts are reconciled and never blindly re-dispatched.",
        ).with_gpt_action_description("Execute one prepared WebPi rollback receipt through the fixed-action supervisor. Requires service:restart + service:deploy, drain, zero active Jobs and exact generations. The target is a safe receipt backup id; rollback first snapshots the current runtime for reversibility, and switching rollbacks are reconciled rather than retried."),
        SERVICE_DEPLOY_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            def(
                "service_deploy",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("receipt_id", "/deployment_receipt/receipt_id"),
                    super::ToolAuditResultField::pointer("state", "/deployment_receipt/state"),
                    super::ToolAuditResultField::pointer("revision", "/deployment_receipt/revision"),
                    super::ToolAuditResultField::pointer("backup_id", "/deployment_receipt/backup_id"),
                    super::ToolAuditResultField::value("scheduled"),
                    super::ToolAuditResultField::value("state_changed"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: RunControl,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::DesiredState,
                },
                Some(SERVICE_DEPLOY),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Execute one already-prepared WebPi deploy receipt after drain and zero active Jobs. Requires service:restart + service:deploy, exact receipt/lifecycle generations, and a compatible standalone supervisor. Candidate id and exactly three artifact identities are consumed from the immutable durable receipt; the tool accepts no arbitrary path, executable, or argv. A switching receipt is reconciled from signed supervisor result and is never blindly re-dispatched.",
        ).with_gpt_action_description("Execute one prepared WebPi deploy receipt through the fixed-action supervisor. Requires service:restart + service:deploy, drain, zero active Jobs and exact receipt/lifecycle generations. Candidate/artifact identity comes only from the durable receipt; switching deployments are reconciled, never blindly retried."),
        SERVICE_DEPLOY_SCOPES,
    ),
    model_spec(
        def(
            "service_restart",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("receipt_id", "/deployment_receipt/receipt_id"),
                    super::ToolAuditResultField::pointer("state", "/deployment_receipt/state"),
                    super::ToolAuditResultField::pointer("revision", "/deployment_receipt/revision"),
                    super::ToolAuditResultField::value("scheduled"),
                    super::ToolAuditResultField::value("replayed"),
                    super::ToolAuditResultField::value("state_changed"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: RunControl,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(SERVICE_RESTART),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Schedule one supervisor-backed WebPi restart after service drain is active and no Jobs remain. Requires only service:restart. The operation creates or exactly replays a durable receipt, binds to the observed lifecycle generation, and sends a signed fixed-action request to the standalone parent. It accepts no executable or argv.",
        ).with_gpt_action_description("Schedule a controlled WebPi restart after drain and zero active Jobs. Requires service:restart, exact lifecycle generation and idempotency key; uses a signed fixed-action supervisor protocol and durable receipt, never arbitrary process execution."),
    model_spec(
        def(
            "service_drain",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: RunControl,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::DesiredState,
                },
                Some(SERVICE_RESTART),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Enter or leave WebPi service drain mode using an optimistic lifecycle generation fence. Drain mode blocks new consequential runtime tool calls while preserving read-only observability and already-running work. Repeating the already-current desired state is a safe no-op. This tool never restarts or replaces binaries.",
        ).with_gpt_action_description("Set WebPi service drain mode with an optimistic generation fence. Requires service:restart. Drain blocks new consequential tool calls but preserves reads and already-running work; it never restarts or deploys by itself."),
    model_spec(
        def(
            "runtime_diagnostics",
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
                Some(DIAGNOSTICS_READ),
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
        "Read the bounded secrets-safe runtime diagnostic ring. Events contain only code-owned severity/component/code identifiers and optional token-shaped correlation ids; raw log messages, headers, env values, argv and filesystem paths are never stored. Filter by severity/component/correlation_id and inclusive since/until range. Ring metadata reports retained sequence bounds and older-event eviction count.",
    )
    .with_gpt_action_description("Read bounded secrets-safe WebPi runtime diagnostics. Requires diagnostics:read. Returns only structured allowlisted event identifiers; never raw logs, headers, env, commands, or paths."),
    model_spec(
        def(
            "public_tunnel_probe",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelHidden,
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
        "Internal bounded refresh for the Server-configured public WebPi origin. Deployment preflight refreshes this evidence automatically; runtime_status projects the cached result. No caller URL, credentials, redirects, response body, or headers are exposed.",
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
            ),
            "Read runtime status; pass exact client_id for one Runner deployment/source alignment, omit for fleet-wide. Reports shared Job concurrency; global mode includes bounded host_context advisory metadata, never authority.",
        ),
        20,
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
