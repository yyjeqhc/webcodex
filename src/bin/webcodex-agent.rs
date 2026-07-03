use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

#[allow(dead_code)]
#[path = "../agent_init.rs"]
mod agent_init;

#[path = "../build_info.rs"]
mod build_info;

mod webcodex_agent;

use shell_protocol::{
    AgentPolicySummary, ShellAgentJobUpdateRequest, ShellAgentPollRequest, ShellAgentPollResponse,
    ShellAgentProjectSummary, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellClientRegisterResponse, ShellProfileSummaryEntry,
    ShellProfilesSummary, AGENT_PROTOCOL_VERSION_POLLING_V1,
};

// Shared agent-config initialization (types, validation, TOML generation,
// 0600 file writing, HOME-default allowed_roots). Reused by `webcodex-cli`.
use agent_init::{
    parse_bool, required_value, run_agent_init, validate_agent_init_options, AgentInitOptions,
    DEFAULT_INIT_PROJECTS_DIR, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_POLL_INTERVAL_MS,
    TRANSPORT_WEBSOCKET,
};
#[cfg(test)]
use agent_init::{TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC};
#[cfg(test)]
use shell_protocol::{
    AgentEnvelope, AGENT_PROTOCOL_VERSION_QUIC_V1, AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::net::SocketAddr;
#[cfg(test)]
use webcodex_agent::QuicClientConfig;
#[cfg(test)]
use webcodex_agent::{
    agent_project_summary, auto_transport_plan, build_ws_request, default_quic_alpn,
    default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs, effective_transport,
    load_agent_project_summaries_from_dir, max_concurrent_jobs, non_empty_token,
    parse_agent_project_toml, quic_client_bind_addr_for, resolve_quic_config,
    resolve_quic_server_addrs, server_url_to_ws, validate_project_path_policy, websocket_session,
    CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS, WS_OUTGOING_CAPACITY,
};
use webcodex_agent::{
    client_profile_agent_config, default_config_path, err_cmd, find_project_shell_context,
    handle_project_op, hostname, line_edit_stdout, load_config, ok_cmd, projects_dir, run_agent,
    validate_client_profile, validate_shell_config, AgentConfig, AgentPolicy, AgentProjectCache,
    AgentSink, CommandResult, HttpSendConfig, ShellConfig, ShellProfileConfig,
};

const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const SHELL_PROFILE_PREPARE_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreparedShellProfileKey {
    project_key: String,
    profile_name: String,
}

#[derive(Debug, Clone)]
struct PreparedShellProfile {
    profile_name: String,
    program: String,
    args: Vec<String>,
    env_snapshot: HashMap<String, String>,
}

/// Lazily prepared shell environment snapshots. Snapshots are keyed by
/// project/cwd plus profile name because inline init scripts such as
/// `. .venv/bin/activate` are intentionally resolved from the project cwd.
/// Profile config changes require restarting the agent in this phase.
#[derive(Debug, Clone, Default)]
struct PreparedShellProfileCache {
    profiles: Arc<Mutex<HashMap<PreparedShellProfileKey, Arc<PreparedShellProfile>>>>,
}

#[derive(Debug, Clone)]
struct JobManager {
    max_concurrent: usize,
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    queued: Arc<
        Mutex<
            VecDeque<(
                AgentSink,
                AgentPolicy,
                ShellConfig,
                PathBuf,
                ShellAgentShellRequest,
            )>,
        >,
    >,
    prepared_profiles: PreparedShellProfileCache,
}

impl JobManager {
    fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            queued: Arc::new(Mutex::new(VecDeque::new())),
            prepared_profiles: PreparedShellProfileCache::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct RunningJob {
    client_id: String,
    child: Option<Arc<Mutex<Child>>>,
    stop_requested: Arc<AtomicBool>,
}

#[derive(Debug)]
enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentCliAction {
    Run {
        config_path: PathBuf,
        once: bool,
    },
    Init(AgentInitOptions),
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn usage() -> &'static str {
    "Usage: webcodex-agent [--config PATH] [--once]\n\
       webcodex-agent init --server-url URL [--token TOKEN|--token-file PATH] --client-id ID --owner USER --output PATH [--allowed-root PATH...]\n\n\
     Options:\n\
       -h, --help                 Print help and exit\n\
       -V, --version              Print version and exit\n\
       -c, --config PATH          Agent config path for normal runtime\n\
       --profile NAME             Client config profile for default config path\n\
       --once                     Poll once, then exit (polling transport)\n\n\
     With --profile, the default config path is derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit\n\
     --config overrides the profile-derived default.\n\n\
     Init options:\n\
       --server-url URL           WebCodex server URL\n\
       --token TOKEN              Agent token for generated config\n\
       --token-file PATH          Read agent token from file\n\
       --client-id ID             Stable agent client id\n\
       --owner USER               Owner username\n\
       --display-name NAME        Human-readable agent name\n\
       --transport NAME           websocket (default), polling, quic, or auto\n\
       --poll-interval-ms N       Polling interval, default 1000\n\
       --projects-dir PATH        Project config directory, default /etc/webcodex/projects.d\n\
       --allowed-root PATH        Allowed project/root path; repeatable\n\
       --allow-cwd-anywhere BOOL  Allow cwd outside allowed_roots; default false\n\
       --output PATH|-            Output config path, or '-' for stdout\n\
       --overwrite                Replace an existing output file\n\n\
     Environment:\n\
       WEBCODEX_AGENT_CONFIG      default config path override\n\
       WEBCODEX_AGENT_TOKEN       token fallback for init\n\
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

fn parse_args() -> Result<AgentCliAction, String> {
    parse_agent_args(std::env::args().skip(1))
}

fn parse_agent_args<I, S>(args: I) -> Result<AgentCliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    if args.len() == 1 {
        match args[0].as_str() {
            "--help" | "-h" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-agent"),
                    stderr: String::new(),
                });
            }
            _ => {}
        }
    }
    if args.first().is_some_and(|arg| arg == "init") {
        if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
            return Ok(AgentCliAction::Exit {
                code: 0,
                stdout: usage().to_string(),
                stderr: String::new(),
            });
        }
        return parse_agent_init_args(&args[1..]).map(AgentCliAction::Init);
    }

    let mut config_path = std::env::var("WEBCODEX_AGENT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_config_path());
    let mut config_explicit = false;
    let mut profile: Option<String> = None;
    let mut once = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-agent"),
                    stderr: String::new(),
                });
            }
            "--once" => once = true,
            "--config" | "-c" => {
                let Some(path) = args.next() else {
                    return Err("--config requires a path".to_string());
                };
                config_path = PathBuf::from(path);
                config_explicit = true;
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    return Err("--profile requires a value".to_string());
                };
                profile = Some(value);
            }
            _ => return Err(format!("unknown argument: {}\n{}", arg, usage())),
        }
    }
    if let Some(profile) = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?
    {
        if !config_explicit {
            config_path = client_profile_agent_config(&profile);
        }
    }
    Ok(AgentCliAction::Run { config_path, once })
}

fn parse_agent_init_args(args: &[String]) -> Result<AgentInitOptions, String> {
    let mut opts = AgentInitOptions {
        server_url: String::new(),
        token: None,
        token_file: None,
        client_id: String::new(),
        owner: String::new(),
        display_name: None,
        transport: TRANSPORT_WEBSOCKET.to_string(),
        poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        projects_dir: PathBuf::from(DEFAULT_INIT_PROJECTS_DIR),
        output: PathBuf::new(),
        allowed_roots: Vec::new(),
        allow_cwd_anywhere: false,
        overwrite: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = required_value(&mut iter, arg)?,
            "--token" => opts.token = Some(required_value(&mut iter, arg)?),
            "--token-file" => {
                opts.token_file = Some(PathBuf::from(required_value(&mut iter, arg)?))
            }
            "--client-id" => opts.client_id = required_value(&mut iter, arg)?,
            "--owner" => opts.owner = required_value(&mut iter, arg)?,
            "--display-name" => opts.display_name = Some(required_value(&mut iter, arg)?),
            "--transport" => opts.transport = required_value(&mut iter, arg)?,
            "--poll-interval-ms" => {
                let value = required_value(&mut iter, arg)?;
                opts.poll_interval_ms = value
                    .parse::<u64>()
                    .map_err(|_| "--poll-interval-ms must be an integer".to_string())?;
            }
            "--projects-dir" => opts.projects_dir = PathBuf::from(required_value(&mut iter, arg)?),
            "--allowed-root" => opts
                .allowed_roots
                .push(PathBuf::from(required_value(&mut iter, arg)?)),
            "--allow-cwd-anywhere" => {
                opts.allow_cwd_anywhere = parse_bool(&required_value(&mut iter, arg)?)?;
            }
            "--output" => opts.output = PathBuf::from(required_value(&mut iter, arg)?),
            "--overwrite" => opts.overwrite = true,
            "--help" | "-h" => return Err(usage().to_string()),
            _ => return Err(format!("unknown init flag: {}", arg)),
        }
    }
    validate_agent_init_options(&opts)?;
    Ok(opts)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn should_inherit_env_key(key: &str) -> bool {
    !matches!(
        key,
        "WEBCODEX_TOKEN" | "WEBCODEX_AGENT_TOKEN" | "WEBCODEX_USER_TOKEN" | "AUTHORIZATION"
    )
}

fn shell_command_text(shell: &ShellConfig, command: &str) -> String {
    match shell.init_script.as_ref() {
        Some(path) => format!(
            ". {} && (\n{}\n)",
            shell_quote(&path.to_string_lossy()),
            command
        ),
        None => command.to_string(),
    }
}

fn apply_shell_environment(cmd: &mut Command, shell: &ShellConfig) -> Result<(), String> {
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
        cmd.env_remove(key);
    }
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        cmd.env("PATH", joined);
    }
    for (key, value) in &shell.env {
        cmd.env(key, value);
    }
    Ok(())
}

fn apply_env_snapshot(cmd: &mut Command, env_snapshot: &HashMap<String, String>) {
    cmd.env_clear();
    for (key, value) in env_snapshot {
        cmd.env(key, value);
    }
}

fn configured_shell_command(shell: &ShellConfig, command: &str) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let mut cmd = Command::new(&shell.program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, command));
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

fn configured_prepared_shell_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(command);
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

fn configured_shell_job_command(shell: &ShellConfig, command: &str) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let mut cmd = Command::new("setsid");
    cmd.arg(&shell.program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, command));
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

fn configured_prepared_shell_job_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new("setsid");
    cmd.arg(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(command);
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

fn base_shell_env(
    shell: &ShellConfig,
    profile: &ShellProfileConfig,
) -> Result<HashMap<String, String>, String> {
    let mut env: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| should_inherit_env_key(key))
        .collect();
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        if let Some(current) = env.get("PATH") {
            paths.extend(std::env::split_paths(current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        env.insert("PATH".to_string(), joined.to_string_lossy().to_string());
    }
    for (key, value) in &shell.env {
        env.insert(key.clone(), value.clone());
    }
    for (key, value) in &profile.env {
        env.insert(key.clone(), value.clone());
    }
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
        env.remove(key);
    }
    Ok(env)
}

fn stderr_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    const MAX_ERR: usize = 4096;
    if text.len() <= MAX_ERR {
        return text;
    }
    let mut start = text.len() - MAX_ERR;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!("[stderr truncated]\n{}", &text[start..])
}

fn run_prepare_command(
    mut cmd: Command,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn profile prepare command: {}", e))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "profile prepare stdout pipe missing".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "profile prepare stderr pipe missing".to_string())?;
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare stdout: {}", e))
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stderr
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare stderr: {}", e))
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _stdout = stdout_handle
                        .join()
                        .map_err(|_| "profile prepare stdout reader panicked".to_string())??;
                    let stderr = stderr_handle
                        .join()
                        .map_err(|_| "profile prepare stderr reader panicked".to_string())??;
                    return Err(format!(
                        "profile prepare timed out after {} seconds; stderr tail: {}",
                        timeout.as_secs(),
                        stderr_tail(&stderr)
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed to wait profile prepare command: {}", e));
            }
        }
    };
    let stdout = stdout_handle
        .join()
        .map_err(|_| "profile prepare stdout reader panicked".to_string())??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| "profile prepare stderr reader panicked".to_string())??;
    Ok((status, stdout, stderr))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_env_payload(
    payload: &[u8],
    profile_name: &str,
) -> Result<HashMap<String, String>, String> {
    let mut env = HashMap::new();
    for entry in payload.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': entry missing '='",
                profile_name
            ));
        };
        let key = std::str::from_utf8(&entry[..eq]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': key is not UTF-8",
                profile_name
            )
        })?;
        if key.is_empty() {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': empty env key",
                profile_name
            ));
        }
        let value = std::str::from_utf8(&entry[eq + 1..]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': value is not UTF-8",
                profile_name
            )
        })?;
        if should_inherit_env_key(key) {
            env.insert(key.to_string(), value.to_string());
        }
    }
    Ok(env)
}

fn capture_profile_env_snapshot(
    profile_name: &str,
    profile: &ShellProfileConfig,
    program: &str,
    args: &[String],
    prepare_cwd: &Path,
    initial_env: HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    let Some(init_script) = profile.init_script.as_deref() else {
        return Ok(initial_env);
    };
    let marker = format!("__WEBCODEX_ENV_START_{}__", uuid::Uuid::new_v4().simple());
    let prepare_script = format!(
        "set -e\n{}\nprintf '\\n{}\\n'\nenv -0\n",
        init_script, marker
    );
    let mut cmd = Command::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(prepare_script).current_dir(prepare_cwd).env_clear();
    for (key, value) in initial_env {
        cmd.env(key, value);
    }
    let (status, stdout, stderr) =
        run_prepare_command(cmd, Duration::from_secs(SHELL_PROFILE_PREPARE_TIMEOUT_SECS)).map_err(
            |e| {
                format!(
                    "failed to prepare shell profile '{}' at {}: {}",
                    profile_name,
                    prepare_cwd.display(),
                    e
                )
            },
        )?;
    if !status.success() {
        return Err(format!(
            "failed to prepare shell profile '{}' at {}: exit code {}; stderr tail: {}",
            profile_name,
            prepare_cwd.display(),
            status.code().unwrap_or(-1),
            stderr_tail(&stderr)
        ));
    }
    let marker_pos = find_bytes(&stdout, marker.as_bytes()).ok_or_else(|| {
        format!(
            "failed to prepare shell profile '{}' at {}: env marker not found",
            profile_name,
            prepare_cwd.display()
        )
    })?;
    let mut payload_start = marker_pos + marker.len();
    while stdout
        .get(payload_start)
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        payload_start += 1;
    }
    let mut snapshot = parse_env_payload(&stdout[payload_start..], profile_name)?;
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
        snapshot.remove(key);
    }
    Ok(snapshot)
}

