use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod common;
mod discovery;
mod jobs;

use super::input_schemas::{session_guards_schema, session_mode_schema};
use common::{
    array_schema, default_output_schema, job_lifecycle_summary_schema, nullable_schema,
    open_object_schema, permission_summary_schema, schema_type, search_match_schema,
    wrapped_output_schema,
};

pub(crate) fn output_schema_for_tool(name: &str) -> Value {
    if let Some(schema) = jobs::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = discovery::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = coding_tasks::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = checkpoints::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = artifacts::output_schema_for_tool(name) {
        return schema;
    }

    match name {
        "start_session" => wrapped_output_schema(vec![
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
            (
                "mode",
                session_mode_schema("Effective session mode."),
            ),
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
        ]),
        "session_summary" => wrapped_output_schema(vec![
            ("session_id", schema_type("string", "Opaque session id.")),
            (
                "project",
                nullable_schema("string", "Optional project associated with the task."),
            ),
            (
                "title",
                nullable_schema("string", "Optional session title."),
            ),
            (
                "mode",
                session_mode_schema("Effective session mode."),
            ),
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
                array_schema(
                    open_object_schema("Bounded session event."),
                    "Recent events.",
                ),
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
        ]),
        "post_session_message" => wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            ("session_id", schema_type("string", "Business session id whose message board was updated.")),
            ("message_id", schema_type("string", "Created wc_msg_* message id.")),
            ("message", open_object_schema("Created session message.")),
        ]),
        "list_session_messages" => wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            ("session_id", schema_type("string", "Business session id whose messages were listed.")),
            (
                "messages",
                array_schema(open_object_schema("Session message."), "Newest-first messages matching the filters."),
            ),
        ]),
        "resolve_session_message" => wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            ("session_id", schema_type("string", "Business session id containing the message.")),
            ("message_id", schema_type("string", "Resolved wc_msg_* message id.")),
            ("message", open_object_schema("Resolved session message.")),
        ]),
        "session_discussion_summary" => wrapped_output_schema(vec![
            ("success", schema_type("boolean", "Always true on success.")),
            ("session_id", schema_type("string", "Business session id being summarized.")),
            ("counts", open_object_schema("Structured message counts.")),
            (
                "open_guidance",
                array_schema(open_object_schema("Open guidance message."), "Bounded newest-first open guidance."),
            ),
            (
                "open_questions",
                array_schema(open_object_schema("Open question message."), "Bounded newest-first open questions."),
            ),
            (
                "open_risks",
                array_schema(open_object_schema("Open risk message."), "Bounded newest-first open risks."),
            ),
            (
                "open_todos",
                array_schema(open_object_schema("Open todo message."), "Bounded newest-first open todos."),
            ),
            (
                "recent_progress",
                array_schema(open_object_schema("Recent progress message."), "Bounded newest-first progress messages."),
            ),
            (
                "recent_decisions",
                array_schema(open_object_schema("Recent decision message."), "Bounded newest-first decision messages."),
            ),
        ]),
        "session_handoff_summary" => wrapped_output_schema(vec![
            (
                "summary_only",
                schema_type("boolean", "True only for compact summary_only output."),
            ),
            ("session_id", schema_type("string", "Business session id being handed off.")),
            ("project", nullable_schema("string", "Optional runtime project id, when provided.")),
            (
                "workspace_clean",
                schema_type("boolean", "Compact summary_only workspace cleanliness verdict."),
            ),
            (
                "hygiene_clean",
                schema_type("boolean", "Compact summary_only hygiene cleanliness verdict."),
            ),
            ("title", nullable_schema("string", "Optional session title.")),
            ("mode", session_mode_schema("Session mode.")),
            ("guards", session_guards_schema("Effective session guards.")),
            ("created_at", schema_type("integer", "Session creation unix timestamp.")),
            ("updated_at", schema_type("integer", "Session last-update unix timestamp.")),
            ("counts", open_object_schema("Bounded structured counts: events, failed_tool_calls, messages, open_todos, open_risks, open_questions, open_guidance.")),
            (
                "open_todos",
                array_schema(open_object_schema("Bounded open todo message."), "Bounded newest-first open todos."),
            ),
            (
                "open_risks",
                array_schema(open_object_schema("Bounded open risk message."), "Bounded newest-first open risks."),
            ),
            (
                "open_questions",
                array_schema(open_object_schema("Bounded open question message."), "Bounded newest-first open questions."),
            ),
            (
                "open_guidance",
                array_schema(open_object_schema("Bounded open guidance message."), "Bounded newest-first open guidance."),
            ),
            (
                "recent_progress",
                array_schema(open_object_schema("Bounded recent progress message."), "Bounded newest-first recent progress."),
            ),
            (
                "recent_decisions",
                array_schema(open_object_schema("Bounded recent decision message."), "Bounded newest-first recent decisions."),
            ),
            (
                "recent_failed_tools",
                array_schema(open_object_schema("Bounded failed tool call summary: tool_name, error_kind, failure_kind, created_at, write_like, job_like."), "Bounded newest-first recent failed tool calls. Never includes raw input payloads."),
            ),
            (
                "tool_failures",
                open_object_schema("Expected/unexpected tool failure classification from the session ledger. Counts expected failures, unexpected failures, expectation mismatches, and expected-failure calls that unexpectedly succeeded. Never includes raw input payloads, command text, stdout/stderr, tails, or excerpts."),
            ),
            (
                "expected_failed_tool_calls",
                array_schema(open_object_schema("Bounded expected failed tool call summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."), "Expected failed tool calls whose expectation matched."),
            ),
            (
                "unexpected_failed_tool_calls",
                array_schema(open_object_schema("Bounded unexpected failed tool call summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."), "Unexpected failed tool calls requiring review."),
            ),
            (
                "expectation_mismatches",
                array_schema(open_object_schema("Bounded expectation mismatch summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."), "Expected failures whose actual failure kind did not match."),
            ),
            (
                "unexpected_success_tool_calls",
                array_schema(open_object_schema("Bounded unexpected success summary: event_id, tool_name, project, assertion_name, expected_failure_kind, actual_failure_kind, status, success, created_at."), "Calls marked expected_failure=true that succeeded."),
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
                open_object_schema("Ledger-derived validation-like tool-call summary with status/reason: not_run, passed, failed, mixed, or unknown. Does not include stdout/stderr bodies. Minimal diagnostics, when available, are parsed only from bounded tails or safe result metadata and never infer root cause; parser.available remains false when session ledger events lack those fields."),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Bounded suggested next actions for the receiving agent."),
            ),
        ]),
        "workspace_hygiene_check" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                nullable_schema("string", "Canonical runtime project id, when resolved."),
            ),
            ("git_available", schema_type("boolean", "True when the project is a git repository.")),
            ("clean", schema_type("boolean", "True when git is available and no findings were reported.")),
            (
                "counts",
                open_object_schema("Bounded finding counts: findings, critical, high, medium, low, untracked, tracked, large_files, secret_like_paths, cache_paths."),
            ),
            (
                "findings",
                array_schema(
                    open_object_schema("Hygiene finding: path, kind, severity, tracked_status, reason, recommendation. Never includes file contents."),
                    "Bounded hygiene findings. Path is project-relative. Secret-like files are identified by name only.",
                ),
            ),
            ("truncated", schema_type("boolean", "True when findings were truncated to max_findings.")),
            (
                "warnings",
                array_schema(schema_type("string", "Warning code."), "Warning codes such as non_git_project."),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Bounded suggested next actions."),
            ),
        ]),
        "delete_project_files" => wrapped_output_schema(vec![
            ("ok", schema_type("boolean", "True when the delete command completed successfully.")),
            (
                "deleted_paths",
                array_schema(schema_type("string", "Deleted project-relative path."), "Requested paths removed with rm -f."),
            ),
            (
                "missing_paths",
                array_schema(schema_type("string", "Missing project-relative path."), "Reserved for future missing-path detail; currently empty for rm -f success."),
            ),
            (
                "refused_paths",
                array_schema(schema_type("string", "Refused project-relative path."), "Reserved for future refused-path detail; cleanup path validation failures still return a failed tool result."),
            ),
            (
                "stdout_present",
                schema_type("boolean", "Whether the underlying command produced stdout. Raw stdout is not exposed by default."),
            ),
            (
                "stderr_present",
                schema_type("boolean", "Whether the underlying command produced stderr. Raw stderr is not exposed by default."),
            ),
        ]),
        "bind_current_session" => wrapped_output_schema(vec![
            ("bound", schema_type("boolean", "True when the binding was stored.")),
            ("session_id", schema_type("string", "Bound session id.")),
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                schema_type("string", "Canonical runtime project id used in the binding key."),
            ),
            ("mode", session_mode_schema("Bound session mode.")),
            ("guards", session_guards_schema("Effective guards for the bound session.")),
        ]),
        "current_session" => wrapped_output_schema(vec![
            ("found", schema_type("boolean", "True when a live binding exists.")),
            ("session_id", schema_type("string", "Bound session id, when found.")),
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                schema_type("string", "Canonical runtime project id used in the binding key."),
            ),
            ("mode", session_mode_schema("Bound session mode, when found.")),
            ("guards", session_guards_schema("Effective guards for the bound session.")),
        ]),
        "unbind_current_session" => wrapped_output_schema(vec![
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
        ]),
        "read_file" => wrapped_output_schema(vec![
            ("content", schema_type("string", "File content.")),
            ("path", schema_type("string", "Project-relative path.")),
            (
                "start_line",
                schema_type("integer", "1-based starting line."),
            ),
            ("limit", schema_type("integer", "Maximum requested line count.")),
            (
                "total_lines",
                schema_type("integer", "Total line count, when available."),
            ),
            (
                "numbered_text",
                schema_type(
                    "string",
                    "Optional line-numbered content when with_line_numbers=true.",
                ),
            ),
            (
                "lines",
                array_schema(
                    open_object_schema("Line object with 1-based line and text fields."),
                    "Optional structured lines when with_line_numbers=true.",
                ),
            ),
        ]),
        "search_project_text" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved project id.")),
            ("pattern", schema_type("string", "Search pattern.")),
            ("path", schema_type("string", "Project-relative search root.")),
            (
                "backend",
                schema_type("string", "Search backend used: rg, grep, or native."),
            ),
            (
                "matches",
                array_schema(
                    search_match_schema(),
                    "Bounded search matches.",
                ),
            ),
            ("count", schema_type("integer", "Returned match count.")),
            (
                "truncated",
                schema_type("boolean", "Whether more matches were available."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Search command exit code, when available."),
            ),
            (
                "context_before",
                schema_type("integer", "Effective context lines before each match."),
            ),
            (
                "context_after",
                schema_type("integer", "Effective context lines after each match."),
            ),
        ]),
        "git_status" | "git_diff" => wrapped_output_schema(vec![
            (
                "exit_code",
                nullable_schema("integer", "Git command exit code."),
            ),
            ("stdout", schema_type("string", "Git command stdout.")),
            ("stderr", schema_type("string", "Git command stderr.")),
        ]),
        "git_diff_summary" => wrapped_output_schema(vec![
            (
                "status",
                schema_type("string", "Porcelain git status output."),
            ),
            (
                "diff_stat",
                schema_type("string", "Git diff --stat output."),
            ),
            (
                "changed_files",
                array_schema(
                    open_object_schema("Changed file summary."),
                    "Changed files.",
                ),
            ),
        ]),
        "git_diff_hunks" => wrapped_output_schema(vec![
            (
                "files",
                array_schema(open_object_schema("File diff hunks."), "Changed files."),
            ),
            ("hunk_count", schema_type("integer", "Returned hunk count.")),
            (
                "truncated",
                schema_type("boolean", "Whether output was bounded/truncated."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Git diff exit code."),
            ),
            ("stderr", schema_type("string", "Git diff stderr.")),
        ]),
        "git_log" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project id.")),
            ("limit", schema_type("integer", "Effective commit limit.")),
            ("skip", schema_type("integer", "Effective commit offset.")),
            ("count", schema_type("integer", "Returned commit count.")),
            (
                "truncated",
                schema_type("boolean", "Whether more commits were available."),
            ),
            (
                "commits",
                array_schema(open_object_schema("Git commit summary."), "Recent commits."),
            ),
        ]),
        "show_changes" => wrapped_output_schema(vec![
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
                nullable_schema("string", "Current git branch from porcelain status."),
            ),
            ("head", open_object_schema("Current HEAD commit metadata.")),
            (
                "clean",
                schema_type("boolean", "Whether the worktree is clean."),
            ),
            ("counts", open_object_schema("Parsed status counts.")),
            (
                "files",
                array_schema(open_object_schema("Changed file status."), "Changed files."),
            ),
            (
                "diff_stat",
                schema_type("string", "Git diff --stat output."),
            ),
            (
                "hunks",
                array_schema(
                    open_object_schema("Bounded file diff hunks."),
                    "Diff hunks.",
                ),
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
                "session",
                nullable_schema("object", "Optional session activity summary."),
            ),
        ]),
        "cargo_fmt" | "cargo_check" | "cargo_test" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Runtime project id.")),
            ("command", schema_type("string", "Cargo command executed.")),
            (
                "cwd",
                schema_type("string", "Project-relative working directory."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Cargo command exit code."),
            ),
            (
                "duration_ms",
                schema_type("integer", "Command duration in milliseconds."),
            ),
            ("stdout_tail", schema_type("string", "Bounded stdout tail.")),
            ("stderr_tail", schema_type("string", "Bounded stderr tail.")),
            (
                "stdout_truncated",
                schema_type("boolean", "Whether stdout was truncated."),
            ),
            (
                "stderr_truncated",
                schema_type("boolean", "Whether stderr was truncated."),
            ),
            (
                "passed",
                schema_type("boolean", "Whether exit_code was zero."),
            ),
            (
                "warnings_count",
                nullable_schema("integer", "Heuristic warning count for cargo_check."),
            ),
            (
                "errors_count",
                nullable_schema("integer", "Heuristic error count for cargo_check."),
            ),
            (
                "tests_passed",
                nullable_schema("integer", "Parsed passed test count for cargo_test."),
            ),
            (
                "tests_failed",
                nullable_schema("integer", "Parsed failed test count for cargo_test."),
            ),
        ]),
        "apply_patch" | "apply_patch_checked" => wrapped_output_schema(vec![
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
        ]),
        "validate_patch" => wrapped_output_schema(vec![
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
        ]),
        "replace_line_range" | "delete_line_range" => wrapped_output_schema(vec![
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
        ]),
        "apply_text_edits" => wrapped_output_schema(vec![
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
        ]),
        "insert_at_line" => wrapped_output_schema(vec![
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
        ]),
        "replace_exact_block" => wrapped_output_schema(vec![
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
        ]),
        "insert_before_pattern" | "insert_after_pattern" => wrapped_output_schema(vec![
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
        ]),
        _ => default_output_schema(),
    }
}
