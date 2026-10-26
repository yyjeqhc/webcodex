use salvo::prelude::*;
use std::sync::Arc;

mod auth;
mod handlers;
mod telemetry;

pub(crate) use auth::{
    detached_initiator_identity_from_auth, effective_register_owner, enforce_register_owner,
    enforce_runner_transport, requested_by_from_auth, require_runner_transport_scope,
    runner_access_from_auth,
};
pub use handlers::{
    runner_job_update, runner_offline, runner_persistent_shell_result, runner_poll,
    runner_register, runner_result,
};
pub(crate) use telemetry::{
    observe_server_stream_disconnect, observe_server_stream_incoming_envelope,
    observe_server_stream_ingress_processing, observe_server_stream_outgoing_channel,
    observe_server_stream_writer_send, tool_request_trace_telemetry, RunnerStreamMetricOutcome,
};
pub(crate) use webcodex_runner_registry::{
    command_preview, process_preview, recovery_timeout_sweep, script_preview, EnqueueLspError,
    EnqueueRunnerSkillError, JobShellTarget, JobStartContext, JobStartExecution, JobStartRequest,
    JobValidationCorrelation, JobValidationPlan, RunnerFeature, RunnerFeatureSet, RunnerRegistry,
    RunnerSemanticView, RunnerTransport, ShellJobVisibility, COMMAND_PREVIEW_MAX_CHARS,
    RECOVERY_SWEEP_INTERVAL_SECS, RUNNER_ONLINE_WINDOW_SECS,
};

/// Mounted below the authenticated `/api` router. Keep Runner transport
/// separate from model execution, which enters the ToolRuntime kernel.
pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};

    Router::new()
        .push(Router::with_path(api_path(RouteId::RunnerRegister)).post(runner_register))
        .push(Router::with_path(api_path(RouteId::RunnerOffline)).post(runner_offline))
        .push(Router::with_path(api_path(RouteId::RunnerPoll)).post(runner_poll))
        .push(Router::with_path(api_path(RouteId::RunnerResult)).post(runner_result))
        .push(
            Router::with_path(api_path(RouteId::RunnerPersistentShellResult))
                .post(runner_persistent_shell_result),
        )
        .push(Router::with_path(api_path(RouteId::RunnerJobUpdate)).post(runner_job_update))
}

fn get_registry(depot: &Depot) -> Option<Arc<RunnerRegistry>> {
    depot.obtain::<Arc<RunnerRegistry>>().ok().cloned()
}

#[cfg(test)]
pub(crate) use webcodex_runner_registry::{TRANSPORT_POLLING, TRANSPORT_WEBSOCKET};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
