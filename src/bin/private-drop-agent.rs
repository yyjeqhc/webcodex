use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

use shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentJobUpdateResponse, ShellAgentPollRequest,
    ShellAgentPollResponse, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentResultResponse, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellClientRegisterResponse,
};

const DEFAULT_CONFIG_PATH: &str = "/etc/private-drop-agent/agent.toml";
const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const DEFAULT_MAX_TIMEOUT_SECS: u64 = 3600;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const PROJECT_SCAN_CACHE_MS: u64 = 5000;

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
    policy: AgentPolicy,
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

#[derive(Debug)]
struct CommandResult {
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct JobManager {
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
}

#[derive(Debug, Clone)]
struct RunningJob {
    child: Arc<Mutex<Child>>,
    stop_requested: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentProjectFile {
    id: String,
    path: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
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
    "Usage: private-drop-agent [--config PATH] [--once]\n\n\
     Environment:\n\
       PRIVATE_DROP_AGENT_CONFIG  default config path override\n\n\
     Example agent.toml:\n\
       server_url = \"https://v4.yyjeqhc.cn\"\n\
       token = \"...\"\n\
       client_id = \"xrh\"\n\
       display_name = \"XRH\"\n\
       owner = \"yyjeqhc\"\n\
       projects_dir = \"/root/.config/private-drop-agent/projects.d\"\n\
       poll_interval_ms = 1000\n\
\n\
       [policy]\n\
       allow_raw_shell = true\n\
       allow_cwd_anywhere = true\n\
       max_timeout_secs = 3600\n\
       max_output_bytes = 262144\n"
}

fn parse_args() -> Result<(PathBuf, bool), String> {
    let mut config_path = std::env::var("PRIVATE_DROP_AGENT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH));
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
        .join(".config/private-drop-agent/projects.d")
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
                    "private-drop-agent project warning: {} hook {} command {} is empty",
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
                "private-drop-agent project warning: failed to read {}: {}",
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
                    "private-drop-agent project warning: failed to read {}: {}",
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
                    "private-drop-agent project warning: skipping {}: {}",
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
                "private-drop-agent project warning: duplicate project id {} in {}; skipping",
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

fn register(
    client: &Client,
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
) -> Result<(), String> {
    let mut capabilities = cfg.capabilities.clone().unwrap_or_default();
    capabilities.jobs = true;
    capabilities.file_read = true;
    capabilities.file_write = true;
    let body = ShellClientRegisterRequest {
        client_id: cfg.client_id.clone(),
        display_name: cfg.display_name.clone(),
        owner: cfg.owner.clone(),
        hostname: cfg.hostname.clone().or_else(hostname),
        capabilities: Some(capabilities),
        projects: Some(project_cache.get(cfg)),
    };
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
    timeout_secs: u64,
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
    let spawn = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
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
    loop {
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

fn submit_result(
    client: &Client,
    cfg: &AgentConfig,
    request_id: String,
    result: CommandResult,
) -> Result<bool, String> {
    let body = ShellAgentResultRequest {
        client_id: cfg.client_id.clone(),
        request_id,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        duration_ms: result.duration_ms,
        error: result.error,
    };
    let response: ShellAgentResultResponse =
        post_json(client, cfg, "/api/shell/agent/result", &body)?;
    if response.success {
        Ok(true)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "result submission failed without error".to_string()))
    }
}

fn send_job_update(
    client: &Client,
    cfg: &AgentConfig,
    body: &ShellAgentJobUpdateRequest,
) -> Result<(), String> {
    let response: ShellAgentJobUpdateResponse =
        post_json(client, cfg, "/api/shell/agent/job_update", body)?;
    if response.success {
        Ok(())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "job_update failed without error".to_string()))
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

fn send_start_failure(
    client: &Client,
    cfg: &AgentConfig,
    request: ShellAgentShellRequest,
    error: String,
) {
    if let Some(job_id) = request.job_id {
        let _ = send_job_update(
            client,
            cfg,
            &ShellAgentJobUpdateRequest {
                client_id: cfg.client_id.clone(),
                job_id,
                request_id: Some(request.request_id),
                status: "failed".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                exit_code: None,
                duration_ms: Some(0),
                error: Some(error),
            },
        );
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
    fn start(&self, client: Client, cfg: AgentConfig, request: ShellAgentShellRequest) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        if !cfg.policy.allow_raw_shell {
            send_start_failure(
                &client,
                &cfg,
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
        if let Err(e) = cwd_allowed(&cfg.policy, &cwd_path) {
            send_start_failure(&client, &cfg, request, e);
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
                send_start_failure(
                    &client,
                    &cfg,
                    request,
                    format!("failed to spawn command: {}", e),
                );
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));
        let stop_requested = Arc::new(AtomicBool::new(false));
        self.jobs.lock().unwrap().insert(
            job_id.clone(),
            RunningJob {
                child: child.clone(),
                stop_requested: stop_requested.clone(),
            },
        );
        let _ = send_job_update(
            &client,
            &cfg,
            &ShellAgentJobUpdateRequest {
                client_id: cfg.client_id.clone(),
                job_id: job_id.clone(),
                request_id: Some(request.request_id.clone()),
                status: "running".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                exit_code: None,
                duration_ms: None,
                error: None,
            },
        );
        let jobs = self.jobs.clone();
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
            let timeout_secs = request.timeout_secs.min(cfg.policy.max_timeout_secs).max(1);
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
                    let _ = send_job_update(
                        &client,
                        &cfg,
                        &ShellAgentJobUpdateRequest {
                            client_id: cfg.client_id.clone(),
                            job_id: job_id.clone(),
                            request_id: Some(request.request_id.clone()),
                            status: "running".to_string(),
                            stdout_chunk: (!out.is_empty()).then_some(out),
                            stderr_chunk: (!err.is_empty()).then_some(err),
                            exit_code: None,
                            duration_ms: None,
                            error: None,
                        },
                    );
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
            let _ = send_job_update(
                &client,
                &cfg,
                &ShellAgentJobUpdateRequest {
                    client_id: cfg.client_id.clone(),
                    job_id: job_id.clone(),
                    request_id: Some(request.request_id),
                    status: final_status.0,
                    stdout_chunk: (!out.is_empty()).then_some(out),
                    stderr_chunk: (!err.is_empty()).then_some(err),
                    exit_code: final_status.1,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: final_status.2,
                },
            );
            jobs.lock().unwrap().remove(&job_id);
        });
    }

    fn stop(&self, job_id: &str) -> Result<(), String> {
        let (child, stop_requested) = {
            let jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.get(job_id) else {
                return Err(format!("unknown local job: {}", job_id));
            };
            (job.child.clone(), job.stop_requested.clone())
        };
        stop_requested.store(true, Ordering::SeqCst);
        kill_child_group(&child).map_err(|e| format!("failed to kill job {}: {}", job_id, e))
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
    match request.kind.as_str() {
        "start_job" => {
            jobs.start(client.clone(), cfg.clone(), request);
            Ok(true)
        }
        "stop_job" => {
            if let Some(job_id) = request.job_id.as_deref() {
                if let Err(e) = jobs.stop(job_id) {
                    eprintln!("private-drop-agent stop_job error: {}", e);
                }
            }
            Ok(true)
        }
        "file_read" | "file_write" | "file_list" => {
            let request_id = request.request_id.clone();
            let result = handle_file_request(&cfg.policy, &request);
            submit_result(client, cfg, request_id, result)
        }
        _ => {
            let result = run_shell(
                &cfg.policy,
                request.cwd.as_deref(),
                &request.command,
                request.timeout_secs,
            );
            submit_result(client, cfg, request.request_id, result)
        }
    }
}

fn run_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = JobManager::default();
    let mut project_cache = AgentProjectCache::default();
    register(&client, &cfg, &mut project_cache)?;
    eprintln!(
        "private-drop-agent registered client_id={} server={}",
        cfg.client_id, cfg.server_url
    );
    loop {
        match handle_one_poll(&client, &cfg, &jobs, &mut project_cache) {
            Ok(ran_request) => {
                if once {
                    return Ok(());
                }
                if !ran_request {
                    std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                }
            }
            Err(e) => {
                eprintln!("private-drop-agent poll error: {}", e);
                if once {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                let _ = register(&client, &cfg, &mut project_cache);
            }
        }
    }
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
        eprintln!("private-drop-agent failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_project_toml_parse_sorts_hook_names() {
        let project = parse_agent_project_toml(
            r#"
id = "private-drop"
path = "/root/git/private-drop"
kind = "rust"

[hooks]
precommit = ["cargo test"]
doctor = ["git status --short"]
"#,
        )
        .unwrap();
        let summary = agent_project_summary(&project, 123456, false);
        assert_eq!(summary.id, "private-drop");
        assert_eq!(summary.name.as_deref(), Some("private-drop"));
        assert_eq!(summary.path, "/root/git/private-drop");
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
path = "/tmp/private-drop"
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
}
