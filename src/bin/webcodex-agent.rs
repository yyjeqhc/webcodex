use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

use shell_protocol::{
    read_quic_frame, write_quic_frame, AgentEnvelope, AgentPolicySummary, QuicFrameError,
    ShellAgentJobUpdateRequest, ShellAgentJobUpdateResponse, ShellAgentPollRequest,
    ShellAgentPollResponse, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentResultResponse, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellClientRegisterResponse, ShellProfileSummaryEntry,
    ShellProfilesSummary, AGENT_PROTOCOL_VERSION_POLLING_V1, AGENT_PROTOCOL_VERSION_QUIC_V1,
    AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};

// Shared agent-config initialization (types, validation, TOML generation,
// 0600 file writing, HOME-default allowed_roots). Reused by `webcodex-cli`.
use agent_init::{
    effective_allowed_roots, parse_bool, required_value, run_agent_init,
    validate_agent_init_options, AgentInitOptions, DEFAULT_INIT_PROJECTS_DIR,
    DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_TIMEOUT_SECS, DEFAULT_POLL_INTERVAL_MS,
    TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET,
};

const DEFAULT_CONFIG_PATH: &str = "/etc/webcodex/agent.toml";
const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const PROJECT_SCAN_CACHE_MS: u64 = 5000;
const SHELL_PROFILE_PREPARE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_CONCURRENT_JOBS: usize = 2;
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
    /// Experimental custom QUIC agent transport config (Phase 5A). Only used
    /// when `transport = "quic"`. `None` keeps the default websocket/polling
    /// behavior unchanged.
    #[serde(default)]
    quic: Option<QuicClientConfig>,
    #[serde(default)]
    shell: ShellConfig,
}

/// Agent-side QUIC transport configuration (`[quic]` in `agent.toml`). All
/// fields are required when `transport = "quic"`; `run_quic_agent` validates
/// them before connecting. The token is NOT stored here — it stays in the
/// top-level `token` field and is carried in the `Register` envelope's
/// `auth_token` field, mirroring the `Authorization: Bearer` header used by
/// the websocket/polling paths.
#[derive(Debug, Clone, Deserialize)]
struct QuicClientConfig {
    /// `host:port` of the server's QUIC listener (e.g. `host:8443`).
    server_addr: String,
    /// TLS SNI / server name to verify the certificate against. Must match the
    /// cert's SAN (typically the domain name).
    server_name: String,
    /// ALPN protocol; must match the server's `WEBCODEX_QUIC_ALPN`.
    #[serde(default = "default_quic_alpn")]
    alpn: String,
    /// Connection timeout in seconds.
    #[serde(default = "default_quic_connect_timeout_secs")]
    connect_timeout_secs: u64,
    /// QUIC keepalive interval in seconds.
    #[serde(default = "default_quic_keepalive_interval_secs")]
    keepalive_interval_secs: u64,
}

fn default_quic_alpn() -> String {
    "webcodex-agent/1".to_string()
}
fn default_quic_connect_timeout_secs() -> u64 {
    10
}
fn default_quic_keepalive_interval_secs() -> u64 {
    20
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ShellConfig {
    #[serde(default)]
    default_profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, ShellProfileConfig>,
    #[serde(default = "default_shell_program")]
    program: String,
    #[serde(default = "default_shell_args")]
    args: Vec<String>,
    #[serde(default)]
    path_prepend: Vec<PathBuf>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    init_script: Option<PathBuf>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            default_profile: None,
            profiles: BTreeMap::new(),
            program: default_shell_program(),
            args: default_shell_args(),
            path_prepend: Vec::new(),
            env: HashMap::new(),
            init_script: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
struct ShellProfileConfig {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    program: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    init_script: Option<String>,
}

fn default_shell_program() -> String {
    "sh".to_string()
}

fn default_shell_args() -> Vec<String> {
    vec!["-c".to_string()]
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

/// Minimal HTTP send configuration used by the polling `AgentSink`. We do not
/// store the whole `AgentConfig` here: policy and concurrency limits stay
/// with the agent config and are passed alongside the sink.
#[derive(Debug, Clone)]
struct HttpSendConfig {
    client: Client,
    server_url: String,
    token: String,
    client_id: String,
    agent_instance_id: String,
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
        agent_instance_id: String,
    },
}

impl AgentSink {
    fn client_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.client_id,
            AgentSink::WebSocket { client_id, .. } => client_id,
        }
    }

    /// Active agent process identity carried by this sink so every result /
    /// job_update submission includes it.
    fn agent_instance_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.agent_instance_id,
            AgentSink::WebSocket {
                agent_instance_id, ..
            } => agent_instance_id,
        }
    }

    /// Submit the result of a synchronous shell/file request. Mirrors the old
    /// `submit_result` free function but routes over the active transport.
    fn submit_result(&self, request_id: String, result: CommandResult) -> Result<bool, String> {
        let body = ShellAgentResultRequest {
            client_id: self.client_id().to_string(),
            agent_instance_id: self.agent_instance_id().to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProjectFile {
    id: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shell_profile: Option<String>,
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

#[derive(Debug, Clone)]
struct AgentProjectShellContext {
    id: String,
    path: String,
    shell_profile: Option<String>,
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
       --once                     Poll once, then exit (polling transport)\n\n\
     Init options:\n\
       --server-url URL           WebCodex server URL\n\
       --token TOKEN              Agent token for generated config\n\
       --token-file PATH          Read agent token from file\n\
       --client-id ID             Stable agent client id\n\
       --owner USER               Owner username\n\
       --display-name NAME        Human-readable agent name\n\
       --transport NAME           websocket (default), polling, or quic (experimental)\n\
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
                    stdout: format!("webcodex-agent {}\n", env!("CARGO_PKG_VERSION")),
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
                    stdout: format!("webcodex-agent {}\n", env!("CARGO_PKG_VERSION")),
                    stderr: String::new(),
                });
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

fn validate_env_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('=')
        && !key.contains('\0')
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn validate_shell_profile_name(context: &str, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} cannot be empty", context));
    }
    if name.contains("..") {
        return Err(format!("{} cannot contain '..'", context));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("{} cannot contain slash or backslash", context));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '_', '-', and '.'",
            context
        ));
    }
    Ok(())
}

