//! Runtime tool call wire/data model and behavioral helpers.
//!
//! This module owns the model-visible tool call enum, parsing by runtime tool
//! name, and the project/session accessors used by dispatch guards and audit
//! logging.

use super::sessions::{
    strip_tool_call_expectation_metadata, SessionMessageKind, SessionMessagePriority,
    SessionMessageStatus, ToolCallRecorderMetadata,
};
use super::tool_inputs::{
    default_true, ApplyTextEditInput, CheckpointValidationInput, SessionMode,
};
use super::tool_names::{is_known_tool_name, model_visible_tool_names_csv};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    },

    /// Create a task session and return deterministic startup context: project
    /// resolution, optional runtime/git/rules summaries, recommended flow, and
    /// explicit current-session binding state. Never calls an LLM.
    StartCodingTask {
        project: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        mode: SessionMode,
        #[serde(default)]
        deny_write_tools: bool,
        #[serde(default)]
        deny_shell_tools: bool,
        #[serde(default)]
        include_runtime_status: Option<bool>,
        #[serde(default)]
        include_git: Option<bool>,
        #[serde(default)]
        include_recent_commits: Option<bool>,
        #[serde(default)]
        include_rules: Option<bool>,
        #[serde(default)]
        include_tool_manifest: Option<bool>,
        #[serde(default)]
        tool_manifest_categories: Option<Vec<String>>,
        #[serde(default)]
        tool_manifest_limit: Option<usize>,
        #[serde(default)]
        bind_current: bool,
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

    /// Explicitly bind an existing project-scoped session as the caller's
    /// process-local in-memory current session for later project tool calls on
    /// this transport.
    BindCurrentSession { project: String, session_id: String },

    /// Return the caller's process-local current session binding for a project,
    /// if any.
    CurrentSession { project: String },

    /// Remove the caller's process-local current session binding for a project.
    /// Idempotent.
    UnbindCurrentSession { project: String },

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

    /// Start Codex CLI as an async background job.
    RunCodex {
        project: String,
        prompt: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        approval_mode: Option<String>,
        #[serde(default)]
        timeout_secs: Option<i64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        extra_args: Option<Vec<String>>,
    },

    /// Query the status of a running/finished job.
    JobStatus {
        job_id: String,
        #[serde(default)]
        include_command_preview: bool,
    },

    /// Retrieve stdout/stderr log of a job.
    JobLog {
        job_id: String,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        tail_lines: Option<usize>,
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
    /// so the console never reads full logs by default.
    JobTail {
        job_id: String,
        #[serde(default)]
        tail_lines: Option<usize>,
    },

    /// Replace a (unique) substring in a project file via the owning agent.
    /// Safer than `run_shell` sed/awk/python one-liners for text edits: the
    /// edit travels through a native file-op request, sensitive paths are
    /// rejected, and the file is left untouched whenever `old` is missing or
    /// ambiguous.
    ReplaceInFile {
        project: String,
        path: String,
        old: String,
        new: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_replacements: Option<i64>,
        #[serde(default)]
        allow_multiple: Option<bool>,
    },

    /// Replace a literal block that must occur exactly once in a UTF-8 file.
    ReplaceExactBlock {
        project: String,
        path: String,
        old_text: String,
        new_text: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_old_sha256: Option<String>,
    },

    /// Insert literal text before a literal pattern that must occur exactly once.
    InsertBeforePattern {
        project: String,
        path: String,
        pattern: String,
        text: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Insert literal text after a literal pattern that must occur exactly once.
    InsertAfterPattern {
        project: String,
        path: String,
        pattern: String,
        text: String,
        #[serde(default)]
        session_id: Option<String>,
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

    /// Replace a 1-based inclusive line range in a UTF-8 file via the owning
    /// agent. The original range may be guarded by sha256 and/or prefix checks.
    ReplaceLineRange {
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        new_text: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_old_sha256: Option<String>,
        #[serde(default)]
        expected_old_prefix: Option<String>,
    },

    /// Insert text before a 1-based line in a UTF-8 file via the owning agent.
    /// `line == total_lines + 1` appends at EOF; optional guards apply to the
    /// anchor line (or the empty EOF anchor).
    InsertAtLine {
        project: String,
        path: String,
        line: usize,
        text: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_anchor_sha256: Option<String>,
        #[serde(default)]
        expected_anchor_prefix: Option<String>,
    },

    /// Delete a 1-based inclusive line range in a UTF-8 file via the owning
    /// agent. Equivalent to `replace_line_range` with an empty replacement but
    /// exposed separately for easier tool selection.
    DeleteLineRange {
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        expected_old_sha256: Option<String>,
        #[serde(default)]
        expected_old_prefix: Option<String>,
    },

    /// Apply a bounded batch of atomic text edits to a single UTF-8 file via
    /// the owning agent. Intended for large-block refactors (whole function
    /// clusters) where line-number edits cause drift and structural pollution.
    /// Every edit is validated against the original file before any write: each
    /// `old_text`/`anchor_text` must match exactly once, edits must not overlap
    /// ambiguously, and an optional `expected_file_sha256` guards the whole
    /// file. Only when all edits validate is the file replaced atomically.
    /// `dry_run` computes the plan without writing. Exposed only through
    /// runtime tools / MCP / `callRuntimeTool` (no dedicated OpenAPI op).
    ApplyTextEdits {
        project: String,
        path: String,
        edits: Vec<ApplyTextEditInput>,
        #[serde(default)]
        dry_run: Option<bool>,
        #[serde(default)]
        expected_file_sha256: Option<String>,
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
    RuntimeStatus,

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// and recommended flows. Intended as a lightweight alternative to
    /// `list_tools` for long-running tasks where the full input/output schemas
    /// cause ResponseTooLargeError. Read-only runtime introspection; never
    /// exposes schemas, tokens, secrets, or internal paths.
    ToolManifest {
        #[serde(default)]
        category: Option<String>,
        #[serde(default = "default_true")]
        include_recommended_flows: bool,
        #[serde(default = "default_true")]
        include_risk_summary: bool,
    },
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
        if !is_known_tool_name(name) {
            return Err(format!(
                "unknown tool '{}'. Available tools: {}. Call listRuntimeTools \
                 (POST /api/tools/list) or the list_tools runtime tool to \
                 discover accepted tool names.",
                name,
                model_visible_tool_names_csv()
            ));
        }
        let recorder_metadata = ToolCallRecorderMetadata::from_arguments(&arguments);
        let arguments = strip_tool_call_expectation_metadata(arguments);
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("tool".to_string(), Value::String(name.to_string()));
        if matches!(
            name,
            "artifact_upload_chunk" | "artifact_upload_finish" | "artifact_upload_abort"
        ) {
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
        let unit_tool = matches!(name, "list_projects" | "list_agents" | "runtime_status");
        if !unit_tool {
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
            wrapped.insert("params".to_string(), params);
        }
        let call = serde_json::from_value(Value::Object(wrapped))
            .map_err(|e| format!("invalid arguments for tool '{}': {}", name, e))?;
        Ok((call, recorder_metadata))
    }

    pub(crate) fn tool_name(&self) -> &'static str {
        match self {
            Self::ListTools { .. } => "list_tools",
            Self::StartSession { .. } => "start_session",
            Self::StartCodingTask { .. } => "start_coding_task",
            Self::FinishCodingTask { .. } => "finish_coding_task",
            Self::SessionSummary { .. } => "session_summary",
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
            Self::RunShell { .. } => "run_shell",
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
            Self::RunJob { .. } => "run_job",
            Self::StopJob { .. } => "stop_job",
            Self::RunCodex { .. } => "run_codex",
            Self::JobStatus { .. } => "job_status",
            Self::JobLog { .. } => "job_log",
            Self::ListProjectFiles { .. } => "list_project_files",
            Self::SearchProjectText { .. } => "search_project_text",
            Self::GitDiffSummary { .. } => "git_diff_summary",
            Self::ShowChanges { .. } => "show_changes",
            Self::WorkspaceHygieneCheck { .. } => "workspace_hygiene_check",
            Self::ListJobs { .. } => "list_jobs",
            Self::JobTail { .. } => "job_tail",
            Self::ReplaceInFile { .. } => "replace_in_file",
            Self::ReplaceExactBlock { .. } => "replace_exact_block",
            Self::InsertBeforePattern { .. } => "insert_before_pattern",
            Self::InsertAfterPattern { .. } => "insert_after_pattern",
            Self::WriteProjectFile { .. } => "write_project_file",
            Self::SaveProjectArtifact { .. } => "save_project_artifact",
            Self::ReadProjectArtifactMetadata { .. } => "read_project_artifact_metadata",
            Self::ReadProjectArtifact { .. } => "read_project_artifact",
            Self::ArtifactUploadBegin { .. } => "artifact_upload_begin",
            Self::ArtifactUploadChunk { .. } => "artifact_upload_chunk",
            Self::ArtifactUploadFinish { .. } => "artifact_upload_finish",
            Self::ArtifactUploadAbort { .. } => "artifact_upload_abort",
            Self::ReplaceLineRange { .. } => "replace_line_range",
            Self::InsertAtLine { .. } => "insert_at_line",
            Self::DeleteLineRange { .. } => "delete_line_range",
            Self::ApplyTextEdits { .. } => "apply_text_edits",
            Self::ListProjects => "list_projects",
            Self::RegisterProject { .. } => "register_project",
            Self::CreateProject { .. } => "create_project",
            Self::ListAgents => "list_agents",
            Self::RuntimeStatus => "runtime_status",
            Self::ToolManifest { .. } => "tool_manifest",
        }
    }

    pub(crate) fn session_id(&self) -> Option<&str> {
        match self {
            Self::RunShell { session_id, .. }
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
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::RunCodex { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::SearchProjectText { session_id, .. }
            | Self::GitDiffSummary { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::ReplaceInFile { session_id, .. }
            | Self::ReplaceExactBlock { session_id, .. }
            | Self::InsertBeforePattern { session_id, .. }
            | Self::InsertAfterPattern { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ReplaceLineRange { session_id, .. }
            | Self::InsertAtLine { session_id, .. }
            | Self::DeleteLineRange { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. } => session_id.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn with_effective_session_id(mut self, effective_session_id: String) -> Self {
        match &mut self {
            Self::RunShell { session_id, .. }
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
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::RunCodex { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::SearchProjectText { session_id, .. }
            | Self::GitDiffSummary { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::ReplaceInFile { session_id, .. }
            | Self::ReplaceExactBlock { session_id, .. }
            | Self::InsertBeforePattern { session_id, .. }
            | Self::InsertAfterPattern { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ReplaceLineRange { session_id, .. }
            | Self::InsertAtLine { session_id, .. }
            | Self::DeleteLineRange { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. } => {
                if session_id.is_none() {
                    *session_id = Some(effective_session_id);
                }
            }
            _ => {}
        }
        self
    }

    pub(crate) fn project(&self) -> Option<&str> {
        match self {
            Self::RunShell { project, .. }
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
            | Self::RunJob { project, .. }
            | Self::StopJob { project, .. }
            | Self::RunCodex { project, .. }
            | Self::ListProjectFiles { project, .. }
            | Self::SearchProjectText { project, .. }
            | Self::GitDiffSummary { project, .. }
            | Self::ShowChanges { project, .. }
            | Self::ReplaceInFile { project, .. }
            | Self::ReplaceExactBlock { project, .. }
            | Self::InsertBeforePattern { project, .. }
            | Self::InsertAfterPattern { project, .. }
            | Self::WriteProjectFile { project, .. }
            | Self::SaveProjectArtifact { project, .. }
            | Self::ReadProjectArtifactMetadata { project, .. }
            | Self::ReadProjectArtifact { project, .. }
            | Self::ArtifactUploadBegin { project, .. }
            | Self::ArtifactUploadChunk { project, .. }
            | Self::ArtifactUploadFinish { project, .. }
            | Self::ArtifactUploadAbort { project, .. }
            | Self::ReplaceLineRange { project, .. }
            | Self::InsertAtLine { project, .. }
            | Self::DeleteLineRange { project, .. }
            | Self::ApplyTextEdits { project, .. }
            | Self::BindCurrentSession { project, .. }
            | Self::CurrentSession { project }
            | Self::UnbindCurrentSession { project }
            | Self::WorkspaceCheckpointCreate { project, .. }
            | Self::WorkspaceCheckpointList { project, .. }
            | Self::WorkspaceCheckpointShow { project, .. }
            | Self::WorkspaceCheckpointRestore { project, .. }
            | Self::WorkspaceCheckpointDelete { project, .. }
            | Self::WorkspaceHygieneCheck { project, .. } => Some(project.as_str()),
            Self::StartCodingTask { project, .. } | Self::FinishCodingTask { project, .. } => {
                Some(project.as_str())
            }
            Self::SessionHandoffSummary { project, .. } => project.as_deref(),
            _ => None,
        }
    }
}
