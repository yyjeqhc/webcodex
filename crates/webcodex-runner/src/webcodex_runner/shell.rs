use super::config::{
    dialect_for_program, platform_default_dialect, validate_shell_config, AgentPolicy, ShellConfig,
    ShellDialect, ShellProfileConfig,
};
use super::output::{CommandResult, ShellCommandResult};
use super::output_text::{
    append_bounded_text, normalize_captured_output_text, normalize_output_text,
    CapturedOutputEncoding, FullStreamUtf8Validity, LeadingBom, OutputTextSource,
};
use super::projects::find_project_shell_context;
use crate::shell_protocol::{ShellScriptLanguage, ShellScriptPayload};
use std::collections::HashMap;
#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
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
const RAW_TAIL_CAPTURE_ALLOWANCE: usize = 4;

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
    if profile.is_none() {
        validate_shell_config(shell)?;
    }
    configured_process_command(shell, profile, program, args, None)
}

fn configured_process_command(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<Command, String> {
    let resolved_program = resolve_process_program(shell, profile, program, cwd)?;
    let mut cmd = Command::new(resolved_program);
    cmd.args(args);
    // ManagedChild (or JobManager for structured validation) owns this process
    // tree. The executable and every argument remain separate OS values.
    match profile {
        Some(profile) => apply_env_snapshot(&mut cmd, &profile.env_snapshot),
        None => apply_shell_environment(&mut cmd, shell)?,
    }
    Ok(cmd)
}

fn resolve_process_program(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    cwd: Option<&Path>,
) -> Result<OsString, String> {
    #[cfg(windows)]
    {
        let path = configured_process_path(shell, profile)?;
        let program_path = Path::new(program);
        let resolved_input = if !program_path.is_absolute() && program_path.components().count() > 1
        {
            cwd.map(|cwd| cwd.join(program_path))
                .unwrap_or_else(|| program_path.to_path_buf())
                .to_string_lossy()
                .into_owned()
        } else {
            program.to_string()
        };
        return match super::util::resolve_program_in_path(&resolved_input, &path) {
            Some(super::util::ResolvedProgram::Native(path)) => Ok(path.into_os_string()),
            Some(super::util::ResolvedProgram::Batch(_)) => Err(
                "unsupported_executable_type: Windows .cmd/.bat files require shell/script semantics and cannot preserve run_process native argv; use run_shell as the current explicit escape hatch"
                    .to_string(),
            ),
            None => Err(format!(
                "structured process executable is unavailable or has an unsupported Windows extension: {program}"
            )),
        };
    }
    #[cfg(not(windows))]
    {
        let _ = (shell, profile, cwd);
        Ok(OsString::from(program))
    }
}

fn configured_process_path(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
) -> Result<OsString, String> {
    if let Some(profile) = profile {
        return Ok(env_lookup(&profile.env_snapshot, "PATH")
            .map(OsString::from)
            .unwrap_or_default());
    }
    if let Some(configured) = env_lookup(&shell.env, "PATH") {
        return Ok(OsString::from(configured));
    }
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    if shell.path_prepend.is_empty() {
        return Ok(inherited);
    }
    let mut paths = shell.path_prepend.clone();
    paths.extend(std::env::split_paths(&inherited));
    std::env::join_paths(paths)
        .map_err(|error| format!("failed to build process PATH from shell.path_prepend: {error}"))
}

fn configured_script_interpreter(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    language: ShellScriptLanguage,
) -> Result<OsString, String> {
    let configured_program = profile
        .map(|profile| profile.program.as_str())
        .unwrap_or(shell.program.as_str());
    let configured_basename = Path::new(configured_program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(configured_program)
        .to_ascii_lowercase();
    let configured_matches = match language {
        ShellScriptLanguage::Sh => matches!(configured_basename.as_str(), "sh" | "sh.exe"),
        ShellScriptLanguage::Bash => {
            matches!(configured_basename.as_str(), "bash" | "bash.exe")
        }
        ShellScriptLanguage::Powershell if cfg!(windows) => matches!(
            configured_basename.as_str(),
            "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
        ),
        ShellScriptLanguage::Powershell => {
            matches!(configured_basename.as_str(), "pwsh" | "pwsh.exe")
        }
    };
    let mut candidates = Vec::new();
    if configured_matches {
        candidates.push(configured_program.to_string());
    }
    match language {
        ShellScriptLanguage::Sh => candidates.push("sh".to_string()),
        ShellScriptLanguage::Bash => candidates.push("bash".to_string()),
        ShellScriptLanguage::Powershell if cfg!(windows) => {
            candidates.push("pwsh".to_string());
            candidates.push("powershell".to_string());
        }
        ShellScriptLanguage::Powershell => candidates.push("pwsh".to_string()),
    }
    candidates.dedup_by(|left, right| {
        if cfg!(windows) {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    });
    let path = configured_process_path(shell, profile)?;
    for candidate in candidates {
        if let Some(super::util::ResolvedProgram::Native(path)) =
            super::util::resolve_program_in_path(&candidate, &path)
        {
            return Ok(path.into_os_string());
        }
    }
    Err(format!(
        "interpreter_unavailable: {} interpreter is unavailable; command was not started",
        language.as_str()
    ))
}

fn build_script_command(
    interpreter: impl Into<OsString>,
    language: ShellScriptLanguage,
    script_path: &Path,
    args: &[String],
) -> Command {
    let mut command = Command::new(interpreter.into());
    match language {
        ShellScriptLanguage::Sh | ShellScriptLanguage::Bash => {
            command.arg(script_path);
        }
        ShellScriptLanguage::Powershell => {
            command.arg("-NoProfile").arg("-NonInteractive");
            if cfg!(windows) {
                // Match the Runner's existing Windows PowerShell policy: a
                // process-scoped bypass keeps Runner-owned temporary .ps1 files
                // executable under the stock Restricted machine policy.
                command.arg("-ExecutionPolicy").arg("Bypass");
            }
            command.arg("-File").arg(script_path);
        }
    }
    command.args(args);
    command
}

fn script_setup_error(action: &str, error: &std::io::Error) -> String {
    format!(
        "script_setup_failed: failed to {action} Runner-owned temporary script file ({:?}); command was not started",
        error.kind()
    )
}

fn create_temporary_script(
    payload: &ShellScriptPayload,
    inspect_scratch: Option<&crate::command_sandbox::InspectScratch>,
) -> Result<(tempfile::TempPath, PathBuf, PathBuf), String> {
    let mut builder = tempfile::Builder::new();
    builder
        .prefix("webcodex-script-")
        .suffix(payload.language.file_extension());
    let mut file = match inspect_scratch {
        Some(scratch) => builder
            .tempfile_in(scratch.path())
            .map_err(|error| script_setup_error("create", &error))?,
        None => builder
            .tempfile()
            .map_err(|error| script_setup_error("create", &error))?,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| script_setup_error("secure", &error))?;
    }
    if payload.language == ShellScriptLanguage::Powershell {
        // Windows PowerShell 5.1 needs a UTF-8 BOM to preserve arbitrary
        // Unicode in script files. pwsh also accepts it. The BOM is encoding
        // metadata, so a leading param(...) block remains the first script
        // construct and no behavioral preamble is injected.
        file.write_all(&[0xEF, 0xBB, 0xBF])
            .map_err(|error| script_setup_error("write", &error))?;
    }
    file.write_all(payload.script.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| script_setup_error("write", &error))?;
    let original_path = file.path().to_path_buf();
    // Avoid `canonicalize` here: on Windows it commonly adds a `\\?\` prefix
    // that Windows PowerShell 5.1 does not reliably accept for `-File`.
    let absolute_path = if file.path().is_absolute() {
        file.path().to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(file.path()))
            .map_err(|error| script_setup_error("resolve", &error))?
    };
    Ok((file.into_temp_path(), original_path, absolute_path))
}

fn redact_temporary_script_path(result: &mut ShellCommandResult, paths: &[&Path]) {
    for value in [
        result.result.stdout.as_mut(),
        result.result.stderr.as_mut(),
        result.result.error.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        for path in paths {
            let rendered = path.to_string_lossy();
            if !rendered.is_empty() {
                *value = value.replace(rendered.as_ref(), "<temporary-script>");
                let alternate = if rendered.contains('\\') {
                    rendered.replace('\\', "/")
                } else {
                    rendered.replace('/', "\\")
                };
                if alternate != rendered {
                    *value = value.replace(&alternate, "<temporary-script>");
                }
            }
        }
    }
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
    const MAX_ERR: usize = 4096;
    normalize_output_text(bytes, false, MAX_ERR, OutputTextSource::LocalProcess)
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
    let mut child = ManagedChild::spawn(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))
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

#[derive(Debug)]
struct BoundedPipeTail {
    bytes: Vec<u8>,
    raw_truncated: bool,
    encoding: CapturedOutputEncoding,
}

impl BoundedPipeTail {
    fn normalize(&self, max_output_bytes: usize) -> String {
        normalize_captured_output_text(
            &self.bytes,
            self.raw_truncated,
            max_output_bytes,
            OutputTextSource::LocalProcess,
            self.encoding,
        )
    }

    #[cfg(test)]
    fn normalize_as_windows_for_test(&self, max_output_bytes: usize) -> String {
        super::output_text::normalize_captured_output_text_as_windows_for_test(
            &self.bytes,
            self.raw_truncated,
            max_output_bytes,
            self.encoding,
        )
    }
}

#[derive(Debug)]
struct IncrementalUtf8Validator {
    valid_so_far: bool,
    pending: Vec<u8>,
}

fn incomplete_utf8_sequence_len(first: u8) -> Option<usize> {
    match first {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

impl IncrementalUtf8Validator {
    fn new() -> Self {
        Self {
            valid_so_far: true,
            pending: Vec::with_capacity(3),
        }
    }

    fn push(&mut self, mut bytes: &[u8]) {
        if !self.valid_so_far {
            return;
        }
        if !self.pending.is_empty() {
            let sequence_len = incomplete_utf8_sequence_len(self.pending[0])
                .expect("pending UTF-8 starts with a valid multibyte lead byte");
            let needed = sequence_len - self.pending.len();
            let take = needed.min(bytes.len());
            if take < needed {
                self.pending.extend_from_slice(&bytes[..take]);
                if std::str::from_utf8(&self.pending)
                    .is_err_and(|error| error.error_len().is_some())
                {
                    self.pending.clear();
                    self.valid_so_far = false;
                    return;
                }
                debug_assert!(self.pending.len() <= 3);
                return;
            }
            let mut sequence = [0_u8; 4];
            sequence[..self.pending.len()].copy_from_slice(&self.pending);
            sequence[self.pending.len()..sequence_len].copy_from_slice(&bytes[..take]);
            self.pending.clear();
            if std::str::from_utf8(&sequence[..sequence_len]).is_err() {
                self.valid_so_far = false;
                return;
            }
            bytes = &bytes[take..];
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => {}
            Err(error) if error.error_len().is_some() => {
                self.valid_so_far = false;
            }
            Err(error) => {
                self.pending
                    .extend_from_slice(&bytes[error.valid_up_to()..]);
                debug_assert!(self.pending.len() <= 3);
            }
        }
    }

    fn finish(&self) -> FullStreamUtf8Validity {
        if self.valid_so_far && self.pending.is_empty() {
            FullStreamUtf8Validity::Valid
        } else {
            FullStreamUtf8Validity::Invalid
        }
    }
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
) -> Result<(std::process::ExitStatus, BoundedPipeTail, BoundedPipeTail), String> {
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
) -> Result<(std::process::ExitStatus, BoundedPipeTail, BoundedPipeTail), String> {
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
) -> Result<BoundedPipeTail, String> {
    // Four extra raw bytes retain truncation evidence plus enough alignment
    // room for the largest UTF-8 scalar. The decoded result is independently
    // bounded after transcoding.
    let retained_limit = max_bytes
        .saturating_add(RAW_TAIL_CAPTURE_ALLOWANCE)
        .max(RAW_TAIL_CAPTURE_ALLOWANCE);
    let mut output = Vec::with_capacity(retained_limit.min(64 * 1024));
    let mut prefix = Vec::with_capacity(3);
    let mut utf8_validator = IncrementalUtf8Validator::new();
    let mut total_bytes = 0usize;
    let mut raw_truncated = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let read = pipe
            .read(&mut chunk)
            .map_err(|error| format!("failed to read {stream_name}: {error}"))?;
        if read == 0 {
            let encoding = CapturedOutputEncoding {
                full_stream_utf8: utf8_validator.finish(),
                leading_bom: leading_bom(&prefix),
            };
            if raw_truncated {
                align_and_restore_bom(encoding, total_bytes, &mut output);
            }
            return Ok(BoundedPipeTail {
                bytes: output,
                raw_truncated,
                encoding,
            });
        }
        if prefix.len() < 3 {
            let prefix_bytes = (3 - prefix.len()).min(read);
            prefix.extend_from_slice(&chunk[..prefix_bytes]);
        }
        utf8_validator.push(&chunk[..read]);
        total_bytes = total_bytes.saturating_add(read);
        output.extend_from_slice(&chunk[..read]);
        if output.len() > retained_limit {
            let discard = output.len() - retained_limit;
            output.drain(..discard);
            raw_truncated = true;
        }
    }
}

fn leading_bom(prefix: &[u8]) -> LeadingBom {
    if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
        LeadingBom::Utf8
    } else if prefix.starts_with(&[0xFF, 0xFE]) {
        LeadingBom::Utf16Le
    } else if prefix.starts_with(&[0xFE, 0xFF]) {
        LeadingBom::Utf16Be
    } else {
        LeadingBom::None
    }
}

fn align_and_restore_bom(encoding: CapturedOutputEncoding, total_bytes: usize, tail: &mut Vec<u8>) {
    match encoding.leading_bom {
        LeadingBom::Utf8 => restore_utf8_bom(tail),
        LeadingBom::Utf16Le => restore_utf16_bom(tail, total_bytes, true),
        LeadingBom::Utf16Be => restore_utf16_bom(tail, total_bytes, false),
        LeadingBom::None if encoding.full_stream_utf8 == FullStreamUtf8Validity::Valid => {
            align_valid_utf8_tail(tail);
        }
        LeadingBom::None => {}
    }
}

fn align_valid_utf8_tail(tail: &mut Vec<u8>) {
    let discard = tail
        .iter()
        .take(3)
        .take_while(|byte| **byte & 0b1100_0000 == 0b1000_0000)
        .count();
    tail.drain(..discard);
}

fn restore_utf8_bom(tail: &mut Vec<u8>) {
    let replace = 3.min(tail.len());
    tail.drain(..replace);
    align_valid_utf8_tail(tail);
    let mut with_bom = Vec::with_capacity(3 + tail.len());
    with_bom.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    with_bom.extend_from_slice(tail);
    *tail = with_bom;
}

fn restore_utf16_bom(tail: &mut Vec<u8>, total_bytes: usize, little_endian: bool) {
    let mut tail_start = total_bytes.saturating_sub(tail.len());
    let alignment_discard = if tail_start < 2 {
        2 - tail_start
    } else {
        (tail_start - 2) % 2
    }
    .min(tail.len());
    if alignment_discard > 0 {
        tail.drain(..alignment_discard);
        tail_start = tail_start.saturating_add(alignment_discard);
    }
    debug_assert!(tail_start >= 2 || tail.is_empty());
    let replace = 2.min(tail.len());
    tail.drain(..replace);
    if tail.len() >= 2 {
        let first = if little_endian {
            u16::from_le_bytes([tail[0], tail[1]])
        } else {
            u16::from_be_bytes([tail[0], tail[1]])
        };
        if (0xDC00..=0xDFFF).contains(&first) {
            tail.drain(..2);
        }
    }
    let bom = if little_endian {
        [0xFF, 0xFE]
    } else {
        [0xFE, 0xFF]
    };
    let mut with_bom = Vec::with_capacity(2 + tail.len());
    with_bom.extend_from_slice(&bom);
    with_bom.extend_from_slice(tail);
    *tail = with_bom;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_process_with_profiles_in_sandbox_and_execution_state(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> ShellCommandResult {
    run_process_with_profiles_in_sandbox_and_execution_state_with_start_hook(
        generation,
        policy,
        shell,
        projects_dir,
        cache,
        cwd,
        executable,
        args,
        stdin,
        timeout_secs,
        stop_requested,
        sandbox,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_process_with_profiles_in_sandbox_and_execution_state_with_start_hook(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    // Structured execution intentionally receives the same policy treatment
    // as run_shell. Absence of shell syntax is not a permission bypass.
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("structured process execution is disabled by local agent policy".into()),
        });
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(error) = cwd_allowed(policy, &cwd_path) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(error),
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

    // Preparing a configured profile can execute its Runner-owned init
    // script. Inspect requests must not do that outside the sandbox, so they
    // use only the configured base environment. In either case the requested
    // executable and argv never pass through a shell parser.
    let profile = if inspect_scratch.is_none() {
        match resolve_prepared_shell_profile(
            generation,
            shell,
            projects_dir,
            &cwd_path,
            cwd.is_some(),
            cache,
            stop_requested,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(error),
                })
            }
        }
    } else {
        None
    };
    let cmd = match configured_process_command(
        shell,
        profile.as_deref(),
        executable,
        args,
        Some(&cwd_path),
    ) {
        Ok(cmd) => cmd,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
            })
        }
    };
    execute_configured_command(
        policy,
        cmd,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        inspect_scratch.as_ref(),
        start,
        "failed to spawn structured process",
        on_started,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script_with_profiles_in_sandbox_and_execution_state(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
) -> ShellCommandResult {
    run_script_with_profiles_in_sandbox_and_execution_state_with_start_hook(
        generation,
        policy,
        shell,
        projects_dir,
        cache,
        cwd,
        payload,
        stdin,
        timeout_secs,
        stop_requested,
        sandbox,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script_with_profiles_in_sandbox_and_execution_state_with_start_hook(
    generation: u64,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    sandbox: Option<&str>,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    // Typed script execution is consequential and receives the same Runner
    // policy treatment as raw shell and structured native processes.
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("structured script execution is disabled by local agent policy".into()),
        });
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(error) = cwd_allowed(policy, &cwd_path) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(error),
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

    // Inspect mode cannot execute profile preparation outside Landlock. It
    // uses the base configured environment and places the script itself in
    // the same sandbox-visible private scratch that remains writable.
    let profile = if inspect_scratch.is_none() {
        match resolve_prepared_shell_profile(
            generation,
            shell,
            projects_dir,
            &cwd_path,
            cwd.is_some(),
            cache,
            stop_requested,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(error),
                })
            }
        }
    } else {
        None
    };
    // Resolve the semantic interpreter before creating the payload file. A
    // missing interpreter is therefore a definite pre-start rejection with
    // no script side effect and no fallback to the configured shell parser.
    let interpreter =
        match configured_script_interpreter(shell, profile.as_deref(), payload.language) {
            Ok(interpreter) => interpreter,
            Err(error) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(error),
                })
            }
        };
    let (temporary_path, original_path, absolute_path) =
        match create_temporary_script(payload, inspect_scratch.as_ref()) {
            Ok(temporary) => temporary,
            Err(error) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(error),
                })
            }
        };
    let mut command =
        build_script_command(interpreter, payload.language, &absolute_path, &payload.args);
    match profile.as_deref() {
        Some(profile) => apply_env_snapshot(&mut command, &profile.env_snapshot),
        None => {
            if let Err(error) = apply_shell_environment(&mut command, shell) {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(error),
                });
            }
        }
    }
    let mut result = execute_configured_command(
        policy,
        command,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        inspect_scratch.as_ref(),
        start,
        "failed to spawn script interpreter",
        on_started,
    );
    redact_temporary_script_path(&mut result, &[original_path.as_path(), &absolute_path]);
    if let Err(error) = temporary_path.close() {
        // Cleanup is infrastructure after the child result is already known.
        // Never rewrite completed/timed_out/outcome_unknown lifecycle truth.
        tracing::warn!(
            language = payload.language.as_str(),
            error_kind = ?error.kind(),
            "failed to remove Runner-owned temporary script file"
        );
    }
    result
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

