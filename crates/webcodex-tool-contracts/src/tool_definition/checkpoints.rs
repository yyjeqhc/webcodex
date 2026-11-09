use super::RunnerCapabilityRequirement::{FileRead, FileWrite, OwnerOnly};
use super::ToolVisibility::ModelVisible;
use super::{
    git_like, model_spec, permission_risk, ToolDeclaration, ToolDefinition, PERMISSION_RISK_PATCH,
    TOOL_CATEGORY_CHECKPOINT,
};
use crate::metadata::{
    ToolPathHint::{None as NoPath, Patch},
    ToolRisk::{ProjectWrite, Read},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_NATIVE,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    git_like(model_spec(
        ToolDeclaration {
            name: "create_workspace_checkpoint",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL.context(
                super::ToolAuditContextPolicy::Fields(&[
                    super::ToolAuditResultField::value("checkpoint_id"),
                    super::ToolAuditResultField::value("head"),
                    super::ToolAuditResultField::value("branch"),
                    super::ToolAuditResultField::value("complete"),
                    super::ToolAuditResultField::value("tracked_diff_bytes"),
                    super::ToolAuditResultField::value("staged_diff_bytes"),
                    super::ToolAuditResultField::value("untracked_file_count"),
                    super::ToolAuditResultField::value("status_summary"),
                    super::ToolAuditResultField::value("kind"),
                ]),
            ),
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CHECKPOINT,
            runner_capability: Some(FileRead),
            provider_id: TOOL_PROVIDER_NATIVE,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: super::ToolRisk::CheckpointManage,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_READ),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE
                .failure(super::ToolFailureEvidence::ProvenNoStateChangeNonActionable)
                .lifecycle(super::ToolSessionLifecycleEffect::Mutation),
        }.define(),
        "Create a bounded workspace checkpoint outside the project worktree. Captures HEAD, status, text diffs, and optional small untracked text files.",
    )),
    model_spec(
        ToolDeclaration {
            name: "list_workspace_checkpoints",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CHECKPOINT,
            runner_capability: Some(OwnerOnly),
            provider_id: TOOL_PROVIDER_NATIVE,
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
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }.define(),
        "List checkpoint metadata for a project without returning full diffs or saved file content.",
    ),
    model_spec(
        ToolDeclaration {
            name: "read_workspace_checkpoint",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CHECKPOINT,
            runner_capability: Some(OwnerOnly),
            provider_id: TOOL_PROVIDER_NATIVE,
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
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }.define(),
        "Show bounded checkpoint metadata, file list, skipped files, and optional diff stat. Does not return full diff/content by default.",
    ),
    git_like(permission_risk(
        model_spec(
            ToolDeclaration {
                name: "restore_workspace_checkpoint",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                visibility: ModelVisible,
                category: TOOL_CATEGORY_CHECKPOINT,
                runner_capability: Some(FileWrite),
                provider_id: TOOL_PROVIDER_NATIVE,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                required_scope: Some(PROJECT_WRITE),
                requires_project: true,
                path_hint: Patch,
                destructive: true,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE.changed_paths(super::ToolChangedPathEvidence::ResultField("changed_paths")).lifecycle(super::ToolSessionLifecycleEffect::Mutation),
            }.define(),
            "Restore a checkpoint after confirm=true. Requires matching HEAD and refuses unsafe current state rather than half-restoring.",
        ),
        PERMISSION_RISK_PATCH,
    )),
    model_spec(
        ToolDeclaration {
            name: "delete_workspace_checkpoint",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL,
            visibility: ModelVisible,
            category: TOOL_CATEGORY_CHECKPOINT,
            runner_capability: Some(OwnerOnly),
            provider_id: TOOL_PROVIDER_NATIVE,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: NoPath,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE.lifecycle(super::ToolSessionLifecycleEffect::Mutation),
        }.define(),
        "Delete one checkpoint JSON file after confirm=true. Does not touch the project worktree.",
    ),
];