impl PreparedShellProfileCache {
    /// Number of currently prepared snapshots. Used only for the sanitized
    /// observability summary; never exposes snapshot contents.
    fn len(&self) -> usize {
        self.profiles.lock().unwrap().len()
    }

    fn get_or_prepare(
        &self,
        shell: &ShellConfig,
        profile_name: &str,
        project_key: String,
        prepare_cwd: &Path,
    ) -> Result<Arc<PreparedShellProfile>, String> {
        let key = PreparedShellProfileKey {
            project_key,
            profile_name: profile_name.to_string(),
        };
        if let Some(prepared) = self.profiles.lock().unwrap().get(&key).cloned() {
            return Ok(prepared);
        }
        let profile = shell.profiles.get(profile_name).ok_or_else(|| {
            format!(
                "shell profile '{}' is not configured for project/cwd {}",
                profile_name,
                prepare_cwd.display()
            )
        })?;
        let program = profile
            .program
            .clone()
            .unwrap_or_else(|| shell.program.clone());
        let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
        let initial_env = base_shell_env(shell, profile)?;
        let env_snapshot = capture_profile_env_snapshot(
            profile_name,
            profile,
            &program,
            &args,
            prepare_cwd,
            initial_env,
        )?;
        let prepared = Arc::new(PreparedShellProfile {
            profile_name: profile_name.to_string(),
            program,
            args,
            env_snapshot,
        });
        self.profiles.lock().unwrap().insert(key, prepared.clone());
        Ok(prepared)
    }
}

fn shell_profile_project_key(project_id: Option<&str>, path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    match project_id {
        Some(id) => format!("project:{}:{}", id, path),
        None => format!("cwd:{}", path),
    }
}

fn resolve_prepared_shell_profile(
    shell: &ShellConfig,
    projects_dir: &Path,
    cwd_path: &Path,
    request_has_cwd: bool,
    cache: &PreparedShellProfileCache,
) -> Result<Option<Arc<PreparedShellProfile>>, String> {
    let project = request_has_cwd
        .then(|| find_project_shell_context(projects_dir, cwd_path))
        .flatten();
    let profile_name = project
        .as_ref()
        .and_then(|project| project.shell_profile.as_deref())
        .or(shell.default_profile.as_deref());
    let Some(profile_name) = profile_name else {
        return Ok(None);
    };
    let prepare_cwd = project
        .as_ref()
        .map(|project| PathBuf::from(&project.path))
        .unwrap_or_else(|| cwd_path.to_path_buf());
    if let Some(project) = &project {
        if project.shell_profile.as_deref() == Some(profile_name)
            && !shell.profiles.contains_key(profile_name)
        {
            return Err(format!(
                "project '{}' shell_profile '{}' does not match any shell.profiles entry",
                project.id, profile_name
            ));
        }
    }
    let project_key = shell_profile_project_key(
        project.as_ref().map(|project| project.id.as_str()),
        &prepare_cwd,
    );
    cache
        .get_or_prepare(shell, profile_name, project_key, &prepare_cwd)
        .map(Some)
}

fn endpoint(cfg: &AgentConfig, path: &str) -> String {
    format!("{}{}", cfg.server_url.trim_end_matches('/'), path)
}

fn post_json<T, R>(client: &Client, cfg: &AgentConfig, path: &str, body: &T) -> Result<R, String>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let mut req = client.post(endpoint(cfg, path));
    if !cfg.token.trim().is_empty() {
        req = req.bearer_auth(cfg.token.trim());
    }
    let resp = req
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
    agent_instance_id: &str,
    prepared_cache_count: usize,
) -> ShellClientRegisterRequest {
    let capabilities = agent_register_capabilities(cfg);
    ShellClientRegisterRequest {
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
        display_name: cfg.display_name.clone(),
        owner: cfg.owner.clone(),
        hostname: cfg.hostname.clone().or_else(hostname),
        capabilities: Some(capabilities),
        projects: Some(projects),
        agent_protocol_version: Some(protocol_version.to_string()),
        policy: Some(register_policy_summary(cfg, prepared_cache_count)),
    }
}

/// Build the sanitized shell-profiles summary from the static shell config.
/// Exposes only safe metadata: profile names, whether each has an init_script
/// (boolean, never the body), env key counts (never values), the resolved
/// program, and arg counts. `prepared_cache_count` is the number of snapshots
/// prepared at call time (typically 0 right after start). Never includes env
/// values, init_script bodies, tokens, or the full env snapshot.
fn build_shell_profiles_summary(
    shell: &ShellConfig,
    prepared_cache_count: usize,
) -> ShellProfilesSummary {
    let profiles: Vec<ShellProfileSummaryEntry> = shell
        .profiles
        .iter()
        .map(|(name, profile)| {
            let program = profile
                .program
                .clone()
                .unwrap_or_else(|| shell.program.clone());
            let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
            ShellProfileSummaryEntry {
                name: name.clone(),
                has_init_script: profile.init_script.is_some(),
                env_keys_count: profile.env.len(),
                program,
                args_count: args.len(),
            }
        })
        .collect();
    ShellProfilesSummary {
        default_profile: shell.default_profile.clone(),
        configured_count: shell.profiles.len(),
        prepared_cache_count,
        profiles,
    }
}

/// Build the sanitized agent policy summary sent at registration. Mirrors the
/// local `AgentPolicy` but only carries non-secret fields. The shell env
/// values and init_script path are intentionally NOT included. The sanitized
/// shell-profiles summary is attached so observability can show which profile
/// a project resolves to without exposing env values or init_script bodies.
fn register_policy_summary(cfg: &AgentConfig, prepared_cache_count: usize) -> AgentPolicySummary {
    AgentPolicySummary {
        allow_raw_shell: cfg.policy.allow_raw_shell,
        allow_cwd_anywhere: cfg.policy.allow_cwd_anywhere,
        allowed_roots: cfg.policy.allowed_roots.clone(),
        max_timeout_secs: cfg.policy.max_timeout_secs,
        max_output_bytes: cfg.policy.max_output_bytes,
        shell_profiles: Some(build_shell_profiles_summary(
            &cfg.shell,
            prepared_cache_count,
        )),
    }
}

fn register(
    client: &Client,
    cfg: &AgentConfig,
    project_cache: &mut AgentProjectCache,
    agent_instance_id: &str,
    prepared_cache_count: usize,
) -> Result<(), String> {
    let body = build_register_request(
        cfg,
        project_cache.get(cfg),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        agent_instance_id,
        prepared_cache_count,
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

// Test-only wrapper for callers that do not need prepared shell profiles; the
// production request path uses `run_shell_with_profiles` directly.
#[cfg(test)]
fn run_shell(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_impl(
        policy,
        shell,
        None,
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
    )
}

fn run_shell_with_profiles(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_impl(
        policy,
        shell,
        Some((projects_dir, cache)),
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
    )
}

fn run_shell_impl(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    profiles: Option<(&Path, &PreparedShellProfileCache)>,
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
    let mut prepared_profile_name = None;
    let mut cmd = match profiles {
        Some((projects_dir, cache)) => {
            match resolve_prepared_shell_profile(
                shell,
                projects_dir,
                &cwd_path,
                cwd.is_some(),
                cache,
            ) {
                Ok(Some(profile)) => match configured_prepared_shell_command(&profile, command) {
                    Ok(cmd) => {
                        prepared_profile_name = Some(profile.profile_name.clone());
                        cmd
                    }
                    Err(e) => {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to configure shell profile '{}': {}",
                                profile.profile_name, e
                            )),
                        };
                    }
                },
                Ok(None) => match configured_shell_command(shell, command) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(e),
                        };
                    }
                },
                Err(e) => {
                    return CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(e),
                    };
                }
            }
        }
        None => match configured_shell_command(shell, command) {
            Ok(cmd) => cmd,
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(e),
                };
            }
        },
    };
    cmd.current_dir(&cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let spawn = cmd.spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            let error = prepared_profile_name
                .as_deref()
                .map(|profile_name| {
                    format!("failed to spawn shell profile '{}': {}", profile_name, e)
                })
                .unwrap_or_else(|| format!("failed to spawn command: {}", e));
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
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

fn is_line_edit_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "file_replace_line_range"
            | "file_insert_at_line"
            | "file_delete_line_range"
            | "file_replace_exact_block"
            | "file_insert_before_pattern"
            | "file_insert_after_pattern"
            | "file_apply_text_edits"
    )
}

fn is_file_request_kind(kind: &str) -> bool {
    matches!(kind, "file_read" | "file_write" | "file_list") || is_line_edit_request_kind(kind)
}

fn is_sensitive_line_edit_path(path: &str) -> bool {
    let mut components = path.split('/');
    components.any(|component| {
        matches!(
            component,
            ".git" | ".env" | "agent.toml" | "projects.d" | "secrets" | "target" | "node_modules"
        ) || component.starts_with(".env.")
            || component.ends_with(".env")
            || component.ends_with(".toml.bak")
            || component == "webcodex.env"
    })
}

fn validate_line_edit_agent_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let raw = Path::new(path);
    if raw.is_absolute() {
        return Err("line edit path must be project-relative".to_string());
    }
    for component in raw.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => return Err("line edit path must not escape the project".to_string()),
        }
    }
    if is_sensitive_line_edit_path(path) {
        return Err("refusing to edit sensitive path".to_string());
    }
    Ok(())
}

fn normalize_line_edit_text(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    }
}

fn line_edit_text_line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

fn write_file_atomic(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    let mut last_error = None;
    for attempt in 0..16 {
        let tmp = parent.join(format!(".pd-line-{}-{}", std::process::id(), attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                if let Err(e) = file.sync_all() {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                if let Err(e) = std::fs::rename(&tmp, path) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(e.to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(last_error.unwrap_or_else(|| "could not create temporary file".to_string()))
}

fn handle_line_edit_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let content = match std::fs::read(resolved) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "file is not valid UTF-8",
                    }),
                    start,
                )
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "file not found",
                }),
                start,
            )
        }
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": format!("read failed: {}", e),
                }),
                start,
            )
        }
    };
    if content.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "file contains NUL bytes",
            }),
            start,
        );
    }
    if request
        .content
        .as_deref()
        .map(|text| text.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "text cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .old_text
        .as_deref()
        .map(|old_text| old_text.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "old_text cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .pattern
        .as_deref()
        .map(|pattern| pattern.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "pattern cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .expected_prefix
        .as_deref()
        .map(|prefix| prefix.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "expected prefix cannot contain NUL bytes",
            }),
            start,
        );
    }

    let lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').collect()
    };
    let total_lines = lines.len();
    let edit = match request.kind.as_str() {
        "file_replace_line_range" | "file_delete_line_range" => {
            let start_line = request.start_line.unwrap_or(0);
            let end_line = request.end_line.unwrap_or(0);
            if start_line == 0 || end_line < start_line || end_line > total_lines {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "start_line": start_line,
                        "end_line": end_line,
                        "error": "invalid line range",
                    }),
                    start,
                );
            }
            let old_text = lines[start_line - 1..end_line].concat();
            let replacement = if request.kind == "file_delete_line_range" {
                String::new()
            } else {
                normalize_line_edit_text(request.content.as_deref().unwrap_or_default())
            };
            let new_content = format!(
                "{}{}{}",
                lines[..start_line - 1].concat(),
                replacement,
                lines[end_line..].concat()
            );
            (
                old_text,
                new_content,
                end_line - start_line + 1,
                line_edit_text_line_count(&replacement),
                serde_json::json!({"start_line": start_line, "end_line": end_line}),
            )
        }
        "file_insert_at_line" => {
            let line = request.line.unwrap_or(0);
            if line == 0 || line > total_lines + 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "line": line,
                        "error": "line out of range",
                    }),
                    start,
                );
            }
            let old_text = if line <= total_lines {
                lines[line - 1].to_string()
            } else {
                String::new()
            };
            let insertion =
                normalize_line_edit_text(request.content.as_deref().unwrap_or_default());
            let new_content = format!(
                "{}{}{}",
                lines[..line - 1].concat(),
                insertion,
                lines[line - 1..].concat()
            );
            (
                old_text,
                new_content,
                if line <= total_lines { 1 } else { 0 },
                line_edit_text_line_count(&insertion),
                serde_json::json!({"line": line}),
            )
        }
        "file_replace_exact_block" => {
            let old = request.old_text.as_deref().unwrap_or_default();
            let new = request.content.as_deref().unwrap_or_default();
            if old.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "old_text must be non-empty",
                    }),
                    start,
                );
            }
            let before_sha256 = sha256_hex_bytes(content.as_bytes());
            if let Some(expected) = request.expected_sha256.as_deref() {
                if before_sha256 != expected {
                    return line_edit_stdout(
                        serde_json::json!({
                            "changed": false,
                            "path": path,
                            "before_sha256": before_sha256,
                            "error": "Rejected before write: expected_old_sha256 mismatch.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with the current file sha256.",
                        }),
                        start,
                    );
                }
            }
            let matches = content.matches(old).count();
            if matches == 0 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "matches_replaced": 0,
                        "error": format!("Rejected before write: old_text was not found exactly once in path {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact block.", path),
                    }),
                    start,
                );
            }
            if matches > 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "matches_replaced": 0,
                        "error": format!("Rejected before write: old_text matched {} times in path {}; expected exactly one match.\nNo files were modified.\nRetry guidance: make old_text more specific or use replace_line_range with guards.", matches, path),
                    }),
                    start,
                );
            }
            (
                old.to_string(),
                content.replacen(old, new, 1),
                1,
                1,
                serde_json::json!({"matches_replaced": 1}),
            )
        }
        "file_insert_before_pattern" | "file_insert_after_pattern" => {
            let pattern = request.pattern.as_deref().unwrap_or_default();
            let text = request.content.as_deref().unwrap_or_default();
            if pattern.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "pattern must be non-empty literal pattern",
                    }),
                    start,
                );
            }
            if text.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "Rejected before write: inserted text must not be empty.\nNo files were modified.\nRetry guidance: provide the exact text to insert, including any intended newlines.",
                    }),
                    start,
                );
            }
            let matches = content.matches(pattern).count();
            if matches == 0 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "pattern_matches": 0,
                        "error": format!("Rejected before write: pattern was not found exactly once in path {}.\nNo files were modified.\nRetry guidance: read the file again and retry with a more specific literal pattern.", path),
                    }),
                    start,
                );
            }
            if matches > 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "pattern_matches": matches,
                        "error": format!("Rejected before write: pattern matched {} times in path {}; expected exactly one match.\nNo files were modified.\nRetry guidance: use a more specific literal pattern or use insert_at_line with guards.", matches, path),
                    }),
                    start,
                );
            }
            let idx = content.find(pattern).unwrap_or(0);
            let insert_at = if request.kind == "file_insert_after_pattern" {
                idx + pattern.len()
            } else {
                idx
            };
            let mut new_content = String::with_capacity(content.len() + text.len());
            new_content.push_str(&content[..insert_at]);
            new_content.push_str(text);
            new_content.push_str(&content[insert_at..]);
            (
                pattern.to_string(),
                new_content,
                1,
                1,
                serde_json::json!({"pattern_matches": 1}),
            )
        }
        _ => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "invalid operation",
                }),
                start,
            )
        }
    };

    let (old_text, new_content, old_line_count, new_line_count, coords) = edit;
    let old_sha256 = sha256_hex_bytes(old_text.as_bytes());
    let selected_text_sha_guard_applies = request.kind != "file_replace_exact_block";
    if selected_text_sha_guard_applies {
        if let Some(expected) = request.expected_sha256.as_deref() {
            if old_sha256 != expected {
                let err = if request.kind == "file_insert_at_line" {
                    "expected_anchor_sha256 mismatch"
                } else {
                    "expected_old_sha256 mismatch"
                };
                let mut out = serde_json::json!({
                    "changed": false,
                    "path": path,
                    "old_sha256": old_sha256,
                    "error": err,
                });
                merge_json_object(&mut out, coords.clone());
                return line_edit_stdout(out, start);
            }
        }
    }
    if let Some(prefix) = request.expected_prefix.as_deref() {
        if !old_text.starts_with(prefix) {
            let err = if request.kind == "file_insert_at_line" {
                "expected_anchor_prefix mismatch"
            } else {
                "expected_old_prefix mismatch"
            };
            let mut out = serde_json::json!({
                "changed": false,
                "path": path,
                "old_sha256": old_sha256,
                "error": err,
            });
            merge_json_object(&mut out, coords.clone());
            return line_edit_stdout(out, start);
        }
    }
    if let Err(e) = write_file_atomic(resolved, &new_content) {
        let mut out = serde_json::json!({
            "changed": false,
            "path": path,
            "old_sha256": old_sha256,
            "error": format!("write failed: {}", e),
        });
        merge_json_object(&mut out, coords.clone());
        return line_edit_stdout(out, start);
    }
    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let mut out = serde_json::json!({
        "path": path,
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "before_sha256": sha256_hex_bytes(content.as_bytes()),
        "after_sha256": new_sha256,
        "old_line_count": old_line_count,
        "new_line_count": new_line_count,
        "bytes_before": content.len(),
        "bytes_after": new_content.len(),
        "bytes_written": new_content.len(),
        "changed": new_content != content,
    });
    merge_json_object(&mut out, coords);
    line_edit_stdout(out, start)
}

