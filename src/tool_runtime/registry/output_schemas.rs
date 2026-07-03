use serde_json::{json, Value};

use super::input_schemas::{
    checkpoint_labels_schema, checkpoint_validation_schema, session_guards_schema,
    session_mode_schema,
};

fn schema_type(kind: &str, description: &str) -> Value {
    json!({
        "type": kind,
        "description": description,
    })
}

fn nullable_schema(kind: &str, description: &str) -> Value {
    json!({
        "anyOf": [
            { "type": kind },
            { "type": "null" }
        ],
        "description": description,
    })
}

fn array_schema(items: Value, description: &str) -> Value {
    json!({
        "type": "array",
        "items": items,
        "description": description,
    })
}

fn open_object_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
    })
}

fn search_context_line_schema() -> Value {
    json!({
        "type": "object",
        "description": "A context line adjacent to a search match.",
        "properties": {
            "line": {
                "type": "integer",
                "description": "1-based line number."
            },
            "text": {
                "type": "string",
                "description": "Line text."
            }
        },
        "required": ["line", "text"],
        "additionalProperties": true
    })
}

fn search_match_schema() -> Value {
    let context_lines = array_schema(search_context_line_schema(), "Context lines.");
    json!({
        "type": "object",
        "description": "Search match with path, 1-based line, preview, and bounded context lines.",
        "properties": {
            "path": {
                "type": "string",
                "description": "Project-relative file path."
            },
            "line": {
                "type": "integer",
                "description": "1-based match line number."
            },
            "preview": {
                "type": "string",
                "description": "Matched line preview."
            },
            "context_before": context_lines.clone(),
            "context_after": context_lines,
        },
        "required": ["path", "line", "preview", "context_before", "context_after"],
        "additionalProperties": true
    })
}

fn session_hint_schema() -> Value {
    json!({
        "type": "object",
        "description": "Optional lightweight hint that the recorder session has open guidance, question, todo, or risk messages. Counts only; never includes message text.",
        "properties": {
            "has_open_messages": {
                "type": "boolean",
                "description": "True when any counted open session-local message exists."
            },
            "open_counts": {
                "type": "object",
                "description": "Open message counts by counted kind.",
                "properties": {
                    "guidance": { "type": "integer", "minimum": 0 },
                    "question": { "type": "integer", "minimum": 0 },
                    "todo": { "type": "integer", "minimum": 0 },
                    "risk": { "type": "integer", "minimum": 0 }
                },
                "required": ["guidance", "question", "todo", "risk"],
                "additionalProperties": false
            },
            "highest_priority": {
                "type": "string",
                "enum": ["low", "normal", "high"],
                "description": "Highest priority among counted open messages."
            },
            "suggested_next_tool": {
                "type": "string",
                "enum": ["session_discussion_summary"],
                "description": "Tool to call when the model needs the bounded message details."
            }
        },
        "required": [
            "has_open_messages",
            "open_counts",
            "highest_priority",
            "suggested_next_tool"
        ],
        "additionalProperties": false
    })
}

fn wrapped_output_schema(output_properties: Vec<(&str, Value)>) -> Value {
    let mut output_properties = output_properties;
    output_properties.extend([
        (
            "session_recorded",
            schema_type(
                "boolean",
                "True when this tool call was recorded in a provided session_id.",
            ),
        ),
        (
            "session_id",
            schema_type(
                "string",
                "Session id used for telemetry recording, when provided.",
            ),
        ),
        (
            "session_event_id",
            schema_type(
                "string",
                "Session event id for the recorded finished tool call.",
            ),
        ),
        ("session_hint", session_hint_schema()),
    ]);
    let properties = output_properties
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "output": {
                "type": "object",
                "properties": properties,
                "additionalProperties": true
            },
            "error": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ]
            }
        },
        "required": ["success"],
        "additionalProperties": true,
    })
}

fn default_output_schema() -> Value {
    wrapped_output_schema(vec![])
}

