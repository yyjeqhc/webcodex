use serde_json::Value;

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};
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
            "direct_retry_safe": {
                "type": "boolean",
                "description": "True only when a corrected request may be retried against the same observed expected_sha256 without rereading. It never authorizes automatic replay of the rejected payload."
            },
            "reread_required": {
                "type": "boolean",
                "description": "True when the caller must reread the affected file before another write attempt."
            },
            "expected_sha256": {
                "type": "string",
                "pattern": "^[a-f0-9]{64}$",
                "description": "Caller-provided expected sha256 on a sha256 mismatch; hash only, never file content."
            },
            "current_sha256": {
                "type": "string",
                "pattern": "^[a-f0-9]{64}$",
                "description": "Current observed file sha256 on a sha256 mismatch; hash only, never file content."
            },
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
        "apply_unified_diff" => Some(wrapped_output_schema(vec![
            ("applied", nullable_schema("boolean", "True only when git apply completed successfully; null when the post-dispatch mutation outcome is unknown.")),
            ("can_apply", nullable_schema("boolean", "Result of the internal applicability preflight; null when applicability was not established.")),
            ("policy_blocked", schema_type("boolean", "True when sensitive-path policy blocked mutation before the applicability check.")),
            ("state_changed", nullable_schema("boolean", "True on confirmed apply success, false when mutation definitely did not start, null when post-dispatch worktree state is uncertain.")),
            ("execution_state", json!({"type":"string","enum":["not_started","completed","outcome_unknown"],"description":"Mutation effect state, not the internal read-only preflight command state."})),
            ("affected_files", array_schema(schema_type("string", "Validated project-relative path declared by the unified diff."), "Bounded affected paths parsed before dispatch.")),
            ("affected_files_truncated", schema_type("boolean", "True when affected_files exceeded the bounded projection.")),
            ("warnings", array_schema(schema_type("string", "Bounded sensitive-path policy warning."), "Bounded policy warnings.")),
            ("warnings_truncated", schema_type("boolean", "True when warnings exceeded the bounded projection.")),
            ("stderr", nullable_schema("string", "Bounded stderr tail from the decisive git apply command, when available.")),
            ("stderr_truncated", schema_type("boolean", "True when stderr was truncated to the model-facing bound.")),
            ("error_kind", nullable_schema("string", "Bounded domain or uncertainty classification; null on confirmed success.")),
            ("expected_format", nullable_schema("string", "unified_diff for malformed/unsupported input recovery; null otherwise.")),
            ("recovery_action", nullable_schema("string", "Bounded next action such as regenerate_unified_diff, retry_same, or inspect_workspace_before_retry; null on success.")),
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
                "state_changed",
                schema_type("boolean", "Authoritative no-effect fact for deterministic rejected batches when false."),
            ),
            (
                "error_kind",
                schema_type("string", "Stable structured rejection/failure kind when unsuccessful."),
            ),
            (
                "change_index",
                nullable_schema("integer", "Zero-based failed file-change index when known; null or absent for batch-global failures."),
            ),
            (
                "edit_index",
                nullable_schema("integer", "Zero-based failed text-edit index when known; null or absent when not edit-specific."),
            ),
            (
                "kind",
                nullable_schema("string", "Failed change or text-edit kind when known."),
            ),
            (
                "path",
                nullable_schema("string", "Project-relative failed path when known."),
            ),
            (
                "retry_guidance",
                schema_type("string", "Bounded recovery guidance for a deterministic no-mutation rejection."),
            ),
            (
                "conflict_recovery",
                edit_conflict_recovery_schema(),
            ),
        ])),
        _ => None,
    }
}
