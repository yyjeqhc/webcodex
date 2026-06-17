use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
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
    ShellAgentPollResponse, ShellAgentProjectCreatePayload, ShellAgentProjectCreateResult,
    ShellAgentProjectGitSnapshot, ShellAgentProjectHookResult, ShellAgentProjectHookStep,
    ShellAgentProjectSummary, ShellAgentProjectWorkflowPayload, ShellAgentProjectWorkflowResult,
    ShellAgentResultRequest, ShellAgentResultResponse, ShellAgentShellRequest,
    ShellClientCapabilities, ShellClientRegisterRequest, ShellClientRegisterResponse,
};

const DEFAULT_CONFIG_PATH: &str = "/etc/private-drop-agent/agent.toml";
const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const DEFAULT_MAX_TIMEOUT_SECS: u64 = 3600;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const PROJECT_SCAN_CACHE_MS: u64 = 5000;
const WORKFLOW_STEP_OUTPUT_BYTES: usize = 16 * 1024;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProjectFile {
    id: String,
    path: String,
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

    fn refresh(&mut self, cfg: &AgentConfig) -> Vec<ShellAgentProjectSummary> {
        self.projects = load_agent_project_summaries(cfg);
        self.refreshed_at = Some(Instant::now());
        self.projects.clone()
    }
}

fn expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(rest);
    }
    PathBuf::from(value)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|e| format!("failed to resolve current directory: {}", e))
    }
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        let parent = current.parent()?;
        current = parent.to_path_buf();
    }
}

fn ensure_project_path_allowed(policy: &AgentPolicy, path: &Path) -> Result<(), String> {
    if policy.allow_cwd_anywhere {
        return Ok(());
    }
    let Some(existing) = existing_ancestor(path) else {
        return Err(format!(
            "path {} has no existing ancestor for policy check",
            path.display()
        ));
    };
    cwd_allowed(policy, &existing)
}

fn is_empty_dir(path: &Path) -> Result<bool, String> {
    let mut entries = std::fs::read_dir(path)
        .map_err(|e| format!("failed to read directory {}: {}", path.display(), e))?;
    Ok(entries.next().is_none())
}

fn ensure_dir(path: &Path, created_paths: &mut Vec<String>) -> Result<(), String> {
    if path.exists() {
        if path.is_dir() {
            return Ok(());
        }
        return Err(format!("{} exists but is not a directory", path.display()));
    }
    std::fs::create_dir_all(path)
        .map_err(|e| format!("failed to create directory {}: {}", path.display(), e))?;
    created_paths.push(path.to_string_lossy().to_string());
    Ok(())
}

fn write_file_if_absent(
    path: &Path,
    content: &str,
    created_paths: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if path.exists() {
        warnings.push(format!("{} already exists; left unchanged", path.display()));
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory {}: {}", parent.display(), e))?;
    }
    std::fs::write(path, content)
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    created_paths.push(path.to_string_lossy().to_string());
    Ok(())
}

fn default_hooks(template: &str) -> HashMap<String, Vec<String>> {
    let mut hooks = HashMap::new();
    match template {
        "rust" => {
            hooks.insert(
                "doctor".to_string(),
                vec![
                    "git status --short".to_string(),
                    "git log --oneline -5".to_string(),
                ],
            );
            hooks.insert(
                "precommit".to_string(),
                vec![
                    ". /root/.cargo/env && cargo fmt --check".to_string(),
                    ". /root/.cargo/env && cargo test".to_string(),
                ],
            );
        }
        "python" => {
            hooks.insert(
                "doctor".to_string(),
                vec![
                    "git status --short".to_string(),
                    "python3 --version".to_string(),
                ],
            );
            hooks.insert(
                "precommit".to_string(),
                vec!["python3 -m compileall .".to_string()],
            );
        }
        _ => {
            hooks.insert(
                "doctor".to_string(),
                vec![
                    "git status --short".to_string(),
                    "git log --oneline -5".to_string(),
                ],
            );
        }
    }
    hooks
}

fn validate_hooks(hooks: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    for (hook, commands) in hooks {
        if hook.trim().is_empty() {
            return Err("hook name cannot be empty".to_string());
        }
        for (idx, command) in commands.iter().enumerate() {
            if command.trim().is_empty() {
                warnings.push(format!("hook {} command {} is empty", hook, idx));
            }
        }
    }
    Ok(warnings)
}

fn safe_rust_package_name(project_id: &str) -> String {
    let mut value = project_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        value = "project".to_string();
    }
    if value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        value = format!("project_{}", value);
    }
    value
}

fn safe_python_module_name(project_id: &str) -> String {
    let mut value = project_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while value.contains("__") {
        value = value.replace("__", "_");
    }
    value = value.trim_matches('_').to_string();
    if value.is_empty() {
        value = "project".to_string();
    }
    if value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        value = format!("project_{}", value);
    }
    value
}

