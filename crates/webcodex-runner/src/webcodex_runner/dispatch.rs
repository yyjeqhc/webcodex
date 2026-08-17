use super::external_tools::ExternalRoute;
use super::lsp::{handle_lsp_request, is_lsp_request_kind, LspSupervisor};
use super::transport::ResultSubmission;
use super::validation::{handle_validation_request, is_validation_request_kind};
use super::{
    handle_computer_request, handle_project_lifecycle_op,
    handle_project_op_with_temporary_projects_root, handle_resolve_or_register_project,
    is_computer_request_kind,
    run_internal_posix_script_with_profiles_in_sandbox_and_execution_state,
    run_internal_search_script_with_profiles_in_sandbox_and_execution_state,
    run_process_with_profiles_in_sandbox_and_execution_state,
    run_script_with_profiles_in_sandbox_and_execution_state,
    run_shell_with_profiles_in_sandbox_and_execution_state, run_ssh_shell_with_execution_state,
    AgentSink, CommandResult, HotAgentConfig, PersistentShellManager, ReloadableAgentConfig,
    ShellCommandResult, SubmitResultError,
};
use crate::shell_protocol::{
    validate_process_argv, validate_raw_shell_wire_command, validate_script_request,
    ShellAgentShellRequest, ShellProcessArgv, ShellScriptPayload, EXTERNAL_SEARCH_REQUEST_PREFIX,
    PROCESS_CWD_MAX_BYTES, PROCESS_STDIN_MAX_BYTES,
    STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS,
};
use crate::{handle_file_request, is_file_request_kind, JobManager, PendingJobStart};
use std::path::Path;
use std::sync::atomic::Ordering;

fn internal_search_script(command: &str) -> Option<&str> {
    let rest = command.strip_prefix(EXTERNAL_SEARCH_REQUEST_PREFIX)?;
    let script = rest.strip_prefix('\n')?;
    (!script.is_empty()).then_some(script)
}

#[allow(clippy::too_many_arguments)]
fn run_native_shell_or_internal_search(
    config: &HotAgentConfig,
    runtime: &ReloadableAgentConfig,
    jobs: &JobManager,
    projects_dir: &Path,
    request: &ShellAgentShellRequest,
) -> ShellCommandResult {
    if request.command.lines().next() == Some(EXTERNAL_SEARCH_REQUEST_PREFIX) {
        let Some(script) = internal_search_script(&request.command) else {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(
                    "invalid_internal_search_request: generated search script is missing; command was not started"
                        .to_string(),
                ),
            });
        };
        return run_internal_search_script_with_profiles_in_sandbox_and_execution_state(
            config.generation,
            &config.policy,
            &config.shell,
            projects_dir,
            &jobs.prepared_profiles,
            request.cwd.as_deref(),
            script,
            request.timeout_secs,
            Some(runtime.shutdown_flag()),
            request.sandbox.as_deref(),
        );
    }
    run_shell_with_profiles_in_sandbox_and_execution_state(
        config.generation,
        &config.policy,
        &config.shell,
        projects_dir,
        &jobs.prepared_profiles,
        request.cwd.as_deref(),
        &request.command,
        request.stdin.as_deref(),
        request.timeout_secs,
        Some(runtime.shutdown_flag()),
        request.sandbox.as_deref(),
    )
}

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
    // Computer operations are an explicit typed protocol surface. Unknown
    // computer_* requests must never reach external providers or shell fallback.
    if request.kind.starts_with("computer_") && !is_computer_request_kind(&request.kind) {
        let result = CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(
                "invalid_request: unsupported computer request kind; command was not started"
                    .to_string(),
            ),
        };
        return sink
            .submit_result_with_metadata(request.request_id, result, config, runtime)
            .map(|_| true);
    }
    if is_computer_request_kind(&request.kind) {
        let request_id = request.request_id.clone();
        let result = handle_computer_request(&request);
        return sink
            .submit_result_with_metadata(request_id, result, config, runtime)
            .map(|_| true);
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
    if request.kind == "run_process" {
        let request_id = request.request_id.clone();
        let result = if ssh_resource.is_some() {
            ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(
                    "structured_process_ssh_unsupported: native argv is unavailable for SSH resources; command was not started"
                        .to_string(),
                ),
            })
        } else {
            match validate_run_process_request(&request) {
                Ok(process) => run_process_with_profiles_in_sandbox_and_execution_state(
                    config.generation,
                    policy,
                    shell,
                    projects_dir,
                    &jobs.prepared_profiles,
                    request.cwd.as_deref(),
                    &process.executable,
                    &process.args,
                    request.stdin.as_deref(),
                    request.timeout_secs,
                    Some(runtime.shutdown_flag()),
                    request.sandbox.as_deref(),
                ),
                Err(error) => ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(0),
                    error: Some(format!(
                        "invalid_structured_process_request: {error}; command was not started"
                    )),
                }),
            }
        };
        return sink
            .submit_shell_result_with_metadata(request_id, result, config, runtime)
            .map(|_| true);
    }
    if request.kind == "run_script" {
        let request_id = request.request_id.clone();
        let result = if ssh_resource.is_some() {
            ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(
                    "structured_script_ssh_unsupported: typed script payloads are unavailable for SSH resources; command was not started"
                        .to_string(),
                ),
            })
        } else {
            match validate_run_script_request(&request) {
                Ok(script) => run_script_with_profiles_in_sandbox_and_execution_state(
                    config.generation,
                    policy,
                    shell,
                    projects_dir,
                    &jobs.prepared_profiles,
                    request.cwd.as_deref(),
                    script,
                    request.stdin.as_deref(),
                    request.timeout_secs,
                    Some(runtime.shutdown_flag()),
                    request.sandbox.as_deref(),
                ),
                Err(error) => ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(0),
                    error: Some(format!(
                        "invalid_structured_script_request: {error}; command was not started"
                    )),
                }),
            }
        };
        return sink
            .submit_shell_result_with_metadata(request_id, result, config, runtime)
            .map(|_| true);
    }
    if request.kind == "run_internal_posix_script" {
        let request_id = request.request_id.clone();
        let result = if ssh_resource.is_some() {
            ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(
                    "internal_posix_script_ssh_unsupported: generated internal programs are unavailable for SSH resources; command was not started"
                        .to_string(),
                ),
            })
        } else {
            match validate_internal_posix_script_request(&request) {
                Ok(script) => {
                    run_internal_posix_script_with_profiles_in_sandbox_and_execution_state(
                        config.generation,
                        policy,
                        shell,
                        projects_dir,
                        &jobs.prepared_profiles,
                        request.cwd.as_deref(),
                        script,
                        request.timeout_secs,
                        Some(runtime.shutdown_flag()),
                        request.sandbox.as_deref(),
                    )
                }
                Err(error) => ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(0),
                    error: Some(format!(
                        "invalid_internal_posix_script_request: {error}; command was not started"
                    )),
                }),
            }
        };
        return sink
            .submit_shell_result_with_metadata(request_id, result, config, runtime)
            .map(|_| true);
    }
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
    if request.kind == "run_shell" {
        if let Err(error) = validate_raw_shell_wire_command(&request.command) {
            let result = ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(format!(
                    "invalid_raw_shell_request: {error}; command was not started"
                )),
            });
            return sink
                .submit_shell_result_with_metadata(request.request_id, result, config, runtime)
                .map(|_| true);
        }
    }

    // Inspect requests must stay on the native execution path where Landlock
    // is applied in pre_exec. External providers are not an equivalent local
    // filesystem write boundary.
    if ssh_resource.is_some()
        && !matches!(
            request.kind.as_str(),
            "run_shell"
                | "start_job"
                | "start_validation_job"
                | "start_process_job"
                | "start_script_job"
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
            let result =
                run_native_shell_or_internal_search(config, runtime, jobs, projects_dir, &request);
            external_tools.complete_native_fallback(fallback, &result.result);
            return sink
                .submit_shell_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true);
        }
        ExternalRoute::Native => {}
    }
    match request.kind.as_str() {
        "start_job" | "start_validation_job" | "start_process_job" | "start_script_job" => {
            jobs.enqueue(
                sink.clone(),
                PendingJobStart {
                    generation: config.generation,
                    policy: policy.clone(),
                    shell: shell.clone(),
                    ssh: config.ssh.clone(),
                    projects_dir: projects_dir.to_path_buf(),
                    request,
                },
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
                (None, _) => run_native_shell_or_internal_search(
                    config,
                    runtime,
                    jobs,
                    projects_dir,
                    &request,
                ),
            };
            sink.submit_shell_result_with_metadata(request_id, result, config, runtime)
                .map(|_| true)
        }
    }
}

