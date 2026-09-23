use serde_json::Value;

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};
use serde_json::json;

fn apply_patch_edit_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "chunk_index": {"type": "integer", "minimum": 0},
            "change_context_present": {"type": "boolean"},
            "old_line_count": {"type": "integer", "minimum": 0},
            "new_line_count": {"type": "integer", "minimum": 0},
            "end_of_file": {"type": "boolean"},
            "match_mode": {
                "description": "Validated current apply_patch positioning mode; null only for unanchored append.",
                "anyOf": [
                    {"type": "string", "enum": ["exact", "trim_end", "trim", "normalized"]},
                    {"type": "null"}
                ]
            },
            "match_source": {
                "type": "string",
                "enum": ["old_lines", "change_context", "append"],
                "description": "Validated positioning source for this current apply_patch chunk."
            },
            "matched_start_line": {
                "type": "integer",
                "minimum": 1,
                "description": "Validated 1-based matched/insertion line."
            },
            "candidate_count": {
                "description": "Validated candidate count in the selected match tier; null only for unanchored append.",
                "anyOf": [
                    {"type": "integer", "minimum": 1},
                    {"type": "null"}
                ]
            },
            "unique_match": {
                "type": "boolean",
                "description": "Validated fact that the final mutation target was unique at its selected tier; for anchored pure additions this means the change_context itself was unique."
            },
            "strict_match": {
                "type": "boolean",
                "description": "Validated exact-and-unique positioning fact retained as match metadata; matching_mode is the request authority."
            }
        },
        "required": [
            "chunk_index", "change_context_present", "old_line_count", "new_line_count",
            "end_of_file", "match_mode", "match_source", "matched_start_line",
            "candidate_count", "unique_match", "strict_match"
        ]
    })
}

fn apply_patch_match_diagnostic_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Body-free structural diagnostics for a deterministic apply_patch context mismatch. The Server validates this projection against the original parsed patch and the failed file/chunk before exposing it. Contains only positions, counts, and match-source classification; never source or patch line text.",
        "properties": {
            "chunk_index": {"type": "integer", "minimum": 0},
            "match_source": {"type": "string", "enum": ["old_lines", "change_context"]},
            "search_start_line": {"type": "integer", "minimum": 1},
            "expected_line_count": {"type": "integer", "minimum": 1},
            "available_line_count": {"type": "integer", "minimum": 0},
            "closest_start_line": {
                "anyOf": [
                    {"type": "integer", "minimum": 1},
                    {"type": "null"}
                ]
            },
            "closest_exact_line_matches": {"type": "integer", "minimum": 0},
            "closest_trim_end_line_matches": {"type": "integer", "minimum": 0},
            "closest_trim_line_matches": {"type": "integer", "minimum": 0},
            "first_exact_mismatch_offset": {
                "anyOf": [
                    {"type": "integer", "minimum": 1},
                    {"type": "null"}
                ]
            }
        },
        "required": [
            "chunk_index", "match_source", "search_start_line", "expected_line_count",
            "available_line_count", "closest_start_line", "closest_exact_line_matches",
            "closest_trim_end_line_matches", "closest_trim_line_matches",
            "first_exact_mismatch_offset"
        ]
    })
}

fn apply_patch_match_rejection_diagnostic_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Server-validated, body-free classification of a deterministic matching_mode rejection. Candidate positions are equal structural observation targets, never a winner/preference signal; ambiguous matches require matched_start_line=null.",
        "properties": {
            "classification": {"type": "string", "enum": ["unique_fuzzy_candidate", "ambiguous_candidate"]},
            "requested_matching_mode": {"type": "string", "enum": ["unique", "exact_unique"]},
            "chunk_index": {"type": "integer", "minimum": 0},
            "match_mode": {"type": "string", "enum": ["exact", "trim_end", "trim", "normalized"]},
            "match_source": {"type": "string", "enum": ["old_lines", "change_context"]},
            "matched_start_line": {
                "description": "Validated 1-based candidate location only for a unique fuzzy candidate; null for ambiguous candidates so no first match is presented as authoritative.",
                "anyOf": [
                    {"type": "integer", "minimum": 1},
                    {"type": "null"}
                ]
            },
            "candidate_count": {"type": "integer", "minimum": 1},
            "candidate_start_lines": {
                "type": "array",
                "minItems": 1,
                "maxItems": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS,
                "items": {"type": "integer", "minimum": 1},
                "description": "Ascending bounded candidate starts. Ordering is positional only and never preference."
            },
            "candidate_positions_truncated": {"type": "boolean"},
            "expected_line_count": {"type": "integer", "minimum": 1},
            "matching_mode_satisfied": {"type": "boolean", "const": false}
        },
        "required": [
            "classification", "requested_matching_mode", "chunk_index", "match_mode", "match_source",
            "matched_start_line", "candidate_count", "candidate_start_lines",
            "candidate_positions_truncated", "expected_line_count", "matching_mode_satisfied"
        ]
    })
}

