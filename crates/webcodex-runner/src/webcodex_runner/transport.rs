use super::config::{
    max_concurrent_jobs, projects_dir, validate_quic_config, AgentConfig, HotAgentConfig,
    QuicClientConfig, ReloadableAgentConfig,
};
use super::lsp::LspSupervisor;
use super::projects::AgentProjectCache;
use super::shutdown::{
    ActivityTracker, BackgroundThreads, ShutdownCoordinator, ShutdownDeadline, ShutdownPhaseResult,
    ShutdownReport, BACKGROUND_JOIN_BUDGET, DEFAULT_SHUTDOWN_BUDGET, JOB_DRAIN_BUDGET,
    LSP_SHUTDOWN_BUDGET, PROVIDER_SHUTDOWN_BUDGET,
};
use super::util::contains_any;
use super::{PersistentShellManager, ShellCommandResult};
use crate::agent_init::{TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET};
use crate::shell_protocol::{
    read_quic_frame, write_quic_frame, AgentEnvelope, QuicFrameError, ShellAgentJobUpdateRequest,
    ShellAgentJobUpdateResponse, ShellAgentPersistentShellResultRequest,
    ShellAgentPersistentShellResultResponse, ShellAgentProjectSummary, ShellAgentResultPayload,
    ShellAgentResultRequest, ShellAgentResultResponse, ShellJobInventory,
    AGENT_PROTOCOL_VERSION_QUIC_V1, AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};
use crate::{
    build_register_request_with_provider_status, dispatch_request, handle_one_poll, register,
    AgentHttpError, AgentHttpErrorKind, CommandResult, JobManager, PollingRecoveryAction,
    RegisterRecoveryAction,
};
use reqwest::blocking::Client;
use std::fmt;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// WebSocket outgoing envelope channel capacity.
pub(crate) const WS_OUTGOING_CAPACITY: usize = 64;
/// WebSocket ping interval.
const WS_PING_INTERVAL: Duration = Duration::from_secs(30);
/// Bounded reconnect backoff after a transport disconnect or transient error.
const RECONNECT_BACKOFF_STEPS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];
/// Reset reconnect backoff after a connection stayed up long enough to prove
/// the endpoint is healthy. Immediate flapping still escalates.
const RECONNECT_STABLE_RESET_AFTER: Duration = Duration::from_secs(60);
/// Bounded wait for the writer task to flush its last frame and close the
/// sink during shutdown. A split WebSocket sink's `close()` waits for the
/// peer's close acknowledgement, which is delivered through the read half;
/// once the read loop has broken the read half is no longer polled, so
/// `close()` can hang indefinitely on a half-closed socket. Bounding it
/// guarantees `websocket_session` (and therefore the reconnect loop) always
/// makes progress instead of stalling forever after a disconnect.
const WS_WRITER_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
/// Process-shutdown control frames are best effort, but enqueueing them must
/// never wait behind a permanently full transport channel.
const TRANSPORT_CONTROL_SEND_TIMEOUT: Duration = Duration::from_millis(250);
/// Bounded wait for Tokio blocking tasks when the transport runtime exits.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
/// A blocking polling request must return early enough to leave useful time
/// for the process-wide cleanup budget.
const POLLING_HTTP_TIMEOUT: Duration = Duration::from_secs(5);
/// Reload listener polls its stop flag every 100ms, so one second is ample
/// while still preserving most of the global budget for child processes.
const CONFIG_RELOAD_JOIN_BUDGET: Duration = Duration::from_secs(1);
/// Granularity for signal-aware sleeps in the blocking polling loop.
const POLLING_SHUTDOWN_SLEEP_SLICE: Duration = Duration::from_millis(50);
/// Polling session recovery is persistent but capped: after reaching 10s,
/// repeated failures continue at 10s until recovery, shutdown, or a fatal
/// auth/protocol/config response.
const POLLING_RECOVERY_BACKOFF_STEPS: [Duration; 5] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];
/// The current server contract keeps an active polling instance online for
/// 60 seconds. Permit one lease window plus scheduling slack during deployment
/// replacement, but do not let a real duplicate runner retry forever.
const POLLING_LEASE_CONFLICT_MAX_WAIT: Duration = Duration::from_secs(75);
/// Result submission endpoint used by the polling transport sink.
const AGENT_RESULT_PATH: &str = "/api/shell/agent/result";
const AGENT_PERSISTENT_SHELL_RESULT_PATH: &str = "/api/shell/agent/persistent_shell_result";
/// Bounded same-payload retry backoff for transient result submission
/// failures over the polling transport. After the last step the payload is
/// released with an explicit dropped outcome, so a single result can never
/// monopolize the polling loop or trigger outer re-registration recovery.
const RESULT_SUBMIT_RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

fn send_provider_metadata(
    tx: &tokio::sync::mpsc::Sender<AgentEnvelope>,
    runtime: &ReloadableAgentConfig,
    expected_generation: Option<u64>,
) {
    runtime.with_active(|config| {
        if expected_generation.is_some_and(|expected| expected != config.generation) {
            return;
        }
        let Some((mut status, revision)) = config.external_tools.claim_status_update() else {
            return;
        };
        status.config_reload = config.reload_status();
        if tx
            .try_send(AgentEnvelope::RuntimeMetadata {
                tool_providers: status,
            })
            .is_err()
        {
            config.external_tools.release_status_update(revision);
        } else {
            config.external_tools.mark_status_reported(revision);
        }
    });
}

#[derive(Clone)]
pub(crate) struct AgentRuntimeState {
    lsp: LspSupervisor,
    config: Arc<ReloadableAgentConfig>,
    jobs: JobManager,
    persistent_shells: PersistentShellManager,
    coordinator: Arc<ShutdownCoordinator>,
    reload_threads: Arc<BackgroundThreads>,
    background_threads: Arc<BackgroundThreads>,
    dispatches: ActivityTracker,
}

impl AgentRuntimeState {
    pub(crate) fn new(cfg: &AgentConfig, path: PathBuf) -> Self {
        Self::with_shutdown_budget(cfg, path, DEFAULT_SHUTDOWN_BUDGET)
    }

    fn with_shutdown_budget(cfg: &AgentConfig, path: PathBuf, budget: Duration) -> Self {
        let jobs = JobManager::new(max_concurrent_jobs(cfg));
        // Persistent shells reuse the same authenticated OpenSSH multiplex pool
        // as async jobs: one transport per (session, resource, generation),
        // never a second SSH configuration or connection pool.
        let persistent_shells = PersistentShellManager::new(&cfg.shell, jobs.ssh_pool.clone());
        Self {
            lsp: LspSupervisor::default(),
            config: Arc::new(ReloadableAgentConfig::new(cfg.clone(), path)),
            jobs,
            persistent_shells,
            coordinator: Arc::new(ShutdownCoordinator::new(budget)),
            reload_threads: Arc::new(BackgroundThreads::default()),
            background_threads: Arc::new(BackgroundThreads::default()),
            dispatches: ActivityTracker::default(),
        }
    }

    fn request_shutdown_signal(&self) {
        self.coordinator.request_signal();
        let deadline = self.coordinator.deadline().instant();
        self.config.begin_shutdown();
        self.jobs.stop_accepting_work();
        self.persistent_shells.close_all("runner_shutdown");
        self.lsp.begin_shutdown_until(deadline);
    }

    fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.coordinator.requested_flag()
    }

    fn shutdown_requested(&self) -> bool {
        self.coordinator.is_requested()
    }

    fn project_summaries(
        &self,
        cache: &mut AgentProjectCache,
        cfg: &AgentConfig,
    ) -> Vec<ShellAgentProjectSummary> {
        let shutdown = self.shutdown_flag();
        cache.get_with_shutdown(cfg, Some(shutdown.as_ref()))
    }

    fn transport_runtime_shutdown_timeout(&self) -> Duration {
        if self.shutdown_requested() {
            RUNTIME_SHUTDOWN_TIMEOUT.min(
                self.coordinator
                    .deadline()
                    .instant()
                    .saturating_duration_since(Instant::now()),
            )
        } else {
            RUNTIME_SHUTDOWN_TIMEOUT
        }
    }

    async fn wait_for_shutdown(&self) {
        while !self.shutdown_requested() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn register_reload_thread(&self, handle: std::thread::JoinHandle<()>) {
        self.reload_threads.register(handle);
    }

    fn register_background_thread(&self, handle: std::thread::JoinHandle<()>) {
        self.background_threads.register(handle);
    }

    fn shutdown(&self) -> ShutdownReport {
        self.coordinator.run_once(|deadline| self.cleanup(deadline))
    }

    fn cleanup(&self, deadline: ShutdownDeadline) -> Vec<ShutdownPhaseResult> {
        let mut phases = Vec::with_capacity(10);

        let started = Instant::now();
        phases.push(if self.coordinator.signal_received() {
            ShutdownPhaseResult::completed("signal_received", started, 0)
        } else {
            ShutdownPhaseResult::skipped("signal_received", started)
        });

        let started = Instant::now();
        self.config.begin_shutdown();
        self.jobs.stop_accepting_work();
        self.persistent_shells.close_all("runner_shutdown");
        self.lsp.begin_shutdown_until(deadline.instant());
        phases.push(ShutdownPhaseResult::completed(
            "stop_accepting_work",
            started,
            0,
        ));

        let started = Instant::now();
        let reload_resources = self.reload_threads.pending();
        let reload = self
            .reload_threads
            .join_until(deadline.phase_deadline(CONFIG_RELOAD_JOIN_BUDGET));
        phases.push(shutdown_phase(
            "config_reload_stop",
            started,
            reload_resources,
            reload.timed_out,
            reload.panicked,
            "reload_thread_panicked",
        ));

        let started = Instant::now();
        let cancelled = self.jobs.cancel_queued_for_shutdown();
        phases.push(if cancelled == 0 {
            ShutdownPhaseResult::skipped("queued_jobs_cancel", started)
        } else {
            ShutdownPhaseResult::completed("queued_jobs_cancel", started, cancelled)
        });

        let started = Instant::now();
        let job_batch = self.jobs.signal_all_for_shutdown();
        let active_jobs = job_batch.running;
        let signal_failures = job_batch.failures;
        phases.push(shutdown_phase(
            "active_jobs_signal",
            started,
            active_jobs,
            0,
            signal_failures,
            "job_signal_failed",
        ));

        let started = Instant::now();
        let jobs = self
            .jobs
            .drain_shutdown(job_batch, deadline.phase_deadline(JOB_DRAIN_BUDGET));
        phases.push(shutdown_phase(
            "active_jobs_drain",
            started,
            jobs.resources,
            jobs.timed_out,
            jobs.failures.saturating_sub(signal_failures),
            "job_reap_failed",
        ));

        let started = Instant::now();
        let provider_deadline = deadline.phase_deadline(PROVIDER_SHUTDOWN_BUDGET);
        let mut provider_connections = 0usize;
        let mut provider_timeouts = 0usize;
        let mut provider_failures = 0usize;
        for router in self.config.external_routers() {
            let outcome = router.shutdown_until(provider_deadline);
            provider_connections = provider_connections.saturating_add(outcome.connections);
            provider_timeouts = provider_timeouts.saturating_add(outcome.timed_out);
            provider_failures = provider_failures.saturating_add(outcome.failures);
        }
        phases.push(shutdown_phase(
            "external_providers_stop",
            started,
            provider_connections,
            provider_timeouts,
            provider_failures,
            "provider_shutdown_failed",
        ));

        let started = Instant::now();
        let lsp = self
            .lsp
            .shutdown_until(deadline.phase_deadline(LSP_SHUTDOWN_BUDGET));
        phases.push(shutdown_phase(
            "lsp_servers_stop",
            started,
            lsp.servers,
            lsp.timed_out + usize::from(lsp.reaper_timed_out),
            lsp.failures,
            "lsp_shutdown_failed",
        ));

        let started = Instant::now();
        let background_deadline = deadline.phase_deadline(BACKGROUND_JOIN_BUDGET);
        let background_resources = self.reload_threads.pending()
            + self.background_threads.pending()
            + self.jobs.worker_count()
            + self.dispatches.active();
        let reload_retry = self.reload_threads.join_until(background_deadline);
        let joined = self.background_threads.join_until(background_deadline);
        let workers_done = self.jobs.wait_for_workers(background_deadline);
        let dispatches_done = self.dispatches.wait_until(background_deadline);
        let background_timeouts = reload_retry.timed_out
            + joined.timed_out
            + usize::from(!workers_done)
            + usize::from(!dispatches_done);
        phases.push(shutdown_phase(
            "background_threads_join",
            started,
            background_resources,
            background_timeouts,
            reload_retry.panicked + joined.panicked,
            "background_thread_panicked",
        ));

        phases.push(ShutdownPhaseResult::completed(
            "shutdown_complete",
            Instant::now(),
            0,
        ));
        phases
    }
}

fn shutdown_phase(
    phase: &'static str,
    started: Instant,
    resources: usize,
    timed_out: usize,
    failures: usize,
    failure_code: &'static str,
) -> ShutdownPhaseResult {
    if timed_out > 0 {
        ShutdownPhaseResult::timed_out(phase, started, resources)
    } else if failures > 0 {
        ShutdownPhaseResult::failed(phase, started, resources, failure_code)
    } else if resources == 0 {
        ShutdownPhaseResult::skipped(phase, started)
    } else {
        ShutdownPhaseResult::completed(phase, started, resources)
    }
}

fn sleep_or_shutdown(delay: Duration, shutdown: &AtomicBool) -> bool {
    let start = Instant::now();
    while start.elapsed() < delay {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        let remaining = delay.saturating_sub(start.elapsed());
        std::thread::sleep(remaining.min(POLLING_SHUTDOWN_SLEEP_SLICE));
    }
    shutdown.load(Ordering::SeqCst)
}

async fn async_sleep_or_shutdown(delay: Duration, runtime: &AgentRuntimeState) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        _ = runtime.wait_for_shutdown() => true,
    }
}

async fn future_or_shutdown<F>(future: F, runtime: &AgentRuntimeState) -> Option<F::Output>
where
    F: std::future::Future,
{
    tokio::select! {
        result = future => Some(result),
        _ = runtime.wait_for_shutdown() => None,
    }
}

fn install_shutdown_listener(
    runtime: AgentRuntimeState,
) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("webcodex-runner-shutdown".to_string())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                tokio::select! {
                    _ = shutdown_signal() => runtime.request_shutdown_signal(),
                    _ = runtime.wait_for_shutdown() => {}
                }
            });
        })
        .map_err(|_| "failed to start process shutdown signal listener".to_string())
}

fn complete_polling_shutdown(runtime: &AgentRuntimeState) -> Result<(), String> {
    runtime.request_shutdown_signal();
    runtime.shutdown();
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentTransportError {
    Transient(String),
    Fatal(String),
}

impl AgentTransportError {
    fn transient(message: impl Into<String>) -> Self {
        Self::Transient(message.into())
    }

    fn fatal(message: impl Into<String>) -> Self {
        Self::Fatal(message.into())
    }

    fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    fn into_message(self) -> String {
        match self {
            Self::Transient(message) | Self::Fatal(message) => message,
        }
    }
}

impl fmt::Display for AgentTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(message) | Self::Fatal(message) => f.write_str(message),
        }
    }
}

fn is_fatal_auth_or_register_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "register rejected",
            "register_failed",
            "register_forbidden",
            "unauthorized",
            "forbidden",
            "invalid token",
            "bad token",
            "auth failed",
            "authentication",
            "expected registered ack",
            "register ack was not text",
            "register ack is not a valid envelope",
        ],
    )
}

fn is_fatal_config_or_tls_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "invalid websocket url",
            "server_url must be",
            "transport=quic requires",
            "[quic]",
            "certificate",
            "webpki",
            "notvalidforname",
            "unknownissuer",
            "invalid server name",
            "invalid dns",
            "no application protocol",
            "alpn mismatch",
        ],
    )
}

fn classify_session_error(message: impl Into<String>) -> AgentTransportError {
    let message = message.into();
    if is_fatal_auth_or_register_error(&message) || is_fatal_config_or_tls_error(&message) {
        AgentTransportError::fatal(message)
    } else {
        AgentTransportError::transient(message)
    }
}

fn redact_url_queries(message: &str) -> String {
    let mut remaining = message;
    let mut redacted = String::with_capacity(message.len());
    loop {
        let http = remaining.find("http://");
        let https = remaining.find("https://");
        let url_start = match (http, https) {
            (Some(http), Some(https)) => http.min(https),
            (Some(http), None) => http,
            (None, Some(https)) => https,
            (None, None) => {
                redacted.push_str(remaining);
                break;
            }
        };
        redacted.push_str(&remaining[..url_start]);
        let url = &remaining[url_start..];
        let url_end = url.find(char::is_whitespace).unwrap_or(url.len());
        let segment = &url[..url_end];
        if let Some(query_start) = segment.find('?') {
            redacted.push_str(&segment[..query_start]);
            redacted.push_str("?[redacted]");
        } else {
            redacted.push_str(segment);
        }
        remaining = &url[url_end..];
    }
    redacted
}

fn concise_log_error(message: &str, token: &str) -> String {
    let mut sanitized = redact_url_queries(message).replace(['\r', '\n'], " ");
    let token = token.trim();
    if !token.is_empty() {
        sanitized = sanitized.replace(token, "[redacted]");
    }
    const MAX_CHARS: usize = 180;
    if sanitized.chars().count() > MAX_CHARS {
        let mut out = sanitized.chars().take(MAX_CHARS).collect::<String>();
        out.push_str("...");
        out
    } else {
        sanitized
    }
}

fn server_log_label(server_url: &str) -> String {
    match url::Url::parse(server_url) {
        Ok(parsed) => {
            let Some(host) = parsed.host_str() else {
                return parsed.scheme().to_string();
            };
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{}]", host)
            } else {
                host.to_string()
            };
            match parsed.port() {
                Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
                None => format!("{}://{}", parsed.scheme(), host),
            }
        }
        Err(_) => server_url
            .split('?')
            .next()
            .unwrap_or(server_url)
            .trim_end_matches('/')
            .to_string(),
    }
}

fn enabled_projects_count(projects: &[ShellAgentProjectSummary]) -> usize {
    projects.iter().filter(|project| !project.disabled).count()
}

