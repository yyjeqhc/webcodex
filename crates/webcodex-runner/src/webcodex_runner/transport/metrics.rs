//! Fail-open stream metrics and bounded control delivery.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunnerStreamMetricOutcome {
    Success,
    Closed,
    Backpressure,
    TransportError,
    Timeout,
}

impl RunnerStreamMetricOutcome {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Closed => "closed",
            Self::Backpressure => "backpressure",
            Self::TransportError => "transport_error",
            Self::Timeout => "timeout",
        }
    }
}

pub(super) fn observe_runtime_metric_fail_open(observe: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(observe));
}

macro_rules! runtime_metric_info {
    ($($fields:tt)*) => {
        observe_runtime_metric_fail_open(|| tracing::info!($($fields)*))
    };
}

pub(super) fn bounded_stream_envelope_kind(kind: &str) -> &'static str {
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

pub(super) fn successful_stream_duration(
    outcome: RunnerStreamMetricOutcome,
    duration: Option<Duration>,
) -> Option<Duration> {
    (outcome == RunnerStreamMetricOutcome::Success)
        .then_some(duration)
        .flatten()
}

pub(super) fn observe_runner_stream_incoming_envelope(
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

pub(super) fn observe_runner_stream_request_dispatch_wait(
    transport: StreamTransport,
    duration: Duration,
) {
    runtime_metric_info!(
        metric = "runner_stream_request_dispatch_wait_seconds",
        value = duration.as_secs_f64(),
        transport = transport.name(),
        "runtime_metric"
    );
}

pub(super) fn observe_runner_request_duration(transport: &'static str, duration_ms: u64) {
    runtime_metric_info!(
        metric = "runner_request_duration_seconds",
        value = duration_ms as f64 / 1000.0,
        transport,
        "runtime_metric"
    );
}

pub(super) fn observe_runner_stream_outgoing_channel(
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
pub(super) fn try_send_runner_stream_control(
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

pub(super) fn observe_runner_stream_writer_send(
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

pub(super) fn observe_runner_stream_disconnect(transport: StreamTransport) {
    runtime_metric_info!(
        metric = "runner_stream_session_disconnects_total",
        value = 1_u64,
        transport = transport.name(),
        "runtime_metric"
    );
}
