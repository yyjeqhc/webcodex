use super::external_tools::ExternalRoute;
use super::lsp::{handle_lsp_request, is_lsp_request_kind, LspSupervisor};
use super::transport::ResultSubmission;
use super::validation::{handle_validation_request, is_validation_request_kind};
use super::{
    handle_project_lifecycle_op, handle_project_op_with_temporary_projects_root,
    handle_resolve_or_register_project, run_shell_with_profiles_in_sandbox_and_execution_state,
    run_ssh_shell_with_execution_state, AgentSink, CommandResult, HotAgentConfig,
    PersistentShellManager, ReloadableAgentConfig, ShellCommandResult, SubmitResultError,
};
use crate::shell_protocol::ShellAgentShellRequest;
use crate::{handle_file_request, is_file_request_kind, JobManager};
use std::path::Path;
use std::sync::atomic::Ordering;

/// Execute a single agent request (shell/file/job/lsp/validation) and send the
/// result over the active transport. This is the shared dispatch path used by
/// both the polling loop (`handle_one_poll`) and the WebSocket loop. It contains
/// no transport-specific code: all outgoing traffic goes through `sink`.
pub(crate) fn dispatch_request(
    sink: &AgentSink,
    config: &HotAgentConfig,
    runtime: &ReloadableAgentConfig,
    jobs: &JobManager,
    persistent_shells: &PersistentShellManager,
    projects_dir: &Path,
    lsp: &LspSupervisor,
    request: ShellAgentShellRequest,
) -> Result<bool, SubmitResultError> {
    if runtime.shutdown_flag().load(Ordering::SeqCst) {
        return Ok(false);
    }
    // File operations are an explicit protocol surface. Unknown `file_*`
    // requests must fail before provider routing or any shell fallback.
    if request.kind.starts_with("file_") && !is_file_request_kind(&request.kind) {
        let result = CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(
                "unsupported_file_request_kind: unsupported file request kind; command was not started"
                    .to_string(),
            ),
        };
        return sink
            .submit_result_with_metadata(request.request_id, result, config, runtime)
            .map(|_| true);
    }
    let policy = &config.policy;
    let shell = &config.shell;
    let external_tools = &config.external_tools;
    let ssh_resource = request
        .job_context
        .as_ref()
        .and_then(|context| context.ssh_resource.as_deref());
    let ssh_session_id = request
        .job_context
        .as_ref()
        .and_then(|context| context.workflow_session_id.as_deref());
    if request.kind == "persistent_shell" {
        let request_id = request.request_id.clone();
        let operation = request.persistent_shell.clone();
        let result = persistent_shells.handle(
            policy,
            shell,
            &config.ssh,
            config.generation,
            projects_dir,
            &request,
        );
        let submitted = sink.submit_persistent_shell_result(request_id, result);
        if !matches!(&submitted, Ok(ResultSubmission::Accepted)) {
            if let Some(operation) = operation {
                let _ = persistent_shells.close_exact(
                    &operation.shell_id,
                    &operation.workflow_session_id,
                    &operation.runtime_project_id,
                    "persistent_shell_result_not_accepted",
                );
            }
        }
        return submitted.map(|_| true);
    }
    // Inspect requests must stay on the native execution path where Landlock
    // is applied in pre_exec. External providers are not an equivalent local
    // filesystem write boundary.
    if ssh_resource.is_some()
        && !matches!(
            request.kind.as_str(),
            "run_shell" | "start_job" | "start_validation_job"
        )
    {
        let result = CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(
                "ssh_resource_unsupported_for_request: SSH resources are only available to run_shell and run_job; command was not started".to_string(),
            ),
        };
        return sink
            .submit_result_with_metadata(request.request_id, result, config, runtime)
            .map(|_| true);
    }
    let external_route = if request.sandbox.is_some() || ssh_resource.is_some() {
        ExternalRoute::Native
    } else {
        external_tools.route_with_shutdown(policy, &request, Some(runtime.shutdown_flag()))
    };
    match external_route {
        ExternalRoute::Handled(result) => {
            return sink
                .submit_result_with_metadata(request.request_id, result, config, runtime)
                .map(|_| true);
        }
        ExternalRoute::NativeFallback(fallback) => {
            let request_id = request.request_id.clone();
            if is_file_request_kind(&request.kind) {
                let result = handle_file_request(policy, &request);
                external_tools.complete_native_fallback(fallback, &result);
                return sink
                    .submit_result_with_metadata(request_id, result, config, runtime)
                    .map(|_| true);
            }
            let result = run_shell_with_profiles_in_sandbox_and_execution_state(
                config.generation,
                policy,
                shell,
                projects_dir,
                &jobs.prepared_profiles,
                request.cwd.as_deref(),
                &request.command,
                request.stdin.as_deref(),
                request.timeout_secs,
                Some(runtime.shutdown_flag()),
                request.sandbox.as_deref(),
            );
            external_tools.complete_native_fallback(fallback, &result.result);
            return sink
                .submit_shell_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true);
        }
        ExternalRoute::Native => {}
    }
    match request.kind.as_str() {
        "start_job" | "start_validation_job" => {
            jobs.enqueue(
                sink.clone(),
                config.generation,
                policy.clone(),
                shell.clone(),
                config.ssh.clone(),
                projects_dir.to_path_buf(),
                request,
            );
            Ok(true)
        }
        "stop_job" => {
            jobs.install_sink(sink.clone());
            if let Some(job_id) = request.job_id.as_deref() {
                if let Err(e) = jobs.stop(job_id) {
                    eprintln!("webcodex-runner stop_job error: {}", e);
                }
            }
            Ok(true)
        }
        kind if is_file_request_kind(kind) => {
            let request_id = request.request_id.clone();
            let result = handle_file_request(policy, &request);
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        "register_project" | "create_project" => {
            let request_id = request.request_id.clone();
            let result = handle_project_op_with_temporary_projects_root(
                policy,
                projects_dir,
                runtime.temporary_projects_root(),
                &request,
            );
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        "resolve_or_register_project" => {
            let request_id = request.request_id.clone();
            let result = handle_resolve_or_register_project(policy, projects_dir, &request);
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        "project_lifecycle_enable"
        | "project_lifecycle_disable"
        | "project_lifecycle_unregister" => {
            let request_id = request.request_id.clone();
            let result = handle_project_lifecycle_op(policy, projects_dir, &request);
            if result.exit_code == Some(0)
                && matches!(
                    request.kind.as_str(),
                    "project_lifecycle_disable" | "project_lifecycle_unregister"
                )
            {
                if let Some(project_id) = lifecycle_project_id(&request) {
                    let runtime_project_id = format!("agent:{}:{}", request.client_id, project_id);
                    persistent_shells
                        .close_project(&runtime_project_id, "project_execution_disabled");
                }
            }
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        kind if is_lsp_request_kind(kind) => {
            // Explicit LSP branch — must never fall through to shell execution.
            let request_id = request.request_id.clone();
            let result = handle_lsp_request(policy, projects_dir, lsp, &request);
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        kind if is_validation_request_kind(kind) => {
            // Explicit validation bridge branch — never fall through to shell.
            let request_id = request.request_id.clone();
            let result = handle_validation_request(
                policy,
                projects_dir,
                &request,
                Some(runtime.shutdown_flag()),
            );
            sink.submit_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
        _ => {
            let request_id = request.request_id.clone();
            let result = match (ssh_resource, ssh_session_id) {
                (Some(resource), Some(session_id)) => run_ssh_shell_with_execution_state(
                    &jobs.ssh_pool,
                    config.generation,
                    &config.ssh,
                    policy,
                    resource,
                    session_id,
                    request.cwd.as_deref(),
                    &request.command,
                    request.stdin.as_deref(),
                    request.timeout_secs,
                    Some(runtime.shutdown_flag()),
                    request.sandbox.as_deref(),
                ),
                (Some(_), None) => ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(0),
                    error: Some(
                        "ssh_session_required: an SSH resource requires a Workflow Session id; command was not started".to_string(),
                    ),
                }),
                (None, _) => run_shell_with_profiles_in_sandbox_and_execution_state(
                    config.generation,
                    policy,
                    shell,
                    projects_dir,
                    &jobs.prepared_profiles,
                    request.cwd.as_deref(),
                    &request.command,
                    request.stdin.as_deref(),
                    request.timeout_secs,
                    Some(runtime.shutdown_flag()),
                    request.sandbox.as_deref(),
                ),
            };
            sink.submit_shell_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
    }
}

fn lifecycle_project_id(request: &ShellAgentShellRequest) -> Option<String> {
    request
        .stdin
        .as_deref()
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
        .and_then(|payload| {
            payload
                .get("project_id")
                .and_then(|project_id| project_id.as_str())
                .map(str::to_string)
        })
}

pub(crate) fn is_project_op(kind: &str) -> bool {
    matches!(
        kind,
        "register_project"
            | "create_project"
            | "resolve_or_register_project"
            | "project_lifecycle_enable"
            | "project_lifecycle_disable"
            | "project_lifecycle_unregister"
    )
}