fn apply_patch_recovery_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Server-derived, body-free reread hint for a validated deterministic no-write apply_patch rejection. Copy `items` into the direct `read_files` tool for the same project. It is emitted only when the failed target and structural facts prove a bounded reread is safe; the Runner cannot choose the tool, path, or arguments.",
        "properties": {
            "action": {"type": "string", "enum": ["read_files"]},
            "reason": {"type": "string", "enum": ["context_mismatch", "matching_mode_rejected_unique_fuzzy", "matching_mode_rejected_ambiguous"]},
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": {"type": "string", "minLength": 1},
                        "start_line": {"type": "integer", "minimum": 1},
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES
                        }
                    },
                    "required": ["path", "start_line", "limit"]
                }
            },
            "change_index": {
                "type": "integer",
                "minimum": 0,
                "maximum": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_FILE_CHANGES - 1
            },
            "chunk_index": {
                "type": "integer",
                "minimum": 0,
                "maximum": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CHUNKS_PER_FILE - 1
            }
        },
        "required": [
            "action", "reason", "items", "change_index", "chunk_index"
        ]
    })
}

fn apply_patch_file_summary_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_FILE_CHANGES,
        "description": "Validated per-file patch-plan summaries for the current 0.4 apply_patch success contract, including bounded update/rename match metadata. Never file content.",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "index": {"type": "integer", "minimum": 0},
                "kind": {"type": "string", "enum": ["create", "edit", "delete", "rename"]},
                "path": {"type": "string", "minLength": 1},
                "to_path": {"anyOf": [{"type": "string", "minLength": 1}, {"type": "null"}]},
                "old_sha256": {"anyOf": [
                    {"type": "string", "pattern": "^[a-f0-9]{64}$"}, {"type": "null"}
                ]},
                "new_sha256": {"anyOf": [
                    {"type": "string", "pattern": "^[a-f0-9]{64}$"}, {"type": "null"}
                ]},
                "changed": {"type": "boolean"},
                "would_change": {"type": "boolean"},
                "edits": {
                    "type": "array",
                    "maxItems": webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CHUNKS_PER_FILE,
                    "items": apply_patch_edit_summary_schema()
                }
            },
            "required": [
                "index", "kind", "path", "to_path", "old_sha256", "new_sha256",
                "changed", "would_change", "edits"
            ]
        }
    })
}

fn apply_text_edit_summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "index": {"type":"integer","minimum":0,"maximum":19},
            "kind": {"type":"string","enum":["replace_exact","insert_before","insert_after","delete_exact"]},
            "old_start_line": {"type":"integer","minimum":1},
            "old_end_line": {"type":"integer","minimum":1},
            "new_line_count": {"type":"integer","minimum":0},
            "would_change": {"type":"boolean"},
            "match_count": {"type":"integer","minimum":1},
            "expected_match_count": {"type":"integer","minimum":1,"maximum":1024},
            "match_ranges": {"type":"array","maxItems":webcodex_core::apply_edits_shared::MAX_APPLY_TEXT_MATCH_RANGES_PER_EDIT,"items":edit_success_match_range_schema()},
            "match_ranges_truncated": {"type":"boolean"},
            "warning": {
                "type": "string",
                "enum": [webcodex_core::apply_edits_shared::APPLY_TEXT_EDIT_DUPLICATE_ANCHOR_WARNING],
                "description": "Optional non-blocking duplicate-anchor advisory. The Server preserves only this canonical fixed text when it maps to the original insert edit."
            }
        }
    })
}

