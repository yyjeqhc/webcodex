use super::config::{
    max_concurrent_jobs, project_registry_dir, validate_quic_config, QuicClientConfig,
    ReloadableRunnerConfig, RunnerConfig,
};
use super::contains_any;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use super::detached_job::DetachedJobStore;
#[cfg(windows)]
use super::exit_diagnostics::RunnerExitDiagnostics;
use super::job_manager::JobManager;
use super::lsp::LspSupervisor;
use super::projects::RunnerProjectCache;
use super::shutdown::{
    ActivityTracker, BackgroundThreads, ShutdownCoordinator, ShutdownDeadline, ShutdownPhaseResult,
    ShutdownReport, BACKGROUND_JOIN_BUDGET, BROWSER_SHUTDOWN_BUDGET, DEFAULT_SHUTDOWN_BUDGET,
    JOB_DRAIN_BUDGET, LSP_SHUTDOWN_BUDGET, PROVIDER_SHUTDOWN_BUDGET,
};
use super::PersistentShellManager;
use crate::runner_config::{
    TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET,
};
use crate::runner_protocol::{
    read_quic_frame, write_quic_frame, write_quic_register_frame, QuicFrameError,
    QuicRegisterFrame, RunnerEnvelope, RunnerOfflineRequest, RunnerProjectSummary,
    ShellJobInventory, ShellProjectInventoryStatus,
};
#[cfg(test)]
use crate::runner_protocol::{
    PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES, PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
};
use webcodex_browser::BrowserSupervisor;

mod project_inventory;
mod result_submission;
mod websocket_connect;

use crate::{
    build_register_request_with_provider_status, dispatch_request_with_outcome, handle_one_poll,
    register, PollingDispatchSupervisor, PollingRecoveryAction, RegisterRecoveryAction,
};
#[cfg(test)]
use crate::{CommandResult, RunnerHttpError, RunnerHttpErrorKind};
#[cfg(test)]
use project_inventory::{
    handle_project_inventory_status, ProjectInventoryStatusAction,
    POLLING_PROJECT_REFRESH_INTERVAL, PROJECT_INVENTORY_STAGING_RETRY_BACKOFF_STEPS,
};
use project_inventory::{
    log_project_inventory_degraded, paged_sync_after_registration, polling_projects_for_poll,
    try_queue_project_inventory_page, PollingProjectRefresh, ProjectInventorySync,
    StreamingProjectInventoryCoordinator,
};
use reqwest::blocking::Client;
#[cfg(test)]
use result_submission::{
    dropped_result_log_line, permanent_result_rejection_log_line, result_http_error_disposition,
    ResultHttpErrorDisposition, RESULT_SUBMIT_RETRY_BACKOFF, RUNNER_RESULT_PATH,
};
pub(crate) use result_submission::{
    HttpSendConfig, ResultSubmission, RunnerSink, SubmitResultError,
};
use std::fmt;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use websocket_connect::connect_websocket_request;
pub(crate) use websocket_connect::{build_ws_request, server_url_to_ws};
#[cfg(test)]
use websocket_connect::{
    connect_websocket_request_with_proxy, http_proxy_connect_tunnel, parse_http_proxy_endpoint,
    target_authority, websocket_proxy_from_env_with, websocket_target_endpoint, HttpProxyEndpoint,
    WS_PROXY_CONNECT_HEADER_MAX_BYTES,
};

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
/// Bounded wait for a streaming writer to flush its final control frame and
/// close its send side during graceful shutdown. WebSocket additionally polls
/// the read half for its close handshake; QUIC waits for the writer to finish
/// the SendStream before closing the connection. Neither path may hang process
/// shutdown on a non-responsive peer.
const STREAM_WRITER_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
/// Quinn's default peer idle timeout is 30 seconds. Keep this explicit on the
/// Runner side so the configured QUIC keepalive contract has a stable bound.
const QUIC_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Process-shutdown control frames are best effort, but enqueueing them must
/// never wait behind a permanently full transport channel.
const TRANSPORT_CONTROL_SEND_TIMEOUT: Duration = Duration::from_millis(250);
/// Bounded wait for Tokio blocking tasks when the transport runtime exits.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
/// A blocking polling request must return early enough to leave useful time
/// for the process-wide cleanup budget.
const POLLING_HTTP_TIMEOUT: Duration = Duration::from_secs(5);
/// Graceful polling shutdown is best effort and must not turn Ctrl-C into a
/// multi-second network wait when the network is already degraded.
const POLLING_OFFLINE_TIMEOUT: Duration = Duration::from_secs(1);
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
/// Empty successful polls back off independently from transient transport
/// recovery. The configured poll interval remains the minimum; the built-in
/// progression stops at 5 seconds so idle dispatch latency stays well below
/// the Server's 30-second default synchronous tool wait.
const POLLING_IDLE_BACKOFF_STEPS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

/// Older Servers may keep an active polling instance leased for 60 seconds.
/// Preserve a bounded compatibility wait when such a Server returns the legacy
/// exact lease-conflict error; current Servers use explicit registration takeover.
const POLLING_LEASE_CONFLICT_MAX_WAIT: Duration = Duration::from_secs(75);
const RUNNER_OFFLINE_PATH: &str = "/api/shell/agent/offline";
fn send_provider_metadata(
    transport: StreamTransport,
    tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    runtime: &ReloadableRunnerConfig,
    expected_generation: Option<u64>,
) {
    runtime.with_active(|config| {
        if expected_generation.is_some_and(|expected| expected != config.generation) {
            // An accepted result may belong to the generation that performed a
            // successful config reload. Treat that result only as a delivery
            // trigger: metadata is always read from the current active
            // generation below, so stale work can never publish stale routing.
            tracing::debug!(
                expected_generation,
                active_generation = config.generation,
                "publishing current Runner metadata after config generation changed"
            );
        }
        let Some((mut status, revision)) = config.external_tools.claim_status_update() else {
            return;
        };
        status.config_reload = config.reload_status();
        if try_send_runner_stream_control(
            transport,
            tx,
            RunnerEnvelope::RuntimeMetadata {
                tool_providers: status,
                mcp_gateway_providers: Some(runtime.mcp_gateway().provider_inventory()),
            },
        ) {
            config.external_tools.mark_status_reported(revision);
        } else {
            config.external_tools.release_status_update(revision);
        }
    });
}

#[derive(Clone)]
pub(crate) struct RunnerRuntimeState {
    lsp: LspSupervisor,
    browser: BrowserSupervisor,
    config: Arc<ReloadableRunnerConfig>,
    jobs: JobManager,
    persistent_shells: PersistentShellManager,
    coordinator: Arc<ShutdownCoordinator>,
    reload_threads: Arc<BackgroundThreads>,
    background_threads: Arc<BackgroundThreads>,
    dispatches: ActivityTracker,
    #[cfg(windows)]
    exit_diagnostics: Option<Arc<RunnerExitDiagnostics>>,
}

impl RunnerRuntimeState {
    pub(crate) fn new(cfg: &RunnerConfig, path: PathBuf) -> Self {
        Self::with_shutdown_budget(cfg, path, DEFAULT_SHUTDOWN_BUDGET)
    }

    fn with_shutdown_budget(cfg: &RunnerConfig, path: PathBuf, budget: Duration) -> Self {
        let jobs = JobManager::new(max_concurrent_jobs(cfg))
            .with_detached_profile_identity(&cfg.server_url);
        // Persistent shells reuse the same authenticated OpenSSH multiplex pool
        // as async jobs: one transport per (session, resource, generation),
        // never a second SSH configuration or connection pool.
        let persistent_shells = PersistentShellManager::new(&cfg.shell, jobs.ssh_pool().clone());
        Self {
            lsp: LspSupervisor::default(),
            browser: BrowserSupervisor::new(),
            config: Arc::new(ReloadableRunnerConfig::new(cfg.clone(), path)),
            jobs,
            persistent_shells,
            coordinator: Arc::new(ShutdownCoordinator::new(budget)),
            reload_threads: Arc::new(BackgroundThreads::default()),
            background_threads: Arc::new(BackgroundThreads::default()),
            dispatches: ActivityTracker::default(),
            #[cfg(windows)]
            exit_diagnostics: None,
        }
    }