fn registered_log_line(cfg: &AgentConfig, actual_transport: &str, projects_count: usize) -> String {
    format!(
        "webcodex-runner registered client_id={} server={} preferred_transport={} actual_transport={} projects={}",
        cfg.client_id,
        server_log_label(&cfg.server_url),
        effective_transport(cfg),
        actual_transport,
        projects_count
    )
}

fn auto_quic_not_configured_log_line() -> &'static str {
    "webcodex-runner transport auto: quic not configured; skipping"
}

fn auto_trying_log_line(transport: &str) -> String {
    format!("webcodex-runner transport auto: {} trying", transport)
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = async {
            if let Some(signal) = sigterm.as_mut() {
                let _ = signal.recv().await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(unix)]
fn install_reload_listener(
    runtime: Arc<ReloadableAgentConfig>,
) -> Result<std::thread::JoinHandle<()>, String> {
    let signal_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "failed to initialize config reload signal listener".to_string())?;
    let mut sighup = {
        let _guard = signal_runtime.enter();
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(|_| "failed to install config reload signal listener".to_string())?
    };
    std::thread::Builder::new()
        .name("webcodex-runner-reload".to_string())
        .spawn(move || {
            signal_runtime.block_on(async move {
                while !runtime.is_stopping() {
                    match tokio::time::timeout(Duration::from_millis(100), sighup.recv()).await {
                        Ok(Some(_)) => {
                            runtime.reload();
                        }
                        Ok(None) => break,
                        Err(_) => {}
                    }
                }
            });
        })
        .map_err(|_| "failed to start config reload signal listener".to_string())
}

/// Minimal HTTP send configuration used by the polling `AgentSink`. We do not
/// store the whole `AgentConfig` here: policy and concurrency limits stay
/// with the agent config and are passed alongside the sink.
#[derive(Debug, Clone)]
pub(crate) struct HttpSendConfig {
    pub(crate) client: Client,
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) client_id: String,
    pub(crate) agent_instance_id: String,
    pub(crate) shutdown: Arc<AtomicBool>,
}

/// Transport-neutral outgoing channel for an agent. Both the polling loop and
/// the WebSocket loop build an `AgentSink` and hand it to the shared
/// `dispatch_request` / `JobManager` execution path. This shared boundary lets
/// the agent speak either transport without duplicating execution logic.
#[derive(Debug, Clone)]
pub(crate) enum AgentSink {
    /// Polling transport: POST results/job_updates to the HTTP endpoints.
    Http(HttpSendConfig),
    /// WebSocket transport: push envelopes through an mpsc that a writer task
    /// drains onto the socket.
    WebSocket {
        tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
        client_id: String,
        agent_instance_id: String,
    },
    /// QUIC transport: push envelopes through an mpsc that a single writer
    /// task drains onto the bidirectional stream.
    Quic {
        tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
        client_id: String,
        agent_instance_id: String,
    },
}

/// Outcome of a result submission that no longer needs the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResultSubmission {
    /// The server accepted the result.
    Accepted,
    /// The server permanently rejected this exact payload (e.g. the request
    /// expired, was cancelled, or this instance lost the lease). The payload
    /// has been logged once (bounded, redacted) and released; the caller must
    /// keep polling instead of retrying it.
    RejectedPermanent,
    /// A transient HTTP failure persisted through every bounded retry. The
    /// payload has been logged once (bounded, redacted) and released so the
    /// polling runner remains live without retrying forever or entering the
    /// unrelated re-registration recovery path.
    DroppedAfterRetryExhaustion,
}

/// Structured result failures that require the current agent/session to stop.
/// Polling HTTP transients never reach this type: they are retried in place
/// and become `DroppedAfterRetryExhaustion` if the bounded budget is exhausted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubmitResultError {
    /// 401/403: credentials are wrong or revoked. Never retried.
    FatalAuth(String),
    /// 404: endpoint missing or incompatible server. Never retried.
    FatalProtocol(String),
    /// Invalid HTTP URL or non-recoverable TLS configuration. Never retried.
    FatalConfig(String),
    /// A WebSocket/QUIC outgoing channel closed before the result could be
    /// queued. This is a transport-session failure, not an HTTP retry outcome.
    TransportClosed(String),
    /// Process shutdown interrupted an HTTP retry backoff. The polling loop
    /// handles this as a clean shutdown rather than an operational failure.
    Shutdown(String),
}

impl fmt::Display for SubmitResultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FatalAuth(message)
            | Self::FatalProtocol(message)
            | Self::FatalConfig(message)
            | Self::TransportClosed(message)
            | Self::Shutdown(message) => f.write_str(message),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultHttpErrorDisposition {
    RetryTransient,
    RejectPermanent,
    FatalAuth,
    FatalProtocol,
    FatalConfig,
}

fn result_http_error_disposition(kind: &AgentHttpErrorKind) -> ResultHttpErrorDisposition {
    match kind {
        AgentHttpErrorKind::ServerUnavailable
        | AgentHttpErrorKind::Status
        | AgentHttpErrorKind::RequestTimeout
        | AgentHttpErrorKind::Request
        | AgentHttpErrorKind::DecodeTransient => ResultHttpErrorDisposition::RetryTransient,
        AgentHttpErrorKind::ClientRejected => ResultHttpErrorDisposition::RejectPermanent,
        AgentHttpErrorKind::Auth => ResultHttpErrorDisposition::FatalAuth,
        AgentHttpErrorKind::NotFound | AgentHttpErrorKind::ProtocolDecode => {
            ResultHttpErrorDisposition::FatalProtocol
        }
        AgentHttpErrorKind::Config => ResultHttpErrorDisposition::FatalConfig,
    }
}

/// One bounded, redacted diagnostic line for a permanently rejected result.
/// Emitted exactly once per payload: permanent rejections are never retried,
/// so this cannot repeat for the same result.
fn permanent_result_rejection_log_line(request_id: &str, error: &str, token: &str) -> String {
    format!(
        "webcodex-runner result permanently rejected request_id={} error={}; dropping this result and continuing to poll",
        concise_log_error(request_id, token),
        concise_log_error(error, token)
    )
}

/// One bounded, redacted warning after all transient submission attempts have
/// failed. It makes the possible result loss explicit without exposing raw
/// response bodies, credentials, or multiline request errors.
fn dropped_result_log_line(request_id: &str, attempts: usize, error: &str, token: &str) -> String {
    format!(
        "webcodex-runner result submission retries exhausted request_id={} attempts={} error={}; dropping this result and continuing to poll",
        concise_log_error(request_id, token),
        attempts,
        concise_log_error(error, token)
    )
}

/// Submit one result over the polling HTTP transport. Transient failures are
/// retried in place with bounded backoff; permanent rejections release the
/// payload after a single bounded log line; exhausted transient failures also
/// release it with an explicit dropped outcome; only auth/protocol failures
/// surface as errors that terminate the polling agent.
fn submit_result_http(
    h: &HttpSendConfig,
    body: &ShellAgentResultPayload,
) -> Result<ResultSubmission, SubmitResultError> {
    let mut attempt = 0usize;
    loop {
        let error = match post_json_raw::<_, ShellAgentResultResponse>(
            &h.client,
            &h.server_url,
            &h.token,
            AGENT_RESULT_PATH,
            body,
        ) {
            Ok(resp) if resp.success => return Ok(ResultSubmission::Accepted),
            Ok(resp) => {
                // A structured `success: false` answer is an explicit server
                // decision about this payload; resending it cannot succeed.
                let reason = resp
                    .error
                    .unwrap_or_else(|| "result submission failed without error".to_string());
                eprintln!(
                    "{}",
                    permanent_result_rejection_log_line(&body.result.request_id, &reason, &h.token,)
                );
                return Ok(ResultSubmission::RejectedPermanent);
            }
            Err(error) => error,
        };
        match result_http_error_disposition(&error.kind) {
            ResultHttpErrorDisposition::RejectPermanent => {
                eprintln!(
                    "{}",
                    permanent_result_rejection_log_line(
                        &body.result.request_id,
                        &error.to_string(),
                        &h.token
                    )
                );
                return Ok(ResultSubmission::RejectedPermanent);
            }
            ResultHttpErrorDisposition::FatalAuth => {
                return Err(SubmitResultError::FatalAuth(error.to_string()));
            }
            ResultHttpErrorDisposition::FatalProtocol => {
                return Err(SubmitResultError::FatalProtocol(error.to_string()));
            }
            ResultHttpErrorDisposition::FatalConfig => {
                return Err(SubmitResultError::FatalConfig(error.to_string()));
            }
            ResultHttpErrorDisposition::RetryTransient => {
                let Some(delay) = RESULT_SUBMIT_RETRY_BACKOFF.get(attempt).copied() else {
                    eprintln!(
                        "{}",
                        dropped_result_log_line(
                            &body.result.request_id,
                            attempt + 1,
                            &error.to_string(),
                            &h.token
                        )
                    );
                    return Ok(ResultSubmission::DroppedAfterRetryExhaustion);
                };
                attempt += 1;
                if sleep_or_shutdown(delay, h.shutdown.as_ref()) {
                    return Err(SubmitResultError::Shutdown(
                        "result submission retry interrupted by process shutdown".to_string(),
                    ));
                }
            }
        }
    }
}

