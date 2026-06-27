use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

use shell_protocol::{
    AgentEnvelope, ShellAgentJobUpdateRequest, ShellAgentJobUpdateResponse, ShellAgentPollRequest,
    ShellAgentPollResponse, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentResultResponse, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellClientRegisterResponse, AGENT_PROTOCOL_VERSION_POLLING_V1,
    AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};

const DEFAULT_CONFIG_PATH: &str = "/etc/webcodex/agent.toml";
const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const DEFAULT_MAX_TIMEOUT_SECS: u64 = 3600;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const PROJECT_SCAN_CACHE_MS: u64 = 5000;
const DEFAULT_MAX_CONCURRENT_JOBS: usize = 2;
/// Config value selecting the polling transport (HTTP `/api/shell/agent/poll`).
const TRANSPORT_POLLING: &str = "polling";
/// Config value selecting the WebSocket transport (preferred long-lived).
const TRANSPORT_WEBSOCKET: &str = "websocket";
/// WebSocket outgoing envelope channel capacity.
const WS_OUTGOING_CAPACITY: usize = 64;
/// WebSocket ping interval.
const WS_PING_INTERVAL: Duration = Duration::from_secs(30);
/// Reconnect backoff after a WebSocket session ends.
const WS_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Bounded wait for the writer task to flush its last frame and close the
/// sink during shutdown. A split WebSocket sink's `close()` waits for the
/// peer's close acknowledgement, which is delivered through the read half;
/// once the read loop has broken the read half is no longer polled, so
/// `close()` can hang indefinitely on a half-closed socket. Bounding it
/// guarantees `websocket_session` (and therefore the reconnect loop) always
/// makes progress instead of stalling forever after a disconnect.
const WS_WRITER_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Deserialize)]
struct AgentConfig {
    server_url: String,
    token: String,
    client_id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    projects_dir: Option<PathBuf>,
    #[serde(default = "default_poll_interval_ms")]
    poll_interval_ms: u64,
    #[serde(default)]
    capabilities: Option<ShellClientCapabilities>,
    #[serde(default)]
    max_concurrent_jobs: Option<usize>,
    #[serde(default)]
    policy: AgentPolicy,
    /// Transport selection: `"polling"` (default, HTTP fallback) or
    /// `"websocket"` (preferred long-lived connection).
    #[serde(default)]
    transport: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentPolicy {
    #[serde(default = "default_true")]
    allow_raw_shell: bool,
    #[serde(default = "default_true")]
    allow_cwd_anywhere: bool,
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
    #[serde(default = "default_max_timeout_secs")]
    max_timeout_secs: u64,
    #[serde(default = "default_max_output_bytes")]
    max_output_bytes: usize,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            allow_raw_shell: true,
            allow_cwd_anywhere: true,
            allowed_roots: Vec::new(),
            max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_poll_interval_ms() -> u64 {
    DEFAULT_POLL_INTERVAL_MS
}

fn default_max_timeout_secs() -> u64 {
    DEFAULT_MAX_TIMEOUT_SECS
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

fn max_concurrent_jobs(cfg: &AgentConfig) -> usize {
    cfg.max_concurrent_jobs
        .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS)
        .max(1)
}

#[derive(Debug)]
struct CommandResult {
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: Option<u64>,
    error: Option<String>,
}

/// Minimal HTTP send configuration used by the polling `AgentSink`. We do not
/// store the whole `AgentConfig` here: policy and concurrency limits stay
/// with the agent config and are passed alongside the sink.
#[derive(Debug, Clone)]
struct HttpSendConfig {
    client: Client,
    server_url: String,
    token: String,
    client_id: String,
}

/// Transport-neutral outgoing channel for an agent. Both the polling loop and
/// the WebSocket loop build an `AgentSink` and hand it to the shared
/// `dispatch_request` / `JobManager` execution path. This is the single seam
/// that lets the agent speak either transport without duplicating execution
/// logic.
#[derive(Debug, Clone)]
enum AgentSink {
    /// Polling transport: POST results/job_updates to the HTTP endpoints.
    Http(HttpSendConfig),
    /// WebSocket transport: push envelopes through an mpsc that a writer task
    /// drains onto the socket.
    WebSocket {
        tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
        client_id: String,
    },
}

impl AgentSink {
    fn client_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.client_id,
            AgentSink::WebSocket { client_id, .. } => client_id,
        }
    }

    /// Submit the result of a synchronous shell/file request. Mirrors the old
    /// `submit_result` free function but routes over the active transport.
    fn submit_result(&self, request_id: String, result: CommandResult) -> Result<bool, String> {
        let body = ShellAgentResultRequest {
            client_id: self.client_id().to_string(),
            request_id,
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: result.duration_ms,
            error: result.error,
        };
        match self {
            AgentSink::Http(h) => {
                let resp: ShellAgentResultResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/result",
                    &body,
                )?;
                if resp.success {
                    Ok(true)
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "result submission failed without error".to_string()))
                }
            }
            AgentSink::WebSocket { tx, .. } => {
                let env = AgentEnvelope::Result { payload: body };
                tx.blocking_send(env)
                    .map_err(|_| "websocket send failed".to_string())?;
                Ok(true)
            }
        }
    }

    /// Push an incremental/final job update. Mirrors the old `send_job_update`
    /// free function.
    fn send_job_update(&self, body: &ShellAgentJobUpdateRequest) -> Result<(), String> {
        match self {
            AgentSink::Http(h) => {
                let resp: ShellAgentJobUpdateResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/job_update",
                    body,
                )?;
                if resp.success {
                    Ok(())
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "job_update failed without error".to_string()))
                }
            }
            AgentSink::WebSocket { tx, .. } => {
                let env = AgentEnvelope::JobUpdate {
                    payload: body.clone(),
                };
                tx.blocking_send(env)
                    .map_err(|_| "websocket send failed".to_string())
            }
        }
    }
}

/// Send a JSON POST to the server and decode the response. Same wire behavior
/// as `post_json` but takes the raw connection bits so it can be used from
/// `AgentSink::Http` without an `AgentConfig`.
fn post_json_raw<T, R>(
    client: &Client,
    server_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<R, String>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let resp = client
        .post(url)
        .bearer_auth(token)
        .json(body)
        .send()
        .map_err(|e| format!("request {} failed: {}", path, e))?;
    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("failed to read response {}: {}", path, e))?;
    if !status.is_success() {
        return Err(format!("{} returned {}: {}", path, status, text));
    }
    serde_json::from_str(&text).map_err(|e| format!("failed to parse response {}: {}", path, e))
}

#[derive(Debug, Clone)]
struct JobManager {
    max_concurrent: usize,
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    queued: Arc<Mutex<VecDeque<(AgentSink, AgentPolicy, ShellAgentShellRequest)>>>,
}

