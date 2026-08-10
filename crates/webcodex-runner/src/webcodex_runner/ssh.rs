//! Runner-local SSH resource execution.
//!
//! The pool deliberately delegates authentication and Host alias resolution to
//! the Runner host's OpenSSH client. WebCodex only keeps named resource
//! metadata plus short-lived in-memory control-socket state; it never stores
//! keys, passwords, SSH configuration, or a transport in a Workflow Session.

use super::config::SshConfig;
use super::output::{CommandResult, ShellCommandResult};
use super::shutdown::lock_unpoison;
use super::AgentPolicy;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const SSH_CONNECT_TIMEOUT_SECS: u64 = 10;
const SSH_CONTROL_PERSIST_SECS: u64 = 300;
const SSH_PIPE_DRAIN_TIMEOUT_SECS: u64 = 2;

/// In-memory identity for one OpenSSH multiplex transport. The Runner config
/// generation protects a request after an operator changes a resource host.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SshConnectionKey {
    session_id: String,
    resource_name: String,
    generation: u64,
}

#[derive(Debug, Clone)]
struct SshConnection {
    key: SshConnectionKey,
    control_path: PathBuf,
    host: String,
    default_cwd: Option<String>,
    /// Test-only callers can point the OpenSSH client at an isolated config;
    /// production leaves this `None` and uses the Runner user's normal SSH
    /// configuration exactly as intended.
    config_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct SshPoolState {
    control_root: Option<PathBuf>,
    entries: HashMap<SshConnectionKey, SshConnection>,
    next_control_id: u64,
    test_config_path: Option<PathBuf>,
}

/// Lightweight, process-local OpenSSH multiplex pool.
///
/// Each `ssh` command still opens an independent remote exec channel. The
/// control socket only reuses the authenticated transport and is not restored
/// across a Runner restart.
#[derive(Debug, Clone, Default)]
pub(crate) struct SshConnectionPool {
    state: Arc<Mutex<SshPoolState>>,
}

/// A ready-to-spawn SSH command paired with the pool entry it used.
pub(crate) struct PreparedSshCommand {
    pub(crate) command: Command,
    pub(crate) key: SshConnectionKey,
}

/// A ready-to-spawn long-lived SSH shell command with the resource's default
/// remote cwd. Used only by the Unix remote persistent-shell transport.
#[cfg(unix)]
pub(crate) struct PreparedPersistentShellCommand {
    pub(crate) command: Command,
    pub(crate) default_cwd: Option<String>,
}

impl SshConnectionPool {
    /// Whether this Runner can advertise SSH-shell support. A missing OpenSSH
    /// executable is a capability absence, not a later silent local fallback.
    pub(crate) fn is_available() -> bool {
        if !cfg!(unix) {
            return false;
        }
        Command::new("ssh")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    /// Resolve a named resource, establish/reuse its control transport, and
    /// build a direct SSH command that owns its local process group itself.
    pub(crate) fn prepare_command(
        &self,
        generation: u64,
        config: &SshConfig,
        resource_name: &str,
        session_id: &str,
        cwd: Option<&str>,
        command: &str,
    ) -> Result<PreparedSshCommand, String> {
        self.prepare_command_inner(
            generation,
            config,
            resource_name,
            session_id,
            cwd,
            command,
            true,
        )
    }

    /// Build an SSH command whose process tree is owned by JobManager's
    /// ManagedChild rather than the legacy setsid hook.
    pub(crate) fn prepare_job_command(
        &self,
        generation: u64,
        config: &SshConfig,
        resource_name: &str,
        session_id: &str,
        cwd: Option<&str>,
        command: &str,
    ) -> Result<PreparedSshCommand, String> {
        self.prepare_command_inner(
            generation,
            config,
            resource_name,
            session_id,
            cwd,
            command,
            false,
        )
    }

    fn prepare_command_inner(
        &self,
        generation: u64,
        config: &SshConfig,
        resource_name: &str,
        session_id: &str,
        cwd: Option<&str>,
        command: &str,
        configure_process_group: bool,
    ) -> Result<PreparedSshCommand, String> {
        if !cfg!(unix) {
            return Err("ssh_shell_unavailable: this Runner host does not support SSH resources; command was not started".to_string());
        }
        if !is_safe_resource_name(resource_name) {
            return Err(
                "ssh_resource_invalid: resource name is invalid; command was not started"
                    .to_string(),
            );
        }
        if !is_safe_session_id(session_id) {
            return Err(
                "ssh_session_required: an SSH resource requires a valid Workflow Session id; command was not started".to_string(),
            );
        }
        let resource = match config.resources.get(resource_name) {
            Some(resource) => resource,
            None => {
                // A hot reload can remove a resource while an old transport
                // is still pooled. It must not stay alive merely because the
                // next request fails name resolution.
                self.release_resource(resource_name);
                return Err(format!(
                    "ssh_resource_not_found: resource '{}' is not configured on this Runner; command was not started",
                    resource_name
                ));
            }
        };
        let requested_cwd = normalize_remote_cwd(cwd)?;
        let connection = self.connection_for(
            generation,
            resource_name,
            session_id,
            &resource.host,
            resource.default_cwd.as_deref(),
        )?;
        let effective_cwd = requested_cwd.or(connection.default_cwd.clone());
        let remote_script = remote_script(effective_cwd.as_deref(), command);
        let mut ssh = ssh_command(&connection);
        ssh.arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("LogLevel=ERROR")
            .arg("-S")
            .arg(&connection.control_path)
            .arg(&connection.host)
            .arg(remote_script);
        if configure_process_group {
            configure_private_process_group(&mut ssh);
        }
        Ok(PreparedSshCommand {
            command: ssh,
            key: connection.key,
        })
    }

    /// A transport error after spawning has uncertain remote delivery. Forget
    /// the entry so the *next* command establishes a fresh transport; never
    /// retry the just-submitted command automatically.
    pub(crate) fn invalidate_after_transport_failure(&self, key: &SshConnectionKey) {
        let mut state = lock_unpoison(&self.state);
        if let Some(connection) = state.entries.remove(key) {
            close_control_socket(&connection);
        }
    }

    /// Resolve a named resource, establish/reuse its control transport, and
    /// construct a ready-to-spawn `ssh` command that runs one long-lived remote
    /// shell. Unlike `prepare_command`, the remote command is just the shell
    /// binary (no `-c` script); the Runner drives it over stdin. The long-lived
    /// channel reuses the same authenticated control socket (`ControlMaster=no
    /// -S <path>`) and never creates a second master.
    #[cfg(unix)]
    pub(crate) fn prepare_persistent_shell_command(
        &self,
        generation: u64,
        config: &SshConfig,
        resource_name: &str,
        session_id: &str,
        shell_program: &str,
    ) -> Result<PreparedPersistentShellCommand, String> {
        if !cfg!(unix) {
            return Err("ssh_shell_unavailable: this Runner host does not support SSH resources; shell was not started".to_string());
        }
        if !is_safe_resource_name(resource_name) {
            return Err(
                "ssh_resource_invalid: resource name is invalid; shell was not started".to_string(),
            );
        }
        if !is_safe_session_id(session_id) {
            return Err(
                "ssh_session_required: an SSH resource requires a valid Workflow Session id; shell was not started".to_string(),
            );
        }
        let resource = match config.resources.get(resource_name) {
            Some(resource) => resource,
            None => {
                self.release_resource(resource_name);
                return Err(format!(
                    "ssh_resource_not_found: resource '{}' is not configured on this Runner; shell was not started",
                    resource_name
                ));
            }
        };
        let connection = self.connection_for(
            generation,
            resource_name,
            session_id,
            &resource.host,
            resource.default_cwd.as_deref(),
        )?;
        let mut ssh = ssh_command(&connection);
        ssh.arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("LogLevel=ERROR")
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-S")
            .arg(&connection.control_path)
            .arg(&connection.host)
            .arg(shell_program);
        configure_private_process_group(&mut ssh);
        Ok(PreparedPersistentShellCommand {
            command: ssh,
            default_cwd: connection.default_cwd.clone(),
        })
    }

    /// Release every Session's generation for a resource that is no longer
    /// valid in the active Runner configuration.
    fn release_resource(&self, resource_name: &str) {
        let removed = {
            let mut state = lock_unpoison(&self.state);
            let keys = state
                .entries
                .keys()
                .filter(|key| key.resource_name == resource_name)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| state.entries.remove(&key))
                .collect::<Vec<_>>()
        };
        for connection in removed {
            retire_control_socket(&connection);
        }
    }

