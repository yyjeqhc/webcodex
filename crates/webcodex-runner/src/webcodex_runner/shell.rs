use super::config::{
    dialect_for_program, platform_default_dialect, validate_shell_config, AgentPolicy, ShellConfig,
    ShellDialect, ShellProfileConfig,
};
use super::output::{CommandResult, ShellCommandResult};
use super::projects::find_project_shell_context;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use webcodex_process::{GracefulTermination, ManagedChild};

const SHELL_PROFILE_PREPARE_TIMEOUT_SECS: u64 = 30;
const PROCESS_GROUP_TERMINATION_GRACE: Duration = Duration::from_millis(50);
const PROFILE_PREPARE_PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreparedShellProfileKey {
    generation: u64,
    project_key: String,
    profile_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedShellProfile {
    pub(crate) profile_name: String,
    program: String,
    args: Vec<String>,
    dialect: ShellDialect,
    env_snapshot: HashMap<String, String>,
}

/// Lazily prepared shell environment snapshots. Snapshots are keyed by
/// config generation, project/cwd, and profile name because inline init
/// scripts such as `. .venv/bin/activate` are intentionally resolved from the
/// project cwd. A successful hot reload retires older cached generations after
/// the new generation prepares its first snapshot.
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedShellProfileCache {
    profiles: Arc<Mutex<HashMap<PreparedShellProfileKey, Arc<PreparedShellProfile>>>>,
}

/// POSIX sh single-quote escaping.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// PowerShell single-quote escaping (an embedded single quote is doubled).
/// PowerShell's single-quoted strings are literal, so spaces, backslashes,
/// double quotes, `$`, Unicode, and `C:\...` Windows paths need no further
/// escaping; only `'` does.
fn shell_quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Resolve the effective shell dialect: an explicit config value wins, then a
/// known shell program basename, then the platform default. Profiles pass
/// `profile.dialect.or(shell.dialect)` as the explicit value so they inherit
/// the parent shell dialect unless designed otherwise.
fn resolve_dialect(program: &str, explicit: Option<ShellDialect>) -> ShellDialect {
    explicit
        .or_else(|| dialect_for_program(program))
        .unwrap_or_else(platform_default_dialect)
}

const SENSITIVE_ENV_KEYS: [&str; 4] = [
    "WEBCODEX_TOKEN",
    "WEBCODEX_AGENT_TOKEN",
    "WEBCODEX_USER_TOKEN",
    "AUTHORIZATION",
];

/// Sensitive environment keys must never reach child processes. Windows
/// environment names are case-insensitive, so a mixed-case spelling such as
/// `WebCodex_Token` must be filtered too; Unix stays case-sensitive.
fn is_sensitive_env_key(key: &str) -> bool {
    if cfg!(windows) {
        let upper = key.to_ascii_uppercase();
        SENSITIVE_ENV_KEYS
            .iter()
            .any(|sensitive| *sensitive == upper)
    } else {
        SENSITIVE_ENV_KEYS.contains(&key)
    }
}

fn should_inherit_env_key(key: &str) -> bool {
    !is_sensitive_env_key(key)
}

/// Case-insensitive lookup on Windows (where environment names are
/// case-insensitive), exact match on Unix.
fn env_lookup<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a String> {
    if cfg!(windows) {
        env.iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    } else {
        env.get(key)
    }
}

/// Insert `key=value`, replacing any existing entry that names the same
/// environment variable. On Windows the replacement is case-insensitive so a
/// snapshot never carries both `Path` and `PATH` (which would make the final
/// child environment depend on HashMap iteration order); on Unix it is exact.
fn env_insert(env: &mut HashMap<String, String>, key: &str, value: String) {
    if cfg!(windows) {
        env.retain(|candidate, _| !candidate.eq_ignore_ascii_case(key));
    }
    env.insert(key.to_string(), value);
}

/// Remove every sensitive environment key from `env`, case-insensitively on
/// Windows (a profile could configure `webcodetoken = ...`).
fn remove_sensitive_env(env: &mut HashMap<String, String>) {
    let sensitive: Vec<String> = env
        .keys()
        .filter(|key| is_sensitive_env_key(key))
        .cloned()
        .collect();
    for key in sensitive {
        env.remove(&key);
    }
}

/// Deterministic UTF-8 setup for redirected PowerShell output. Bounded to the
/// child process: when stdout is redirected, .NET only caches these encodings
/// instead of calling SetConsoleOutputCP, so the parent Runner console state
/// is never mutated. PowerShell 5.1 otherwise writes through the console code
/// page (OEM), which would corrupt Unicode output and the env snapshot.
const POWERSHELL_UTF8_PREAMBLE: &str = concat!(
    "try { $OutputEncoding = [Console]::InputEncoding = ",
    "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch { }",
);

/// Wrap `command` so the PowerShell process exits with a meaningful status:
/// the shell's own `exit N` statements pass through, a failing trailing native
/// command is reported through `$LASTEXITCODE`, a failing PowerShell statement
/// returns 1, and a successful trailing PowerShell statement returns 0 even if
/// an earlier native command left a stale non-zero `$LASTEXITCODE` behind.
/// PowerShell 5.1 does not propagate these statuses consistently on its own,
/// so inspect `$?` immediately after the requested command and exit explicitly.
fn powershell_command_text(command: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         $LASTEXITCODE = 0\n\
         {command}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         exit 0"
    )
}

