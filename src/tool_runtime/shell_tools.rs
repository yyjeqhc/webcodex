//! Runtime dispatch adapter for shell tool calls.

use super::project_resolution::ResolvedProject;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_shell_tool(
        &self,
        call: ToolCall,
        target: ResolvedProject,
        ssh_resource: Option<&str>,
        validation_assertion_name: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::RunProcess {
                interactive,
                project: _,
                executable,
                args,
                stdin,
                session_id,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
            } => {
                if interactive {
                    return self
                        .run_interactive_process(
                            target,
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
                        .await;
                }
                self.run_process_with_contract_for_resource(
                    target,
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
                project: _,
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
                    target,
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
                project: _,
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
                    target,
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
                project: _,
                command,
                session_id,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                shell,
                login,
            } => {
                self.run_shell_with_contract_for_resource(
                    std::future::ready(Ok(target)),
                    command,
                    timeout_secs,
                    sync_wait_secs,
                    cwd,
                    purpose,
                    shell,
                    login,
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
