use serde_json::json;
use std::time::Duration;

use super::helpers::{resolve_local_cwd, run_command_sync};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::ShellRunRequest;

impl ToolRuntime {
    pub(crate) async fn run_shell(
        &self,
        project: String,
        command: String,
        timeout_secs: Option<u64>,
        cwd: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let timeout = timeout_secs.unwrap_or(60).max(1);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
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
                Err(e) => return ToolResult::err(e),
            };
            match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(response)) => {
                    let success = response.error.is_none() && response.exit_code == Some(0);
                    let output = json!({
                        "exit_code": response.exit_code,
                        "stdout": response.stdout,
                        "stderr": response.stderr,
                        "duration_ms": response.duration_ms,
                    });
                    if success {
                        ToolResult::ok(output)
                    } else {
                        ToolResult::err(
                            response
                                .error
                                .unwrap_or_else(|| "command failed".to_string()),
                        )
                    }
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("shell request waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err(format!(
                        "timed out waiting {} seconds for agent shell result",
                        wait_timeout
                    ))
                }
            }
        } else {
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            };
            let result = tokio::task::spawn_blocking({
                let cmd = command;
                move || run_command_sync(&cmd, &cwd_path, timeout)
            })
            .await;
            match result {
                Ok((exit_code, stdout, stderr, duration_ms)) => {
                    let success = exit_code == 0;
                    let output = json!({
                        "exit_code": exit_code,
                        "stdout": stdout,
                        "stderr": stderr,
                        "duration_ms": duration_ms,
                    });
                    if success {
                        ToolResult::ok(output)
                    } else {
                        ToolResult::err(format!("command exited with code {}", exit_code))
                    }
                }
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }
}