/// Dot-source a shell init script, then run the command only after the init
/// script reported success. Native-command failure inside the init script
/// blocks the command (like POSIX `. <path> && (...)`); a terminating
/// PowerShell error aborts the whole script, which also blocks the command.
///
/// `$?` is inspected immediately after dot-sourcing so a failed dot-source
/// operation (for example, an unavailable script) blocks the command. Native
/// failure status is preserved through `$LASTEXITCODE`; ordinary PowerShell
/// non-terminating errors retain PowerShell's own dot-source semantics.
fn powershell_init_command_text(init_script: &Path, command: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         . {}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         $LASTEXITCODE = 0\n\
         {command}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         exit 0",
        shell_quote_powershell(&init_script.to_string_lossy()),
    )
}

fn shell_command_text(shell: &ShellConfig, dialect: ShellDialect, command: &str) -> String {
    match (dialect, shell.init_script.as_ref()) {
        (ShellDialect::Posix, Some(path)) => format!(
            ". {} && (\n{}\n)",
            shell_quote(&path.to_string_lossy()),
            command
        ),
        (ShellDialect::Posix, None) => command.to_string(),
        (ShellDialect::PowerShell, Some(path)) => powershell_init_command_text(path, command),
        (ShellDialect::PowerShell, None) => powershell_command_text(command),
    }
}

/// Command text for an already-prepared profile execution. POSIX shells get
/// the raw command (the last statement's status is the shell status); the
/// PowerShell wrapper adds the explicit exit-status propagation.
fn prepared_shell_command_text(dialect: ShellDialect, command: &str) -> String {
    match dialect {
        ShellDialect::Posix => command.to_string(),
        ShellDialect::PowerShell => powershell_command_text(command),
    }
}

fn apply_shell_environment(cmd: &mut Command, shell: &ShellConfig) -> Result<(), String> {
    // Rust's Windows env handling is case-insensitive (like the OS itself), so
    // removing the canonical spellings also removes mixed-case variants such
    // as `WebCodex_Token`.
    for key in SENSITIVE_ENV_KEYS {
        cmd.env_remove(key);
    }
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        cmd.env("PATH", joined);
    }
    for (key, value) in &shell.env {
        if !is_sensitive_env_key(key) {
            cmd.env(key, value);
        }
    }
    Ok(())
}

fn apply_env_snapshot(cmd: &mut Command, env_snapshot: &HashMap<String, String>) {
    cmd.env_clear();
    for (key, value) in env_snapshot {
        cmd.env(key, value);
    }
}

/// On Windows, resolve a bare shell program name through the platform rules
/// so an extensionless POSIX shim shadowing the real executable is never
/// selected (CreateProcess would fail with error 193). Path-qualified values
/// are used verbatim; an unresolvable bare name falls back to the configured
/// value so the spawn surfaces the real error.
fn resolved_shell_program(program: &str) -> String {
    #[cfg(windows)]
    {
        let path = Path::new(program);
        if path.components().count() <= 1 && !path.is_absolute() {
            if let Some(resolved) = super::util::resolve_program_in_path(
                program,
                std::env::var_os("PATH")
                    .as_deref()
                    .unwrap_or(OsStr::new("")),
            ) {
                return resolved.path().to_string_lossy().into_owned();
            }
        }
    }
    #[cfg(not(windows))]
    let _ = program;
    program.to_string()
}

fn configured_shell_command(shell: &ShellConfig, command: &str) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let dialect = resolve_dialect(&shell.program, shell.dialect);
    let program = resolved_shell_program(&shell.program);
    let mut cmd = Command::new(program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, dialect, command));
    // The shell execution path owns its process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

fn configured_prepared_shell_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(prepared_shell_command_text(profile.dialect, command));
    // The shell execution path owns its process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

pub(crate) fn configured_shell_job_command(
    shell: &ShellConfig,
    command: &str,
) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let dialect = resolve_dialect(&shell.program, shell.dialect);
    let program = resolved_shell_program(&shell.program);
    let mut cmd = Command::new(program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, dialect, command));
    // JobManager owns this process tree through ManagedChild; do not add
    // the legacy setsid pre_exec here. ManagedChild creates the private group.
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

pub(crate) fn configured_prepared_shell_job_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(prepared_shell_command_text(profile.dialect, command));
    // JobManager owns this process tree through ManagedChild; do not add
    // the legacy setsid pre_exec here. ManagedChild creates the private group.
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

pub(crate) fn configured_validation_job_command(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    args: &[String],
) -> Result<Command, String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    // JobManager owns this process tree through ManagedChild; do not add
    // the legacy setsid pre_exec here. ManagedChild creates the private group.
    match profile {
        Some(profile) => apply_env_snapshot(&mut cmd, &profile.env_snapshot),
        None => {
            validate_shell_config(shell)?;
            apply_shell_environment(&mut cmd, shell)?;
        }
    }
    Ok(cmd)
}