#[cfg(test)]
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
    let cmd = match profiles.filter(|_| inspect_scratch.is_none()) {
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
    let spawn_error_prefix = prepared_profile_name
        .as_deref()
        .map(|profile_name| format!("failed to spawn shell profile '{profile_name}'"))
        .unwrap_or_else(|| "failed to spawn command".to_string());
    execute_configured_command(
        policy,
        cmd,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        inspect_scratch.as_ref(),
        start,
        &spawn_error_prefix,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_configured_command(
    policy: &AgentPolicy,
    mut cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    inspect_scratch: Option<&crate::command_sandbox::InspectScratch>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    cmd.current_dir(cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    if let Some(scratch) = inspect_scratch {
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
    // ManagedChild owns the whole process tree: a private process group on
    // Unix, a kill-on-close Job Object on Windows. `child_mut()` below only
    // accesses pipe handles; every termination still uses the managed tree.
    let spawn = ManagedChild::spawn(&mut cmd);
    let mut child = match spawn {
        Ok(child) => child,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("{spawn_error_prefix}: {error}")),
            });
        }
    };
    if let Some(on_started) = on_started {
        on_started();
    }
    let mut stdin_writer = match stdin {
        Some(input) => match child.child_mut().stdin.take() {
            Some(mut child_stdin) => {
                let input = input.as_bytes().to_vec();
                let (tx, rx) = mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let _ = tx.send(child_stdin.write_all(&input));
                });
                Some(rx)
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
        },
        None => None,
    };
    loop {
        if let Err(error) = poll_stdin_writer(&mut stdin_writer) {
            let cleanup = terminate_child_without_output(child).err();
            return ShellCommandResult::outcome_unknown(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(with_cleanup_error(
                    format!("failed to write command stdin: {error}"),
                    cleanup,
                )),
            });
        }
        if stop_requested
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            let duration_ms = start.elapsed().as_millis() as u64;
            return match terminate_and_read_pipes(child, policy.max_output_bytes) {
                Ok((_status, stdout, stderr)) => {
                    let mut stderr = stderr.normalize(policy.max_output_bytes);
                    append_bounded_text(
                        &mut stderr,
                        "job stopped by request",
                        policy.max_output_bytes,
                    );
                    ShellCommandResult::completed(CommandResult {
                        exit_code: Some(-1),
                        stdout: Some(stdout.normalize(policy.max_output_bytes)),
                        stderr: Some(stderr),
                        duration_ms: Some(duration_ms),
                        error: Some("job stopped".to_string()),
                    })
                }
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
                            let mut stderr = stderr.normalize(policy.max_output_bytes);
                            append_bounded_text(
                                &mut stderr,
                                &format!("command timed out after {} seconds", timeout_secs),
                                policy.max_output_bytes,
                            );
                            ShellCommandResult::timed_out(CommandResult {
                                exit_code: Some(-1),
                                stdout: Some(stdout.normalize(policy.max_output_bytes)),
                                stderr: Some(stderr),
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
    if let Err(error) = finish_stdin_writer(stdin_writer) {
        let cleanup = terminate_child_without_output(child).err();
        return ShellCommandResult::outcome_unknown(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(with_cleanup_error(
                format!("failed to write command stdin: {error}"),
                cleanup,
            )),
        });
    }
    match terminate_and_read_pipes(child, policy.max_output_bytes) {
        Ok((status, stdout, stderr)) => ShellCommandResult::completed(CommandResult {
            exit_code: Some(status.code().unwrap_or(-1)),
            stdout: Some(stdout.normalize(policy.max_output_bytes)),
            stderr: Some(stderr.normalize(policy.max_output_bytes)),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        }),
        Err(e) => spawned_output_failure(start, e),
    }
}

fn poll_stdin_writer(
    receiver: &mut Option<mpsc::Receiver<std::io::Result<()>>>,
) -> Result<(), String> {
    let Some(active) = receiver.as_ref() else {
        return Ok(());
    };
    match active.try_recv() {
        Ok(Ok(())) => {
            *receiver = None;
            Ok(())
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            // The child may deliberately close stdin before exiting. Its
            // terminal status and output remain the source of truth.
            *receiver = None;
            Ok(())
        }
        Ok(Err(error)) => {
            *receiver = None;
            Err(error.to_string())
        }
        Err(mpsc::TryRecvError::Empty) => Ok(()),
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            Err("stdin writer ended without a result".to_string())
        }
    }
}