/// Maximum file size accepted by `file_apply_text_edits` on the agent side.
const APPLY_TEXT_EDITS_MAX_FILE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
/// Maximum number of edits in one `file_apply_text_edits` batch.
const APPLY_TEXT_EDITS_MAX_EDITS: usize = 20;
/// Maximum byte size of a single edit field on the agent side.
const APPLY_TEXT_EDITS_MAX_FIELD_BYTES: usize = 512 * 1024; // 512 KiB

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentTextEditKind {
    ReplaceExact,
    InsertAfter,
    InsertBefore,
    DeleteExact,
}

impl AgentTextEditKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ReplaceExact => "replace_exact",
            Self::InsertAfter => "insert_after",
            Self::InsertBefore => "insert_before",
            Self::DeleteExact => "delete_exact",
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentTextEdit {
    kind: AgentTextEditKind,
    #[serde(default)]
    old_text: Option<String>,
    #[serde(default)]
    new_text: Option<String>,
    #[serde(default)]
    anchor_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentApplyTextEditsPayload {
    edits: Vec<AgentTextEdit>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    expected_file_sha256: Option<String>,
}

fn handle_apply_text_edits_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload_json = request.content.as_deref().unwrap_or_default();
    let payload: AgentApplyTextEditsPayload = match serde_json::from_str(payload_json) {
        Ok(p) => p,
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": format!("invalid edits payload: {}", e),
                }),
                start,
            );
        }
    };
    let dry_run = payload.dry_run.unwrap_or(false);
    if payload.edits.is_empty() {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "edits must contain at least one edit",
            }),
            start,
        );
    }
    if payload.edits.len() > APPLY_TEXT_EDITS_MAX_EDITS {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": format!(
                    "too many edits; maximum is {}",
                    APPLY_TEXT_EDITS_MAX_EDITS
                ),
            }),
            start,
        );
    }

    // Read + UTF-8 validate the original file.
    let bytes = match std::fs::read(resolved) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "file not found",
                }),
                start,
            );
        }
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": format!("read failed: {}", e),
                }),
                start,
            );
        }
    };
    if bytes.len() > APPLY_TEXT_EDITS_MAX_FILE_BYTES {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": format!(
                    "file too large; maximum is {} bytes",
                    APPLY_TEXT_EDITS_MAX_FILE_BYTES
                ),
            }),
            start,
        );
    }
    let original = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "file is not valid UTF-8",
                }),
                start,
            );
        }
    };
    if original.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "file contains NUL bytes",
            }),
            start,
        );
    }

    let old_sha256 = sha256_hex_bytes(original.as_bytes());
    if let Some(expected) = payload.expected_file_sha256.as_deref() {
        if old_sha256 != expected {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "dry_run": dry_run,
                    "old_sha256": old_sha256,
                    "error": "Rejected before write: expected_file_sha256 mismatch.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with the current file sha256.",
                }),
                start,
            );
        }
    }

    // Resolve each edit to (start, end, replacement, index) against original.
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(payload.edits.len());
    for (index, edit) in payload.edits.iter().enumerate() {
        let kind = &edit.kind;
        let (needle, replacement): (&str, String) = match kind {
            AgentTextEditKind::ReplaceExact => {
                let old = match edit.old_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "old_text must be non-empty",
                            start,
                        );
                    }
                };
                let new = edit.new_text.clone().unwrap_or_default();
                (old, new)
            }
            AgentTextEditKind::DeleteExact => {
                let old = match edit.old_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "old_text must be non-empty",
                            start,
                        );
                    }
                };
                (old, String::new())
            }
            AgentTextEditKind::InsertBefore | AgentTextEditKind::InsertAfter => {
                let anchor = match edit.anchor_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "anchor_text must be non-empty",
                            start,
                        );
                    }
                };
                let new = match edit.new_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v.to_string(),
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "new_text must be non-empty",
                            start,
                        );
                    }
                };
                (anchor, new)
            }
        };
        if needle.contains('\0') {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                "match text cannot contain NUL bytes",
                start,
            );
        }
        if needle.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
            || replacement.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
        {
            return apply_text_edits_error(path, index, kind.as_str(), "field too large", start);
        }
        let matches = original.matches(needle).count();
        if matches == 0 {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                "match text was not found",
                start,
            );
        }
        if matches > 1 {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                &format!(
                    "match text matched {} times; refusing ambiguous edit",
                    matches
                ),
                start,
            );
        }
        let start_off = original.find(needle).expect("unique match already counted");
        let end_off = start_off + needle.len();
        let (range_start, range_end) = match kind {
            AgentTextEditKind::InsertBefore => (start_off, start_off),
            AgentTextEditKind::InsertAfter => (end_off, end_off),
            _ => (start_off, end_off),
        };
        ops.push((range_start, range_end, replacement, index));
    }

    ops.sort_by_key(|&(s, e, _, i)| (s, e, i));
    for w in ops.windows(2) {
        let (_, e1, _, _) = w[0];
        let (s2, _, _, _) = w[1];
        if s2 < e1 {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "dry_run": dry_run,
                    "error": "Rejected before write: edits overlap; refusing ambiguous atomic edit batch.\nNo files were modified.\nRetry guidance: read the file again and ensure edit match ranges do not overlap.",
                }),
                start,
            );
        }
    }

    // Build the new content by slicing the original at op boundaries.
    let mut new_content = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut edit_summaries: Vec<serde_json::Value> = Vec::with_capacity(ops.len());
    for &(start_off, end_off, ref replacement, index) in &ops {
        new_content.push_str(&original[cursor..start_off]);
        new_content.push_str(replacement);
        cursor = end_off;
        let edit = &payload.edits[index];
        let old_start_line = 1 + original[..start_off].matches('\n').count();
        let mut old_end_line = 1 + original[..end_off].matches('\n').count();
        if end_off > start_off
            && end_off <= original.len()
            && original.as_bytes()[end_off - 1] == b'\n'
        {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end_off == start_off {
            old_end_line = old_start_line;
        }
        let new_line_count = if replacement.is_empty() {
            0
        } else {
            replacement.lines().count()
        };
        edit_summaries.push(serde_json::json!({
            "index": index,
            "kind": edit.kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": new_line_count,
        }));
    }
    new_content.push_str(&original[cursor..]);

    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let changed = new_content != original;

    if dry_run {
        return line_edit_stdout(
            serde_json::json!({
                "path": path,
                "dry_run": true,
                "applied_count": payload.edits.len(),
                "old_sha256": old_sha256,
                "new_sha256": new_sha256,
                "changed": false,
                "would_change": changed,
                "edits": edit_summaries,
                "changed_paths": [path],
            }),
            start,
        );
    }

    if !changed {
        return line_edit_stdout(
            serde_json::json!({
                "path": path,
                "dry_run": false,
                "applied_count": payload.edits.len(),
                "old_sha256": old_sha256,
                "new_sha256": new_sha256,
                "changed": false,
                "would_change": false,
                "edits": edit_summaries,
                "changed_paths": [],
            }),
            start,
        );
    }

    if let Err(e) = write_file_atomic(resolved, &new_content) {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "dry_run": false,
                "old_sha256": old_sha256,
                "error": format!("write failed: {}", e),
            }),
            start,
        );
    }
    line_edit_stdout(
        serde_json::json!({
            "path": path,
            "dry_run": false,
            "applied_count": payload.edits.len(),
            "old_sha256": old_sha256,
            "new_sha256": new_sha256,
            "changed": true,
            "would_change": true,
            "edits": edit_summaries,
            "changed_paths": [path],
        }),
        start,
    )
}

fn apply_text_edits_error(
    path: &str,
    index: usize,
    kind: &str,
    msg: &str,
    start: Instant,
) -> CommandResult {
    line_edit_stdout(
        serde_json::json!({
            "changed": false,
            "path": path,
            "error_kind": match kind {
                "replace_exact" | "delete_exact" => "match_error",
                _ => "match_error",
            },
            "edit_index": index,
            "kind": kind,
            "message": format!(
                "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact match text.",
                index, kind, msg
            ),
        }),
        start,
    )
}

fn merge_json_object(target: &mut serde_json::Value, source: serde_json::Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn handle_file_read_request(
    policy: &AgentPolicy,
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let max = request
        .max_bytes
        .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES)
        .min(policy.max_output_bytes);
    if let (Some(start_line), Some(end_line)) = (request.start_line, request.end_line) {
        return handle_file_read_range_request(resolved, start_line, end_line, max, start);
    }

    match std::fs::read(resolved) {
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

fn handle_file_read_range_request(
    resolved: &Path,
    start_line: usize,
    end_line: usize,
    max: usize,
    start: Instant,
) -> CommandResult {
    if start_line == 0 || end_line < start_line {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("invalid line range for file_read".to_string()),
        };
    }

    let file = match std::fs::File::open(resolved) {
        Ok(file) => file,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to read {}: {}", resolved.display(), e)),
            }
        }
    };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut content = String::new();
    let mut total_lines = 0usize;
    let mut wrote_any_line = false;

    loop {
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(bytes) => bytes,
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "failed to read UTF-8 text {}: {}",
                        resolved.display(),
                        e
                    )),
                }
            }
        };
        if bytes_read == 0 {
            break;
        }
        total_lines = total_lines.saturating_add(1);
        if total_lines >= start_line && total_lines <= end_line {
            let line_content = line.strip_suffix('\n').unwrap_or(&line);
            let line_content = line_content.strip_suffix('\r').unwrap_or(line_content);
            let additional_len = line_content.len() + usize::from(wrote_any_line);
            if content.len().saturating_add(additional_len) > max {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "range output too large: {} bytes exceeds max_bytes {}",
                        content.len().saturating_add(additional_len),
                        max
                    )),
                };
            }
            if wrote_any_line {
                content.push('\n');
            }
            content.push_str(line_content);
            wrote_any_line = true;
        }
    }

    let limit = end_line.saturating_sub(start_line).saturating_add(1);
    let out = serde_json::json!({
        "format": "webcodex.file_read_range.v1",
        "content": content,
        "total_lines": total_lines,
        "start_line": start_line,
        "limit": limit,
    });
    CommandResult {
        exit_code: Some(0),
        stdout: Some(out.to_string()),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
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
    if is_line_edit_request_kind(&request.kind) {
        if let Err(e) = validate_line_edit_agent_path(path) {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            };
        }
    }
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
        "file_replace_line_range"
        | "file_insert_at_line"
        | "file_delete_line_range"
        | "file_replace_exact_block"
        | "file_insert_before_pattern"
        | "file_insert_after_pattern" => handle_line_edit_file_request(request, &resolved, start),
        "file_apply_text_edits" => handle_apply_text_edits_file_request(request, &resolved, start),
        "file_read" => handle_file_read_request(policy, request, &resolved, start),
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

#[derive(Debug, Default)]
struct CreatedProjectPaths {
    project_dir_created: Option<PathBuf>,
    paths: Vec<PathBuf>,
}

impl CreatedProjectPaths {
    fn mark_project_dir_created(&mut self, path: PathBuf) {
        self.project_dir_created = Some(path);
    }