fn apply_text_edits_file_summary_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": webcodex_core::apply_edits_shared::MAX_APPLY_FILE_CHANGES,
        "description": "Server-validated per-file apply_text_edits success summaries. Final snapshots use read_revision; Runner SHA-256 values are internal and are not model-facing.",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "index": {"type": "integer", "minimum": 0},
                "kind": {"type": "string", "enum": ["create", "edit", "delete", "rename"]},
                "path": {"type": "string", "minLength": 1},
                "to_path": {"anyOf": [{"type": "string", "minLength": 1}, {"type": "null"}]},
                "changed": {"type": "boolean"},
                "would_change": {"type": "boolean"},
                "read_revision": {
                    "description": "Fresh model-facing snapshot revision for the final file after confirmed non-dry-run success; null for delete and dry-run results.",
                    "anyOf": [
                        {"type": "integer", "minimum": 1, "maximum": 9007199254740991_u64},
                        {"type": "null"}
                    ]
                },
                "edits": {
                    "type": "array",
                    "maxItems": webcodex_core::apply_edits_shared::MAX_APPLY_TEXT_EDITS,
                    "items": apply_text_edit_summary_schema(),
                    "description": "Bounded source-free per-edit summaries. The optional duplicate-anchor warning is Server-sanitized to one fixed non-blocking advisory; existing structural metadata remains additive."
                }
            },
            "required": [
                "index", "kind", "path", "to_path", "changed", "would_change",
                "read_revision", "edits"
            ]
        }
    })
}

fn edit_success_match_range_schema() -> Value {
    let mut schema = edit_candidate_range_schema();
    schema["required"] = json!(["occurrence", "start_line", "end_line"]);
    schema
}

fn edit_candidate_range_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "occurrence": {"type": "integer", "minimum": 1},
            "start_line": {"type": "integer", "minimum": 1},
            "end_line": {"type": "integer", "minimum": 1}
        },
        "required": ["start_line", "end_line"]
    })
}

fn conflicting_edit_ranges_schema() -> Value {
    let mut schema = array_schema(
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "edit_index": {"type": "integer", "minimum": 0},
                "start_line": {"type": "integer", "minimum": 1},
                "end_line": {"type": "integer", "minimum": 1}
            },
            "required": ["edit_index", "start_line", "end_line"]
        }),
        "At most the resolved source-line ranges for conflicting edits; derived from the authoritative transactional edit plan and contains no source or replacement text.",
    );
    schema["maxItems"] = json!(2);
    schema
}

