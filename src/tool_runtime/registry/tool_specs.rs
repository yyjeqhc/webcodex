use serde_json::Value;

use super::super::tool_definition::{lookup_tool_definition, model_visible_tool_definitions};
use super::super::tool_spec::ToolSpec;
use super::super::ToolRuntime;
use super::input_schemas::{
    apply_patch_checked_input_schema, apply_patch_input_schema, apply_text_edits_input_schema,
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema,
    checkpoint_create_input_schema, checkpoint_delete_input_schema, checkpoint_list_input_schema,
    checkpoint_restore_input_schema, checkpoint_show_input_schema, create_project_input_schema,
    current_session_input_schema, delete_project_files_input_schema,
    discard_untracked_input_schema, empty_input_schema, finish_coding_task_input_schema,
    git_diff_hunks_input_schema, git_diff_input_schema, git_diff_summary_input_schema,
    git_log_input_schema, git_restore_paths_input_schema, git_status_input_schema,
    insert_after_pattern_input_schema, insert_before_pattern_input_schema, job_log_input_schema,
    job_status_input_schema, job_tail_input_schema, list_jobs_input_schema,
    list_project_files_input_schema, list_session_messages_input_schema, list_tools_input_schema,
    post_session_message_input_schema, read_file_input_schema, register_project_input_schema,
    replace_exact_block_input_schema, replace_in_file_input_schema,
    resolve_session_message_input_schema, run_codex_input_schema, run_job_input_schema,
    run_shell_input_schema, search_project_text_input_schema,
    session_discussion_summary_input_schema, session_handoff_summary_input_schema,
    session_summary_input_schema, show_changes_input_schema, start_coding_task_input_schema,
    start_session_input_schema, stop_job_input_schema, tool_manifest_input_schema,
    validate_patch_input_schema, with_common_testing_metadata, with_optional_session_id,
    workspace_hygiene_check_input_schema, write_project_file_input_schema,
};
use super::{object_schema, output_schema_for_tool, tool_annotations};

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let declarations = vec![
            tool_spec(
                "list_tools",
                "List runtime tools. Full output includes schemas and may be large; use summary_only with category, features, or limit for bounded GPT Action discovery.",
                list_tools_input_schema(),
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
                session_summary_input_schema(),
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
                checkpoint_list_input_schema(),
            ),
            tool_spec(
                "workspace_checkpoint_show",
                "Show bounded checkpoint metadata, file list, skipped files, and optional diff stat. Does not return full diff/content by default.",
                checkpoint_show_input_schema(),
            ),
            tool_spec(
                "workspace_checkpoint_restore",
                "Restore a checkpoint after confirm=true. Requires matching HEAD and refuses unsafe current state rather than half-restoring.",
                checkpoint_restore_input_schema(),
            ),
            tool_spec(
                "workspace_checkpoint_delete",
                "Delete one checkpoint JSON file after confirm=true. Does not touch the project worktree.",
                checkpoint_delete_input_schema(),
            ),
            tool_spec(
                "list_projects",
                "List agent-registered runtime projects, execution mode, and smoke-selection capabilities such as git_available and recommended_for_smoke.",
                empty_input_schema(),
            ),
            tool_spec(
                "register_project",
                "Register an existing directory as a WebCodex project on a selected agent. "
                    .to_string()
                    + "Mutation with side effects; constrained by agent policy. The agent validates "
                    + "the path, writes projects_dir/<id>.toml atomically, and refreshes its "
                    + "project list. Requires Bearer auth.",
                register_project_input_schema(),
            ),
            tool_spec(
                "create_project",
                "Create a new directory on the selected agent and register it as a WebCodex "
                    .to_string()
                    + "project. Mutation with side effects; constrained by agent policy. Creates "
                    + "directory, optional template, optional git init, writes projects_dir/<id>.toml "
                    + "atomically. Requires Bearer auth.",
                create_project_input_schema(),
            ),
            tool_spec(
                "list_agents",
                "List connected local/remote execution agents.",
                empty_input_schema(),
            ),
            tool_spec(
                "runtime_status",
                "Return a structured runtime health/observability summary (service "
                    .to_string()
                    + "metadata, projects config status, agent client summaries, and job counts). "
                    + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
                empty_input_schema(),
            ),
            tool_spec(
                "tool_manifest",
                "Return a compact, bounded tool manifest with categories, accepted flattened args, risk "
                    .to_string()
                    + "summary, and recommended flows. Lightweight alternative to list_tools for "
                    + "long tasks. Read-only; never exposes schemas, tokens, or internal paths.",
                tool_manifest_input_schema(),
            ),
            tool_spec(
                "run_shell",
                "Bounded command escape hatch for validation, builds, tests, or diagnostics only. Do not use as the primary file editing path; prefer cargo_* / validate_patch for common checks and structured line edit tools for source edits.",
                run_shell_input_schema(),
            ),
            tool_spec(
                "run_job",
                "Start an asynchronous shell job inside an agent-registered project.".to_string(),
                run_job_input_schema(),
            ),
            tool_spec(
                "stop_job",
                "Stop a bounded runtime job started through WebCodex. Requires confirm=true, obeys project/session ownership, never exposes stdout/stderr, and returns stop_effect/terminal lifecycle fields.",
                stop_job_input_schema(),
            ),
            tool_spec(
                "run_codex",
                "Optional Codex CLI delegation as an async project job. Requires Codex CLI installed and configured on the owning agent. Use only when the user explicitly asks to delegate to Codex; otherwise use WebCodex file/git/shell/line-edit tools directly.",
                run_codex_input_schema(),
            ),
            tool_spec(
                "job_status",
                "Get bounded lifecycle status for a runtime job. Omits command_preview by default and never returns stdout/stderr bodies.",
                job_status_input_schema(),
            ),
            tool_spec(
                "job_log",
                "Read stdout/stderr for a runtime job.",
                job_log_input_schema(),
            ),
            tool_spec(
                "list_project_files",
                "List files in an agent-registered project directory (bounded, "
                    .to_string()
                    + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                    + "to the owning registered agent; the server never reads the agent project "
                    + "path directly.",
                list_project_files_input_schema(),
            ),
            tool_spec(
                "search_project_text",
                "Default inspect/search tool for project text (rg-first with grep fallback). Returns structured output: matches with path, 1-based line, preview/context, plus backend, truncated, count, context_before, and context_after.",
                search_project_text_input_schema(),
            ),
            tool_spec(
                "git_diff_summary",
                "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                git_diff_summary_input_schema(),
            ),
            tool_spec(
                "show_changes",
                "Default inspect/review tool before final response. Read-only worktree plus optional session summary; reports status, warnings, next actions, and bounded hunks without modifying files.",
                show_changes_input_schema(),
            ),
            tool_spec(
                "list_jobs",
                "List bounded runtime job summaries across agent and local executors. "
                    .to_string()
                    + "Never returns stdout/stderr bodies — only metadata (job_id, kind, status, "
                    + "project, timestamps, exit_code).",
                list_jobs_input_schema(),
            ),
            tool_spec(
                "job_tail",
                "Return bounded stdout/stderr tails for a job.",
                job_tail_input_schema(),
            ),
            tool_spec(
                "read_file",
                "Default inspect tool for targeted source reading. Reads bounded UTF-8 file ranges from an agent-registered project, optionally with 1-based line numbers for structured line edits.",
                read_file_input_schema(),
            ),
            tool_spec(
                "git_status",
                "Run git status --porcelain for a project.",
                git_status_input_schema(),
            ),
            tool_spec(
                "git_diff",
                "Run git diff for a project, optionally scoped to paths.",
                git_diff_input_schema(),
            ),
            tool_spec(
                "git_diff_hunks",
                "Return bounded structured git diff hunks for review. Supports optional paths and cached diff; does not modify the worktree.",
                git_diff_hunks_input_schema(),
            ),
            tool_spec(
                "git_log",
                "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.",
                git_log_input_schema(),
            ),
            tool_spec(
                "cargo_fmt",
                "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation.",
                cargo_fmt_input_schema(),
            ),
            tool_spec(
                "cargo_check",
                "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation; use before raw run_shell when applicable.",
                cargo_check_input_schema(),
            ),
            tool_spec(
                "cargo_test",
                "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails; use before raw run_shell when applicable.",
                cargo_test_input_schema(),
            ),
            tool_spec(
                "apply_patch",
                "Apply a unified diff patch to an agent-registered project.".to_string(),
                apply_patch_input_schema(),
            ),
            tool_spec(
                "apply_patch_checked",
                "Validated unified-diff edit tool for broad or multi-file patches. Returns a diff summary; for local line edits prefer replace_line_range, insert_at_line, delete_line_range, or apply_text_edits.",
                apply_patch_checked_input_schema(),
            ),
            tool_spec(
                "delete_project_files",
                "Delete selected project-relative files only; safer than arbitrary rm for cleanup.",
                delete_project_files_input_schema(),
            ),
            tool_spec(
                "git_restore_paths",
                "Restore selected tracked paths with git restore; does not remove untracked files.",
                git_restore_paths_input_schema(),
            ),
            tool_spec(
                "discard_untracked",
                "Discard selected untracked files with git clean -f -- <paths>.",
                discard_untracked_input_schema(),
            ),
            tool_spec(
                "validate_patch",
                "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.",
                validate_patch_input_schema(),
            ),
            tool_spec(
                "replace_in_file",
                "Literal pattern compatibility path for short exact replacements. Prefer replace_line_range, insert_at_line, or delete_line_range when line numbers are available; fails without writing when old text is missing or ambiguous.",
                replace_in_file_input_schema(),
            ),
            tool_spec(
                "replace_exact_block",
                "Replace literal UTF-8 text that matches exactly once; no regex or auto-format. Use line edit tools when line numbers are known.",
                replace_exact_block_input_schema(),
            ),
            tool_spec(
                "insert_before_pattern",
                "Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                insert_before_pattern_input_schema(),
            ),
            tool_spec(
                "insert_after_pattern",
                "Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                insert_after_pattern_input_schema(),
            ),
            tool_spec(
                "write_project_file",
                "Whole-file write compatibility path for new files or deliberate small overwrites. Prefer structured line edits or apply_text_edits for source changes; requires overwrite/guards for existing files.",
                write_project_file_input_schema(),
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
