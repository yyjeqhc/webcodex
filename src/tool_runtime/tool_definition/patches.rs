use super::AgentCapability::{ApplyPatch, Shell};
use super::ToolVisibility::ModelVisible;
use super::{adaptive_runtime_direct, def, model_spec, ToolDefinition, TOOL_CATEGORY_PATCH};
use crate::tool_runtime::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};
use crate::tool_runtime::registry::input_schemas::{
    apply_patch_input_schema, apply_unified_diff_input_schema,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            def(
                "apply_patch",
                ModelVisible,
                TOOL_CATEGORY_PATCH,
                Some(ApplyPatch),
                TOOL_PROVIDER_AGENT,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(PROJECT_WRITE),
                true,
                Patch,
                false,
                false,
            ),
            "Primary model edit path for contextual/multi-file Codex patches. Transactional with SHA rechecks, rollback, dry_run, recovery. Inspect strict_match; set strict_matching=true to require exact-unique positioning. Use apply_text_edits for small exact edits; unified diff for external diffs.",
            apply_patch_input_schema,
        ),
        60,
    ),
    model_spec(
    def(
        "apply_unified_diff",
        ModelVisible,
        TOOL_CATEGORY_PATCH,
        Some(Shell),
        TOOL_PROVIDER_AGENT,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: ProjectWrite,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(PROJECT_WRITE),
        true,
        Patch,
        false,
        false,
    ),
    "External raw unified-diff mutation path. Prefer apply_patch for model-generated changes and apply_text_edits for small exact guarded edits. Performs bounded preflight before applying and never needs a separate validation call. Input must be a standard unified diff; shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata.",
    apply_unified_diff_input_schema,
),
];