impl AgentSink {
    pub(crate) fn client_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.client_id,
            AgentSink::WebSocket { client_id, .. } => client_id,
            AgentSink::Quic { client_id, .. } => client_id,
        }
    }

    /// Active agent process identity carried by this sink so every result /
    /// job_update submission includes it.
    pub(crate) fn agent_instance_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.agent_instance_id,
            AgentSink::WebSocket {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentSink::Quic {
                agent_instance_id, ..
            } => agent_instance_id,
        }
    }

    /// Submit the result of a synchronous shell/file request. Mirrors the old
    /// `submit_result` free function but routes over the active transport.
    pub(crate) fn submit_result(
        &self,
        request_id: String,
        result: CommandResult,
    ) -> Result<ResultSubmission, SubmitResultError> {
        self.submit_result_payload(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: self.client_id().to_string(),
                agent_instance_id: self.agent_instance_id().to_string(),
                request_id,
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                duration_ms: result.duration_ms,
                error: result.error,
            },
            command_execution_state: None,
        })
    }

    fn submit_result_payload(
        &self,
        body: ShellAgentResultPayload,
    ) -> Result<ResultSubmission, SubmitResultError> {
        match self {
            AgentSink::Http(h) => submit_result_http(h, &body),
            AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. } => {
                let env = AgentEnvelope::Result { payload: body };
                tx.blocking_send(env).map_err(|_| {
                    SubmitResultError::TransportClosed(
                        "agent transport result channel closed".to_string(),
                    )
                })?;
                Ok(ResultSubmission::Accepted)
            }
        }
    }

    pub(crate) fn submit_shell_result_with_metadata(
        &self,
        request_id: String,
        shell_result: ShellCommandResult,
        config: &HotAgentConfig,
        runtime: &ReloadableAgentConfig,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let ShellCommandResult {
            result,
            execution_state,
        } = shell_result;
        let body = ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: self.client_id().to_string(),
                agent_instance_id: self.agent_instance_id().to_string(),
                request_id,
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                duration_ms: result.duration_ms,
                error: result.error,
            },
            command_execution_state: Some(execution_state),
        };
        let submitted = self.submit_result_payload(body);
        if matches!(&submitted, Ok(ResultSubmission::Accepted)) {
            self.send_provider_metadata_best_effort(config.generation, runtime);
        }
        submitted
    }

    pub(crate) fn submit_result_with_metadata(
        &self,
        request_id: String,
        result: CommandResult,
        config: &HotAgentConfig,
        runtime: &ReloadableAgentConfig,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let submitted = self.submit_result(request_id, result);
        // Provider metadata is a best-effort follow-up on push transports, not
        // proof that a rejected or dropped result was accepted. Send it only
        // after the result reached the transport successfully.
        if matches!(&submitted, Ok(ResultSubmission::Accepted)) {
            self.send_provider_metadata_best_effort(config.generation, runtime);
        }
        submitted
    }

    /// Submit one Runner-authoritative persistent-shell lifecycle result. It
    /// has its own envelope and HTTP endpoint because PersistentShell is not a
    /// synchronous one-shot shell result and is never represented as a Job.
    pub(crate) fn submit_persistent_shell_result(
        &self,
        request_id: String,
        result: crate::shell_protocol::PersistentShellResult,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let body = ShellAgentPersistentShellResultRequest {
            client_id: self.client_id().to_string(),
            agent_instance_id: self.agent_instance_id().to_string(),
            request_id,
            result,
        };
        match self {
            AgentSink::Http(h) => submit_persistent_shell_result_http(h, &body),
            AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. } => {
                tx.blocking_send(AgentEnvelope::PersistentShellResult { payload: body })
                    .map_err(|_| {
                        SubmitResultError::TransportClosed(
                            "agent transport persistent shell result channel closed".to_string(),
                        )
                    })?;
                Ok(ResultSubmission::Accepted)
            }
        }
    }

    fn send_provider_metadata_best_effort(&self, generation: u64, runtime: &ReloadableAgentConfig) {
        let (AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. }) = self else {
            return;
        };
        send_provider_metadata(tx, runtime, Some(generation));
    }

    /// Push an incremental/final job update. Mirrors the old `send_job_update`
    /// free function. Job updates stay best-effort: callers ignore failures
    /// and the terminal state is still resolved by the final result path.
    pub(crate) fn send_job_update(&self, body: &ShellAgentJobUpdateRequest) -> Result<(), String> {
        match self {
            AgentSink::Http(h) => {
                let resp: ShellAgentJobUpdateResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/job_update",
                    body,
                )
                .map_err(|e| e.to_string())?;
                if resp.success {
                    Ok(())
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "job_update failed without error".to_string()))
                }
            }
            AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. } => {
                let env = AgentEnvelope::JobUpdate {
                    payload: body.clone(),
                };
                tx.blocking_send(env)
                    .map_err(|_| "agent transport send failed".to_string())
            }
        }
    }
}

fn submit_persistent_shell_result_http(
    h: &HttpSendConfig,
    body: &ShellAgentPersistentShellResultRequest,
) -> Result<ResultSubmission, SubmitResultError> {
    let mut attempt = 0usize;
    loop {
        let error = match post_json_raw::<_, ShellAgentPersistentShellResultResponse>(
            &h.client,
            &h.server_url,
            &h.token,
            AGENT_PERSISTENT_SHELL_RESULT_PATH,
            body,
        ) {
            Ok(response) if response.success => return Ok(ResultSubmission::Accepted),
            Ok(response) => {
                let reason = response.error.unwrap_or_else(|| {
                    "persistent shell result submission failed without error".to_string()
                });
                eprintln!(
                    "{}",
                    permanent_result_rejection_log_line(&body.request_id, &reason, &h.token)
                );
                return Ok(ResultSubmission::RejectedPermanent);
            }
            Err(error) => error,
        };
        match result_http_error_disposition(&error.kind) {
            ResultHttpErrorDisposition::RejectPermanent => {
                eprintln!(
                    "{}",
                    permanent_result_rejection_log_line(
                        &body.request_id,
                        &error.to_string(),
                        &h.token,
                    )
                );
                return Ok(ResultSubmission::RejectedPermanent);
            }
            ResultHttpErrorDisposition::FatalAuth => {
                return Err(SubmitResultError::FatalAuth(error.to_string()));
            }
            ResultHttpErrorDisposition::FatalProtocol => {
                return Err(SubmitResultError::FatalProtocol(error.to_string()));
            }
            ResultHttpErrorDisposition::FatalConfig => {
                return Err(SubmitResultError::FatalConfig(error.to_string()));
            }
            ResultHttpErrorDisposition::RetryTransient => {
                let Some(delay) = RESULT_SUBMIT_RETRY_BACKOFF.get(attempt).copied() else {
                    eprintln!(
                        "{}",
                        dropped_result_log_line(
                            &body.request_id,
                            attempt + 1,
                            &error.to_string(),
                            &h.token,
                        )
                    );
                    return Ok(ResultSubmission::DroppedAfterRetryExhaustion);
                };
                attempt += 1;
                if sleep_or_shutdown(delay, h.shutdown.as_ref()) {
                    return Err(SubmitResultError::Shutdown(
                        "persistent shell result retry interrupted by process shutdown".to_string(),
                    ));
                }
            }
        }
    }
}

/// Send a JSON POST to the server and decode the response. Same wire behavior
/// as `post_json` but takes the raw connection bits so it can be used from
/// `AgentSink::Http` without an `AgentConfig`. Preserves the structured
/// `AgentHttpError` classification for callers that must act on it.
fn post_json_raw<T, R>(
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
    crate::post_json_with_auth(client, server_url, token, path, body)
}

pub(crate) fn non_empty_token(token: &str) -> Option<String> {
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(crate) fn run_agent(cfg: AgentConfig, config_path: PathBuf, once: bool) -> Result<(), String> {
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
        .unwrap_or(TRANSPORT_WEBSOCKET)
        .to_string();
    // The LSP supervisor belongs to the agent process rather than any server
    // transport session and is shared across reconnects.
    let runtime = AgentRuntimeState::new(&cfg, config_path);
    let shutdown_listener = install_shutdown_listener(runtime.clone())?;
    runtime.register_background_thread(shutdown_listener);
    #[cfg(unix)]
    match install_reload_listener(Arc::clone(&runtime.config)) {
        Ok(reload_listener) => runtime.register_reload_thread(reload_listener),
        Err(error) => {
            runtime.shutdown();
            return Err(error);
        }
    }
    let result = match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_agent(cfg, once, &agent_instance_id, &runtime),
        TRANSPORT_QUIC => run_quic_agent(cfg, once, &agent_instance_id, &runtime),
        TRANSPORT_AUTO => run_auto_agent(cfg, once, &agent_instance_id, &runtime),
        _ => run_polling_agent(cfg, once, &agent_instance_id, &runtime),
    };
    runtime.shutdown();
    result
}

pub(crate) fn effective_transport(cfg: &AgentConfig) -> &str {
    cfg.transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
}

pub(crate) fn auto_transport_plan(cfg: &AgentConfig) -> Vec<&'static str> {
    let mut plan = Vec::new();
    if cfg.quic.is_some() {
        plan.push(TRANSPORT_QUIC);
    }
    plan.push(TRANSPORT_WEBSOCKET);
    plan.push(TRANSPORT_POLLING);
    plan
}