    fn track(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    fn cleanup(&self) {
        for path in self.paths.iter().rev() {
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Some(dir) = &self.project_dir_created {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

fn write_created_file(
    path: &Path,
    content: &[u8],
    created_paths: &mut CreatedProjectPaths,
) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    created_paths.track(path.to_path_buf());
    file.write_all(content)
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
            agent_instance_id: sink.agent_instance_id().to_string(),
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

    fn enqueue(
        &self,
        sink: AgentSink,
        policy: AgentPolicy,
        shell: ShellConfig,
        projects_dir: PathBuf,
        request: ShellAgentShellRequest,
    ) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        let client_id = sink.client_id().to_string();
        let active = self.active_job_count(&client_id);
        if active >= self.max_concurrent {
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: client_id.clone(),
                agent_instance_id: sink.agent_instance_id().to_string(),
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
                .push_back((sink, policy, shell, projects_dir, request));
            return;
        }
        self.start_now(sink, policy, shell, projects_dir, request);
    }

    fn start_now(
        &self,
        sink: AgentSink,
        policy: AgentPolicy,
        shell: ShellConfig,
        projects_dir: PathBuf,
        request: ShellAgentShellRequest,
    ) {
        self.start_shell_job(sink, policy, shell, projects_dir, request);
    }

    fn start_available_queued(&self) {
        loop {
            let next = {
                let jobs = self.jobs.lock().unwrap();
                let mut queued = self.queued.lock().unwrap();
                let mut selected = None;
                for (idx, (_, _policy, _shell, _projects_dir, request)) in queued.iter().enumerate()
                {
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
            let Some((sink, policy, shell, projects_dir, request)) = next else {
                return;
            };
            self.start_now(sink, policy, shell, projects_dir, request);
        }
    }

    fn start_shell_job(
        &self,
        sink: AgentSink,
        policy: AgentPolicy,
        shell: ShellConfig,
        projects_dir: PathBuf,
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
        let mut prepared_profile_name = None;
        let mut cmd = match resolve_prepared_shell_profile(
            &shell,
            &projects_dir,
            &cwd_path,
            request.cwd.is_some(),
            &self.prepared_profiles,
        ) {
            Ok(Some(profile)) => {
                match configured_prepared_shell_job_command(&profile, &request.command) {
                    Ok(cmd) => {
                        prepared_profile_name = Some(profile.profile_name.clone());
                        cmd
                    }
                    Err(e) => {
                        send_start_failure(
                            &sink,
                            request,
                            format!(
                                "failed to configure shell profile '{}': {}",
                                profile.profile_name, e
                            ),
                        );
                        return;
                    }
                }
            }
            Ok(None) => match configured_shell_job_command(&shell, &request.command) {
                Ok(cmd) => cmd,
                Err(e) => {
                    send_start_failure(&sink, request, e);
                    return;
                }
            },
            Err(e) => {
                send_start_failure(&sink, request, e);
                return;
            }
        };
        let spawn = cmd
            .current_dir(&cwd_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawn {
            Ok(c) => c,
            Err(e) => {
                let error = prepared_profile_name
                    .as_deref()
                    .map(|profile_name| {
                        format!("failed to spawn shell profile '{}': {}", profile_name, e)
                    })
                    .unwrap_or_else(|| format!("failed to spawn command: {}", e));
                send_start_failure(&sink, request, error);
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
            agent_instance_id: sink.agent_instance_id().to_string(),
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
        let prepared_profiles = self.prepared_profiles.clone();
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
                        agent_instance_id: sink.agent_instance_id().to_string(),
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
                agent_instance_id: sink.agent_instance_id().to_string(),
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
                prepared_profiles,
            };
            manager.start_available_queued();
        });
    }

    fn stop(&self, job_id: &str) -> Result<(), String> {
        let queued_job = {
            let mut queued = self.queued.lock().unwrap();
            if let Some(pos) = queued
                .iter()
                .position(|(_, _, _, _, request)| request.job_id.as_deref() == Some(job_id))
            {
                queued.remove(pos)
            } else {
                None
            }
        };
        if let Some((sink, _policy, _shell, _projects_dir, request)) = queued_job {
            let request_id = request.request_id.clone();
            let job_id = request.job_id.clone().unwrap_or_default();
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                agent_instance_id: sink.agent_instance_id().to_string(),
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
    shell: &ShellConfig,
    jobs: &JobManager,
    projects_dir: &Path,
    request: ShellAgentShellRequest,
) -> Result<bool, String> {
    match request.kind.as_str() {
        "start_job" => {
            jobs.enqueue(
                sink.clone(),
                policy.clone(),
                shell.clone(),
                projects_dir.to_path_buf(),
                request,
            );
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
        kind if is_file_request_kind(kind) => {
            let request_id = request.request_id.clone();
            let result = handle_file_request(policy, &request);
            sink.submit_result(request_id, result)
        }
        "register_project" | "create_project" => {
            let request_id = request.request_id.clone();
            let result = handle_project_op(policy, projects_dir, &request);
            sink.submit_result(request_id, result)
        }
        _ => {
            let request_id = request.request_id.clone();
            let result = run_shell_with_profiles(
                policy,
                shell,
                projects_dir,
                &jobs.prepared_profiles,
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

fn is_project_op(kind: &str) -> bool {
    kind == "register_project" || kind == "create_project"
}

fn handle_one_poll(
    client: &Client,
    cfg: &AgentConfig,
    jobs: &JobManager,
    project_cache: &mut AgentProjectCache,
    agent_instance_id: &str,
) -> Result<bool, String> {
    let poll = ShellAgentPollRequest {
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
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
    let project_op = is_project_op(&request.kind);
    let sink = AgentSink::Http(HttpSendConfig {
        client: client.clone(),
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
    });
    let result = dispatch_request(
        &sink,
        &cfg.policy,
        &cfg.shell,
        jobs,
        &projects_dir(&cfg),
        request,
    );
    if project_op && result.is_ok() {
        project_cache.invalidate();
    }
    result
}

fn main() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let action = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    let (config_path, once) = match action {
        AgentCliAction::Run { config_path, once } => (config_path, once),
        AgentCliAction::Init(opts) => match run_agent_init(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        },
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            std::process::exit(code);
        }
    };
    let cfg = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    if cfg.token.trim().is_empty() {
        eprintln!(
            "webcodex-agent warning: agent token is empty; connecting without Authorization; the server must be started with --open"
        );
    }
    if let Err(e) = run_agent(cfg, once) {
        eprintln!("webcodex-agent failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_init::generated_agent_config_toml;
    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
            shell: ShellConfig::default(),
            transport: None,
            quic: None,
        }
    }

    fn init_opts(output: PathBuf) -> AgentInitOptions {
        AgentInitOptions {
            server_url: "https://v4.example.test/".to_string(),
            token: Some("wc_agent_fake_test_token".to_string()),
            token_file: None,
            client_id: "alice-laptop".to_string(),
            owner: "alice".to_string(),
            display_name: Some("Alice Laptop".to_string()),
            transport: TRANSPORT_WEBSOCKET.to_string(),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            projects_dir: PathBuf::from("/etc/webcodex/projects.d"),
            output,
            allowed_roots: vec![PathBuf::from("/srv/projects")],
            allow_cwd_anywhere: false,
            overwrite: false,
        }
    }

    fn quic_client_config() -> QuicClientConfig {
        QuicClientConfig {
            server_addr: "v4.example.test:8443".to_string(),
            server_name: "v4.example.test".to_string(),
            alpn: default_quic_alpn(),
            connect_timeout_secs: default_quic_connect_timeout_secs(),
            keepalive_interval_secs: default_quic_keepalive_interval_secs(),
        }
    }

    #[test]
    fn agent_config_defaults_transport_to_websocket_without_quic_section() {
        // No transport field and no [quic] section: default stays websocket.
        let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
"#;
        let cfg: AgentConfig = toml::from_str(toml).unwrap();
        assert!(cfg.transport.is_none());
        assert!(cfg.quic.is_none());
        assert_eq!(effective_transport(&cfg), TRANSPORT_WEBSOCKET);
        assert_eq!(
            auto_transport_plan(&cfg),
            vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
        );
    }

    #[test]
    fn agent_config_accepts_transport_quic_with_quic_section() {
        let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "quic"

[quic]
server_addr = "v4.example.test:8443"
server_name = "v4.example.test"
"#;
        let cfg: AgentConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.transport.as_deref(), Some("quic"));
        let quic = cfg.quic.expect("quic section");
        assert_eq!(quic.server_addr, "v4.example.test:8443");
        assert_eq!(quic.server_name, "v4.example.test");
        // Defaults applied.
        assert_eq!(quic.alpn, "webcodex-agent/1");
        assert_eq!(quic.connect_timeout_secs, 10);
        assert_eq!(quic.keepalive_interval_secs, 20);
    }

    #[test]
    fn agent_config_accepts_transport_auto() {
        let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "auto"
"#;
        let cfg: AgentConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.transport.as_deref(), Some(TRANSPORT_AUTO));
        assert_eq!(effective_transport(&cfg), TRANSPORT_AUTO);
        assert_eq!(
            auto_transport_plan(&cfg),
            vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
        );
    }

    #[test]
    fn auto_transport_plan_tries_quic_then_websocket_then_polling() {
        let mut cfg = test_config(PathBuf::from("/tmp/x"));
        cfg.transport = Some(TRANSPORT_AUTO.to_string());
        cfg.quic = Some(quic_client_config());
        assert_eq!(
            auto_transport_plan(&cfg),
            vec![TRANSPORT_QUIC, TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
        );
    }

    #[test]
    fn strict_quic_transport_still_requires_quic_section() {
        let mut cfg = test_config(PathBuf::from("/tmp/x"));
        cfg.transport = Some(TRANSPORT_QUIC.to_string());
        let err = resolve_quic_config(&cfg).unwrap_err();
        assert!(err.contains("transport=quic requires a [quic] section"));
        assert_eq!(effective_transport(&cfg), TRANSPORT_QUIC);
    }

    #[test]
    fn resolve_quic_config_errors_when_section_missing() {
        let mut cfg = test_config(PathBuf::from("/tmp/x"));
        cfg.transport = Some("quic".to_string());
        let err = resolve_quic_config(&cfg).unwrap_err();
        assert!(err.contains("[quic]"), "err was: {err}");
    }

    #[test]
    fn resolve_quic_config_errors_when_server_addr_or_name_missing() {
        let mut cfg = test_config(PathBuf::from("/tmp/x"));
        cfg.transport = Some("quic".to_string());

        // Missing server_addr.
        cfg.quic = Some(QuicClientConfig {
            server_addr: "  ".to_string(),
            server_name: "v4.example.test".to_string(),
            alpn: default_quic_alpn(),
            connect_timeout_secs: 10,
            keepalive_interval_secs: 20,
        });
        let err = resolve_quic_config(&cfg).unwrap_err();
        assert!(err.contains("server_addr"), "err was: {err}");

        // Missing server_name.
        cfg.quic = Some(QuicClientConfig {
            server_addr: "v4.example.test:8443".to_string(),
            server_name: String::new(),
            alpn: default_quic_alpn(),
            connect_timeout_secs: 10,
            keepalive_interval_secs: 20,
        });
        let err = resolve_quic_config(&cfg).unwrap_err();
        assert!(err.contains("server_name"), "err was: {err}");
    }

    #[test]
    fn resolve_quic_config_accepts_valid_section() {
        let mut cfg = test_config(PathBuf::from("/tmp/x"));
        cfg.transport = Some("quic".to_string());
        cfg.quic = Some(quic_client_config());
        let resolved = resolve_quic_config(&cfg).unwrap();
        assert_eq!(resolved.server_addr, "v4.example.test:8443");
        assert_eq!(resolved.server_name, "v4.example.test");
    }

    #[test]
    fn resolve_quic_server_addrs_accepts_hostname_port() {
        let addrs = resolve_quic_server_addrs("localhost:8443").unwrap();
        assert!(addrs.iter().any(|addr| addr.port() == 8443));
    }

    #[test]
    fn resolve_quic_server_addrs_rejects_missing_port() {
        let err = resolve_quic_server_addrs("localhost").unwrap_err();
        assert!(err.contains("failed to resolve"), "err was: {err}");
    }

    #[test]
    fn quic_client_bind_addr_matches_remote_address_family() {
        let v4: SocketAddr = "127.0.0.1:8443".parse().unwrap();
        let v6: SocketAddr = "[::1]:8443".parse().unwrap();
        assert!(quic_client_bind_addr_for(v4).is_ipv4());
        assert!(quic_client_bind_addr_for(v6).is_ipv6());
    }

    #[test]
    fn agent_cli_help_and_version_exit_before_runtime() {
        match parse_agent_args(["--help"]).unwrap() {
            AgentCliAction::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("Usage: webcodex-agent"));
                assert!(stdout.contains("webcodex-agent init"));
                assert!(stderr.is_empty());
            }
            other => panic!("expected help exit, got {other:?}"),
        }
        match parse_agent_args(["--version"]).unwrap() {
            AgentCliAction::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(code, 0);
                assert!(stdout.starts_with(&format!(
                    "webcodex-agent {} (commit ",
                    env!("CARGO_PKG_VERSION")
                )));
                assert!(stdout.trim_end().ends_with(')'));
                assert_ne!(
                    stdout,
                    format!("webcodex-agent {}\n", env!("CARGO_PKG_VERSION"))
                );
                assert!(stderr.is_empty());
            }
            other => panic!("expected version exit, got {other:?}"),
        }
    }

    #[test]
    fn agent_version_output_includes_build_metadata() {
        match parse_agent_args(["-V"]).unwrap() {
            AgentCliAction::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("commit "));
                assert!(stdout.starts_with("webcodex-agent "));
                assert!(stderr.is_empty());
            }
            other => panic!("expected version exit, got {other:?}"),
        }
    }

    #[test]
    fn agent_cli_legacy_runtime_args_are_preserved() {
        let action = parse_agent_args(["--config", "/tmp/agent.toml", "--once"]).unwrap();
        assert_eq!(
            action,
            AgentCliAction::Run {
                config_path: PathBuf::from("/tmp/agent.toml"),
                once: true,
            }
        );
    }

    #[test]
    fn agent_cli_profile_derives_default_config_path() {
        let action = parse_agent_args(["--profile", "special"]).unwrap();
        assert_eq!(
            action,
            AgentCliAction::Run {
                config_path: client_profile_agent_config("special"),
                once: false,
            }
        );
    }

    #[test]
    fn agent_cli_explicit_config_overrides_profile() {
        let action =
            parse_agent_args(["--profile", "special", "--config", "/tmp/agent.toml"]).unwrap();
        assert_eq!(
            action,
            AgentCliAction::Run {
                config_path: PathBuf::from("/tmp/agent.toml"),
                once: false,
            }
        );
    }

    #[test]
    fn agent_cli_rejects_unsafe_profile() {
        let err = parse_agent_args(["--profile", "../x"]).unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }

    #[test]
    fn agent_init_writes_valid_toml_that_existing_parser_accepts() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        let msg = run_agent_init(init_opts(output.clone())).unwrap();
        assert!(msg.contains("agent.toml"));

        let cfg = load_config(&output).unwrap();
        assert_eq!(cfg.server_url, "https://v4.example.test");
        assert_eq!(cfg.token, "wc_agent_fake_test_token");
        assert_eq!(cfg.client_id, "alice-laptop");
        assert_eq!(cfg.owner.as_deref(), Some("alice"));
        assert_eq!(cfg.display_name.as_deref(), Some("Alice Laptop"));
        assert_eq!(cfg.transport.as_deref(), Some(TRANSPORT_WEBSOCKET));
        assert_eq!(cfg.poll_interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert_eq!(
            cfg.projects_dir.as_deref(),
            Some(Path::new("/etc/webcodex/projects.d"))
        );
        assert!(!cfg.policy.allow_cwd_anywhere);
        assert_eq!(
            cfg.policy.allowed_roots,
            vec![PathBuf::from("/srv/projects")]
        );
        let caps = cfg.capabilities.unwrap();
        assert!(caps.shell);
        assert!(caps.file_read);
        assert!(caps.file_write);
        assert!(caps.git);
        assert!(caps.jobs);
        assert!(caps.async_jobs);
        assert!(caps.async_shell_jobs);
    }

    #[test]
    fn empty_tokens_config_parser_accepts_empty_and_whitespace_token() {
        for token in ["", "   "] {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("agent.toml");
            std::fs::write(
                &path,
                format!(
                    "server_url = \"http://127.0.0.1:8000\"\ntoken = \"{}\"\nclient_id = \"open-agent\"\n[policy]\nallow_cwd_anywhere = true\n",
                    token
                ),
            )
            .unwrap();

            let cfg = load_config(&path).unwrap();
            assert_eq!(cfg.token, token);
            assert_eq!(non_empty_token(&cfg.token), None);
        }
    }

    #[test]
    fn empty_tokens_agent_init_still_rejects_empty_token_sources() {
        let mut opts = init_opts(PathBuf::from("-"));
        opts.token = Some("   ".to_string());
        let err = generated_agent_config_toml(&opts).unwrap_err();
        assert!(err.contains("--token cannot be empty"), "{err}");

        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("agent.token");
        std::fs::write(&token_file, "  \n").unwrap();
        let mut opts = init_opts(PathBuf::from("-"));
        opts.token = None;
        opts.token_file = Some(token_file);
        let err = generated_agent_config_toml(&opts).unwrap_err();
        assert!(err.contains("--token-file cannot be empty"), "{err}");

        let _guard = TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_AGENT_TOKEN", "   ");
        let mut opts = init_opts(PathBuf::from("-"));
        opts.token = None;
        opts.token_file = None;
        let err = generated_agent_config_toml(&opts).unwrap_err();
        assert!(
            err.contains("WEBCODEX_AGENT_TOKEN cannot be empty"),
            "{err}"
        );
        std::env::remove_var("WEBCODEX_AGENT_TOKEN");
    }

    #[test]
    fn agent_config_without_shell_section_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true
"#,
        )
        .unwrap();

        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.shell, ShellConfig::default());
    }

    #[test]
    fn agent_config_shell_profiles_parse() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "sh"
args = ["-c"]
init_script = '''
export RUST_BACKTRACE=1
'''

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/bin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"

[shell.profiles.py-venv]
description = "Project-local Python virtual environment"
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
"#,
        )
        .unwrap();

        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.shell.default_profile.as_deref(), Some("rust"));
        let rust = cfg.shell.profiles.get("rust").unwrap();
        assert_eq!(rust.description.as_deref(), Some("Rust development tools"));
        assert_eq!(rust.program.as_deref(), Some("sh"));
        assert_eq!(rust.args.as_ref().unwrap(), &vec!["-c".to_string()]);
        assert_eq!(
            rust.env.get("CARGO_HOME").map(String::as_str),
            Some("/root/.cargo")
        );
        assert!(rust
            .init_script
            .as_deref()
            .unwrap()
            .contains("RUST_BACKTRACE=1"));
        assert!(cfg.shell.profiles.contains_key("py-venv"));
    }

    #[test]
    fn agent_config_shell_default_profile_must_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
