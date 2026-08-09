use serde_json::json;
use std::time::Duration;

use super::helpers::{
    bounded_tail, command_failed_message, command_outcome_unknown_message,
    command_rejected_message, command_timeout_message, looks_like_command_timeout,
    project_relative_agent_cwd, project_relative_cwd, resolve_agent_cwd, resolve_local_cwd,
    resolve_sync_timeout_secs, run_process_sync_bounded_with_sandbox,
    sync_timeout_out_of_range_result, LocalRunFailure, COMMAND_STDIO_TAIL_CHARS,
    DEFAULT_RUN_SHELL_TIMEOUT_SECS,
};
use super::shell::{
    agent_command_lifecycle, command_execution_state_name, dispatch_uncertainty_lifecycle,
};
use super::{ExecutionPurpose, ToolResult, ToolRuntime};
use crate::shell_client::process_preview;
use crate::shell_protocol::{
    validate_process_argv, ShellCommandExecutionState, ShellProcessArgv, PROCESS_CWD_MAX_BYTES,
    PROCESS_STDIN_MAX_BYTES,
};

fn command_started(state: ShellCommandExecutionState) -> bool {
    !matches!(state, ShellCommandExecutionState::NotStarted)
}

fn command_completed(state: ShellCommandExecutionState) -> bool {
    matches!(state, ShellCommandExecutionState::Completed)
}

fn success_output(
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

fn command_failure_result(
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

fn outcome_unknown_result(reason: impl AsRef<str>) -> ToolResult {
    process_tool_failure_result(
        command_outcome_unknown_message(reason),
        "outcome_unknown",
        ShellCommandExecutionState::OutcomeUnknown,
    )
}

pub(crate) fn classify_process_failure(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unsupported_executable_type") {
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

fn local_error_state(exit_code: i32, stderr: &str) -> Option<ShellCommandExecutionState> {
    if exit_code != -1 {
        return None;
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.starts_with("failed to execute process:")
        || lower.starts_with("failed to configure inspect sandbox:")
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
    ) -> ToolResult {
        let timeout = match resolve_sync_timeout_secs(timeout_secs, DEFAULT_RUN_SHELL_TIMEOUT_SECS)
        {
            Ok(timeout) => timeout,
            Err(_) => {
                return sync_timeout_out_of_range_result(
                    "run_process",
                    DEFAULT_RUN_SHELL_TIMEOUT_SECS,
                )
            }
        };
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
            result
        } else {
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
            result
        }
    }
}