pub(crate) fn base_shell_env(
    shell: &ShellConfig,
    profile: &ShellProfileConfig,
) -> Result<HashMap<String, String>, String> {
    let mut env: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| should_inherit_env_key(key))
        .collect();
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        // The inherited Windows PATH may be spelled `Path`; lookup must be
        // case-insensitive or the prepended entries would replace it instead
        // of extending it.
        if let Some(current) = env_lookup(&env, "PATH") {
            paths.extend(std::env::split_paths(current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        env_insert(&mut env, "PATH", joined.to_string_lossy().to_string());
    }
    for (key, value) in &shell.env {
        env_insert(&mut env, key, value.clone());
    }
    for (key, value) in &profile.env {
        env_insert(&mut env, key, value.clone());
    }
    // A profile could configure a sensitive name in any case; Windows filters
    // case-insensitively.
    remove_sensitive_env(&mut env);
    Ok(env)
}

fn stderr_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    const MAX_ERR: usize = 4096;
    if text.len() <= MAX_ERR {
        return text;
    }
    let mut start = text.len() - MAX_ERR;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!("[stderr truncated]\n{}", &text[start..])
}

struct ProfilePreparePipeReader {
    stream_name: &'static str,
    result_rx: mpsc::Receiver<Result<Vec<u8>, String>>,
    handle: std::thread::JoinHandle<()>,
}

impl ProfilePreparePipeReader {
    fn finish_until(self, deadline: Instant) -> Result<Vec<u8>, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self.result_rx.recv_timeout(remaining) {
            Ok(result) => {
                join_profile_prepare_reader_until(self.handle, deadline, self.stream_name)?;
                result
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "profile prepare {} reader did not finish before the cleanup deadline",
                self.stream_name
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                join_profile_prepare_reader_until(self.handle, deadline, self.stream_name)?;
                Err(format!(
                    "profile prepare {} reader exited without a result",
                    self.stream_name
                ))
            }
        }
    }
}

fn join_profile_prepare_reader_until(
    handle: std::thread::JoinHandle<()>,
    deadline: Instant,
    stream_name: &'static str,
) -> Result<(), String> {
    while !handle.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "profile prepare {stream_name} reader did not join before the cleanup deadline"
            ));
        }
        std::thread::sleep(Duration::from_millis(5).min(remaining));
    }
    handle
        .join()
        .map_err(|_| format!("profile prepare {stream_name} reader panicked"))
}

fn spawn_profile_prepare_pipe_reader(
    stream_name: &'static str,
    mut pipe: impl Read + Send + 'static,
) -> ProfilePreparePipeReader {
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = pipe
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare {stream_name}: {e}"));
        let _ = result_tx.send(result);
    });
    ProfilePreparePipeReader {
        stream_name,
        result_rx,
        handle,
    }
}

fn collect_profile_prepare_output(
    stdout: ProfilePreparePipeReader,
    stderr: ProfilePreparePipeReader,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let deadline = Instant::now() + PROFILE_PREPARE_PIPE_DRAIN_TIMEOUT;
    let stdout = stdout.finish_until(deadline)?;
    let stderr = stderr.finish_until(deadline)?;
    Ok((stdout, stderr))
}

fn run_prepare_command(
    mut cmd: Command,
    timeout: Duration,
    stop_requested: Option<&AtomicBool>,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    if stop_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("profile prepare stopped during runner shutdown".to_string());
    }
    // ManagedChild owns the whole profile-prepare process tree: a private
    // process group on Unix, a kill-on-close Job Object on Windows.
    let mut child = ManagedChild::spawn(&mut cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))
        .map_err(|e| format!("failed to spawn profile prepare command: {}", e))?;
    let stdout = match child.child_mut().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let cleanup = terminate_child_without_output(child).err();
            return Err(with_cleanup_error(
                "profile prepare stdout pipe missing",
                cleanup,
            ));
        }
    };
    let stderr = match child.child_mut().stderr.take() {
        Some(stderr) => stderr,
        None => {
            drop(stdout);
            let cleanup = terminate_child_without_output(child).err();
            return Err(with_cleanup_error(
                "profile prepare stderr pipe missing",
                cleanup,
            ));
        }
    };
    let stdout_reader = spawn_profile_prepare_pipe_reader("stdout", stdout);
    let stderr_reader = spawn_profile_prepare_pipe_reader("stderr", stderr);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if stop_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    let cleanup = terminate_child_process_tree(&mut child).err();
                    let output = collect_profile_prepare_output(stdout_reader, stderr_reader).err();
                    return Err(with_cleanup_error(
                        output.map_or_else(
                            || "profile prepare stopped during runner shutdown".to_string(),
                            |error| {
                                format!(
                                    "profile prepare stopped during runner shutdown; failed to collect output: {error}"
                                )
                            },
                        ),
                        cleanup,
                    ));
                }
                if start.elapsed() >= timeout {
                    let cleanup = terminate_child_process_tree(&mut child).err();
                    return match collect_profile_prepare_output(stdout_reader, stderr_reader) {
                        Ok((_stdout, stderr)) => Err(format!(
                            "profile prepare timed out after {} seconds; stderr tail: {}{}",
                            timeout.as_secs(),
                            stderr_tail(&stderr),
                            cleanup
                                .as_deref()
                                .map(|error| format!("; cleanup failed: {error}"))
                                .unwrap_or_default(),
                        )),
                        Err(error) => Err(with_cleanup_error(
                            format!(
                                "profile prepare timed out after {} seconds; failed to collect output: {}",
                                timeout.as_secs(),
                                error
                            ),
                            cleanup,
                        )),
                    };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let cleanup = terminate_child_process_tree(&mut child).err();
                let output = collect_profile_prepare_output(stdout_reader, stderr_reader).err();
                let base = match output {
                    Some(error) => {
                        format!("failed to wait profile prepare command: {}; failed to collect output: {}", e, error)
                    }
                    None => format!("failed to wait profile prepare command: {}", e),
                };
                return Err(with_cleanup_error(base, cleanup));
            }
        }
    };
    // The direct child has already exited, but its managed process tree can
    // still contain background descendants that inherited these pipe handles.
    // Terminate the whole tree before waiting on the readers so they see EOF
    // promptly.
    let cleanup = terminate_child_process_tree(&mut child).err();
    let output = collect_profile_prepare_output(stdout_reader, stderr_reader);
    match (cleanup, output) {
        (None, Ok((stdout, stderr))) => Ok((status, stdout, stderr)),
        (Some(cleanup), Ok(_)) => Err(format!(
            "failed to clean up profile prepare command process group: {cleanup}"
        )),
        (None, Err(error)) => Err(format!("failed to collect profile prepare output: {error}")),
        (Some(cleanup), Err(error)) => Err(format!(
            "failed to clean up profile prepare command process group: {cleanup}; failed to collect output: {error}"
        )),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_env_payload(
    payload: &[u8],
    profile_name: &str,
) -> Result<HashMap<String, String>, String> {
    let mut env = HashMap::new();
    for entry in payload.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': entry missing '='",
                profile_name
            ));
        };
        let key = std::str::from_utf8(&entry[..eq]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': key is not UTF-8",
                profile_name
            )
        })?;
        if key.is_empty() {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': empty env key",
                profile_name
            ));
        }
        let value = std::str::from_utf8(&entry[eq + 1..]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': value is not UTF-8",
                profile_name
            )
        })?;
        if should_inherit_env_key(key) {
            env.insert(key.to_string(), value.to_string());
        }
    }
    Ok(env)
}

