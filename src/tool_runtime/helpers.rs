use crate::shell_protocol::{
    ShellScriptLanguage, ShellScriptPayload, RAW_SHELL_COMMAND_MAX_BYTES, RAW_SHELL_WIRE_MAX_BYTES,
};
use serde_json::json;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(test)]
pub(crate) fn run_command_sync(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    run_command_sync_with_shell(cmd, cwd, timeout_secs, "sh")
}

#[cfg(test)]
pub(crate) fn run_command_sync_with_shell(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
    shell: &str,
) -> (i32, String, String, u64) {
    run_command_sync_with_shell_and_sandbox(cmd, cwd, timeout_secs, shell, None)
}

pub(crate) fn run_command_sync_with_shell_and_sandbox(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
    shell: &str,
    sandbox: Option<&str>,
) -> (i32, String, String, u64) {
    let start = Instant::now();
    let mut command = std::process::Command::new(shell);
    command
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // Put the command in its own process group so its whole subtree can be
    // reaped as a group. Argument 0 makes the child a group leader whose pgid
    // equals its pid. Without this, a backgrounded grandchild that inherits the
    // stdout/stderr pipes (e.g. `some-daemon &`) keeps the pipe write-end open,
    // and `wait_with_output()` below blocks on pipe EOF *forever* — the exact
    // intermittent "no reply" hang this guards against.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let _scratch = match sandbox {
        None => None,
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE) => {
            let scratch = match crate::command_sandbox::InspectScratch::create() {
                Ok(scratch) => scratch,
                Err(error) => {
                    return (
                        -1,
                        String::new(),
                        format!("Failed to configure inspect sandbox: {error}"),
                        start.elapsed().as_millis() as u64,
                    )
                }
            };
            if let Err(error) =
                crate::command_sandbox::sandbox_command_inspect(&mut command, &scratch)
            {
                return (
                    -1,
                    String::new(),
                    format!("Failed to configure inspect sandbox: {error}"),
                    start.elapsed().as_millis() as u64,
                );
            }
            Some(scratch)
        }
        Some(other) => {
            return (
                -1,
                String::new(),
                format!("Failed to configure inspect sandbox: unknown sandbox mode '{other}'"),
                start.elapsed().as_millis() as u64,
            )
        }
    };
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                -1,
                String::new(),
                format!("Failed to execute command: {}", e),
                start.elapsed().as_millis() as u64,
            );
        }
    };
    // Under `process_group(0)` the child's pid is also its process-group id.
    let pgid = child.id();
    let timeout = Duration::from_secs(timeout_secs);
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                // Still reap the group so a spawned subtree is not leaked.
                reap_process_group(pgid);
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for command: {}", e),
                    start.elapsed().as_millis() as u64,
                );
            }
        }
    }
    // Whether the command timed out or exited on its own, reap the entire
    // process group before draining output. This kills any backgrounded
    // grandchildren still holding the stdout/stderr pipes so `wait_with_output`
    // observes EOF promptly instead of blocking indefinitely. On a clean exit
    // with no stragglers the signal simply finds nothing to kill.
    reap_process_group(pgid);
    let output = child.wait_with_output();
    let elapsed = start.elapsed().as_millis() as u64;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if timed_out {
                if !stderr.is_empty() && !stderr.ends_with('\n') {
                    stderr.push('\n');
                }
                stderr.push_str(&format!("Command timed out after {} seconds", timeout_secs));
                (-1, stdout, stderr, elapsed)
            } else {
                let code = out.status.code().unwrap_or(-1);
                (code, stdout, stderr, elapsed)
            }
        }
        Err(e) if timed_out => (
            -1,
            String::new(),
            format!(
                "Command timed out after {} seconds; failed to collect output: {}",
                timeout_secs, e
            ),
            elapsed,
        ),
        Err(e) => (
            -1,
            String::new(),
            format!("Failed to collect command output: {}", e),
            elapsed,
        ),
    }
}