impl JobManager {
    fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            queued: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

#[derive(Debug, Clone)]
struct RunningJob {
    client_id: String,
    child: Option<Arc<Mutex<Child>>>,
    stop_requested: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProjectFile {
    id: String,
    path: String,
    #[serde(default = "default_true")]
    allow_patch: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    hooks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
struct AgentProjectCache {
    projects: Vec<ShellAgentProjectSummary>,
    refreshed_at: Option<Instant>,
}

#[derive(Debug)]
enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

fn usage() -> &'static str {
    "Usage: webcodex-agent [--config PATH] [--once]\n\n\
     Environment:\n\
       WEBCODEX_AGENT_CONFIG      default config path override\n\
     Example agent.toml:\n\
       server_url = \"https://v4.yyjeqhc.cn\"\n\
       token = \"...\"\n\
       client_id = \"xrh\"\n\
       display_name = \"XRH\"\n\
       owner = \"yyjeqhc\"\n\
       projects_dir = \"/root/.config/webcodex/projects.d\"\n\
       poll_interval_ms = 1000\n\
\n\
       [policy]\n\
       allow_raw_shell = true\n\
       allow_cwd_anywhere = true\n\
       max_timeout_secs = 3600\n\
       max_output_bytes = 262144\n"
}

fn parse_args() -> Result<(PathBuf, bool), String> {
    let mut config_path = std::env::var("WEBCODEX_AGENT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_config_path());
    let mut once = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--once" => once = true,
            "--config" | "-c" => {
                let Some(path) = args.next() else {
                    return Err("--config requires a path".to_string());
                };
                config_path = PathBuf::from(path);
            }
            _ => return Err(format!("unknown argument: {}\n{}", arg, usage())),
        }
    }
    Ok((config_path, once))
}

fn default_config_path() -> PathBuf {
    let home_path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/webcodex/agent.toml"));
    let system_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    for path in [home_path.clone(), Some(system_path.clone())]
        .into_iter()
        .flatten()
    {
        if path.exists() {
            return path;
        }
    }
    home_path
        .or_else(|| Some(system_path))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn load_config(path: &Path) -> Result<AgentConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read config {}: {}", path.display(), e))?;
    let cfg: AgentConfig = toml::from_str(&content)
        .map_err(|e| format!("failed to parse config {}: {}", path.display(), e))?;
    if cfg.server_url.trim().is_empty() {
        return Err("server_url cannot be empty".to_string());
    }
    if cfg.token.trim().is_empty() {
        return Err("token cannot be empty".to_string());
    }
    if cfg.client_id.trim().is_empty() {
        return Err("client_id cannot be empty".to_string());
    }
    if cfg.poll_interval_ms == 0 {
        return Err("poll_interval_ms must be > 0".to_string());
    }
    if !cfg.policy.allow_cwd_anywhere && cfg.policy.allowed_roots.is_empty() {
        return Err("policy.allowed_roots must be set when allow_cwd_anywhere=false".to_string());
    }
    Ok(cfg)
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn default_projects_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/webcodex/projects.d")
}

fn projects_dir(cfg: &AgentConfig) -> PathBuf {
    cfg.projects_dir
        .clone()
        .unwrap_or_else(default_projects_dir)
}

fn validate_project_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id == "." || id == ".." {
        return Err("id cannot be '.' or '..'".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("id may only contain ASCII letters, digits, '-', '_', and '.'".to_string());
    }
    Ok(())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_agent_project_toml(content: &str) -> Result<AgentProjectFile, String> {
    let mut project: AgentProjectFile =
        toml::from_str(content).map_err(|e| format!("failed to parse project toml: {}", e))?;
    project.id = project.id.trim().to_string();
    validate_project_id(&project.id)?;
    project.path = project.path.trim().to_string();
    if project.path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    project.name = trim_optional(project.name);
    project.kind = trim_optional(project.kind);
    project.description = trim_optional(project.description);
    let mut hooks = HashMap::new();
    for (name, commands) in project.hooks {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("hook name cannot be empty".to_string());
        }
        hooks.insert(name, commands);
    }
    project.hooks = hooks;
    Ok(project)
}

fn run_git_capture(path: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn agent_project_summary(
    project: &AgentProjectFile,
    updated_at: i64,
    include_git: bool,
) -> ShellAgentProjectSummary {
    let mut hooks = project.hooks.keys().cloned().collect::<Vec<_>>();
    hooks.sort();
    let (git_branch, git_head, git_dirty) = if include_git {
        let branch = run_git_capture(&project.path, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let head = run_git_capture(&project.path, &["log", "-1", "--pretty=format:%h"]);
        let dirty = run_git_capture(&project.path, &["status", "--short"])
            .map(|status| !status.trim().is_empty());
        (branch, head, dirty)
    } else {
        (None, None, None)
    };
    ShellAgentProjectSummary {
        id: project.id.clone(),
        name: project.name.clone().or_else(|| Some(project.id.clone())),
        path: project.path.clone(),
        allow_patch: project.allow_patch,
        kind: project.kind.clone(),
        description: project.description.clone(),
        hooks,
        disabled: project.disabled,
        git_branch,
        git_head,
        git_dirty,
        updated_at,
    }
}

fn warn_empty_hook_commands(source: &Path, project: &AgentProjectFile) {
    for (hook, commands) in &project.hooks {
        for (idx, command) in commands.iter().enumerate() {
            if command.trim().is_empty() {
                eprintln!(
                    "webcodex-agent project warning: {} hook {} command {} is empty",
                    source.display(),
                    hook,
                    idx
                );
            }
        }
    }
}

fn load_agent_project_summaries_from_dir(dir: &Path) -> Vec<ShellAgentProjectSummary> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!(
                "webcodex-agent project warning: failed to read {}: {}",
                dir.display(),
                e
            );
            return Vec::new();
        }
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();

    let updated_at = chrono::Utc::now().timestamp();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "webcodex-agent project warning: failed to read {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        let project = match parse_agent_project_toml(&content) {
            Ok(project) => project,
            Err(e) => {
                eprintln!(
                    "webcodex-agent project warning: skipping {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        if project.disabled {
            continue;
        }
        if !seen.insert(project.id.clone()) {
            eprintln!(
                "webcodex-agent project warning: duplicate project id {} in {}; skipping",
                project.id,
                file.display()
            );
            continue;
        }
        warn_empty_hook_commands(&file, &project);
        projects.push(agent_project_summary(&project, updated_at, true));
    }
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    projects
}

fn load_agent_project_summaries(cfg: &AgentConfig) -> Vec<ShellAgentProjectSummary> {
    load_agent_project_summaries_from_dir(&projects_dir(cfg))
}

impl AgentProjectCache {
    fn get(&mut self, cfg: &AgentConfig) -> Vec<ShellAgentProjectSummary> {
        if self.refreshed_at.is_some_and(|refreshed_at| {
            refreshed_at.elapsed() < Duration::from_millis(PROJECT_SCAN_CACHE_MS)
        }) {
            return self.projects.clone();
        }
        self.projects = load_agent_project_summaries(cfg);
        self.refreshed_at = Some(Instant::now());
        self.projects.clone()
    }
}

fn endpoint(cfg: &AgentConfig, path: &str) -> String {
    format!("{}{}", cfg.server_url.trim_end_matches('/'), path)
}

fn post_json<T, R>(client: &Client, cfg: &AgentConfig, path: &str, body: &T) -> Result<R, String>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let resp = client
        .post(endpoint(cfg, path))
        .bearer_auth(&cfg.token)
        .json(body)
        .send()
        .map_err(|e| format!("request {} failed: {}", path, e))?;
    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("failed to read response {}: {}", path, e))?;
    if !status.is_success() {
        return Err(format!("{} returned {}: {}", path, status, text));
    }
    serde_json::from_str(&text).map_err(|e| format!("failed to parse response {}: {}", path, e))
}

