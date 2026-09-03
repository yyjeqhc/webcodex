use super::RunnerCapabilityRequirement::GitOrShell;
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, change_summary_like, context_recovery_only, def, git_like, model_spec,
    ToolDefinition, TOOL_CATEGORY_GIT,
};
use crate::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::Read, PROJECT_READ, TOOL_PROVIDER_RUNNER,
};
use crate::registry::input_schemas::{
    git_diff_hunks_input_schema, git_diff_input_schema, git_diff_summary_input_schema,
    git_log_input_schema, git_review_summary_input_schema, git_status_input_schema,
    show_changes_input_schema,
};

pub(super) const SUMMARY_DEFINITIONS: &[ToolDefinition] = &[
    context_recovery_only(change_summary_like(git_like(model_spec(
        def(
            "git_diff_summary",
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
        ),
        "Read-only git diff summary for a project: `git status --porcelain`, `git diff --stat`, and a parsed changed-file list. Does not modify the worktree.",
        git_diff_summary_input_schema,
    )))),
    adaptive_runtime_direct(
        context_recovery_only(change_summary_like(git_like(model_spec(
            def(
                "git_review_summary",
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
            ),
            "Deterministic bounded committed-range review map. Use before targeted git_diff_hunks/read_file during branch or PR review. Does not judge correctness and never mutates the repository.",
            git_review_summary_input_schema,
        )))),
        120,
    ),
    adaptive_runtime_direct(
        context_recovery_only(change_summary_like(git_like(model_spec(
            def(
                "show_changes",
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
            ),
            "Default inspect/review tool before final response. Read-only worktree overview with status, warnings, next actions, and bounded hunks. If hunks truncate, diff_review_handoff points to git_diff_hunks for focused/paged review.",
            show_changes_input_schema,
        )))),
        130,
    ),
];

pub(super) const DETAIL_DEFINITIONS: &[ToolDefinition] = &[
    context_recovery_only(git_like(model_spec(
        def(
            "git_status",
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
        ),
        "Run git status --porcelain for a project.",
        git_status_input_schema,
    ))),
    context_recovery_only(git_like(model_spec(
        def(
            "git_diff",
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
        ),
        "Run git diff for a project, optionally scoped to paths.",
        git_diff_input_schema,
    ))),
    context_recovery_only(change_summary_like(git_like(model_spec(
        def(
            "git_diff_hunks",
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
        ),
        "Targeted/paged diff review for worktree/cached or exact base/head ranges, with paths and scope-bound opaque continuation. Replay scope and paging inputs unchanged. Continuation pages later records; hunk_line_limit needs a fresh call with larger max_hunk_lines and/or narrower paths. Read-only.",
        git_diff_hunks_input_schema,
    )))),
    context_recovery_only(git_like(model_spec(
        def(
            "git_log",
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
        ),
        "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.",
        git_log_input_schema,
    ))),
];