fn write_template_files(
    project_id: &str,
    name: &str,
    template: &str,
    project_path: &Path,
    created_paths: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let readme = format!("# {}\n", name);
    match template {
        "empty" => {
            write_file_if_absent(
                &project_path.join("README.md"),
                &readme,
                created_paths,
                warnings,
            )?;
        }
        "rust" => {
            let crate_name = safe_rust_package_name(project_id);
            write_file_if_absent(
                &project_path.join("README.md"),
                &readme,
                created_paths,
                warnings,
            )?;
            write_file_if_absent(
                &project_path.join("Cargo.toml"),
                &format!(
                    "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
                    crate_name
                ),
                created_paths,
                warnings,
            )?;
            let src_dir = project_path.join("src");
            ensure_dir(&src_dir, created_paths)?;
            write_file_if_absent(
                &src_dir.join("main.rs"),
                "fn main() {\n    println!(\"Hello, world!\");\n}\n",
                created_paths,
                warnings,
            )?;
        }
        "python" => {
            let module_name = safe_python_module_name(project_id);
            write_file_if_absent(
                &project_path.join("README.md"),
                &readme,
                created_paths,
                warnings,
            )?;
            write_file_if_absent(
                &project_path.join("pyproject.toml"),
                &format!(
                    "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"{}\"\nrequires-python = \">=3.9\"\ndependencies = []\n",
                    safe_rust_package_name(project_id),
                    name.replace('"', "\\\"")
                ),
                created_paths,
                warnings,
            )?;
            let module_dir = project_path.join("src").join(module_name);
            ensure_dir(&module_dir, created_paths)?;
            write_file_if_absent(&module_dir.join("__init__.py"), "", created_paths, warnings)?;
        }
        "docs" => {
            write_file_if_absent(
                &project_path.join("README.md"),
                &readme,
                created_paths,
                warnings,
            )?;
            let docs_dir = project_path.join("docs");
            ensure_dir(&docs_dir, created_paths)?;
            write_file_if_absent(&docs_dir.join("index.md"), &readme, created_paths, warnings)?;
        }
        _ => return Err("template must be one of empty, rust, python, docs".to_string()),
    }
    Ok(())
}

fn run_git_init(project_path: &Path, warnings: &mut Vec<String>) -> Result<bool, String> {
    if project_path.join(".git").exists() {
        warnings.push(".git already exists; skipped git init".to_string());
        return Ok(false);
    }
    let output = std::process::Command::new("git")
        .arg("init")
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run git init: {}", e))?;
    if output.status.success() {
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "git init failed".to_string()
        } else {
            format!("git init failed: {}", stderr)
        })
    }
}

fn write_registry_file_atomic(
    registry_file: &Path,
    project: &AgentProjectFile,
) -> Result<(), String> {
    if let Some(parent) = registry_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create projects_dir {}: {}", parent.display(), e))?;
    }
    let content = toml::to_string_pretty(project)
        .map_err(|e| format!("failed to serialize project registry toml: {}", e))?;
    let tmp = registry_file.with_file_name(format!(
        ".{}.tmp.{}",
        registry_file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project.toml"),
        std::process::id()
    ));
    std::fs::write(&tmp, content).map_err(|e| {
        format!(
            "failed to write temp registry file {}: {}",
            tmp.display(),
            e
        )
    })?;
    std::fs::rename(&tmp, registry_file).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "failed to move registry file {} into place: {}",
            registry_file.display(),
            e
        )
    })
}

fn project_create_error(
    error: String,
    created_paths: Vec<String>,
    registry_file: Option<String>,
    git_initialized: bool,
    warnings: Vec<String>,
) -> ShellAgentProjectCreateResult {
    ShellAgentProjectCreateResult {
        success: false,
        project: None,
        created_paths,
        registry_file,
        git_initialized,
        warnings,
        error: Some(error),
    }
}

