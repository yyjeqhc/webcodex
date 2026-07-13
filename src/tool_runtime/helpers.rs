use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) fn run_command_sync(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    let start = Instant::now();
    let mut command = std::process::Command::new("sh");
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
    let bound_secs = timeout_secs.saturating_add(LOCAL_RUN_HARD_GRACE_SECS);
    let task = tokio::task::spawn_blocking(move || run_command_sync(&cmd, &cwd, timeout_secs));
    match tokio::time::timeout(Duration::from_secs(bound_secs), task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(LocalRunFailure::Join(e.to_string())),
        Err(_) => Err(LocalRunFailure::HardTimeout { bound_secs }),
    }
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

pub(crate) fn validate_project_relative_path(path: &str) -> Result<(), String> {
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Ok(());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    Ok(())
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
pub(crate) const DEFAULT_CARGO_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_RUN_SHELL_TIMEOUT_SECS: u64 = 60;

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
        "Command timed out after {}s.\nCommand was started.\nOutput tails before timeout:\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: use run_job for longer commands or rerun with a narrower test filter.",
        timeout_secs, stdout_tail, stderr_tail
    )
}

pub(crate) fn looks_like_command_timeout(
    exit_code: Option<i32>,
    stderr: &str,
    timeout_secs: u64,
) -> bool {
    exit_code == Some(-1)
        && stderr.contains(&format!("Command timed out after {} seconds", timeout_secs))
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

pub(crate) fn read_json(path: PathBuf) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

pub(crate) fn read_trim(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) const MAX_LOCAL_LOG_LINES: usize = 500;

pub(crate) fn read_lines_from(
    path: PathBuf,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> (String, usize) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
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
            .unwrap_or(MAX_LOCAL_LOG_LINES);
        (total.saturating_sub(tail), tail)
    };
    let end_idx = (start_idx + limit).min(total);
    let selected = lines[start_idx..end_idx].join("\n");
    // 1-based line number to request for the next chunk.
    let next_line = end_idx + 1;
    (selected, next_line)
}

#[cfg(test)]
mod tests {
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