fn read_files_recovery_call_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tool": {"type": "string", "const": "read_files"},
            "arguments": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "project": {"type": "string", "minLength": 1},
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 1,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {"path": {"type": "string", "minLength": 1}},
                            "required": ["path"]
                        }
                    }
                },
                "required": ["project", "items"]
            }
        },
        "required": ["tool", "arguments"]
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
                schema_type("boolean", "True when the request successfully targeted an existing file with expected_read_revision resolved to the Runner's exact SHA guard."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes written to the final file; zero for a confirmed no-change rewrite. Result metadata does not include file content, is not a shell-execution interface, and does not expose environment, token, or secret values."),
            ),
            (
                "sha256",
                nullable_schema("string", "Informational sha256 of the final file when available; stale guarded-write conflicts are projected through read revisions instead of exposing Runner SHA recovery truth."),
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
                "recovery",
                read_files_recovery_call_schema(),
            ),
        ])),
        "apply_patch" => Some(wrapped_output_schema(vec![
            ("dry_run", schema_type("boolean", "Whether this was a dry-run with no file writes.")),
            ("requested_matching_mode", json!({"type":"string","enum":["first_match","unique","exact_unique"],"description":"Server-validated positioning mode requested for this apply_patch invocation."})),
            ("applied_count", schema_type("integer", "Number of parsed file operations in the patch.")),
            ("changed", schema_type("boolean", "Whether the worktree was confirmed changed by this request.")),
            ("would_change", schema_type("boolean", "Whether the fully preflighted patch plan would change the worktree.")),
            ("files", apply_patch_file_summary_schema()),
            ("changed_paths", schema_type("array", "Validated project-relative source and destination paths touched by the patch plan.")),
            ("state_changed", nullable_schema("boolean", "True or false for a trustworthy patch effect; null when a dispatched mutation may have completed but its result is unavailable or invalid.")),
            ("execution_state", json!({"type":"string","enum":["not_started","completed","outcome_unknown"],"description":"Transactional patch mutation effect state."})),
            ("error_kind", nullable_schema("string", "Stable parse, preflight, conflict, capability, transaction, or uncertainty classification.")),
            ("failure_kind", nullable_schema("string", "not_started, capability_unavailable, or outcome_unknown for delivery/admission failures.")),
            ("recovery_action", nullable_schema("string", "Bounded next action such as reread_and_regenerate_patch, read_equal_candidates_and_refine_context, read_equal_candidates_and_add_exact_context, or inspect_workspace_before_retry.")),
            ("rollback_complete", nullable_schema("boolean", "Whether a failed transactional apply fully restored all earlier changes.")),
            ("change_index", nullable_schema("integer", "Zero-based failed file-operation index when known.")),
            ("kind", nullable_schema("string", "Failed patch file-operation kind when known.")),
            ("path", nullable_schema("string", "Validated project-relative failed path when safe and known.")),
            ("patch_line", nullable_schema("integer", "One-based patch line for a syntax error when known.")),
            ("expected_format", nullable_schema("string", "codex_patch for parse-format recovery; null otherwise.")),
            ("match_diagnostic", apply_patch_match_diagnostic_schema()),
            ("match_rejection_diagnostic", apply_patch_match_rejection_diagnostic_schema()),
            ("recovery", apply_patch_recovery_schema()),
            ("retry_guidance", schema_type("string", "Bounded recovery guidance for deterministic no-mutation rejection.")),
        ])),
        "apply_text_edits" => Some(wrapped_output_schema(vec![
            (
                "dry_run",
                schema_type("boolean", "Whether this was a dry-run (no write)."),
            ),
            (
                "applied_count",
                schema_type("integer", "Number of confirmed applied file changes; zero for dry_run."),
            ),
            ("planned_count", schema_type("integer", "Number of fully planned file changes, including dry_run.")),
            ("change_summary", json!({"type":"object","additionalProperties":false,"properties":{
                "requested_changes":{"type":"integer","minimum":1,"maximum":16},
                "changed_files":{"type":"integer","minimum":0,"maximum":16},
                "logical_edits":{"type":"integer","minimum":0},
                "resolved_matches":{"type":"integer","minimum":0},
                "warnings":{"type":"integer","minimum":0}
            },"required":["requested_changes","changed_files","logical_edits","resolved_matches","warnings"]})),
            (
                "ignored_noop_count",
                schema_type("integer", "Number of provable empty insert operations ignored without invalidating the transactional batch."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether the worktree was changed."),
            ),
            (
                "would_change",
                schema_type("boolean", "Whether the batch plan changes the worktree."),
            ),
            ("files", apply_text_edits_file_summary_schema()),
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
                "rollback_complete",
                nullable_schema("boolean", "Whether a failed transactional apply fully restored every prior change; false makes the final workspace state uncertain."),
            ),
            (
                "change_index",
                nullable_schema("integer", "Zero-based failed file-change index when known; null or absent for batch-global failures."),
            ),
            (
                "path_conflict_change_indices",
                json!({
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 2,
                    "items": {"type": "integer", "minimum": 0, "maximum": 15},
                    "description": "Server-preflight indices [first occupant, conflicting change] for a repeated source/destination path. May be equal for a self-conflict. Identifies the conflict, not permission to merge sequential edits."
                }),
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
                "match_count",
                schema_type("integer", "Exact-match count reported for a deterministic text conflict when useful."),
            ),
            ("expected_match_count", schema_type("integer", "Caller-required exact count for match_count_mismatch.")),
            ("actual_match_count", schema_type("integer", "Observed exact count in the requested scope for match_count_mismatch.")),
            ("line_scope", json!({"anyOf":[{"type":"object","properties":{"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1}}},{"type":"null"}]})),
            ("direct_retry_safe", schema_type("boolean", "Whether the failed exact edit can be retried without a new read.")),
            ("reread_required", schema_type("boolean", "Whether a fresh read is required before correction.")),
            (
                "candidate_ranges",
                json!({"type":"array","maxItems":webcodex_core::apply_edits_shared::MAX_APPLY_TEXT_CONFLICT_CANDIDATES,"items":edit_candidate_range_schema(),"description":"Bounded candidate source ranges. occurrence is included only when the current read revision makes positional retry safe."}),
            ),
            (
                "candidates_truncated",
                schema_type("boolean", "True when additional exact-match candidates exist beyond candidate_ranges."),
            ),
            (
                "conflicting_edit_indices",
                array_schema(schema_type("integer", "Zero-based edit index participating in an overlap conflict."), "The edit indices whose planned ranges overlap."),
            ),
            (
                "conflicting_edit_ranges",
                conflicting_edit_ranges_schema(),
            ),
            (
                "recovery",
                read_files_recovery_call_schema(),
            ),
        ])),
        _ => None,
    }
}