fn agent_register_capabilities(cfg: &AgentConfig) -> ShellClientCapabilities {
    let mut capabilities = cfg.capabilities.clone().unwrap_or_default();
    capabilities.jobs = true;
    capabilities.file_read = true;
    capabilities.file_write = true;
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    capabilities
}

fn build_register_request(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    protocol_version: &str,
) -> ShellClientRegisterRequest {
    let capabilities = agent_register_capabilities(cfg);
    ShellClientRegisterRequest {
        client_id: cfg.client_id.clone(),
        display_name: cfg.display_name.clone(),
        owner: cfg.owner.clone(),
        hostname: cfg.hostname.clone().or_else(hostname),
        capabilities: Some(capabilities),
        projects: Some(projects),
        agent_protocol_version: Some(protocol_version.to_string()),
    }
}

fn register(
    client: &Client,
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
) -> Result<(), String> {
    let body = build_register_request(
        cfg,
        project_cache.get(cfg),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
    );
    let response: ShellClientRegisterResponse =
        post_json(client, cfg, "/api/shell/agent/register", &body)?;
    if response.success {
        Ok(())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "register failed without error".to_string()))
    }
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("failed to access {}: {}", path.display(), e))
}

fn cwd_allowed(policy: &AgentPolicy, cwd: &Path) -> Result<(), String> {
    if policy.allow_cwd_anywhere {
        return Ok(());
    }
    let cwd = canonicalize_existing(cwd)?;
    for root in &policy.allowed_roots {
        let root = canonicalize_existing(root)?;
        if cwd == root || cwd.starts_with(&root) {
            return Ok(());
        }
    }
    Err(format!(
        "cwd {} is outside allowed_roots",
        cwd.to_string_lossy()
    ))
}

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn truncate_bytes(bytes: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!(
        "[output truncated to last {} bytes]\n{}",
        max,
        &text[start..]
    )
}

fn read_pipes(
    mut child: std::process::Child,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout pipe missing".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr pipe missing".to_string())?;
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stdout.read_to_end(&mut buf).map(|_| buf);
        result.map_err(|e| format!("failed to read stdout: {}", e))
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stderr.read_to_end(&mut buf).map(|_| buf);
        result.map_err(|e| format!("failed to read stderr: {}", e))
    });
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait command: {}", e))?;
    let stdout = stdout_handle
        .join()
        .map_err(|_| "stdout reader panicked".to_string())??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| "stderr reader panicked".to_string())??;
    Ok((status, stdout, stderr))
}

fn run_shell(
    policy: &AgentPolicy,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    if !policy.allow_raw_shell {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("raw shell is disabled by local agent policy".to_string()),
        };
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(e) = cwd_allowed(policy, &cwd_path) {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(e),
        };
    }
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let start = Instant::now();
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(&cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let spawn = cmd.spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to spawn command: {}", e)),
            };
        }
    };
    if let Some(input) = stdin {
        match child.stdin.take() {
            Some(mut child_stdin) => {
                if let Err(e) = child_stdin.write_all(input.as_bytes()) {
                    let _ = child.kill();
                    return CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(format!("failed to write command stdin: {}", e)),
                    };
                }
            }
            None => {
                let _ = child.kill();
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some("stdin pipe missing".to_string()),
                };
            }
        }
    }
    loop {
        if stop_requested
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            let _ = child.kill();
            let duration_ms = start.elapsed().as_millis() as u64;
            return match read_pipes(child) {
                Ok((_status, stdout, stderr)) => CommandResult {
                    exit_code: Some(-1),
                    stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
                    stderr: Some(format!(
                        "{}{}job stopped by request",
                        truncate_bytes(&stderr, policy.max_output_bytes),
                        if stderr.is_empty() { "" } else { "\n" },
                    )),
                    duration_ms: Some(duration_ms),
                    error: Some("job stopped".to_string()),
                },
                Err(e) => CommandResult {
                    exit_code: Some(-1),
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(duration_ms),
                    error: Some(format!("job stopped; failed to collect output: {}", e)),
                },
            };
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return match read_pipes(child) {
                        Ok((_status, stdout, stderr)) => CommandResult {
                            exit_code: Some(-1),
                            stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
                            stderr: Some(format!(
                                "{}{}command timed out after {} seconds",
                                truncate_bytes(&stderr, policy.max_output_bytes),
                                if stderr.is_empty() { "" } else { "\n" },
                                timeout_secs
                            )),
                            duration_ms: Some(duration_ms),
                            error: Some("command timed out".to_string()),
                        },
                        Err(e) => CommandResult {
                            exit_code: Some(-1),
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(duration_ms),
                            error: Some(format!(
                                "command timed out; failed to collect output: {}",
                                e
                            )),
                        },
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!("failed to wait command: {}", e)),
                };
            }
        }
    }
    match read_pipes(child) {
        Ok((status, stdout, stderr)) => CommandResult {
            exit_code: Some(status.code().unwrap_or(-1)),
            stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
            stderr: Some(truncate_bytes(&stderr, policy.max_output_bytes)),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(e) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(e),
        },
    }
}

fn resolve_requested_path(
    policy: &AgentPolicy,
    cwd: Option<&str>,
    path: &str,
) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    let raw_path = PathBuf::from(path);
    let resolved = if raw_path.is_absolute() {
        raw_path
    } else {
        let base = cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        base.join(raw_path)
    };
    let mut parent_for_policy = if resolved.exists() {
        resolved.clone()
    } else {
        resolved
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| resolved.clone())
    };
    while !parent_for_policy.exists() {
        let Some(parent) = parent_for_policy.parent() else {
            break;
        };
        parent_for_policy = parent.to_path_buf();
    }
    cwd_allowed(policy, &parent_for_policy)?;
    Ok(resolved)
}

