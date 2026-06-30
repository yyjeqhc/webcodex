use serde_json::json;
use std::time::Duration;

use super::helpers::{
    bounded_tail, command_failed_message, command_rejected_message, command_timeout_message,
    looks_like_command_timeout, resolve_local_cwd, run_command_sync, COMMAND_STDIO_TAIL_CHARS,
};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::ShellRunRequest;

pub(crate) struct ProjectCommandOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration_ms: u64,
}

impl ToolRuntime {
    fn run_shell_failure_result(
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
        timeout_secs: u64,
    ) -> ToolResult {
        let (stdout_tail, stdout_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
        let (stderr_tail, stderr_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
        let output = json!({
            "exit_code": exit_code,
            "duration_ms": duration_ms,
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
        });
        let error = if looks_like_command_timeout(exit_code, &stderr, timeout_secs) {
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
            Err(e) => return ToolResult::err(command_rejected_message(
                e.to_message(),
                "verify the project id with list_projects, then retry with a registered project.",
            )),
        };
        let timeout = timeout_secs.unwrap_or(60).max(1);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "refresh the agent project registry with list_projects, then retry.",
                    ))
                }
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
                    return ToolResult::err(command_rejected_message(
                        e,
                        "confirm the agent is connected and the command request is allowed, then retry or use run_job for long-running work.",
                    ))
                }
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let success = response.error.is_none() && response.exit_code == Some(0);
                    if success {
                        ToolResult::ok(json!({
                            "exit_code": response.exit_code,
                            "stdout": response.stdout,
                            "stderr": response.stderr,
                            "duration_ms": response.duration_ms,
                        }))
                    } else if let Some(error) = response.error {
                        ToolResult::err(command_rejected_message(
                            error,
                            "inspect the rejection reason, adjust the cwd/command/project, then retry.",
                        ))
                    } else {
                        Self::run_shell_failure_result(
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
                    ToolResult::err(command_rejected_message(
                        "shell request waiter was dropped before a result was returned",
                        "check agent connectivity, then retry or use run_job for recoverable long-running work.",
                    ))
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err(command_timeout_message(wait_timeout, "", ""))
                }
            }
        } else {
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(path) => path,
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "read the project root and choose an existing project-relative cwd, then retry.",
                    ))
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
                        ToolResult::ok(json!({
                            "exit_code": exit_code,
                            "stdout": stdout,
                            "stderr": stderr,
                            "duration_ms": duration_ms,
                        }))
                    } else {
                        Self::run_shell_failure_result(
                            Some(exit_code),
                            stdout,
                            stderr,
                            Some(duration_ms),
                            timeout,
                        )
                    }
                }
                Err(e) => ToolResult::err(command_rejected_message(
                    format!("task join error: {}", e),
                    "retry the command; if the worker keeps failing, inspect server logs.",
                )),
            }
        }
    }
}
