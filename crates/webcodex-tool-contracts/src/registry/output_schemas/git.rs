use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, suggested_tool_call_schema,
    wrapped_output_schema,
};
use webcodex_core::runtime_contract::GeneratedFollowUpKind;

fn git_log_suggested_call_schema() -> Value {
    suggested_tool_call_schema(
        GeneratedFollowUpKind::MechanicallyFollowable,
        "read_git_log",
        json!({
            "type": "object",
            "description": "Parser-ready read_git_log arguments for the next page of the same exact history snapshot.",
            "additionalProperties": false,
            "properties": {
                "project": {"type": "string", "minLength": 1},
                "head_commit": {
                    "type": "string",
                    "minLength": 40,
                    "maxLength": 40,
                    "pattern": "^[0-9a-f]{40}$"
                },
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "skip": {"type": "integer", "minimum": 0, "maximum": 10000},
                "session_id": {"type": "string", "pattern": "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"}
            },
            "required": ["project", "head_commit", "limit", "skip"]
        }),
        "Parser-ready advisory read_git_log call for the next page of the same exact commit snapshot. It grants no Project or Session authority.",
    )
}

fn git_diff_hunks_recovery_arguments_schema() -> Value {
    json!({
        "type": "object",
        "description": "Parser-ready read_git_diff_hunks arguments for one bounded recovery step.",
        "additionalProperties": false,
        "properties": {
            "project": {"type": "string"},
            "paths": {"type": "array", "items": {"type": "string"}},
            "max_hunks": {"type": "integer"},
            "max_hunk_lines": {"type": "integer"},
            "max_page_bytes": {
                "type": "integer",
                "minimum": webcodex_core::runtime_contract::MIN_GIT_DIFF_HUNKS_PAGE_BYTES,
                "maximum": webcodex_core::runtime_contract::MAX_GIT_DIFF_HUNKS_PAGE_BYTES
            },
            "cached": {"type": "boolean"},
            "base_commit": {"type": "string"},
            "head_commit": {"type": "string"},
            "continuation": {"type": "string"}
        },
        "required": ["project", "paths", "max_hunks", "max_hunk_lines", "max_page_bytes"]
    })
}

fn git_diff_hunks_recovery_call_schema() -> Value {
    suggested_tool_call_schema(
        GeneratedFollowUpKind::MechanicallyFollowable,
        "read_git_diff_hunks",
        git_diff_hunks_recovery_arguments_schema(),
        "Parser-ready advisory read_git_diff_hunks call for later-record continuation, proven bounded parameter refinement, or exact current-hunk fragment continuation. It grants no authority and is not the continuation identity itself.",
    )
}

fn git_diff_hunks_recovery_schema() -> Value {
    let mut page_call = git_diff_hunks_recovery_call_schema();
    page_call["properties"]["arguments"]["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("continuation"));
    let mut refine_call = git_diff_hunks_recovery_call_schema();
    refine_call["properties"]["arguments"]["properties"]["continuation"] = json!({"not": {}});
    json!({
        "type": "object",
        "description": "Independent recovery lanes: current_hunk explains omitted lines and offers a proven refinement or exact fragment call when available; later_hunks offers continuation to later logical records only. Absent lanes need no recovery. Fixed ceilings never receive a fake call.",
        "additionalProperties": false,
        "allOf": [{"anyOf": [{"required": ["current_hunk"]}, {"required": ["later_hunks"]}]}],
        "properties": {
            "current_hunk": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "reason_code": {
                        "type": "string",
                        "enum": [
                            "larger_max_hunk_lines_available",
                            "hunk_fragment_continuation_available",
                            "page_byte_budget_prevents_proven_recovery",
                            "max_hunk_lines_ceiling_reached",
                            "max_hunk_lines_ceiling_insufficient",
                            "bounded_recovery_unavailable"
                        ],
                        "description": "Whether omitted current-hunk content needs a fresh bounded refinement, exact fragment continuation, or cannot be safely recovered within the bounds."
                    },
                    "next_call": git_diff_hunks_recovery_call_schema()
                },
                "required": ["reason_code"],
                "allOf": [
                    {
                        "if": {"properties": {"reason_code": {"const": "larger_max_hunk_lines_available"}}},
                        "then": {"properties": {"next_call": refine_call}, "required": ["next_call"]}
                    },
                    {
                        "if": {"properties": {"reason_code": {"const": "hunk_fragment_continuation_available"}}},
                        "then": {"properties": {"next_call": page_call}, "required": ["next_call"]}
                    },
                    {
                        "if": {"properties": {"reason_code": {"enum": [
                            "page_byte_budget_prevents_proven_recovery",
                            "max_hunk_lines_ceiling_reached",
                            "max_hunk_lines_ceiling_insufficient",
                            "bounded_recovery_unavailable"
                        ]}}},
                        "then": {"not": {"required": ["next_call"]}}
                    }
                ]
            },
            "later_hunks": {
                "type": "object",
                "description": "Later logical records only; this does not recover omitted current-hunk lines.",
                "additionalProperties": false,
                "properties": {"next_call": page_call},
                "required": ["next_call"]
            }
        }
    })
}

