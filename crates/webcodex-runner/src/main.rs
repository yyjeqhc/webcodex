use reqwest::blocking::Client;
#[cfg(test)]
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use webcodex_process::{GracefulTermination, ManagedChild};
use webcodex_runner::shutdown::{lock_unpoison, ActivityTracker, BackgroundThreads};

mod webcodex_runner;
use webcodex_runner::job_manager::JobManager;
#[cfg(test)]
use webcodex_runner::{cwd_allowed, PreparedShellProfileCache};

use runner_operation::RunnerFileOperation;
#[cfg(test)]
use runner_operation::RunnerOperation;
use webcodex_core::{
    apply_edits_shared, apply_patch_shared, artifact_policy, build_info, lsp_bridge, mcp_gateway,
    runner_operation, runner_protocol, validation_bridge,
};
use webcodex_runner_config as runner_config;
use webcodex_workspace::project_overview;
#[cfg(feature = "workspace-checkpoints")]
use webcodex_workspace::workspace_checkpoint;

use runner_protocol::{
    validation_infrastructure_failure_code, RunnerCapabilities, RunnerPolicySummary,
    RunnerPollPayload, RunnerPollRequest, RunnerPollResponse, RunnerProjectSummary,
    RunnerRegisterRequest, RunnerRegisterResponse, RunnerRequest, ShellJobInventory,
    ShellJobTestCountEvidence, ShellJobValidationStep, ShellProfileSummaryEntry,
    ShellProfilesSummary, ShellProjectInventoryPage, ShellProjectInventoryStatus,
    RUNNER_PROTOCOL_GENERATION_V2, VALIDATION_STEP_WAIT_FAILED_CODE,
};
#[cfg(test)]
use runner_protocol::{RunnerJobUpdateRequest, ShellCommandExecutionState};

