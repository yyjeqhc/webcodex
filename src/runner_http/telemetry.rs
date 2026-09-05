use super::RunnerRegistry;
use serde_json::{json, Value};
use std::sync::Arc;
use webcodex_core::runner_protocol::{RunnerJobUpdateRequest, RunnerRequest, RunnerResultPayload};
use webcodex_core::ssh_resource::SshResourceRequest;
use webcodex_runner_registry::RunnerRegistryTelemetry;

#[derive(Debug, Default)]
struct ToolRequestTraceRunnerRegistryTelemetry;

impl RunnerRegistryTelemetry for ToolRequestTraceRunnerRegistryTelemetry {
    fn request_enqueued(
        &self,
        request: &RunnerRequest,
        request_id: &str,
        client_id: &str,
        kind: &str,
        job_id: Option<&str>,
        runner_instance_id: Option<&str>,
        runner_transport: Option<&str>,
        runner_version: Option<&str>,
        runner_git_commit: Option<&str>,
    ) {
        if kind == "ssh_resource" {
            let payload = ssh_resource_trace_payload(request);
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

    fn runner_result_accepted(&self, request_id: &str, payload: &RunnerResultPayload) {
        crate::tool_request_trace::capture_runner_result(request_id, payload);
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

fn ssh_resource_trace_payload(request: &RunnerRequest) -> Value {
    let parsed = request
        .content
        .as_deref()
        .and_then(|content| serde_json::from_str::<SshResourceRequest>(content).ok());
    let (action, resource_name, target_present, default_cwd_present) = match parsed {
        Some(SshResourceRequest::List) => ("list", None, false, false),
        Some(SshResourceRequest::Register {
            name, default_cwd, ..
        }) => ("register", Some(name), true, default_cwd.is_some()),
        Some(SshResourceRequest::Remove { name, .. }) => ("remove", Some(name), false, false),
        None => ("invalid", None, request.content.is_some(), false),
    };
    json!({
        "request_id": request.request_id,
        "client_id": request.client_id,
        "kind": "ssh_resource",
        "action": action,
        "resource_name": resource_name,
        "target_present": target_present,
        "default_cwd_present": default_cwd_present,
    })
}

pub(crate) fn registry_with_tool_request_trace() -> RunnerRegistry {
    RunnerRegistry::with_telemetry(Arc::new(ToolRequestTraceRunnerRegistryTelemetry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_resource_trace_projection_never_contains_target_or_default_cwd() {
        let target = "17724@w10";
        let cwd = "C:/private/work";
        let request = RunnerRequest {
            request_id: "request-1".to_string(),
            client_id: "runner-1".to_string(),
            kind: "ssh_resource".to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: Some(
                serde_json::to_string(&SshResourceRequest::Register {
                    expected_revision: 7,
                    name: "w10".to_string(),
                    target: target.to_string(),
                    default_cwd: Some(cwd.to_string()),
                })
                .unwrap(),
            ),
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 60,
            requested_by: "ssh_resource".to_string(),
            created_at: 1,
            validation: None,
            lsp: None,
            job_context: None,
            persistent_shell: None,
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        };
        let payload = ssh_resource_trace_payload(&request);
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains(target));
        assert!(!serialized.contains(cwd));
        assert_eq!(payload["action"], "register");
        assert_eq!(payload["resource_name"], "w10");
        assert_eq!(payload["target_present"], true);
        assert_eq!(payload["default_cwd_present"], true);
    }
}
