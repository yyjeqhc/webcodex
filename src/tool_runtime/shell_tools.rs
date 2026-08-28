//! Runtime dispatch adapter for shell tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_shell_tool(
        &self,
        call: ToolCall,
        sandbox: Option<&str>,
        ssh_resource: Option<&str>,
        validation_assertion_identity: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::RunProcess {
                project,
                executable,
                args,
                stdin,
                session_id,
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
                    session_id,
                    validation_assertion_identity,
                    auth,
                )
                .await
            }
            ToolCall::RunDetachedProcess {
                project,
                idempotency_key,
                executable,
                args,
                stdin,
                session_id,
                timeout_secs,
                cwd,
                purpose,
            } => {
                self.run_detached_process_with_contract(
                    project,
                    idempotency_key,
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
                )
                .await
            }
            ToolCall::RunScript {
                project,
                language,
                script,
                args,
                stdin,
                session_id,
                timeout_secs,
                cwd,
                purpose,
            } => {
                self.run_script_with_contract_in_sandbox(
                    project,
                    language,
                    script,
                    args,
                    stdin,
                    timeout_secs,
                    cwd,
                    purpose,
                    sandbox,
                    ssh_resource,
                    session_id,
                    validation_assertion_identity,
                    auth,
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
                    auth,
                )
                .await
            }
            _ => unreachable!("non-shell tool routed to shell dispatcher"),
        }
    }
}
