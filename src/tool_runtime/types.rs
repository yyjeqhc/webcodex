use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Serde default helper: `true`. Used by `ToolCall` variants whose `allow_patch`
/// field defaults to true (matching the agent-side project TOML parser).
pub fn default_true() -> bool {
    true
}

// =============================================================================
// Tool input — one variant per tool
// =============================================================================

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "tool", content = "params", rename_all = "snake_case")]
pub enum ToolCall {
    /// List registered tool runtime tools.
    ListTools,

    /// Start an in-memory task tracking session. The session id is opaque and
    /// may be passed as REST `session_id` or MCP `_session_id` metadata on
    /// later tool calls.
    StartSession {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },

    /// Return a bounded structured summary of recorded session events.
    SessionSummary {
        session_id: String,
        #[serde(default)]
        limit: Option<usize>,
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
    JobStatus { job_id: String },

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

    /// Search text inside an agent-registered project (bounded matches). Each
    /// match carries a project-relative path, 1-based line number, and a
    /// preview line. Sensitive/build directories (`.git`, `target`,
    /// `node_modules`) are excluded by default. Routed to the owning agent via
    /// a bounded `grep -rnI` shell call.
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
    /// command is a fixed helper, old/new travel over stdin (never interpolated
    /// into the shell command), sensitive paths are rejected, and the file is
    /// left untouched whenever `old` is missing or ambiguous.
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
    /// the agent filesystem directly; the fixed helper runs on the agent.
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
    /// base64-encoded and decoded by a fixed helper on the agent side.
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
}

/// The exact, ordered set of runtime tool names accepted by
/// `ToolCall::from_tool_name`. Kept in sync with the `ToolCall` variants
/// (which use `#[serde(rename_all = "snake_case")]`). Used to produce helpful
/// "unknown tool" errors that list every accepted name instead of leaking a
/// raw serde variant error.
pub const KNOWN_TOOL_NAMES: &[&str] = &[
    "list_tools",
    "start_session",
    "session_summary",
    "run_shell",
    "apply_patch",
    "apply_patch_checked",
    "delete_project_files",
    "git_restore_paths",
    "discard_untracked",
    "validate_patch",
    "replace_in_file",
    "replace_exact_block",
    "insert_before_pattern",
    "insert_after_pattern",
    "write_project_file",
    "save_project_artifact",
    "read_project_artifact_metadata",
    "read_project_artifact",
    "replace_line_range",
    "insert_at_line",
    "delete_line_range",
    "git_status",
    "git_diff",
    "git_diff_hunks",
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "read_file",
    "run_job",
    "run_codex",
    "job_status",
    "job_log",
    "list_project_files",
    "search_project_text",
    "git_diff_summary",
    "show_changes",
    "list_jobs",
    "job_tail",
    "list_projects",
    "register_project",
    "create_project",
    "list_agents",
    "runtime_status",
];

/// Returns `true` if `name` is a recognized runtime tool name. Public so the
/// HTTP/MCP adapters can decide whether to emit the rich "unknown tool" error.
pub fn is_known_tool_name(name: &str) -> bool {
    KNOWN_TOOL_NAMES.iter().any(|n| *n == name)
}

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
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
                KNOWN_TOOL_NAMES.join(", ")
            ));
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("tool".to_string(), Value::String(name.to_string()));
        let unit_tool = matches!(
            name,
            "list_tools" | "list_projects" | "list_agents" | "runtime_status"
        );
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
        serde_json::from_value(Value::Object(wrapped))
            .map_err(|e| format!("invalid arguments for tool '{}': {}", name, e))
    }

    pub(crate) fn tool_name(&self) -> &'static str {
        match self {
            Self::ListTools => "list_tools",
            Self::StartSession { .. } => "start_session",
            Self::SessionSummary { .. } => "session_summary",
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
            Self::CargoFmt { .. } => "cargo_fmt",
            Self::CargoCheck { .. } => "cargo_check",
            Self::CargoTest { .. } => "cargo_test",
            Self::ReadFile { .. } => "read_file",
            Self::RunJob { .. } => "run_job",
            Self::RunCodex { .. } => "run_codex",
            Self::JobStatus { .. } => "job_status",
            Self::JobLog { .. } => "job_log",
            Self::ListProjectFiles { .. } => "list_project_files",
            Self::SearchProjectText { .. } => "search_project_text",
            Self::GitDiffSummary { .. } => "git_diff_summary",
            Self::ShowChanges { .. } => "show_changes",
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
            Self::ReplaceLineRange { .. } => "replace_line_range",
            Self::InsertAtLine { .. } => "insert_at_line",
            Self::DeleteLineRange { .. } => "delete_line_range",
            Self::ListProjects => "list_projects",
            Self::RegisterProject { .. } => "register_project",
            Self::CreateProject { .. } => "create_project",
            Self::ListAgents => "list_agents",
            Self::RuntimeStatus => "runtime_status",
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
            | Self::CargoFmt { session_id, .. }
            | Self::CargoCheck { session_id, .. }
            | Self::CargoTest { session_id, .. }
            | Self::ReadFile { session_id, .. }
            | Self::RunJob { session_id, .. }
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
            | Self::ReplaceLineRange { session_id, .. }
            | Self::InsertAtLine { session_id, .. }
            | Self::DeleteLineRange { session_id, .. } => session_id.as_deref(),
            _ => None,
        }
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
            | Self::CargoFmt { project, .. }
            | Self::CargoCheck { project, .. }
            | Self::CargoTest { project, .. }
            | Self::ReadFile { project, .. }
            | Self::RunJob { project, .. }
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
            | Self::ReplaceLineRange { project, .. }
            | Self::InsertAtLine { project, .. }
            | Self::DeleteLineRange { project, .. } => Some(project.as_str()),
            _ => None,
        }
    }

    pub(crate) fn session_log_arguments(&self) -> Value {
        match self {
            Self::RunShell {
                project,
                timeout_secs,
                cwd,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "timeout_secs": timeout_secs,
                "cwd": cwd,
            }),
            Self::RunJob {
                project,
                timeout_secs,
                cwd,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "timeout_secs": timeout_secs,
                "cwd": cwd,
            }),
            Self::RunCodex {
                project,
                approval_mode,
                timeout_secs,
                cwd,
                extra_args,
                ..
            } => serde_json::json!({
                "project": project,
                "prompt_present": true,
                "approval_mode": approval_mode,
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "extra_args_count": extra_args.as_ref().map(Vec::len),
            }),
            Self::ApplyPatch { project, .. } => serde_json::json!({
                "project": project,
                "patch_present": true,
            }),
            Self::ApplyPatchChecked {
                project,
                deny_sensitive_paths,
                ..
            }
            | Self::ValidatePatch {
                project,
                deny_sensitive_paths,
                ..
            } => serde_json::json!({
                "project": project,
                "patch_present": true,
                "deny_sensitive_paths": deny_sensitive_paths,
            }),
            Self::DeleteProjectFiles { project, paths, .. }
            | Self::GitRestorePaths { project, paths, .. }
            | Self::DiscardUntracked { project, paths, .. } => serde_json::json!({
                "project": project,
                "paths": paths,
            }),
            Self::GitStatus { project, .. } | Self::GitDiffSummary { project, .. } => {
                serde_json::json!({
                    "project": project,
                })
            }
            Self::GitDiff { project, args, .. } => serde_json::json!({
                "project": project,
                "args_count": args.as_ref().map(Vec::len),
            }),
            Self::GitDiffHunks {
                project,
                paths,
                max_hunks,
                max_hunk_lines,
                cached,
                ..
            } => serde_json::json!({
                "project": project,
                "paths": paths,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "cached": cached,
            }),
            Self::CargoFmt {
                project,
                cwd,
                check,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "check": check,
                "timeout_secs": timeout_secs,
            }),
            Self::CargoCheck {
                project,
                cwd,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "all_targets": all_targets,
                "all_features": all_features,
                "no_default_features": no_default_features,
                "features_present": features.as_ref().is_some_and(|v| !v.is_empty()),
                "package": package,
                "timeout_secs": timeout_secs,
            }),
            Self::CargoTest {
                project,
                cwd,
                filter,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                no_run,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "filter_present": filter.as_ref().is_some_and(|v| !v.is_empty()),
                "all_targets": all_targets,
                "all_features": all_features,
                "no_default_features": no_default_features,
                "features_present": features.as_ref().is_some_and(|v| !v.is_empty()),
                "package": package,
                "no_run": no_run,
                "timeout_secs": timeout_secs,
            }),
            Self::ReadFile {
                project,
                path,
                start_line,
                limit,
                with_line_numbers,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "limit": limit,
                "with_line_numbers": with_line_numbers,
            }),
            Self::ListProjectFiles {
                project,
                path,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "limit": limit,
            }),
            Self::SearchProjectText {
                project,
                path,
                limit,
                context_before,
                context_after,
                ..
            } => serde_json::json!({
                "project": project,
                "pattern_present": true,
                "path": path,
                "limit": limit,
                "context_before": context_before,
                "context_after": context_after,
            }),
            Self::ShowChanges {
                project,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
                ..
            } => serde_json::json!({
                "project": project,
                "include_diff": include_diff,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "session_event_limit": session_event_limit,
            }),
            Self::ReplaceInFile {
                project,
                path,
                expected_replacements,
                allow_multiple,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "old_present": true,
                "new_present": true,
                "expected_replacements": expected_replacements,
                "allow_multiple": allow_multiple,
            }),
            Self::ReplaceExactBlock {
                project,
                path,
                expected_old_sha256,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "old_text_present": true,
                "new_text_present": true,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::InsertBeforePattern { project, path, .. }
            | Self::InsertAfterPattern { project, path, .. } => serde_json::json!({
                "project": project,
                "path": path,
                "pattern_present": true,
                "text_present": true,
            }),
            Self::WriteProjectFile {
                project,
                path,
                overwrite,
                expected_sha256,
                expected_content_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_present": true,
                "overwrite": overwrite,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_content_prefix_present": expected_content_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::SaveProjectArtifact {
                project,
                path,
                mime_type,
                overwrite,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_base64_present": true,
                "mime_type": mime_type,
                "overwrite": overwrite,
            }),
            Self::ReadProjectArtifactMetadata { project, path, .. } => serde_json::json!({
                "project": project,
                "path": path,
            }),
            Self::ReadProjectArtifact {
                project,
                path,
                encoding,
                offset,
                length,
                max_bytes,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "encoding": encoding,
                "offset": offset,
                "length": length,
                "max_bytes": max_bytes,
            }),
            Self::ReplaceLineRange {
                project,
                path,
                start_line,
                end_line,
                expected_old_sha256,
                expected_old_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
                "new_text_present": true,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_old_prefix_present": expected_old_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::InsertAtLine {
                project,
                path,
                line,
                expected_anchor_sha256,
                expected_anchor_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "text_present": true,
                "expected_anchor_sha256_present": expected_anchor_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_anchor_prefix_present": expected_anchor_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::DeleteLineRange {
                project,
                path,
                start_line,
                end_line,
                expected_old_sha256,
                expected_old_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_old_prefix_present": expected_old_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            _ => serde_json::json!({}),
        }
    }
}