/// Best-effort SIGKILL of an entire process group (`kill(-pgid, SIGKILL)`; a
/// negative target signals every process in the group). Reaps background
/// grandchildren a synchronous command may have left holding its stdout/stderr
/// pipes, which would otherwise block `wait_with_output()` on pipe EOF forever.
///
/// `pgid` is always our own child's pid (made a group leader via
/// `process_group(0)`), so this only ever targets that command's own subtree —
/// never an unrelated or caller-supplied pid. Uses the direct syscall rather
/// than shelling out: `run_command_sync` is a hot path, and the `kill` binary
/// rejects negative pgid arguments on some coreutils builds. Failure (e.g. the
/// group already fully exited, ESRCH) is expected and ignored. No-op on
/// non-Unix targets.
#[cfg(unix)]
fn reap_process_group(pgid: u32) {
    // Guard against ever signalling pid 0 / -1 ("current group" / "all
    // processes") if a caller somehow passed a zero id.
    if pgid == 0 {
        return;
    }
    // SAFETY: plain syscall with no memory arguments; a stale/invalid pgid
    // yields ESRCH which we deliberately ignore.
    unsafe {
        libc::kill(-(pgid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn reap_process_group(_pgid: u32) {}

/// Grace added on top of a local command's own `timeout_secs` to form the
/// outer bound in [`run_command_sync_bounded`]. Covers the post-exit group
/// reap and output drain, which are normally near-instant.
pub(crate) const LOCAL_RUN_HARD_GRACE_SECS: u64 = 10;

/// Failure surfaced by [`run_command_sync_bounded`]'s outer backstop rather
/// than by the command itself (a command's own non-zero exit / timeout is
/// reported through the `Ok` tuple).
pub(crate) enum LocalRunFailure {
    /// The blocking task did not come back within
    /// `timeout_secs + LOCAL_RUN_HARD_GRACE_SECS`. `run_command_sync` bounds
    /// the wait on the direct child and reaps its process group, but the
    /// output drain can still wedge if a descendant escapes the group (e.g.
    /// via `setsid`) while holding the stdout/stderr pipes. This converts
    /// that wedge into a prompt timeout error instead of an unbounded await;
    /// the detached blocking thread is abandoned until the straggler exits,
    /// since `spawn_blocking` cannot be cancelled.
    HardTimeout { bound_secs: u64 },
    /// The blocking task panicked or the runtime is shutting down.
    Join(String),
}

/// Run [`run_command_sync`] on the blocking pool, bounded by an outer hard
/// timeout so a wedged output drain can never park the caller — and with it
/// the MCP request driving it — indefinitely.
pub(crate) async fn run_command_sync_bounded(
    cmd: String,
    cwd: PathBuf,
    timeout_secs: u64,
) -> Result<(i32, String, String, u64), LocalRunFailure> {
    run_command_sync_bounded_with_shell(cmd, cwd, timeout_secs, "sh".to_string()).await
}

pub(crate) async fn run_command_sync_bounded_with_shell(
    cmd: String,
    cwd: PathBuf,
    timeout_secs: u64,
    shell: String,
) -> Result<(i32, String, String, u64), LocalRunFailure> {
    run_command_sync_bounded_with_shell_and_sandbox(cmd, cwd, timeout_secs, shell, None).await
}

pub(crate) async fn run_command_sync_bounded_with_shell_and_sandbox(
    cmd: String,
    cwd: PathBuf,
    timeout_secs: u64,
    shell: String,
    sandbox: Option<String>,
) -> Result<(i32, String, String, u64), LocalRunFailure> {
    let bound_secs = timeout_secs.saturating_add(LOCAL_RUN_HARD_GRACE_SECS);
    let task = tokio::task::spawn_blocking(move || {
        run_command_sync_with_shell_and_sandbox(
            &cmd,
            &cwd,
            timeout_secs,
            &shell,
            sandbox.as_deref(),
        )
    });
    match tokio::time::timeout(Duration::from_secs(bound_secs), task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(LocalRunFailure::Join(e.to_string())),
        Err(_) => Err(LocalRunFailure::HardTimeout { bound_secs }),
    }
}

/// Maximum retained bytes per stream for one local synchronous direct process.
/// Reader threads continuously drain the pipes, retaining only the tail, so a
/// noisy child cannot deadlock on a full pipe or turn `run_process` into an
/// unbounded output channel.
const LOCAL_PROCESS_OUTPUT_MAX_BYTES: usize = 256 * 1024;
type LocalProcessResult = (i32, String, String, u64);

pub(crate) async fn run_process_sync_bounded_with_sandbox(
    executable: String,
    args: Vec<String>,
    stdin: Option<String>,
    cwd: PathBuf,
    timeout_secs: u64,
    sandbox: Option<String>,
) -> Result<(i32, String, String, u64), LocalRunFailure> {
    let bound_secs = timeout_secs.saturating_add(LOCAL_RUN_HARD_GRACE_SECS);
    let task = tokio::task::spawn_blocking(move || {
        run_process_sync_with_sandbox(
            &executable,
            &args,
            stdin.as_deref(),
            &cwd,
            timeout_secs,
            sandbox.as_deref(),
        )
    });
    match tokio::time::timeout(Duration::from_secs(bound_secs), task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => Err(LocalRunFailure::Join(error.to_string())),
        Err(_) => Err(LocalRunFailure::HardTimeout { bound_secs }),
    }
}

pub(crate) async fn run_script_sync_bounded_with_sandbox(
    payload: ShellScriptPayload,
    stdin: Option<String>,
    cwd: PathBuf,
    timeout_secs: u64,
    sandbox: Option<String>,
) -> Result<LocalProcessResult, LocalRunFailure> {
    let bound_secs = timeout_secs.saturating_add(LOCAL_RUN_HARD_GRACE_SECS);
    let task = tokio::task::spawn_blocking(move || {
        run_script_sync_with_sandbox(
            &payload,
            stdin.as_deref(),
            &cwd,
            timeout_secs,
            sandbox.as_deref(),
        )
    });
    match tokio::time::timeout(Duration::from_secs(bound_secs), task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => Err(LocalRunFailure::Join(error.to_string())),
        Err(_) => Err(LocalRunFailure::HardTimeout { bound_secs }),
    }
}

fn run_process_sync_with_sandbox(
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: &Path,
    timeout_secs: u64,
    sandbox: Option<&str>,
) -> LocalProcessResult {
    let start = Instant::now();
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let _scratch = match create_local_inspect_scratch(sandbox, start) {
        Ok(scratch) => scratch,
        Err(result) => return result,
    };
    if let Err(result) = sandbox_local_command(&mut command, _scratch.as_ref(), start) {
        return result;
    }
    execute_local_process_command(command, stdin, timeout_secs, start)
}

fn execute_local_process_command(
    mut command: Command,
    stdin: Option<&str>,
    timeout_secs: u64,
    start: Instant,
) -> LocalProcessResult {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to execute process: {error}"),
                start.elapsed().as_millis() as u64,
            )
        }
    };
    let pgid = child.id();
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_local_direct_process(&mut child, pgid);
            return (
                -1,
                String::new(),
                "Failed to collect process output: stdout pipe missing".to_string(),
                start.elapsed().as_millis() as u64,
            );
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_local_direct_process(&mut child, pgid);
            return (
                -1,
                String::new(),
                "Failed to collect process output: stderr pipe missing".to_string(),
                start.elapsed().as_millis() as u64,
            );
        }
    };
    let stdout_reader = spawn_bounded_process_reader(stdout);
    let stderr_reader = spawn_bounded_process_reader(stderr);
    let stdin_writer = stdin.map(|input| {
        let input = input.as_bytes().to_vec();
        let child_stdin = child.stdin.take();
        std::thread::spawn(move || match child_stdin {
            Some(mut child_stdin) => child_stdin.write_all(&input),
            None => Err(std::io::Error::other("stdin pipe missing")),
        })
    });

    let timeout = Duration::from_secs(timeout_secs);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() >= timeout => {
                timed_out = true;
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                terminate_local_direct_process(&mut child, pgid);
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for process: {error}"),
                    start.elapsed().as_millis() as u64,
                );
            }
        }
    };
    let status = if timed_out {
        terminate_local_direct_process(&mut child, pgid);
        None
    } else {
        // The direct child is terminal, but descendants may still own output
        // pipes. Unix process-group ownership lets us close that whole tree.
        reap_process_group(pgid);
        status
    };
    let stdout = finish_bounded_process_reader(stdout_reader);
    let stderr = finish_bounded_process_reader(stderr_reader);
    let elapsed = start.elapsed().as_millis() as u64;
    let stdin_error = stdin_writer
        .and_then(|writer| writer.join().ok())
        .and_then(Result::err)
        .filter(|error| error.kind() != std::io::ErrorKind::BrokenPipe);
    match (stdout, stderr, stdin_error) {
        (_, _, Some(error)) => (
            -1,
            String::new(),
            format!("Failed to write process stdin: {error}"),
            elapsed,
        ),
        (Err(error), _, _) | (_, Err(error), _) => (
            -1,
            String::new(),
            format!("Failed to collect process output: {error}"),
            elapsed,
        ),
        (Ok(stdout), Ok(mut stderr), None) if timed_out => {
            if !stderr.is_empty() && !stderr.ends_with('\n') {
                stderr.push('\n');
            }
            stderr.push_str(&format!("Command timed out after {timeout_secs} seconds"));
            (-1, stdout, stderr, elapsed)
        }
        (Ok(stdout), Ok(stderr), None) => (
            status.and_then(|status| status.code()).unwrap_or(-1),
            stdout,
            stderr,
            elapsed,
        ),
    }
}

