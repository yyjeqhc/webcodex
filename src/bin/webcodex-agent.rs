use reqwest::blocking::Client;
use std::collections::{HashMap, VecDeque};
use std::error::Error as StdError;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;

#[allow(dead_code)]
#[path = "../lsp_bridge.rs"]
mod lsp_bridge;

#[allow(dead_code)]
#[path = "../validation_bridge.rs"]
mod validation_bridge;

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

#[path = "../apply_edits_shared.rs"]
mod apply_edits_shared;

#[allow(dead_code)]
#[path = "../agent_init.rs"]
mod agent_init;

#[path = "../artifact_policy.rs"]
mod artifact_policy;

#[path = "../build_info.rs"]
mod build_info;

#[path = "../project_overview.rs"]
mod project_overview;

#[path = "../workspace_checkpoint.rs"]
mod workspace_checkpoint;

#[cfg(test)]
#[path = "webcodex_agent/job_manager_tests.rs"]
mod job_manager_tests;
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
    DEFAULT_INIT_PROJECTS_DIR, DEFAULT_POLL_INTERVAL_MS, TRANSPORT_WEBSOCKET,
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
    default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    default_websocket_connect_timeout_secs, effective_transport, handle_project_op,
    load_agent_project_summaries_from_dir, max_concurrent_jobs, non_empty_token,
    parse_agent_project_toml, quic_client_bind_addr_for, resolve_quic_config,
    resolve_quic_server_addrs, run_shell, run_shell_with_profiles, server_url_to_ws,
    sha256_hex_bytes, validate_project_path_policy, websocket_session, ShellProfileConfig,
    CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS, WS_OUTGOING_CAPACITY,
};
use webcodex_agent::{
    client_profile_agent_config, configured_prepared_shell_job_command,
    configured_shell_job_command, cwd_allowed, default_config_path, dispatch_request, err_cmd,
    handle_apply_text_edits_file_request, handle_artifact_file_request, handle_basic_file_request,
    handle_checkpoint_file_request, handle_line_edit_file_request, handle_replace_in_file_request,
    handle_write_project_file_request, hostname, is_artifact_request_kind,
    is_basic_file_request_kind, is_checkpoint_request_kind, is_line_edit_request_kind,
    is_project_op, load_config, ok_cmd, projects_dir, resolve_prepared_shell_profile,
    resolve_requested_path, run_agent, validate_client_profile, validate_line_edit_agent_path,
    AgentConfig, AgentPolicy, AgentProjectCache, AgentSink, CommandResult, HttpSendConfig,
    PreparedShellProfileCache, ShellConfig,
};

const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const AGENT_REGISTER_PATH: &str = "/api/shell/agent/register";
const AGENT_POLL_PATH: &str = "/api/shell/agent/poll";

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentHttpErrorKind {
    ServerUnavailable,
    Auth,
    NotFound,
    Status,
    RequestTimeout,
    Request,
    Decode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentHttpError {
    kind: AgentHttpErrorKind,
    path: String,
    summary: String,
}

impl AgentHttpError {
    fn status(path: &str, status: reqwest::StatusCode, body: &str) -> Self {
        let kind = match status.as_u16() {
            401 | 403 => AgentHttpErrorKind::Auth,
            404 => AgentHttpErrorKind::NotFound,
            502 | 503 | 504 => AgentHttpErrorKind::ServerUnavailable,
            _ if looks_like_proxy_html_error(body) => AgentHttpErrorKind::ServerUnavailable,
            _ => AgentHttpErrorKind::Status,
        };
        Self {
            kind,
            path: path.to_string(),
            summary: http_status_summary(status),
        }
    }

    fn request(path: &str, error: reqwest::Error) -> Self {
        let chain = error_chain_text(&error);
        let kind = if looks_like_server_down_request(&error, &chain) {
            AgentHttpErrorKind::ServerUnavailable
        } else if error.is_timeout() {
            AgentHttpErrorKind::RequestTimeout
        } else {
            AgentHttpErrorKind::Request
        };
        Self {
            kind,
            path: path.to_string(),
            summary: request_error_summary(error, &chain),
        }
    }

    fn decode(path: &str, error: serde_json::Error) -> Self {
        Self {
            kind: AgentHttpErrorKind::Decode,
            path: path.to_string(),
            summary: format!(
                "invalid JSON response: {}",
                bounded_single_line(&error.to_string())
            ),
        }
    }
}

impl std::fmt::Display for AgentHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            AgentHttpErrorKind::ServerUnavailable => {
                write!(f, "server unavailable for {}: {}", self.path, self.summary)
            }
            AgentHttpErrorKind::Auth => write!(
                f,
                "authentication failed for {}: {}; check agent token/config",
                self.path, self.summary
            ),
            AgentHttpErrorKind::NotFound => write!(
                f,
                "endpoint missing or incompatible server for {}: {}",
                self.path, self.summary
            ),
            AgentHttpErrorKind::Status
            | AgentHttpErrorKind::RequestTimeout
            | AgentHttpErrorKind::Request
            | AgentHttpErrorKind::Decode => {
                write!(f, "{} request failed: {}", self.path, self.summary)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PollErrorKind {
    ServerUnavailable,
    Auth,
    EndpointMissing,
    Retryable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PollError {
    kind: PollErrorKind,
    terminal: bool,
    message: String,
}

impl PollError {
    fn terminal(kind: PollErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            terminal: true,
            message: message.into(),
        }
    }

    fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: PollErrorKind::Retryable,
            terminal: false,
            message: message.into(),
        }
    }

    fn from_http(error: AgentHttpError) -> Self {
        match error.kind {
            AgentHttpErrorKind::ServerUnavailable => Self::terminal(
                PollErrorKind::ServerUnavailable,
                format!(
                    "server unavailable while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::Auth => Self::terminal(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::NotFound => Self::terminal(
                PollErrorKind::EndpointMissing,
                format!(
                    "poll endpoint missing or incompatible server while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::RequestTimeout => Self::retryable(format!(
                "poll request timed out while polling {}: {}",
                error.path, error.summary
            )),
            AgentHttpErrorKind::Status
            | AgentHttpErrorKind::Request
            | AgentHttpErrorKind::Decode => Self::retryable(format!(
                "poll request failed while polling {}: {}",
                error.path, error.summary
            )),
        }
    }

    fn from_response_error(error: Option<String>) -> Self {
        let message = error.unwrap_or_else(|| "poll failed without error".to_string());
        let summary = bounded_single_line(&message);
        if looks_like_auth_failure_message(&summary) {
            Self::terminal(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    AGENT_POLL_PATH, summary
                ),
            )
        } else {
            Self::retryable(summary)
        }
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn http_status_summary(status: reqwest::StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {}", status.as_u16(), reason),
        None => format!("HTTP {}", status.as_u16()),
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_proxy_html_error(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("<html")
        && contains_any(
            &lower,
            &[
                "bad gateway",
                "service unavailable",
                "gateway timeout",
                "nginx",
                "upstream",
            ],
        )
}

fn looks_like_server_down_request(error: &reqwest::Error, chain: &str) -> bool {
    if error.is_connect() {
        return true;
    }
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "connection refused",
            "connection reset",
            "connection aborted",
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
            "broken pipe",
        ],
    )
}

fn looks_like_auth_failure_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "unauthorized",
            "forbidden",
            "invalid token",
            "bad token",
            "auth failed",
            "authentication",
        ],
    )
}