fn handle_file_request(policy: &AgentPolicy, request: &ShellAgentShellRequest) -> CommandResult {
    let Some(path) = request.path.as_deref() else {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("file request missing path".to_string()),
        };
    };
    let start = Instant::now();
    let resolved = match resolve_requested_path(policy, request.cwd.as_deref(), path) {
        Ok(path) => path,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            }
        }
    };
    match request.kind.as_str() {
        "file_read" => {
            let max = request
                .max_bytes
                .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES)
                .min(policy.max_output_bytes);
            match std::fs::read(&resolved) {
                Ok(bytes) => {
                    if bytes.len() > max {
                        CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "file too large: {} bytes exceeds max_bytes {}",
                                bytes.len(),
                                max
                            )),
                        }
                    } else {
                        CommandResult {
                            exit_code: Some(0),
                            stdout: Some(String::from_utf8_lossy(&bytes).to_string()),
                            stderr: Some(String::new()),
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: None,
                        }
                    }
                }
                Err(e) => CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!("failed to read {}: {}", resolved.display(), e)),
                },
            }
        }
        "file_write" => {
            let content = request.content.clone().unwrap_or_default();
            if content.len() > policy.max_output_bytes {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "content too large: {} bytes exceeds max_output_bytes {}",
                        content.len(),
                        policy.max_output_bytes
                    )),
                };
            }
            if let Some(expected) = request.expected_sha256.as_deref() {
                match std::fs::read(&resolved) {
                    Ok(existing) => {
                        let actual = sha256_hex_bytes(&existing);
                        if !actual.eq_ignore_ascii_case(expected) {
                            return CommandResult {
                                exit_code: None,
                                stdout: None,
                                stderr: None,
                                duration_ms: Some(start.elapsed().as_millis() as u64),
                                error: Some(format!(
                                    "expected_sha256 mismatch: expected {}, actual {}",
                                    expected, actual
                                )),
                            };
                        }
                    }
                    Err(e) => {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to read existing file for expected_sha256 {}: {}",
                                resolved.display(),
                                e
                            )),
                        };
                    }
                }
            }
            if request.create_dirs {
                if let Some(parent) = resolved.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to create parent directory {}: {}",
                                parent.display(),
                                e
                            )),
                        };
                    }
                }
            }
            match std::fs::write(&resolved, content.as_bytes()) {
                Ok(()) => CommandResult {
                    exit_code: Some(0),
                    stdout: Some(content.len().to_string()),
                    stderr: Some(String::new()),
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: None,
                },
                Err(e) => CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!("failed to write {}: {}", resolved.display(), e)),
                },
            }
        }
        "file_list" => match std::fs::read_dir(&resolved) {
            Ok(entries) => {
                let mut names = Vec::new();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let suffix = entry
                        .file_type()
                        .ok()
                        .filter(|t| t.is_dir())
                        .map(|_| "/")
                        .unwrap_or("");
                    names.push(format!("{}{}", name, suffix));
                }
                names.sort();
                CommandResult {
                    exit_code: Some(0),
                    stdout: Some(format!("{}\n", names.join("\n"))),
                    stderr: Some(String::new()),
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: None,
                }
            }
            Err(e) => CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to list {}: {}", resolved.display(), e)),
            },
        },
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("unknown file request kind: {}", request.kind)),
        },
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<OutputChunk>,
    stdout: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let _ = if stdout {
                        tx.send(OutputChunk::Stdout(text))
                    } else {
                        tx.send(OutputChunk::Stderr(text))
                    };
                }
                Err(_) => break,
            }
        }
    })
}

/// Report a job-start failure over the active transport. Used by
/// `JobManager::start_shell_job` when spawn/cwd/policy checks fail before the
/// job can run.
fn send_start_failure(sink: &AgentSink, request: ShellAgentShellRequest, error: String) {
    if let Some(job_id) = request.job_id {
        let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
            client_id: sink.client_id().to_string(),
            job_id,
            request_id: Some(request.request_id),
            status: "failed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: None,
            duration_ms: Some(0),
            error: Some(error),
            finished: true,
        });
    }
}

fn kill_child_group(child: &Arc<Mutex<Child>>) -> Result<(), String> {
    let pid = child
        .lock()
        .map_err(|_| "job child lock poisoned".to_string())?
        .id();
    let _ = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(format!("-{}", pid))
        .status();
    std::thread::sleep(Duration::from_millis(50));
    let _ = child
        .lock()
        .map_err(|_| "job child lock poisoned".to_string())?
        .kill();
    Ok(())
}

impl JobManager {
    fn has_work(&self) -> bool {
        !self.jobs.lock().unwrap().is_empty() || !self.queued.lock().unwrap().is_empty()
    }

    fn active_job_count(&self, client_id: &str) -> usize {
        self.jobs
            .lock()
            .unwrap()
            .values()
            .filter(|job| job.client_id == client_id)
            .count()
    }

