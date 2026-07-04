use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "apply_patch" | "apply_patch_checked" => Some(wrapped_output_schema(vec![
            (
                "exit_code",
                nullable_schema("integer", "Patch command exit code."),
            ),
            ("stdout", schema_type("string", "Patch command stdout.")),
            ("stderr", schema_type("string", "Patch command stderr.")),
            (
                "changed_files",
                array_schema(
                    open_object_schema("Changed file summary."),
                    "Changed files.",
                ),
            ),
            (
                "applied",
                schema_type("boolean", "Whether the patch was applied."),
            ),
            (
                "check",
                open_object_schema("Patch validation/check result."),
            ),
        ])),
        "validate_patch" => Some(wrapped_output_schema(vec![
            (
                "valid",
                schema_type("boolean", "Whether the patch passed validation."),
            ),
            (
                "applies",
                schema_type("boolean", "Whether git apply --check succeeded."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Validation command exit code."),
            ),
            ("stdout", schema_type("string", "Validation stdout.")),
            ("stderr", schema_type("string", "Validation stderr.")),
            (
                "diff_stat",
                schema_type("string", "Patch diff stat, when available."),
            ),
        ])),
        "replace_line_range" | "delete_line_range" => Some(wrapped_output_schema(vec![
            ("path", schema_type("string", "Project-relative path.")),
            (
                "start_line",
                schema_type("integer", "1-based inclusive start line."),
            ),
            (
                "end_line",
                schema_type("integer", "1-based inclusive end line."),
            ),
            (
                "old_sha256",
                schema_type("string", "sha256 of the original selected range."),
            ),
            (
                "new_sha256",
                schema_type("string", "sha256 of the entire file after the edit."),
            ),
            (
                "old_line_count",
                schema_type("integer", "Number of original selected lines."),
            ),
            (
                "new_line_count",
                schema_type("integer", "Number of replacement lines."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes in the file written after the edit."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether file contents changed."),
            ),
        ])),
        "apply_text_edits" => Some(wrapped_output_schema(vec![
            ("path", schema_type("string", "Project-relative path.")),
            (
                "dry_run",
                schema_type("boolean", "Whether this was a dry-run (no write)."),
            ),
            (
                "applied_count",
                schema_type("integer", "Number of edits applied in the batch."),
            ),
            (
                "old_sha256",
                schema_type("string", "sha256 of the original file content."),
            ),
            (
                "new_sha256",
                schema_type("string", "sha256 of the file after all edits."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether file contents changed."),
            ),
            (
                "would_change",
                schema_type("boolean", "Whether the batch would change the file."),
            ),
            (
                "edits",
                schema_type(
                    "array",
                    "Per-edit summary objects (index, kind, line counts).",
                ),
            ),
            (
                "changed_paths",
                schema_type("array", "Paths touched by the edit batch."),
            ),
        ])),
        "insert_at_line" => Some(wrapped_output_schema(vec![
            ("path", schema_type("string", "Project-relative path.")),
            ("line", schema_type("integer", "1-based insertion line.")),
            (
                "old_sha256",
                schema_type("string", "sha256 of the anchor line, or empty EOF anchor."),
            ),
            (
                "new_sha256",
                schema_type("string", "sha256 of the entire file after the edit."),
            ),
            (
                "old_line_count",
                schema_type("integer", "Anchor line count: 1 or 0 at EOF."),
            ),
            (
                "new_line_count",
                schema_type("integer", "Number of inserted lines."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes in the file written after the edit."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether file contents changed."),
            ),
        ])),
        "replace_exact_block" => Some(wrapped_output_schema(vec![
            ("path", schema_type("string", "Project-relative path.")),
            (
                "bytes_before",
                schema_type("integer", "File size in bytes before edit."),
            ),
            (
                "bytes_after",
                schema_type("integer", "File size in bytes after edit."),
            ),
            (
                "matches_replaced",
                schema_type("integer", "Literal matches replaced; always 1 on success."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether file contents changed."),
            ),
        ])),
        "insert_before_pattern" | "insert_after_pattern" => Some(wrapped_output_schema(vec![
            ("path", schema_type("string", "Project-relative path.")),
            (
                "bytes_before",
                schema_type("integer", "File size in bytes before edit."),
            ),
            (
                "bytes_after",
                schema_type("integer", "File size in bytes after edit."),
            ),
            (
                "pattern_matches",
                schema_type("integer", "Literal pattern matches; always 1 on success."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether file contents changed."),
            ),
        ])),
        _ => None,
    }
}