#[derive(Debug, Clone)]
struct RetryBackoff {
    attempts: usize,
    steps: &'static [Duration],
}

impl RetryBackoff {
    fn new(steps: &'static [Duration]) -> Self {
        Self { attempts: 0, steps }
    }

    fn reset(&mut self) {
        self.attempts = 0;
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self
            .steps
            .get(self.attempts)
            .copied()
            .unwrap_or_else(|| *self.steps.last().expect("retry backoff is non-empty"));
        self.attempts = self.attempts.saturating_add(1);
        delay
    }
}

fn next_lease_conflict_delay(backoff: &mut RetryBackoff, elapsed: Duration) -> Option<Duration> {
    if elapsed >= POLLING_LEASE_CONFLICT_MAX_WAIT {
        return None;
    }
    Some(
        backoff
            .next_delay()
            .min(POLLING_LEASE_CONFLICT_MAX_WAIT.saturating_sub(elapsed)),
    )
}

fn format_delay(delay: Duration) -> String {
    if delay.as_millis() % 1000 == 0 {
        format!("{}s", delay.as_secs())
    } else {
        format!("{}ms", delay.as_millis())
    }
}

fn schedule_reconnect(transport: &str, backoff: &mut RetryBackoff) -> Duration {
    let delay = backoff.next_delay();
    eprintln!(
        "webcodex-runner reconnect attempt scheduled transport={} delay={}",
        transport,
        format_delay(delay)
    );
    tracing::debug!(
        transport,
        delay_ms = delay.as_millis() as u64,
        "webcodex-runner reconnect attempt scheduled"
    );
    delay
}