fn create_local_inspect_scratch(
    sandbox: Option<&str>,
    start: Instant,
) -> Result<Option<crate::command_sandbox::InspectScratch>, LocalProcessResult> {
    match sandbox {
        None => Ok(None),
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE) => {
            crate::command_sandbox::InspectScratch::create()
                .map(Some)
                .map_err(|error| {
                    (
                        -1,
                        String::new(),
                        format!("Failed to configure inspect sandbox: {error}"),
                        start.elapsed().as_millis() as u64,
                    )
                })
        }
        Some(other) => Err((
            -1,
            String::new(),
            format!("Failed to configure inspect sandbox: unknown sandbox mode '{other}'"),
            start.elapsed().as_millis() as u64,
        )),
    }
}

fn sandbox_local_command(
    command: &mut Command,
    scratch: Option<&crate::command_sandbox::InspectScratch>,
    start: Instant,
) -> Result<(), LocalProcessResult> {
    let Some(scratch) = scratch else {
        return Ok(());
    };
    crate::command_sandbox::sandbox_command_inspect(command, scratch).map_err(|error| {
        (
            -1,
            String::new(),
            format!("Failed to configure inspect sandbox: {error}"),
            start.elapsed().as_millis() as u64,
        )
    })
}

fn run_script_sync_with_sandbox(
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    cwd: &Path,
    timeout_secs: u64,
    sandbox: Option<&str>,
) -> LocalProcessResult {
    let start = Instant::now();
    let interpreter = match find_local_script_interpreter(payload.language) {
        Some(interpreter) => interpreter,
        None => {
            return (
                -1,
                String::new(),
                format!(
                "interpreter_unavailable: {} interpreter is unavailable; command was not started",
                payload.language.as_str()
            ),
                start.elapsed().as_millis() as u64,
            )
        }
    };
    let scratch = match create_local_inspect_scratch(sandbox, start) {
        Ok(scratch) => scratch,
        Err(result) => return result,
    };
    let mut builder = tempfile::Builder::new();
    builder
        .prefix("webcodex-script-")
        .suffix(payload.language.file_extension());
    let mut file = match match scratch.as_ref() {
        Some(scratch) => builder.tempfile_in(scratch.path()),
        None => builder.tempfile(),
    } {
        Ok(file) => file,
        Err(error) => return local_script_setup_failure("create", &error, start),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = file
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
        {
            return local_script_setup_failure("secure", &error, start);
        }
    }
    if payload.language == ShellScriptLanguage::Powershell {
        if let Err(error) = file.write_all(&[0xEF, 0xBB, 0xBF]) {
            return local_script_setup_failure("write", &error, start);
        }
    }
    if let Err(error) = file
        .write_all(payload.script.as_bytes())
        .and_then(|_| file.flush())
    {
        return local_script_setup_failure("write", &error, start);
    }
    let original_path = file.path().to_path_buf();
    // Windows PowerShell 5.1 can reject the extended `\\?\` path prefix that
    // `canonicalize` commonly returns. Tempfile paths are normally absolute;
    // make a relative platform temp setting absolute without canonicalizing.
    let absolute_path = if file.path().is_absolute() {
        file.path().to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(file.path()),
            Err(error) => return local_script_setup_failure("resolve", &error, start),
        }
    };
    let temporary_path = file.into_temp_path();
    let mut command =
        build_local_script_command(interpreter, payload.language, &absolute_path, &payload.args);
    command
        .current_dir(cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Err(result) = sandbox_local_command(&mut command, scratch.as_ref(), start) {
        return result;
    }
    let mut result = execute_local_process_command(command, stdin, timeout_secs, start);
    redact_local_script_paths(&mut result, &[original_path.as_path(), &absolute_path]);
    if let Err(error) = temporary_path.close() {
        tracing::warn!(
            language = payload.language.as_str(),
            error_kind = ?error.kind(),
            "failed to remove Server-owned compatibility temporary script file"
        );
    }
    result
}

