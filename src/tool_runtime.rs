//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

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
    },

    /// List all configured projects.
    ListProjects,

    /// List connected shell/agent clients.
    ListAgents,
}

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("tool".to_string(), Value::String(name.to_string()));
        let unit_tool = matches!(name, "list_tools" | "list_projects" | "list_agents");
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

// =============================================================================
// Runtime
// =============================================================================

#[derive(Clone)]
pub struct ToolRuntime {
    pub projects: Arc<ProjectsState>,
    pub shell_clients: Arc<ShellClientRegistry>,
    local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
}

impl ToolRuntime {
    pub fn new(projects: Arc<ProjectsState>, shell_clients: Arc<ShellClientRegistry>) -> Self {
        Self {
            projects,
            shell_clients,
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

    /// Main dispatch — call from MCP handler or GPT Actions handler.
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::ListTools => ToolResult::ok(json!({ "tools": self.tool_specs() })),

            ToolCall::ListProjects => self.list_projects(),

            ToolCall::ListAgents => self.list_agents().await,

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

            ToolCall::JobLog { job_id, offset } => self.job_log(job_id, offset).await,
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
                    ("offset", "integer", "Optional stdout line cursor.", false),
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
        if prompt.len() > 100_000 {
            return ToolResult::err("prompt is too large; maximum is 100000 bytes");
        }
        let command = match build_codex_command(&prompt, approval_mode.as_deref(), extra_args) {
            Ok(command) => command,
            Err(e) => return ToolResult::err(e),
        };
        let result = self
            .run_job(project, command, timeout_secs.or(Some(3600)), cwd)
            .await;
        if !result.success {
            return result;
        }
        let mut output = result.output;
        if let Some(obj) = output.as_object_mut() {
            obj.insert("kind".to_string(), Value::String("codex".to_string()));
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

    async fn job_log(&self, job_id: String, offset: Option<usize>) -> ToolResult {
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            return local_job_log(&job_id, &record, offset);
        }
        match self
            .shell_clients
            .job_log(&job_id, offset, None, Some(500))
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
    prompt: &str,
    approval_mode: Option<&str>,
    extra_args: Option<Vec<String>>,
) -> Result<String, String> {
    let codex_bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
    validate_cli_arg(&codex_bin, "CODEX_BIN")?;
    let approval_mode = approval_mode
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("CODEX_APPROVAL_MODE").ok())
        .unwrap_or_else(|| "full-auto".to_string());
    validate_cli_arg(&approval_mode, "approval_mode")?;
    let extra_args = extra_args.unwrap_or_default();
    if extra_args.len() > 32 {
        return Err("extra_args may contain at most 32 arguments".to_string());
    }
    let mut parts = vec![
        shell_escape_simple(&codex_bin),
        "--approval-mode".to_string(),
        shell_escape_simple(&approval_mode),
    ];
    for (idx, arg) in extra_args.iter().enumerate() {
        validate_cli_arg(arg, &format!("extra_args[{}]", idx))?;
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
    let status = read_trim(record.dir.join("status")).unwrap_or_else(|| "running".to_string());
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let finished_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
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
        "executor": "local",
        "kind": meta.get("kind").cloned().unwrap_or_else(|| Value::String("shell".to_string())),
    }))
}

fn local_job_log(job_id: &str, record: &LocalJobRecord, offset: Option<usize>) -> ToolResult {
    let stdout = read_lines_from(record.dir.join("stdout.log"), offset, Some(500));
    let stderr = read_lines_from(record.dir.join("stderr.log"), None, Some(500));
    let status = read_trim(record.dir.join("status")).unwrap_or_else(|| "running".to_string());
    ToolResult::ok(json!({
        "job_id": job_id,
        "status": status,
        "stdout": stdout.0,
        "stderr": stderr.0,
        "next_stdout_line": stdout.1,
        "next_stderr_line": stderr.1,
    }))
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

fn read_lines_from(
    path: PathBuf,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> (String, usize) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let lines = content.lines().collect::<Vec<_>>();
    let start = offset.unwrap_or_else(|| {
        tail_lines
            .map(|tail| lines.len().saturating_sub(tail))
            .unwrap_or(0)
    });
    let selected = lines
        .iter()
        .skip(start)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let next = lines.len() + 1;
    (selected, next)
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
