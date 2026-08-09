//! Runtime tool call wire/data model and behavioral helpers.
//!
//! This module owns the model-visible tool call enum, parsing by runtime tool
//! name, and the project/session accessors used by dispatch guards and audit
//! logging.

use super::sessions::{
    strip_tool_call_expectation_metadata, SessionExecutionContext, SessionMessageKind,
    SessionMessagePriority, SessionMessageStatus, ToolCallRecorderMetadata,
};
use super::tool_definition::{lookup_tool_definition, model_visible_tool_names_csv};
use super::tool_inputs::{
    default_true, ApplyFileChangeInput, CheckpointValidationInput, ExecutionPurpose,
    ExecutionShell, SessionMode, StartupDetail,
};
use crate::shell_protocol::ShellScriptLanguage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const TOOL_CALL_TOOL_FIELD: &str = "tool";
pub(crate) const TOOL_CALL_PARAMS_FIELD: &str = "params";
pub(crate) const TOOL_CALL_ARGUMENTS_FIELD: &str = "arguments";
pub(crate) const TOOL_CALL_WRAPPER_FIELDS: &[&str] = &[
    TOOL_CALL_TOOL_FIELD,
    TOOL_CALL_PARAMS_FIELD,
    TOOL_CALL_ARGUMENTS_FIELD,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultMode {
    Matches,
    FilesWithMatches,
    Count,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadFilesItem {
    #[serde(deserialize_with = "deserialize_non_empty_read_path")]
    pub path: String,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SearchProjectTextsQuery {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub context_before: Option<usize>,
    #[serde(default)]
    pub context_after: Option<usize>,
    #[serde(default)]
    pub include_globs: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_globs: Option<Vec<String>>,
    #[serde(default)]
    pub result_mode: Option<SearchResultMode>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
}

fn deserialize_non_empty_read_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    if path.trim().is_empty() {
        return Err(serde::de::Error::custom("path must not be empty"));
    }
    Ok(path)
}

fn deserialize_read_files_items<'de, D>(deserializer: D) -> Result<Vec<ReadFilesItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let items = Vec::<ReadFilesItem>::deserialize(deserializer)?;
    if !(1..=8).contains(&items.len()) {
        return Err(serde::de::Error::custom(
            "items must contain between 1 and 8 entries",
        ));
    }
    Ok(items)
}

fn deserialize_search_project_texts_queries<'de, D>(
    deserializer: D,
) -> Result<Vec<SearchProjectTextsQuery>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let queries = Vec::<SearchProjectTextsQuery>::deserialize(deserializer)?;
    if !(1..=8).contains(&queries.len()) {
        return Err(serde::de::Error::custom(
            "queries must contain between 1 and 8 entries",
        ));
    }
    Ok(queries)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "tool", content = "params", rename_all = "snake_case")]
