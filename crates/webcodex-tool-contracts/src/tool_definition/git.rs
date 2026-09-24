use super::RunnerCapabilityRequirement::GitOrShell;
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, change_summary_like, def, git_like, model_spec, require_all_scopes,
    ToolDefinition, TOOL_CATEGORY_GIT,
};
use crate::metadata::{
    ToolPathHint::{None as NoPath, PathList},
    ToolRisk::{ProjectWrite, Read},
    JOB_RUN, PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const SUMMARY_DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        change_summary_like(git_like(model_spec(
            def(
                "git_review_summary",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("project"),
                    super::ToolAuditResultField::value("scope"),
                    super::ToolAuditResultField::value("stats"),
                    super::ToolAuditResultField::value("coverage"),
                    super::ToolAuditResultField::value("truncation"),
                    super::ToolAuditResultField::value("deterministic"),
                    super::ToolAuditResultField::value("llm_summary"),
                    super::ToolAuditResultField::value("truncated"),
                    super::ToolAuditResultField::value("reason_code"),
                    super::ToolAuditResultField::array_len("signal_count", "signals"),
                    super::ToolAuditResultField::array_len("file_count", "files"),
                ])
                .drop_null_request_values(),
                ModelVisible,
                TOOL_CATEGORY_GIT,
                Some(GitOrShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read(),
            ),
            "Deterministic bounded committed-range review map for broad or unknown ranges when a file/change map helps choose targeted git_diff_hunks/read_files. Small bounded understood committed diffs may use native Git directly. Does not judge correctness and never mutates the repository.",
        ))),
        120,
    ),
    adaptive_runtime_direct(
        change_summary_like(git_like(model_spec(
            def(
                "show_changes",
                super::ToolAuditPolicy::TYPED_CANONICAL.context(
                    super::ToolAuditContextPolicy::Fields(&[
                        super::ToolAuditResultField::value("clean"),
                        super::ToolAuditResultField::value("branch"),
                        super::ToolAuditResultField::value("head"),
                        super::ToolAuditResultField::value("upstream"),
                        super::ToolAuditResultField::value("ahead"),
                        super::ToolAuditResultField::value("behind"),
                        super::ToolAuditResultField::value("counts"),
                        super::ToolAuditResultField::value("changed_files"),
                    ]),
                ),
                ModelVisible,
                TOOL_CATEGORY_GIT,
                Some(GitOrShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::WorkspaceReview).diff_review(super::ToolDiffReviewEvidence::ArgumentBool("include_diff")),
            ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
            "Canonical bounded workspace-wide review when a worktree overview, compact Session signals, or structured closeout evidence is useful; tiny targeted Git observations need not call it first. Read-only; recent Session event history is omitted unless session_event_limit is explicitly positive. If hunks truncate, diff_review_handoff classifies page/line/mixed truncation and provides a parser-ready git_diff_hunks recovery call.",
        ).with_gpt_action_description("Review current worktree changes and optional bounded diff hunks before handoff. If diff output truncates, follow the returned git_diff_hunks recovery call. Read-only; recent Session event history is opt-in."))),
        130,
    ),
];

pub(super) const DETAIL_DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(git_like(model_spec(
        def(
            "git_commit_paths",
            super::ToolAuditPolicy::TYPED_CANONICAL.drop_null_request_values(),
            ModelVisible,
            TOOL_CATEGORY_GIT,
            Some(GitOrShell),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
            PathList,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Commit exactly requested changed file paths with an atomic expected_head fence and isolated temporary index; normal Git clean filters may run under job:run authority, ordinary commit hooks are bypassed so they cannot add unrelated paths, and the tool never pushes.",
    )), &[PROJECT_WRITE, JOB_RUN]),
    git_like(model_spec(
        def(
            "git_status",
            super::ToolAuditPolicy::TYPED_CANONICAL
                .context(super::ToolAuditContextPolicy::WorkingTreeStatus),
            ModelVisible,
            TOOL_CATEGORY_GIT,
            Some(GitOrShell),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::WorkspaceReview),
        )
        .with_composition_policy(super::ToolCompositionPolicy::Parallel)
        .with_host_orchestration_hint(
            super::ToolHostOrchestrationHint::independent_parallel_read(),
        ),
        "Run git status --porcelain for a project.",
    )),
    adaptive_runtime_direct(
        change_summary_like(git_like(model_spec(
            def(
                "git_diff_hunks",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("project"),
                    super::ToolAuditResultField::value("scope"),
                    super::ToolAuditResultField::value("cached"),
                    super::ToolAuditResultField::value("hunk_count"),
                    super::ToolAuditResultField::value("truncated"),
                    super::ToolAuditResultField::value("truncation_reasons"),
                    super::ToolAuditResultField::value("has_more"),
                    super::ToolAuditResultField::value("exit_code"),
                    super::ToolAuditResultField::value("error_kind"),
                    super::ToolAuditResultField::value("reason_code"),
                    super::ToolAuditResultField::array_len("file_count", "files"),
                ])
                .session_input(super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                    "continuation",
                ])),
                ModelVisible,
                TOOL_CATEGORY_GIT,
                Some(GitOrShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::DiffReview).diff_review(super::ToolDiffReviewEvidence::Always),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read(),
            ),
            "Targeted/paged diff review with scope/fence-bound opaque continuation for worktree/cached or exact base/head ranges; use when safe bounded traversal matters. max_page_bytes controls the raw producer page and defaults to the input-schema shared safe producer maximum; it is separate from the 512 KiB final model-facing result ceiling. Copy continuation only through the returned parser-ready next_call. recovery.later_hunks.next_call means the next logical diff record, never an intra-hunk cursor. recovery.current_hunk.next_call is current-hunk-only: bounded refinement when independently proven, or the existing exact hunk-fragment token at the next complete diff line. Line-budget and page-byte-budget truncation may both use that fragment only with proven positive complete-line progress; otherwise fixed ceilings advertise no recovery when safe forward progress is not proven. Read-only.",
        ).with_gpt_action_description("Read bounded diff hunks for worktree/cached or exact base/head ranges. Follow recovery.later_hunks only for the next logical record; use recovery.current_hunk for returned exact complete-line hunk continuation when proven safe. Never guess offsets."))),
        125,
    ),
    git_like(model_spec(
        def(
            "git_log",
            super::ToolAuditPolicy::TYPED_CANONICAL.context(
                super::ToolAuditContextPolicy::Fields(&[
                    super::ToolAuditResultField::value("commits"),
                    super::ToolAuditResultField::value("head_commit"),
                    super::ToolAuditResultField::value("next_skip"),
                    super::ToolAuditResultField::value("suggested_call"),
                    super::ToolAuditResultField::value("truncated"),
                ]),
            ),
            ModelVisible,
            TOOL_CATEGORY_GIT,
            Some(GitOrShell),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
        "Return bounded structured recent git commit history for a project. The first page resolves current HEAD to one exact 40-hex head_commit and reads that commit snapshot; later pages may supply the same head_commit so branch movement or rewrites cannot drift the traversal. When truncated and bounded forward progress is available, suggested_call is the sole parser-ready continuation and carries project, exact head_commit, effective limit, next skip, and Session identity when present. next_skip remains domain metadata only; null means the final page or the existing 10000 skip bound prevents a safe forward page. If the exact commit snapshot is no longer available, fail closed rather than falling back to current HEAD. Retained-tail or malformed source records also fail closed. Does not return commit bodies or modify the worktree.",
    )),
];
