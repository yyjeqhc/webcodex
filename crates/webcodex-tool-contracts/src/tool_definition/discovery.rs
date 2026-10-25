use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, control_def, model_spec, ToolDefinition, TOOL_CATEGORY_PROJECT,
    TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolRisk::{ProjectWrite, Read},
    PROJECT_READ, PROJECT_WRITE, RUNTIME_READ,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        control_def(
            "resolve_workspace",
            super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
            super::ToolAuditSessionInputPolicy::OmitTopLevel(&["path", "query", "client_id"])),
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read,
            approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
            Some(PROJECT_READ),
            false,
        ),
        "Locate a registered workspace by exact Runner plus path or literal query; no Session or registration is created. Returns one resolved workspace or bounded ambiguous candidates, never the first fuzzy match. Git fields are cached Runner inventory, not current filesystem proof; incomplete inventory stays explicit. Starting or resuming work is a separate action. Gateway via call_runtime_tool.",
    ).with_discovery_keywords(&["locate", "resolve", "registered", "workspace", "path", "project", "定位", "已注册", "工作区", "路径", "项目"]),
    model_spec(
        control_def(
            "unregister_projects",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            super::ToolSemanticContract { effect: super::ToolEffect::Mutate, risk: ProjectWrite,
            approval: super::ToolApprovalPolicy::Standard, idempotency: super::ToolIdempotency::NonIdempotent },
            Some(PROJECT_WRITE),
            true,
        ),
        "Preview or batch-remove (unregister) up to 16 explicit Project registrations with exact revisions; never delete files, directories, branches or Session history. Default dry_run=true inspects current registrations, not a lease. Execution requires dry_run=false and confirm=true; each item uses canonical owner/CAS/active-Job checks. Sequential, non-atomic, per-item outcomes; uncertainty stops remaining items. Never replay the batch blindly; reconcile exact failed or unknown items.",
    ).with_discovery_keywords(&["batch", "unregister", "remove", "registrations", "projects", "批量", "注销", "移除", "注册", "项目"]),

    model_spec(
        control_def(
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
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            false,
        )
        .with_activity(
            super::ToolActivityPresentation::Support,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        "Discover caller-visible Projects with exact client_id/project filters or literal query. Pages default to 30 and clamp to 100 in stable Runtime Project ID order; continue with next_offset. Each page rechecks current visibility, so registry/authorization changes may repeat or omit rows: restart at offset=0 and deduplicate exact IDs. summary_only omits maintenance details; include_git_summary adds cached Git facts. Use resolve_workspace for unique path selection. Exact Runtime IDs and principal-scoped project_ref remain authoritative selectors; refs grant no authority.",
    ),
    model_spec(
        control_def(
            "register_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
        ),
        "Register an existing directory, including a non-Git or ad-hoc workspace, as a Project on one Runner. Successful onboarding returns the canonical Runtime Project id and a Server-issued project_ref when a stable root identity is available. Use this when the directory already exists; policy still bounds allowed paths.",
    ).with_discovery_keywords(&["register", "existing", "project", "directory", "注册", "现有", "项目", "目录"]),
    model_spec(
        control_def(
            "unregister_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
        ),
        "Unregister one exact Runner project using the revision from list_projects. Removes registration only; never deletes source, worktree, or branch. Re-list after an indeterminate outcome.",
    ),
    model_spec(
        control_def(
            "create_project",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
        ),
        "Create a directory on one Runner and register it as a Project. Successful onboarding returns the canonical Runtime Project id and a Server-issued project_ref when a stable root identity is available. Use this for a new workspace; existing directories belong on the registration path.",
    ).with_discovery_keywords(&["create", "new", "project", "directory", "scaffold", "创建", "新建", "项目", "目录"]),
    model_spec(
        control_def(
            "list_runners",
            super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                super::ToolAuditSessionInputPolicy::OmitTopLevel(&["client_id", "client_ids"]),
            ),
            ModelVisible,
            TOOL_CATEGORY_RUNTIME,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(RUNTIME_READ),
            false,
        )
        .with_activity(
            super::ToolActivityPresentation::Support,
            super::ToolActivityInteraction::NonMeaningful,
        ),
        "Find Runners by client_id/client_ids, query, status or limit; summary_only returns compact health/build/Job counts without full inventories. Full mode includes shared Job concurrency and host_context advisory metadata; never authority.",
    ),
    adaptive_runtime_direct(
        model_spec(
            control_def(
                "get_runtime_status",
                super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                    super::ToolAuditSessionInputPolicy::OmitTopLevel(&["client_id"]),
                ),
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            )
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read(),
            ),
            "Read runtime health and protocol/build/source alignment; exact client_id focuses one Runner and its Job concurrency. compact=true or summary_only=true selects sparse counts without inventories. Canonical/API default is full diagnostics; MCP defaults to sparse. Explicit compact=false adds bounded Server process and Session writer observations, not remote Runner resource measurements. Missing or unsupported measurements stay unavailable.",
        ).with_discovery_keywords(&["server", "runtime", "health", "runners", "connection", "status", "服务", "运行", "状态", "健康", "连接"]),
        20,
        super::ToolDirectReason::CoreWorkflow,
    ),
    model_spec(
            control_def(
                "read_current_window_activity",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            ),
            "Read bounded sanitized ActionAudit activity only for the current Host Window from transport identity. Requires runtime:read; every event retains current Project and principal visibility checks. No window selector, arguments, output, credentials, paths, or Host/model-state diagnosis. response_handed_at_ms means WebCodex constructed the response and handed it to the HTTP framework or returned from the handler; it does not prove downstream receipt. next_call_gap_ms exists only when a later canonical event was observed. Summary gap threshold counts are cumulative and, with totals, are descriptive WebCodex-observed request timing only; short gaps never prove Host cells, model turns, model thinking, frontend/network/user delay, or MCP orchestration. overlapping_call_count counts only persisted window_transition_kind=overlap and is never inferred from gap duration.",
        ),
    adaptive_runtime_direct(
        model_spec(
            control_def(
                "read_tool_manifest",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::NonMeaningful,
            ),
            "Find tools by query/category/intent and limit. Exact tool_name returns one contract and route. Discovery never executes work, registers Host tools or grants authority.",
        ),
        30,
        super::ToolDirectReason::CoreWorkflow,
    ),
];
