use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};
use serde_json::json;

fn edit_conflict_recovery_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema_version": {"type": "integer", "const": 1},
            "conflict_kind": {"type": "string", "enum": [
                "multiple_matches", "match_not_found", "occurrence_out_of_range",
                "overlapping_edits", "sha256_mismatch"
            ]},
            "recovery_action": {"type": "string", "enum": [
                "select_occurrence_or_refine_match", "reread_or_refine_match",
                "choose_valid_occurrence_or_refine_match", "refine_edit_batch", "reread_file"
            ]},
            "occurrence_selector_supported": {"type": "boolean"},
            "match_count": {"type": "integer", "minimum": 0},
            "requested_occurrence": {"type": "integer", "minimum": 1},
            "candidate_ranges": {
                "type": "array", "maxItems": 8,
                "items": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "occurrence": {"type": "integer", "minimum": 1},
                        "start_line": {"type": "integer", "minimum": 1},
                        "end_line": {"type": "integer", "minimum": 1}
                    },
                    "required": ["occurrence", "start_line", "end_line"]
                }
            },
            "candidates_truncated": {"type": "boolean"},
            "conflicting_edit_indices": {
                "type": "array", "maxItems": 2,
                "items": {"type": "integer", "minimum": 0, "maximum": 19}
            }
        },
        "required": ["schema_version", "conflict_kind", "recovery_action", "occurrence_selector_supported"]
    })
}

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
        "write_project_file" => Some(wrapped_output_schema(vec![
            (
                "path",
                nullable_schema("string", "Project-relative path reported by the agent; null only when the agent could not parse the request payload."),
            ),
            (
                "created",
                schema_type("boolean", "True when the whole-file write created a new file."),
            ),
            (
                "overwritten",
                schema_type("boolean", "True when the whole-file write replaced an existing file."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes written to the final file. Result metadata only; does not include file content, is not a shell-execution interface, and does not expose environment, token, or secret values."),
            ),
            (
                "sha256",
                nullable_schema("string", "sha256 of the written file, current file on sha guard mismatch, or null when unavailable."),
            ),
            (
                "warning",
                nullable_schema("string", "Whole-file write safety warning, such as an unguarded overwrite warning; null otherwise."),
            ),
            (
                "error",
                schema_type("string", "Agent-side whole-file write rejection message, when unsuccessful."),
            ),
        ])),
        "apply_text_edits" => Some(wrapped_output_schema(vec![
            (
                "dry_run",
                schema_type("boolean", "Whether this was a dry-run (no write)."),
            ),
            (
                "applied_count",
                schema_type("integer", "Number of file changes applied in the batch."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether the worktree was changed."),
            ),
            (
                "would_change",
                schema_type("boolean", "Whether the batch plan changes the worktree."),
            ),
            (
                "files",
                schema_type(
                    "array",
                    "Per-file summaries with kind, paths, changed state, and old/new sha256 values.",
                ),
            ),
            (
                "changed_paths",
                schema_type("array", "Paths touched by the edit batch."),
            ),
            (
                "conflict_recovery",
                edit_conflict_recovery_schema(),
            ),
        ])),
        _ => None,
    }
}
