//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

use crate::auth::AuthContext;
use crate::config::CodexConfig;
use crate::projects::{Executor, ProjectConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentProjectSummary, ShellFileOpRequest, ShellJobInfo, ShellJobOpRequest, ShellRunRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

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
struct LocalJobRecord {
    project: String,
    dir: PathBuf,
}

/// Local job statuses that are still active (not yet terminal). A stop/timeout
/// only acts on these; terminal jobs (`completed`/`failed`/`stopped`/`lost`)
/// are left untouched.
const ACTIVE_LOCAL_STATUSES: &[&str] = &["running", "queued"];

/// Outcome of attempting to terminate a local job's process group.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminateOutcome {
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
trait LocalJobKiller: Send + Sync {
    /// Terminate the process group led by `pid`/`pgid`. Sends SIGTERM, waits
    /// briefly, and escalates to SIGKILL if the leader is still alive. Never
    /// panics; a failure to signal is reflected as a `Terminated` outcome
    /// without escalation (the caller persists a conservative `lost` status
    /// regardless).
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome;
}

/// Production `LocalJobKiller` backed by the `kill` shell command.
struct SystemJobKiller;

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
enum AgentCapability {
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
/// `configured_public_url` is `None` when `DROP_PUBLIC_URL` is unset; the
/// observability output reports this as `null` so a deployer can immediately
/// see that the public URL has not been configured.
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub auth_enabled: bool,
    pub configured_public_url: Option<String>,
}

impl RuntimeInfo {
    /// Build `RuntimeInfo` from the process environment. Reads `DROP_TOKEN`
    /// (presence) and `DROP_PUBLIC_URL`.
    pub fn from_env() -> Self {
        let auth_enabled = std::env::var("DROP_TOKEN")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let configured_public_url = std::env::var("DROP_PUBLIC_URL")
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
const ACTIVE_JOB_STATUSES: &[&str] = &["running", "queued", "agent_queued", "stop_requested"];

#[derive(Clone)]
pub struct ToolRuntime {
    pub projects: Arc<ProjectsState>,
    pub shell_clients: Arc<ShellClientRegistry>,
    pub codex: Arc<CodexConfig>,
    pub runtime_info: Arc<RuntimeInfo>,
    local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
    job_killer: Arc<dyn LocalJobKiller>,
}

impl ToolRuntime {
    pub fn new(
        projects: Arc<ProjectsState>,
        shell_clients: Arc<ShellClientRegistry>,
        codex: Arc<CodexConfig>,
        runtime_info: Arc<RuntimeInfo>,
    ) -> Self {
        Self {
            projects,
            shell_clients,
            codex,
            runtime_info,
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
        }
    }

    fn agent_project_runtime_id(client_id: &str, project_id: &str) -> String {
        format!("agent:{}:{}", client_id, project_id)
    }

    fn agent_project_config(client_id: &str, project: &ShellAgentProjectSummary) -> ProjectConfig {
        ProjectConfig {
            path: project.path.clone(),
            executor: Executor::Agent,
            client_id: Some(client_id.to_string()),
            allow_patch: project.allow_patch,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    async fn resolve_agent_registered_project(
        &self,
        project: &str,
    ) -> Result<Option<ProjectConfig>, String> {
        let Some(rest) = project.strip_prefix("agent:") else {
            return Ok(None);
        };
        let Some((client_id, agent_project_id)) = rest.split_once(':') else {
            return Err("agent project ids must use agent:<client_id>:<project_id>".to_string());
        };
        if client_id.trim().is_empty() || agent_project_id.trim().is_empty() {
            return Err("agent project ids must use agent:<client_id>:<project_id>".to_string());
        }
        let client = self
            .shell_clients
            .get_client_view(client_id)
            .await
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        let Some(project_summary) = client.projects.iter().find(|p| p.id == agent_project_id)
        else {
            return Err(format!(
                "agent project '{}' is not registered by client '{}'",
                agent_project_id, client_id
            ));
        };
        if project_summary.disabled {
            return Err(format!(
                "agent project '{}' on client '{}' is disabled",
                agent_project_id, client_id
            ));
        }
        Ok(Some(Self::agent_project_config(
            &client.client_id,
            project_summary,
        )))
    }

    async fn resolve_project(&self, project: &str) -> Result<ProjectConfig, String> {
        if let Some(project) = self.resolve_agent_registered_project(project).await? {
            return Ok(project);
        }
        Err(format!(
            "Unknown project '{}'. Server-side projects.toml is not used by the runtime surface; use an agent-registered id like agent:<client_id>:<project_id> from listProjects.",
            project
        ))
    }

    /// The capability an agent-backed tool variant requires from the agent
    /// client. Non-agent tools (and tools without a project) require nothing.
    fn required_agent_capability(call: &ToolCall) -> Option<AgentCapability> {
        match call {
            ToolCall::RunShell { .. }
            | ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyPatchChecked { .. }
            | ToolCall::DeleteProjectFiles { .. }
            | ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. } => Some(AgentCapability::Shell),
            // validate_patch runs read-only `git apply --check`/`--stat` via
            // the agent shell path; it requires the same shell capability as
            // apply_patch but never mutates the worktree.
            ToolCall::ValidatePatch { .. } => Some(AgentCapability::Shell),
            ToolCall::ReadFile { .. } | ToolCall::ListProjectFiles { .. } => {
                Some(AgentCapability::FileRead)
            }
            // Search runs a bounded `grep` via the agent shell path.
            ToolCall::SearchProjectText { .. } => Some(AgentCapability::Shell),
            ToolCall::GitStatus { .. }
            | ToolCall::GitDiff { .. }
            | ToolCall::GitDiffSummary { .. } => Some(AgentCapability::GitOrShell),
            ToolCall::RunJob { .. } | ToolCall::RunCodex { .. } => Some(AgentCapability::AsyncJobs),
            ToolCall::ListTools
            | ToolCall::ListProjects
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. } => None,
        }
    }

    /// Enforce the owner boundary and capability requirements for agent-backed
    /// runtime tools before dispatching. This is the single place where the
    /// runtime paths (`/api/tools/call`, `/api/codex/run`, `/api/projects/*`,
    /// `/mcp`) check that the caller is allowed to drive an agent. Legacy
    /// `/api/shell/*` handlers keep their own `assert_shell_client_owner`
    /// checks; this method closes the gap for the runtime paths.
    ///
    /// Returns `Ok(())` for local-executor projects and project-less tools so
    /// they are unaffected.
    async fn authorize_agent_tool(
        &self,
        call: &ToolCall,
        auth: Option<&AuthContext>,
    ) -> Result<(), String> {
        let project = match call {
            ToolCall::RunShell { project, .. }
            | ToolCall::ApplyPatch { project, .. }
            | ToolCall::ApplyPatchChecked { project, .. }
            | ToolCall::DeleteProjectFiles { project, .. }
            | ToolCall::GitRestorePaths { project, .. }
            | ToolCall::DiscardUntracked { project, .. }
            | ToolCall::ValidatePatch { project, .. }
            | ToolCall::GitStatus { project }
            | ToolCall::GitDiff { project, .. }
            | ToolCall::GitDiffSummary { project }
            | ToolCall::ReadFile { project, .. }
            | ToolCall::ListProjectFiles { project, .. }
            | ToolCall::SearchProjectText { project, .. }
            | ToolCall::RunJob { project, .. }
            | ToolCall::RunCodex { project, .. } => project,
            _ => return Ok(()),
        };
        let required = match Self::required_agent_capability(call) {
            Some(cap) => cap,
            None => return Ok(()),
        };
        let proj = self.resolve_project(project).await?;
        if !proj.is_agent() {
            return Ok(());
        }
        let client_id = proj.agent_client_id()?.to_string();
        let view = self
            .shell_clients
            .get_client_view(&client_id)
            .await
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        // Owner boundary: bootstrap tokens and dev mode (auth disabled) pass.
        // Otherwise the API key username must match the agent's declared owner.
        crate::shell_client::assert_shell_client_owner(auth, &client_id, view.owner.as_deref())?;
        // Capability check via the registry helper so the requirement is
        // expressed as a named capability, not a raw struct field access.
        let supported = match required {
            AgentCapability::Shell => {
                self.shell_clients
                    .client_supports(&client_id, "shell")
                    .await?
            }
            AgentCapability::FileRead => {
                self.shell_clients
                    .client_supports(&client_id, "file_read")
                    .await?
            }
            AgentCapability::GitOrShell => {
                self.shell_clients
                    .client_supports(&client_id, "shell")
                    .await?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "git")
                        .await?
            }
            AgentCapability::AsyncJobs => {
                self.shell_clients
                    .client_supports(&client_id, "async_jobs")
                    .await?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "async_shell_jobs")
                        .await?
            }
        };
        if !supported {
            let label = match required {
                AgentCapability::Shell => "shell",
                AgentCapability::FileRead => "file_read",
                AgentCapability::GitOrShell => "shell or git",
                AgentCapability::AsyncJobs => "async shell jobs",
            };
            return Err(format!(
                "agent client {} does not support {}",
                client_id, label
            ));
        }
        Ok(())
    }

    /// Main dispatch — call from MCP handler or GPT Actions handler.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means agent-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. `dispatch` is kept for internal/tests callers that only
    /// use local-executor projects.
    #[allow(dead_code)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Dispatch carrying the caller's auth context. Agent-backed tools enforce
    /// the owner boundary and capability requirements through
    /// `authorize_agent_tool`; local-executor tools are unaffected. Wrappers
    /// stay thin: they only forward the depot `AuthContext` here.
    pub async fn dispatch_with_auth(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(err) = self.authorize_agent_tool(&call, auth).await {
            return ToolResult::err(err);
        }
        match call {
            ToolCall::ListTools => ToolResult::ok(json!({ "tools": self.tool_specs() })),

            ToolCall::ListProjects => self.list_projects().await,

            ToolCall::ListAgents => self.list_agents().await,

            ToolCall::RuntimeStatus => self.runtime_status().await,

            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
            } => self.run_shell(project, command, timeout_secs, cwd).await,

            ToolCall::ApplyPatch { project, patch } => self.apply_patch(project, patch).await,

            ToolCall::ApplyPatchChecked {
                project,
                patch,
                deny_sensitive_paths,
            } => {
                self.apply_patch_checked(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::DeleteProjectFiles { project, paths } => {
                self.delete_project_files(project, paths).await
            }

            ToolCall::GitRestorePaths { project, paths } => {
                self.git_restore_paths(project, paths).await
            }

            ToolCall::DiscardUntracked { project, paths } => {
                self.discard_untracked(project, paths).await
            }

            ToolCall::ValidatePatch {
                project,
                patch,
                deny_sensitive_paths,
            } => {
                self.validate_patch(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::GitStatus { project } => self.git_status(project).await,

            ToolCall::GitDiff { project, args } => self.git_diff(project, args).await,

            ToolCall::ReadFile {
                project,
                path,
                start_line,
                limit,
            } => self.read_file(project, path, start_line, limit).await,

            ToolCall::RunJob {
                project,
                command,
                timeout_secs,
                cwd,
            } => self.run_job(project, command, timeout_secs, cwd).await,

            ToolCall::RunCodex {
                project,
                prompt,
                approval_mode,
                timeout_secs,
                cwd,
                extra_args,
            } => {
                self.run_codex(
                    project,
                    prompt,
                    approval_mode,
                    timeout_secs,
                    cwd,
                    extra_args,
                )
                .await
            }

            ToolCall::JobStatus { job_id } => self.job_status(job_id).await,

            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => self.job_log(job_id, offset, tail_lines).await,

            ToolCall::ListProjectFiles {
                project,
                path,
                limit,
            } => self.list_project_files(project, path, limit).await,

            ToolCall::SearchProjectText {
                project,
                pattern,
                path,
                limit,
            } => {
                self.search_project_text(project, pattern, path, limit)
                    .await
            }

            ToolCall::GitDiffSummary { project } => self.git_diff_summary(project).await,

            ToolCall::ListJobs { limit, status } => self.list_jobs(limit, status).await,

            ToolCall::JobTail { job_id, tail_lines } => self.job_tail(job_id, tail_lines).await,
        }
    }

    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list_tools".to_string(),
                description: "List tools exposed by this Private Drop runtime.".to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "list_projects".to_string(),
                description: "List agent-registered runtime projects and their execution mode."
                    .to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "list_agents".to_string(),
                description: "List connected local/remote execution agents.".to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "runtime_status".to_string(),
                description: "Return a structured runtime health/observability summary (service "
                    .to_string()
                    + "metadata, projects config status, agent client summaries, and job counts). "
                    + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "run_shell".to_string(),
                description: "Run a short shell command inside an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
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
                ]),
            },
            ToolSpec {
                name: "run_job".to_string(),
                description: "Start an asynchronous shell job inside an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
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
                ]),
            },
            ToolSpec {
                name: "run_codex".to_string(),
                description: "Start Codex CLI as an asynchronous project job.".to_string(),
                input_schema: object_schema(vec![
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
                ]),
            },
            ToolSpec {
                name: "job_status".to_string(),
                description: "Get status for a runtime job.".to_string(),
                input_schema: object_schema(vec![("job_id", "string", "Job id.", true)]),
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
            },
            ToolSpec {
                name: "list_project_files".to_string(),
                description: "List files in an agent-registered project directory (bounded, "
                    .to_string()
                    + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                    + "to the owning registered agent; the server never reads the agent project "
                    + "path directly.",
                input_schema: object_schema(vec![
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
                ]),
            },
            ToolSpec {
                name: "search_project_text".to_string(),
                description: "Search text inside an agent-registered project (bounded matches)."
                    .to_string()
                    + " Each match carries a project-relative path, 1-based line number, and a "
                    + "preview line. Sensitive/build directories (.git, target, node_modules) are "
                    + "excluded by default.",
                input_schema: object_schema(vec![
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
                ]),
            },
            ToolSpec {
                name: "git_diff_summary".to_string(),
                description: "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                input_schema: object_schema(vec![(
                    "project",
                    "string",
                    "Agent-registered project id.",
                    true,
                )]),
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
            },
            ToolSpec {
                name: "read_file".to_string(),
                description: "Read a UTF-8 file from an agent-registered project.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based line offset.", false),
                    ("limit", "integer", "Maximum line count.", false),
                ]),
            },
            ToolSpec {
                name: "git_status".to_string(),
                description: "Run git status --porcelain for a project.".to_string(),
                input_schema: object_schema(vec![(
                    "project",
                    "string",
                    "Configured project id.",
                    true,
                )]),
            },
            ToolSpec {
                name: "git_diff".to_string(),
                description: "Run git diff for a project, optionally scoped to paths.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("args", "array", "Optional path list.", false),
                ]),
            },
            ToolSpec {
                name: "apply_patch".to_string(),
                description: "Apply a unified diff patch to an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("patch", "string", "Unified diff patch.", true),
                ]),
            },
            ToolSpec {
                name: "apply_patch_checked".to_string(),
                description: "Validate a patch, apply it only if it can apply, then return the post-apply diff summary.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", "Unified diff patch.", true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings before applying.", false),
                ]),
            },
            ToolSpec {
                name: "delete_project_files".to_string(),
                description: "Delete selected project-relative files only; safer than arbitrary rm for cleanup.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative file paths to delete.", true),
                ]),
            },
            ToolSpec {
                name: "git_restore_paths".to_string(),
                description: "Restore selected tracked paths with git restore; does not remove untracked files.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative tracked paths to restore.", true),
                ]),
            },
            ToolSpec {
                name: "discard_untracked".to_string(),
                description: "Discard selected untracked files with git clean -f -- <paths>.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative untracked paths to remove.", true),
                ]),
            },
            ToolSpec {
                name: "validate_patch".to_string(),
                description: "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", "Unified diff patch to validate.", true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings.", false),
                ]),
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
                "git_status", "git_diff", "git_diff_summary"
            ]),
            "git": pick(&[
                "git_status", "git_diff", "git_diff_summary",
                "git_restore_paths", "discard_untracked"
            ]),
            "patch": pick(&["apply_patch", "apply_patch_checked", "validate_patch"]),
            "shell": pick(&["run_shell", "run_job"]),
            "jobs": pick(&[
                "run_codex", "run_job", "job_status", "job_log",
                "list_jobs", "job_tail"
            ]),
            "runtime": pick(&[
                "list_tools", "list_projects", "list_agents", "runtime_status"
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
            "Inspect: use git_diff_summary then read_file before proposing changes.",
            "Patch: call validate_patch to dry-run, then apply_patch_checked to apply safely.",
            "Cleanup: use delete_project_files / git_restore_paths / discard_untracked instead of ad hoc rm.",
            "Jobs: start run_codex, then poll job_status and read job_log/job_tail.",
        ]
    }

    // -------------------------------------------------------------------------
    // Individual tool implementations — delegate to existing codex/ functions
    // and shell_client handlers.
    // -------------------------------------------------------------------------

    async fn list_projects(&self) -> ToolResult {
        let mut list: Vec<Value> = Vec::new();
        for client in self.shell_clients.list_clients().await {
            for project in client.projects.iter().filter(|p| !p.disabled) {
                list.push(json!({
                    "id": Self::agent_project_runtime_id(&client.client_id, &project.id),
                    "agent_project_id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "executor": "agent",
                    "client_id": client.client_id,
                    "allow_patch": project.allow_patch,
                    "source": "agent_registered",
                }));
            }
        }
        list.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        ToolResult::ok(Value::Array(list))
    }

    async fn list_agents(&self) -> ToolResult {
        ToolResult::ok(json!({
            "agents": self.shell_clients.list_clients().await
        }))
    }

    /// Build the runtime observability summary. Read-only; never exposes
    /// tokens, api keys, full env, complete project path lists, or
    /// stdout/stderr. Returns a structured JSON object with service metadata,
    /// project config status, agent client summaries, and job counts.
    async fn runtime_status(&self) -> ToolResult {
        // -- projects summary -------------------------------------------------
        let (projects_configured, projects_count, projects_load_error) =
            match self.projects.config.as_ref() {
                Some(cfg) => (true, cfg.projects.len(), None),
                None => (
                    false,
                    0,
                    self.projects
                        .load_error
                        .clone()
                        .or_else(|| Some("Projects not configured".to_string())),
                ),
            };
        let projects = json!({
            "configured": projects_configured,
            "count": projects_count,
            "config_path": self.projects.config_path,
            "load_error": projects_load_error,
        });

        // -- agents summary ---------------------------------------------------
        // Build a trimmed client list so the summary never leaks per-request
        // state. Only carry fields useful for observability. `last_seen` is a
        // unix timestamp (seconds) of the most recent heartbeat/result; the
        // console uses it to render how stale an agent is and to make a
        // websocket agent flipping `online` -> `stale` visually obvious.
        let clients = self.shell_clients.list_clients().await;
        let agent_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        // `stale_count` = registered agents whose `last_seen` is older than the
        // online window (status == "stale"). Truly offline agents are removed
        // from the registry on disconnect, so they never appear here; the
        // legacy `offline_count` field is retained (it mirrors `stale_count`
        // for the registered set) for backward compatibility with existing
        // callers/tests.
        let stale_count = agent_count.saturating_sub(online_count);
        let offline_count = stale_count;
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "pending_requests": c.pending_requests,
                    "capabilities": c.capabilities,
                    "projects_count": c.projects.len(),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "offline_count": offline_count,
            "clients": clients_summary,
        });

        // -- jobs summary -----------------------------------------------------
        // Agent-known jobs come from the registry; local jobs come from the
        // in-memory map. Active = running/queued/agent_queued/stop_requested.
        let agent_jobs = self.shell_clients.list_jobs(None).await;
        let agent_known_count = agent_jobs.len();
        let local_jobs_map = self.local_jobs.lock().await;
        let local_known_count = local_jobs_map.len();
        // Avoid double-counting: agent jobs are tracked separately from local
        // jobs (local jobs are only in the in-memory map; agent jobs are only
        // in the registry). Count active across both.
        let agent_active = agent_jobs
            .iter()
            .filter(|j| ACTIVE_JOB_STATUSES.contains(&j.status.as_str()))
            .count();
        let mut local_active = 0usize;
        for record in local_jobs_map.values() {
            if let Some(status) = std::fs::read_to_string(record.dir.join("status"))
                .ok()
                .map(|s| s.trim().to_string())
            {
                let normalized = normalize_local_status(&status);
                if ACTIVE_JOB_STATUSES.contains(&normalized.as_str()) {
                    local_active += 1;
                }
            }
        }
        let active_count = agent_active + local_active;
        let jobs = json!({
            "agent_known_count": agent_known_count,
            "local_known_count": local_known_count,
            "active_count": active_count,
        });

        // -- tools summary ----------------------------------------------------
        let specs = self.tool_specs();
        let tools_count = specs.len();
        let tools_names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        let tools = json!({
            "count": tools_count,
            "names": tools_names,
        });

        ToolResult::ok(json!({
            "service": "private-drop",
            "version": env!("CARGO_PKG_VERSION"),
            "server_time": chrono::Utc::now().timestamp(),
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            "agents": agents,
            "jobs": jobs,
            "tools": tools,
        }))
    }

    async fn run_shell(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let timeout = timeout_secs.unwrap_or(60).max(1);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let effective_cwd = cwd.or_else(|| Some(proj.path.clone()));
            let wait_timeout = timeout.min(120);
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: effective_cwd,
                        command,
                        stdin: None,
                        timeout_secs: timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(result) => result,
                Err(e) => return ToolResult::err(e),
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let success = response.error.is_none() && response.exit_code == Some(0);
                    let output = json!({
                        "exit_code": response.exit_code,
                        "stdout": response.stdout,
                        "stderr": response.stderr,
                        "duration_ms": response.duration_ms,
                    });
                    if success {
                        ToolResult::ok(output)
                    } else {
                        ToolResult::err(
                            response
                                .error
                                .unwrap_or_else(|| "command failed".to_string()),
                        )
                    }
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("shell request waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err(format!(
                        "timed out waiting {} seconds for agent shell result",
                        wait_timeout
                    ))
                }
            }
        } else {
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            };
            let result = tokio::task::spawn_blocking({
                let cmd = command;
                move || run_command_sync(&cmd, &cwd_path, timeout)
            })
            .await;
            match result {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    let success = exit_code == 0;
                    let output = json!({
                        "exit_code": exit_code,
                        "stdout": stdout,
                        "stderr": stderr,
                        "duration_ms": duration_ms,
                    });
                    if success {
                        ToolResult::ok(output)
                    } else {
                        ToolResult::err(format!("command exited with code {}", exit_code))
                    }
                }
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    async fn apply_patch(&self, project: String, patch: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.allow_patch() {
            return ToolResult::err("Patch is not allowed for this project");
        }
        if patch.is_empty() {
            return ToolResult::err("Patch cannot be empty");
        }
        let changed = parse_changed_files_from_patch(&patch);
        if changed.is_empty() {
            return ToolResult::err("Patch does not declare any changed files");
        }
        for file in &changed {
            if let Err(e) = validate_patch_file_path(file) {
                return ToolResult::err(e);
            }
        }
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (check_req_id, check_rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id: client_id.clone(),
                        cwd: Some(proj.path.clone()),
                        command: "git apply --check - && echo OK".to_string(),
                        stdin: Some(patch.clone()),
                        timeout_secs: 60,
                        wait_timeout_secs: 62,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            let check_result = tokio::time::timeout(Duration::from_secs(64), check_rx).await;
            match check_result {
                Ok(Ok(resp)) if resp.exit_code != Some(0) => {
                    return ToolResult::ok(json!({
                        "success": false,
                        "changed_files": changed,
                        "stdout": resp.stdout,
                        "stderr": resp.stderr,
                        "error": "git apply --check failed",
                    }));
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&check_req_id).await;
                    return ToolResult::err("timed out during patch validation");
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&check_req_id).await;
                    return ToolResult::err("patch validation request dropped");
                }
                _ => {}
            }
            let (apply_req_id, apply_rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: "git apply -".to_string(),
                        stdin: Some(patch.clone()),
                        timeout_secs: 60,
                        wait_timeout_secs: 62,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            match tokio::time::timeout(Duration::from_secs(64), apply_rx).await {
                Ok(Ok(resp)) => {
                    let success = resp.exit_code == Some(0);
                    ToolResult::ok(json!({
                        "success": success,
                        "changed_files": changed,
                        "stdout": resp.stdout,
                        "stderr": resp.stderr,
                    }))
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&apply_req_id).await;
                    ToolResult::err("apply request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&apply_req_id).await;
                    ToolResult::err("timed out applying patch")
                }
            }
        } else {
            let root = proj.root();
            if !root.exists() {
                return ToolResult::err("Project root does not exist");
            }
            let patch_clone = patch.clone();
            let root_clone = root.clone();
            let result =
                tokio::task::spawn_blocking(move || apply_patch_local(&root_clone, &patch_clone))
                    .await;
            match result {
                Ok(Ok((success, stdout, stderr))) => ToolResult::ok(json!({
                    "success": success,
                    "changed_files": changed,
                    "stdout": stdout,
                    "stderr": stderr,
                })),
                Ok(Err(e)) => ToolResult::err(e),
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    async fn apply_patch_checked(
        &self,
        project: String,
        patch: String,
        deny_sensitive_paths: Option<bool>,
    ) -> ToolResult {
        let deny = deny_sensitive_paths.unwrap_or(true);
        let validate = self
            .validate_patch(project.clone(), patch.clone(), Some(deny))
            .await;
        if !validate.success {
            return validate;
        }
        let can_apply = validate.output["can_apply"].as_bool().unwrap_or(false);
        if !can_apply {
            return ToolResult::ok(json!({
                "applied": false,
                "validate": validate.output,
                "apply": Value::Null,
                "diff_summary": Value::Null,
            }));
        }
        let apply = self.apply_patch(project.clone(), patch).await;
        if !apply.success {
            return ToolResult::ok(json!({
                "applied": false,
                "validate": validate.output,
                "apply": apply,
                "diff_summary": Value::Null,
            }));
        }
        let diff_summary = self.git_diff_summary(project).await;
        ToolResult::ok(json!({
            "applied": apply.output["success"].as_bool().unwrap_or(false),
            "validate": validate.output,
            "apply": apply.output,
            "diff_summary": diff_summary.output,
        }))
    }

    async fn delete_project_files(&self, project: String, paths: Vec<String>) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("rm -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "deleted_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    async fn git_restore_paths(&self, project: String, paths: Vec<String>) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("git restore -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "restored_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    async fn discard_untracked(&self, project: String, paths: Vec<String>) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("git clean -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "discarded_untracked_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    /// Validate (preflight) a unified diff patch against an agent-registered
    /// project without applying it.
    ///
    /// This is a **dry-run only** tool: it runs `git apply --check` (to test
    /// applicability) and `git apply --stat` (to produce a summary) through
    /// the owning agent's shell execution path. It never invokes the real
    /// `git apply` application mode, never falls back to `apply_patch`, and
    /// never modifies the worktree. The server never reads the agent project
    /// filesystem directly — all checks are routed to the owning agent.
    ///
    /// Input is validated up front (empty / NUL / oversized patches are
    /// rejected before project resolution) so bounds checks are independent
    /// of routing. Sensitive filenames produce `warnings` rather than a hard
    /// reject; absolute paths and `..` traversal are hard-rejected so the
    /// preflight never escapes the project boundary.
    async fn validate_patch(
        &self,
        project: String,
        patch: String,
        deny_sensitive_paths: Option<bool>,
    ) -> ToolResult {
        // ---- Input validation (before any project resolution) ----
        if patch.is_empty() {
            return ToolResult::err("Patch cannot be empty");
        }
        if patch.contains('\0') {
            return ToolResult::err("Patch contains NUL byte");
        }
        if patch.len() > MAX_VALIDATE_PATCH_BYTES {
            return ToolResult::err(format!(
                "Patch too large ({} bytes); maximum is {} bytes",
                patch.len(),
                MAX_VALIDATE_PATCH_BYTES
            ));
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.allow_patch() {
            return ToolResult::err("Patch is not allowed for this project");
        }
        // ---- Patch path analysis ----
        let affected = parse_changed_files_from_patch(&patch);
        if affected.is_empty() {
            return ToolResult::err("Patch does not declare any changed files");
        }
        // Hard-reject paths that escape the project boundary. Collect warnings
        // for sensitive filenames. Callers can request a hard policy block so
        // full-auto loops do not accidentally ignore sensitive-path warnings.
        let mut warnings: Vec<String> = Vec::new();
        for file in &affected {
            if let Err(e) = validate_preflight_path(file) {
                return ToolResult::err(e);
            }
            warnings.extend(sensitive_path_warnings(file));
        }
        warnings.sort();
        warnings.dedup();
        let deny_sensitive = deny_sensitive_paths.unwrap_or(false);
        if deny_sensitive && !warnings.is_empty() {
            return ToolResult::ok(json!({
                "can_apply": false,
                "policy_blocked": true,
                "affected_files": affected,
                "stat": Value::Null,
                "stdout": Value::Null,
                "stderr": "sensitive path policy blocked patch preflight",
                "warnings": warnings,
            }));
        }

        // ---- Agent routing ----
        // validate_patch must run through the owning agent; the server never
        // reads the agent project filesystem directly, and server-configured
        // legacy projects are not a supported runtime surface for this tool.
        if !proj.is_agent() {
            return ToolResult::err(
                "validate_patch requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        // ---- 1) git apply --check (read-only applicability test) ----
        let (check_req_id, check_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id: client_id.clone(),
                    cwd: Some(proj.path.clone()),
                    command: "git apply --check -".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        let check_resp = match tokio::time::timeout(Duration::from_secs(64), check_rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("patch validation request dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("timed out during patch validation");
            }
        };
        let can_apply = check_resp.exit_code == Some(0);
        let check_stdout = check_resp.stdout.clone();
        let check_stderr = check_resp.stderr.clone();

        // ---- 2) git apply --stat (read-only summary) ----
        // `--stat` only parses the diff and prints a summary; it does not
        // check applicability and does not write files. It works regardless
        // of `can_apply`, so the caller always gets a summary.
        let (stat_req_id, stat_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id,
                    cwd: Some(proj.path.clone()),
                    command: "git apply --stat -".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        let stat_resp = match tokio::time::timeout(Duration::from_secs(64), stat_rx).await {
            Ok(Ok(resp)) => resp.stdout,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&stat_req_id).await;
                None
            }
            Err(_) => {
                self.shell_clients.cancel_request(&stat_req_id).await;
                None
            }
        };

        // ---- Structured result ----
        // ToolResult.success reflects whether the *validation* ran cleanly;
        // `can_apply` reports whether the patch would apply. A non-applicable
        // patch is a normal preflight outcome (success=true, can_apply=false),
        // not a tool error, so the agent loop can read it and regenerate.
        ToolResult::ok(json!({
            "can_apply": can_apply,
            "policy_blocked": false,
            "affected_files": affected,
            "stat": stat_resp,
            "stdout": check_stdout,
            "stderr": check_stderr,
            "warnings": warnings,
        }))
    }

    async fn git_status(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: "git status --porcelain".to_string(),
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 32,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => ToolResult::ok(json!({
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                    "exit_code": resp.exit_code,
                })),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            }
        } else {
            let root = proj.root();
            let result = tokio::task::spawn_blocking(move || {
                run_command_sync("git status --porcelain", &root, 30)
            })
            .await;
            match result {
                Ok((exit_code, stdout, stderr, _)) => ToolResult::ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })),
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    async fn git_diff(&self, project: String, args: Option<Vec<String>>) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let diff_args = args.unwrap_or_default();
        let cmd = if diff_args.is_empty() {
            "git diff".to_string()
        } else {
            let escaped: Vec<String> = diff_args.iter().map(|a| shell_escape_simple(a)).collect();
            format!("git diff -- {}", escaped.join(" "))
        };
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: cmd,
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 32,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => ToolResult::ok(json!({
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                    "exit_code": resp.exit_code,
                })),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            }
        } else {
            let root = proj.root();
            let result =
                tokio::task::spawn_blocking(move || run_command_sync(&cmd, &root, 30)).await;
            match result {
                Ok((exit_code, stdout, stderr, _)) => ToolResult::ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })),
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    async fn read_file(
        &self,
        project: String,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "read".to_string(),
                        client_id,
                        path: path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        max_bytes: Some(512 * 1024),
                        expected_sha256: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    read_file_content_result(resp.stdout.unwrap_or_default(), start_line, limit)
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent read_file failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent read_file waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent read_file")
                }
            };
        }
        let file_path = proj.root().join(&path);
        let canonical_root = match proj.root().canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        if !canonical.is_file() {
            return ToolResult::err("Path is not a file");
        }
        let content = match std::fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };
        read_file_content_result(content, start_line, limit)
    }

    async fn run_job(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let max_runtime = timeout_secs.unwrap_or(3600).clamp(1, 604800);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            match self
                .shell_clients
                .start_job(
                    ShellJobOpRequest {
                        op: "start".to_string(),
                        client_id: Some(client_id),
                        cwd: cwd.or_else(|| Some(proj.path.clone())),
                        command: Some(command),
                        timeout_secs: Some(max_runtime as u64),
                        job_id: None,
                        since_stdout_line: None,
                        since_stderr_line: None,
                        tail_lines: None,
                        limit: None,
                        codex: None,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(job) => ToolResult::ok(json!({ "job_id": job.job_id })),
                Err(e) => ToolResult::err(e),
            }
        } else {
            let root = proj.root();
            let job_id = uuid::Uuid::new_v4().to_string();
            let dir = root.join(format!(".codex/jobs/{}", job_id));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return ToolResult::err(format!("Failed to create job dir: {}", e));
            }
            let now = chrono::Utc::now().timestamp();
            let mut meta = json!({
                "job_id": job_id,
                "project": project.clone(),
                "command": command,
                "status": "running",
                "created_at": now,
                "started_at": now,
                "max_runtime_secs": max_runtime,
                "executor": "local",
                "path": proj.path.clone(),
                "kind": "shell",
            });
            if let Err(e) = std::fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap_or_default(),
            ) {
                return ToolResult::err(format!("Failed to write metadata: {}", e));
            }
            let cmd_content = format!("#!/usr/bin/env bash\n{}\n", command);
            if let Err(e) = std::fs::write(dir.join("command.sh"), &cmd_content) {
                return ToolResult::err(format!("Failed to write command.sh: {}", e));
            }
            let _ = std::fs::write(dir.join("status"), "running");
            let dir_s = dir.to_string_lossy().to_string();
            let wrapper = format!(
                "bash {0}/command.sh > {0}/stdout.log 2> {0}/stderr.log; code=$?; echo $code > {0}/exit_code; finished=$(date +%s); echo $finished > {0}/finished_at; if [ $code -eq 0 ]; then echo completed > {0}/status; else echo failed > {0}/status; fi",
                shell_escape_simple(&dir_s)
            );
            match std::process::Command::new("setsid")
                .arg("sh")
                .arg("-c")
                .arg(wrapper)
                .current_dir(&root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    // `setsid` makes the child a session + process-group
                    // leader, so child.id() is both the leader pid and the
                    // process-group id. Record the pgid so timeout/stop can
                    // signal the whole subtree (`kill -<pgid>`).
                    let pgid = child.id() as i64;
                    let _ = std::fs::write(dir.join("pid"), child.id().to_string());
                    meta["process_group_id"] = json!(pgid);
                    let _ = std::fs::write(
                        dir.join("metadata.json"),
                        serde_json::to_string_pretty(&meta).unwrap_or_default(),
                    );
                    self.local_jobs
                        .lock()
                        .await
                        .insert(job_id.clone(), LocalJobRecord { project, dir });
                    ToolResult::ok(json!({ "job_id": job_id }))
                }
                Err(e) => ToolResult::err(format!("Failed to spawn job: {}", e)),
            }
        }
    }

    async fn run_codex(
        &self,
        project: String,
        prompt: String,
        approval_mode: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
        extra_args: Option<Vec<String>>,
    ) -> ToolResult {
        if prompt.trim().is_empty() {
            return ToolResult::err("prompt cannot be empty");
        }
        if prompt.contains('\0') {
            return ToolResult::err("prompt cannot contain NUL bytes");
        }
        if prompt.len() > self.codex.max_prompt_bytes {
            return ToolResult::err(format!(
                "prompt is too large; maximum is {} bytes",
                self.codex.max_prompt_bytes
            ));
        }
        if let Some(mode) = approval_mode.as_deref() {
            if mode.contains('\0') {
                return ToolResult::err("approval_mode cannot contain NUL bytes");
            }
        }
        let project_for_output = project.clone();
        let command =
            match build_codex_command(&self.codex, &prompt, approval_mode.as_deref(), extra_args) {
                Ok(command) => command,
                Err(e) => return ToolResult::err(e),
            };
        let result = self
            .run_job(
                project,
                command,
                timeout_secs.or(Some(self.codex.default_timeout_secs)),
                cwd,
            )
            .await;
        if !result.success {
            return result;
        }
        let mut output = result.output;
        if let Some(obj) = output.as_object_mut() {
            obj.insert("kind".to_string(), Value::String("codex".to_string()));
            obj.insert("project".to_string(), Value::String(project_for_output));
            obj.insert(
                "status_endpoint".to_string(),
                Value::String("/api/jobs/status".to_string()),
            );
            obj.insert(
                "log_endpoint".to_string(),
                Value::String("/api/jobs/log".to_string()),
            );
            if let Some(job_id) = obj.get("job_id").and_then(Value::as_str) {
                if let Some(record) = self.local_jobs.lock().await.get(job_id).cloned() {
                    let mut meta = read_json(record.dir.join("metadata.json"));
                    if let Some(meta_obj) = meta.as_object_mut() {
                        meta_obj.insert("kind".to_string(), Value::String("codex".to_string()));
                    }
                    let _ = std::fs::write(
                        record.dir.join("metadata.json"),
                        serde_json::to_string_pretty(&meta).unwrap_or_default(),
                    );
                }
            }
        }
        ToolResult::ok(output)
    }

    async fn job_status(&self, job_id: String) -> ToolResult {
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            return local_job_status(&job_id, &record, killer);
        }
        // Fall through to agent-backed jobs. If the agent registry does not
        // know this job either, attempt local recovery from on-disk metadata
        // so jobs started before a server restart remain queryable.
        if self.shell_clients.get_job(&job_id).await.is_err() {
            if let Some(record) = self.recover_local_job(&job_id).await {
                return local_job_status(&job_id, &record, killer);
            }
            return ToolResult::err(format!("unknown job: {}", job_id));
        }
        match self.shell_clients.get_job(&job_id).await {
            Ok(job) => ToolResult::ok(json!({
                "job_id": job.job_id,
                "status": job.status,
                "exit_code": job.exit_code,
                "started_at": job.started_at,
                "ended_at": job.ended_at,
                "duration_ms": job.duration_ms,
                "elapsed_secs": job.elapsed_secs,
                "client_id": job.client_id,
                "command_preview": job.command_preview,
                "error": job.error,
            })),
            Err(e) => ToolResult::err(e),
        }
    }

    async fn job_log(
        &self,
        job_id: String,
        offset: Option<usize>,
        tail_lines: Option<usize>,
    ) -> ToolResult {
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            return local_job_log(&job_id, &record, killer, offset, tail_lines);
        }
        if self.shell_clients.get_job(&job_id).await.is_err() {
            if let Some(record) = self.recover_local_job(&job_id).await {
                return local_job_log(&job_id, &record, killer, offset, tail_lines);
            }
            return ToolResult::err(format!("unknown job: {}", job_id));
        }
        match self
            .shell_clients
            .job_log(&job_id, offset, None, tail_lines.or(Some(500)))
            .await
        {
            Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => {
                ToolResult::ok(json!({
                    "job_id": job.job_id,
                    "status": job.status,
                    "stdout": stdout,
                    "stderr": stderr,
                    "next_stdout_line": next_stdout_line,
                    "next_stderr_line": next_stderr_line,
                }))
            }
            Err(e) => ToolResult::err(e),
        }
    }

    // -------------------------------------------------------------------------
    // Phase A read-only console tools
    // -------------------------------------------------------------------------

    /// `list_project_files`: bounded, project-relative file listing routed to
    /// the owning registered agent via the `file_list` op. The server never
    /// reads the agent project path directly. Returns `path` + `kind`
    /// (file/dir); size/mtime are not exposed by the current file op protocol.
    async fn list_project_files(
        &self,
        project: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rel_path = path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(e) = validate_project_relative_path(&rel_path) {
            return ToolResult::err(e);
        }
        let max_entries = limit.unwrap_or(200).clamp(1, 500);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "list".to_string(),
                        client_id,
                        path: rel_path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        max_bytes: None,
                        expected_sha256: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (entries, truncated) =
                        parse_file_list_entries(&stdout, &rel_path, max_entries);
                    ToolResult::ok(json!({
                        "project": project,
                        "path": rel_path,
                        "entries": entries,
                        "truncated": truncated,
                    }))
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent list_project_files failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent list_project_files waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent list_project_files")
                }
            };
        }
        // Local-executor parity path (the runtime surface is agent-first; this
        // branch mirrors read_file/git_status for structural consistency).
        let root = proj.root();
        let dir = if rel_path == "." {
            root.to_path_buf()
        } else {
            root.join(&rel_path)
        };
        let canonical_root = match root.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical_dir = match dir.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical_dir.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        let (entries, truncated) = match std::fs::read_dir(&canonical_dir) {
            Ok(rd) => {
                let mut all = Vec::new();
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    all.push(json!({
                        "path": relative_entry_path(&rel_path, &name),
                        "kind": if is_dir { "dir" } else { "file" },
                    }));
                }
                all.sort_by(|a, b| {
                    a["path"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["path"].as_str().unwrap_or(""))
                });
                let truncated = all.len() > max_entries;
                all.truncate(max_entries);
                (all, truncated)
            }
            Err(e) => return ToolResult::err(format!("Failed to list directory: {}", e)),
        };
        ToolResult::ok(json!({
            "project": project,
            "path": rel_path,
            "entries": entries,
            "truncated": truncated,
        }))
    }

    /// `search_project_text`: bounded text search routed to the owning agent
    /// via a bounded `grep -rnI` shell call. Excludes `.git`, `target`, and
    /// `node_modules` by default. Each match carries a project-relative path,
    /// 1-based line number, and a preview line.
    async fn search_project_text(
        &self,
        project: String,
        pattern: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> ToolResult {
        if pattern.trim().is_empty() {
            return ToolResult::err("pattern cannot be empty");
        }
        if pattern.contains('\0') {
            return ToolResult::err("pattern cannot contain NUL bytes");
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rel_path = path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(e) = validate_project_relative_path(&rel_path) {
            return ToolResult::err(e);
        }
        let max_matches = limit.unwrap_or(50).clamp(1, 200);
        let cmd = search_project_text_command(&pattern, &rel_path, max_matches);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: cmd,
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 32,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (matches, truncated) = parse_search_matches(&stdout, max_matches);
                    ToolResult::ok(json!({
                        "project": project,
                        "pattern": pattern,
                        "path": rel_path,
                        "matches": matches,
                        "count": matches.len(),
                        "truncated": truncated,
                        "exit_code": resp.exit_code,
                    }))
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            };
        }
        let root = proj.root();
        let result = tokio::task::spawn_blocking(move || run_command_sync(&cmd, &root, 30)).await;
        match result {
            Ok((exit_code, stdout, _stderr, _)) => {
                let (matches, truncated) = parse_search_matches(&stdout, max_matches);
                ToolResult::ok(json!({
                    "project": project,
                    "pattern": pattern,
                    "path": rel_path,
                    "matches": matches,
                    "count": matches.len(),
                    "truncated": truncated,
                    "exit_code": exit_code,
                }))
            }
            Err(e) => ToolResult::err(format!("task join error: {}", e)),
        }
    }

    /// `git_diff_summary`: read-only inspection returning `git status
    /// --porcelain`, `git diff --stat`, and a parsed changed-file list. Does
    /// not modify the worktree.
    async fn git_diff_summary(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let cmd = git_diff_summary_command();
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: cmd,
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 32,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (porcelain, diff_stat) = split_diff_summary(&stdout);
                    let porcelain_summary = parse_porcelain_summary(&porcelain);
                    ToolResult::ok(json!({
                        "porcelain": porcelain,
                        "diff_stat": diff_stat,
                        "changed_files": porcelain_summary.changed_files,
                        "changed_files_count": porcelain_summary.changed_files_count,
                        "tracked_changed_files": porcelain_summary.tracked_changed_files,
                        "untracked_files": porcelain_summary.untracked_files,
                        "ignored_files": porcelain_summary.ignored_files,
                        "exit_code": resp.exit_code,
                    }))
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            };
        }
        let root = proj.root();
        let result = tokio::task::spawn_blocking(move || run_command_sync(&cmd, &root, 30)).await;
        match result {
            Ok((exit_code, stdout, _stderr, _)) => {
                let (porcelain, diff_stat) = split_diff_summary(&stdout);
                let porcelain_summary = parse_porcelain_summary(&porcelain);
                ToolResult::ok(json!({
                    "porcelain": porcelain,
                    "diff_stat": diff_stat,
                    "changed_files": porcelain_summary.changed_files,
                    "changed_files_count": porcelain_summary.changed_files_count,
                    "tracked_changed_files": porcelain_summary.tracked_changed_files,
                    "untracked_files": porcelain_summary.untracked_files,
                    "ignored_files": porcelain_summary.ignored_files,
                    "exit_code": exit_code,
                }))
            }
            Err(e) => ToolResult::err(format!("task join error: {}", e)),
        }
    }

    /// `list_jobs`: bounded job summaries across agent and local executors.
    /// Never returns stdout/stderr bodies — only metadata.
    async fn list_jobs(&self, limit: Option<usize>, status: Option<String>) -> ToolResult {
        let max = limit.unwrap_or(20).clamp(1, 100);
        let status_filter = status
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // Agent jobs come pre-bounded to `max` by the registry. Local jobs are
        // collected fully (the in-memory map is small) so truncation can be
        // detected accurately for the common local-only case.
        let agent_jobs = self.shell_clients.list_jobs(Some(max)).await;
        let mut summaries: Vec<Value> = agent_jobs
            .iter()
            .filter(|j| {
                status_filter
                    .as_ref()
                    .map(|s| s == &j.status)
                    .unwrap_or(true)
            })
            .map(agent_job_summary_value)
            .collect();
        let local_jobs_map = self.local_jobs.lock().await;
        for (job_id, record) in local_jobs_map.iter() {
            if let Some(summary) = local_job_summary_value(job_id, record, &status_filter) {
                summaries.push(summary);
            }
        }
        summaries.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["created_at"].as_i64().unwrap_or(0))
        });
        let truncated = summaries.len() > max;
        summaries.truncate(max);
        ToolResult::ok(json!({
            "jobs": summaries,
            "count": summaries.len(),
            "truncated": truncated,
        }))
    }

    /// `job_tail`: bounded stdout/stderr tails for a job. Reuses the bounded
    /// `job_log` path with a tail-focused default so the console never reads
    /// full logs by default.
    async fn job_tail(&self, job_id: String, tail_lines: Option<usize>) -> ToolResult {
        let tail = tail_lines.unwrap_or(200).clamp(1, 500);
        self.job_log(job_id, None, Some(tail)).await
    }

    /// Stop a local job by terminating its process group and marking it
    /// `stopped`.
    ///
    /// This is an internal lifecycle method intended as the implementation
    /// backing a future explicit stop API; it is deliberately **not** exposed
    /// as a GPT Actions / MCP write tool, to avoid surfacing an arbitrary kill
    /// surface to remote callers. Only jobs we created and recorded (in-memory
    /// or recoverable on disk) can be stopped, and the pid/pgid come
    /// exclusively from the job's own on-disk files — never from caller input.
    pub async fn stop_job(&self, job_id: String) -> ToolResult {
        if !is_safe_job_id(&job_id) {
            return ToolResult::err("invalid job id");
        }
        let cached = {
            let jobs = self.local_jobs.lock().await;
            jobs.get(&job_id).cloned()
        };
        let record = match cached {
            Some(r) => r,
            None => match self.recover_local_job(&job_id).await {
                Some(r) => r,
                None => return ToolResult::err(format!("unknown job: {}", job_id)),
            },
        };
        stop_local_job(&job_id, &record, self.job_killer.as_ref())
    }

    /// Recover a local job from on-disk `.codex/jobs/<job_id>/metadata.json`
    /// under any configured project root. Rejects job ids that could escape
    /// the project directory and verifies the metadata matches the configured
    /// project before caching the record in memory.
    async fn recover_local_job(&self, job_id: &str) -> Option<LocalJobRecord> {
        if !is_safe_job_id(job_id) {
            return None;
        }
        let projects = self.projects.config.as_ref()?;
        for (id, proj) in &projects.projects {
            let root = proj.root();
            let job_dir = root.join(format!(".codex/jobs/{}", job_id));
            let meta_path = job_dir.join("metadata.json");
            if !meta_path.exists() {
                continue;
            }
            // Path safety: canonicalize both and verify the job dir is under
            // the configured project root.
            let canonical_root = match root.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let canonical_job_dir = match job_dir.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !canonical_job_dir.starts_with(&canonical_root) {
                continue;
            }
            // Verify metadata belongs to this configured project. This stops a
            // recovered job from one project being mistaken for another.
            let meta = read_json(meta_path);
            let meta_project = meta.get("project").and_then(Value::as_str).unwrap_or("");
            let meta_path_str = meta.get("path").and_then(Value::as_str).unwrap_or("");
            if meta_project != id || meta_path_str != proj.path {
                continue;
            }
            let record = LocalJobRecord {
                project: id.clone(),
                dir: job_dir.clone(),
            };
            self.local_jobs
                .lock()
                .await
                .insert(job_id.to_string(), record.clone());
            return Some(record);
        }
        None
    }
}