fn validate_shell_profile_config(name: &str, profile: &ShellProfileConfig) -> Result<(), String> {
    if profile
        .program
        .as_ref()
        .is_some_and(|program| program.trim().is_empty())
    {
        return Err(format!("shell.profiles.{}.program cannot be empty", name));
    }
    if let Some(args) = &profile.args {
        if args.is_empty() {
            return Err(format!(
                "shell.profiles.{}.args must include the command flag, for example [\"-c\"]",
                name
            ));
        }
        if args.iter().any(|arg| arg.trim().is_empty()) {
            return Err(format!(
                "shell.profiles.{}.args cannot contain empty values",
                name
            ));
        }
    }
    for key in profile.env.keys() {
        if !validate_env_key(key) {
            return Err(format!(
                "shell.profiles.{}.env contains invalid key '{}'",
                name, key
            ));
        }
    }
    if profile
        .init_script
        .as_ref()
        .is_some_and(|script| script.trim().is_empty())
    {
        return Err(format!(
            "shell.profiles.{}.init_script cannot be empty",
            name
        ));
    }
    Ok(())
}

fn validate_shell_config(shell: &ShellConfig) -> Result<(), String> {
    if let Some(default_profile) = &shell.default_profile {
        validate_shell_profile_name("shell.default_profile", default_profile)?;
        if !shell.profiles.contains_key(default_profile) {
            return Err(format!(
                "shell.default_profile '{}' does not match any shell.profiles entry",
                default_profile
            ));
        }
    }
    for (name, profile) in &shell.profiles {
        validate_shell_profile_name("shell profile name", name)?;
        validate_shell_profile_config(name, profile)?;
    }
    if shell.program.trim().is_empty() {
        return Err("shell.program cannot be empty".to_string());
    }
    if shell.args.is_empty() {
        return Err("shell.args must include the command flag, for example [\"-c\"]".to_string());
    }
    if shell.args.iter().any(|arg| arg.trim().is_empty()) {
        return Err("shell.args cannot contain empty values".to_string());
    }
    if shell
        .path_prepend
        .iter()
        .any(|path| path.as_os_str().is_empty())
    {
        return Err("shell.path_prepend cannot contain empty paths".to_string());
    }
    for key in shell.env.keys() {
        if !validate_env_key(key) {
            return Err(format!("shell.env contains invalid key '{}'", key));
        }
    }
    if shell
        .init_script
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err("shell.init_script cannot be empty".to_string());
    }
    Ok(())
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

fn validate_optional_toml_string(
    table: &toml::map::Map<String, toml::Value>,
    field: &str,
    path: &str,
) -> Result<(), String> {
    if table
        .get(field)
        .is_some_and(|value| !matches!(value, toml::Value::String(_)))
    {
        return Err(format!("{} must be a string", path));
    }
    Ok(())
}

