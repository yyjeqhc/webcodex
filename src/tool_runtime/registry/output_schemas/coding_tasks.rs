use serde_json::Value;

use super::common::{
    array_schema, job_lifecycle_summary_schema, nullable_schema, open_object_schema,
    permission_profile_schema, permission_summary_schema, schema_type, wrapped_output_schema,
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
                open_object_schema("Ledger-based validation-like tool-call summary with status/reason: not_run, passed, failed, mixed, or unknown. The status field remains strict ledger history; latest_status and historical_failures distinguish final validation state from resolved historical failures. Does not include stdout/stderr bodies. Minimal diagnostics, when available, are parsed only from bounded tails or safe result metadata and never infer root cause."),
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
                open_object_schema("Operator-friendly compact sanity verdict for summary_only output: status pass/warn/fail, blocking, blocking_reasons, warning_reasons, and suggested_next_actions. Additive UX summary only; does not change safety semantics."),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Bounded suggested next actions based on unexpected failures, workspace, and jobs."),
            ),
        ])),
        _ => None,
    }
}
