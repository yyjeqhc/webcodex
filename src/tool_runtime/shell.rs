use serde_json::json;
use std::time::Duration;

use super::helpers::{
    bounded_tail, command_failed_message, command_outcome_unknown_message,
    command_rejected_message, command_timeout_message, explicit_shell_dispatch_command,
    looks_like_command_timeout, project_relative_agent_cwd, project_relative_cwd,
    resolve_agent_cwd, resolve_local_cwd, resolve_sync_timeout_secs,
    run_command_sync_bounded_with_shell_and_sandbox, sync_timeout_out_of_range_result,
    validate_raw_shell_command_length, LocalRunFailure, COMMAND_STDIO_TAIL_CHARS,
    DEFAULT_RUN_SHELL_TIMEOUT_SECS, MAX_SYNC_TIMEOUT_SECS, MIN_SYNC_TIMEOUT_SECS,
};
use super::process::add_structured_continuation_facts;
use super::structured_execution::{
    await_hidden_structured_job, HiddenStructuredJobWait, STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
};
use super::tool_result::ToolResult;
use super::{ExecutionPurpose, ExecutionShell, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_client::{command_preview, ShellJobStartMetadata, ShellJobVisibility};
use crate::shell_protocol::{
    ShellCommandExecutionState, ShellJobInfo, ShellJobOpRequest, ShellRunRequest, ShellRunResponse,
};

pub(crate) struct ProjectCommandOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration_ms: u64,
    pub(crate) error: Option<String>,
    pub(crate) execution_state: ShellCommandExecutionState,
}

fn command_started(execution_state: ShellCommandExecutionState) -> bool {
    !matches!(execution_state, ShellCommandExecutionState::NotStarted)
}

fn command_completed(execution_state: ShellCommandExecutionState) -> bool {
    matches!(execution_state, ShellCommandExecutionState::Completed)
}

pub(crate) fn command_execution_state_name(
    execution_state: ShellCommandExecutionState,
) -> &'static str {
    match execution_state {
        ShellCommandExecutionState::NotStarted => "not_started",
        ShellCommandExecutionState::OutcomeUnknown => "outcome_unknown",
        ShellCommandExecutionState::Completed => "completed",
        ShellCommandExecutionState::TimedOut => "timed_out",
    }
}

pub(crate) fn agent_command_lifecycle(
    response: &ShellRunResponse,
    timeout_secs: u64,
) -> ShellCommandExecutionState {
    match response.command_execution_state {
        Some(execution_state) => execution_state,
        None if response.request_dispatched == Some(false) => {
            ShellCommandExecutionState::NotStarted
        }
        None if looks_like_command_timeout(
            response.exit_code,
            response.stderr.as_deref().unwrap_or_default(),
            timeout_secs,
        ) =>
        {
            ShellCommandExecutionState::TimedOut
        }
        None if response.error.is_none() && response.exit_code.is_some() => {
            ShellCommandExecutionState::Completed
        }
        None => ShellCommandExecutionState::OutcomeUnknown,
    }
}