/// POSIX profile-prepare script: `set -e`, the configured init snippet, a
/// marker line, then a NUL-delimited `env -0` dump. Unchanged from the legacy
/// Unix behavior.
fn posix_profile_prepare_script(init_script: &str, marker: &str) -> String {
    format!(
        "set -e\n{}\nprintf '\\n{}\\n'\nenv -0\n",
        init_script, marker
    )
}

/// PowerShell profile-prepare script. The UTF-8 preamble makes the env dump
/// deterministic Unicode; `$ErrorActionPreference = 'Stop'` mirrors `set -e`
/// for cmdlet errors (a terminating error in the snippet aborts preparation
/// and reports a non-zero exit instead of producing a truncated snapshot); the
/// trailing `$LASTEXITCODE` truthiness check mirrors it for the last native
/// command (`$LASTEXITCODE` is `$null` after a pure-PowerShell snippet, which
/// is falsy). The marker is a host-formatted line, then each environment
/// entry is written as `NAME=VALUE\0` straight through `[Console]::Out` — no
/// human-formatted table, no line wrapping, values may contain `=`, spaces,
/// quotes, newlines, or any shell metacharacter, and entries are unambiguous
/// because the value can never contain NUL. `Get-ChildItem Env:` reflects the
/// process block after the init snippet ran.
fn powershell_profile_prepare_script(init_script: &str, marker: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         $ErrorActionPreference = 'Stop'\n\
         try {{\n\
         {init_script}\n\
         if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}\n\
         }} catch {{\n\
         [Console]::Error.WriteLine($_)\n\
         exit 1\n\
         }}\n\
         Write-Output '{marker}'\n\
         Get-ChildItem Env: | ForEach-Object {{ \
         [Console]::Out.Write($_.Name + '=' + $_.Value + [string][char]0) }}\n\
         [Console]::Out.Flush()"
    )
}

fn capture_profile_env_snapshot(
    profile_name: &str,
    profile: &ShellProfileConfig,
    program: &str,
    args: &[String],
    dialect: ShellDialect,
    prepare_cwd: &Path,
    initial_env: HashMap<String, String>,
    stop_requested: Option<&AtomicBool>,
) -> Result<HashMap<String, String>, String> {
    let Some(init_script) = profile.init_script.as_deref() else {
        return Ok(initial_env);
    };
    let marker = format!("__WEBCODEX_ENV_START_{}__", uuid::Uuid::new_v4().simple());
    let prepare_script = match dialect {
        ShellDialect::Posix => posix_profile_prepare_script(init_script, &marker),
        ShellDialect::PowerShell => powershell_profile_prepare_script(init_script, &marker),
    };
    let mut cmd = Command::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(prepare_script).current_dir(prepare_cwd).env_clear();
    // `run_prepare_command` owns this process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    for (key, value) in initial_env {
        cmd.env(key, value);
    }
    let (status, stdout, stderr) = run_prepare_command(
        cmd,
        Duration::from_secs(SHELL_PROFILE_PREPARE_TIMEOUT_SECS),
        stop_requested,
    )
    .map_err(|e| {
        format!(
            "failed to prepare shell profile '{}' at {}: {}",
            profile_name,
            prepare_cwd.display(),
            e
        )
    })?;
    if !status.success() {
        return Err(format!(
            "failed to prepare shell profile '{}' at {}: exit code {}; stderr tail: {}",
            profile_name,
            prepare_cwd.display(),
            status.code().unwrap_or(-1),
            stderr_tail(&stderr)
        ));
    }
    let marker_pos = find_bytes(&stdout, marker.as_bytes()).ok_or_else(|| {
        format!(
            "failed to prepare shell profile '{}' at {}: env marker not found",
            profile_name,
            prepare_cwd.display()
        )
    })?;
    let mut payload_start = marker_pos + marker.len();
    while stdout
        .get(payload_start)
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        payload_start += 1;
    }
    let mut snapshot = parse_env_payload(&stdout[payload_start..], profile_name)?;
    // The init snippet may have exported a sensitive variable in any case;
    // Windows filters case-insensitively.
    remove_sensitive_env(&mut snapshot);
    Ok(snapshot)
}