// =============================================================================
// Standalone helpers
// =============================================================================

fn run_command_sync(cmd: &str, cwd: &Path, timeout_secs: u64) -> (i32, String, String, u64) {
    let start = Instant::now();
    let spawn = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawn {
        Ok(c) => c,
        Err(e) => {
            return (
                -1,
                String::new(),
                format!("Failed to execute command: {}", e),
                start.elapsed().as_millis() as u64,
            );
        }
    };
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let elapsed = start.elapsed().as_millis() as u64;
                    return match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                            let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();
                            if !stderr.is_empty() && !stderr.ends_with('\n') {
                                stderr.push('\n');
                            }
                            stderr.push_str(&format!(
                                "Command timed out after {} seconds",
                                timeout_secs
                            ));
                            (-1, stdout, stderr, elapsed)
                        }
                        Err(e) => (
                            -1,
                            String::new(),
                            format!(
                                "Command timed out after {} seconds; failed to collect output: {}",
                                timeout_secs, e
                            ),
                            elapsed,
                        ),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for command: {}", e),
                    start.elapsed().as_millis() as u64,
                );
            }
        }
    }
    match child.wait_with_output() {
        Ok(out) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let code = out.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => (
            -1,
            String::new(),
            format!("Failed to collect command output: {}", e),
            start.elapsed().as_millis() as u64,
        ),
    }
}