// =============================================================================
// Tool output
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub success: bool,
    /// Main payload — always a JSON object so both MCP and GPT Actions
    /// can forward it verbatim.
    pub output: Value,
    /// Optional human-readable error when success == false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(msg.into()),
        }
    }

    pub fn err_with_output(msg: impl Into<String>, output: Value) -> Self {
        Self {
            success: false,
            output,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub annotations: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalJobRecord {
    pub(crate) project: String,
    pub(crate) dir: PathBuf,
}

/// Local job statuses that are still active (not yet terminal). A stop/timeout
/// only acts on these; terminal jobs (`completed`/`failed`/`stopped`/`lost`)
/// are left untouched.
pub(crate) const ACTIVE_LOCAL_STATUSES: &[&str] = &["running", "queued"];

/// Outcome of attempting to terminate a local job's process group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminateOutcome {
    /// The process group was alive and was signalled. `escalated_to_kill` is
    /// true when SIGTERM did not suffice within the grace window and SIGKILL
    /// was sent to the whole group.
    Terminated { pgid: i64, escalated_to_kill: bool },
    /// No live process was found for the recorded pid (already exited).
    AlreadyGone,
}

/// Abstraction over terminating a local job's process group.
///
/// The production implementation shells out to `kill -TERM/-KILL -<pgid>`
/// (negative pid => whole process group). Local jobs are spawned with
/// `setsid`, which makes the wrapper shell a session and process-group
/// leader, so `-<pgid>` reaches the wrapper and every descendant it spawned
/// in a single signal — reliably reclaiming the whole subtree rather than
/// orphaning children.
///
/// Tests inject a fake to assert the runtime targets the correct pgid without
/// spawning real processes. The runtime only ever passes pids/pgids read from
/// its own on-disk job files (never caller-supplied pids), so this trait is
/// never an arbitrary kill surface.
pub(crate) trait LocalJobKiller: Send + Sync {
    /// Terminate the process group led by `pid`/`pgid`. Sends SIGTERM, waits
    /// briefly, and escalates to SIGKILL if the leader is still alive. Never
    /// panics; a failure to signal is reflected as a `Terminated` outcome
    /// without escalation (the caller persists a conservative `lost` status
    /// regardless).
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome;
}

/// Production `LocalJobKiller` backed by the `kill` shell command.
pub(crate) struct SystemJobKiller;

impl SystemJobKiller {
    /// True if a process with `pid` is currently alive (`kill -0`).
    fn is_alive(pid: i64) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Send `signal` (e.g. `-TERM`/`-KILL`) to the whole process group `pgid`
    /// (negative pid). Failures are swallowed: a non-existent group yields a
    /// non-zero exit which we treat as "nothing left to signal".
    fn signal_group(pgid: i64, signal: &str) {
        match std::process::Command::new("kill")
            .arg(signal)
            .arg(format!("-{}", pgid))
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => {
                tracing::debug!(
                    pgid,
                    signal,
                    status = %status,
                    "local job process-group signal did not report success"
                );
            }
            Err(e) => {
                tracing::warn!(
                    pgid,
                    signal,
                    error = %e,
                    "failed to signal local job process group"
                );
            }
        }
    }
}