    fn request_shutdown_signal(&self) {
        self.coordinator.request_signal();
        #[cfg(windows)]
        if let Some(diagnostics) = self.exit_diagnostics.as_ref() {
            // Persist signal provenance before any cleanup work. If the process
            // disappears while cleanup is in progress, the lifecycle record must
            // still distinguish an interrupted graceful shutdown from an exit
            // that never entered Rust shutdown handling.
            diagnostics.mark_shutdown_signal_received();
        }
        let deadline = self.coordinator.deadline().instant();
        self.config.begin_shutdown();
        self.jobs.stop_accepting_work();
        self.persistent_shells.close_all("runner_shutdown");
        self.browser.begin_shutdown();
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
        cache: &mut RunnerProjectCache,
        cfg: &RunnerConfig,
    ) -> Vec<RunnerProjectSummary> {
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
        self.coordinator.wait_requested().await;
    }

    #[cfg(any(unix, test))]
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
        let mut phases = Vec::with_capacity(11);

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
        self.browser.begin_shutdown();
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
        let active_jobs = job_batch.running();
        let signal_failures = job_batch.failures();
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
            jobs.resources(),
            jobs.timed_out(),
            jobs.failures().saturating_sub(signal_failures),
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
        let browser = self
            .browser
            .shutdown_until(deadline.phase_deadline(BROWSER_SHUTDOWN_BUDGET));
        phases.push(shutdown_phase(
            "browser_runtimes_stop",
            started,
            browser.browsers,
            browser.timed_out,
            browser.failures,
            "browser_shutdown_failed",
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
        let coding_agents = self
            .config
            .coding_agents()
            .map(|manager| manager.drain_workers_until(background_deadline))
            .unwrap_or_default();
        let background_timeouts = reload_retry.timed_out
            + joined.timed_out
            + usize::from(!workers_done)
            + usize::from(!dispatches_done)
            + coding_agents.timed_out;
        phases.push(shutdown_phase(
            "background_threads_join",
            started,
            background_resources + coding_agents.resources,
            background_timeouts,
            reload_retry.panicked + joined.panicked + coding_agents.panicked,
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

async fn async_sleep_or_shutdown(delay: Duration, runtime: &RunnerRuntimeState) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        _ = runtime.wait_for_shutdown() => true,
    }
}

async fn future_or_shutdown<F>(future: F, runtime: &RunnerRuntimeState) -> Option<F::Output>
where
    F: std::future::Future,
{
    tokio::select! {
        result = future => Some(result),
        _ = runtime.wait_for_shutdown() => None,
    }
}

fn install_shutdown_listener(
    runtime: RunnerRuntimeState,
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

#[cfg(windows)]
const PARENT_PIPE_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[cfg(windows)]
fn install_parent_liveness_listener(runtime: RunnerRuntimeState) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_PIPE};

    let stdin = std::io::stdin();
    let handle = stdin.as_raw_handle() as usize;
    if unsafe { GetFileType(handle as _) } == FILE_TYPE_PIPE {
        let listener = spawn_windows_pipe_parent_liveness_listener(handle, runtime)?;
        // The process owns stdin for its whole lifetime. The listener stops on
        // pipe EOF/error or an already-requested Runner shutdown, so detaching
        // the JoinHandle does not transfer ownership of any external resource.
        drop(listener);
        return Ok(());
    }

    install_blocking_parent_liveness_listener(runtime)
}

#[cfg(not(windows))]
fn install_parent_liveness_listener(runtime: RunnerRuntimeState) -> Result<(), String> {
    install_blocking_parent_liveness_listener(runtime)
}

fn install_blocking_parent_liveness_listener(runtime: RunnerRuntimeState) -> Result<(), String> {
    use std::io::Read;

    let listener = std::thread::Builder::new()
        .name("webcodex-runner-parent-lease".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 64];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => {
                        runtime.request_shutdown_signal();
                        return;
                    }
                    Ok(_) => {}
                }
            }
        })
        .map_err(|_| "failed to start parent-liveness listener".to_string())?;
    // Non-pipe stdin may require a genuinely blocking read. Keep the legacy
    // detached behavior for consoles and Unix streams; process exit reclaims
    // the listener while EOF still triggers exact-generation shutdown.
    drop(listener);
    Ok(())
}

