//! Polling request admission, bounded dispatch workers and recovery decisions.
//! Transport supervision owns reconnects; the dispatch module owns request execution.
use super::http_client::{
    bounded_single_line, is_unknown_polling_session, looks_like_auth_failure_message, post_json,
    RunnerHttpError, RunnerHttpErrorKind,
};
use super::{HttpSendConfig, RunnerSink, SubmitResultError};
use crate::webcodex_runner;
use crate::webcodex_runner::config::{
    project_registry_dir, HotRunnerConfig, ReloadableRunnerConfig, RunnerConfig,
};
use crate::webcodex_runner::dispatch::{dispatch_request_with_outcome, RunnerDispatchOutcome};
use crate::webcodex_runner::job_manager::JobManager;
use crate::webcodex_runner::projects::RunnerProjectCache;
use crate::webcodex_runner::shutdown::{ActivityTracker, BackgroundThreads};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use webcodex_core::runner_protocol::{
    RunnerPollPayload, RunnerPollRequest, RunnerPollResponse, RunnerRequest,
    ShellProjectInventoryPage, ShellProjectInventoryStatus,
};

pub(crate) const RUNNER_POLL_PATH: &str = "/api/shell/agent/poll";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PollingRecoveryAction {
    RetryPoll,
    ReRegister,
    Fatal,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PollErrorKind {
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
pub(crate) struct PollError {
    pub(crate) kind: PollErrorKind,
    pub(crate) message: String,
}

impl PollError {
    pub(crate) fn new(kind: PollErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn from_http(error: RunnerHttpError, client_id: &str) -> Self {
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
    pub(crate) fn from_submit(error: SubmitResultError) -> Self {
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

    pub(crate) fn from_response_error(client_id: &str, error: Option<String>) -> Self {
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

    pub(crate) fn recovery_action(&self) -> PollingRecoveryAction {
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
    pub(crate) fn is_terminal(&self) -> bool {
        self.recovery_action() == PollingRecoveryAction::Fatal
    }

    #[cfg(test)]
    pub(crate) fn is_shutdown(&self) -> bool {
        self.kind == PollErrorKind::Shutdown
    }

    pub(crate) fn into_message(self) -> String {
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

pub(crate) struct PollingDispatch {
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

pub(crate) struct PollingDispatchCompletion {
    request_id: String,
    dispatch_result: Result<RunnerDispatchOutcome, SubmitResultError>,
}

/// Sends a completion even if a worker unwinds. It is declared before the
/// ActivityGuard in the worker so reverse drop order releases the activity
/// slot only after dispatch and result submission, then publishes completion.
pub(crate) struct PollingDispatchCompletionOnDrop {
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

    pub(crate) fn has_capacity(&self) -> bool {
        self.in_flight < POLLING_DISPATCH_MAX_IN_FLIGHT
    }

    pub(crate) fn spawn(&mut self, dispatch: PollingDispatch) -> Result<(), PollError> {
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

    pub(crate) fn record_completion(
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

pub(crate) fn handle_one_poll(
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
    let computer_session_update = webcodex_runner::computer_session::changed_availability();
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
        computer_session_availability: computer_session_update,
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
    if let Some(available) = computer_session_update {
        webcodex_runner::computer_session::mark_availability_reported(available);
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