    fn enqueue(&self, sink: AgentSink, policy: AgentPolicy, request: ShellAgentShellRequest) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        let client_id = sink.client_id().to_string();
        let active = self.active_job_count(&client_id);
        if active >= self.max_concurrent {
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: client_id.clone(),
                job_id: job_id.clone(),
                request_id: Some(request.request_id.clone()),
                status: "agent_queued".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                finished: false,
            });
            self.queued
                .lock()
                .unwrap()
                .push_back((sink, policy, request));
            return;
        }
        self.start_now(sink, policy, request);
    }

    fn start_now(&self, sink: AgentSink, policy: AgentPolicy, request: ShellAgentShellRequest) {
        self.start_shell_job(sink, policy, request);
    }

    fn start_available_queued(&self) {
        loop {
            let next = {
                let jobs = self.jobs.lock().unwrap();
                let mut queued = self.queued.lock().unwrap();
                let mut selected = None;
                for (idx, (_, _policy, request)) in queued.iter().enumerate() {
                    let active = jobs
                        .values()
                        .filter(|job| job.client_id == request.client_id)
                        .count();
                    if active < self.max_concurrent {
                        selected = Some(idx);
                        break;
                    }
                }
                selected.and_then(|idx| queued.remove(idx))
            };
            let Some((sink, policy, request)) = next else {
                return;
            };
            self.start_now(sink, policy, request);
        }
    }

    fn start_shell_job(
        &self,
        sink: AgentSink,
        policy: AgentPolicy,
        request: ShellAgentShellRequest,
    ) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        if !policy.allow_raw_shell {
            send_start_failure(
                &sink,
                request,
                "raw shell is disabled by local agent policy".to_string(),
            );
            return;
        }
        let cwd_path = request
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        if let Err(e) = cwd_allowed(&policy, &cwd_path) {
            send_start_failure(&sink, request, e);
            return;
        }
        let start = Instant::now();
        let spawn = std::process::Command::new("setsid")
            .arg("sh")
            .arg("-c")
            .arg(&request.command)
            .current_dir(&cwd_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawn {
            Ok(c) => c,
            Err(e) => {
                send_start_failure(&sink, request, format!("failed to spawn command: {}", e));
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));
        let stop_requested = Arc::new(AtomicBool::new(false));
        let client_id = sink.client_id().to_string();
        self.jobs.lock().unwrap().insert(
            job_id.clone(),
            RunningJob {
                client_id: client_id.clone(),
                child: Some(child.clone()),
                stop_requested: stop_requested.clone(),
            },
        );
        let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
            client_id: client_id.clone(),
            job_id: job_id.clone(),
            request_id: Some(request.request_id.clone()),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            finished: false,
        });
        let jobs = self.jobs.clone();
        let queued = self.queued.clone();
        let max_concurrent = self.max_concurrent;
        std::thread::spawn(move || {
            let (tx, rx) = mpsc::channel::<OutputChunk>();
            let mut readers = Vec::new();
            if let Some(stdout) = stdout {
                readers.push(spawn_reader(stdout, tx.clone(), true));
            }
            if let Some(stderr) = stderr {
                readers.push(spawn_reader(stderr, tx.clone(), false));
            }
            drop(tx);
            let timeout_secs = request.timeout_secs.min(policy.max_timeout_secs).max(1);
            let final_status;
            loop {
                let mut out = String::new();
                let mut err = String::new();
                while let Ok(chunk) = rx.try_recv() {
                    match chunk {
                        OutputChunk::Stdout(t) => out.push_str(&t),
                        OutputChunk::Stderr(t) => err.push_str(&t),
                    }
                }
                if !out.is_empty() || !err.is_empty() {
                    let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                        client_id: sink.client_id().to_string(),
                        job_id: job_id.clone(),
                        request_id: Some(request.request_id.clone()),
                        status: "running".to_string(),
                        stdout_chunk: (!out.is_empty()).then_some(out),
                        stderr_chunk: (!err.is_empty()).then_some(err),
                        stdout_tail: None,
                        stderr_tail: None,
                        exit_code: None,
                        duration_ms: None,
                        error: None,
                        finished: false,
                    });
                }
                match child.lock().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        let stopped = stop_requested.load(Ordering::SeqCst);
                        final_status = (
                            if stopped {
                                "stopped"
                            } else if status.success() {
                                "completed"
                            } else {
                                "failed"
                            }
                            .to_string(),
                            Some(status.code().unwrap_or(-1)),
                            if stopped {
                                Some("job stopped by request".to_string())
                            } else {
                                None
                            },
                        );
                        break;
                    }
                    Ok(None) => {
                        if start.elapsed() >= Duration::from_secs(timeout_secs) {
                            stop_requested.store(true, Ordering::SeqCst);
                            let _ = kill_child_group(&child);
                            final_status = (
                                "timeout".to_string(),
                                Some(-1),
                                Some(format!("job timed out after {} seconds", timeout_secs)),
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        final_status = (
                            "failed".to_string(),
                            None,
                            Some(format!("failed to wait job: {}", e)),
                        );
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
            }
            for reader in readers {
                let _ = reader.join();
            }
            let mut out = String::new();
            let mut err = String::new();
            while let Ok(chunk) = rx.try_recv() {
                match chunk {
                    OutputChunk::Stdout(t) => out.push_str(&t),
                    OutputChunk::Stderr(t) => err.push_str(&t),
                }
            }
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                job_id: job_id.clone(),
                request_id: Some(request.request_id),
                status: final_status.0,
                stdout_chunk: (!out.is_empty()).then_some(out),
                stderr_chunk: (!err.is_empty()).then_some(err),
                stdout_tail: None,
                stderr_tail: None,
                exit_code: final_status.1,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: final_status.2,
                finished: true,
            });
            jobs.lock().unwrap().remove(&job_id);
            let manager = JobManager {
                max_concurrent,
                jobs: jobs.clone(),
                queued: queued.clone(),
            };
            manager.start_available_queued();
        });
    }

    fn stop(&self, job_id: &str) -> Result<(), String> {
        let queued_job = {
            let mut queued = self.queued.lock().unwrap();
            if let Some(pos) = queued
                .iter()
                .position(|(_, _, request)| request.job_id.as_deref() == Some(job_id))
            {
                queued.remove(pos)
            } else {
                None
            }
        };
        if let Some((sink, _policy, request)) = queued_job {
            let request_id = request.request_id.clone();
            let job_id = request.job_id.clone().unwrap_or_default();
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                job_id,
                request_id: Some(request_id),
                status: "stopped".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: Some("job stopped before start".to_string()),
                exit_code: Some(-1),
                duration_ms: Some(0),
                error: Some("job stopped before start".to_string()),
                finished: true,
            });
            return Ok(());
        }
        let (child, stop_requested) = {
            let jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.get(job_id) else {
                return Err(format!("unknown local job: {}", job_id));
            };
            (job.child.clone(), job.stop_requested.clone())
        };
        stop_requested.store(true, Ordering::SeqCst);
        if let Some(child) = child {
            kill_child_group(&child).map_err(|e| format!("failed to kill job {}: {}", job_id, e))
        } else {
            Ok(())
        }
    }
}
/// Execute a single agent request (shell/file/job) and send the result over
/// the active transport. This is the shared dispatch path used by both the
/// polling loop (`handle_one_poll`) and the WebSocket loop. It contains no
/// transport-specific code: all outgoing traffic goes through `sink`.
fn dispatch_request(
    sink: &AgentSink,
    policy: &AgentPolicy,
    jobs: &JobManager,
    request: ShellAgentShellRequest,
) -> Result<bool, String> {
    match request.kind.as_str() {
        "start_job" => {
            jobs.enqueue(sink.clone(), policy.clone(), request);
            Ok(true)
        }
        "stop_job" => {
            if let Some(job_id) = request.job_id.as_deref() {
                if let Err(e) = jobs.stop(job_id) {
                    eprintln!("webcodex-agent stop_job error: {}", e);
                }
            }
            Ok(true)
        }
        "file_read" | "file_write" | "file_list" => {
            let request_id = request.request_id.clone();
            let result = handle_file_request(policy, &request);
            sink.submit_result(request_id, result)
        }
        _ => {
            let request_id = request.request_id.clone();
            let result = run_shell(
                policy,
                request.cwd.as_deref(),
                &request.command,
                request.stdin.as_deref(),
                request.timeout_secs,
                None,
            );
            sink.submit_result(request_id, result)
        }
    }
}

fn handle_one_poll(
    client: &Client,
    cfg: &AgentConfig,
    jobs: &JobManager,
    project_cache: &mut AgentProjectCache,
) -> Result<bool, String> {
    let poll = ShellAgentPollRequest {
        client_id: cfg.client_id.clone(),
        projects: Some(project_cache.get(cfg)),
    };
    let response: ShellAgentPollResponse = post_json(client, cfg, "/api/shell/agent/poll", &poll)?;
    if !response.success {
        return Err(response
            .error
            .unwrap_or_else(|| "poll failed without error".to_string()));
    }
    let Some(request) = response.request else {
        return Ok(false);
    };
    let sink = AgentSink::Http(HttpSendConfig {
        client: client.clone(),
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
    });
    dispatch_request(&sink, &cfg.policy, jobs, request)
}