fn create_agent_project(
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
    payload: ShellAgentProjectCreatePayload,
) -> ShellAgentProjectCreateResult {
    let mut created_paths = Vec::new();
    let mut warnings = Vec::new();
    let mut git_initialized = false;

    let project_id = payload.project_id.trim().to_string();
    if let Err(e) = validate_project_id(&project_id) {
        return project_create_error(e, created_paths, None, false, warnings);
    }
    if payload.path.trim().is_empty() {
        return project_create_error(
            "path cannot be empty".to_string(),
            created_paths,
            None,
            false,
            warnings,
        );
    }
    let template = payload.template.trim();
    if !matches!(template, "empty" | "rust" | "python" | "docs") {
        return project_create_error(
            "template must be one of empty, rust, python, docs".to_string(),
            created_paths,
            None,
            false,
            warnings,
        );
    }
    let project_path = match absolute_path(expand_tilde_path(payload.path.trim())) {
        Ok(path) => path,
        Err(e) => return project_create_error(e, created_paths, None, false, warnings),
    };
    if project_path == Path::new("/") {
        return project_create_error(
            "path cannot be filesystem root".to_string(),
            created_paths,
            None,
            false,
            warnings,
        );
    }
    let projects_dir = match absolute_path(projects_dir(cfg)) {
        Ok(path) => path,
        Err(e) => return project_create_error(e, created_paths, None, false, warnings),
    };
    if project_path == projects_dir {
        return project_create_error(
            "path cannot be the agent projects_dir".to_string(),
            created_paths,
            None,
            false,
            warnings,
        );
    }
    if let Err(e) = ensure_project_path_allowed(&cfg.policy, &project_path) {
        return project_create_error(e, created_paths, None, false, warnings);
    }

    let registry_file = projects_dir.join(format!("{}.toml", project_id));
    let registry_file_text = registry_file.to_string_lossy().to_string();
    if registry_file.exists() && !payload.allow_existing {
        return project_create_error(
            format!("registry file already exists: {}", registry_file.display()),
            created_paths,
            Some(registry_file_text),
            false,
            warnings,
        );
    }

    let existing_project = if registry_file.exists() {
        match std::fs::read_to_string(&registry_file)
            .map_err(|e| format!("failed to read {}: {}", registry_file.display(), e))
            .and_then(|content| parse_agent_project_toml(&content))
        {
            Ok(project) => Some(project),
            Err(e) => {
                return project_create_error(
                    e,
                    created_paths,
                    Some(registry_file_text),
                    false,
                    warnings,
                )
            }
        }
    } else {
        None
    };

    if project_path.exists() {
        if !project_path.is_dir() {
            return project_create_error(
                format!(
                    "path exists but is not a directory: {}",
                    project_path.display()
                ),
                created_paths,
                Some(registry_file_text),
                false,
                warnings,
            );
        }
        match is_empty_dir(&project_path) {
            Ok(true) => {}
            Ok(false) if !payload.allow_existing => {
                return project_create_error(
                    format!("path is not empty: {}", project_path.display()),
                    created_paths,
                    Some(registry_file_text),
                    false,
                    warnings,
                )
            }
            Ok(false) => warnings.push(format!(
                "{} is not empty; existing files will not be overwritten",
                project_path.display()
            )),
            Err(e) => {
                return project_create_error(
                    e,
                    created_paths,
                    Some(registry_file_text),
                    false,
                    warnings,
                )
            }
        }
    } else if let Err(e) = ensure_dir(&project_path, &mut created_paths) {
        return project_create_error(e, created_paths, Some(registry_file_text), false, warnings);
    }

    let name = trim_optional(payload.name)
        .or_else(|| {
            existing_project
                .as_ref()
                .and_then(|project| project.name.clone())
        })
        .unwrap_or_else(|| project_id.clone());
    let kind = trim_optional(payload.kind)
        .or_else(|| {
            existing_project
                .as_ref()
                .and_then(|project| project.kind.clone())
        })
        .or_else(|| (template != "empty").then(|| template.to_string()));
    let description = trim_optional(payload.description).or_else(|| {
        existing_project
            .as_ref()
            .and_then(|project| project.description.clone())
    });
    let hooks = if let Some(hooks) = payload.hooks {
        hooks
    } else if let Some(project) = existing_project {
        project.hooks
    } else {
        default_hooks(template)
    };
    match validate_hooks(&hooks) {
        Ok(mut hook_warnings) => warnings.append(&mut hook_warnings),
        Err(e) => {
            return project_create_error(
                e,
                created_paths,
                Some(registry_file_text),
                false,
                warnings,
            )
        }
    }

    if let Err(e) = write_template_files(
        &project_id,
        &name,
        template,
        &project_path,
        &mut created_paths,
        &mut warnings,
    ) {
        return project_create_error(e, created_paths, Some(registry_file_text), false, warnings);
    }

    if payload.git_init {
        match run_git_init(&project_path, &mut warnings) {
            Ok(initialized) => git_initialized = initialized,
            Err(e) => {
                return project_create_error(
                    e,
                    created_paths,
                    Some(registry_file_text),
                    false,
                    warnings,
                )
            }
        }
    }

    let project_file = AgentProjectFile {
        id: project_id.clone(),
        path: project_path.to_string_lossy().to_string(),
        name: Some(name),
        kind,
        description,
        disabled: false,
        hooks,
    };
    if let Err(e) = write_registry_file_atomic(&registry_file, &project_file) {
        return project_create_error(
            e,
            created_paths,
            Some(registry_file_text),
            git_initialized,
            warnings,
        );
    }
    created_paths.push(registry_file_text.clone());

    let projects = project_cache.refresh(cfg);
    let project = projects
        .into_iter()
        .find(|project| project.id == project_id)
        .or_else(|| {
            Some(agent_project_summary(
                &project_file,
                chrono::Utc::now().timestamp(),
                true,
            ))
        });
    ShellAgentProjectCreateResult {
        success: true,
        project,
        created_paths,
        registry_file: Some(registry_file_text),
        git_initialized,
        warnings,
        error: None,
    }
}

fn normalize_agent_workflow_mode(mode: &str) -> Result<String, String> {
    let mode = mode.trim();
    let mode = if mode.is_empty() { "snapshot" } else { mode };
    match mode {
        "snapshot" | "doctor" | "hook" | "precommit" => Ok(mode.to_string()),
        _ => Err("mode must be one of snapshot, doctor, hook, precommit".to_string()),
    }
}