#[cfg(windows)]
fn spawn_windows_pipe_parent_liveness_listener(
    pipe_handle: usize,
    runtime: RunnerRuntimeState,
) -> Result<std::thread::JoinHandle<()>, String> {
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    std::thread::Builder::new()
        .name("webcodex-runner-parent-lease".to_string())
        .spawn(move || {
            let pipe = pipe_handle as windows_sys::Win32::Foundation::HANDLE;
            let mut discard = [0_u8; 64];
            loop {
                if runtime.shutdown_requested() {
                    return;
                }

                let mut available = 0_u32;
                let peeked = unsafe {
                    PeekNamedPipe(
                        pipe,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if peeked == 0 {
                    // Preserve the historical lease contract: any stdin read
                    // failure is equivalent to parent EOF and requests Runner
                    // shutdown. Broken anonymous pipes land here without a
                    // blocking ReadFile on the Windows startup path.
                    runtime.request_shutdown_signal();
                    return;
                }

                if available == 0 {
                    std::thread::sleep(PARENT_PIPE_POLL_INTERVAL);
                    continue;
                }

                let to_read = available.min(discard.len() as u32);
                let mut bytes_read = 0_u32;
                let read = unsafe {
                    ReadFile(
                        pipe,
                        discard.as_mut_ptr(),
                        to_read,
                        &mut bytes_read,
                        std::ptr::null_mut(),
                    )
                };
                if read == 0 || bytes_read == 0 {
                    runtime.request_shutdown_signal();
                    return;
                }
            }
        })
        .map_err(|_| "failed to start parent-liveness listener".to_string())
}

fn send_polling_offline_best_effort(client: &Client, cfg: &RunnerConfig, runner_instance_id: &str) {
    let url = format!(
        "{}{}",
        cfg.server_url.trim_end_matches('/'),
        RUNNER_OFFLINE_PATH
    );
    let mut request = client.post(url).timeout(POLLING_OFFLINE_TIMEOUT);
    if !cfg.token.trim().is_empty() {
        request = request.bearer_auth(cfg.token.trim());
    }
    let body = RunnerOfflineRequest {
        client_id: cfg.client_id.clone(),
        runner_instance_id: runner_instance_id.to_string(),
    };
    match request.json(&body).send() {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => tracing::debug!(
            status = %response.status(),
            "webcodex-runner polling offline notice was not accepted"
        ),
        Err(error) => tracing::debug!(
            error = %concise_log_error(&error.to_string(), &cfg.token),
            "webcodex-runner polling offline notice failed"
        ),
    }
}

fn complete_polling_shutdown(
    client: &Client,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered: bool,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    runtime.request_shutdown_signal();
    runtime.shutdown();
    // Publish offline only after local workers have drained/stopped. Sending it
    // earlier would let a final polling result or Job update refresh last_seen
    // after the Server had already marked the Runner offline. A replacement
    // process never waits for this cleanup because registration takeover is
    // immediate and the delayed notice is instance-scoped.
    if registered {
        send_polling_offline_best_effort(client, cfg, runner_instance_id);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerTransportError {
    Transient(String),
    ProxyConfiguration(String),
    Fatal(String),
}

impl RunnerTransportError {
    fn transient(message: impl Into<String>) -> Self {
        Self::Transient(message.into())
    }

    fn proxy_configuration(message: impl Into<String>) -> Self {
        Self::ProxyConfiguration(message.into())
    }

    fn fatal(message: impl Into<String>) -> Self {
        Self::Fatal(message.into())
    }

    fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    fn is_proxy_configuration(&self) -> bool {
        matches!(self, Self::ProxyConfiguration(_))
    }

    fn into_message(self) -> String {
        match self {
            Self::Transient(message) | Self::ProxyConfiguration(message) | Self::Fatal(message) => {
                message
            }
        }
    }
}

impl fmt::Display for RunnerTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(message) | Self::ProxyConfiguration(message) | Self::Fatal(message) => {
                f.write_str(message)
            }
        }
    }
}

impl From<String> for RunnerTransportError {
    fn from(message: String) -> Self {
        classify_session_error(message)
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

fn classify_session_error(message: impl Into<String>) -> RunnerTransportError {
    let message = message.into();
    if is_fatal_auth_or_register_error(&message) || is_fatal_config_or_tls_error(&message) {
        RunnerTransportError::fatal(message)
    } else {
        RunnerTransportError::transient(message)
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

fn enabled_projects_count(projects: &[RunnerProjectSummary]) -> usize {
    projects.iter().filter(|project| !project.disabled).count()
}

fn registered_log_line(
    cfg: &RunnerConfig,
    actual_transport: &str,
    projects_count: usize,
) -> String {
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
pub(crate) fn install_reload_listener(
    runtime: Arc<ReloadableRunnerConfig>,
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

pub(crate) fn non_empty_token(token: &str) -> Option<String> {
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(crate) fn run_runner(
    cfg: RunnerConfig,
    config_path: PathBuf,
    once: bool,
    stop_on_stdin_eof: bool,
) -> Result<(), String> {
    // Generate the per-process agent instance identity once. It is stable for
    // the whole process lifetime, including across WebSocket reconnects, so the
    // server can treat this process as a single active lease for `client_id`.
    // It is not a secret and is never persisted to disk. Windows exit diagnostics
    // use a separate local diagnostic id and therefore preserve that boundary.
    let runner_instance_id = uuid::Uuid::new_v4().to_string();
    let transport = cfg
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
        .to_string();
    #[cfg(windows)]
    let exit_diagnostics = {
        let build = crate::runner_build_info();
        match RunnerExitDiagnostics::start(
            &cfg.client_id,
            &cfg.server_url,
            &transport,
            crate::process_started_at(),
            build.version.as_deref(),
            build.git_commit.as_deref(),
            build.git_dirty,
        ) {
            Ok(diagnostics) => {
                diagnostics.install_panic_hook();
                Some(diagnostics)
            }
            Err(error) => {
                tracing::warn!(error = %error, "Windows Runner exit diagnostics unavailable; Runner continues");
                None
            }
        }
    };
    // The LSP supervisor belongs to the Runner process rather than any server
    // transport session and is shared across reconnects.
    let runtime = RunnerRuntimeState::new(&cfg, config_path.clone());
    #[cfg(windows)]
    let runtime = {
        let mut runtime = runtime;
        runtime.exit_diagnostics = exit_diagnostics.clone();
        runtime
    };
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    match DetachedJobStore::default_root_for_runner(&cfg.client_id, &cfg.server_url) {
        Ok(root) => match runtime.jobs.recover_detached_jobs(
            DetachedJobStore::new(root),
            &cfg.client_id,
            &runner_instance_id,
        ) {
            Ok(count) if count > 0 => {
                tracing::info!(count, "recovered detached Jobs before Runner registration");
            }
            Ok(_) => {}
            Err(error) => {
                // Recovery is fail-closed for the detached records but must not
                // brick ordinary Runner service. Omitting an untrusted/corrupt
                // detached record lets normal Server reconciliation mark it lost.
                tracing::error!(error = %error, "detached Job restart recovery failed closed");
            }
        },
        Err(error) => {
            tracing::error!(error = %error, "detached Job state root is unavailable");
        }
    }
    if stop_on_stdin_eof {
        if let Err(error) = install_parent_liveness_listener(runtime.clone()) {
            #[cfg(windows)]
            if let Some(diagnostics) = exit_diagnostics.as_ref() {
                diagnostics.mark_terminal(false, "parent_liveness_listener_install_failed", None);
            }
            return Err(error);
        }
    }
    let shutdown_listener = match install_shutdown_listener(runtime.clone()) {
        Ok(listener) => listener,
        Err(error) => {
            #[cfg(windows)]
            if let Some(diagnostics) = exit_diagnostics.as_ref() {
                diagnostics.mark_terminal(false, "shutdown_listener_install_failed", None);
            }
            return Err(error);
        }
    };
    runtime.register_background_thread(shutdown_listener);
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_running();
    }
    #[cfg(unix)]
    match install_reload_listener(Arc::clone(&runtime.config)) {
        Ok(reload_listener) => runtime.register_reload_thread(reload_listener),
        Err(error) => {
            runtime.shutdown();
            return Err(error);
        }
    }
    let result = match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_runner(cfg, once, &runner_instance_id, &runtime),
        TRANSPORT_QUIC => run_quic_runner(cfg, once, &runner_instance_id, &runtime),
        TRANSPORT_AUTO => run_auto_runner(cfg, once, &runner_instance_id, &runtime),
        _ => run_polling_runner(cfg, once, &runner_instance_id, &runtime),
    };
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_transport_returned(result.is_ok());
    }
    let _shutdown = runtime.shutdown();
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_terminal(
            result.is_ok(),
            if result.is_ok() {
                "transport_completed"
            } else {
                "transport_returned_error"
            },
            Some(&_shutdown),
        );
    }
    result
}

pub(crate) fn effective_transport(cfg: &RunnerConfig) -> &str {
    cfg.transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
}

pub(crate) fn auto_transport_plan(cfg: &RunnerConfig) -> Vec<&'static str> {
    let mut plan = Vec::new();
    if cfg.quic.is_some() {
        plan.push(TRANSPORT_QUIC);
    }
    plan.push(TRANSPORT_WEBSOCKET);
    plan.push(TRANSPORT_POLLING);
    plan
}

#[derive(Debug, Clone)]
struct PollingIdleBackoff {
    initial: Duration,
    next_step: usize,
    first: bool,
}

impl PollingIdleBackoff {
    fn new(initial: Duration) -> Self {
        Self {
            initial,
            next_step: 0,
            first: true,
        }
    }

    fn reset(&mut self) {
        self.next_step = 0;
        self.first = true;
    }

    fn next_delay(&mut self) -> Duration {
        if self.first {
            self.first = false;
            return self.initial;
        }
        while let Some(step) = POLLING_IDLE_BACKOFF_STEPS.get(self.next_step).copied() {
            self.next_step += 1;
            if step > self.initial {
                return step;
            }
        }
        self.initial.max(
            *POLLING_IDLE_BACKOFF_STEPS
                .last()
                .expect("polling idle backoff is non-empty"),
        )
    }
}

fn polling_idle_delay(backoff: &mut PollingIdleBackoff, ran_request: bool) -> Option<Duration> {
    if ran_request {
        backoff.reset();
        None
    } else {
        Some(backoff.next_delay())
    }
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
    if delay.as_millis().is_multiple_of(1000) {
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
enum RunnerStreamMetricOutcome {
    Success,
    Closed,
    Backpressure,
    TransportError,
    Timeout,
}

impl RunnerStreamMetricOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Closed => "closed",
            Self::Backpressure => "backpressure",
            Self::TransportError => "transport_error",
            Self::Timeout => "timeout",
        }
    }
}

fn observe_runtime_metric_fail_open(observe: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(observe));
}

macro_rules! runtime_metric_info {
    ($($fields:tt)*) => {
        observe_runtime_metric_fail_open(|| tracing::info!($($fields)*))
    };
}

fn bounded_stream_envelope_kind(kind: &str) -> &'static str {
    match kind {
        "request" => "request",
        "result" => "result",
        "job_update" => "job_update",
        "persistent_shell_result" => "persistent_shell_result",
        "ping" => "ping",
        "pong" => "pong",
        "project_inventory_page" | "project_inventory_status" => "project_inventory",
        "runtime_metadata" => "provider_metadata",
        "goodbye" => "goodbye",
        _ => "control",
    }
}

fn successful_stream_duration(
    outcome: RunnerStreamMetricOutcome,
    duration: Option<Duration>,
) -> Option<Duration> {
    (outcome == RunnerStreamMetricOutcome::Success)
        .then_some(duration)
        .flatten()
}

