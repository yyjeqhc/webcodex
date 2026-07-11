use serde_json::{json, Value};

use super::super::input_schemas::{session_guards_schema, session_mode_schema};
use super::common::{
    array_schema, evidence_history_schema, evidence_integrity_schema, job_lifecycle_summary_schema,
    nullable_schema, open_object_schema, permission_summary_schema, schema_type,
    task_outcome_schema, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "start_session" => Some(wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            ("session_id", schema_type("string", "Opaque session id.")),
            (
                "project",
                nullable_schema("string", "Optional project associated with the task."),
            ),
            (
                "project_input",
                nullable_schema("string", "Original project input, when provided."),
            ),
            (
                "resolved_project",
                nullable_schema(
                    "string",
                    "Resolved full runtime project id, when a project was provided.",
                ),
            ),
            (
                "title",
                nullable_schema("string", "Optional session title."),
            ),
            ("mode", session_mode_schema("Effective session mode.")),
            (
                "guards",
                session_guards_schema("Effective task guard settings for this session."),
            ),
            (
                "created_at",
                schema_type("integer", "Unix timestamp in seconds."),
            ),
            (
                "project_instructions",
                nullable_schema(
                    "object",
                    "Best-effort project-local instruction files loaded at session start (e.g. AGENTS.md). null when no project was provided. Project-local guidance only; does not override system/platform/WebCodex safety policy.",
                ),
            ),
        ])),
        "session_summary" => Some(wrapped_output_schema(vec![
            ("session_id", schema_type("string", "Opaque session id.")),
            (
                "project",
                nullable_schema("string", "Optional project associated with the task."),
            ),
            (
                "title",
                nullable_schema("string", "Optional session title."),
            ),
            ("mode", session_mode_schema("Effective session mode.")),
            (
                "guards",
                session_guards_schema("Effective task guard settings for this session."),
            ),
            (
                "created_at",
                schema_type("integer", "Unix timestamp in seconds."),
            ),
            (
                "updated_at",
                schema_type("integer", "Unix timestamp in seconds."),
            ),
            ("counts", open_object_schema("Structured event counters.")),
            (
                "events",
                array_schema(open_object_schema("Bounded session event."), "Recent events."),
            ),
            (
                "messages",
                open_object_schema("Bounded session message-board summary: counts plus at most five recent progress messages; never the full message queue."),
            ),
            (
                "project_instructions",
                nullable_schema(
                    "object",
                    "Summary-only projection of project-local instructions loaded at session start (no content bodies). Present when the session was created with a project. Project-local guidance only; does not override system/platform/WebCodex safety policy.",
                ),
            ),
        ])),
        "validation_summary" => Some(validation_summary_tool_output_schema()),
        "post_session_message" => Some(wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            (
                "session_id",
                schema_type("string", "Business session id whose message board was updated."),
            ),
            (
                "message_id",
                schema_type("string", "Created wc_msg_* message id."),
            ),
            ("message", open_object_schema("Created session message.")),
        ])),
        "list_session_messages" => Some(wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            (
                "session_id",
                schema_type("string", "Business session id whose messages were listed."),
            ),
            (
                "messages",
                array_schema(
                    open_object_schema("Session message."),
                    "Newest-first messages matching the filters.",
                ),
            ),
        ])),
        "resolve_session_message" => Some(wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            (
                "session_id",
                schema_type("string", "Business session id containing the message."),
            ),
            (
                "message_id",
                schema_type("string", "Resolved wc_msg_* message id."),
            ),
            ("message", open_object_schema("Resolved session message.")),
        ])),
        "session_discussion_summary" => Some(wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            (
                "session_id",
                schema_type("string", "Business session id being summarized."),
            ),
            ("counts", open_object_schema("Structured message counts.")),
            (
                "open_guidance",
                array_schema(
                    open_object_schema("Open guidance message."),
                    "Bounded newest-first open guidance.",
                ),
            ),
            (
                "open_questions",
                array_schema(
                    open_object_schema("Open question message."),
                    "Bounded newest-first open questions.",
                ),
            ),
            (
                "open_risks",
                array_schema(
                    open_object_schema("Open risk message."),
                    "Bounded newest-first open risks.",
                ),
            ),
            (
                "open_todos",
                array_schema(
                    open_object_schema("Open todo message."),
                    "Bounded newest-first open todos.",
                ),
            ),
            (
                "recent_progress",
                array_schema(
                    open_object_schema("Recent progress message."),
                    "Bounded newest-first progress messages.",
                ),
            ),
            (
                "recent_decisions",
                array_schema(
                    open_object_schema("Recent decision message."),
                    "Bounded newest-first decision messages.",
                ),
            ),
        ])),
        "session_handoff_summary" => Some(wrapped_output_schema(vec![
            (
                "summary_only",
                schema_type("boolean", "True only for compact summary_only output."),
            ),
            (
                "session_id",
                schema_type("string", "Business session id being handed off."),
            ),
            (
                "project",
                nullable_schema("string", "Optional runtime project id, when provided."),
            ),
            (
                "workspace_clean",
                schema_type(
                    "boolean",
                    "Compact summary_only workspace cleanliness verdict.",
                ),
            ),
            (
                "hygiene_clean",
                schema_type("boolean", "Compact summary_only hygiene cleanliness verdict."),
            ),
            ("title", nullable_schema("string", "Optional session title.")),
            ("mode", session_mode_schema("Session mode.")),
            (
                "guards",
                session_guards_schema("Effective session guards."),
            ),
            (
                "created_at",
                schema_type("integer", "Session creation unix timestamp."),
            ),
            (
                "updated_at",
                schema_type("integer", "Session last-update unix timestamp."),
            ),
            (
                "counts",
                open_object_schema("Bounded structured counts: events, failed_tool_calls, messages, open_todos, open_risks, open_questions, open_guidance."),
            ),
            (
                "open_todos",
                array_schema(
                    open_object_schema("Bounded open todo message."),
                    "Bounded newest-first open todos.",
                ),
            ),
            (
                "open_risks",
                array_schema(
                    open_object_schema("Bounded open risk message."),
                    "Bounded newest-first open risks.",
                ),
            ),
            (
                "open_questions",
                array_schema(
                    open_object_schema("Bounded open question message."),
                    "Bounded newest-first open questions.",
                ),
            ),
            (
                "open_guidance",
                array_schema(
                    open_object_schema("Bounded open guidance message."),
                    "Bounded newest-first open guidance.",
                ),
            ),
            (
                "recent_progress",
                array_schema(
                    open_object_schema("Bounded recent progress message."),
                    "Bounded newest-first recent progress.",
                ),
            ),
            (
                "recent_decisions",
                array_schema(
                    open_object_schema("Bounded recent decision message."),
                    "Bounded newest-first recent decisions.",
                ),
            ),
            (
                "recent_failed_tools",
                array_schema(
                    open_object_schema("Bounded failed tool call summary: tool_name, error_kind, failure_kind, created_at, write_like, job_like."),
                    "Bounded newest-first recent failed tool calls. Never includes raw input payloads.",
                ),
            ),
            (
                "tool_failures",
                open_object_schema("Expected/unexpected tool failure classification from the session ledger. Counts expected failures, unexpected failures, expectation mismatches, and expected-failure calls that unexpectedly succeeded. Never includes raw input payloads, command text, stdout/stderr, tails, or excerpts."),
            ),
            (
                "expected_failed_tool_calls",
                array_schema(
                    open_object_schema("Bounded expected failed tool call summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."),
                    "Expected failed tool calls whose expectation matched.",
                ),
            ),
            (
                "unexpected_failed_tool_calls",
                array_schema(
                    open_object_schema("Bounded unexpected failed tool call summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."),
                    "Unexpected failed tool calls requiring review.",
                ),
            ),
            (
                "expectation_mismatches",
                array_schema(
                    open_object_schema("Bounded expectation mismatch summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."),
                    "Expected failures whose actual failure kind did not match.",
                ),
            ),
            (
                "unexpected_success_tool_calls",
                array_schema(
                    open_object_schema("Bounded unexpected success summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."),
                    "Calls marked expected_failure=true that succeeded.",
                ),
            ),
            (
                "permissions",
                permission_summary_schema("Deterministic bounded permission decision summary from the session ledger. Counts high-risk auto-approved tools only; never includes stdout/stderr, env, tokens, secrets, or raw input content."),
            ),
            (
                "jobs",
                job_lifecycle_summary_schema("Bounded job lifecycle summary for handoff. active_jobs_present is emitted only for blocking_active_count > 0; stop_requested-only jobs use nonblocking jobs_terminal_pending. Never includes stdout/stderr or command text."),
            ),
            (
                "workspace",
                open_object_schema("Bounded workspace summary when project is provided: project, git_available, non_git_project, clean, branch, head, changed_files_count, warnings, suggested_next_actions. Never includes hunks or full diffs."),
            ),
            (
                "checkpoints",
                open_object_schema("Bounded checkpoint candidates when project is provided: latest_last_known_good and recent list. Never includes validation.commands or diffs."),
            ),
            (
                "validation",
                open_object_schema("Ledger-derived validation-like tool-call summary with status/reason: not_run, passed, failed, mixed, or unknown. Parser version 2 provides bounded structured diagnostics from bounded validation metadata while retaining backward-compatible first_diagnostic, failed_tests, and first_failed_test fields. Full and summary_only closeout preserve the same validation evidence. Does not include stdout/stderr bodies and performs no root-cause inference; parser.available remains false when session ledger events lack those fields. latest_status and historical_failures retain the existing final-state and resolved-history semantics."),
            ),
            (
                "review_evidence",
                review_evidence_schema("Ledger-derived non-cargo review evidence summary for full and summary_only outputs. Counts successful read/search/diff/workspace/hygiene inspection tools from the session ledger and exposes bounded tools for compact explainability. For docs-only or read-only audit tasks, validation.status may remain not_run while review_evidence.total is greater than zero. Does not include file contents, stdout/stderr, diff hunks, command text, tokens, secrets, or raw input payloads. Does not change validation.status or make the verdict pass."),
            ),
            (
                "verdict",
                open_object_schema("Legacy aggregate closeout verdict for full and summary_only output: task_outcome fail or evidence_integrity error maps to blocking fail; otherwise task_outcome warn or evidence_integrity warning maps to non-blocking warn; otherwise pass. Resolved evidence history alone does not lower the verdict."),
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
                array_schema(
                    schema_type("string", "Short suggested action."),
                    "Bounded suggested next actions for the receiving agent.",
                ),
            ),
        ])),
        "bind_current_session" => Some(wrapped_output_schema(vec![
            ("bound", schema_type("boolean", "True when the binding was stored.")),
            ("session_id", schema_type("string", "Bound session id.")),
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                schema_type("string", "Canonical runtime project id used in the binding key."),
            ),
            ("mode", session_mode_schema("Bound session mode.")),
            (
                "guards",
                session_guards_schema("Effective guards for the bound session."),
            ),
        ])),
        "current_session" => Some(wrapped_output_schema(vec![
            ("found", schema_type("boolean", "True when a live binding exists.")),
            (
                "session_id",
                schema_type("string", "Bound session id, when found."),
            ),
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                schema_type("string", "Canonical runtime project id used in the binding key."),
            ),
            ("mode", session_mode_schema("Bound session mode, when found.")),
            (
                "guards",
                session_guards_schema("Effective guards for the bound session."),
            ),
        ])),
        "unbind_current_session" => Some(wrapped_output_schema(vec![
            (
                "unbound",
                schema_type("boolean", "True when the unbind request succeeded."),
            ),
            (
                "had_binding",
                schema_type("boolean", "True when a binding existed before this call."),
            ),
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                schema_type("string", "Canonical runtime project id used in the binding key."),
            ),
        ])),
        _ => None,
    }
}