fn validate_shell_profile_toml_shape(content: &str) -> Result<(), String> {
    let value: toml::Value = toml::from_str(content)
        .map_err(|e| format!("failed to parse config TOML syntax: {}", e))?;
    let Some(shell) = value.get("shell") else {
        return Ok(());
    };
    let Some(shell) = shell.as_table() else {
        return Err("shell must be a table".to_string());
    };
    validate_optional_toml_string(shell, "default_profile", "shell.default_profile")?;
    let Some(profiles) = shell.get("profiles") else {
        return Ok(());
    };
    let Some(profiles) = profiles.as_table() else {
        return Err("shell.profiles must be a table".to_string());
    };
    for (name, profile) in profiles {
        let Some(profile) = profile.as_table() else {
            return Err(format!("shell.profiles.{} must be a table", name));
        };
        validate_optional_toml_string(
            profile,
            "description",
            &format!("shell.profiles.{}.description", name),
        )?;
        validate_optional_toml_string(
            profile,
            "program",
            &format!("shell.profiles.{}.program", name),
        )?;
        validate_optional_toml_string(
            profile,
            "init_script",
            &format!("shell.profiles.{}.init_script", name),
        )?;
        if let Some(args) = profile.get("args") {
            let Some(args) = args.as_array() else {
                return Err(format!(
                    "shell.profiles.{}.args must be a string array",
                    name
                ));
            };
            if args
                .iter()
                .any(|arg| !matches!(arg, toml::Value::String(_)))
            {
                return Err(format!(
                    "shell.profiles.{}.args must be a string array",
                    name
                ));
            }
        }
        if let Some(env) = profile.get("env") {
            let Some(env) = env.as_table() else {
                return Err(format!("shell.profiles.{}.env must be a string map", name));
            };
            if env
                .values()
                .any(|value| !matches!(value, toml::Value::String(_)))
            {
                return Err(format!("shell.profiles.{}.env must be a string map", name));
            }
        }
    }
    Ok(())
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

fn load_config(path: &Path) -> Result<AgentConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read config {}: {}", path.display(), e))?;
    validate_shell_profile_toml_shape(&content)
        .map_err(|e| format!("failed to parse config {}: {}", path.display(), e))?;
    let mut cfg: AgentConfig = toml::from_str(&content)
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
    // Phase 5A: when allowed_roots is missing/empty, default to [$HOME] so a
    // minimal agent.toml without an explicit policy.allowed_roots still works
    // predictably. If HOME is unavailable and allow_cwd_anywhere is false,
    // surface a clear configuration error. Explicit allowed_roots is preserved
    // as-is and overrides the HOME default.
    let effective =
        effective_allowed_roots(&cfg.policy.allowed_roots, cfg.policy.allow_cwd_anywhere)?;
    cfg.policy.allowed_roots = effective;
    validate_shell_config(&cfg.shell)?;
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
    if let Some(shell_profile) = &project.shell_profile {
        validate_shell_profile_name("project.shell_profile", shell_profile)?;
    }
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

fn load_agent_project_shell_contexts_from_dir(dir: &Path) -> Vec<AgentProjectShellContext> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(project) = parse_agent_project_toml(&content) else {
            continue;
        };
        if project.disabled || !seen.insert(project.id.clone()) {
            continue;
        }
        projects.push(AgentProjectShellContext {
            id: project.id,
            path: project.path,
            shell_profile: project.shell_profile,
        });
    }
    projects
}

fn find_project_shell_context(
    projects_dir: &Path,
    cwd_path: &Path,
) -> Option<AgentProjectShellContext> {
    let cwd = cwd_path.canonicalize().ok()?;
    load_agent_project_shell_contexts_from_dir(projects_dir)
        .into_iter()
        .filter_map(|project| {
            let project_path = PathBuf::from(&project.path).canonicalize().ok()?;
            if cwd == project_path || cwd.starts_with(&project_path) {
                Some((project_path.components().count(), project))
            } else {
                None
            }
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, project)| project)
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
        shell_profile: project.shell_profile.clone(),
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

    fn invalidate(&mut self) {
        self.projects.clear();
        self.refreshed_at = None;
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

/// System directories that must never be used as a project root unless they are
/// explicitly under an `allowed_roots` entry. Even when `allow_cwd_anywhere`
/// is true, these roots are rejected to prevent accidental registration of
/// critical system paths.
const DANGEROUS_PROJECT_ROOTS: &[&str] = &[
    "/", "/etc", "/bin", "/sbin", "/usr", "/var", "/proc", "/sys", "/dev", "/run", "/boot",
];

/// Escape a string for use as a TOML basic string (double-quoted). NUL is
/// rejected up front by validation, so we only handle backslash, quote, and
/// common control characters.
fn toml_basic_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Build a deterministic project TOML string compatible with the existing
/// `parse_agent_project_toml` parser. The field order is fixed so the output
/// is reproducible.
fn build_project_toml(
    id: &str,
    name: &str,
    path: &str,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    let mut toml = String::new();
    toml.push_str(&format!("id = {}\n", toml_basic_string(id)));
    toml.push_str(&format!("name = {}\n", toml_basic_string(name)));
    toml.push_str(&format!("path = {}\n", toml_basic_string(path)));
    if let Some(desc) = description {
        toml.push_str(&format!("description = {}\n", toml_basic_string(desc)));
    }
    toml.push_str(&format!("allow_patch = {}\n", allow_patch));
    toml
}

/// Validate the project `id` for project-management operations. Stricter than
/// the existing `validate_project_id`: no dots (prevents any path-like
/// interpretation), only ASCII letters/digits/dash/underscore.
fn validate_project_op_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id.contains('\0') {
        return Err("id must not contain NUL".to_string());
    }
    if id.len() > 64 {
        return Err("id must be at most 64 characters".to_string());
    }
    if id.contains('/') || id.contains('\\') {
        return Err("id must not contain slash or backslash".to_string());
    }
    if id == ".." || id == "." || id.contains("..") {
        return Err("id must not contain dot-dot traversal".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(())
}

/// Validate the project `name`: non-empty after trim, <= 120 chars, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 characters".to_string());
    }
    Ok(())
}

/// Validate the optional `description`: <= 500 chars, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 characters".to_string());
    }
    Ok(())
}

