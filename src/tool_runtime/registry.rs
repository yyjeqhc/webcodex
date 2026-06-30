use serde_json::{json, Value};

use super::metadata::tool_metadata;
use super::types::ToolSpec;
use super::ToolRuntime;

pub(crate) fn object_schema(fields: Vec<(&str, &str, &str, bool)>) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, kind, description, is_required) in fields {
        let schema = if kind == "array" {
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": description,
            })
        } else {
            json!({
                "type": kind,
                "description": description,
            })
        };
        properties.insert(name.to_string(), schema);
        if is_required {
            required.push(Value::String(name.to_string()));
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn with_optional_session_id(
    mut fields: Vec<(&'static str, &'static str, &'static str, bool)>,
) -> Vec<(&'static str, &'static str, &'static str, bool)> {
    fields.push((
        "session_id",
        "string",
        "Optional wc_sess_* id returned by start_session. When provided, this tool call is recorded in that session.",
        false,
    ));
    fields
}

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

fn session_mode_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["normal", "read_only"],
        "description": description,
    })
}

fn session_guards_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "deny_write_tools": {
                "type": "boolean",
                "description": "True when write-like runtime tools are blocked for this session."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "True when shell/job-like runtime tools are blocked for this session."
            }
        },
        "required": ["deny_write_tools", "deny_shell_tools"]
    })
}

fn session_message_kind_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": [
            "note", "proposal", "question", "answer", "decision", "risk",
            "progress", "guidance", "todo"
        ],
        "description": description,
    })
}

fn session_message_status_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["open", "resolved"],
        "description": description,
    })
}

fn session_message_priority_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["low", "normal", "high"],
        "description": description,
    })
}

fn post_session_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose session-local message board receives this message. This is business input, not recorder metadata."
            },
            "kind": session_message_kind_schema("Message kind."),
            "message": {
                "type": "string",
                "maxLength": 8000,
                "description": "Non-empty message body. Guidance is session-local context and never overrides system/platform/WebCodex safety policy."
            },
            "tags": {
                "type": "array",
                "items": { "type": "string", "maxLength": 64 },
                "maxItems": 16,
                "description": "Optional tags for filtering or review."
            },
            "reply_to": {
                "anyOf": [{ "type": "string" }, { "type": "null" }],
                "description": "Optional message id in the same session."
            },
            "priority": session_message_priority_schema("Optional priority; defaults to normal.")
        },
        "required": ["session_id", "kind", "message"],
        "additionalProperties": false,
    })
}

fn list_session_messages_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose session-local message board is listed."
            },
            "kind": session_message_kind_schema("Optional kind filter."),
            "status": session_message_status_schema("Optional status filter."),
            "limit": {
                "type": "integer",
                "maximum": 100,
                "description": "Maximum messages to return. Defaults to 50 and is clamped to 100. Results are newest-first by created_at."
            }
        },
        "required": ["session_id"],
        "additionalProperties": false,
    })
}

fn resolve_session_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id containing the message."
            },
            "message_id": {
                "type": "string",
                "description": "wc_msg_* id returned by post_session_message."
            },
            "resolution": {
                "type": "string",
                "maxLength": 8000,
                "description": "Optional resolution note."
            }
        },
        "required": ["session_id", "message_id"],
        "additionalProperties": false,
    })
}

fn session_discussion_summary_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose message board should be summarized."
            },
            "limit": {
                "type": "integer",
                "maximum": 100,
                "description": "Maximum recent progress/decision messages to return. Defaults to 50 and is clamped to 100."
            }
        },
        "required": ["session_id"],
        "additionalProperties": false,
    })
}