fn validation_summary_tool_output_schema() -> Value {
    let mut schema = wrapped_output_schema(vec![
        (
            "project",
            schema_type("string", "Resolved complete runtime project id."),
        ),
        (
            "session_id",
            schema_type("string", "Explicit business session id queried."),
        ),
        ("validation", validation_evidence_schema()),
    ]);
    schema["properties"]["output"]["additionalProperties"] = json!(false);
    schema
}

fn validation_evidence_schema() -> Value {
    let event = validation_event_schema();
    json!({
        "type": "object",
        "description": "Bounded deterministic validation evidence derived only from safe session-ledger metadata. Never contains commands, raw event payloads, validation excerpts, full stdout/stderr, environment variables, or root-cause inference.",
        "additionalProperties": false,
        "properties": {
            "available": schema_type("boolean", "True when validation-like ledger events exist."),
            "status": { "type": "string", "enum": ["not_run", "passed", "failed", "mixed", "unknown"] },
            "reason": { "anyOf": [{"type": "string"}, {"type": "null"}] },
            "latest": { "anyOf": [event.clone(), {"type": "null"}] },
            "latest_status": { "type": "string", "enum": ["not_run", "passed", "failed", "unknown"] },
            "historical_failures": validation_historical_failures_schema(),
            "source": { "type": "string", "enum": ["session_ledger"] },
            "events_total": { "type": "integer", "minimum": 0 },
            "successes": { "type": "integer", "minimum": 0 },
            "failures": { "type": "integer", "minimum": 0 },
            "latest_success": event.clone(),
            "latest_failure": event.clone(),
            "events": {
                "type": "array",
                "maxItems": 100,
                "items": event,
                "description": "Bounded validation history only; never raw session events."
            },
            "parser": validation_parser_metadata_schema(),
            "cargo_test_zero_tests_run": schema_type("boolean", "True when a successful cargo_test event explicitly reported zero tests run."),
            "skipped": schema_type("boolean", "True only when validation summary generation was explicitly skipped by a closeout caller.")
        },
        "required": [
            "available", "status", "reason", "latest", "latest_status",
            "historical_failures", "source", "events_total", "events", "parser",
            "cargo_test_zero_tests_run"
        ]
    })
}