fn run_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    let transport = cfg
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_POLLING)
        .to_string();
    match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_agent(cfg, once),
        _ => run_polling_agent(cfg, once),
    }
}

fn run_polling_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let mut project_cache = AgentProjectCache::default();
    register(&client, &cfg, &mut project_cache)?;
    eprintln!(
        "webcodex-agent registered client_id={} server={} transport=polling",
        cfg.client_id, cfg.server_url
    );
    loop {
        match handle_one_poll(&client, &cfg, &jobs, &mut project_cache) {
            Ok(ran_request) => {
                if once {
                    while jobs.has_work() {
                        std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                    }
                    return Ok(());
                }
                if !ran_request {
                    std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                }
            }
            Err(e) => {
                eprintln!("webcodex-agent poll error: {}", e);
                if once {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                let _ = register(&client, &cfg, &mut project_cache);
            }
        }
    }
}

// ============================================================================
// WebSocket agent transport
// ============================================================================
//
// The WebSocket mode keeps one long-lived connection to the server. The server
// pushes `Request` envelopes; the agent executes them via the same
// `dispatch_request` path the polling loop uses, and sends `Result` /
// `JobUpdate` envelopes back. Polling is unchanged and remains the fallback.

/// Convert an `http(s)://` server URL into a `ws(s)://` URL plus path.
fn server_url_to_ws(server_url: &str, path: &str) -> Result<String, String> {
    let base = server_url.trim_end_matches('/');
    let ws = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{}{}", rest, path)
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{}{}", rest, path)
    } else if base.starts_with("ws://") || base.starts_with("wss://") {
        format!("{}{}", base, path)
    } else {
        return Err(format!(
            "server_url must be http(s)://... for websocket transport; got {}",
            server_url
        ));
    };
    Ok(ws)
}

/// Build a WebSocket handshake request carrying the Bearer token in the
/// `Authorization` header, matching the server `AuthMiddleware`.
fn build_ws_request(
    ws_url: &str,
    token: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| format!("invalid websocket url: {}", e))?;
    let value = format!("Bearer {}", token);
    let header_value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value)
        .map_err(|e| format!("invalid token header value: {}", e))?;
    request.headers_mut().insert(
        tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
        header_value,
    );
    Ok(request)
}

/// Entry point for the WebSocket transport. Runs a tokio current-thread
/// runtime and reconnects on session failure.
fn run_websocket_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        loop {
            let projects = project_cache.get(&cfg);
            match websocket_session(&cfg, projects).await {
                Ok(()) => {
                    if once {
                        return Ok(());
                    }
                    eprintln!("webcodex-agent websocket session ended; reconnecting");
                    tokio::time::sleep(WS_RECONNECT_BACKOFF).await;
                }
                Err(e) => {
                    eprintln!("webcodex-agent websocket error: {}", e);
                    if once {
                        return Err(e);
                    }
                    tokio::time::sleep(WS_RECONNECT_BACKOFF).await;
                }
            }
        }
    })
}

/// One WebSocket connection lifecycle: connect, register, then serve requests
/// until the socket closes or a fatal server error arrives.
async fn websocket_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let ws_url = server_url_to_ws(&cfg.server_url, "/api/agents/ws")?;
    let request = build_ws_request(&ws_url, &cfg.token)?;
    let (mut ws_stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("websocket connect failed: {}", e))?;

    // Register over the socket.
    let register_payload =
        build_register_request(cfg, projects, AGENT_PROTOCOL_VERSION_WEBSOCKET_V1);
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
    };
    let reg_json =
        serde_json::to_string(&reg_env).map_err(|e| format!("failed to encode register: {}", e))?;
    ws_stream
        .send(WsMessage::Text(reg_json.into()))
        .await
        .map_err(|e| format!("failed to send register: {}", e))?;

    // Wait for Registered ack.
    let ack_msg = ws_stream
        .next()
        .await
        .ok_or_else(|| "server closed before register ack".to_string())?
        .map_err(|e| format!("failed to read register ack: {}", e))?;
    let ack_text = ack_msg
        .into_text()
        .map_err(|_| "register ack was not text".to_string())?;
    let ack = AgentEnvelope::from_slice(ack_text.as_bytes())
        .map_err(|e| format!("register ack is not a valid envelope: {}", e))?;
    match ack {
        AgentEnvelope::Registered { success: true, .. } => {}
        AgentEnvelope::Registered { error, .. } => {
            return Err(error.unwrap_or_else(|| "register rejected".to_string()));
        }
        AgentEnvelope::Error { message, .. } => return Err(message),
        other => return Err(format!("expected registered ack, got {}", other.kind())),
    }
    eprintln!(
        "webcodex-agent registered client_id={} server={} transport=websocket",
        cfg.client_id, cfg.server_url
    );

    // Split socket into writer (drains outgoing envelopes) and reader.
    let (mut sink, mut stream) = ws_stream.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            match serde_json::to_string(&env) {
                Ok(json) => {
                    if sink.send(WsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // The sink is dropped here. We intentionally do NOT call
        // `sink.close()`: on a split WebSocket sink the close handshake is
        // delivered through the read half, which is no longer polled once
        // the read loop breaks, so an unbounded `close().await` can hang and
        // block the reconnect loop. Dropping the sink lets the OS tear down
        // the socket; the server reconciles via its disconnect path either
        // way.
    });

    let sink_handle = AgentSink::WebSocket {
        tx: out_tx.clone(),
        client_id: cfg.client_id.clone(),
    };
    let jobs = JobManager::new(max_concurrent_jobs(cfg));
    let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
    ping_interval.tick().await; // skip immediate first tick

    loop {
        tokio::select! {
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("webcodex-agent websocket read error: {}", e);
                        break;
                    }
                    None => break,
                };
                if msg.is_close() {
                    break;
                }
                let text = match msg.into_text() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let env = match AgentEnvelope::from_slice(text.as_bytes()) {
                    Ok(env) => env,
                    Err(e) => {
                        eprintln!("webcodex-agent websocket malformed envelope: {}", e);
                        continue;
                    }
                };
                match env {
                    AgentEnvelope::Request { request } => {
                        let sink_handle = sink_handle.clone();
                        let policy = cfg.policy.clone();
                        let jobs = jobs.clone();
                        // Execution is blocking (shell/file/jobs); run it off
                        // the async runtime thread. dispatch_request sends
                        // results/updates via the shared AgentSink.
                        tokio::task::spawn_blocking(move || {
                            let _ = dispatch_request(&sink_handle, &policy, &jobs, request);
                        });
                    }
                    AgentEnvelope::Ping { ts } => {
                        let _ = out_tx.send(AgentEnvelope::Pong { ts }).await;
                    }
                    AgentEnvelope::Pong { .. } => {
                        // Normal keepalive response from the server to our
                        // Ping. This is expected liveness traffic: do not
                        // log at info level, do not disconnect, do not treat
                        // it as an unexpected envelope. Staying silent here
                        // keeps the agent log quiet during idle periods.
                    }
                    AgentEnvelope::Error { code, message } => {
                        eprintln!(
                            "webcodex-agent websocket server error {}: {}",
                            code, message
                        );
                        break;
                    }
                    other => {
                        eprintln!(
                            "webcodex-agent websocket ignoring unexpected envelope: {}",
                            other.kind()
                        );
                    }
                }
            }
            _ = ping_interval.tick() => {
                let _ = out_tx
                    .send(AgentEnvelope::Ping {
                        ts: chrono::Utc::now().timestamp(),
                    })
                    .await;
            }
        }
    }

    // Shutdown: drop the sender so the writer stops sending, drop the read
    // half so the underlying socket can be torn down, then give the writer a
    // brief grace window to flush any in-flight frame before aborting it. We
    // must NOT await an unbounded graceful close (see the writer task note):
    // bounding here guarantees `websocket_session` (and therefore the
    // reconnect loop) always makes progress after a disconnect.
    drop(out_tx);
    drop(stream);
    let mut writer_task = writer_task;
    if tokio::time::timeout(WS_WRITER_CLOSE_TIMEOUT, &mut writer_task)
        .await
        .is_err()
    {
        writer_task.abort();
    }
    // Give in-flight job threads a moment to flush final updates through the
    // (now closed) sink; they will log send errors and exit on their own.
    while jobs.has_work() {
        std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms.min(1000)));
    }
    Ok(())
}

