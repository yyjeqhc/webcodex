use super::RunnerRegistry;
use std::sync::Arc;
use webcodex_core::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentResultPayload, ShellAgentShellRequest,
};
use webcodex_runner_registry::RunnerRegistryTelemetry;

#[derive(Debug, Default)]
struct ToolRequestTraceRunnerRegistryTelemetry;

impl RunnerRegistryTelemetry for ToolRequestTraceRunnerRegistryTelemetry {
    fn request_enqueued(
        &self,
        request: &ShellAgentShellRequest,
        request_id: &str,
        client_id: &str,
        kind: &str,
        job_id: Option<&str>,
        agent_instance_id: Option<&str>,
        runner_transport: Option<&str>,
        runner_version: Option<&str>,
        runner_git_commit: Option<&str>,
    ) {
        crate::tool_request_trace::record_runner_request_enqueued(
            request,
            request_id,
            client_id,
            kind,
            job_id,
            agent_instance_id,
            runner_transport,
            runner_version,
            runner_git_commit,
        );
    }

    fn runner_result_accepted(&self, request_id: &str, payload: &ShellAgentResultPayload) {
        crate::tool_request_trace::capture_runner_result(request_id, payload);
    }

    fn runner_result_finalized(&self, request_id: &str) {
        crate::tool_request_trace::finalize_runner_result_correlation(request_id);
    }

    fn runner_job_update_accepted(
        &self,
        request_id: Option<&str>,
        job_id: &str,
        payload: &ShellAgentJobUpdateRequest,
    ) {
        crate::tool_request_trace::capture_runner_job_update(request_id, job_id, payload);
    }

    fn runner_job_finalized(&self, request_id: Option<&str>, job_id: &str) {
        crate::tool_request_trace::finalize_runner_job_correlation(request_id, job_id);
    }
}

pub(crate) fn registry_with_tool_request_trace() -> RunnerRegistry {
    RunnerRegistry::with_telemetry(Arc::new(ToolRequestTraceRunnerRegistryTelemetry))
}