impl LocalJobKiller for SystemJobKiller {
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
        if !Self::is_alive(pid) {
            return TerminateOutcome::AlreadyGone;
        }
        Self::signal_group(pgid, "-TERM");
        // Bounded grace window for graceful shutdown, then escalate to SIGKILL.
        // This blocks the calling (async) thread for at most ~300ms and only
        // when a genuinely-alive overtime process is being reclaimed.
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if !Self::is_alive(pid) {
                return TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill: false,
                };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let escalated = Self::is_alive(pid);
        if escalated {
            Self::signal_group(pgid, "-KILL");
        }
        TerminateOutcome::Terminated {
            pgid,
            escalated_to_kill: escalated,
        }
    }
}

/// Capability an agent-backed tool requires. Kept private to the runtime;
/// only used by `authorize_agent_tool` to map a `ToolCall` variant to the
/// capability flag it needs on the agent client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentCapability {
    /// `run_shell`, `apply_patch` (agent path runs `git apply` via shell).
    Shell,
    /// `read_file` (agent path uses the file_read request kind).
    FileRead,
    /// Native file mutation requests handled by the agent.
    FileWrite,
    /// `git_status` / `git_diff` (agent path runs git via shell; accept either
    /// an explicit `git` capability or `shell`).
    GitOrShell,
    /// `run_job` / `run_codex` (agent path starts an async job).
    AsyncJobs,
}