fn validate_run_process_request(
    request: &ShellAgentShellRequest,
) -> Result<&ShellProcessArgv, String> {
    if request.job_id.is_some() {
        return Err("job_id is not supported by synchronous run_process".to_string());
    }
    if !request.command.is_empty() {
        return Err("command must be empty when process is present".to_string());
    }
    if request.script.is_some() {
        return Err("script must be absent when process is present".to_string());
    }
    let process = request
        .process
        .as_ref()
        .ok_or_else(|| "process payload is required".to_string())?;
    validate_process_argv(process)?;
    if let Some(stdin) = request.stdin.as_deref() {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = request.cwd.as_deref() {
        if cwd.len() > PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {PROCESS_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if request.timeout_secs == 0
        || request.timeout_secs > STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS
    {
        return Err(format!(
            "timeout_secs must be between 1 and {STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS}"
        ));
    }
    Ok(process)
}

fn validate_run_script_request(
    request: &ShellAgentShellRequest,
) -> Result<&ShellScriptPayload, String> {
    if request.job_id.is_some() {
        return Err("job_id is not supported by synchronous run_script".to_string());
    }
    if !request.command.is_empty() {
        return Err("command must be empty when script is present".to_string());
    }
    if request.process.is_some() {
        return Err("process must be absent when script is present".to_string());
    }
    let script = request
        .script
        .as_ref()
        .ok_or_else(|| "script payload is required".to_string())?;
    validate_script_request(
        script,
        request.stdin.as_deref(),
        request.cwd.as_deref(),
        request.timeout_secs,
    )?;
    if request.timeout_secs > STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS}"
        ));
    }
    Ok(script)
}

fn validate_internal_posix_script_request(
    request: &ShellAgentShellRequest,
) -> Result<&str, String> {
    let script = validate_run_script_request(request)?;
    if request.stdin.is_some() {
        return Err("stdin must be absent for an internal POSIX script".to_string());
    }
    if script.language != crate::shell_protocol::ShellScriptLanguage::Sh {
        return Err("internal POSIX script language must be sh".to_string());
    }
    if !script.args.is_empty() {
        return Err("internal POSIX script args must be empty".to_string());
    }
    Ok(&script.script)
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