fn load_agent_project_from_dir(dir: &Path, project_id: &str) -> Result<AgentProjectFile, String> {
    validate_project_id(project_id)?;
    let file = dir.join(format!("{}.toml", project_id));
    let content = match std::fs::read_to_string(&file) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("agent project '{}' is not configured", project_id));
        }
        Err(e) => return Err(format!("failed to read {}: {}", file.display(), e)),
    };
    let project = parse_agent_project_toml(&content)
        .map_err(|e| format!("failed to parse {}: {}", file.display(), e))?;
    if project.id != project_id {
        return Err(format!(
            "agent project registry file {} has id '{}', expected '{}'",
            file.display(),
            project.id,
            project_id
        ));
    }
    Ok(project)
}

fn run_git_capture_checked(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run git {}: {}", args.join(" "), e))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        format!("git {} failed: {}", args.join(" "), stderr)
    })
}

fn collect_agent_project_git_snapshot(path: &Path) -> ShellAgentProjectGitSnapshot {
    let status_short = match run_git_capture_checked(path, &["status", "--short"]) {
        Ok(status) => status,
        Err(e) => {
            return ShellAgentProjectGitSnapshot {
                available: false,
                branch: None,
                head: None,
                head_subject: None,
                status_short: None,
                dirty: None,
                diff_stat: None,
                changed_files: Vec::new(),
                error: Some(e),
            };
        }
    };

    let mut errors = Vec::new();
    let branch = match run_git_capture_checked(path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(value) => Some(value),
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let (head, head_subject) =
        match run_git_capture_checked(path, &["log", "-1", "--pretty=format:%h%x00%s"]) {
            Ok(value) if value.is_empty() => (None, None),
            Ok(value) => match value.split_once('\0') {
                Some((head, subject)) => (Some(head.to_string()), Some(subject.to_string())),
                None => (Some(value), None),
            },
            Err(e) => {
                errors.push(e);
                (None, None)
            }
        };
    let diff_stat = match run_git_capture_checked(path, &["diff", "--stat"]) {
        Ok(value) => Some(value),
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let changed_files = match run_git_capture_checked(path, &["diff", "--name-status"]) {
        Ok(value) => value
            .lines()
            .map(|line| line.trim_end().to_string())
            .filter(|line| !line.is_empty())
            .collect(),
        Err(e) => {
            errors.push(e);
            Vec::new()
        }
    };

    ShellAgentProjectGitSnapshot {
        available: true,
        branch,
        head,
        head_subject,
        status_short: Some(status_short.clone()),
        dirty: Some(!status_short.trim().is_empty()),
        diff_stat,
        changed_files,
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

fn push_git_snapshot_warning(
    warnings: &mut Vec<String>,
    label: &str,
    snapshot: &ShellAgentProjectGitSnapshot,
) {
    if let Some(error) = snapshot.error.as_deref() {
        warnings.push(format!("{} git snapshot warning: {}", label, error));
    }
}

fn workflow_output_tail(value: Option<String>) -> String {
    truncate_bytes(
        value.unwrap_or_default().as_bytes(),
        WORKFLOW_STEP_OUTPUT_BYTES,
    )
}

fn agent_project_hook_result(
    policy: &AgentPolicy,
    project_path: &Path,
    hook: &str,
    commands: &[String],
    timeout_secs: u64,
) -> ShellAgentProjectHookResult {
    let mut steps = Vec::new();
    let mut result_error = None;
    let cwd = project_path.to_string_lossy().to_string();
    for (idx, command) in commands.iter().enumerate() {
        let start = Instant::now();
        if command.trim().is_empty() {
            steps.push(ShellAgentProjectHookStep {
                index: idx,
                command: command.clone(),
                exit_code: None,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                success: false,
            });
            result_error = Some("hook command cannot be empty".to_string());
            break;
        }
        let command_result = run_shell(policy, Some(&cwd), command, timeout_secs);
        let step_success = command_result.error.is_none() && command_result.exit_code == Some(0);
        let step_error = command_result.error.clone();
        let exit_code = command_result.exit_code;
        let duration_ms = command_result
            .duration_ms
            .unwrap_or_else(|| start.elapsed().as_millis() as u64);
        let stdout_tail = workflow_output_tail(command_result.stdout);
        let stderr_tail = workflow_output_tail(command_result.stderr);
        steps.push(ShellAgentProjectHookStep {
            index: idx,
            command: command.clone(),
            exit_code,
            stdout_tail,
            stderr_tail,
            duration_ms,
            success: step_success,
        });
        if !step_success {
            result_error = step_error.or_else(|| Some("hook command failed".to_string()));
            break;
        }
    }
    ShellAgentProjectHookResult {
        hook: hook.to_string(),
        success: result_error.is_none(),
        steps,
        error: result_error,
    }
}

fn recommended_agent_project_next_action(
    hook_result: Option<&ShellAgentProjectHookResult>,
    git_after: &ShellAgentProjectGitSnapshot,
    warnings: &[String],
    project: Option<&ShellAgentProjectSummary>,
) -> String {
    if hook_result.is_some_and(|hook_result| !hook_result.success) {
        return "Fix failing hook step".to_string();
    }
    if warnings
        .iter()
        .any(|warning| warning.contains("agent project") && warning.contains("not configured"))
    {
        return "Create or configure the agent project".to_string();
    }
    if git_after.dirty == Some(true)
        && project
            .map(|project| project.hooks.iter().any(|hook| hook == "precommit"))
            .unwrap_or(false)
    {
        return "Run precommit hook before committing".to_string();
    }
    if git_after.dirty == Some(true) {
        return "Review git diff before committing".to_string();
    }
    if git_after.dirty == Some(false) {
        return "No changes detected".to_string();
    }
    "Review workflow warnings".to_string()
}

fn project_workflow_error_result(
    project_id: String,
    mode: String,
    project: Option<ShellAgentProjectSummary>,
    mut warnings: Vec<String>,
    error: String,
) -> ShellAgentProjectWorkflowResult {
    if warnings.is_empty() {
        warnings.push(error.clone());
    }
    let git_before = ShellAgentProjectGitSnapshot::default();
    let git_after = ShellAgentProjectGitSnapshot::default();
    let recommended_next_action =
        recommended_agent_project_next_action(None, &git_after, &warnings, project.as_ref());
    ShellAgentProjectWorkflowResult {
        success: false,
        project_id,
        project,
        mode,
        git_before,
        hook_result: None,
        git_after,
        warnings,
        recommended_next_action,
        error: Some(error),
    }
}

fn workflow_hook_name(payload: &ShellAgentProjectWorkflowPayload, mode: &str) -> Option<String> {
    match mode {
        "hook" => Some(
            payload
                .hook
                .as_deref()
                .map(str::trim)
                .filter(|hook| !hook.is_empty())
                .unwrap_or("doctor")
                .to_string(),
        ),
        "precommit" => Some(
            payload
                .hook
                .as_deref()
                .map(str::trim)
                .filter(|hook| !hook.is_empty())
                .unwrap_or("precommit")
                .to_string(),
        ),
        _ => None,
    }
}

fn normalized_doctor_hook(payload: &ShellAgentProjectWorkflowPayload) -> String {
    let value = payload.doctor_hook.trim();
    if value.is_empty() {
        "doctor".to_string()
    } else {
        value.to_string()
    }
}

fn run_agent_project_workflow(
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
    payload: ShellAgentProjectWorkflowPayload,
) -> ShellAgentProjectWorkflowResult {
    let project_id = payload.project_id.trim().to_string();
    let mode = match normalize_agent_workflow_mode(&payload.mode) {
        Ok(mode) => mode,
        Err(e) => {
            return project_workflow_error_result(
                project_id,
                "snapshot".to_string(),
                None,
                Vec::new(),
                e,
            );
        }
    };
    if let Err(e) = validate_project_id(&project_id) {
        return project_workflow_error_result(project_id, mode, None, Vec::new(), e);
    }

    let mut project = match load_agent_project_from_dir(&projects_dir(cfg), &project_id) {
        Ok(project) => project,
        Err(e) => {
            return project_workflow_error_result(project_id, mode, None, vec![e.clone()], e);
        }
    };
    if project.disabled {
        let summary = agent_project_summary(&project, chrono::Utc::now().timestamp(), false);
        let error = format!("agent project '{}' is disabled", project_id);
        return project_workflow_error_result(
            project_id,
            mode,
            Some(summary),
            vec![error.clone()],
            error,
        );
    }

    let project_path = match absolute_path(expand_tilde_path(project.path.trim())) {
        Ok(path) => path,
        Err(e) => {
            let summary = agent_project_summary(&project, chrono::Utc::now().timestamp(), false);
            return project_workflow_error_result(project_id, mode, Some(summary), Vec::new(), e);
        }
    };
    project.path = project_path.to_string_lossy().to_string();
    let summary_without_git =
        || agent_project_summary(&project, chrono::Utc::now().timestamp(), false);
    if let Err(e) = ensure_project_path_allowed(&cfg.policy, &project_path) {
        return project_workflow_error_result(
            project_id,
            mode,
            Some(summary_without_git()),
            Vec::new(),
            e,
        );
    }
    if !project_path.exists() {
        let error = format!("project path does not exist: {}", project_path.display());
        return project_workflow_error_result(
            project_id,
            mode,
            Some(summary_without_git()),
            Vec::new(),
            error,
        );
    }
    if !project_path.is_dir() {
        let error = format!(
            "project path is not a directory: {}",
            project_path.display()
        );
        return project_workflow_error_result(
            project_id,
            mode,
            Some(summary_without_git()),
            Vec::new(),
            error,
        );
    }

    let mut warnings = Vec::new();
    let git_before = collect_agent_project_git_snapshot(&project_path);
    push_git_snapshot_warning(&mut warnings, "before", &git_before);

    let mut success = true;
    let mut error = None;
    let mut hook_result = None;

    match mode.as_str() {
        "snapshot" => {}
        "doctor" => {
            if project.hooks.is_empty() {
                warnings.push(format!(
                    "agent project '{}' has no hooks configured",
                    project_id
                ));
            }
            if payload.run_doctor_hook {
                let hook_name = normalized_doctor_hook(&payload);
                match project.hooks.get(&hook_name) {
                    Some(commands) if !commands.is_empty() => {
                        let result = agent_project_hook_result(
                            &cfg.policy,
                            &project_path,
                            &hook_name,
                            commands,
                            payload.timeout_secs,
                        );
                        if !result.success {
                            success = false;
                            error = result.error.clone();
                        }
                        hook_result = Some(result);
                    }
                    _ => warnings.push(format!(
                        "agent project hook '{}' is not configured",
                        hook_name
                    )),
                }
            }
        }
        "hook" | "precommit" => {
            let hook_name = workflow_hook_name(&payload, &mode).unwrap_or_else(|| {
                if mode == "precommit" {
                    "precommit".to_string()
                } else {
                    "doctor".to_string()
                }
            });
            match project.hooks.get(&hook_name) {
                Some(commands) if !commands.is_empty() => {
                    let result = agent_project_hook_result(
                        &cfg.policy,
                        &project_path,
                        &hook_name,
                        commands,
                        payload.timeout_secs,
                    );
                    success = result.success;
                    if !result.success {
                        error = result.error.clone();
                    }
                    hook_result = Some(result);
                }
                _ => {
                    success = false;
                    error = Some(format!(
                        "agent project hook '{}' is not configured",
                        hook_name
                    ));
                }
            }
        }
        _ => unreachable!("mode was validated"),
    }

    let git_after = collect_agent_project_git_snapshot(&project_path);
    push_git_snapshot_warning(&mut warnings, "after", &git_after);
    let refreshed = project_cache.refresh(cfg);
    let summary = refreshed
        .into_iter()
        .find(|summary| summary.id == project_id)
        .unwrap_or_else(|| agent_project_summary(&project, chrono::Utc::now().timestamp(), true));
    let recommended_next_action = recommended_agent_project_next_action(
        hook_result.as_ref(),
        &git_after,
        &warnings,
        Some(&summary),
    );
    if error.is_some() {
        success = false;
    }

    ShellAgentProjectWorkflowResult {
        success,
        project_id,
        project: Some(summary),
        mode,
        git_before,
        hook_result,
        git_after,
        warnings,
        recommended_next_action,
        error,
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
    capabilities.project_create = true;
    capabilities.project_workflow = true;
    capabilities
}

fn register(
    client: &Client,
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
) -> Result<(), String> {
    let capabilities = agent_register_capabilities(cfg);
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
        project_create: None,
        project_workflow: None,
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

fn submit_project_create_result(
    client: &Client,
    cfg: &AgentConfig,
    request_id: String,
    result: ShellAgentProjectCreateResult,
) -> Result<bool, String> {
    let success = result.success && result.error.is_none();
    let error = result.error.clone();
    let body = ShellAgentResultRequest {
        client_id: cfg.client_id.clone(),
        request_id,
        exit_code: Some(if success { 0 } else { 1 }),
        stdout: None,
        stderr: None,
        duration_ms: None,
        error,
        project_create: Some(result),
        project_workflow: None,
    };
    let response: ShellAgentResultResponse =
        post_json(client, cfg, "/api/shell/agent/result", &body)?;
    if response.success {
        Ok(true)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "project create submission failed without error".to_string()))
    }
}

fn submit_project_workflow_result(
    client: &Client,
    cfg: &AgentConfig,
    request_id: String,
    result: ShellAgentProjectWorkflowResult,
) -> Result<bool, String> {
    let success = result.success && result.error.is_none();
    let error = result.error.clone();
    let body = ShellAgentResultRequest {
        client_id: cfg.client_id.clone(),
        request_id,
        exit_code: Some(if success { 0 } else { 1 }),
        stdout: None,
        stderr: None,
        duration_ms: None,
        error,
        project_create: None,
        project_workflow: Some(result),
    };
    let response: ShellAgentResultResponse =
        post_json(client, cfg, "/api/shell/agent/result", &body)?;
    if response.success {
        Ok(true)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "project workflow submission failed without error".to_string()))
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
        "create_project" => {
            let request_id = request.request_id.clone();
            let result = match request.project_create {
                Some(payload) => create_agent_project(cfg, project_cache, payload),
                None => ShellAgentProjectCreateResult {
                    success: false,
                    project: None,
                    created_paths: Vec::new(),
                    registry_file: None,
                    git_initialized: false,
                    warnings: Vec::new(),
                    error: Some(
                        "create_project request missing project_create payload".to_string(),
                    ),
                },
            };
            submit_project_create_result(client, cfg, request_id, result)
        }
        "project_workflow" => {
            let request_id = request.request_id.clone();
            let result = match request.project_workflow {
                Some(payload) => run_agent_project_workflow(cfg, project_cache, payload),
                None => ShellAgentProjectWorkflowResult {
                    success: false,
                    project_id: String::new(),
                    project: None,
                    mode: "snapshot".to_string(),
                    git_before: ShellAgentProjectGitSnapshot::default(),
                    hook_result: None,
                    git_after: ShellAgentProjectGitSnapshot::default(),
                    warnings: vec![
                        "project_workflow request missing project_workflow payload".to_string()
                    ],
                    recommended_next_action: "Review workflow warnings".to_string(),
                    error: Some(
                        "project_workflow request missing project_workflow payload".to_string(),
                    ),
                },
            };
            submit_project_workflow_result(client, cfg, request_id, result)
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
            policy: AgentPolicy::default(),
        }
    }

    fn create_payload(
        project_id: &str,
        path: &Path,
        template: &str,
    ) -> ShellAgentProjectCreatePayload {
        ShellAgentProjectCreatePayload {
            project_id: project_id.to_string(),
            path: path.to_string_lossy().to_string(),
            name: None,
            kind: None,
            description: None,
            template: template.to_string(),
            git_init: false,
            allow_existing: false,
            hooks: None,
        }
    }

    fn workflow_payload(project_id: &str, mode: &str) -> ShellAgentProjectWorkflowPayload {
        ShellAgentProjectWorkflowPayload {
            project_id: project_id.to_string(),
            mode: mode.to_string(),
            hook: None,
            run_doctor: true,
            run_doctor_hook: false,
            doctor_hook: "doctor".to_string(),
            timeout_secs: 10,
        }
    }

    fn write_project_registry(
        projects_dir: &Path,
        project_id: &str,
        project_path: &Path,
        disabled: bool,
        hooks: HashMap<String, Vec<String>>,
    ) {
        std::fs::create_dir_all(projects_dir).unwrap();
        let project = AgentProjectFile {
            id: project_id.to_string(),
            path: project_path.to_string_lossy().to_string(),
            name: Some(project_id.to_string()),
            kind: Some("rust".to_string()),
            description: None,
            disabled,
            hooks,
        };
        std::fs::write(
            projects_dir.join(format!("{}.toml", project_id)),
            toml::to_string_pretty(&project).unwrap(),
        )
        .unwrap();
    }

    fn run_git(path: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_clean_git_repo(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        run_git(path, &["init"]);
        std::fs::write(path.join("README.md"), "# demo\n").unwrap();
        run_git(path, &["add", "README.md"]);
        run_git(
            path,
            &[
                "-c",
                "user.name=Private Drop Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "Initial commit",
            ],
        );
    }

    fn hooks(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(name, commands)| {
                (
                    (*name).to_string(),
                    commands
                        .iter()
                        .map(|command| (*command).to_string())
                        .collect(),
                )
            })
            .collect()
    }

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

    #[test]
    fn create_rust_project_writes_template_and_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        let cfg = test_config(projects_dir.clone());
        let mut cache = AgentProjectCache::default();

        let result = create_agent_project(
            &cfg,
            &mut cache,
            create_payload("foo", &project_path, "rust"),
        );

        assert!(result.success, "{:?}", result.error);
        assert!(project_path.join("Cargo.toml").exists());
        assert!(project_path.join("src/main.rs").exists());
        assert!(project_path.join("README.md").exists());
        assert!(projects_dir.join("foo.toml").exists());
        let projects = load_agent_project_summaries_from_dir(&projects_dir);
        let foo = projects.iter().find(|project| project.id == "foo").unwrap();
        assert_eq!(foo.kind.as_deref(), Some("rust"));
        assert_eq!(foo.hooks, vec!["doctor", "precommit"]);
    }

    #[test]
    fn create_python_project_writes_module_template() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/py-demo");
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let result = create_agent_project(
            &cfg,
            &mut cache,
            create_payload("py-demo", &project_path, "python"),
        );

        assert!(result.success, "{:?}", result.error);
        assert!(project_path.join("pyproject.toml").exists());
        assert!(project_path.join("src/py_demo/__init__.py").exists());
        let project = result.project.unwrap();
        assert_eq!(project.kind.as_deref(), Some("python"));
        assert_eq!(project.hooks, vec!["doctor", "precommit"]);
    }

    #[test]
    fn create_project_rejects_invalid_project_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        for id in ["", "../x", "a/b"] {
            let mut cache = AgentProjectCache::default();
            let result = create_agent_project(
                &cfg,
                &mut cache,
                create_payload(id, &tmp.path().join("work/project"), "empty"),
            );
            assert!(!result.success, "id {id:?} should fail");
            assert!(result.error.is_some());
        }
    }

    #[test]
    fn create_project_rejects_non_empty_existing_dir_without_allow_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let project_path = tmp.path().join("work/foo");
        std::fs::create_dir_all(&project_path).unwrap();
        std::fs::write(project_path.join("README.md"), "keep me").unwrap();
        let mut cache = AgentProjectCache::default();

        let result = create_agent_project(
            &cfg,
            &mut cache,
            create_payload("foo", &project_path, "docs"),
        );

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not empty"));
        assert_eq!(
            std::fs::read_to_string(project_path.join("README.md")).unwrap(),
            "keep me"
        );
    }

    #[test]
    fn create_project_rejects_existing_registry_file_without_allow_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        std::fs::create_dir_all(&projects_dir).unwrap();
        std::fs::write(
            projects_dir.join("foo.toml"),
            "id = \"foo\"\npath = \"/tmp/foo\"\n",
        )
        .unwrap();
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let result = create_agent_project(
            &cfg,
            &mut cache,
            create_payload("foo", &tmp.path().join("work/foo"), "empty"),
        );

        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("registry file already exists"));
    }

    #[test]
    fn create_project_allow_existing_does_not_overwrite_files() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let project_path = tmp.path().join("work/notes");
        std::fs::create_dir_all(&project_path).unwrap();
        std::fs::write(project_path.join("README.md"), "existing").unwrap();
        let mut payload = create_payload("notes", &project_path, "docs");
        payload.allow_existing = true;
        let mut cache = AgentProjectCache::default();

        let result = create_agent_project(&cfg, &mut cache, payload);

        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            std::fs::read_to_string(project_path.join("README.md")).unwrap(),
            "existing"
        );
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("left unchanged")));
        assert!(project_path.join("docs/index.md").exists());
    }

    #[test]
    fn agent_register_capabilities_enable_project_workflow() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let capabilities = agent_register_capabilities(&cfg);
        assert!(capabilities.project_create);
        assert!(capabilities.project_workflow);
    }

    #[test]
    fn project_workflow_snapshot_returns_git_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        init_clean_git_repo(&project_path);
        write_project_registry(
            &projects_dir,
            "foo",
            &project_path,
            false,
            hooks(&[("precommit", &["true"])]),
        );
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let result =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("foo", "snapshot"));

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.mode, "snapshot");
        assert!(result.git_before.available);
        assert!(result.git_after.available);
        assert_eq!(result.project.unwrap().id, "foo");
    }

    #[test]
    fn project_workflow_missing_project_is_structured_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let mut cache = AgentProjectCache::default();

        let result =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("missing", "snapshot"));

        assert!(!result.success);
        assert_eq!(
            result.error.as_deref(),
            Some("agent project 'missing' is not configured")
        );
        assert_eq!(
            result.recommended_next_action,
            "Create or configure the agent project"
        );
    }

    #[test]
    fn project_workflow_disabled_project_is_structured_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        std::fs::create_dir_all(&project_path).unwrap();
        write_project_registry(&projects_dir, "foo", &project_path, true, HashMap::new());
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let result =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("foo", "snapshot"));

        assert!(!result.success);
        assert_eq!(
            result.error.as_deref(),
            Some("agent project 'foo' is disabled")
        );
    }

    #[test]
    fn project_workflow_missing_precommit_is_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        init_clean_git_repo(&project_path);
        write_project_registry(&projects_dir, "foo", &project_path, false, HashMap::new());
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let result =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("foo", "precommit"));

        assert!(!result.success);
        assert_eq!(
            result.error.as_deref(),
            Some("agent project hook 'precommit' is not configured")
        );
    }

    #[test]
    fn project_workflow_hook_failure_keeps_git_snapshots() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        init_clean_git_repo(&project_path);
        write_project_registry(
            &projects_dir,
            "foo",
            &project_path,
            false,
            hooks(&[("doctor", &["false"])]),
        );
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();
        let mut payload = workflow_payload("foo", "hook");
        payload.hook = Some("doctor".to_string());

        let result = run_agent_project_workflow(&cfg, &mut cache, payload);

        assert!(!result.success);
        assert_eq!(result.recommended_next_action, "Fix failing hook step");
        assert!(result.git_before.available);
        assert!(result.git_after.available);
        let hook_result = result.hook_result.unwrap();
        assert!(!hook_result.success);
        assert_eq!(hook_result.steps.len(), 1);
        assert!(!hook_result.steps[0].success);
    }

    #[test]
    fn project_workflow_hook_success_returns_success() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        init_clean_git_repo(&project_path);
        write_project_registry(
            &projects_dir,
            "foo",
            &project_path,
            false,
            hooks(&[("doctor", &["true"])]),
        );
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();
        let mut payload = workflow_payload("foo", "hook");
        payload.hook = Some("doctor".to_string());

        let result = run_agent_project_workflow(&cfg, &mut cache, payload);

        assert!(result.success, "{:?}", result.error);
        assert!(result.hook_result.unwrap().success);
    }

    #[test]
    fn project_workflow_recommended_next_action_handles_clean_and_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("config/projects.d");
        let project_path = tmp.path().join("work/foo");
        init_clean_git_repo(&project_path);
        write_project_registry(
            &projects_dir,
            "foo",
            &project_path,
            false,
            hooks(&[("precommit", &["true"])]),
        );
        let cfg = test_config(projects_dir);
        let mut cache = AgentProjectCache::default();

        let clean =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("foo", "snapshot"));
        assert!(clean.success);
        assert_eq!(clean.recommended_next_action, "No changes detected");

        std::fs::write(project_path.join("dirty.txt"), "dirty\n").unwrap();
        let dirty =
            run_agent_project_workflow(&cfg, &mut cache, workflow_payload("foo", "snapshot"));
        assert!(dirty.success);
        assert_eq!(
            dirty.recommended_next_action,
            "Run precommit hook before committing"
        );
    }
}