/// Check whether a canonicalized project path is allowed by the agent policy.
/// Returns Ok(()) if the path is safe, Err otherwise.
///
/// - If `allow_cwd_anywhere` is false, the path must be under an explicit
///   `allowed_roots` entry.
/// - If `allow_cwd_anywhere` is true, the path is allowed unless it is one of
///   the `DANGEROUS_PROJECT_ROOTS` (and not under an explicit `allowed_roots`).
fn validate_project_path_policy(policy: &AgentPolicy, canonical_path: &Path) -> Result<(), String> {
    let path_str = canonical_path.to_string_lossy().to_string();
    // If under an explicit allowed_root, always allow.
    for root in &policy.allowed_roots {
        if let Ok(canonical_root) = canonicalize_existing(root) {
            if canonical_path == &canonical_root || canonical_path.starts_with(&canonical_root) {
                return Ok(());
            }
        }
    }
    if !policy.allow_cwd_anywhere {
        return Err(format!(
            "path {} is outside allowed_roots and allow_cwd_anywhere is false",
            path_str
        ));
    }
    // allow_cwd_anywhere is true: reject dangerous system roots.
    for &dangerous in DANGEROUS_PROJECT_ROOTS {
        let dangerous_root = Path::new(dangerous);
        let is_dangerous = if dangerous_root == Path::new("/") {
            canonical_path == dangerous_root
        } else {
            canonical_path == dangerous_root || canonical_path.starts_with(dangerous_root)
        };
        if is_dangerous {
            return Err(format!(
                "path {} is under a dangerous system root; register it under an explicit allowed_roots entry if intended",
                path_str
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ProjectTomlWriteResult {
    config_path: PathBuf,
    created_config: bool,
    overwritten: bool,
}

/// Write a project TOML file atomically into `projects_dir`. Creates
/// `projects_dir` if missing. Returns write metadata on success.
/// The temp file is written and fsynced, then renamed to `<id>.toml`.
fn write_project_toml_atomic(
    projects_dir: &Path,
    id: &str,
    toml_content: &str,
    overwrite: bool,
) -> Result<ProjectTomlWriteResult, String> {
    // Ensure projects_dir exists.
    std::fs::create_dir_all(projects_dir).map_err(|e| {
        format!(
            "failed to create projects_dir {}: {}",
            projects_dir.display(),
            e
        )
    })?;
    let canonical_dir = canonicalize_existing(projects_dir)?;
    let config_path = canonical_dir.join(format!("{}.toml", id));
    // Guard against path escape: the final config path must be inside the
    // canonical projects_dir. The id validation already rejects slashes and
    // dot-dot, but this is a defense-in-depth check.
    if !config_path.starts_with(&canonical_dir) {
        return Err("project config path would escape projects_dir".to_string());
    }
    let existed_before = config_path.exists();
    if existed_before && !overwrite {
        return Err(format!(
            "project config already exists at {}; set overwrite=true to replace",
            config_path.display()
        ));
    }
    let temp_path = canonical_dir.join(format!(".{}.toml.tmp", id));
    {
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| format!("failed to create temp file {}: {}", temp_path.display(), e))?;
        file.write_all(toml_content.as_bytes())
            .map_err(|e| format!("failed to write temp file {}: {}", temp_path.display(), e))?;
        let _ = file.sync_all();
    }
    std::fs::rename(&temp_path, &config_path).map_err(|e| {
        format!(
            "failed to rename temp file to {}: {}",
            config_path.display(),
            e
        )
    })?;
    Ok(ProjectTomlWriteResult {
        config_path,
        created_config: !existed_before,
        overwritten: existed_before && overwrite,
    })
}

/// Handle `register_project` / `create_project` agent requests. Parses the
/// JSON payload from `request.stdin`, validates fields and path against
/// policy, writes `projects_dir/<id>.toml` atomically (and for
/// `create_project` creates the directory / templates / optional git init),
/// and returns structured JSON in `CommandResult.stdout`.
fn handle_project_op(
    policy: &AgentPolicy,
    projects_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let start = Instant::now();
    let kind = request.kind.as_str();
    let payload = match request.stdin.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("{} request missing stdin payload", kind)),
            };
        }
    };
    let json: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to parse {} payload: {}", kind, e)),
            };
        }
    };
    let get_str = |key: &str| -> Result<String, String> {
        json.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{} missing required field '{}'", kind, key))
    };
    let id = match get_str("id") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let name = match get_str("name") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let path = match get_str("path") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let description = json
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let allow_patch = json
        .get("allow_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let overwrite = json
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if let Err(e) = validate_project_op_id(&id) {
        return err_cmd(start, e);
    }
    if let Err(e) = validate_project_op_name(&name) {
        return err_cmd(start, e);
    }
    if let Some(ref desc) = description {
        if let Err(e) = validate_project_op_description(desc) {
            return err_cmd(start, e);
        }
    }
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') {
        return err_cmd(start, "path must be a non-empty absolute path".to_string());
    }

    let client_id = request.client_id.clone();
    let runtime_id = format!("agent:{}:{}", client_id, id);

    let toml_content = build_project_toml(&id, &name, &path, &description, allow_patch);

    if kind == "register_project" {
        // The directory must exist and be a directory.
        let path_buf = PathBuf::from(&path);
        let canonical = match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "path does not exist or cannot be canonicalized: {}: {}",
                        path, e
                    ),
                );
            }
        };
        if !canonical.is_dir() {
            return err_cmd(start, format!("path {} is not a directory", path));
        }
        if let Err(e) = validate_project_path_policy(policy, &canonical) {
            return err_cmd(start, e);
        }
        let write_result =
            match write_project_toml_atomic(projects_dir, &id, &toml_content, overwrite) {
                Ok(p) => p,
                Err(e) => return err_cmd(start, e),
            };
        let result = serde_json::json!({
            "id": runtime_id,
            "agent_project_id": id,
            "client_id": client_id,
            "name": name,
            "path": path,
            "description": description,
            "projects_config_path": write_result.config_path.to_string_lossy(),
            "created_config": write_result.created_config,
            "overwritten": write_result.overwritten,
            "allow_patch": allow_patch,
        });
        return ok_cmd(start, result);
    }

    // create_project
    let template = json
        .get("template")
        .and_then(|v| v.as_str())
        .unwrap_or("empty")
        .to_string();
    let git_init = json
        .get("git_init")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let allow_existing_empty = json
        .get("allow_existing_empty")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if template != "empty" && template != "basic" {
        return err_cmd(
            start,
            format!("unknown template '{}'; supported: empty, basic", template),
        );
    }

    let path_buf = PathBuf::from(&path);
    let mut created_directory = false;
    let mut created_paths = CreatedProjectPaths::default();

    // Determine the canonical parent for policy validation. If the path exists,
    // canonicalize it directly. If not, canonicalize the existing ancestor.
    let canonical_for_policy = if path_buf.exists() {
        match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!("path cannot be canonicalized: {}: {}", path, e),
                );
            }
        }
    } else {
        // Find the nearest existing ancestor and canonicalize it.
        let mut ancestor = path_buf.clone();
        while !ancestor.exists() {
            if let Some(parent) = ancestor.parent() {
                ancestor = parent.to_path_buf();
            } else {
                break;
            }
        }
        match ancestor.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "parent path cannot be canonicalized: {}: {}",
                        ancestor.display(),
                        e
                    ),
                );
            }
        }
    };
    if let Err(e) = validate_project_path_policy(policy, &canonical_for_policy) {
        return err_cmd(start, e);
    }

    // Handle existing vs new directory.
    if path_buf.exists() {
        let meta = match std::fs::metadata(&path_buf) {
            Ok(m) => m,
            Err(e) => return err_cmd(start, format!("failed to stat path {}: {}", path, e)),
        };
        if !meta.is_dir() {
            return err_cmd(
                start,
                format!("path {} exists but is not a directory", path),
            );
        }
        // Check if the directory is empty.
        let is_empty = match std::fs::read_dir(&path_buf) {
            Ok(mut it) => it.next().is_none(),
            Err(e) => {
                return err_cmd(start, format!("failed to read directory {}: {}", path, e));
            }
        };
        if !is_empty {
            return err_cmd(
                start,
                format!("path {} already exists and is not empty", path),
            );
        }
        if !allow_existing_empty {
            return err_cmd(
                start,
                format!(
                    "path {} already exists; set allow_existing_empty=true to use it",
                    path
                ),
            );
        }
    } else {
        // Create the directory.
        if let Err(e) = std::fs::create_dir_all(&path_buf) {
            return err_cmd(start, format!("failed to create directory {}: {}", path, e));
        }
        created_directory = true;
        created_paths.mark_project_dir_created(path_buf.clone());
    }

    // Apply template.
    if template == "basic" {
        let readme = if let Some(ref desc) = description {
            format!("# {}\n\n{}\n", name, desc)
        } else {
            format!("# {}\n", name)
        };
        let readme_path = path_buf.join("README.md");
        if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths) {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write README.md: {}", e));
        }
        let gitignore = "target/\nnode_modules/\n.env\n*.log\n";
        let gitignore_path = path_buf.join(".gitignore");
        if let Err(e) =
            write_created_file(&gitignore_path, gitignore.as_bytes(), &mut created_paths)
        {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write .gitignore: {}", e));
        }
    } else if template == "empty" {
        // For empty template, optionally create README.md if description is provided.
        if let Some(ref desc) = description {
            let readme = format!("# {}\n\n{}\n", name, desc);
            let readme_path = path_buf.join("README.md");
            if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths)
            {
                created_paths.cleanup();
                return err_cmd(start, format!("failed to write README.md: {}", e));
            }
        }
    }

    // git init.
    let mut git_initialized = false;
    if git_init {
        match std::process::Command::new("git")
            .arg("init")
            .current_dir(&path_buf)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) if output.status.success() => {
                git_initialized = true;
                created_paths.track(path_buf.join(".git"));
            }
            Ok(output) => {
                created_paths.cleanup();
                let stderr = String::from_utf8_lossy(&output.stderr);
                return err_cmd(start, format!("git init failed: {}", stderr.trim()));
            }
            Err(e) => {
                created_paths.cleanup();
                return err_cmd(start, format!("git init failed (is git installed?): {}", e));
            }
        }
    }

    // Write project TOML.
    let write_result = match write_project_toml_atomic(projects_dir, &id, &toml_content, overwrite)
    {
        Ok(p) => p,
        Err(e) => {
            created_paths.cleanup();
            return err_cmd(start, e);
        }
    };
    let result = serde_json::json!({
        "id": runtime_id,
        "agent_project_id": id,
        "client_id": client_id,
        "name": name,
        "path": path,
        "description": description,
        "projects_config_path": write_result.config_path.to_string_lossy(),
        "created_directory": created_directory,
        "created_config": write_result.created_config,
        "overwritten": write_result.overwritten,
        "allow_patch": allow_patch,
        "template": template,
        "git_initialized": git_initialized,
    });
    ok_cmd(start, result)
}