fn local_script_setup_failure(
    action: &str,
    error: &std::io::Error,
    start: Instant,
) -> LocalProcessResult {
    (
        -1,
        String::new(),
        format!(
            "script_setup_failed: failed to {action} Server-owned temporary script file ({:?}); command was not started",
            error.kind()
        ),
        start.elapsed().as_millis() as u64,
    )
}

fn build_local_script_command(
    interpreter: PathBuf,
    language: ShellScriptLanguage,
    script_path: &Path,
    args: &[String],
) -> Command {
    let mut command = Command::new(interpreter);
    match language {
        ShellScriptLanguage::Sh | ShellScriptLanguage::Bash => {
            command.arg(script_path);
        }
        ShellScriptLanguage::Powershell => {
            command.arg("-NoProfile").arg("-NonInteractive");
            if cfg!(windows) {
                command.arg("-ExecutionPolicy").arg("Bypass");
            }
            command.arg("-File").arg(script_path);
        }
    }
    command.args(args);
    command
}

fn find_local_script_interpreter(language: ShellScriptLanguage) -> Option<PathBuf> {
    let candidates: &[&str] = match language {
        ShellScriptLanguage::Sh => {
            if cfg!(windows) {
                &["sh.exe"]
            } else {
                &["sh"]
            }
        }
        ShellScriptLanguage::Bash => {
            if cfg!(windows) {
                &["bash.exe"]
            } else {
                &["bash"]
            }
        }
        ShellScriptLanguage::Powershell => {
            if cfg!(windows) {
                &["pwsh.exe", "powershell.exe"]
            } else {
                &["pwsh"]
            }
        }
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    for directory in std::env::split_paths(&path) {
        for candidate in candidates {
            let path = directory.join(candidate);
            if local_executable_file(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn local_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn redact_local_script_paths(result: &mut LocalProcessResult, paths: &[&Path]) {
    for value in [&mut result.1, &mut result.2] {
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

fn terminate_local_direct_process(child: &mut Child, pgid: u32) {
    reap_process_group(pgid);
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn spawn_bounded_process_reader(
    mut pipe: impl Read + Send + 'static,
) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut retained = Vec::new();
        let mut chunk = [0_u8; 8192];
        let result = loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break Ok(String::from_utf8_lossy(&retained).to_string()),
                Ok(count) => {
                    retained.extend_from_slice(&chunk[..count]);
                    if retained.len() > LOCAL_PROCESS_OUTPUT_MAX_BYTES {
                        let discard = retained.len() - LOCAL_PROCESS_OUTPUT_MAX_BYTES;
                        retained.drain(..discard);
                    }
                }
                Err(error) => break Err(error.to_string()),
            }
        };
        let _ = tx.send(result);
    });
    rx
}

fn finish_bounded_process_reader(
    reader: mpsc::Receiver<Result<String, String>>,
) -> Result<String, String> {
    reader
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "process output reader did not finish".to_string())?
}

pub(crate) fn resolve_local_cwd(
    proj: &crate::projects::ProjectConfig,
    cwd: Option<&str>,
) -> Result<PathBuf, String> {
    let root = proj.root();
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Project root does not exist: {}", e))?;
    let requested = match cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) {
        Some(cwd) => {
            let path = PathBuf::from(cwd);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => root,
    };
    let canonical = requested
        .canonicalize()
        .map_err(|e| format!("cwd does not exist: {}", e))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("cwd is outside project directory".to_string());
    }
    Ok(canonical)
}

pub(crate) fn project_relative_cwd(
    proj: &crate::projects::ProjectConfig,
    resolved: &Path,
) -> Result<String, String> {
    let root = proj
        .root()
        .canonicalize()
        .map_err(|e| format!("Project root does not exist: {e}"))?;
    let relative = resolved
        .strip_prefix(&root)
        .map_err(|_| "cwd is outside project directory".to_string())?;
    if relative.as_os_str().is_empty() {
        Ok(".".to_string())
    } else {
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }
}

pub(crate) fn project_relative_agent_cwd(
    proj: &crate::projects::ProjectConfig,
    resolved: &str,
) -> Result<String, String> {
    let root = proj.root();
    let resolved = Path::new(resolved);
    let relative = resolved
        .strip_prefix(root)
        .map_err(|_| "cwd is outside project directory".to_string())?;
    if relative.as_os_str().is_empty() {
        Ok(".".to_string())
    } else {
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }
}

/// Resolve an Agent command cwd against the registered project root.
///
/// The server cannot canonicalize paths on a remote Agent host, so this
/// performs the project-relative/lexical boundary check before dispatch. The
/// Agent remains responsible for canonicalizing the existing path against its
/// configured `allowed_roots`, which rejects symlink escapes.
pub(crate) fn resolve_agent_cwd(
    proj: &crate::projects::ProjectConfig,
    cwd: Option<&str>,
) -> Result<String, String> {
    let root = proj.root();
    let requested = match cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) {
        Some(cwd) => {
            if cwd == "." {
                return Ok(root.to_string_lossy().to_string());
            }
            let path = PathBuf::from(cwd);
            if path.is_absolute() {
                if path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
                {
                    return Err("cwd cannot contain parent traversal".to_string());
                }
                path
            } else {
                validate_project_relative_path(cwd)?;
                root.join(path)
            }
        }
        None => root.clone(),
    };
    if requested != root && !requested.starts_with(&root) {
        return Err("cwd is outside project directory".to_string());
    }
    Ok(requested.to_string_lossy().to_string())
}

