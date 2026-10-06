use crate::registry::RunnerTransport;
use std::fmt::Debug;
use std::time::Duration;
use webcodex_core::mcp_gateway::McpGatewayToolResult;
use webcodex_core::runner_operation::RunnerOperation;
use webcodex_core::runner_protocol::{RunnerJobUpdateRequest, RunnerRequest, RunnerResultPayload};

/// One validated tool result observed at the Server's exact pending-request
/// boundary. References are borrowed for the callback only; this creates no
/// record, retention, export, or read authority. Request arguments are absent.
///
/// `result` is already wire-bounded, but may contain private provider content.
/// An observer must independently qualify the provider/tool and project only
/// approved bounded metadata. Acceptance is not effect success, client receipt,
/// a Workflow Session, or execution/model attestation.
pub struct ValidatedMcpToolResult<'a> {
    pub request_id: &'a str,
    pub client_id: &'a str,
    pub runner_instance_id: &'a str,
    pub provider_id: &'a str,
    pub provider_instance_id: &'a str,
    pub tool_name: &'a str,
    pub result: &'a McpGatewayToolResult,
}

/// Fail-open telemetry callbacks invoked only from authoritative registry
/// lifecycle points. Implementations must not re-enter the registry. Callback
/// results never participate in admission, lease, ownership, replay, or
/// dispatch decisions.
pub trait RunnerRegistryTelemetry: Debug + Send + Sync {
    #[allow(clippy::too_many_arguments)]
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
    );

    /// A queued request was authoritatively dequeued for one Runner transport.
    /// `queue_wait` is measured entirely on the Server monotonic clock.
    fn runner_request_dequeued(
        &self,
        _request_id: &str,
        _transport: RunnerTransport,
        _queue_wait: Duration,
    ) {
    }

    fn runner_result_accepted(&self, request_id: &str, payload: &RunnerResultPayload);

    /// Unlike `runner_result_accepted`, runs only after bridge response
    /// validation for the exact tool call, before delivery to its waiter.
    /// A rejected/malformed/expired response never reaches this callback.
    /// Default is no-op: no payload collection or exporter is enabled.
    /// Implementations must be bounded/nonblocking and must not re-enter the
    /// registry or use observed fields to grant authority or replay an effect.
    fn mcp_tool_result_validated(&self, _observation: ValidatedMcpToolResult<'_>) {}

    /// The matching ordinary Result was accepted. `round_trip` is Server
    /// enqueue -> Server result acceptance on one monotonic clock; it includes
    /// queueing, Runner execution, transport and serialization, and is not
    /// one-way network latency.
    fn runner_request_round_trip(
        &self,
        _request_id: &str,
        _transport: RunnerTransport,
        _round_trip: Duration,
    ) {
    }

    fn runner_result_finalized(&self, request_id: &str);

    fn runner_job_update_accepted(
        &self,
        request_id: Option<&str>,
        job_id: &str,
        payload: &RunnerJobUpdateRequest,
    );

    fn runner_job_finalized(&self, request_id: Option<&str>, job_id: &str);
}

#[derive(Debug, Default)]
pub struct NoopRunnerRegistryTelemetry;

impl RunnerRegistryTelemetry for NoopRunnerRegistryTelemetry {
    fn request_enqueued(
        &self,
        _request: &RunnerRequest,
        _operation: &RunnerOperation,
        _request_id: &str,
        _client_id: &str,
        _job_id: Option<&str>,
        _agent_instance_id: Option<&str>,
        _runner_transport: Option<&str>,
        _runner_version: Option<&str>,
        _runner_git_commit: Option<&str>,
    ) {
    }

    fn runner_result_accepted(&self, _request_id: &str, _payload: &RunnerResultPayload) {}

    fn runner_result_finalized(&self, _request_id: &str) {}

    fn runner_job_update_accepted(
        &self,
        _request_id: Option<&str>,
        _job_id: &str,
        _payload: &RunnerJobUpdateRequest,
    ) {
    }

    fn runner_job_finalized(&self, _request_id: Option<&str>, _job_id: &str) {}
}
