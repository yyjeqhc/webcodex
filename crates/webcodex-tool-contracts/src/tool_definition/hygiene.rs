use super::RunnerCapabilityRequirement::{GitOrShell, Shell, StructuredProcess};
use super::ToolVisibility::ModelVisible;
use super::{def, git_like, model_spec, ToolDefinition, TOOL_CATEGORY_CLEANUP};
use crate::metadata::{
    ToolPathHint::{None as NoPath, PathList},
    ToolRisk::{ProjectWrite, Read},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[model_spec(
        def(
            "check_workspace_hygiene",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_CLEANUP,
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
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::HygieneReview),
        ),
        "Default pre-final workspace hygiene review; read-only. Detects dirty worktree, untracked temp/smoke files, cache dirs, secret-like names, and large untracked files before validation or handoff. Never reads file contents.",
)];

pub(super) const CLEANUP_DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "delete_project_files",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_CLEANUP,
            Some(Shell),
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
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Delete selected project-relative files only; safer than arbitrary rm for cleanup.",
    ),
    git_like(model_spec(
        def(
            "restore_git_paths",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_CLEANUP,
            Some(StructuredProcess),
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
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Restore selected tracked paths with git restore; does not remove untracked files.",
    )),
    git_like(model_spec(
        def(
            "discard_untracked",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_CLEANUP,
            Some(StructuredProcess),
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
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Discard selected untracked files with git clean -f -- <paths>.",
    )),
];