fn finish_stdin_writer(
    receiver: Option<mpsc::Receiver<std::io::Result<()>>>,
) -> Result<(), String> {
    let Some(receiver) = receiver else {
        return Ok(());
    };
    match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err("stdin writer did not finish after process exit".to_string())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("stdin writer ended without a result".to_string())
        }
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
    use std::sync::{Arc, OnceLock};

    struct ProcessArgvHelper {
        _temp: tempfile::TempDir,
        path: PathBuf,
    }

    static PROCESS_ARGV_HELPER: OnceLock<Arc<ProcessArgvHelper>> = OnceLock::new();

    fn process_argv_helper() -> PathBuf {
        PROCESS_ARGV_HELPER
            .get_or_init(|| {
                let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/process_argv_helper.rs");
                let temp = tempfile::tempdir().unwrap();
                let output = temp.path().join(format!(
                    "process-argv-helper{}",
                    std::env::consts::EXE_SUFFIX
                ));
                let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
                let result = Command::new(rustc)
                    .arg("--edition=2021")
                    .arg("--crate-name=webcodex_process_argv_helper")
                    .arg(source)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .expect("run rustc for process argv helper");
                assert!(
                    result.status.success(),
                    "process argv helper compilation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                Arc::new(ProcessArgvHelper {
                    _temp: temp,
                    path: output,
                })
            })
            .path
            .clone()
    }

    fn run_direct_process(
        cwd: &Path,
        executable: &Path,
        args: &[String],
        stdin: Option<&str>,
        timeout_secs: u64,
    ) -> ShellCommandResult {
        let projects_dir = tempfile::tempdir().unwrap();
        run_process_with_profiles_in_sandbox_and_execution_state(
            1,
            &unrestricted_policy(),
            &ShellConfig::default(),
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.to_string_lossy().as_ref()),
            &executable.to_string_lossy(),
            args,
            stdin,
            timeout_secs,
            None,
            None,
        )
    }

    fn run_direct_script(
        cwd: &Path,
        language: ShellScriptLanguage,
        script: String,
        args: Vec<String>,
        stdin: Option<&str>,
        timeout_secs: u64,
        sandbox: Option<&str>,
    ) -> ShellCommandResult {
        let projects_dir = tempfile::tempdir().unwrap();
        run_script_with_profiles_in_sandbox_and_execution_state(
            1,
            &unrestricted_policy(),
            &ShellConfig::default(),
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.to_string_lossy().as_ref()),
            &ShellScriptPayload {
                language,
                script,
                args,
            },
            stdin,
            timeout_secs,
            None,
            sandbox,
        )
    }

    fn unrestricted_policy() -> AgentPolicy {
        AgentPolicy {
            allow_cwd_anywhere: true,
            ..AgentPolicy::default()
        }
    }

    #[test]
    fn phase_f_powershell_utf8_preamble_precedes_requested_shell_command() {
        let requested = "Write-Output '中文 🙂'";
        let command = powershell_command_text(requested);
        assert!(command.starts_with(POWERSHELL_UTF8_PREAMBLE));
        let preamble_end = command.find('\n').expect("preamble line ending");
        let requested_start = command.find(requested).expect("requested command");
        assert!(preamble_end < requested_start);
        assert!(command.contains("$LASTEXITCODE = 0"));
        assert!(command.ends_with("exit 0"));

        let init = Path::new(r"C:\runner profile\init.ps1");
        let initialized = powershell_init_command_text(init, requested);
        assert!(initialized.starts_with(POWERSHELL_UTF8_PREAMBLE));
        assert!(
            initialized
                .find(". 'C:\\runner profile\\init.ps1'")
                .unwrap()
                < initialized.find(requested).unwrap()
        );

        let prepared = powershell_profile_prepare_script("Write-Output init", "MARKER");
        assert!(prepared.starts_with(POWERSHELL_UTF8_PREAMBLE));
    }

    #[test]
    fn phase_f_bounded_raw_tail_aligns_complete_utf8_before_windows_decode() {
        assert_eq!(RAW_TAIL_CAPTURE_ALLOWANCE, 4);
        let text = "中🙂".repeat(64);
        let bytes = text.as_bytes();
        let max = 64;
        let raw_tail_offset = bytes.len() - (max + RAW_TAIL_CAPTURE_ALLOWANCE);
        assert_eq!(raw_tail_offset, 380);
        assert!(
            !text.is_char_boundary(raw_tail_offset),
            "test tail must begin inside a UTF-8 scalar"
        );
        let aligned_offset = (raw_tail_offset..bytes.len())
            .find(|offset| text.is_char_boundary(*offset))
            .unwrap();
        assert_eq!(aligned_offset, 381);

        let captured = read_bounded_pipe_tail(std::io::Cursor::new(bytes), max, "stdout").unwrap();
        assert!(captured.raw_truncated);
        assert_eq!(
            captured.encoding.full_stream_utf8,
            FullStreamUtf8Validity::Valid
        );
        assert_eq!(captured.encoding.leading_bom, LeadingBom::None);
        assert_eq!(captured.bytes, &bytes[aligned_offset..]);
        assert!(captured.bytes.len() <= max + RAW_TAIL_CAPTURE_ALLOWANCE);
        assert!(
            !crate::webcodex_runner::output_text::captured_windows_output_uses_oem_for_test(
                &captured.bytes,
                captured.encoding,
            )
        );

        let normalized = captured.normalize_as_windows_for_test(max);
        assert!(normalized.len() <= max);
        assert!(std::str::from_utf8(normalized.as_bytes()).is_ok());
        assert!(normalized.starts_with("[output truncated]\n"));
        assert!(!normalized.contains('\u{fffd}'));
        let retained = normalized.strip_prefix("[output truncated]\n").unwrap();
        assert!(!retained.is_empty());
        assert!(text.ends_with(retained), "{retained:?}");
        assert!(retained.contains('中'));
        assert!(retained.contains('🙂'));
    }

    #[test]
    fn phase_f_bounded_raw_tail_restores_utf8_bom_after_scalar_alignment() {
        let text = "中🙂".repeat(64);
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(text.as_bytes());
        let max = 64;
        let raw_tail_offset = bytes.len() - (max + RAW_TAIL_CAPTURE_ALLOWANCE);
        let content_offset = raw_tail_offset - 3;
        assert_eq!(raw_tail_offset, 383);
        assert_eq!(content_offset, 380);
        assert!(
            !text.is_char_boundary(content_offset),
            "test tail must begin inside a UTF-8 scalar"
        );
        let capacity_offset = content_offset + 3;
        assert!(!text.is_char_boundary(capacity_offset));
        let aligned_content_offset = (capacity_offset..text.len())
            .find(|offset| text.is_char_boundary(*offset))
            .unwrap();
        assert_eq!(aligned_content_offset, 385);

        let captured = read_bounded_pipe_tail(std::io::Cursor::new(&bytes), max, "stdout").unwrap();
        assert!(captured.raw_truncated);
        assert_eq!(
            captured.encoding,
            CapturedOutputEncoding {
                full_stream_utf8: FullStreamUtf8Validity::Valid,
                leading_bom: LeadingBom::Utf8,
            }
        );
        let mut expected = vec![0xEF, 0xBB, 0xBF];
        expected.extend_from_slice(&bytes[3 + aligned_content_offset..]);
        assert_eq!(captured.bytes, expected);
        assert!(captured.bytes.len() <= max + RAW_TAIL_CAPTURE_ALLOWANCE);

        let normalized = captured.normalize_as_windows_for_test(max);
        assert!(normalized.len() <= max);
        assert!(normalized.starts_with("[output truncated]\n"));
        assert!(!normalized.contains('\u{feff}'));
        assert!(!normalized.contains('\u{fffd}'));
        let retained = normalized.strip_prefix("[output truncated]\n").unwrap();
        assert!(!retained.is_empty());
        assert!(text.ends_with(retained), "{retained:?}");
        assert!(retained.contains('中'));
        assert!(retained.contains('🙂'));
    }

    #[test]
    fn phase_f_invalid_full_stream_keeps_oem_classification_when_suffix_is_utf8() {
        let max = 32;
        let mut bytes = vec![0xFF];
        bytes.extend(std::iter::repeat_n(b'x', 100));
        let captured = read_bounded_pipe_tail(std::io::Cursor::new(bytes), max, "stderr").unwrap();
        assert!(captured.raw_truncated);
        assert_eq!(
            captured.encoding,
            CapturedOutputEncoding {
                full_stream_utf8: FullStreamUtf8Validity::Invalid,
                leading_bom: LeadingBom::None,
            }
        );
        assert!(std::str::from_utf8(&captured.bytes).is_ok());
        assert!(
            crate::webcodex_runner::output_text::captured_windows_output_uses_oem_for_test(
                &captured.bytes,
                captured.encoding,
            )
        );
        assert!(captured.bytes.len() <= max + RAW_TAIL_CAPTURE_ALLOWANCE);
        assert!(captured.normalize_as_windows_for_test(max).len() <= max);
    }

    #[test]
    fn phase_f_utf8_validator_handles_split_scalars_and_latches_invalidity() {
        let scalar = "🙂".as_bytes();
        let mut validator = IncrementalUtf8Validator::new();
        validator.push(&scalar[..1]);
        assert!(validator.valid_so_far);
        assert_eq!(validator.pending.len(), 1);
        validator.push(&scalar[1..3]);
        assert!(validator.valid_so_far);
        assert_eq!(validator.pending.len(), 3);
        validator.push(&scalar[3..]);
        assert!(validator.valid_so_far);
        assert!(validator.pending.is_empty());
        assert_eq!(validator.finish(), FullStreamUtf8Validity::Valid);

        let mut across_capture_reads = vec![b'x'; 8191];
        across_capture_reads.extend_from_slice(scalar);
        let captured = read_bounded_pipe_tail(
            std::io::Cursor::new(&across_capture_reads),
            across_capture_reads.len(),
            "stdout",
        )
        .unwrap();
        assert!(!captured.raw_truncated);
        assert_eq!(
            captured.encoding.full_stream_utf8,
            FullStreamUtf8Validity::Valid
        );

        let mut invalid = IncrementalUtf8Validator::new();
        invalid.push(&[0xE2]);
        assert!(invalid.valid_so_far);
        assert_eq!(invalid.pending.len(), 1);
        invalid.push(b"A");
        assert!(!invalid.valid_so_far);
        assert!(invalid.pending.is_empty());
        invalid.push("valid later 🙂".as_bytes());
        assert!(!invalid.valid_so_far);
        assert!(invalid.pending.len() <= 3);
        assert_eq!(invalid.finish(), FullStreamUtf8Validity::Invalid);

        let mut incomplete_at_eof = IncrementalUtf8Validator::new();
        incomplete_at_eof.push(&scalar[..3]);
        assert_eq!(incomplete_at_eof.pending.len(), 3);
        assert_eq!(incomplete_at_eof.finish(), FullStreamUtf8Validity::Invalid);
    }

    #[test]
    fn phase_f_bounded_raw_tail_preserves_utf16_bom_and_unit_alignment() {
        let max = 32;

        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "中".repeat(1000).encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let captured = read_bounded_pipe_tail(std::io::Cursor::new(utf16), max, "stderr").unwrap();
        assert!(captured.raw_truncated);
        assert!(captured.bytes.len() <= max + RAW_TAIL_CAPTURE_ALLOWANCE);
        assert!(captured.bytes.starts_with(&[0xFF, 0xFE]));
        assert_eq!((captured.bytes.len() - 2) % 2, 0);

        let mut surrogate_utf16 = vec![0xFF, 0xFE];
        for unit in "🙂".repeat(1000).encode_utf16() {
            surrogate_utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let captured =
            read_bounded_pipe_tail(std::io::Cursor::new(surrogate_utf16), max, "stderr").unwrap();
        let first_retained_unit = u16::from_le_bytes([captured.bytes[2], captured.bytes[3]]);
        assert!((0xD800..=0xDBFF).contains(&first_retained_unit));
        let normalized = captured.normalize_as_windows_for_test(max);
        assert!(!normalized.contains('\u{fffd}'));
        assert!(normalized.ends_with("🙂🙂🙂"));
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

    #[test]
    fn structured_process_preserves_literal_argv_and_empty_boundaries() {
        let cwd = tempfile::tempdir().unwrap();
        let helper = process_argv_helper();
        let values = vec![
            String::new(),
            "two words".to_string(),
            "\"double\" and 'single'".to_string(),
            "; semicolon".to_string(),
            "$(not a command)".to_string(),
            "a&b|c".to_string(),
            r"C:\path with spaces\trailing\\".to_string(),
            "雪だるま☃".to_string(),
        ];
        let mut args = vec!["argv".to_string()];
        args.extend(values.clone());

        let result = run_direct_process(cwd.path(), &helper, &args, None, 10);

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0));
        let expected = values
            .iter()
            .map(|value| format!("{}:{value}\n", value.len()))
            .collect::<String>();
        assert_eq!(result.result.stdout.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn structured_process_does_not_interpret_shell_injection_arguments() {
        let cwd = tempfile::tempdir().unwrap();
        let helper = process_argv_helper();
        let marker = cwd.path().join("marker");
        let values = vec!["$(touch marker)".to_string(), "; touch marker".to_string()];
        let mut args = vec!["argv".to_string()];
        args.extend(values.clone());

        let result = run_direct_process(cwd.path(), &helper, &args, None, 10);

        assert_eq!(result.result.exit_code, Some(0));
        assert!(!marker.exists());
        let stdout = result.result.stdout.unwrap();
        for value in values {
            assert!(stdout.contains(&format!("{}:{value}", value.len())));
        }
    }

    #[test]
    fn structured_process_supports_empty_args_and_bounded_stdin() {
        let cwd = tempfile::tempdir().unwrap();
        let helper = process_argv_helper();
        let empty = run_direct_process(cwd.path(), &helper, &[], None, 10);
        assert_eq!(empty.result.exit_code, Some(0));
        assert_eq!(empty.result.stdout.as_deref(), Some(""));

        let stdin = "line one\nUnicode 雪\n";
        let with_stdin =
            run_direct_process(cwd.path(), &helper, &["stdin".to_string()], Some(stdin), 10);
        assert_eq!(with_stdin.result.exit_code, Some(0));
        assert_eq!(with_stdin.result.stdout.as_deref(), Some(stdin));
    }

    #[test]
    fn structured_process_can_exceed_legacy_shell_command_limit() {
        let cwd = tempfile::tempdir().unwrap();
        let helper = process_argv_helper();
        let args = vec!["argv".to_string(), "a".repeat(4_500), "b".repeat(4_500)];
        assert!(args.iter().map(String::len).sum::<usize>() > 8_000);

        let result = run_direct_process(cwd.path(), &helper, &args, None, 10);

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0));
        let stdout = result.result.stdout.unwrap();
        assert!(stdout.starts_with("4500:"));
        assert!(stdout.contains("\n4500:"));
    }

    #[test]
    fn structured_process_reports_prestart_completion_and_nonzero_truthfully() {
        let cwd = tempfile::tempdir().unwrap();
        let missing = cwd.path().join("definitely-missing-process");
        let not_started = run_direct_process(cwd.path(), &missing, &[], None, 10);
        assert_eq!(
            not_started.execution_state,
            ShellCommandExecutionState::NotStarted
        );
        assert_eq!(not_started.result.exit_code, None);

        let helper = process_argv_helper();
        let completed = run_direct_process(
            cwd.path(),
            &helper,
            &["exit".to_string(), "0".to_string()],
            None,
            10,
        );
        assert_eq!(
            completed.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(completed.result.exit_code, Some(0));

        let nonzero = run_direct_process(
            cwd.path(),
            &helper,
            &["exit".to_string(), "23".to_string()],
            None,
            10,
        );
        assert_eq!(
            nonzero.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(nonzero.result.exit_code, Some(23));
    }

    #[test]
    fn structured_process_known_timeout_is_timed_out() {
        let cwd = tempfile::tempdir().unwrap();
        let result = run_direct_process(
            cwd.path(),
            &process_argv_helper(),
            &["sleep".to_string(), "2000".to_string()],
            None,
            1,
        );

        assert_eq!(result.execution_state, ShellCommandExecutionState::TimedOut);
        assert_eq!(result.result.exit_code, Some(-1));
    }

    #[cfg(unix)]
    #[test]
    fn structured_sh_script_exceeds_legacy_limit_uses_temp_file_and_cleans_it() {
        let cwd = tempfile::tempdir().unwrap();
        let observed_path = cwd.path().join("observed-script-path");
        let mut script = "# payload padding\n".repeat(2_400);
        script.push_str(&format!(
            "printf '%s' \"$0\" > '{}'\nprintf 'path=%s\\nhello\\n' \"$0\"\n",
            observed_path.display()
        ));
        assert!(script.len() > 32 * 1024);

        let result = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            script,
            Vec::new(),
            None,
            10,
            None,
        );

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0));
        let temporary_path =
            PathBuf::from(std::fs::read_to_string(&observed_path).expect("script path evidence"));
        assert_eq!(
            temporary_path.extension().and_then(|value| value.to_str()),
            Some("sh")
        );
        assert!(!temporary_path.starts_with(cwd.path()));
        assert!(
            !temporary_path.exists(),
            "temporary script must be removed after terminal execution"
        );
        let stdout = result.result.stdout.unwrap();
        assert!(stdout.contains("path=<temporary-script>"), "{stdout}");
        assert!(stdout.ends_with("hello\n"), "{stdout}");
        assert!(
            !stdout.contains(&temporary_path.to_string_lossy().to_string()),
            "absolute temporary path must be redacted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn structured_bash_script_preserves_content_and_literal_argument_boundaries() {
        let cwd = tempfile::tempdir().unwrap();
        let marker = cwd.path().join("marker");
        let observed_path = cwd.path().join("bash-script-path");
        let args = vec![
            String::new(),
            "two words".to_string(),
            "$(touch marker)".to_string(),
            "; touch marker".to_string(),
            r"C:\path with spaces\trailing\\".to_string(),
            "雪だるま☃".to_string(),
        ];
        let script = format!(
            r#"printf '%s' "$0" > '{}'
literal=$(cat <<'WEBCODEX_LITERAL'
quotes: "'" $()
semicolons: ;;; pipes: |||
backslashes: C:\one\two\\
Unicode: 雪だるま☃
WEBCODEX_LITERAL
)
printf '%s\n' "$literal"
for value in "$@"; do
  printf '%s:%s\n' "${{#value}}" "$value"
done
"#,
            observed_path.display()
        );

        let result = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Bash,
            script,
            args.clone(),
            None,
            10,
            None,
        );

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0));
        assert!(!marker.exists(), "shell-looking args must remain data");
        let stdout = result.result.stdout.unwrap();
        assert!(stdout.contains(r#"quotes: "'" $()"#), "{stdout}");
        assert!(stdout.contains("semicolons: ;;; pipes: |||"), "{stdout}");
        assert!(stdout.contains(r"backslashes: C:\one\two\\"), "{stdout}");
        assert!(stdout.contains("Unicode: 雪だるま☃"), "{stdout}");
        for value in &args {
            assert!(
                stdout.contains(&format!("{}:{value}", value.chars().count())),
                "missing literal arg {value:?} in {stdout:?}"
            );
        }
        let temporary_path = PathBuf::from(std::fs::read_to_string(observed_path).unwrap());
        assert_eq!(
            temporary_path.extension().and_then(|value| value.to_str()),
            Some("sh")
        );
        assert!(!temporary_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn structured_script_stdin_nonzero_and_timeout_preserve_lifecycle() {
        let cwd = tempfile::tempdir().unwrap();
        let input = "line one\nUnicode 雪\n";
        let stdin_result = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            "cat".to_string(),
            Vec::new(),
            Some(input),
            10,
            None,
        );
        assert_eq!(stdin_result.result.exit_code, Some(0));
        assert_eq!(stdin_result.result.stdout.as_deref(), Some(input));

        let language_semantics = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            "false\nprintf 'continued\\n'".to_string(),
            Vec::new(),
            None,
            10,
            None,
        );
        assert_eq!(language_semantics.result.exit_code, Some(0));
        assert_eq!(
            language_semantics.result.stdout.as_deref(),
            Some("continued\n"),
            "Runner must not inject set -e"
        );

        let nonzero = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            "exit 23".to_string(),
            Vec::new(),
            None,
            10,
            None,
        );
        assert_eq!(
            nonzero.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(nonzero.result.exit_code, Some(23));

        let timed_out = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            "sleep 2".to_string(),
            Vec::new(),
            None,
            1,
            None,
        );
        assert_eq!(
            timed_out.execution_state,
            ShellCommandExecutionState::TimedOut
        );
        assert_eq!(timed_out.result.exit_code, Some(-1));
    }

    #[test]
    fn missing_script_interpreter_is_prestart_and_does_not_run_script() {
        let cwd = tempfile::tempdir().unwrap();
        let marker = cwd.path().join("marker");
        let projects_dir = tempfile::tempdir().unwrap();
        let mut shell = ShellConfig::default();
        shell.program = "custom-shell".to_string();
        shell.env.insert("PATH".to_string(), String::new());
        let result = run_script_with_profiles_in_sandbox_and_execution_state(
            1,
            &unrestricted_policy(),
            &shell,
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.path().to_string_lossy().as_ref()),
            &ShellScriptPayload {
                language: ShellScriptLanguage::Bash,
                script: "touch marker".to_string(),
                args: Vec::new(),
            },
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
        assert!(result
            .result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("interpreter_unavailable"));
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn arbitrary_configured_shell_is_not_treated_as_a_script_language() {
        use std::os::unix::fs::PermissionsExt;

        let cwd = tempfile::tempdir().unwrap();
        let marker = cwd.path().join("custom-shell-ran");
        let custom_shell = cwd.path().join("custom-shell");
        std::fs::write(
            &custom_shell,
            format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&custom_shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        let projects_dir = tempfile::tempdir().unwrap();
        let mut shell = ShellConfig::default();
        shell.program = custom_shell.to_string_lossy().into_owned();
        shell.env.insert("PATH".to_string(), String::new());
        let result = run_script_with_profiles_in_sandbox_and_execution_state(
            1,
            &unrestricted_policy(),
            &shell,
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.path().to_string_lossy().as_ref()),
            &ShellScriptPayload {
                language: ShellScriptLanguage::Bash,
                script: "true".to_string(),
                args: Vec::new(),
            },
            None,
            10,
            None,
            None,
        );

        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::NotStarted
        );
        assert!(result
            .result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("interpreter_unavailable"));
        assert!(!marker.exists());
    }

    #[test]
    fn sh_and_bash_plans_pass_a_script_file_without_command_text_mode() {
        use std::ffi::OsStr;

        let script_path = Path::new("/runner/scratch/payload.sh");
        let script_args = vec!["two words".to_string(), "$(literal)".to_string()];
        for (language, interpreter) in [
            (ShellScriptLanguage::Sh, "sh"),
            (ShellScriptLanguage::Bash, "bash"),
        ] {
            let command = build_script_command(interpreter, language, script_path, &script_args);
            assert_eq!(command.get_program(), OsStr::new(interpreter));
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                [
                    script_path.as_os_str(),
                    OsStr::new("two words"),
                    OsStr::new("$(literal)")
                ]
            );
            assert!(!command
                .get_args()
                .any(|argument| argument == OsStr::new("-c")));
        }
    }

    #[test]
    fn powershell_plan_uses_ps1_file_and_never_command_text_mode() {
        use std::ffi::OsStr;

        let script_path = Path::new("/runner/scratch/payload.ps1");
        let args = vec![
            String::new(),
            "two words".to_string(),
            "$(literal)".to_string(),
            "; literal".to_string(),
        ];
        let command =
            build_script_command("pwsh", ShellScriptLanguage::Powershell, script_path, &args);
        assert_eq!(command.get_program(), OsStr::new("pwsh"));
        let actual = command.get_args().collect::<Vec<_>>();
        let mut expected = vec![OsStr::new("-NoProfile"), OsStr::new("-NonInteractive")];
        if cfg!(windows) {
            expected.extend([OsStr::new("-ExecutionPolicy"), OsStr::new("Bypass")]);
        }
        expected.extend([
            OsStr::new("-File"),
            script_path.as_os_str(),
            OsStr::new(""),
            OsStr::new("two words"),
            OsStr::new("$(literal)"),
            OsStr::new("; literal"),
        ]);
        assert_eq!(actual, expected);
        assert!(!actual.iter().any(|arg| {
            let arg = arg.to_string_lossy();
            arg.eq_ignore_ascii_case("-Command") || arg.eq_ignore_ascii_case("-c")
        }));
    }

    #[test]
    fn phase_f_powershell_temp_file_uses_utf8_bom_without_script_preamble() {
        let payload = ShellScriptPayload {
            language: ShellScriptLanguage::Powershell,
            script: "param([string]$Value)\nWrite-Output $Value".to_string(),
            args: Vec::new(),
        };
        let (temporary_path, _original, absolute) =
            create_temporary_script(&payload, None).unwrap();
        let bytes = std::fs::read(&absolute).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], payload.script.as_bytes());
        temporary_path.close().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspect_script_uses_sandbox_scratch_and_cannot_write_project() {
        if crate::command_sandbox::inspect_sandbox_available().is_err() {
            return;
        }
        let cwd = tempfile::tempdir().unwrap();
        let marker = cwd.path().join("project-marker");
        let script = format!(
            "if printf denied > '{}'; then exit 91; fi\nprintf scratch > \"$TMPDIR/proof\"\ntest \"$(cat \"$TMPDIR/proof\")\" = scratch\nprintf '%s\\n' \"$0\"\n",
            marker.display()
        );
        let result = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Sh,
            script,
            Vec::new(),
            None,
            10,
            Some(crate::command_sandbox::INSPECT_SANDBOX_MODE),
        );
        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0), "{:?}", result.result);
        assert!(!marker.exists());
        assert_eq!(
            result.result.stdout.as_deref(),
            Some("<temporary-script>\n")
        );
    }

    #[test]
    fn powershell_runtime_executes_from_file_when_available() {
        let cwd = tempfile::tempdir().unwrap();
        let projects_dir = tempfile::tempdir().unwrap();
        if configured_script_interpreter(
            &ShellConfig::default(),
            None,
            ShellScriptLanguage::Powershell,
        )
        .is_err()
        {
            return;
        }
        let result = run_script_with_profiles_in_sandbox_and_execution_state(
            1,
            &unrestricted_policy(),
            &ShellConfig::default(),
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.path().to_string_lossy().as_ref()),
            &ShellScriptPayload {
                language: ShellScriptLanguage::Powershell,
                script: "param([string]$Value)\nWrite-Output $Value".to_string(),
                args: vec!["two words".to_string()],
            },
            None,
            10,
            None,
            None,
        );
        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(0), "{:?}", result.result);
        assert!(result
            .result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("two words"));
    }

    #[cfg(windows)]
    #[test]
    fn phase_f_windows_native_utf8_and_utf16_stdout_stderr_normalize() {
        let cwd = tempfile::tempdir().unwrap();
        let helper = process_argv_helper();

        let utf8 = run_direct_process(
            cwd.path(),
            &helper,
            &["windows-utf8-output".to_string()],
            None,
            10,
        );
        assert_eq!(utf8.execution_state, ShellCommandExecutionState::Completed);
        assert_eq!(utf8.result.exit_code, Some(17));
        assert_eq!(utf8.result.stdout.as_deref(), Some("UTF8 中文 🙂\n"));
        assert_eq!(utf8.result.stderr.as_deref(), Some("UTF8 中文 🙂\n"));

        let utf16 = run_direct_process(
            cwd.path(),
            &helper,
            &["windows-utf16-output".to_string()],
            None,
            10,
        );
        assert_eq!(utf16.execution_state, ShellCommandExecutionState::Completed);
        assert_eq!(utf16.result.exit_code, Some(0));
        assert_eq!(utf16.result.stdout.as_deref(), Some("UTF16 中文 🙂\n"));
        assert_eq!(utf16.result.stderr.as_deref(), Some("UTF16 中文 🙂\n"));
    }

    #[cfg(windows)]
    #[test]
    fn phase_f_windows_native_oem_stdout_stderr_use_active_oem_page() {
        let cwd = tempfile::tempdir().unwrap();
        let expected_path = cwd.path().join("expected.txt");
        let result = run_direct_process(
            cwd.path(),
            &process_argv_helper(),
            &[
                "windows-oem-output".to_string(),
                expected_path.to_string_lossy().into_owned(),
            ],
            None,
            10,
        );
        let expected = std::fs::read_to_string(expected_path).unwrap();
        assert!(!expected.is_ascii());
        assert_eq!(
            result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(result.result.exit_code, Some(23));
        assert_eq!(result.result.stdout.as_deref(), Some(expected.as_str()));
        assert_eq!(result.result.stderr.as_deref(), Some(expected.as_str()));
        assert!(!result
            .result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains('\u{fffd}'));

        let bounded_expected_path = cwd.path().join("bounded-expected.txt");
        let projects_dir = tempfile::tempdir().unwrap();
        let mut policy = unrestricted_policy();
        policy.max_output_bytes = 64;
        let bounded = run_process_with_profiles_in_sandbox_and_execution_state(
            1,
            &policy,
            &ShellConfig::default(),
            projects_dir.path(),
            &PreparedShellProfileCache::default(),
            Some(cwd.path().to_string_lossy().as_ref()),
            &process_argv_helper().to_string_lossy(),
            &[
                "windows-oem-output".to_string(),
                bounded_expected_path.to_string_lossy().into_owned(),
                "1000".to_string(),
            ],
            None,
            10,
            None,
            None,
        );
        for output in [bounded.result.stdout, bounded.result.stderr] {
            let output = output.unwrap();
            assert!(output.len() <= policy.max_output_bytes);
            assert!(output.starts_with("[output truncated]\n"), "{output:?}");
            assert!(std::str::from_utf8(output.as_bytes()).is_ok());
        }
    }

    #[cfg(windows)]
    #[test]
    fn phase_f_windows_powershell_shell_and_param_script_keep_semantics() {
        let cwd = tempfile::tempdir().unwrap();
        let shell = ShellConfig::default();
        let shell_result = run_shell_impl(
            &unrestricted_policy(),
            &shell,
            None,
            Some(cwd.path().to_string_lossy().as_ref()),
            "[Console]::Out.WriteLine('shell 中文 🙂'); [Console]::Error.WriteLine('error 中文 🙂'); exit 19",
            None,
            10,
            None,
            None,
        );
        assert_eq!(
            shell_result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(shell_result.result.exit_code, Some(19));
        assert_eq!(
            shell_result.result.stdout.as_deref(),
            Some("shell 中文 🙂\n")
        );
        assert_eq!(
            shell_result.result.stderr.as_deref(),
            Some("error 中文 🙂\n")
        );

        let script_result = run_direct_script(
            cwd.path(),
            ShellScriptLanguage::Powershell,
            "param([string]$Value)\n\
             $out = [Text.Encoding]::UTF8.GetBytes('script ' + $Value + \"`n\")\n\
             [Console]::OpenStandardOutput().Write($out, 0, $out.Length)\n\
             $err = [Text.Encoding]::UTF8.GetBytes('script-error ' + $Value + \"`n\")\n\
             [Console]::OpenStandardError().Write($err, 0, $err.Length)"
                .to_string(),
            vec!["中文 🙂".to_string()],
            None,
            10,
            None,
        );
        assert_eq!(
            script_result.execution_state,
            ShellCommandExecutionState::Completed
        );
        assert_eq!(script_result.result.exit_code, Some(0));
        assert_eq!(
            script_result.result.stdout.as_deref(),
            Some("script 中文 🙂\n")
        );
        assert_eq!(
            script_result.result.stderr.as_deref(),
            Some("script-error 中文 🙂\n")
        );
    }

    #[cfg(windows)]
    #[test]
    fn phase_f_windows_timeout_retains_unicode_and_runs_child_once() {
        let cwd = tempfile::tempdir().unwrap();
        let marker = cwd.path().join("execution-marker");
        let result = run_direct_process(
            cwd.path(),
            &process_argv_helper(),
            &[
                "windows-mark-output-sleep".to_string(),
                marker.to_string_lossy().into_owned(),
                "10000".to_string(),
            ],
            None,
            1,
        );
        assert_eq!(result.execution_state, ShellCommandExecutionState::TimedOut);
        assert_eq!(result.result.exit_code, Some(-1));
        assert!(result
            .result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("partial 中文 🙂\n"));
        assert!(result
            .result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("partial 中文 🙂\n"));
        assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_run_process_accepts_only_native_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let native = temp.path().join("native.exe");
        std::fs::write(&native, b"MZ").unwrap();
        let args = vec![
            "space value".to_string(),
            "\"quotes\"".to_string(),
            r"backslash\\".to_string(),
            "&|;".to_string(),
        ];

        let command = configured_process_command(
            &ShellConfig::default(),
            None,
            &native.to_string_lossy(),
            &args,
            Some(temp.path()),
        )
        .unwrap();
        assert_eq!(command.get_program(), native.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            args.iter()
                .map(|argument| OsStr::new(argument))
                .collect::<Vec<_>>()
        );

        let marker = temp.path().join("batch-started");
        let batch = temp.path().join("script.cmd");
        std::fs::write(
            &batch,
            format!("@echo off\r\ncopy nul \"{}\"\r\n", marker.display()),
        )
        .unwrap();
        let batch_result = run_direct_process(temp.path(), &batch, &args, None, 10);
        assert_eq!(
            batch_result.execution_state,
            ShellCommandExecutionState::NotStarted
        );
        let batch_error = batch_result.result.error.as_deref().unwrap_or_default();
        assert!(
            batch_error.contains("unsupported_executable_type"),
            "{batch_error}"
        );
        assert!(batch_error.contains("run_shell"), "{batch_error}");
        assert!(
            !marker.exists(),
            "run_process must reject Batch before child spawn"
        );

        let unsupported = temp.path().join("script.vbs");
        std::fs::write(&unsupported, b"WScript.Echo 1\r\n").unwrap();
        let unsupported_result = run_direct_process(temp.path(), &unsupported, &[], None, 10);
        assert_eq!(
            unsupported_result.execution_state,
            ShellCommandExecutionState::NotStarted
        );
        assert!(unsupported_result
            .result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("unsupported Windows extension"));
    }
}