pub(crate) fn validate_project_relative_path(path: &str) -> Result<(), String> {
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Ok(());
    }
    let p = Path::new(path);
    if p.has_root()
        || p.components()
            .any(|component| matches!(component, std::path::Component::Prefix(_)))
    {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    Ok(())
}

pub(crate) fn validate_raw_shell_command_length(command: &str) -> Result<(), String> {
    if command.len() > RAW_SHELL_COMMAND_MAX_BYTES {
        return Err(format!(
            "raw shell command exceeds the {RAW_SHELL_COMMAND_MAX_BYTES}-byte UTF-8 limit; use run_script for larger program text or stdin/files/artifacts for large data"
        ));
    }
    Ok(())
}

pub(crate) fn explicit_shell_dispatch_command(
    command: &str,
    shell: &str,
) -> Result<String, String> {
    validate_raw_shell_command_length(command)?;
    let dispatched = format!("exec {shell} -c {}", shell_escape_simple(command));
    if dispatched.len() > RAW_SHELL_WIRE_MAX_BYTES {
        return Err(format!(
            "explicit shell wrapper exceeds the {RAW_SHELL_WIRE_MAX_BYTES}-byte Runner wire limit; use run_script for large or quote-dense shell program text"
        ));
    }
    Ok(dispatched)
}

