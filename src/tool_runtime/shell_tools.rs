//! Runtime dispatch adapter for shell tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_shell_tool(
        &self,
        call: ToolCall,
        sandbox: Option<&str>,
        ssh_resource: Option<&str>,
    ) -> ToolResult {
        match call {
            ToolCall::RunProcess {
                project,
                executable,
                args,
                stdin,
                session_id: _,
                timeout_secs,
                cwd,
                purpose,
            } => {
                self.run_process_with_contract_in_sandbox(
                    project,
                    executable,
                    args,
                    stdin,
                    timeout_secs,
                    cwd,
                    purpose,
                    sandbox,
                    ssh_resource,
                )
                .await
            }
            ToolCall::RunShell {
                project,
                command,
                session_id,
                timeout_secs,
                cwd,
                purpose,
                shell,
            } => {
                self.run_shell_with_contract_in_sandbox(
                    project,
                    command,
                    timeout_secs,
                    cwd,
                    purpose,
                    shell,
                    sandbox,
                    ssh_resource,
                    session_id.as_deref(),
                )
                .await
            }
            _ => unreachable!("non-shell tool routed to shell dispatcher"),
        }
    }
}
