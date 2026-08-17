use serde_json::json;
use std::time::Duration;

use super::helpers::{
    bounded_tail, command_failed_message, command_outcome_unknown_message,
    command_rejected_message, command_timeout_message, looks_like_command_timeout,
    project_relative_agent_cwd, project_relative_cwd, resolve_agent_cwd, resolve_local_cwd,
    run_process_sync_bounded_with_sandbox, LocalRunFailure, COMMAND_STDIO_TAIL_CHARS,
};
use super::shell::{
    agent_command_lifecycle, command_execution_state_name, dispatch_uncertainty_lifecycle,
};
use super::structured_execution::{
    await_hidden_structured_job, HiddenStructuredJobWait, StructuredExecutionBudget,
};
use super::{ExecutionPurpose, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_client::{
    process_preview, ShellJobStartMetadata, ShellJobVisibility, StructuredJobExecution,
};
use crate::shell_protocol::{
    validate_process_argv, ShellCommandExecutionState, ShellJobInfo, ShellJobOpRequest,
    ShellProcessArgv, PROCESS_CWD_MAX_BYTES, PROCESS_STDIN_MAX_BYTES,
    STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS,
};

fn command_started(state: ShellCommandExecutionState) -> bool {
    !matches!(state, ShellCommandExecutionState::NotStarted)
}

fn command_completed(state: ShellCommandExecutionState) -> bool {
    matches!(state, ShellCommandExecutionState::Completed)
}

pub(crate) fn success_output(
    exit_code: i32,
    stdout: String,
    stderr: String,
    duration_ms: Option<u64>,
) -> serde_json::Value {
    let (stdout_tail, stdout_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
    let (stderr_tail, stderr_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
    json!({
        "exit_code": exit_code,
        "stdout_tail": stdout_tail,
        "stderr_tail": stderr_tail,
        "stdout_lines": stdout.lines().count(),
        "stderr_lines": stderr.lines().count(),
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "duration_ms": duration_ms,
        "command_started": true,
        "command_completed": true,
        "command_ok": true,
        "execution_state": command_execution_state_name(ShellCommandExecutionState::Completed),
        "failure_kind": null,
        "tool_failure": false,
    })
}

pub(crate) fn command_failure_result(
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    duration_ms: Option<u64>,
    timeout_secs: u64,
    state: ShellCommandExecutionState,
) -> ToolResult {
    let (stdout_tail, stdout_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
    let (stderr_tail, stderr_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
    let timed_out = state == ShellCommandExecutionState::TimedOut;
    let output = json!({
        "exit_code": exit_code,
        "duration_ms": duration_ms,
        "stdout_tail": stdout_tail,
        "stderr_tail": stderr_tail,
        "stdout_lines": stdout.lines().count(),
        "stderr_lines": stderr.lines().count(),
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "command_started": command_started(state),
        "command_completed": command_completed(state),
        "command_ok": false,
        "execution_state": command_execution_state_name(state),
        "failure_kind": if timed_out { "timeout" } else { "command_exit_nonzero" },
        "tool_failure": false,
    });
    ToolResult {
        success: false,
        error: Some(if timed_out {
            command_timeout_message(timeout_secs, &stdout_tail, &stderr_tail)
        } else {
            command_failed_message(exit_code, &stdout_tail, &stderr_tail)
        }),
        output,
    }
}

pub(crate) fn process_tool_failure_result(
    message: impl Into<String>,
    failure_kind: &'static str,
    state: ShellCommandExecutionState,
) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "command_started": command_started(state),
            "command_completed": command_completed(state),
            "command_ok": false,
            "exit_code": null,
            "execution_state": command_execution_state_name(state),
            "failure_kind": failure_kind,
            "tool_failure": true,
        }),
    )
}

pub(crate) fn outcome_unknown_result(reason: impl AsRef<str>) -> ToolResult {
    process_tool_failure_result(
        command_outcome_unknown_message(reason),
        "outcome_unknown",
        ShellCommandExecutionState::OutcomeUnknown,
    )
}

pub(crate) fn add_structured_continuation_facts(
    result: &mut ToolResult,
    effective_timeout_secs: u64,
    sync_wait_secs: u64,
    async_handoff_available: bool,
) {
    let terminal = result
        .output
        .get("execution_state")
        .and_then(serde_json::Value::as_str)
        != Some("outcome_unknown");
    result.output["promoted_to_job"] = json!(false);
    result.output["terminal"] = json!(terminal);
    result.output["job_id"] = serde_json::Value::Null;
    result.output["job_status"] = serde_json::Value::Null;
    result.output["effective_timeout_secs"] = json!(effective_timeout_secs);
    result.output["sync_wait_secs"] = json!(sync_wait_secs);
    result.output["async_handoff_available"] = json!(async_handoff_available);
}

pub(crate) fn terminal_structured_job_result(
    job: &ShellJobInfo,
    stdout: String,
    stderr: String,
    timeout_secs: u64,
) -> ToolResult {
    let state = job
        .command_execution_state
        .unwrap_or(ShellCommandExecutionState::OutcomeUnknown);
    let mut result = match state {
        ShellCommandExecutionState::NotStarted => {
            let reason = job
                .error
                .as_deref()
                .unwrap_or("Runner rejected the structured execution before child spawn");
            process_tool_failure_result(
                command_rejected_message(
                    reason,
                    "inspect the rejection, correct the typed request, then retry.",
                ),
                classify_process_failure(reason),
                state,
            )
        }
        ShellCommandExecutionState::OutcomeUnknown => outcome_unknown_result(
            job.error
                .as_deref()
                .unwrap_or("the Runner lost a trustworthy terminal process result"),
        ),
        ShellCommandExecutionState::TimedOut => command_failure_result(
            job.exit_code,
            stdout,
            stderr,
            job.duration_ms,
            timeout_secs,
            state,
        ),
        ShellCommandExecutionState::Completed
            if job.status == "completed" && job.exit_code == Some(0) && job.error.is_none() =>
        {
            ToolResult::ok(success_output(0, stdout, stderr, job.duration_ms))
        }
        ShellCommandExecutionState::Completed => command_failure_result(
            job.exit_code,
            stdout,
            stderr,
            job.duration_ms,
            timeout_secs,
            state,
        ),
    };
    if job.stdout_log_truncated {
        result.output["stdout_truncated"] = json!(true);
    }
    if job.stderr_log_truncated {
        result.output["stderr_truncated"] = json!(true);
    }
    result
}

pub(crate) fn classify_process_failure(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("interpreter_unavailable") || lower.contains("interpreter is unavailable") {
        "interpreter_unavailable"
    } else if lower.contains("script_setup_failed")
        || lower.contains("temporary script setup failed")
    {
        "script_setup_failed"
    } else if lower.contains("invalid_structured_script_request") {
        "invalid_arguments"
    } else if lower.contains("unsupported_executable_type") {
        "unsupported_executable_type"
    } else if lower.contains("capability_unavailable")
        || lower.contains("structured_process_argv")
        || lower.contains("does not support")
    {
        "capability_unavailable"
    } else if lower.contains("failed to spawn")
        || lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("cannot find")
        || lower.contains("executable is unavailable")
    {
        "spawn_failed"
    } else if lower.contains("offline")
        || lower.contains("not connected")
        || lower.contains("no connected")
        || lower.contains("unknown agent")
        || lower.contains("unknown_project")
    {
        "agent_offline"
    } else if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("outside")
        || lower.contains("not allowed")
        || lower.contains("sandbox")
    {
        "permission_denied"
    } else {
        "runtime_error"
    }
}

fn validate_process_input(
    process: &ShellProcessArgv,
    stdin: Option<&str>,
    cwd: Option<&str>,
) -> Result<(), String> {
    validate_process_argv(process)?;
    if let Some(stdin) = stdin {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {PROCESS_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    Ok(())
}

pub(crate) fn local_error_state(
    exit_code: i32,
    stderr: &str,
) -> Option<ShellCommandExecutionState> {
    if exit_code != -1 {
        return None;
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.starts_with("failed to execute process:")
        || lower.starts_with("failed to configure inspect sandbox:")
        || lower.starts_with("interpreter_unavailable:")
        || lower.starts_with("script_setup_failed:")
    {
        Some(ShellCommandExecutionState::NotStarted)
    } else if lower.starts_with("failed to wait for process:")
        || lower.starts_with("failed to collect process output:")
        || lower.starts_with("failed to write process stdin:")
    {
        Some(ShellCommandExecutionState::OutcomeUnknown)
    } else {
        None
    }
}

fn decorate(
    output: &mut serde_json::Value,
    purpose: ExecutionPurpose,
    summary: &str,
    cwd: &str,
    executor: &str,
) {
    output["execution_source"] = json!("run_process");
    output["purpose"] = json!(purpose.as_str());
    output["process_summary"] = json!(summary);
    output["cwd"] = json!(cwd);
    output["executor"] = json!(executor);
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_process_with_contract_in_sandbox(
        &self,
        project: String,
        executable: String,
        args: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        sandbox: Option<&str>,
        ssh_resource: Option<&str>,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.run_process_with_contract_mode(
            project,
            executable,
            args,
            stdin,
            timeout_secs,
            cwd,
            purpose,
            sandbox,
            ssh_resource,
            session_id,
            auth,
            true,
        )
        .await
    }

    /// Execute one server-owned fixed process synchronously without exposing the
    /// model-facing structured-execution Job handoff. Effectful internal tools
    /// must not report success while their fixed mutation is still running.
    pub(super) async fn run_internal_process_sync(
        &self,
        project: String,
        executable: String,
        args: Vec<String>,
        timeout_secs: u64,
    ) -> ToolResult {
        self.run_process_with_contract_mode(
            project,
            executable,
            args,
            None,
            Some(timeout_secs),
            None,
            Some(ExecutionPurpose::Operation),
            None,
            None,
            None,
            None,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_process_with_contract_mode(
        &self,
        project: String,
        executable: String,
        args: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        sandbox: Option<&str>,
        ssh_resource: Option<&str>,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
        allow_async_handoff: bool,
    ) -> ToolResult {
        let budget = match StructuredExecutionBudget::resolve(timeout_secs) {
            Ok(budget) => budget,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        format!("run_process {error}"),
                        "pass timeout_secs between 1 and 3600, or omit it for the default of 60 seconds.",
                    ),
                    "invalid_arguments",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let timeout = budget.effective_timeout_secs;
        let process = ShellProcessArgv { executable, args };
        if let Err(error) = validate_process_input(&process, stdin.as_deref(), cwd.as_deref()) {
            return process_tool_failure_result(
                command_rejected_message(
                    error,
                    "correct the structured process fields and retry; use run_shell only when shell syntax is required.",
                ),
                "invalid_arguments",
                ShellCommandExecutionState::NotStarted,
            );
        }
        if ssh_resource.is_some() {
            return process_tool_failure_result(
                command_rejected_message(
                    "named Session SSH resources do not support native structured argv",
                    "use run_shell explicitly for this SSH resource, or run_process against the Runner-host project.",
                ),
                "unsupported_resource",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let summary = process_preview(&process.executable, process.args.iter().map(String::as_str));
        let declared_purpose = purpose.unwrap_or_default();
        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        error.to_message(),
                        "verify the project id with list_projects, then retry with a registered project.",
                    ),
                    "agent_offline",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        if proj.is_agent() {
            let client_id =
                match proj.agent_client_id() {
                    Ok(client_id) => client_id.to_string(),
                    Err(error) => return process_tool_failure_result(
                        command_rejected_message(
                            error,
                            "refresh the agent project registry with list_projects, then retry.",
                        ),
                        "agent_offline",
                        ShellCommandExecutionState::NotStarted,
                    ),
                };
            let effective_cwd = match resolve_agent_cwd(&proj, cwd.as_deref()) {
                Ok(cwd) => cwd,
                Err(error) => {
                    return process_tool_failure_result(
                        command_rejected_message(
                            error,
                            "choose '.', an existing project-relative cwd, or an absolute path inside the registered project root.",
                        ),
                        "permission_denied",
                        ShellCommandExecutionState::NotStarted,
                    )
                }
            };
            let resolved_cwd = project_relative_agent_cwd(&proj, &effective_cwd)
                .unwrap_or_else(|_| ".".to_string());
            let capabilities = match self.shell_clients.get_client_capabilities(&client_id).await {
                Ok(capabilities) => capabilities,
                Err(error) => {
                    let mut result = process_tool_failure_result(
                        command_rejected_message(
                            error.to_string(),
                            "confirm the Runner is registered and connected, then retry.",
                        ),
                        "agent_offline",
                        ShellCommandExecutionState::NotStarted,
                    );
                    decorate(
                        &mut result.output,
                        declared_purpose,
                        &summary,
                        &resolved_cwd,
                        "agent",
                    );
                    add_structured_continuation_facts(
                        &mut result,
                        timeout,
                        timeout.min(STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS),
                        false,
                    );
                    return result;
                }
            };
            let async_handoff_available = allow_async_handoff
                && capabilities.structured_execution_jobs
                && (capabilities.async_jobs || capabilities.async_shell_jobs);
            if !async_handoff_available
                && timeout > STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS
            {
                let mut result = process_tool_failure_result(
                    command_rejected_message(
                        "capability_unavailable: this Runner does not support durable typed structured execution Jobs",
                        "upgrade the Runner to one advertising structured_execution_jobs, or request timeout_secs at most 120 seconds.",
                    ),
                    "capability_unavailable",
                    ShellCommandExecutionState::NotStarted,
                );
                decorate(
                    &mut result.output,
                    declared_purpose,
                    &summary,
                    &resolved_cwd,
                    "agent",
                );
                add_structured_continuation_facts(
                    &mut result,
                    timeout,
                    STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS,
                    false,
                );
                return result;
            }
            if async_handoff_available && timeout > budget.sync_wait_secs {
                let job = self
                    .shell_clients
                    .start_job_with_metadata_for_auth(
                        ShellJobOpRequest {
                            op: "start".to_string(),
                            client_id: Some(client_id),
                            cwd: Some(effective_cwd),
                            command: Some(String::new()),
                            timeout_secs: Some(timeout),
                            job_id: None,
                            since_stdout_line: None,
                            since_stderr_line: None,
                            tail_lines: None,
                            limit: None,
                            codex: None,
                        },
                        "tool_runtime".to_string(),
                        ShellJobStartMetadata {
                            project_id: Some(project.clone()),
                            session_id,
                            project_cwd: Some(resolved_cwd.clone()),
                            purpose: Some(declared_purpose.as_str().to_string()),
                            shell: Some("direct_argv".to_string()),
                            visibility: ShellJobVisibility::HiddenUntilHandoff,
                            sandbox: sandbox.map(str::to_string),
                            structured_execution: Some(StructuredJobExecution::Process(process)),
                            stdin,
                            ..Default::default()
                        },
                        auth,
                    )
                    .await;
                let job = match job {
                    Ok(job) => job,
                    Err(error) => {
                        let mut result = process_tool_failure_result(
                            command_rejected_message(
                                &error,
                                "confirm the Runner is connected and advertises structured_execution_jobs, then retry only if target state proves no process started.",
                            ),
                            classify_process_failure(&error),
                            ShellCommandExecutionState::NotStarted,
                        );
                        decorate(
                            &mut result.output,
                            declared_purpose,
                            &summary,
                            &resolved_cwd,
                            "agent",
                        );
                        add_structured_continuation_facts(
                            &mut result,
                            timeout,
                            budget.sync_wait_secs,
                            true,
                        );
                        return result;
                    }
                };
                let wait = self
                    .structured_execution_sync_wait
                    .min(Duration::from_secs(budget.sync_wait_secs));
                let handoff = await_hidden_structured_job(
                    self.shell_clients.clone(),
                    job.job_id.clone(),
                    wait,
                    auth.cloned(),
                )
                .await;
                let mut result = match handoff {
                    Ok(HiddenStructuredJobWait::Terminal {
                        job,
                        stdout,
                        stderr,
                    }) => {
                        let result = terminal_structured_job_result(&job, stdout, stderr, timeout);
                        self.shell_clients
                            .remove_projected_hidden_structured_job_record(&job.job_id)
                            .await;
                        result
                    }
                    Ok(HiddenStructuredJobWait::Continued {
                        job,
                        execution_state,
                        command_started,
                    }) => ToolResult::ok(json!({
                        "execution_state": execution_state,
                        "command_started": command_started,
                        "command_completed": false,
                        "command_ok": false,
                        "exit_code": null,
                        "failure_kind": null,
                        "tool_failure": false,
                        "promoted_to_job": true,
                        "terminal": false,
                        "job_id": job.job_id,
                        "job_status": job.status,
                        "observation_token": job.observation_token,
                        "effective_timeout_secs": timeout,
                        "sync_wait_secs": budget.sync_wait_secs,
                        "async_handoff_available": true,
                        "stdout_tail": "",
                        "stderr_tail": "",
                        "stdout_lines": 0,
                        "stderr_lines": 0,
                        "stdout_truncated": false,
                        "stderr_truncated": false,
                    })),
                    Err(error) => outcome_unknown_result(format!(
                        "the durable process Job could not be observed during handoff: {error}"
                    )),
                };
                if result.output["promoted_to_job"] != json!(true) {
                    add_structured_continuation_facts(
                        &mut result,
                        timeout,
                        budget.sync_wait_secs,
                        true,
                    );
                }
                decorate(
                    &mut result.output,
                    declared_purpose,
                    &summary,
                    &resolved_cwd,
                    "agent",
                );
                return result;
            }
            let wait_timeout = timeout;
            let (request_id, receiver) = match self
                .shell_clients
                .enqueue_process_with_sandbox(
                    client_id,
                    Some(effective_cwd),
                    process,
                    stdin,
                    timeout,
                    wait_timeout,
                    "tool_runtime".to_string(),
                    sandbox.map(str::to_string),
                )
                .await
            {
                Ok(enqueued) => enqueued,
                Err(error) => {
                    return process_tool_failure_result(
                        command_rejected_message(
                            &error,
                            "confirm the Runner is connected and advertises structured_process_argv, then retry only if target state proves no process started.",
                        ),
                        classify_process_failure(&error),
                        ShellCommandExecutionState::NotStarted,
                    )
                }
            };
            let mut result = match tokio::time::timeout(
                Duration::from_secs(wait_timeout + 2),
                receiver,
            )
            .await
            {
                Ok(Ok(response)) => {
                    let state = agent_command_lifecycle(&response, timeout);
                    let exit_code = response.exit_code;
                    let stdout = response.stdout.unwrap_or_default();
                    let stderr = response.stderr.unwrap_or_default();
                    match state {
                        ShellCommandExecutionState::NotStarted => {
                            let reason = response
                                .error
                                .as_deref()
                                .unwrap_or("Runner rejected the process before spawn");
                            process_tool_failure_result(
                                    command_rejected_message(
                                        reason,
                                        "inspect the rejection, correct executable/argv/cwd, then retry.",
                                    ),
                                    classify_process_failure(reason),
                                    state,
                                )
                        }
                        ShellCommandExecutionState::OutcomeUnknown => {
                            outcome_unknown_result(response.error.as_deref().unwrap_or(
                                "the Runner did not return a trustworthy terminal result",
                            ))
                        }
                        ShellCommandExecutionState::TimedOut => command_failure_result(
                            exit_code,
                            stdout,
                            stderr,
                            response.duration_ms,
                            timeout,
                            state,
                        ),
                        ShellCommandExecutionState::Completed
                            if response.error.is_none() && exit_code == Some(0) =>
                        {
                            ToolResult::ok(success_output(0, stdout, stderr, response.duration_ms))
                        }
                        ShellCommandExecutionState::Completed => command_failure_result(
                            exit_code,
                            stdout,
                            stderr,
                            response.duration_ms,
                            timeout,
                            state,
                        ),
                    }
                }
                Ok(Err(_)) => {
                    let dispatch = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    if dispatch == Some(false) {
                        process_tool_failure_result(
                            command_rejected_message(
                                "process request waiter was dropped before Runner dispatch",
                                "check Runner connectivity, then retry.",
                            ),
                            "runtime_error",
                            ShellCommandExecutionState::NotStarted,
                        )
                    } else {
                        outcome_unknown_result(
                            "process request waiter was dropped after dispatch may have occurred",
                        )
                    }
                }
                Err(_) => {
                    let dispatch = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    let state = dispatch_uncertainty_lifecycle(dispatch);
                    if state == ShellCommandExecutionState::NotStarted {
                        process_tool_failure_result(
                                command_rejected_message(
                                    format!(
                                        "timed out waiting {wait_timeout} seconds before Runner dispatch"
                                    ),
                                    "check Runner connectivity and availability, then retry.",
                                ),
                                "timeout",
                                state,
                            )
                    } else {
                        outcome_unknown_result(format!(
                                "timed out waiting {wait_timeout} seconds for the Runner process result"
                            ))
                    }
                }
            };
            decorate(
                &mut result.output,
                declared_purpose,
                &summary,
                &resolved_cwd,
                "agent",
            );
            add_structured_continuation_facts(
                &mut result,
                timeout,
                if async_handoff_available {
                    budget.sync_wait_secs
                } else {
                    timeout
                },
                async_handoff_available,
            );
            result
        } else {
            if timeout > STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS {
                let mut result = process_tool_failure_result(
                    command_rejected_message(
                        "capability_unavailable: the server-local compatibility executor has no durable typed Job handoff",
                        "use an Agent-owned project with structured_execution_jobs, or request timeout_secs at most 120 seconds.",
                    ),
                    "capability_unavailable",
                    ShellCommandExecutionState::NotStarted,
                );
                decorate(&mut result.output, declared_purpose, &summary, ".", "local");
                add_structured_continuation_facts(
                    &mut result,
                    timeout,
                    STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS,
                    false,
                );
                return result;
            }
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(cwd) => cwd,
                Err(error) => {
                    return process_tool_failure_result(
                        command_rejected_message(
                            error,
                            "choose an existing project-relative cwd inside the project.",
                        ),
                        "permission_denied",
                        ShellCommandExecutionState::NotStarted,
                    )
                }
            };
            let resolved_cwd =
                project_relative_cwd(&proj, &cwd_path).unwrap_or_else(|_| ".".to_string());
            let mut result = match run_process_sync_bounded_with_sandbox(
                process.executable,
                process.args,
                stdin,
                cwd_path,
                timeout,
                sandbox.map(str::to_string),
            )
            .await
            {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    match local_error_state(exit_code, &stderr) {
                        Some(ShellCommandExecutionState::NotStarted) => {
                            process_tool_failure_result(
                                command_rejected_message(
                                    &stderr,
                                    "correct the executable, cwd, or sandbox configuration, then retry.",
                                ),
                                classify_process_failure(&stderr),
                                ShellCommandExecutionState::NotStarted,
                            )
                        }
                        Some(ShellCommandExecutionState::OutcomeUnknown) => {
                            outcome_unknown_result(&stderr)
                        }
                        _ if exit_code == 0 => ToolResult::ok(success_output(
                            exit_code,
                            stdout,
                            stderr,
                            Some(duration_ms),
                        )),
                        _ => {
                            let state =
                                if looks_like_command_timeout(Some(exit_code), &stderr, timeout) {
                                    ShellCommandExecutionState::TimedOut
                                } else {
                                    ShellCommandExecutionState::Completed
                                };
                            command_failure_result(
                                Some(exit_code),
                                stdout,
                                stderr,
                                Some(duration_ms),
                                timeout,
                                state,
                            )
                        }
                    }
                }
                Err(LocalRunFailure::HardTimeout { bound_secs }) => {
                    outcome_unknown_result(format!(
                    "the local process did not return within the {bound_secs}-second hard bound"
                ))
                }
                Err(LocalRunFailure::Join(error)) => outcome_unknown_result(format!(
                    "the local process worker ended without returning a result: {error}"
                )),
            };
            decorate(
                &mut result.output,
                declared_purpose,
                &summary,
                &resolved_cwd,
                "local",
            );
            add_structured_continuation_facts(&mut result, timeout, timeout, false);
            result
        }
    }
}
