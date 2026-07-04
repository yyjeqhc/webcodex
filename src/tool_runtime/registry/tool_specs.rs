use serde_json::{json, Value};

use super::super::tool_definition::{lookup_tool_definition, model_visible_tool_definitions};
use super::super::tool_spec::ToolSpec;
use super::super::ToolRuntime;
use super::input_schemas::{
    apply_text_edits_input_schema, checkpoint_create_input_schema, checkpoint_project_input_schema,
    current_session_input_schema, finish_coding_task_input_schema,
    list_session_messages_input_schema, post_session_message_input_schema,
    resolve_session_message_input_schema, session_discussion_summary_input_schema,
    session_handoff_summary_input_schema, start_coding_task_input_schema,
    start_session_input_schema, with_common_testing_metadata, with_optional_session_id,
    workspace_hygiene_check_input_schema, PATCH_FIELD_DESCRIPTION,
};
use super::{object_schema, output_schema_for_tool, tool_annotations};

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let declarations = vec![
            tool_spec(
                "list_tools",
                "List runtime tools. Full output includes schemas and may be large; use summary_only with category, features, or limit for bounded GPT Action discovery.",
                json!({
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "Optional tool_manifest category filter such as artifact, edit, session, git, or runtime."
                        },
                        "features": {
                            "type": "string",
                            "description": "Optional loose feature filter such as artifact_upload, upload, read, edit, session, git, or validation."
                        },
                        "summary_only": {
                            "type": "boolean",
                            "description": "When true, omit full input/output schemas and return compact tool summaries."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum returned tools for focused discovery; capped at 100."
                        }
                    },
                    "required": [],
                    "additionalProperties": false,
                }),
            ),
            tool_spec(
                "start_session",
                "Create a bounded task tracking session and return its explicit wc_sess_* session_id. Read-only; records session ledger metadata where persistence is configured, never modifies a project, and does not by itself bind future calls as current.",
                start_session_input_schema(),
            ),
            tool_spec(
                "start_coding_task",
                "Deterministic coding-task startup aggregate. Requires project, creates a session, returns explicit session_id, project resolution, optional runtime/git/rules context, recommended flow, warnings, and current binding state. Never calls an LLM; bind_current defaults false.",
                start_coding_task_input_schema(),
            ),
            tool_spec(
                "finish_coding_task",
                "Deterministic coding-task finish aggregate for an explicit session_id. Returns show_changes, optional hygiene and handoff, validation-like ledger events, workspace warnings, and dirty-state signals. Never calls an LLM, emits raw stdout/stderr, or infers validation root causes.",
                finish_coding_task_input_schema(),
            ),
            tool_spec(
                "session_summary",
                "Return a bounded structured summary from the session ledger for an explicit session_id: recorded events, message-board summary, task mode, and guards. Uses durable ledger data where session persistence is configured; does not rely on current-session binding.",
                object_schema(vec![
                    ("session_id", "string", "Opaque session id returned by start_session.", true),
                    ("limit", "integer", "Maximum recent events to return, capped by the runtime.", false),
                ]),
            ),
            tool_spec(
                "post_session_message",
                "Post a bounded session-local message into the recorded session ledger for collaboration, progress, user guidance, or design discussion. Metadata-only; does not modify project files. Guidance never overrides system/platform/WebCodex safety policy.",
                post_session_message_input_schema(),
            ),
            tool_spec(
                "list_session_messages",
                "List bounded session-local messages from the recorded session ledger in stable newest-first order, optionally filtered by kind and status.",
                list_session_messages_input_schema(),
            ),
            tool_spec(
                "resolve_session_message",
                "Mark a session-local ledger message resolved. Idempotent when the message is already resolved; metadata-only and never modifies project files.",
                resolve_session_message_input_schema(),
            ),
            tool_spec(
                "session_discussion_summary",
                "Return a bounded structured aggregate of session-local discussion from the recorded session ledger. Does not call an LLM or generate natural-language summaries.",
                session_discussion_summary_input_schema(),
            ),
            tool_spec(
                "session_handoff_summary",
                "Read-only handoff for multi-step tasks, explicit session_id. Returns session ledger msgs, failed tools, ledger-derived validation, workspace/checkpoints. Diagnostics need bounded tails or safe result metadata; validation.parser.available false if missing. Does not depend on current-session binding.",
                session_handoff_summary_input_schema(),
            ),
            tool_spec(
                "workspace_hygiene_check",
                "Default pre-final workspace hygiene review; read-only. Detects dirty worktree, untracked temp/smoke files, cache dirs, secret-like names, and large untracked files before validation or handoff. Never reads file contents.",
                workspace_hygiene_check_input_schema(),
            ),
            tool_spec(
                "bind_current_session",
                "Bind an existing project-scoped session as the current session for this caller, transport, and project. This is process-local in-memory control metadata, not the durable session ledger, and may be lost on restart. Read-only; never modifies project files.",
                current_session_input_schema(true),
            ),
            tool_spec(
                "current_session",
                "Return the process-local in-memory current-session binding for this caller, transport, and project, if a live binding exists. This is convenience control metadata, not the durable session ledger, and may be lost on restart.",
                current_session_input_schema(false),
            ),
            tool_spec(
                "unbind_current_session",
                "Remove the process-local in-memory current-session binding for this caller, transport, and project. This only clears convenience control metadata, not the durable session ledger. Idempotent and read-only.",
                current_session_input_schema(false),
            ),
            tool_spec(
                "workspace_checkpoint_create",
                "Create a bounded workspace checkpoint outside the project worktree. Captures HEAD, status, text diffs, and optional small untracked text files.",
                checkpoint_create_input_schema(),
            ),
            tool_spec(
                "workspace_checkpoint_list",
                "List checkpoint metadata for a project without returning full diffs or saved file content.",
                checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("limit", "integer", "Maximum checkpoints to return (default 20, max 100).", false),
                ]),
            ),
            tool_spec(
                "workspace_checkpoint_show",
                "Show bounded checkpoint metadata, file list, skipped files, and optional diff stat. Does not return full diff/content by default.",
                checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id returned by workspace_checkpoint_create.", true),
                    ("include_diff_stat", "boolean", "Include tracked/staged diff stat strings (default false).", false),
                ]),
            ),
            tool_spec(
                "workspace_checkpoint_restore",
                "Restore a checkpoint after confirm=true. Requires matching HEAD and refuses unsafe current state rather than half-restoring.",
                checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id to restore.", true),
                    ("confirm", "boolean", "Must be true to restore.", true),
                ]),
            ),
            tool_spec(
                "workspace_checkpoint_delete",
                "Delete one checkpoint JSON file after confirm=true. Does not touch the project worktree.",
                checkpoint_project_input_schema(vec![
                    ("project", "string", "Runtime project id.", true),
                    ("checkpoint_id", "string", "wc_ckpt_* id to delete.", true),
                    ("confirm", "boolean", "Must be true to delete.", true),
                ]),
            ),
            tool_spec(
                "list_projects",
                "List agent-registered runtime projects, execution mode, and smoke-selection capabilities such as git_available and recommended_for_smoke.",
                object_schema(vec![]),
            ),
            tool_spec(
                "register_project",
                "Register an existing directory as a WebCodex project on a selected agent. "
                    .to_string()
                    + "Mutation with side effects; constrained by agent policy. The agent validates "
                    + "the path, writes projects_dir/<id>.toml atomically, and refreshes its "
                    + "project list. Requires Bearer auth.",
                object_schema(vec![
                    ("client_id", "string", "Registered agent client_id.", true),
                    ("id", "string", "Project id (ASCII letters, digits, '-', '_'; no slash).", true),
                    ("name", "string", "Human-readable project name.", true),
                    ("path", "string", "Absolute directory path on the agent host.", true),
                    ("description", "string", "Optional project description.", false),
                    ("allow_patch", "boolean", "Allow patch operations on this project (default true).", false),
                    ("overwrite", "boolean", "Overwrite an existing project config file (default false).", false),
                ]),
            ),
            tool_spec(
                "create_project",
                "Create a new directory on the selected agent and register it as a WebCodex "
                    .to_string()
                    + "project. Mutation with side effects; constrained by agent policy. Creates "
                    + "directory, optional template, optional git init, writes projects_dir/<id>.toml "
                    + "atomically. Requires Bearer auth.",
                object_schema(vec![
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
            ),
            tool_spec(
                "list_agents",
                "List connected local/remote execution agents.",
                object_schema(vec![]),
            ),
            tool_spec(
                "runtime_status",
                "Return a structured runtime health/observability summary (service "
                    .to_string()
                    + "metadata, projects config status, agent client summaries, and job counts). "
                    + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
                object_schema(vec![]),
            ),
            tool_spec(
                "tool_manifest",
                "Return a compact, bounded tool manifest with categories, accepted flattened args, risk "
                    .to_string()
                    + "summary, and recommended flows. Lightweight alternative to list_tools for "
                    + "long tasks. Read-only; never exposes schemas, tokens, or internal paths.",
                json!({
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
            ),
            tool_spec(
                "run_shell",
                "Bounded command escape hatch for validation, builds, tests, or diagnostics only. Do not use as the primary file editing path; prefer cargo_* / validate_patch for common checks and structured line edit tools for source edits.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "run_job",
                "Start an asynchronous shell job inside an agent-registered project.".to_string(),
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "stop_job",
                "Stop a bounded runtime job started through WebCodex. Requires confirm=true, obeys project/session ownership, never exposes stdout/stderr, and returns stop_effect/terminal lifecycle fields.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id that must match the job project.", true),
                    ("job_id", "string", "Runtime job id returned by run_job.", true),
                    (
                        "confirm",
                        "boolean",
                        "Must be true to stop or no-op an already-finished job; false returns confirmation_required.",
                        false,
                    ),
                ])),
            ),
            tool_spec(
                "run_codex",
                "Optional Codex CLI delegation as an async project job. Requires Codex CLI installed and configured on the owning agent. Use only when the user explicitly asks to delegate to Codex; otherwise use WebCodex file/git/shell/line-edit tools directly.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "job_status",
                "Get bounded lifecycle status for a runtime job. Omits command_preview by default and never returns stdout/stderr bodies.",
                object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "include_command_preview",
                        "boolean",
                        "Optional debug flag. Defaults to false; when true, includes bounded command_preview metadata. stdout/stderr bodies are never included.",
                        false,
                    ),
                ]),
            ),
            tool_spec(
                "job_log",
                "Read stdout/stderr for a runtime job.",
                object_schema(vec![
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
            ),
            tool_spec(
                "list_project_files",
                "List files in an agent-registered project directory (bounded, "
                    .to_string()
                    + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                    + "to the owning registered agent; the server never reads the agent project "
                    + "path directly.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "search_project_text",
                "Default inspect/search tool for project text (rg-first with grep fallback). Returns structured output: matches with path, 1-based line, preview/context, plus backend, truncated, count, context_before, and context_after.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "git_diff_summary",
                "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                object_schema(with_optional_session_id(vec![(
                    "project",
                    "string",
                    "Agent-registered project id.",
                    true,
                )])),
            ),
            tool_spec(
                "show_changes",
                "Default inspect/review tool before final response. Read-only worktree plus optional session summary; reports status, warnings, next actions, and bounded hunks without modifying files.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("session_id", "string", "Optional wc_sess_* id to summarize with the git changes.", false),
                    ("include_diff", "boolean", "Include bounded diff hunks (default false).", false),
                    ("max_hunks", "integer", "Maximum hunks to return when include_diff=true (clamped).", false),
                    ("max_hunk_lines", "integer", "Maximum lines per hunk when include_diff=true (clamped).", false),
                    ("session_event_limit", "integer", "Maximum recent session events to include (clamped).", false),
                ])),
            ),
            tool_spec(
                "list_jobs",
                "List bounded runtime job summaries across agent and local executors. "
                    .to_string()
                    + "Never returns stdout/stderr bodies — only metadata (job_id, kind, status, "
                    + "project, timestamps, exit_code).",
                object_schema(vec![
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
            ),
            tool_spec(
                "job_tail",
                "Return bounded stdout/stderr tails for a job.",
                object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "tail_lines",
                        "integer",
                        "Optional number of trailing lines to return per stream.",
                        false,
                    ),
                ]),
            ),
            tool_spec(
                "read_file",
                "Default inspect tool for targeted source reading. Reads bounded UTF-8 file ranges from an agent-registered project, optionally with 1-based line numbers for structured line edits.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "git_status",
                "Run git status --porcelain for a project.",
                object_schema(with_optional_session_id(vec![(
                    "project",
                    "string",
                    "Configured project id.",
                    true,
                )])),
            ),
            tool_spec(
                "git_diff",
                "Run git diff for a project, optionally scoped to paths.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("args", "array", "Optional path list.", false),
                ])),
            ),
            tool_spec(
                "git_diff_hunks",
                "Return bounded structured git diff hunks for review. Supports optional paths and cached diff; does not modify the worktree.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Optional project-relative paths to scope diff.", false),
                    ("max_hunks", "integer", "Maximum hunks to return (clamped).", false),
                    ("max_hunk_lines", "integer", "Maximum lines per hunk (clamped).", false),
                    ("cached", "boolean", "Use staged diff via git diff --cached.", false),
                ])),
            ),
            tool_spec(
                "git_log",
                "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("limit", "integer", "Maximum commits to return (default 20, clamped to 1..100).", false),
                    ("skip", "integer", "Number of recent commits to skip (default 0, clamped to 0..10000).", false),
                ])),
            ),
            tool_spec(
                "cargo_fmt",
                "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("cwd", "string", "Optional project-relative working directory.", false),
                    ("check", "boolean", "Run cargo fmt -- --check instead of formatting.", false),
                    ("timeout_secs", "integer", "Command timeout in seconds.", false),
                ])),
            ),
            tool_spec(
                "cargo_check",
                "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation; use before raw run_shell when applicable.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("cwd", "string", "Optional project-relative working directory.", false),
                    ("all_targets", "boolean", "Include --all-targets (default true).", false),
                    ("all_features", "boolean", "Include --all-features.", false),
                    ("no_default_features", "boolean", "Include --no-default-features.", false),
                    ("features", "string", "Feature list passed to --features.", false),
                    ("package", "string", "Package passed to -p.", false),
                    ("timeout_secs", "integer", "Command timeout in seconds.", false),
                ])),
            ),
            tool_spec(
                "cargo_test",
                "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails; use before raw run_shell when applicable.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "apply_patch",
                "Apply a unified diff patch to an agent-registered project.".to_string(),
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Configured project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                ])),
            ),
            tool_spec(
                "apply_patch_checked",
                "Validated unified-diff edit tool for broad or multi-file patches. Returns a diff summary; for local line edits prefer replace_line_range, insert_at_line, delete_line_range, or apply_text_edits.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings before applying.", false),
                ])),
            ),
            tool_spec(
                "delete_project_files",
                "Delete selected project-relative files only; safer than arbitrary rm for cleanup.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative file paths to delete.", true),
                ])),
            ),
            tool_spec(
                "git_restore_paths",
                "Restore selected tracked paths with git restore; does not remove untracked files.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative tracked paths to restore.", true),
                ])),
            ),
            tool_spec(
                "discard_untracked",
                "Discard selected untracked files with git clean -f -- <paths>.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative untracked paths to remove.", true),
                ])),
            ),
            tool_spec(
                "validate_patch",
                "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings.", false),
                ])),
            ),
            tool_spec(
                "replace_in_file",
                "Literal pattern compatibility path for short exact replacements. Prefer replace_line_range, insert_at_line, or delete_line_range when line numbers are available; fails without writing when old text is missing or ambiguous.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "replace_exact_block",
                "Replace literal UTF-8 text that matches exactly once; no regex or auto-format. Use line edit tools when line numbers are known.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("old_text", "string", "Non-empty literal block; must match exactly once.", true),
                    ("new_text", "string", "Replacement text; may be empty to delete the block.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for current whole-file content.", false),
                ])),
            ),
            tool_spec(
                "insert_before_pattern",
                "Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("pattern", "string", "Non-empty literal pattern; must match exactly once.", true),
                    ("text", "string", "Non-empty text to insert, including intended newlines.", true),
                ])),
            ),
            tool_spec(
                "insert_after_pattern",
                "Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("pattern", "string", "Non-empty literal pattern; must match exactly once.", true),
                    ("text", "string", "Non-empty text to insert, including intended newlines.", true),
                ])),
            ),
            tool_spec(
                "write_project_file",
                "Whole-file write compatibility path for new files or deliberate small overwrites. Prefer structured line edits or apply_text_edits for source changes; requires overwrite/guards for existing files.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "save_project_artifact",
                "Write a bounded binary project artifact from base64. Use for imported session files, generated images, PDFs, and zip files; not for UTF-8 source edits.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path.", true),
                    ("content_base64", "string", "Base64-encoded binary content.", true),
                    ("mime_type", "string", "Optional MIME type.", false),
                    ("overwrite", "boolean", "Allow overwriting an existing file (default false).", false),
                ])),
            ),
            tool_spec(
                "read_project_artifact_metadata",
                "Read bounded metadata for a binary artifact; images include dimensions and zip archives are counted but never extracted. Set allow_missing=true to make a missing artifact a successful exists=false negative assertion.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative artifact path.", true),
                    ("allow_missing", "boolean", "When true, a missing artifact returns exists=false instead of an error.", false),
                ])),
            ),
            tool_spec(
                "read_project_artifact",
                "Chunked content read for a project artifact. Returns base64 for one small segment plus full-file sha256/MIME metadata; not a large-file transfer tool.",
                object_schema(with_optional_session_id(vec![
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
            ),
            tool_spec(
                "artifact_upload_begin",
                "Begin a bounded chunked binary artifact upload. Creates a project-local temporary upload session; finish commits atomically to the target path. For smoke octet-stream uploads, use artifacts/smoke/<name>.artifact or omit mime_type when appropriate.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative output path.", true),
                    ("expected_bytes", "integer", "Optional final byte count guard.", false),
                    ("expected_sha256", "string", "Optional final sha256 guard.", false),
                    ("mime_type", "string", "Optional MIME type.", false),
                    ("overwrite", "boolean", "Allow overwriting an existing file at finish (default false).", false),
                ])),
            ),
            tool_spec(
                "artifact_upload_chunk",
                "Append one base64 chunk to an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id to the target path.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                    ("offset", "integer", "Expected current upload byte offset.", true),
                    ("content_base64", "string", "Base64-encoded chunk; decoded chunk max is 65536 bytes.", true),
                ])),
            ),
            tool_spec(
                "artifact_upload_finish",
                "Finish an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before atomic commit.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                ])),
            ),
            tool_spec(
                "artifact_upload_abort",
                "Abort an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before cleanup and reports final_file_exists without touching the final target.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.", true),
                    ("upload_id", "string", "Opaque wc_upload_* id from artifact_upload_begin.", true),
                ])),
            ),
            tool_spec(
                "replace_line_range",
                "Preferred source-code edit tool for local line changes with clear line numbers. Replaces a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based inclusive start line.", true),
                    ("end_line", "integer", "1-based inclusive end line.", true),
                    ("new_text", "string", "Replacement text; empty deletes the range.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for the original range text.", false),
                    ("expected_old_prefix", "string", "Optional prefix guard for the original range text.", false),
                ])),
            ),
            tool_spec(
                "insert_at_line",
                "Preferred source-code edit tool for local line changes with clear line numbers. Inserts before a specified 1-based line; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("line", "integer", "1-based insertion line; total_lines+1 appends at EOF.", true),
                    ("text", "string", "Text to insert.", true),
                    ("expected_anchor_sha256", "string", "Optional sha256 guard for anchor line or empty EOF anchor.", false),
                    ("expected_anchor_prefix", "string", "Optional prefix guard for anchor line or empty EOF anchor.", false),
                ])),
            ),
            tool_spec(
                "delete_line_range",
                "Preferred source-code edit tool for local line changes with clear line numbers. Deletes a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                object_schema(with_optional_session_id(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based inclusive start line.", true),
                    ("end_line", "integer", "1-based inclusive end line.", true),
                    ("expected_old_sha256", "string", "Optional sha256 guard for the original range text.", false),
                    ("expected_old_prefix", "string", "Optional prefix guard for the original range text.", false),
                ])),
            ),
            tool_spec(
                "apply_text_edits",
                "Preferred batch text edit tool for coordinated source changes in one UTF-8 file. Applies bounded exact replace/insert/delete edits atomically only when all matches validate as unique/non-overlapping. Supports dry_run and sha256 guard.",
                apply_text_edits_input_schema(),
            ),
        ];
        model_visible_tool_definitions()
            .map(|definition| {
                declarations
                    .iter()
                    .find(|spec| spec.name == definition.name)
                    .unwrap_or_else(|| {
                        panic!(
                            "{} public ToolDefinition is missing a ToolSpec declaration",
                            definition.name
                        )
                    })
                    .clone()
            })
            .map(with_common_testing_metadata)
            .collect()
    }

    /// The sorted list of accepted runtime tool names (mirrors `tool_specs`).
    #[cfg(test)]
    pub fn tool_names(&self) -> Vec<String> {
        model_visible_tool_definitions()
            .map(|definition| definition.name.to_string())
            .collect()
    }
}

fn tool_spec(name: &'static str, description: impl Into<String>, input_schema: Value) -> ToolSpec {
    debug_assert!(
        lookup_tool_definition(name).is_some(),
        "{name} ToolSpec is missing a ToolDefinition"
    );
    ToolSpec {
        name: name.to_string(),
        description: description.into(),
        input_schema,
        output_schema: output_schema_for_tool(name),
        annotations: tool_annotations(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use crate::tool_runtime::RuntimeInfo;
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
