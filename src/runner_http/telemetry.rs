use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use webcodex_core::runner_operation::RunnerOperation;
use webcodex_core::runner_protocol::{RunnerJobUpdateRequest, RunnerRequest, RunnerResultPayload};
use webcodex_core::ssh_resource::SshResourceRequest;
use webcodex_runner_registry::{RunnerRegistryTelemetry, RunnerTransport};

fn observe_runtime_metric_fail_open(observe: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(observe));
}

// Per-envelope transport metrics are intentionally DEBUG-level. Emitting one
// INFO record for every request/result/job_update/ping can dominate operational
// logs and add avoidable I/O under busy Job streams; lifecycle and degradation
// events remain INFO/WARN at their dedicated call sites.
macro_rules! runtime_metric_info {
    ($($fields:tt)*) => {
        observe_runtime_metric_fail_open(|| tracing::debug!($($fields)*))
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerStreamMetricOutcome {
    Success,
    Closed,
    Backpressure,
    TransportError,
    Rejected,
}

impl RunnerStreamMetricOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Closed => "closed",
            Self::Backpressure => "backpressure",
            Self::TransportError => "transport_error",
            Self::Rejected => "rejected",
        }
    }
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

fn emit_server_runner_duration(
    metric: &'static str,
    transport: RunnerTransport,
    duration: Duration,
) {
    runtime_metric_info!(
        metric,
        value = duration.as_secs_f64(),
        transport = transport.as_str(),
        "runtime_metric"
    );
}

pub(crate) fn observe_server_stream_outgoing_channel(
    transport: RunnerTransport,
    envelope_kind: &'static str,
    wait: Option<Duration>,
    backpressured: bool,
    outcome: RunnerStreamMetricOutcome,
) {
    let transport = transport.as_str();
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    let successful_wait = successful_stream_duration(outcome, wait);
    let outcome = outcome.as_str();
    runtime_metric_info!(
        metric = "server_stream_outgoing_channel_events_total",
        value = 1_u64,
        transport,
        envelope_kind,
        outcome,
        "runtime_metric"
    );
    if backpressured {
        runtime_metric_info!(
            metric = "server_stream_outgoing_backpressure_total",
            value = 1_u64,
            transport,
            envelope_kind,
            "runtime_metric"
        );
    }
    if let Some(wait) = successful_wait {
        runtime_metric_info!(
            metric = "server_stream_outgoing_channel_wait_seconds",
            value = wait.as_secs_f64(),
            transport,
            envelope_kind,
            "runtime_metric"
        );
    }
}

pub(crate) fn observe_server_stream_writer_send(
    transport: RunnerTransport,
    envelope_kind: &'static str,
    duration: Option<Duration>,
    outcome: RunnerStreamMetricOutcome,
) {
    let transport = transport.as_str();
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    let successful_duration = successful_stream_duration(outcome, duration);
    let outcome = outcome.as_str();
    runtime_metric_info!(
        metric = "server_stream_outgoing_envelopes_total",
        value = 1_u64,
        transport,
        envelope_kind,
        outcome,
        "runtime_metric"
    );
    if let Some(duration) = successful_duration {
        runtime_metric_info!(
            metric = "server_stream_writer_send_seconds",
            value = duration.as_secs_f64(),
            transport,
            envelope_kind,
            "runtime_metric"
        );
    }
}

pub(crate) fn observe_server_stream_incoming_envelope(
    transport: RunnerTransport,
    envelope_kind: &'static str,
) {
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    runtime_metric_info!(
        metric = "server_stream_incoming_envelopes_total",
        value = 1_u64,
        transport = transport.as_str(),
        envelope_kind,
        "runtime_metric"
    );
}

pub(crate) fn observe_server_stream_ingress_processing(
    transport: RunnerTransport,
    envelope_kind: &'static str,
    duration: Duration,
    outcome: RunnerStreamMetricOutcome,
) {
    let envelope_kind = bounded_stream_envelope_kind(envelope_kind);
    runtime_metric_info!(
        metric = "server_stream_ingress_processing_seconds",
        value = duration.as_secs_f64(),
        transport = transport.as_str(),
        envelope_kind,
        outcome = outcome.as_str(),
        "runtime_metric"
    );
}

pub(crate) fn observe_server_stream_disconnect(transport: RunnerTransport) {
    runtime_metric_info!(
        metric = "server_stream_session_disconnects_total",
        value = 1_u64,
        transport = transport.as_str(),
        "runtime_metric"
    );
}

#[derive(Debug, Default)]
struct ToolRequestTraceRunnerRegistryTelemetry;

impl RunnerRegistryTelemetry for ToolRequestTraceRunnerRegistryTelemetry {
    fn request_enqueued(
        &self,
        request: &RunnerRequest,
        operation: &RunnerOperation,
        request_id: &str,
        client_id: &str,
        job_id: Option<&str>,
        runner_instance_id: Option<&str>,
        runner_transport: Option<&str>,
        runner_version: Option<&str>,
        runner_git_commit: Option<&str>,
    ) {
        let kind = operation.wire_kind();
        if let RunnerOperation::SshResource(ssh_operation) = operation {
            let payload = ssh_resource_trace_payload(request_id, client_id, ssh_operation);
            crate::tool_request_trace::record_runner_request_enqueued(
                &payload,
                request_id,
                client_id,
                kind,
                job_id,
                runner_instance_id,
                runner_transport,
                runner_version,
                runner_git_commit,
            );
        } else {
            crate::tool_request_trace::record_runner_request_enqueued(
                request,
                request_id,
                client_id,
                kind,
                job_id,
                runner_instance_id,
                runner_transport,
                runner_version,
                runner_git_commit,
            );
        }
    }