fn reset_backoff_after_stable_session(backoff: &mut RetryBackoff, started_at: Instant) {
    if started_at.elapsed() >= RECONNECT_STABLE_RESET_AFTER {
        backoff.reset();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamTransport {
    WebSocket,
    Quic,
}

impl StreamTransport {
    fn name(self) -> &'static str {
        match self {
            Self::WebSocket => TRANSPORT_WEBSOCKET,
            Self::Quic => TRANSPORT_QUIC,
        }
    }

    fn ping_interval(self) -> Duration {
        match self {
            Self::WebSocket => WS_PING_INTERVAL,
            Self::Quic => QUIC_PING_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamSupervisorMode {
    Strict(StreamTransport),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamSupervisorExit {
    Completed,
    PollingFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSessionExit {
    Completed,
    TransportDisconnected,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StreamSessionDecision {
    Complete { shutdown: bool },
    Reconnect(Option<AgentTransportError>),
    TryNext(AgentTransportError),
    Fatal(String),
}

fn decide_stream_session(
    mode: StreamSupervisorMode,
    transport: StreamTransport,
    once: bool,
    result: Result<AgentSessionExit, String>,
) -> StreamSessionDecision {
    match result {
        Ok(AgentSessionExit::Shutdown) => StreamSessionDecision::Complete { shutdown: true },
        Ok(AgentSessionExit::Completed) => StreamSessionDecision::Complete { shutdown: false },
        Ok(AgentSessionExit::TransportDisconnected) if once => {
            StreamSessionDecision::Complete { shutdown: false }
        }
        Ok(AgentSessionExit::TransportDisconnected) => StreamSessionDecision::Reconnect(None),
        Err(error) => {
            let error = classify_session_error(error);
            if error.is_fatal()
                || matches!(mode, StreamSupervisorMode::Strict(_)) && once
                || mode == StreamSupervisorMode::Auto
                    && transport == StreamTransport::WebSocket
                    && once
            {
                StreamSessionDecision::Fatal(error.into_message())
            } else if matches!(mode, StreamSupervisorMode::Strict(_)) {
                StreamSessionDecision::Reconnect(Some(error))
            } else {
                StreamSessionDecision::TryNext(error)
            }
        }
    }
}

fn stream_transport_plan(cfg: &AgentConfig, mode: StreamSupervisorMode) -> Vec<StreamTransport> {
    match mode {
        StreamSupervisorMode::Strict(transport) => vec![transport],
        StreamSupervisorMode::Auto => auto_transport_plan(cfg)
            .into_iter()
            .filter_map(|transport| match transport {
                TRANSPORT_QUIC => Some(StreamTransport::Quic),
                TRANSPORT_WEBSOCKET => Some(StreamTransport::WebSocket),
                _ => None,
            })
            .collect(),
    }
}

async fn run_stream_session(
    transport: StreamTransport,
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    once: bool,
    runtime: &AgentRuntimeState,
) -> Result<AgentSessionExit, String> {
    match transport {
        StreamTransport::WebSocket => {
            websocket_session(cfg, projects, agent_instance_id, runtime).await
        }
        StreamTransport::Quic => {
            quic_session(cfg, projects, agent_instance_id, once, runtime).await
        }
    }
}

async fn supervise_stream_transports(
    cfg: &AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
    mode: StreamSupervisorMode,
) -> Result<StreamSupervisorExit, String> {
    let mut project_cache = AgentProjectCache::default();
    let mut backoff = RetryBackoff::new(&RECONNECT_BACKOFF_STEPS);
    'supervisor: loop {
        if mode == StreamSupervisorMode::Auto && cfg.quic.is_none() {
            eprintln!("{}", auto_quic_not_configured_log_line());
        }
        for transport in stream_transport_plan(cfg, mode) {
            if mode == StreamSupervisorMode::Auto {
                eprintln!("{}", auto_trying_log_line(transport.name()));
            }
            let projects = runtime.project_summaries(&mut project_cache, cfg);
            let session_started = Instant::now();
            let result =
                run_stream_session(transport, cfg, projects, agent_instance_id, once, runtime)
                    .await;
            project_cache.invalidate();
            match decide_stream_session(mode, transport, once, result) {
                StreamSessionDecision::Complete { shutdown } => {
                    if shutdown {
                        runtime.shutdown();
                    }
                    return Ok(StreamSupervisorExit::Completed);
                }
                StreamSessionDecision::Reconnect(error) => {
                    if let Some(error) = error {
                        eprintln!(
                            "webcodex-runner {} error: {}; reconnecting",
                            transport.name(),
                            error
                        );
                        tracing::debug!(
                            transport = transport.name(),
                            error = %error,
                            "webcodex-runner stream transport transient error"
                        );
                    } else {
                        reset_backoff_after_stable_session(&mut backoff, session_started);
                        eprintln!(
                            "webcodex-runner {} connection closed; reconnecting",
                            transport.name()
                        );
                    }
                    let delay = schedule_reconnect(transport.name(), &mut backoff);
                    if async_sleep_or_shutdown(delay, runtime).await {
                        runtime.shutdown();
                        return Ok(StreamSupervisorExit::Completed);
                    }
                    continue 'supervisor;
                }
                StreamSessionDecision::TryNext(error) => {
                    let log_error = concise_log_error(&error.to_string(), &cfg.token);
                    match transport {
                        StreamTransport::Quic => eprintln!(
                            "webcodex-runner transport auto: quic unavailable: {}; trying websocket",
                            log_error
                        ),
                        StreamTransport::WebSocket => eprintln!(
                            "webcodex-runner transport auto: websocket failed: {}; falling back to polling",
                            log_error
                        ),
                    }
                    tracing::debug!(
                        transport = transport.name(),
                        error = %log_error,
                        "webcodex-runner auto transport attempt failed"
                    );
                }
                StreamSessionDecision::Fatal(error) => return Err(error),
            }
        }
        debug_assert_eq!(mode, StreamSupervisorMode::Auto);
        eprintln!("{}", auto_trying_log_line(TRANSPORT_POLLING));
        return Ok(StreamSupervisorExit::PollingFallback);
    }
}

fn run_stream_transport_agent(
    cfg: &AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
    mode: StreamSupervisorMode,
) -> Result<StreamSupervisorExit, String> {
    let runtime_for_shutdown = runtime.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    let result = rt.block_on(supervise_stream_transports(
        cfg,
        once,
        agent_instance_id,
        runtime,
        mode,
    ));
    rt.shutdown_timeout(runtime_for_shutdown.transport_runtime_shutdown_timeout());
    result
}

fn run_auto_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
) -> Result<(), String> {
    match run_stream_transport_agent(
        &cfg,
        once,
        agent_instance_id,
        runtime,
        StreamSupervisorMode::Auto,
    )? {
        StreamSupervisorExit::Completed => Ok(()),
        StreamSupervisorExit::PollingFallback => {
            run_polling_agent(cfg, once, agent_instance_id, runtime)
        }
    }
}

fn run_polling_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
) -> Result<(), String> {
    let shutdown = runtime.shutdown_flag();
    run_polling_agent_with_shutdown(cfg, once, agent_instance_id, shutdown, runtime)
}

fn run_polling_agent_with_shutdown(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
    shutdown: Arc<AtomicBool>,
    runtime: &AgentRuntimeState,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(POLLING_HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = runtime.jobs.clone();
    let mut project_cache = AgentProjectCache::default();
    let mut registered = false;
    let mut recovering = false;
    let mut session_refreshed_during_recovery = false;
    let mut recovery_backoff = RetryBackoff::new(&POLLING_RECOVERY_BACKOFF_STEPS);
    let mut lease_conflict_started: Option<Instant> = None;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return complete_polling_shutdown(runtime);
        }
        if !registered {
            match register(
                &client,
                &cfg,
                &runtime.config,
                &mut project_cache,
                Some(shutdown.as_ref()),
                agent_instance_id,
                jobs.prepared_profiles.len(),
                &jobs,
            ) {
                Ok((projects_count, registered_jobs)) => {
                    registered = true;
                    lease_conflict_started = None;
                    recovery_backoff.reset();
                    let sink = AgentSink::Http(HttpSendConfig {
                        client: client.clone(),
                        server_url: cfg.server_url.clone(),
                        token: cfg.token.clone(),
                        client_id: cfg.client_id.clone(),
                        agent_instance_id: agent_instance_id.to_string(),
                        shutdown: Arc::clone(&shutdown),
                    });
                    jobs.install_sink(sink);
                    jobs.replay_snapshots_since(&registered_jobs);
                    if recovering {
                        eprintln!(
                            "webcodex-runner polling session refreshed during recovery client_id={}",
                            concise_log_error(&cfg.client_id, &cfg.token)
                        );
                        session_refreshed_during_recovery = true;
                    }
                    eprintln!(
                        "{}",
                        registered_log_line(&cfg, TRANSPORT_POLLING, projects_count)
                    );
                }
                Err(error) => match error.recovery_action() {
                    RegisterRecoveryAction::Fatal => return Err(error.into_message()),
                    RegisterRecoveryAction::Retry => {
                        recovering = true;
                        let delay = recovery_backoff.next_delay();
                        eprintln!(
                            "webcodex-runner transient register failure; retrying delay={} error={}",
                            format_delay(delay),
                            concise_log_error(&error.to_string(), &cfg.token)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_shutdown(runtime);
                        }
                    }
                    RegisterRecoveryAction::WaitForLease => {
                        recovering = true;
                        let started = lease_conflict_started.get_or_insert_with(Instant::now);
                        let elapsed = started.elapsed();
                        let Some(delay) = next_lease_conflict_delay(&mut recovery_backoff, elapsed)
                        else {
                            return Err(format!(
                                "active-instance lease conflict for client_id={} did not clear within {}",
                                concise_log_error(&cfg.client_id, &cfg.token),
                                format_delay(POLLING_LEASE_CONFLICT_MAX_WAIT)
                            ));
                        };
                        eprintln!(
                            "webcodex-runner active-instance lease conflict; waiting client_id={} delay={}",
                            concise_log_error(&cfg.client_id, &cfg.token),
                            format_delay(delay)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_shutdown(runtime);
                        }
                    }
                },
            }
            continue;
        }
        match handle_one_poll(
            &client,
            &cfg,
            &runtime.config,
            &jobs,
            &runtime.persistent_shells,
            &mut project_cache,
            agent_instance_id,
            &runtime.lsp,
            &shutdown,
            &runtime.dispatches,
            once,
        ) {
            Ok(ran_request) => {
                recovery_backoff.reset();
                lease_conflict_started = None;
                if recovering {
                    eprintln!(
                        "webcodex-runner polling recovery succeeded phase=poll client_id={}",
                        concise_log_error(&cfg.client_id, &cfg.token)
                    );
                    recovering = false;
                    session_refreshed_during_recovery = false;
                }
                if once {
                    while jobs.has_work() {
                        if sleep_or_shutdown(
                            Duration::from_millis(cfg.poll_interval_ms),
                            shutdown.as_ref(),
                        ) {
                            return complete_polling_shutdown(runtime);
                        }
                    }
                    return Ok(());
                }
                if !ran_request {
                    if sleep_or_shutdown(
                        Duration::from_millis(cfg.poll_interval_ms),
                        shutdown.as_ref(),
                    ) {
                        return complete_polling_shutdown(runtime);
                    }
                }
            }
            Err(e) => {
                // Polling has no durable transport lease to prove a Server
                // still knows these process handles. Fail closed at the first
                // recovery boundary instead of advertising false survival
                // after a Server restart or lost registration.
                runtime
                    .persistent_shells
                    .close_all("runner_transport_disconnected");
                match e.recovery_action() {
                    PollingRecoveryAction::Shutdown => {
                        return complete_polling_shutdown(runtime);
                    }
                    PollingRecoveryAction::Fatal => return Err(e.into_message()),
                    PollingRecoveryAction::RetryPoll => {
                        recovering = true;
                        // Refresh the same-instance registration once per
                        // recovery episode. This covers a server restart that
                        // lost in-memory session state without registering on
                        // every repeated 5xx response.
                        if !session_refreshed_during_recovery {
                            registered = false;
                        }
                        let delay = recovery_backoff.next_delay();
                        eprintln!(
                            "webcodex-runner transient poll failure; retrying delay={} error={}",
                            format_delay(delay),
                            concise_log_error(&e.to_string(), &cfg.token)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_shutdown(runtime);
                        }
                    }
                    PollingRecoveryAction::ReRegister => {
                        recovering = true;
                        registered = false;
                        session_refreshed_during_recovery = false;
                        let delay = recovery_backoff.next_delay();
                        eprintln!(
                            "webcodex-runner polling session lost; re-registering delay={} error={}",
                            format_delay(delay),
                            concise_log_error(&e.to_string(), &cfg.token)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_shutdown(runtime);
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Shared streaming transport lifecycle
// ============================================================================
//
// WebSocket and QUIC keep their own frame codecs and close mechanics. Register
// acknowledgement, dispatch, keepalive, disconnect, and shutdown policy live
// here once for both long-lived transports.

/// Interval between agent-initiated keepalive Pings.
const QUIC_PING_INTERVAL: Duration = Duration::from_secs(30);

type RunnerWebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

enum StreamRead {
    Envelope(AgentEnvelope),
    Closed,
}

enum RegisteredStream {
    WebSocket {
        reader: futures_util::stream::SplitStream<RunnerWebSocket>,
        writer: tokio::task::JoinHandle<()>,
    },
    Quic {
        reader: quinn::RecvStream,
        writer: tokio::task::JoinHandle<()>,
        connection: quinn::Connection,
        endpoint: quinn::Endpoint,
    },
}

impl RegisteredStream {
    async fn receive(&mut self) -> Result<StreamRead, String> {
        use futures_util::StreamExt;

        match self {
            Self::WebSocket { reader, .. } => loop {
                let message = match reader.next().await {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        tracing::debug!(
                            transport = "websocket",
                            error = ?error,
                            "webcodex-runner websocket read error"
                        );
                        return Ok(StreamRead::Closed);
                    }
                    None => {
                        tracing::debug!(
                            transport = "websocket",
                            "webcodex-runner websocket stream ended"
                        );
                        return Ok(StreamRead::Closed);
                    }
                };
                if let tokio_tungstenite::tungstenite::Message::Close(frame) = message {
                    if let Some(frame) = frame {
                        tracing::debug!(
                            transport = "websocket",
                            close_code = ?frame.code,
                            close_reason = %frame.reason,
                            "webcodex-runner websocket close frame received"
                        );
                    } else {
                        tracing::debug!(
                            transport = "websocket",
                            "webcodex-runner websocket close frame received"
                        );
                    }
                    return Ok(StreamRead::Closed);
                }
                let text = match message.into_text() {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                match AgentEnvelope::from_slice(text.as_bytes()) {
                    Ok(envelope) => return Ok(StreamRead::Envelope(envelope)),
                    Err(error) => {
                        eprintln!("webcodex-runner websocket malformed envelope: {}", error);
                    }
                }
            },
            Self::Quic { reader, .. } => match read_quic_frame(reader).await {
                Ok(envelope) => Ok(StreamRead::Envelope(envelope)),
                Err(QuicFrameError::EmptyStream) => {
                    tracing::debug!(
                        transport = "quic",
                        "webcodex-runner quic stream closed by peer"
                    );
                    Ok(StreamRead::Closed)
                }
                Err(error) => Err(format!("quic stream read error: {}", error)),
            },
        }
    }

    async fn finish(self, graceful: bool) {
        use futures_util::StreamExt;

        match self {
            Self::WebSocket {
                mut reader,
                mut writer,
            } => {
                if !graceful {
                    writer.abort();
                    return;
                }
                // Continue polling the read half while the writer flushes
                // Goodbye and the close frame. One absolute deadline bounds
                // both the writer and peer-close observation.
                let close_deadline = tokio::time::Instant::now() + WS_WRITER_CLOSE_TIMEOUT;
                let mut reader_open = true;
                let mut writer_finished = false;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(close_deadline) => {
                            writer.abort();
                            break;
                        }
                        _ = &mut writer => {
                            writer_finished = true;
                            break;
                        }
                        message = reader.next(), if reader_open => {
                            if !matches!(message, Some(Ok(_))) {
                                reader_open = false;
                            }
                        }
                    }
                }
                while writer_finished && reader_open {
                    tokio::select! {
                        _ = tokio::time::sleep_until(close_deadline) => break,
                        message = reader.next() => {
                            if !matches!(message, Some(Ok(message)) if !message.is_close()) {
                                reader_open = false;
                            }
                        }
                    }
                }
            }
            Self::Quic {
                mut writer,
                connection,
                endpoint,
                ..
            } => {
                if graceful {
                    connection.close(quinn::VarInt::from_u32(0), b"process shutdown");
                    endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
                    if tokio::time::timeout(WS_WRITER_CLOSE_TIMEOUT, &mut writer)
                        .await
                        .is_err()
                    {
                        writer.abort();
                    }
                } else {
                    writer.abort();
                }
            }
        }
    }
}

fn registered_ack(ack: AgentEnvelope) -> Result<(), String> {
    match ack {
        AgentEnvelope::Registered { success: true, .. } => Ok(()),
        AgentEnvelope::Registered { error, .. } => Err(format!(
            "register rejected by server: {}",
            error.unwrap_or_else(|| "no server error message".to_string())
        )),
        AgentEnvelope::Error { code, message } => Err(format!(
            "server error during register {}: {}",
            code, message
        )),
        other => Err(format!("expected registered ack, got {}", other.kind())),
    }
}

fn handle_stream_envelope(
    transport: StreamTransport,
    envelope: AgentEnvelope,
    cfg: &AgentConfig,
    sink: &AgentSink,
    out_tx: &tokio::sync::mpsc::Sender<AgentEnvelope>,
    runtime: &AgentRuntimeState,
) -> Option<String> {
    match envelope {
        AgentEnvelope::Request { request } => {
            let sink = sink.clone();
            let config = Arc::clone(&runtime.config);
            let hot = config.snapshot();
            let jobs = runtime.jobs.clone();
            let persistent_shells = runtime.persistent_shells.clone();
            let projects_dir = match projects_dir(cfg) {
                Ok(dir) => dir,
                Err(error) => return Some(error),
            };
            let lsp = runtime.lsp.clone();
            let dispatch_guard = runtime.dispatches.enter();
            tokio::task::spawn_blocking(move || {
                let _dispatch_guard = dispatch_guard;
                let _ = dispatch_request(
                    &sink,
                    &hot,
                    &config,
                    &jobs,
                    &persistent_shells,
                    &projects_dir,
                    &lsp,
                    request,
                );
            });
            None
        }
        AgentEnvelope::Ping { ts } => {
            let _ = out_tx.try_send(AgentEnvelope::Pong { ts });
            None
        }
        AgentEnvelope::Pong { .. } => None,
        AgentEnvelope::Registered { .. } if transport == StreamTransport::Quic => None,
        AgentEnvelope::Error { code, message } => {
            Some(format!("server error {}: {}", code, message))
        }
        other => {
            eprintln!(
                "webcodex-runner {} ignoring unexpected envelope: {}",
                transport.name(),
                other.kind()
            );
            None
        }
    }
}

async fn serve_registered_stream<F>(
    transport: StreamTransport,
    cfg: &AgentConfig,
    agent_instance_id: &str,
    registered_jobs: &ShellJobInventory,
    out_tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
    mut stream: RegisteredStream,
    runtime: &AgentRuntimeState,
    shutdown: F,
) -> Result<AgentSessionExit, String>
where
    F: std::future::Future<Output = ()>,
{
    let sink = match transport {
        StreamTransport::WebSocket => AgentSink::WebSocket {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            agent_instance_id: agent_instance_id.to_string(),
        },
        StreamTransport::Quic => AgentSink::Quic {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            agent_instance_id: agent_instance_id.to_string(),
        },
    };
    let jobs = runtime.jobs.clone();
    jobs.install_sink(sink.clone());
    jobs.replay_snapshots_since(registered_jobs);
    let mut ping_interval = tokio::time::interval(transport.ping_interval());
    ping_interval.tick().await;
    let mut shutdown = Box::pin(shutdown);
    let mut shutdown_requested = false;
    let mut session_error = None;

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                runtime.request_shutdown_signal();
                shutdown_requested = true;
                break;
            }
            read = stream.receive() => {
                match read {
                    Ok(StreamRead::Envelope(envelope)) => {
                        if let Some(error) =
                            handle_stream_envelope(transport, envelope, cfg, &sink, &out_tx, runtime)
                        {
                            session_error = Some(error);
                            break;
                        }
                    }
                    Ok(StreamRead::Closed) => break,
                    Err(error) => {
                        session_error = Some(error);
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                tracing::debug!(
                    transport = transport.name(),
                    "webcodex-runner stream keepalive ping"
                );
                send_provider_metadata(&out_tx, &runtime.config, None);
                let _ = out_tx.try_send(AgentEnvelope::Ping {
                    ts: chrono::Utc::now().timestamp(),
                });
            }
        }
    }

    if shutdown_requested {
        let _ = tokio::time::timeout(
            TRANSPORT_CONTROL_SEND_TIMEOUT,
            out_tx.send(AgentEnvelope::Goodbye {
                reason: Some("process shutdown".to_string()),
            }),
        )
        .await;
    } else if jobs.has_work() {
        tracing::warn!(
            transport = transport.name(),
            "webcodex-runner stream disconnected with active jobs; reconnecting without waiting"
        );
    }
    if !shutdown_requested && transport == StreamTransport::Quic {
        runtime
            .persistent_shells
            .close_all("runner_transport_disconnected");
    }
    drop(sink);
    drop(out_tx);
    stream.finish(shutdown_requested).await;
    if !shutdown_requested && transport == StreamTransport::WebSocket {
        runtime
            .persistent_shells
            .close_all("runner_transport_disconnected");
    }
    if let Some(error) = session_error {
        return Err(error);
    }
    Ok(if shutdown_requested {
        AgentSessionExit::Shutdown
    } else {
        AgentSessionExit::TransportDisconnected
    })
}

// The custom QUIC transport is a QUIC stream, not HTTP/3. It intentionally
// keeps one serialized bidirectional stream today so a future multistream
// implementation can change this adapter without changing the supervisor.
fn run_quic_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
) -> Result<(), String> {
    run_stream_transport_agent(
        &cfg,
        once,
        agent_instance_id,
        runtime,
        StreamSupervisorMode::Strict(StreamTransport::Quic),
    )
    .map(|_| ())
}

/// Validate the `[quic]` config section. Returns a cloned, resolved config so
/// the session owns a concrete value (defaults applied).
pub(crate) fn resolve_quic_config(cfg: &AgentConfig) -> Result<QuicClientConfig, String> {
    let quic = cfg
        .quic
        .clone()
        .ok_or_else(|| "transport=quic requires a [quic] section in agent.toml".to_string())?;
    validate_quic_config(&quic)?;
    Ok(quic)
}

pub(crate) fn resolve_quic_server_addrs(server_addr: &str) -> Result<Vec<SocketAddr>, String> {
    let addrs = server_addr
        .to_socket_addrs()
        .map_err(|e| {
            format!(
                "failed to resolve [quic] server_addr '{}': {}",
                server_addr, e
            )
        })?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!(
            "[quic] server_addr '{}' resolved to no socket addresses",
            server_addr
        ));
    }
    Ok(addrs)
}

pub(crate) fn quic_client_bind_addr_for(server_addr: SocketAddr) -> SocketAddr {
    if server_addr.is_ipv6() {
        "[::]:0"
            .parse()
            .expect("hard-coded IPv6 client bind address is valid")
    } else {
        "0.0.0.0:0"
            .parse()
            .expect("hard-coded IPv4 client bind address is valid")
    }
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

fn classify_quic_agent_connect_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("certificate")
        || lower.contains("cert")
        || lower.contains("webpki")
        || lower.contains("notvalidforname")
        || lower.contains("unknownissuer")
    {
        "certificate verify failed; check [quic].server_name and the certificate SAN/issuer"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connect timeout; check UDP firewall/security group/NAT and that the server QUIC listener is enabled"
    } else if lower.contains("alpn") || lower.contains("no application protocol") {
        "handshake failed; check WEBCODEX_QUIC_ENABLED, listener bind, and ALPN"
    } else if lower.contains("applicationclosed")
        || lower.contains("connectionclosed")
        || lower.contains("closed")
    {
        "handshake failed; check WEBCODEX_QUIC_ENABLED, listener bind, and server availability"
    } else {
        "handshake failed"
    }
}

/// One QUIC connection lifecycle: connect, register, dispatch requests until
/// the stream closes or a fatal server error arrives. In `--once` mode,
/// completes one ping/pong after the ack then returns.
async fn quic_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    once: bool,
    runtime: &AgentRuntimeState,
) -> Result<AgentSessionExit, String> {
    let quic = resolve_quic_config(cfg)?;
    let client_crypto = build_quic_client_crypto(&quic)?;
    let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let server_addrs = resolve_quic_server_addrs(&quic.server_addr)?;
    let mut connect_errors = Vec::new();
    let mut client_endpoint = None;
    let mut conn = None;
    for server_addr in server_addrs {
        if runtime.shutdown_requested() {
            return Ok(AgentSessionExit::Shutdown);
        }
        let endpoint = match quinn::Endpoint::client(quic_client_bind_addr_for(server_addr)) {
            Ok(endpoint) => endpoint,
            Err(e) => {
                connect_errors.push(format!(
                    "{}: failed to bind quic client endpoint: {}",
                    server_addr, e
                ));
                continue;
            }
        };
        let connect =
            match endpoint.connect_with(client_config.clone(), server_addr, &quic.server_name) {
                Ok(connect) => connect,
                Err(e) => {
                    connect_errors.push(format!(
                        "{}: failed to start quic connect: {}",
                        server_addr, e
                    ));
                    continue;
                }
            };
        let Some(connect_result) = future_or_shutdown(
            tokio::time::timeout(Duration::from_secs(quic.connect_timeout_secs), connect),
            runtime,
        )
        .await
        else {
            return Ok(AgentSessionExit::Shutdown);
        };
        match connect_result {
            Ok(Ok(connection)) => {
                client_endpoint = Some(endpoint);
                conn = Some(connection);
                break;
            }
            Err(_) => connect_errors.push(format!(
                "{} timed out after {}s; check UDP firewall/security group/NAT and that the server QUIC listener is enabled",
                server_addr, quic.connect_timeout_secs
            )),
            Ok(Err(e)) => {
                let raw = e.to_string();
                connect_errors.push(format!(
                    "{}: {} ({})",
                    server_addr,
                    classify_quic_agent_connect_error(&raw),
                    raw
                ));
            }
        }
    }
    let client_endpoint = client_endpoint.ok_or_else(|| {
        format!(
            "quic connect to {} failed for all resolved addresses: {}",
            quic.server_addr,
            connect_errors.join("; ")
        )
    })?;
    let conn = conn.expect("client endpoint is set only after a successful QUIC connection");

    // ALPN is enforced by quinn during the TLS handshake: a connection only
    // completes when the client and server agree on a matching ALPN. A
    // mismatch fails the handshake (surfaced as the connect error above).

    // Open a single bidirectional stream for register/ack/keepalive.
    let Some(open_result) = future_or_shutdown(conn.open_bi(), runtime).await else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(AgentSessionExit::Shutdown);
    };
    let (mut send, mut recv) =
        open_result.map_err(|e| format!("failed to open quic bidirectional stream: {}", e))?;

    // Register. The token is carried in `auth_token`; the server authenticates
    // it exactly like the websocket/polling paths. It is never logged.
    let projects_count = enabled_projects_count(&projects);
    let registered_jobs = runtime.jobs.inventory();
    let (register_payload, provider, provider_revision) =
        build_register_request_with_provider_status(
            cfg,
            &runtime.config,
            projects,
            AGENT_PROTOCOL_VERSION_QUIC_V1,
            agent_instance_id,
            0,
            registered_jobs.clone(),
        );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: non_empty_token(&cfg.token),
    };
    let Some(register_write) =
        future_or_shutdown(write_quic_frame(&mut send, &reg_env), runtime).await
    else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(AgentSessionExit::Shutdown);
    };
    register_write.map_err(|e| format!("failed to send quic register: {}", e))?;

    // Wait for the Registered ack.
    let Some(ack_result) = future_or_shutdown(
        tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv)),
        runtime,
    )
    .await
    else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(AgentSessionExit::Shutdown);
    };
    let ack = ack_result
        .map_err(|_| "quic register ack timed out".to_string())?
        .map_err(|e| format!("failed to read quic register ack: {}", e))?;
    registered_ack(ack)?;
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_QUIC, projects_count)
    );

    if once {
        // Complete one ping/pong round trip then exit, mirroring the websocket
        // `--once` semantics.
        let ping = AgentEnvelope::Ping {
            ts: chrono::Utc::now().timestamp(),
        };
        let Some(ping_write) =
            future_or_shutdown(write_quic_frame(&mut send, &ping), runtime).await
        else {
            conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
            client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            return Ok(AgentSessionExit::Shutdown);
        };
        ping_write.map_err(|e| format!("quic once ping send failed: {}", e))?;
        let Some(pong_result) = future_or_shutdown(
            tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv)),
            runtime,
        )
        .await
        else {
            conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
            client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            return Ok(AgentSessionExit::Shutdown);
        };
        let resp = pong_result
            .map_err(|_| "quic once pong timed out".to_string())?
            .map_err(|e| format!("quic once pong read failed: {}", e))?;
        match resp {
            AgentEnvelope::Pong { .. } => {}
            other => return Err(format!("expected pong, got {}", other.kind())),
        }
        let _ = write_quic_frame(
            &mut send,
            &AgentEnvelope::Goodbye {
                reason: Some("once complete".to_string()),
            },
        )
        .await;
        let _ = send.finish();
        return Ok(AgentSessionExit::Completed);
    }

    // Outgoing envelopes share one writer so future QUIC multistream work can
    // change the transport adapter without duplicating the session lifecycle.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            if write_quic_frame(&mut send, &env).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });
    serve_registered_stream(
        StreamTransport::Quic,
        cfg,
        agent_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::Quic {
            reader: recv,
            writer: writer_task,
            connection: conn,
            endpoint: client_endpoint,
        },
        runtime,
        runtime.wait_for_shutdown(),
    )
    .await
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
pub(crate) fn server_url_to_ws(server_url: &str, path: &str) -> Result<String, String> {
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
            server_log_label(server_url)
        ));
    };
    Ok(ws)
}