fn observe_runner_stream_incoming_envelope(
    transport: StreamTransport,
    envelope_kind: &'static str,
) {
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    runtime_metric_info!(
        metric = "runner_stream_incoming_envelopes_total",
        value = 1_u64,
        transport = transport.name(),
        envelope_kind,
        "runtime_metric"
    );
}

fn observe_runner_stream_request_dispatch_wait(transport: StreamTransport, duration: Duration) {
    runtime_metric_info!(
        metric = "runner_stream_request_dispatch_wait_seconds",
        value = duration.as_secs_f64(),
        transport = transport.name(),
        "runtime_metric"
    );
}

fn observe_runner_request_duration(transport: &'static str, duration_ms: u64) {
    runtime_metric_info!(
        metric = "runner_request_duration_seconds",
        value = duration_ms as f64 / 1000.0,
        transport,
        "runtime_metric"
    );
}

fn observe_runner_stream_outgoing_channel(
    transport: StreamTransport,
    envelope_kind: &'static str,
    wait: Option<Duration>,
    backpressured: bool,
    outcome: RunnerStreamMetricOutcome,
) {
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    let successful_wait = successful_stream_duration(outcome, wait);
    runtime_metric_info!(
        metric = "runner_stream_outgoing_channel_events_total",
        value = 1_u64,
        transport = transport.name(),
        envelope_kind,
        outcome = outcome.as_str(),
        "runtime_metric"
    );
    if backpressured {
        runtime_metric_info!(
            metric = "runner_stream_outgoing_backpressure_total",
            value = 1_u64,
            transport = transport.name(),
            envelope_kind,
            "runtime_metric"
        );
    }
    if let Some(wait) = successful_wait {
        runtime_metric_info!(
            metric = "runner_stream_outgoing_channel_wait_seconds",
            value = wait.as_secs_f64(),
            transport = transport.name(),
            envelope_kind,
            "runtime_metric"
        );
    }
}
fn try_send_runner_stream_control(
    transport: StreamTransport,
    tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    envelope: RunnerEnvelope,
) -> bool {
    let envelope_kind = envelope.kind();
    match tx.try_send(envelope) {
        Ok(()) => {
            observe_runner_stream_outgoing_channel(
                transport,
                envelope_kind,
                None,
                false,
                RunnerStreamMetricOutcome::Success,
            );
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            observe_runner_stream_outgoing_channel(
                transport,
                envelope_kind,
                None,
                true,
                RunnerStreamMetricOutcome::Backpressure,
            );
            false
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            observe_runner_stream_outgoing_channel(
                transport,
                envelope_kind,
                None,
                false,
                RunnerStreamMetricOutcome::Closed,
            );
            false
        }
    }
}

fn observe_runner_stream_writer_send(
    transport: StreamTransport,
    envelope_kind: &'static str,
    duration: Option<Duration>,
    outcome: RunnerStreamMetricOutcome,
) {
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    let successful_duration = successful_stream_duration(outcome, duration);
    runtime_metric_info!(
        metric = "runner_stream_outgoing_envelopes_total",
        value = 1_u64,
        transport = transport.name(),
        envelope_kind,
        outcome = outcome.as_str(),
        "runtime_metric"
    );
    if let Some(duration) = successful_duration {
        runtime_metric_info!(
            metric = "runner_stream_writer_send_seconds",
            value = duration.as_secs_f64(),
            transport = transport.name(),
            envelope_kind,
            "runtime_metric"
        );
    }
}

