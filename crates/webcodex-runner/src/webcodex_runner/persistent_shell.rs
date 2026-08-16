use super::config::{
    validate_shell_config, AgentPolicy, ShellConfig, ShellProfileConfig, SshConfig,
};
use super::projects::{find_project_shell_context_by_id, AgentProjectShellContext};
#[cfg(unix)]
use super::remote_shell::{remote_shell_bootstrap, RemoteShellTransport};
use super::shell::{base_shell_env, cwd_allowed, shell_quote};
#[cfg(unix)]
use super::ssh::SshConnectionPool;
use crate::shell_protocol::{
    PersistentShellRequest, PersistentShellResult, ShellAgentShellRequest,
    RAW_SHELL_COMMAND_MAX_BYTES,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use webcodex_persistent_shell::{
    canonical_dialect, PersistentShellManager as ProcessManager, ShellError, ShellExecResult,
    ShellIdentity, ShellLaunch, ShellLimits, ShellState, ShellSummary,
};

const EXECUTOR_AGENT: &str = "agent";
/// Only the Unix remote (SSH) transport opens shells under this executor id.
#[cfg(unix)]
const EXECUTOR_SSH: &str = "ssh";
const TERMINAL_RECORDS: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct PersistentShellManager {
    processes: ProcessManager,
    /// Pool reused only by the Unix remote (SSH) persistent shell transport.
    #[cfg(unix)]
    ssh_pool: SshConnectionPool,
}

impl PersistentShellManager {
    /// `ssh_pool` is consumed only by the Unix remote (SSH) transport. The
    /// parameter stays on every platform so the constructor signature is
    /// identical across targets; Windows names it with a leading underscore
    /// because it is intentionally unused there.
    pub(crate) fn new(
        shell: &ShellConfig,
        #[cfg(unix)] ssh_pool: super::ssh::SshConnectionPool,
        #[cfg(not(unix))] _ssh_pool: super::ssh::SshConnectionPool,
    ) -> Self {
        Self {
            processes: ProcessManager::new(limits(shell)),
            #[cfg(unix)]
            ssh_pool,
        }
    }

    pub(crate) fn handle(
        &self,
        policy: &AgentPolicy,
        shell: &ShellConfig,
        ssh: &SshConfig,
        ssh_generation: u64,
        projects_dir: &Path,
        request: &ShellAgentShellRequest,
    ) -> PersistentShellResult {
        self.processes.update_limits(limits(shell));
        let ssh_resource = request
            .job_context
            .as_ref()
            .and_then(|context| context.ssh_resource.as_deref());
        let Some(operation) = request.persistent_shell.as_ref() else {
            return error_result(
                "",
                "",
                "",
                "persistent_shell_invalid_request",
                "persistent shell payload is required",
            );
        };
        if operation.action == "close" {
            return self.close(operation);
        }

        let project =
            match validate_boundary(policy, shell, projects_dir, &request.client_id, operation) {
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
                        request,
                        operation,
                        resource,
                        &project,
                    )
                } else {
                    self.open(policy, shell, request, operation, &project)
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

    #[cfg(unix)]
    fn open_ssh(
        &self,
        policy: &AgentPolicy,
        ssh: &SshConfig,
        ssh_generation: u64,
        request: &ShellAgentShellRequest,
        operation: &PersistentShellRequest,
        resource_name: &str,
        _project: &AgentProjectShellContext,
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
            client_id: Some(request.client_id.clone()),
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

    /// Windows has no SSH persistent shell (and no persistent shell at all
    /// yet). Fail closed with a stable error; the Runner never advertises
    /// `ssh_persistent_shell` on Windows.
    #[cfg(not(unix))]
    fn open_ssh(
        &self,
        _policy: &AgentPolicy,
        _ssh: &SshConfig,
        _ssh_generation: u64,
        _request: &ShellAgentShellRequest,
        operation: &PersistentShellRequest,
        _resource_name: &str,
        _project: &AgentProjectShellContext,
    ) -> PersistentShellResult {
        error_result(
            &operation.shell_id,
            &operation.workflow_session_id,
            &operation.runtime_project_id,
            "persistent_shell_unsupported",
            "persistent shell is not supported on Windows yet",
        )
    }

    fn exec_ssh(
        &self,
        policy: &AgentPolicy,
        ssh: &SshConfig,
        ssh_generation: u64,
        operation: &PersistentShellRequest,
        resource_name: &str,
        _project: &AgentProjectShellContext,
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

    fn status_ssh(
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

    fn open(
        &self,
        policy: &AgentPolicy,
        shell: &ShellConfig,
        request: &ShellAgentShellRequest,
        operation: &PersistentShellRequest,
        project: &AgentProjectShellContext,
    ) -> PersistentShellResult {
        let launch = match build_launch(policy, shell, request, operation, project) {
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

    fn exec(
        &self,
        policy: &AgentPolicy,
        shell: &ShellConfig,
        operation: &PersistentShellRequest,
        project: &AgentProjectShellContext,
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
        policy: &AgentPolicy,
        shell: &ShellConfig,
        operation: &PersistentShellRequest,
        project: &AgentProjectShellContext,
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

    #[cfg(all(test, unix))]
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

fn validate_boundary(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    client_id: &str,
    operation: &PersistentShellRequest,
) -> Result<AgentProjectShellContext, (&'static str, String)> {
    if !policy.allow_raw_shell {
        return Err((
            "raw_shell_disabled",
            "persistent shells are disabled by the current Runner raw shell policy".to_string(),
        ));
    }
    validate_shell_config(shell).map_err(|message| ("persistent_shell_config_invalid", message))?;
    let prefix = format!("agent:{client_id}:");
    let project_id = operation
        .runtime_project_id
        .strip_prefix(&prefix)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            (
                "persistent_shell_project_mismatch",
                "runtime project does not belong to this Runner".to_string(),
            )
        })?;
    find_project_shell_context_by_id(projects_dir, project_id).ok_or_else(|| {
        (
            "persistent_shell_project_unavailable",
            "project is disabled, unregistered, or not executable by this Runner".to_string(),
        )
    })
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

fn resolve_cwd(
    project: &AgentProjectShellContext,
    requested: Option<&str>,
) -> Result<PathBuf, (&'static str, String)> {
    let root = PathBuf::from(&project.path)
        .canonicalize()
        .map_err(|error| {
            (
                "persistent_shell_project_unavailable",
                format!("failed to access project root: {error}"),
            )
        })?;
    let requested = requested
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let requested = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    };
    let cwd = requested.canonicalize().map_err(|error| {
        (
            "persistent_shell_cwd_invalid",
            format!("failed to access persistent shell cwd: {error}"),
        )
    })?;
    if cwd != root && !cwd.starts_with(&root) {
        return Err((
            "persistent_shell_cwd_outside_project",
            "persistent shell cwd is outside the registered project root".to_string(),
        ));
    }
    Ok(cwd)
}

fn selected_profile<'a>(
    shell: &'a ShellConfig,
    project: &AgentProjectShellContext,
) -> Result<(Option<String>, Option<&'a ShellProfileConfig>), (&'static str, String)> {
    let name = project
        .shell_profile
        .as_deref()
        .or(shell.default_profile.as_deref());
    match name {
        Some(name) => shell
            .profiles
            .get(name)
            .map(|profile| (Some(name.to_string()), Some(profile)))
            .ok_or_else(|| {
                (
                    "persistent_shell_profile_unavailable",
                    format!("shell profile '{name}' is not configured"),
                )
            }),
        None => Ok((None, None)),
    }
}

fn build_launch(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    request: &ShellAgentShellRequest,
    operation: &PersistentShellRequest,
    project: &AgentProjectShellContext,
) -> Result<ShellLaunch, (&'static str, String)> {
    let cwd = resolve_cwd(project, operation.cwd.as_deref())?;
    cwd_allowed(policy, &cwd).map_err(|message| ("persistent_shell_cwd_denied", message))?;
    build_launch_at_cwd(
        shell,
        request,
        operation,
        project,
        cwd,
        policy.max_output_bytes,
    )
}

fn build_launch_at_cwd(
    shell: &ShellConfig,
    request: &ShellAgentShellRequest,
    operation: &PersistentShellRequest,
    project: &AgentProjectShellContext,
    cwd: PathBuf,
    max_output_bytes: usize,
) -> Result<ShellLaunch, (&'static str, String)> {
    let (profile_name, profile) = selected_profile(shell, project)?;
    let explicit = operation.shell.as_deref();
    if explicit.is_some_and(|dialect| !matches!(dialect, "sh" | "bash")) {
        return Err((
            "persistent_shell_dialect_unsupported",
            "persistent shell must be 'sh' or 'bash'".to_string(),
        ));
    }
    let program = explicit
        .map(str::to_string)
        .or_else(|| profile.and_then(|profile| profile.program.clone()))
        .unwrap_or_else(|| shell.program.clone());
    let dialect = canonical_dialect(&program).ok_or_else(|| {
        (
            "persistent_shell_dialect_unsupported",
            "configured persistent shell program must resolve to sh or bash".to_string(),
        )
    })?;
    let empty_profile = ShellProfileConfig::default();
    let env = base_shell_env(shell, profile.unwrap_or(&empty_profile))
        .map_err(|message| ("persistent_shell_environment_invalid", message))?;
    let initialization = match profile {
        Some(profile) => profile.init_script.clone(),
        None => shell
            .init_script
            .as_ref()
            .map(|path| format!(". {}", shell_quote(&path.to_string_lossy()))),
    };
    let args = if dialect == "bash" {
        vec!["--noprofile".to_string(), "--norc".to_string()]
    } else {
        Vec::new()
    };
    Ok(ShellLaunch {
        identity: ShellIdentity {
            shell_id: operation.shell_id.clone(),
            workflow_session_id: operation.workflow_session_id.clone(),
            runtime_project_id: operation.runtime_project_id.clone(),
            executor: EXECUTOR_AGENT.to_string(),
            client_id: Some(request.client_id.clone()),
        },
        dialect: dialect.to_string(),
        profile: profile_name,
        program,
        args,
        initial_cwd: cwd,
        env,
        initialization,
        max_output_bytes,
    })
}

fn validate_open_shell_boundary(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    project: &AgentProjectShellContext,
    summary: &ShellSummary,
) -> Result<(), (&'static str, String)> {
    let cwd = resolve_cwd(project, Some(&summary.cwd.to_string_lossy()))?;
    cwd_allowed(policy, &cwd).map_err(|message| ("persistent_shell_cwd_denied", message))?;
    let (profile_name, _) = selected_profile(shell, project)?;
    if summary.profile != profile_name {
        return Err((
            "shell_reset_required",
            "the project shell profile changed; close and reopen the persistent shell".to_string(),
        ));
    }
    Ok(())
}

fn summary_result(
    summary: ShellSummary,
    execution_state: &str,
    already_closed: bool,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: summary.identity.shell_id,
        workflow_session_id: summary.identity.workflow_session_id,
        runtime_project_id: summary.identity.runtime_project_id,
        shell_state: summary.state.as_str().to_string(),
        execution_state: execution_state.to_string(),
        command_started: false,
        command_completed: false,
        exit_code: summary.exit_code,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 0,
        cwd: Some(summary.cwd.to_string_lossy().to_string()),
        initial_cwd: Some(summary.initial_cwd.to_string_lossy().to_string()),
        shell: Some(summary.dialect),
        profile: summary.profile,
        created_at: Some(summary.created_at),
        last_activity_at: Some(summary.last_activity_at),
        busy: summary.busy,
        already_closed,
        close_reason: summary.close_reason,
        error_code: None,
        error: None,
    }
}

fn exec_result(
    operation: &PersistentShellRequest,
    result: ShellExecResult,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: result.shell_id,
        workflow_session_id: operation.workflow_session_id.clone(),
        runtime_project_id: operation.runtime_project_id.clone(),
        shell_state: result.shell_state.as_str().to_string(),
        execution_state: result.execution_state,
        command_started: result.command_started,
        command_completed: result.command_completed,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        stdout_truncated: result.stdout_truncated,
        stderr_truncated: result.stderr_truncated,
        duration_ms: result.duration_ms,
        cwd: Some(result.cwd.to_string_lossy().to_string()),
        initial_cwd: None,
        shell: None,
        profile: None,
        created_at: None,
        last_activity_at: None,
        busy: false,
        already_closed: false,
        close_reason: None,
        error_code: result.error_code,
        error: result.error,
    }
}

fn shell_error_result(
    operation: &PersistentShellRequest,
    error: ShellError,
) -> PersistentShellResult {
    error_result(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
        error.code,
        error.message,
    )
}

fn summary_error_result(
    summary: ShellSummary,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    let mut result = summary_result(summary, "rejected", false);
    result.error_code = Some(code.to_string());
    result.error = Some(message.into());
    result
}

/// Like `summary_error_result` but fetches the current status from the manager
/// first, so an SSH exec rejection still reports the authoritative running
/// state (and closes a stale shell) the way the local path does.
fn summary_error_result_from_status(
    processes: &ProcessManager,
    operation: &PersistentShellRequest,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    match processes.status(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
    ) {
        Ok(summary) => summary_error_result(summary, code, message),
        Err(error) => shell_error_result(operation, error),
    }
}

fn error_result(
    shell_id: &str,
    workflow_session_id: &str,
    runtime_project_id: &str,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: shell_id.to_string(),
        workflow_session_id: workflow_session_id.to_string(),
        runtime_project_id: runtime_project_id.to_string(),
        shell_state: if code == "shell_reset_required" {
            ShellState::Poisoned.as_str().to_string()
        } else {
            "unknown".to_string()
        },
        execution_state: "rejected".to_string(),
        command_started: false,
        command_completed: false,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 0,
        cwd: None,
        initial_cwd: None,
        shell: None,
        profile: None,
        created_at: None,
        last_activity_at: None,
        busy: false,
        already_closed: false,
        close_reason: None,
        error_code: Some(code.to_string()),
        error: Some(message.into()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::shell_protocol::ShellAgentShellRequest;
    use std::collections::BTreeMap;

    fn request(action: &str, shell_id: &str, command: Option<&str>) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: format!("req-{action}"),
            client_id: "agent-1".to_string(),
            kind: "persistent_shell".to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: command.unwrap_or_default().to_string(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 5,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: Some(PersistentShellRequest {
                action: action.to_string(),
                shell_id: shell_id.to_string(),
                workflow_session_id: "wc_sess_test".to_string(),
                runtime_project_id: "agent:agent-1:demo".to_string(),
                cwd: None,
                shell: Some("bash".to_string()),
                command: command.map(str::to_string),
                timeout_secs: Some(5),
                purpose: None,
            }),
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, AgentPolicy) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let projects = temp.path().join("projects.d");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(project.join("sub")).unwrap();
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(
            projects.join("demo.toml"),
            format!("id = \"demo\"\npath = \"{}\"\n", project.display()),
        )
        .unwrap();
        let policy = AgentPolicy {
            allow_raw_shell: true,
            allow_cwd_anywhere: false,
            allowed_roots: vec![project.clone()],
            max_timeout_secs: 30,
            max_output_bytes: 16 * 1024,
        };
        (temp, project, projects, policy)
    }

    #[test]
    fn runner_preserves_state_and_rechecks_raw_shell_policy() {
        let (_temp, _project, projects, policy) = fixture();
        let shell = ShellConfig::default();
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        let mut denied_open_policy = policy.clone();
        denied_open_policy.allow_raw_shell = false;
        let denied_open = manager.handle(
            &denied_open_policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request("open", "wc_shell_denied_open", None),
        );
        assert_eq!(
            denied_open.error_code.as_deref(),
            Some("raw_shell_disabled")
        );
        assert_eq!(manager.active_count(), 0);

        let opened = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request("open", "wc_shell_runner", None),
        );
        assert_eq!(opened.shell_state, "running");

        let exported = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_runner",
                Some("export WC_RUNNER_STATE=ready; cd sub; wc_runner_fn() { printf fn; }"),
            ),
        );
        assert_eq!(exported.exit_code, Some(0));
        let observed = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_runner",
                Some("printf '%s:%s:' \"$WC_RUNNER_STATE\" \"$PWD\"; wc_runner_fn"),
            ),
        );
        assert!(observed.stdout.starts_with("ready:"));
        assert!(observed.stdout.ends_with(":fn"));

        let mut denied_policy = policy;
        denied_policy.allow_raw_shell = false;
        let denied = manager.handle(
            &denied_policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request("exec", "wc_shell_runner", Some("printf denied")),
        );
        assert_eq!(denied.error_code.as_deref(), Some("raw_shell_disabled"));
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn rejected_exec_keeps_authoritative_running_state() {
        let (_temp, _project, projects, policy) = fixture();
        let shell = ShellConfig::default();
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        assert_eq!(
            manager
                .handle(
                    &policy,
                    &shell,
                    &SshConfig::default(),
                    1,
                    &projects,
                    &request("open", "wc_shell_rejected_exec", None),
                )
                .shell_state,
            "running"
        );

        let mut invalid = request("exec", "wc_shell_rejected_exec", Some("printf ignored"));
        invalid.persistent_shell.as_mut().unwrap().timeout_secs = Some(policy.max_timeout_secs + 1);
        let rejected = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &invalid,
        );
        assert_eq!(
            rejected.error_code.as_deref(),
            Some("persistent_shell_invalid_timeout")
        );
        assert_eq!(rejected.shell_state, "running");
        assert_eq!(manager.active_count(), 1);

        let oversized = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_rejected_exec",
                Some(&"x".repeat(RAW_SHELL_COMMAND_MAX_BYTES + 1)),
            ),
        );
        assert_eq!(
            oversized.error_code.as_deref(),
            Some("persistent_shell_invalid_command")
        );
        assert_eq!(oversized.shell_state, "running");

        let observed = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_rejected_exec",
                Some("printf still-running"),
            ),
        );
        assert_eq!(observed.stdout, "still-running");
    }

    #[test]
    fn exec_reapplies_current_output_limit() {
        let (_temp, _project, projects, mut policy) = fixture();
        let shell = ShellConfig::default();
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        assert_eq!(
            manager
                .handle(
                    &policy,
                    &shell,
                    &SshConfig::default(),
                    1,
                    &projects,
                    &request("open", "wc_shell_output_policy", None),
                )
                .shell_state,
            "running"
        );

        policy.max_output_bytes = 1024;
        let result = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_output_policy",
                Some("i=0; while [ \"$i\" -lt 5000 ]; do printf x; i=$((i+1)); done"),
            ),
        );
        assert!(result.stdout_truncated);
        assert!(result.stdout.len() <= policy.max_output_bytes);
    }

    #[test]
    fn runner_profile_initialization_runs_once_at_open() {
        let (_temp, _project, projects, policy) = fixture();
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "persistent".to_string(),
            ShellProfileConfig {
                program: Some("bash".to_string()),
                init_script: Some("WC_PROFILE_COUNT=1; export WC_PROFILE_COUNT".to_string()),
                ..ShellProfileConfig::default()
            },
        );
        let mut shell = ShellConfig {
            default_profile: Some("persistent".to_string()),
            profiles,
            ..ShellConfig::default()
        };
        shell.max_persistent_shells = 2;
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        assert_eq!(
            manager
                .handle(
                    &policy,
                    &shell,
                    &SshConfig::default(),
                    1,
                    &projects,
                    &request("open", "wc_shell_profile", None),
                )
                .shell_state,
            "running"
        );
        let first = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_profile",
                Some("printf %s \"$WC_PROFILE_COUNT\"; WC_PROFILE_COUNT=2"),
            ),
        );
        assert_eq!(first.stdout, "1");
        let second = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request(
                "exec",
                "wc_shell_profile",
                Some("printf %s \"$WC_PROFILE_COUNT\""),
            ),
        );
        assert_eq!(second.stdout, "2");
    }

    #[test]
    fn runner_rejects_profile_initialization_outside_project() {
        let (_temp, _project, projects, policy) = fixture();
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "escaping".to_string(),
            ShellProfileConfig {
                init_script: Some("cd ..".to_string()),
                ..ShellProfileConfig::default()
            },
        );
        let shell = ShellConfig {
            default_profile: Some("escaping".to_string()),
            profiles,
            ..ShellConfig::default()
        };
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());

        let result = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request("open", "wc_shell_profile_escape", None),
        );
        assert_eq!(
            result.error_code.as_deref(),
            Some("persistent_shell_cwd_outside_project")
        );
        assert_ne!(result.shell_state, "running");
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn runner_closes_shell_that_moves_outside_project() {
        let (_temp, _project, projects, policy) = fixture();
        let shell = ShellConfig::default();
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        assert_eq!(
            manager
                .handle(
                    &policy,
                    &shell,
                    &SshConfig::default(),
                    1,
                    &projects,
                    &request("open", "wc_shell_boundary", None),
                )
                .shell_state,
            "running"
        );

        let escaped = manager.handle(
            &policy,
            &shell,
            &SshConfig::default(),
            1,
            &projects,
            &request("exec", "wc_shell_boundary", Some("cd ..")),
        );
        assert_eq!(escaped.error_code.as_deref(), Some("shell_reset_required"));
        assert_ne!(escaped.shell_state, "running");
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn runner_rejects_runtime_project_identity_mismatch() {
        let (_temp, _project, projects, policy) = fixture();
        let shell = ShellConfig::default();
        let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
        let mut wrong = request("open", "wc_shell_wrong", None);
        wrong.persistent_shell.as_mut().unwrap().runtime_project_id =
            "agent:other:demo".to_string();
        let result = manager.handle(&policy, &shell, &SshConfig::default(), 1, &projects, &wrong);
        assert_eq!(
            result.error_code.as_deref(),
            Some("persistent_shell_project_mismatch")
        );
        assert_eq!(manager.active_count(), 0);
    }
}