fn object_schema(fields: Vec<(&str, &str, &str, bool)>) -> Value {
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

fn resolve_local_cwd(
    proj: &crate::projects::ProjectConfig,
    cwd: Option<&str>,
) -> Result<PathBuf, String> {
    let root = proj.root();
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Project root does not exist: {}", e))?;
    let requested = match cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) {
        Some(cwd) => {
            let path = PathBuf::from(cwd);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => root,
    };
    let canonical = requested
        .canonicalize()
        .map_err(|e| format!("cwd does not exist: {}", e))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("cwd is outside project directory".to_string());
    }
    Ok(canonical)
}

fn build_codex_command(
    codex: &CodexConfig,
    prompt: &str,
    approval_mode: Option<&str>,
    extra_args: Option<Vec<String>>,
) -> Result<String, String> {
    validate_cli_arg(&codex.bin, "CODEX_BIN")?;
    // Resolve the effective approval mode. An explicit request value wins over
    // the config default. Empty/blank, none, off, and disabled all mean "do not
    // pass --approval-mode" so the runtime stays compatible with Codex CLI
    // builds that do not support the flag.
    let resolved_approval_mode = match approval_mode {
        Some(v) => v.trim().to_string(),
        None => codex.approval_mode.clone(),
    };
    if resolved_approval_mode.contains('\0') {
        return Err("approval_mode cannot contain NUL bytes".to_string());
    }
    let approval_disabled = is_approval_mode_disabled(&resolved_approval_mode);
    let extra_args = extra_args.unwrap_or_default();
    if extra_args.len() > 32 {
        return Err("extra_args may contain at most 32 arguments".to_string());
    }
    for (idx, arg) in extra_args.iter().enumerate() {
        validate_cli_arg(arg, &format!("extra_args[{}]", idx))?;
        if !codex.is_extra_arg_allowed(arg) {
            return Err(format!(
                "extra_args[{}] '{}' is not in CODEX_ALLOWED_EXTRA_ARGS allowlist",
                idx, arg
            ));
        }
    }
    let mut parts = vec![shell_escape_simple(&codex.bin)];
    if !approval_disabled {
        parts.push("--approval-mode".to_string());
        parts.push(shell_escape_simple(&resolved_approval_mode));
    }
    for arg in &extra_args {
        parts.push(shell_escape_simple(arg));
    }
    parts.push(shell_escape_simple(prompt));
    Ok(parts.join(" "))
}

/// Returns true when an approval-mode value means "do not pass --approval-mode".
/// Empty/whitespace, `none`, `off`, and `disabled` (case-insensitive) disable
/// the flag so the runtime works with Codex CLI builds that lack it.
fn is_approval_mode_disabled(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    v.is_empty() || v == "none" || v == "off" || v == "disabled"
}

fn validate_cli_arg(value: &str, field: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("{} cannot contain NUL bytes", field));
    }
    if value.trim().is_empty() {
        return Err(format!("{} cannot be empty", field));
    }
    Ok(())
}

