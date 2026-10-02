//! Runner transport bootstrap, supervision, and shared resource ownership.

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
use webcodex_browser::BrowserSupervisor;
use webcodex_core::runner_protocol::{
    read_quic_frame, write_quic_frame, write_quic_register_frame, QuicFrameError,
    QuicRegisterFrame, RunnerEnvelope, RunnerOfflineRequest, RunnerProjectSummary,
    ShellJobInventory, ShellProjectInventoryStatus,
};
#[cfg(test)]
use webcodex_core::runner_protocol::{
    PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES, PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
};
use webcodex_runner_config::{
    TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET,
};

pub(crate) mod http_client;
pub(crate) mod poll_dispatch;
mod project_inventory;
pub(crate) mod registration;
mod result_submission;
mod websocket_connect;

use super::dispatch::dispatch_request_with_outcome;
#[cfg(test)]
use super::output::CommandResult;
#[cfg(test)]
use http_client::{RunnerHttpError, RunnerHttpErrorKind};
use poll_dispatch::{handle_one_poll, PollingDispatchSupervisor, PollingRecoveryAction};
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
use registration::{build_register_request_with_provider_status, register, RegisterRecoveryAction};
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

// ============================================================================
// Shared streaming transport lifecycle
// ============================================================================
//
// WebSocket and QUIC keep their own frame codecs and close mechanics. Register
// acknowledgement, dispatch, keepalive, disconnect, and shutdown policy are
// shared by the stream and stream_envelopes modules.

/// Interval between agent-initiated keepalive Pings.
const QUIC_PING_INTERVAL: Duration = Duration::from_secs(30);

mod backoff;
mod bootstrap;
mod diagnostics;
mod listeners;
mod metrics;
mod polling;
mod quic;
mod runtime;
mod stream;
mod stream_envelopes;
mod supervisor;
mod websocket;

use backoff::{
    format_delay, next_lease_conflict_delay, polling_idle_delay,
    reset_backoff_after_stable_session, schedule_reconnect, PollingIdleBackoff, RetryBackoff,
};
use bootstrap::send_provider_metadata;
pub(crate) use bootstrap::{auto_transport_plan, effective_transport, non_empty_token, run_runner};
pub(crate) use diagnostics::RunnerTransportError;
use diagnostics::{
    auto_quic_not_configured_log_line, auto_trying_log_line, classify_session_error,
    concise_log_error, enabled_projects_count, registered_log_line, server_log_label,
};
#[cfg(unix)]
pub(crate) use listeners::install_reload_listener;
#[cfg(windows)]
use listeners::install_service_stop_listener;
use listeners::{
    async_sleep_or_shutdown, future_or_shutdown, install_parent_liveness_listener,
    install_shutdown_listener,
};
#[cfg(windows)]
#[cfg(test)]
use listeners::{spawn_windows_pipe_parent_liveness_listener, PARENT_PIPE_POLL_INTERVAL};
#[cfg(test)]
use metrics::{
    bounded_stream_envelope_kind, observe_runtime_metric_fail_open, successful_stream_duration,
};
use metrics::{
    observe_runner_request_duration, observe_runner_stream_disconnect,
    observe_runner_stream_incoming_envelope, observe_runner_stream_outgoing_channel,
    observe_runner_stream_request_dispatch_wait, observe_runner_stream_writer_send,
    try_send_runner_stream_control, RunnerStreamMetricOutcome,
};
#[cfg(test)]
use polling::run_polling_runner_with_shutdown;
use polling::{run_polling_runner, sleep_or_shutdown};
#[cfg(test)]
use quic::build_quic_transport_config;
#[cfg(test)]
pub(crate) use quic::{quic_client_bind_addr_for, resolve_quic_config, resolve_quic_server_addrs};
use quic::{quic_session, run_quic_runner};
#[cfg(test)]
use stream::{finish_quic_writer, StreamRead};
use stream::{
    serve_registered_stream, wait_for_quic_peer_close, RegisteredStream, RunnerWebSocket,
    StreamWriterExit,
};
use stream_envelopes::{handle_stream_envelope, registered_ack};
pub(crate) use supervisor::RunnerSessionExit;
#[cfg(test)]
use supervisor::{decide_stream_session, StreamSessionDecision};
use supervisor::{run_auto_runner, run_stream_transport_runner, StreamSupervisorMode};
#[cfg(test)]
pub(crate) use websocket::websocket_session;
#[cfg(test)]
use websocket::websocket_session_with_shutdown;
use websocket::{run_websocket_runner, websocket_session_classified};

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