/// Build a WebSocket handshake request, carrying a Bearer token only when the
/// configured token is non-empty. Open-mode agents intentionally send no
/// credential so the server must have explicit anonymous mode enabled.
pub(crate) fn build_ws_request(
    ws_url: &str,
    token: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = ws_url.into_client_request().map_err(|e| {
        format!(
            "invalid websocket url for {}: {}",
            server_log_label(ws_url),
            e
        )
    })?;
    if let Some(token) = non_empty_token(token) {
        let value = format!("Bearer {}", token);
        let header_value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value)
            .map_err(|e| format!("invalid token header value: {}", e))?;
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
            header_value,
        );
    }
    Ok(request)
}

fn run_websocket_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
) -> Result<(), String> {
    run_stream_transport_agent(
        &cfg,
        once,
        agent_instance_id,
        runtime,
        StreamSupervisorMode::Strict(StreamTransport::WebSocket),
    )
    .map(|_| ())
}

/// One WebSocket connection lifecycle: connect, register, then serve requests
/// until the socket closes or a fatal server error arrives.
pub(crate) async fn websocket_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
) -> Result<AgentSessionExit, String> {
    websocket_session_with_shutdown(
        cfg,
        projects,
        agent_instance_id,
        runtime,
        runtime.wait_for_shutdown(),
    )
    .await
}