#[cfg(test)]
use runner_config::{TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET};
#[cfg(test)]
use runner_protocol::{RunnerEnvelope, RUNNER_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::net::SocketAddr;
use webcodex_runner::contains_any;
#[cfg(all(test, feature = "workspace-checkpoints"))]
use webcodex_runner::is_checkpoint_request_kind;
use webcodex_runner::output_text::{OutputTextDecoder, OutputTextSource};
#[cfg(test)]
use webcodex_runner::QuicClientConfig;
#[cfg(test)]
use webcodex_runner::{
    auto_transport_plan, build_ws_request, default_quic_alpn, default_quic_connect_timeout_secs,
    default_quic_keepalive_interval_secs, default_websocket_connect_timeout_secs,
    effective_transport, load_runner_project_summaries_from_dir, non_empty_token,
    parse_runner_project_toml, quic_client_bind_addr_for, resolve_quic_config,
    resolve_quic_server_addrs, run_shell, runner_project_summary, server_url_to_ws,
    sha256_hex_bytes, validate_project_path_policy, websocket_session, RunnerRuntimeState,
    ShellProfileConfig, CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS, WS_OUTGOING_CAPACITY,
};
use webcodex_runner::{
    client_profile_runner_config, configured_validation_job_command, default_config_path,
    dispatch_request_with_outcome, err_cmd, handle_apply_patch_file_request,
    handle_apply_text_edits_file_request, handle_artifact_file_operation,
    handle_basic_file_request, handle_write_project_file_request, hostname, load_config,
    max_concurrent_jobs, ok_cmd, project_registry_dir, resolve_requested_path, run_runner,
    validate_client_profile, validate_structured_edit_runner_path, CommandResult, HotRunnerConfig,
    HttpSendConfig, PreparedShellProfile, ReloadableRunnerConfig, RunnerConfig,
    RunnerDispatchOutcome, RunnerPolicy, RunnerProjectCache, RunnerSink, ShellConfig,
    SubmitResultError,
};
#[cfg(test)]
use webcodex_runner::{
    dispatch_request, is_artifact_request_kind, is_basic_file_request_kind,
    is_structured_edit_request_kind,
};

#[cfg(feature = "workspace-checkpoints")]
use webcodex_runner::handle_checkpoint_file_request;
#[cfg(test)]
use webcodex_runner::SshConfig;
use webcodex_runner::SshConnectionPool;

const RUNNER_REGISTER_PATH: &str = "/api/shell/agent/register";
const RUNNER_POLL_PATH: &str = "/api/shell/agent/poll";
/// Polling HTTP responses can carry the current largest 15 MiB request
/// payloads plus their JSON envelope, but must never be loaded without a
/// finite bound.
const RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunnerCliAction {
    Run {
        config_path: PathBuf,
        once: bool,
        stop_on_stdin_eof: bool,
    },
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn usage() -> &'static str {
    "Usage: webcodex-runner [--config PATH] [--once] [--stop-on-stdin-eof]\n\n\
     Options:\n\
       -h, --help                 Print help and exit\n\
       -V, --version              Print version and exit\n\
       -c, --config PATH          Runner config path for normal runtime\n\
       --profile NAME             Client config profile for default config path\n\
       --once                     Complete one successful poll, then exit (polling transport)\n\
       --stop-on-stdin-eof        Stop when the invoking parent closes stdin\n\n\
     With --profile, the default config path is derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit\n\
     --config overrides the profile-derived default.\n\n\
     Environment:\n\
       WEBCODEX_RUNNER_CONFIG     default config path override\n\
     Example runner.toml:\n\
       server_url = \"https://v4.yyjeqhc.cn\"\n\
       token = \"...\"\n\
       client_id = \"xrh\"\n\
       display_name = \"XRH\"\n\
       owner = \"yyjeqhc\"\n\
       project_registry_dir = \"/root/.config/webcodex/project-registry\"\n\
       poll_interval_ms = 1000\n\
\n\
       [policy]\n\
       allow_raw_shell = true\n\
       allow_cwd_anywhere = true\n\
       max_timeout_secs = 3600\n\
       max_output_bytes = 262144\n"
}

fn parse_args() -> Result<RunnerCliAction, String> {
    parse_runner_args(std::env::args().skip(1))
}

fn parse_runner_args<I, S>(args: I) -> Result<RunnerCliAction, String>
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
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            _ => {}
        }
    }
    let runner_config_env = std::env::var("WEBCODEX_RUNNER_CONFIG").ok();
    let legacy_agent_config_env = std::env::var("WEBCODEX_AGENT_CONFIG").ok();
    let mut config_path: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut once = false;
    let mut stop_on_stdin_eof = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            "--once" => once = true,
            "--stop-on-stdin-eof" => stop_on_stdin_eof = true,
            "--config" | "-c" => {
                let Some(path) = args.next() else {
                    return Err("--config requires a path".to_string());
                };
                config_path = Some(PathBuf::from(path));
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
    let profile = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?;
    let config_path = if let Some(config_path) = config_path {
        config_path
    } else {
        if let Some(profile) = profile {
            client_profile_runner_config(&profile)?
        } else {
            if runner_config_env.is_some() && legacy_agent_config_env.is_some() {
                return Err(
                    "WEBCODEX_RUNNER_CONFIG and legacy WEBCODEX_AGENT_CONFIG cannot both be set"
                        .to_string(),
                );
            }
            if runner_config_env.is_none() && legacy_agent_config_env.is_some() {
                eprintln!(
                    "webcodex-runner warning: WEBCODEX_AGENT_CONFIG is deprecated; use WEBCODEX_RUNNER_CONFIG instead. Legacy startup compatibility will be removed in WebCodex {}.",
                    runner_config::paths::LEGACY_RUNNER_CONFIG_REMOVAL_VERSION
                );
            }
            runner_config_env
                .or(legacy_agent_config_env)
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(default_config_path)?
        }
    };
    Ok(RunnerCliAction::Run {
        config_path,
        once,
        stop_on_stdin_eof,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunnerHttpErrorKind {
    ServerUnavailable,
    Auth,
    NotFound,
    /// A local URL/TLS configuration failure that retrying cannot repair.
    Config,
    /// 4xx (other than auth/endpoint kinds): the server understood the
    /// exchange and rejected this exact request. Resending the identical
    /// payload cannot succeed.
    ClientRejected,
    Status,
    RequestTimeout,
    Request,
    /// The response was incomplete or was recognizably produced by a
    /// temporary proxy/upstream failure.
    DecodeTransient,
    /// The response was complete enough to prove that it does not implement
    /// the expected server protocol.
    ProtocolDecode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerHttpError {
    kind: RunnerHttpErrorKind,
    path: String,
    summary: String,
    /// Bounded structured server error, when the response contract supplied
    /// one. Recovery classifiers use this instead of parsing display strings.
    server_error: Option<String>,
}

impl RunnerHttpError {
    fn status(path: &str, status: reqwest::StatusCode, body: &str) -> Self {
        let kind = match status.as_u16() {
            401 | 403 => RunnerHttpErrorKind::Auth,
            404 => RunnerHttpErrorKind::NotFound,
            // Explicitly retryable request-level statuses.
            408 | 429 => RunnerHttpErrorKind::Status,
            code if (500..600).contains(&code) => RunnerHttpErrorKind::ServerUnavailable,
            code if (400..500).contains(&code) => RunnerHttpErrorKind::ClientRejected,
            _ if looks_like_proxy_html_error(body) => RunnerHttpErrorKind::ServerUnavailable,
            _ => RunnerHttpErrorKind::Status,
        };
        let server_error = structured_body_error(body);
        let mut summary = http_status_summary(status);
        if kind == RunnerHttpErrorKind::ClientRejected {
            if let Some(detail) = server_error.as_deref() {
                summary = format!("{}: {}", summary, detail);
            }
        }
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary,
            server_error,
        }
    }

    fn request(path: &str, error: reqwest::Error) -> Self {
        let chain = error_chain_text(&error);
        let kind = if error.is_builder() || looks_like_fatal_tls_request(&chain) {
            RunnerHttpErrorKind::Config
        } else if looks_like_server_down_request(&error, &chain) {
            RunnerHttpErrorKind::ServerUnavailable
        } else if error.is_timeout() {
            RunnerHttpErrorKind::RequestTimeout
        } else {
            RunnerHttpErrorKind::Request
        };
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary: request_error_summary(error, &chain),
            server_error: None,
        }
    }

    fn decode_transient(path: &str, summary: String) -> Self {
        Self {
            kind: RunnerHttpErrorKind::DecodeTransient,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }

    fn protocol_decode(path: &str, summary: String) -> Self {
        Self {
            kind: RunnerHttpErrorKind::ProtocolDecode,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }
}

impl std::fmt::Display for RunnerHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            RunnerHttpErrorKind::ServerUnavailable => {
                write!(f, "server unavailable for {}: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::Auth => write!(
                f,
                "authentication failed for {}: {}; check agent token/config",
                self.path, self.summary
            ),
            RunnerHttpErrorKind::NotFound => write!(
                f,
                "endpoint missing or incompatible server for {}: {}",
                self.path, self.summary
            ),
            RunnerHttpErrorKind::Config => {
                write!(
                    f,
                    "HTTP/TLS configuration failed for {}: {}",
                    self.path, self.summary
                )
            }
            RunnerHttpErrorKind::ClientRejected => {
                write!(f, "server rejected {} request: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::Status
            | RunnerHttpErrorKind::RequestTimeout
            | RunnerHttpErrorKind::Request => {
                write!(f, "{} request failed: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::DecodeTransient => {
                write!(
                    f,
                    "transient response corruption for {}: {}",
                    self.path, self.summary
                )
            }
            RunnerHttpErrorKind::ProtocolDecode => write!(
                f,
                "response from {} incompatible with server protocol: {}",
                self.path, self.summary
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterRecoveryAction {
    Retry,
    WaitForLease,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegisterErrorKind {
    Transient,
    LeaseConflict,
    Auth,
    EndpointMissing,
    Rejected,
    Config,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisterError {
    kind: RegisterErrorKind,
    message: String,
}

impl RegisterError {
    fn from_http(error: RunnerHttpError, client_id: &str) -> Self {
        let kind = match error.kind {
            RunnerHttpErrorKind::ServerUnavailable
            | RunnerHttpErrorKind::Status
            | RunnerHttpErrorKind::RequestTimeout
            | RunnerHttpErrorKind::Request
            | RunnerHttpErrorKind::DecodeTransient => RegisterErrorKind::Transient,
            RunnerHttpErrorKind::Auth => RegisterErrorKind::Auth,
            RunnerHttpErrorKind::NotFound => RegisterErrorKind::EndpointMissing,
            RunnerHttpErrorKind::Config => RegisterErrorKind::Config,
            RunnerHttpErrorKind::ProtocolDecode => RegisterErrorKind::Protocol,
            RunnerHttpErrorKind::ClientRejected
                if is_active_instance_lease_conflict(client_id, error.server_error.as_deref()) =>
            {
                RegisterErrorKind::LeaseConflict
            }
            RunnerHttpErrorKind::ClientRejected => RegisterErrorKind::Rejected,
        };
        let message = if error.kind == RunnerHttpErrorKind::ProtocolDecode {
            format!(
                "register response incompatible with server protocol: endpoint={} {}",
                error.path, error.summary
            )
        } else {
            error.to_string()
        };
        Self { kind, message }
    }

    fn from_response_error(client_id: &str, error: Option<String>) -> Self {
        let summary =
            bounded_single_line(error.as_deref().unwrap_or("register failed without error"));
        let kind = if is_active_instance_lease_conflict(client_id, Some(&summary)) {
            RegisterErrorKind::LeaseConflict
        } else if looks_like_auth_failure_message(&summary) {
            RegisterErrorKind::Auth
        } else {
            RegisterErrorKind::Rejected
        };
        Self {
            kind,
            message: format!("register rejected by server: {summary}"),
        }
    }

    fn recovery_action(&self) -> RegisterRecoveryAction {
        match self.kind {
            RegisterErrorKind::Transient => RegisterRecoveryAction::Retry,
            RegisterErrorKind::LeaseConflict => RegisterRecoveryAction::WaitForLease,
            RegisterErrorKind::Auth
            | RegisterErrorKind::EndpointMissing
            | RegisterErrorKind::Rejected
            | RegisterErrorKind::Config
            | RegisterErrorKind::Protocol => RegisterRecoveryAction::Fatal,
        }
    }

    fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollingRecoveryAction {
    RetryPoll,
    ReRegister,
    Fatal,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PollErrorKind {
    Transient,
    SessionLost,
    Auth,
    EndpointMissing,
    Rejected,
    Config,
    Protocol,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PollError {
    kind: PollErrorKind,
    message: String,
}

impl PollError {
    fn new(kind: PollErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn from_http(error: RunnerHttpError, client_id: &str) -> Self {
        match error.kind {
            RunnerHttpErrorKind::ServerUnavailable => Self::new(
                PollErrorKind::Transient,
                format!(
                    "server unavailable while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::Auth => Self::new(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::NotFound => Self::new(
                PollErrorKind::EndpointMissing,
                format!(
                    "poll endpoint missing or incompatible server while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::Config => Self::new(
                PollErrorKind::Config,
                format!(
                    "HTTP/TLS configuration failed while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::RequestTimeout => Self::new(
                PollErrorKind::Transient,
                format!(
                    "poll request timed out while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::ClientRejected
                if is_unknown_polling_session(client_id, error.server_error.as_deref()) =>
            {
                Self::new(
                    PollErrorKind::SessionLost,
                    format!(
                        "polling session is not registered for client_id={}",
                        bounded_single_line(client_id)
                    ),
                )
            }
            RunnerHttpErrorKind::ClientRejected => Self::new(
                PollErrorKind::Rejected,
                format!(
                    "server permanently rejected polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::Status | RunnerHttpErrorKind::Request => Self::new(
                PollErrorKind::Transient,
                format!(
                    "poll request failed while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::DecodeTransient => Self::new(
                PollErrorKind::Transient,
                format!(
                    "transient poll response corruption: endpoint={} {}",
                    error.path, error.summary
                ),
            ),
            RunnerHttpErrorKind::ProtocolDecode => Self::new(
                PollErrorKind::Protocol,
                format!(
                    "poll response incompatible with server protocol: endpoint={} {}",
                    error.path, error.summary
                ),
            ),
        }
    }

    /// Classify a fatal result submission failure surfaced by
    /// `dispatch_request`. Permanent rejection and exhausted transient retries
    /// are resolved as payload-lifecycle outcomes inside the HTTP sink, so
    /// neither can trigger polling sleep/re-registration recovery here.
    fn from_submit(error: SubmitResultError) -> Self {
        match error {
            SubmitResultError::FatalAuth(message) => Self::new(PollErrorKind::Auth, message),
            SubmitResultError::FatalProtocol(message) => {
                Self::new(PollErrorKind::EndpointMissing, message)
            }
            SubmitResultError::FatalConfig(message) => Self::new(PollErrorKind::Config, message),
            SubmitResultError::TransportClosed(message) => {
                Self::new(PollErrorKind::Rejected, message)
            }
            SubmitResultError::Shutdown(message) => Self::new(PollErrorKind::Shutdown, message),
        }
    }

    fn from_response_error(client_id: &str, error: Option<String>) -> Self {
        let message = error.unwrap_or_else(|| "poll failed without error".to_string());
        let summary = bounded_single_line(&message);
        if looks_like_auth_failure_message(&summary) {
            Self::new(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    RUNNER_POLL_PATH, summary
                ),
            )
        } else if is_unknown_polling_session(client_id, Some(&summary)) {
            Self::new(
                PollErrorKind::SessionLost,
                format!(
                    "polling session is not registered for client_id={}",
                    bounded_single_line(client_id)
                ),
            )
        } else {
            Self::new(
                PollErrorKind::Rejected,
                format!("server permanently rejected polling response: {summary}"),
            )
        }
    }

    fn recovery_action(&self) -> PollingRecoveryAction {
        match self.kind {
            PollErrorKind::Transient => PollingRecoveryAction::RetryPoll,
            PollErrorKind::SessionLost => PollingRecoveryAction::ReRegister,
            PollErrorKind::Auth
            | PollErrorKind::EndpointMissing
            | PollErrorKind::Rejected
            | PollErrorKind::Config
            | PollErrorKind::Protocol => PollingRecoveryAction::Fatal,
            PollErrorKind::Shutdown => PollingRecoveryAction::Shutdown,
        }
    }

    #[cfg(test)]
    fn is_terminal(&self) -> bool {
        self.recovery_action() == PollingRecoveryAction::Fatal
    }

    #[cfg(test)]
    fn is_shutdown(&self) -> bool {
        self.kind == PollErrorKind::Shutdown
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

/// Polling admits four ordinary Runner requests concurrently before it stops
/// polling. The Server remains the only pending-work queue: no fifth request is
/// dequeued until one worker completes. Four matches the current bounded control
/// responsiveness target while avoiding an unbounded process-local thread fanout;
/// Job scheduling remains a separate configured queue/concurrency contract.
pub(crate) const POLLING_DISPATCH_MAX_IN_FLIGHT: usize = 4;

struct PollingDispatch {
    request_id: String,
    sink: RunnerSink,
    config: Arc<HotRunnerConfig>,
    runtime: Arc<ReloadableRunnerConfig>,
    jobs: JobManager,
    persistent_shells: webcodex_runner::PersistentShellManager,
    project_registry_dir: PathBuf,
    lsp: webcodex_runner::LspSupervisor,
    browser: webcodex_browser::BrowserSupervisor,
    request: RunnerRequest,
}

impl PollingDispatch {
    fn run(self) -> Result<RunnerDispatchOutcome, SubmitResultError> {
        dispatch_request_with_outcome(
            &self.sink,
            &self.config,
            &self.runtime,
            &self.jobs,
            &self.persistent_shells,
            &self.project_registry_dir,
            &self.lsp,
            &self.browser,
            self.request,
        )
    }
}

struct PollingDispatchCompletion {
    request_id: String,
    dispatch_result: Result<RunnerDispatchOutcome, SubmitResultError>,
}

/// Sends a completion even if a worker unwinds. It is declared before the
/// ActivityGuard in the worker so reverse drop order releases the activity
/// slot only after dispatch and result submission, then publishes completion.
struct PollingDispatchCompletionOnDrop {
    completion_tx: mpsc::SyncSender<PollingDispatchCompletion>,
    request_id: String,
    dispatch_result: Option<Result<RunnerDispatchOutcome, SubmitResultError>>,
}

impl PollingDispatchCompletionOnDrop {
    fn new(completion_tx: mpsc::SyncSender<PollingDispatchCompletion>, request_id: String) -> Self {
        Self {
            completion_tx,
            request_id,
            dispatch_result: None,
        }
    }

    fn complete(&mut self, result: Result<RunnerDispatchOutcome, SubmitResultError>) {
        self.dispatch_result = Some(result);
    }
}

impl Drop for PollingDispatchCompletionOnDrop {
    fn drop(&mut self) {
        let dispatch_result = self.dispatch_result.take().unwrap_or_else(|| {
            Err(SubmitResultError::TransportClosed(
                "polling dispatch worker closed unexpectedly".to_string(),
            ))
        });
        let _ = self.completion_tx.send(PollingDispatchCompletion {
            request_id: std::mem::take(&mut self.request_id),
            dispatch_result,
        });
    }
}

/// Process-local coordination for normal polling dispatches. The Server queue
/// remains the only pending-work queue: this supervisor admits at most
/// `POLLING_DISPATCH_MAX_IN_FLIGHT` already-dequeued requests, creates no local
/// holding queue, and returns worker completion/fatal submission outcomes to
/// the polling control loop.
pub(crate) struct PollingDispatchSupervisor {
    completion_tx: mpsc::SyncSender<PollingDispatchCompletion>,
    completion_rx: mpsc::Receiver<PollingDispatchCompletion>,
    in_flight: usize,
    background_threads: Arc<BackgroundThreads>,
    dispatches: ActivityTracker,
}

impl PollingDispatchSupervisor {
    pub(crate) fn new(
        background_threads: Arc<BackgroundThreads>,
        dispatches: ActivityTracker,
    ) -> Self {
        let (completion_tx, completion_rx) = mpsc::sync_channel(POLLING_DISPATCH_MAX_IN_FLIGHT);
        Self {
            completion_tx,
            completion_rx,
            in_flight: 0,
            background_threads,
            dispatches,
        }
    }

    fn has_capacity(&self) -> bool {
        self.in_flight < POLLING_DISPATCH_MAX_IN_FLIGHT
    }

    fn spawn(&mut self, dispatch: PollingDispatch) -> Result<(), PollError> {
        if !self.has_capacity() {
            return Err(PollError::new(
                PollErrorKind::Config,
                "polling dispatch capacity invariant violated",
            ));
        }
        let completion_tx = self.completion_tx.clone();
        let dispatch_guard = self.dispatches.enter();
        let request_id = dispatch.request_id.clone();
        let handle = std::thread::Builder::new()
            .name("webcodex-poll-dispatch".to_string())
            .spawn(move || {
                let mut completion =
                    PollingDispatchCompletionOnDrop::new(completion_tx, request_id);
                let _dispatch_guard = dispatch_guard;
                completion.complete(dispatch.run());
            })
            .map_err(|error| {
                PollError::new(
                    PollErrorKind::Config,
                    format!("failed to start polling dispatch worker: {error}"),
                )
            })?;
        self.in_flight += 1;
        self.background_threads.register(handle);
        Ok(())
    }

    fn record_completion(
        &mut self,
        project_cache: &mut RunnerProjectCache,
        completion: PollingDispatchCompletion,
    ) -> Result<bool, SubmitResultError> {
        self.in_flight = self.in_flight.checked_sub(1).unwrap_or_else(|| {
            debug_assert!(false, "polling completion without an in-flight dispatch");
            0
        });
        let _request_id = completion.request_id;
        match completion.dispatch_result {
            Ok(outcome) => {
                if outcome.project_cache_invalidation_required {
                    project_cache.invalidate();
                }
                Ok(outcome.handled)
            }
            Err(error) => Err(error),
        }
    }

    /// Inspect every completion currently available. This is called before
    /// each poll and after each poll/dispatch turn, so a fatal result delivery
    /// failure cannot be silently discarded by background dispatch.
    pub(crate) fn drain_completed(
        &mut self,
        project_cache: &mut RunnerProjectCache,
    ) -> Result<(), PollError> {
        loop {
            match self.completion_rx.try_recv() {
                Ok(completion) => {
                    let result = self
                        .record_completion(project_cache, completion)
                        .map(|_| ())
                        .map_err(PollError::from_submit);
                    let _ = self.background_threads.reap_finished();
                    result?;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let _ = self.background_threads.reap_finished();
                    return Ok(());
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
    }

    /// Apply backpressure before another Server dequeue. There is no local
    /// pending queue: when all slots are occupied the control loop waits for
    /// one worker completion (or shutdown) and only then polls again.
    pub(crate) fn wait_for_capacity_or_shutdown(
        &mut self,
        project_cache: &mut RunnerProjectCache,
        shutdown: &AtomicBool,
    ) -> Result<(), PollError> {
        while !self.has_capacity() {
            if shutdown.load(Ordering::SeqCst) {
                return Err(PollError::from_submit(SubmitResultError::Shutdown(
                    "process shutdown".to_string(),
                )));
            }
            match self.completion_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(completion) => {
                    let result = self
                        .record_completion(project_cache, completion)
                        .map(|_| ())
                        .map_err(PollError::from_submit);
                    let _ = self.background_threads.reap_finished();
                    result?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
        Ok(())
    }

    /// Preserve the former synchronous dispatch's bounded shutdown race:
    /// already-returned fatal auth/protocol/config outcomes win over clean
    /// shutdown. A worker-reported Shutdown is remembered while remaining
    /// completions get a short chance to expose a sibling fatal outcome.
    pub(crate) fn wait_for_shutdown_outcome(
        &mut self,
        project_cache: &mut RunnerProjectCache,
        wait: Duration,
    ) -> Result<(), PollError> {
        let deadline = Instant::now() + wait;
        let mut shutdown_error = None;
        while self.in_flight > 0 && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self
                .completion_rx
                .recv_timeout(remaining.min(Duration::from_millis(25)))
            {
                Ok(completion) => {
                    match self.record_completion(project_cache, completion) {
                        Ok(_) => {}
                        Err(error @ SubmitResultError::Shutdown(_)) => {
                            shutdown_error = Some(error);
                        }
                        Err(error) => {
                            let _ = self.background_threads.reap_finished();
                            return Err(PollError::from_submit(error));
                        }
                    }
                    let _ = self.background_threads.reap_finished();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
        let _ = self.background_threads.reap_finished();
        if let Some(error) = shutdown_error {
            Err(PollError::from_submit(error))
        } else {
            Ok(())
        }
    }
}

fn http_status_summary(status: reqwest::StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {}", status.as_u16(), reason),
        None => format!("HTTP {}", status.as_u16()),
    }
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

fn looks_like_fatal_tls_request(chain: &str) -> bool {
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "certificate verify failed",
            "invalid peer certificate",
            "unknownissuer",
            "notvalidforname",
            "certificateunknown",
            "invalid certificate",
            "no application protocol",
            "alpn mismatch",
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

fn is_active_instance_lease_conflict(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!(
        "agent client {} is already online with a different instance",
        client_id
    );
    error == Some(expected.as_str())
}

fn is_unknown_polling_session(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!("unknown shell client: {}", client_id);
    error == Some(expected.as_str())
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

fn bounded_endpoint_path(path: &str) -> String {
    let without_query = path.split_once('?').map_or(path, |(path, _)| path);
    bounded_single_line(without_query)
}

/// Extract the structured `error` field from a JSON error response body, if
/// present. Non-JSON bodies (proxy HTML, truncated payloads) yield `None` so
/// raw response bytes never leak into diagnostics.
fn structured_body_error(body: &str) -> Option<String> {
    const MAX_PARSE_BYTES: usize = 64 * 1024;
    if body.len() > MAX_PARSE_BYTES {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?.as_str()?;
    let error = bounded_single_line(error);
    if error.is_empty() {
        None
    } else {
        Some(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BoundedResponseBody {
    bytes: Vec<u8>,
    exceeded_limit: bool,
}

fn read_bounded_response_body<R: Read>(
    reader: &mut R,
    content_length: Option<u64>,
    max_bytes: usize,
) -> std::io::Result<BoundedResponseBody> {
    let read_limit = (max_bytes as u64).saturating_add(1);
    let initial_capacity = content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(initial_capacity);
    reader.take(read_limit).read_to_end(&mut bytes)?;
    let exceeded_limit = bytes.len() > max_bytes;
    if exceeded_limit {
        bytes.truncate(max_bytes);
    }
    Ok(BoundedResponseBody {
        bytes,
        exceeded_limit,
    })
}

fn bounded_response_content_type(
    value: Option<&reqwest::header::HeaderValue>,
    token: &str,
) -> String {
    match value {
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|value| {
                let media_type = value.split(';').next()?.trim();
                let lower = media_type.to_ascii_lowercase();
                let token = token.trim();
                if media_type.is_empty()
                    || lower.contains("authorization")
                    || lower.contains("bearer")
                    || (!token.is_empty() && media_type.contains(token))
                    || !media_type.chars().all(|ch| {
                        ch.is_ascii_alphanumeric()
                            || matches!(
                                ch,
                                '/' | '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '-'
                            )
                    })
                {
                    None
                } else {
                    Some(bounded_single_line(media_type))
                }
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "<redacted-or-invalid>".to_string()),
        None => "<missing>".to_string(),
    }
}

fn response_decode_summary(
    status: reqwest::StatusCode,
    content_type: &str,
    detail: impl AsRef<str>,
) -> String {
    format!(
        "status={} content_type={} {}",
        http_status_summary(status),
        content_type,
        detail.as_ref()
    )
}

fn looks_like_transient_proxy_response(content_type: &str, body: &[u8]) -> bool {
    const MAX_INSPECT_BYTES: usize = 8 * 1024;
    let inspected = &body[..body.len().min(MAX_INSPECT_BYTES)];
    let text = String::from_utf8_lossy(inspected);
    let lower = text.to_ascii_lowercase();
    let has_temporary_gateway_marker = contains_any(
        &lower,
        &[
            "bad gateway",
            "service unavailable",
            "gateway timeout",
            "upstream connect error",
            "upstream connection error",
            "upstream unavailable",
            "proxy error",
            "temporarily unavailable",
        ],
    );
    let looks_html = content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/html"))
        || lower.contains("<html")
        || lower.contains("<!doctype html");
    if looks_html && has_temporary_gateway_marker {
        return true;
    }
    let plain = lower.trim();
    body.len() <= MAX_INSPECT_BYTES
        && (matches!(
            plain,
            "bad gateway"
                | "service unavailable"
                | "gateway timeout"
                | "upstream unavailable"
                | "temporarily unavailable"
        ) || plain.starts_with("upstream connect error")
            || plain.starts_with("upstream connection error"))
}

fn serde_json_category_name(error: &serde_json::Error) -> &'static str {
    match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

fn decode_json_response<R>(
    path: &str,
    status: reqwest::StatusCode,
    content_type: &str,
    body: BoundedResponseBody,
) -> Result<R, RunnerHttpError>
where
    R: serde::de::DeserializeOwned,
{
    if body.bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(RunnerHttpError::decode_transient(
            path,
            response_decode_summary(status, content_type, "empty response body"),
        ));
    }
    if looks_like_transient_proxy_response(content_type, &body.bytes) {
        return Err(RunnerHttpError::decode_transient(
            path,
            response_decode_summary(
                status,
                content_type,
                "recognized temporary proxy/upstream response",
            ),
        ));
    }
    if body.exceeded_limit {
        return Err(RunnerHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                content_type,
                format!(
                    "response body exceeds limit_bytes={}",
                    RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    serde_json::from_slice(&body.bytes).map_err(|error| {
        let detail = format!(
            "serde_category={} line={} column={}",
            serde_json_category_name(&error),
            error.line(),
            error.column()
        );
        let summary = response_decode_summary(status, content_type, detail);
        if error.is_eof() {
            RunnerHttpError::decode_transient(path, summary)
        } else {
            RunnerHttpError::protocol_decode(path, summary)
        }
    })
}

fn post_json<T, R>(
    client: &Client,
    cfg: &RunnerConfig,
    path: &str,
    body: &T,
) -> Result<R, RunnerHttpError>
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
) -> Result<R, RunnerHttpError>
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
        .map_err(|e| RunnerHttpError::request(path, e))?;
    let status = resp.status();
    let content_type =
        bounded_response_content_type(resp.headers().get(reqwest::header::CONTENT_TYPE), token);
    let content_length = resp.content_length();
    if content_length.is_some_and(|length| length > RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES as u64) {
        if !status.is_success() {
            return Err(RunnerHttpError::status(path, status, ""));
        }
        return Err(RunnerHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                &content_type,
                format!(
                    "declared response body exceeds limit_bytes={}",
                    RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    let mut resp = resp;
    let body = match read_bounded_response_body(
        &mut resp,
        content_length,
        RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES,
    ) {
        Ok(body) => body,
        Err(error) if status.is_success() => {
            return Err(RunnerHttpError::decode_transient(
                path,
                response_decode_summary(
                    status,
                    &content_type,
                    format!("response body read interrupted io_kind={:?}", error.kind()),
                ),
            ));
        }
        Err(_) => return Err(RunnerHttpError::status(path, status, "")),
    };
    if !status.is_success() {
        let text = String::from_utf8_lossy(&body.bytes);
        return Err(RunnerHttpError::status(path, status, &text));
    }
    decode_json_response(path, status, &content_type, body)
}

/// Hidden, test/ops-only knob: parse `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION`
/// as a boolean. Default false (reconciliation stays on). Inline rather than
/// shared because the runner crate does not depend on the server config helpers.
fn disable_job_state_reconciliation_for_test() -> bool {
    matches!(
        std::env::var("WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION")
            .ok()
            .map(|raw| raw.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn runner_register_capabilities(cfg: &RunnerConfig) -> RunnerCapabilities {
    let mut capabilities = cfg.capabilities.clone().unwrap_or_default();
    // This binary accepts a structured local sh/bash selector on raw shell
    // requests. Older Runners omit the bit so current Servers fail closed.
    capabilities.explicit_shell_selection = true;
    capabilities.jobs = true;
    capabilities.file_read = true;
    capabilities.file_write = true;
    // This binary implements the narrow internal seek/read export-chunk path.
    // Older binaries omit the field so Control uses the existing slow fallback.
    capabilities.artifact_export_chunk_read = true;
    // Large export metadata (size/SHA/MIME) is verified with bounded streaming
    // I/O. Keep this separate from chunk-read support for rolling upgrades.
    capabilities.artifact_export_streaming_metadata = true;
    // This binary implements the complete bounded structured delete contract.
    // Older binaries omit the field and therefore keep using the Server's legacy path.
    capabilities.structured_file_delete = true;
    // This binary enforces ApplyTextEditInput.occurrence exactly. Older binaries
    // omit this additive effect-semantics capability and must not receive selectors.
    capabilities.apply_text_edit_occurrence = true;
    // Unique exact local edits may be preflighted against current content
    // without a historical whole-file SHA. The transactional source-SHA fence
    // before mutation remains mandatory.
    capabilities.apply_text_edit_local_guard_without_sha = true;
    // Line scopes are an additive rolling-upgrade fence: advertise only because
    // this binary resolves full-match containment before any mutation.
    capabilities.apply_text_edit_line_scope = true;
    // Codex Patch is an additive request kind with Runner-authoritative parsing and
    // transaction semantics. Older Runners omit it and must fail closed.
    capabilities.apply_patch = true;
    // WebCodex 0.4 requires every successful patch to expose the complete bounded
    // patch-plan/match metadata consumed by Server validation. Older apply_patch
    // implementations omit this capability and are rejected before dispatch.
    capabilities.apply_patch_match_metadata = true;
    // Enum-based matching is the 0.4 model-facing authority. Older Runners omit
    // it, so current Servers fail closed instead of falling back to old defaults.
    capabilities.apply_patch_matching_mode = true;
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    // SSH support intentionally depends on the local OpenSSH executable.
    // Authentication and Host aliases remain entirely Runner-local.
    capabilities.ssh_shell = SshConnectionPool::is_available();
    // This binary installs the bounded, process-local persistent-shell
    // manager. Older binaries omit this field and therefore fail closed.
    capabilities.persistent_shell = webcodex_persistent_shell::local_shell_supported();
    // SSH persistent shells reuse the same OpenSSH executable as `ssh_shell`.
    // Older binaries omit this field and therefore fail closed; it is never
    // inferred from `ssh_shell` + `persistent_shell`.
    capabilities.ssh_persistent_shell = SshConnectionPool::persistent_shell_available();
    capabilities.structured_validation_argv = true;
    // This binary durably round-trips Cargo test-count assertions with
    // validation Job context and reconciliation snapshots.
    capabilities.structured_cargo_test_count_assertion = true;
    // Explicit require_tests/no_run policy changes validation proof semantics,
    // so advertise durable preservation independently from the older count
    // assertion capability for rolling upgrades.
    capabilities.structured_cargo_test_execution_policy = true;
    // `--lib` expands the older structured Cargo test argv vocabulary, so
    // advertise it separately for mixed Server/Runner rolling upgrades.
    capabilities.structured_cargo_test_lib = true;
    // This binary accepts both legacy Go validation argv from old Servers and
    // the current machine-readable JSON argv. Do not trust static config or
    // infer this from generic structured validation support.
    capabilities.structured_go_test_json = true;
    // This binary also understands the first-class go_test durable metadata
    // identity. Keep this independent from JSON parsing so an old Runner that
    // supported Connector Go evidence cannot be mistaken for a first-class
    // go_test executor by a newer Server.
    capabilities.structured_go_test_tool = true;
    // Focused first-class go_test packages extend the older fixed `./...`
    // wire shape, so advertise them independently for rolling upgrades.
    capabilities.structured_go_test_packages = true;
    capabilities.structured_process_argv = true;
    capabilities.structured_script_payload = true;
    // JavaScript extends the older typed-script wire enum. Advertise it
    // separately so a newer Server never sends that variant to an older Runner
    // which already advertised structured_script_payload.
    capabilities.structured_script_javascript = true;
    // TypeScript extends the same typed-script wire enum independently from
    // JavaScript. This bit means the binary understands the semantic protocol;
    // local Node availability/version is resolved only when execution starts.
    capabilities.structured_script_typescript = true;
    capabilities.internal_posix_script = true;
    capabilities.structured_execution_jobs = true;
    // Detached process ownership is an independent additive authority. Until
    // each native backend is implemented and dogfooded it must fail closed
    // rather than being inferred from structured process + durable Jobs.
    capabilities.detached_process_jobs =
        cfg!(any(target_os = "linux", target_os = "macos", windows));
    capabilities.project_lifecycle = true;
    // This binary implements resolve_or_register_project; do not trust config to
    // advertise a capability that the binary does not implement.
    capabilities.project_path_registration = true;
    capabilities.managed_worktree = true;
    // Configured live roots and managed active Skills share one Runner-local runtime
    // boundary; managed lifecycle authority remains independently advertised.
    capabilities.skill_runtime = true;
    capabilities.skill_resource_execution = true;
    capabilities.skill_management = true;
    // Native Tool Plugins are a separate Runner-local gateway capability. Keep
    // this explicit even when zero Plugins are configured so cross-platform
    // `plugin_tool reload` can target the exact Runner.
    capabilities.native_tool_plugins = true;
    capabilities.managed_ssh_resources = true;
    // Formal config check/reload is implemented directly against this process's
    // startup-bound runner.toml path on every supported platform. Unix SIGHUP is
    // only an additional trigger and is not part of this capability contract.
    capabilities.runner_config_control = true;
    // Configured instruction files are observed only through the narrow Runner-owned snapshot boundary.
    capabilities.instruction_runtime = true;
    // MCP gateway support is fenced by the validated provider inventory in
    // registration rather than a separate capability bit. Older binaries omit
    // that inventory, so a newer Server will never target them.
    // `job_state_reconciliation` is on by default. A hidden, test/ops-only env
    // knob lets an E2E exercise the valid generation-2 no-reconciliation mode
    // (it then has no job inventory and a disconnect falls straight to `lost`).
    // Default production behavior is unchanged: only the explicit opt-out
    // disables it, and the server already rejects inventory without the
    // capability and vice-versa.
    // Browser capabilities are registration-required and depend on the actual
    // Runner-local Chromium-family discovery result. The Server must never infer
    // them from OS, protocol generation, shell, or Computer capabilities.
    let browser_available = webcodex_browser::discover_chromium_executable().is_some();
    capabilities.browser_observe = browser_available;
    capabilities.browser_control = browser_available;
    capabilities.browser_launch = browser_available;
    // Native read-only desktop observation is implemented only on macOS and
    // Windows. Unsupported platforms advertise false and fail closed.
    capabilities.computer_observe = cfg!(any(target_os = "macos", windows));
    // Installed-application discovery and exact launch are native macOS/Windows
    // additive capabilities. Neither is inferred from observation/control or
    // from each other.
    capabilities.computer_application_discovery = cfg!(any(target_os = "macos", windows));
    capabilities.computer_application_launch = cfg!(any(target_os = "macos", windows));
    // Exact full-display discovery/snapshot is independently implemented by
    // the native macOS and Windows backends; unsupported platforms fail closed.
    capabilities.computer_display_observe = cfg!(any(target_os = "macos", windows));
    // Snapshot-fenced exact coordinate pointer input is independently implemented by
    // the native macOS and Windows backends; unsupported platforms fail closed.
    capabilities.computer_pointer_control = cfg!(any(target_os = "macos", windows));
    // Bounded Unicode-text clipboard observation/replacement are separate
    // native capabilities on macOS and Windows.
    capabilities.computer_clipboard_read = cfg!(any(target_os = "macos", windows));
    capabilities.computer_clipboard_write = cfg!(any(target_os = "macos", windows));
    // Region/downscale snapshot requests use a distinct additive wire fence so
    // old Runners that support only whole-window snapshots fail closed.
    capabilities.computer_snapshot_region = cfg!(any(target_os = "macos", windows));
    // Accessibility inspection is a separate read-only semantic capability.
    // macOS AX and Windows UI Automation share the same model-facing tree;
    // observation authority never implies computer-control authority.
    capabilities.computer_accessibility_observe = cfg!(any(target_os = "macos", windows));
    // Normalized element-state observation is a separate rolling-upgrade wire
    // capability implemented by the same native read-only backends.
    capabilities.computer_element_state = cfg!(any(target_os = "macos", windows));
    // Accessibility control is independently fenced and implemented by the
    // native macOS AX and Windows UI Automation backends.
    capabilities.computer_control = cfg!(any(target_os = "macos", windows));
    // Semantic native scroll-to-visible is independently fenced for rolling upgrades;
    // existing computer_control support never implies it.
    capabilities.computer_scroll_to_element = cfg!(any(target_os = "macos", windows));
    // Closed key input is a separate effect/wire capability implemented by the
    // native macOS and Windows paths and is never implied by control.
    capabilities.computer_key_input = cfg!(any(target_os = "macos", windows));
    // Exact window activation is a separate effect/wire capability. It is
    // independently advertised by native macOS and Windows implementations.
    capabilities.computer_window_activate = cfg!(any(target_os = "macos", windows));
    // Bounded Accessibility text input is a separate rolling-upgrade fence;
    // older native Runners with computer_control must not be treated as capable.
    capabilities.computer_text_input = cfg!(any(target_os = "macos", windows));
    capabilities.job_state_reconciliation = !disable_job_state_reconciliation_for_test();

    // New agents always advertise read-only LSP navigation. Older agents omit
    // the field and deserialize as false on the server.
    capabilities.lsp_read_only_navigation = true;
    // Advertise the distinct capability only because this binary installs the
    // bounded typed prepare/incoming/outgoing traversal implementation.
    capabilities.lsp_call_hierarchy = true;
    capabilities
}

#[cfg(test)]
fn build_register_request(
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    prepared_cache_count: usize,
) -> RunnerRegisterRequest {
    let runtime = ReloadableRunnerConfig::new(cfg.clone(), PathBuf::new());
    build_register_request_with_provider_status(
        cfg,
        &runtime,
        runner_instance_id,
        prepared_cache_count,
        ShellJobInventory {
            active_complete: true,
            jobs: Vec::new(),
        },
    )
    .0
}

fn build_register_request_with_provider_status(
    cfg: &RunnerConfig,
    runtime: &ReloadableRunnerConfig,
    runner_instance_id: &str,
    prepared_cache_count: usize,
    job_inventory: ShellJobInventory,
) -> (
    RunnerRegisterRequest,
    Arc<webcodex_runner::external_tools::ExternalToolRouter>,
    u64,
) {
    let hot = runtime.snapshot();
    let mut capabilities = runner_register_capabilities(cfg);
    let coding_agent_providers = runtime
        .coding_agents()
        .map(|manager| manager.providers())
        .unwrap_or_default();
    let coding_agent_inventory = runtime.coding_agents().map(|manager| manager.inventory());
    capabilities.coding_agent_runs = !coding_agent_providers.is_empty();
    let (mut tool_providers, revision) = hot.external_tools.registration_status();
    tool_providers.config_reload = hot.reload_status();
    (
        RunnerRegisterRequest {
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
            runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
            display_name: cfg.display_name.clone(),
            owner: cfg.owner.clone(),
            hostname: cfg.hostname.clone().or_else(hostname),
            host_context: cfg.host_context.clone(),
            capabilities,
            policy: Some(register_policy_summary(
                &hot,
                prepared_cache_count,
                tool_providers,
                runtime.mcp_gateway().provider_inventory(),
            )),
            process_started_at: Some(process_started_at()),
            build: Some(runner_build_info()),
            job_concurrency_limit: Some(max_concurrent_jobs(cfg)),
            // A Runner with reconciliation disabled for E2E must not send a job
            // inventory: the server rejects inventory without the capability and
            // vice-versa.
            job_inventory: if disable_job_state_reconciliation_for_test() {
                None
            } else {
                Some(job_inventory)
            },
            coding_agent_providers: (!coding_agent_providers.is_empty())
                .then_some(coding_agent_providers),
            coding_agent_inventory,
        },
        Arc::clone(&hot.external_tools),
        revision,
    )
}

/// Unix timestamp when this runner process started. Captured on first call;
/// `run_runner` initializes it at startup so registration payloads report the
/// real process start, not the first register time after a reconnect.
fn process_started_at() -> i64 {
    static STARTED_AT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *STARTED_AT.get_or_init(|| chrono::Utc::now().timestamp())
}

/// Non-secret runner build identity for mixed-version diagnostics.
fn runner_build_info() -> runner_protocol::RunnerBuildInfo {
    let info = build_info::current();
    runner_protocol::RunnerBuildInfo {
        version: Some(info.version.to_string()),
        git_commit: info.git_commit.map(str::to_string),
        git_dirty: info.git_dirty,
        built_at: info.built_at.map(str::to_string),
        target: info.target.map(str::to_string),
        architecture: info.architecture.map(str::to_string),
    }
}

/// Shell dialect derived from a program path basename. Only `sh` and `bash`
/// map to portable POSIX dialects; `powershell`/`powershell.exe`/`pwsh` map to
/// the Windows PowerShell dialect; anything else is `custom` and callers that
/// need deterministic syntax must select an explicit `shell=sh|bash` (or a
/// configured dialect on the Runner side).
fn shell_dialect_for_program(program: &str) -> &'static str {
    match std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
    {
        "sh" => "sh",
        "bash" => "bash",
        "powershell" | "powershell.exe" | "pwsh" => "powershell",
        _ => "custom",
    }
}

/// Build the sanitized shell-profiles summary from the active shell config.
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
            let dialect = shell_dialect_for_program(&program);
            ShellProfileSummaryEntry {
                name: name.clone(),
                has_init_script: profile.init_script.is_some(),
                env_keys_count: profile.env.len(),
                program,
                args_count: args.len(),
                dialect: Some(dialect.to_string()),
            }
        })
        .collect();
    // Default execution path when the caller selects no explicit shell:
    // shell.default_profile if set, otherwise the plain shell program.
    // (A project-level shell_profile override is reported per project.)
    let default_program = shell
        .default_profile
        .as_deref()
        .and_then(|name| shell.profiles.get(name))
        .and_then(|profile| profile.program.clone())
        .unwrap_or_else(|| shell.program.clone());
    let default_dialect = shell_dialect_for_program(&default_program).to_string();
    // Report only semantic shells this exact Runner can resolve through its
    // effective execution PATH. This keeps model recovery guidance from
    // suggesting bash/sh merely because the protocol supports those selectors.
    let mut available: Vec<String> = Vec::new();
    for (name, language) in [
        ("sh", runner_protocol::ShellScriptLanguage::Sh),
        ("bash", runner_protocol::ShellScriptLanguage::Bash),
    ] {
        if webcodex_runner::explicit_shell_available(shell, language) {
            available.push(name.to_string());
        }
    }
    for entry in &profiles {
        if let Some(dialect) = entry.dialect.as_deref() {
            if !available.iter().any(|existing| existing == dialect) {
                available.push(dialect.to_string());
            }
        }
    }
    ShellProfilesSummary {
        default_profile: shell.default_profile.clone(),
        configured_count: shell.profiles.len(),
        prepared_cache_count,
        profiles,
        default_dialect: Some(default_dialect),
        available_dialects: Some(available),
    }
}

/// Build the sanitized Runner policy summary sent at registration. The wire
/// projection remains unchanged; it mirrors local `RunnerPolicy` but carries
/// only non-secret fields. The shell env
/// values and init_script path are intentionally NOT included. The sanitized
/// shell-profiles summary is attached so observability can show which profile
/// a project resolves to without exposing env values or init_script bodies.
fn register_policy_summary(
    cfg: &HotRunnerConfig,
    prepared_cache_count: usize,
    tool_providers: runner_protocol::ToolProvidersStatus,
    mcp_gateway_providers: Vec<crate::mcp_gateway::McpGatewayProvider>,
) -> RunnerPolicySummary {
    RunnerPolicySummary {
        allow_raw_shell: cfg.policy.allow_raw_shell,
        allow_cwd_anywhere: cfg.policy.allow_cwd_anywhere,
        allowed_roots: cfg.policy.allowed_roots.clone(),
        max_timeout_secs: cfg.policy.max_timeout_secs,
        max_output_bytes: cfg.policy.max_output_bytes,
        shell_profiles: Some(build_shell_profiles_summary(
            &cfg.shell,
            prepared_cache_count,
        )),
        tool_providers: Some(tool_providers),
        mcp_gateway_providers: Some(mcp_gateway_providers),
    }
}

fn register(
    client: &Client,
    cfg: &RunnerConfig,
    runtime: &ReloadableRunnerConfig,
    project_cache: &mut RunnerProjectCache,
    shutdown: Option<&AtomicBool>,
    runner_instance_id: &str,
    prepared_cache_count: usize,
    jobs: &JobManager,
) -> Result<
    (
        usize,
        ShellJobInventory,
        Vec<RunnerProjectSummary>,
        ShellProjectInventoryStatus,
    ),
    RegisterError,
> {
    let projects = project_cache.get_with_shutdown(cfg, shutdown);
    let projects_count = projects.iter().filter(|project| !project.disabled).count();
    let job_inventory = jobs.inventory();
    let (body, provider, provider_revision) = build_register_request_with_provider_status(
        cfg,
        runtime,
        runner_instance_id,
        prepared_cache_count,
        job_inventory.clone(),
    );
    let response: RunnerRegisterResponse = post_json(client, cfg, RUNNER_REGISTER_PATH, &body)
        .map_err(|error| RegisterError::from_http(error, &cfg.client_id))?;
    if response.success {
        provider.mark_status_reported(provider_revision);
        let inventory_status = response
            .client
            .as_ref()
            .and_then(|client| client.project_inventory.clone())
            .ok_or_else(|| RegisterError {
                kind: RegisterErrorKind::Protocol,
                message: "register response missing canonical project_inventory acknowledgement; Server is incompatible with this 0.4 Runner".to_string(),
            })?;
        Ok((projects_count, job_inventory, projects, inventory_status))
    } else {
        Err(RegisterError::from_response_error(
            &cfg.client_id,
            response.error,
        ))
    }
}

#[cfg(test)]
fn is_file_request_kind(kind: &str) -> bool {
    #[cfg(feature = "workspace-checkpoints")]
    if is_checkpoint_request_kind(kind) {
        return true;
    }
    is_basic_file_request_kind(kind)
        || is_structured_edit_request_kind(kind)
        || is_artifact_request_kind(kind)
}

fn handle_file_operation(policy: &RunnerPolicy, operation: &RunnerFileOperation) -> CommandResult {
    let request = operation.payload();
    let path = request.path.as_str();
    let start = Instant::now();
    if matches!(
        operation,
        RunnerFileOperation::WriteProjectFile(_)
            | RunnerFileOperation::ApplyTextEdits(_)
            | RunnerFileOperation::ApplyPatch(_)
    ) {
        if let Err(e) = validate_structured_edit_runner_path(path) {
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
    match operation {
        RunnerFileOperation::WriteProjectFile(_) => {
            handle_write_project_file_request(request, &resolved, start)
        }
        RunnerFileOperation::ApplyTextEdits(_) => {
            handle_apply_text_edits_file_request(policy, request, start)
        }
        RunnerFileOperation::ApplyPatch(_) => {
            handle_apply_patch_file_request(policy, request, start)
        }
        RunnerFileOperation::SaveProjectArtifact(_)
        | RunnerFileOperation::ReadProjectArtifactMetadata(_)
        | RunnerFileOperation::ReadProjectArtifact(_)
        | RunnerFileOperation::ReadProjectArtifactExportChunk(_)
        | RunnerFileOperation::ArtifactUploadBegin(_)
        | RunnerFileOperation::ArtifactUploadChunk(_)
        | RunnerFileOperation::ArtifactUploadFinish(_)
        | RunnerFileOperation::ArtifactUploadAbort(_) => {
            handle_artifact_file_operation(operation, &resolved, start)
        }
        #[cfg(feature = "workspace-checkpoints")]
        RunnerFileOperation::CheckpointCreate(_) | RunnerFileOperation::CheckpointRestore(_) => {
            handle_checkpoint_file_request(operation, &resolved, start)
        }
        #[cfg(not(feature = "workspace-checkpoints"))]
        RunnerFileOperation::CheckpointCreate(_) | RunnerFileOperation::CheckpointRestore(_) => {
            CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(
                    "workspace checkpoints are unsupported in this Runner build".to_string(),
                ),
            }
        }
        RunnerFileOperation::Read(_)
        | RunnerFileOperation::Write(_)
        | RunnerFileOperation::List(_)
        | RunnerFileOperation::ProjectOverview(_)
        | RunnerFileOperation::DeleteProjectFiles(_)
        | RunnerFileOperation::SkillListPackages(_)
        | RunnerFileOperation::SkillReadFile(_) => {
            handle_basic_file_request(policy, operation, &resolved, start)
        }
    }
}

#[cfg(test)]
fn handle_file_request(policy: &RunnerPolicy, request: &RunnerRequest) -> CommandResult {
    match request.decode_operation() {
        Ok(RunnerOperation::File(operation)) => handle_file_operation(policy, &operation),
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("invalid file request".to_string()),
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
    mut reader: R,
    tx: mpsc::SyncSender<OutputChunk>,
    stdout: bool,
    source: OutputTextSource,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // A bounded channel plus fixed-size reads prevents a fast child (or
        // one enormous line) from retaining unbounded output in the runner
        // while a transport send is slow.
        let mut buf = [0_u8; 8 * 1024];
        let mut decoder = OutputTextDecoder::new(source);
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
                Ok(read) => {
                    let text = decoder.push(&buf[..read], false);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                }
                Err(_) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
            }
        }
    })
}

fn drain_output_chunks(rx: &mpsc::Receiver<OutputChunk>, stdout: &mut String, stderr: &mut String) {
    while let Ok(chunk) = rx.try_recv() {
        match chunk {
            OutputChunk::Stdout(text) => stdout.push_str(&text),
            OutputChunk::Stderr(text) => stderr.push_str(&text),
        }
    }
}

/// Drain the bounded output channel while joining reader threads until
/// `deadline`. Draining and joining must progress together: a reader can be
/// blocked in `SyncSender::send` after the child exits, so waiting for the
/// reader before draining the channel creates a terminal-output race and can
/// drop the final validation summary. Returns the number of readers detached
/// after the existing bounded cleanup deadline.
fn drain_and_join_reader_threads_until(
    mut readers: Vec<std::thread::JoinHandle<()>>,
    rx: &mpsc::Receiver<OutputChunk>,
    stdout: &mut String,
    stderr: &mut String,
    deadline: Instant,
) -> usize {
    loop {
        drain_output_chunks(rx, stdout, stderr);
        let mut index = 0;
        while index < readers.len() {
            if readers[index].is_finished() {
                let reader = readers.swap_remove(index);
                let _ = reader.join();
            } else {
                index += 1;
            }
        }
        if readers.is_empty() {
            // A finished reader may have sent its final chunk just before the
            // join became observable.
            drain_output_chunks(rx, stdout, stderr);
            return 0;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            // Dropping a JoinHandle detaches it. The output channel is bounded,
            // so an abnormal pipe holder cannot retain unbounded runner memory
            // or block process shutdown.
            drain_output_chunks(rx, stdout, stderr);
            return readers.len();
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

fn wait_failure_error(validation: bool, error: &std::io::Error) -> String {
    if validation {
        VALIDATION_STEP_WAIT_FAILED_CODE.to_string()
    } else {
        format!("failed to wait job: {error}")
    }
}

fn validation_failed_step(status: &str, error: Option<&str>, step_name: &str) -> Option<String> {
    (status == "failed"
        && error
            .and_then(validation_infrastructure_failure_code)
            .is_none())
    .then(|| step_name.to_string())
}

fn validation_module_available(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    cwd: &Path,
    step: &ShellJobValidationStep,
    shutdown: Option<&AtomicBool>,
) -> bool {
    if step.program != "python" {
        return true;
    }
    let Some(module) = step
        .args
        .windows(2)
        .find(|p| p[0] == "-m")
        .map(|p| p[1].as_str())
    else {
        return false;
    };
    const PROBE: &str =
        "import importlib.util,sys;sys.exit(0 if importlib.util.find_spec(sys.argv[1]) else 42)";
    let args = ["-I", "-c", PROBE, module].map(str::to_string);
    let Ok(mut command) =
        configured_validation_job_command(shell, profile, &step.program, &args, cwd)
    else {
        return false;
    };
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(child) = ManagedChild::spawn(&mut command) else {
        return false;
    };
    let child = Arc::new(Mutex::new(child));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let wait_result = {
            let mut child = lock_unpoison(&child);
            child.try_wait()
        };
        match wait_result {
            Ok(Some(status)) => {
                let success = status.success();
                let _ = terminate_managed_tree(&child);
                return success;
            }
            Ok(None) => {
                if shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst))
                    || Instant::now() >= deadline
                {
                    let _ = terminate_managed_tree(&child);
                    return false;
                }
            }
            Err(_) => {
                let _ = terminate_managed_tree(&child);
                return false;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Forcefully terminate the entire process tree owned by a job.
///
/// The platform detail (SIGKILL to a private process group on Unix,
/// `TerminateJobObject` on Windows) stays inside `webcodex-process`.
fn terminate_managed_tree(child: &Arc<Mutex<ManagedChild>>) -> Result<(), String> {
    lock_unpoison(child)
        .terminate_tree()
        .map_err(|error| error.to_string())
}

/// Request graceful tree termination, escalating to force termination where the
/// platform cannot deliver a graceful signal to the whole tree (Windows Job
/// Objects). An already-exited tree is idempotent success.
fn request_terminate_managed_tree(child: &Arc<Mutex<ManagedChild>>) -> Result<(), String> {
    // Bind the result first so the temporary `MutexGuard` is released before
    // the match arms: the Unsupported arm re-locks the same mutex, and a guard
    // still alive across the match would deadlock that re-lock.
    let outcome = lock_unpoison(child).request_terminate_tree();
    match outcome {
        Ok(GracefulTermination::Requested | GracefulTermination::AlreadyExited) => Ok(()),
        Ok(GracefulTermination::Unsupported) => terminate_managed_tree(child),
        Err(error) => Err(error.to_string()),
    }
}

/// Wait, bounded by `deadline`, until the managed process tree is empty.
///
/// Returns `Ok(true)` when the tree exited within the budget, `Ok(false)` when
/// the deadline elapsed first, and `Err` on a platform failure. The lock is
/// held while polling, so a concurrent `terminate_tree` blocks only until this
/// bounded wait returns; nothing here waits on another thread's progress.
fn wait_managed_tree_exit(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Ok(false);
    }
    lock_unpoison(child)
        .wait_tree_exit(remaining)
        .map_err(|error| error.to_string())
}

/// Non-blocking probe: is the managed process tree still running?
///
/// Opportunistically reaps the direct child so a Unix zombie is not mistaken
/// for a live tree member. A busy lock or a platform probe failure is treated
/// conservatively as "still running".
fn managed_tree_running(child: &Arc<Mutex<ManagedChild>>) -> bool {
    let mut guard = match child.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return true,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    if guard.try_wait().is_err() {
        return true;
    }
    match guard.try_tree_exit() {
        Ok(true) => false,
        Ok(false) => true,
        Err(_) => true,
    }
}

/// Complete a job step's tree lifecycle after the direct child's status has
/// been decided: give the tree a short bounded window to exit on its own,
/// force-terminate whatever remains, and confirm the tree emptied, so
/// pipe-holding descendants cannot stall the output readers forever.
fn cleanup_managed_tree(child: &Arc<Mutex<ManagedChild>>) {
    const NATURAL_EXIT_GRACE: Duration = Duration::from_millis(500);
    const FORCE_EXIT_GRACE: Duration = Duration::from_millis(500);
    let natural_deadline = Instant::now() + NATURAL_EXIT_GRACE;
    if !wait_managed_tree_exit(child, natural_deadline).unwrap_or(false) {
        let _ = terminate_managed_tree(child);
        let force_deadline = Instant::now() + FORCE_EXIT_GRACE;
        let _ = wait_managed_tree_exit(child, force_deadline);
    }
}

/// Best-effort bounded reap of the direct child. Returns `Ok(true)` once the
/// direct child has been reaped, `Ok(false)` if the deadline elapsed first
/// (including when another thread reaped it concurrently), and `Err` on a wait
/// failure. The lock is only ever taken briefly with `try_lock`.
fn reap_managed_direct_child(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    loop {
        let mut guard = match child.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        match guard.try_wait() {
            Ok(Some(_)) => return Ok(true),
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

fn observe_cargo_test_count_chunks(
    accumulator: &mut Option<webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator>,
    stdout: &str,
    stderr: &str,
) {
    if let Some(accumulator) = accumulator.as_mut() {
        accumulator.push_stdout_chunk(stdout);
        accumulator.push_stderr_chunk(stderr);
    }
}

fn finish_cargo_test_count_evidence(
    accumulator: Option<webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator>,
) -> Option<ShellJobTestCountEvidence> {
    accumulator.map(|accumulator| {
        let metadata = accumulator.finish();
        ShellJobTestCountEvidence {
            tests_detected: metadata.tests_detected,
            tests_run_count: metadata.tests_run_count,
            status: metadata.count_evidence_status,
        }
    })
}

fn handle_one_poll(
    client: &Client,
    cfg: &RunnerConfig,
    runtime: &Arc<ReloadableRunnerConfig>,
    jobs: &JobManager,
    persistent_shells: &webcodex_runner::PersistentShellManager,
    project_cache: &mut RunnerProjectCache,
    project_inventory_page: Option<ShellProjectInventoryPage>,
    runner_instance_id: &str,
    lsp: &webcodex_runner::LspSupervisor,
    browser: &webcodex_browser::BrowserSupervisor,
    shutdown: &Arc<AtomicBool>,
    dispatches: &ActivityTracker,
    polling_dispatches: &mut PollingDispatchSupervisor,
    once: bool,
) -> Result<(bool, Option<ShellProjectInventoryStatus>), PollError> {
    let metadata_config = runtime.snapshot();
    let provider_update =
        metadata_config
            .external_tools
            .claim_status_update()
            .map(|(mut status, revision)| {
                status.config_reload = metadata_config.reload_status();
                (
                    status,
                    Arc::clone(&metadata_config.external_tools),
                    revision,
                )
            });
    let poll = RunnerPollPayload {
        request: RunnerPollRequest {
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
        },
        tool_providers: provider_update
            .as_ref()
            .map(|(status, _, _)| status.clone()),
        mcp_gateway_providers: provider_update
            .as_ref()
            .map(|_| runtime.mcp_gateway().provider_inventory()),
        project_inventory_page,
    };
    let response: RunnerPollResponse = match post_json(client, cfg, RUNNER_POLL_PATH, &poll) {
        Ok(response) => response,
        Err(error) => {
            if let Some((_, provider, revision)) = provider_update {
                provider.release_status_update(revision);
            }
            return Err(PollError::from_http(error, &cfg.client_id));
        }
    };
    if !response.success {
        if let Some((_, provider, revision)) = provider_update {
            provider.release_status_update(revision);
        }
        return Err(PollError::from_response_error(
            &cfg.client_id,
            response.error,
        ));
    }
    if let Some((_, provider, revision)) = provider_update {
        provider.mark_status_reported(revision);
    }
    let sink = RunnerSink::Http(HttpSendConfig {
        client: client.clone(),
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
        runner_instance_id: runner_instance_id.to_string(),
        shutdown: Arc::clone(shutdown),
    });
    jobs.install_sink(sink.clone());
    let inventory_status = response.project_inventory.clone();
    let Some(request) = response.request else {
        return Ok((false, inventory_status));
    };
    let hot = runtime.snapshot();
    let runtime = Arc::clone(runtime);
    let jobs = jobs.clone();
    let persistent_shells = persistent_shells.clone();
    let project_registry_dir = match project_registry_dir(cfg) {
        Ok(dir) => dir,
        Err(error) => return Err(PollError::new(PollErrorKind::Config, error)),
    };
    let lsp = lsp.clone();
    let browser = browser.clone();
    let dispatch = PollingDispatch {
        request_id: request.request_id.clone(),
        sink,
        config: hot,
        runtime,
        jobs,
        persistent_shells,
        project_registry_dir,
        lsp,
        browser,
        request,
    };
    if !once {
        polling_dispatches.spawn(dispatch)?;
        return Ok((true, inventory_status));
    }

    // `--once` deliberately retains its existing synchronous contract: the
    // one delivered ordinary request stays tracked until dispatch and result
    // submission finish, and the caller still drains any Job work afterward.
    let dispatch_guard = dispatches.enter();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("webcodex-poll-dispatch-once".to_string())
        .spawn(move || {
            let _dispatch_guard = dispatch_guard;
            let _ = result_tx.send(dispatch.run());
        })
        .map_err(|error| {
            PollError::new(
                PollErrorKind::Config,
                format!("failed to start polling dispatch worker: {error}"),
            )
        })?;
    polling_dispatches.background_threads.register(handle);
    let result = loop {
        match result_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::SeqCst) {
                    // A dispatch is already in flight: its result may itself be
                    // a terminal auth/protocol error that must surface rather
                    // than be masked as a clean shutdown. Give the in-flight
                    // submission a bounded window to deliver its result before
                    // falling back to the shutdown outcome. This avoids losing
                    // a fatal submit (e.g. a 401/403/404 the server already
                    // returned) when the shutdown flag flips during the HTTP
                    // round-trip.
                    if let Ok(result) = result_rx.recv_timeout(Duration::from_millis(500)) {
                        break result;
                    }
                    return Err(PollError::from_submit(SubmitResultError::Shutdown(
                        "process shutdown".to_string(),
                    )));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                    "polling dispatch worker closed".to_string(),
                )));
            }
        }
    };
    if let Ok(outcome) = &result {
        if outcome.project_cache_invalidation_required {
            project_cache.invalidate();
        }
    }
    let _ = polling_dispatches.background_threads.reap_finished();
    result
        .map(|outcome| (outcome.handled, inventory_status))
        .map_err(PollError::from_submit)
}

fn main() {
    if let Some(code) =
        webcodex_runner::detached_job::maybe_run_internal_mode(std::env::args().skip(1))
    {
        std::process::exit(code);
    }
    // Pin the process start timestamp before any transport work so register
    // payloads report real process identity even after reconnect loops.
    let _ = process_started_at();
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
    let (config_path, once, stop_on_stdin_eof) = match action {
        RunnerCliAction::Run {
            config_path,
            once,
            stop_on_stdin_eof,
        } => (config_path, once, stop_on_stdin_eof),
        RunnerCliAction::Exit {
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
    if config_path.file_name().and_then(|name| name.to_str())
        == Some(runner_config::paths::LEGACY_AGENT_CONFIG_FILE)
    {
        eprintln!(
            "webcodex-runner warning: legacy Runner config filename 'agent.toml' is deprecated; rename it to 'runner.toml' before WebCodex {}.",
            runner_config::paths::LEGACY_RUNNER_CONFIG_REMOVAL_VERSION
        );
    }
    let cfg = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    if cfg.token.trim().is_empty() {
        eprintln!(
            "webcodex-runner warning: agent token is empty; connecting without Authorization; the server must be started with --open"
        );
    }
    if let Err(e) = run_runner(cfg, config_path, once, stop_on_stdin_eof) {
        eprintln!("webcodex-runner failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
