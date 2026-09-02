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
                "occurrence_outside_line_scope", "overlapping_edits", "sha256_mismatch"
            ]},
            "recovery_action": {"type": "string", "enum": [
                "select_occurrence_or_refine_match", "reread_or_refine_match",
                "choose_valid_occurrence_or_refine_match", "narrow_line_scope_or_select_occurrence",
                "adjust_line_scope_or_refine_match", "align_occurrence_with_line_scope",
                "refine_edit_batch", "reread_file"
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
            "line_scope": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "start_line": {"type": "integer", "minimum": 1},
                    "end_line": {"type": "integer", "minimum": 1}
                },
                "required": ["start_line", "end_line"]
            },
            "line_scope_match_count": {"type": "integer", "minimum": 0},
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
                schema_type("boolean", "True when the request created a new file."),
            ),
            (
                "overwritten",
                schema_type("boolean", "True when the request successfully targeted an existing file with its exact sha256 guard."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes written to the final file; zero for a confirmed no-change rewrite. Result metadata does not include file content, is not a shell-execution interface, and does not expose environment, token, or secret values."),
            ),
            (
                "sha256",
                nullable_schema("string", "sha256 of the final file, current file on sha guard mismatch, or null when unavailable."),
            ),
            (
                "changed",
                schema_type("boolean", "Runner-authoritative file-content change fact when a trustworthy result was received."),
            ),
            (
                "state_changed",
                nullable_schema("boolean", "True or false for a trustworthy effect result; null when a dispatched write may have completed but its result is unavailable or invalid."),
            ),
            (
                "execution_state",
                json!({"type":"string","enum":["not_started","completed","outcome_unknown"],"description":"Whole-file mutation effect state; never a shell-command lifecycle."}),
            ),
            (
                "error_kind",
                nullable_schema("string", "Stable preflight or outcome_unknown classification when unsuccessful."),
            ),
            (
                "failure_kind",
                nullable_schema("string", "not_started or outcome_unknown for delivery-boundary failures."),
            ),
            (
                "recovery_action",
                nullable_schema("string", "Bounded next action; outcome_unknown requires workspace inspection before another write."),
            ),
            (
                "retry_guidance",
                schema_type("string", "Bounded correction guidance for a deterministic preflight rejection."),
            ),
            (
                "error",
                schema_type("string", "Agent-side whole-file write rejection message, when unsuccessful."),
            ),
        ])),
        "apply_patch" => Some(wrapped_output_schema(vec![
            ("dry_run", schema_type("boolean", "Whether this was a dry-run with no file writes.")),
            ("applied_count", schema_type("integer", "Number of parsed file operations in the patch.")),
            ("changed", schema_type("boolean", "Whether the worktree was confirmed changed by this request.")),
            ("would_change", schema_type("boolean", "Whether the fully preflighted patch plan would change the worktree.")),
            ("files", schema_type("array", "Per-file bounded summaries; update edits include match_mode exact|trim_end|trim|null (widest positioning tier used), match_source, 1-based matched_start_line, candidate_count, and strict_match=true only when every positioning match was exact and unique. Never includes file content.")),
            ("changed_paths", schema_type("array", "Validated project-relative source and destination paths touched by the patch plan.")),
            ("state_changed", nullable_schema("boolean", "True or false for a trustworthy patch effect; null when a dispatched mutation may have completed but its result is unavailable or invalid.")),
            ("execution_state", json!({"type":"string","enum":["not_started","completed","outcome_unknown"],"description":"Transactional patch mutation effect state."})),
            ("error_kind", nullable_schema("string", "Stable parse, preflight, conflict, capability, transaction, or uncertainty classification.")),
            ("failure_kind", nullable_schema("string", "not_started, capability_unavailable, or outcome_unknown for delivery/admission failures.")),
            ("recovery_action", nullable_schema("string", "Bounded next action such as regenerate_patch, reread_or_regenerate_patch, upgrade_or_reconnect_runner, or inspect_workspace_before_retry.")),
            ("rollback_complete", nullable_schema("boolean", "Whether a failed transactional apply fully restored all earlier changes.")),
            ("change_index", nullable_schema("integer", "Zero-based failed file-operation index when known.")),
            ("kind", nullable_schema("string", "Failed patch file-operation kind when known.")),
            ("path", nullable_schema("string", "Validated project-relative failed path when safe and known.")),
            ("patch_line", nullable_schema("integer", "One-based patch line for a syntax error when known.")),
            ("expected_format", nullable_schema("string", "codex_patch for parse-format recovery; null otherwise.")),
            ("retry_guidance", schema_type("string", "Bounded recovery guidance for deterministic no-mutation rejection.")),
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
                nullable_schema("boolean", "True or false for a trustworthy edit effect; null when a dispatched mutation may have completed but its result is unavailable or invalid."),
            ),
            (
                "execution_state",
                json!({"type":"string","enum":["not_started","completed","outcome_unknown"],"description":"Transactional edit mutation effect state; never a shell-command lifecycle."}),
            ),
            (
                "error_kind",
                schema_type("string", "Stable structured rejection/failure kind when unsuccessful."),
            ),
            (
                "failure_kind",
                nullable_schema("string", "not_started or outcome_unknown for delivery-boundary failures."),
            ),
            (
                "recovery_action",
                nullable_schema("string", "Bounded next action; outcome_unknown requires workspace inspection before another write."),
            ),
            (
                "rollback_complete",
                nullable_schema("boolean", "Whether a failed transactional apply fully restored every prior change; false makes the final workspace state uncertain."),
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