fn read_file_content_result(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    if eff_start > total_lines {
        return ToolResult::ok(json!({
            "content": "",
            "total_lines": total_lines,
            "start_line": eff_start,
            "limit": eff_limit,
        }));
    }
    let start_idx = eff_start - 1;
    let end_idx = (start_idx + eff_limit).min(total_lines);
    let slice = all_lines[start_idx..end_idx].join("\n");
    ToolResult::ok(json!({
        "content": slice,
        "total_lines": total_lines,
        "start_line": eff_start,
        "limit": eff_limit,
    }))
}

// =============================================================================
// Phase A read-only console helpers
// =============================================================================

/// Build the project-relative path for a single entry returned by an agent
/// `file_list` op. `rel_path` is the project-relative directory the caller
/// requested (`"."` for the project root); `name` is the bare entry name.
fn relative_entry_path(rel_path: &str, name: &str) -> String {
    let trimmed = rel_path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        name.to_string()
    } else {
        format!("{}/{}", trimmed, name)
    }
}

/// Validate a user-supplied project-relative path for console read-only tools.
/// The owning agent still enforces its local root policy, but these tools
/// promise project-relative behavior and should not intentionally target
/// absolute paths or parent traversal.
fn validate_project_relative_path(path: &str) -> Result<(), String> {
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Ok(());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    Ok(())
}

/// Parse agent `file_list` stdout (one entry per line, dirs suffixed with
/// `/`) into bounded project-relative entries with a file/dir kind. Returns
/// the entries and whether the source exceeded `max_entries`.
fn parse_file_list_entries(stdout: &str, rel_path: &str, max_entries: usize) -> (Vec<Value>, bool) {
    let mut all: Vec<Value> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let (name, is_dir) = if let Some(stripped) = line.strip_suffix('/') {
            (stripped.to_string(), true)
        } else {
            (line.to_string(), false)
        };
        if name.is_empty() {
            continue;
        }
        all.push(json!({
            "path": relative_entry_path(rel_path, &name),
            "kind": if is_dir { "dir" } else { "file" },
        }));
    }
    all.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or("")
            .cmp(b["path"].as_str().unwrap_or(""))
    });
    let truncated = all.len() > max_entries;
    all.truncate(max_entries);
    (all, truncated)
}

/// Build a bounded `grep -rnI` command for `search_project_text`. Excludes
/// sensitive/build directories (`.git`, `target`, `node_modules`) by default
/// and caps output with `head -n (max_matches + 1)` so the runtime can detect
/// truncation without requesting an unbounded stream.
fn search_project_text_command(pattern: &str, rel_path: &str, max_matches: usize) -> String {
    let escaped_pattern = shell_escape_simple(pattern);
    let escaped_target = shell_escape_simple(rel_path);
    // head -n N+1: one extra line lets the parser flag truncation.
    let head = max_matches.saturating_add(1);
    format!(
        "grep -rnI --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules -e {pattern} {target} 2>/dev/null | head -n {head}",
        pattern = escaped_pattern,
        target = escaped_target,
        head = head,
    )
}

/// Parse `grep -rnI` output lines (`path:lineno:content`) into bounded match
/// objects. Strips a leading `./` so paths are project-relative. Returns the
/// matches and whether the source exceeded `max_matches`.
fn parse_search_matches(stdout: &str, max_matches: usize) -> (Vec<Value>, bool) {
    let mut matches: Vec<Value> = Vec::new();
    let mut truncated = false;
    for line in stdout.lines() {
        if matches.len() >= max_matches {
            truncated = true;
            break;
        }
        let mut parts = line.splitn(3, ':');
        let (Some(path), Some(lineno), Some(content)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let line_no: usize = match lineno.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let clean_path = path.strip_prefix("./").unwrap_or(path).to_string();
        matches.push(json!({
            "path": clean_path,
            "line": line_no,
            "preview": content,
        }));
    }
    (matches, truncated)
}

/// Sentinel separating `git status --porcelain` from `git diff --stat` in the
/// combined `git_diff_summary` command output. Chosen to be extremely unlikely
/// to appear in real git output.
const DIFF_SUMMARY_SENTINEL: &str = "@@PRIVATE_DROP_DIFF_SUMMARY_SEP@@";

/// Build the read-only `git_diff_summary` command. Runs `git status
/// --porcelain` and `git diff --stat` separated by a unique sentinel. No
/// mutating git subcommand is emitted.
fn git_diff_summary_command() -> String {
    format!(
        "git status --porcelain; printf '\\n{sentinel}\\n'; git diff --stat",
        sentinel = DIFF_SUMMARY_SENTINEL,
    )
}

/// Split the combined `git_diff_summary` stdout into the porcelain section and
/// the `diff --stat` section. If the sentinel is absent, everything is treated
/// as porcelain (defensive; should not happen in practice).
fn split_diff_summary(stdout: &str) -> (String, String) {
    if let Some((before, after)) = stdout.split_once(DIFF_SUMMARY_SENTINEL) {
        (
            before.trim_end_matches(['\n', '\r']).to_string(),
            after
                .trim_start_matches(['\n', '\r'])
                .trim_end()
                .to_string(),
        )
    } else {
        (stdout.trim_end().to_string(), String::new())
    }
}

#[derive(Debug, Clone, Default)]
struct PorcelainSummary {
    changed_files: Vec<String>,
    tracked_changed_files: Vec<String>,
    untracked_files: Vec<String>,
    ignored_files: Vec<String>,
    changed_files_count: usize,
}

/// Parse `git status --porcelain` output into tracked/untracked buckets.
/// Handles renames (`R  old -> new` -> `new`) and quoted paths.
fn parse_porcelain_summary(porcelain: &str) -> PorcelainSummary {
    let mut summary = PorcelainSummary::default();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = &line[..2];
        let path_part = &line[3..];
        let path = if let Some((_, dst)) = path_part.split_once(" -> ") {
            dst
        } else {
            path_part
        };
        let path = path.trim().trim_matches('"');
        if path.is_empty() {
            continue;
        }
        match status {
            "??" => summary.untracked_files.push(path.to_string()),
            "!!" => summary.ignored_files.push(path.to_string()),
            _ => summary.tracked_changed_files.push(path.to_string()),
        }
        summary.changed_files.push(path.to_string());
    }
    summary.changed_files_count = summary.changed_files.len();
    summary
}

/// Backward-compatible helper for older tests/callers that only need all paths.
#[allow(dead_code)]
fn parse_porcelain_files(porcelain: &str) -> Vec<String> {
    parse_porcelain_summary(porcelain).changed_files
}

fn validate_limited_cleanup_paths(
    paths: &[String],
    deny_sensitive: bool,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("paths cannot be empty".to_string());
    }
    if paths.len() > 64 {
        return Err("paths may contain at most 64 entries".to_string());
    }
    let mut clean = Vec::new();
    for raw in paths {
        validate_project_relative_path(raw)?;
        let path = raw.trim().trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() || path == "." {
            return Err("path must name a file or tracked path, not the project root".to_string());
        }
        if deny_sensitive {
            let warnings = sensitive_path_warnings(path);
            if !warnings.is_empty() {
                return Err(format!(
                    "refusing sensitive cleanup path '{}': {}",
                    path,
                    warnings.join("; ")
                ));
            }
        }
        if !clean.iter().any(|p: &String| p == path) {
            clean.push(path.to_string());
        }
    }
    Ok(clean)
}

fn shell_join_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| shell_escape_simple(p))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build a bounded job summary `Value` for an agent-known job. Never includes
/// stdout/stderr bodies.
fn agent_job_summary_value(job: &ShellJobInfo) -> Value {
    json!({
        "job_id": job.job_id,
        "kind": job.kind,
        "status": job.status,
        "project": job.project_id,
        "executor": "agent",
        "client_id": job.client_id,
        "created_at": job.created_at,
        "started_at": job.started_at,
        "ended_at": job.ended_at,
        "duration_ms": job.duration_ms,
        "elapsed_secs": job.elapsed_secs,
        "exit_code": job.exit_code,
    })
}

/// Build a bounded job summary `Value` for a local on-disk job by reading
/// lightweight metadata/status files. Returns `None` when a status filter is
/// set and the job does not match. Never includes stdout/stderr bodies.
fn local_job_summary_value(
    job_id: &str,
    record: &LocalJobRecord,
    status_filter: &Option<String>,
) -> Option<Value> {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if let Some(filter) = status_filter {
        if &status != filter {
            return None;
        }
    }
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let ended_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
    let kind = meta
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("shell")
        .to_string();
    Some(json!({
        "job_id": job_id,
        "kind": kind,
        "status": status,
        "project": record.project,
        "executor": "local",
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": ended_at,
        "exit_code": exit_code,
    }))
}

fn local_job_status(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> ToolResult {
    // Reclaim overtime jobs before reading status: this persists a terminal
    // `lost` status (and terminates the process group) so callers see a
    // consistent terminal state and we don't leak processes.
    let timeout_note = enforce_local_job_timeout(record, killer);
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let finished_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64);
    let elapsed_secs = started_at.map(|started| {
        finished_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .saturating_sub(started) as u64
    });
    let mut output = json!({
        "job_id": job_id,
        "project": record.project,
        "status": status,
        "exit_code": exit_code,
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": finished_at,
        "elapsed_secs": elapsed_secs,
        "max_runtime_secs": max_runtime_secs,
        "executor": "local",
        "kind": meta.get("kind").cloned().unwrap_or_else(|| Value::String("shell".to_string())),
    });
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    ToolResult::ok(output)
}

fn local_job_log(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> ToolResult {
    // A log query on an overtime job also reclaims it so the reported status
    // is terminal and the process group is not leaked.
    let timeout_note = enforce_local_job_timeout(record, killer);
    let stdout = read_lines_from(record.dir.join("stdout.log"), offset, tail_lines);
    let stderr = read_lines_from(record.dir.join("stderr.log"), offset, tail_lines);
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    let mut output = json!({
        "job_id": job_id,
        "status": status,
        "stdout": stdout.0,
        "stderr": stderr.0,
        "next_stdout_line": stdout.1,
        "next_stderr_line": stderr.1,
    });
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    ToolResult::ok(output)
}

/// Resolve the process-group id to signal for a local job. Prefers an explicit
/// `process_group_id` in metadata (written by current spawn code); falls back
/// to the `pid` file, which under `setsid` is equal to the pgid. Returns
/// `None` when neither is recorded (e.g. very old metadata predating pid
/// tracking) — in that case we never guess at a pid to kill.
fn resolve_job_pgid(meta: &Value, record: &LocalJobRecord) -> Option<i64> {
    meta.get("process_group_id")
        .and_then(Value::as_i64)
        .or_else(|| read_trim(record.dir.join("pid")).and_then(|s| s.parse::<i64>().ok()))
}

/// If a local job is still `running` but has exceeded `max_runtime_secs`,
/// terminate its process group and persist a terminal `lost` status. Returns a
/// short human-readable note when a timeout was enforced, or `None` if the job
/// is not running or not over time.
///
/// Safety: the pid/pgid come only from this job's own on-disk files (written by
/// us at spawn time via `setsid`). We never kill based on caller-supplied pids.
/// If no pid/pgid is recorded, we only mark the job `lost` — never guess. Kill
/// failures never panic; a conservative `lost` status is persisted regardless.
fn enforce_local_job_timeout(
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> Option<String> {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    if normalize_local_status(&raw_status) != "running" {
        return None;
    }
    let started_at = meta.get("started_at").and_then(Value::as_i64)?;
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64)?;
    // The wrapper writes `finished_at` before `status`. If it exists, the job
    // just finished (or was already reclaimed) — do not double-reclaim.
    if read_trim(record.dir.join("finished_at")).is_some() {
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    if now.saturating_sub(started_at) <= max_runtime_secs {
        return None;
    }
    // Over time. Reclaim the process group if we recorded one.
    let pgid = resolve_job_pgid(&meta, record);
    let note = match pgid {
        Some(pgid) => {
            let pid = read_trim(record.dir.join("pid"))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!(
                        "timed out after {}s; process group {} terminated ({})",
                        max_runtime_secs, pgid, sig
                    )
                }
                TerminateOutcome::AlreadyGone => format!(
                    "timed out after {}s; process group {} already exited; marked lost",
                    max_runtime_secs, pgid
                ),
            }
        }
        None => format!(
            "timed out after {}s; no pid/process_group_id on record; marked lost",
            max_runtime_secs
        ),
    };
    // Persist terminal state so subsequent reads are consistent and we don't
    // repeatedly attempt to kill. The wrapper shell was part of the group and
    // is now gone, so it will not write its own status/finished_at.
    let _ = std::fs::write(record.dir.join("status"), "lost");
    let _ = std::fs::write(record.dir.join("finished_at"), now.to_string());
    Some(note)
}

/// Stop a local job by terminating its process group and persisting a
/// `stopped` status. Only acts on active jobs; terminal jobs are left alone.
/// Like `enforce_local_job_timeout`, the pid/pgid come only from the job's own
/// on-disk files, and missing pid/pgid yields a conservative `stopped` marker
/// without guessing. Kill failures never panic.
fn stop_local_job(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> ToolResult {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if !ACTIVE_LOCAL_STATUSES.contains(&status.as_str()) {
        return ToolResult::ok(json!({
            "job_id": job_id,
            "project": record.project,
            "status": status,
            "note": "job already terminal; not stopped again",
        }));
    }
    let now = chrono::Utc::now().timestamp();
    let note = match resolve_job_pgid(&meta, record) {
        Some(pgid) => {
            let pid = read_trim(record.dir.join("pid"))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!("stopped; process group {} terminated ({})", pgid, sig)
                }
                TerminateOutcome::AlreadyGone => {
                    format!("stopped; process group {} already exited", pgid)
                }
            }
        }
        None => "stopped; no pid/process_group_id on record; marked stopped".to_string(),
    };
    let _ = std::fs::write(record.dir.join("status"), "stopped");
    let _ = std::fs::write(record.dir.join("finished_at"), now.to_string());
    ToolResult::ok(json!({
        "job_id": job_id,
        "project": record.project,
        "status": "stopped",
        "note": note,
    }))
}

/// Validate a job id before using it to construct a filesystem path. Rejects
/// path separators, traversal sequences, and overly long ids.
fn is_safe_job_id(job_id: &str) -> bool {
    if job_id.is_empty() || job_id.len() > 80 || job_id.contains("..") {
        return false;
    }
    job_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Normalize an on-disk local job status into the canonical status set:
/// `queued`, `running`, `completed`, `failed`, `stopped`, `lost`. An empty
/// status (job just started, file not written yet) defaults to `running`;
/// any unrecognized value is treated as `lost`.
fn normalize_local_status(raw: &str) -> String {
    match raw.trim() {
        "queued" | "running" | "completed" | "failed" | "stopped" => raw.trim().to_string(),
        "" => "running".to_string(),
        _ => "lost".to_string(),
    }
}

fn read_json(path: PathBuf) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn read_trim(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

const MAX_LOCAL_LOG_LINES: usize = 500;

fn read_lines_from(
    path: PathBuf,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> (String, usize) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    // `offset` is a 1-based line cursor (matching agent `since_stdout_line`).
    // When provided, read forward from that line, bounded to MAX_LOCAL_LOG_LINES.
    // Otherwise return the last `tail_lines` (bounded), defaulting to the last
    // MAX_LOCAL_LOG_LINES lines. Output is always bounded.
    let (start_idx, limit) = if let Some(off) = offset {
        let s = off.saturating_sub(1).min(total);
        (s, MAX_LOCAL_LOG_LINES)
    } else {
        let tail = tail_lines
            .filter(|t| *t > 0)
            .map(|t| t.min(MAX_LOCAL_LOG_LINES))
            .unwrap_or(MAX_LOCAL_LOG_LINES);
        (total.saturating_sub(tail), tail)
    };
    let end_idx = (start_idx + limit).min(total);
    let selected = lines[start_idx..end_idx].join("\n");
    // 1-based line number to request for the next chunk.
    let next_line = end_idx + 1;
    (selected, next_line)
}

fn shell_escape_simple(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn parse_changed_files_from_patch(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            if let Some(b_pos) = line.rfind(" b/") {
                let file = &line[b_pos + 3..];
                if !files.iter().any(|f: &String| f == file) {
                    files.push(file.to_string());
                }
            }
            continue;
        }
        for prefix in ["+++ b/", "--- a/"] {
            if let Some(file) = line.strip_prefix(prefix) {
                if file != "/dev/null" && !files.iter().any(|f: &String| f == file) {
                    files.push(file.to_string());
                }
            }
        }
    }
    files
}

fn validate_patch_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("patch path cannot be empty".to_string());
    }
    if path.starts_with('/') {
        return Err(format!("Absolute paths are not allowed: {}", path));
    }
    if path.contains("..") {
        return Err(format!("Path traversal (..) is not allowed: {}", path));
    }
    let sensitive = [".env", ".env.local", "secret.pem", "id_rsa", ".git/config"];
    if sensitive.iter().any(|s| path.contains(s)) {
        return Err(format!("Cannot modify sensitive path: {}", path));
    }
    Ok(())
}

/// Maximum accepted patch size for `validate_patch`, in bytes. Kept
/// conservative to bound memory use and the agent stdin payload size. The
/// patch is sent to the agent as stdin for `git apply`; larger patches should
/// be split.
/// This is a preflight-only bound; it does not affect `apply_patch`.
const MAX_VALIDATE_PATCH_BYTES: usize = 256 * 1024; // 256 KiB

/// Hard-reject patch file paths that would escape the project boundary during
/// `validate_patch` preflight. Unlike `validate_patch_file_path` (used by the
/// real `apply_patch`), this does **not** reject sensitive filenames — those
/// are reported as `warnings` instead so the caller can still see the dry-run
/// result. Only absolute paths, `..` traversal, and NUL bytes are hard
/// rejects, ensuring the preflight never escapes the project root.
fn validate_preflight_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("patch path cannot be empty".to_string());
    }
    if path.starts_with('/') {
        return Err(format!("Absolute paths are not allowed: {}", path));
    }
    if path.contains("..") {
        return Err(format!("Path traversal (..) is not allowed: {}", path));
    }
    if path.contains('\0') {
        return Err("NUL byte in patch path is not allowed".to_string());
    }
    Ok(())
}