#[cfg(test)]
mod raw_shell_bound_tests {
    use super::*;

    #[test]
    fn authored_raw_shell_bound_and_explicit_wrapper_headroom_are_consistent() {
        let exact = "x".repeat(RAW_SHELL_COMMAND_MAX_BYTES);
        validate_raw_shell_command_length(&exact).unwrap();
        assert!(validate_raw_shell_command_length(&(exact + "x")).is_err());

        let quote_dense = "'".repeat(RAW_SHELL_COMMAND_MAX_BYTES);
        let dispatched = explicit_shell_dispatch_command(&quote_dense, "bash").unwrap();
        assert!(dispatched.len() > RAW_SHELL_COMMAND_MAX_BYTES);
        assert!(dispatched.len() <= RAW_SHELL_WIRE_MAX_BYTES);
        assert!(dispatched.starts_with("exec bash -c "));
    }
}

pub(crate) fn shell_escape_simple(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

pub(crate) fn validate_limited_cleanup_paths(
    paths: &[String],
    deny_sensitive: bool,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("paths cannot be empty".to_string());
    }
    if paths.len() > 64 {
        return Err("paths may contain at most 64 entries".to_string());
    }
    let mut clean = Vec::new();
    for raw in paths {
        validate_project_relative_path(raw)?;
        let path = raw.trim().trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() || path == "." {
            return Err("path must name a file or tracked path, not the project root".to_string());
        }
        if deny_sensitive {
            let warnings = super::patch::sensitive_path_warnings(path);
            if !warnings.is_empty() {
                return Err(format!(
                    "refusing sensitive cleanup path '{}': {}",
                    path,
                    warnings.join("; ")
                ));
            }
        }
        if !clean.iter().any(|p: &String| p == path) {
            clean.push(path.to_string());
        }
    }
    Ok(clean)
}

pub(crate) fn shell_join_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| shell_escape_simple(p))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn bounded_tail(text: &str, max_chars: usize) -> (String, bool) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), false);
    }
    let tail: String = text.chars().skip(total - max_chars).collect();
    (tail, true)
}

pub(crate) const COMMAND_STDIO_TAIL_CHARS: usize = 12_000;

/// Synchronous agent-wait tools share this hard upper bound with
/// `shell_client` validation (`wait_timeout_secs` must be <= 120).
pub(crate) const MIN_SYNC_TIMEOUT_SECS: u64 = 1;
pub(crate) const MAX_SYNC_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_RUN_SHELL_TIMEOUT_SECS: u64 = 60;

