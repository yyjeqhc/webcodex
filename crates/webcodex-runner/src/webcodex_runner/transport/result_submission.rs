use std::fmt;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;

use super::http_client::{RunnerHttpError, RunnerHttpErrorKind};
use crate::webcodex_runner::config::{HotRunnerConfig, ReloadableRunnerConfig};
use crate::webcodex_runner::dispatch::runner_tool_trace_enabled;
use crate::webcodex_runner::output::CommandResult;
use crate::webcodex_runner::ShellCommandResult;
use webcodex_core::runner_protocol::{
    RunnerEnvelope, RunnerJobUpdateRequest, RunnerJobUpdateResponse,
    RunnerPersistentShellResultRequest, RunnerPersistentShellResultResponse, RunnerResultPayload,
    RunnerResultRequest, RunnerResultResponse,
};

use super::{
    concise_log_error, observe_runner_request_duration, observe_runner_stream_outgoing_channel,
    send_provider_metadata, sleep_or_shutdown, RunnerStreamMetricOutcome, StreamTransport,
};

/// Result submission endpoint used by the polling transport sink.
pub(super) const RUNNER_RESULT_PATH: &str = "/api/shell/agent/result";
const RUNNER_PERSISTENT_SHELL_RESULT_PATH: &str = "/api/shell/agent/persistent_shell_result";
/// Bounded same-payload retry backoff for transient result submission
/// failures over the polling transport. After the last step the payload is
/// released with an explicit dropped outcome, so a single result can never
/// monopolize the polling loop or trigger outer re-registration recovery.
pub(super) const RESULT_SUBMIT_RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

/// Minimal HTTP send configuration used by the polling `RunnerSink`. We do not
/// store the whole `RunnerConfig` here: policy and concurrency limits stay
/// with the Runner config and are passed alongside the sink.
#[derive(Debug, Clone)]
pub(crate) struct HttpSendConfig {
    pub(crate) client: Client,
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) client_id: String,
    pub(crate) runner_instance_id: String,
    pub(crate) shutdown: Arc<AtomicBool>,
}

