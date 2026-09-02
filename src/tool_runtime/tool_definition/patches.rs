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
            "Apply model-generated Codex *** Begin Patch DSL transactionally. Supports add/update/delete/move with bounded context matching, Runner-side source SHA rechecks, rollback, dry_run, and structured recovery. Prefer apply_text_edits for small precise edits; use apply_unified_diff for external Git diffs.",
            apply_patch_input_schema,
        ),
        65,
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
    "Canonical complex/multi-file raw unified-diff mutation. Prefer apply_text_edits for ordinary guarded local edits. This tool performs its own bounded preflight, applies only after it passes, and never needs a separate validation call. Input must be a standard unified diff; shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata.",
    apply_unified_diff_input_schema,
),
];