/// Sensitive path components that `validate_patch` should warn about (but not
/// hard-reject). The preflight still runs; the caller sees the warning and can
/// decide whether to proceed with `apply_patch`. Matching is case-insensitive
/// substring so it catches `foo/.env`, `agent.toml.bak`, `target/debug`, etc.
fn sensitive_path_warnings(path: &str) -> Vec<String> {
    let lower = path.to_lowercase();
    let sensitive = [
        "agent.toml",
        "private-drop.env",
        ".env",
        "projects.d",
        ".git",
        "target",
        "node_modules",
    ];
    let mut warnings = Vec::new();
    for name in sensitive {
        if lower.contains(name) {
            warnings.push(format!(
                "patch touches sensitive path component '{}': {}",
                name, path
            ));
        }
    }
    warnings
}

fn apply_patch_local(root: &Path, patch: &str) -> Result<(bool, String, String), String> {
    if !root.exists() {
        return Err("Project root does not exist".to_string());
    }
    let patch_file = root.join(format!(".codex-patch-{}.diff", uuid::Uuid::new_v4()));
    std::fs::write(&patch_file, patch)
        .map_err(|e| format!("Failed to write temp patch file: {}", e))?;
    let check = run_command_sync(
        &format!(
            "git apply --check {}",
            shell_escape_simple(&patch_file.display().to_string())
        ),
        root,
        60,
    );
    if check.0 != 0 {
        let _ = std::fs::remove_file(&patch_file);
        return Ok((false, check.1, check.2));
    }
    let apply = run_command_sync(
        &format!(
            "git apply {}",
            shell_escape_simple(&patch_file.display().to_string())
        ),
        root,
        60,
    );
    let _ = std::fs::remove_file(&patch_file);
    Ok((apply.0 == 0, apply.1, apply.2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
    use crate::shell_client::ShellClientRegistry;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        let shell_clients = Arc::new(ShellClientRegistry::default());
        ToolRuntime::new(
            projects,
            shell_clients,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    // =========================================================================
    // Phase 1.1: ToolCall::from_tool_name
    // =========================================================================

    #[test]
    fn from_tool_name_parses_unit_tools_without_arguments() {
        for name in [
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
        ] {
            let call =
                ToolCall::from_tool_name(name, Value::Null).unwrap_or_else(|e| panic!("{}", e));
            assert!(
                matches!(
                    call,
                    ToolCall::ListTools
                        | ToolCall::ListProjects
                        | ToolCall::ListAgents
                        | ToolCall::RuntimeStatus
                ),
                "unit tool {} should parse",
                name
            );
        }
    }

    #[test]
    fn from_tool_name_parses_unit_tools_with_empty_object() {
        let call = ToolCall::from_tool_name("list_tools", json!({})).unwrap();
        assert!(matches!(call, ToolCall::ListTools));
    }

    #[test]
    fn from_tool_name_parses_run_shell_with_required_fields() {
        let call = ToolCall::from_tool_name(
            "run_shell",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .unwrap();
        match call {
            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
            } => {
                assert_eq!(project, "demo");
                assert_eq!(command, "echo hi");
                assert_eq!(timeout_secs, None);
                assert_eq!(cwd, None);
            }
            other => panic!("expected RunShell, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_run_shell_with_optional_fields() {
        let call = ToolCall::from_tool_name(
            "run_shell",
            json!({"project": "demo", "command": "ls", "timeout_secs": 5, "cwd": "sub"}),
        )
        .unwrap();
        match call {
            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
            } => {
                assert_eq!(project, "demo");
                assert_eq!(command, "ls");
                assert_eq!(timeout_secs, Some(5));
                assert_eq!(cwd, Some("sub".to_string()));
            }
            other => panic!("expected RunShell, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_run_codex_with_all_fields() {
        let call = ToolCall::from_tool_name(
            "run_codex",
            json!({
                "project": "demo",
                "prompt": "fix tests",
                "approval_mode": "suggest",
                "timeout_secs": 120,
                "cwd": "src",
                "extra_args": ["--verbose"]
            }),
        )
        .unwrap();
        match call {
            ToolCall::RunCodex {
                project,
                prompt,
                approval_mode,
                timeout_secs,
                cwd,
                extra_args,
            } => {
                assert_eq!(project, "demo");
                assert_eq!(prompt, "fix tests");
                assert_eq!(approval_mode.as_deref(), Some("suggest"));
                assert_eq!(timeout_secs, Some(120));
                assert_eq!(cwd.as_deref(), Some("src"));
                assert_eq!(extra_args.unwrap(), vec!["--verbose".to_string()]);
            }
            other => panic!("expected RunCodex, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_job_status_and_job_log() {
        let call = ToolCall::from_tool_name("job_status", json!({"job_id": "abc"})).unwrap();
        assert!(matches!(call, ToolCall::JobStatus { ref job_id } if job_id == "abc"));

        let call =
            ToolCall::from_tool_name("job_log", json!({"job_id": "abc", "offset": 10})).unwrap();
        match call {
            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => {
                assert_eq!(job_id, "abc");
                assert_eq!(offset, Some(10));
                assert_eq!(tail_lines, None);
            }
            other => panic!("expected JobLog, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_read_file_and_git_tools() {
        let call =
            ToolCall::from_tool_name("read_file", json!({"project": "demo", "path": "README.md"}))
                .unwrap();
        assert!(matches!(call, ToolCall::ReadFile { .. }));

        let call = ToolCall::from_tool_name("git_status", json!({"project": "demo"})).unwrap();
        assert!(matches!(call, ToolCall::GitStatus { .. }));

        let call =
            ToolCall::from_tool_name("git_diff", json!({"project": "demo", "args": ["--stat"]}))
                .unwrap();
        assert!(matches!(call, ToolCall::GitDiff { .. }));

        let call =
            ToolCall::from_tool_name("apply_patch", json!({"project": "demo", "patch": "diff"}))
                .unwrap();
        assert!(matches!(call, ToolCall::ApplyPatch { .. }));

        let call =
            ToolCall::from_tool_name("run_job", json!({"project": "demo", "command": "make"}))
                .unwrap();
        assert!(matches!(call, ToolCall::RunJob { .. }));
    }

    #[test]
    fn from_tool_name_rejects_unknown_tool_name() {
        let err = ToolCall::from_tool_name("not_a_tool", Value::Null).unwrap_err();
        assert!(err.contains("not_a_tool"));
    }

    #[test]
    fn from_tool_name_rejects_missing_required_field() {
        let err = ToolCall::from_tool_name("run_shell", json!({"command": "echo"})).unwrap_err();
        assert!(
            err.contains("project"),
            "error should mention missing field: {}",
            err
        );

        let err = ToolCall::from_tool_name("job_status", json!({})).unwrap_err();
        assert!(err.contains("job_id"));
    }

    #[test]
    fn from_tool_name_rejects_wrong_field_type() {
        let err = ToolCall::from_tool_name("run_shell", json!({"project": 123, "command": "echo"}))
            .unwrap_err();
        assert!(!err.is_empty());

        let err = ToolCall::from_tool_name("run_codex", json!({"project": "demo", "prompt": 42}))
            .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn from_tool_name_rejects_unknown_variant_field() {
        // extra_args must be an array, not a string.
        let err = ToolCall::from_tool_name(
            "run_codex",
            json!({"project": "demo", "prompt": "x", "extra_args": "--verbose"}),
        )
        .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn from_tool_name_error_includes_tool_name() {
        let err = ToolCall::from_tool_name("run_shell", json!({})).unwrap_err();
        assert!(err.contains("run_shell"));
    }

    // =========================================================================
    // Phase 1.2: tool_specs() shape
    // =========================================================================

    #[test]
    fn tool_specs_names_are_unique() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let mut names = specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        names.sort();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "tool names must be unique");
    }

    #[test]
    fn tool_specs_names_are_snake_case() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            assert!(
                !spec.name.contains('-'),
                "tool name '{}' should use snake_case (no hyphens)",
                spec.name
            );
            assert!(
                spec.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "tool name '{}' should be snake_case",
                spec.name
            );
        }
    }

    #[test]
    fn tool_specs_input_schemas_are_objects() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            let schema = &spec.input_schema;
            assert_eq!(
                schema["type"].as_str(),
                Some("object"),
                "tool '{}' input schema must be type object",
                spec.name
            );
            assert!(
                schema["properties"].is_object(),
                "tool '{}' input schema must have properties object",
                spec.name
            );
            assert!(
                schema["required"].is_array(),
                "tool '{}' input schema must have required array",
                spec.name
            );
        }
    }

    #[test]
    fn tool_specs_required_fields_match_deserialization() {
        // For every tool spec, building arguments with only the required
        // fields must deserialize successfully, and omitting any required
        // field must fail.
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            let required: Vec<String> = spec.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();

            // Build a minimal valid args object using a placeholder for each
            // required field based on its declared type.
            let mut minimal = serde_json::Map::new();
            let properties = spec.input_schema["properties"].as_object().unwrap();
            for field in &required {
                let prop = &properties[field.as_str()];
                let kind = prop["type"].as_str().unwrap_or("string");
                let placeholder = match kind {
                    "integer" => json!(1),
                    "array" => json!([]),
                    "boolean" => json!(true),
                    _ => json!("value"),
                };
                minimal.insert(field.clone(), placeholder);
            }
            let args = Value::Object(minimal);
            ToolCall::from_tool_name(&spec.name, args)
                .unwrap_or_else(|e| panic!("tool '{}' minimal args failed: {}", spec.name, e));

            // Omitting each required field should fail.
            for field in &required {
                let mut partial = serde_json::Map::new();
                for f in &required {
                    if f != field {
                        let prop = &properties[f.as_str()];
                        let kind = prop["type"].as_str().unwrap_or("string");
                        let placeholder = match kind {
                            "integer" => json!(1),
                            "array" => json!([]),
                            "boolean" => json!(true),
                            _ => json!("value"),
                        };
                        partial.insert(f.clone(), placeholder);
                    }
                }
                let err = ToolCall::from_tool_name(&spec.name, Value::Object(partial))
                    .err()
                    .unwrap_or_else(|| {
                        panic!(
                            "tool '{}' should reject missing required field '{}'",
                            spec.name, field
                        )
                    });
                assert!(
                    err.contains(field),
                    "tool '{}' error for missing '{}' should mention field: {}",
                    spec.name,
                    field,
                    err
                );
            }
        }
    }

    #[test]
    fn tool_specs_optional_fields_are_not_required() {
        // Optional fields (e.g. timeout_secs, cwd) must not appear in required.
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let run_shell = specs.iter().find(|s| s.name == "run_shell").unwrap();
        let required: Vec<String> = run_shell.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"command".to_string()));
        assert!(!required.contains(&"timeout_secs".to_string()));
        assert!(!required.contains(&"cwd".to_string()));
    }

    #[test]
    fn tool_specs_covers_expected_tool_set() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for expected in [
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
            "run_shell",
            "run_job",
            "run_codex",
            "job_status",
            "job_log",
            "read_file",
            "git_status",
            "git_diff",
            "apply_patch",
            "apply_patch_checked",
            "validate_patch",
            "delete_project_files",
            "git_restore_paths",
            "discard_untracked",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "expected tool '{}' in specs: {:?}",
                expected,
                names
            );
        }
    }

    #[test]
    fn tool_specs_descriptions_fit_gpt_action_limit() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            assert!(
                spec.description.chars().count() <= 300,
                "{} description is too long: {} chars",
                spec.name,
                spec.description.chars().count()
            );
        }
    }

    // =========================================================================
    // Phase 2: schema coverage for the generic callRuntimeTool tool set
    // =========================================================================

    /// Helper: fetch a ToolSpec by name from the runtime.
    fn spec_named<'a>(specs: &'a [ToolSpec], name: &str) -> &'a ToolSpec {
        specs
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("tool '{}' missing from specs", name))
    }

    /// Helper: the `required` field of a tool's input schema, as Strings.
    fn required_fields(spec: &ToolSpec) -> Vec<String> {
        spec.input_schema["required"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn tool_specs_apply_patch_checked_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "apply_patch_checked");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"patch".to_string()));
        assert!(!required.contains(&"deny_sensitive_paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_validate_patch_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "validate_patch");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"patch".to_string()));
        assert!(!required.contains(&"deny_sensitive_paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_git_diff_summary_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "git_diff_summary");
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_delete_project_files_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "delete_project_files");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_git_restore_paths_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "git_restore_paths");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_discard_untracked_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "discard_untracked");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_list_project_files_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "list_project_files");
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        // path/limit are optional.
        assert!(!required.contains(&"path".to_string()));
        assert!(!required.contains(&"limit".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_search_project_text_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "search_project_text");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"pattern".to_string()));
        assert!(!required.contains(&"path".to_string()));
        assert!(!required.contains(&"limit".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_list_jobs_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "list_jobs");
        let required = required_fields(spec);
        // list_jobs has only optional fields.
        assert!(required.is_empty());
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_job_tail_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "job_tail");
        let required = required_fields(spec);
        assert_eq!(required, vec!["job_id".to_string()]);
        assert!(!required.contains(&"tail_lines".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_categories_and_recommended_flows_are_well_formed() {
        let runtime = test_runtime();
        let categories = runtime.tool_categories();
        // Every declared category is a non-empty array of known tool names.
        let names = runtime.tool_names();
        for (cat, members) in categories.as_object().unwrap() {
            let arr = members.as_array().unwrap();
            assert!(!arr.is_empty(), "category '{}' must not be empty", cat);
            for m in arr {
                let name = m.as_str().unwrap();
                assert!(
                    names.iter().any(|n| n == name),
                    "category '{}' lists unknown tool '{}'",
                    cat,
                    name
                );
            }
        }
        // Each expected category is present.
        for cat in [
            "inspect", "git", "patch", "shell", "jobs", "runtime", "cleanup",
        ] {
            assert!(
                categories.as_object().unwrap().contains_key(cat),
                "missing category {}",
                cat
            );
        }
        // recommended_flows are short and non-empty.
        let flows = ToolRuntime::recommended_flows();
        assert!(!flows.is_empty());
        for flow in &flows {
            assert!(flow.chars().count() <= 300, "flow too long: {}", flow);
        }
    }

    #[test]
    fn from_tool_name_unknown_tool_lists_available_tools_and_hint() {
        let err = ToolCall::from_tool_name("definitely_not_a_tool", Value::Null).unwrap_err();
        assert!(err.contains("definitely_not_a_tool"));
        assert!(
            err.contains("listRuntimeTools") || err.contains("list_tools"),
            "unknown-tool error should hint at discovery: {}",
            err
        );
        // Should list at least a couple of known tool names.
        assert!(err.contains("git_diff_summary"));
        assert!(err.contains("apply_patch_checked"));
        // Must not leak secret/config artifacts.
        let lower = err.to_lowercase();
        for forbidden in ["token", "authorization", "agent.toml", "drop.env", "secret"] {
            assert!(
                !lower.contains(&forbidden),
                "unknown-tool error must not leak '{}': {}",
                forbidden,
                err
            );
        }
    }

    #[test]
    fn known_tool_names_matches_spec_count() {
        let runtime = test_runtime();
        let spec_count = runtime.tool_specs().len();
        assert_eq!(
            KNOWN_TOOL_NAMES.len(),
            spec_count,
            "KNOWN_TOOL_NAMES must stay in sync with tool_specs()"
        );
        // Every known name must be recognized (i.e. must NOT yield the
        // "unknown tool" error). Unit tools parse with null args; non-unit
        // tools fail with a missing-field error, which is still a recognition
        // success (the variant matched).
        for name in KNOWN_TOOL_NAMES {
            assert!(
                is_known_tool_name(name),
                "known name '{}' not recognized by is_known_tool_name",
                name
            );
            let result = ToolCall::from_tool_name(name, Value::Null);
            match result {
                Ok(_) => {}
                Err(e) => {
                    assert!(
                        !e.contains("unknown tool"),
                        "known tool '{}' was treated as unknown: {}",
                        name,
                        e
                    );
                }
            }
        }
        // An unknown name must still produce the unknown-tool error.
        let err = ToolCall::from_tool_name("not_a_real_tool", Value::Null).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    // =========================================================================
    // Phase 2: local job recovery, path safety, status normalization, bounded logs
    // =========================================================================

    fn local_project_config(path: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn runtime_with_project(root: &Path, project_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            project_id.to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    /// Write a fake on-disk local job simulating a job that survived a restart.
    fn write_fake_job(
        root: &Path,
        job_id: &str,
        project: &str,
        path: &str,
        status: &str,
        stdout: &str,
        stderr: &str,
        meta_extra: Value,
    ) -> PathBuf {
        let dir = root.join(format!(".codex/jobs/{}", job_id));
        fs::create_dir_all(&dir).unwrap();
        let mut meta = json!({
            "job_id": job_id,
            "project": project,
            "path": path,
            "command": "echo test",
            "status": "running",
            "created_at": 1000,
            "started_at": 1000,
            "max_runtime_secs": 3600,
            "executor": "local",
            "kind": "shell",
        });
        if let (Value::Object(ref mut m), Value::Object(extra)) = (&mut meta, meta_extra) {
            for (k, v) in extra {
                m.insert(k, v);
            }
        }
        fs::write(
            dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("status"), status).unwrap();
        fs::write(dir.join("stdout.log"), stdout).unwrap();
        fs::write(dir.join("stderr.log"), stderr).unwrap();
        dir
    }

    #[test]
    fn is_safe_job_id_rejects_path_traversal_and_separators() {
        assert!(is_safe_job_id("11111111-2222-3333-4444-555555555555"));
        assert!(is_safe_job_id("job.1_2-3"));
        assert!(!is_safe_job_id("../escape"));
        assert!(!is_safe_job_id("a/b"));
        assert!(!is_safe_job_id("a\\b"));
        assert!(!is_safe_job_id(".."));
        assert!(!is_safe_job_id("a..b/../c"));
        assert!(!is_safe_job_id(""));
        assert!(!is_safe_job_id("a\0b"));
    }

    #[test]
    fn normalize_local_status_maps_known_and_unknown_values() {
        assert_eq!(normalize_local_status("running"), "running");
        assert_eq!(normalize_local_status("completed"), "completed");
        assert_eq!(normalize_local_status("failed"), "failed");
        assert_eq!(normalize_local_status("stopped"), "stopped");
        assert_eq!(normalize_local_status("queued"), "queued");
        assert_eq!(normalize_local_status("  failed  "), "failed");
        assert_eq!(normalize_local_status(""), "running");
        assert_eq!(normalize_local_status("weird-state"), "lost");
    }

    #[test]
    fn read_lines_from_is_bounded_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, next) = read_lines_from(path, None, None);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
        // Default is tail: last 500 lines.
        assert_eq!(lines[0], "line 501");
        assert_eq!(lines.last().unwrap(), &"line 1000");
        assert_eq!(next, 1001);
    }

    #[test]
    fn read_lines_from_supports_offset_pagination() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=600)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, next) = read_lines_from(path.clone(), Some(1), None);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines.last().unwrap(), &"line 500");
        assert_eq!(next, 501);

        let (text2, next2) = read_lines_from(path, Some(501), None);
        let lines2: Vec<&str> = text2.lines().collect();
        assert_eq!(lines2.len(), 100);
        assert_eq!(lines2[0], "line 501");
        assert_eq!(lines2.last().unwrap(), &"line 600");
        assert_eq!(next2, 601);
    }

    #[test]
    fn read_lines_from_supports_tail_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, _next) = read_lines_from(path, None, Some(10));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0], "line 991");
        assert_eq!(lines.last().unwrap(), &"line 1000");
    }

    #[test]
    fn read_lines_from_tail_is_capped_to_max() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        // Requesting more than MAX returns at most MAX.
        let (text, _) = read_lines_from(path, None, Some(5000));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
    }

    #[tokio::test]
    async fn recover_local_job_status_after_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let project_id = "demo";
        let runtime = runtime_with_project(root, project_id);
        let job_id = "11111111-2222-3333-4444-555555555555";
        write_fake_job(
            root,
            job_id,
            project_id,
            &root.to_string_lossy(),
            "completed",
            "hello\n",
            "",
            json!({}),
        );
        // local_jobs is empty (simulating restart); recovery should find it.
        assert!(runtime.local_jobs.lock().await.is_empty());
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert_eq!(result.output["project"], project_id);
        assert_eq!(result.output["executor"], "local");
        assert_eq!(result.output["kind"], "shell");
        // Recovered job is now cached in memory.
        assert!(runtime.local_jobs.lock().await.contains_key(job_id));
    }

    #[tokio::test]
    async fn recover_local_job_log_after_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "22222222-3333-4444-5555-666666666666";
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "stdout line\n",
            "stderr line\n",
            json!({}),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["stdout"], "stdout line");
        assert_eq!(result.output["stderr"], "stderr line");
        assert!(result.output["next_stdout_line"].is_number());
    }

    #[tokio::test]
    async fn recover_local_job_rejects_unsafe_job_id() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        // Path-traversal job ids must not reach the filesystem.
        let result = runtime.job_status("../escape".to_string()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn recover_local_job_rejects_metadata_project_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "33333333-4444-5555-6666-777777777777";
        // Metadata claims project "other"; configured project is "demo".
        write_fake_job(
            root,
            job_id,
            "other",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({}),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(!result.success, "mismatched metadata must not be recovered");
    }

    #[tokio::test]
    async fn recover_local_job_rejects_metadata_path_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "44444444-5555-6666-7777-888888888888";
        // Metadata path points elsewhere even though project id matches.
        write_fake_job(
            root,
            job_id,
            "demo",
            "/some/other/path",
            "running",
            "",
            "",
            json!({}),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(
            !result.success,
            "mismatched metadata path must not be recovered"
        );
    }

    #[tokio::test]
    async fn recover_local_job_unknown_when_no_metadata_anywhere() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        let result = runtime
            .job_status("55555555-6666-7777-8888-999999999999".to_string())
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn local_job_status_marks_over_time_running_job_lost() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "66666666-7777-8888-9999-000000000000";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "lost");
    }

    #[tokio::test]
    async fn local_job_status_keeps_completed_job_completed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "77777777-8888-9999-0000-111111111111";
        let past = chrono::Utc::now().timestamp() - 100_000;
        // Completed jobs stay completed even if max_runtime would have passed.
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "completed");
    }

    // =========================================================================
    // Phase 11: local job lifecycle hardening (process-group reclamation)
    // =========================================================================

    /// Test double for `LocalJobKiller` that records the (pid, pgid) pairs it
    /// was asked to terminate without touching any real process. Deterministic
    /// by construction — no real `kill` is invoked, so these tests never flake
    /// on process timing.
    #[derive(Default, Clone)]
    struct FakeJobKiller {
        calls: Arc<std::sync::Mutex<Vec<(i64, i64)>>>,
    }

    impl FakeJobKiller {
        fn calls(&self) -> Vec<(i64, i64)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl LocalJobKiller for FakeJobKiller {
        fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
            self.calls.lock().unwrap().push((pid, pgid));
            // Fake pids are never alive; report AlreadyGone. The runtime still
            // persists a terminal status, which is what the tests assert.
            TerminateOutcome::AlreadyGone
        }
    }

    fn runtime_with_fake_killer(root: &Path, project_id: &str) -> (ToolRuntime, FakeJobKiller) {
        let mut runtime = runtime_with_project(root, project_id);
        let killer = FakeJobKiller::default();
        let killer_dyn: Arc<dyn LocalJobKiller> = Arc::new(killer.clone());
        runtime.job_killer = killer_dyn;
        (runtime, killer)
    }

    /// Write a fake on-disk local job plus a `pid` file and `process_group_id`
    /// metadata field, simulating a job spawned by the current code.
    fn write_fake_job_with_pgid(
        root: &Path,
        job_id: &str,
        project: &str,
        path: &str,
        status: &str,
        pid: i64,
        meta_extra: Value,
    ) -> PathBuf {
        let dir = write_fake_job(root, job_id, project, path, status, "", "", meta_extra);
        fs::write(dir.join("pid"), pid.to_string()).unwrap();
        dir
    }

    #[tokio::test]
    async fn run_job_rejects_server_configured_project_without_local_spawn() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let result = runtime
            .run_job("demo".to_string(), "true".to_string(), Some(10), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("projects.toml"));
        assert!(runtime.local_jobs.lock().await.is_empty());
    }

    #[tokio::test]
    async fn timeout_terminates_recorded_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "12121212-3434-5656-7878-909090909090";
        let past = chrono::Utc::now().timestamp() - 100_000;
        let dir = write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            12345,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 12345 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        assert!(result.output["note"]
            .as_str()
            .unwrap()
            .contains("process group 12345"));
        // The recorded pgid was targeted for termination.
        assert_eq!(killer.calls(), vec![(12345, 12345)]);
        // Terminal state persisted to disk.
        assert_eq!(read_trim(dir.join("status")).unwrap(), "lost");
        assert!(read_trim(dir.join("finished_at")).is_some());
    }

    #[tokio::test]
    async fn timeout_without_pid_only_marks_lost_no_kill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "13131313-4545-6767-8989-101010101010";
        let past = chrono::Utc::now().timestamp() - 100_000;
        // No pid file, no process_group_id — simulates very old metadata that
        // predates pid/pgid tracking. We must NOT guess a pid to kill.
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        // No kill attempted because no pid/pgid was recorded.
        assert!(killer.calls().is_empty());
        assert!(result.output["note"].as_str().unwrap().contains("no pid"));
    }

    #[tokio::test]
    async fn job_log_also_reclaims_timeout_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "14141414-5656-7878-9090-111111111111";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            4242,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 4242 }),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        assert_eq!(killer.calls(), vec![(4242, 4242)]);
    }

    #[tokio::test]
    async fn timeout_does_not_affect_completed_job() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "15151515-6767-8989-1010-121212121212";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            9999,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 9999 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert!(killer.calls().is_empty());
    }

    #[tokio::test]
    async fn stop_job_terminates_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "16161616-7878-9090-1111-131313131313";
        let now = chrono::Utc::now().timestamp();
        let dir = write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            7777,
            json!({ "started_at": now, "max_runtime_secs": 3600, "process_group_id": 7777 }),
        );
        let result = runtime.stop_job(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "stopped");
        assert_eq!(killer.calls(), vec![(7777, 7777)]);
        assert_eq!(read_trim(dir.join("status")).unwrap(), "stopped");
        assert!(read_trim(dir.join("finished_at")).is_some());
    }

    #[tokio::test]
    async fn stop_job_leaves_completed_job_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "17171717-8989-1010-1212-141414141414";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            8888,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 8888 }),
        );
        let result = runtime.stop_job(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert!(killer.calls().is_empty());
    }

    #[tokio::test]
    async fn stop_job_rejects_unsafe_job_id() {
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
        let result = runtime.stop_job("../escape".to_string()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("invalid job id"));
    }

    #[tokio::test]
    async fn stop_job_unknown_job_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
        let result = runtime
            .stop_job("55555555-6666-7777-8888-999999999999".to_string())
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn job_log_recovery_returns_bounded_output() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "88888888-9999-0000-1111-222222222222";
        let stdout = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            &stdout,
            "",
            json!({}),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success);
        let out = result.output["stdout"].as_str().unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
        assert!(out.contains("line 1000"));
        assert!(!out.contains("line 1\n"));
    }

    #[tokio::test]
    async fn job_log_recovery_paginates_with_offset() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "99999999-0000-1111-2222-333333333333";
        let stdout = (1..=600)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            &stdout,
            "",
            json!({}),
        );
        let first = runtime.job_log(job_id.to_string(), Some(1), None).await;
        assert!(first.success);
        let out = first.output["stdout"].as_str().unwrap();
        assert!(out.contains("line 1"));
        assert!(out.contains("line 500"));
        assert!(!out.contains("line 501"));
        assert_eq!(first.output["next_stdout_line"], 501);

        let second = runtime.job_log(job_id.to_string(), Some(501), None).await;
        assert!(second.success);
        let out2 = second.output["stdout"].as_str().unwrap();
        assert!(out2.contains("line 501"));
        assert!(out2.contains("line 600"));
        assert_eq!(second.output["next_stdout_line"], 601);
    }

    // =========================================================================
    // Phase 3: harden run_codex — command construction, validation, output
    // =========================================================================

    fn codex_config_with_allowlist(allowlist: &[&str]) -> CodexConfig {
        CodexConfig {
            bin: "codex".to_string(),
            approval_mode: String::new(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: allowlist.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn runtime_with_codex(root: &Path, codex: CodexConfig) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            "demo".to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(codex),
            Arc::new(RuntimeInfo::default()),
        )
    }

    #[test]
    fn build_codex_command_uses_default_bin_and_approval_mode() {
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "fix tests", None, None).unwrap();
        // Default approval_mode is disabled (empty), so --approval-mode is not
        // emitted. This keeps the runtime compatible with Codex CLI builds
        // that do not support the flag.
        assert!(
            !cmd.contains("--approval-mode"),
            "default command must not include --approval-mode, got: {}",
            cmd
        );
        assert!(cmd.starts_with("'codex' "));
        assert!(cmd.ends_with("'fix tests'"));
    }

    #[test]
    fn build_codex_command_uses_configured_bin_and_approval_mode() {
        let codex = CodexConfig {
            bin: "/usr/local/bin/codex".to_string(),
            approval_mode: "suggest".to_string(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: vec![],
        };
        let cmd = build_codex_command(&codex, "hello", None, None).unwrap();
        assert!(cmd.starts_with("'/usr/local/bin/codex' --approval-mode 'suggest' "));
    }

    #[test]
    fn build_codex_command_config_suggest_emits_flag() {
        // CODEX_APPROVAL_MODE=suggest should include --approval-mode suggest.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
        assert!(cmd.contains("--approval-mode 'suggest'"));
    }

    #[test]
    fn build_codex_command_config_none_omits_flag() {
        // CODEX_APPROVAL_MODE=none must not emit --approval-mode.
        for value in ["none", "off", "disabled", "NONE", "Off"] {
            let codex = CodexConfig {
                approval_mode: value.to_string(),
                ..CodexConfig::default()
            };
            let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
            assert!(
                !cmd.contains("--approval-mode"),
                "CODEX_APPROVAL_MODE={:?} should omit --approval-mode, got: {}",
                value,
                cmd
            );
        }
    }

    #[test]
    fn build_codex_command_request_approval_mode_overrides_config() {
        // A config with suggest is overridden by an explicit request value.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", Some("full-auto"), None).unwrap();
        assert!(cmd.contains("--approval-mode 'full-auto'"));
        assert!(!cmd.contains("'suggest'"));
    }

    #[test]
    fn build_codex_command_request_approval_mode_none_omits_flag() {
        // request approval_mode=none overrides a non-empty config and omits the
        // flag entirely.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        for value in ["none", "off", "disabled", ""] {
            let cmd = build_codex_command(&codex, "hi", Some(value), None).unwrap();
            assert!(
                !cmd.contains("--approval-mode"),
                "request approval_mode={:?} should omit --approval-mode, got: {}",
                value,
                cmd
            );
        }
    }

    #[test]
    fn build_codex_command_request_approval_mode_blank_omits_flag() {
        // A blank request value means disabled (not "fall back to config").
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", Some("   "), None).unwrap();
        assert!(!cmd.contains("--approval-mode"));
    }

    #[test]
    fn build_codex_command_shell_escapes_prompt() {
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "rm -rf /'; echo pwned", None, None).unwrap();
        // The single quote in the prompt must be escaped with '\'\'',
        // preventing the trailing "; echo pwned" from running as a command.
        assert!(cmd.contains("'\\''"));
        // The whole prompt is wrapped in single quotes, so the semicolon is
        // literal, not a command separator.
        assert!(cmd.contains("'; echo pwned'"));
    }

    #[test]
    fn build_codex_command_rejects_empty_prompt_via_validate() {
        // build_codex_command itself does not check emptiness (run_codex does),
        // but an empty prompt still gets escaped. Verify it doesn't panic.
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "", None, None).unwrap();
        // Empty prompt produces a trailing ''.
        assert!(cmd.ends_with(" ''"));
    }

    #[test]
    fn build_codex_command_rejects_extra_args_by_default() {
        let codex = CodexConfig::default(); // empty allowlist
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--verbose".to_string()]))
            .unwrap_err();
        assert!(err.contains("allowlist"));
        assert!(err.contains("--verbose"));
    }

    #[test]
    fn build_codex_command_allows_allowlisted_extra_args() {
        let codex = codex_config_with_allowlist(&["--verbose", "--json"]);
        let cmd = build_codex_command(
            &codex,
            "hi",
            None,
            Some(vec!["--verbose".to_string(), "--json".to_string()]),
        )
        .unwrap();
        assert!(cmd.contains("'--verbose'"));
        assert!(cmd.contains("'--json'"));
    }

    #[test]
    fn build_codex_command_rejects_non_allowlisted_extra_args() {
        let codex = codex_config_with_allowlist(&["--verbose"]);
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--danger".to_string()]))
            .unwrap_err();
        assert!(err.contains("allowlist"));
        assert!(err.contains("--danger"));
    }

    #[test]
    fn build_codex_command_rejects_nul_in_extra_arg() {
        let codex = codex_config_with_allowlist(&["--verbose"]);
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--ver\0bose".to_string()]))
            .unwrap_err();
        assert!(err.contains("NUL"));
    }

    #[test]
    fn build_codex_command_rejects_too_many_extra_args() {
        let allowed: Vec<String> = (0..40).map(|i| format!("--a{}", i)).collect();
        let codex = CodexConfig {
            allowed_extra_args: allowed.clone(),
            ..CodexConfig::default()
        };
        let too_many: Vec<String> = allowed;
        let err = build_codex_command(&codex, "hi", None, Some(too_many)).unwrap_err();
        assert!(err.contains("at most 32"));
    }

    #[tokio::test]
    async fn run_codex_rejects_empty_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "   ".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn run_codex_rejects_nul_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix\0tests".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn run_codex_rejects_oversized_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let codex = CodexConfig {
            max_prompt_bytes: 16,
            ..CodexConfig::default()
        };
        let runtime = runtime_with_codex(tmp.path(), codex);
        let big = "x".repeat(100);
        let result = runtime
            .run_codex("demo".to_string(), big, None, None, None, None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("too large"));
        assert!(err.contains("16"));
    }

    #[tokio::test]
    async fn run_codex_rejects_nul_approval_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix tests".to_string(),
                Some("full\0auto".to_string()),
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn run_codex_rejects_extra_args_without_allowlist() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix tests".to_string(),
                None,
                None,
                None,
                Some(vec!["--verbose".to_string()]),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("allowlist"));
    }

    #[tokio::test]
    async fn run_codex_agent_output_contains_structured_fields() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", None, caps).await;
        let project = agent_test_project_id("oe");
        let result = runtime
            .run_codex(
                project.clone(),
                "echo hello".to_string(),
                None,
                Some(10),
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["job_id"].is_string());
        assert_eq!(result.output["kind"], "codex");
        assert_eq!(result.output["project"], project);
        assert_eq!(result.output["status_endpoint"], "/api/jobs/status");
        assert_eq!(result.output["log_endpoint"], "/api/jobs/log");
        assert!(
            runtime.local_jobs.lock().await.is_empty(),
            "agent-backed Codex jobs must not create server-local job metadata"
        );
    }

    #[tokio::test]
    async fn run_codex_rejects_server_configured_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_codex(root, CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "echo hello".to_string(),
                None,
                Some(10),
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("projects.toml"));
        assert!(runtime.local_jobs.lock().await.is_empty());
    }

    #[tokio::test]
    async fn run_codex_agent_uses_configured_command_builder() {
        let codex = CodexConfig {
            default_timeout_secs: 42,
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let mut runtime = runtime_with_agent_project("oe");
        runtime.codex = Arc::new(codex);
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", None, caps).await;
        let result = runtime
            .run_codex(
                agent_test_project_id("oe"),
                "echo hi".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = runtime.shell_clients.list_jobs(None).await;
        assert_eq!(jobs.len(), 1);
        // The configured approval_mode flows through build_codex_command into
        // the agent job's command preview.
        assert!(
            jobs[0]
                .command_preview
                .contains("--approval-mode 'suggest'"),
            "{}",
            jobs[0].command_preview
        );
    }

    #[tokio::test]
    async fn run_codex_agent_omits_approval_mode_when_disabled() {
        // Default (disabled) approval_mode must not emit --approval-mode.
        let codex = CodexConfig::default();
        let mut runtime = runtime_with_agent_project("om");
        runtime.codex = Arc::new(codex);
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "om", None, caps).await;
        let result = runtime
            .run_codex(
                agent_test_project_id("om"),
                "echo hi".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = runtime.shell_clients.list_jobs(None).await;
        assert_eq!(jobs.len(), 1);
        assert!(
            !jobs[0].command_preview.contains("--approval-mode"),
            "disabled approval_mode must omit --approval-mode, got: {}",
            jobs[0].command_preview
        );
    }

    // =========================================================================
    // Phase 6: agent capability checks, owner boundary, structured errors
    // =========================================================================

    use crate::shell_protocol::{
        ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
        ShellClientCapabilities, ShellClientRegisterRequest,
    };

    fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            user_id: username.map(|u| format!("user-{}", u)),
            username: username.map(str::to_string),
            api_key_id: username.map(|u| format!("key-{}", u)),
            api_key_name: username.map(|u| format!("{} key", u)),
            is_bootstrap,
        }
    }

    fn agent_project_config(path: &str, client_id: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Agent,
            client_id: Some(client_id.to_string()),
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn runtime_with_agent_project(client_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            "agent-proj".to_string(),
            agent_project_config("/tmp/agent-proj", client_id),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    async fn register_agent(
        runtime: &ToolRuntime,
        client_id: &str,
        owner: Option<&str>,
        caps: ShellClientCapabilities,
    ) {
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                display_name: None,
                owner: owner.map(str::to_string),
                hostname: None,
                capabilities: Some(caps),
                projects: Some(vec![registered_project("agent-proj", "/tmp/agent-proj")]),
                agent_protocol_version: Some("polling-v1".to_string()),
            })
            .await
            .unwrap();
    }

    fn agent_test_project_id(client_id: &str) -> String {
        ToolRuntime::agent_project_runtime_id(client_id, "agent-proj")
    }

    fn registered_project(id: &str, path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(id.to_string()),
            path: path.to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            description: None,
            hooks: Vec::new(),
            disabled: false,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 123,
        }
    }

    #[tokio::test]
    async fn apply_patch_agent_does_not_require_server_local_project_root() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: "patcher".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(caps),
                projects: Some(vec![registered_project(
                    "agent-proj",
                    "/definitely/not/on/server/private-drop-agent-only",
                )]),
                agent_protocol_version: Some("polling-v1".to_string()),
            })
            .await
            .unwrap();

        let project = agent_test_project_id("patcher");
        let patch = "diff --git a/REMOTE_ONLY.md b/REMOTE_ONLY.md\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/REMOTE_ONLY.md\n\
@@ -0,0 +1 @@\n\
+remote\n"
            .to_string();
        let runtime_for_task = runtime.clone();
        let apply_task =
            tokio::spawn(async move { runtime_for_task.apply_patch(project, patch).await });

        let mut check_req = None;
        for _ in 0..10 {
            check_req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "patcher".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if check_req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let check_req =
            check_req.expect("apply_patch should enqueue git apply --check for the agent");
        assert_eq!(check_req.command, "git apply --check - && echo OK");
        assert!(check_req
            .stdin
            .as_deref()
            .unwrap_or("")
            .contains("REMOTE_ONLY.md"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "patcher".to_string(),
                request_id: check_req.request_id,
                exit_code: Some(0),
                stdout: Some("OK\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let mut apply_req = None;
        for _ in 0..10 {
            apply_req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "patcher".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if apply_req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let apply_req = apply_req.expect("apply_patch should enqueue git apply for the agent");
        assert_eq!(apply_req.command, "git apply -");
        assert!(apply_req
            .stdin
            .as_deref()
            .unwrap_or("")
            .contains("REMOTE_ONLY.md"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "patcher".to_string(),
                request_id: apply_req.request_id,
                exit_code: Some(0),
                stdout: Some(String::new()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let result = apply_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["success"], true);
        assert!(result.output["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("REMOTE_ONLY.md")));
    }

    async fn register_agent_with_projects(
        runtime: &ToolRuntime,
        client_id: &str,
        owner: Option<&str>,
        caps: ShellClientCapabilities,
        projects: Vec<ShellAgentProjectSummary>,
    ) {
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                display_name: None,
                owner: owner.map(str::to_string),
                hostname: None,
                capabilities: Some(caps),
                projects: Some(projects),
                agent_protocol_version: Some("polling-v1".to_string()),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_projects_returns_agent_registered_projects_without_server_config() {
        let runtime = test_runtime();
        register_agent_with_projects(
            &runtime,
            "workstation-1",
            None,
            ShellClientCapabilities::default(),
            vec![registered_project("private-drop", "/root/git/private-drop")],
        )
        .await;

        let result = runtime.dispatch(ToolCall::ListProjects).await;
        assert!(result.success, "{:?}", result.error);
        let projects = result.output.as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], "agent:workstation-1:private-drop");
        assert_eq!(projects[0]["agent_project_id"], "private-drop");
        assert_eq!(projects[0]["executor"], "agent");
        assert_eq!(projects[0]["source"], "agent_registered");
    }

    #[tokio::test]
    async fn server_configured_project_id_is_not_resolved_by_runtime_surface() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: "agent-proj".to_string(),
                    command: "echo hi".to_string(),
                    timeout_secs: Some(1),
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("projects.toml"));
    }

    #[tokio::test]
    async fn agent_run_shell_without_shell_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support shell"), "{}", err);
        assert!(err.contains("agent client oe"), "{}", err);
    }

    #[tokio::test]
    async fn agent_read_file_without_file_read_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        // Default caps: shell=true, file_read=false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReadFile {
                    project: agent_test_project_id("oe"),
                    path: "README.md".to_string(),
                    start_line: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support file_read"), "{}", err);
    }

    #[tokio::test]
    async fn agent_run_job_without_async_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        // Default caps: async_jobs=false, async_shell_jobs=false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support async shell jobs"), "{}", err);
    }

    #[tokio::test]
    async fn agent_git_status_without_shell_or_git_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false; // git stays false by default
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::GitStatus {
                    project: agent_test_project_id("oe"),
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support shell or git"), "{}", err);
    }

    #[tokio::test]
    async fn agent_tool_unknown_client_returns_unknown_shell_client_error() {
        // Project points at client "ghost" which never registered.
        let runtime = runtime_with_agent_project("ghost");
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("ghost"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("unknown shell client"), "{}", err);
        assert!(err.contains("ghost"), "{}", err);
    }

    #[tokio::test]
    async fn agent_tool_rejects_non_owner_api_key() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let bob = auth_context(Some("bob"), false);
        // Use run_job (async) so the test does not hang if owner check leaked.
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bob),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("owned by alice"), "{}", err);
        assert!(err.contains("belongs to bob"), "{}", err);
    }

    #[tokio::test]
    async fn agent_tool_rejects_missing_auth_context() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        // dispatch_with_auth(None): no owner can be proven for an owned agent.
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                None,
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.contains("owned by alice") || err.contains("belongs to anonymous"),
            "{}",
            err
        );
    }

    #[tokio::test]
    async fn agent_tool_allows_owner_api_key_for_run_job() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let alice = auth_context(Some("alice"), false);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&alice),
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["job_id"].is_string());
    }

    #[tokio::test]
    async fn agent_tool_allows_bootstrap_token_for_run_job() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(result.success, "{:?}", result.error);
    }

    #[tokio::test]
    async fn server_configured_local_project_is_not_runtime_surface() {
        // The ChatGPT runtime surface is agent-registered only. A server-side
        // local project config may still exist in older internal modules, but
        // ToolRuntime must not treat it as an exposed project.
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunCodex {
                    project: "demo".to_string(),
                    prompt: "echo hi".to_string(),
                    approval_mode: None,
                    timeout_secs: Some(10),
                    cwd: None,
                    extra_args: None,
                },
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // Phase 7: runtime_status observability tool
    // =========================================================================

    fn runtime_with_info(info: RuntimeInfo) -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        ToolRuntime::new(
            projects,
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(info),
        )
    }

    #[test]
    fn runtime_status_is_in_tool_specs() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "runtime_status"),
            "runtime_status must be in tool_specs: {:?}",
            names
        );
    }

    #[test]
    fn from_tool_name_parses_runtime_status() {
        let call = ToolCall::from_tool_name("runtime_status", Value::Null).unwrap();
        assert!(matches!(call, ToolCall::RuntimeStatus));
        // Also accepts an empty object.
        let call = ToolCall::from_tool_name("runtime_status", json!({})).unwrap();
        assert!(matches!(call, ToolCall::RuntimeStatus));
    }

    #[tokio::test]
    async fn runtime_status_with_no_projects_returns_configured_false() {
        let runtime = test_runtime();
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let out = &result.output;
        assert_eq!(out["service"], "private-drop");
        assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
        assert!(out["server_time"].is_i64());
        assert!(out["pid"].is_i64());
        // No projects.toml -> configured false, load_error present.
        assert_eq!(out["projects"]["configured"], false);
        assert_eq!(out["projects"]["count"], 0);
        assert!(out["projects"]["load_error"].is_string());
    }

    #[tokio::test]
    async fn runtime_status_with_loaded_project_returns_configured_true() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let out = &result.output;
        assert_eq!(out["projects"]["configured"], true);
        assert_eq!(out["projects"]["count"], 1);
        assert!(out["projects"]["load_error"].is_null());
    }

    #[tokio::test]
    async fn runtime_status_does_not_expose_tokens_or_secrets() {
        let info = RuntimeInfo {
            auth_enabled: true,
            configured_public_url: Some("https://example.com".to_string()),
        };
        let runtime = runtime_with_info(info);
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let serialized = serde_json::to_string(&result.output).unwrap();
        // The summary must never contain secret-like field names.
        for forbidden in [
            "token",
            "DROP_TOKEN",
            "api_key",
            "apikey",
            "secret",
            "password",
            "authorization",
            "bearer",
        ] {
            assert!(
                !serialized
                    .to_lowercase()
                    .contains(&forbidden.to_lowercase()),
                "runtime_status output must not contain '{}': {}",
                forbidden,
                serialized
            );
        }
        // auth_enabled is a bool, not the token value.
        assert_eq!(result.output["auth_enabled"], true);
    }

    #[tokio::test]
    async fn runtime_status_auth_enabled_reflects_runtime_info() {
        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: false,
            configured_public_url: None,
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["auth_enabled"], false);
        assert!(result.output["configured_public_url"].is_null());

        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: true,
            configured_public_url: Some("https://drop.example.com".to_string()),
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["auth_enabled"], true);
        assert_eq!(
            result.output["configured_public_url"],
            "https://drop.example.com"
        );
    }

    #[tokio::test]
    async fn runtime_status_agent_summary_includes_protocol_version() {
        use crate::shell_protocol::ShellClientRegisterRequest;
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "agent-1".to_string(),
                display_name: Some("Workstation".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![]),
                agent_protocol_version: Some("polling-v1".to_string()),
            })
            .await
            .unwrap();
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let agents = &result.output["agents"];
        assert_eq!(agents["count"], 1);
        assert_eq!(agents["online_count"], 1);
        assert_eq!(agents["offline_count"], 0);
        assert_eq!(agents["stale_count"], 0);
        let clients = agents["clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["client_id"], "agent-1");
        assert_eq!(clients[0]["agent_protocol_version"], "polling-v1");
        assert_eq!(clients[0]["transport"], "polling");
        assert_eq!(clients[0]["connected"], true);
        assert!(clients[0]["capabilities"].is_object());
        assert_eq!(clients[0]["projects_count"], 0);
        // last_seen must be present as an integer unix timestamp (seconds).
        assert!(
            clients[0]["last_seen"].is_i64(),
            "last_seen must be an integer timestamp: {:?}",
            clients[0]["last_seen"]
        );
    }

    #[tokio::test]
    async fn runtime_status_marks_stale_websocket_agent_with_last_seen() {
        use crate::shell_client::TRANSPORT_WEBSOCKET;
        use crate::shell_protocol::ShellClientRegisterRequest;
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-stale".to_string(),
                display_name: Some("Stale WS".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![]),
                agent_protocol_version: Some("websocket-v1".to_string()),
            })
            .await
            .unwrap();
        registry
            .set_transport("ws-stale", TRANSPORT_WEBSOCKET)
            .await
            .unwrap();
        // Force the agent past the 60s online window so it reads as stale.
        let stale_ts = chrono::Utc::now().timestamp() - 120;
        registry.set_last_seen_for_test("ws-stale", stale_ts).await;

        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let agents = &result.output["agents"];
        assert_eq!(agents["count"], 1);
        assert_eq!(agents["online_count"], 0);
        assert_eq!(agents["stale_count"], 1);
        assert_eq!(agents["offline_count"], 1);
        let entry = &agents["clients"][0];
        assert_eq!(entry["client_id"], "ws-stale");
        assert_eq!(entry["transport"], "websocket");
        assert_eq!(entry["status"], "stale");
        assert_eq!(entry["connected"], false);
        assert_eq!(entry["last_seen"], stale_ts);
    }

    #[tokio::test]
    async fn runtime_status_reflects_websocket_transport_label() {
        let registry = Arc::new(ShellClientRegistry::default());
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry.clone(),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-agent".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("websocket-v1".to_string()),
            })
            .await
            .unwrap();
        // Flip the transport label the same way the WebSocket handler does.
        registry
            .set_transport("ws-agent", crate::shell_client::TRANSPORT_WEBSOCKET)
            .await
            .unwrap();

        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let clients = &result.output["agents"]["clients"];
        let entry = clients
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["client_id"] == "ws-agent")
            .expect("ws-agent present");
        assert_eq!(entry["transport"], "websocket");
        assert_eq!(entry["agent_protocol_version"], "websocket-v1");
    }

    #[tokio::test]
    async fn runtime_status_counts_local_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        // Write a fake local job in "running" state and register it in the
        // in-memory map so runtime_status counts it.
        let job_dir = root.join(".codex/jobs/job-active");
        fs::create_dir_all(&job_dir).unwrap();
        fs::write(job_dir.join("status"), "running").unwrap();
        let meta_json = json!({
            "job_id": "job-active",
            "project": "demo",
            "command": "sleep 10",
            "status": "running",
            "created_at": 1,
            "started_at": 1,
            "max_runtime_secs": 600,
            "executor": "local",
            "path": root.to_string_lossy(),
            "kind": "shell",
        });
        fs::write(
            job_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta_json).unwrap(),
        )
        .unwrap();
        runtime.local_jobs.lock().await.insert(
            "job-active".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir: job_dir,
            },
        );
        // Also write a completed job to verify it's not counted as active.
        let done_dir = root.join(".codex/jobs/job-done");
        fs::create_dir_all(&done_dir).unwrap();
        fs::write(done_dir.join("status"), "completed").unwrap();
        fs::write(
            done_dir.join("metadata.json"),
            serde_json::to_string(&json!({
                "job_id": "job-done",
                "project": "demo",
                "command": "true",
                "status": "completed",
                "created_at": 1,
                "started_at": 1,
                "executor": "local",
                "path": root.to_string_lossy(),
                "kind": "shell",
            }))
            .unwrap(),
        )
        .unwrap();
        runtime.local_jobs.lock().await.insert(
            "job-done".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir: done_dir,
            },
        );

        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let jobs = &result.output["jobs"];
        assert_eq!(jobs["local_known_count"], 2);
        // Only the running job is active.
        assert_eq!(jobs["active_count"], 1);
        assert_eq!(jobs["agent_known_count"], 0);
    }

    #[tokio::test]
    async fn runtime_status_tools_summary_lists_names() {
        let runtime = test_runtime();
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let tools = &result.output["tools"];
        let names = tools["names"].as_array().unwrap();
        assert!(names.len() > 0);
        assert!(
            names.iter().any(|n| n == "runtime_status"),
            "tools.names must include runtime_status: {:?}",
            names
        );
        assert_eq!(tools["count"], names.len() as i64);
    }

    // =========================================================================
    // Phase A read-only console tools
    // =========================================================================

    #[test]
    fn from_tool_name_parses_phase_a_tools() {
        let call =
            ToolCall::from_tool_name("list_project_files", json!({"project": "demo"})).unwrap();
        match call {
            ToolCall::ListProjectFiles {
                project,
                path,
                limit,
            } => {
                assert_eq!(project, "demo");
                assert_eq!(path, None);
                assert_eq!(limit, None);
            }
            other => panic!("expected ListProjectFiles, got {:?}", other),
        }

        let call = ToolCall::from_tool_name(
            "search_project_text",
            json!({"project": "demo", "pattern": "fn main", "limit": 5}),
        )
        .unwrap();
        match call {
            ToolCall::SearchProjectText {
                project,
                pattern,
                path,
                limit,
            } => {
                assert_eq!(project, "demo");
                assert_eq!(pattern, "fn main");
                assert_eq!(path, None);
                assert_eq!(limit, Some(5));
            }
            other => panic!("expected SearchProjectText, got {:?}", other),
        }

        let call =
            ToolCall::from_tool_name("git_diff_summary", json!({"project": "demo"})).unwrap();
        assert!(matches!(call, ToolCall::GitDiffSummary { project } if project == "demo"));

        // list_jobs has only optional fields; null arguments must still parse.
        let call = ToolCall::from_tool_name("list_jobs", Value::Null).unwrap();
        assert!(matches!(
            call,
            ToolCall::ListJobs {
                limit: None,
                status: None
            }
        ));
        let call = ToolCall::from_tool_name("list_jobs", json!({"limit": 3, "status": "running"}))
            .unwrap();
        match call {
            ToolCall::ListJobs { limit, status } => {
                assert_eq!(limit, Some(3));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected ListJobs, got {:?}", other),
        }

        let call = ToolCall::from_tool_name("job_tail", json!({"job_id": "abc", "tail_lines": 10}))
            .unwrap();
        match call {
            ToolCall::JobTail { job_id, tail_lines } => {
                assert_eq!(job_id, "abc");
                assert_eq!(tail_lines, Some(10));
            }
            other => panic!("expected JobTail, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_list_jobs_with_null_arguments_parses() {
        // Regression: a non-unit tool with all-optional fields must deserialize
        // when a caller passes `null` arguments (normalized to an empty object).
        let call = ToolCall::from_tool_name("list_jobs", Value::Null)
            .unwrap_or_else(|e| panic!("list_jobs with null args should parse: {}", e));
        assert!(matches!(call, ToolCall::ListJobs { .. }));
    }

    #[test]
    fn tool_specs_include_phase_a_tools() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for expected in [
            "list_project_files",
            "search_project_text",
            "git_diff_summary",
            "list_jobs",
            "job_tail",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "tool_specs must include '{}': {:?}",
                expected,
                names
            );
        }
    }

    // =========================================================================
    // validate_patch (patch preflight / dry-run)
    // =========================================================================

    #[test]
    fn from_tool_name_parses_checked_and_cleanup_tools() {
        let checked = ToolCall::from_tool_name(
            "apply_patch_checked",
            json!({"project":"agent:c:p","patch":"diff","deny_sensitive_paths":true}),
        )
        .unwrap();
        assert!(matches!(
            checked,
            ToolCall::ApplyPatchChecked { project, patch, deny_sensitive_paths }
                if project == "agent:c:p" && patch == "diff" && deny_sensitive_paths == Some(true)
        ));

        let delete = ToolCall::from_tool_name(
            "delete_project_files",
            json!({"project":"agent:c:p","paths":["tmp.txt"]}),
        )
        .unwrap();
        assert!(
            matches!(delete, ToolCall::DeleteProjectFiles { project, paths } if project == "agent:c:p" && paths == vec!["tmp.txt"])
        );

        let restore = ToolCall::from_tool_name(
            "git_restore_paths",
            json!({"project":"agent:c:p","paths":["README.md"]}),
        )
        .unwrap();
        assert!(
            matches!(restore, ToolCall::GitRestorePaths { project, paths } if project == "agent:c:p" && paths == vec!["README.md"])
        );

        let discard = ToolCall::from_tool_name(
            "discard_untracked",
            json!({"project":"agent:c:p","paths":["tmp.txt"]}),
        )
        .unwrap();
        assert!(
            matches!(discard, ToolCall::DiscardUntracked { project, paths } if project == "agent:c:p" && paths == vec!["tmp.txt"])
        );
    }

    #[test]
    fn parse_porcelain_summary_buckets_untracked_files() {
        let summary = parse_porcelain_summary(
            " M README.md\n?? tmp.txt\nR  old.rs -> new.rs\n!! ignored.log\n",
        );
        assert_eq!(summary.tracked_changed_files, vec!["README.md", "new.rs"]);
        assert_eq!(summary.untracked_files, vec!["tmp.txt"]);
        assert_eq!(summary.ignored_files, vec!["ignored.log"]);
        assert_eq!(summary.changed_files_count, 4);
    }

    #[test]
    fn cleanup_paths_reject_sensitive_and_project_root() {
        let root = vec![".".to_string()];
        assert!(validate_limited_cleanup_paths(&root, true).is_err());
        let sensitive = vec!["agent.toml".to_string()];
        assert!(validate_limited_cleanup_paths(&sensitive, true).is_err());
        let safe = vec!["tmp_web_codex_smoke.txt".to_string()];
        assert_eq!(
            validate_limited_cleanup_paths(&safe, true).unwrap(),
            vec!["tmp_web_codex_smoke.txt".to_string()]
        );
    }
    #[test]
    fn from_tool_name_parses_validate_patch() {
        let call = ToolCall::from_tool_name(
            "validate_patch",
            json!({"project": "agent:c:p", "patch": "diff"}),
        )
        .unwrap();
        assert!(
            matches!(call, ToolCall::ValidatePatch { project, patch, .. } if project == "agent:c:p" && patch == "diff")
        );
    }

    #[test]
    fn tool_specs_include_validate_patch() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "validate_patch"),
            "tool_specs must include validate_patch: {:?}",
            names
        );
    }

    #[test]
    fn validate_preflight_path_rejects_boundary_escapes() {
        // In-bounds relative paths are accepted.
        assert!(validate_preflight_path("README.md").is_ok());
        assert!(validate_preflight_path("src/main.rs").is_ok());
        // Absolute paths, traversal, empty, and NUL are hard rejects.
        assert!(validate_preflight_path("").is_err());
        assert!(validate_preflight_path("/etc/passwd").is_err());
        assert!(validate_preflight_path("../outside").is_err());
        assert!(validate_preflight_path("src/../../outside").is_err());
        assert!(validate_preflight_path("src\0main.rs").is_err());
        // Sensitive filenames are NOT hard-rejected here (they become warnings).
        assert!(validate_preflight_path(".env").is_ok());
        assert!(validate_preflight_path("agent.toml").is_ok());
    }

    #[test]
    fn sensitive_path_warnings_flags_sensitive_names() {
        assert!(sensitive_path_warnings(".env")
            .iter()
            .any(|w| w.contains(".env")));
        assert!(sensitive_path_warnings("config/agent.toml")
            .iter()
            .any(|w| w.contains("agent.toml")));
        assert!(sensitive_path_warnings("private-drop.env")
            .iter()
            .any(|w| w.contains("private-drop.env")));
        assert!(sensitive_path_warnings("projects.d/x.toml")
            .iter()
            .any(|w| w.contains("projects.d")));
        assert!(sensitive_path_warnings(".git/config")
            .iter()
            .any(|w| w.contains(".git")));
        assert!(sensitive_path_warnings("target/debug/x")
            .iter()
            .any(|w| w.contains("target")));
        assert!(sensitive_path_warnings("node_modules/x")
            .iter()
            .any(|w| w.contains("node_modules")));
        // A normal source file produces no warnings.
        assert!(sensitive_path_warnings("src/main.rs").is_empty());
        // Matching is case-insensitive.
        assert!(sensitive_path_warnings("TARGET/foo")
            .iter()
            .any(|w| w.contains("target")));
    }

    #[tokio::test]
    async fn validate_patch_rejects_empty_patch() {
        let runtime = test_runtime();
        let result = runtime
            .validate_patch("agent:c:p".to_string(), "".to_string(), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn validate_patch_rejects_nul_byte_patch() {
        let runtime = test_runtime();
        let result = runtime
            .validate_patch("agent:c:p".to_string(), "diff\0--- a/f\n".to_string(), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn validate_patch_rejects_oversized_patch() {
        let runtime = test_runtime();
        // Build a patch one byte over the limit.
        let oversized = "x".repeat(MAX_VALIDATE_PATCH_BYTES + 1);
        let result = runtime
            .validate_patch("agent:c:p".to_string(), oversized, None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("too large"), "got: {}", err);
    }

    #[tokio::test]
    async fn validate_patch_rejects_non_agent_project() {
        // A server-configured (local) project is not a supported runtime
        // surface for validate_patch. resolve_project rejects it before the
        // agent dry-run path, and the server never reads the filesystem.
        let runtime = test_runtime();
        let patch = "--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\nhello\n+world\n";
        let result = runtime
            .validate_patch("agent:nope:nope".to_string(), patch.to_string(), None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.to_lowercase().contains("unknown") || err.to_lowercase().contains("agent"),
            "expected a routing/rejection error for non-agent project, got: {}",
            err
        );
    }

    #[test]
    fn max_validate_patch_bytes_is_conservative() {
        // Pin the conservative upper bound so it is not accidentally raised.
        assert_eq!(MAX_VALIDATE_PATCH_BYTES, 256 * 1024);
        assert!(MAX_VALIDATE_PATCH_BYTES <= 1024 * 1024);
    }

    #[test]
    fn parse_file_list_entries_is_bounded_and_marks_truncation() {
        // Simulate agent file_list stdout: dirs suffixed with '/'.
        let stdout = "Cargo.toml\nsrc/\nREADME.md\ntarget/\nCargo.lock\n";
        // First, without truncation, verify kinds and project-relative paths.
        let (all, truncated_full) = parse_file_list_entries(stdout, ".", 10);
        assert!(!truncated_full);
        assert_eq!(all.len(), 5);
        let src = all.iter().find(|e| e["path"] == "src").expect("src entry");
        assert_eq!(src["kind"], "dir");
        let cargo = all
            .iter()
            .find(|e| e["path"] == "Cargo.toml")
            .expect("Cargo.toml entry");
        assert_eq!(cargo["kind"], "file");

        // With a tight bound, output is truncated and sorted alphabetically.
        let (bounded, truncated) = parse_file_list_entries(stdout, ".", 3);
        assert_eq!(bounded.len(), 3);
        assert!(truncated);
        let paths: Vec<&str> = bounded
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        // Sorted: Cargo.lock, Cargo.toml, README.md come first.
        assert_eq!(paths, vec!["Cargo.lock", "Cargo.toml", "README.md"]);
    }

    #[test]
    fn parse_file_list_entries_prepends_subpath_for_relative_paths() {
        let stdout = "main.rs\nlib.rs\n";
        let (entries, truncated) = parse_file_list_entries(stdout, "src", 10);
        assert!(!truncated);
        let paths: Vec<&str> = entries
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn validate_project_relative_path_rejects_absolute_and_parent_traversal() {
        assert!(validate_project_relative_path(".").is_ok());
        assert!(validate_project_relative_path("src").is_ok());
        assert!(validate_project_relative_path("src/main.rs").is_ok());
        assert!(validate_project_relative_path("/etc").is_err());
        assert!(validate_project_relative_path("../outside").is_err());
        assert!(validate_project_relative_path("src/../../outside").is_err());
        assert!(validate_project_relative_path("src\0main.rs").is_err());
    }

    #[test]
    fn parse_search_matches_is_bounded_and_strips_dot_slash() {
        let stdout = "./src/main.rs:10:fn main() {}\n./src/lib.rs:3:pub fn x()\n./src/a:1:1\n";
        let (matches, truncated) = parse_search_matches(stdout, 2);
        assert_eq!(matches.len(), 2);
        assert!(truncated);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 10);
        assert_eq!(matches[0]["preview"], "fn main() {}");
        assert_eq!(matches[1]["path"], "src/lib.rs");
    }

    #[test]
    fn parse_search_matches_skips_lines_without_line_number() {
        // Binary file matches or malformed lines are skipped, not counted.
        let stdout = "binary:file\nsrc/main.rs:5:hit\n";
        let (matches, _truncated) = parse_search_matches(stdout, 10);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/main.rs");
    }

    #[test]
    fn parse_porcelain_files_handles_basic_rename_and_quoted_paths() {
        let porcelain =
            " M src/main.rs\nA  new_file.rs\nR  old_name.rs -> new_name.rs\n?? \"quoted path.rs\"";
        let files = parse_porcelain_files(porcelain);
        assert_eq!(
            files,
            vec![
                "src/main.rs",
                "new_file.rs",
                "new_name.rs",
                "quoted path.rs",
            ]
        );
    }

    #[test]
    fn split_diff_summary_separates_porcelain_and_stat() {
        let stdout = format!(
            " M src/a.rs\nA  src/b.rs\n\n{}\n src/a.rs | 2 +-\n 1 file changed",
            DIFF_SUMMARY_SENTINEL,
        );
        let (porcelain, diff_stat) = split_diff_summary(&stdout);
        assert!(porcelain.contains("src/a.rs"));
        assert!(porcelain.contains("src/b.rs"));
        assert!(!porcelain.contains(DIFF_SUMMARY_SENTINEL));
        assert!(diff_stat.contains("1 file changed"));
        assert!(!diff_stat.contains(DIFF_SUMMARY_SENTINEL));
    }

    #[test]
    fn split_diff_summary_without_sentinel_returns_all_as_porcelain() {
        let (porcelain, diff_stat) = split_diff_summary("just status lines");
        assert_eq!(porcelain, "just status lines");
        assert_eq!(diff_stat, "");
    }

    #[test]
    fn search_project_text_command_excludes_sensitive_dirs_and_bounds_output() {
        let cmd = search_project_text_command("fn main", "src", 25);
        assert!(cmd.contains("--exclude-dir=.git"));
        assert!(cmd.contains("--exclude-dir=target"));
        assert!(cmd.contains("--exclude-dir=node_modules"));
        assert!(cmd.contains("head -n 26"));
        assert!(cmd.contains("grep -rnI"));
    }

    #[test]
    fn git_diff_summary_command_is_read_only() {
        let cmd = git_diff_summary_command();
        // Must run only read-only git inspection subcommands.
        assert!(cmd.contains("git status --porcelain"));
        assert!(cmd.contains("git diff --stat"));
        // No mutating subcommands may appear.
        for forbidden in [
            "apply", "commit", "checkout", "reset", "push", "stash", "merge", "rebase", "rm ",
        ] {
            assert!(
                !cmd.contains(forbidden),
                "git_diff_summary command must not contain '{}': {}",
                forbidden,
                cmd
            );
        }
    }

    #[tokio::test]
    async fn list_jobs_returns_bounded_summaries_without_stdout_stderr() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        // Seed a local job whose on-disk logs contain sensitive-looking text.
        let dir = write_fake_job(
            root,
            "job-secret",
            "demo",
            &root.to_string_lossy(),
            "completed",
            "DROP_TOKEN=supersecret\nline2",
            "Authorization: Bearer xyz",
            json!({}),
        );
        runtime.local_jobs.lock().await.insert(
            "job-secret".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir,
            },
        );
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: None,
                status: None,
            })
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = result.output["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job["job_id"], "job-secret");
        assert_eq!(job["status"], "completed");
        assert_eq!(job["executor"], "local");
        // Summaries must never carry stdout/stderr bodies.
        assert!(
            job.get("stdout").is_none(),
            "list_jobs summary must not include stdout"
        );
        assert!(
            job.get("stderr").is_none(),
            "list_jobs summary must not include stderr"
        );
        // And the serialized summary must not leak the secret log text.
        let serialized = serde_json::to_string(job).unwrap();
        assert!(
            !serialized.contains("supersecret"),
            "list_jobs summary leaked stdout secret: {}",
            serialized
        );
        assert!(
            !serialized.contains("Bearer xyz"),
            "list_jobs summary leaked stderr secret: {}",
            serialized
        );
    }

    #[tokio::test]
    async fn list_jobs_respects_limit_bound() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        for i in 0..5 {
            let dir = write_fake_job(
                root,
                &format!("job-{}", i),
                "demo",
                &root.to_string_lossy(),
                "completed",
                "",
                "",
                json!({}),
            );
            runtime.local_jobs.lock().await.insert(
                format!("job-{}", i),
                LocalJobRecord {
                    project: "demo".to_string(),
                    dir,
                },
            );
        }
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: Some(2),
                status: None,
            })
            .await;
        assert!(result.success);
        let jobs = result.output["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(result.output["truncated"], true);
    }

    #[tokio::test]
    async fn list_jobs_requires_no_agent_capability() {
        // list_jobs has no project and no agent capability requirement, so it
        // succeeds even with no registered agent.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: None,
                status: None,
            })
            .await;
        assert!(result.success);
        assert!(result.output["jobs"].is_array());
    }

    #[tokio::test]
    async fn job_tail_reaches_job_logic_without_agent_auth() {
        // job_tail bypasses agent authorization (no project). An unknown job
        // returns a structured "unknown job" error, proving it reached the job
        // layer rather than an authorization gate.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::JobTail {
                job_id: "no-such-job".to_string(),
                tail_lines: None,
            })
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("unknown job"),
            "job_tail should report unknown job"
        );
    }

    #[tokio::test]
    async fn list_project_files_requires_file_read_capability() {
        let runtime = runtime_with_agent_project("oe");
        // Default capabilities have file_read = false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ListProjectFiles {
                    project: agent_test_project_id("oe"),
                    path: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("file_read"),
            "list_project_files should require file_read capability"
        );
    }

    #[tokio::test]
    async fn search_project_text_requires_shell_capability() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SearchProjectText {
                    project: agent_test_project_id("oe"),
                    pattern: "fn".to_string(),
                    path: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("shell"),
            "search_project_text should require shell capability"
        );
    }

    #[tokio::test]
    async fn git_diff_summary_requires_git_or_shell_capability() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::GitDiffSummary {
                    project: agent_test_project_id("oe"),
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        // GitOrShell accepts `shell` or `git`; with both off it is rejected.
        let err = result.error.unwrap();
        assert!(
            err.contains("shell") || err.contains("git"),
            "git_diff_summary should require shell or git capability: {}",
            err
        );
    }

    #[tokio::test]
    async fn list_project_files_rejects_non_agent_project_id() {
        // A bare project id (not agent:<client>:<project>) is not resolved by
        // the runtime surface — proving routing goes through the owning agent.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ListProjectFiles {
                project: "some-local-id".to_string(),
                path: None,
                limit: None,
            })
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("agent") || err.contains("projects.toml"));
    }

    #[tokio::test]
    async fn list_project_files_rejects_absolute_or_parent_paths_before_agent_request() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        for path in ["/etc", "../outside"] {
            let result = runtime
                .dispatch_with_auth(
                    ToolCall::ListProjectFiles {
                        project: agent_test_project_id("oe"),
                        path: Some(path.to_string()),
                        limit: None,
                    },
                    Some(&bootstrap),
                )
                .await;
            assert!(!result.success, "path {} should be rejected", path);
            let err = result.error.unwrap();
            assert!(
                err.contains("project-relative") || err.contains("parent traversal"),
                "unexpected error for {}: {}",
                path,
                err
            );
        }
    }

    #[tokio::test]
    async fn search_project_text_rejects_empty_pattern() {
        // Authorization runs before the tool body, so register an agent with
        // shell capability to reach the empty-pattern validation.
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SearchProjectText {
                    project: agent_test_project_id("oe"),
                    pattern: "   ".to_string(),
                    path: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("pattern"));
    }

    #[tokio::test]
    async fn search_project_text_rejects_absolute_or_parent_paths_before_agent_request() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        for path in ["/etc", "../outside"] {
            let result = runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project: agent_test_project_id("oe"),
                        pattern: "needle".to_string(),
                        path: Some(path.to_string()),
                        limit: None,
                    },
                    Some(&bootstrap),
                )
                .await;
            assert!(!result.success, "path {} should be rejected", path);
            let err = result.error.unwrap();
            assert!(
                err.contains("project-relative") || err.contains("parent traversal"),
                "unexpected error for {}: {}",
                path,
                err
            );
        }
    }
}