fn main() {
    let (config_path, once) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    let cfg = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    if let Err(e) = run_agent(cfg, once) {
        eprintln!("webcodex-agent failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(projects_dir: PathBuf) -> AgentConfig {
        AgentConfig {
            server_url: "http://127.0.0.1:8000".to_string(),
            token: "test-token".to_string(),
            client_id: "oe".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            projects_dir: Some(projects_dir),
            poll_interval_ms: 1000,
            capabilities: None,
            max_concurrent_jobs: None,
            policy: AgentPolicy::default(),
            transport: None,
        }
    }

    #[test]
    fn agent_project_toml_parse_sorts_hook_names() {
        let project = parse_agent_project_toml(
            r#"
id = "webcodex"
path = "/root/git/webcodex"
kind = "rust"

[hooks]
precommit = ["cargo test"]
doctor = ["git status --short"]
"#,
        )
        .unwrap();
        let summary = agent_project_summary(&project, 123456, false);
        assert_eq!(summary.id, "webcodex");
        assert_eq!(summary.name.as_deref(), Some("webcodex"));
        assert_eq!(summary.path, "/root/git/webcodex");
        assert_eq!(summary.kind.as_deref(), Some("rust"));
        assert_eq!(summary.hooks, vec!["doctor", "precommit"]);
        assert_eq!(summary.updated_at, 123456);
        assert_eq!(summary.git_branch, None);
    }

    #[test]
    fn agent_project_toml_rejects_invalid_id() {
        let err = parse_agent_project_toml(
            r#"
id = "bad id"
path = "/tmp/webcodex"
"#,
        )
        .unwrap_err();
        assert!(err.contains("ASCII letters"));
    }

    #[test]
    fn missing_projects_dir_returns_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing-projects.d");
        let projects = load_agent_project_summaries_from_dir(&missing);
        assert!(projects.is_empty());
    }

    #[test]
    fn max_concurrent_jobs_defaults_to_two_and_clamps_to_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = test_config(tmp.path().join("config/projects.d"));
        assert_eq!(max_concurrent_jobs(&cfg), DEFAULT_MAX_CONCURRENT_JOBS);

        cfg.max_concurrent_jobs = Some(0);
        assert_eq!(max_concurrent_jobs(&cfg), 1);

        cfg.max_concurrent_jobs = Some(4);
        assert_eq!(max_concurrent_jobs(&cfg), 4);
    }

    #[test]
    fn shell_job_success_and_failure_results_are_structured() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();

        let success = run_shell(
            &cfg.policy,
            Some(&cwd),
            "printf hello; printf warn >&2",
            None,
            10,
            None,
        );
        assert_eq!(success.exit_code, Some(0));
        assert_eq!(success.stdout.as_deref(), Some("hello"));
        assert_eq!(success.stderr.as_deref(), Some("warn"));
        assert!(success.error.is_none());

        let failure = run_shell(&cfg.policy, Some(&cwd), "exit 7", None, 10, None);
        assert_eq!(failure.exit_code, Some(7));
        assert!(failure.error.is_none());
    }

    #[test]
    fn shell_job_writes_stdin_to_child() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();

        let result = run_shell(
            &cfg.policy,
            Some(&cwd),
            "cat",
            Some("stdin payload\n"),
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("stdin payload\n"));
        assert!(result.error.is_none());
    }

    #[test]
    fn shell_job_timeout_returns_timeout_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();

        let result = run_shell(&cfg.policy, Some(&cwd), "sleep 2", None, 1, None);
        assert_eq!(result.exit_code, Some(-1));
        assert_eq!(result.error.as_deref(), Some("command timed out"));
        assert!(result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("command timed out after 1 seconds"));
    }

    #[test]
    fn shell_job_stop_flag_is_best_effort() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();
        let stop_requested = AtomicBool::new(true);

        let result = run_shell(
            &cfg.policy,
            Some(&cwd),
            "sleep 2",
            None,
            10,
            Some(&stop_requested),
        );
        assert_eq!(result.exit_code, Some(-1));
        assert_eq!(result.error.as_deref(), Some("job stopped"));
        assert!(result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("job stopped by request"));
    }

    #[test]
    fn shell_job_stdout_stderr_are_bounded() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = test_config(tmp.path().join("config/projects.d"));
        cfg.policy.max_output_bytes = 8;
        let cwd = tmp.path().to_string_lossy().to_string();

        let result = run_shell(
            &cfg.policy,
            Some(&cwd),
            "printf 0123456789; printf abcdefghij >&2",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        let stdout = result.stdout.unwrap();
        let stderr = result.stderr.unwrap();
        assert!(stdout.contains("[output truncated to last 8 bytes]"));
        assert!(stdout.ends_with("23456789"));
        assert!(stderr.contains("[output truncated to last 8 bytes]"));
        assert!(stderr.ends_with("cdefghij"));
    }

    #[test]
    fn register_request_announces_polling_v1_protocol_version() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let body = build_register_request(&cfg, Vec::new(), AGENT_PROTOCOL_VERSION_POLLING_V1);
        assert_eq!(body.client_id, "oe");
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(AGENT_PROTOCOL_VERSION_POLLING_V1)
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some("polling-v1"));
        // Capabilities advertised by the agent include async + file access.
        let caps = body.capabilities.expect("agent registers capabilities");
        assert!(caps.shell);
        assert!(caps.file_read);
        assert!(caps.file_write);
        assert!(caps.async_jobs);
        assert!(caps.async_shell_jobs);
    }

    // ------------------------------------------------------------------------
    // WebSocket transport helpers + shared dispatch over a WebSocket sink
    // ------------------------------------------------------------------------

    #[test]
    fn server_url_to_ws_converts_http_https_and_rejects_bare() {
        assert_eq!(
            server_url_to_ws("http://127.0.0.1:8080", "/api/agents/ws").unwrap(),
            "ws://127.0.0.1:8080/api/agents/ws"
        );
        assert_eq!(
            server_url_to_ws("https://example.com/", "/api/agents/ws").unwrap(),
            "wss://example.com/api/agents/ws"
        );
        // Already a ws(s) URL passes through.
        assert_eq!(
            server_url_to_ws("wss://example.com", "/api/agents/ws").unwrap(),
            "wss://example.com/api/agents/ws"
        );
        assert!(server_url_to_ws("ftp://x", "/api/agents/ws").is_err());
    }

    #[test]
    fn build_register_request_announces_websocket_v1_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let body = build_register_request(&cfg, Vec::new(), AGENT_PROTOCOL_VERSION_WEBSOCKET_V1);
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1)
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some("websocket-v1"));
    }

    fn ws_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
        (
            AgentSink::WebSocket {
                tx,
                client_id: client_id.to_string(),
            },
            rx,
        )
    }

    #[test]
    fn websocket_sink_submit_result_sends_result_envelope() {
        let (sink, mut rx) = ws_sink("ws-client");
        let result = CommandResult {
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(3),
            error: None,
        };
        assert!(sink.submit_result("req-9".to_string(), result).unwrap());
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.client_id, "ws-client");
                assert_eq!(payload.request_id, "req-9");
                assert_eq!(payload.exit_code, Some(0));
                assert_eq!(payload.stdout.as_deref(), Some("hi"));
            }
            other => panic!("expected result, got {:?}", other.kind()),
        }
    }

    #[test]
    fn websocket_sink_send_job_update_sends_job_update_envelope() {
        let (sink, mut rx) = ws_sink("ws-client");
        let body = ShellAgentJobUpdateRequest {
            client_id: "ws-client".to_string(),
            job_id: "job-1".to_string(),
            request_id: Some("req-1".to_string()),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            finished: false,
        };
        sink.send_job_update(&body).unwrap();
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::JobUpdate { payload } => {
                assert_eq!(payload.job_id, "job-1");
                assert_eq!(payload.status, "running");
            }
            other => panic!("expected job_update, got {:?}", other.kind()),
        }
    }

    #[test]
    fn dispatch_request_run_shell_sends_result_over_websocket_sink() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();
        let (sink, mut rx) = ws_sink("ws-client");
        let jobs = JobManager::new(max_concurrent_jobs(&cfg));
        let request = ShellAgentShellRequest {
            request_id: "req-shell".to_string(),
            client_id: "ws-client".to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some(cwd),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            create_dirs: false,
            command: "printf wsok".to_string(),
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
        };
        let ran = dispatch_request(&sink, &cfg.policy, &jobs, request).unwrap();
        assert!(ran);
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.request_id, "req-shell");
                assert_eq!(payload.exit_code, Some(0));
                assert_eq!(payload.stdout.as_deref(), Some("wsok"));
            }
            other => panic!("expected result, got {:?}", other.kind()),
        }
    }

    #[test]
    fn http_sink_client_id_matches_config() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let client = Client::new();
        let sink = AgentSink::Http(HttpSendConfig {
            client,
            server_url: cfg.server_url.clone(),
            token: cfg.token.clone(),
            client_id: cfg.client_id.clone(),
        });
        assert_eq!(sink.client_id(), "oe");
    }

    // ------------------------------------------------------------------------
    // WebSocket session: Pong must be handled as keepalive, not unexpected
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn websocket_session_accepts_pong_without_error_or_disconnect() {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;
        use tokio_tungstenite::tungstenite::Message as WsMessage;

        // Minimal WS server. It:
        //   1. reads the agent's Register,
        //   2. sends a Registered ack,
        //   3. sends a Pong (the frame that previously triggered the noisy
        //      "ignoring unexpected envelope: pong" path),
        //   4. sends a Ping and waits for the agent's Pong reply — if the
        //      agent had exited on the Pong in step 3 it would never reply,
        //      and this receive would time out (failing the test),
        //   5. drops the socket so the agent's session returns cleanly.
        //
        // This both guards the "Pong is not unexpected" regression and proves
        // the session stays alive after a Pong.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            // Read Register.
            let reg_msg = ws.next().await.unwrap().unwrap();
            let reg_env =
                AgentEnvelope::from_slice(reg_msg.into_text().unwrap().as_bytes()).unwrap();
            assert!(matches!(reg_env, AgentEnvelope::Register { .. }));

            // Ack register.
            let ack = AgentEnvelope::Registered {
                success: true,
                client: None,
                error: None,
            };
            ws.send(WsMessage::Text(ack.to_json().unwrap().into()))
                .await
                .unwrap();

            // Send a Pong — the agent must accept it as keepalive and stay
            // connected (this is the regression we are guarding against).
            let pong = AgentEnvelope::Pong { ts: 42 };
            ws.send(WsMessage::Text(pong.to_json().unwrap().into()))
                .await
                .unwrap();

            // Probe liveness: send a Ping and expect a Pong reply. If the
            // agent had broken out of its read loop on the Pong above, this
            // would time out.
            ws.send(WsMessage::Text(
                AgentEnvelope::Ping { ts: 7 }.to_json().unwrap().into(),
            ))
            .await
            .unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(2), ws.next())
                .await
                .expect("agent did not reply to ping after pong (session exited on pong)")
                .expect("stream open")
                .expect("ok message");
            match AgentEnvelope::from_slice(reply.into_text().unwrap().as_bytes()).unwrap() {
                AgentEnvelope::Pong { ts } => assert_eq!(ts, 7),
                other => panic!("expected pong reply, got {:?}", other.kind()),
            }

            // Drop the socket; the agent's reader will error/EOF and the
            // session returns cleanly. Avoids a close-handshake that can hang
            // on a current-thread test runtime.
            drop(ws);
        });

        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = test_config(tmp.path().join("config/projects.d"));
        cfg.server_url = format!("http://{}", addr);
        cfg.transport = Some(TRANSPORT_WEBSOCKET.to_string());

        let outcome =
            tokio::time::timeout(Duration::from_secs(10), websocket_session(&cfg, Vec::new()))
                .await
                .expect("websocket_session did not complete in time");

        // The session must end (server dropped the socket) and must NOT have
        // returned an error — a Pong is normal keepalive traffic.
        assert!(
            outcome.is_ok(),
            "websocket_session errored on Pong (regression): {:?}",
            outcome
        );

        server_task.await.unwrap();
    }
}