fn validation_historical_failures_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "count": { "type": "integer", "minimum": 0 },
            "resolved": { "type": "boolean" },
            "unresolved": { "type": "boolean" }
        },
        "required": ["count", "resolved", "unresolved"]
    })
}

fn validation_parser_metadata_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "available": { "type": "boolean" },
            "kind": { "type": "string", "enum": ["structured_validation_parser"] },
            "version": { "type": "integer", "enum": [2] },
            "source": { "type": "string", "enum": ["bounded_validation_metadata"] },
            "raw_output_exposed": { "type": "boolean", "enum": [false] },
            "limitations": {
                "type": "array",
                "maxItems": 3,
                "items": { "type": "string", "maxLength": 160 }
            },
            "reason": { "type": "string", "maxLength": 160 }
        },
        "required": ["available", "kind", "version", "source", "raw_output_exposed", "limitations"]
    })
}

fn validation_event_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tool_name": { "type": "string", "enum": ["cargo_fmt", "cargo_check", "cargo_test", "validate_patch", "apply_patch_checked"] },
            "validation_kind": { "type": "string", "enum": ["format", "check", "test", "patch_preflight", "patch_apply_checked"] },
            "success": { "type": "boolean" },
            "failure_kind": { "type": "string", "enum": ["compile_error", "test_failure", "timeout", "process_exit", "format_diff", "unknown"] },
            "exit_code": { "type": "integer" },
            "summary": { "type": "string", "maxLength": 80 },
            "project": { "type": "string", "maxLength": 512 },
            "session_id": { "type": "string", "maxLength": 128 },
            "started_at": { "type": "integer" },
            "completed_at": { "type": "integer" },
            "duration_ms": { "type": "integer", "minimum": 0 },
            "affected_paths": {
                "type": "array",
                "maxItems": 20,
                "items": { "type": "string", "maxLength": 512 }
            },
            "diagnostics": validation_diagnostics_schema(),
            "tests_detected": { "type": "boolean" },
            "tests_run_count": { "type": "integer", "minimum": 0 },
            "zero_tests_run": { "type": "boolean" }
        },
        "required": ["tool_name", "validation_kind", "success", "failure_kind", "summary", "session_id"]
    })
}

