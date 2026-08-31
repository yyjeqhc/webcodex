use super::AgentCapability::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{def, model_spec, ToolDefinition, TOOL_CATEGORY_EDIT};
use crate::tool_runtime::metadata::{
    ToolPathHint::{PathList, SinglePath},
    ToolRisk::ProjectWrite,
    PROJECT_WRITE, TOOL_PROVIDER_AGENT,
};
use crate::tool_runtime::registry::input_schemas::{
    apply_text_edits_input_schema, write_project_file_input_schema,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "write_project_file",
            ModelVisible,
            TOOL_CATEGORY_EDIT,
            Some(FileWrite),
            TOOL_PROVIDER_AGENT,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
            SinglePath,
            false,
            false,
        ),
        "Create new files or intentional whole-file rewrites. Not preferred for ordinary local edits—prefer apply_text_edits. Inspect current content and worktree changes before overwriting; do not silently clobber user edits.",
        write_project_file_input_schema,
    ),
    model_spec(
        def(
            "apply_text_edits",
            ModelVisible,
            TOOL_CATEGORY_EDIT,
            Some(FileWrite),
            TOOL_PROVIDER_AGENT,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
            PathList,
            false,
            false,
        ),
        "Canonical transactional file-change editing on the current worktree. Uses per-file SHA guards and dry_run. Exact edits are unique by default; optional 1-based inclusive line_scope fences full matches while occurrence remains global source order. SHA conflict requires reread.",
        apply_text_edits_input_schema,
    ),
];
