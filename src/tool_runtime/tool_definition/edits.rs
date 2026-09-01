use super::AgentCapability::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{adaptive_runtime_direct, def, model_spec, ToolDefinition, TOOL_CATEGORY_EDIT};
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
    adaptive_runtime_direct(
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
            "Canonical transactional file-change tool, preferred for ordinary local edit/create/delete/rename on the current worktree, not HEAD. Whole batch uses per-file hashes and dry_run; prefer over whole-file. Exact edits are unique by default; optional 1-based inclusive line_scope fences full matches while occurrence remains global source order. SHA conflict requires reread.",
            apply_text_edits_input_schema,
        ),
        60,
    ),
];
