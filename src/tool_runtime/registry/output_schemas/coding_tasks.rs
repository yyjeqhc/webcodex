use serde_json::{json, Value};

use super::common::{
    array_schema, evidence_history_schema, evidence_integrity_schema, job_lifecycle_summary_schema,
    nullable_schema, open_object_schema, permission_profile_schema, permission_summary_schema,
    schema_type, task_outcome_schema, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "start_coding_task" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Original project input.")),
            (
                "resolved_project",
                open_object_schema("Resolved project id, path, executor, and safe project metadata."),
            ),
            (
                "session",
                open_object_schema("Created session id, mode, guards, explicit-session guidance, and current binding state."),
            ),
            (
                "runtime_status",
                nullable_schema("object", "Full runtime_status output, or compact startup runtime observability when compact_startup=true; null when not requested."),
            ),
            (
                "permissions",
                permission_profile_schema("Current permission/approval profile for this task."),
            ),
            (
                "rules",
                nullable_schema("object", "Deterministic project instruction source summary when requested; null otherwise."),
            ),
            (
                "git",
                nullable_schema("object", "Structured worktree/git summary when requested; null otherwise."),
            ),
            (
                "semantic_navigation",
                semantic_navigation_schema(),
            ),
            (
                "tool_manifest",
                open_object_schema("Compact tool_manifest output when requested; absent otherwise. Never includes full input/output schemas."),
            ),
            (
                "recommended_flow",
                open_object_schema("Deterministic recommended inspect/edit/validate/review/handoff tool groups."),
            ),
            (
                "startup_verdict",
                open_object_schema("Operator-friendly startup sanity verdict: status pass/warn/fail, blocking boolean, compact checks, and bounded suggested_next_actions. Additive UX summary only; does not change safety semantics."),
            ),
            (
                "warnings",
                array_schema(open_object_schema("Startup warning."), "Bounded startup warnings."),
            ),
        ])),
        "finish_coding_task" => Some(wrapped_output_schema(vec![
            (
                "summary_only",
                schema_type("boolean", "True only for compact summary_only output."),
            ),
            ("project", schema_type("string", "Original project input.")),
            (
                "resolved_project",
                open_object_schema("Resolved project id, path, executor, and safe project metadata."),
            ),
            ("session_id", schema_type("string", "Explicit task session id.")),
            (
                "workspace_clean",
                schema_type("boolean", "Compact summary_only workspace cleanliness verdict."),
            ),
            (
                "hygiene_clean",
                schema_type("boolean", "Compact summary_only hygiene cleanliness verdict."),
            ),
            (
                "workspace",
                open_object_schema("Workspace cleanliness, changed file count, and warnings."),
            ),
            (
                "changes",
                open_object_schema("show_changes output and hunk truncation metadata."),
            ),
            (
                "validation",
                open_object_schema("Ledger-based validation-like tool-call summary with status/reason: not_run, passed, failed, mixed, or unknown. Parser version 2 provides bounded structured diagnostics from bounded validation metadata while retaining backward-compatible first_diagnostic, failed_tests, and first_failed_test fields. Full and summary_only closeout preserve the same validation evidence. Does not include stdout/stderr bodies and performs no root-cause inference. latest_status and historical_failures retain the existing final-state and resolved-history semantics."),
            ),
            (
                "review_evidence",
                review_evidence_schema("Ledger-derived non-cargo review evidence summary for full and summary_only outputs. Counts successful read/search/diff/workspace/hygiene inspection tools from the session ledger and exposes bounded tools for compact explainability. For docs-only or read-only audit tasks, validation.status may remain not_run while review_evidence.total is greater than zero. Does not include file contents, stdout/stderr, diff hunks, command text, tokens, secrets, or raw input payloads. Does not change validation.status or make the verdict pass."),
            ),
            (
                "permissions",
                permission_summary_schema("Deterministic bounded permission decision summary from the session ledger. Counts high-risk auto-approved tools only; never includes stdout/stderr, env, tokens, secrets, or raw input content."),
            ),
            (
                "tool_failures",
                open_object_schema("Expected/unexpected tool failure classification from the session ledger. Counts expected failures, unexpected failures, expectation mismatches, and expected-failure calls that unexpectedly succeeded. Compact output includes counts only."),
            ),
            (
                "hygiene",
                nullable_schema("object", "workspace_hygiene_check output when requested; null otherwise."),
            ),
            (
                "handoff",
                nullable_schema("object", "session_handoff_summary output when requested; null otherwise."),
            ),
            (
                "jobs",
                job_lifecycle_summary_schema("Bounded job lifecycle summary for finish. active_jobs_present is emitted only for blocking_active_count > 0; stop_requested-only jobs use nonblocking jobs_terminal_pending. Never includes stdout/stderr or command text."),
            ),
            (
                "final_warnings",
                array_schema(open_object_schema("Finish warning."), "Bounded finish warnings."),
            ),
            (
                "warnings",
                array_schema(open_object_schema("Compact finish warning."), "Bounded compact summary_only warnings."),
            ),
            (
                "verdict",
                open_object_schema("Legacy aggregate closeout verdict for full and summary_only output: task_outcome fail or evidence_integrity error maps to blocking fail; otherwise task_outcome warn or evidence_integrity warning maps to non-blocking warn; otherwise pass. Resolved evidence history alone does not lower the verdict."),
            ),
            (
                "finish_verdict",
                open_object_schema("Alias of the legacy verdict for full and summary_only finish_coding_task output. Callers should report this final closeout verdict instead of nested show_changes.verdict or workspace_hygiene_check.verdict."),
            ),
            (
                "task_outcome",
                task_outcome_schema("Final task completion outcome with status pass/warn/fail, blocking, and task-only reasons. Resolved validation history and expected-failure audit metadata do not lower this status."),
            ),
            (
                "evidence_history",
                evidence_history_schema("Validation evidence history status: clean, mixed_resolved, mixed_unresolved, or failed. Does not replace validation.status or validation.latest_status."),
            ),
            (
                "evidence_integrity",
                evidence_integrity_schema("Expected-failure and validation-evidence integrity status: clean, warning, or error, with bounded reason identifiers."),
            ),
            (
                "informational_notes",
                array_schema(
                    schema_type("string", "Completed-state informational note."),
                    "Bounded completed-state facts, separate from executable suggested_next_actions.",
                ),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Top-level summary_only final closeout actions. May be non-empty when final verdict suggested_next_actions is non-empty, and preserves bounded finish actions."),
            ),
        ])),
        _ => None,
    }
}