fn validation_diagnostics_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "available": { "type": "boolean" },
            "parser": { "type": "string", "enum": ["structured_validation_parser"] },
            "reason": { "type": "string", "maxLength": 160 },
            "diagnostic_count": { "type": "integer", "minimum": 0 },
            "diagnostics": {
                "type": "array",
                "maxItems": 20,
                "items": cargo_diagnostic_schema()
            },
            "returned_diagnostic_count": { "type": "integer", "minimum": 0, "maximum": 20 },
            "diagnostics_truncated": { "type": "boolean" },
            "invalid_diagnostics_omitted": { "type": "integer", "minimum": 0 },
            "first_diagnostic": cargo_diagnostic_schema(),
            "test_summary": cargo_test_summary_schema(),
            "failed_tests": {
                "type": "array",
                "maxItems": 20,
                "uniqueItems": true,
                "items": { "type": "string", "maxLength": 240 }
            },
            "failed_test_details": {
                "type": "array",
                "maxItems": 20,
                "items": failed_test_detail_schema()
            },
            "first_failed_test": { "type": "string", "maxLength": 240 },
            "failed_tests_truncated": { "type": "boolean" },
            "truncated": { "type": "boolean" }
        },
        "required": [
            "available", "parser", "diagnostics", "returned_diagnostic_count",
            "diagnostics_truncated", "invalid_diagnostics_omitted", "failed_tests",
            "failed_test_details", "failed_tests_truncated"
        ]
    })
}

fn cargo_diagnostic_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "severity": { "type": "string", "enum": ["error", "warning", "unknown"] },
            "code": { "type": "string", "maxLength": 64 },
            "file": { "type": "string", "maxLength": 512 },
            "line": { "type": "integer", "minimum": 1 },
            "column": { "type": "integer", "minimum": 1 },
            "message": { "type": "string", "maxLength": 240 }
        },
        "required": ["severity", "message"]
    })
}

fn failed_test_detail_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string", "maxLength": 240 },
            "failure_kind": { "type": "string", "enum": ["assertion", "panic", "unknown"] },
            "file": { "anyOf": [{"type": "string", "maxLength": 512}, {"type": "null"}] },
            "line": { "anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}] },
            "column": { "anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}] }
        },
        "required": ["name", "failure_kind", "file", "line", "column"]
    })
}

fn cargo_test_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "passed": { "type": "integer", "minimum": 0 },
            "failed": { "type": "integer", "minimum": 0 },
            "ignored": { "type": "integer", "minimum": 0 }
        }
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
