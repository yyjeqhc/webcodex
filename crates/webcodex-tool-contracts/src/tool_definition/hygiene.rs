use super::RunnerCapabilityRequirement::{GitOrShell, Shell, StructuredProcess};
use super::ToolVisibility::ModelVisible;
use super::{git_like, model_spec, ToolDeclaration, ToolDefinition, TOOL_CATEGORY_CLEANUP};
use crate::metadata::{
    ToolPathHint::{None as NoPath, PathList},
    ToolRisk::{ProjectWrite, Read},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[model_spec(
        ToolDeclaration {
            name: "check_workspace_hygiene",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CLEANUP,
            runner_capability: Some(GitOrShell),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            required_scope: Some(PROJECT_READ),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::HygieneReview),
        }.define(),
        "Default pre-final workspace hygiene review; read-only. Detects dirty worktree, untracked temp/smoke files, cache dirs, secret-like names, and large untracked files before validation or handoff. Never reads file contents.",
)];

pub(super) const CLEANUP_DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        ToolDeclaration {
            name: "delete_project_files",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CLEANUP,
            runner_capability: Some(Shell),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: PathList,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        "Delete selected project-relative files only; safer than arbitrary rm for cleanup.",
    ),
    git_like(model_spec(
        ToolDeclaration {
            name: "restore_git_paths",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CLEANUP,
            runner_capability: Some(StructuredProcess),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: PathList,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        "Restore selected tracked paths with git restore; does not remove untracked files.",
    )),
    git_like(model_spec(
        ToolDeclaration {
            name: "discard_untracked",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CLEANUP,
            runner_capability: Some(StructuredProcess),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: PathList,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        "Discard selected untracked files with git clean -f -- <paths>.",
    )),
];