fn semantic_navigation_schema() -> Value {
    json!({
        "type": "object",
        "description": "Always-present bounded Rust semantic-navigation capability summary. Derived only from a typed agent status probe; never contains transport envelopes, process output, paths, environment variables, or symbol/location data.",
        "additionalProperties": false,
        "properties": {
            "supported": schema_type("boolean", "True when the project is agent-backed, the owning agent is connected, and it advertises lsp_read_only_navigation."),
            "available": schema_type("boolean", "True when supported Rust navigation has an available executable or an existing running/initializing server slot. A crashed slot stays available only while the agent still reports the executable as available."),
            "recommended": schema_type("boolean", "True only for available or running status."),
            "status": {
                "type": "string",
                "enum": [
                    "running",
                    "available",
                    "initializing",
                    "crashed",
                    "unavailable",
                    "not_applicable",
                    "agent_unavailable",
                    "agent_capability_unavailable",
                    "probe_timeout",
                    "probe_failed"
                ]
            },
            "language": {
                "anyOf": [
                    { "type": "string", "enum": ["rust"] },
                    { "type": "null" }
                ]
            },
            "server": {
                "anyOf": [
                    { "type": "string", "enum": ["rust-analyzer"] },
                    { "type": "null" }
                ]
            },
            "position_encoding": {
                "anyOf": [
                    { "type": "string", "enum": ["utf-8", "utf-16", "utf-32"] },
                    { "type": "null" }
                ]
            },
            "tools": {
                "type": "array",
                "maxItems": 7,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["lsp_status", "document_symbols", "goto_definition", "find_references", "document_diagnostics", "hover", "workspace_symbols"]
                }
            },
            "preferred_flow": {
                "type": "array",
                "maxItems": 6,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["document_symbols", "goto_definition", "find_references", "hover", "read_file", "search_project_text"]
                }
            },
            "limitations": {
                "type": "array",
                "maxItems": 5,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["rust_only", "read_only", "workspace_only", "no_dependency_navigation", "full_text_sync_only"]
                }
            },
            "reason_code": {
                "anyOf": [
                    {
                        "type": "string",
                        "enum": [
                            "project_not_agent_backed",
                            "rust_not_detected",
                            "agent_not_connected",
                            "lsp_capability_not_advertised",
                            "server_crashed",
                            "server_unavailable",
                            "status_probe_timed_out",
                            "status_probe_failed",
                            "malformed_agent_result"
                        ]
                    },
                    { "type": "null" }
                ]
            }
        },
        "required": [
            "supported",
            "available",
            "recommended",
            "status",
            "language",
            "server",
            "position_encoding",
            "tools",
            "preferred_flow",
            "limitations",
            "reason_code"
        ]
    })
}

fn review_evidence_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
        "properties": {
            "available": schema_type("boolean", "True when review evidence summary is available."),
            "source": schema_type("string", "Review evidence source, usually session_ledger."),
            "total": schema_type("integer", "Total successful review evidence tool calls counted."),
            "read_only_inspection_count": schema_type("integer", "Successful read-only inspection tool calls counted."),
            "search_count": schema_type("integer", "Successful search tool calls counted."),
            "diff_review_count": schema_type("integer", "Successful diff review tool calls counted."),
            "workspace_review_count": schema_type("integer", "Successful workspace review tool calls counted."),
            "hygiene_review_count": schema_type("integer", "Successful hygiene review tool calls counted."),
            "tools": {
                "type": "array",
                "maxItems": 20,
                "description": "Bounded unique review evidence tool names only; never file contents, diff hunks, stdout/stderr, command text, tokens, secrets, or raw input payloads.",
                "items": schema_type("string", "Review evidence tool name.")
            }
        }
    })
}