/// Read-only structured validation tools (`cargo_check`, `cargo_test`,
/// `cargo_fmt(check=true)`) define `timeout_secs` as the total runtime budget
/// of the command, independent of how long the tool call itself blocks. The
/// command runs once; a long validation continues as a Job and returns a
/// `job_id` instead of being killed at the sync wait boundary.
pub(crate) const MIN_VALIDATION_TIMEOUT_SECS: u64 = 1;
pub(crate) const MAX_VALIDATION_TIMEOUT_SECS: u64 = 3600;
/// Default total runtime budget per structured validation tool when the caller
/// omits `timeout_secs`.
pub(crate) const DEFAULT_CARGO_CHECK_TIMEOUT_SECS: u64 = 600;
pub(crate) const DEFAULT_CARGO_TEST_TIMEOUT_SECS: u64 = 1800;
pub(crate) const DEFAULT_CARGO_FMT_TIMEOUT_SECS: u64 = 120;

/// Internal synchronous wait window for a structured validation. The tool call
/// blocks up to this long for the command to finish in-process; after that the
/// same execution is promoted to a queryable Job. Kept below the 120s MCP
/// hard ceiling so transport/result serialization keeps ~30s of headroom.
pub(crate) const SYNC_VALIDATION_WAIT_SECS: u64 = 90;

/// Resolve a synchronous command timeout. Out-of-range values are rejected
/// (not clamped) so callers cannot request longer waits than the sync path
/// can honor.
pub(crate) fn resolve_sync_timeout_secs(
    timeout_secs: Option<u64>,
    default: u64,
) -> Result<u64, String> {
    debug_assert!((MIN_SYNC_TIMEOUT_SECS..=MAX_SYNC_TIMEOUT_SECS).contains(&default));
    let value = timeout_secs.unwrap_or(default);
    if !(MIN_SYNC_TIMEOUT_SECS..=MAX_SYNC_TIMEOUT_SECS).contains(&value) {
        return Err(format!(
            "timeout_secs must be between {} and {}",
            MIN_SYNC_TIMEOUT_SECS, MAX_SYNC_TIMEOUT_SECS
        ));
    }
    Ok(value)
}

/// Structured pre-execution rejection for an out-of-range synchronous timeout.
/// Messages name the calling tool and never leak the underlying shell request
/// implementation (`runShell` / `run_shell`).
pub(crate) fn sync_timeout_out_of_range_result(
    tool_name: &str,
    default: u64,
) -> super::tool_result::ToolResult {
    super::tool_result::ToolResult::err_with_output(
        command_rejected_message(
            format!(
                "{tool_name} timeout_secs must be between {MIN_SYNC_TIMEOUT_SECS} and {MAX_SYNC_TIMEOUT_SECS}"
            ),
            format!(
                "pass timeout_secs between {MIN_SYNC_TIMEOUT_SECS} and {MAX_SYNC_TIMEOUT_SECS}, or omit it for the default of {default} seconds. For longer work use run_job."
            ),
        ),
        json!({
            "command_started": false,
            "command_completed": false,
            "command_ok": false,
            "exit_code": null,
            "execution_state": "not_started",
            "failure_kind": "invalid_arguments",
            "tool_failure": true,
        }),
    )
}

pub(crate) fn command_rejected_message(
    reason: impl AsRef<str>,
    guidance: impl AsRef<str>,
) -> String {
    format!(
        "Rejected before starting command: {}.\nNo command was started.\nNo files were modified.\nRetry guidance: {}",
        reason.as_ref(),
        guidance.as_ref()
    )
}

pub(crate) fn command_outcome_unknown_message(reason: impl AsRef<str>) -> String {
    format!(
        "Command execution outcome is unknown: {}.\nThe command may have started or produced side effects, but WebCodex did not receive a terminal result.\nDo not automatically retry a potentially side-effecting command.\nRetry guidance: inspect the actual Job, process, service, or target state as appropriate before deciding whether retry is safe.",
        reason.as_ref()
    )
}

pub(crate) fn command_failed_message(
    exit_code: Option<i32>,
    stdout_tail: &str,
    stderr_tail: &str,
) -> String {
    let status = exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "Command exited with status {}.\nNo files were modified by WebCodex itself; command side effects, if any, are from the invoked command.\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: inspect stderr/stdout above, then fix the reported issue or use a narrower tool.",
        status, stdout_tail, stderr_tail
    )
}

pub(crate) fn command_timeout_message(
    timeout_secs: u64,
    stdout_tail: &str,
    stderr_tail: &str,
) -> String {
    format!(
        "Command timed out after {}s.\nCommand definitely started, but WebCodex cannot prove its side effects ended with the timeout.\nOutput tails before timeout:\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: do not blindly retry. First inspect the actual process, service, and target state. If validation is safe and idempotent, use run_job for longer observation or a narrower invocation.",
        timeout_secs, stdout_tail, stderr_tail
    )
}