impl PreparedShellProfileCache {
    /// Number of currently prepared snapshots. Used only for the sanitized
    /// observability summary; never exposes snapshot contents.
    pub(crate) fn len(&self) -> usize {
        self.profiles.lock().unwrap().len()
    }

    fn get_or_prepare(
        &self,
        generation: u64,
        shell: &ShellConfig,
        profile_name: &str,
        project_key: String,
        prepare_cwd: &Path,
        stop_requested: Option<&AtomicBool>,
    ) -> Result<Arc<PreparedShellProfile>, String> {
        let key = PreparedShellProfileKey {
            generation,
            project_key,
            profile_name: profile_name.to_string(),
        };
        let profiles = self.profiles.lock().unwrap();
        if let Some(prepared) = profiles.get(&key).cloned() {
            return Ok(prepared);
        }
        drop(profiles);
        let profile = shell.profiles.get(profile_name).ok_or_else(|| {
            format!(
                "shell profile '{}' is not configured for project/cwd {}",
                profile_name,
                prepare_cwd.display()
            )
        })?;
        let program = resolved_shell_program(
            &profile
                .program
                .clone()
                .unwrap_or_else(|| shell.program.clone()),
        );
        let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
        // The profile inherits the parent shell dialect unless it (or the
        // parent) explicitly configures one; the prepare script and every
        // later command in this profile use the same resolved dialect.
        let dialect = resolve_dialect(&program, profile.dialect.or(shell.dialect));
        let initial_env = base_shell_env(shell, profile)?;
        let env_snapshot = capture_profile_env_snapshot(
            profile_name,
            profile,
            &program,
            &args,
            dialect,
            prepare_cwd,
            initial_env,
            stop_requested,
        )?;
        let prepared = Arc::new(PreparedShellProfile {
            profile_name: profile_name.to_string(),
            program,
            args,
            dialect,
            env_snapshot,
        });
        let mut profiles = self.profiles.lock().unwrap();
        if let Some(cached) = profiles.get(&key).cloned() {
            return Ok(cached);
        }
        if profiles.keys().any(|cached| cached.generation > generation) {
            return Ok(prepared);
        }
        profiles.retain(|cached, _| cached.generation == generation);
        profiles.insert(key, prepared.clone());
        Ok(prepared)
    }
}

fn shell_profile_project_key(project_id: Option<&str>, path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    match project_id {
        Some(id) => format!("project:{}:{}", id, path),
        None => format!("cwd:{}", path),
    }
}

pub(crate) fn resolve_prepared_shell_profile(
    generation: u64,
    shell: &ShellConfig,
    projects_dir: &Path,
    cwd_path: &Path,
    request_has_cwd: bool,
    cache: &PreparedShellProfileCache,
    stop_requested: Option<&AtomicBool>,
) -> Result<Option<Arc<PreparedShellProfile>>, String> {
    let project = request_has_cwd
        .then(|| find_project_shell_context(projects_dir, cwd_path))
        .flatten();
    let profile_name = project
        .as_ref()
        .and_then(|project| project.shell_profile.as_deref())
        .or(shell.default_profile.as_deref());
    let Some(profile_name) = profile_name else {
        return Ok(None);
    };
    let prepare_cwd = project
        .as_ref()
        .map(|project| PathBuf::from(&project.path))
        .unwrap_or_else(|| cwd_path.to_path_buf());
    if let Some(project) = &project {
        if project.shell_profile.as_deref() == Some(profile_name)
            && !shell.profiles.contains_key(profile_name)
        {
            return Err(format!(
                "project '{}' shell_profile '{}' does not match any shell.profiles entry",
                project.id, profile_name
            ));
        }
    }
    let project_key = shell_profile_project_key(
        project.as_ref().map(|project| project.id.as_str()),
        &prepare_cwd,
    );
    cache
        .get_or_prepare(
            generation,
            shell,
            profile_name,
            project_key,
            &prepare_cwd,
            stop_requested,
        )
        .map(Some)
}

pub(crate) fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("failed to access {}: {}", path.display(), e))
}

pub(crate) fn cwd_allowed(policy: &AgentPolicy, cwd: &Path) -> Result<(), String> {
    if policy.allow_cwd_anywhere {
        return Ok(());
    }
    let cwd = canonicalize_existing(cwd)?;
    for root in &policy.allowed_roots {
        let root = canonicalize_existing(root)?;
        // Case-insensitive component-wise containment on Windows.
        if webcodex_agent_config::paths::path_is_within(&cwd, &root) {
            return Ok(());
        }
    }
    Err(format!(
        "cwd {} is outside allowed_roots",
        cwd.to_string_lossy()
    ))
}