/// Transport-neutral outgoing channel for a Runner. Both the polling loop and
/// the WebSocket loop build a `RunnerSink` and hand it to the shared
/// `dispatch_request` / `JobManager` execution path. This shared boundary lets
/// the Runner speak either transport without duplicating execution logic.
#[derive(Debug, Clone)]
pub(crate) enum RunnerSink {
    /// Polling transport: POST results/job_updates to the HTTP endpoints.
    Http(HttpSendConfig),
    /// WebSocket transport: push envelopes through an mpsc that a writer task
    /// drains onto the socket.
    WebSocket {
        tx: tokio::sync::mpsc::Sender<RunnerEnvelope>,
        client_id: String,
        runner_instance_id: String,
    },
    /// QUIC transport: push envelopes through an mpsc that a single writer
    /// task drains onto the bidirectional stream.
    Quic {
        tx: tokio::sync::mpsc::Sender<RunnerEnvelope>,
        client_id: String,
        runner_instance_id: String,
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
pub(super) enum ResultHttpErrorDisposition {
    RetryTransient,
    RejectPermanent,
    FatalAuth,
    FatalProtocol,
    FatalConfig,
}

pub(super) fn result_http_error_disposition(
    kind: &RunnerHttpErrorKind,
) -> ResultHttpErrorDisposition {
    match kind {
        RunnerHttpErrorKind::ServerUnavailable
        | RunnerHttpErrorKind::Status
        | RunnerHttpErrorKind::RequestTimeout
        | RunnerHttpErrorKind::Request
        | RunnerHttpErrorKind::DecodeTransient => ResultHttpErrorDisposition::RetryTransient,
        RunnerHttpErrorKind::ClientRejected => ResultHttpErrorDisposition::RejectPermanent,
        RunnerHttpErrorKind::Auth => ResultHttpErrorDisposition::FatalAuth,
        RunnerHttpErrorKind::NotFound | RunnerHttpErrorKind::ProtocolDecode => {
            ResultHttpErrorDisposition::FatalProtocol
        }
        RunnerHttpErrorKind::Config => ResultHttpErrorDisposition::FatalConfig,
    }
}

/// One bounded, redacted diagnostic line for a permanently rejected result.
/// Emitted exactly once per payload: permanent rejections are never retried,
/// so this cannot repeat for the same result.
pub(super) fn permanent_result_rejection_log_line(
    request_id: &str,
    error: &str,
    token: &str,
) -> String {
    format!(
        "webcodex-runner result permanently rejected request_id={} error={}; dropping this result and continuing to poll",
        concise_log_error(request_id, token),
        concise_log_error(error, token)
    )
}

/// One bounded, redacted warning after all transient submission attempts have
/// failed. It makes the possible result loss explicit without exposing raw
/// response bodies, credentials, or multiline request errors.
pub(super) fn dropped_result_log_line(
    request_id: &str,
    attempts: usize,
    error: &str,
    token: &str,
) -> String {
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
    body: &RunnerResultPayload,
) -> Result<ResultSubmission, SubmitResultError> {
    let mut attempt = 0usize;
    loop {
        let error = match post_json_raw::<_, RunnerResultResponse>(
            &h.client,
            &h.server_url,
            &h.token,
            RUNNER_RESULT_PATH,
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

impl RunnerSink {
    pub(crate) fn client_id(&self) -> &str {
        match self {
            RunnerSink::Http(h) => &h.client_id,
            RunnerSink::WebSocket { client_id, .. } => client_id,
            RunnerSink::Quic { client_id, .. } => client_id,
        }
    }

    fn transport_name(&self) -> &'static str {
        match self {
            RunnerSink::Http(_) => webcodex_runner_config::TRANSPORT_POLLING,
            RunnerSink::WebSocket { .. } => webcodex_runner_config::TRANSPORT_WEBSOCKET,
            RunnerSink::Quic { .. } => webcodex_runner_config::TRANSPORT_QUIC,
        }
    }

    fn stream_transport(&self) -> Option<StreamTransport> {
        match self {
            RunnerSink::Http(_) => None,
            RunnerSink::WebSocket { .. } => Some(StreamTransport::WebSocket),
            RunnerSink::Quic { .. } => Some(StreamTransport::Quic),
        }
    }

    /// Active Runner process identity carried by this sink so every result /
    /// job_update submission includes it.
    pub(crate) fn runner_instance_id(&self) -> &str {
        match self {
            RunnerSink::Http(h) => &h.runner_instance_id,
            RunnerSink::WebSocket {
                runner_instance_id, ..
            } => runner_instance_id,
            RunnerSink::Quic {
                runner_instance_id, ..
            } => runner_instance_id,
        }
    }

    /// Submit the result of a synchronous shell/file request. Mirrors the old
    /// `submit_result` free function but routes over the active transport.
    pub(crate) fn submit_result(
        &self,
        request_id: String,
        result: CommandResult,
    ) -> Result<ResultSubmission, SubmitResultError> {
        self.submit_result_payload(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: self.client_id().to_string(),
                runner_instance_id: self.runner_instance_id().to_string(),
                request_id,
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: result.duration_ms,
                error: result.error,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
    }

    /// Submit one closed MCP gateway response. The response is typed separately
    /// from stdout/stderr so provider data cannot become a shell-result tunnel.
    pub(crate) fn submit_mcp_gateway_result(
        &self,
        request_id: String,
        response: webcodex_core::mcp_gateway::McpGatewayResponse,
    ) -> Result<ResultSubmission, SubmitResultError> {
        self.submit_result_payload(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: self.client_id().to_string(),
                runner_instance_id: self.runner_instance_id().to_string(),
                request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: Some(response),
            plugin_gateway: None,
            coding_agent: None,
        })
    }

    /// Submit one closed native Tool Plugin response. Plugin protocol traffic
    /// remains Runner-local; only the bounded typed WebCodex response crosses
    /// the Server transport.
    pub(crate) fn submit_plugin_gateway_result(
        &self,
        request_id: String,
        response: webcodex_core::plugin::PluginGatewayResponse,
    ) -> Result<ResultSubmission, SubmitResultError> {
        self.submit_result_payload(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: self.client_id().to_string(),
                runner_instance_id: self.runner_instance_id().to_string(),
                request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: Some(response),
            coding_agent: None,
        })
    }

    /// Submit one closed CodingAgentRun response. ACP JSON-RPC remains local to
    /// the Runner and never crosses this typed transport result boundary.
    pub(crate) fn submit_coding_agent_result(
        &self,
        request_id: String,
        response: webcodex_core::coding_agent::CodingAgentResponse,
    ) -> Result<ResultSubmission, SubmitResultError> {
        self.submit_result_payload(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: self.client_id().to_string(),
                runner_instance_id: self.runner_instance_id().to_string(),
                request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: Some(response),
        })
    }

    fn submit_result_payload(
        &self,
        body: RunnerResultPayload,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let request_id = body.result.request_id.clone();
        if let Some(duration_ms) = body.result.duration_ms {
            observe_runner_request_duration(self.transport_name(), duration_ms);
        }
        if runner_tool_trace_enabled() {
            tracing::info!(
                event = "runner_tool_result_submit_started",
                runner_request_id = %request_id,
                runner_client_id = self.client_id(),
                runner_agent_instance_id = self.runner_instance_id(),
                "runner_tool_result_submit_started"
            );
        }
        let submitted = match self {
            RunnerSink::Http(h) => submit_result_http(h, &body),
            RunnerSink::WebSocket { tx, .. } | RunnerSink::Quic { tx, .. } => {
                let transport = self
                    .stream_transport()
                    .expect("push RunnerSink must have stream transport");
                let queue_started = Instant::now();
                let env = RunnerEnvelope::Result { payload: body };
                match tx.blocking_send(env) {
                    Ok(()) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "result",
                            Some(queue_started.elapsed()),
                            false,
                            RunnerStreamMetricOutcome::Success,
                        );
                        Ok(ResultSubmission::Accepted)
                    }
                    Err(_) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "result",
                            None,
                            false,
                            RunnerStreamMetricOutcome::Closed,
                        );
                        Err(SubmitResultError::TransportClosed(
                            "agent transport result channel closed".to_string(),
                        ))
                    }
                }
            }
        };
        if runner_tool_trace_enabled() {
            match &submitted {
                Ok(outcome) => tracing::info!(
                    event = "runner_tool_result_submit_finished",
                    runner_request_id = %request_id,
                    outcome = ?outcome,
                    "runner_tool_result_submit_finished"
                ),
                Err(error) => tracing::warn!(
                    event = "runner_tool_result_submit_finished",
                    runner_request_id = %request_id,
                    error = %error,
                    "runner_tool_result_submit_finished"
                ),
            }
        }
        submitted
    }

    pub(crate) fn submit_shell_result_with_metadata(
        &self,
        request_id: String,
        shell_result: ShellCommandResult,
        config: &HotRunnerConfig,
        runtime: &ReloadableRunnerConfig,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let ShellCommandResult {
            result,
            execution_state,
            stdout_truncated,
            stderr_truncated,
        } = shell_result;
        let body = RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: self.client_id().to_string(),
                runner_instance_id: self.runner_instance_id().to_string(),
                request_id,
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                stdout_truncated,
                stderr_truncated,
                duration_ms: result.duration_ms,
                error: result.error,
            },
            command_execution_state: Some(execution_state),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
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
        config: &HotRunnerConfig,
        runtime: &ReloadableRunnerConfig,
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
        result: webcodex_core::runner_protocol::PersistentShellResult,
    ) -> Result<ResultSubmission, SubmitResultError> {
        let body = RunnerPersistentShellResultRequest {
            client_id: self.client_id().to_string(),
            runner_instance_id: self.runner_instance_id().to_string(),
            request_id,
            result,
        };
        match self {
            RunnerSink::Http(h) => submit_persistent_shell_result_http(h, &body),
            RunnerSink::WebSocket { tx, .. } | RunnerSink::Quic { tx, .. } => {
                let transport = self
                    .stream_transport()
                    .expect("push RunnerSink must have stream transport");
                let queue_started = Instant::now();
                let env = RunnerEnvelope::PersistentShellResult { payload: body };
                match tx.blocking_send(env) {
                    Ok(()) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "persistent_shell_result",
                            Some(queue_started.elapsed()),
                            false,
                            RunnerStreamMetricOutcome::Success,
                        );
                        Ok(ResultSubmission::Accepted)
                    }
                    Err(_) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "persistent_shell_result",
                            None,
                            false,
                            RunnerStreamMetricOutcome::Closed,
                        );
                        Err(SubmitResultError::TransportClosed(
                            "agent transport persistent shell result channel closed".to_string(),
                        ))
                    }
                }
            }
        }
    }

    fn send_provider_metadata_best_effort(
        &self,
        generation: u64,
        runtime: &ReloadableRunnerConfig,
    ) {
        let (RunnerSink::WebSocket { tx, .. } | RunnerSink::Quic { tx, .. }) = self else {
            return;
        };
        let transport = self
            .stream_transport()
            .expect("push RunnerSink must have stream transport");
        send_provider_metadata(transport, tx, runtime, Some(generation));
    }

    pub(crate) fn same_job_update_target(&self, other: &Self) -> bool {
        match (self, other) {
            (RunnerSink::Http(left), RunnerSink::Http(right)) => {
                left.server_url == right.server_url
                    && left.client_id == right.client_id
                    && left.runner_instance_id == right.runner_instance_id
            }
            (RunnerSink::WebSocket { tx: left, .. }, RunnerSink::WebSocket { tx: right, .. })
            | (RunnerSink::Quic { tx: left, .. }, RunnerSink::Quic { tx: right, .. }) => {
                left.same_channel(right)
            }
            _ => false,
        }
    }

    /// Non-blocking stream enqueue used by the JobManager's single bounded
    /// delivery worker. HTTP remains a bounded synchronous request, but only
    /// that delivery worker waits for it; child output capture never does.
    /// `Ok(false)` means the live WS/QUIC queue is full and the caller must
    /// retain the update for a later retry rather than drop it.
    pub(crate) fn try_send_job_update(
        &self,
        body: &RunnerJobUpdateRequest,
    ) -> Result<bool, String> {
        match self {
            RunnerSink::Http(h) => {
                let resp: RunnerJobUpdateResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/job_update",
                    body,
                )
                .map_err(|e| e.to_string())?;
                if resp.success {
                    Ok(true)
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "job_update failed without error".to_string()))
                }
            }
            RunnerSink::WebSocket { tx, .. } | RunnerSink::Quic { tx, .. } => {
                let transport = self
                    .stream_transport()
                    .expect("push RunnerSink must have stream transport");
                let env = RunnerEnvelope::JobUpdate {
                    payload: body.clone(),
                };
                match tx.try_send(env) {
                    Ok(()) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "job_update",
                            None,
                            false,
                            RunnerStreamMetricOutcome::Success,
                        );
                        Ok(true)
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "job_update",
                            None,
                            true,
                            RunnerStreamMetricOutcome::Backpressure,
                        );
                        Ok(false)
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "job_update",
                            None,
                            false,
                            RunnerStreamMetricOutcome::Closed,
                        );
                        Err("agent transport send failed".to_string())
                    }
                }
            }
        }
    }

    /// Push an incremental/final job update. Mirrors the old `send_job_update`
    /// free function. Job updates stay best-effort: callers ignore failures
    /// and the terminal state is still resolved by the final result path.
    pub(crate) fn send_job_update(&self, body: &RunnerJobUpdateRequest) -> Result<(), String> {
        match self {
            RunnerSink::Http(h) => {
                let resp: RunnerJobUpdateResponse = post_json_raw(
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
            RunnerSink::WebSocket { tx, .. } | RunnerSink::Quic { tx, .. } => {
                let transport = self
                    .stream_transport()
                    .expect("push RunnerSink must have stream transport");
                let queue_started = Instant::now();
                let env = RunnerEnvelope::JobUpdate {
                    payload: body.clone(),
                };
                match tx.blocking_send(env) {
                    Ok(()) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "job_update",
                            Some(queue_started.elapsed()),
                            false,
                            RunnerStreamMetricOutcome::Success,
                        );
                        Ok(())
                    }
                    Err(_) => {
                        observe_runner_stream_outgoing_channel(
                            transport,
                            "job_update",
                            None,
                            false,
                            RunnerStreamMetricOutcome::Closed,
                        );
                        Err("agent transport send failed".to_string())
                    }
                }
            }
        }
    }
}

fn submit_persistent_shell_result_http(
    h: &HttpSendConfig,
    body: &RunnerPersistentShellResultRequest,
) -> Result<ResultSubmission, SubmitResultError> {
    let mut attempt = 0usize;
    loop {
        let error = match post_json_raw::<_, RunnerPersistentShellResultResponse>(
            &h.client,
            &h.server_url,
            &h.token,
            RUNNER_PERSISTENT_SHELL_RESULT_PATH,
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
/// `RunnerSink::Http` without an `RunnerConfig`. Preserves the structured
/// `RunnerHttpError` classification for callers that must act on it.
fn post_json_raw<T, R>(
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
    crate::webcodex_runner::transport::http_client::post_json_with_auth(
        client, server_url, token, path, body,
    )
}
