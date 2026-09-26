use super::RunnerCapabilityRequirement::{ApplyPatch, Shell};
use super::ToolVisibility::ModelHidden;
use super::{
    def, model_spec, permission_risk, ToolDefinition, PERMISSION_RISK_PATCH, TOOL_CATEGORY_PATCH,
};
use crate::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    permission_risk(
        model_spec(
            def(
            "apply_patch",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelHidden,
            TOOL_CATEGORY_PATCH,
            Some(ApplyPatch),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
            Patch,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE.changed_paths(super::ToolChangedPathEvidence::ResultField("changed_paths")),
            ),
            "Available for naturally patch-shaped contextual edits or externally supplied Codex patch input when patch form is genuinely the clearest reliable representation; it is not the default recovery for other edit failures. Repetitive targets need stable unique context (function/impl/type/test/module), not repeated lines or short fragments. Transactional: source rechecks, rollback, dry_run. matching_mode=unique is the default; matching_mode_rejected never justifies weakening the guard or switching to first_match. Reread/refine current source as needed, preserve unique/exact_unique on retry, and inspect the workspace on outcome_unknown.",
        ),
        PERMISSION_RISK_PATCH,
    ),
    permission_risk(
        model_spec(
            def(
                "apply_unified_diff",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelHidden,
                TOOL_CATEGORY_PATCH,
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
                Patch,
                true,
                false,
                super::ToolSessionEvidencePolicy::NONE.changed_paths(super::ToolChangedPathEvidence::ResultField("affected_files")),
            ),
            "External raw unified-diff mutation path. Use only when the input is already a standard unified diff and that representation is the clearest reliable mutation. Performs bounded preflight before applying and never needs a separate validation call. Shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata; choose other mutation paths according to the task rather than converting model-generated work into unified diff by default.",
        ),
        PERMISSION_RISK_PATCH,
    ),
];