/// Build a success `CommandResult` with JSON output in stdout.
fn ok_cmd(start: Instant, result: serde_json::Value) -> CommandResult {
    CommandResult {
        exit_code: Some(0),
        stdout: Some(serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

/// Build an error `CommandResult`.
fn err_cmd(start: Instant, msg: String) -> CommandResult {
    CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: Some(msg),
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
        "file_read" | "file_write" | "file_list" => {
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

fn run_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    // Generate the per-process agent instance identity once. It is stable for
    // the whole process lifetime, including across WebSocket reconnects, so the
    // server can treat this process as a single active lease for `client_id`.
    // It is not a secret and is never persisted to disk.
    let agent_instance_id = uuid::Uuid::new_v4().to_string();
    let transport = cfg
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_POLLING)
        .to_string();
    match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_agent(cfg, once, &agent_instance_id),
        TRANSPORT_QUIC => run_quic_agent(cfg, once, &agent_instance_id),
        _ => run_polling_agent(cfg, once, &agent_instance_id),
    }
}

fn run_polling_agent(cfg: AgentConfig, once: bool, agent_instance_id: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let mut project_cache = AgentProjectCache::default();
    register(
        &client,
        &cfg,
        &mut project_cache,
        agent_instance_id,
        jobs.prepared_profiles.len(),
    )?;
    eprintln!(
        "webcodex-agent registered client_id={} server={} transport=polling",
        cfg.client_id, cfg.server_url
    );
    loop {
        match handle_one_poll(&client, &cfg, &jobs, &mut project_cache, agent_instance_id) {
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
                let _ = register(
                    &client,
                    &cfg,
                    &mut project_cache,
                    agent_instance_id,
                    jobs.prepared_profiles.len(),
                );
            }
        }
    }
}

// ============================================================================
// Experimental custom QUIC agent transport (Phase 5A)
// ============================================================================
//
// A custom QUIC *stream* transport (NOT HTTP/3). The agent opens a single QUIC
// bidirectional stream to the server, sends a `Register` envelope carrying the
// agent token in `auth_token` (there is no HTTP middleware to set an
// `Authorization` header), reads a `Registered` ack, then keeps the connection
// alive with `Ping`/`Pong`. Phase 5A does NOT implement request dispatch /
// `job_update` over QUIC — the agent registers and shows online, but does not
// receive `Request` envelopes. WebSocket/polling behavior is unchanged.

/// Reconnect backoff after a QUIC session ends.
const QUIC_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Interval between agent-initiated keepalive Pings.
const QUIC_PING_INTERVAL: Duration = Duration::from_secs(30);

/// Entry point for the QUIC transport. Runs a tokio current-thread runtime and
/// reconnects on session failure, mirroring `run_websocket_agent`.
fn run_quic_agent(cfg: AgentConfig, once: bool, agent_instance_id: &str) -> Result<(), String> {
    let agent_instance_id = agent_instance_id.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        loop {
            let projects = project_cache.get(&cfg);
            match quic_session(&cfg, projects, &agent_instance_id, once).await {
                Ok(()) => {
                    project_cache.invalidate();
                    if once {
                        return Ok(());
                    }
                    eprintln!("webcodex-agent quic session ended; reconnecting");
                    tokio::time::sleep(QUIC_RECONNECT_BACKOFF).await;
                }
                Err(e) => {
                    project_cache.invalidate();
                    eprintln!("webcodex-agent quic error: {}", e);
                    if once {
                        return Err(e);
                    }
                    tokio::time::sleep(QUIC_RECONNECT_BACKOFF).await;
                }
            }
        }
    })
}