fn show_changes_handoff_arguments_schema() -> Value {
    json!({
        "type": "object",
        "description": "Ready-to-call worktree read_git_diff_hunks arguments. paths stays empty when read_workspace_changes cannot prove a narrower omitted-line path.",
        "additionalProperties": false,
        "properties": {
            "project": {"type": "string"},
            "cached": {"type": "boolean", "const": false},
            "paths": {"type": "array", "items": {"type": "string"}},
            "max_hunks": {"type": "integer"},
            "max_hunk_lines": {"type": "integer"},
            "max_page_bytes": {
                "type": "integer",
                "const": webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
            }
        },
        "required": ["project", "cached", "paths", "max_hunks", "max_hunk_lines", "max_page_bytes"]
    })
}

fn show_changes_diff_hunk_schema() -> Value {
    json!({
        "type": "object",
        "description": "One bounded diff hunk. source_completeness is the single authoritative statement about whether the returned hunk is proven source-complete.",
        "additionalProperties": true,
        "properties": {
            "source_completeness": {
                "type": "string",
                "enum": ["complete", "unknown"],
                "description": "complete means producer metadata and parsing prove this returned hunk source-complete; unknown means the system cannot prove completeness."
            }
        },
        "required": ["source_completeness"]
    })
}

fn show_changes_diff_file_schema() -> Value {
    json!({
        "type": "object",
        "description": "One changed file with bounded diff hunks.",
        "additionalProperties": true,
        "properties": {
            "hunks": {
                "type": "array",
                "items": show_changes_diff_hunk_schema()
            }
        }
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "commit_git_paths" => Some(wrapped_output_schema(vec![
            ("committed", nullable_schema("boolean", "True only when the exact-path commit is known to have completed; null when dispatch outcome is unknown.")),
            ("expected_head", schema_type("string", "Exact caller-supplied HEAD fence.")),
            ("previous_head", nullable_schema("string", "Known parent HEAD used by the successful commit, or null before it is proven.")),
            ("actual_head", nullable_schema("string", "Observed HEAD when a deterministic precondition failure can report it.")),
            ("new_head", nullable_schema("string", "New exact commit SHA when known.")),
            (
                "committed_paths",
                array_schema(
                    schema_type("string", "Exact project-relative path committed."),
                    "Exact requested paths committed on known success; empty otherwise.",
                ),
            ),
            ("state_changed", nullable_schema("boolean", "Whether repository HEAD is known to have changed.")),
            ("outcome_unknown", schema_type("boolean", "True only when Runner dispatch may have executed but no trustworthy terminal result was received.")),
            ("failure_kind", nullable_schema("string", "Stable bounded commit rejection/failure kind.")),
            ("hook_policy", schema_type("string", "Always bypassed_exact_tree: commit-tree is used so hooks cannot add unrelated paths.")),
        ])),
        "get_git_status" => Some(wrapped_output_schema(vec![
            (
                "exit_code",
                nullable_schema("integer", "Git command exit code."),
            ),
            ("stdout", schema_type("string", "Git command stdout.")),
            ("stderr", schema_type("string", "Git command stderr.")),
        ])),
        "review_changes" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project input.")),
            ("snapshot", nullable_schema("object", "Exact retained Git review snapshot identity, scope/source fence, completeness, and reuse state.")),
            ("summary", nullable_schema("object", "Initial-page bounded review summary; continuation pages may omit it.")),
            ("files", array_schema(open_object_schema("Bounded changed-file metadata."), "Initial-page changed files; continuation pages may return an empty array.")),
            ("signals", array_schema(open_object_schema("Bounded deterministic or Session review signal."), "Initial-page review signals; continuation pages may return an empty array.")),
            ("diff", nullable_schema("object", "Bounded review patch. has_more includes fragments; only outer next_call continues this snapshot. Workspace basis is head_to_frozen_workspace.")),
            ("continuation", nullable_schema("string", "Opaque same-snapshot review continuation, or null when no bounded page continuation is available.")),
            (
                "next_call",
                {
                    let call = suggested_tool_call_schema(
                        GeneratedFollowUpKind::MechanicallyFollowable,
                        "review_changes",
                        crate::input_schema_for_tool("review_changes"),
                        "Parser-ready same-snapshot review_changes continuation. The Server has already fixed the scope, projection, and continuation identity, so the Host may follow it mechanically while ordinary authorization and stale-snapshot checks still apply.",
                    );
                    json!({"anyOf": [call, {"type": "null"}]})
                },
            ),
            ("reason_code", nullable_schema("string", "Stable failure/stale reason when review cannot proceed.")),
        ])),
        "read_git_review_summary" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project input.")),
            (
                "scope",
                open_object_schema("Exact requested commits, merge-base, ancestry, commit count, and effective diff range."),
            ),
            (
                "stats",
                open_object_schema("Exact aggregate committed-range file and line statistics."),
            ),
            (
                "file_classes",
                open_object_schema("Deterministic observed file-class counts plus partial metadata."),
            ),
            (
                "subsystems",
                array_schema(open_object_schema("Bounded deterministic subsystem bucket."), "Touched subsystem buckets."),
            ),
            (
                "signals",
                array_schema(open_object_schema("Bounded reviewer-attention signal; never a correctness claim."), "Deterministic review signals."),
            ),
            (
                "files",
                array_schema(open_object_schema("Bounded changed-file review metadata and symbol hints."), "Changed files returned within fixed bounds."),
            ),
            (
                "coverage",
                open_object_schema("Production/test/docs change observation; false becomes null when classification is partial."),
            ),
            ("bounds", open_object_schema("Fixed producer and model-result bounds used by this invocation.")),
            (
                "truncation",
                open_object_schema("Explicit file, symbol, subsystem, and signal partiality metadata."),
            ),
            ("deterministic", schema_type("boolean", "Always true for this built-in deterministic classifier.")),
            ("llm_summary", schema_type("boolean", "Always false; no LLM is used for this review map.")),
            ("truncated", schema_type("boolean", "Whether any review-map observation is partial or bounded.")),
            (
                "warnings",
                array_schema(schema_type("string", "Stable bounded review warning."), "Bounded non-fatal warnings."),
            ),
            ("reason_code", nullable_schema("string", "Stable structured failure reason when review observation cannot proceed.")),
        ])),
        "read_git_diff_hunks" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project input.")),
            (
                "paths",
                array_schema(
                    schema_type("string", "Normalized project-relative diff path."),
                    "Normalized diff scope paths.",
                ),
            ),
            ("cached", schema_type("boolean", "Whether the staged diff is inspected.")),
            (
                "max_page_bytes",
                schema_type("integer", "Effective raw producer-page byte budget. This is not the final serialized model-facing result budget."),
            ),
            (
                "scope",
                json!({
                    "type": "object",
                    "description": "Committed-range identity when base_commit/head_commit mode is used; omitted for worktree/cached mode.",
                    "additionalProperties": false,
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["committed"]
                        },
                        "requested_base": nullable_schema("string", "Normalized exact requested base commit id, or null when input is invalid."),
                        "requested_head": nullable_schema("string", "Normalized exact requested head commit id, or null when input is invalid."),
                        "merge_base": nullable_schema("string", "Single exact best merge-base used for the review diff, or null before range resolution succeeds."),
                        "base_is_ancestor": nullable_schema("boolean", "Whether requested_base is an ancestor of requested_head, or null before range resolution succeeds."),
                        "diff_range": nullable_schema("string", "Effective merge_base..requested_head committed diff range, or null before range resolution succeeds.")
                    },
                    "required": [
                        "mode",
                        "requested_base",
                        "requested_head",
                        "merge_base",
                        "base_is_ancestor",
                        "diff_range"
                    ]
                }),
            ),
            (
                "files",
                array_schema(open_object_schema("File diff hunks."), "Changed files."),
            ),
            ("hunk_count", schema_type("integer", "Returned hunk count.")),
            (
                "truncated",
                schema_type("boolean", "Whether any page or per-hunk preview bound fired."),
            ),
            (
                "truncation_reasons",
                array_schema(
                    schema_type("string", "Stable git diff hunk truncation reason."),
                    "Stable reasons for page/hunk truncation.",
                ),
            ),
            (
                "has_more",
                schema_type("boolean", "Whether another logical diff page exists."),
            ),
            ("recovery", git_diff_hunks_recovery_schema()),
            (
                "exit_code",
                nullable_schema("integer", "Git diff exit code."),
            ),
            ("stderr", schema_type("string", "Bounded Git diff stderr.")),
        ])),
        "read_git_log" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project id.")),
            (
                "head_commit",
                json!({
                    "anyOf": [
                        {
                            "type": "string",
                            "minLength": 40,
                            "maxLength": 40,
                            "pattern": "^[0-9a-f]{40}$"
                        },
                        {"type": "null"}
                    ],
                    "description": "Exact canonical 40-hex commit snapshot used for this page; null only for an unborn repository."
                }),
            ),
            ("limit", schema_type("integer", "Effective commit limit.")),
            ("skip", schema_type("integer", "Effective commit offset.")),
            ("count", schema_type("integer", "Returned commit count.")),
            (
                "truncated",
                schema_type("boolean", "Whether more commits were available."),
            ),
            (
                "next_skip",
                nullable_schema(
                    "integer",
                    "Domain metadata for the next commit offset when safe forward progress exists inside the 10000 skip bound. Models should continue through suggested_call, which also carries the exact head_commit snapshot fence; null on the final page or at the skip ceiling.",
                ),
            ),
            (
                "commits",
                array_schema(open_object_schema("Git commit summary."), "Recent commits."),
            ),
            ("suggested_call", git_log_suggested_call_schema()),
        ])),
        "read_workspace_changes" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project id.")),
            (
                "git_available",
                schema_type(
                    "boolean",
                    "Whether git-backed inspection was available. False for non-git projects.",
                ),
            ),
            (
                "non_git_project",
                schema_type(
                    "boolean",
                    "True when the project directory is not inside a git repository.",
                ),
            ),
            (
                "git_error",
                nullable_schema(
                    "string",
                    "Short summary when git-backed inspection is unavailable; null otherwise.",
                ),
            ),
            (
                "branch",
                nullable_schema(
                    "string",
                    "Current git branch from porcelain status; null for detached or unavailable Git state.",
                ),
            ),
            (
                "upstream_status",
                json!({
                    "type": "string",
                    "enum": ["available", "absent", "gone", "unobserved"],
                    "description": "Whether a tracking branch is available, absent, gone, or unobserved."
                }),
            ),
            (
                "upstream_reason_code",
                nullable_schema(
                    "string",
                    "Stable reason code for gone or unobserved upstream state.",
                ),
            ),
            (
                "upstream",
                nullable_schema("string", "Configured upstream tracking branch when observed."),
            ),
            (
                "ahead",
                nullable_schema("integer", "Commits ahead of an available upstream."),
            ),
            (
                "behind",
                nullable_schema("integer", "Commits behind an available upstream."),
            ),
            (
                "head",
                json!({
                    "type": "object",
                    "description": "Current HEAD commit metadata.",
                    "properties": {
                        "commit": nullable_schema("string", "Full HEAD commit hash when observed."),
                        "short": nullable_schema("string", "Short HEAD commit hash when observed."),
                        "summary": nullable_schema("string", "HEAD commit subject when observed.")
                    },
                    "required": ["commit", "short", "summary"],
                    "additionalProperties": false
                }),
            ),
            (
                "status_observation",
                json!({
                    "type": "object",
                    "description": "Independent git status execution and repository-probe result.",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["observed", "non_git", "command_failed", "output_unavailable"]
                        },
                        "reason_code": nullable_schema("string", "Stable reason code when status was not observed."),
                        "exit_code": nullable_schema("integer", "Exit code from git status itself."),
                        "repository_probe": {
                            "type": "string",
                            "enum": ["inside_worktree", "outside_worktree", "unavailable"]
                        },
                        "repository_probe_exit_code": nullable_schema("integer", "Exit code from the explicit repository probe.")
                    },
                    "required": [
                        "status", "reason_code", "exit_code", "repository_probe",
                        "repository_probe_exit_code"
                    ],
                    "additionalProperties": false
                }),
            ),
            (
                "clean",
                nullable_schema(
                    "boolean",
                    "Whether the worktree is clean when Git was observed; null otherwise.",
                ),
            ),
            (
                "counts",
                json!({
                    "type": "object",
                    "description": "Parsed status counts. conflicted is null when Git status was not observed.",
                    "properties": {
                        "modified": schema_type("integer", "Modified file count."),
                        "added": schema_type("integer", "Added file count."),
                        "deleted": schema_type("integer", "Deleted file count."),
                        "renamed": schema_type("integer", "Renamed file count."),
                        "copied": schema_type("integer", "Copied file count."),
                        "untracked": schema_type("integer", "Untracked file count."),
                        "conflicted": nullable_schema("integer", "Conflict count when observed; null otherwise."),
                        "staged": schema_type("integer", "Staged file count."),
                        "unstaged": schema_type("integer", "Unstaged file count.")
                    },
                    "required": [
                        "modified", "added", "deleted", "renamed", "copied",
                        "untracked", "conflicted", "staged", "unstaged"
                    ],
                    "additionalProperties": false
                }),
            ),
            (
                "files",
                array_schema(open_object_schema("Changed file status."), "Changed files."),
            ),
            (
                "files_total",
                nullable_schema(
                    "integer",
                    "Exact count of all status entries, even when the returned file records were bounded by the production-side limit. Null when status was not observed.",
                ),
            ),
            (
                "files_returned",
                schema_type(
                    "integer",
                    "Number of changed-file records actually returned (files.len()).",
                ),
            ),
            (
                "files_truncated",
                schema_type(
                    "boolean",
                    "Whether the returned file records were bounded by the production-side limit.",
                ),
            ),
            (
                "files_limit",
                schema_type(
                    "integer",
                    "Production-side cap on returned changed-file records.",
                ),
            ),
            (
                "transport_safe",
                schema_type(
                    "boolean",
                    "Whether the production-side output stayed within the transport budget so no tail-retention truncation occurred.",
                ),
            ),
            (
                "output_budget_bytes",
                schema_type(
                    "integer",
                    "Production-side stdout budget in bytes the command is constructed to stay under.",
                ),
            ),
            (
                "output_truncated",
                schema_type(
                    "boolean",
                    "Whether the final output was truncated by the output budget (reported via this structured field, never inferred from a truncation marker string).",
                ),
            ),
            (
                "truncation_reasons",
                array_schema(
                    schema_type("string", "Stable reason code for an output truncation."),
                    "Reasons the production-side output was bounded/truncated.",
                ),
            ),
            (
                "diff_stat",
                schema_type("string", "Git diff --stat output."),
            ),
            (
                "diff_exit",
                nullable_schema(
                    "integer",
                    "Real full `git diff` exit code captured by the production-side loop; null when unavailable.",
                ),
            ),
            (
                "diff_status",
                json!({
                    "type": "object",
                    "description": "Structured full `git diff` observation: observed (exit captured), command_failed (non-zero exit), or output_unavailable.",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["observed", "command_failed", "output_unavailable"]
                        },
                        "exit_code": nullable_schema("integer", "Full git diff exit code when observed.")
                    },
                    "required": ["status", "exit_code"],
                    "additionalProperties": false
                }),
            ),
            (
                "diff_stat_exit",
                nullable_schema(
                    "integer",
                    "Real `git diff --stat` exit code; null when unavailable.",
                ),
            ),
            (
                "diff_stat_status",
                json!({
                    "type": "object",
                    "description": "Structured `git diff --stat` observation. This inspection status is independent from transport_safe and participates in tool success for confirmed Git worktrees.",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["observed", "command_failed", "output_unavailable"]
                        },
                        "exit_code": nullable_schema(
                            "integer",
                            "Real git diff --stat exit code when observed."
                        ),
                        "reason_code": nullable_schema(
                            "string",
                            "Stable reason code when diff-stat did not succeed."
                        )
                    },
                    "required": ["status", "exit_code", "reason_code"],
                    "additionalProperties": false
                }),
            ),
            (
                "head_exit",
                nullable_schema(
                    "integer",
                    "Real `git log -1` HEAD metadata exit code; null when unavailable.",
                ),
            ),
            (
                "hunks",
                array_schema(
                    show_changes_diff_file_schema(),
                    "Diff hunks. source_completeness is authoritative for each returned hunk; top-level truncation metadata describes omitted records/content outside that per-hunk proof.",
                ),
            ),
            (
                "hunk_count",
                schema_type("integer", "Returned bounded diff hunk count."),
            ),
            (
                "hunks_truncated",
                schema_type("boolean", "Whether diff hunks were truncated by limits."),
            ),
            (
                "diff_review_handoff",
                json!({
                    "type": "object",
                    "description": "Present only when bounded read_workspace_changes diff hunks are incomplete. next_call is the sole model-facing handoff action; top-level truncation_reasons retain the diagnostic cause.",
                    "additionalProperties": false,
                    "properties": {
                        "next_call": suggested_tool_call_schema(
                            GeneratedFollowUpKind::MechanicallyFollowable,
                            "read_git_diff_hunks",
                            show_changes_handoff_arguments_schema(),
                            "Parser-ready first focused diff-review call. It starts a fresh read_git_diff_hunks observation and carries no invented continuation identity.",
                        )
                    },
                    "required": ["next_call"]
                }),
            ),
            (
                "untracked_previews",
                array_schema(
                    open_object_schema("Bounded untracked file preview or skip reason."),
                    "Untracked file previews.",
                ),
            ),
            (
                "untracked_previews_truncated",
                schema_type(
                    "boolean",
                    "Whether the untracked preview file list was bounded/truncated.",
                ),
            ),
            (
                "warnings",
                array_schema(open_object_schema("Review warning."), "Warnings."),
            ),
            (
                "suggested_next_actions",
                array_schema(
                    schema_type("string", "Suggested action."),
                    "Suggested actions.",
                ),
            ),
            (
                "verdict",
                open_object_schema("Operator-friendly review verdict: status pass/warn/fail, blocking, blocking_reasons, warning_reasons, and suggested_next_actions. Additive UX summary only; does not change safety semantics."),
            ),
            (
                "session",
                nullable_schema("object", "Optional session activity summary."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Git inspection command exit code."),
            ),
            ("stderr", schema_type("string", "Bounded Git inspection stderr.")),
        ])),
        _ => None,
    }
}

pub(super) fn show_changes_output_value_schema() -> Value {
    output_schema_for_tool("read_workspace_changes")
        .expect("read_workspace_changes output schema")
        .pointer("/properties/output")
        .cloned()
        .expect("read_workspace_changes wrapped output value schema")
}
