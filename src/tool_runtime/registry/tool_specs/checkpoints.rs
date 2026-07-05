use super::super::input_schemas::{
    checkpoint_create_input_schema, checkpoint_delete_input_schema, checkpoint_list_input_schema,
    checkpoint_restore_input_schema, checkpoint_show_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "workspace_checkpoint_create",
            "Create a bounded workspace checkpoint outside the project worktree. Captures HEAD, status, text diffs, and optional small untracked text files.",
            checkpoint_create_input_schema(),
        ),
        tool_spec(
            "workspace_checkpoint_list",
            "List checkpoint metadata for a project without returning full diffs or saved file content.",
            checkpoint_list_input_schema(),
        ),
        tool_spec(
            "workspace_checkpoint_show",
            "Show bounded checkpoint metadata, file list, skipped files, and optional diff stat. Does not return full diff/content by default.",
            checkpoint_show_input_schema(),
        ),
        tool_spec(
            "workspace_checkpoint_restore",
            "Restore a checkpoint after confirm=true. Requires matching HEAD and refuses unsafe current state rather than half-restoring.",
            checkpoint_restore_input_schema(),
        ),
        tool_spec(
            "workspace_checkpoint_delete",
            "Delete one checkpoint JSON file after confirm=true. Does not touch the project worktree.",
            checkpoint_delete_input_schema(),
        ),
    ]
}
