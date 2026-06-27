use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// =============================================================================
// Tool input — one variant per tool
// =============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "tool", content = "params", rename_all = "snake_case")]
pub enum ToolCall {
    /// List registered tool runtime tools.
    ListTools,

    /// Execute a shell command in a project directory (sync, short-lived).
    RunShell {
        project: String,
        command: String,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
    },

    /// Apply a unified diff patch to a project.
    ApplyPatch { project: String, patch: String },

    /// Validate then apply a unified diff patch in one safer full-auto step.
    ApplyPatchChecked {
        project: String,
        patch: String,
        #[serde(default)]
        deny_sensitive_paths: Option<bool>,
    },

    /// Delete project-relative files only (not directories).
    DeleteProjectFiles { project: String, paths: Vec<String> },

    /// Restore tracked paths with `git restore -- <paths>`.
    GitRestorePaths { project: String, paths: Vec<String> },

    /// Discard selected untracked files with `git clean -f -- <paths>`.
    DiscardUntracked { project: String, paths: Vec<String> },

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
        deny_sensitive_paths: Option<bool>,
    },

    /// Run `git status` on a project.
    GitStatus { project: String },

    /// Run `git diff` on a project.
    GitDiff {
        project: String,
        #[serde(default)]
        args: Option<Vec<String>>,
    },

    /// Read a file from a project.
    ReadFile {
        project: String,
        path: String,
        #[serde(default)]
        start_line: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Start an async background job (long-running commands, codex CLI, etc.).
    RunJob {
        project: String,
        command: String,
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
        path: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Read-only git diff summary for a project: `git status --porcelain`,
    /// `git diff --stat`, and a parsed changed-file list. Does not modify the
    /// worktree. Routed to the owning agent.
    GitDiffSummary { project: String },

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
        expected_replacements: Option<i64>,
        #[serde(default)]
        allow_multiple: Option<bool>,
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
        overwrite: Option<bool>,
        #[serde(default)]
        expected_sha256: Option<String>,
        #[serde(default)]
        expected_content_prefix: Option<String>,
    },

    /// List all agent-registered runtime projects.
    ListProjects,

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
    "run_shell",
    "apply_patch",
    "apply_patch_checked",
    "delete_project_files",
    "git_restore_paths",
    "discard_untracked",
    "validate_patch",
    "replace_in_file",
    "write_project_file",
    "git_status",
    "git_diff",
    "read_file",
    "run_job",
    "run_codex",
    "job_status",
    "job_log",
    "list_project_files",
    "search_project_text",
    "git_diff_summary",
    "list_jobs",
    "job_tail",
    "list_projects",
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
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
        let _ = std::process::Command::new("kill")
            .arg(signal)
            .arg(format!("-{}", pgid))
            .status();
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
}

impl RuntimeInfo {
    /// Build `RuntimeInfo` from the process environment. Reads
    /// `WEBCODEX_TOKEN` (presence) and `WEBCODEX_PUBLIC_URL`.
    pub fn from_env() -> Self {
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
        }
    }
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            auth_enabled: false,
            configured_public_url: None,
        }
    }
}

/// Statuses counted as "active" by the runtime_status observability summary.
/// A job is active when it is still in flight: queued for an agent, running,
/// or has been asked to stop but has not terminated yet.
pub(crate) const ACTIVE_JOB_STATUSES: &[&str] =
    &["running", "queued", "agent_queued", "stop_requested"];
