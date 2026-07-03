use serde_json::{json, Value};

use super::super::types::{is_model_hidden_tool_name, ToolSpec};
use super::super::ToolRuntime;
use super::input_schemas::{
    apply_text_edits_input_schema, checkpoint_create_input_schema, checkpoint_project_input_schema,
    current_session_input_schema, finish_coding_task_input_schema,
    list_session_messages_input_schema, post_session_message_input_schema,
    resolve_session_message_input_schema, session_discussion_summary_input_schema,
    session_handoff_summary_input_schema, start_coding_task_input_schema,
    start_session_input_schema, with_optional_session_id, workspace_hygiene_check_input_schema,
    PATCH_FIELD_DESCRIPTION,
};
use super::{object_schema, output_schema_for_tool, tool_annotations};

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let specs = vec![
            ToolSpec {
                name: "list_tools".to_string(),
                description: "List tools exposed by this WebCodex runtime.".to_string(),
                input_schema: object_schema(vec![]),
                output_schema: output_schema_for_tool("list_tools"),
                annotations: tool_annotations("list_tools"),
            },
            ToolSpec {
                name: "start_session".to_string(),
                description: "Create a bounded task tracking session and return its explicit wc_sess_* session_id. Read-only; records session ledger metadata where persistence is configured, never modifies a project, and does not by itself bind future calls as current.".to_string(),
                input_schema: start_session_input_schema(),
                output_schema: output_schema_for_tool("start_session"),
                annotations: tool_annotations("start_session"),
            },
            ToolSpec {
                name: "start_coding_task".to_string(),
                description: "Deterministic coding-task startup aggregate. Requires project, creates a session, returns explicit session_id, project resolution, optional runtime/git/rules context, recommended flow, warnings, and current binding state. Never calls an LLM; bind_current defaults false.".to_string(),
                input_schema: start_coding_task_input_schema(),
                output_schema: output_schema_for_tool("start_coding_task"),
                annotations: tool_annotations("start_coding_task"),
            },
            ToolSpec {
                name: "finish_coding_task".to_string(),
                description: "Deterministic coding-task finish aggregate for an explicit session_id. Returns show_changes, optional hygiene and handoff, validation-like ledger events, workspace warnings, and dirty-state signals. Never calls an LLM, emits raw stdout/stderr, or infers validation root causes.".to_string(),
                input_schema: finish_coding_task_input_schema(),
                output_schema: output_schema_for_tool("finish_coding_task"),
                annotations: tool_annotations("finish_coding_task"),
            },
            ToolSpec {
                name: "session_summary".to_string(),
                description: "Return a bounded structured summary from the session ledger for an explicit session_id: recorded events, message-board summary, task mode, and guards. Uses durable ledger data where session persistence is configured; does not rely on current-session binding.".to_string(),
                input_schema: object_schema(vec![
                    ("session_id", "string", "Opaque session id returned by start_session.", true),
                    ("limit", "integer", "Maximum recent events to return, capped by the runtime.", false),
                ]),
                output_schema: output_schema_for_tool("session_summary"),
                annotations: tool_annotations("session_summary"),
            },
            ToolSpec {
                name: "post_session_message".to_string(),
                description: "Post a bounded session-local message into the recorded session ledger for collaboration, progress, user guidance, or design discussion. Metadata-only; does not modify project files. Guidance never overrides system/platform/WebCodex safety policy.".to_string(),
                input_schema: post_session_message_input_schema(),
                output_schema: output_schema_for_tool("post_session_message"),
                annotations: tool_annotations("post_session_message"),
            },
            ToolSpec {
                name: "list_session_messages".to_string(),
                description: "List bounded session-local messages from the recorded session ledger in stable newest-first order, optionally filtered by kind and status.".to_string(),
                input_schema: list_session_messages_input_schema(),
                output_schema: output_schema_for_tool("list_session_messages"),
                annotations: tool_annotations("list_session_messages"),
            },
            ToolSpec {
                name: "resolve_session_message".to_string(),
                description: "Mark a session-local ledger message resolved. Idempotent when the message is already resolved; metadata-only and never modifies project files.".to_string(),
                input_schema: resolve_session_message_input_schema(),
                output_schema: output_schema_for_tool("resolve_session_message"),
                annotations: tool_annotations("resolve_session_message"),
            },
            ToolSpec {
                name: "session_discussion_summary".to_string(),
                description: "Return a bounded structured aggregate of session-local discussion from the recorded session ledger. Does not call an LLM or generate natural-language summaries.".to_string(),
                input_schema: session_discussion_summary_input_schema(),
                output_schema: output_schema_for_tool("session_discussion_summary"),
                annotations: tool_annotations("session_discussion_summary"),
            },
            ToolSpec {
                name: "session_handoff_summary".to_string(),
                description: "Read-only handoff for multi-step tasks, explicit session_id. Returns session ledger msgs, failed tools, ledger-derived validation, workspace/checkpoints. Diagnostics need bounded tails or safe result metadata; validation.parser.available false if missing. Does not depend on current-session binding.".to_string(),
                input_schema: session_handoff_summary_input_schema(),
                output_schema: output_schema_for_tool("session_handoff_summary"),
                annotations: tool_annotations("session_handoff_summary"),
            },
            ToolSpec {
                name: "workspace_hygiene_check".to_string(),
                description: "Default pre-final workspace hygiene review; read-only. Detects dirty worktree, untracked temp/smoke files, cache dirs, secret-like names, and large untracked files before validation or handoff. Never reads file contents.".to_string(),
                input_schema: workspace_hygiene_check_input_schema(),
                output_schema: output_schema_for_tool("workspace_hygiene_check"),
                annotations: tool_annotations("workspace_hygiene_check"),
            },
            ToolSpec {
                name: "bind_current_session".to_string(),
                description: "Bind an existing project-scoped session as the current session for this caller, transport, and project. This is process-local in-memory control metadata, not the durable session ledger, and may be lost on restart. Read-only; never modifies project files.".to_string(),
                input_schema: current_session_input_schema(true),
                output_schema: output_schema_for_tool("bind_current_session"),
                annotations: tool_annotations("bind_current_session"),
            },
            ToolSpec {
                name: "current_session".to_string(),
                description: "Return the process-local in-memory current-session binding for this caller, transport, and project, if a live binding exists. This is convenience control metadata, not the durable session ledger, and may be lost on restart.".to_string(),
                input_schema: current_session_input_schema(false),
                output_schema: output_schema_for_tool("current_session"),
                annotations: tool_annotations("current_session"),
            },
            ToolSpec {
                name: "unbind_current_session".to_string(),
                description: "Remove the process-local in-memory current-session binding for this caller, transport, and project. This only clears convenience control metadata, not the durable session ledger. Idempotent and read-only.".to_string(),
                input_schema: current_session_input_schema(false),
                output_schema: output_schema_for_tool("unbind_current_session"),
                annotations: tool_annotations("unbind_current_session"),
            },
            ToolSpec {
                name: "workspace_checkpoint_create".to_string(),
                description: "Create a bounded workspace checkpoint outside the project worktree. Captures HEAD, status, text diffs, and optional small untracked text files.".to_string(),
                input_schema: checkpoint_create_input_schema(),
                output_schema: output_schema_for_tool("workspace_checkpoint_create"),
                annotations: tool_annotations("workspace_checkpoint_create"),
            },
            ToolSpec {
                name: "workspace_checkpoint_list".to_string(),
                description: "List checkpoint metadata for a project without returning full diffs or saved file content.".to_string(),
                input_schema: checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("limit", "integer", "Maximum checkpoints to return (default 20, max 100).", false),
                ]),
                output_schema: output_schema_for_tool("workspace_checkpoint_list"),
                annotations: tool_annotations("workspace_checkpoint_list"),
            },
            ToolSpec {
                name: "workspace_checkpoint_show".to_string(),
                description: "Show bounded checkpoint metadata, file list, skipped files, and optional diff stat. Does not return full diff/content by default.".to_string(),
                input_schema: checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id returned by workspace_checkpoint_create.", true),
                    ("include_diff_stat", "boolean", "Include tracked/staged diff stat strings (default false).", false),
                ]),
                output_schema: output_schema_for_tool("workspace_checkpoint_show"),
                annotations: tool_annotations("workspace_checkpoint_show"),
            },
            ToolSpec {
                name: "workspace_checkpoint_restore".to_string(),
                description: "Restore a checkpoint after confirm=true. Requires matching HEAD and refuses unsafe current state rather than half-restoring.".to_string(),
                input_schema: checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id to restore.", true),
                    ("confirm", "boolean", "Must be true to restore.", true),
                ]),
                output_schema: output_schema_for_tool("workspace_checkpoint_restore"),
                annotations: tool_annotations("workspace_checkpoint_restore"),
            },
            ToolSpec {
                name: "workspace_checkpoint_delete".to_string(),
                description: "Delete one checkpoint JSON file after confirm=true. Does not touch the project worktree.".to_string(),
                input_schema: checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id to delete.", true),
                    ("confirm", "boolean", "Must be true to delete.", true),
                ]),
                output_schema: output_schema_for_tool("workspace_checkpoint_delete"),
                annotations: tool_annotations("workspace_checkpoint_delete"),
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
                name: "tool_manifest".to_string(),
                description: "Return a compact, bounded tool manifest with categories, risk "
                    .to_string()
                    + "summary, and recommended flows. Lightweight alternative to list_tools for "
                    + "long tasks. Read-only; never exposes schemas, tokens, or internal paths.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "Optional category filter (e.g. session, edit, git, checkpoint, runtime, job, validation)."
                        },
                        "include_recommended_flows": {
                            "type": "boolean",
                            "description": "Include recommended_flows in the output (default true)."
                        },
                        "include_risk_summary": {
                            "type": "boolean",
                            "description": "Include risk_summary in the output (default true)."
                        }
                    },
                    "required": [],
                    "additionalProperties": false,
                }),
                output_schema: output_schema_for_tool("tool_manifest"),
                annotations: tool_annotations("tool_manifest"),
            },
            ToolSpec {
                name: "run_shell".to_string(),
                description: "Bounded command escape hatch for validation, builds, tests, or diagnostics only. Do not use as the primary file editing path; prefer cargo_* / validate_patch for common checks and structured line edit tools for source edits.".to_string(),
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
                description: "Default inspect/search tool for project text (rg-first with grep fallback). Returns structured output: matches with path, 1-based line, preview/context, plus backend, truncated, count, context_before, and context_after.".to_string(),
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
                description: "Default inspect/review tool before final response. Read-only worktree plus optional session summary; reports status, warnings, next actions, and bounded hunks without modifying files.".to_string(),
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
                description: "Default inspect tool for targeted source reading. Reads bounded UTF-8 file ranges from an agent-registered project, optionally with 1-based line numbers for structured line edits.".to_string(),
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
                description: "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation; use before raw run_shell when applicable.".to_string(),
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
                description: "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails; use before raw run_shell when applicable.".to_string(),
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
                description: "Validated unified-diff edit tool for broad or multi-file patches. Returns a diff summary; for local line edits prefer replace_line_range, insert_at_line, delete_line_range, or apply_text_edits.".to_string(),
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
                description: "Literal pattern compatibility path for short exact replacements. Prefer replace_line_range, insert_at_line, or delete_line_range when line numbers are available; fails without writing when old text is missing or ambiguous.".to_string(),
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
                description: "Whole-file write compatibility path for new files or deliberate small overwrites. Prefer structured line edits or apply_text_edits for source changes; requires overwrite/guards for existing files.".to_string(),
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
                name: "artifact_upload_begin".to_string(),
                description: "Begin a bounded chunked binary artifact upload. Creates a project-local temporary upload session; finish commits atomically to the target path.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path.", true),
                    ("expected_bytes", "integer", "Optional final byte count guard.", false),
                    ("expected_sha256", "string", "Optional final sha256 guard.", false),
                    ("mime_type", "string", "Optional MIME type.", false),
                    ("overwrite", "boolean", "Allow overwriting an existing file at finish (default false).", false),
                ])),
                output_schema: output_schema_for_tool("artifact_upload_begin"),
                annotations: tool_annotations("artifact_upload_begin"),
            },
            ToolSpec {
                name: "artifact_upload_chunk".to_string(),
                description: "Append one base64 chunk to an active artifact upload. The caller must pass the expected offset; raw chunk content is not recorded in sessions.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path from begin.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                    ("offset", "integer", "Expected current upload byte offset.", true),
                    ("content_base64", "string", "Base64-encoded chunk; decoded chunk max is 65536 bytes.", true),
                ])),
                output_schema: output_schema_for_tool("artifact_upload_chunk"),
                annotations: tool_annotations("artifact_upload_chunk"),
            },
            ToolSpec {
                name: "artifact_upload_finish".to_string(),
                description: "Verify expected size/sha256 for an active artifact upload and atomically commit the temporary file to the target artifact path.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path from begin.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                ])),
                output_schema: output_schema_for_tool("artifact_upload_finish"),
                annotations: tool_annotations("artifact_upload_finish"),
            },
            ToolSpec {
                name: "artifact_upload_abort".to_string(),
                description: "Abort an active artifact upload and remove its project-local temporary file and sidecar; does not touch the final target path.".to_string(),
                input_schema: object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path from begin.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                ])),
                output_schema: output_schema_for_tool("artifact_upload_abort"),
                annotations: tool_annotations("artifact_upload_abort"),
            },
            ToolSpec {
                name: "replace_line_range".to_string(),
                description: "Preferred source-code edit tool for local line changes with clear line numbers. Replaces a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.".to_string(),
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
                description: "Preferred source-code edit tool for local line changes with clear line numbers. Inserts before a specified 1-based line; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.".to_string(),
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
                description: "Preferred source-code edit tool for local line changes with clear line numbers. Deletes a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.".to_string(),
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
            ToolSpec {
                name: "apply_text_edits".to_string(),
                description: "Preferred batch text edit tool for coordinated source changes in one UTF-8 file. Applies bounded exact replace/insert/delete edits atomically only when all matches validate as unique/non-overlapping. Supports dry_run and sha256 guard.".to_string(),
                input_schema: apply_text_edits_input_schema(),
                output_schema: output_schema_for_tool("apply_text_edits"),
                annotations: tool_annotations("apply_text_edits"),
            },
        ];
        specs
            .into_iter()
            .filter(|spec| !is_model_hidden_tool_name(&spec.name))
            .collect()
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
                "start_coding_task",
                "read_file", "search_project_text", "show_changes", "list_project_files",
                "git_status", "git_diff", "git_diff_summary", "git_diff_hunks", "git_log",
                "workspace_checkpoint_list", "workspace_checkpoint_show"
            ]),
            "projects": pick(&["list_projects", "register_project", "create_project"]),
            "git": pick(&[
                "git_status", "git_diff", "git_diff_summary", "git_diff_hunks", "git_log",
                "show_changes",
                "git_restore_paths", "discard_untracked",
                "workspace_checkpoint_create", "workspace_checkpoint_restore"
            ]),
            "review": pick(&[
                "finish_coding_task",
                "show_changes", "git_diff_hunks", "workspace_hygiene_check",
                "git_diff_summary", "git_log", "git_status", "git_diff",
                "workspace_checkpoint_show", "workspace_checkpoint_list"
            ]),
            "validation": pick(&[
                "cargo_fmt", "cargo_check", "cargo_test", "validate_patch",
                "apply_patch_checked"
            ]),
            "patch": pick(&["apply_patch", "apply_patch_checked", "validate_patch"]),
            "edit": pick(&[
                "replace_line_range", "insert_at_line", "delete_line_range",
                "apply_text_edits", "apply_patch_checked",
                "replace_in_file", "replace_exact_block",
                "insert_before_pattern", "insert_after_pattern",
                "write_project_file", "save_project_artifact",
                "read_project_artifact_metadata", "read_project_artifact",
                "artifact_upload_begin", "artifact_upload_chunk",
                "artifact_upload_finish", "artifact_upload_abort"
            ]),
            "shell": pick(&["cargo_fmt", "cargo_check", "cargo_test", "run_shell", "run_job"]),
            "jobs": pick(&[
                "run_job", "job_status", "job_log",
                "list_jobs", "job_tail"
            ]),
            "runtime": pick(&[
                "list_tools", "start_session", "start_coding_task", "finish_coding_task",
                "session_summary",
                "post_session_message", "list_session_messages",
                "resolve_session_message", "session_discussion_summary",
                "session_handoff_summary",
                "bind_current_session", "current_session", "unbind_current_session",
                "workspace_checkpoint_create", "workspace_checkpoint_list",
                "workspace_checkpoint_show", "workspace_checkpoint_restore",
                "workspace_checkpoint_delete",
                "list_projects", "list_agents", "runtime_status", "tool_manifest"
            ]),
            "cleanup": pick(&[
                "delete_project_files", "git_restore_paths", "discard_untracked",
                "workspace_checkpoint_delete"
            ]),
            "checkpoint": pick(&[
                "workspace_checkpoint_create", "workspace_checkpoint_list",
                "workspace_checkpoint_show", "workspace_checkpoint_restore",
                "workspace_checkpoint_delete"
            ]),
        })
    }

    /// Short, GPT-facing flow hints. Each entry is well under the 300-char
    /// ToolSpec/operation description budget.
    pub fn recommended_flows() -> Vec<&'static str> {
        vec![
            "Discovery: resolve project with list_projects/runtime_status, then load rules/context with read_file before editing.",
            "Inspect: use read_file, search_project_text, and show_changes before editing.",
            "Edit: prefer replace_line_range / insert_at_line / delete_line_range for local line edits; use apply_text_edits for batches; use apply_patch_checked for broad diffs.",
            "Validate: use cargo_check / cargo_test / validate_patch when applicable. raw run_shell is a bounded escape hatch, not the primary editing or validation path.",
            "Review: use show_changes / git_diff_hunks / workspace_hygiene_check before final response.",
            "Handoff: use session_summary / session_handoff_summary when a task spans multiple steps.",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use crate::tool_runtime::types::RuntimeInfo;
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