// =============================================================================
// Runtime
// =============================================================================

/// Lightweight runtime metadata injected into `ToolRuntime` so observability
/// tools (e.g. `runtime_status`) can report auth/public-url state without the
/// runtime holding a full `Config` (which would couple it to HTTP/fs details).
///
/// `configured_public_url` is `None` when `WEBCODEX_PUBLIC_URL` is unset; the
/// observability output reports this as `null` so a deployer can immediately
/// see that the public URL has not been configured.
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub auth_enabled: bool,
    pub configured_public_url: Option<String>,
    pub quic: Option<std::sync::Arc<std::sync::Mutex<crate::config::QuicRuntimeStatus>>>,
}

impl RuntimeInfo {
    /// Build `RuntimeInfo` from the process environment. Reads
    /// `WEBCODEX_TOKEN` (presence) and `WEBCODEX_PUBLIC_URL`.
    // Kept for the server binary and tests; the agent-only binary builds this
    // module without wiring runtime HTTP metadata, so it is intentionally idle
    // in that compile unit.
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        Self::from_env_with_quic_config(&crate::config::QuicServerConfig::from_env())
    }

    pub fn from_env_with_quic_config(quic_cfg: &crate::config::QuicServerConfig) -> Self {
        let auth_enabled = std::env::var("WEBCODEX_TOKEN")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let configured_public_url = std::env::var("WEBCODEX_PUBLIC_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        Self {
            auth_enabled,
            configured_public_url,
            quic: Some(std::sync::Arc::new(std::sync::Mutex::new(
                quic_cfg.runtime_status(),
            ))),
        }
    }
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            auth_enabled: false,
            configured_public_url: None,
            quic: Some(std::sync::Arc::new(std::sync::Mutex::new(
                crate::config::QuicServerConfig::default().runtime_status(),
            ))),
        }
    }
}

/// Statuses counted as "active" by the runtime_status observability summary.
/// A job is active when it is still in flight: queued for an agent, running,
/// or has been asked to stop but has not terminated yet.
pub(crate) const ACTIVE_JOB_STATUSES: &[&str] =
    &["running", "queued", "agent_queued", "stop_requested"];