pub(crate) fn looks_like_command_timeout(
    exit_code: Option<i32>,
    stderr: &str,
    timeout_secs: u64,
) -> bool {
    exit_code == Some(-1)
        && stderr
            .to_ascii_lowercase()
            .contains(&format!("command timed out after {} seconds", timeout_secs))
}

pub(crate) fn is_safe_job_id(job_id: &str) -> bool {
    if job_id.is_empty() || job_id.len() > 80 || job_id.contains("..") {
        return false;
    }
    job_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub(crate) fn normalize_local_status(raw: &str) -> String {
    match raw.trim() {
        "queued" | "running" | "started" | "stop_requested" | "completed" | "failed"
        | "stopped" | "lost" | "timeout" | "timed_out" | "cancelled" => raw.trim().to_string(),
        "" => "running".to_string(),
        _ => "lost".to_string(),
    }
}

#[cfg(test)]
pub(crate) fn read_trim(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) const MAX_LOCAL_LOG_LINES: usize = 500;
pub(crate) const DEFAULT_JOB_LOG_TAIL_LINES: usize = 200;

#[cfg(test)]
pub(crate) fn read_lines_from(
    path: PathBuf,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> (String, usize, usize, bool) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    read_lines_from_text(&content, offset, tail_lines)
}

#[cfg(test)]
pub(crate) fn read_lines_from_text(
    content: &str,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> (String, usize, usize, bool) {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    // `offset` is a 1-based line cursor (matching agent `since_stdout_line`).
    // When provided, read forward from that line, bounded to MAX_LOCAL_LOG_LINES.
    // Otherwise return the last `tail_lines` (bounded), defaulting to the last
    // MAX_LOCAL_LOG_LINES lines. Output is always bounded.
    let (start_idx, limit) = if let Some(off) = offset {
        let s = off.saturating_sub(1).min(total);
        (s, MAX_LOCAL_LOG_LINES)
    } else {
        let tail = tail_lines
            .filter(|t| *t > 0)
            .map(|t| t.min(MAX_LOCAL_LOG_LINES))
            .unwrap_or(DEFAULT_JOB_LOG_TAIL_LINES);
        (total.saturating_sub(tail), tail)
    };
    let end_idx = (start_idx + limit).min(total);
    let selected = lines[start_idx..end_idx].join("\n");
    // 1-based line number to request for the next chunk.
    let next_line = end_idx + 1;
    (selected, next_line, total, start_idx > 0 || end_idx < total)
}

#[cfg(test)]
mod tests {
    // Every test in this module is Unix-only, so the glob import is only
    // needed there.
    #[cfg(unix)]
    use super::*;

    /// Regression guard for the local-command infinite hang: a shell that exits
    /// immediately after backgrounding a long-lived process which inherits the
    /// stdout/stderr pipes must NOT make `run_command_sync` block on pipe EOF.
    /// Before the process-group reap this returned only after the background
    /// `sleep` exited (~5s); now the group is killed and it returns promptly.
    #[cfg(unix)]
    #[test]
    fn run_command_sync_does_not_hang_on_backgrounded_pipe_holder() {
        let dir = std::env::temp_dir();
        let start = Instant::now();
        let (code, stdout, _stderr, _ms) = run_command_sync("echo done; sleep 5 &", &dir, 10);
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "run_command_sync blocked on a backgrounded pipe holder for {:?}",
            start.elapsed()
        );
        assert_eq!(code, 0, "foreground command should still report success");
        assert!(stdout.contains("done"), "stdout was: {stdout:?}");
    }

    /// A normal command's exit code and output are unchanged by the reap.
    #[cfg(unix)]
    #[test]
    fn run_command_sync_preserves_exit_code_and_output() {
        let dir = std::env::temp_dir();
        let (code, stdout, _stderr, _ms) = run_command_sync("echo hello", &dir, 10);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "hello");

        let (code, _stdout, _stderr, _ms) = run_command_sync("exit 3", &dir, 10);
        assert_eq!(code, 3, "non-zero exit codes must survive the reap");
    }

    /// A genuinely slow foreground command still hits the timeout path.
    #[cfg(unix)]
    #[test]
    fn run_command_sync_times_out_foreground_command() {
        let dir = std::env::temp_dir();
        let start = Instant::now();
        let (code, _stdout, stderr, _ms) = run_command_sync("sleep 30", &dir, 1);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timeout should fire near 1s, took {:?}",
            start.elapsed()
        );
        assert_eq!(code, -1);
        assert!(
            stderr.contains("timed out"),
            "expected timeout note, got: {stderr:?}"
        );
    }
}