    fn runner_request_dequeued(
        &self,
        _request_id: &str,
        transport: RunnerTransport,
        queue_wait: Duration,
    ) {
        emit_server_runner_duration(
            "server_runner_request_queue_wait_seconds",
            transport,
            queue_wait,
        );
    }

    fn runner_result_accepted(&self, request_id: &str, payload: &RunnerResultPayload) {
        crate::tool_request_trace::capture_runner_result(request_id, payload);
    }

    fn runner_request_round_trip(
        &self,
        _request_id: &str,
        transport: RunnerTransport,
        round_trip: Duration,
    ) {
        emit_server_runner_duration(
            "server_runner_request_round_trip_seconds",
            transport,
            round_trip,
        );
    }

    fn runner_result_finalized(&self, request_id: &str) {
        crate::tool_request_trace::finalize_runner_result_correlation(request_id);
    }

    fn runner_job_update_accepted(
        &self,
        request_id: Option<&str>,
        job_id: &str,
        payload: &RunnerJobUpdateRequest,
    ) {
        crate::tool_request_trace::capture_runner_job_update(request_id, job_id, payload);
    }

    fn runner_job_finalized(&self, request_id: Option<&str>, job_id: &str) {
        crate::tool_request_trace::finalize_runner_job_correlation(request_id, job_id);
    }
}

fn ssh_resource_trace_payload(
    request_id: &str,
    client_id: &str,
    request: &SshResourceRequest,
) -> Value {
    let (action, resource_name, target_present, default_cwd_present) = match request {
        SshResourceRequest::List => ("list", None, false, false),
        SshResourceRequest::Register {
            name, default_cwd, ..
        } => ("register", Some(name.as_str()), true, default_cwd.is_some()),
        SshResourceRequest::Remove { name, .. } => ("remove", Some(name.as_str()), false, false),
    };
    json!({
        "request_id": request_id,
        "client_id": client_id,
        "kind": "ssh_resource",
        "action": action,
        "resource_name": resource_name,
        "target_present": target_present,
        "default_cwd_present": default_cwd_present,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_stream_runtime_metrics_are_fail_open() {
        observe_runtime_metric_fail_open(|| panic!("synthetic metric sink failure"));
    }

    #[test]
    fn runner_stream_metric_dimensions_are_bounded_and_payload_safe() {
        assert_eq!(
            [
                RunnerTransport::Polling.as_str(),
                RunnerTransport::WebSocket.as_str(),
                RunnerTransport::Quic.as_str(),
            ],
            ["polling", "websocket", "quic"]
        );
        assert_eq!(
            [
                RunnerStreamMetricOutcome::Success.as_str(),
                RunnerStreamMetricOutcome::Closed.as_str(),
                RunnerStreamMetricOutcome::Backpressure.as_str(),
                RunnerStreamMetricOutcome::TransportError.as_str(),
                RunnerStreamMetricOutcome::Rejected.as_str(),
            ],
            [
                "success",
                "closed",
                "backpressure",
                "transport_error",
                "rejected",
            ]
        );
        assert_eq!(bounded_stream_envelope_kind("result"), "result");
        assert_eq!(bounded_stream_envelope_kind("job_update"), "job_update");
        assert_eq!(
            bounded_stream_envelope_kind("project_inventory_page"),
            "project_inventory"
        );
        assert_eq!(
            bounded_stream_envelope_kind("runtime_metadata"),
            "provider_metadata"
        );
        assert_eq!(
            bounded_stream_envelope_kind("/private/path?token=secret"),
            "control"
        );
    }

    #[test]
    fn failed_stream_outcomes_never_emit_success_latency_samples() {
        let duration = Some(Duration::from_millis(7));
        assert_eq!(
            successful_stream_duration(RunnerStreamMetricOutcome::Success, duration),
            duration
        );
        assert_eq!(
            successful_stream_duration(RunnerStreamMetricOutcome::Closed, duration),
            None
        );
        assert_eq!(
            successful_stream_duration(RunnerStreamMetricOutcome::Backpressure, duration),
            None
        );
        assert_eq!(
            successful_stream_duration(RunnerStreamMetricOutcome::TransportError, duration),
            None
        );
    }

    #[test]
    fn ssh_resource_trace_projection_never_contains_target_or_default_cwd() {
        let target = "17724@w10";
        let cwd = "C:/private/work";
        let request = SshResourceRequest::Register {
            expected_revision: 7,
            name: "w10".to_string(),
            target: target.to_string(),
            default_cwd: Some(cwd.to_string()),
        };
        let payload = ssh_resource_trace_payload("request-1", "runner-1", &request);
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains(target));
        assert!(!serialized.contains(cwd));
        assert_eq!(payload["request_id"], "request-1");
        assert_eq!(payload["client_id"], "runner-1");
        assert_eq!(payload["kind"], "ssh_resource");
        assert_eq!(payload["action"], "register");
        assert_eq!(payload["resource_name"], "w10");
        assert_eq!(payload["target_present"], true);
        assert_eq!(payload["default_cwd_present"], true);
    }
}

pub(crate) fn tool_request_trace_telemetry(
) -> Arc<dyn webcodex_runner_registry::RunnerRegistryTelemetry> {
    Arc::new(ToolRequestTraceRunnerRegistryTelemetry)
}
