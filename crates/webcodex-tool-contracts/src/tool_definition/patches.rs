use super::RunnerCapabilityRequirement::{ApplyPatch, Shell};
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, permission_risk, ToolDefinition,
    PERMISSION_RISK_PATCH, TOOL_CATEGORY_PATCH,
};
use crate::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};
use crate::registry::input_schemas::{apply_patch_input_schema, apply_unified_diff_input_schema};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        permission_risk(
            model_spec(
                def(
                "apply_patch",
                ModelVisible,
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
                ),
                "Primary model edit path for contextual/multi-file Codex patches. Transactional with SHA rechecks, rollback, dry_run, recovery. Put multiple edits to the same file as multiple chunks inside one `*** Update File` operation; duplicate file operations for the same path are rejected. A deterministic zero-write context mismatch or validated unique-fuzzy strict rejection may include body-free diagnostics plus Server-derived recovery; when recovery.action=read_files, pass recovery.items to read_files for the same project, then regenerate the whole patch. Ambiguous strict rejection never selects the Runner's first candidate: expand exact unique context instead. outcome_unknown requires workspace inspection first. Set strict_matching=true for exact-unique positioning and never relax it as a recovery shortcut. Use apply_text_edits for small exact edits; unified diff for external diffs.",
                apply_patch_input_schema,
            ),
            PERMISSION_RISK_PATCH,
        ),
        60,
    ),
    permission_risk(
        model_spec(
            def(
                "apply_unified_diff",
                ModelVisible,
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
            ),
            "External raw unified-diff mutation path. Prefer apply_patch for model-generated changes and apply_text_edits for small exact guarded edits. Performs bounded preflight before applying and never needs a separate validation call. Input must be a standard unified diff; shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata.",
            apply_unified_diff_input_schema,
        ),
        PERMISSION_RISK_PATCH,
    ),
];