async fn websocket_session_with_shutdown<F>(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    runtime: &AgentRuntimeState,
    shutdown: F,
) -> Result<AgentSessionExit, String>
where
    F: std::future::Future<Output = ()>,
{
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let mut shutdown = Box::pin(shutdown);
    let ws_url = server_url_to_ws(&cfg.server_url, "/api/agents/ws")?;
    let request = build_ws_request(&ws_url, &cfg.token)?;
    let connect = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(cfg.websocket_connect_timeout_secs),
            tokio_tungstenite::connect_async(request),
        ) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(AgentSessionExit::Shutdown);
        }
    };
    let (mut ws_stream, _resp) = connect
        .map_err(|_| {
            format!(
                "websocket connect timed out after {}s",
                cfg.websocket_connect_timeout_secs
            )
        })?
        .map_err(|e| format!("websocket connect failed: {}", e))?;

    // Register over the socket. The prepared-profile cache is empty at
    // registration time (snapshots are prepared lazily on first use), so
    // `prepared_cache_count` is reported as 0 here.
    let projects_count = enabled_projects_count(&projects);
    let registered_jobs = runtime.jobs.inventory();
    let (register_payload, provider, provider_revision) =
        build_register_request_with_provider_status(
            cfg,
            &runtime.config,
            projects,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            agent_instance_id,
            0,
            registered_jobs.clone(),
        );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: None,
    };
    let reg_json =
        serde_json::to_string(&reg_env).map_err(|e| format!("failed to encode register: {}", e))?;
    tokio::select! {
        result = ws_stream.send(WsMessage::Text(reg_json.into())) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(AgentSessionExit::Shutdown);
        }
    }
    .map_err(|e| format!("failed to send register: {}", e))?;

    // Wait for Registered ack.
    let ack_msg = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(10), ws_stream.next()) => {
            result.map_err(|_| "websocket register ack timed out".to_string())?
        }
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(AgentSessionExit::Shutdown);
        }
    }
    .ok_or_else(|| "server closed before register ack".to_string())?
    .map_err(|e| format!("failed to read register ack: {}", e))?;
    let ack_text = ack_msg
        .into_text()
        .map_err(|_| "register ack was not text".to_string())?;
    let ack = AgentEnvelope::from_slice(ack_text.as_bytes())
        .map_err(|e| format!("register ack is not a valid envelope: {}", e))?;
    registered_ack(ack)?;
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_WEBSOCKET, projects_count)
    );

    // Split socket into writer (drains outgoing envelopes) and reader.
    let (mut sink, stream) = ws_stream.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    let writer_task = tokio::spawn(async move {
        let mut graceful_close = false;
        while let Some(env) = out_rx.recv().await {
            let is_goodbye = matches!(env, AgentEnvelope::Goodbye { .. });
            match serde_json::to_string(&env) {
                Ok(json) => {
                    if sink.send(WsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
            if is_goodbye {
                graceful_close = true;
                break;
            }
        }
        if graceful_close {
            // The session loop continues polling the split read half while
            // awaiting this task, allowing tungstenite's close handshake to
            // progress without turning this into an unbounded wait.
            let _ = sink.close().await;
        }
    });
    serve_registered_stream(
        StreamTransport::WebSocket,
        cfg,
        agent_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::WebSocket {
            reader: stream,
            writer: writer_task,
        },
        runtime,
        shutdown,
    )
    .await
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