pub enum ToolCall {
    /// List registered tool runtime tools.
    ListTools {
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        features: Option<String>,
        #[serde(default)]
        summary_only: bool,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Create a bounded task tracking session and return an explicit opaque
    /// session id. Later callers should pass that id explicitly (for example as
    /// REST `recording_session_id` wrapper metadata, tool-specific
    /// `session_id`, or MCP `_session_id`) or bind it as current separately.
    StartSession {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        mode: SessionMode,
        #[serde(default)]
        deny_write_tools: bool,
        #[serde(default)]
        deny_shell_tools: bool,
        #[serde(default)]
        execution_context: Option<SessionExecutionContext>,
    },

    /// Create a task session and return deterministic startup context: project
    /// resolution, runtime/git summaries scaled by `detail`, recommended flow,
    /// and explicit current-session binding state. Never calls an LLM.
    ///
    /// `detail` is the only projection control. The removed legacy startup
    /// flags (`compact_startup`, `include_*`, `tool_manifest_*`) are rejected
    /// as unknown fields by strict argument validation.
    StartCodingTask {
        /// Existing runtime project id. Omit this with `client_id` plus
        /// `path` for Runner-side path resolution/registration, or with only
        /// `client_id` for the legacy managed temporary-project flow.
        #[serde(default)]
        project: String,
        /// Runner client that owns `path`, or that should create the managed
        /// temporary project when neither `project` nor `path` is supplied.
        #[serde(default)]
        client_id: Option<String>,
        /// Existing absolute directory on the selected Runner. The Runner
        /// canonicalizes, policy-checks, reuses, or permanently registers it.
        #[serde(default)]
        path: Option<String>,
        /// Optional safe display name for a Runner-managed temporary project.
        #[serde(default)]
        temporary_project_name: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        mode: SessionMode,
        #[serde(default)]
        deny_write_tools: bool,
        #[serde(default)]
        deny_shell_tools: bool,
        #[serde(default)]
        detail: StartupDetail,
        /// Explicitly continue one existing Workflow Session. This business
        /// input is distinct from project-tool `session_id` and wrapper-level
        /// `recording_session_id` metadata.
        #[serde(default)]
        resume_session_id: Option<String>,
        #[serde(default = "default_true")]
        bind_current: bool,
        /// Explicitly create and bind an isolated Workflow Session instead of
        /// continuing the current window/project/repository context.
        #[serde(default)]
        new_session: bool,
        /// Optional complete replacement. Omission preserves the current value
        /// on continuation; `{}` explicitly clears it.
        #[serde(default)]
        execution_context: Option<SessionExecutionContext>,
    },

    /// Start a normal coding task with practical defaults, or continue one by
    /// `session_id`. Thin wrapper over `start_coding_task` that never binds a
    /// current window, never creates a temporary project, and returns only a
    /// compact startup projection. `session_id` is explicit business input for
    /// the exact Workflow Session to continue; it is distinct from wrapper
    /// `recording_session_id` metadata and never a current-session fallback.
    WorkOnProject {
        #[serde(default)]
        project: String,
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        instruction: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Return deterministic finish context for an explicit task session:
    /// changes, workspace hygiene, session/handoff summaries, and bounded
    /// validation-like ledger events. Never calls an LLM.
    FinishCodingTask {
        project: String,
        session_id: String,
        #[serde(default)]
        summary_only: bool,
        #[serde(default)]
        include_diff: Option<bool>,
        #[serde(default)]
        include_workspace: Option<bool>,
        #[serde(default)]
        include_hygiene: Option<bool>,
        #[serde(default)]
        include_handoff: Option<bool>,
        #[serde(default)]
        include_validation_summary: Option<bool>,
    },

    /// Return a bounded structured summary of recorded session ledger data for
    /// an explicit session id.
    SessionSummary {
        session_id: String,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Replace the complete execution defaults of one known active Workflow
    /// Session after resolving and authorizing its exact project. The
    /// in-memory context/event commit is atomic; ledger persistence is queued.
    UpdateSessionContext {
        project: String,
        session_id: String,
        execution_context: SessionExecutionContext,
    },

    /// Explicitly close a workflow session (`Active → Closed`). Requires an
    /// explicit `session_id` (never current-session fallback). Idempotent when
    /// already closed. Does not archive or evict; clears bindings to the closed
    /// Session.
    CloseSession {
        session_id: String,
    },

    /// Read bounded structured validation evidence already present in an
    /// explicit project-scoped session ledger. Never executes validation,
    /// shell commands, agent requests, or project file reads.
    ValidationSummary {
        project: String,
        session_id: String,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Post a bounded session-local ledger message for collaboration, progress,
    /// guidance, or design discussion. This is session metadata only.
    PostSessionMessage {
        session_id: String,
        kind: SessionMessageKind,
        message: String,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        reply_to: Option<String>,
        #[serde(default)]
        priority: SessionMessagePriority,
    },

    /// List session-local ledger messages in stable newest-first order.
    ListSessionMessages {
        session_id: String,
        #[serde(default)]
        kind: Option<SessionMessageKind>,
        #[serde(default)]
        status: Option<SessionMessageStatus>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Mark a session-local message resolved. Idempotent for already resolved
    /// messages.
    ResolveSessionMessage {
        session_id: String,
        message_id: String,
        #[serde(default)]
        resolution: Option<String>,
    },

    /// Return a bounded structured aggregate of session-local ledger discussion.
    SessionDiscussionSummary {
        session_id: String,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Return a bounded structured handoff summary for an explicit session id:
    /// session ledger info, message-board state, recent progress/decisions,
    /// open todos/risks/questions/guidance, recent failed tool calls, and
    /// optional workspace, checkpoint, and ledger-derived validation metadata.
    /// Read-only; never calls an LLM or generates natural-language summaries.
    /// Exposed only through runtime tools / MCP / `callRuntimeTool` (no
    /// dedicated OpenAPI op).
    SessionHandoffSummary {
        session_id: String,
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        include_workspace: Option<bool>,
        #[serde(default)]
        include_checkpoints: Option<bool>,
        #[serde(default)]
        include_validation: Option<bool>,
        #[serde(default)]
        summary_only: bool,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Explicitly bind an existing project-scoped session as current for the
    /// client window, caller, transport, and project.
    BindCurrentSession {
        project: String,
        session_id: String,
    },

    /// Return this window/caller/transport's exact current session binding for
    /// a project, restoring its process-local cache from the ledger if needed.
    CurrentSession {
        project: String,
    },

    /// Remove this window/caller/transport's exact current session binding from
    /// both the process-local cache and durable ledger projection. Idempotent.
    UnbindCurrentSession {
        project: String,
    },

    /// Create a bounded last-known-good workspace checkpoint outside the
    /// project worktree.
    WorkspaceCheckpointCreate {
        project: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        include_untracked: Option<bool>,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        validation: Option<CheckpointValidationInput>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// List checkpoint metadata for a project without returning diffs.
    WorkspaceCheckpointList {
        project: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Show bounded checkpoint metadata and file lists without full diff
    /// content.
    WorkspaceCheckpointShow {
        project: String,
        checkpoint_id: String,
        #[serde(default)]
        include_diff_stat: Option<bool>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Restore a workspace checkpoint after explicit confirmation.
    WorkspaceCheckpointRestore {
        project: String,
        checkpoint_id: String,
        confirm: bool,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Delete a persisted checkpoint file after explicit confirmation.
    WorkspaceCheckpointDelete {
        project: String,
        checkpoint_id: String,
        confirm: bool,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Execute one native process directly from a structured executable and
    /// argv. No shell parser, environment mutation, PTY, or durable handoff is
    /// part of this synchronous v1 contract.
    RunProcess {
        project: String,
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        stdin: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Execute bounded script content transported as typed data and written to
    /// a Runner-owned temporary file. The selected language is explicit and
    /// never inherited from Session default_shell.
    RunScript {
        project: String,
        language: ShellScriptLanguage,
        script: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        stdin: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Execute a shell command in a project directory (sync, short-lived).
    RunShell {
        project: String,
        command: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
        #[serde(default)]
        shell: Option<ExecutionShell>,
    },

    /// Open one explicit command-oriented persistent shell for this Workflow
    /// Session. It is not shared with run_shell/run_job and is not a Job.
    OpenSessionShell {
        project: String,
        session_id: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        shell: Option<ExecutionShell>,
    },

    /// Execute one framed command in an already-open persistent shell.
    SessionShellExec {
        project: String,
        session_id: String,
        shell_id: String,
        command: String,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Read Runner-authoritative lifecycle state for a persistent shell.
    SessionShellStatus {
        project: String,
        session_id: String,
        shell_id: String,
    },

    /// Close a persistent shell and its complete process group. Idempotent for
    /// an already-closed shell retained by the current Server process.
    CloseSessionShell {
        project: String,
        session_id: String,
        shell_id: String,
    },

    /// Apply a unified diff patch to a project.
    ApplyPatch {
        project: String,
        patch: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Validate then apply a unified diff patch in one safer full-auto step.
    ApplyPatchChecked {
        project: String,
        patch: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        deny_sensitive_paths: Option<bool>,
    },

    /// Delete project-relative files only (not directories).
    DeleteProjectFiles {
        project: String,
        paths: Vec<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Restore tracked paths with `git restore -- <paths>`.
    GitRestorePaths {
        project: String,
        paths: Vec<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Discard selected untracked files with `git clean -f -- <paths>`.
    DiscardUntracked {
        project: String,
        paths: Vec<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Validate (preflight) a unified diff patch against an agent-registered
    /// project **without applying it**. Dry-run only: runs `git apply --check`
    /// and `git apply --stat` through the owning agent. Never modifies the
    /// worktree and never falls back to a real apply. Intended for full-auto
    /// coding agent loops that want to check a generated patch before calling
    /// `apply_patch`.
    ValidatePatch {
        project: String,
        patch: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        deny_sensitive_paths: Option<bool>,
    },

    /// Run `git status` on a project.
    GitStatus {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Run `git diff` on a project.
    GitDiff {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        args: Option<Vec<String>>,
    },

    /// Return bounded structured recent git commit history.
    GitLog {
        project: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        skip: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Return bounded structured hunks from `git diff`.
    GitDiffHunks {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        paths: Option<Vec<String>>,
        #[serde(default)]
        max_hunks: Option<usize>,
        #[serde(default)]
        max_hunk_lines: Option<usize>,
        #[serde(default)]
        cached: Option<bool>,
    },

    /// Run `cargo fmt` in an agent-registered Rust project.
    CargoFmt {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        check: Option<bool>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },

    /// Run `cargo check` in an agent-registered Rust project.
    CargoCheck {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        all_targets: Option<bool>,
        #[serde(default)]
        all_features: Option<bool>,
        #[serde(default)]
        no_default_features: Option<bool>,
        #[serde(default)]
        features: Option<String>,
        #[serde(default)]
        package: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },

    /// Run `cargo test` in an agent-registered Rust project.
    CargoTest {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        filter: Option<String>,
        #[serde(default)]
        all_targets: Option<bool>,
        #[serde(default)]
        all_features: Option<bool>,
        #[serde(default)]
        no_default_features: Option<bool>,
        #[serde(default)]
        features: Option<String>,
        #[serde(default)]
        package: Option<String>,
        #[serde(default)]
        no_run: Option<bool>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },

    /// Read a file from a project.
    ReadFile {
        project: String,
        path: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        start_line: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        with_line_numbers: Option<bool>,
    },

    /// Read up to eight UTF-8 files or file ranges under one bounded call.
    ReadFiles {
        project: String,
        #[serde(deserialize_with = "deserialize_read_files_items")]
        items: Vec<ReadFilesItem>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        with_line_numbers: Option<bool>,
    },

    /// Start an async background job (long-running commands, codex CLI, etc.).
    RunJob {
        project: String,
        command: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        timeout_secs: Option<i64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
        #[serde(default)]
        shell: Option<ExecutionShell>,
    },

    /// Stop a bounded runtime job after explicit confirmation.
    StopJob {
        project: String,
        job_id: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        confirm: bool,
    },

    /// Query the status of a running/finished job.
    JobStatus {
        job_id: String,
        #[serde(default)]
        include_command_preview: bool,
    },

    /// Retrieve stdout/stderr log of a job. When `after_observation_token` and
    /// `wait_secs` are both supplied, this is a single bounded wait (up to
    /// `wait_secs`, 1..=60) until the current opaque Job observation token
    /// differs or the Job becomes terminal; it is never a subscription or
    /// streaming connection.
    JobLog {
        job_id: String,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        tail_lines: Option<usize>,
        #[serde(default)]
        after_observation_token: Option<String>,
        #[serde(default)]
        wait_secs: Option<u64>,
    },

    /// List files in an agent-registered project directory (bounded, read-only).
    /// Returns project-relative paths plus a file/dir kind. Routed to the
    /// owning registered agent via the `file_list` op; the server never reads
    /// the agent project path directly.
    ListProjectFiles {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// List the project's tracked files from the Git index, with glob
    /// filtering and automatic directory rollup. Unlike `ListProjectFiles`
    /// (one directory, filesystem order) this answers "what is in this
    /// project" in a single bounded call and never descends into ignored
    /// directories such as `.venv` or `target`.
    ListProjectTrackedFiles {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        globs: Option<Vec<String>>,
        #[serde(default)]
        depth: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
    },

    /// Return a deterministic, bounded, metadata-only overview of an
    /// agent-registered project. The owning agent scans directory entries;
    /// file contents are never read and no LLM is used.
    ProjectOverview {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        max_depth: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Search text inside a project (bounded matches, rg-first with grep
    /// fallback). Each match carries a project-relative path, 1-based line
    /// number, preview line, and bounded context arrays. Sensitive/build
    /// directories are excluded by default.
    SearchProjectText {
        project: String,
        pattern: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        context_before: Option<usize>,
        #[serde(default)]
        context_after: Option<usize>,
        #[serde(default)]
        include_globs: Option<Vec<String>>,
        #[serde(default)]
        exclude_globs: Option<Vec<String>>,
        #[serde(default)]
        result_mode: Option<SearchResultMode>,
        #[serde(default)]
        timeout_secs: Option<i64>,
    },

    /// Run up to eight independent bounded project-text searches under one
    /// project authorization and outer Session event.
    SearchProjectTexts {
        project: String,
        #[serde(deserialize_with = "deserialize_search_project_texts_queries")]
        queries: Vec<SearchProjectTextsQuery>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read-only git diff summary for a project: `git status --porcelain`,
    /// `git diff --stat`, and a parsed changed-file list. Does not modify the
    /// worktree. Routed to the owning agent.
    GitDiffSummary {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read-only model-facing git worktree summary for a project. Reports
    /// branch/head, parsed status counts/files, diff stat, warnings, suggested
    /// next actions, and optional bounded diff hunks. Routed to the owning
    /// agent.
    ShowChanges {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        include_diff: Option<bool>,
        #[serde(default)]
        max_hunks: Option<usize>,
        #[serde(default)]
        max_hunk_lines: Option<usize>,
        #[serde(default)]
        session_event_limit: Option<usize>,
    },

    /// List bounded runtime job summaries across agent and local executors.
    /// Never returns stdout/stderr bodies — only metadata (job_id, kind,
    /// status, project, timestamps, exit_code).
    ListJobs {
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        status: Option<String>,
    },

    /// Return bounded stdout/stderr tails for a job. Defaults to a bounded tail
    /// so the console never reads full logs by default. When `after_observation_token`
    /// and `wait_secs` are both supplied, this is a single bounded wait (up to
    /// `wait_secs`, 1..=60) until the current opaque Job observation token
    /// differs or the Job becomes terminal; it is never a subscription or streaming
    /// connection.
    JobTail {
        job_id: String,
        #[serde(default)]
        tail_lines: Option<usize>,
        #[serde(default)]
        after_observation_token: Option<String>,
        #[serde(default)]
        wait_secs: Option<u64>,
    },

    /// Write a UTF-8 file in a project via the owning agent. Creates new files
    /// and (with `overwrite`) replaces existing ones, gating overwrites on an
    /// optional `expected_sha256` / `expected_content_prefix` so a stale caller
    /// cannot clobber a file that changed underneath it. The server never reads
    /// the agent filesystem directly; the write runs as a native agent file op.
    WriteProjectFile {
        project: String,
        path: String,
        content: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        overwrite: Option<bool>,
        #[serde(default)]
        expected_sha256: Option<String>,
        #[serde(default)]
        expected_content_prefix: Option<String>,
    },

    /// Write a binary artifact in a project via the owning agent. The payload is
    /// base64-encoded and decoded by the agent's native artifact file-op path.
    SaveProjectArtifact {
        project: String,
        path: String,
        content_base64: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        mime_type: Option<String>,
        #[serde(default)]
        overwrite: Option<bool>,
    },

    /// Read bounded metadata for a binary project artifact. Zip files are
    /// counted but never extracted.
    ReadProjectArtifactMetadata {
        project: String,
        path: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        allow_missing: Option<bool>,
    },

    /// Read one bounded binary content segment for a project artifact. Returns
    /// base64 for the requested chunk plus full-file sha256 and MIME metadata.
    /// MCP callers may request one complete, size-limited PNG/JPEG/WebP for
    /// native image content framing; that transport-only option is deliberately
    /// not part of the generic REST/GPT Actions schema.
    ReadProjectArtifact {
        project: String,
        path: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        encoding: Option<String>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        length: Option<usize>,
        #[serde(default)]
        max_bytes: Option<usize>,
        #[serde(default)]
        as_image: Option<bool>,
    },

    /// Begin a bounded chunked binary artifact upload. The agent creates a
    /// project-local temporary upload file and returns an opaque upload id.
    ArtifactUploadBegin {
        project: String,
        path: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_bytes: Option<usize>,
        #[serde(default)]
        expected_sha256: Option<String>,
        #[serde(default)]
        mime_type: Option<String>,
        #[serde(default)]
        overwrite: Option<bool>,
    },

    /// Append one base64-encoded chunk to a bounded artifact upload.
    ArtifactUploadChunk {
        project: String,
        path: String,
        upload_id: String,
        offset: usize,
        content_base64: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Verify and atomically commit a bounded artifact upload.
    ArtifactUploadFinish {
        project: String,
        path: String,
        upload_id: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Abort a bounded artifact upload and remove its temporary files.
    ArtifactUploadAbort {
        project: String,
        path: String,
        upload_id: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Apply a bounded transactional batch of edit/create/delete/rename file
    /// changes via the owning agent. Every existing input file requires a
    /// sha256 precondition and every change is preflighted before the first
    /// mutation. `dry_run` computes the full plan without writing. Exposed only
    /// through runtime tools / MCP / `callRuntimeTool` (no dedicated OpenAPI
    /// operation).
    ApplyTextEdits {
        project: String,
        changes: Vec<ApplyFileChangeInput>,
        #[serde(default)]
        dry_run: Option<bool>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read-only workspace hygiene inspection. Detects pollution risks before
    /// deployment smoke, model handoff, or real development: dirty worktree,
    /// untracked temporary/smoke/anchor files, cache directories, secret-like
    /// path names, and large untracked files. Never cleans, deletes, restores,
    /// or modifies the project. Never reads file contents, env values, tokens,
    /// or stdout/stderr bodies. Suspicious secret files are identified by
    /// path/name only. Exposed only through runtime tools / MCP /
    /// `callRuntimeTool` (no dedicated OpenAPI op).
    WorkspaceHygieneCheck {
        project: String,
        #[serde(default)]
        max_findings: Option<usize>,
        #[serde(default)]
        include_tracked: Option<bool>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// List all agent-registered runtime projects.

    /// Probe agent-side rust-analyzer availability without starting it.
    LspStatus {
        project: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Hierarchical document symbols for a project-relative Rust file.
    DocumentSymbols {
        project: String,
        path: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Latest bounded rust-analyzer diagnostics for a project-relative file.
    DocumentDiagnostics {
        project: String,
        path: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Hover information at a 1-based Unicode scalar position.
    Hover {
        project: String,
        path: String,
        line: usize,
        column: usize,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Bounded workspace symbols matching a non-empty query.
    WorkspaceSymbols {
        project: String,
        query: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Goto definition at a 1-based Unicode scalar position.
    GotoDefinition {
        project: String,
        path: String,
        line: usize,
        column: usize,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Find references at a 1-based Unicode scalar position.
    FindReferences {
        project: String,
        path: String,
        line: usize,
        column: usize,
        #[serde(default = "default_true")]
        include_declaration: bool,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    ListProjects,

    /// Register an existing directory as a WebCodex project on a selected
    /// agent. The agent validates the path against its own policy, writes a
    /// `projects_dir/<id>.toml` file atomically, and refreshes its local
    /// project list. The server refreshes its cached project summaries for
    /// that agent so `listProjects` sees the new project immediately. This is
    /// a mutating agent-side operation constrained by agent policy; the server
    /// never writes project config files on the agent host directly.
    RegisterProject {
        client_id: String,
        id: String,
        name: String,
        path: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default = "default_true")]
        allow_patch: bool,
        #[serde(default)]
        overwrite: bool,
    },

    /// Create a new directory on the selected agent and register it as a
    /// WebCodex project. The agent validates the path against its own policy,
    /// creates the directory (and optional template files / git init), writes
    /// a `projects_dir/<id>.toml` file atomically, and refreshes its local
    /// project list. The server refreshes its cached project summaries so
    /// `listProjects` sees the new project immediately. This is a mutating
    /// agent-side operation constrained by agent policy; the server never
    /// creates directories or writes project config files on the agent host
    /// directly.
    CreateProject {
        client_id: String,
        id: String,
        name: String,
        path: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default = "default_true")]
        allow_patch: bool,
        #[serde(default)]
        template: Option<String>,
        #[serde(default)]
        git_init: bool,
        #[serde(default)]
        allow_existing_empty: bool,
        #[serde(default)]
        overwrite: bool,
    },

    /// List connected shell/agent clients.
    ListAgents,

    /// Return a structured runtime health/observability summary.
    ///
    /// This is a read-only observability tool: it never exposes tokens,
    /// secrets, full env, or stdout/stderr. It returns service metadata,
    /// project config status, agent client summaries, and job counts.
    RuntimeStatus {
        #[serde(default)]
        compact: bool,
        #[serde(default)]
        summary_only: bool,
    },

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// recommended flows, and optional intent-shaped tool views. Intent views
    /// only filter and rank discovery output; they do not change tool behavior,
    /// policy, permissions, execution, or finish verdict semantics. Intended as
    /// a lightweight alternative to `list_tools` for long-running tasks where
    /// the full input/output schemas cause ResponseTooLargeError. Read-only
    /// runtime introspection; never exposes schemas, tokens, secrets, or
    /// internal paths.
    ToolManifest {
        #[serde(default)]
        category: Option<String>,
        /// Optional task intent view such as coding, audit, exploration,
        /// release, or discovery. Distinct from `category`. Discovery filtering
        /// only; does not change tool behavior or finish verdict semantics.
        #[serde(default)]
        intent: Option<String>,
        #[serde(default = "default_true")]
        include_recommended_flows: bool,
        #[serde(default = "default_true")]
        include_risk_summary: bool,
    },
}

/// Strict argument validation for `start_coding_task`: the removed legacy
/// startup flags must fail loudly instead of being silently ignored by serde.
fn reject_unknown_start_coding_task_fields(arguments: &Value) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "project",
        "client_id",
        "path",
        "temporary_project_name",
        "title",
        "mode",
        "deny_write_tools",
        "deny_shell_tools",
        "detail",
        "resume_session_id",
        "bind_current",
        "new_session",
        "execution_context",
        // Wrapper/session metadata that transports may leave in params.
        "session_id",
        "recording_session_id",
        "_session_id",
    ];
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let unknown: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !ALLOWED.contains(key))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(format!(
        "invalid arguments for tool 'start_coding_task': unknown field(s) {}. \
         Startup projection is controlled solely by detail=minimal|standard|full; \
         the legacy startup flags were removed.",
        unknown.join(", ")
    ))
}

fn validate_coding_project_source_shape(
    tool_name: &str,
    arguments: &Value,
    managed_temporary_allowed: bool,
) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    let project = arguments.contains_key("project");
    let client_id = arguments.contains_key("client_id");
    let path = arguments.contains_key("path");
    let temporary_project_name = arguments.contains_key("temporary_project_name");
    for field in ["project", "client_id", "path", "temporary_project_name"] {
        if arguments
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': {field} must not be empty"
            ));
        }
    }
    if path
        && arguments
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !std::path::Path::new(path).is_absolute())
    {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': path must be absolute"
        ));
    }
    if project {
        let mut conflicts = Vec::new();
        if client_id {
            conflicts.push("client_id");
        }
        if path {
            conflicts.push("path");
        }
        if temporary_project_name {
            conflicts.push("temporary_project_name");
        }
        return if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': conflicting fields project and {}",
                conflicts.join(", ")
            ))
        };
    }
    if path {
        if temporary_project_name {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': conflicting fields path and temporary_project_name"
            ));
        }
        return if client_id {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': missing client_id required with path"
            ))
        };
    }
    if temporary_project_name && !client_id {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': missing client_id required with temporary_project_name"
        ));
    }
    if managed_temporary_allowed && client_id {
        return Ok(());
    }
    if client_id {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': missing path required with client_id"
        ));
    }
    let expected = if managed_temporary_allowed {
        "project, client_id + path, or the existing client_id managed temporary-project source"
    } else {
        "project or client_id + path"
    };
    Err(format!(
        "invalid arguments for tool '{tool_name}': missing project source; expected {expected}"
    ))
}

fn reject_unknown_read_files_fields(arguments: &Value) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "project",
        "items",
        "session_id",
        "with_line_numbers",
        // Wrapper/session metadata that transports may leave in params.
        "allow_cross_project_session",
        "recording_session_id",
        "_session_id",
    ];
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let unknown: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !ALLOWED.contains(key))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(format!(
        "invalid arguments for tool 'read_files': unknown field(s) {}",
        unknown.join(", ")
    ))
}

fn reject_unknown_search_project_texts_fields(arguments: &Value) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "project",
        "queries",
        "session_id",
        // Wrapper/session metadata that transports may leave in params.
        "allow_cross_project_session",
        "recording_session_id",
        "_session_id",
    ];
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let unknown: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !ALLOWED.contains(key))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(format!(
        "invalid arguments for tool 'search_project_texts': unknown field(s) {}",
        unknown.join(", ")
    ))
}

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
        Self::from_tool_name_with_recorder_metadata(name, arguments).map(|(call, _)| call)
    }

    pub(crate) fn from_tool_name_with_recorder_metadata(
        name: &str,
        arguments: Value,
    ) -> Result<(Self, ToolCallRecorderMetadata), String> {
        // Reject unknown tool names up front with a helpful message that lists
        // every accepted tool and points the caller at listRuntimeTools. This
        // avoids leaking a raw serde "unknown variant" error and gives custom
        // GPTs an actionable discovery hint.
        let definition = lookup_tool_definition(name).ok_or_else(|| {
            format!(
                "unknown tool '{}'. Available tools: {}. Call listRuntimeTools \
                 (POST /api/tools/list) or the list_tools runtime tool to \
                 discover accepted tool names.",
                name,
                model_visible_tool_names_csv()
            )
        })?;
        let recorder_metadata = ToolCallRecorderMetadata::from_arguments(&arguments);
        let arguments = strip_tool_call_expectation_metadata(arguments);
        if name == "start_coding_task" {
            if let Err(message) = reject_unknown_start_coding_task_fields(&arguments) {
                return Err(message);
            }
            validate_coding_project_source_shape(name, &arguments, true)?;
        }
        if name == "work_on_project" {
            validate_coding_project_source_shape(name, &arguments, false)?;
        }
        if name == "read_files" {
            reject_unknown_read_files_fields(&arguments)?;
        }
        if name == "search_project_texts" {
            reject_unknown_search_project_texts_fields(&arguments)?;
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert(
            TOOL_CALL_TOOL_FIELD.to_string(),
            Value::String(name.to_string()),
        );
        if definition.requires_artifact_upload_path_binding() {
            let missing_path = arguments
                .as_object()
                .and_then(|obj| obj.get("path"))
                .and_then(Value::as_str)
                .map(str::is_empty)
                .unwrap_or(true);
            if missing_path {
                return Err(format!(
                    "invalid arguments for tool '{}': path is required and must match the path \
                     used by artifact_upload_begin to bind upload_id to the requested target path",
                    name
                ));
            }
        }
        if !definition.uses_unit_arguments() {
            // Non-unit tools always carry a `params` object so variants whose
            // fields are all optional (e.g. `list_jobs`) still deserialize when
            // a caller passes `null` arguments. A null argument is normalized
            // to an empty object; required-field validation still fires for
            // tools that need fields.
            let params = if arguments.is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                arguments
            };
            wrapped.insert(TOOL_CALL_PARAMS_FIELD.to_string(), params);
        }
        let call = serde_json::from_value(Value::Object(wrapped))
            .map_err(|e| format!("invalid arguments for tool '{}': {}", name, e))?;
        Ok((call, recorder_metadata))
    }

    /// Raw command text for shell-like calls. Consumed only by the workspace
    /// activity recorder, which truncates it to a bounded preview and honors
    /// the operator's preview config switch — it is never logged verbatim.
    pub(crate) fn command_text(&self) -> Option<&str> {
        match self {
            Self::RunShell { command, .. }
            | Self::SessionShellExec { command, .. }
            | Self::RunJob { command, .. } => Some(command),
            _ => None,
        }
    }

    pub(crate) fn tool_name(&self) -> &'static str {
        match self {
            Self::ListTools { .. } => "list_tools",
            Self::StartSession { .. } => "start_session",
            Self::StartCodingTask { .. } => "start_coding_task",
            Self::WorkOnProject { .. } => "work_on_project",
            Self::FinishCodingTask { .. } => "finish_coding_task",
            Self::SessionSummary { .. } => "session_summary",
            Self::UpdateSessionContext { .. } => "update_session_context",
            Self::CloseSession { .. } => "close_session",
            Self::ValidationSummary { .. } => "validation_summary",
            Self::PostSessionMessage { .. } => "post_session_message",
            Self::ListSessionMessages { .. } => "list_session_messages",
            Self::ResolveSessionMessage { .. } => "resolve_session_message",
            Self::SessionDiscussionSummary { .. } => "session_discussion_summary",
            Self::SessionHandoffSummary { .. } => "session_handoff_summary",
            Self::BindCurrentSession { .. } => "bind_current_session",
            Self::CurrentSession { .. } => "current_session",
            Self::UnbindCurrentSession { .. } => "unbind_current_session",
            Self::WorkspaceCheckpointCreate { .. } => "workspace_checkpoint_create",
            Self::WorkspaceCheckpointList { .. } => "workspace_checkpoint_list",
            Self::WorkspaceCheckpointShow { .. } => "workspace_checkpoint_show",
            Self::WorkspaceCheckpointRestore { .. } => "workspace_checkpoint_restore",
            Self::WorkspaceCheckpointDelete { .. } => "workspace_checkpoint_delete",
            Self::RunProcess { .. } => "run_process",
            Self::RunScript { .. } => "run_script",
            Self::RunShell { .. } => "run_shell",
            Self::OpenSessionShell { .. } => "open_session_shell",
            Self::SessionShellExec { .. } => "session_shell_exec",
            Self::SessionShellStatus { .. } => "session_shell_status",
            Self::CloseSessionShell { .. } => "close_session_shell",
            Self::ApplyPatch { .. } => "apply_patch",
            Self::ApplyPatchChecked { .. } => "apply_patch_checked",
            Self::DeleteProjectFiles { .. } => "delete_project_files",
            Self::GitRestorePaths { .. } => "git_restore_paths",
            Self::DiscardUntracked { .. } => "discard_untracked",
            Self::ValidatePatch { .. } => "validate_patch",
            Self::GitStatus { .. } => "git_status",
            Self::GitDiff { .. } => "git_diff",
            Self::GitDiffHunks { .. } => "git_diff_hunks",
            Self::GitLog { .. } => "git_log",
            Self::CargoFmt { .. } => "cargo_fmt",
            Self::CargoCheck { .. } => "cargo_check",
            Self::CargoTest { .. } => "cargo_test",
            Self::ReadFile { .. } => "read_file",
            Self::ReadFiles { .. } => "read_files",
            Self::RunJob { .. } => "run_job",
            Self::StopJob { .. } => "stop_job",
            Self::JobStatus { .. } => "job_status",
            Self::JobLog { .. } => "job_log",
            Self::ListProjectFiles { .. } => "list_project_files",
            Self::ListProjectTrackedFiles { .. } => "list_project_tracked_files",
            Self::ProjectOverview { .. } => "project_overview",
            Self::SearchProjectText { .. } => "search_project_text",
            Self::SearchProjectTexts { .. } => "search_project_texts",
            Self::GitDiffSummary { .. } => "git_diff_summary",
            Self::ShowChanges { .. } => "show_changes",
            Self::WorkspaceHygieneCheck { .. } => "workspace_hygiene_check",
            Self::ListJobs { .. } => "list_jobs",
            Self::JobTail { .. } => "job_tail",
            Self::WriteProjectFile { .. } => "write_project_file",
            Self::SaveProjectArtifact { .. } => "save_project_artifact",
            Self::ReadProjectArtifactMetadata { .. } => "read_project_artifact_metadata",
            Self::ReadProjectArtifact { .. } => "read_project_artifact",
            Self::ArtifactUploadBegin { .. } => "artifact_upload_begin",
            Self::ArtifactUploadChunk { .. } => "artifact_upload_chunk",
            Self::ArtifactUploadFinish { .. } => "artifact_upload_finish",
            Self::ArtifactUploadAbort { .. } => "artifact_upload_abort",
            Self::ApplyTextEdits { .. } => "apply_text_edits",
            Self::LspStatus { .. } => "lsp_status",
            Self::DocumentSymbols { .. } => "document_symbols",
            Self::DocumentDiagnostics { .. } => "document_diagnostics",
            Self::Hover { .. } => "hover",
            Self::WorkspaceSymbols { .. } => "workspace_symbols",
            Self::GotoDefinition { .. } => "goto_definition",
            Self::FindReferences { .. } => "find_references",
            Self::ListProjects => "list_projects",
            Self::RegisterProject { .. } => "register_project",
            Self::CreateProject { .. } => "create_project",
            Self::ListAgents => "list_agents",
            Self::RuntimeStatus { .. } => "runtime_status",
            Self::ToolManifest { .. } => "tool_manifest",
        }
    }

    pub(crate) fn session_id(&self) -> Option<&str> {
        match self {
            Self::RunProcess { session_id, .. }
            | Self::RunScript { session_id, .. }
            | Self::RunShell { session_id, .. }
            | Self::ApplyPatch { session_id, .. }
            | Self::ApplyPatchChecked { session_id, .. }
            | Self::DeleteProjectFiles { session_id, .. }
            | Self::GitRestorePaths { session_id, .. }
            | Self::DiscardUntracked { session_id, .. }
            | Self::ValidatePatch { session_id, .. }
            | Self::GitStatus { session_id, .. }
            | Self::GitDiff { session_id, .. }
            | Self::GitDiffHunks { session_id, .. }
            | Self::GitLog { session_id, .. }
            | Self::CargoFmt { session_id, .. }
            | Self::CargoCheck { session_id, .. }
            | Self::CargoTest { session_id, .. }
            | Self::ReadFile { session_id, .. }
            | Self::ReadFiles { session_id, .. }
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::ListProjectTrackedFiles { session_id, .. }
            | Self::ProjectOverview { session_id, .. }
            | Self::SearchProjectText { session_id, .. }
            | Self::SearchProjectTexts { session_id, .. }
            | Self::GitDiffSummary { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. }
            | Self::LspStatus { session_id, .. }
            | Self::DocumentSymbols { session_id, .. }
            | Self::DocumentDiagnostics { session_id, .. }
            | Self::Hover { session_id, .. }
            | Self::WorkspaceSymbols { session_id, .. }
            | Self::GotoDefinition { session_id, .. }
            | Self::FindReferences { session_id, .. } => session_id.as_deref(),
            Self::SessionHandoffSummary { session_id, .. } => Some(session_id.as_str()),
            Self::WorkOnProject { session_id, .. } => session_id.as_deref(),
            Self::OpenSessionShell { session_id, .. }
            | Self::SessionShellExec { session_id, .. }
            | Self::SessionShellStatus { session_id, .. }
            | Self::CloseSessionShell { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        }
    }

    pub(crate) fn with_effective_session_id(mut self, effective_session_id: String) -> Self {
        match &mut self {
            Self::RunProcess { session_id, .. }
            | Self::RunScript { session_id, .. }
            | Self::RunShell { session_id, .. }
            | Self::ApplyPatch { session_id, .. }
            | Self::ApplyPatchChecked { session_id, .. }
            | Self::DeleteProjectFiles { session_id, .. }
            | Self::GitRestorePaths { session_id, .. }
            | Self::DiscardUntracked { session_id, .. }
            | Self::ValidatePatch { session_id, .. }
            | Self::GitStatus { session_id, .. }
            | Self::GitDiff { session_id, .. }
            | Self::GitDiffHunks { session_id, .. }
            | Self::GitLog { session_id, .. }
            | Self::CargoFmt { session_id, .. }
            | Self::CargoCheck { session_id, .. }
            | Self::CargoTest { session_id, .. }
            | Self::ReadFile { session_id, .. }
            | Self::ReadFiles { session_id, .. }
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::ListProjectTrackedFiles { session_id, .. }
            | Self::ProjectOverview { session_id, .. }
            | Self::SearchProjectText { session_id, .. }
            | Self::SearchProjectTexts { session_id, .. }
            | Self::GitDiffSummary { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. }
            | Self::LspStatus { session_id, .. }
            | Self::DocumentSymbols { session_id, .. }
            | Self::DocumentDiagnostics { session_id, .. }
            | Self::Hover { session_id, .. }
            | Self::WorkspaceSymbols { session_id, .. }
            | Self::GotoDefinition { session_id, .. }
            | Self::FindReferences { session_id, .. } => {
                if session_id.is_none() {
                    *session_id = Some(effective_session_id);
                }
            }
            _ => {}
        }
        self
    }

    /// Apply project-matched Session defaults without overwriting explicit
    /// per-call arguments.
    pub(crate) fn with_session_execution_context(
        mut self,
        execution_context: &SessionExecutionContext,
    ) -> Self {
        match &mut self {
            Self::RunProcess { cwd, .. } | Self::RunScript { cwd, .. } => {
                if cwd.is_none() {
                    *cwd = execution_context.default_cwd.clone();
                }
            }
            Self::RunShell { cwd, shell, .. }
            | Self::RunJob { cwd, shell, .. }
            | Self::OpenSessionShell { cwd, shell, .. } => {
                if cwd.is_none() {
                    *cwd = execution_context.default_cwd.clone();
                }
                if shell.is_none() {
                    *shell = execution_context.default_shell;
                }
            }
            _ => {}
        }
        self
    }

    pub(crate) fn project(&self) -> Option<&str> {
        match self {
            Self::RunProcess { project, .. }
            | Self::RunScript { project, .. }
            | Self::RunShell { project, .. }
            | Self::OpenSessionShell { project, .. }
            | Self::SessionShellExec { project, .. }
            | Self::SessionShellStatus { project, .. }
            | Self::CloseSessionShell { project, .. }
            | Self::ApplyPatch { project, .. }
            | Self::ApplyPatchChecked { project, .. }
            | Self::DeleteProjectFiles { project, .. }
            | Self::GitRestorePaths { project, .. }
            | Self::DiscardUntracked { project, .. }
            | Self::ValidatePatch { project, .. }
            | Self::GitStatus { project, .. }
            | Self::GitDiff { project, .. }
            | Self::GitDiffHunks { project, .. }
            | Self::GitLog { project, .. }
            | Self::CargoFmt { project, .. }
            | Self::CargoCheck { project, .. }
            | Self::CargoTest { project, .. }
            | Self::ReadFile { project, .. }
            | Self::ReadFiles { project, .. }
            | Self::RunJob { project, .. }
            | Self::StopJob { project, .. }
            | Self::ListProjectFiles { project, .. }
            | Self::ListProjectTrackedFiles { project, .. }
            | Self::ProjectOverview { project, .. }
            | Self::SearchProjectText { project, .. }
            | Self::SearchProjectTexts { project, .. }
            | Self::GitDiffSummary { project, .. }
            | Self::ShowChanges { project, .. }
            | Self::WriteProjectFile { project, .. }
            | Self::SaveProjectArtifact { project, .. }
            | Self::ReadProjectArtifactMetadata { project, .. }
            | Self::ReadProjectArtifact { project, .. }
            | Self::ArtifactUploadBegin { project, .. }
            | Self::ArtifactUploadChunk { project, .. }
            | Self::ArtifactUploadFinish { project, .. }
            | Self::ArtifactUploadAbort { project, .. }
            | Self::ApplyTextEdits { project, .. }
            | Self::BindCurrentSession { project, .. }
            | Self::CurrentSession { project }
            | Self::UnbindCurrentSession { project }
            | Self::WorkspaceCheckpointCreate { project, .. }
            | Self::WorkspaceCheckpointList { project, .. }
            | Self::WorkspaceCheckpointShow { project, .. }
            | Self::WorkspaceCheckpointRestore { project, .. }
            | Self::WorkspaceCheckpointDelete { project, .. }
            | Self::WorkspaceHygieneCheck { project, .. }
            | Self::LspStatus { project, .. }
            | Self::DocumentSymbols { project, .. }
            | Self::DocumentDiagnostics { project, .. }
            | Self::Hover { project, .. }
            | Self::WorkspaceSymbols { project, .. }
            | Self::GotoDefinition { project, .. }
            | Self::FindReferences { project, .. } => Some(project.as_str()),
            Self::StartCodingTask { project, .. } if !project.trim().is_empty() => {
                Some(project.as_str())
            }
            Self::WorkOnProject { project, .. } if !project.trim().is_empty() => {
                Some(project.as_str())
            }
            Self::FinishCodingTask { project, .. } => Some(project.as_str()),
            Self::UpdateSessionContext { project, .. }
            | Self::ValidationSummary { project, .. } => Some(project.as_str()),
            Self::SessionHandoffSummary { project, .. } => project.as_deref(),
            _ => None,
        }
    }
}