fn truncate_bytes(bytes: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!(
        "[output truncated to last {} bytes]\n{}",
        max,
        &text[start..]
    )
}

fn with_cleanup_error(base: impl Into<String>, cleanup: Option<String>) -> String {
    match cleanup {
        Some(cleanup) => format!("{}; cleanup failed: {}", base.into(), cleanup),
        None => base.into(),
    }
}

/// Terminate a command and its entire managed process tree, then confirm the
/// whole tree exited and reap the direct child. Callers do this before waiting
/// for output pipes, so descendants cannot keep them open after a timeout,
/// stop, executor failure, or direct-child exit.
///
/// The whole tree is owned by the [`ManagedChild`]: a private process group on
/// Unix, a kill-on-close Job Object on Windows. No pid/pgid is handled here
/// directly.
fn terminate_child_process_tree(child: &mut ManagedChild) -> Result<(), String> {
    terminate_child_process_tree_until(child, Instant::now() + Duration::from_secs(1))
}

/// Terminate the managed command tree within one overall cleanup deadline.
///
/// Every phase recomputes its remaining budget from the single deadline, so an
/// expired graceful phase can never silently reuse a stale deadline for the
/// force-confirmation wait.
///
/// 1. Graceful request ([`GracefulTermination`]): on Unix this delivers SIGTERM
///    to the whole managed process group, which gets a bounded grace
///    ([`PROCESS_GROUP_TERMINATION_GRACE`]) to exit on its own; on Windows the
///    request reports `Unsupported` and the next phase escalates immediately.
/// 2. Force phase: `terminate_tree` for anything still alive.
/// 3. Whole-tree exit confirmation: `wait_tree_exit`, not just the direct
///    child (a direct-child exit never proves the tree is gone).
/// 4. Direct-child reap within the remaining budget.
fn terminate_child_process_tree_until(
    child: &mut ManagedChild,
    deadline: Instant,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut tree_exited = false;
    let mut force_tree = false;
    match child.request_terminate_tree() {
        Ok(GracefulTermination::Requested) => {
            let grace_deadline = deadline.min(Instant::now() + PROCESS_GROUP_TERMINATION_GRACE);
            let grace = grace_deadline.saturating_duration_since(Instant::now());
            match child.wait_tree_exit(grace) {
                Ok(true) => tree_exited = true,
                Ok(false) => force_tree = true,
                Err(error) => {
                    errors.push(format!(
                        "failed to wait for command process tree graceful exit: {error}"
                    ));
                    force_tree = true;
                }
            }
        }
        // Once the backend reports that the tree is already gone, do not
        // probe the numeric Unix process-group id again. The direct child may
        // already have been reaped, allowing that id to be reused by an
        // unrelated process group between calls.
        Ok(GracefulTermination::AlreadyExited) => tree_exited = true,
        Ok(GracefulTermination::Unsupported) => force_tree = true,
        Err(error) => {
            errors.push(format!(
                "failed to request graceful command process tree termination: {error}"
            ));
            force_tree = true;
        }
    }
    if force_tree {
        if let Err(error) = child.terminate_tree() {
            errors.push(format!("failed to terminate command process tree: {error}"));
        }
    }
    if !tree_exited {
        // Confirm the complete tree exited, not just the direct child.
        // Forceful termination can complete asynchronously (notably Job Object
        // teardown on Windows), so use the remaining cleanup budget.
        let remaining = deadline.saturating_duration_since(Instant::now());
        match child.wait_tree_exit(remaining) {
            Ok(true) => {}
            Ok(false) => {
                errors.push("command process tree did not exit before deadline".to_string())
            }
            Err(error) => errors.push(format!(
                "failed to wait for command process tree exit: {error}"
            )),
        }
    }
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    errors.push("command child reap timed out".to_string());
                    break;
                }
                std::thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) => {
                errors.push(format!("failed to reap command child: {error}"));
                break;
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn terminate_child_without_output(mut child: ManagedChild) -> Result<(), String> {
    let result = terminate_child_process_tree(&mut child);
    // The direct child has been reaped above. Closing the local pipe handles
    // is sufficient on error paths where the response intentionally has no
    // command output.
    drop(child.child_mut().stdout.take());
    drop(child.child_mut().stderr.take());
    result
}

fn terminate_and_read_pipes(
    mut child: ManagedChild,
    max_output_bytes: usize,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let cleanup = terminate_child_process_tree_until(&mut child, deadline).err();
    let output = read_pipes_until(child, max_output_bytes, deadline);
    match (cleanup, output) {
        (None, Ok(output)) => Ok(output),
        (Some(cleanup), Ok(_)) => Err(format!(
            "failed to terminate command process tree: {cleanup}"
        )),
        (None, Err(error)) => Err(error),
        (Some(cleanup), Err(error)) => Err(format!(
            "failed to terminate command process tree: {cleanup}; failed to collect output: {error}"
        )),
    }
}

fn read_pipes_until(
    mut child: ManagedChild,
    max_output_bytes: usize,
    deadline: Instant,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| "stdout pipe missing".to_string())?;
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| "stderr pipe missing".to_string())?;
    let (stdout_tx, stdout_rx) = mpsc::sync_channel(1);
    let stdout_handle = std::thread::spawn(move || {
        let _ = stdout_tx.send(read_bounded_pipe_tail(stdout, max_output_bytes, "stdout"));
    });
    let (stderr_tx, stderr_rx) = mpsc::sync_channel(1);
    let stderr_handle = std::thread::spawn(move || {
        let _ = stderr_tx.send(read_bounded_pipe_tail(stderr, max_output_bytes, "stderr"));
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("command child wait timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) => return Err(format!("failed to wait command: {error}")),
        }
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let stdout = stdout_rx
        .recv_timeout(remaining)
        .map_err(|_| "stdout reader did not finish before cleanup deadline".to_string())??;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let stderr = stderr_rx
        .recv_timeout(remaining)
        .map_err(|_| "stderr reader did not finish before cleanup deadline".to_string())??;
    if stdout_handle.is_finished() {
        let _ = stdout_handle.join();
    }
    if stderr_handle.is_finished() {
        let _ = stderr_handle.join();
    }
    Ok((status, stdout, stderr))
}

fn read_bounded_pipe_tail(
    mut pipe: impl Read,
    max_bytes: usize,
    stream_name: &'static str,
) -> Result<Vec<u8>, String> {
    let retained_limit = max_bytes.saturating_add(1);
    let mut output = Vec::with_capacity(retained_limit.min(64 * 1024));
    let mut chunk = [0_u8; 8192];
    loop {
        let read = pipe
            .read(&mut chunk)
            .map_err(|error| format!("failed to read {stream_name}: {error}"))?;
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

// Test-only wrapper for callers that do not need prepared shell profiles; the
// production request path uses `run_shell_with_profiles` directly.
#[cfg(test)]
pub(crate) fn run_shell(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_impl(
        policy,
        shell,
        None,
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
        None,
    )
    .result
}

#[cfg(test)]
pub(crate) fn run_shell_with_profiles(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_with_profiles_in_sandbox(
        generation,
        policy,
        shell,
        projects_dir,
        cache,
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_shell_with_profiles_in_sandbox(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> CommandResult {
    run_shell_with_profiles_in_sandbox_and_execution_state(
        generation,
        policy,
        shell,
        projects_dir,
        cache,
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
pub(crate) fn run_shell_with_profiles_in_sandbox_and_execution_state(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> ShellCommandResult {
    run_shell_impl(
        policy,
        shell,
        Some((generation, projects_dir, cache)),
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
        sandbox,
    )
}

fn run_shell_impl(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    profiles: Option<(u64, &Path, &PreparedShellProfileCache)>,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> ShellCommandResult {
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("raw shell is disabled by local agent policy".to_string()),
        });
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(e) = cwd_allowed(policy, &cwd_path) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(e),
        });
    }
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let start = Instant::now();
    let inspect_scratch = match sandbox {
        None => None,
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE) => {
            match crate::command_sandbox::InspectScratch::create() {
                Ok(scratch) => Some(scratch),
                Err(error) => {
                    return ShellCommandResult::not_started(CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(format!("inspect sandbox unavailable: {error}")),
                    })
                }
            }
        }
        Some(other) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("unknown sandbox mode '{other}'")),
            })
        }
    };
    let mut prepared_profile_name = None;
    // Preparing a profile executes its init script. In inspect mode that
    // preparation must not happen outside Landlock, so use the base configured
    // shell and run its optional init script as part of the sandboxed command.
    let mut cmd = match profiles.filter(|_| inspect_scratch.is_none()) {
        Some((generation, projects_dir, cache)) => {
            match resolve_prepared_shell_profile(
                generation,
                shell,
                projects_dir,
                &cwd_path,
                cwd.is_some(),
                cache,
                stop_requested,
            ) {
                Ok(Some(profile)) => match configured_prepared_shell_command(&profile, command) {
                    Ok(cmd) => {
                        prepared_profile_name = Some(profile.profile_name.clone());
                        cmd
                    }
                    Err(e) => {
                        return ShellCommandResult::not_started(CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to configure shell profile '{}': {}",
                                profile.profile_name, e
                            )),
                        });
                    }
                },
                Ok(None) => match configured_shell_command(shell, command) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        return ShellCommandResult::not_started(CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(e),
                        });
                    }
                },
                Err(e) => {
                    return ShellCommandResult::not_started(CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(e),
                    });
                }
            }
        }
        None => match configured_shell_command(shell, command) {
            Ok(cmd) => cmd,
            Err(e) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(e),
                });
            }
        },
    };
    cmd.current_dir(&cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    if let Some(scratch) = inspect_scratch.as_ref() {
        if let Err(error) = crate::command_sandbox::sandbox_command_inspect(&mut cmd, scratch) {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("inspect sandbox unavailable: {error}")),
            });
        }
    }
    // ManagedChild owns the whole shell process tree: a private process group
    // on Unix, a kill-on-close Job Object on Windows. `child_mut()` below only
    // accesses pipe handles; every termination still goes through the managed
    // tree API.
    let spawn = ManagedChild::spawn(&mut cmd);
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            let error = prepared_profile_name
                .as_deref()
                .map(|profile_name| {
                    format!("failed to spawn shell profile '{}': {}", profile_name, e)
                })
                .unwrap_or_else(|| format!("failed to spawn command: {}", e));
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
            });
        }
    };
    if let Some(input) = stdin {
        match child.child_mut().stdin.take() {
            Some(mut child_stdin) => {
                if let Err(e) = child_stdin.write_all(input.as_bytes()) {
                    // A command may reject a request or report a missing
                    // capability before consuming its payload. Once it closes
                    // stdin, BrokenPipe says nothing about the command's own
                    // result, so preserve its exit status and output. Other
                    // write failures still belong to the executor.
                    if e.kind() != std::io::ErrorKind::BrokenPipe {
                        let cleanup = terminate_child_without_output(child).err();
                        return ShellCommandResult::outcome_unknown(CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(with_cleanup_error(
                                format!("failed to write command stdin: {}", e),
                                cleanup,
                            )),
                        });
                    }
                }
            }
            None => {
                let cleanup = terminate_child_without_output(child).err();
                return ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(with_cleanup_error("stdin pipe missing", cleanup)),
                });
            }
        }
    }
    loop {
        if stop_requested
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            let duration_ms = start.elapsed().as_millis() as u64;
            return match terminate_and_read_pipes(child, policy.max_output_bytes) {
                Ok((_status, stdout, stderr)) => ShellCommandResult::completed(CommandResult {
                    exit_code: Some(-1),
                    stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
                    stderr: Some(format!(
                        "{}{}job stopped by request",
                        truncate_bytes(&stderr, policy.max_output_bytes),
                        if stderr.is_empty() { "" } else { "\n" },
                    )),
                    duration_ms: Some(duration_ms),
                    error: Some("job stopped".to_string()),
                }),
                Err(e) => ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: Some(-1),
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(duration_ms),
                    error: Some(format!("job stopped; failed to collect output: {}", e)),
                }),
            };
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return match terminate_and_read_pipes(child, policy.max_output_bytes) {
                        Ok((_status, stdout, stderr)) => {
                            ShellCommandResult::timed_out(CommandResult {
                                exit_code: Some(-1),
                                stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
                                stderr: Some(format!(
                                    "{}{}command timed out after {} seconds",
                                    truncate_bytes(&stderr, policy.max_output_bytes),
                                    if stderr.is_empty() { "" } else { "\n" },
                                    timeout_secs
                                )),
                                duration_ms: Some(duration_ms),
                                error: Some("command timed out".to_string()),
                            })
                        }
                        Err(e) => ShellCommandResult::outcome_unknown(CommandResult {
                            exit_code: Some(-1),
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(duration_ms),
                            error: Some(format!(
                                "command timed out; failed to collect output: {}",
                                e
                            )),
                        }),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let cleanup = terminate_child_without_output(child).err();
                return ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(with_cleanup_error(
                        format!("failed to wait command: {}", e),
                        cleanup,
                    )),
                });
            }
        }
    }
    match terminate_and_read_pipes(child, policy.max_output_bytes) {
        Ok((status, stdout, stderr)) => ShellCommandResult::completed(CommandResult {
            exit_code: Some(status.code().unwrap_or(-1)),
            stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
            stderr: Some(truncate_bytes(&stderr, policy.max_output_bytes)),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        }),
        Err(e) => spawned_output_failure(start, e),
    }
}