    #[cfg(test)]
    pub(crate) fn connection_count(&self) -> usize {
        lock_unpoison(&self.state).entries.len()
    }

    #[cfg(test)]
    pub(crate) fn with_test_config(config_path: PathBuf) -> Self {
        let pool = Self::default();
        lock_unpoison(&pool.state).test_config_path = Some(config_path);
        pool
    }

    #[cfg(test)]
    pub(crate) fn control_path_for(
        &self,
        generation: u64,
        resource_name: &str,
        session_id: &str,
    ) -> Option<PathBuf> {
        lock_unpoison(&self.state)
            .entries
            .get(&SshConnectionKey {
                generation,
                resource_name: resource_name.to_string(),
                session_id: session_id.to_string(),
            })
            .map(|connection| connection.control_path.clone())
    }

    fn connection_for(
        &self,
        generation: u64,
        resource_name: &str,
        session_id: &str,
        host: &str,
        default_cwd: Option<&str>,
    ) -> Result<SshConnection, String> {
        let key = SshConnectionKey {
            session_id: session_id.to_string(),
            resource_name: resource_name.to_string(),
            generation,
        };
        // Hold the small pool mutex across an initial OpenSSH handshake. This
        // prevents two concurrent requests with the same Session/resource
        // from creating separate masters. Existing checks are local control
        // socket probes and ordinarily return immediately.
        let mut state = lock_unpoison(&self.state);
        // A changed hot configuration must not leave an older host mapping
        // attached to this Session/resource pair. Close it eagerly rather
        // than waiting for process exit; a subsequent request gets a master
        // for the new generation only.
        let stale = state
            .entries
            .keys()
            .filter(|candidate| {
                candidate.session_id == session_id
                    && candidate.resource_name == resource_name
                    && candidate.generation != generation
            })
            .cloned()
            .collect::<Vec<_>>();
        for stale_key in stale {
            if let Some(entry) = state.entries.remove(&stale_key) {
                retire_control_socket(&entry);
            }
        }
        if let Some(existing) = state.entries.get(&key).cloned() {
            if control_socket_healthy(&existing) {
                return Ok(existing);
            }
            if let Some(connection) = state.entries.remove(&key) {
                close_control_socket(&connection);
            }
        }
        let root = ensure_control_root(&mut state)?;
        state.next_control_id = state.next_control_id.saturating_add(1);
        let control_path = root.join(format!("c{:016x}", state.next_control_id));
        let connection = SshConnection {
            key: key.clone(),
            control_path,
            host: host.to_string(),
            default_cwd: default_cwd.map(str::to_string),
            config_path: state.test_config_path.clone(),
        };
        establish_control_socket(&connection)?;
        state.entries.insert(key, connection.clone());
        Ok(connection)
    }
}

impl Drop for SshConnectionPool {
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) != 1 {
            return;
        }
        let (root, entries) = {
            let mut state = lock_unpoison(&self.state);
            let root = state.control_root.take();
            let entries = std::mem::take(&mut state.entries)
                .into_values()
                .collect::<Vec<_>>();
            (root, entries)
        };
        for entry in entries {
            close_control_socket(&entry);
        }
        if let Some(root) = root {
            // The directory is freshly created by this process under the
            // system temp directory and contains only control sockets.
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

/// Execute a short remote shell command through a Session-bound SSH resource.
#[cfg(test)]
pub(crate) fn run_ssh_shell(
    pool: &SshConnectionPool,
    generation: u64,
    config: &SshConfig,
    policy: &AgentPolicy,
    resource_name: &str,
    session_id: &str,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> CommandResult {
    run_ssh_shell_with_execution_state(
        pool,
        generation,
        config,
        policy,
        resource_name,
        session_id,
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
        sandbox,
    )
    .result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_ssh_shell_with_execution_state(
    pool: &SshConnectionPool,
    generation: u64,
    config: &SshConfig,
    policy: &AgentPolicy,
    resource_name: &str,
    session_id: &str,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> ShellCommandResult {
    let start = Instant::now();
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(command_error(
            start,
            "raw shell is disabled by local agent policy".to_string(),
        ));
    }
    if sandbox.is_some() {
        return ShellCommandResult::not_started(command_error(
            start,
            "ssh_sandbox_unavailable: SSH resources cannot run in the local inspect sandbox; command was not started".to_string(),
        ));
    }
    let prepared =
        match pool.prepare_command(generation, config, resource_name, session_id, cwd, command) {
            Ok(prepared) => prepared,
            Err(error) => return ShellCommandResult::not_started(command_error(start, error)),
        };
    let key = prepared.key.clone();
    let mut result = run_piped_ssh_command(
        prepared.command,
        policy.max_output_bytes,
        timeout_secs.min(policy.max_timeout_secs).max(1),
        stdin,
        stop_requested,
        start,
    );
    if is_transport_failure(result.result.exit_code, result.result.stderr.as_deref()) {
        pool.invalidate_after_transport_failure(&key);
        if let Some(stderr) = result.result.stderr.as_mut() {
            if !stderr.is_empty() && !stderr.ends_with('\n') {
                stderr.push('\n');
            }
            stderr.push_str(
                "webcodex: SSH transport ended after dispatch; the command may have started and was not retried",
            );
        }
        if result.result.error.is_none() {
            result.result.error = Some(
                "ssh_transport_failed: command may have started and was not retried".to_string(),
            );
        }
        result.execution_state = crate::shell_protocol::ShellCommandExecutionState::OutcomeUnknown;
    }
    result
}

/// Used by async job handling to apply the same conservative invalidation
/// decision after its SSH child exits.
pub(crate) fn is_transport_failure(exit_code: Option<i32>, stderr: Option<&str>) -> bool {
    if exit_code != Some(255) {
        return false;
    }
    let stderr = stderr.unwrap_or_default().to_ascii_lowercase();
    [
        "control socket connect",
        "mux_client",
        "master is dead",
        "connection reset",
        "broken pipe",
        "connection closed",
    ]
    .iter()
    .any(|marker| stderr.contains(marker))
}

fn ensure_control_root(state: &mut SshPoolState) -> Result<PathBuf, String> {
    if let Some(root) = &state.control_root {
        return Ok(root.clone());
    }
    for _ in 0..4 {
        let candidate = std::env::temp_dir().join(format!(
            "wc-ssh-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        #[cfg(unix)]
        let create_result = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700).create(&candidate)
        };
        #[cfg(not(unix))]
        let create_result = std::fs::create_dir(&candidate);
        match create_result {
            Ok(()) => {
                state.control_root = Some(candidate.clone());
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                return Err(
                    "ssh_connection_pool_unavailable: could not create Runner-local control socket directory; command was not started".to_string(),
                )
            }
        }
    }
    Err(
        "ssh_connection_pool_unavailable: could not allocate Runner-local control socket directory; command was not started".to_string(),
    )
}

fn establish_control_socket(connection: &SshConnection) -> Result<(), String> {
    let mut ssh = ssh_command(connection);
    ssh.arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("LogLevel=ERROR")
        .arg("-o")
        .arg(format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"))
        .arg("-o")
        .arg("ControlMaster=yes")
        .arg("-o")
        // Keep the master available long enough for interactive Session reuse,
        // but bound idle transports so abandoned Sessions cannot accumulate
        // one OpenSSH process and control socket forever.
        .arg(format!("ControlPersist={SSH_CONTROL_PERSIST_SECS}"))
        .arg("-o")
        .arg(format!("ControlPath={}", connection.control_path.display()))
        .arg("-N")
        .arg("-f")
        .arg(&connection.host)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match ssh.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) | Err(_) => Err(format!(
            "ssh_connection_failed: resource '{}'; command was not started",
            connection.key.resource_name
        )),
    }
}

fn control_socket_healthy(connection: &SshConnection) -> bool {
    ssh_command(connection)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("LogLevel=ERROR")
        .arg("-S")
        .arg(&connection.control_path)
        .arg("-O")
        .arg("check")
        .arg(&connection.host)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Stop accepting new multiplexed channels without terminating channels that
/// are already running. Config changes and resource removal retire a transport
/// this way; the bounded ControlPersist lifetime lets it exit after the last
/// active channel finishes.
fn retire_control_socket(connection: &SshConnection) {
    let _ = ssh_command(connection)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("LogLevel=ERROR")
        .arg("-S")
        .arg(&connection.control_path)
        .arg("-O")
        .arg("stop")
        .arg(&connection.host)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn close_control_socket(connection: &SshConnection) {
    let status = ssh_command(connection)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("LogLevel=ERROR")
        .arg("-S")
        .arg(&connection.control_path)
        .arg("-O")
        .arg("exit")
        .arg(&connection.host)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.is_ok_and(|status| status.success()) {
        return;
    }
    // `-O exit` is the normal path. Some OpenSSH builds can reject the
    // control request after a transport-side failure even though the master
    // is still alive. The socket lives in our private 0700 directory, so a
    // successful `-O check` identifies only the master that this pool created.
    // Release it conservatively instead of leaking an orphaned local client.
    #[cfg(unix)]
    if let Some(pid) = control_master_pid(connection) {
        // SAFETY: `pid` came from the authenticated control socket located in
        // this pool's private directory. SIGTERM affects only that SSH master.
        unsafe {
            let _ = libc::kill(pid, libc::SIGTERM);
        }
    }
}

#[cfg(unix)]
fn control_master_pid(connection: &SshConnection) -> Option<i32> {
    let output = ssh_command(connection)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("LogLevel=ERROR")
        .arg("-S")
        .arg(&connection.control_path)
        .arg("-O")
        .arg("check")
        .arg(&connection.host)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    message.split("pid=").nth(1).and_then(|suffix| {
        suffix
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>()
            .parse::<i32>()
            .ok()
    })
}

fn ssh_command(connection: &SshConnection) -> Command {
    let mut command = Command::new("ssh");
    if let Some(config_path) = &connection.config_path {
        command.arg("-F").arg(config_path);
    }
    command
}

fn normalize_remote_cwd(cwd: Option<&str>) -> Result<Option<String>, String> {
    let Some(cwd) = cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return Ok(None);
    };
    if cwd.len() > 4096 || cwd.chars().any(char::is_control) {
        return Err(
            "ssh_remote_cwd_invalid: cwd must be a bounded remote path without control characters; command was not started".to_string(),
        );
    }
    Ok(Some(cwd.to_string()))
}

fn remote_script(cwd: Option<&str>, command: &str) -> String {
    match cwd {
        Some(cwd) => format!(
            "if ! cd {}; then printf >&2 '%s\\n' 'webcodex: remote cwd is unavailable'; exit 125; fi\n{}",
            shell_quote(cwd),
            command
        ),
        None => command.to_string(),
    }
}

fn shell_quote(value: &str) -> String {
    let mut escaped = String::from("'");
    for part in value.split('\'') {
        if escaped.len() > 1 {
            escaped.push_str("'\"'\"'");
        }
        escaped.push_str(part);
    }
    escaped.push('\'');
    escaped
}

fn is_safe_resource_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn is_safe_session_id(value: &str) -> bool {
    value.starts_with("wc_sess_")
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn configure_private_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid` is async-signal-safe and runs only in the spawned
        // child before exec, giving job stop/timeout a private group to reap.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
}

fn run_piped_ssh_command(
    mut command: Command,
    max_output_bytes: usize,
    timeout_secs: u64,
    stdin: Option<&str>,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
) -> ShellCommandResult {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return ShellCommandResult::not_started(command_error(
                start,
                "ssh_command_spawn_failed: command was not started".to_string(),
            ))
        }
    };
    if let Some(input) = stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            if let Err(error) = child_stdin.write_all(input.as_bytes()) {
                if error.kind() != std::io::ErrorKind::BrokenPipe {
                    let _ = terminate_ssh_child(&mut child);
                    return ShellCommandResult::outcome_unknown(command_error(
                        start,
                        "ssh_command_stdin_failed: command may have started and was not retried"
                            .to_string(),
                    ));
                }
            }
        } else {
            let _ = terminate_ssh_child(&mut child);
            return ShellCommandResult::outcome_unknown(command_error(
                start,
                "ssh_command_stdin_failed: command may have started and was not retried"
                    .to_string(),
            ));
        }
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (stdout_tx, stdout_rx) = mpsc::sync_channel(1);
    let (stderr_tx, stderr_rx) = mpsc::sync_channel(1);
    if let Some(stdout) = stdout {
        std::thread::spawn(move || {
            let _ = stdout_tx.send(read_bounded_pipe_tail(stdout, max_output_bytes));
        });
    } else {
        drop(stdout_tx);
    }
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            let _ = stderr_tx.send(read_bounded_pipe_tail(stderr, max_output_bytes));
        });
    } else {
        drop(stderr_tx);
    }

    let mut stopped = false;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if stop_requested.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
                    stopped = true;
                    break terminate_ssh_child(&mut child).ok();
                }
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    timed_out = true;
                    break terminate_ssh_child(&mut child).ok();
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = terminate_ssh_child(&mut child);
                return ShellCommandResult::outcome_unknown(command_error(
                    start,
                    "ssh_command_wait_failed: command may have started and was not retried"
                        .to_string(),
                ));
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(SSH_PIPE_DRAIN_TIMEOUT_SECS);
    let stdout = stdout_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let stderr = stderr_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let mut stderr = String::from_utf8_lossy(&stderr).into_owned();
    if stopped {
        append_line(&mut stderr, "job stopped by request");
        let result = CommandResult {
            exit_code: Some(-1),
            stdout: Some(String::from_utf8_lossy(&stdout).into_owned()),
            stderr: Some(stderr),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("job stopped".to_string()),
        };
        return if status.is_some() {
            ShellCommandResult::completed(result)
        } else {
            ShellCommandResult::outcome_unknown(result)
        };
    }
    if timed_out {
        append_line(
            &mut stderr,
            &format!("command timed out after {timeout_secs} seconds"),
        );
        let result = CommandResult {
            exit_code: Some(-1),
            stdout: Some(String::from_utf8_lossy(&stdout).into_owned()),
            stderr: Some(stderr),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("command timed out".to_string()),
        };
        return if status.is_some() {
            ShellCommandResult::timed_out(result)
        } else {
            ShellCommandResult::outcome_unknown(result)
        };
    }
    ShellCommandResult::completed(CommandResult {
        exit_code: status.and_then(|status| status.code()).or(Some(-1)),
        stdout: Some(String::from_utf8_lossy(&stdout).into_owned()),
        stderr: Some(stderr),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    })
}

fn terminate_ssh_child(child: &mut Child) -> Result<ExitStatus, String> {
    #[cfg(unix)]
    {
        let pid = child.id();
        if pid != 0 {
            // SAFETY: the SSH child creates its own process group before exec,
            // so this only signals the command started by this invocation.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => return Err("SSH child did not exit after termination".to_string()),
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn read_bounded_pipe_tail(mut pipe: impl Read, max_bytes: usize) -> Result<Vec<u8>, String> {
    let retained_limit = max_bytes;
    let mut output = Vec::with_capacity(retained_limit.min(64 * 1024));
    let mut chunk = [0_u8; 8192];
    loop {
        let read = pipe.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(output);
        }
        output.extend_from_slice(&chunk[..read]);
        if output.len() > retained_limit {
            let discard = output.len() - retained_limit;
            output.drain(..discard);
        }
    }
}

fn append_line(value: &mut String, suffix: &str) {
    if !value.is_empty() && !value.ends_with('\n') {
        value.push('\n');
    }
    value.push_str(suffix);
}

fn command_error(start: Instant, error: String) -> CommandResult {
    CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: Some(error),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{remote_script, run_ssh_shell, shell_quote, SshConnectionPool};
    use crate::webcodex_runner::config::{AgentPolicy, SshConfig, SshResourceConfig};
    use std::collections::BTreeMap;
    use std::net::{TcpListener, TcpStream};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct TestSshServer {
        _temp: tempfile::TempDir,
        child: Child,
        client_config: PathBuf,
        alias: String,
        remote_cwd: PathBuf,
    }

    impl Drop for TestSshServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl TestSshServer {
        fn start() -> Option<Self> {
            let sshd = executable_on_path("sshd")?;
            if Command::new(&sshd)
                .arg("-V")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_err()
            {
                return None;
            }
            let temp = tempfile::tempdir().expect("create SSH test directory");
            let remote_cwd = temp.path().join("remote");
            std::fs::create_dir(&remote_cwd).expect("create remote cwd");
            let host_key = temp.path().join("host_ed25519");
            let user_key = temp.path().join("user_ed25519");
            generate_key(&host_key);
            generate_key(&user_key);
            let authorized_keys = temp.path().join("authorized_keys");
            std::fs::copy(user_key.with_extension("pub"), &authorized_keys)
                .expect("copy test public key");
            let listener = TcpListener::bind("127.0.0.1:0").expect("reserve SSH test port");
            let port = listener.local_addr().expect("read SSH test port").port();
            drop(listener);
            let user = String::from_utf8(
                Command::new("id")
                    .args(["-un"])
                    .output()
                    .expect("resolve SSH test user")
                    .stdout,
            )
            .expect("SSH test user is UTF-8")
            .trim()
            .to_string();
            let server_config = temp.path().join("sshd_config");
            std::fs::write(
                &server_config,
                format!(
                    "ListenAddress 127.0.0.1\nPort {port}\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin yes\nStrictModes no\nUsePAM no\nPrintMotd no\nLogLevel ERROR\n",
                    host_key.display(),
                    temp.path().join("sshd.pid").display(),
                    authorized_keys.display(),
                ),
            )
            .expect("write SSH daemon config");
            let alias = "webcodex-test-ssh".to_string();
            let client_config = temp.path().join("ssh_config");
            std::fs::write(
                &client_config,
                format!(
                    "Host {alias}\n  HostName 127.0.0.1\n  Port {port}\n  User {user}\n  IdentityFile {}\n  IdentitiesOnly yes\n  StrictHostKeyChecking no\n  UserKnownHostsFile /dev/null\n  GlobalKnownHostsFile /dev/null\n  LogLevel ERROR\n",
                    user_key.display(),
                ),
            )
            .expect("write SSH client config");
            let checked = Command::new(&sshd)
                .arg("-t")
                .arg("-f")
                .arg(&server_config)
                .output()
                .expect("validate SSH test daemon config");
            assert!(
                checked.status.success(),
                "invalid SSH test daemon config: {}",
                String::from_utf8_lossy(&checked.stderr)
            );
            let mut child = Command::new(&sshd)
                .arg("-D")
                .arg("-e")
                .arg("-f")
                .arg(&server_config)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start SSH test daemon");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                    return Some(Self {
                        _temp: temp,
                        child,
                        client_config,
                        alias,
                        remote_cwd,
                    });
                }
                if let Some(status) = child.try_wait().expect("poll SSH test daemon") {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        use std::io::Read;
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    panic!("SSH test daemon exited early ({status}): {stderr}");
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("SSH test daemon did not listen within five seconds");
        }
    }

    fn generate_key(path: &Path) {
        let status = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .status()
            .expect("run ssh-keygen for test SSH daemon");
        assert!(status.success(), "ssh-keygen failed");
    }

    fn executable_on_path(name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
    }

    fn ssh_config(server: &TestSshServer) -> SshConfig {
        let mut resources = BTreeMap::new();
        for name in ["tmp", "alt"] {
            resources.insert(
                name.to_string(),
                SshResourceConfig {
                    host: server.alias.clone(),
                    default_cwd: Some(server.remote_cwd.to_string_lossy().into_owned()),
                },
            );
        }
        SshConfig { resources }
    }

    fn run(
        pool: &SshConnectionPool,
        config: &SshConfig,
        resource: &str,
        session_id: &str,
        command: &str,
    ) -> super::CommandResult {
        run_at_cwd(pool, config, resource, session_id, None, command)
    }

    fn run_at_cwd(
        pool: &SshConnectionPool,
        config: &SshConfig,
        resource: &str,
        session_id: &str,
        cwd: Option<&str>,
        command: &str,
    ) -> super::CommandResult {
        run_ssh_shell(
            pool,
            7,
            config,
            &AgentPolicy::default(),
            resource,
            session_id,
            cwd,
            command,
            None,
            10,
            None,
            None,
        )
    }

    #[test]
    fn remote_cwd_is_shell_quoted_without_interpolating_it() {
        let script = remote_script(Some("/tmp/a' b"), "printf ok");
        assert!(script.contains("cd '/tmp/a'\"'\"' b'"), "{script}");
        assert!(script.ends_with("printf ok"), "{script}");
        assert_eq!(shell_quote("plain"), "'plain'");
    }

    #[test]
    fn config_generation_change_does_not_interrupt_active_remote_channel() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let marker = server.remote_cwd.join("generation-channel-started");
        let remote_command = format!(
            "printf started > {}; sleep 1; printf old-generation",
            shell_quote(marker.to_string_lossy().as_ref())
        );
        let prepared = pool
            .prepare_command(
                7,
                &config,
                "tmp",
                "wc_sess_generation",
                None,
                &remote_command,
            )
            .expect("prepare generation 7 channel");
        let old_channel = std::thread::spawn(move || {
            let mut command = prepared.command;
            command.output().expect("run generation 7 channel")
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "old generation channel did not start");

        let next = run_ssh_shell(
            &pool,
            8,
            &config,
            &AgentPolicy::default(),
            "tmp",
            "wc_sess_generation",
            None,
            "printf new-generation",
            None,
            10,
            None,
            None,
        );
        assert_eq!(next.exit_code, Some(0), "{next:?}");
        assert_eq!(next.stdout.as_deref(), Some("new-generation"));

        let old = old_channel.join().expect("join generation 7 channel");
        assert!(
            old.status.success(),
            "generation change interrupted active channel: {}",
            String::from_utf8_lossy(&old.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&old.stdout), "old-generation");
        assert_eq!(
            pool.connection_count(),
            1,
            "only the active config generation remains reusable"
        );
    }

    #[test]
    fn reuses_session_transport_but_not_remote_shell_state_and_reconnects() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let missing = run(&pool, &config, "missing", "wc_sess_missing", "printf no");
        assert!(
            missing
                .error
                .as_deref()
                .is_some_and(|error| error.contains("ssh_resource_not_found")),
            "{missing:?}"
        );

        let cwd_pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let unavailable_cwd_path = server
            .remote_cwd
            .join("not-there")
            .to_string_lossy()
            .into_owned();
        let unavailable_cwd = run_at_cwd(
            &cwd_pool,
            &config,
            "tmp",
            "wc_sess_missing_cwd",
            Some(&unavailable_cwd_path),
            "printf never-runs",
        );
        assert_eq!(unavailable_cwd.exit_code, Some(125), "{unavailable_cwd:?}");
        assert!(
            unavailable_cwd
                .stderr
                .as_deref()
                .is_some_and(|stderr| stderr.contains("webcodex: remote cwd is unavailable")),
            "{unavailable_cwd:?}"
        );

        let first = run(
            &pool,
            &config,
            "tmp",
            "wc_sess_alpha",
            "export WEBCODEX_SSH_TEST_STATE=kept; pwd; printf first",
        );
        assert_eq!(first.exit_code, Some(0), "{first:?}");
        assert!(first.error.is_none(), "{first:?}");
        assert_eq!(first.stderr.as_deref(), Some(""), "{first:?}");
        assert!(
            first
                .stdout
                .as_deref()
                .is_some_and(|stdout| stdout.contains(server.remote_cwd.to_string_lossy().as_ref())),
            "{first:?}"
        );
        let first_control = pool
            .control_path_for(7, "tmp", "wc_sess_alpha")
            .expect("initial control socket");
        assert_eq!(
            std::fs::metadata(first_control.parent().expect("control root"))
                .expect("control root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "SSH control sockets must stay inside a private Runner directory"
        );
        assert_eq!(pool.connection_count(), 1);

        let isolated = run(
            &pool,
            &config,
            "tmp",
            "wc_sess_alpha",
            "test -z \"${WEBCODEX_SSH_TEST_STATE+x}\" && printf isolated",
        );
        assert_eq!(isolated.exit_code, Some(0), "{isolated:?}");
        assert_eq!(isolated.stdout.as_deref(), Some("isolated"));
        assert_eq!(isolated.stderr.as_deref(), Some(""), "{isolated:?}");
        assert_eq!(
            pool.connection_count(),
            1,
            "same Session/resource reuses transport"
        );

        let other_session = run(
            &pool,
            &config,
            "tmp",
            "wc_sess_beta",
            "printf other-session",
        );
        assert_eq!(other_session.exit_code, Some(0), "{other_session:?}");
        let other_resource = run(
            &pool,
            &config,
            "alt",
            "wc_sess_alpha",
            "printf other-resource",
        );
        assert_eq!(other_resource.exit_code, Some(0), "{other_resource:?}");
        assert_eq!(
            pool.connection_count(),
            3,
            "pool keys include Session and resource"
        );

        let check_result = Command::new("ssh")
            .arg("-F")
            .arg(&server.client_config)
            .arg("-S")
            .arg(&first_control)
            .arg("-O")
            .arg("check")
            .arg(&server.alias)
            .output()
            .expect("inspect test SSH master");
        assert!(
            check_result.status.success(),
            "could not inspect test SSH master: {}",
            String::from_utf8_lossy(&check_result.stderr)
        );
        let master_pid = format!(
            "{}{}",
            String::from_utf8_lossy(&check_result.stdout),
            String::from_utf8_lossy(&check_result.stderr)
        )
        .split("pid=")
        .nth(1)
        .and_then(|suffix| {
            suffix
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse::<i32>()
                .ok()
        })
        .expect("SSH master check reports its pid");
        // SAFETY: the pid is reported by the control socket owned by this
        // test's pool, so it identifies only the temporary test master.
        assert_eq!(unsafe { libc::kill(master_pid, libc::SIGTERM) }, 0);
        std::thread::sleep(Duration::from_millis(50));
        let reconnected = run(&pool, &config, "tmp", "wc_sess_alpha", "printf reconnected");
        assert_eq!(reconnected.exit_code, Some(0), "{reconnected:?}");
        assert_eq!(reconnected.stdout.as_deref(), Some("reconnected"));
        assert_ne!(
            pool.control_path_for(7, "tmp", "wc_sess_alpha"),
            Some(first_control),
            "a dead master gets a fresh control socket on the next command"
        );

        let mut removed_resource_config = config.clone();
        removed_resource_config.resources.remove("tmp");
        let removed = run(
            &pool,
            &removed_resource_config,
            "tmp",
            "wc_sess_alpha",
            "printf never-started",
        );
        assert!(
            removed
                .error
                .as_deref()
                .is_some_and(|error| error.contains("ssh_resource_not_found")),
            "{removed:?}"
        );
        assert!(
            pool.control_path_for(7, "tmp", "wc_sess_alpha").is_none(),
            "removing a resource releases its old Session transport"
        );
        assert_eq!(
            pool.connection_count(),
            1,
            "removing a resource releases every pooled Session transport for that name"
        );
    }

    #[test]
    fn remote_async_jobs_stream_output_and_stop_through_the_existing_lifecycle() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let mut manager = crate::JobManager::new(1);
        manager.ssh_pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let sink = crate::webcodex_runner::AgentSink::WebSocket {
            tx,
            client_id: "ssh-agent".to_string(),
            agent_instance_id: "ssh-instance".to_string(),
        };
        manager.enqueue(
            sink.clone(),
            11,
            AgentPolicy::default(),
            crate::webcodex_runner::ShellConfig::default(),
            config.clone(),
            PathBuf::new(),
            ssh_job_request("ssh-job-complete", "tmp", "printf job-ok"),
        );
        let completed = wait_for_job_update(&mut rx, "ssh-job-complete", |update| update.finished);
        assert_eq!(completed.status, "completed", "{completed:?}");
        assert_eq!(completed.exit_code, Some(0), "{completed:?}");
        assert!(
            completed
                .log_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.stdout.tail.contains("job-ok")),
            "{completed:?}"
        );

        manager.enqueue(
            sink.clone(),
            11,
            AgentPolicy::default(),
            crate::webcodex_runner::ShellConfig::default(),
            config.clone(),
            PathBuf::new(),
            ssh_job_request("ssh-job-stop", "tmp", "sleep 30"),
        );
        let running =
            wait_for_job_update(&mut rx, "ssh-job-stop", |update| update.status == "running");
        assert!(!running.finished, "{running:?}");
        manager.stop("ssh-job-stop").expect("stop remote job");
        let stopped = wait_for_job_update(&mut rx, "ssh-job-stop", |update| update.finished);
        assert_eq!(stopped.status, "stopped", "{stopped:?}");
        assert_eq!(stopped.exit_code, Some(-1), "{stopped:?}");

        manager.enqueue(
            sink,
            11,
            AgentPolicy::default(),
            crate::webcodex_runner::ShellConfig::default(),
            config,
            PathBuf::new(),
            ssh_job_request("ssh-job-missing", "missing", "printf never-started"),
        );
        let missing = wait_for_job_update(&mut rx, "ssh-job-missing", |update| update.finished);
        assert_eq!(missing.status, "failed", "{missing:?}");
        assert!(
            missing
                .error
                .as_deref()
                .is_some_and(|error| error.contains("ssh_resource_not_found")),
            "{missing:?}"
        );
        assert!(
            missing
                .error
                .as_deref()
                .is_some_and(|error| error.contains("command was not started")),
            "{missing:?}"
        );
    }

    fn ssh_job_request(
        job_id: &str,
        resource: &str,
        command: &str,
    ) -> crate::shell_protocol::ShellAgentShellRequest {
        serde_json::from_value(serde_json::json!({
            "request_id": format!("request-{job_id}"),
            "client_id": "ssh-agent",
            "kind": "start_job",
            "job_id": job_id,
            "command": command,
            "timeout_secs": 60,
            "requested_by": "test",
            "created_at": 1,
            "job_context": {
                "runtime_project_id": "agent:ssh-agent:remote-project",
                "workflow_session_id": "wc_sess_ssh_job",
                "ssh_resource": resource,
                "project_cwd": ".",
                "purpose": "other",
                "shell": "remote",
                "command_preview": "remote test command",
                "validation_steps": []
            }
        }))
        .expect("build remote SSH job request")
    }

    fn wait_for_job_update(
        rx: &mut tokio::sync::mpsc::Receiver<crate::shell_protocol::AgentEnvelope>,
        job_id: &str,
        predicate: impl Fn(&crate::shell_protocol::ShellAgentJobUpdateRequest) -> bool,
    ) -> crate::shell_protocol::ShellAgentJobUpdateRequest {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(crate::shell_protocol::AgentEnvelope::JobUpdate { payload })
                    if payload.job_id == job_id && predicate(&payload) =>
                {
                    return payload;
                }
                Ok(_) | Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    panic!("remote SSH job update channel closed")
                }
            }
        }
        panic!("timed out waiting for remote SSH job {job_id}");
    }

    /// A projects.d directory with a registered `remote-project` owned by the
    /// `ssh-agent` Runner. SSH persistent shells still require the project to
    /// be registered on the Runner (it owns the agent + resource binding);
    /// only execution happens remotely.
    fn ssh_projects_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create ssh projects dir");
        std::fs::write(
            dir.path().join("remote-project.toml"),
            "id = \"remote-project\"\npath = \"/srv/remote-project\"\n",
        )
        .expect("write remote-project.toml");
        dir
    }

    fn ssh_persistent_shell_request(
        action: &str,
        shell_id: &str,
        resource: &str,
        command: Option<&str>,
    ) -> crate::shell_protocol::ShellAgentShellRequest {
        ssh_persistent_shell_request_at_cwd(action, shell_id, resource, None, command)
    }

    fn ssh_persistent_shell_request_at_cwd(
        action: &str,
        shell_id: &str,
        resource: &str,
        cwd: Option<&str>,
        command: Option<&str>,
    ) -> crate::shell_protocol::ShellAgentShellRequest {
        serde_json::from_value(serde_json::json!({
            "request_id": format!("req-{action}-{shell_id}"),
            "client_id": "ssh-agent",
            "kind": "persistent_shell",
            "command": command.unwrap_or(""),
            "timeout_secs": 30,
            "requested_by": "test",
            "created_at": 1,
            "job_context": {
                "runtime_project_id": "agent:ssh-agent:remote-project",
                "workflow_session_id": "wc_sess_ssh_pshell",
                "ssh_resource": resource,
                "project_cwd": ".",
                "purpose": "other",
                "shell": "remote",
                "command_preview": "",
                "validation_steps": []
            },
            "persistent_shell": {
                "action": action,
                "shell_id": shell_id,
                "workflow_session_id": "wc_sess_ssh_pshell",
                "runtime_project_id": "agent:ssh-agent:remote-project",
                "cwd": cwd,
                "shell": "bash",
                "command": command,
                "timeout_secs": 20,
                "purpose": null
            }
        }))
        .expect("build remote SSH persistent shell request")
    }

    fn ssh_config_without_default(server: &TestSshServer) -> SshConfig {
        let mut resources = BTreeMap::new();
        resources.insert(
            "nodefault".to_string(),
            SshResourceConfig {
                host: server.alias.clone(),
                default_cwd: None,
            },
        );
        SshConfig { resources }
    }

    #[test]
    fn remote_persistent_shell_preserves_state_across_execs() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("open", "wc_shell_rps", "tmp", None),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(opened.error_code, None, "{opened:?}");

        // cd, export, a shell variable, a function, and umask must all persist.
        let setup = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_rps",
                "tmp",
                Some("cd /tmp; export WC_RPS=kept; WC_LOCAL=v; rps_fn() { printf fn; }; umask 027"),
            ),
        );
        assert_eq!(setup.exit_code, Some(0), "{setup:?}");
        assert_eq!(setup.shell_state, "running", "{setup:?}");

        let observed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_rps",
                "tmp",
                Some("printf '%s:%s:' \"$PWD\" \"$WC_RPS\"; rps_fn; printf ':%s' \"$(umask)\""),
            ),
        );
        assert_eq!(observed.exit_code, Some(0), "{observed:?}");
        assert_eq!(observed.shell_state, "running", "{observed:?}");
        assert!(
            observed.stdout.contains("/tmp:kept:fn:0027"),
            "remote state did not persist: {:?}",
            observed.stdout
        );
        // Internal markers must never leak into user output.
        assert!(
            !observed.stdout.contains("WCPS"),
            "marker leaked: {:?}",
            observed.stdout
        );
        assert!(
            !observed.stderr.contains("WCPS"),
            "marker leaked: {:?}",
            observed.stderr
        );

        // unset propagates.
        let unset = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("exec", "wc_shell_rps", "tmp", Some("unset WC_RPS")),
        );
        assert_eq!(unset.exit_code, Some(0), "{unset:?}");
        let gone = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_rps",
                "tmp",
                Some("printf '%s' \"${WC_RPS-unset}\""),
            ),
        );
        assert_eq!(gone.stdout, "unset", "{gone:?}");

        let closed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("close", "wc_shell_rps", "tmp", None),
        );
        assert_eq!(closed.shell_state, "closed", "{closed:?}");
    }

    #[test]
    fn remote_persistent_shell_resets_when_resource_is_removed() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("open", "wc_shell_rps_reset", "tmp", None),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");

        // Removing the resource from the active config invalidates the shell.
        let mut removed = config.clone();
        removed.resources.remove("tmp");
        let exec = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &removed,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_rps_reset",
                "tmp",
                Some("printf forbidden"),
            ),
        );
        assert_eq!(
            exec.error_code.as_deref(),
            Some("shell_reset_required"),
            "{exec:?}"
        );
    }

    #[test]
    fn remote_persistent_shell_resets_when_config_generation_changes() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();
        let shell_id = "wc_shell_rps_gen";
        let resource = "tmp";

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("open", shell_id, resource, None),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");

        // The first exec runs on the generation-7 binding and reaches the
        // remote host.
        let first = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                shell_id,
                resource,
                Some("printf generation-7-command"),
            ),
        );
        assert_eq!(first.exit_code, Some(0), "{first:?}");
        assert_eq!(first.stdout, "generation-7-command", "{first:?}");
        assert_eq!(first.shell_state, "running", "{first:?}");

        // The Runner config advanced to generation 8 for the same resource
        // name. The old shell must not run the user command.
        let reset = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            8,
            &projects,
            &ssh_persistent_shell_request("exec", shell_id, resource, Some("printf forbidden")),
        );
        assert_eq!(
            reset.error_code.as_deref(),
            Some("shell_reset_required"),
            "{reset:?}"
        );
        assert!(!reset.command_started, "{reset:?}");
        assert_eq!(reset.stdout, "", "{reset:?}");

        // The old shell is closed and can no longer run user commands; a
        // further exec reports its terminal state instead of executing.
        let stale = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            8,
            &projects,
            &ssh_persistent_shell_request("exec", shell_id, resource, Some("printf stale")),
        );
        assert!(
            stale.error_code.is_some(),
            "closed old shell must reject further execs: {stale:?}"
        );
        assert!(!stale.command_started, "{stale:?}");
        assert_eq!(stale.stdout, "", "{stale:?}");
        assert_eq!(manager.active_count(), 0, "{stale:?}");

        // Reopening on generation 8 gives a working shell again.
        let reopened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            8,
            &projects,
            &ssh_persistent_shell_request("open", "wc_shell_rps_gen_new", resource, None),
        );
        assert_eq!(reopened.shell_state, "running", "{reopened:?}");
        let after = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            8,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_rps_gen_new",
                resource,
                Some("printf generation-8-command"),
            ),
        );
        assert_eq!(after.exit_code, Some(0), "{after:?}");
        assert_eq!(after.stdout, "generation-8-command", "{after:?}");
    }

    #[test]
    fn remote_persistent_shell_applies_explicit_cwd_over_resource_default() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();

        // The resource default_cwd is server.remote_cwd; an explicit cwd must
        // win.
        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request_at_cwd(
                "open",
                "wc_shell_cwd_explicit",
                "tmp",
                Some("/tmp"),
                None,
            ),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(opened.cwd.as_deref(), Some("/tmp"), "{opened:?}");
        assert_eq!(opened.initial_cwd.as_deref(), Some("/tmp"), "{opened:?}");
        assert!(
            !opened.stdout.contains("WCPS"),
            "marker leaked: {:?}",
            opened.stdout
        );

        let observed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("exec", "wc_shell_cwd_explicit", "tmp", Some("pwd -P")),
        );
        assert_eq!(observed.exit_code, Some(0), "{observed:?}");
        assert_eq!(observed.stdout.trim(), "/tmp", "{observed:?}");
        assert_eq!(observed.cwd.as_deref(), Some("/tmp"), "{observed:?}");
        assert!(
            !observed.stdout.contains("WCPS"),
            "marker leaked: {:?}",
            observed.stdout
        );
        assert!(
            !observed.stderr.contains("WCPS"),
            "marker leaked: {:?}",
            observed.stderr
        );
    }

    #[test]
    fn remote_persistent_shell_uses_session_cwd_when_no_explicit_open_cwd() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();
        let session_cwd = server.remote_cwd.join("session-dir");
        std::fs::create_dir(&session_cwd).expect("create session cwd");
        let session_cwd = session_cwd.to_string_lossy().into_owned();

        // The Server sends the Session default_cwd as operation.cwd. It must
        // become the actual remote initial cwd even though the resource also
        // has a default_cwd.
        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request_at_cwd(
                "open",
                "wc_shell_cwd_session",
                "tmp",
                Some(&session_cwd),
                None,
            ),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(
            opened.cwd.as_deref(),
            Some(session_cwd.as_str()),
            "{opened:?}"
        );
        assert_eq!(
            opened.initial_cwd.as_deref(),
            Some(session_cwd.as_str()),
            "{opened:?}"
        );

        let observed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("exec", "wc_shell_cwd_session", "tmp", Some("pwd -P")),
        );
        assert_eq!(observed.exit_code, Some(0), "{observed:?}");
        assert_eq!(observed.stdout.trim(), session_cwd, "{observed:?}");
        assert_eq!(
            observed.cwd.as_deref(),
            Some(session_cwd.as_str()),
            "{observed:?}"
        );
    }

    #[test]
    fn remote_persistent_shell_uses_resource_default_cwd_when_nothing_else() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();
        let resource_cwd = server.remote_cwd.to_string_lossy().into_owned();

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("open", "wc_shell_cwd_resource", "tmp", None),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(
            opened.cwd.as_deref(),
            Some(resource_cwd.as_str()),
            "{opened:?}"
        );

        let observed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("exec", "wc_shell_cwd_resource", "tmp", Some("pwd -P")),
        );
        assert_eq!(observed.exit_code, Some(0), "{observed:?}");
        assert_eq!(observed.stdout.trim(), resource_cwd, "{observed:?}");
        assert_eq!(
            observed.cwd.as_deref(),
            Some(resource_cwd.as_str()),
            "{observed:?}"
        );
    }

    #[test]
    fn remote_persistent_shell_keeps_login_dir_when_no_cwd_is_specified() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config_without_default(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();

        // The remote login directory: what a plain `ssh host pwd -P` reports
        // when no cd is issued.
        let login_dir = Command::new("ssh")
            .arg("-F")
            .arg(&server.client_config)
            .arg(&server.alias)
            .arg("pwd -P")
            .output()
            .expect("resolve SSH test login directory");
        assert!(
            login_dir.status.success(),
            "could not resolve SSH test login directory: {}",
            String::from_utf8_lossy(&login_dir.stderr)
        );
        let login_dir = String::from_utf8_lossy(&login_dir.stdout)
            .trim()
            .to_string();
        assert!(!login_dir.is_empty(), "SSH login directory is empty");

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("open", "wc_shell_cwd_login", "nodefault", None),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert!(opened.error.is_none(), "{opened:?}");
        assert_eq!(
            opened.cwd.as_deref(),
            Some(login_dir.as_str()),
            "{opened:?}"
        );
        assert_eq!(
            opened.initial_cwd.as_deref(),
            Some(login_dir.as_str()),
            "without any requested cwd the reported initial cwd is the login directory, not an empty seed"
        );

        let changed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_cwd_login",
                "nodefault",
                Some("cd /tmp"),
            ),
        );
        assert_eq!(changed.exit_code, Some(0), "{changed:?}");
        assert_eq!(changed.cwd.as_deref(), Some("/tmp"), "{changed:?}");

        let status = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("status", "wc_shell_cwd_login", "nodefault", None),
        );
        assert_eq!(status.cwd.as_deref(), Some("/tmp"), "{status:?}");
        assert_eq!(
            status.initial_cwd.as_deref(),
            Some(login_dir.as_str()),
            "status must retain the login directory as initial_cwd: {status:?}"
        );

        let closed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("close", "wc_shell_cwd_login", "nodefault", None),
        );
        assert_eq!(closed.cwd.as_deref(), Some("/tmp"), "{closed:?}");
        assert_eq!(
            closed.initial_cwd.as_deref(),
            Some(login_dir.as_str()),
            "close must retain the login directory as initial_cwd: {closed:?}"
        );
    }

    #[test]
    fn remote_persistent_shell_freezes_physical_cwd_from_symlink_request() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();
        let physical = server.remote_cwd.join("physical");
        let logical = server.remote_cwd.join("logical");
        std::fs::create_dir(&physical).expect("create physical remote cwd");
        symlink(&physical, &logical).expect("create remote cwd symlink");
        let physical = physical.canonicalize().expect("canonicalize physical cwd");
        let physical = physical.to_string_lossy().into_owned();
        let logical = logical.to_string_lossy().into_owned();

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request_at_cwd(
                "open",
                "wc_shell_cwd_symlink",
                "tmp",
                Some(&logical),
                None,
            ),
        );
        assert_eq!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(opened.cwd.as_deref(), Some(physical.as_str()), "{opened:?}");
        assert_eq!(
            opened.initial_cwd.as_deref(),
            Some(physical.as_str()),
            "initial_cwd must use pwd -P rather than the requested symlink: {opened:?}"
        );

        let changed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("exec", "wc_shell_cwd_symlink", "tmp", Some("cd /tmp")),
        );
        assert_eq!(changed.exit_code, Some(0), "{changed:?}");

        let status = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("status", "wc_shell_cwd_symlink", "tmp", None),
        );
        assert_eq!(status.cwd.as_deref(), Some("/tmp"), "{status:?}");
        assert_eq!(
            status.initial_cwd.as_deref(),
            Some(physical.as_str()),
            "{status:?}"
        );

        let closed = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request("close", "wc_shell_cwd_symlink", "tmp", None),
        );
        assert_eq!(closed.cwd.as_deref(), Some("/tmp"), "{closed:?}");
        assert_eq!(
            closed.initial_cwd.as_deref(),
            Some(physical.as_str()),
            "{closed:?}"
        );
    }

    #[test]
    fn remote_persistent_shell_open_fails_when_cwd_is_unavailable() {
        let Some(server) = TestSshServer::start() else {
            eprintln!("skipping SSH integration test because sshd is unavailable");
            return;
        };
        let config = ssh_config(&server);
        let pool = SshConnectionPool::with_test_config(server.client_config.clone());
        let manager = crate::webcodex_runner::PersistentShellManager::new(
            &crate::webcodex_runner::ShellConfig::default(),
            pool,
        );
        let policy = AgentPolicy::default();
        let projects_dir = ssh_projects_dir();
        let projects = projects_dir.path().to_path_buf();
        let missing = server
            .remote_cwd
            .join("does-not-exist")
            .to_string_lossy()
            .into_owned();

        let opened = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request_at_cwd(
                "open",
                "wc_shell_cwd_missing",
                "tmp",
                Some(&missing),
                None,
            ),
        );
        assert_eq!(
            opened.error_code.as_deref(),
            Some("persistent_shell_initialization_failed"),
            "{opened:?}"
        );
        assert_ne!(opened.shell_state, "running", "{opened:?}");
        assert_eq!(manager.active_count(), 0, "{opened:?}");

        // The failed shell is fully torn down; nothing can be executed against
        // a shell that failed to open.
        let stale = manager.handle(
            &policy,
            &crate::webcodex_runner::ShellConfig::default(),
            &config,
            7,
            &projects,
            &ssh_persistent_shell_request(
                "exec",
                "wc_shell_cwd_missing",
                "tmp",
                Some("printf never-runs"),
            ),
        );
        assert!(
            stale.error_code.is_some(),
            "failed open must leave no usable shell: {stale:?}"
        );
        assert_eq!(stale.stdout, "", "{stale:?}");
    }
}