program = "sh"
"#,
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.contains("default_profile"), "{err}");
        assert!(err.contains("missing"), "{err}");
    }

    #[test]
    fn agent_config_shell_profile_name_must_be_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles."bad/name"]
program = "sh"
"#,
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.contains("shell profile name"), "{err}");
        assert!(err.contains("slash"), "{err}");
    }

    #[test]
    fn agent_config_shell_profile_type_errors_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust]
args = "-c"
"#,
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.contains("failed to parse config"), "{err}");
        assert!(err.contains("args"), "{err}");
    }

    #[test]
    fn agent_config_shell_profile_env_type_errors_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust.env]
PATH = ["/root/.cargo/bin"]
"#,
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.contains("failed to parse config"), "{err}");
        assert!(err.contains("env"), "{err}");
    }

    #[test]
    fn agent_config_shell_errors_do_not_include_init_script_body() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
        std::fs::write(
            &path,
            format!(
                r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
init_script = '''
export SECRET={}
'''
"#,
                secret
            ),
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.contains("default_profile"), "{err}");
        assert!(!err.contains(secret), "{err}");
    }

    #[test]
    fn agent_init_refuses_overwrite_unless_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        run_agent_init(init_opts(output.clone())).unwrap();
        let err = run_agent_init(init_opts(output.clone())).unwrap_err();
        assert!(err.contains("already exists"));

        let mut overwrite = init_opts(output);
        overwrite.overwrite = true;
        run_agent_init(overwrite).unwrap();
    }

    #[test]
    fn agent_init_supports_stdout_output() {
        let content = run_agent_init(init_opts(PathBuf::from("-"))).unwrap();
        assert!(content.contains("server_url = \"https://v4.example.test\""));
        assert_eq!(content.matches("wc_agent_fake_test_token").count(), 1);
        let parsed: AgentConfig = toml::from_str(&content).unwrap();
        assert_eq!(parsed.client_id, "alice-laptop");
    }

    #[cfg(unix)]
    #[test]
    fn agent_init_writes_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        run_agent_init(init_opts(output.clone())).unwrap();
        let mode = std::fs::metadata(output).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn agent_init_token_file_and_env_fallback_work() {
        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("agent.token");
        std::fs::write(&token_file, "wc_agent_fake_file_token\n").unwrap();
        let mut opts = init_opts(PathBuf::from("-"));
        opts.token = None;
        opts.token_file = Some(token_file);
        let content = generated_agent_config_toml(&opts).unwrap();
        assert!(content.contains("wc_agent_fake_file_token"));

        let _guard = TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_AGENT_TOKEN", "wc_agent_fake_env_token");
        let mut opts = init_opts(PathBuf::from("-"));
        opts.token = None;
        opts.token_file = None;
        let content = generated_agent_config_toml(&opts).unwrap();
        assert!(content.contains("wc_agent_fake_env_token"));
        std::env::remove_var("WEBCODEX_AGENT_TOKEN");
    }

    #[test]
    fn agent_project_toml_parse_sorts_hook_names() {
        let project = parse_agent_project_toml(
            r#"
id = "webcodex"
path = "/root/git/webcodex"
kind = "rust"
shell_profile = "rust"

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
        assert_eq!(project.shell_profile.as_deref(), Some("rust"));
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
    fn agent_project_toml_hints_when_server_projects_format_is_used() {
        let err = parse_agent_project_toml(
            r#"
[projects.smoke]
path = "/root/webcodex-smoke"
"#,
        )
        .unwrap_err();
        assert!(err.contains("missing field"), "{err}");
        assert!(err.contains("server projects.toml"), "{err}");
        assert!(
            err.contains("Agent projects.d files must use top-level fields"),
            "{err}"
        );
        assert!(err.contains("id = \"smoke\""), "{err}");
        assert!(err.contains("path = \"/path/to/repo\""), "{err}");
    }

    #[test]
    fn agent_project_toml_rejects_invalid_shell_profile() {
        let err = parse_agent_project_toml(
            r#"
id = "demo"
path = "/tmp/webcodex"
shell_profile = "../rust"
"#,
        )
        .unwrap_err();
        assert!(err.contains("project.shell_profile"), "{err}");
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
    fn shell_config_default_preserves_sh_c_behavior() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();
        let result = run_shell(
            &cfg.policy,
            &ShellConfig::default(),
            Some(&cwd),
            "printf default-ok",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("default-ok"));
    }

    #[test]
    fn shell_config_path_prepend_discovers_fake_executable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir(&bin_dir).unwrap();
        let exe = bin_dir.join("webcodex-fake-tool");
        std::fs::write(&exe, "#!/bin/sh\nprintf fake-tool-ok\n").unwrap();
        let mut perms = std::fs::metadata(&exe).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&exe, perms).unwrap();
        let shell = ShellConfig {
            path_prepend: vec![bin_dir],
            ..ShellConfig::default()
        };
        let cwd = tmp.path().to_string_lossy().to_string();
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            "webcodex-fake-tool",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("fake-tool-ok"));
    }

    #[test]
    fn shell_config_env_values_are_available() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let shell = ShellConfig {
            env: HashMap::from([("WEBCODEX_TEST_VALUE".to_string(), "env-ok".to_string())]),
            ..ShellConfig::default()
        };
        let cwd = tmp.path().to_string_lossy().to_string();
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            "printf %s \"$WEBCODEX_TEST_VALUE\"",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("env-ok"));
    }

    #[test]
    fn shell_config_init_script_is_sourced() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let init = tmp.path().join("init.sh");
        std::fs::write(&init, "export WEBCODEX_INIT_TEST=init-ok\n").unwrap();
        let shell = ShellConfig {
            init_script: Some(init),
            ..ShellConfig::default()
        };
        let cwd = tmp.path().to_string_lossy().to_string();
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            "printf %s \"$WEBCODEX_INIT_TEST\"",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("init-ok"));
    }

    #[test]
    fn shell_config_bash_like_args_are_respected_when_available() {
        if !Path::new("/bin/bash").exists() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let shell = ShellConfig {
            program: "/bin/bash".to_string(),
            args: vec!["-lc".to_string()],
            ..ShellConfig::default()
        };
        let cwd = tmp.path().to_string_lossy().to_string();
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            "[[ 1 -eq 1 ]] && printf bash-ok",
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.as_deref(), Some("bash-ok"));
    }

    fn shell_with_profiles(
        default_profile: Option<&str>,
        profiles: Vec<(&str, ShellProfileConfig)>,
    ) -> ShellConfig {
        ShellConfig {
            default_profile: default_profile.map(str::to_string),
            profiles: profiles
                .into_iter()
                .map(|(name, profile)| (name.to_string(), profile))
                .collect(),
            ..ShellConfig::default()
        }
    }

    fn profile_env(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn write_agent_project(
        projects_dir: &Path,
        id: &str,
        path: &Path,
        shell_profile: Option<&str>,
    ) {
        std::fs::create_dir_all(projects_dir).unwrap();
        let shell_profile = shell_profile
            .map(|profile| format!("shell_profile = {:?}\n", profile))
            .unwrap_or_default();
        std::fs::write(
            projects_dir.join(format!("{}.toml", id)),
            format!(
                "id = {:?}\npath = {:?}\nname = {:?}\n{}",
                id,
                path.to_string_lossy(),
                id,
                shell_profile
            ),
        )
        .unwrap();
    }

    fn run_profile_shell(
        policy: &AgentPolicy,
        shell: &ShellConfig,
        projects_dir: &Path,
        cache: &PreparedShellProfileCache,
        cwd: &Path,
        command: &str,
    ) -> CommandResult {
        let cwd = cwd.to_string_lossy().to_string();
        run_shell_with_profiles(
            policy,
            shell,
            projects_dir,
            cache,
            Some(&cwd),
            command,
            None,
            10,
            None,
        )
    }

    #[test]
    fn prepared_profile_env_is_available_to_run_shell() {
        let tmp = tempfile::tempdir().unwrap();
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    env: profile_env(&[("WEBCODEX_TEST_PROFILE", "from_env")]),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "printf %s \"$WEBCODEX_TEST_PROFILE\"",
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("from_env"));
    }

    #[test]
    fn prepared_profile_init_script_export_is_available_to_run_shell() {
        let tmp = tempfile::tempdir().unwrap();
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("/bin/sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some("export WEBCODEX_TEST_PROFILE=from_snapshot".to_string()),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "printf %s \"$WEBCODEX_TEST_PROFILE\"",
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("from_snapshot"));
    }

    #[test]
    fn prepared_profile_init_script_is_project_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("project");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir_all(project_dir.join(".venv/bin")).unwrap();
        std::fs::write(
            project_dir.join(".venv/bin/activate"),
            "export WEBCODEX_PROJECT_VENV=project_local\n",
        )
        .unwrap();
        write_agent_project(&projects_dir, "demo", &project_dir, Some("py-venv"));
        let shell = shell_with_profiles(
            None,
            vec![(
                "py-venv",
                ShellProfileConfig {
                    program: Some("/bin/sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some(". .venv/bin/activate".to_string()),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            &projects_dir,
            &PreparedShellProfileCache::default(),
            &project_dir,
            "printf %s \"$WEBCODEX_PROJECT_VENV\"",
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("project_local"));
    }

    #[test]
    fn project_shell_profile_overrides_default_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("project");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir_all(&project_dir).unwrap();
        write_agent_project(&projects_dir, "demo", &project_dir, Some("project"));
        let shell = shell_with_profiles(
            Some("default"),
            vec![
                (
                    "default",
                    ShellProfileConfig {
                        env: profile_env(&[("WEBCODEX_TEST_PROFILE", "default")]),
                        ..ShellProfileConfig::default()
                    },
                ),
                (
                    "project",
                    ShellProfileConfig {
                        env: profile_env(&[("WEBCODEX_TEST_PROFILE", "project")]),
                        ..ShellProfileConfig::default()
                    },
                ),
            ],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            &projects_dir,
            &PreparedShellProfileCache::default(),
            &project_dir,
            "printf %s \"$WEBCODEX_TEST_PROFILE\"",
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("project"));
    }

    fn shell_job_request(cwd: &Path, command: &str) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "req-job".to_string(),
            client_id: "ws-client".to_string(),
            kind: "start_job".to_string(),
            job_id: Some("job-profile".to_string()),
            cwd: Some(cwd.to_string_lossy().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: command.to_string(),
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    fn wait_for_job_stdout(rx: &mut tokio::sync::mpsc::Receiver<AgentEnvelope>) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stdout = String::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(AgentEnvelope::JobUpdate { payload }) => {
                    if let Some(chunk) = payload.stdout_chunk {
                        stdout.push_str(&chunk);
                    }
                    if payload.finished {
                        return stdout;
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        panic!("timed out waiting for job completion; stdout so far: {stdout:?}");
    }

    fn line_edit_request(
        cwd: &Path,
        kind: &str,
        path: &str,
        content: Option<&str>,
        start_line: Option<usize>,
        end_line: Option<usize>,
        line: Option<usize>,
        expected_sha256: Option<String>,
        expected_prefix: Option<&str>,
    ) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: format!("req-{kind}"),
            client_id: "agent-1".to_string(),
            kind: kind.to_string(),
            job_id: None,
            cwd: Some(cwd.to_string_lossy().to_string()),
            path: Some(path.to_string()),
            content: content.map(str::to_string),
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256,
            expected_prefix: expected_prefix.map(str::to_string),
            start_line,
            end_line,
            line,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    fn anchor_edit_request(
        cwd: &Path,
        kind: &str,
        path: &str,
        old_text: Option<&str>,
        pattern: Option<&str>,
        content: Option<&str>,
        expected_sha256: Option<String>,
    ) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: format!("req-{kind}"),
            client_id: "agent-1".to_string(),
            kind: kind.to_string(),
            job_id: None,
            cwd: Some(cwd.to_string_lossy().to_string()),
            path: Some(path.to_string()),
            content: content.map(str::to_string),
            max_bytes: None,
            old_text: old_text.map(str::to_string),
            pattern: pattern.map(str::to_string),
            expected_sha256,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    fn file_read_request(
        cwd: &Path,
        path: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
        max_bytes: Option<usize>,
    ) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "req-file-read".to_string(),
            client_id: "agent-1".to_string(),
            kind: "file_read".to_string(),
            job_id: None,
            cwd: Some(cwd.to_string_lossy().to_string()),
            path: Some(path.to_string()),
            content: None,
            max_bytes,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line,
            end_line,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    fn line_edit_json(result: CommandResult) -> serde_json::Value {
        assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
    }

    fn file_read_json(result: CommandResult) -> serde_json::Value {
        assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
    }

    #[test]
    fn agent_file_read_without_range_preserves_plain_text_output() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("small.txt"), "one\ntwo\n").unwrap();

        let out = handle_file_request(
            &policy,
            &file_read_request(tmp.path(), "small.txt", None, None, Some(1024)),
        );

        assert_eq!(out.exit_code, Some(0), "unexpected result: {out:?}");
        assert_eq!(out.stdout.as_deref(), Some("one\ntwo\n"));
    }

    #[test]
    fn agent_file_read_range_reads_large_file_subset_under_max_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let mut content = String::new();
        for n in 1..=500 {
            content.push_str(&format!("line-{n:04}\n"));
        }
        std::fs::write(tmp.path().join("large.txt"), content).unwrap();

        let out = file_read_json(handle_file_request(
            &policy,
            &file_read_request(tmp.path(), "large.txt", Some(250), Some(252), Some(128)),
        ));

        assert_eq!(out["format"], "webcodex.file_read_range.v1");
        assert_eq!(out["content"], "line-0250\nline-0251\nline-0252");
        assert_eq!(out["total_lines"], 500);
        assert_eq!(out["start_line"], 250);
        assert_eq!(out["limit"], 3);
    }

    #[test]
    fn agent_file_read_range_beyond_total_lines_returns_empty_content() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("short.txt"), "one\ntwo\nthree\n").unwrap();

        let out = file_read_json(handle_file_request(
            &policy,
            &file_read_request(tmp.path(), "short.txt", Some(10), Some(12), Some(128)),
        ));

        assert_eq!(out["format"], "webcodex.file_read_range.v1");
        assert_eq!(out["content"], "");
        assert_eq!(out["total_lines"], 3);
        assert_eq!(out["start_line"], 10);
        assert_eq!(out["limit"], 3);
    }

    #[test]
    fn agent_file_read_range_preserves_empty_selected_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("blank.txt"), "\nsecond\nthird\n").unwrap();

        let out = file_read_json(handle_file_request(
            &policy,
            &file_read_request(tmp.path(), "blank.txt", Some(1), Some(2), Some(128)),
        ));

        assert_eq!(out["format"], "webcodex.file_read_range.v1");
        assert_eq!(out["content"], "\nsecond");
        assert_eq!(out["total_lines"], 3);
        assert_eq!(out["start_line"], 1);
        assert_eq!(out["limit"], 2);
    }

    #[test]
    fn agent_file_read_range_output_obeys_max_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("limited.txt"), "alpha\nbeta\n").unwrap();

        let out = handle_file_request(
            &policy,
            &file_read_request(tmp.path(), "limited.txt", Some(1), Some(1), Some(4)),
        );

        assert!(out.exit_code.is_none(), "unexpected success: {out:?}");
        assert!(out.error.expect("error").contains("exceeds max_bytes"));
    }

    #[test]
    fn replace_exact_block_replaces_single_block() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "alpha\nold block\nomega\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some("old block\n"),
                None,
                Some("new block\n"),
                None,
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["matches_replaced"], 1);
        assert_eq!(out["bytes_before"], "alpha\nold block\nomega\n".len());
        assert_eq!(out["bytes_after"], "alpha\nnew block\nomega\n".len());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "alpha\nnew block\nomega\n"
        );
    }

    #[test]
    fn replace_exact_block_accepts_matching_whole_file_sha256_guard() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        let original = "alpha\nold block\nomega\n";
        std::fs::write(&file, original).unwrap();
        let whole_file_sha256 = sha256_hex_bytes(original.as_bytes());

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some("old block\n"),
                None,
                Some("new block\n"),
                Some(whole_file_sha256),
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["matches_replaced"], 1);
        assert_ne!(
            out.get("error").and_then(|v| v.as_str()),
            Some("expected_old_sha256 mismatch")
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "alpha\nnew block\nomega\n"
        );
    }

    #[test]
    fn replace_exact_block_rejects_mismatched_whole_file_sha256_guard() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        let original = "alpha\nold block\nomega\n";
        std::fs::write(&file, original).unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some("old block\n"),
                None,
                Some("new block\n"),
                Some(
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                ),
            ),
        ));
        assert_eq!(out["changed"], false);
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("expected_old_sha256 mismatch"));
        assert!(err.contains("No files were modified"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }

    #[test]
    fn replace_exact_block_rejects_missing_old_text_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "alpha\nomega\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some("missing"),
                None,
                Some("new"),
                None,
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("Rejected before write"));
        assert!(err.contains("No files were modified"));
        assert!(err.contains("Retry guidance"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\nomega\n");
    }

    #[test]
    fn replace_exact_block_rejects_multiple_matches_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "dup\ndup\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some("dup"),
                None,
                Some("x"),
                None,
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("Rejected before write"));
        assert!(err.contains("expected exactly one match"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup\ndup\n");
    }

    #[test]
    fn replace_exact_block_rejects_empty_old_text() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("anchor.txt"), "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "anchor.txt",
                Some(""),
                None,
                Some("x"),
                None,
            ),
        ));
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("old_text must be non-empty"));
    }

    #[test]
    fn replace_exact_block_rejects_non_utf8_file() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("binary.bin");
        std::fs::write(&file, [0xff, 0xfe, 0xfd]).unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_replace_exact_block",
                "binary.bin",
                Some("old"),
                None,
                Some("new"),
                None,
            ),
        ));
        assert!(out["error"].as_str().unwrap().contains("not valid UTF-8"));
        assert_eq!(std::fs::read(&file).unwrap(), vec![0xff, 0xfe, 0xfd]);
    }

    #[test]
    fn insert_before_pattern_inserts_before_single_literal_match() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "alpha\nomega\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_before_pattern",
                "anchor.txt",
                None,
                Some("omega"),
                Some("before\n"),
                None,
            ),
        ));
        assert_eq!(out["pattern_matches"], 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "alpha\nbefore\nomega\n"
        );
    }

    #[test]
    fn insert_after_pattern_inserts_after_single_literal_match() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "alpha\nomega\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_after_pattern",
                "anchor.txt",
                None,
                Some("alpha"),
                Some("-after"),
                None,
            ),
        ));
        assert_eq!(out["pattern_matches"], 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "alpha-after\nomega\n"
        );
    }

    #[test]
    fn insert_pattern_rejects_missing_pattern_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_before_pattern",
                "anchor.txt",
                None,
                Some("missing"),
                Some("x"),
                None,
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("Rejected before write"));
        assert!(err.contains("No files were modified"));
        assert!(err.contains("Retry guidance"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
    }

    #[test]
    fn insert_pattern_rejects_multiple_matches_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("anchor.txt");
        std::fs::write(&file, "x-x-x").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_after_pattern",
                "anchor.txt",
                None,
                Some("x"),
                Some("!"),
                None,
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("expected exactly one match"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x-x-x");
    }

    #[test]
    fn insert_pattern_rejects_empty_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("anchor.txt"), "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_before_pattern",
                "anchor.txt",
                None,
                Some(""),
                Some("x"),
                None,
            ),
        ));
        assert!(out["error"].as_str().unwrap().contains("literal pattern"));
    }

    #[test]
    fn insert_pattern_rejects_empty_text() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("anchor.txt"), "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &anchor_edit_request(
                tmp.path(),
                "file_insert_after_pattern",
                "anchor.txt",
                None,
                Some("alpha"),
                Some(""),
                None,
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("inserted text must not be empty"));
        assert!(err.contains("Retry guidance"));
    }

    fn apply_text_edits_request(
        cwd: &Path,
        path: &str,
        payload: serde_json::Value,
    ) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "req-apply-text-edits".to_string(),
            client_id: "agent-1".to_string(),
            kind: "file_apply_text_edits".to_string(),
            job_id: None,
            cwd: Some(cwd.to_string_lossy().to_string()),
            path: Some(path.to_string()),
            content: Some(payload.to_string()),
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    #[test]
    fn file_apply_text_edits_replace_exact_writes_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "old\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "edits": [
                        {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                    ]
                }),
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["would_change"], true);
        assert_eq!(out["changed_paths"][0], "target.txt");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
    }

    #[test]
    fn file_apply_text_edits_dry_run_does_not_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "old\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "dry_run": true,
                    "edits": [
                        {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                    ]
                }),
            ),
        ));
        assert_eq!(out["dry_run"], true);
        assert_eq!(out["changed"], false);
        assert_eq!(out["would_change"], true);
        assert_eq!(out["changed_paths"][0], "target.txt");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
    }

    #[test]
    fn file_apply_text_edits_rejects_missing_match_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "edits": [
                        {"kind": "replace_exact", "old_text": "missing", "new_text": "x"}
                    ]
                }),
            ),
        ));
        let msg = out["message"].as_str().unwrap();
        assert!(msg.contains("match text was not found"));
        assert!(msg.contains("No files were modified"));
        assert_eq!(out["changed"], false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
    }

    #[test]
    fn file_apply_text_edits_rejects_ambiguous_match_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "dup-dup\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "edits": [
                        {"kind": "replace_exact", "old_text": "dup", "new_text": "x"}
                    ]
                }),
            ),
        ));
        let msg = out["message"].as_str().unwrap();
        assert!(msg.contains("refusing ambiguous edit"));
        assert!(msg.contains("No files were modified"));
        assert_eq!(out["changed"], false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup-dup\n");
    }

    #[test]
    fn file_apply_text_edits_expected_file_sha256_mismatch_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "alpha\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "expected_file_sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdead",
                    "edits": [
                        {"kind": "replace_exact", "old_text": "alpha", "new_text": "beta"}
                    ]
                }),
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("expected_file_sha256 mismatch"));
        assert!(err.contains("No files were modified"));
        assert_eq!(out["changed"], false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
    }

    #[test]
    fn file_apply_text_edits_insert_before_after_and_delete_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "edits": [
                        {"kind": "insert_after", "anchor_text": "alpha\n", "new_text": "ALPHA-AFTER\n"},
                        {"kind": "delete_exact", "old_text": "beta\n"},
                        {"kind": "insert_before", "anchor_text": "gamma\n", "new_text": "GAMMA-BEFORE\n"}
                    ]
                }),
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["applied_count"], 3);
        assert_eq!(out["changed_paths"][0], "target.txt");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "alpha\nALPHA-AFTER\nGAMMA-BEFORE\ngamma\n"
        );
    }

    #[test]
    fn file_apply_text_edits_rejects_overlapping_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "abcdef\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "edits": [
                        {"kind": "replace_exact", "old_text": "abc", "new_text": "ABC"},
                        {"kind": "replace_exact", "old_text": "cde", "new_text": "CDE"}
                    ]
                }),
            ),
        ));
        let err = out["error"].as_str().unwrap();
        assert!(err.contains("edits overlap"));
        assert!(err.contains("No files were modified"));
        assert_eq!(out["changed"], false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "abcdef\n");
    }

    #[test]
    fn agent_native_line_edit_replace_insert_delete_happy_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        let file = tmp.path().join("src/example.rs");
        std::fs::write(&file, "one\ntwo\nthree\nfour\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_replace_line_range",
                "src/example.rs",
                Some("TWO\nTHREE"),
                Some(2),
                Some(3),
                None,
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["start_line"], 2);
        assert_eq!(out["end_line"], 3);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "one\nTWO\nTHREE\nfour\n"
        );

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_insert_at_line",
                "src/example.rs",
                Some("middle"),
                None,
                None,
                Some(2),
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["line"], 2);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "one\nmiddle\nTWO\nTHREE\nfour\n"
        );

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_delete_line_range",
                "src/example.rs",
                None,
                Some(2),
                Some(3),
                None,
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["old_line_count"], 2);
        assert_eq!(out["new_line_count"], 0);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "one\nTHREE\nfour\n"
        );
    }

    #[test]
    fn agent_native_line_edit_guards_reject_without_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("example.rs");
        std::fs::write(&file, "one\ntwo\nthree\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_replace_line_range",
                "example.rs",
                Some("TWO"),
                Some(2),
                Some(2),
                None,
                Some(
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                ),
                None,
            ),
        ));
        assert_eq!(out["changed"], false);
        assert_eq!(out["error"], "expected_old_sha256 mismatch");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\ntwo\nthree\n");

        let anchor = sha256_hex_bytes("two\n".as_bytes());
        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_insert_at_line",
                "example.rs",
                Some("middle"),
                None,
                None,
                Some(2),
                Some(anchor),
                Some("three"),
            ),
        ));
        assert_eq!(out["changed"], false);
        assert_eq!(out["error"], "expected_anchor_prefix mismatch");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    fn agent_native_line_edit_rejects_ranges_utf8_sensitive_and_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("example.rs"), "one\ntwo\n").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_delete_line_range",
                "example.rs",
                None,
                Some(2),
                Some(3),
                None,
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], false);
        assert_eq!(out["error"], "invalid line range");

        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_insert_at_line",
                "example.rs",
                Some("three"),
                None,
                None,
                Some(3),
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], true);
        assert_eq!(out["old_line_count"], 0);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("example.rs")).unwrap(),
            "one\ntwo\nthree\n"
        );

        std::fs::write(tmp.path().join("bad.bin"), [0xff, 0xfe]).unwrap();
        let out = line_edit_json(handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_replace_line_range",
                "bad.bin",
                Some("ok"),
                Some(1),
                Some(1),
                None,
                None,
                None,
            ),
        ));
        assert_eq!(out["changed"], false);
        assert_eq!(out["error"], "file is not valid UTF-8");

        let sensitive = handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_replace_line_range",
                ".env",
                Some("SECRET=2"),
                Some(1),
                Some(1),
                None,
                None,
                None,
            ),
        );
        assert!(sensitive
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("sensitive"));

        let escaped = handle_file_request(
            &policy,
            &line_edit_request(
                tmp.path(),
                "file_replace_line_range",
                "../outside.txt",
                Some("x"),
                Some(1),
                Some(1),
                None,
                None,
                None,
            ),
        );
        assert!(escaped
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("escape"));
    }

    #[test]
    fn prepared_profile_run_shell_and_run_job_see_same_env() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("project");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir_all(&project_dir).unwrap();
        write_agent_project(&projects_dir, "demo", &project_dir, Some("test"));
        let shell = shell_with_profiles(
            None,
            vec![(
                "test",
                ShellProfileConfig {
                    env: profile_env(&[("WEBCODEX_TEST_PROFILE", "same")]),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let jobs = JobManager::new(1);
        let shell_result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            &projects_dir,
            &jobs.prepared_profiles,
            &project_dir,
            "printf %s \"$WEBCODEX_TEST_PROFILE\"",
        );
        assert_eq!(shell_result.stdout.as_deref(), Some("same"));

        let (sink, mut rx) = ws_sink("ws-client");
        dispatch_request(
            &sink,
            &AgentPolicy::default(),
            &shell,
            &jobs,
            &projects_dir,
            shell_job_request(&project_dir, "printf %s \"$WEBCODEX_TEST_PROFILE\""),
        )
        .unwrap();
        assert_eq!(wait_for_job_stdout(&mut rx), "same");
    }

    #[test]
    fn prepared_profile_init_script_runs_once_per_project_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let counter = tmp.path().join("prepare-count");
        let init_script = format!(
            "count=$(cat {:?} 2>/dev/null || echo 0)\ncount=$((count + 1))\nprintf '%s\\n' \"$count\" > {:?}\nexport WEBCODEX_TEST_PROFILE=counted",
            counter.to_string_lossy(),
            counter.to_string_lossy()
        );
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some(init_script),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let cache = PreparedShellProfileCache::default();
        for _ in 0..2 {
            let result = run_profile_shell(
                &AgentPolicy::default(),
                &shell,
                tmp.path(),
                &cache,
                tmp.path(),
                "printf %s \"$WEBCODEX_TEST_PROFILE\"",
            );
            assert_eq!(result.exit_code, Some(0), "{result:?}");
            assert_eq!(result.stdout.as_deref(), Some("counted"));
        }
        assert_eq!(std::fs::read_to_string(counter).unwrap().trim(), "1");
    }

    #[test]
    fn prepared_profile_init_script_stdout_noise_does_not_break_env_capture() {
        let tmp = tempfile::tempdir().unwrap();
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some(
                        "echo noise before env\nexport WEBCODEX_TEST_PROFILE=ok".to_string(),
                    ),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "printf %s \"$WEBCODEX_TEST_PROFILE\"",
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("ok"));
    }

    #[test]
    fn prepared_profile_errors_do_not_leak_init_script_body() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some(format!("export SECRET={secret}\nfalse")),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "true",
        );
        let err = result.error.expect("prepare should fail");
        assert!(err.contains("failed to prepare shell profile"), "{err}");
        assert!(!err.contains(secret), "{err}");
    }

    #[test]
    fn prepared_profile_filters_webcodex_token_env() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("WEBCODEX_TOKEN");
        std::env::set_var("WEBCODEX_TOKEN", "secret-token");
        let tmp = tempfile::tempdir().unwrap();
        let shell =
            shell_with_profiles(Some("test"), vec![("test", ShellProfileConfig::default())]);
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "if [ -z \"${WEBCODEX_TOKEN+x}\" ]; then printf absent; else printf present; fi",
        );
        match saved {
            Some(value) => std::env::set_var("WEBCODEX_TOKEN", value),
            None => std::env::remove_var("WEBCODEX_TOKEN"),
        }
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("absent"));
    }

    #[test]
    fn prepared_profile_missing_marker_is_reported_without_script_body() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    init_script: Some(format!("export SECRET={secret}\nexec >/dev/null")),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "true",
        );
        let err = result.error.expect("prepare should fail");
        assert!(err.contains("env marker not found"), "{err}");
        assert!(!err.contains(secret), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn prepared_profile_env_payload_parse_failure_is_reported() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let fake_env = bin.join("env");
        std::fs::write(&fake_env, "#!/bin/sh\nprintf 'bad\\000'\n").unwrap();
        let mut perms = std::fs::metadata(&fake_env).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_env, perms).unwrap();
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("/bin/sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    env: profile_env(&[("PATH", bin.to_string_lossy().as_ref())]),
                    init_script: Some("export WEBCODEX_TEST_PROFILE=ok".to_string()),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "true",
        );
        let err = result.error.expect("prepare should fail");
        assert!(err.contains("entry missing '='"), "{err}");
    }

    #[test]
    fn prepared_profile_program_spawn_failure_mentions_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let shell = shell_with_profiles(
            Some("test"),
            vec![(
                "test",
                ShellProfileConfig {
                    program: Some("/definitely/missing/webcodex-shell".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            "true",
        );
        let err = result.error.expect("spawn should fail");
        assert!(
            err.contains("failed to spawn shell profile 'test'"),
            "{err}"
        );
    }

    #[test]
    fn project_shell_profile_missing_profile_returns_clear_error() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("project");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir_all(&project_dir).unwrap();
        write_agent_project(&projects_dir, "demo", &project_dir, Some("missing"));
        let result = run_profile_shell(
            &AgentPolicy::default(),
            &ShellConfig::default(),
            &projects_dir,
            &PreparedShellProfileCache::default(),
            &project_dir,
            "true",
        );
        let err = result.error.expect("profile should be missing");
        assert!(
            err.contains("project 'demo' shell_profile 'missing'"),
            "{err}"
        );
    }

    #[test]
    fn shell_job_success_and_failure_results_are_structured() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();

        let success = run_shell(
            &cfg.policy,
            &cfg.shell,
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

        let failure = run_shell(
            &cfg.policy,
            &cfg.shell,
            Some(&cwd),
            "exit 7",
            None,
            10,
            None,
        );
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
            &cfg.shell,
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

        let result = run_shell(
            &cfg.policy,
            &cfg.shell,
            Some(&cwd),
            "sleep 2",
            None,
            1,
            None,
        );
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
            &cfg.shell,
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
            &cfg.shell,
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
    fn register_request_announces_correct_protocol_version() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        for (version, expected_str) in [
            (AGENT_PROTOCOL_VERSION_POLLING_V1, "polling-v1"),
            (AGENT_PROTOCOL_VERSION_WEBSOCKET_V1, "websocket-v1"),
            (AGENT_PROTOCOL_VERSION_QUIC_V1, "quic-v1"),
        ] {
            let body = build_register_request(&cfg, Vec::new(), version, "inst-1", 0);
            assert_eq!(body.agent_instance_id, "inst-1");
            assert_eq!(
                body.agent_protocol_version.as_deref(),
                Some(version),
                "version mismatch for {expected_str}"
            );
            assert_eq!(body.agent_protocol_version.as_deref(), Some(expected_str));
        }
        // Also verify capabilities are advertised (check once for polling).
        let body = build_register_request(
            &cfg,
            Vec::new(),
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            "inst-1",
            0,
        );
        let caps = body.capabilities.expect("agent registers capabilities");
        assert!(caps.shell);
        assert!(caps.file_read);
        assert!(caps.file_write);
        assert!(caps.async_jobs);
        assert!(caps.async_shell_jobs);
    }

    #[test]
    fn register_request_carries_sanitized_shell_profiles_summary() {
        // A config with one profile carrying a secret env value and a secret
        // init_script body. The sanitized summary must report the profile name,
        // has_init_script=true, and env_keys_count, but MUST NOT include the env
        // value or the init_script body.
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = test_config(tmp.path().join("config/projects.d"));
        let secret_env = "DO_NOT_LEAK_THIS_ENV_VALUE";
        let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
        cfg.shell = shell_with_profiles(
            Some("rust"),
            vec![(
                "rust",
                ShellProfileConfig {
                    program: Some("sh".to_string()),
                    args: Some(vec!["-c".to_string()]),
                    env: profile_env(&[("SECRET_KEY", secret_env)]),
                    init_script: Some(secret_script.to_string()),
                    ..ShellProfileConfig::default()
                },
            )],
        );
        let body = build_register_request(
            &cfg,
            Vec::new(),
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            "inst-1",
            0,
        );
        let policy = body.policy.expect("agent registers a policy");
        let summary = policy
            .shell_profiles
            .as_ref()
            .expect("sanitized shell profiles summary is present");
        assert_eq!(summary.default_profile.as_deref(), Some("rust"));
        assert_eq!(summary.configured_count, 1);
        assert_eq!(summary.profiles.len(), 1);
        let entry = &summary.profiles[0];
        assert_eq!(entry.name, "rust");
        assert!(entry.has_init_script);
        assert_eq!(entry.env_keys_count, 1);
        assert_eq!(entry.program, "sh");
        assert_eq!(entry.args_count, 1);
        // Sanitization: the rendered summary never carries env values or the
        // init_script body.
        let rendered = serde_json::to_string(summary).unwrap();
        assert!(!rendered.contains(secret_env), "{rendered}");
        assert!(!rendered.contains(secret_script), "{rendered}");
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
    fn generated_agent_instance_id_is_non_empty_uuid_like() {
        // `run_agent` generates the instance id the same way; verify the
        // format here without driving the full agent loop.
        let id = uuid::Uuid::new_v4().to_string();
        assert!(!id.is_empty());
        // Canonical UUID v4 is 36 chars: 8-4-4-4-12 hex groups.
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        // The register builder carries it through unchanged.
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let body =
            build_register_request(&cfg, Vec::new(), AGENT_PROTOCOL_VERSION_POLLING_V1, &id, 0);
        assert_eq!(body.agent_instance_id, id);
        assert!(!body.agent_instance_id.is_empty());
    }

    fn ws_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
        (
            AgentSink::WebSocket {
                tx,
                client_id: client_id.to_string(),
                agent_instance_id: "ws-inst".to_string(),
            },
            rx,
        )
    }

    fn quic_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
        (
            AgentSink::Quic {
                tx,
                client_id: client_id.to_string(),
                agent_instance_id: "quic-inst".to_string(),
            },
            rx,
        )
    }

    #[test]
    fn sink_submit_result_sends_result_envelope() {
        type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
        for (label, make_sink, expected_client, expected_instance) in [
            ("ws", ws_sink as SinkFactory, "ws-client", "ws-inst"),
            ("quic", quic_sink as SinkFactory, "quic-client", "quic-inst"),
        ] {
            let (sink, mut rx) = make_sink(expected_client);
            let result = CommandResult {
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(3),
                error: None,
            };
            assert!(
                sink.submit_result("req-9".to_string(), result).unwrap(),
                "{label}"
            );
            let env = rx.try_recv().expect("envelope was sent");
            match env {
                AgentEnvelope::Result { payload } => {
                    assert_eq!(payload.client_id, expected_client, "{label}");
                    assert_eq!(payload.agent_instance_id, expected_instance, "{label}");
                    assert_eq!(payload.request_id, "req-9");
                    assert_eq!(payload.exit_code, Some(0));
                    assert_eq!(payload.stdout.as_deref(), Some("hi"));
                }
                other => panic!("{label}: expected result, got {:?}", other.kind()),
            }
        }
    }

    #[test]
    fn sink_send_job_update_sends_job_update_envelope() {
        type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
        for (label, make_sink, expected_client) in [
            ("ws", ws_sink as SinkFactory, "ws-client"),
            ("quic", quic_sink as SinkFactory, "quic-client"),
        ] {
            let (sink, mut rx) = make_sink(expected_client);
            let body = ShellAgentJobUpdateRequest {
                client_id: expected_client.to_string(),
                agent_instance_id: sink.agent_instance_id().to_string(),
                job_id: "job-1".to_string(),
                request_id: Some("req-1".to_string()),
                status: "running".to_string(),
                stdout_chunk: Some(format!("{label}-chunk")),
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
                    assert_eq!(payload.client_id, expected_client, "{label}");
                    assert_eq!(
                        payload.agent_instance_id,
                        sink.agent_instance_id(),
                        "{label}"
                    );
                    assert_eq!(payload.job_id, "job-1", "{label}");
                    assert_eq!(payload.status, "running", "{label}");
                    assert_eq!(
                        payload.stdout_chunk.as_deref(),
                        Some(format!("{label}-chunk").as_str()),
                        "{label}"
                    );
                }
                other => panic!("{label}: expected job_update, got {:?}", other.kind()),
            }
        }
    }

    #[test]
    fn file_request_kind_includes_anchor_edit_ops() {
        for kind in [
            "file_read",
            "file_write",
            "file_list",
            "file_replace_line_range",
            "file_insert_at_line",
            "file_delete_line_range",
            "file_replace_exact_block",
            "file_insert_before_pattern",
            "file_insert_after_pattern",
        ] {
            assert!(
                is_file_request_kind(kind),
                "{kind} should route to file handler"
            );
        }
        assert!(!is_file_request_kind("run_shell"));
    }

    #[test]
    fn dispatch_request_anchor_edit_routes_to_file_handler() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let cwd = tmp.path().to_string_lossy().to_string();
        std::fs::write(tmp.path().join("anchor.txt"), "old block\n").unwrap();
        let (sink, mut rx) = ws_sink("ws-client");
        let jobs = JobManager::new(max_concurrent_jobs(&cfg));
        let request = ShellAgentShellRequest {
            request_id: "req-anchor".to_string(),
            client_id: "ws-client".to_string(),
            kind: "file_replace_exact_block".to_string(),
            job_id: None,
            cwd: Some(cwd),
            path: Some("anchor.txt".to_string()),
            content: Some("new block\n".to_string()),
            max_bytes: None,
            old_text: Some("old block\n".to_string()),
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
        };
        let pdir = projects_dir(&cfg);
        let ran = dispatch_request(&sink, &cfg.policy, &cfg.shell, &jobs, &pdir, request).unwrap();
        assert!(ran);
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.request_id, "req-anchor");
                assert_eq!(payload.exit_code, Some(0));
                let stdout = payload.stdout.expect("file handler returns JSON stdout");
                assert!(stdout.contains("\"changed\":true"), "stdout was {stdout}");
                assert_eq!(
                    std::fs::read_to_string(tmp.path().join("anchor.txt")).unwrap(),
                    "new block\n"
                );
            }
            other => panic!("expected result, got {:?}", other.kind()),
        }
    }

    #[test]
    fn dispatch_request_run_shell_sends_result_over_sink() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let jobs = JobManager::new(max_concurrent_jobs(&cfg));
        let pdir = projects_dir(&cfg);

        type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
        for (label, make_sink, client_id, cmd) in [
            ("ws", ws_sink as SinkFactory, "ws-client", "printf wsok"),
            (
                "quic",
                quic_sink as SinkFactory,
                "quic-client",
                "printf quic-ok",
            ),
        ] {
            let (sink, mut rx) = make_sink(client_id);
            let request = ShellAgentShellRequest {
                request_id: format!("req-{label}"),
                client_id: client_id.to_string(),
                kind: "run_shell".to_string(),
                job_id: None,
                cwd: Some(tmp.path().to_string_lossy().to_string()),
                path: None,
                content: None,
                max_bytes: None,
                old_text: None,
                pattern: None,
                expected_sha256: None,
                expected_prefix: None,
                start_line: None,
                end_line: None,
                line: None,
                create_dirs: false,
                command: cmd.to_string(),
                stdin: None,
                timeout_secs: 10,
                requested_by: "tester".to_string(),
                created_at: 0,
            };
            let ran =
                dispatch_request(&sink, &cfg.policy, &cfg.shell, &jobs, &pdir, request).unwrap();
            assert!(ran, "{label}");
            let env = rx.try_recv().expect("result envelope was sent");
            match env {
                AgentEnvelope::Result { payload } => {
                    assert_eq!(payload.request_id, format!("req-{label}"));
                    assert_eq!(payload.exit_code, Some(0));
                    assert_eq!(
                        payload.stdout.as_deref(),
                        Some(cmd.split_whitespace().last().unwrap())
                    );
                }
                other => panic!("{label}: expected result, got {:?}", other.kind()),
            }
        }
    }

    fn project_policy(root: &Path) -> AgentPolicy {
        AgentPolicy {
            allow_cwd_anywhere: false,
            allowed_roots: vec![root.to_path_buf()],
            ..AgentPolicy::default()
        }
    }

    fn project_request(kind: &str, payload: serde_json::Value) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: format!("req-{}", kind),
            client_id: "oe".to_string(),
            kind: kind.to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: None,
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: Some(payload.to_string()),
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
        }
    }

    fn project_ok(result: CommandResult) -> serde_json::Value {
        assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
    }

    fn project_err(result: CommandResult) -> String {
        assert!(
            result.exit_code.is_none(),
            "unexpected success: {:?}",
            result
        );
        result.error.expect("error")
    }

    #[test]
    fn register_project_writes_valid_toml_into_projects_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("repo");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir(&project_dir).unwrap();
        let policy = project_policy(tmp.path());
        let req = project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy(),
                "description": "A demo project",
                "allow_patch": false
            }),
        );

        let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
        assert_eq!(value["created_config"], true);
        assert_eq!(value["overwritten"], false);

        let content = std::fs::read_to_string(projects_dir.join("demo.toml")).unwrap();
        let parsed = parse_agent_project_toml(&content).unwrap();
        assert_eq!(parsed.id, "demo");
        assert_eq!(parsed.name.as_deref(), Some("Demo"));
        assert_eq!(parsed.path, project_dir.to_string_lossy());
        assert!(!parsed.allow_patch);
    }

    #[test]
    fn register_project_rejects_path_outside_allowed_roots() {
        let allowed = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let projects_dir = allowed.path().join("projects.d");
        let policy = project_policy(allowed.path());
        let req = project_request(
            "register_project",
            serde_json::json!({
                "id": "outside",
                "name": "Outside",
                "path": outside.path().to_string_lossy()
            }),
        );

        let err = project_err(handle_project_op(&policy, &projects_dir, &req));
        assert!(err.contains("outside allowed_roots"), "{err}");
        assert!(!projects_dir.join("outside.toml").exists());
    }

    #[test]
    fn register_project_rejects_dangerous_subpaths_without_explicit_root() {
        let policy = AgentPolicy {
            allow_cwd_anywhere: true,
            allowed_roots: Vec::new(),
            ..AgentPolicy::default()
        };

        for path in [
            "/etc/nginx",
            "/usr/local",
            "/var/lib",
            "/proc/self",
            "/dev/shm",
        ] {
            let err = validate_project_path_policy(&policy, Path::new(path)).unwrap_err();
            assert!(err.contains("dangerous system root"), "{path}: {err}");
        }

        validate_project_path_policy(&policy, Path::new("/usr2/local")).unwrap();
    }

    #[test]
    fn load_config_defaults_empty_allowed_roots_to_home() {
        let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(home) = home {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("agent.toml");
            std::fs::write(
                &path,
                "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n",
            )
            .unwrap();
            let cfg = load_config(&path).unwrap();
            assert_eq!(
                cfg.policy.allowed_roots,
                vec![home],
                "empty allowed_roots must default to HOME"
            );
        }
    }

    #[test]
    fn load_config_explicit_allowed_roots_override_home_default() {
        let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n[policy]\nallowed_roots = [\"/root/git\"]\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(
            cfg.policy.allowed_roots,
            vec![PathBuf::from("/root/git")],
            "explicit allowed_roots must override the HOME default"
        );
    }

    #[test]
    fn load_config_empty_roots_without_home_and_no_cwd_anywhere_errors() {
        let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("HOME");
        std::env::remove_var("HOME");
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n\
             [policy]\nallow_cwd_anywhere = false\n",
        )
        .unwrap();
        let err = load_config(&path).unwrap_err();
        assert!(err.contains("allowed_roots is empty"));
        match saved {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn register_project_overwrite_semantics_are_accurate() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("repo");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir(&project_dir).unwrap();
        let policy = project_policy(tmp.path());
        let payload = |overwrite| {
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy(),
                "overwrite": overwrite
            })
        };

        let first = project_ok(handle_project_op(
            &policy,
            &projects_dir,
            &project_request("register_project", payload(false)),
        ));
        assert_eq!(first["created_config"], true);
        assert_eq!(first["overwritten"], false);

        let err = project_err(handle_project_op(
            &policy,
            &projects_dir,
            &project_request("register_project", payload(false)),
        ));
        assert!(err.contains("already exists"), "{err}");

        let overwritten = project_ok(handle_project_op(
            &policy,
            &projects_dir,
            &project_request("register_project", payload(true)),
        ));
        assert_eq!(overwritten["created_config"], false);
        assert_eq!(overwritten["overwritten"], true);
    }

    #[test]
    fn create_project_basic_creates_readme_and_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("new-project");
        let projects_dir = tmp.path().join("projects.d");
        let policy = project_policy(tmp.path());
        let req = project_request(
            "create_project",
            serde_json::json!({
                "id": "basic",
                "name": "Basic",
                "path": project_dir.to_string_lossy(),
                "description": "Basic template",
                "template": "basic"
            }),
        );

        let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
        assert_eq!(value["created_directory"], true);
        assert!(project_dir.join("README.md").exists());
        assert!(project_dir.join(".gitignore").exists());
        assert!(std::fs::read_to_string(project_dir.join("README.md"))
            .unwrap()
            .contains("Basic template"));
    }

    #[test]
    fn create_project_rejects_existing_non_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("existing");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir(&project_dir).unwrap();
        let keep = project_dir.join("keep.txt");
        std::fs::write(&keep, "keep").unwrap();
        let policy = project_policy(tmp.path());
        let req = project_request(
            "create_project",
            serde_json::json!({
                "id": "existing",
                "name": "Existing",
                "path": project_dir.to_string_lossy(),
                "template": "basic",
                "allow_existing_empty": true
            }),
        );

        let err = project_err(handle_project_op(&policy, &projects_dir, &req));
        assert!(err.contains("not empty"), "{err}");
        assert_eq!(std::fs::read_to_string(keep).unwrap(), "keep");
    }

    #[test]
    fn create_project_rejects_unknown_template() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("new-project");
        let projects_dir = tmp.path().join("projects.d");
        let policy = project_policy(tmp.path());
        let req = project_request(
            "create_project",
            serde_json::json!({
                "id": "badtemplate",
                "name": "Bad Template",
                "path": project_dir.to_string_lossy(),
                "template": "cargo"
            }),
        );

        let err = project_err(handle_project_op(&policy, &projects_dir, &req));
        assert!(err.contains("unknown template"), "{err}");
        assert!(!project_dir.exists());
    }

    #[test]
    fn create_project_created_config_and_overwritten_semantics_are_accurate() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("empty-project");
        let projects_dir = tmp.path().join("projects.d");
        let policy = project_policy(tmp.path());
        let payload = |overwrite| {
            serde_json::json!({
                "id": "empty",
                "name": "Empty",
                "path": project_dir.to_string_lossy(),
                "template": "empty",
                "allow_existing_empty": true,
                "overwrite": overwrite
            })
        };

        let first = project_ok(handle_project_op(
            &policy,
            &projects_dir,
            &project_request("create_project", payload(false)),
        ));
        assert_eq!(first["created_directory"], true);
        assert_eq!(first["created_config"], true);
        assert_eq!(first["overwritten"], false);

        let second = project_ok(handle_project_op(
            &policy,
            &projects_dir,
            &project_request("create_project", payload(true)),
        ));
        assert_eq!(second["created_directory"], false);
        assert_eq!(second["created_config"], false);
        assert_eq!(second["overwritten"], true);
    }

    #[test]
    fn create_project_cleanup_removes_only_files_created_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("existing-empty");
        std::fs::create_dir(&project_dir).unwrap();
        let projects_dir_file = tmp.path().join("projects.d-is-file");
        std::fs::write(&projects_dir_file, "not a dir").unwrap();
        let policy = project_policy(tmp.path());
        let req = project_request(
            "create_project",
            serde_json::json!({
                "id": "cleanup",
                "name": "Cleanup",
                "path": project_dir.to_string_lossy(),
                "template": "basic",
                "allow_existing_empty": true
            }),
        );

        let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
        assert!(err.contains("projects_dir"), "{err}");
        assert!(project_dir.exists());
        assert!(!project_dir.join("README.md").exists());
        assert!(!project_dir.join(".gitignore").exists());
    }

    #[test]
    fn create_project_does_not_delete_pre_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("existing");
        std::fs::create_dir(&project_dir).unwrap();
        let pre_existing = project_dir.join("pre-existing.txt");
        std::fs::write(&pre_existing, "original").unwrap();
        let projects_dir_file = tmp.path().join("projects.d-is-file");
        std::fs::write(&projects_dir_file, "not a dir").unwrap();
        let policy = project_policy(tmp.path());
        let req = project_request(
            "create_project",
            serde_json::json!({
                "id": "keep",
                "name": "Keep",
                "path": project_dir.to_string_lossy(),
                "template": "basic",
                "allow_existing_empty": true
            }),
        );

        let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
        assert!(err.contains("not empty"), "{err}");
        assert_eq!(std::fs::read_to_string(pre_existing).unwrap(), "original");
    }

    #[test]
    fn agent_project_cache_invalidate_refreshes_after_project_op() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("repo");
        let projects_dir = tmp.path().join("projects.d");
        std::fs::create_dir(&project_dir).unwrap();
        let mut cfg = test_config(projects_dir.clone());
        cfg.policy = project_policy(tmp.path());
        let mut cache = AgentProjectCache::default();
        assert!(cache.get(&cfg).is_empty());

        let req = project_request(
            "register_project",
            serde_json::json!({
                "id": "cached",
                "name": "Cached",
                "path": project_dir.to_string_lossy()
            }),
        );
        project_ok(handle_project_op(&cfg.policy, &projects_dir, &req));

        assert!(
            cache.get(&cfg).is_empty(),
            "cache should still be stale before invalidation"
        );
        cache.invalidate();
        let projects = cache.get(&cfg);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "cached");
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
            agent_instance_id: "inst-1".to_string(),
        });
        assert_eq!(sink.client_id(), "oe");
        assert_eq!(sink.agent_instance_id(), "inst-1");
    }

    #[test]
    fn empty_tokens_are_not_sent_as_credentials() {
        use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

        let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "").unwrap();
        assert!(request.headers().get(AUTHORIZATION).is_none());

        let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "   \t").unwrap();
        assert!(request.headers().get(AUTHORIZATION).is_none());

        let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "  abc123  ").unwrap();
        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer abc123"
        );

        assert_eq!(non_empty_token(""), None);
        assert_eq!(non_empty_token("   \t"), None);
        assert_eq!(non_empty_token("  abc123  "), Some("abc123".to_string()));
    }

    #[test]
    fn empty_tokens_http_register_omits_authorization_header() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0u8; 16 * 1024];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(
                request.starts_with("POST /api/shell/agent/register "),
                "unexpected request: {request}"
            );
            assert!(
                !request.to_ascii_lowercase().contains("authorization:"),
                "empty token must not send Authorization header: {request}"
            );
            let body = r#"{"success":true,"client":null,"error":null}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = test_config(tmp.path().join("projects.d"));
        cfg.server_url = format!("http://{}", addr);
        cfg.token = "   \t".to_string();

        let client = Client::builder().no_proxy().build().unwrap();
        let mut project_cache = AgentProjectCache::default();
        register(&client, &cfg, &mut project_cache, "inst-empty-token", 0).unwrap();
        server.join().unwrap();
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

        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            websocket_session(&cfg, Vec::new(), "inst-1"),
        )
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
