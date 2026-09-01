//! Runtime dispatch adapter for shell tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_shell_tool(
        &self,
        call: ToolCall,
        ssh_resource: Option<&str>,
        validation_assertion_name: Option<&str>,
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
                sync_wait_secs,
                cwd,
                purpose,
            } => {
                self.run_process_with_contract_for_resource(
                    project,
                    executable,
                    args,
                    stdin,
                    timeout_secs,
                    sync_wait_secs,
                    cwd,
                    purpose,
                    ssh_resource,
                    session_id,
                    validation_assertion_name,
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
                sync_wait_secs,
                cwd,
                purpose,
            } => {
                self.run_script_with_contract_for_resource(
                    project,
                    language,
                    script,
                    args,
                    stdin,
                    timeout_secs,
                    sync_wait_secs,
                    cwd,
                    purpose,
                    ssh_resource,
                    session_id,
                    validation_assertion_name,
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
                self.run_shell_with_contract_for_resource(
                    project,
                    command,
                    timeout_secs,
                    cwd,
                    purpose,
                    shell,
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
