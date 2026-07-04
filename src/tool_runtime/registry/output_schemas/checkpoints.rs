use serde_json::Value;

use super::super::input_schemas::{checkpoint_labels_schema, checkpoint_validation_schema};
use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "workspace_checkpoint_create" => Some(wrapped_output_schema(vec![
            (
                "checkpoint_id",
                schema_type("string", "Created wc_ckpt_* id."),
            ),
            ("project", schema_type("string", "Project input.")),
            (
                "resolved_project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "title",
                nullable_schema("string", "Optional checkpoint title."),
            ),
            ("kind", schema_type("string", "Semantic checkpoint kind.")),
            (
                "labels",
                checkpoint_labels_schema("Simple checkpoint labels."),
            ),
            (
                "validation",
                checkpoint_validation_schema("Bounded validation metadata."),
            ),
            (
                "head",
                schema_type("string", "HEAD commit captured by the checkpoint."),
            ),
            (
                "branch",
                nullable_schema("string", "Current branch, if attached."),
            ),
            ("created_at", schema_type("integer", "Unix timestamp.")),
            (
                "tracked_diff_bytes",
                schema_type("integer", "Unstaged tracked diff size in bytes."),
            ),
            (
                "staged_diff_bytes",
                schema_type("integer", "Staged diff size in bytes."),
            ),
            (
                "untracked_files",
                array_schema(
                    open_object_schema("Stored untracked file metadata."),
                    "Stored untracked file metadata.",
                ),
            ),
            (
                "skipped_files",
                array_schema(
                    open_object_schema("Skipped file metadata."),
                    "Skipped files and reasons.",
                ),
            ),
            (
                "status_summary",
                open_object_schema("Parsed git status summary."),
            ),
            (
                "complete",
                schema_type(
                    "boolean",
                    "True when checkpoint content is complete and restorable.",
                ),
            ),
            (
                "storage_path",
                schema_type(
                    "string",
                    "Server state-dir checkpoint path, outside the project worktree.",
                ),
            ),
        ])),
        "workspace_checkpoint_list" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Project input.")),
            (
                "resolved_project",
                schema_type("string", "Resolved runtime project id."),
            ),
            ("limit", schema_type("integer", "Effective list limit.")),
            (
                "checkpoints",
                array_schema(
                    open_object_schema("Checkpoint metadata."),
                    "Checkpoint metadata without full diff/content.",
                ),
            ),
        ])),
        "workspace_checkpoint_show" => Some(wrapped_output_schema(vec![
            ("checkpoint_id", schema_type("string", "Checkpoint id.")),
            ("project", schema_type("string", "Project input.")),
            (
                "resolved_project",
                schema_type("string", "Resolved runtime project id."),
            ),
            ("title", nullable_schema("string", "Optional title.")),
            ("kind", schema_type("string", "Semantic checkpoint kind.")),
            (
                "labels",
                checkpoint_labels_schema("Simple checkpoint labels."),
            ),
            (
                "validation",
                checkpoint_validation_schema("Bounded validation metadata."),
            ),
            ("head", schema_type("string", "Checkpoint HEAD commit.")),
            (
                "branch",
                nullable_schema("string", "Checkpoint branch, if attached."),
            ),
            ("created_at", schema_type("integer", "Unix timestamp.")),
            (
                "files",
                array_schema(
                    open_object_schema("Tracked/untracked file metadata."),
                    "Checkpoint file list without full diff/content.",
                ),
            ),
            (
                "skipped_files",
                array_schema(
                    open_object_schema("Skipped file metadata."),
                    "Skipped files and reasons.",
                ),
            ),
            (
                "status_summary",
                open_object_schema("Parsed git status summary."),
            ),
            (
                "storage_path",
                schema_type(
                    "string",
                    "Server state-dir checkpoint path, outside the project worktree.",
                ),
            ),
        ])),
        "workspace_checkpoint_restore" => Some(wrapped_output_schema(vec![
            (
                "restored",
                schema_type("boolean", "True when restore completed."),
            ),
            (
                "checkpoint_id",
                schema_type("string", "Restored checkpoint id."),
            ),
            ("project", schema_type("string", "Project input.")),
            (
                "resolved_project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "changed_paths",
                array_schema(
                    schema_type("string", "Project-relative path."),
                    "Paths restored from the checkpoint.",
                ),
            ),
            (
                "warnings",
                array_schema(
                    open_object_schema("Warning."),
                    "Warnings emitted during restore.",
                ),
            ),
        ])),
        "workspace_checkpoint_delete" => Some(wrapped_output_schema(vec![
            (
                "deleted",
                schema_type("boolean", "True when checkpoint file was deleted."),
            ),
            (
                "checkpoint_id",
                schema_type("string", "Deleted checkpoint id."),
            ),
            ("project", schema_type("string", "Project input.")),
            (
                "resolved_project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "storage_path",
                schema_type("string", "Deleted checkpoint path."),
            ),
        ])),
        _ => None,
    }
}