pub(crate) fn output_schema_for_tool(name: &str) -> Value {
    match name {
        "run_shell" => wrapped_output_schema(vec![
            (
                "duration_ms",
                schema_type("integer", "Command duration in milliseconds."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            ("stdout", schema_type("string", "Captured stdout.")),
            ("stderr", schema_type("string", "Captured stderr.")),
            (
                "stdout_tail",
                schema_type("string", "Bounded stdout tail on failure."),
            ),
            (
                "stderr_tail",
                schema_type("string", "Bounded stderr tail on failure."),
            ),
            (
                "stdout_truncated",
                schema_type("boolean", "Whether stdout_tail was truncated."),
            ),
            (
                "stderr_truncated",
                schema_type("boolean", "Whether stderr_tail was truncated."),
            ),
            (
                "command_started",
                schema_type("boolean", "Whether the command process was started."),
            ),
            (
                "command_completed",
                schema_type(
                    "boolean",
                    "Whether the command reached a terminal result before tool timeout.",
                ),
            ),
            (
                "command_ok",
                schema_type("boolean", "Whether the command completed with exit code 0."),
            ),
            (
                "failure_kind",
                nullable_schema(
                    "string",
                    "Structured failure kind such as command_exit_nonzero, timeout, agent_offline, spawn_failed, permission_denied, tool_schema_error, or runtime_error.",
                ),
            ),
            (
                "tool_failure",
                schema_type(
                    "boolean",
                    "True for WebCodex tool/runtime failures; false for command exit status failures.",
                ),
            ),
        ]),
        "run_job" | "run_codex" => wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            ("kind", schema_type("string", "Job kind.")),
            ("status", schema_type("string", "Initial job status.")),
            ("project", schema_type("string", "Project id.")),
        ]),
        "job_status" => wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            ("status", schema_type("string", "Current job status.")),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            (
                "started_at",
                nullable_schema("string", "Job start timestamp."),
            ),
            ("ended_at", nullable_schema("string", "Job end timestamp.")),
            (
                "error",
                nullable_schema("string", "Job error message, when available."),
            ),
        ]),
        "job_log" => wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            (
                "stdout",
                schema_type("string", "Captured stdout or selected stdout tail."),
            ),
            (
                "stderr",
                schema_type("string", "Captured stderr or selected stderr tail."),
            ),
            (
                "offset",
                schema_type("integer", "Requested stdout line offset."),
            ),
            (
                "next_offset",
                schema_type("integer", "Next stdout line offset."),
            ),
            (
                "tail_lines",
                schema_type("integer", "Requested tail line count."),
            ),
        ]),
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
                open_object_schema("Projects configuration status."),
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
                "quic",
                open_object_schema("QUIC transport status, when enabled."),
            ),
        ]),
        "list_projects" => wrapped_output_schema(vec![
            (
                "projects",
                array_schema(open_object_schema("Project summary."), "Runtime projects."),
            ),
            ("count", schema_type("integer", "Project count.")),
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
                array_schema(open_object_schema("Tool metadata."), "Runtime tool specs."),
            ),
            ("count", schema_type("integer", "Tool count.")),
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
                        "Compact tool entry: name, category, provider, risk, read_only, requires_project, path_hint, destructive, shell_like, oauth_scope.",
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
                "rules",
                nullable_schema("object", "Deterministic project instruction source summary when requested; null otherwise."),
            ),
            (
                "git",
                nullable_schema("object", "Structured worktree/git summary when requested; null otherwise."),
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
            ("project", schema_type("string", "Original project input.")),
            (
                "resolved_project",
                open_object_schema("Resolved project id, path, executor, and safe project metadata."),
            ),
            ("session_id", schema_type("string", "Explicit task session id.")),
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
                open_object_schema("Ledger-based validation-like tool-call summary. Does not include stdout/stderr bodies and does not parse compiler or test output."),
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
                "final_warnings",
                array_schema(open_object_schema("Finish warning."), "Bounded finish warnings."),
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
            ("session_id", schema_type("string", "Business session id being handed off.")),
            ("project", nullable_schema("string", "Optional runtime project id, when provided.")),
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
                "workspace",
                open_object_schema("Bounded workspace summary when project is provided: project, git_available, non_git_project, clean, branch, head, changed_files_count, warnings, suggested_next_actions. Never includes hunks or full diffs."),
            ),
            (
                "checkpoints",
                open_object_schema("Bounded checkpoint candidates when project is provided: latest_last_known_good and recent list. Never includes validation.commands or diffs."),
            ),
            (
                "validation",
                open_object_schema("Ledger-derived validation-like tool-call summary. Does not include stdout/stderr bodies and does not parse compiler or test output. parser.available remains false until a parser exists."),
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
