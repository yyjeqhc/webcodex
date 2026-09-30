//! Local open/exec with current project and profile boundary checks.

use super::*;

impl PersistentShellManager {
    pub(super) fn open(
        &self,
        policy: &RunnerPolicy,
        shell: &ShellConfig,
        client_id: &str,
        operation: &PersistentShellRequest,
        project: &RunnerProjectShellContext,
    ) -> PersistentShellResult {
        let launch = match build_launch(policy, shell, client_id, operation, project) {
            Ok(launch) => launch,
            Err((code, message)) => {
                return error_result(
                    &operation.shell_id,
                    &operation.workflow_session_id,
                    &operation.runtime_project_id,
                    code,
                    message,
                )
            }
        };
        match self.processes.open(launch) {
            Ok(summary) => {
                if let Err((code, message)) =
                    validate_open_shell_boundary(policy, shell, project, &summary)
                {
                    let terminal = self.processes.close(
                        &operation.shell_id,
                        &operation.workflow_session_id,
                        &operation.runtime_project_id,
                        code,
                    );
                    let mut result = terminal
                        .map(|closed| {
                            summary_result(closed.summary, "rejected", closed.already_closed)
                        })
                        .unwrap_or_else(|_| {
                            error_result(
                                &operation.shell_id,
                                &operation.workflow_session_id,
                                &operation.runtime_project_id,
                                code,
                                &message,
                            )
                        });
                    result.error_code = Some(code.to_string());
                    result.error = Some(message);
                    result
                } else {
                    summary_result(summary, "opened", false)
                }
            }
            Err(error) => shell_error_result(operation, error),
        }
    }

    pub(super) fn exec(
        &self,
        policy: &RunnerPolicy,
        shell: &ShellConfig,
        operation: &PersistentShellRequest,
        project: &RunnerProjectShellContext,
    ) -> PersistentShellResult {
        let summary = match self.processes.status(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
        ) {
            Ok(summary) => summary,
            Err(error) => return shell_error_result(operation, error),
        };
        if let Err((code, message)) = validate_open_shell_boundary(policy, shell, project, &summary)
        {
            let _ = self.processes.close(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                code,
            );
            return error_result(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                code,
                message,
            );
        }
        let command = match operation.command.as_deref() {
            Some(command) => command,
            None => {
                return summary_error_result(
                    summary,
                    "persistent_shell_invalid_request",
                    "command is required for persistent shell exec",
                )
            }
        };
        if command.len() > RAW_SHELL_COMMAND_MAX_BYTES {
            return summary_error_result(
                summary,
                "persistent_shell_invalid_command",
                format!("command exceeds the {RAW_SHELL_COMMAND_MAX_BYTES}-byte Runner limit"),
            );
        }
        let timeout_secs = operation.timeout_secs.unwrap_or(30);
        if timeout_secs == 0 || timeout_secs > policy.max_timeout_secs {
            return summary_error_result(
                summary,
                "persistent_shell_invalid_timeout",
                format!(
                    "timeout_secs must be between 1 and {}",
                    policy.max_timeout_secs
                ),
            );
        }
        if let Err(error) = self.processes.set_output_limit(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
            policy.max_output_bytes,
        ) {
            return shell_error_result(operation, error);
        }
        match self.processes.exec(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
            command,
            Duration::from_secs(timeout_secs),
        ) {
            Ok(result) => self.finalize_exec(policy, shell, operation, project, result),
            Err(error) => match self.processes.status(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
            ) {
                Ok(summary) => summary_error_result(summary, error.code, error.message),
                Err(_) => shell_error_result(operation, error),
            },
        }
    }

    fn finalize_exec(
        &self,
        policy: &RunnerPolicy,
        shell: &ShellConfig,
        operation: &PersistentShellRequest,
        project: &RunnerProjectShellContext,
        mut result: ShellExecResult,
    ) -> PersistentShellResult {
        if result.shell_state != ShellState::Running {
            return exec_result(operation, result);
        }
        let boundary = self
            .processes
            .status(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
            )
            .map_err(|error| (error.code, error.message))
            .and_then(|summary| validate_open_shell_boundary(policy, shell, project, &summary));
        if let Err((code, message)) = boundary {
            let terminal_state = self
                .processes
                .close(
                    &operation.shell_id,
                    &operation.workflow_session_id,
                    &operation.runtime_project_id,
                    code,
                )
                .map(|closed| closed.summary.state)
                .unwrap_or(ShellState::Poisoned);
            result.shell_state = terminal_state;
            result.error_code = Some("shell_reset_required".to_string());
            result.error = Some(format!(
                "{message}; persistent shell was closed before another command could run"
            ));
        }
        exec_result(operation, result)
    }
}
