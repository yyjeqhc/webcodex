use serde_json::json;
use std::time::Duration;

use super::helpers::{
    command_rejected_message, explicit_shell_dispatch_command, looks_like_command_timeout,
    project_relative_runner_cwd, resolve_runner_cwd, validate_raw_shell_command_length,
    MAX_SYNC_TIMEOUT_SECS, MIN_SYNC_TIMEOUT_SECS,
};
use super::process::{
    add_structured_continuation_facts, command_failure_result, execution_admission_failure_result,
    outcome_unknown_result, process_tool_failure_result, success_output,
};
use super::structured_execution::{
    await_structured_job_result, await_structured_request_response, structured_response_result,
    StructuredExecutionBudget, StructuredRequestKind,
};
use super::tool_result::ToolResult;
use super::{ExecutionPurpose, ExecutionShell, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_http::{
    command_preview, RunnerFeature, ShellJobStartMetadata, ShellJobVisibility,
};
use crate::runner_protocol::{
    ShellCommandExecutionState, ShellJobInfo, ShellJobOpRequest, ShellRunRequest, ShellRunResponse,
};

pub(crate) struct ProjectCommandOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    pub(crate) duration_ms: u64,
    pub(crate) error: Option<String>,
    pub(crate) execution_state: ShellCommandExecutionState,
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

pub(crate) fn runner_command_lifecycle(
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
    pub(super) fn classify_run_shell_response_failure(message: &str) -> &'static str {
        let lower = message.to_ascii_lowercase();
        if lower.contains("offline")
            || lower.contains("not connected")
            || lower.contains("no connected")
            || lower.contains("unknown agent")
            || lower.contains("unknown_project")
        {
            "agent_offline"
        } else if lower.contains("capability_unavailable")
            || lower.contains("agent_capability_unavailable")
            || lower.contains("does not support")
        {
            "capability_unavailable"
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
        self.run_project_command_capture_impl(project, command, timeout_secs, cwd, false)
            .await
    }

    pub(crate) async fn run_project_internal_posix_script_capture(
        &self,
        project: &str,
        script: String,
        timeout_secs: u64,
        cwd: Option<String>,
    ) -> Result<ProjectCommandOutput, String> {
        self.run_project_command_capture_impl(project, script, timeout_secs, cwd, true)
            .await
    }

    async fn run_project_command_capture_impl(
        &self,
        project: &str,
        command: String,
        timeout_secs: u64,
        cwd: Option<String>,
        internal_posix_script: bool,
    ) -> Result<ProjectCommandOutput, String> {
        let proj = self.resolve_project(project).await?;
        // Shared root of the sync agent-wait contract: wait_timeout_secs and
        // command timeout must both stay within 1..=120 before enqueue so
        // Runner HTTP validation never rejects with implementation-detail
        // errors about runShell.
        if !(MIN_SYNC_TIMEOUT_SECS..=MAX_SYNC_TIMEOUT_SECS).contains(&timeout_secs) {
            return Err(format!(
                "timeout_secs must be between {MIN_SYNC_TIMEOUT_SECS} and {MAX_SYNC_TIMEOUT_SECS}"
            ));
        }
        let timeout = timeout_secs;
        let client_id = proj.client_id.clone();
        let effective_cwd = Some(resolve_runner_cwd(&proj, cwd.as_deref())?);
        let wait_timeout = timeout;
        let (request_id, rx) = if internal_posix_script {
            self.runner_registry
                .enqueue_internal_posix_script(
                    client_id,
                    effective_cwd,
                    command,
                    timeout,
                    wait_timeout,
                    "tool_runtime".to_string(),
                )
                .await?
        } else {
            self.runner_registry
                .enqueue_run(
                    ShellRunRequest {
                        login: false,
                        client_id,
                        cwd: effective_cwd,
                        command,
                        stdin: None,
                        timeout_secs: timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await?
        };
        match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
            Ok(Ok(response)) => {
                let execution_state = runner_command_lifecycle(&response, timeout);
                let exit_code = response.exit_code;
                let stderr = response.stderr.unwrap_or_default();
                Ok(ProjectCommandOutput {
                    exit_code,
                    stdout: response.stdout.unwrap_or_default(),
                    stderr,
                    stdout_truncated: response.stdout_truncated,
                    stderr_truncated: response.stderr_truncated,
                    duration_ms: response.duration_ms.unwrap_or_default(),
                    execution_state,
                    error: response.error,
                })
            }
            Ok(Err(_)) => {
                let request_dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let execution_state = dispatch_uncertainty_lifecycle(request_dispatched);
                Ok(ProjectCommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
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
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let execution_state = dispatch_uncertainty_lifecycle(request_dispatched);
                Ok(ProjectCommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                    duration_ms: wait_timeout.saturating_mul(1_000),
                    error: Some(format!(
                        "timed out waiting {wait_timeout} seconds for agent shell result"
                    )),
                    execution_state,
                })
            }
        }
    }

    pub(super) fn run_shell_terminal_job_result(
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
                process_tool_failure_result(
                    command_rejected_message(
                        reason,
                        "inspect the rejection reason and retry only after confirming no command started.",
                    ),
                    Self::classify_run_shell_response_failure(reason),
                    state,
                )
            }
            ShellCommandExecutionState::OutcomeUnknown => outcome_unknown_result(
                job.error
                    .as_deref()
                    .unwrap_or("the Runner lost a trustworthy terminal shell Job result"),
            ),
            ShellCommandExecutionState::Completed
                if job.status == "completed" && job.exit_code == Some(0) && job.error.is_none() =>
            {
                ToolResult::ok(success_output(0, stdout, stderr, job.duration_ms))
            }
            ShellCommandExecutionState::TimedOut | ShellCommandExecutionState::Completed => {
                command_failure_result(
                    job.exit_code,
                    stdout,
                    stderr,
                    job.duration_ms,
                    timeout_secs,
                    state,
                )
            }
        };
        if result.success {
            // A recovered terminal snapshot is conclusive, but not an ordinary
            // fast success. Preserve its canonical recovery facts for projection.
            if let Some(state) = &job.recovery_state {
                result.output["recovery_state"] = json!(state);
            }
            if job.recovered_after_server_restart {
                result.output["recovered_after_server_restart"] = json!(true);
            }
            if let Some(at) = job.reconciled_at {
                result.output["reconciled_at"] = json!(at);
            }
            if let Some(reason) = &job.recovery_reason_code {
                result.output["recovery_reason_code"] = json!(reason);
            }
        }
        if job.stdout_log_truncated {
            result.output["stdout_truncated"] = json!(true);
        }
        if job.stderr_log_truncated {
            result.output["stderr_truncated"] = json!(true);
        }
        result
    }

    #[cfg(test)]
    pub(crate) async fn run_shell(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
    ) -> ToolResult {
        self.run_shell_with_contract(project, command, timeout_secs, None, cwd, None, None)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn run_shell_with_contract(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
    ) -> ToolResult {
        self.run_shell_with_contract_for_resource(
            project,
            command,
            timeout_secs,
            sync_wait_secs,
            cwd,
            purpose,
            shell,
            false,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_shell_with_contract_for_resource(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
        login: bool,
        ssh_resource: Option<&str>,
        session_id: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if login && (shell != Some(ExecutionShell::Bash) || ssh_resource.is_some()) {
            return process_tool_failure_result(
                "run_shell login=true requires local shell=bash".to_string(),
                "invalid_arguments",
                ShellCommandExecutionState::NotStarted,
            );
        }
        if let Err(error) = validate_raw_shell_command_length(&command) {
            return process_tool_failure_result(
                command_rejected_message(
                    error,
                    "use run_script for larger shell program text or stdin/files/artifacts for large data.",
                ),
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let budget = match StructuredExecutionBudget::resolve_with_sync_wait(
            timeout_secs,
            sync_wait_secs,
        ) {
            Ok(budget) => budget,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        format!("run_shell {error}"),
                        "pass positive timeout_secs/sync_wait_secs values or omit them for defaults; oversized values are clamped to the shared structured-execution ceilings.",
                    ),
                    "invalid_arguments",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let timeout = budget.effective_timeout_secs;
        if ssh_resource.is_some() && sync_wait_secs.is_some() {
            return process_tool_failure_result(
                command_rejected_message(
                    "named Session SSH resources do not support sync_wait_secs because remote shell execution has no Runner-owned durable Job handoff",
                    "omit sync_wait_secs for direct SSH execution, or use Runner-host run_shell when same-execution Job handoff is required.",
                ),
                "unsupported_resource",
                ShellCommandExecutionState::NotStarted,
            );
        }
        if ssh_resource.is_some() && timeout > MAX_SYNC_TIMEOUT_SECS {
            return process_tool_failure_result(
                command_rejected_message(
                    format!("named Session SSH run_shell supports at most {MAX_SYNC_TIMEOUT_SECS}s total runtime because remote durable shell handoff is unavailable"),
                    "request a direct SSH timeout within the supported ceiling; remote durable shell execution is not provided by run_shell.",
                ),
                "capability_unavailable",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => {
                return process_tool_failure_result(
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
        let client_id = proj.client_id.clone();
        let (effective_cwd, resolved_cwd) = if ssh_resource.is_some() {
            let remote_cwd = cwd.as_deref().map(str::trim).filter(|cwd| !cwd.is_empty());
            if remote_cwd.is_some_and(|cwd| cwd.len() > 4096 || cwd.chars().any(char::is_control)) {
                return process_tool_failure_result(
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
            let effective_cwd = match resolve_runner_cwd(&proj, cwd.as_deref()) {
                    Ok(cwd) => cwd,
                    Err(e) => {
                        return process_tool_failure_result(
                            command_rejected_message(
                                e,
                                "choose '.', an existing project-relative cwd, or an absolute path inside the registered project root.",
                            ),
                            "permission_denied",
                            ShellCommandExecutionState::NotStarted,
                        )
                    }
                };
            let resolved_cwd = project_relative_runner_cwd(&proj, &effective_cwd)
                .unwrap_or_else(|_| ".".to_string());
            (Some(effective_cwd), resolved_cwd)
        };
        if ssh_resource.is_some() && session_id.is_none() {
            return process_tool_failure_result(
                command_rejected_message(
                    "ssh_session_required: an SSH resource requires a Workflow Session id",
                    "start or resume the Session, then retry without automatic command retry.",
                ),
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let actual_shell = if login {
            "bash_login"
        } else {
            shell
                .map(ExecutionShell::as_str)
                .unwrap_or(if ssh_resource.is_some() {
                    "remote"
                } else {
                    "configured"
                })
        };
        let dispatched_command = match (ssh_resource, shell) {
            // Named SSH keeps the existing remote-login-shell compatibility
            // wrapper. Local explicit shells are selected structurally by the
            // Runner, so never expose this POSIX wrapper to configured PowerShell.
            (Some(_), Some(shell)) => {
                match explicit_shell_dispatch_command(&command, shell.as_str()) {
                    Ok(command) => command,
                    Err(error) => {
                        return process_tool_failure_result(
                            command_rejected_message(
                                error,
                                "use run_script for substantially larger typed program text.",
                            ),
                            "runtime_error",
                            ShellCommandExecutionState::NotStarted,
                        )
                    }
                }
            }
            _ => command.clone(),
        };
        let handoff_requested = ssh_resource.is_none() && timeout > budget.sync_wait_secs;
        let async_handoff_available = if handoff_requested {
            let features = match self
                .runner_registry
                .get_runner_feature_set(&client_id)
                .await
            {
                Ok(features) => features,
                Err(error) => {
                    let mut result = process_tool_failure_result(
                        command_rejected_message(
                            error,
                            "confirm the Runner is registered and connected, then retry; the command was not started.",
                        ),
                        "agent_offline",
                        ShellCommandExecutionState::NotStarted,
                    );
                    add_structured_continuation_facts(
                        &mut result,
                        timeout,
                        budget.sync_wait_secs,
                        false,
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
            features.supports(RunnerFeature::Shell)
                && shell.is_none_or(|_| features.supports(RunnerFeature::ExplicitShellSelection))
                && (features.supports(RunnerFeature::AsyncJobs)
                    || features.supports(RunnerFeature::AsyncShellJobs))
        } else {
            false
        };
        if handoff_requested && !async_handoff_available && timeout > MAX_SYNC_TIMEOUT_SECS {
            let mut result = process_tool_failure_result(
                command_rejected_message(
                    format!("capability_unavailable: this Runner cannot preserve the requested {timeout}s run_shell lifetime because durable shell Job handoff is unavailable"),
                    format!("use a Runner advertising durable shell Jobs or keep timeout_secs <= {MAX_SYNC_TIMEOUT_SECS}; the command was not started."),
                ),
                "capability_unavailable",
                ShellCommandExecutionState::NotStarted,
            );
            add_structured_continuation_facts(&mut result, timeout, budget.sync_wait_secs, false);
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
        if async_handoff_available {
            let job = self
                .runner_registry
                .start_job_with_metadata_for_access_typed(
                    ShellJobOpRequest {
                        op: "start".to_string(),
                        client_id: Some(client_id.clone()),
                        cwd: effective_cwd.clone(),
                        command: Some(dispatched_command.clone()),
                        login,
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
                        explicit_shell: if ssh_resource.is_none() { shell } else { None },
                        visibility: ShellJobVisibility::HiddenUntilHandoff,
                        ..Default::default()
                    },
                    crate::runner_http::runner_access_from_auth(auth).as_ref(),
                    None,
                )
                .await;
            let job = match job {
                Ok(job) => job,
                Err(error) => {
                    let mut result = execution_admission_failure_result(
                        error,
                        "confirm the Runner is connected and retry only after confirming no shell Job started.",
                    );
                    add_structured_continuation_facts(
                        &mut result,
                        timeout,
                        budget.sync_wait_secs,
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
            let mut result = match await_structured_job_result(
                self.runner_registry.clone(),
                job.job_id,
                self.structured_execution_sync_wait,
                auth,
                budget,
                &command_summary,
                declared_purpose,
                Self::run_shell_terminal_job_result,
            )
            .await
            {
                Ok(result) => result,
                Err(failure) => return failure.into_tool_result(&project, budget),
            };
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
            .runner_registry
            .enqueue_run_with_ssh_typed(
                ShellRunRequest {
                    client_id,
                    cwd: effective_cwd,
                    command: dispatched_command,
                    login,
                    stdin: None,
                    timeout_secs: timeout,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
                ssh_resource.map(str::to_string),
                ssh_resource
                    .zip(session_id)
                    .map(|(_, session_id)| session_id.to_string()),
                if ssh_resource.is_none() { shell } else { None },
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                return execution_admission_failure_result(
                        e,
                        "confirm the agent is connected and the command request is allowed, then retry with the same shell path. Use run_job only when immediate asynchronous shell start is intentional; if a native child must survive Runner restart/replacement, use run_detached_process from the start.",
                    );
            }
        };
        let response = match await_structured_request_response(
            &self.runner_registry,
            &request_id,
            rx,
            wait_timeout,
            StructuredRequestKind::Shell,
        )
        .await
        {
            Ok(response) => response,
            Err(result) => return result,
        };
        let mut result =
            structured_response_result(response, timeout, StructuredRequestKind::Shell);
        decorate_execution_output(
            &mut result.output,
            declared_purpose,
            &command_summary,
            &resolved_cwd,
            actual_shell,
            "agent",
        );
        // Direct requests have no Job identity. Only received Runner responses
        // carry execution presentation, and only proven successes get this tuple.
        if result.success {
            add_structured_continuation_facts(&mut result, timeout, budget.sync_wait_secs, false);
        }
        if let Some(resource) = ssh_resource {
            result.output["ssh_resource"] = json!(resource);
        }
        result
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
    use super::{dispatch_uncertainty_lifecycle, runner_command_lifecycle};
    use crate::runner_protocol::{ShellCommandExecutionState, ShellRunResponse};

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
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: None,
            error: Some("Rejected before starting command".to_string()),
            request_dispatched: Some(true),
            command_execution_state: Some(ShellCommandExecutionState::OutcomeUnknown),
        };
        assert_eq!(
            runner_command_lifecycle(&response, 30),
            ShellCommandExecutionState::OutcomeUnknown
        );
    }
}
