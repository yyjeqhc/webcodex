use super::RunnerCapabilityRequirement::{ApplyPatch, Shell};
use super::ToolVisibility::ModelHidden;
use super::{
    model_spec, permission_risk, ToolDeclaration, ToolDefinition, PERMISSION_RISK_PATCH,
    TOOL_CATEGORY_PATCH,
};
use crate::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    permission_risk(
        model_spec(
            ToolDeclaration {
                name: "apply_patch",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                visibility: ModelHidden,
                category: TOOL_CATEGORY_PATCH,
                runner_capability: Some(ApplyPatch),
                provider_id: TOOL_PROVIDER_RUNNER,
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
                session_evidence: super::ToolSessionEvidencePolicy::NONE.changed_paths(super::ToolChangedPathEvidence::ResultField("changed_paths")),
            }.define(),
            "Available for naturally patch-shaped contextual edits or externally supplied Codex patch input when patch form is genuinely the clearest reliable representation; it is not the default recovery for other edit failures. Repetitive targets need stable unique context (function/impl/type/test/module), not repeated lines or short fragments. Transactional: source rechecks, rollback, dry_run. matching_mode=unique is the default; matching_mode_rejected never justifies weakening the guard or switching to first_match. Reread/refine current source as needed, preserve unique/exact_unique on retry, and inspect the workspace on outcome_unknown.",
        ).with_discovery_keywords(&["apply", "patch", "add", "delete", "update", "files", "应用", "补丁", "新增", "删除", "修改", "文件"]),
        PERMISSION_RISK_PATCH,
    ),
    permission_risk(
        model_spec(
            ToolDeclaration {
                name: "apply_unified_diff",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                visibility: ModelHidden,
                category: TOOL_CATEGORY_PATCH,
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
                path_hint: Patch,
                destructive: true,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE.changed_paths(super::ToolChangedPathEvidence::ResultField("affected_files")),
            }.define(),
            "External raw unified-diff mutation path. Use only when the input is already a standard unified diff and that representation is the clearest reliable mutation. Performs bounded preflight before applying and never needs a separate validation call. Shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata; choose other mutation paths according to the task rather than converting model-generated work into unified diff by default.",
        ).with_discovery_keywords(&["apply", "unified", "diff", "patch", "应用", "统一", "补丁", "差异"]),
        PERMISSION_RISK_PATCH,
    ),
];