/// Validate the `[quic]` config section. Returns a cloned, resolved config so
/// the session owns a concrete value (defaults applied).
fn resolve_quic_config(cfg: &AgentConfig) -> Result<QuicClientConfig, String> {
    let quic = cfg
        .quic
        .clone()
        .ok_or_else(|| "transport=quic requires a [quic] section in agent.toml".to_string())?;
    if quic.server_addr.trim().is_empty() {
        return Err("[quic] server_addr is required for transport=quic".to_string());
    }
    if quic.server_name.trim().is_empty() {
        return Err("[quic] server_name is required for transport=quic".to_string());
    }
    if quic.alpn.trim().is_empty() {
        return Err("[quic] alpn cannot be empty".to_string());
    }
    if quic.connect_timeout_secs == 0 {
        return Err("[quic] connect_timeout_secs must be > 0".to_string());
    }
    if quic.keepalive_interval_secs == 0 {
        return Err("[quic] keepalive_interval_secs must be > 0".to_string());
    }
    Ok(quic)
}

/// The rustls crypto provider for the QUIC client. The dependency tree pulls
/// both `aws-lc-rs` and `ring`, so rustls cannot auto-select; pin aws-lc-rs
/// explicitly per config via `builder_with_provider` (thread-safe, no global
/// install).
fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Build the quinn-wrapped rustls client config for the QUIC transport. The
/// agent validates the server certificate against the Mozilla root store
/// (webpki-roots) using `server_name` as the SNI/verification name — TLS is
/// transport security, not authentication; the agent token still authenticates
/// the agent.
fn build_quic_client_crypto(
    quic: &QuicClientConfig,
) -> Result<quinn::crypto::rustls::QuicClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    // `RootCertStore` implements `Extend<TrustAnchor>` (in-place, infallible).
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut client_crypto = rustls::ClientConfig::builder_with_provider(rustls_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("failed to select rustls protocol versions: {}", e))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![quic.alpn.as_bytes().to_vec()];
    quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
        .map_err(|e| format!("failed to build quinn client crypto: {}", e))
}