fn spawned_output_failure(start: Instant, error: String) -> ShellCommandResult {
    ShellCommandResult::outcome_unknown(CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: Some(error),
    })
}

#[cfg(test)]
mod runner_lifecycle_tests {
    use super::*;
    use crate::shell_protocol::ShellCommandExecutionState;

    fn unrestricted_policy() -> AgentPolicy {
        AgentPolicy {
            allow_cwd_anywhere: true,
            ..AgentPolicy::default()
        }
    }

    #[test]
    fn pre_spawn_rejection_is_not_started() {
        let mut policy = unrestricted_policy();
        policy.allow_raw_shell = false;
        let result = run_shell_impl(
            &policy,
            &ShellConfig::default(),
            None,
            None,
            "exit 0",
            None,
            10,
            None,
            None,
        );

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::NotStarted
        );
        assert!(result.result.exit_code.is_none());
    }

    #[test]
    fn terminal_process_result_is_completed() {
        let result = run_shell_impl(
            &unrestricted_policy(),
            &ShellConfig::default(),
            None,
            None,
            "exit 7",
            None,
            10,
            None,
            None,
        );

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn known_process_timeout_is_timed_out() {
        let result = run_shell_impl(
            &unrestricted_policy(),
            &ShellConfig::default(),
            None,
            None,
            "sleep 2",
            None,
            1,
            None,
            None,
        );

        assert_eq!(result.execution_state, ShellCommandExecutionState::TimedOut);
        assert_eq!(result.result.exit_code, Some(-1));
    }

    #[test]
    fn post_spawn_missing_output_pipe_is_outcome_unknown() {
        let mut command = configured_shell_command(&ShellConfig::default(), "exit 0").unwrap();
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = ManagedChild::spawn(&mut command).unwrap();
        drop(child.child_mut().stdout.take());

        let error = terminate_and_read_pipes(child, 1024).unwrap_err();
        assert!(error.contains("stdout pipe missing"), "{error}");
        let result = spawned_output_failure(Instant::now(), error);
        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::OutcomeUnknown
        );
    }
}
