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
    change_summary_like(git_like(model_spec(
        def(
            "read_git_review_summary",
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
            super::ToolSessionEvidencePolicy::NONE
                .review(super::ToolReviewEvidence::ReadOnlyInspection),
        )
        .with_composition_policy(super::ToolCompositionPolicy::Parallel)
        .with_host_orchestration_hint(super::ToolHostOrchestrationHint::independent_parallel_read()),
        "Summarize an exact committed Git range as bounded file/class/symbol metadata and change statistics, without returning patch bodies. Commits and the merge-base are pinned; incomplete coverage stays explicit. Ordinary review uses review_changes. Read-only.",
    ))),
    adaptive_runtime_direct(
        change_summary_like(git_like(model_spec(
            def(
                "review_changes",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("project"),
                    super::ToolAuditResultField::value("snapshot"),
                    super::ToolAuditResultField::value("continuation"),
                    super::ToolAuditResultField::value("reason_code"),
                    super::ToolAuditResultField::array_len("signal_count", "signals"),
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
                super::ToolSessionEvidencePolicy::NONE
                    .review(super::ToolReviewEvidence::DiffReview)
                    .diff_review(super::ToolDiffReviewEvidence::Always),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel),
            "Review a fenced Git snapshot with three-line context. Workspace is the net HEAD-to-worktree patch, including staged/untracked contents, not an index-only plan. Committed scope pins commits/merge-base. Follow only next_call with unchanged inputs; workspace changes fail closed. Use get_git_status for cleanliness.",
        ))),
        120,
        super::ToolDirectReason::CoreWorkflow,
    ),
    change_summary_like(git_like(
            model_spec(
                def(
                    "read_workspace_changes",
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
                    super::ToolSessionEvidencePolicy::NONE
                        .review(super::ToolReviewEvidence::WorkspaceReview)
                        .diff_review(super::ToolDiffReviewEvidence::ArgumentBool("include_diff")),
                )
                .with_composition_policy(super::ToolCompositionPolicy::Parallel),
                "Read a structured workspace summary: Git branch, HEAD, staged/unstaged/untracked files and diff statistics, without a patch by default. Optional include_diff adds bounded unstaged hunks and safe untracked previews, not a complete staged patch. Ordinary code review uses review_changes; recent Session event history is opt-in. Read-only.",
            ),
        )),
];
pub(super) const DETAIL_DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(git_like(model_spec(
        def(
            "commit_git_paths",
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
            "get_git_status",
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
        "Read current Git dirty/staged/untracked status for one exact Project without generating a review diff; use review_changes for patch content.",
    )),
    change_summary_like(git_like(
        model_spec(
            def(
                "read_git_diff_hunks",
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
                super::ToolSessionEvidencePolicy::NONE
                    .review(super::ToolReviewEvidence::DiffReview)
                    .diff_review(super::ToolDiffReviewEvidence::Always),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read(),
            ),
            "Read bounded Git diff hunks for selected paths: cached=true selects staged changes, omission selects unstaged changes; exact base_commit+head_commit selects a committed range. Preserves source fences, bounded page/hunk continuation and safe recovery. Use review_changes for a complete net-workspace review. Read-only.",
        ),
    )),
    git_like(model_spec(
        def(
            "read_git_log",
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