fn local_command_error_lifecycle(
    exit_code: i32,
    stderr: &str,
) -> Option<ShellCommandExecutionState> {
    if exit_code != -1 {
        return None;
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("failed to collect output")
        || lower.starts_with("failed to collect command output:")
    {
        return Some(ShellCommandExecutionState::OutcomeUnknown);
    }
    if stderr.starts_with("Failed to configure inspect sandbox:")
        || stderr.starts_with("Failed to execute command:")
    {
        return Some(ShellCommandExecutionState::NotStarted);
    }
    if stderr.starts_with("Failed to wait for command:") {
        return Some(ShellCommandExecutionState::OutcomeUnknown);
    }
    None
}

pub(crate) fn dispatch_uncertainty_lifecycle(
    request_dispatched: Option<bool>,
) -> ShellCommandExecutionState {
    if request_dispatched == Some(false) {
        ShellCommandExecutionState::NotStarted
    } else {
        ShellCommandExecutionState::OutcomeUnknown
    }
}

impl ToolRuntime {
    fn run_shell_success_output(
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
            "command_started": command_started(ShellCommandExecutionState::Completed),
            "command_completed": command_completed(ShellCommandExecutionState::Completed),
            "command_ok": true,
            "execution_state": command_execution_state_name(ShellCommandExecutionState::Completed),
            "failure_kind": null,
            "tool_failure": false,
        })
    }

    fn run_shell_command_failure_result(
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
        timeout_secs: u64,
        execution_state: ShellCommandExecutionState,
    ) -> ToolResult {
        let (stdout_tail, stdout_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
        let (stderr_tail, stderr_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
        let timed_out = execution_state == ShellCommandExecutionState::TimedOut;
        let output = json!({
            "exit_code": exit_code,
            "duration_ms": duration_ms,
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_lines": stdout.lines().count(),
            "stderr_lines": stderr.lines().count(),
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
            "command_started": command_started(execution_state),
            "command_completed": command_completed(execution_state),
            "command_ok": false,
            "execution_state": command_execution_state_name(execution_state),
            "failure_kind": if timed_out { "timeout" } else { "command_exit_nonzero" },
            "tool_failure": false,
        });
        let error = if timed_out {
            command_timeout_message(timeout_secs, &stdout_tail, &stderr_tail)
        } else {
            command_failed_message(exit_code, &stdout_tail, &stderr_tail)
        };
        ToolResult {
            success: false,
            output,
            error: Some(error),
        }
    }

    fn run_shell_tool_failure_result(
        message: String,
        failure_kind: &'static str,
        execution_state: ShellCommandExecutionState,
    ) -> ToolResult {
        ToolResult::err_with_output(
            message,
            json!({
                "command_started": command_started(execution_state),
                "command_completed": command_completed(execution_state),
                "command_ok": false,
                "exit_code": null,
                "execution_state": command_execution_state_name(execution_state),
                "failure_kind": failure_kind,
                "tool_failure": true,
            }),
        )
    }

    fn run_shell_outcome_unknown_result(reason: impl AsRef<str>) -> ToolResult {
        Self::run_shell_tool_failure_result(
            command_outcome_unknown_message(reason),
            "outcome_unknown",
            ShellCommandExecutionState::OutcomeUnknown,
        )
    }

    fn classify_run_shell_enqueue_failure(message: &str) -> &'static str {
        let lower = message.to_ascii_lowercase();
        if lower.contains("offline")
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
        {
            "permission_denied"
        } else if lower.contains("timeout") || lower.contains("timed out") {
            "timeout"
        } else {
            "runtime_error"
        }
    }

    pub(crate) async fn run_project_command_capture(
        &self,
        project: &str,
        command: String,
        timeout_secs: u64,
        cwd: Option<String>,
    ) -> Result<ProjectCommandOutput, String> {
        self.run_project_command_capture_with_sandbox(project, command, timeout_secs, cwd, None)
            .await
    }

    pub(crate) async fn run_project_command_capture_with_sandbox(
        &self,
        project: &str,
        command: String,
        timeout_secs: u64,
        cwd: Option<String>,
        sandbox: Option<&str>,
    ) -> Result<ProjectCommandOutput, String> {
        self.run_project_command_capture_impl(project, command, timeout_secs, cwd, sandbox, false)
            .await
    }

    pub(crate) async fn run_project_internal_posix_script_capture(
        &self,
        project: &str,
        script: String,
        timeout_secs: u64,
        cwd: Option<String>,
    ) -> Result<ProjectCommandOutput, String> {
        self.run_project_command_capture_impl(project, script, timeout_secs, cwd, None, true)
            .await
    }

    async fn run_project_command_capture_impl(
        &self,
        project: &str,
        command: String,
        timeout_secs: u64,
        cwd: Option<String>,
        sandbox: Option<&str>,
        internal_posix_script: bool,
    ) -> Result<ProjectCommandOutput, String> {
        let proj = self.resolve_project(project).await?;
        // Shared root of the sync agent-wait contract: wait_timeout_secs and
        // command timeout must both stay within 1..=120 before enqueue so
        // shell_client validation never rejects with implementation-detail
        // errors about runShell.
        if !(MIN_SYNC_TIMEOUT_SECS..=MAX_SYNC_TIMEOUT_SECS).contains(&timeout_secs) {
            return Err(format!(
                "timeout_secs must be between {MIN_SYNC_TIMEOUT_SECS} and {MAX_SYNC_TIMEOUT_SECS}"
            ));
        }
        let timeout = timeout_secs;
        if proj.is_agent() {
            let client_id = proj.agent_client_id()?.to_string();
            let effective_cwd = Some(resolve_agent_cwd(&proj, cwd.as_deref())?);
            let wait_timeout = timeout;
            let (request_id, rx) = if internal_posix_script {
                self.shell_clients
                    .enqueue_internal_posix_script(
                        client_id,
                        effective_cwd,
                        command,
                        timeout,
                        wait_timeout,
                        "tool_runtime".to_string(),
                        sandbox.map(str::to_string),
                    )
                    .await?
            } else {
                self.shell_clients
                    .enqueue_run_with_sandbox(
                        ShellRunRequest {
                            client_id,
                            cwd: effective_cwd,
                            command,
                            stdin: None,
                            timeout_secs: timeout,
                            wait_timeout_secs: wait_timeout,
                        },
                        "tool_runtime".to_string(),
                        sandbox.map(str::to_string),
                    )
                    .await?
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let execution_state = agent_command_lifecycle(&response, timeout);
                    let exit_code = response.exit_code;
                    let stderr = response.stderr.unwrap_or_default();
                    Ok(ProjectCommandOutput {
                        exit_code,
                        stdout: response.stdout.unwrap_or_default(),
                        stderr,
                        duration_ms: response.duration_ms.unwrap_or_default(),
                        execution_state,
                        error: response.error,
                    })
                }
                Ok(Err(_)) => {
                    let request_dispatched = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    let execution_state = dispatch_uncertainty_lifecycle(request_dispatched);
                    Ok(ProjectCommandOutput {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: 0,
                        error: Some(
                            if execution_state == ShellCommandExecutionState::NotStarted {
                                "shell request waiter was dropped before the queued request was dispatched"
                                .to_string()
                            } else {
                                "shell request waiter was dropped after dispatch may have occurred"
                                    .to_string()
                            },
                        ),
                        execution_state,
                    })
                }
                Err(_) => {
                    let request_dispatched = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    let execution_state = dispatch_uncertainty_lifecycle(request_dispatched);
                    Ok(ProjectCommandOutput {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: wait_timeout.saturating_mul(1_000),
                        error: Some(format!(
                            "timed out waiting {wait_timeout} seconds for agent shell result"
                        )),
                        execution_state,
                    })
                }
            }
        } else {
            let cwd_path = resolve_local_cwd(&proj, cwd.as_deref())?;
            let result = run_command_sync_bounded_with_shell_and_sandbox(
                command,
                cwd_path,
                timeout,
                "sh".to_string(),
                sandbox.map(str::to_string),
            )
            .await;
            match result {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    let execution_state = local_command_error_lifecycle(exit_code, &stderr)
                        .unwrap_or_else(|| {
                            if looks_like_command_timeout(Some(exit_code), &stderr, timeout) {
                                ShellCommandExecutionState::TimedOut
                            } else {
                                ShellCommandExecutionState::Completed
                            }
                        });
                    Ok(ProjectCommandOutput {
                        exit_code: Some(exit_code),
                        stdout,
                        stderr,
                        duration_ms,
                        error: None,
                        execution_state,
                    })
                }
                Err(LocalRunFailure::HardTimeout { bound_secs }) => Ok(ProjectCommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: bound_secs.saturating_mul(1_000),
                    error: Some(format!(
                        "the local command did not return within the {bound_secs}-second hard bound"
                    )),
                    execution_state: ShellCommandExecutionState::OutcomeUnknown,
                }),
                Err(LocalRunFailure::Join(error)) => Ok(ProjectCommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: 0,
                    error: Some(format!(
                        "the local command worker ended without returning a result: {error}"
                    )),
                    execution_state: ShellCommandExecutionState::OutcomeUnknown,
                }),
            }
        }
    }

    fn run_shell_terminal_job_result(
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
                    .unwrap_or("Runner rejected the shell Job before command start");
                Self::run_shell_tool_failure_result(
                    command_rejected_message(
                        reason,
                        "inspect the rejection reason and retry only after confirming no command started.",
                    ),
                    Self::classify_run_shell_enqueue_failure(reason),
                    state,
                )
            }
            ShellCommandExecutionState::OutcomeUnknown => Self::run_shell_outcome_unknown_result(
                job.error
                    .as_deref()
                    .unwrap_or("the Runner lost a trustworthy terminal shell Job result"),
            ),
            ShellCommandExecutionState::TimedOut => Self::run_shell_command_failure_result(
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
                ToolResult::ok(Self::run_shell_success_output(
                    0,
                    stdout,
                    stderr,
                    job.duration_ms,
                ))
            }
            ShellCommandExecutionState::Completed => Self::run_shell_command_failure_result(
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

    pub(crate) async fn run_shell(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
    ) -> ToolResult {
        self.run_shell_with_contract(project, command, timeout_secs, cwd, None, None)
            .await
    }

    pub(crate) async fn run_shell_with_contract(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
    ) -> ToolResult {
        self.run_shell_with_contract_in_sandbox(
            project,
            command,
            timeout_secs,
            cwd,
            purpose,
            shell,
            None,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_shell_with_contract_in_sandbox(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
        sandbox: Option<&str>,
        ssh_resource: Option<&str>,
        session_id: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_raw_shell_command_length(&command) {
            return Self::run_shell_tool_failure_result(
                command_rejected_message(
                    error,
                    "use run_script for larger shell program text or stdin/files/artifacts for large data.",
                ),
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let timeout = match resolve_sync_timeout_secs(timeout_secs, DEFAULT_RUN_SHELL_TIMEOUT_SECS)
        {
            Ok(timeout) => timeout,
            Err(_) => {
                return sync_timeout_out_of_range_result(
                    "run_shell",
                    DEFAULT_RUN_SHELL_TIMEOUT_SECS,
                )
            }
        };
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => {
                return Self::run_shell_tool_failure_result(
                    command_rejected_message(
                        e.to_message(),
                        "verify the project id with list_projects, then retry with a registered project.",
                    ),
                    "agent_offline",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let declared_purpose = purpose.unwrap_or_default();
        let command_summary = command_preview(&command);
        if ssh_resource.is_some() && !proj.is_agent() {
            return Self::run_shell_tool_failure_result(
                command_rejected_message(
                    "ssh_resource_requires_agent_project: SSH resources require a project owned by a connected Runner",
                    "start or update a Session for an agent-backed project, then retry.",
                ),
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }
        if proj.is_agent() {
            let client_id =
                match proj.agent_client_id() {
                    Ok(id) => id.to_string(),
                    Err(e) => return Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            e,
                            "refresh the agent project registry with list_projects, then retry.",
                        ),
                        "agent_offline",
                        ShellCommandExecutionState::NotStarted,
                    ),
                };
            let (effective_cwd, resolved_cwd) = if ssh_resource.is_some() {
                let remote_cwd = cwd.as_deref().map(str::trim).filter(|cwd| !cwd.is_empty());
                if remote_cwd
                    .is_some_and(|cwd| cwd.len() > 4096 || cwd.chars().any(char::is_control))
                {
                    return Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            "ssh_remote_cwd_invalid: cwd must be a bounded remote path without control characters",
                            "choose a valid remote cwd or omit it to use the Session/resource default.",
                        ),
                        "runtime_error",
                        ShellCommandExecutionState::NotStarted,
                    );
                }
                (
                    remote_cwd.map(str::to_string),
                    remote_cwd.unwrap_or(".").to_string(),
                )
            } else {
                let effective_cwd = match resolve_agent_cwd(&proj, cwd.as_deref()) {
                    Ok(cwd) => cwd,
                    Err(e) => {
                        return Self::run_shell_tool_failure_result(
                            command_rejected_message(
                                e,
                                "choose '.', an existing project-relative cwd, or an absolute path inside the registered project root.",
                            ),
                            "permission_denied",
                            ShellCommandExecutionState::NotStarted,
                        )
                    }
                };
                let resolved_cwd = project_relative_agent_cwd(&proj, &effective_cwd)
                    .unwrap_or_else(|_| ".".to_string());
                (Some(effective_cwd), resolved_cwd)
            };
            if ssh_resource.is_some() && session_id.is_none() {
                return Self::run_shell_tool_failure_result(
                    command_rejected_message(
                        "ssh_session_required: an SSH resource requires a Workflow Session id",
                        "start or resume the Session, then retry without automatic command retry.",
                    ),
                    "runtime_error",
                    ShellCommandExecutionState::NotStarted,
                );
            }
            let actual_shell =
                shell
                    .map(ExecutionShell::as_str)
                    .unwrap_or(if ssh_resource.is_some() {
                        "remote"
                    } else {
                        "configured"
                    });
            let dispatched_command = match shell {
                Some(shell) => match explicit_shell_dispatch_command(&command, shell.as_str()) {
                    Ok(command) => command,
                    Err(error) => return Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            error,
                            "use run_script for large or quote-dense explicit-shell program text.",
                        ),
                        "runtime_error",
                        ShellCommandExecutionState::NotStarted,
                    ),
                },
                None => command.clone(),
            };
            let async_handoff_available =
                if timeout > DEFAULT_RUN_SHELL_TIMEOUT_SECS && ssh_resource.is_none() {
                    self.shell_clients
                        .get_client_capabilities(&client_id)
                        .await
                        .is_ok_and(|capabilities| {
                            capabilities.shell
                                && (capabilities.async_jobs || capabilities.async_shell_jobs)
                        })
                } else {
                    false
                };
            if async_handoff_available {
                let job = self
                    .shell_clients
                    .start_job_with_metadata_for_auth(
                        ShellJobOpRequest {
                            op: "start".to_string(),
                            client_id: Some(client_id.clone()),
                            cwd: effective_cwd.clone(),
                            command: Some(dispatched_command.clone()),
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
                            session_id: session_id.map(str::to_string),
                            project_cwd: Some(resolved_cwd.clone()),
                            purpose: Some(declared_purpose.as_str().to_string()),
                            shell: Some(actual_shell.to_string()),
                            visibility: ShellJobVisibility::HiddenUntilHandoff,
                            sandbox: sandbox.map(str::to_string),
                            ..Default::default()
                        },
                        auth,
                    )
                    .await;
                let job = match job {
                    Ok(job) => job,
                    Err(error) => {
                        let mut result = Self::run_shell_tool_failure_result(
                            command_rejected_message(
                                &error,
                                "confirm the Runner is connected and retry only after confirming no shell Job started.",
                            ),
                            Self::classify_run_shell_enqueue_failure(&error),
                            ShellCommandExecutionState::NotStarted,
                        );
                        add_structured_continuation_facts(
                            &mut result,
                            timeout,
                            STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
                            true,
                        );
                        decorate_execution_output(
                            &mut result.output,
                            declared_purpose,
                            &command_summary,
                            &resolved_cwd,
                            actual_shell,
                            "agent",
                        );
                        return result;
                    }
                };
                let wait = self
                    .structured_execution_sync_wait
                    .min(Duration::from_secs(STRUCTURED_EXECUTION_SYNC_WAIT_SECS));
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
                        let result = Self::run_shell_terminal_job_result(
                            &job,
                            stdout,
                            stderr,
                            timeout,
                        );
                        self.shell_clients
                            .remove_projected_hidden_terminal_job_record(&job.job_id)
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
                        "sync_wait_secs": STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
                        "async_handoff_available": true,
                        "stdout_tail": "",
                        "stderr_tail": "",
                        "stdout_lines": 0,
                        "stderr_lines": 0,
                        "stdout_truncated": false,
                        "stderr_truncated": false,
                    })),
                    Err(error) => Self::run_shell_outcome_unknown_result(format!(
                        "the hidden durable shell Job {} could not be safely promoted or observed during handoff: {error}. Do not redispatch this command; inspect Job inventory and target state before deciding whether any retry is safe.",
                        job.job_id
                    )),
                };
                if result.output["promoted_to_job"] != json!(true) {
                    add_structured_continuation_facts(
                        &mut result,
                        timeout,
                        STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
                        true,
                    );
                }
                decorate_execution_output(
                    &mut result.output,
                    declared_purpose,
                    &command_summary,
                    &resolved_cwd,
                    actual_shell,
                    "agent",
                );
                return result;
            }

            let wait_timeout = timeout;
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_run_with_sandbox_and_ssh(
                    ShellRunRequest {
                        client_id,
                        cwd: effective_cwd,
                        command: dispatched_command,
                        stdin: None,
                        timeout_secs: timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                    sandbox.map(str::to_string),
                    ssh_resource.map(str::to_string),
                    ssh_resource
                        .zip(session_id)
                        .map(|(_, session_id)| session_id.to_string()),
                )
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    let failure_kind = Self::classify_run_shell_enqueue_failure(&e);
                    return Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            e,
                            "confirm the agent is connected and the command request is allowed, then retry or use run_job for long-running work.",
                        ),
                        failure_kind,
                        ShellCommandExecutionState::NotStarted,
                    );
                }
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let lifecycle = agent_command_lifecycle(&response, timeout);
                    let exit_code = response.exit_code;
                    let stdout = response.stdout.unwrap_or_default();
                    let stderr = response.stderr.unwrap_or_default();
                    let duration_ms = response.duration_ms;
                    let error = response.error;
                    let mut result = match lifecycle {
                        ShellCommandExecutionState::NotStarted => {
                            let reason = error
                                .as_deref()
                                .unwrap_or("Runner rejected the command before process spawn");
                            Self::run_shell_tool_failure_result(
                                command_rejected_message(
                                    reason,
                                    "inspect the rejection reason, adjust the cwd/command/project, then retry.",
                                ),
                                Self::classify_run_shell_enqueue_failure(reason),
                                ShellCommandExecutionState::NotStarted,
                            )
                        }
                        ShellCommandExecutionState::OutcomeUnknown => {
                            Self::run_shell_outcome_unknown_result(error.as_deref().unwrap_or(
                                "the Runner did not return a trustworthy terminal result",
                            ))
                        }
                        ShellCommandExecutionState::TimedOut => {
                            Self::run_shell_command_failure_result(
                                exit_code,
                                stdout,
                                stderr,
                                duration_ms,
                                timeout,
                                ShellCommandExecutionState::TimedOut,
                            )
                        }
                        ShellCommandExecutionState::Completed
                            if error.is_none() && exit_code == Some(0) =>
                        {
                            ToolResult::ok(Self::run_shell_success_output(
                                0,
                                stdout,
                                stderr,
                                duration_ms,
                            ))
                        }
                        ShellCommandExecutionState::Completed => {
                            Self::run_shell_command_failure_result(
                                exit_code,
                                stdout,
                                stderr,
                                duration_ms,
                                timeout,
                                ShellCommandExecutionState::Completed,
                            )
                        }
                    };
                    decorate_execution_output(
                        &mut result.output,
                        declared_purpose,
                        &command_summary,
                        &resolved_cwd,
                        actual_shell,
                        "agent",
                    );
                    if let Some(resource) = ssh_resource {
                        result.output["ssh_resource"] = json!(resource);
                    }
                    result
                }
                Ok(Err(_)) => {
                    let dispatch_state = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    if dispatch_state == Some(false) {
                        Self::run_shell_tool_failure_result(
                            command_rejected_message(
                                "shell request waiter was dropped before the queued request was dispatched",
                                "check agent connectivity, then retry or use run_job for recoverable long-running work.",
                            ),
                            "runtime_error",
                            ShellCommandExecutionState::NotStarted,
                        )
                    } else {
                        Self::run_shell_outcome_unknown_result(
                            "shell request waiter was dropped before a result was returned",
                        )
                    }
                }
                Err(_) => {
                    let dispatch_state = self
                        .shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    if dispatch_state == Some(false) {
                        Self::run_shell_tool_failure_result(
                            command_rejected_message(
                                format!(
                                    "timed out waiting {wait_timeout} seconds before the queued agent request was dispatched"
                                ),
                                "check agent connectivity and availability, then retry or use run_job for long-running work.",
                            ),
                            "timeout",
                            ShellCommandExecutionState::NotStarted,
                        )
                    } else {
                        Self::run_shell_outcome_unknown_result(format!(
                            "timed out waiting {wait_timeout} seconds for the agent shell result"
                        ))
                    }
                }
            }
        } else {
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(path) => path,
                Err(e) => {
                    return Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            e,
                            "read the project root and choose an existing project-relative cwd, then retry.",
                        ),
                        "permission_denied",
                        ShellCommandExecutionState::NotStarted,
                    )
                }
            };
            let resolved_cwd =
                project_relative_cwd(&proj, &cwd_path).unwrap_or_else(|_| ".".to_string());
            let actual_shell = shell.map(ExecutionShell::as_str).unwrap_or("sh");
            let result = match run_command_sync_bounded_with_shell_and_sandbox(
                command,
                cwd_path,
                timeout,
                actual_shell.to_string(),
                sandbox.map(str::to_string),
            )
            .await
            {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    match local_command_error_lifecycle(exit_code, &stderr) {
                        Some(ShellCommandExecutionState::NotStarted) => {
                            Self::run_shell_tool_failure_result(
                                command_rejected_message(
                                    &stderr,
                                    "correct the local shell or sandbox configuration, then retry.",
                                ),
                                "runtime_error",
                                ShellCommandExecutionState::NotStarted,
                            )
                        }
                        Some(ShellCommandExecutionState::OutcomeUnknown) => {
                            Self::run_shell_outcome_unknown_result(&stderr)
                        }
                        _ if exit_code == 0 => ToolResult::ok(Self::run_shell_success_output(
                            exit_code,
                            stdout,
                            stderr,
                            Some(duration_ms),
                        )),
                        _ => {
                            let lifecycle =
                                if looks_like_command_timeout(Some(exit_code), &stderr, timeout) {
                                    ShellCommandExecutionState::TimedOut
                                } else {
                                    ShellCommandExecutionState::Completed
                                };
                            Self::run_shell_command_failure_result(
                                Some(exit_code),
                                stdout,
                                stderr,
                                Some(duration_ms),
                                timeout,
                                lifecycle,
                            )
                        }
                    }
                }
                // The command's own timeout is reported through the Ok tuple;
                // this arm means the post-exit output drain wedged (a
                // descendant escaped the process group while holding the
                // pipes) and the outer backstop fired instead of parking the
                // MCP request indefinitely.
                Err(LocalRunFailure::HardTimeout { bound_secs }) => {
                    Self::run_shell_outcome_unknown_result(format!(
                        "the local command did not return within the {bound_secs}-second hard bound"
                    ))
                }
                Err(LocalRunFailure::Join(e)) => Self::run_shell_outcome_unknown_result(format!(
                    "the local command worker ended without returning a result: {e}"
                )),
            };
            let mut result = result;
            decorate_execution_output(
                &mut result.output,
                declared_purpose,
                &command_summary,
                &resolved_cwd,
                actual_shell,
                "local",
            );
            result
        }
    }
}

