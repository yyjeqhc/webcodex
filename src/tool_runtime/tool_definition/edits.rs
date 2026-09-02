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
        "Create new files or intentional whole-file rewrites. Existing-file overwrite requires the exact current expected_sha256. Prefer apply_patch for model-generated changes and apply_text_edits for small exact guarded edits; inspect current content and worktree changes before replacing a file.",
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
            "Precision fallback for small exact guarded file changes on the current worktree. Transactional and SHA-guarded; exact matches are unique by default, occurrence remains global source order, and optional line_scope fences matches. Prefer apply_patch for contextual, multi-hunk, or multi-file model changes.",
            apply_text_edits_input_schema,
        ),
        60,
    ),
];