fn error_chain_text(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = StdError::source(error);
    while let Some(err) = source {
        parts.push(err.to_string());
        source = err.source();
    }
    parts.join(": ")
}

fn request_error_summary(error: reqwest::Error, chain: &str) -> String {
    let lower = chain.to_ascii_lowercase();
    if lower.contains("connection refused") {
        "connection refused".to_string()
    } else if lower.contains("connection reset") {
        "connection reset".to_string()
    } else if lower.contains("connection aborted") {
        "connection aborted".to_string()
    } else if lower.contains("broken pipe") {
        "broken pipe".to_string()
    } else if contains_any(
        &lower,
        &[
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
        ],
    ) {
        "connection closed before response completed".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else if error.is_timeout() {
        "request timed out".to_string()
    } else {
        bounded_single_line(&error.without_url().to_string())
    }
}

fn bounded_single_line(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.chars() {
        let ch = if ch.is_whitespace() || ch.is_control() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
        if out.chars().count() >= MAX_CHARS {
            out.push_str("...");
            break;
        }
    }
    out.trim().to_string()
}

fn post_json<T, R>(
    client: &Client,
    cfg: &AgentConfig,
    path: &str,
    body: &T,
) -> Result<R, AgentHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    post_json_with_auth(client, &cfg.server_url, &cfg.token, path, body)
}

fn post_json_with_auth<T, R>(
    client: &Client,
    server_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<R, AgentHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let mut req = client.post(url);
    if !token.trim().is_empty() {
        req = req.bearer_auth(token.trim());
    }
    let resp = req
        .json(body)
        .send()
        .map_err(|e| AgentHttpError::request(path, e))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| AgentHttpError::request(path, e))?;
    if !status.is_success() {
        return Err(AgentHttpError::status(path, status, &text));
    }
    serde_json::from_str(&text).map_err(|e| AgentHttpError::decode(path, e))
}

fn agent_register_capabilities(cfg: &AgentConfig) -> ShellClientCapabilities {
    let mut capabilities = cfg.capabilities.clone().unwrap_or_default();
    capabilities.jobs = true;
    capabilities.file_read = true;
    capabilities.file_write = true;
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    // New agents always advertise read-only LSP navigation. Older agents omit
    // the field and deserialize as false on the server.
    capabilities.lsp_read_only_navigation = true;
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
) -> Result<usize, String> {
    let projects = project_cache.get(cfg);
    let projects_count = projects.iter().filter(|project| !project.disabled).count();
    let body = build_register_request(
        cfg,
        projects,
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        agent_instance_id,
        prepared_cache_count,
    );
    let response: ShellClientRegisterResponse =
        post_json(client, cfg, AGENT_REGISTER_PATH, &body).map_err(|e| e.to_string())?;
    if response.success {
        Ok(projects_count)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "register failed without error".to_string()))
    }
}