/// One QUIC connection lifecycle: connect, register, keepalive until the stream
/// closes or a fatal server error arrives. Phase 5A: register + ack + ping/pong
/// only; no `Request` dispatch. In `--once` mode, completes one ping/pong after
/// the ack then returns.
async fn quic_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    once: bool,
) -> Result<(), String> {
    let quic = resolve_quic_config(cfg)?;
    let client_crypto = build_quic_client_crypto(&quic)?;
    let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let client_endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())
        .map_err(|e| format!("failed to bind quic client endpoint: {}", e))?;
    let server_addr: std::net::SocketAddr = quic
        .server_addr
        .parse()
        .map_err(|e| format!("invalid [quic] server_addr '{}': {}", quic.server_addr, e))?;
    let connect = client_endpoint
        .connect_with(client_config, server_addr, &quic.server_name)
        .map_err(|e| format!("failed to start quic connect: {}", e))?;
    let conn = tokio::time::timeout(Duration::from_secs(quic.connect_timeout_secs), connect)
        .await
        .map_err(|_| format!("quic connect to {} timed out", quic.server_addr))?
        .map_err(|e| format!("quic connect to {} failed: {}", quic.server_addr, e))?;

    // ALPN is enforced by quinn during the TLS handshake: a connection only
    // completes when the client and server agree on a matching ALPN. A
    // mismatch fails the handshake (surfaced as the connect error above).

    // Open a single bidirectional stream for register/ack/keepalive.
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("failed to open quic bidirectional stream: {}", e))?;

    // Register. The token is carried in `auth_token`; the server authenticates
    // it exactly like the websocket/polling paths. It is never logged.
    let register_payload = build_register_request(
        cfg,
        projects,
        AGENT_PROTOCOL_VERSION_QUIC_V1,
        agent_instance_id,
        0,
    );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: Some(cfg.token.clone()),
    };
    write_quic_frame(&mut send, &reg_env)
        .await
        .map_err(|e| format!("failed to send quic register: {}", e))?;

    // Wait for the Registered ack.
    let ack = tokio::time::timeout(Duration::from_secs(15), read_quic_frame(&mut recv))
        .await
        .map_err(|_| "quic register ack timed out".to_string())?
        .map_err(|e| format!("failed to read quic register ack: {}", e))?;
    match ack {
        AgentEnvelope::Registered { success: true, .. } => {}
        AgentEnvelope::Registered { error, .. } => {
            return Err(error.unwrap_or_else(|| "register rejected".to_string()));
        }
        AgentEnvelope::Error { message, .. } => return Err(message),
        other => return Err(format!("expected registered ack, got {}", other.kind())),
    }
    eprintln!(
        "webcodex-agent registered client_id={} server={} transport=quic",
        cfg.client_id, quic.server_addr
    );

    if once {
        // Complete one ping/pong round trip then exit, mirroring the websocket
        // `--once` semantics.
        let ping = AgentEnvelope::Ping {
            ts: chrono::Utc::now().timestamp(),
        };
        write_quic_frame(&mut send, &ping)
            .await
            .map_err(|e| format!("quic once ping send failed: {}", e))?;
        let resp = tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv))
            .await
            .map_err(|_| "quic once pong timed out".to_string())?
            .map_err(|e| format!("quic once pong read failed: {}", e))?;
        match resp {
            AgentEnvelope::Pong { .. } => {}
            other => return Err(format!("expected pong, got {}", other.kind())),
        }
        let _ = send.finish();
        return Ok(());
    }

    // Keepalive loop: wait up to QUIC_PING_INTERVAL for a frame; if the server
    // is silent, emit a Ping. A Pong (reply) or Ping (server-initiated) keeps
    // the connection live. A server Error or a clean stream EOF ends the
    // session. The server does not push Request envelopes in 5A.
    loop {
        match tokio::time::timeout(QUIC_PING_INTERVAL, read_quic_frame(&mut recv)).await {
            Ok(Ok(AgentEnvelope::Pong { .. })) => {
                // Keepalive reply; connection is live.
            }
            Ok(Ok(AgentEnvelope::Ping { ts })) => {
                let pong = AgentEnvelope::Pong { ts };
                write_quic_frame(&mut send, &pong)
                    .await
                    .map_err(|e| format!("quic pong send failed: {}", e))?;
            }
            Ok(Ok(AgentEnvelope::Registered { .. })) => {
                // Ignore a redundant ack.
            }
            Ok(Ok(AgentEnvelope::Error { message, .. })) => {
                return Err(format!("server error: {}", message));
            }
            Ok(Ok(other)) => {
                eprintln!(
                    "webcodex-agent quic received unexpected envelope {}; ignoring",
                    other.kind()
                );
            }
            Ok(Err(QuicFrameError::EmptyStream)) => {
                // Server closed the stream cleanly.
                return Ok(());
            }
            Ok(Err(e)) => {
                return Err(format!("quic stream read error: {}", e));
            }
            Err(_) => {
                // Timeout with no frame: emit a keepalive Ping.
                let ping = AgentEnvelope::Ping {
                    ts: chrono::Utc::now().timestamp(),
                };
                write_quic_frame(&mut send, &ping)
                    .await
                    .map_err(|e| format!("quic ping send failed: {}", e))?;
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
fn run_websocket_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
) -> Result<(), String> {
    let agent_instance_id = agent_instance_id.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        loop {
            let projects = project_cache.get(&cfg);
            match websocket_session(&cfg, projects, &agent_instance_id).await {
                Ok(()) => {
                    project_cache.invalidate();
                    if once {
                        return Ok(());
                    }
                    eprintln!("webcodex-agent websocket session ended; reconnecting");
                    tokio::time::sleep(WS_RECONNECT_BACKOFF).await;
                }
                Err(e) => {
                    project_cache.invalidate();
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
    agent_instance_id: &str,
) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let ws_url = server_url_to_ws(&cfg.server_url, "/api/agents/ws")?;
    let request = build_ws_request(&ws_url, &cfg.token)?;
    let (mut ws_stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("websocket connect failed: {}", e))?;

    // Register over the socket. The prepared-profile cache is empty at
    // registration time (snapshots are prepared lazily on first use), so
    // `prepared_cache_count` is reported as 0 here.
    let register_payload = build_register_request(
        cfg,
        projects,
        AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
        agent_instance_id,
        0,
    );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: None,
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
        agent_instance_id: agent_instance_id.to_string(),
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
                        let shell = cfg.shell.clone();
                        let jobs = jobs.clone();
                        let projects_dir = projects_dir(&cfg);
                        // Execution is blocking (shell/file/jobs); run it off
                        // the async runtime thread. dispatch_request sends
                        // results/updates via the shared AgentSink.
                        tokio::task::spawn_blocking(move || {
                            let _ = dispatch_request(
                                &sink_handle,
                                &policy,
                                &shell,
                                &jobs,
                                &projects_dir,
                                request,
                            );
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
    fn agent_config_defaults_transport_to_polling_without_quic_section() {
        // No transport field and no [quic] section: defaults stay unchanged.
        let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
"#;
        let cfg: AgentConfig = toml::from_str(toml).unwrap();
        assert!(cfg.transport.is_none());
        assert!(cfg.quic.is_none());
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
                assert_eq!(
                    stdout,
                    format!("webcodex-agent {}\n", env!("CARGO_PKG_VERSION"))
                );
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
            expected_sha256: None,
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
    fn register_request_announces_polling_v1_protocol_version() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let body = build_register_request(
            &cfg,
            Vec::new(),
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            "inst-1",
            0,
        );
        assert_eq!(body.client_id, "oe");
        assert_eq!(body.agent_instance_id, "inst-1");
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
    fn build_register_request_announces_websocket_v1_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let body = build_register_request(
            &cfg,
            Vec::new(),
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            "inst-1",
            0,
        );
        assert_eq!(body.agent_instance_id, "inst-1");
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1)
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some("websocket-v1"));
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
                assert_eq!(payload.agent_instance_id, "ws-inst");
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
            agent_instance_id: "ws-inst".to_string(),
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
        let pdir = projects_dir(&cfg);
        let ran = dispatch_request(&sink, &cfg.policy, &cfg.shell, &jobs, &pdir, request).unwrap();
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
            expected_sha256: None,
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