fn decorate_execution_output(
    output: &mut serde_json::Value,
    purpose: ExecutionPurpose,
    command_summary: &str,
    cwd: &str,
    shell: &str,
    executor: &str,
) {
    output["execution_source"] = json!("run_shell");
    output["purpose"] = json!(purpose.as_str());
    output["command_summary"] = json!(command_summary);
    output["cwd"] = json!(cwd);
    output["shell"] = json!(shell);
    output["executor"] = json!(executor);
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{
        agent_command_lifecycle, dispatch_uncertainty_lifecycle, local_command_error_lifecycle,
    };
    use crate::shell_protocol::{ShellCommandExecutionState, ShellRunResponse};

    #[test]
    fn local_executor_errors_preserve_the_start_boundary() {
        assert!(matches!(
            local_command_error_lifecycle(-1, "Failed to execute command: shell missing"),
            Some(ShellCommandExecutionState::NotStarted)
        ));
        assert!(matches!(
            local_command_error_lifecycle(-1, "Failed to wait for command: executor error"),
            Some(ShellCommandExecutionState::OutcomeUnknown)
        ));
        assert!(matches!(
            local_command_error_lifecycle(
                -1,
                "Command timed out after 1 seconds; failed to collect output: pipe error"
            ),
            Some(ShellCommandExecutionState::OutcomeUnknown)
        ));
        assert!(local_command_error_lifecycle(7, "ordinary stderr").is_none());
    }

    #[test]
    fn capture_wait_uncertainty_requires_definite_undispatch_evidence() {
        assert_eq!(
            dispatch_uncertainty_lifecycle(Some(false)),
            ShellCommandExecutionState::NotStarted
        );
        assert_eq!(
            dispatch_uncertainty_lifecycle(Some(true)),
            ShellCommandExecutionState::OutcomeUnknown
        );
        assert_eq!(
            dispatch_uncertainty_lifecycle(None),
            ShellCommandExecutionState::OutcomeUnknown
        );
    }

    #[test]
    fn agent_lifecycle_uses_structured_evidence_not_error_prose() {
        let response = ShellRunResponse {
            success: false,
            request_id: "req-1".to_string(),
            client_id: "agent-1".to_string(),
            cwd: None,
            command_preview: "ignored".to_string(),
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: None,
            error: Some("Rejected before starting command".to_string()),
            request_dispatched: Some(true),
            command_execution_state: Some(ShellCommandExecutionState::OutcomeUnknown),
        };
        assert_eq!(
            agent_command_lifecycle(&response, 30),
            ShellCommandExecutionState::OutcomeUnknown
        );
    }
}
