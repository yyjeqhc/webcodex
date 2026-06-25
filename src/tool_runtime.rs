//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

use crate::auth::AuthContext;
use crate::config::CodexConfig;
use crate::projects::ProjectsState;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{ShellFileOpRequest, ShellJobOpRequest, ShellRunRequest};
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

    /// List all configured projects.
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

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("tool".to_string(), Value::String(name.to_string()));
        let unit_tool = matches!(
            name,
            "list_tools" | "list_projects" | "list_agents" | "runtime_status"
        );
        if !unit_tool && !arguments.is_null() {
            wrapped.insert("params".to_string(), arguments);
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
        }
    }

    fn resolve_project(&self, project: &str) -> Result<&crate::projects::ProjectConfig, String> {
        let projects = self.projects.config.as_ref().ok_or_else(|| {
            self.projects
                .load_error
                .clone()
                .unwrap_or_else(|| "Projects not configured".to_string())
        })?;
        projects.get_project(project)
    }

    /// The capability an agent-backed tool variant requires from the agent
    /// client. Non-agent tools (and tools without a project) require nothing.
    fn required_agent_capability(call: &ToolCall) -> Option<AgentCapability> {
        match call {
            ToolCall::RunShell { .. } | ToolCall::ApplyPatch { .. } => Some(AgentCapability::Shell),
            ToolCall::ReadFile { .. } => Some(AgentCapability::FileRead),
            ToolCall::GitStatus { .. } | ToolCall::GitDiff { .. } => {
                Some(AgentCapability::GitOrShell)
            }
            ToolCall::RunJob { .. } | ToolCall::RunCodex { .. } => Some(AgentCapability::AsyncJobs),
            ToolCall::ListTools
            | ToolCall::ListProjects
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. } => None,
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
            | ToolCall::GitStatus { project }
            | ToolCall::GitDiff { project, .. }
            | ToolCall::ReadFile { project, .. }
            | ToolCall::RunJob { project, .. }
            | ToolCall::RunCodex { project, .. } => project,
            _ => return Ok(()),
        };
        let required = match Self::required_agent_capability(call) {
            Some(cap) => cap,
            None => return Ok(()),
        };
        let proj = self.resolve_project(project)?;
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

            ToolCall::ListProjects => self.list_projects(),

            ToolCall::ListAgents => self.list_agents().await,

            ToolCall::RuntimeStatus => self.runtime_status().await,

            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
            } => self.run_shell(project, command, timeout_secs, cwd).await,

            ToolCall::ApplyPatch { project, patch } => self.apply_patch(project, patch).await,

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
                description: "List configured projects and their execution mode.".to_string(),
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
                description: "Run a short shell command inside a configured project.".to_string(),
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
                description: "Start an asynchronous shell job inside a configured project."
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
                        "Codex approval mode, default full-auto.",
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
                name: "read_file".to_string(),
                description: "Read a UTF-8 file from a configured project.".to_string(),
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
                description: "Apply a unified diff patch to a configured project.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("patch", "string", "Unified diff patch.", true),
                ]),
            },
        ]
    }

    // -------------------------------------------------------------------------
    // Individual tool implementations — delegate to existing codex/ functions
    // and shell_client handlers.
    // -------------------------------------------------------------------------

    fn list_projects(&self) -> ToolResult {
        let projects = match self.projects.config.as_ref() {
            Some(cfg) => cfg,
            None => {
                let error = self
                    .projects
                    .load_error
                    .clone()
                    .unwrap_or_else(|| "Projects not configured".to_string());
                return ToolResult::err(error);
            }
        };
        let list: Vec<Value> = projects
            .projects
            .iter()
            .map(|(id, proj)| {
                json!({
                    "id": id,
                    "path": proj.path,
                    "executor": if proj.is_agent() { "agent" } else { "local" },
                    "client_id": proj.client_id,
                    "allow_patch": proj.allow_patch,
                })
            })
            .collect();
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
        // state. Only carry fields useful for observability.
        let clients = self.shell_clients.list_clients().await;
        let agent_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        let offline_count = agent_count.saturating_sub(online_count);
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
                    "capabilities": c.capabilities,
                    "projects_count": c.projects.len(),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
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
        let proj = match self.resolve_project(&project) {
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
            let cwd_path = match resolve_local_cwd(proj, cwd.as_deref()) {
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
        let proj = match self.resolve_project(&project) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.allow_patch() {
            return ToolResult::err("Patch is not allowed for this project");
        }
        if patch.is_empty() {
            return ToolResult::err("Patch cannot be empty");
        }
        let root = proj.root();
        if !root.exists() {
            return ToolResult::err("Project root does not exist");
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
            let patch_cmd = format!(
                "cd {} && git apply --check - && echo OK",
                shell_escape_simple(&proj.path)
            );
            let (check_req_id, check_rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id: client_id.clone(),
                        cwd: Some(proj.path.clone()),
                        command: format!("echo {} | {}", shell_escape_simple(&patch), patch_cmd),
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
            let apply_cmd = format!("cd {} && git apply -", shell_escape_simple(&proj.path));
            let (apply_req_id, apply_rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: format!("echo {} | {}", shell_escape_simple(&patch), apply_cmd),
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

    async fn git_status(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project) {
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
        let proj = match self.resolve_project(&project) {
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
        let proj = match self.resolve_project(&project) {
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
        let proj = match self.resolve_project(&project) {
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
            let meta = json!({
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
                    let _ = std::fs::write(dir.join("pid"), child.id().to_string());
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
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            return local_job_status(&job_id, &record);
        }
        // Fall through to agent-backed jobs. If the agent registry does not
        // know this job either, attempt local recovery from on-disk metadata
        // so jobs started before a server restart remain queryable.
        if self.shell_clients.get_job(&job_id).await.is_err() {
            if let Some(record) = self.recover_local_job(&job_id).await {
                return local_job_status(&job_id, &record);
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
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            return local_job_log(&job_id, &record, offset, tail_lines);
        }
        if self.shell_clients.get_job(&job_id).await.is_err() {
            if let Some(record) = self.recover_local_job(&job_id).await {
                return local_job_log(&job_id, &record, offset, tail_lines);
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
    let approval_mode = approval_mode
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| codex.approval_mode.clone());
    validate_cli_arg(&approval_mode, "approval_mode")?;
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
    let mut parts = vec![
        shell_escape_simple(&codex.bin),
        "--approval-mode".to_string(),
        shell_escape_simple(&approval_mode),
    ];
    for arg in &extra_args {
        parts.push(shell_escape_simple(arg));
    }
    parts.push(shell_escape_simple(prompt));
    Ok(parts.join(" "))
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

fn local_job_status(job_id: &str, record: &LocalJobRecord) -> ToolResult {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let mut status = normalize_local_status(&raw_status);
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let finished_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64);
    // Detect over-time running jobs and mark them lost. The process itself is
    // not killed here; this normalizes status so callers see a terminal state.
    if status == "running" {
        if let (Some(started), Some(max_rt)) = (started_at, max_runtime_secs) {
            if finished_at.is_none() {
                let now = chrono::Utc::now().timestamp();
                if now.saturating_sub(started) > max_rt {
                    status = "lost".to_string();
                }
            }
        }
    }
    let elapsed_secs = started_at.map(|started| {
        finished_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .saturating_sub(started) as u64
    });
    ToolResult::ok(json!({
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
    }))
}

fn local_job_log(
    job_id: &str,
    record: &LocalJobRecord,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> ToolResult {
    let stdout = read_lines_from(record.dir.join("stdout.log"), offset, tail_lines);
    let stderr = read_lines_from(record.dir.join("stderr.log"), offset, tail_lines);
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    ToolResult::ok(json!({
        "job_id": job_id,
        "status": status,
        "stdout": stdout.0,
        "stderr": stderr.0,
        "next_stdout_line": stdout.1,
        "next_stderr_line": stderr.1,
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
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "expected tool '{}' in specs: {:?}",
                expected,
                names
            );
        }
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
            approval_mode: "full-auto".to_string(),
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
        // Expected: 'codex' --approval-mode 'full-auto' 'fix tests'
        assert!(cmd.starts_with("'codex' --approval-mode 'full-auto' "));
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
    fn build_codex_command_request_approval_mode_overrides_config() {
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "hi", Some("suggest"), None).unwrap();
        assert!(cmd.contains("--approval-mode 'suggest'"));
        assert!(!cmd.contains("full-auto"));
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
    async fn run_codex_output_contains_structured_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Initialize a git repo so run_job's wrapper (git apply etc.) is not
        // needed — run_job only spawns a shell wrapper, no git required.
        std::fs::create_dir_all(root).unwrap();
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
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["job_id"].is_string());
        assert_eq!(result.output["kind"], "codex");
        assert_eq!(result.output["project"], "demo");
        assert_eq!(result.output["status_endpoint"], "/api/jobs/status");
        assert_eq!(result.output["log_endpoint"], "/api/jobs/log");
    }

    #[tokio::test]
    async fn run_codex_metadata_kind_is_codex() {
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
        assert!(result.success, "{:?}", result.error);
        let job_id = result.output["job_id"].as_str().unwrap().to_string();
        // The job dir metadata.json should record kind = codex.
        let record = runtime.local_jobs.lock().await.get(&job_id).cloned();
        assert!(record.is_some(), "job should be cached in local_jobs");
        let meta = read_json(record.unwrap().dir.join("metadata.json"));
        assert_eq!(meta["kind"], "codex");
        assert_eq!(meta["project"], "demo");
    }

    #[tokio::test]
    async fn run_codex_uses_default_timeout_from_config() {
        // When timeout_secs is None, run_codex uses codex.default_timeout_secs.
        // We verify by checking metadata.json max_runtime_secs matches config.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let codex = CodexConfig {
            default_timeout_secs: 42,
            ..CodexConfig::default()
        };
        let runtime = runtime_with_codex(root, codex);
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "echo hi".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        let job_id = result.output["job_id"].as_str().unwrap().to_string();
        let record = runtime
            .local_jobs
            .lock()
            .await
            .get(&job_id)
            .cloned()
            .unwrap();
        let meta = read_json(record.dir.join("metadata.json"));
        assert_eq!(meta["max_runtime_secs"], 42);
    }

    // =========================================================================
    // Phase 6: agent capability checks, owner boundary, structured errors
    // =========================================================================

    use crate::shell_protocol::{ShellClientCapabilities, ShellClientRegisterRequest};

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
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
            })
            .await
            .unwrap();
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
                    project: "agent-proj".to_string(),
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
    async fn local_project_unaffected_by_agent_auth_checks() {
        // Local-executor projects must still work when no auth context is
        // supplied (e.g. internal callers using dispatch_with_auth(None)).
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
        assert!(result.success, "{:?}", result.error);
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
        let clients = agents["clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["client_id"], "agent-1");
        assert_eq!(clients[0]["agent_protocol_version"], "polling-v1");
        assert_eq!(clients[0]["connected"], true);
        assert!(clients[0]["capabilities"].is_object());
        assert_eq!(clients[0]["projects_count"], 0);
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
}