fn start_session_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Optional runtime project id associated with this task."
            },
            "title": {
                "type": "string",
                "description": "Optional human-readable task title."
            },
            "mode": session_mode_schema("Optional session mode. Defaults to normal. read_only automatically blocks write-like and shell/job-like tools."),
            "deny_write_tools": {
                "type": "boolean",
                "description": "Optional task guard. When true, write-like tools such as apply_patch, write_project_file, replace_line_range, insert_at_line, and delete_line_range are blocked before execution."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "Optional task guard. When true, shell/job-like tools such as run_shell, run_job, run_codex, cargo_fmt, cargo_check, and cargo_test are blocked before execution."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

fn current_session_input_schema(require_session_id: bool) -> Value {
    let mut fields = vec![(
        "project",
        "string",
        "Runtime project id whose current session binding should be inspected or updated.",
        true,
    )];
    if require_session_id {
        fields.push((
            "session_id",
            "string",
            "Existing wc_sess_* id returned by start_session for this project.",
            true,
        ));
    }
    object_schema(fields)
}

const PATCH_FIELD_DESCRIPTION: &str = "raw standard unified diff only. Do not include Codex apply_patch wrapper syntax, shell heredocs, \"*** Begin Patch\", \"*** Update File\", or \"*** End Patch\". The first non-empty line should be \"diff --git ...\", \"--- ...\", or another git-apply-compatible unified diff header.";

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

fn tool_annotations(name: &str) -> Value {
    let metadata = tool_metadata(name);
    let read_only = metadata.read_only;
    let destructive = metadata.destructive;
    let open_world = metadata.shell_like;
    let idempotent = metadata.read_only;
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": idempotent,
        "openWorldHint": open_world,
    })
}

