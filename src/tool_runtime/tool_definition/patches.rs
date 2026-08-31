use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{def, model_spec, ToolDefinition, TOOL_CATEGORY_PATCH};
use crate::tool_runtime::metadata::{
    ToolPathHint::Patch, ToolRisk::ProjectWrite, PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};
use crate::tool_runtime::registry::input_schemas::apply_unified_diff_input_schema;

pub(super) const DEFINITIONS: &[ToolDefinition] = &[model_spec(
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
)];