fn is_file_request_kind(kind: &str) -> bool {
    is_basic_file_request_kind(kind)
        || is_line_edit_request_kind(kind)
        || is_artifact_request_kind(kind)
        || is_checkpoint_request_kind(kind)
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
        "file_replace_in_file" => handle_replace_in_file_request(request, &resolved, start),
        "file_write_project_file" => handle_write_project_file_request(request, &resolved, start),
        "file_apply_text_edits" => handle_apply_text_edits_file_request(policy, request, start),
        "file_save_project_artifact"
        | "file_read_project_artifact_metadata"
        | "file_read_project_artifact"
        | "file_artifact_upload_begin"
        | "file_artifact_upload_chunk"
        | "file_artifact_upload_finish"
        | "file_artifact_upload_abort" => handle_artifact_file_request(request, &resolved, start),
        "file_checkpoint_create" | "file_checkpoint_restore" => {
            handle_checkpoint_file_request(request, &resolved, start)
        }
        "file_read" | "file_write" | "file_list" | "file_project_overview" => {
            handle_basic_file_request(policy, request, &resolved, start)
        }
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

#[cfg(unix)]
fn classify_process_group_signal_error(
    pgid: u32,
    signal: i32,
    error: std::io::Error,
) -> Result<bool, String> {
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Err(format!(
            "permission denied signaling process group {pgid} with signal {signal}"
        )),
        _ => Err(format!(
            "failed to signal process group {pgid} with signal {signal}: {error}"
        )),
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: u32, signal: i32) -> Result<bool, String> {
    let target = i32::try_from(pgid).map_err(|_| format!("process-group id {pgid} exceeds i32"))?;
    // SAFETY: callers only pass the private process-group id of a child that
    // this JobManager launched through `setsid`.
    if unsafe { libc::kill(-target, signal) } == 0 {
        Ok(true)
    } else {
        classify_process_group_signal_error(pgid, signal, std::io::Error::last_os_error())
    }
}

fn kill_child_group(child: &Arc<Mutex<Child>>) -> Result<(), String> {
    let pid = child
        .lock()
        .map_err(|_| "job child lock poisoned".to_string())?
        .id();
    #[cfg(unix)]
    {
        if pid == 0 {
            return Err("job child has invalid process-group id 0".to_string());
        }
        if !signal_process_group(pid, libc::SIGTERM)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
        // Escalate the whole group, not only the leader; a descendant may
        // ignore SIGTERM or outlive the wrapper shell.
        if signal_process_group(pid, 0)? {
            let _ = signal_process_group(pid, libc::SIGKILL)?;
        }
    }
    #[cfg(not(unix))]
    child
        .lock()
        .map_err(|_| "job child lock poisoned".to_string())?
        .kill()
        .map_err(|error| error.to_string())?;
    Ok(())
}

impl JobManager {
    fn has_work(&self) -> bool {
        !self.jobs.lock().unwrap().is_empty() || !self.queued.lock().unwrap().is_empty()
    }

    fn stop_all(&self) {
        self.queued.lock().unwrap().clear();
        let running = {
            let jobs = self.jobs.lock().unwrap();
            jobs.iter()
                .map(|(job_id, job)| {
                    (
                        job_id.clone(),
                        job.child.clone(),
                        job.stop_requested.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        for (job_id, child, stop_requested) in running {
            stop_requested.store(true, Ordering::SeqCst);
            if let Some(child) = child {
                if let Err(e) = kill_child_group(&child) {
                    eprintln!("webcodex-agent stop_job error: failed to kill job {job_id}: {e}");
                }
            }
        }
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
fn handle_one_poll(
    client: &Client,
    cfg: &AgentConfig,
    jobs: &JobManager,
    project_cache: &mut AgentProjectCache,
    agent_instance_id: &str,
    lsp: &webcodex_agent::LspSupervisor,
) -> Result<bool, PollError> {
    let poll = ShellAgentPollRequest {
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
        projects: Some(project_cache.get(cfg)),
    };
    let response: ShellAgentPollResponse =
        post_json(client, cfg, AGENT_POLL_PATH, &poll).map_err(PollError::from_http)?;
    if !response.success {
        return Err(PollError::from_response_error(response.error));
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
        lsp,
        request,
    );
    if project_op && result.is_ok() {
        project_cache.invalidate();
    }
    result.map_err(PollError::retryable)
}

fn main() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

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
            websocket_connect_timeout_secs: default_websocket_connect_timeout_secs(),
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
            cfg.websocket_connect_timeout_secs,
            default_websocket_connect_timeout_secs()
        );
        assert_eq!(
            auto_transport_plan(&cfg),
            vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
        );
    }

    #[test]
    fn agent_config_rejects_zero_websocket_connect_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
websocket_connect_timeout_secs = 0
"#,
        )
        .unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(
            err.contains("websocket_connect_timeout_secs must be > 0"),
            "{err}"
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
            validation: None,
            lsp: None,
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
            validation: None,
            lsp: None,
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
            validation: None,
            lsp: None,
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
            validation: None,
            lsp: None,
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
        let expected_sha256 = sha256_hex_bytes(content.as_bytes());
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
        assert_eq!(out["sha256"], expected_sha256);
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
        mut payload: serde_json::Value,
    ) -> ShellAgentShellRequest {
        if payload.get("changes").is_none() {
            let expected_sha256 = payload
                .get("expected_file_sha256")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    sha256_hex_bytes(&std::fs::read(cwd.join(path)).unwrap_or_default())
                });
            payload = serde_json::json!({
                "dry_run": payload.get("dry_run").cloned().unwrap_or(serde_json::Value::Bool(false)),
                "changes": [{
                    "kind": "edit",
                    "path": path,
                    "expected_sha256": expected_sha256,
                    "edits": payload.get("edits").cloned().unwrap_or_else(|| serde_json::json!([]))
                }]
            });
        }
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
            validation: None,
            lsp: None,
        }
    }

    fn json_file_op_request(
        cwd: &Path,
        kind: &str,
        path: &str,
        payload: serde_json::Value,
    ) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: format!("req-{kind}"),
            client_id: "agent-1".to_string(),
            kind: kind.to_string(),
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
            validation: None,
            lsp: None,
        }
    }

    fn fake_zip_eocd_with_entries(entries: u16) -> Vec<u8> {
        let mut bytes = b"PK\x05\x06".to_vec();
        bytes.extend_from_slice(&[0, 0]); // disk number
        bytes.extend_from_slice(&[0, 0]); // central directory disk
        bytes.extend_from_slice(&entries.to_le_bytes());
        bytes.extend_from_slice(&entries.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]); // central directory size
        bytes.extend_from_slice(&[0, 0, 0, 0]); // central directory offset
        bytes.extend_from_slice(&[0, 0]); // comment length
        bytes
    }

    fn artifact_upload_temp_paths(
        root: &Path,
        artifact_path: &str,
        upload_id: &str,
    ) -> (PathBuf, PathBuf) {
        let target = root.join(artifact_path);
        let parent = target.parent().expect("artifact path parent");
        (
            parent.join(format!(".wc-upload-{upload_id}.part")),
            parent.join(format!(".wc-upload-{upload_id}.json")),
        )
    }

    fn directory_contains_name_prefix(dir: &Path, prefix: &str) -> bool {
        if !dir.exists() {
            return false;
        }
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .any(|name| name.starts_with(prefix))
    }

    fn assert_upload_temp_files_exist(root: &Path, artifact_path: &str, upload_id: &str) {
        let (part, sidecar) = artifact_upload_temp_paths(root, artifact_path, upload_id);
        assert!(
            part.exists(),
            "missing upload part file: {}",
            part.display()
        );
        assert!(
            sidecar.exists(),
            "missing upload sidecar file: {}",
            sidecar.display()
        );
        let parent = part.parent().unwrap();
        assert!(
            !directory_contains_name_prefix(parent, ".pd-upload-"),
            "legacy .pd upload temp files must not be created in {}",
            parent.display()
        );
    }

    fn assert_no_upload_temp_files(root: &Path, artifact_path: &str) {
        let target = root.join(artifact_path);
        let Some(parent) = target.parent() else {
            return;
        };
        assert!(
            !directory_contains_name_prefix(parent, ".wc-upload-"),
            "upload temp files remained in {}",
            parent.display()
        );
        assert!(
            !directory_contains_name_prefix(parent, ".pd-upload-"),
            "legacy .pd upload temp files remained in {}",
            parent.display()
        );
    }

    #[test]
    fn file_save_project_artifact_writes_binary_and_blocks_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/tiny.png";
        let content_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            [0x89, b'P', b'N', b'G'],
        );
        let payload = serde_json::json!({
            "path": path,
            "content_base64": content_base64,
            "mime_type": "image/png",
            "overwrite": false,
            "max_bytes": 1024,
        });

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_save_project_artifact",
                path,
                payload.clone(),
            ),
        ));

        assert_eq!(out["path"], path);
        assert_eq!(out["bytes_written"], 4);
        assert_eq!(out["mime_type"], "image/png");
        assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(
            std::fs::read(tmp.path().join(path)).unwrap(),
            vec![0x89, b'P', b'N', b'G']
        );
        let parent = tmp.path().join("artifacts/imports");
        assert!(
            !directory_contains_name_prefix(&parent, ".wc-artifact-"),
            "atomic artifact temp file should not remain"
        );
        assert!(
            !directory_contains_name_prefix(&parent, ".pd-artifact-"),
            "legacy .pd artifact temp file should not remain"
        );

        let out2 = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(tmp.path(), "file_save_project_artifact", path, payload),
        ));
        assert!(out2["error"]
            .as_str()
            .unwrap()
            .contains("overwrite is false"));
    }

    #[test]
    fn file_read_project_artifact_metadata_counts_zip_without_extracting() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let zip_path = tmp.path().join("sample.zip");
        std::fs::write(&zip_path, fake_zip_eocd_with_entries(2)).unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact_metadata",
                "sample.zip",
                serde_json::json!({"path": "sample.zip", "max_bytes": 1024}),
            ),
        ));

        assert_eq!(out["mime_type"], "application/zip");
        assert_eq!(out["archive_entries_count"], 2);
        assert!(
            out["modified_at"].as_u64().unwrap() > 0,
            "modified_at should be unix timestamp seconds"
        );
        assert!(!tmp.path().join("a.txt").exists());
        assert!(!tmp.path().join("b.txt").exists());
    }

    #[test]
    fn file_read_project_artifact_reads_binary_chunks() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let bytes = [0, 159, 146, 150, b'a', b'b', b'c', b'd'];
        std::fs::write(tmp.path().join("data.bin"), bytes).unwrap();

        let first = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact",
                "data.bin",
                serde_json::json!({"path": "data.bin", "offset": 0, "length": 4, "max_file_bytes": 1024}),
            ),
        ));
        assert_eq!(first["file_bytes"], bytes.len());
        assert_eq!(first["offset"], 0);
        assert_eq!(first["bytes_returned"], 4);
        assert_eq!(first["next_offset"], 4);
        assert_eq!(first["truncated"], true);
        assert_eq!(first["eof"], false);
        assert_eq!(
            first["content_base64"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..4])
        );

        let second = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact",
                "data.bin",
                serde_json::json!({"path": "data.bin", "offset": 4, "length": 20, "max_file_bytes": 1024}),
            ),
        ));
        assert_eq!(second["sha256"], first["sha256"]);
        assert_eq!(second["offset"], 4);
        assert_eq!(second["bytes_returned"], bytes.len() - 4);
        assert_eq!(second["next_offset"], bytes.len());
        assert_eq!(second["truncated"], false);
        assert_eq!(second["eof"], true);
        assert_eq!(
            second["content_base64"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[4..])
        );

        let at_eof = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact",
                "data.bin",
                serde_json::json!({"path": "data.bin", "offset": bytes.len(), "length": 4, "max_file_bytes": 1024}),
            ),
        ));
        assert_eq!(at_eof["bytes_returned"], 0);
        assert_eq!(at_eof["next_offset"], bytes.len());
        assert_eq!(at_eof["truncated"], false);
        assert_eq!(at_eof["eof"], true);
    }

    #[test]
    fn file_artifact_upload_chunks_finish_and_abort() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/upload.bin";
        let bytes = b"abcdefgh";
        let expected_sha256 = sha256_hex_bytes(bytes);

        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": bytes.len(),
                    "expected_sha256": expected_sha256,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();
        assert!(upload_id.starts_with("wc_upload_"));
        assert_eq!(begin["received_bytes"], 0);
        assert!(!tmp.path().join(path).exists());
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

        let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..4]);
        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": first,
                    "max_chunk_bytes": 4,
                }),
            ),
        ));
        assert_eq!(out["received_bytes"], 4);
        assert_eq!(out["next_offset"], 4);
        assert!(!tmp.path().join(path).exists());
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

        let second =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[4..]);
        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 4,
                    "content_base64": second,
                    "max_chunk_bytes": 4,
                }),
            ),
        ));
        assert_eq!(out["received_bytes"], bytes.len());

        let finish = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                }),
            ),
        ));
        assert_eq!(finish["committed"], true);
        assert_eq!(finish["bytes"], bytes.len());
        assert_eq!(finish["sha256"], sha256_hex_bytes(bytes));
        assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), bytes);
        assert_no_upload_temp_files(tmp.path(), path);

        let abort_path = "artifacts/imports/abort.bin";
        let begin_abort = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                abort_path,
                serde_json::json!({
                    "path": abort_path,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let abort_upload_id = begin_abort["upload_id"].as_str().unwrap();
        assert_upload_temp_files_exist(tmp.path(), abort_path, abort_upload_id);
        let abort = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_abort",
                abort_path,
                serde_json::json!({
                    "path": abort_path,
                    "upload_id": abort_upload_id,
                }),
            ),
        ));
        assert_eq!(abort["aborted"], true);
        assert!(!tmp.path().join(abort_path).exists());
        assert_no_upload_temp_files(tmp.path(), abort_path);
    }

    #[test]
    fn file_artifact_upload_begin_rejects_validation_and_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());

        let sensitive = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                ".env",
                serde_json::json!({
                    "path": ".env",
                    "expected_bytes": 1,
                    "expected_sha256": null,
                    "mime_type": "text/plain",
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert!(sensitive["error"]
            .as_str()
            .unwrap()
            .contains("sensitive artifact path"));

        let bad_hash_path = "artifacts/imports/bad-hash.txt";
        let bad_hash = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                bad_hash_path,
                serde_json::json!({
                    "path": bad_hash_path,
                    "expected_bytes": 1,
                    "expected_sha256": "not-a-sha",
                    "mime_type": "text/plain",
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert!(bad_hash["error"]
            .as_str()
            .unwrap()
            .contains("expected_sha256 must be"));

        let too_large_path = "artifacts/imports/too-large.txt";
        let too_large = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                too_large_path,
                serde_json::json!({
                    "path": too_large_path,
                    "expected_bytes": 5,
                    "expected_sha256": null,
                    "mime_type": "text/plain",
                    "overwrite": false,
                    "max_bytes": 4,
                }),
            ),
        ));
        assert_eq!(too_large["error"], "expected_bytes exceeds max_bytes");

        let unsafe_octet_path = "artifacts/imports/raw.bin";
        let unsafe_octet = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                unsafe_octet_path,
                serde_json::json!({
                    "path": unsafe_octet_path,
                    "expected_bytes": 1,
                    "expected_sha256": null,
                    "mime_type": "application/octet-stream",
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let unsafe_octet_error = unsafe_octet["error"].as_str().unwrap();
        assert!(unsafe_octet_error.contains(".artifact"));
        assert!(unsafe_octet_error.contains(".txt"));
        assert!(unsafe_octet_error.contains("artifacts/smoke/<name>.artifact"));
        assert_eq!(unsafe_octet["failure_kind"], "policy_rejected");

        let existing_path = "artifacts/imports/existing.txt";
        std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
        std::fs::write(tmp.path().join(existing_path), b"old").unwrap();
        let existing = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                existing_path,
                serde_json::json!({
                    "path": existing_path,
                    "expected_bytes": 3,
                    "expected_sha256": null,
                    "mime_type": "text/plain",
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(existing["error"], "file exists and overwrite is false");
        assert_eq!(
            std::fs::read(tmp.path().join(existing_path)).unwrap(),
            b"old"
        );

        #[cfg(unix)]
        {
            let symlink_path = "artifacts/imports/link.txt";
            let victim = tmp.path().join("victim.txt");
            std::fs::write(&victim, b"victim").unwrap();
            std::os::unix::fs::symlink(&victim, tmp.path().join(symlink_path)).unwrap();
            let symlink = line_edit_json(handle_file_request(
                &policy,
                &json_file_op_request(
                    tmp.path(),
                    "file_artifact_upload_begin",
                    symlink_path,
                    serde_json::json!({
                        "path": symlink_path,
                        "expected_bytes": 3,
                        "expected_sha256": null,
                        "mime_type": "text/plain",
                        "overwrite": true,
                        "max_bytes": 1024,
                    }),
                ),
            ));
            assert_eq!(
                symlink["error"],
                "refusing to overwrite symlink artifact path"
            );
            assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
        }
    }

    #[test]
    fn file_artifact_upload_chunk_rejects_validation_and_keeps_final_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/chunk.bin";
        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024 * 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();

        let invalid_id = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": "bad",
                    "offset": 0,
                    "content_base64": "YQ==",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert!(invalid_id["error"]
            .as_str()
            .unwrap()
            .contains("upload_id must start"));

        let invalid_base64 = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": "not valid base64!",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert!(invalid_base64["error"]
            .as_str()
            .unwrap()
            .contains("invalid base64"));

        let empty = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": "",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert!(empty["error"]
            .as_str()
            .unwrap()
            .contains("decoded chunk must contain at least 1 byte"));

        let too_large = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            vec![b'x'; 64 * 1024 + 1],
        );
        let too_large = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": too_large,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(too_large["error"], "decoded chunk too large");

        let first = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc"),
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(first["received_bytes"], 3);
        assert!(!tmp.path().join(path).exists());

        let wrong_offset = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": "ZA==",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(
            wrong_offset["error"],
            "offset does not match received_bytes"
        );
        assert_eq!(wrong_offset["received_bytes"], 3);
        assert_eq!(wrong_offset["next_offset"], 3);

        let other_path = "artifacts/imports/other.bin";
        let mismatch = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                other_path,
                serde_json::json!({
                    "path": other_path,
                    "upload_id": upload_id.clone(),
                    "offset": 3,
                    "content_base64": "ZA==",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(
            mismatch["error"],
            "upload_id does not belong to requested path"
        );
        assert!(!tmp.path().join(path).exists());
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
    }

    #[test]
    fn file_artifact_upload_finish_validation_failures_keep_retry_state() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/retry.bin";

        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": 4,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();
        let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc");
        let chunk = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": first,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(chunk["received_bytes"], 3);

        let failed_finish = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                path,
                serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(
            failed_finish["error"],
            "uploaded byte count does not match expected_bytes"
        );
        assert_eq!(failed_finish["committed"], false);
        assert!(!tmp.path().join(path).exists());
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

        let retry_chunk = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 3,
                    "content_base64": "ZA==",
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(retry_chunk["received_bytes"], 4);
        let finish = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                path,
                serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(finish["committed"], true);
        assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"abcd");
        assert_no_upload_temp_files(tmp.path(), path);

        let sha_path = "artifacts/imports/bad-sha.bin";
        let bad_sha = "0000000000000000000000000000000000000000000000000000000000000000";
        let begin_sha = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                sha_path,
                serde_json::json!({
                    "path": sha_path,
                    "expected_bytes": null,
                    "expected_sha256": bad_sha,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let sha_upload_id = begin_sha["upload_id"].as_str().unwrap().to_string();
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abcd");
        let _ = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                sha_path,
                serde_json::json!({
                    "path": sha_path,
                    "upload_id": sha_upload_id.clone(),
                    "offset": 0,
                    "content_base64": data,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        let sha_failed = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                sha_path,
                serde_json::json!({"path": sha_path, "upload_id": sha_upload_id.clone()}),
            ),
        ));
        assert_eq!(
            sha_failed["error"],
            "uploaded sha256 does not match expected_sha256"
        );
        assert_eq!(sha_failed["committed"], false);
        assert!(!tmp.path().join(sha_path).exists());
        assert_upload_temp_files_exist(tmp.path(), sha_path, &sha_upload_id);
    }

    #[test]
    fn file_artifact_upload_finish_refuses_late_target_when_overwrite_false() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/race.bin";
        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();
        let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
        let _ = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": chunk,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        std::fs::write(tmp.path().join(path), b"old").unwrap();
        let finish = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                path,
                serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(finish["error"], "file exists and overwrite is false");
        assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"old");
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
    }

    #[cfg(unix)]
    #[test]
    fn file_artifact_upload_finish_refuses_late_symlink_even_with_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/symlink-race.bin";
        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": true,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();
        let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
        let _ = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": chunk,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));
        let victim = tmp.path().join("victim-race.bin");
        std::fs::write(&victim, b"victim").unwrap();
        std::os::unix::fs::symlink(&victim, tmp.path().join(path)).unwrap();
        let finish = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_finish",
                path,
                serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(
            finish["error"],
            "refusing to overwrite symlink artifact path"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
    }

    #[test]
    fn file_artifact_upload_abort_rejects_wrong_ids_and_cleans_only_temp() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let path = "artifacts/imports/abort-target.bin";
        std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
        std::fs::write(tmp.path().join(path), b"final").unwrap();
        let begin = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                serde_json::json!({
                    "path": path,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "mime_type": null,
                    "overwrite": true,
                    "max_bytes": 1024,
                }),
            ),
        ));
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();
        let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"temp");
        let _ = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_chunk",
                path,
                serde_json::json!({
                    "path": path,
                    "upload_id": upload_id.clone(),
                    "offset": 0,
                    "content_base64": chunk,
                    "max_chunk_bytes": 64 * 1024,
                }),
            ),
        ));

        let missing = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_abort",
                path,
                serde_json::json!({"path": path, "upload_id": "wc_upload_missing"}),
            ),
        ));
        assert!(missing["error"]
            .as_str()
            .unwrap()
            .contains("upload not found"));
        assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

        let other_path = "artifacts/imports/abort-other.bin";
        let mismatch = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_abort",
                other_path,
                serde_json::json!({"path": other_path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(
            mismatch["error"],
            "upload_id does not belong to requested path"
        );
        assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

        let abort = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_abort",
                path,
                serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
            ),
        ));
        assert_eq!(abort["aborted"], true);
        assert_eq!(abort["received_bytes"], 4);
        assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
        assert_no_upload_temp_files(tmp.path(), path);
    }

    #[cfg(unix)]
    #[test]
    fn file_project_artifact_ops_reject_symlink_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside = outside_dir.path().join("outside.bin");
        std::fs::write(&outside, b"outside-secret-content").unwrap();
        std::os::unix::fs::symlink(&outside, root.path().join("leak.bin")).unwrap();
        let mut policy = project_policy(root.path());
        policy.allowed_roots.push(outside_dir.path().to_path_buf());

        let read = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                root.path(),
                "file_read_project_artifact",
                "leak.bin",
                serde_json::json!({"path":"leak.bin","offset":0,"length":8,"max_file_bytes":1024}),
            ),
        ));
        assert_eq!(read["error"], "artifact path escapes project root");
        assert!(!read.to_string().contains("outside-secret-content"));

        let metadata = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                root.path(),
                "file_read_project_artifact_metadata",
                "leak.bin",
                serde_json::json!({"path":"leak.bin","max_bytes":1024}),
            ),
        ));
        assert_eq!(metadata["error"], "artifact path escapes project root");
        assert!(!metadata.to_string().contains("outside-secret-content"));

        let save = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                root.path(),
                "file_save_project_artifact",
                "leak.bin",
                serde_json::json!({
                    "path":"leak.bin",
                    "content_base64":"bmV3",
                    "mime_type":"text/plain",
                    "overwrite":true,
                    "max_bytes":1024
                }),
            ),
        ));
        assert_eq!(save["error"], "refusing to overwrite symlink artifact path");
        assert_eq!(
            std::fs::read(&outside).expect("outside file remains readable"),
            b"outside-secret-content"
        );
        assert!(!save.to_string().contains("outside-secret-content"));
    }

    #[test]
    fn file_replace_in_file_replaces_multiple_when_expected_count_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "a a a").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "target.txt",
                serde_json::json!({
                    "path": "target.txt",
                    "old": "a",
                    "new": "b",
                    "expected_replacements": 3,
                    "allow_multiple": true,
                }),
            ),
        ));

        assert_eq!(out["changed"], true);
        assert_eq!(out["replacements"], 3);
        assert_eq!(out["before_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(out["after_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(out["bytes_written"], "b b b".len());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "b b b");
    }

    #[test]
    fn file_replace_in_file_rejects_missing_and_ambiguous_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let missing_file = tmp.path().join("missing.txt");
        let dup_file = tmp.path().join("dup.txt");
        std::fs::write(&missing_file, "hello world").unwrap();
        std::fs::write(&dup_file, "a a a").unwrap();

        let missing = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "missing.txt",
                serde_json::json!({
                    "old": "absent",
                    "new": "x",
                    "expected_replacements": 1,
                    "allow_multiple": false,
                }),
            ),
        ));
        assert_eq!(missing["changed"], false);
        assert!(missing["error"].as_str().unwrap().contains("not found"));
        assert_eq!(
            std::fs::read_to_string(&missing_file).unwrap(),
            "hello world"
        );

        let ambiguous = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "dup.txt",
                serde_json::json!({
                    "old": "a",
                    "new": "b",
                    "expected_replacements": 1,
                    "allow_multiple": false,
                }),
            ),
        ));
        assert_eq!(ambiguous["changed"], false);
        assert!(ambiguous["error"].as_str().unwrap().contains("multiple"));
        assert_eq!(std::fs::read_to_string(&dup_file).unwrap(), "a a a");
    }

    #[test]
    fn file_replace_in_file_rejects_count_mismatch_and_non_utf8_without_write() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let count_file = tmp.path().join("count.txt");
        let bin_file = tmp.path().join("bin.dat");
        std::fs::write(&count_file, "a a a").unwrap();
        std::fs::write(&bin_file, [0xFF, 0xFE, 0xFD]).unwrap();

        let count = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "count.txt",
                serde_json::json!({
                    "old": "a",
                    "new": "b",
                    "expected_replacements": 2,
                    "allow_multiple": true,
                }),
            ),
        ));
        assert_eq!(count["changed"], false);
        assert_eq!(count["occurrences"], 3);
        assert!(count["error"].as_str().unwrap().contains("mismatch"));
        assert_eq!(std::fs::read_to_string(&count_file).unwrap(), "a a a");

        let non_utf8 = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "bin.dat",
                serde_json::json!({
                    "old": "x",
                    "new": "y",
                    "expected_replacements": 1,
                    "allow_multiple": false,
                }),
            ),
        ));
        assert_eq!(non_utf8["changed"], false);
        assert!(non_utf8["error"].as_str().unwrap().contains("UTF-8"));
    }

    #[test]
    fn file_replace_in_file_rejects_string_allow_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "a a a").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_replace_in_file",
                "target.txt",
                serde_json::json!({
                    "old": "a",
                    "new": "b",
                    "expected_replacements": 3,
                    "allow_multiple": "false",
                }),
            ),
        ));

        assert_eq!(out["changed"], false);
        assert_eq!(out["error"], "allow_multiple must be a boolean");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a a a");
    }

    #[test]
    fn file_write_project_file_creates_parent_dirs_and_reports_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("nested/new.txt");

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "nested/new.txt",
                serde_json::json!({
                    "path": "nested/new.txt",
                    "content": "line1\nline2\n",
                    "overwrite": false,
                    "expected_sha256": null,
                    "expected_content_prefix": null,
                }),
            ),
        ));

        assert_eq!(out["created"], true);
        assert_eq!(out["overwritten"], false);
        assert_eq!(out["bytes_written"], 12);
        assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
        assert!(out["warning"].is_null());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "line1\nline2\n");
    }

    #[test]
    fn file_write_project_file_rejects_existing_without_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "original").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "new",
                    "overwrite": false,
                    "expected_sha256": null,
                    "expected_content_prefix": null,
                }),
            ),
        ));

        assert_eq!(out["created"], false);
        assert_eq!(out["overwritten"], false);
        assert!(out["error"].as_str().unwrap().contains("overwrite"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn file_write_project_file_rejects_string_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "original").unwrap();

        let out = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "new",
                    "overwrite": "false",
                    "expected_sha256": null,
                    "expected_content_prefix": null,
                }),
            ),
        ));

        assert_eq!(out["created"], false);
        assert_eq!(out["error"], "overwrite must be a boolean");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn file_write_project_file_enforces_sha_and_prefix_guards() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "original").unwrap();
        let original_sha = sha256_hex_bytes("original".as_bytes());

        let sha_ok = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "v1 replaced",
                    "overwrite": true,
                    "expected_sha256": original_sha,
                    "expected_content_prefix": null,
                }),
            ),
        ));
        assert_eq!(sha_ok["overwritten"], true);
        assert!(sha_ok["warning"].is_null());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 replaced");

        let prefix_ok = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "v1 final",
                    "overwrite": true,
                    "expected_sha256": null,
                    "expected_content_prefix": "v1 ",
                }),
            ),
        ));
        assert_eq!(prefix_ok["overwritten"], true);
        assert!(prefix_ok["warning"].is_null());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");

        let sha_bad = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "bad",
                    "overwrite": true,
                    "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "expected_content_prefix": null,
                }),
            ),
        ));
        assert_eq!(sha_bad["created"], false);
        assert!(sha_bad["error"].as_str().unwrap().contains("sha256"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");
    }

    #[test]
    fn file_write_project_file_warns_on_unguarded_overwrite_and_rejects_bad_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        let file = tmp.path().join("target.txt");
        std::fs::write(&file, "v2 content").unwrap();

        let prefix_bad = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "bad",
                    "overwrite": true,
                    "expected_sha256": null,
                    "expected_content_prefix": "v1 ",
                }),
            ),
        ));
        assert_eq!(prefix_bad["created"], false);
        assert!(prefix_bad["error"].as_str().unwrap().contains("prefix"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 content");

        let unguarded = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "target.txt",
                serde_json::json!({
                    "content": "unguarded",
                    "overwrite": true,
                    "expected_sha256": null,
                    "expected_content_prefix": null,
                }),
            ),
        ));
        assert_eq!(unguarded["overwritten"], true);
        assert!(unguarded["warning"]
            .as_str()
            .unwrap()
            .contains("expected_sha256"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "unguarded");

        let nul = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_write_project_file",
                "new.txt",
                serde_json::json!({
                    "content": "a\u{0000}b",
                    "overwrite": false,
                    "expected_sha256": null,
                    "expected_content_prefix": null,
                }),
            ),
        ));
        assert_eq!(nul["created"], false);
        assert!(nul["error"].as_str().unwrap().contains("NUL"));
        assert!(!tmp.path().join("new.txt").exists());
    }

    #[test]
    fn file_apply_text_edits_applies_multi_file_transaction() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "gamma\n").unwrap();
        let hash = |path: &str| sha256_hex_bytes(&std::fs::read(tmp.path().join(path)).unwrap());

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "a.txt",
                serde_json::json!({
                    "changes": [
                        {
                            "kind": "edit",
                            "path": "a.txt",
                            "expected_sha256": hash("a.txt"),
                            "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                        },
                        {"kind": "create", "path": "nested/new.txt", "content": "new\n"},
                        {"kind": "delete", "path": "b.txt", "expected_sha256": hash("b.txt")},
                        {"kind": "rename", "path": "c.txt", "to_path": "moved/c.txt", "expected_sha256": hash("c.txt")}
                    ]
                }),
            ),
        ));

        assert_eq!(out["changed"], true);
        assert_eq!(out["applied_count"], 4);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
            "ALPHA\n"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("nested/new.txt")).unwrap(),
            "new\n"
        );
        assert!(!tmp.path().join("b.txt").exists());
        assert!(!tmp.path().join("c.txt").exists());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("moved/c.txt")).unwrap(),
            "gamma\n"
        );
        assert_eq!(out["files"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn file_apply_text_edits_hash_conflict_keeps_every_file_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
        let a_hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("a.txt")).unwrap());

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "a.txt",
                serde_json::json!({
                    "changes": [
                        {
                            "kind": "edit",
                            "path": "a.txt",
                            "expected_sha256": a_hash,
                            "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                        },
                        {
                            "kind": "delete",
                            "path": "b.txt",
                            "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        }
                    ]
                }),
            ),
        ));

        assert_eq!(out["error_kind"], "sha256_conflict");
        assert_eq!(out["change_index"], 1);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
            "alpha\n"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("b.txt")).unwrap(),
            "beta\n"
        );
    }

    #[test]
    fn file_apply_text_edits_rejects_resolved_path_aliases() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.txt"), "alpha\n").unwrap();
        let hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("src/a.txt")).unwrap());

        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "src/a.txt",
                serde_json::json!({
                    "changes": [
                        {
                            "kind": "edit",
                            "path": "src/a.txt",
                            "expected_sha256": hash,
                            "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                        },
                        {
                            "kind": "delete",
                            "path": "src//a.txt",
                            "expected_sha256": hash
                        }
                    ]
                }),
            ),
        ));

        assert_eq!(out["error_kind"], "path_overlap");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("src/a.txt")).unwrap(),
            "alpha\n"
        );
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
        let msg = out["error"].as_str().unwrap();
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
        let msg = out["error"].as_str().unwrap();
        assert!(msg.contains("matched 2 times"));
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
        assert!(err.contains("expected_sha256 does not match"));
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
        assert_eq!(out["applied_count"], 1);
        assert_eq!(out["files"][0]["edits"].as_array().unwrap().len(), 3);
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
        let lsp = webcodex_agent::LspSupervisor::default();
        dispatch_request(
            &sink,
            &AgentPolicy::default(),
            &shell,
            &jobs,
            &projects_dir,
            &lsp,
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
    fn job_manager_stop_all_clears_queue_and_requests_running_stop() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = test_config(tmp.path().join("config/projects.d"));
        let jobs = JobManager::new(1);
        let stop_requested = Arc::new(AtomicBool::new(false));
        jobs.jobs.lock().unwrap().insert(
            "running-job".to_string(),
            RunningJob {
                client_id: "ws-client".to_string(),
                child: None,
                stop_requested: stop_requested.clone(),
            },
        );
        let (sink, mut rx) = ws_sink("ws-client");
        let request = ShellAgentShellRequest {
            request_id: "req-queued".to_string(),
            client_id: "ws-client".to_string(),
            kind: "start_job".to_string(),
            job_id: Some("queued-job".to_string()),
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
            command: "sleep 60".to_string(),
            stdin: None,
            timeout_secs: 60,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
        };

        jobs.enqueue(
            sink,
            cfg.policy.clone(),
            cfg.shell.clone(),
            projects_dir(&cfg),
            request,
        );
        match rx.try_recv().expect("queued status was sent") {
            AgentEnvelope::JobUpdate { payload } => {
                assert_eq!(payload.job_id, "queued-job");
                assert_eq!(payload.status, "agent_queued");
            }
            other => panic!("expected job_update, got {:?}", other.kind()),
        }
        assert_eq!(jobs.queued.lock().unwrap().len(), 1);

        jobs.stop_all();

        assert!(stop_requested.load(Ordering::SeqCst));
        assert!(jobs.queued.lock().unwrap().is_empty());
    }

    #[test]
    fn file_request_kind_includes_anchor_edit_ops() {
        for kind in [
            "file_read",
            "file_write",
            "file_list",
            "file_project_overview",
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
    fn project_overview_agent_request_returns_metadata_without_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join("Cargo.toml"), "private manifest content").unwrap();
        std::fs::write(tmp.path().join("README.md"), "private readme content").unwrap();
        std::fs::write(tmp.path().join(".env"), "TOKEN=not-returned").unwrap();
        let request = json_file_op_request(
            tmp.path(),
            "file_project_overview",
            ".",
            serde_json::json!({"max_depth": 2, "limit": 200}),
        );

        let output = line_edit_json(handle_file_request(&policy, &request));
        assert_eq!(output["schema_version"], 1);
        assert_eq!(output["deterministic"], true);
        assert!(output.to_string().contains("Cargo.toml"));
        assert!(!output.to_string().contains("private manifest content"));
        assert!(!output.to_string().contains("TOKEN=not-returned"));
        assert!(!output.to_string().contains(".env"));
        assert!(!output
            .to_string()
            .contains(&tmp.path().display().to_string()));
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
            validation: None,
            lsp: None,
        };
        let pdir = projects_dir(&cfg);
        let lsp = webcodex_agent::LspSupervisor::default();
        let ran =
            dispatch_request(&sink, &cfg.policy, &cfg.shell, &jobs, &pdir, &lsp, request).unwrap();
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
                validation: None,
                lsp: None,
            };
            let ran = dispatch_request(
                &sink,
                &cfg.policy,
                &cfg.shell,
                &jobs,
                &pdir,
                &webcodex_agent::LspSupervisor::default(),
                request,
            )
            .unwrap();
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
            validation: None,
            lsp: None,
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
            websocket_session(
                &cfg,
                Vec::new(),
                "inst-1",
                &webcodex_agent::LspSupervisor::default(),
            ),
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
