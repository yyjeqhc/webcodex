use serde_json::json;
use std::time::Duration;

use super::helpers::{
    bounded_tail, command_failed_message, command_rejected_message, command_timeout_message,
    looks_like_command_timeout, resolve_local_cwd, run_command_sync, COMMAND_STDIO_TAIL_CHARS,
};
use super::tool_result::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::ShellRunRequest;

pub(crate) struct ProjectCommandOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration_ms: u64,
    pub(crate) error: Option<String>,
}

impl ToolRuntime {
    fn run_shell_success_output(
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
    ) -> serde_json::Value {
        json!({
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "duration_ms": duration_ms,
            "command_started": true,
            "command_completed": true,
            "command_ok": true,
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
    ) -> ToolResult {
        let (stdout_tail, stdout_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
        let (stderr_tail, stderr_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
        let timed_out = looks_like_command_timeout(exit_code, &stderr, timeout_secs);
        let output = json!({
            "exit_code": exit_code,
            "duration_ms": duration_ms,
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
            "command_started": true,
            "command_completed": !timed_out,
            "command_ok": false,
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
        command_started: bool,
        command_completed: bool,
    ) -> ToolResult {
        ToolResult::err_with_output(
            message,
            json!({
                "command_started": command_started,
                "command_completed": command_completed,
                "command_ok": false,
                "exit_code": null,
                "failure_kind": failure_kind,
                "tool_failure": true,
            }),
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
        let proj = self.resolve_project(project).await?;
        let timeout = timeout_secs.max(1);
        if proj.is_agent() {
            let client_id = proj.agent_client_id()?.to_string();
            let effective_cwd = match cwd {
                Some(cwd) => {
                    let joined = std::path::Path::new(&proj.path).join(cwd);
                    Some(joined.to_string_lossy().to_string())
                }
                None => Some(proj.path.clone()),
            };
            let wait_timeout = timeout.min(1_800);
            let (request_id, rx) = self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: effective_cwd,
                        command,
                        stdin: None,
                        timeout_secs: timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await?;
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => Ok(ProjectCommandOutput {
                    exit_code: response.exit_code,
                    stdout: response.stdout.unwrap_or_default(),
                    stderr: response.stderr.unwrap_or_default(),
                    duration_ms: response.duration_ms.unwrap_or_default(),
                    error: response.error,
                }),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    Err("shell request waiter was dropped".to_string())
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    Err(format!(
                        "timed out waiting {} seconds for agent shell result",
                        wait_timeout
                    ))
                }
            }
        } else {
            let cwd_path = resolve_local_cwd(&proj, cwd.as_deref())?;
            let result = tokio::task::spawn_blocking({
                let cmd = command;
                move || run_command_sync(&cmd, &cwd_path, timeout)
            })
            .await
            .map_err(|e| format!("task join error: {}", e))?;
            Ok(ProjectCommandOutput {
                exit_code: Some(result.0),
                stdout: result.1,
                stderr: result.2,
                duration_ms: result.3,
                error: None,
            })
        }
    }

    pub(crate) async fn run_shell(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => {
                return Self::run_shell_tool_failure_result(
                    command_rejected_message(
                        e.to_message(),
                        "verify the project id with list_projects, then retry with a registered project.",
                    ),
                    "agent_offline",
                    false,
                    false,
                )
            }
        };
        let timeout = timeout_secs.unwrap_or(60).max(1);
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
                        false,
                        false,
                    ),
                };
            let effective_cwd = cwd.or_else(|| Some(proj.path.clone()));
            let wait_timeout = timeout.min(120);
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: effective_cwd,
                        command,
                        stdin: None,
                        timeout_secs: timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
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
                        false,
                        false,
                    );
                }
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let success = response.error.is_none() && response.exit_code == Some(0);
                    if success {
                        ToolResult::ok(Self::run_shell_success_output(
                            0,
                            response.stdout.unwrap_or_default(),
                            response.stderr.unwrap_or_default(),
                            response.duration_ms,
                        ))
                    } else if let Some(error) = response.error {
                        Self::run_shell_tool_failure_result(
                            command_rejected_message(
                                &error,
                                "inspect the rejection reason, adjust the cwd/command/project, then retry.",
                            ),
                            Self::classify_run_shell_enqueue_failure(&error),
                            false,
                            false,
                        )
                    } else {
                        Self::run_shell_command_failure_result(
                            response.exit_code,
                            response.stdout.unwrap_or_default(),
                            response.stderr.unwrap_or_default(),
                            response.duration_ms,
                            timeout,
                        )
                    }
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    Self::run_shell_tool_failure_result(
                        command_rejected_message(
                            "shell request waiter was dropped before a result was returned",
                            "check agent connectivity, then retry or use run_job for recoverable long-running work.",
                        ),
                        "runtime_error",
                        false,
                        false,
                    )
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    Self::run_shell_tool_failure_result(
                        command_timeout_message(wait_timeout, "", ""),
                        "timeout",
                        true,
                        false,
                    )
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
                        false,
                        false,
                    )
                }
            };
            let result = tokio::task::spawn_blocking({
                let cmd = command;
                move || run_command_sync(&cmd, &cwd_path, timeout)
            })
            .await;
            match result {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    if exit_code == 0 {
                        ToolResult::ok(Self::run_shell_success_output(
                            exit_code,
                            stdout,
                            stderr,
                            Some(duration_ms),
                        ))
                    } else {
                        Self::run_shell_command_failure_result(
                            Some(exit_code),
                            stdout,
                            stderr,
                            Some(duration_ms),
                            timeout,
                        )
                    }
                }
                Err(e) => Self::run_shell_tool_failure_result(
                    command_rejected_message(
                        format!("task join error: {}", e),
                        "retry the command; if the worker keeps failing, inspect server logs.",
                    ),
                    "runtime_error",
                    false,
                    false,
                ),
            }
        }
    }
}
