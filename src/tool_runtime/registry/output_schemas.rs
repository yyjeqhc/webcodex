use serde_json::Value;

mod common;
mod jobs;

use super::input_schemas::{
    checkpoint_labels_schema, checkpoint_validation_schema, session_guards_schema,
    session_mode_schema,
};
use common::{
    array_schema, default_output_schema, job_lifecycle_summary_schema, nullable_schema,
    open_object_schema, permission_profile_schema, permission_summary_schema, schema_type,
    search_match_schema, wrapped_output_schema,
};

pub(crate) fn output_schema_for_tool(name: &str) -> Value {
    if let Some(schema) = jobs::output_schema_for_tool(name) {
        return schema;
    }

    match name {
        "runtime_status" => wrapped_output_schema(vec![
            ("service", schema_type("string", "Runtime service name.")),
            ("version", schema_type("string", "Runtime version.")),
            (
                "build",
                open_object_schema("Build revision metadata for the running binary."),
            ),
            ("server_time", schema_type("integer", "Server timestamp.")),
            ("pid", schema_type("integer", "Server process id.")),
            (
                "auth_enabled",
                schema_type("boolean", "Whether bearer auth is enabled."),
            ),
            (
                "configured_public_url",
                nullable_schema("string", "Configured public URL, when set."),
            ),
            (
                "projects",
                open_object_schema("Project counts split into server_static, agent_registered, and effective. Legacy configured/count/load_error fields are retained; prefer projects.effective for model-facing status."),
            ),
            (
                "agents",
                open_object_schema("Agent counts and client summaries."),
            ),
            ("jobs", open_object_schema("Runtime job counts.")),
            (
                "tools",
                open_object_schema("Runtime tool counts and names."),
            ),
            (
                "permissions",
                permission_profile_schema("Current permission/approval profile. dev_auto_approve is the self-hosted development default and does not bypass hard safety checks."),
            ),
            (
                "quic",
                open_object_schema("QUIC transport status, when enabled."),
            ),
        ]),
        "list_projects" => wrapped_output_schema(vec![
            (
                "projects",
                array_schema(open_object_schema("Project summary including capabilities.git_available, supports_cleanup_verification, and recommended_for_smoke."), "Runtime projects."),
            ),
            ("count", schema_type("integer", "Project count.")),
            (
                "recommended_for_smoke",
                array_schema(
                    schema_type("string", "Runtime project id recommended for smoke tests."),
                    "Runtime project ids whose capabilities.recommended_for_smoke is true.",
                ),
            ),
        ]),
        "list_agents" => wrapped_output_schema(vec![
            (
                "agents",
                array_schema(open_object_schema("Agent summary."), "Agent summaries."),
            ),
            (
                "clients",
                array_schema(open_object_schema("Client summary."), "Client summaries."),
            ),
            ("count", schema_type("integer", "Agent/client count.")),
        ]),
        "list_tools" => wrapped_output_schema(vec![
            (
                "tools",
                array_schema(
                    open_object_schema("Tool metadata or compact summary."),
                    "Runtime tool specs, or compact summaries when summary_only is true.",
                ),
            ),
            (
                "names",
                array_schema(schema_type("string", "Tool name."), "Returned tool names."),
            ),
            ("count", schema_type("integer", "Tool count.")),
            (
                "total_count",
                schema_type("integer", "Total number of visible runtime tools."),
            ),
            (
                "filtered_count",
                schema_type("integer", "Number of tools matching category/features before limit."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether limit truncated the matching tools."),
            ),
            (
                "hint",
                schema_type("string", "Focused discovery guidance."),
            ),
            (
                "recommended_next",
                schema_type("string", "Recommended next discovery action."),
            ),
        ]),
        "tool_manifest" => wrapped_output_schema(vec![
            (
                "schema_version",
                schema_type("integer", "Manifest schema version."),
            ),
            (
                "tool_count",
                schema_type("integer", "Total number of tools in the runtime."),
            ),
            (
                "count",
                schema_type("integer", "Returned compact tool count after filtering."),
            ),
            (
                "filtered_count",
                schema_type(
                    "integer",
                    "Number of tools after applying the optional category filter.",
                ),
            ),
            (
                "category",
                nullable_schema(
                    "string",
                    "Requested category filter, or null when no filter was applied.",
                ),
            ),
            (
                "categories",
                open_object_schema(
                    "Map of category name to the list of tool names in that category.",
                ),
            ),
            (
                "tools",
                array_schema(
                    open_object_schema(
                        "Compact tool entry: name, category, accepted_flattened_args, deprecated_or_unsupported_args, provider, risk, read_only, requires_project, path_hint, destructive, shell_like, oauth_scope.",
                    ),
                    "Compact tool entries without input/output schemas.",
                ),
            ),
            (
                "risk_summary",
                open_object_schema(
                    "Counts of tools grouped by risk class (read_only, project_write, job_run, etc.).",
                ),
            ),
            (
                "recommended_flows",
                array_schema(
                    open_object_schema("Recommended tool flow with name, purpose, and tools."),
                    "Short list of recommended tool flows for common tasks.",
                ),
            ),
        ]),
        "start_coding_task" => wrapped_output_schema(vec![
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
                nullable_schema("object", "runtime_status output when requested; null otherwise."),
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
                "warnings",
                array_schema(open_object_schema("Startup warning."), "Bounded startup warnings."),
            ),
        ]),
        "finish_coding_task" => wrapped_output_schema(vec![
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
                open_object_schema("Ledger-based validation-like tool-call summary with status/reason: not_run, passed, failed, mixed, or unknown. Does not include stdout/stderr bodies. Minimal diagnostics, when available, are parsed only from bounded tails or safe result metadata and never infer root cause."),
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
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Bounded suggested next actions based on unexpected failures, workspace, and jobs."),
            ),
        ]),
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
        "workspace_checkpoint_create" => wrapped_output_schema(vec![
            ("checkpoint_id", schema_type("string", "Created wc_ckpt_* id.")),
            ("project", schema_type("string", "Project input.")),
            ("resolved_project", schema_type("string", "Resolved runtime project id.")),
            ("title", nullable_schema("string", "Optional checkpoint title.")),
            ("kind", schema_type("string", "Semantic checkpoint kind.")),
            ("labels", checkpoint_labels_schema("Simple checkpoint labels.")),
            (
                "validation",
                checkpoint_validation_schema("Bounded validation metadata."),
            ),
            ("head", schema_type("string", "HEAD commit captured by the checkpoint.")),
            ("branch", nullable_schema("string", "Current branch, if attached.")),
            ("created_at", schema_type("integer", "Unix timestamp.")),
            ("tracked_diff_bytes", schema_type("integer", "Unstaged tracked diff size in bytes.")),
            ("staged_diff_bytes", schema_type("integer", "Staged diff size in bytes.")),
            ("untracked_files", array_schema(open_object_schema("Stored untracked file metadata."), "Stored untracked file metadata.")),
            ("skipped_files", array_schema(open_object_schema("Skipped file metadata."), "Skipped files and reasons.")),
            ("status_summary", open_object_schema("Parsed git status summary.")),
            ("complete", schema_type("boolean", "True when checkpoint content is complete and restorable.")),
            ("storage_path", schema_type("string", "Server state-dir checkpoint path, outside the project worktree.")),
        ]),
        "workspace_checkpoint_list" => wrapped_output_schema(vec![
            ("project", schema_type("string", "Project input.")),
            ("resolved_project", schema_type("string", "Resolved runtime project id.")),
            ("limit", schema_type("integer", "Effective list limit.")),
            ("checkpoints", array_schema(open_object_schema("Checkpoint metadata."), "Checkpoint metadata without full diff/content.")),
        ]),
        "workspace_checkpoint_show" => wrapped_output_schema(vec![
            ("checkpoint_id", schema_type("string", "Checkpoint id.")),
            ("project", schema_type("string", "Project input.")),
            ("resolved_project", schema_type("string", "Resolved runtime project id.")),
            ("title", nullable_schema("string", "Optional title.")),
            ("kind", schema_type("string", "Semantic checkpoint kind.")),
            ("labels", checkpoint_labels_schema("Simple checkpoint labels.")),
            (
                "validation",
                checkpoint_validation_schema("Bounded validation metadata."),
            ),
            ("head", schema_type("string", "Checkpoint HEAD commit.")),
            ("branch", nullable_schema("string", "Checkpoint branch, if attached.")),
            ("created_at", schema_type("integer", "Unix timestamp.")),
            ("files", array_schema(open_object_schema("Tracked/untracked file metadata."), "Checkpoint file list without full diff/content.")),
            ("skipped_files", array_schema(open_object_schema("Skipped file metadata."), "Skipped files and reasons.")),
            ("status_summary", open_object_schema("Parsed git status summary.")),
            ("storage_path", schema_type("string", "Server state-dir checkpoint path, outside the project worktree.")),
        ]),
        "workspace_checkpoint_restore" => wrapped_output_schema(vec![
            ("restored", schema_type("boolean", "True when restore completed.")),
            ("checkpoint_id", schema_type("string", "Restored checkpoint id.")),
            ("project", schema_type("string", "Project input.")),
            ("resolved_project", schema_type("string", "Resolved runtime project id.")),
            ("changed_paths", array_schema(schema_type("string", "Project-relative path."), "Paths restored from the checkpoint.")),
            ("warnings", array_schema(open_object_schema("Warning."), "Warnings emitted during restore.")),
        ]),
        "workspace_checkpoint_delete" => wrapped_output_schema(vec![
            ("deleted", schema_type("boolean", "True when checkpoint file was deleted.")),
            ("checkpoint_id", schema_type("string", "Deleted checkpoint id.")),
            ("project", schema_type("string", "Project input.")),
            ("resolved_project", schema_type("string", "Resolved runtime project id.")),
            ("storage_path", schema_type("string", "Deleted checkpoint path.")),
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
        "save_project_artifact" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes written to the artifact path."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the written artifact."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
        ]),
        "read_project_artifact_metadata" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("exists", schema_type("boolean", "True when the artifact exists.")),
            (
                "missing",
                schema_type("boolean", "True when allow_missing=true and the artifact was absent."),
            ),
            ("bytes", schema_type("integer", "Artifact size in bytes.")),
            (
                "sha256",
                schema_type("string", "sha256 digest of the full artifact file."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Detected or inferred MIME type."),
            ),
            (
                "modified_at",
                schema_type("integer", "File modification time as unix timestamp seconds."),
            ),
            ("width", schema_type("integer", "Image width, when cheaply detected.")),
            (
                "height",
                schema_type("integer", "Image height, when cheaply detected."),
            ),
            (
                "archive_entries_count",
                nullable_schema("integer", "Zip entry count, when cheaply detected."),
            ),
        ]),
        "artifact_upload_begin" | "artifact_upload_chunk" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "upload_id",
                schema_type("string", "Opaque upload id for later chunks, finish, or abort."),
            ),
            (
                "received_bytes",
                schema_type("integer", "Bytes currently received for this upload."),
            ),
            (
                "next_offset",
                schema_type("integer", "Offset to pass with the next chunk."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            ("max_bytes", schema_type("integer", "Maximum upload size in bytes.")),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
            (
                "committed",
                schema_type("boolean", "False until artifact_upload_finish succeeds."),
            ),
        ]),
        "artifact_upload_finish" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("upload_id", schema_type("string", "Committed upload id.")),
            ("bytes", schema_type("integer", "Final artifact size in bytes.")),
            (
                "received_bytes",
                schema_type("integer", "Bytes received before commit."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the committed artifact."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Detected, inferred, or caller-provided MIME type."),
            ),
            ("committed", schema_type("boolean", "True when commit completed.")),
        ]),
        "artifact_upload_abort" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("upload_id", schema_type("string", "Aborted upload id.")),
            (
                "received_bytes",
                schema_type("integer", "Bytes discarded from the temporary upload."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
            ("committed", schema_type("boolean", "False for aborted uploads.")),
            ("aborted", schema_type("boolean", "True when temporary upload files were removed.")),
            (
                "temp_file_removed",
                schema_type("boolean", "True when the temporary upload part file was removed."),
            ),
            (
                "sidecar_removed",
                schema_type("boolean", "True when the temporary upload sidecar was removed."),
            ),
            (
                "final_file_touched",
                schema_type("boolean", "Always false; abort does not touch the final target path."),
            ),
            (
                "final_file_exists",
                schema_type("boolean", "Read-only final target existence after abort."),
            ),
            (
                "changed_path_details",
                array_schema(open_object_schema("Path cleanup status detail."), "Abort cleanup path status details."),
            ),
        ]),
        "read_project_artifact" => wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Detected or inferred MIME type."),
            ),
            (
                "file_bytes",
                schema_type("integer", "Total file size in bytes."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the full artifact file."),
            ),
            ("offset", schema_type("integer", "Requested byte offset.")),
            (
                "bytes_returned",
                schema_type("integer", "Number of bytes returned in this chunk."),
            ),
            (
                "content_base64",
                schema_type("string", "Base64-encoded content for this chunk only."),
            ),
            (
                "next_offset",
                schema_type("integer", "Offset to use for the next chunk."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more bytes remain after this chunk."),
            ),
            (
                "eof",
                schema_type("boolean", "True when this chunk reaches end of file."),
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