fn output_schema_for_tool(name: &str) -> Value {
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
                "matches",
                array_schema(
                    open_object_schema(
                        "Search match with path, 1-based line, preview, and optional context lines.",
                    ),
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

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list_tools".to_string(),
                description: "List tools exposed by this WebCodex runtime.".to_string(),
                input_schema: object_schema(vec![]),
                output_schema: output_schema_for_tool("list_tools"),
                annotations: tool_annotations("list_tools"),
            },
            ToolSpec {
                name: "start_session".to_string(),
                description: "Start an in-memory task tracking session. Read-only; creates bounded recorder metadata only and never modifies a project.".to_string(),
                input_schema: start_session_input_schema(),
                output_schema: output_schema_for_tool("start_session"),
                annotations: tool_annotations("start_session"),
            },
            ToolSpec {
                name: "session_summary".to_string(),
                description: "Return a bounded structured summary of tool calls recorded for a session. In-memory only; events are lost on restart.".to_string(),
                input_schema: object_schema(vec![
                    ("session_id", "string", "Opaque session id returned by start_session.", true),
                    ("limit", "integer", "Maximum recent events to return, capped by the runtime.", false),
                ]),
                output_schema: output_schema_for_tool("session_summary"),
                annotations: tool_annotations("session_summary"),
            },
            ToolSpec {
                name: "post_session_message".to_string(),
                description: "Post a bounded session-local message for collaboration, progress, user guidance, or design discussion. Metadata-only; does not modify project files. Guidance never overrides system/platform/WebCodex safety policy.".to_string(),
                input_schema: post_session_message_input_schema(),
                output_schema: output_schema_for_tool("post_session_message"),
                annotations: tool_annotations("post_session_message"),
            },
            ToolSpec {
                name: "list_session_messages".to_string(),
                description: "List bounded session-local messages in stable newest-first order, optionally filtered by kind and status.".to_string(),
                input_schema: list_session_messages_input_schema(),
                output_schema: output_schema_for_tool("list_session_messages"),
                annotations: tool_annotations("list_session_messages"),
            },
            ToolSpec {
                name: "resolve_session_message".to_string(),
                description: "Mark a session-local message resolved. Idempotent when the message is already resolved; metadata-only and never modifies project files.".to_string(),
                input_schema: resolve_session_message_input_schema(),
                output_schema: output_schema_for_tool("resolve_session_message"),
                annotations: tool_annotations("resolve_session_message"),
            },
            ToolSpec {
                name: "session_discussion_summary".to_string(),
                description: "Return a bounded structured aggregate of session-local discussion. Does not call an LLM or generate natural-language summaries.".to_string(),
                input_schema: session_discussion_summary_input_schema(),
                output_schema: output_schema_for_tool("session_discussion_summary"),
                annotations: tool_annotations("session_discussion_summary"),
            },
            ToolSpec {
                name: "bind_current_session".to_string(),
                description: "Bind an existing project-scoped session as the current session for this caller, transport, and project. Read-only control metadata; never modifies project files.".to_string(),
                input_schema: current_session_input_schema(true),
                output_schema: output_schema_for_tool("bind_current_session"),
                annotations: tool_annotations("bind_current_session"),
            },
            ToolSpec {
                name: "current_session".to_string(),
                description: "Return the current session binding for this caller, transport, and project, if a live binding exists.".to_string(),
                input_schema: current_session_input_schema(false),
                output_schema: output_schema_for_tool("current_session"),
                annotations: tool_annotations("current_session"),
            },
            ToolSpec {
                name: "unbind_current_session".to_string(),
                description: "Remove the current session binding for this caller, transport, and project. Idempotent and read-only.".to_string(),
                input_schema: current_session_input_schema(false),
                output_schema: output_schema_for_tool("unbind_current_session"),
                annotations: tool_annotations("unbind_current_session"),
            },
            ToolSpec {
                name: "list_projects".to_string(),
                description: "List agent-registered runtime projects and their execution mode."
                    .to_string(),
                input_schema: object_schema(vec![]),
                output_schema: output_schema_for_tool("list_projects"),
                annotations: tool_annotations("list_projects"),
            },
            ToolSpec {
                name: "register_project".to_string(),
                description: "Register an existing directory as a WebCodex project on a selected agent. "
                    .to_string()
                    + "Mutation with side effects; constrained by agent policy. The agent validates "
                    + "the path, writes projects_dir/<id>.toml atomically, and refreshes its "
                    + "project list. Requires Bearer auth.",
                input_schema: object_schema(vec![
                    ("client_id", "string", "Registered agent client_id.", true),
                    ("id", "string", "Project id (ASCII letters, digits, '-', '_'; no slash).", true),
                    ("name", "string", "Human-readable project name.", true),
                    ("path", "string", "Absolute directory path on the agent host.", true),
                    ("description", "string", "Optional project description.", false),
                    ("allow_patch", "boolean", "Allow patch operations on this project (default true).", false),
                    ("overwrite", "boolean", "Overwrite an existing project config file (default false).", false),
                ]),
                output_schema: output_schema_for_tool("register_project"),
                annotations: tool_annotations("register_project"),
            },
            ToolSpec {
                name: "create_project".to_string(),
                description: "Create a new directory on the selected agent and register it as a WebCodex "
                    .to_string()
                    + "project. Mutation with side effects; constrained by agent policy. Creates "
                    + "directory, optional template, optional git init, writes projects_dir/<id>.toml "
                    + "atomically. Requires Bearer auth.",
                input_schema: object_schema(vec![
                    ("client_id", "string", "Registered agent client_id.", true),
                    ("id", "string", "Project id (ASCII letters, digits, '-', '_'; no slash).", true),
                    ("name", "string", "Human-readable project name.", true),
                    ("path", "string", "Absolute directory path on the agent host.", true),
                    ("description", "string", "Optional project description.", false),
                    ("allow_patch", "boolean", "Allow patch operations on this project (default true).", false),
                    ("template", "string", "Template: 'empty' (default) or 'basic'.", false),
                    ("git_init", "boolean", "Initialize git in the new directory (default false).", false),
                    ("allow_existing_empty", "boolean", "Allow registering an existing empty directory (default false).", false),
                    ("overwrite", "boolean", "Overwrite an existing project config file (default false).", false),
                ]),
                output_schema: output_schema_for_tool("create_project"),
                annotations: tool_annotations("create_project"),
            },
            ToolSpec {
                name: "list_agents".to_string(),
                description: "List connected local/remote execution agents.".to_string(),
                input_schema: object_schema(vec![]),
                output_schema: output_schema_for_tool("list_agents"),
                annotations: tool_annotations("list_agents"),
            },
            ToolSpec {
                name: "runtime_status".to_string(),
                description: "Return a structured runtime health/observability summary (service "
                    .to_string()
                    + "metadata, projects config status, agent client summaries, and job counts). "
                    + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
                input_schema: object_schema(vec![]),
                output_schema: output_schema_for_tool("runtime_status"),
                annotations: tool_annotations("runtime_status"),
            },
            ToolSpec {
                name: "run_shell".to_string(),
                description: "Run checks, builds, tests, read-only diagnostics, or necessary commands in a project. "
                    .to_string()
                    + "Do not use as the primary project file editing path; prefer structured line edit tools for source edits.",
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("command", "string", "Shell command to run.", true),
                    (
                        "timeout_secs",
                        "integer",
                        "Command timeout in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("run_shell"),
                annotations: tool_annotations("run_shell"),
            },
            ToolSpec {
                name: "run_job".to_string(),
                description: "Start an asynchronous shell job inside an agent-registered project."
                    .to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    (
                        "command",
                        "string",
                        "Shell command to run asynchronously.",
                        true,
                    ),
                    (
                        "timeout_secs",
                        "integer",
                        "Maximum runtime in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("run_job"),
                annotations: tool_annotations("run_job"),
            },
            ToolSpec {
                name: "run_codex".to_string(),
                description: "Optional Codex CLI delegation as an async project job. Requires Codex CLI installed and configured on the owning agent. Use only when the user explicitly asks to delegate to Codex; otherwise use WebCodex file/git/shell/line-edit tools directly.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    (
                        "prompt",
                        "string",
                        "Instruction prompt passed to Codex CLI.",
                        true,
                    ),
                    (
                        "approval_mode",
                        "string",
                        "Codex approval mode. Empty/none/off/disabled omit --approval-mode.",
                        false,
                    ),
                    (
                        "timeout_secs",
                        "integer",
                        "Maximum runtime in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                    (
                        "extra_args",
                        "array",
                        "Optional extra Codex CLI arguments.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("run_codex"),
                annotations: tool_annotations("run_codex"),
            },
            ToolSpec {
                name: "job_status".to_string(),
                description: "Get status for a runtime job.".to_string(),
                input_schema: object_schema(vec![("job_id", "string", "Job id.", true)]),
                output_schema: output_schema_for_tool("job_status"),
                annotations: tool_annotations("job_status"),
            },
            ToolSpec {
                name: "job_log".to_string(),
                description: "Read stdout/stderr for a runtime job.".to_string(),
                input_schema: object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "offset",
                        "integer",
                        "Optional 1-based stdout line cursor.",
                        false,
                    ),
                    (
                        "tail_lines",
                        "integer",
                        "Optional number of trailing stdout lines to return.",
                        false,
                    ),
                ]),
                output_schema: output_schema_for_tool("job_log"),
                annotations: tool_annotations("job_log"),
            },
            ToolSpec {
                name: "list_project_files".to_string(),
                description: "List files in an agent-registered project directory (bounded, "
                    .to_string()
                    + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                    + "to the owning registered agent; the server never reads the agent project "
                    + "path directly.",
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    (
                        "path",
                        "string",
                        "Optional project-relative directory to list (default: project root).",
                        false,
                    ),
                    (
                        "limit",
                        "integer",
                        "Maximum number of entries to return.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("list_project_files"),
                annotations: tool_annotations("list_project_files"),
            },
            ToolSpec {
                name: "search_project_text".to_string(),
                description: "Search text inside an agent-registered project (bounded matches)."
                    .to_string()
                    + " Each match carries a project-relative path, 1-based line number, and a "
                    + "preview line. Sensitive/build directories (.git, target, node_modules) are "
                    + "excluded by default.",
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("pattern", "string", "Text pattern to search for.", true),
                    (
                        "path",
                        "string",
                        "Optional project-relative directory to scope the search (default: project root).",
                        false,
                    ),
                    (
                        "limit",
                        "integer",
                        "Maximum number of matches to return.",
                        false,
                    ),
                    (
                        "context_before",
                        "integer",
                        "Optional number of context lines before each match (clamped to 20).",
                        false,
                    ),
                    (
                        "context_after",
                        "integer",
                        "Optional number of context lines after each match (clamped to 20).",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("search_project_text"),
                annotations: tool_annotations("search_project_text"),
            },
            ToolSpec {
                name: "git_diff_summary".to_string(),
                description: "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                input_schema: object_schema(with_optional_session_id(vec![(
                    "project",
                    "string",
                    "Agent-registered project id.",
                    true,
                )])),
                output_schema: output_schema_for_tool("git_diff_summary"),
                annotations: tool_annotations("git_diff_summary"),
            },
            ToolSpec {
                name: "show_changes".to_string(),
                description: "Read-only git worktree and optional session activity summary for task review. Reports status, warnings, next actions, bounded hunks, and never modifies the worktree.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("session_id", "string", "Optional wc_sess_* id to summarize with the git changes.", false),
                    ("include_diff", "boolean", "Include bounded diff hunks (default false).", false),
                    ("max_hunks", "integer", "Maximum hunks to return when include_diff=true (clamped).", false),
                    ("max_hunk_lines", "integer", "Maximum lines per hunk when include_diff=true (clamped).", false),
                    ("session_event_limit", "integer", "Maximum recent session events to include (clamped).", false),
                ])),
                output_schema: output_schema_for_tool("show_changes"),
                annotations: tool_annotations("show_changes"),
            },
            ToolSpec {
                name: "list_jobs".to_string(),
                description: "List bounded runtime job summaries across agent and local executors. "
                    .to_string()
                    + "Never returns stdout/stderr bodies — only metadata (job_id, kind, status, "
                    + "project, timestamps, exit_code).",
                input_schema: object_schema(vec![
                    (
                        "limit",
                        "integer",
                        "Maximum number of job summaries to return.",
                        false,
                    ),
                    (
                        "status",
                        "string",
                        "Optional status filter (e.g. running, completed, failed).",
                        false,
                    ),
                ]),
                output_schema: output_schema_for_tool("list_jobs"),
                annotations: tool_annotations("list_jobs"),
            },
            ToolSpec {
                name: "job_tail".to_string(),
                description: "Return bounded stdout/stderr tails for a job.".to_string(),
                input_schema: object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "tail_lines",
                        "integer",
                        "Optional number of trailing lines to return per stream.",
                        false,
                    ),
                ]),
                output_schema: output_schema_for_tool("job_tail"),
                annotations: tool_annotations("job_tail"),
            },
            ToolSpec {
                name: "read_file".to_string(),
                description: "Read a UTF-8 file from an agent-registered project.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based line offset.", false),
                    ("limit", "integer", "Maximum line count.", false),
                    (
                        "with_line_numbers",
                        "boolean",
                        "When true, include numbered_text and lines with 1-based line numbers.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("read_file"),
                annotations: tool_annotations("read_file"),
            },
            ToolSpec {
                name: "git_status".to_string(),
                description: "Run git status --porcelain for a project.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![(
                    "project",
                    "string",
                    "Configured project id.",
                    true,
                )])),
                output_schema: output_schema_for_tool("git_status"),
                annotations: tool_annotations("git_status"),
            },
            ToolSpec {
                name: "git_diff".to_string(),
                description: "Run git diff for a project, optionally scoped to paths.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("args", "array", "Optional path list.", false),
                ])),
                output_schema: output_schema_for_tool("git_diff"),
                annotations: tool_annotations("git_diff"),
            },
            ToolSpec {
                name: "git_diff_hunks".to_string(),
                description: "Return bounded structured git diff hunks for review. Supports optional paths and cached diff; does not modify the worktree.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Optional project-relative paths to scope diff.", false),
                    ("max_hunks", "integer", "Maximum hunks to return (clamped).", false),
                    ("max_hunk_lines", "integer", "Maximum lines per hunk (clamped).", false),
                    ("cached", "boolean", "Use staged diff via git diff --cached.", false),
                ])),
                output_schema: output_schema_for_tool("git_diff_hunks"),
                annotations: tool_annotations("git_diff_hunks"),
            },
            ToolSpec {
                name: "git_log".to_string(),
                description: "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("limit", "integer", "Maximum commits to return (default 20, clamped to 1..100).", false),
                    ("skip", "integer", "Number of recent commits to skip (default 0, clamped to 0..10000).", false),
                ])),
                output_schema: output_schema_for_tool("git_log"),
                annotations: tool_annotations("git_log"),
            },
            ToolSpec {
                name: "cargo_fmt".to_string(),
                description: "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("cwd", "string", "Optional project-relative working directory.", false),
                    ("check", "boolean", "Run cargo fmt -- --check instead of formatting.", false),
                    ("timeout_secs", "integer", "Command timeout in seconds.", false),
                ])),
                output_schema: output_schema_for_tool("cargo_fmt"),
                annotations: tool_annotations("cargo_fmt"),
            },
            ToolSpec {
                name: "cargo_check".to_string(),
                description: "Run structured cargo check. Defaults to --all-targets; supports features/package/cwd without shell interpolation.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("cwd", "string", "Optional project-relative working directory.", false),
                    ("all_targets", "boolean", "Include --all-targets (default true).", false),
                    ("all_features", "boolean", "Include --all-features.", false),
                    ("no_default_features", "boolean", "Include --no-default-features.", false),
                    ("features", "string", "Feature list passed to --features.", false),
                    ("package", "string", "Package passed to -p.", false),
                    ("timeout_secs", "integer", "Command timeout in seconds.", false),
                ])),
                output_schema: output_schema_for_tool("cargo_check"),
                annotations: tool_annotations("cargo_check"),
            },
            ToolSpec {
                name: "cargo_test".to_string(),
                description: "Run structured cargo test. Supports optional filter, feature flags, package, --no-run, and bounded output tails.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("cwd", "string", "Optional project-relative working directory.", false),
                    ("filter", "string", "Optional cargo test filter.", false),
                    ("all_targets", "boolean", "Include --all-targets.", false),
                    ("all_features", "boolean", "Include --all-features.", false),
                    ("no_default_features", "boolean", "Include --no-default-features.", false),
                    ("features", "string", "Feature list passed to --features.", false),
                    ("package", "string", "Package passed to -p.", false),
                    ("no_run", "boolean", "Include --no-run.", false),
                    ("timeout_secs", "integer", "Command timeout in seconds.", false),
                ])),
                output_schema: output_schema_for_tool("cargo_test"),
                annotations: tool_annotations("cargo_test"),
            },
            ToolSpec {
                name: "apply_patch".to_string(),
                description: "Apply a unified diff patch to an agent-registered project."
                    .to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                ])),
                output_schema: output_schema_for_tool("apply_patch"),
                annotations: tool_annotations("apply_patch"),
            },
            ToolSpec {
                name: "apply_patch_checked".to_string(),
                description: "Validate/apply a unified diff and return a diff summary. Best for broad or multi-file patches; for local line edits prefer structured line edit tools.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings before applying.", false),
                ])),
                output_schema: output_schema_for_tool("apply_patch_checked"),
                annotations: tool_annotations("apply_patch_checked"),
            },
            ToolSpec {
                name: "delete_project_files".to_string(),
                description: "Delete selected project-relative files only; safer than arbitrary rm for cleanup.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative file paths to delete.", true),
                ])),
                output_schema: output_schema_for_tool("delete_project_files"),
                annotations: tool_annotations("delete_project_files"),
            },
            ToolSpec {
                name: "git_restore_paths".to_string(),
                description: "Restore selected tracked paths with git restore; does not remove untracked files.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative tracked paths to restore.", true),
                ])),
                output_schema: output_schema_for_tool("git_restore_paths"),
                annotations: tool_annotations("git_restore_paths"),
            },
            ToolSpec {
                name: "discard_untracked".to_string(),
                description: "Discard selected untracked files with git clean -f -- <paths>.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative untracked paths to remove.", true),
                ])),
                output_schema: output_schema_for_tool("discard_untracked"),
                annotations: tool_annotations("discard_untracked"),
            },
            ToolSpec {
                name: "validate_patch".to_string(),
                description: "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings.", false),
                ])),
                output_schema: output_schema_for_tool("validate_patch"),
                annotations: tool_annotations("validate_patch"),
            },
            ToolSpec {
                name: "replace_in_file".to_string(),
                description: "Replace a short unique substring in a project file. Good for small exact text changes; not for large source rewrites. Fails without writing when old is missing or ambiguous.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("old", "string", "Non-empty substring to replace.", true),
                    ("new", "string", "Replacement string.", true),
                    (
                        "expected_replacements",
                        "integer",
                        "Expected occurrence count (default 1).",
                        false,
                    ),
                    (
                        "allow_multiple",
                        "boolean",
                        "Allow replacing multiple occurrences (default false).",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("replace_in_file"),
                annotations: tool_annotations("replace_in_file"),
            },
            ToolSpec {
                name: "replace_exact_block".to_string(),
                description: "Replace literal UTF-8 text that matches exactly once; no regex or auto-format. Use line edit tools when line numbers are known.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("old_text", "string", "Non-empty literal block; must match exactly once.", true),
                    ("new_text", "string", "Replacement text; may be empty to delete the block.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for current whole-file content.", false),
                ])),
                output_schema: output_schema_for_tool("replace_exact_block"),
                annotations: tool_annotations("replace_exact_block"),
            },
            ToolSpec {
                name: "insert_before_pattern".to_string(),
                description: "Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("pattern", "string", "Non-empty literal pattern; must match exactly once.", true),
                    ("text", "string", "Non-empty text to insert, including intended newlines.", true),
                ])),
                output_schema: output_schema_for_tool("insert_before_pattern"),
                annotations: tool_annotations("insert_before_pattern"),
            },
            ToolSpec {
                name: "insert_after_pattern".to_string(),
                description: "Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("pattern", "string", "Non-empty literal pattern; must match exactly once.", true),
                    ("text", "string", "Non-empty text to insert, including intended newlines.", true),
                ])),
                output_schema: output_schema_for_tool("insert_after_pattern"),
                annotations: tool_annotations("insert_after_pattern"),
            },
            ToolSpec {
                name: "write_project_file".to_string(),
                description: "Create a new UTF-8 file or deliberately overwrite a small whole file. Not the first choice for ordinary local source edits; prefer line edit tools when scoped by line.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("content", "string", "UTF-8 file content (no NUL).", true),
                    (
                        "overwrite",
                        "boolean",
                        "Allow overwriting an existing file (default false).",
                        false,
                    ),
                    (
                        "expected_sha256",
                        "string",
                        "Required sha256 of the current file when overwriting.",
                        false,
                    ),
                    (
                        "expected_content_prefix",
                        "string",
                        "Required prefix of the current file when overwriting.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("write_project_file"),
                annotations: tool_annotations("write_project_file"),
            },
            ToolSpec {
                name: "save_project_artifact".to_string(),
                description: "Write a bounded binary project artifact from base64. Use for imported session files, generated images, PDFs, and zip files; not for UTF-8 source edits.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path.", true),
                    ("content_base64", "string", "Base64-encoded binary content.", true),
                    ("mime_type", "string", "Optional MIME type.", false),
                    ("overwrite", "boolean", "Allow overwriting an existing file (default false).", false),
                ])),
                output_schema: output_schema_for_tool("save_project_artifact"),
                annotations: tool_annotations("save_project_artifact"),
            },
            ToolSpec {
                name: "read_project_artifact_metadata".to_string(),
                description: "Read bounded metadata for a binary artifact; images include dimensions and zip archives are counted but never extracted.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative artifact path.", true),
                ])),
                output_schema: output_schema_for_tool("read_project_artifact_metadata"),
                annotations: tool_annotations("read_project_artifact_metadata"),
            },
            ToolSpec {
                name: "read_project_artifact".to_string(),
                description: "Chunked content read for a project artifact. Returns base64 for one small segment plus full-file sha256/MIME metadata; not a large-file transfer tool.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative artifact path.", true),
                    (
                        "encoding",
                        "string",
                        "Optional encoding; only base64 is supported (default base64).",
                        false,
                    ),
                    (
                        "offset",
                        "integer",
                        "Optional byte offset to start reading from; defaults to 0.",
                        false,
                    ),
                    (
                        "length",
                        "integer",
                        "Optional chunk length in bytes; defaults to 32768 and cannot exceed 65536.",
                        false,
                    ),
                    (
                        "max_bytes",
                        "integer",
                        "Compatibility alias/upper bound for length; cannot exceed 65536.",
                        false,
                    ),
                ])),
                output_schema: output_schema_for_tool("read_project_artifact"),
                annotations: tool_annotations("read_project_artifact"),
            },
            ToolSpec {
                name: "replace_line_range".to_string(),
                description: "Preferred source-code edit tool for local changes with clear line numbers. Replaces a 1-based inclusive line range; safer than run_shell/sed/perl/python and better than write_project_file for medium edits. Supports sha256/prefix guards.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based inclusive start line.", true),
                    ("end_line", "integer", "1-based inclusive end line.", true),
                    ("new_text", "string", "Replacement text; empty deletes the range.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for the original range text.", false),
                    ("expected_old_prefix", "string", "Optional prefix guard for the original range text.", false),
                ])),
                output_schema: output_schema_for_tool("replace_line_range"),
                annotations: tool_annotations("replace_line_range"),
            },
            ToolSpec {
                name: "insert_at_line".to_string(),
                description: "Preferred source-code edit tool for local changes with clear line numbers. Inserts before a specified 1-based line; safer than run_shell/sed/perl/python and better than write_project_file for medium edits. Supports sha256/prefix guards.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("line", "integer", "1-based insertion line; total_lines+1 appends at EOF.", true),
                    ("text", "string", "Text to insert.", true),
                    ("expected_anchor_sha256", "string", "Optional sha256 guard for anchor line or empty EOF anchor.", false),
                    ("expected_anchor_prefix", "string", "Optional prefix guard for anchor line or empty EOF anchor.", false),
                ])),
                output_schema: output_schema_for_tool("insert_at_line"),
                annotations: tool_annotations("insert_at_line"),
            },
            ToolSpec {
                name: "delete_line_range".to_string(),
                description: "Preferred source-code edit tool for local changes with clear line numbers. Deletes a 1-based inclusive line range; safer than run_shell/sed/perl/python and better than write_project_file for medium edits. Supports sha256/prefix guards.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based inclusive start line.", true),
                    ("end_line", "integer", "1-based inclusive end line.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for the original range text.", false),
                    ("expected_old_prefix", "string", "Optional prefix guard for the original range text.", false),
                ])),
                output_schema: output_schema_for_tool("delete_line_range"),
                annotations: tool_annotations("delete_line_range"),
            },
        ]
    }

    /// The sorted list of accepted runtime tool names (mirrors `tool_specs`).
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_specs().iter().map(|s| s.name.clone()).collect()
    }

    /// Group every accepted tool name into coarse categories so a custom GPT
    /// can pick the right tool family at a glance. A tool may appear in more
    /// than one category. Returned as a JSON object keyed by category.
    pub fn tool_categories(&self) -> Value {
        let names = self.tool_names();
        let pick = |set: &[&str]| -> Vec<String> {
            set.iter()
                .filter(|n| names.iter().any(|x| x == **n))
                .map(|s| s.to_string())
                .collect()
        };
        json!({
            "inspect": pick(&[
                "list_tools", "list_projects", "list_agents", "runtime_status",
                "read_file", "list_project_files", "search_project_text",
                "git_status", "git_diff", "git_diff_summary", "git_diff_hunks", "git_log",
                "show_changes"
            ]),
            "projects": pick(&["list_projects", "register_project", "create_project"]),
            "git": pick(&[
                "git_status", "git_diff", "git_diff_summary", "git_diff_hunks", "git_log",
                "show_changes",
                "git_restore_paths", "discard_untracked"
            ]),
            "review": pick(&[
                "show_changes", "git_diff_hunks", "git_diff_summary", "git_log", "git_status", "git_diff"
            ]),
            "validation": pick(&[
                "cargo_fmt", "cargo_check", "cargo_test", "validate_patch"
            ]),
            "patch": pick(&["apply_patch", "apply_patch_checked", "validate_patch"]),
            "edit": pick(&[
                "replace_in_file", "replace_exact_block",
                "insert_before_pattern", "insert_after_pattern",
                "write_project_file", "save_project_artifact",
                "read_project_artifact_metadata", "read_project_artifact",
                "replace_line_range", "insert_at_line", "delete_line_range",
                "apply_patch_checked"
            ]),
            "shell": pick(&["run_shell", "run_job", "cargo_fmt", "cargo_check", "cargo_test"]),
            "jobs": pick(&[
                "run_codex", "run_job", "job_status", "job_log",
                "list_jobs", "job_tail"
            ]),
            "runtime": pick(&[
                "list_tools", "start_session", "session_summary",
                "post_session_message", "list_session_messages",
                "resolve_session_message", "session_discussion_summary",
                "bind_current_session", "current_session", "unbind_current_session",
                "list_projects", "list_agents", "runtime_status"
            ]),
            "cleanup": pick(&[
                "delete_project_files", "git_restore_paths", "discard_untracked"
            ]),
        })
    }

    /// Short, GPT-facing flow hints. Each entry is well under the 300-char
    /// ToolSpec/operation description budget.
    pub fn recommended_flows() -> Vec<&'static str> {
        vec![
            "Discovery: call list_projects then runtime_status to see agents and projects.",
            "Inspect: use show_changes or git_diff_summary, then read_file before proposing changes.",
            "Review: use show_changes first; request include_diff=true or call git_diff_hunks for bounded hunk review, then run focused tests.",
            "Source code edit: inspect with read_file/search_project_text; prefer replace_line_range/insert_at_line/delete_line_range with guards. apply_patch_checked for broad diffs; run_shell not primary; validate with cargo tools.",
            "Rust validation: use cargo_fmt, cargo_check, cargo_test before run_shell for common checks.",
            "Patch: call validate_patch to dry-run, then apply_patch_checked to apply safely.",
            "Cleanup: use delete_project_files / git_restore_paths / discard_untracked instead of ad hoc rm.",
            "Codex: run_codex is optional delegation only when explicitly requested and Codex CLI is configured; otherwise use direct WebCodex tools.",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::RuntimeInfo;
    use super::*;
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "projects not configured for test".to_string(),
                "test".to_string(),
            )),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    #[test]
    fn tool_specs_patch_fields_reject_codex_wrapper() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for tool in ["apply_patch", "apply_patch_checked", "validate_patch"] {
            let spec = specs
                .iter()
                .find(|spec| spec.name == tool)
                .unwrap_or_else(|| panic!("missing tool spec: {tool}"));
            let description = spec.input_schema["properties"]["patch"]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("missing patch description for {tool}"));
            assert!(
                description.contains("raw standard unified diff"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("Codex apply_patch wrapper"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("*** Begin Patch"),
                "{tool}: {description}"
            );
        }
    }
}
