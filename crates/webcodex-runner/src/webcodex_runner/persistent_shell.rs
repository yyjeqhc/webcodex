#[cfg(windows)]
use super::config::{dialect_for_program, platform_default_dialect, ShellDialect};
use super::config::{
    validate_shell_config, RunnerPolicy, ShellConfig, ShellProfileConfig, SshConfig,
};
use super::projects::{find_project_shell_context_by_id, RunnerProjectShellContext};
#[cfg(any(unix, windows))]
use super::remote_shell::{remote_shell_bootstrap, RemoteShellTransport};
#[cfg(unix)]
use super::shell::shell_quote;
#[cfg(windows)]
use super::shell::shell_quote_powershell;
use super::shell::{base_shell_env, cwd_allowed};
use super::ssh::SshConnectionPool;
use std::path::{Path, PathBuf};
use std::time::Duration;
use webcodex_core::runner_operation::RunnerPersistentShellOperation;
#[cfg(test)]
use webcodex_core::runner_protocol::RunnerRequest;
use webcodex_core::runner_protocol::{
    PersistentShellRequest, PersistentShellResult, RAW_SHELL_COMMAND_MAX_BYTES,
};
#[cfg(any(unix, windows))]
use webcodex_persistent_shell::canonical_dialect;
use webcodex_persistent_shell::{
    PersistentShellManager as ProcessManager, ShellError, ShellExecResult, ShellIdentity,
    ShellLaunch, ShellLimits, ShellState, ShellSummary,
};

const EXECUTOR_AGENT: &str = "agent";
/// Remote named-SSH persistent shells use this executor on supported hosts.
#[cfg(any(unix, windows))]
const EXECUTOR_SSH: &str = "ssh";
const TERMINAL_RECORDS: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct PersistentShellManager {
    processes: ProcessManager,
    /// Runner-local SSH authority/preparation state. Windows persistent SSH uses
    /// the named-resource resolver without Unix ControlMaster multiplexing.
    ssh_pool: SshConnectionPool,
}

mod boundary;
mod launch;
mod local;
mod results;
mod ssh;

use boundary::{resolve_cwd, validate_boundary, validate_open_shell_boundary};
#[cfg(all(test, windows))]
use launch::build_launch_at_cwd;
use launch::{build_launch, selected_profile};
use results::{
    error_result, exec_result, shell_error_result, summary_error_result,
    summary_error_result_from_status, summary_result,
};

impl PersistentShellManager {
    pub(crate) fn new(shell: &ShellConfig, ssh_pool: super::ssh::SshConnectionPool) -> Self {
        Self {
            processes: ProcessManager::new(limits(shell)),
            ssh_pool,
        }
    }

    pub(crate) fn handle_operation(
        &self,
        policy: &RunnerPolicy,
        shell: &ShellConfig,
        ssh: &SshConfig,
        ssh_generation: u64,
        project_registry_dir: &Path,
        client_id: &str,
        request: &RunnerPersistentShellOperation,
    ) -> PersistentShellResult {
        self.processes.update_limits(limits(shell));
        let ssh_resource = request
            .job_context
            .as_ref()
            .and_then(|context| context.ssh_resource.as_deref());
        let operation = &request.request;
        if operation.action == "close" {
            return self.close(operation);
        }

        let project =
            match validate_boundary(policy, shell, project_registry_dir, client_id, operation) {
                Ok(project) => project,
                Err((code, message)) => {
                    if operation.action != "open" {
                        let _ = self.processes.close(
                            &operation.shell_id,
                            &operation.workflow_session_id,
                            &operation.runtime_project_id,
                            code,
                        );
                    }
                    return error_result(
                        &operation.shell_id,
                        &operation.workflow_session_id,
                        &operation.runtime_project_id,
                        code,
                        message,
                    );
                }
            };

        match operation.action.as_str() {
            "open" => {
                if let Some(resource) = ssh_resource {
                    self.open_ssh(
                        policy,
                        ssh,
                        ssh_generation,
                        client_id,
                        operation,
                        resource,
                        &project,
                    )
                } else {
                    self.open(policy, shell, client_id, operation, &project)
                }
            }
            "exec" => {
                if let Some(resource) = ssh_resource {
                    self.exec_ssh(policy, ssh, ssh_generation, operation, resource, &project)
                } else {
                    self.exec(policy, shell, operation, &project)
                }
            }
            "status" => {
                if let Some(resource) = ssh_resource {
                    self.status_ssh(ssh, ssh_generation, operation, resource)
                } else {
                    self.status(operation)
                }
            }
            _ => error_result(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                "persistent_shell_invalid_action",
                format!("unsupported persistent shell action '{}'", operation.action),
            ),
        }
    }

    fn status(&self, operation: &PersistentShellRequest) -> PersistentShellResult {
        match self.processes.status(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
        ) {
            Ok(summary) => {
                let state = if summary.busy { "executing" } else { "idle" };
                summary_result(summary, state, false)
            }
            Err(error) => shell_error_result(operation, error),
        }
    }

    fn close(&self, operation: &PersistentShellRequest) -> PersistentShellResult {
        let reason = operation.purpose.as_deref().unwrap_or("explicit_close");
        match self.processes.close(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
            reason,
        ) {
            Ok(result) => summary_result(result.summary, "closed", result.already_closed),
            Err(error) => shell_error_result(operation, error),
        }
    }

    pub(crate) fn close_project(&self, runtime_project_id: &str, reason: &str) -> usize {
        self.processes.close_project(runtime_project_id, reason)
    }

    #[cfg(test)]
    pub(crate) fn handle(
        &self,
        policy: &RunnerPolicy,
        shell: &ShellConfig,
        ssh: &SshConfig,
        ssh_generation: u64,
        project_registry_dir: &Path,
        request: &RunnerRequest,
    ) -> PersistentShellResult {
        let Some(operation) = request.persistent_shell.clone() else {
            return error_result(
                "",
                "",
                "",
                "persistent_shell_invalid_request",
                "persistent shell payload is required",
            );
        };
        self.handle_operation(
            policy,
            shell,
            ssh,
            ssh_generation,
            project_registry_dir,
            &request.client_id,
            &RunnerPersistentShellOperation {
                request: operation,
                job_context: request.job_context.clone(),
            },
        )
    }

    pub(crate) fn close_exact(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
        reason: &str,
    ) -> Result<(), ShellError> {
        self.processes
            .close(shell_id, workflow_session_id, runtime_project_id, reason)
            .map(|_| ())
    }

    pub(crate) fn close_all(&self, reason: &str) -> usize {
        self.processes.close_all(reason)
    }

    #[cfg(test)]
    pub(crate) fn active_count(&self) -> usize {
        self.processes.active_count()
    }
}

fn limits(shell: &ShellConfig) -> ShellLimits {
    ShellLimits {
        max_shells: shell.max_persistent_shells,
        idle_timeout: Duration::from_secs(shell.persistent_shell_idle_timeout_secs),
        max_terminal_records: TERMINAL_RECORDS,
    }
}

#[cfg(all(test, unix))]
#[path = "persistent_shell/tests/unix.rs"]
mod tests;
#[cfg(all(test, windows))]
#[path = "persistent_shell/tests/windows.rs"]
mod windows_tests;