fn observe_runner_stream_disconnect(transport: StreamTransport) {
    runtime_metric_info!(
        metric = "runner_stream_session_disconnects_total",
        value = 1_u64,
        transport = transport.name(),
        "runtime_metric"
    );
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
pub(crate) enum RunnerSessionExit {
    Completed,
    TransportDisconnected,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StreamSessionDecision {
    Complete { shutdown: bool },
    Reconnect(Option<RunnerTransportError>),
    TryNext(RunnerTransportError),
    Fatal(String),
}

fn decide_stream_session(
    mode: StreamSupervisorMode,
    transport: StreamTransport,
    once: bool,
    result: Result<RunnerSessionExit, RunnerTransportError>,
) -> StreamSessionDecision {
    match result {
        Ok(RunnerSessionExit::Shutdown) => StreamSessionDecision::Complete { shutdown: true },
        Ok(RunnerSessionExit::Completed) => StreamSessionDecision::Complete { shutdown: false },
        Ok(RunnerSessionExit::TransportDisconnected) if once => {
            StreamSessionDecision::Complete { shutdown: false }
        }
        Ok(RunnerSessionExit::TransportDisconnected) => StreamSessionDecision::Reconnect(None),
        Err(error) if error.is_proxy_configuration() => {
            if matches!(mode, StreamSupervisorMode::Strict(_)) {
                StreamSessionDecision::Fatal(error.into_message())
            } else {
                StreamSessionDecision::TryNext(error)
            }
        }
        Err(error) => {
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

fn stream_transport_plan(cfg: &RunnerConfig, mode: StreamSupervisorMode) -> Vec<StreamTransport> {
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
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    once: bool,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, RunnerTransportError> {
    match transport {
        StreamTransport::WebSocket => {
            websocket_session_classified(cfg, projects, runner_instance_id, runtime).await
        }
        StreamTransport::Quic => quic_session(cfg, projects, runner_instance_id, once, runtime)
            .await
            .map_err(classify_session_error),
    }
}

async fn supervise_stream_transports(
    cfg: &RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
    mode: StreamSupervisorMode,
) -> Result<StreamSupervisorExit, String> {
    let mut project_cache = RunnerProjectCache::default();
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
                run_stream_session(transport, cfg, projects, runner_instance_id, once, runtime)
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

fn run_stream_transport_runner(
    cfg: &RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
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
        runner_instance_id,
        runtime,
        mode,
    ));
    rt.shutdown_timeout(runtime_for_shutdown.transport_runtime_shutdown_timeout());
    result
}

fn run_auto_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    match run_stream_transport_runner(
        &cfg,
        once,
        runner_instance_id,
        runtime,
        StreamSupervisorMode::Auto,
    )? {
        StreamSupervisorExit::Completed => Ok(()),
        StreamSupervisorExit::PollingFallback => {
            run_polling_runner(cfg, once, runner_instance_id, runtime)
        }
    }
}

fn run_polling_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    let shutdown = runtime.shutdown_flag();
    run_polling_runner_with_shutdown(cfg, once, runner_instance_id, shutdown, runtime)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollFailureDirective {
    Continue,
    Shutdown,
}

#[allow(clippy::too_many_arguments)]
fn handle_poll_failure(
    error: crate::PollError,
    cfg: &RunnerConfig,
    runtime: &RunnerRuntimeState,
    shutdown: &AtomicBool,
    registered: &mut bool,
    recovering: &mut bool,
    session_refreshed_during_recovery: &mut bool,
    recovery_backoff: &mut RetryBackoff,
) -> Result<PollFailureDirective, String> {
    // Polling has no durable transport lease to prove a Server still knows
    // these process handles. Fail closed at the first recovery boundary
    // instead of advertising false survival after a Server restart or lost
    // registration.
    runtime
        .persistent_shells
        .close_all("runner_transport_disconnected");
    match error.recovery_action() {
        PollingRecoveryAction::Shutdown => Ok(PollFailureDirective::Shutdown),
        PollingRecoveryAction::Fatal => Err(error.into_message()),
        PollingRecoveryAction::RetryPoll => {
            *recovering = true;
            // Refresh the same-instance registration once per recovery
            // episode. This covers a server restart that lost in-memory
            // session state without registering on every repeated 5xx.
            if !*session_refreshed_during_recovery {
                *registered = false;
            }
            let delay = recovery_backoff.next_delay();
            eprintln!(
                "webcodex-runner transient poll failure; retrying delay={} error={}",
                format_delay(delay),
                concise_log_error(&error.to_string(), &cfg.token)
            );
            if sleep_or_shutdown(delay, shutdown) {
                Ok(PollFailureDirective::Shutdown)
            } else {
                Ok(PollFailureDirective::Continue)
            }
        }
        PollingRecoveryAction::ReRegister => {
            *recovering = true;
            *registered = false;
            *session_refreshed_during_recovery = false;
            let delay = recovery_backoff.next_delay();
            eprintln!(
                "webcodex-runner polling session lost; re-registering delay={} error={}",
                format_delay(delay),
                concise_log_error(&error.to_string(), &cfg.token)
            );
            if sleep_or_shutdown(delay, shutdown) {
                Ok(PollFailureDirective::Shutdown)
            } else {
                Ok(PollFailureDirective::Continue)
            }
        }
    }
}

fn complete_polling_after_shutdown(
    client: &Client,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered: bool,
    runtime: &RunnerRuntimeState,
    polling_dispatches: &mut PollingDispatchSupervisor,
    project_cache: &mut RunnerProjectCache,
) -> Result<(), String> {
    // Match the former synchronous dispatch race: a fatal submission response
    // that has already reached a worker gets a bounded chance to reach polling
    // control instead of being masked by a simultaneous clean shutdown.
    if let Err(error) =
        polling_dispatches.wait_for_shutdown_outcome(project_cache, Duration::from_millis(500))
    {
        if error.recovery_action() == PollingRecoveryAction::Fatal {
            return Err(error.into_message());
        }
    }
    complete_polling_shutdown(client, cfg, runner_instance_id, registered, runtime)
}

fn run_polling_runner_with_shutdown(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    shutdown: Arc<AtomicBool>,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(POLLING_HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = runtime.jobs.clone();
    let mut project_cache = RunnerProjectCache::default();
    let mut idle_backoff = PollingIdleBackoff::new(Duration::from_millis(cfg.poll_interval_ms));
    let mut project_refresh = PollingProjectRefresh::new(Instant::now());
    let mut polling_dispatches = PollingDispatchSupervisor::new(
        Arc::clone(&runtime.background_threads),
        runtime.dispatches.clone(),
    );
    let mut registered = false;
    let mut recovering = false;
    let mut session_refreshed_during_recovery = false;
    let mut recovery_backoff = RetryBackoff::new(&POLLING_RECOVERY_BACKOFF_STEPS);
    let mut lease_conflict_started: Option<Instant> = None;
    let mut project_inventory_sync: Option<ProjectInventorySync> = None;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return complete_polling_after_shutdown(
                &client,
                &cfg,
                runner_instance_id,
                registered,
                runtime,
                &mut polling_dispatches,
                &mut project_cache,
            );
        }
        if let Err(error) = polling_dispatches.drain_completed(&mut project_cache) {
            match handle_poll_failure(
                error,
                &cfg,
                runtime,
                shutdown.as_ref(),
                &mut registered,
                &mut recovering,
                &mut session_refreshed_during_recovery,
                &mut recovery_backoff,
            )? {
                PollFailureDirective::Continue => continue,
                PollFailureDirective::Shutdown => {
                    return complete_polling_after_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                        &mut polling_dispatches,
                        &mut project_cache,
                    );
                }
            }
        }
        if !registered {
            match register(
                &client,
                &cfg,
                &runtime.config,
                &mut project_cache,
                Some(shutdown.as_ref()),
                runner_instance_id,
                jobs.prepared_profiles().len(),
                &jobs,
            ) {
                Ok((projects_count, registered_jobs, registered_projects, _inventory_status)) => {
                    registered = true;
                    lease_conflict_started = None;
                    recovery_backoff.reset();
                    idle_backoff.reset();
                    project_refresh.mark_sent(Instant::now());
                    project_inventory_sync =
                        Some(paged_sync_after_registration(registered_projects));
                    let sink = RunnerSink::Http(HttpSendConfig {
                        client: client.clone(),
                        server_url: cfg.server_url.clone(),
                        token: cfg.token.clone(),
                        client_id: cfg.client_id.clone(),
                        runner_instance_id: runner_instance_id.to_string(),
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
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
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
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
                        }
                    }
                },
            }
            continue;
        }
        if !once {
            if let Err(error) = polling_dispatches
                .wait_for_capacity_or_shutdown(&mut project_cache, shutdown.as_ref())
            {
                match handle_poll_failure(
                    error,
                    &cfg,
                    runtime,
                    shutdown.as_ref(),
                    &mut registered,
                    &mut recovering,
                    &mut session_refreshed_during_recovery,
                    &mut recovery_backoff,
                )? {
                    PollFailureDirective::Continue => continue,
                    PollFailureDirective::Shutdown => {
                        return complete_polling_after_shutdown(
                            &client,
                            &cfg,
                            runner_instance_id,
                            registered,
                            runtime,
                            &mut polling_dispatches,
                            &mut project_cache,
                        );
                    }
                }
            }
        }
        if shutdown.load(Ordering::SeqCst) {
            return complete_polling_after_shutdown(
                &client,
                &cfg,
                runner_instance_id,
                registered,
                runtime,
                &mut polling_dispatches,
                &mut project_cache,
            );
        }
        let refresh_projects = if project_inventory_sync.is_none() {
            polling_projects_for_poll(
                &project_refresh,
                &mut project_cache,
                &cfg,
                shutdown.as_ref(),
                Instant::now(),
            )
        } else {
            None
        };
        if let Some(projects) = refresh_projects {
            project_inventory_sync = Some(ProjectInventorySync::new(projects));
        }

        let mut project_inventory_page = None;
        let mut inventory_page_build_failed = false;
        if let Some(sync) = project_inventory_sync.as_mut() {
            match sync.current_page() {
                Ok(page) => project_inventory_page = page,
                Err(reason_code) => {
                    log_project_inventory_degraded(
                        TRANSPORT_POLLING,
                        sync.total_reported(),
                        reason_code,
                    );
                    inventory_page_build_failed = true;
                }
            }
        }
        if inventory_page_build_failed {
            project_inventory_sync = None;
            project_refresh.mark_sent(Instant::now());
        }
        let sent_inventory_page = project_inventory_page.is_some();
        let mut poll_result = handle_one_poll(
            &client,
            &cfg,
            &runtime.config,
            &jobs,
            &runtime.persistent_shells,
            &mut project_cache,
            project_inventory_page,
            runner_instance_id,
            &runtime.lsp,
            &runtime.browser,
            &shutdown,
            &runtime.dispatches,
            &mut polling_dispatches,
            once,
        );
        // A background fatal result submission takes precedence over an
        // unrelated poll response from the same turn.
        if let Err(error) = polling_dispatches.drain_completed(&mut project_cache) {
            poll_result = Err(error);
        }
        match poll_result {
            Ok((ran_request, inventory_status)) => {
                if sent_inventory_page {
                    match inventory_status {
                        Some(status) => {
                            if let Some(sync) = project_inventory_sync.as_mut() {
                                match sync.acknowledge(&status) {
                                    Ok(done) => {
                                        if done {
                                            project_inventory_sync = None;
                                            project_refresh.mark_sent(Instant::now());
                                        }
                                    }
                                    Err(reason_code) => {
                                        log_project_inventory_degraded(
                                            TRANSPORT_POLLING,
                                            sync.total_reported(),
                                            &reason_code,
                                        );
                                        project_inventory_sync = None;
                                        project_refresh.mark_sent(Instant::now());
                                    }
                                }
                            }
                        }
                        None => {
                            return Err(
                                "poll response missing canonical project_inventory acknowledgement; Server is incompatible with this 0.4 Runner"
                                    .to_string(),
                            );
                        }
                    }
                }
                recovery_backoff.reset();
                lease_conflict_started = None;
                if recovering {
                    idle_backoff.reset();
                    eprintln!(
                        "webcodex-runner polling recovery succeeded phase=poll client_id={}",
                        concise_log_error(&cfg.client_id, &cfg.token)
                    );
                    recovering = false;
                    session_refreshed_during_recovery = false;
                }
                if once {
                    // `--once` still completes a negotiated multi-page startup
                    // inventory. Exiting after page 0 would make once-mode a
                    // cardinality-dependent partial-sync path.
                    if project_inventory_sync.is_some() {
                        continue;
                    }
                    while jobs.has_work() {
                        if sleep_or_shutdown(
                            Duration::from_millis(cfg.poll_interval_ms),
                            shutdown.as_ref(),
                        ) {
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
                        }
                    }
                    return complete_polling_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                    );
                }
                if let Some(delay) = polling_idle_delay(&mut idle_backoff, ran_request) {
                    if sleep_or_shutdown(delay, shutdown.as_ref()) {
                        return complete_polling_after_shutdown(
                            &client,
                            &cfg,
                            runner_instance_id,
                            registered,
                            runtime,
                            &mut polling_dispatches,
                            &mut project_cache,
                        );
                    }
                }
            }
            Err(error) => match handle_poll_failure(
                error,
                &cfg,
                runtime,
                shutdown.as_ref(),
                &mut registered,
                &mut recovering,
                &mut session_refreshed_during_recovery,
                &mut recovery_backoff,
            )? {
                PollFailureDirective::Continue => {}
                PollFailureDirective::Shutdown => {
                    return complete_polling_after_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                        &mut polling_dispatches,
                        &mut project_cache,
                    );
                }
            },
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
    Envelope(RunnerEnvelope),
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamWriterExit {
    ChannelClosed,
    GracefulClose,
    TransportFailed,
}

enum RegisteredStream {
    WebSocket {
        reader: futures_util::stream::SplitStream<RunnerWebSocket>,
    },
    Quic {
        reader: quinn::RecvStream,
        connection: quinn::Connection,
        endpoint: quinn::Endpoint,
    },
    #[cfg(test)]
    Test {
        reader: tokio::sync::mpsc::Receiver<StreamRead>,
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
                match RunnerEnvelope::from_slice(text.as_bytes()) {
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
            #[cfg(test)]
            Self::Test { reader } => Ok(reader.recv().await.unwrap_or(StreamRead::Closed)),
        }
    }

    async fn finish(
        self,
        graceful: bool,
        writer: Option<tokio::task::JoinHandle<StreamWriterExit>>,
    ) {
        use futures_util::StreamExt;

        match self {
            Self::WebSocket { mut reader } => {
                let Some(mut writer) = writer else {
                    return;
                };
                if !graceful {
                    writer.abort();
                    return;
                }
                // Continue polling the read half while the writer flushes
                // Goodbye and the close frame. One absolute deadline bounds
                // both the writer and peer-close observation.
                let close_deadline = tokio::time::Instant::now() + STREAM_WRITER_CLOSE_TIMEOUT;
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
                connection,
                endpoint,
                ..
            } => {
                // Graceful order is deliberate: serve_registered_stream has
                // already queued Goodbye and dropped all producers. Let the
                // writer drain it and finish the SendStream first. Quinn's
                // SendStream::finish only queues the FIN; Connection::close is
                // immediate and can discard buffered stream data. Use the same
                // absolute close budget to give the peer a chance to close the
                // connection after receiving Goodbye, then force-close if it
                // does not cooperate. Broken transports skip this grace wait.
                let close_started = tokio::time::Instant::now();
                let writer_graceful =
                    finish_quic_writer(writer, graceful, STREAM_WRITER_CLOSE_TIMEOUT).await;
                if graceful && writer_graceful {
                    let remaining =
                        STREAM_WRITER_CLOSE_TIMEOUT.saturating_sub(close_started.elapsed());
                    wait_for_quic_peer_close(
                        async {
                            let _ = connection.closed().await;
                        },
                        remaining,
                    )
                    .await;
                }
                connection.close(quinn::VarInt::from_u32(0), b"process shutdown");
                endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            }
            #[cfg(test)]
            Self::Test { .. } => {
                if let Some(mut writer) = writer {
                    if graceful {
                        let _ =
                            tokio::time::timeout(STREAM_WRITER_CLOSE_TIMEOUT, &mut writer).await;
                    } else {
                        writer.abort();
                    }
                }
            }
        }
    }
}

async fn finish_quic_writer(
    writer: Option<tokio::task::JoinHandle<StreamWriterExit>>,
    graceful: bool,
    timeout: Duration,
) -> bool {
    let Some(mut writer) = writer else {
        return false;
    };
    if !graceful {
        writer.abort();
        return false;
    }
    match tokio::time::timeout(timeout, &mut writer).await {
        Ok(Ok(StreamWriterExit::GracefulClose)) => true,
        Ok(_) => false,
        Err(_) => {
            writer.abort();
            false
        }
    }
}

async fn wait_for_quic_peer_close<F>(peer_closed: F, timeout: Duration)
where
    F: std::future::Future<Output = ()>,
{
    if timeout.is_zero() {
        return;
    }
    let _ = tokio::time::timeout(timeout, peer_closed).await;
}

fn registered_ack(ack: RunnerEnvelope) -> Result<ShellProjectInventoryStatus, String> {
    match ack {
        RunnerEnvelope::Registered {
            success: true,
            client,
            ..
        } => client
            .and_then(|client| client.project_inventory)
            .ok_or_else(|| "register acknowledgement missing canonical project_inventory status; Server is incompatible with this 0.4 Runner".to_string()),
        RunnerEnvelope::Registered { error, .. } => Err(format!(
            "register rejected by server: {}",
            error.unwrap_or_else(|| "no server error message".to_string())
        )),
        RunnerEnvelope::Error { code, message } => Err(format!(
            "server error during register {}: {}",
            code, message
        )),
        other => Err(format!("expected registered ack, got {}", other.kind())),
    }
}

fn handle_stream_envelope(
    transport: StreamTransport,
    envelope: RunnerEnvelope,
    cfg: &RunnerConfig,
    sink: &RunnerSink,
    out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    project_inventory: &mut StreamingProjectInventoryCoordinator,
    project_inventory_refresh_tx: &tokio::sync::mpsc::Sender<()>,
    runtime: &RunnerRuntimeState,
) -> Option<String> {
    let envelope_kind = envelope.kind();
    observe_runner_stream_incoming_envelope(transport, envelope_kind);
    match envelope {
        RunnerEnvelope::Request { request } => {
            let received_at = Instant::now();
            let sink = sink.clone();
            let config = Arc::clone(&runtime.config);
            let hot = config.snapshot();
            let jobs = runtime.jobs.clone();
            let persistent_shells = runtime.persistent_shells.clone();
            let project_registry_dir = match project_registry_dir(cfg) {
                Ok(dir) => dir,
                Err(error) => return Some(error),
            };
            let lsp = runtime.lsp.clone();
            let browser = runtime.browser.clone();
            let dispatch_guard = runtime.dispatches.enter();
            let project_inventory_refresh_tx = project_inventory_refresh_tx.clone();
            tokio::task::spawn_blocking(move || {
                let _dispatch_guard = dispatch_guard;
                observe_runner_stream_request_dispatch_wait(transport, received_at.elapsed());
                let dispatch_result = dispatch_request_with_outcome(
                    &sink,
                    &hot,
                    &config,
                    &jobs,
                    &persistent_shells,
                    &project_registry_dir,
                    &lsp,
                    &browser,
                    request,
                );
                if dispatch_result
                    .as_ref()
                    .is_ok_and(|outcome| outcome.project_cache_invalidation_required)
                {
                    // Capacity one deliberately coalesces multiple project
                    // mutations. A queued dirty signal already guarantees a
                    // fresh full observation; never block request completion on
                    // inventory synchronization.
                    let _ = project_inventory_refresh_tx.try_send(());
                }
            });
            None
        }
        RunnerEnvelope::Ping { ts } => {
            let _ = try_send_runner_stream_control(transport, out_tx, RunnerEnvelope::Pong { ts });
            None
        }
        RunnerEnvelope::Pong { .. } => None,
        RunnerEnvelope::ProjectInventoryStatus { status } => {
            project_inventory.handle_status(transport, status, cfg, runtime, out_tx);
            None
        }
        RunnerEnvelope::Registered { .. } if transport == StreamTransport::Quic => None,
        RunnerEnvelope::Error { code, message } => {
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
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered_jobs: &ShellJobInventory,
    out_tx: tokio::sync::mpsc::Sender<RunnerEnvelope>,
    mut stream: RegisteredStream,
    mut writer_task: tokio::task::JoinHandle<StreamWriterExit>,
    project_inventory_sync: Option<ProjectInventorySync>,
    runtime: &RunnerRuntimeState,
    shutdown: F,
) -> Result<RunnerSessionExit, String>
where
    F: std::future::Future<Output = ()>,
{
    let sink = match transport {
        StreamTransport::WebSocket => RunnerSink::WebSocket {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
        },
        StreamTransport::Quic => RunnerSink::Quic {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
        },
    };
    let jobs = runtime.jobs.clone();
    jobs.install_sink(sink.clone());
    jobs.replay_snapshots_since(registered_jobs);
    let mut ping_interval = tokio::time::interval(transport.ping_interval());
    ping_interval.tick().await;
    let mut project_inventory = StreamingProjectInventoryCoordinator::new(project_inventory_sync);
    let (project_inventory_refresh_tx, mut project_inventory_refresh_rx) =
        tokio::sync::mpsc::channel::<()>(1);
    let mut shutdown = Box::pin(shutdown);
    let mut shutdown_requested = false;
    let mut session_error = None;
    let mut writer_observed = false;

    loop {
        let project_inventory_retry_at = project_inventory.retry_at();
        let project_inventory_retry_deadline =
            project_inventory_retry_at.unwrap_or_else(tokio::time::Instant::now);
        tokio::select! {
            _ = tokio::time::sleep_until(project_inventory_retry_deadline), if project_inventory_retry_at.is_some() => {
                project_inventory.retry_pending_now(transport, &out_tx);
            }
            refresh = project_inventory_refresh_rx.recv() => {
                if refresh.is_some() {
                    project_inventory.refresh_from_current_projects(
                        transport,
                        cfg,
                        runtime,
                        &out_tx,
                        "project_inventory_local_project_mutation",
                    );
                }
            }
            _ = &mut shutdown => {
                runtime.request_shutdown_signal();
                shutdown_requested = true;
                break;
            }
            writer = &mut writer_task => {
                writer_observed = true;
                let reason_code = match writer {
                    Ok(StreamWriterExit::ChannelClosed) => "writer_channel_closed",
                    Ok(StreamWriterExit::GracefulClose) => "writer_graceful_close_unexpected",
                    Ok(StreamWriterExit::TransportFailed) => "writer_transport_failed",
                    Err(error) if error.is_panic() => "writer_task_panicked",
                    Err(_) => "writer_task_cancelled",
                };
                tracing::debug!(
                    transport = transport.name(),
                    reason_code,
                    "webcodex-runner stream writer ended; terminating session"
                );
                break;
            }
            read = stream.receive() => {
                match read {
                    Ok(StreamRead::Envelope(envelope)) => {
                        if let Some(error) = handle_stream_envelope(
                            transport,
                            envelope,
                            cfg,
                            &sink,
                            &out_tx,
                            &mut project_inventory,
                            &project_inventory_refresh_tx,
                            runtime,
                        ) {
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
                send_provider_metadata(transport, &out_tx, &runtime.config, None);
                // An acknowledgement can be dropped if the Server's outbound
                // channel is saturated. Re-sending the exact pending page is
                // idempotent and gives the sync a bounded periodic recovery path.
                // When the Server explicitly reported staging pressure, the
                // dedicated backoff timer owns retry timing so keepalive cannot
                // collapse that bounded delay into an eager resend.
                if project_inventory.retry_at().is_none() {
                    project_inventory.queue_pending(transport, &out_tx);
                }
                let _ = try_send_runner_stream_control(
                    transport,
                    &out_tx,
                    RunnerEnvelope::Ping {
                        ts: chrono::Utc::now().timestamp(),
                    },
                );
            }
        }
    }

    if shutdown_requested {
        let queue_started = Instant::now();
        match tokio::time::timeout(
            TRANSPORT_CONTROL_SEND_TIMEOUT,
            out_tx.send(RunnerEnvelope::Goodbye {
                reason: Some("process shutdown".to_string()),
            }),
        )
        .await
        {
            Ok(Ok(())) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                Some(queue_started.elapsed()),
                false,
                RunnerStreamMetricOutcome::Success,
            ),
            Ok(Err(_)) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                None,
                false,
                RunnerStreamMetricOutcome::Closed,
            ),
            Err(_) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                None,
                false,
                RunnerStreamMetricOutcome::Timeout,
            ),
        }
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
    let writer = (!writer_observed).then_some(writer_task);
    stream.finish(shutdown_requested, writer).await;
    if !shutdown_requested && transport == StreamTransport::WebSocket {
        runtime
            .persistent_shells
            .close_all("runner_transport_disconnected");
    }
    if !shutdown_requested {
        observe_runner_stream_disconnect(transport);
    }
    if let Some(error) = session_error {
        return Err(error);
    }
    Ok(if shutdown_requested {
        RunnerSessionExit::Shutdown
    } else {
        RunnerSessionExit::TransportDisconnected
    })
}

// The custom QUIC transport is a QUIC stream, not HTTP/3. It intentionally
// keeps one serialized bidirectional stream today so a future multistream
// implementation can change this adapter without changing the supervisor.
fn run_quic_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    run_stream_transport_runner(
        &cfg,
        once,
        runner_instance_id,
        runtime,
        StreamSupervisorMode::Strict(StreamTransport::Quic),
    )
    .map(|_| ())
}

/// Validate the `[quic]` config section. Returns a cloned, resolved config so
/// the session owns a concrete value (defaults applied).
pub(crate) fn resolve_quic_config(cfg: &RunnerConfig) -> Result<QuicClientConfig, String> {
    let quic = cfg.quic.clone().ok_or_else(|| {
        "transport=quic requires a [quic] section in the Runner config".to_string()
    })?;
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

fn build_quic_transport_config(quic: &QuicClientConfig) -> Result<quinn::TransportConfig, String> {
    let idle_timeout: quinn::IdleTimeout = QUIC_IDLE_TIMEOUT
        .try_into()
        .map_err(|_| "failed to encode QUIC idle timeout".to_string())?;
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(idle_timeout));
    transport.keep_alive_interval(Some(Duration::from_secs(quic.keepalive_interval_secs)));
    Ok(transport)
}

fn classify_quic_runner_connect_error(error: &str) -> &'static str {
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
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    once: bool,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, String> {
    let quic = resolve_quic_config(cfg)?;
    let client_crypto = build_quic_client_crypto(&quic)?;
    let mut client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    client_config.transport_config(Arc::new(build_quic_transport_config(&quic)?));
    let server_addrs = resolve_quic_server_addrs(&quic.server_addr)?;
    let mut connect_errors = Vec::new();
    let mut client_endpoint = None;
    let mut conn = None;
    for server_addr in server_addrs {
        if runtime.shutdown_requested() {
            return Ok(RunnerSessionExit::Shutdown);
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
            return Ok(RunnerSessionExit::Shutdown);
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
                    classify_quic_runner_connect_error(&raw),
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
        return Ok(RunnerSessionExit::Shutdown);
    };
    let (mut send, mut recv) =
        open_result.map_err(|e| format!("failed to open quic bidirectional stream: {}", e))?;

    // Credential ownership stays outside the transport-neutral registration payload.
    // The token is never logged.
    let projects_count = enabled_projects_count(&projects);
    let registered_jobs = runtime.jobs.inventory();
    let (register_payload, provider, provider_revision) =
        build_register_request_with_provider_status(
            cfg,
            &runtime.config,
            runner_instance_id,
            0,
            registered_jobs.clone(),
        );
    let register_frame = QuicRegisterFrame::new(register_payload, non_empty_token(&cfg.token));
    let Some(register_write) = future_or_shutdown(
        write_quic_register_frame(&mut send, &register_frame),
        runtime,
    )
    .await
    else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(RunnerSessionExit::Shutdown);
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
        return Ok(RunnerSessionExit::Shutdown);
    };
    let ack = ack_result
        .map_err(|_| "quic register ack timed out".to_string())?
        .map_err(|e| format!("failed to read quic register ack: {}", e))?;
    let _inventory_status = registered_ack(ack)?;
    let mut project_inventory_sync = Some(paged_sync_after_registration(projects));
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_QUIC, projects_count)
    );

    if once {
        // Complete one ping/pong round trip then exit, mirroring the websocket
        // `--once` semantics.
        let ping = RunnerEnvelope::Ping {
            ts: chrono::Utc::now().timestamp(),
        };
        let Some(ping_write) =
            future_or_shutdown(write_quic_frame(&mut send, &ping), runtime).await
        else {
            conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
            client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            return Ok(RunnerSessionExit::Shutdown);
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
            return Ok(RunnerSessionExit::Shutdown);
        };
        let resp = pong_result
            .map_err(|_| "quic once pong timed out".to_string())?
            .map_err(|e| format!("quic once pong read failed: {}", e))?;
        match resp {
            RunnerEnvelope::Pong { .. } => {}
            other => return Err(format!("expected pong, got {}", other.kind())),
        }
        let goodbye = RunnerEnvelope::Goodbye {
            reason: Some("once complete".to_string()),
        };
        let close_started = tokio::time::Instant::now();
        let goodbye_result = tokio::time::timeout(
            STREAM_WRITER_CLOSE_TIMEOUT,
            write_quic_frame(&mut send, &goodbye),
        )
        .await;
        let goodbye_sent = matches!(&goodbye_result, Ok(Ok(())));
        let finish_result = send.finish();
        if goodbye_sent && finish_result.is_ok() {
            let remaining = STREAM_WRITER_CLOSE_TIMEOUT.saturating_sub(close_started.elapsed());
            wait_for_quic_peer_close(
                async {
                    let _ = conn.closed().await;
                },
                remaining,
            )
            .await;
        }
        conn.close(quinn::VarInt::from_u32(0), b"once complete");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"once complete");
        goodbye_result
            .map_err(|_| "quic once goodbye flush timed out".to_string())?
            .map_err(|e| format!("quic once goodbye send failed: {e}"))?;
        finish_result.map_err(|e| format!("quic once send finish failed: {e}"))?;
        return Ok(RunnerSessionExit::Completed);
    }

    // Outgoing envelopes share one writer so future QUIC multistream work can
    // change the transport adapter without duplicating the session lifecycle.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
    try_queue_project_inventory_page(StreamTransport::Quic, &mut project_inventory_sync, &out_tx);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            let envelope_kind = env.kind();
            let graceful = matches!(env, RunnerEnvelope::Goodbye { .. });
            let send_started = Instant::now();
            if write_quic_frame(&mut send, &env).await.is_err() {
                observe_runner_stream_writer_send(
                    StreamTransport::Quic,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            }
            observe_runner_stream_writer_send(
                StreamTransport::Quic,
                envelope_kind,
                Some(send_started.elapsed()),
                RunnerStreamMetricOutcome::Success,
            );
            if graceful {
                return if send.finish().is_ok() {
                    StreamWriterExit::GracefulClose
                } else {
                    StreamWriterExit::TransportFailed
                };
            }
        }
        if send.finish().is_ok() {
            StreamWriterExit::ChannelClosed
        } else {
            StreamWriterExit::TransportFailed
        }
    });
    serve_registered_stream(
        StreamTransport::Quic,
        cfg,
        runner_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::Quic {
            reader: recv,
            connection: conn,
            endpoint: client_endpoint,
        },
        writer_task,
        project_inventory_sync,
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
// pushes `Request` envelopes; the Runner executes them via the same
// `dispatch_request` path the polling loop uses, and sends `Result` /
// `JobUpdate` envelopes back. Polling is unchanged and remains the fallback.

fn run_websocket_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    run_stream_transport_runner(
        &cfg,
        once,
        runner_instance_id,
        runtime,
        StreamSupervisorMode::Strict(StreamTransport::WebSocket),
    )
    .map(|_| ())
}

/// One WebSocket connection lifecycle: connect, register, then serve requests
/// until the socket closes or a fatal server error arrives.
#[cfg(test)]
pub(crate) async fn websocket_session(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, String> {
    websocket_session_classified(cfg, projects, runner_instance_id, runtime)
        .await
        .map_err(RunnerTransportError::into_message)
}

async fn websocket_session_classified(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, RunnerTransportError> {
    websocket_session_with_shutdown(
        cfg,
        projects,
        runner_instance_id,
        runtime,
        runtime.wait_for_shutdown(),
    )
    .await
}

async fn websocket_session_with_shutdown<F>(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
    shutdown: F,
) -> Result<RunnerSessionExit, RunnerTransportError>
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
            connect_websocket_request(request, &ws_url, &cfg.token),
        ) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(RunnerSessionExit::Shutdown);
        }
    };
    let mut ws_stream = connect.map_err(|_| {
        format!(
            "websocket connect timed out after {}s",
            cfg.websocket_connect_timeout_secs
        )
    })??;

    // Register over the socket. The prepared-profile cache is empty at
    // registration time (snapshots are prepared lazily on first use), so
    // `prepared_cache_count` is reported as 0 here.
    let projects_count = enabled_projects_count(&projects);
    let registered_jobs = runtime.jobs.inventory();
    let (register_payload, provider, provider_revision) =
        build_register_request_with_provider_status(
            cfg,
            &runtime.config,
            runner_instance_id,
            0,
            registered_jobs.clone(),
        );
    let reg_env = RunnerEnvelope::Register {
        payload: register_payload,
    };
    let reg_json =
        serde_json::to_string(&reg_env).map_err(|e| format!("failed to encode register: {}", e))?;
    tokio::select! {
        result = ws_stream.send(WsMessage::Text(reg_json.into())) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(RunnerSessionExit::Shutdown);
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
            return Ok(RunnerSessionExit::Shutdown);
        }
    }
    .ok_or_else(|| "server closed before register ack".to_string())?
    .map_err(|e| format!("failed to read register ack: {}", e))?;
    let ack_text = ack_msg
        .into_text()
        .map_err(|_| "register ack was not text".to_string())?;
    let ack = RunnerEnvelope::from_slice(ack_text.as_bytes())
        .map_err(|e| format!("register ack is not a valid envelope: {}", e))?;
    let _inventory_status = registered_ack(ack)?;
    let mut project_inventory_sync = Some(paged_sync_after_registration(projects));
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_WEBSOCKET, projects_count)
    );

    // Split socket into writer (drains outgoing envelopes) and reader.
    let (mut sink, stream) = ws_stream.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
    try_queue_project_inventory_page(
        StreamTransport::WebSocket,
        &mut project_inventory_sync,
        &out_tx,
    );
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            let envelope_kind = env.kind();
            let is_goodbye = matches!(env, RunnerEnvelope::Goodbye { .. });
            let send_started = Instant::now();
            let Ok(json) = serde_json::to_string(&env) else {
                observe_runner_stream_writer_send(
                    StreamTransport::WebSocket,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            };
            if sink.send(WsMessage::Text(json.into())).await.is_err() {
                observe_runner_stream_writer_send(
                    StreamTransport::WebSocket,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            }
            observe_runner_stream_writer_send(
                StreamTransport::WebSocket,
                envelope_kind,
                Some(send_started.elapsed()),
                RunnerStreamMetricOutcome::Success,
            );
            if is_goodbye {
                // The session loop continues polling the split read half while
                // awaiting this task, allowing tungstenite's close handshake to
                // progress without turning this into an unbounded wait.
                return if sink.close().await.is_ok() {
                    StreamWriterExit::GracefulClose
                } else {
                    StreamWriterExit::TransportFailed
                };
            }
        }
        StreamWriterExit::ChannelClosed
    });
    serve_registered_stream(
        StreamTransport::WebSocket,
        cfg,
        runner_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::WebSocket { reader: stream },
        writer_task,
        project_inventory_sync,
        runtime,
        shutdown,
    )
    .await
    .map_err(classify_session_error)
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
