//! Named SSH open/exec/status and immutable transport-binding validation.

use super::*;

impl PersistentShellManager {
    #[cfg(any(unix, windows))]
    pub(super) fn open_ssh(
        &self,
        policy: &RunnerPolicy,
        ssh: &SshConfig,
        ssh_generation: u64,
        client_id: &str,
        operation: &PersistentShellRequest,
        resource_name: &str,
        _project: &RunnerProjectShellContext,
    ) -> PersistentShellResult {
        if !policy.allow_raw_shell {
            return error_result(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                "raw_shell_disabled",
                "persistent shells are disabled by the current Runner raw shell policy",
            );
        }
        let explicit = operation.shell.as_deref();
        if explicit.is_some_and(|dialect| !matches!(dialect, "sh" | "bash")) {
            return error_result(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                "persistent_shell_dialect_unsupported",
                "persistent shell must be 'sh' or 'bash'",
            );
        }
        let shell_program = explicit.unwrap_or("sh");
        let dialect = canonical_dialect(shell_program).unwrap_or("sh").to_string();
        let requested_cwd = operation
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|cwd| !cwd.is_empty())
            .map(str::to_string);
        let (transport, default_cwd) = match RemoteShellTransport::spawn(
            &self.ssh_pool,
            ssh_generation,
            ssh,
            resource_name,
            &operation.workflow_session_id,
            shell_program,
            policy.max_output_bytes,
        ) {
            Ok((transport, default_cwd)) => (transport, default_cwd),
            Err(error) => {
                return error_result(
                    &operation.shell_id,
                    &operation.workflow_session_id,
                    &operation.runtime_project_id,
                    error.code,
                    error.message,
                )
            }
        };
        // Remote cwd priority: explicit open cwd > Session/runner-provided cwd
        // > SSH resource default_cwd > remote login default. The effective cwd
        // is applied to the remote shell's bootstrap below; `initial_cwd` only
        // seeds the summary until the first control frame reports the trusted
        // absolute cwd. When nothing is requested, no `cd` is issued and the
        // shell keeps the remote login directory (never a fabricated `/`).
        let effective_cwd = requested_cwd.or(default_cwd);
        let initial_cwd = effective_cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_default();
        let identity = ShellIdentity {
            shell_id: operation.shell_id.clone(),
            workflow_session_id: operation.workflow_session_id.clone(),
            runtime_project_id: operation.runtime_project_id.clone(),
            executor: EXECUTOR_SSH.to_string(),
            client_id: Some(client_id.to_string()),
        };
        // The bootstrap is the initialization command: it reserves FD 7/8 on the
        // remote shell (so the shared command wrapper's markers always reach the
        // Runner regardless of later user redirects) and applies the effective
        // remote cwd, runs once, drains, and its control frame reports the
        // trusted absolute cwd used to seed the shell. A failed `cd` makes the
        // initialization control frame report a non-zero status, so open fails
        // and the remote shell is torn down instead of falling back to the
        // login directory. It never reaches later commands. No local profile
        // env or init script is sent to the remote host.
        let initialization = remote_shell_bootstrap(effective_cwd.as_deref());
        let transport_box: Box<dyn webcodex_persistent_shell::ShellTransport> = Box::new(transport);
        match self.processes.open_with_transport(
            identity,
            dialect,
            None,
            initial_cwd,
            Some(initialization),
            transport_box,
        ) {
            Ok(summary) => summary_result(summary, "opened", false),
            Err(error) => shell_error_result(operation, error),
        }
    }

    #[cfg(not(any(unix, windows)))]
    pub(super) fn open_ssh(
        &self,
        _policy: &RunnerPolicy,
        _ssh: &SshConfig,
        _ssh_generation: u64,
        _client_id: &str,
        operation: &PersistentShellRequest,
        _resource_name: &str,
        _project: &RunnerProjectShellContext,
    ) -> PersistentShellResult {
        error_result(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
            "persistent_shell_unsupported",
            "named SSH persistent shell is not supported on this Runner host",
        )
    }

    pub(super) fn exec_ssh(
        &self,
        policy: &RunnerPolicy,
        ssh: &SshConfig,
        ssh_generation: u64,
        operation: &PersistentShellRequest,
        resource_name: &str,
        _project: &RunnerProjectShellContext,
    ) -> PersistentShellResult {
        if !policy.allow_raw_shell {
            return error_result(
                &operation.shell_id,
                &operation.workflow_session_id,
                &operation.runtime_project_id,
                "raw_shell_disabled",
                "persistent shells are disabled by the current Runner raw shell policy",
            );
        }
        if let Err((code, message)) = validate_ssh_binding_current(
            &self.processes,
            ssh,
            ssh_generation,
            operation,
            resource_name,
        ) {
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
                return summary_error_result_from_status(
                    &self.processes,
                    operation,
                    "persistent_shell_invalid_request",
                    "command is required for persistent shell exec",
                )
            }
        };
        if command.len() > RAW_SHELL_COMMAND_MAX_BYTES {
            return summary_error_result_from_status(
                &self.processes,
                operation,
                "persistent_shell_invalid_command",
                format!("command exceeds the {RAW_SHELL_COMMAND_MAX_BYTES}-byte Runner limit"),
            );
        }
        let timeout_secs = operation.timeout_secs.unwrap_or(30);
        if timeout_secs == 0 || timeout_secs > policy.max_timeout_secs {
            return summary_error_result_from_status(
                &self.processes,
                operation,
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
            Ok(result) => exec_result(operation, result),
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

    pub(super) fn status_ssh(
        &self,
        ssh: &SshConfig,
        ssh_generation: u64,
        operation: &PersistentShellRequest,
        resource_name: &str,
    ) -> PersistentShellResult {
        if let Err((code, message)) = validate_ssh_binding_current(
            &self.processes,
            ssh,
            ssh_generation,
            operation,
            resource_name,
        ) {
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
        self.status(operation)
    }
}

/// Confirm the SSH binding this shell opened against is still current. The
/// binding captures the named resource and the Runner configuration generation
/// at open time (stored as transport metadata on the shared shell entry), and
/// every later `exec`/`status` on an *active* shell compares it against the
/// active config. A removed resource, an unknown active generation, or a
/// generation that advanced past the opened one invalidates an already-open
/// remote shell: it must be closed and reopened rather than reused against a
/// stale transport. A terminal shell (closed, exited, poisoned, lost) has
/// already released its binding and simply reports its terminal state through
/// the shared state machine.
fn validate_ssh_binding_current(
    processes: &ProcessManager,
    ssh: &SshConfig,
    ssh_generation: u64,
    operation: &PersistentShellRequest,
    resource_name: &str,
) -> Result<(), (&'static str, String)> {
    let summary = match processes.status(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
    ) {
        Ok(summary) => summary,
        Err(error) => return Err((error.code, error.message)),
    };
    if !matches!(summary.state, ShellState::Opening | ShellState::Running) {
        // The shell cannot run a command anymore; leave its terminal state and
        // the Server record intact instead of forcing a reset.
        return Ok(());
    }
    let metadata = match processes.metadata(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
    ) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err((error.code, error.message));
        }
    };
    let opened_generation = match metadata.as_ref().and_then(|metadata| metadata.generation) {
        Some(generation) => generation,
        None => {
            // No transport binding was recorded: the shell was not opened as an
            // SSH persistent shell (or has already gone terminal). Never run a
            // user command against a shell whose binding cannot be verified.
            return Err((
                "shell_reset_required",
                "persistent shell has no recorded SSH binding; close and reopen it".to_string(),
            ));
        }
    };
    if metadata
        .as_ref()
        .and_then(|metadata| metadata.resource.as_deref())
        != Some(resource_name)
    {
        return Err((
            "shell_reset_required",
            format!(
                "persistent shell is bound to a different SSH resource than requested; close and reopen it (expected '{}')",
                resource_name
            ),
        ));
    }
    if !ssh.resources.contains_key(resource_name) {
        return Err((
            "shell_reset_required",
            format!(
                "SSH resource '{}' is no longer configured on this Runner; close and reopen the persistent shell",
                resource_name
            ),
        ));
    }
    if ssh_generation == 0 {
        return Err((
            "shell_reset_required",
            "SSH configuration generation is unknown; close and reopen the persistent shell"
                .to_string(),
        ));
    }
    if opened_generation != ssh_generation {
        return Err((
            "shell_reset_required",
            format!(
                "SSH resource '{}' changed to configuration generation {ssh_generation} after this persistent shell was opened at generation {opened_generation}; close and reopen the persistent shell",
                resource_name
            ),
        ));
    }
    Ok(())
}
