use crate::runner_protocol::{RAW_SHELL_COMMAND_MAX_BYTES, RAW_SHELL_WIRE_MAX_BYTES};
use serde_json::json;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
pub(crate) fn run_command_sync(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    let shell = test_shell();
    run_command_sync_with_shell(cmd, cwd, timeout_secs, &shell)
}

#[cfg(test)]
fn run_command_sync_with_shell(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
    shell: &Path,
) -> (i32, String, String, u64) {
    let mut command = std::process::Command::new(shell);
    #[cfg(windows)]
    command.arg("-s");
    #[cfg(not(windows))]
    command.arg("-c").arg(cmd);
    command.current_dir(cwd);
    #[cfg(windows)]
    let stdin_payload = Some(cmd.as_bytes());
    #[cfg(not(windows))]
    let stdin_payload: Option<&[u8]> = None;
    let (exit_code, stdout, stderr, elapsed_ms, _timed_out) =
        run_test_command_with_timeout(command, stdin_payload, timeout_secs);
    (exit_code, stdout, stderr, elapsed_ms)
}

#[cfg(test)]
pub(crate) fn run_test_command_with_timeout(
    mut command: std::process::Command,
    stdin_payload: Option<&[u8]>,
    timeout_secs: u64,
) -> (i32, String, String, u64, bool) {
    use std::io::{Read, Seek, SeekFrom, Write};

    let start = Instant::now();
    if let Some(stdin_payload) = stdin_payload {
        let mut stdin_source = match tempfile::tempfile() {
            Ok(file) => file,
            Err(error) => {
                return (
                    -1,
                    String::new(),
                    format!("Failed to create stdin source: {error}"),
                    start.elapsed().as_millis() as u64,
                    false,
                );
            }
        };
        if let Err(error) = stdin_source.write_all(stdin_payload) {
            return (
                -1,
                String::new(),
                format!("Failed to write stdin source: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
        if let Err(error) = stdin_source.seek(SeekFrom::Start(0)) {
            return (
                -1,
                String::new(),
                format!("Failed to rewind stdin source: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
        command.stdin(std::process::Stdio::from(stdin_source));
    } else {
        // Match the non-interactive Runner: no payload means EOF, not the
        // invoking console's stdin. Commands such as `git mktree` otherwise
        // wait for user input even though headless CI happens to pass.
        command.stdin(std::process::Stdio::null());
    }
    let mut stdout_capture = match tempfile::tempfile() {
        Ok(file) => file,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to create stdout capture: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
    };
    let mut stderr_capture = match tempfile::tempfile() {
        Ok(file) => file,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to create stderr capture: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
    };
    let stdout_sink = match stdout_capture.try_clone() {
        Ok(file) => file,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to clone stdout capture: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
    };
    let stderr_sink = match stderr_capture.try_clone() {
        Ok(file) => file,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to clone stderr capture: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
    };
    command
        .stdout(std::process::Stdio::from(stdout_sink))
        .stderr(std::process::Stdio::from(stderr_sink));
    // Put the command in its own process group so its whole subtree can be
    // reaped as a group. Regular-file capture prevents pipe-capacity deadlocks,
    // while process-group ownership prevents background descendants from
    // leaking beyond the fixture.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return (
                -1,
                String::new(),
                format!("Failed to execute command: {error}"),
                start.elapsed().as_millis() as u64,
                false,
            );
        }
    };
    // Under process_group(0) the child's pid is also its process-group id.
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
            Err(error) => {
                let _ = child.kill();
                reap_process_group(pgid);
                let _ = child.wait();
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for command: {error}"),
                    start.elapsed().as_millis() as u64,
                    false,
                );
            }
        }
    }
    if timed_out {
        let _ = child.kill();
    }
    // Whether the command timed out or exited on its own, reap the entire
    // process group so backgrounded descendants do not leak past the fixture.
    reap_process_group(pgid);
    let status = child.wait();
    let elapsed = start.elapsed().as_millis() as u64;

    let read_capture = |file: &mut std::fs::File| -> std::io::Result<Vec<u8>> {
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let stdout = read_capture(&mut stdout_capture);
    let stderr = read_capture(&mut stderr_capture);
    match (status, stdout, stderr) {
        (Ok(status), Ok(stdout), Ok(stderr)) => {
            let stdout = String::from_utf8_lossy(&stdout).to_string();
            let mut stderr = String::from_utf8_lossy(&stderr).to_string();
            if timed_out {
                if !stderr.is_empty() && !stderr.ends_with('\n') {
                    stderr.push('\n');
                }
                stderr.push_str(&format!("Command timed out after {timeout_secs} seconds"));
                (-1, stdout, stderr, elapsed, true)
            } else {
                (status.code().unwrap_or(-1), stdout, stderr, elapsed, false)
            }
        }
        (Err(error), _, _) if timed_out => (
            -1,
            String::new(),
            format!(
                "Command timed out after {timeout_secs} seconds; failed to reap command: {error}"
            ),
            elapsed,
            true,
        ),
        (Err(error), _, _) => (
            -1,
            String::new(),
            format!("Failed to wait for command: {error}"),
            elapsed,
            false,
        ),
        (_, Err(error), _) | (_, _, Err(error)) => (
            -1,
            String::new(),
            format!("Failed to collect command output: {error}"),
            elapsed,
            timed_out,
        ),
    }
}

#[cfg(all(test, not(windows)))]
pub(crate) fn test_shell() -> PathBuf {
    PathBuf::from("sh")
}

#[cfg(all(test, windows))]
pub(crate) fn test_shell() -> PathBuf {
    git_for_windows_shell().unwrap_or_else(|| PathBuf::from("sh"))
}

#[cfg(all(test, windows))]
fn git_for_windows_shell() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("--exec-path")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let exec_path = String::from_utf8_lossy(&output.stdout);
    let exec_path = PathBuf::from(exec_path.trim());
    exec_path.ancestors().find_map(|ancestor| {
        let candidate = ancestor.join("bin").join("sh.exe");
        candidate.is_file().then_some(candidate)
    })
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
#[cfg(all(test, unix))]
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

#[cfg(all(test, not(unix)))]
fn reap_process_group(_pgid: u32) {}

pub(crate) fn project_relative_runner_cwd(
    proj: &crate::projects::ProjectConfig,
    resolved: &str,
) -> Result<String, String> {
    if let Some(root) = parse_windows_runner_absolute_path(&proj.path)? {
        let Some(resolved) = parse_windows_runner_absolute_path(resolved)? else {
            return Err("cwd is outside project directory".to_string());
        };
        let relative = windows_runner_descendant_tail(&root, &resolved)
            .ok_or_else(|| "cwd is outside project directory".to_string())?;
        return if relative.is_empty() {
            Ok(".".to_string())
        } else {
            Ok(relative.join("/"))
        };
    }

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

/// Resolve a Runner command cwd against the registered project root.
///
/// The server cannot canonicalize paths on a remote Runner host, so this
/// performs the project-relative/lexical boundary check before dispatch. The
/// Agent remains responsible for canonicalizing the existing path against its
/// configured `allowed_roots`, which rejects symlink escapes.
pub(crate) fn resolve_runner_cwd(
    proj: &crate::projects::ProjectConfig,
    cwd: Option<&str>,
) -> Result<String, String> {
    if let Some(root) = parse_windows_runner_absolute_path(&proj.path)? {
        return resolve_windows_runner_cwd(&root, cwd);
    }

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

/// Pure lexical model of a Runner-owned Windows local-disk path.
///
/// The Server may be running on Unix while the owning Runner is on Windows, so
/// `std::path` on the Server cannot classify or join these paths. This model is
/// deliberately narrower than Windows filesystem semantics: it recognizes only
/// rooted local drive paths (plain or `\\?\` verbatim disk form), rejects parent
/// traversal, and leaves existence/symlink/allowed_roots truth to the Runner.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsRunnerPath {
    verbatim: bool,
    drive: char,
    components: Vec<String>,
}

fn parse_windows_runner_absolute_path(path: &str) -> Result<Option<WindowsRunnerPath>, String> {
    let (verbatim, path) = match path.strip_prefix(r"\\?\") {
        Some(path) => (true, path),
        None => (false, path),
    };
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return Ok(None);
    }

    let components = windows_runner_relative_components(&path[3..])?;
    Ok(Some(WindowsRunnerPath {
        verbatim,
        drive: (bytes[0] as char).to_ascii_uppercase(),
        components,
    }))
}

fn windows_runner_relative_components(path: &str) -> Result<Vec<String>, String> {
    if path.contains('\0') {
        return Err("cwd cannot contain NUL bytes".to_string());
    }
    let mut components = Vec::new();
    for component in path.split(|character| matches!(character, '\\' | '/')) {
        match component {
            "" | "." => {}
            ".." => return Err("cwd cannot contain parent traversal".to_string()),
            component if component.contains(':') => {
                return Err("cwd contains an invalid Windows path component".to_string())
            }
            component => components.push(component.to_string()),
        }
    }
    Ok(components)
}

fn windows_runner_component_eq(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn windows_runner_descendant_tail<'a>(
    root: &WindowsRunnerPath,
    requested: &'a WindowsRunnerPath,
) -> Option<&'a [String]> {
    if root.drive != requested.drive
        || requested.components.len() < root.components.len()
        || !root
            .components
            .iter()
            .zip(&requested.components)
            .all(|(left, right)| windows_runner_component_eq(left, right))
    {
        return None;
    }
    Some(&requested.components[root.components.len()..])
}

fn render_windows_runner_path(root: &WindowsRunnerPath, tail: &[String]) -> String {
    let mut output = if root.verbatim {
        format!("\\\\?\\{}:\\", root.drive)
    } else {
        format!("{}:\\", root.drive)
    };
    let mut components = root.components.iter().chain(tail.iter()).peekable();
    while let Some(component) = components.next() {
        output.push_str(component);
        if components.peek().is_some() {
            output.push('\\');
        }
    }
    output
}

fn resolve_windows_runner_cwd(
    root: &WindowsRunnerPath,
    cwd: Option<&str>,
) -> Result<String, String> {
    let Some(cwd) = cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return Ok(render_windows_runner_path(root, &[]));
    };
    if cwd == "." {
        return Ok(render_windows_runner_path(root, &[]));
    }

    if let Some(absolute) = parse_windows_runner_absolute_path(cwd)? {
        let tail = windows_runner_descendant_tail(root, &absolute)
            .ok_or_else(|| "cwd is outside project directory".to_string())?;
        return Ok(render_windows_runner_path(root, tail));
    }
    if cwd.starts_with(['\\', '/']) {
        return Err(
            "cwd must be project-relative or an absolute Windows local-drive path".to_string(),
        );
    }
    let relative = windows_runner_relative_components(cwd)?;
    Ok(render_windows_runner_path(root, &relative))
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

/// Decode Git's C-style quoted path representation. `core.quotePath=false`
/// preserves UTF-8 but Git still quotes whitespace/control characters, so
/// callers must decode before applying path policy or returning path metadata.
pub(crate) fn decode_git_quoted_path(raw: &str) -> Option<String> {
    let raw = raw.strip_suffix('\r').unwrap_or(raw);
    let raw = if raw.starts_with('"') {
        let bytes = raw.as_bytes();
        let mut index = 1usize;
        let mut closing = None;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = index.saturating_add(2),
                b'"' => {
                    closing = Some(index);
                    break;
                }
                _ => index += 1,
            }
        }
        let closing = closing?;
        let suffix = &raw[closing + 1..];
        if !suffix.is_empty() && !suffix.starts_with('\t') {
            return None;
        }
        &raw[..=closing]
    } else {
        raw.split_once('\t').map(|(path, _)| path).unwrap_or(raw)
    };
    if !raw.starts_with('"') {
        return Some(raw.to_string());
    }
    let inner = &raw[1..raw.len() - 1];
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            let ch = inner[index..].chars().next()?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            index += ch.len_utf8();
            continue;
        }
        index += 1;
        let escaped = *bytes.get(index)?;
        match escaped {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'v' => out.push(0x0b),
            b'f' => out.push(0x0c),
            b'r' => out.push(b'\r'),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                let mut value = (escaped - b'0') as u16;
                let mut consumed = 1;
                while consumed < 3 {
                    let Some(next) = bytes.get(index + consumed).copied() else {
                        break;
                    };
                    if !(b'0'..=b'7').contains(&next) {
                        break;
                    }
                    value = value * 8 + (next - b'0') as u16;
                    consumed += 1;
                }
                if value > u8::MAX as u16 {
                    return None;
                }
                out.push(value as u8);
                index += consumed - 1;
            }
            _ => return None,
        }
        index += 1;
    }
    String::from_utf8(out).ok()
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

pub(crate) use webcodex_core::shell_quote::shell_escape_simple;

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

pub(crate) fn bounded_tail(text: &str, max_chars: usize) -> (String, bool) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), false);
    }
    let tail: String = text.chars().skip(total - max_chars).collect();
    (tail, true)
}

pub(crate) const COMMAND_STDIO_TAIL_CHARS: usize = 12_000;

/// Synchronous Runner-wait tools share this hard upper bound with
/// `runner_http` validation (`wait_timeout_secs` must be <= 120).
pub(crate) const MIN_SYNC_TIMEOUT_SECS: u64 = 1;
pub(crate) const MAX_SYNC_TIMEOUT_SECS: u64 = 120;

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

/// Resolve a synchronous command timeout. Zero remains invalid, while an
/// oversized caller preference is clamped to the largest wait this path can
/// actually honor so the model does not need a second decision just to retry
/// with the documented ceiling.
pub(crate) fn resolve_sync_timeout_secs(
    timeout_secs: Option<u64>,
    default: u64,
) -> Result<u64, String> {
    debug_assert!((MIN_SYNC_TIMEOUT_SECS..=MAX_SYNC_TIMEOUT_SECS).contains(&default));
    let value = timeout_secs.unwrap_or(default);
    if value < MIN_SYNC_TIMEOUT_SECS {
        return Err(format!(
            "timeout_secs must be at least {MIN_SYNC_TIMEOUT_SECS}"
        ));
    }
    Ok(value.min(MAX_SYNC_TIMEOUT_SECS))
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
                "pass timeout_secs between {MIN_SYNC_TIMEOUT_SECS} and {MAX_SYNC_TIMEOUT_SECS}, or omit it for the default of {default} seconds. Duration alone does not select run_job or run_detached_process: ordinary long work should keep the canonical tool/Job handoff; use run_job only for intentional asynchronous shell start, and run_detached_process only for a native child that must survive Runner restart/replacement."
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
        "Command execution outcome is unknown: {}.\nThe command may have started or produced side effects, but WebPi did not receive a terminal result.\nDo not automatically retry a potentially side-effecting command.\nRetry guidance: inspect the actual Job, process, service, or target state as appropriate before deciding whether retry is safe.",
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
        "Command exited with status {}.\nNo files were modified by WebPi itself; command side effects, if any, are from the invoked command.\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: inspect stderr/stdout above, then fix the reported issue or use a narrower tool.",
        status, stdout_tail, stderr_tail
    )
}

pub(crate) fn command_timeout_message(
    timeout_secs: u64,
    stdout_tail: &str,
    stderr_tail: &str,
) -> String {
    format!(
        "Command timed out after {}s.\nCommand definitely started, but WebPi cannot prove its side effects ended with the timeout.\nOutput tails before timeout:\nstdout_tail:\n{}\nstderr_tail:\n{}\nRetry guidance: do not blindly retry. First inspect the actual Job, process, service, and target state. On a fresh safe attempt, keep ordinary long work on its canonical execution tool and Job handoff; use run_job only for intentional asynchronous shell start. If a new native child must survive Runner restart/replacement, use run_detached_process from the start instead of changing tools merely for duration.",
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

pub(crate) use webcodex_core::workflow_session_contract::is_safe_job_id;

pub(crate) const DEFAULT_JOB_LOG_TAIL_LINES: usize = 200;

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_project(path: &str) -> crate::projects::ProjectConfig {
        crate::projects::ProjectConfig {
            path: path.to_string(),
            client_id: "windows-runner".to_string(),
            allow_patch: true,
        }
    }

    #[test]
    fn runner_cwd_preserves_windows_native_path_syntax_across_server_platforms() {
        let project = windows_project(r"\\?\E:\git\webcodex");
        for (cwd, expected, relative) in [
            (None, r"\\?\E:\git\webcodex", "."),
            (Some("."), r"\\?\E:\git\webcodex", "."),
            (
                Some("apps/desktop"),
                r"\\?\E:\git\webcodex\apps\desktop",
                "apps/desktop",
            ),
            (
                Some(r"apps\desktop"),
                r"\\?\E:\git\webcodex\apps\desktop",
                "apps/desktop",
            ),
            (
                Some(r"e:/GIT/WEBCODEX/apps/桌面 project"),
                r"\\?\E:\git\webcodex\apps\桌面 project",
                "apps/桌面 project",
            ),
        ] {
            let resolved = resolve_runner_cwd(&project, cwd).unwrap();
            assert_eq!(resolved, expected, "cwd={cwd:?}");
            assert_eq!(
                project_relative_runner_cwd(&project, &resolved).unwrap(),
                relative,
                "cwd={cwd:?}"
            );
        }

        let plain_project = windows_project(r"E:\git\webcodex");
        let resolved =
            resolve_runner_cwd(&plain_project, Some(r"\\?\e:\GIT\WEBCODEX\apps/desktop")).unwrap();
        assert_eq!(resolved, r"E:\git\webcodex\apps\desktop");
        assert_eq!(
            project_relative_runner_cwd(&plain_project, &resolved).unwrap(),
            "apps/desktop"
        );
    }

    #[test]
    fn runner_cwd_windows_lexical_boundary_rejects_escape_and_ambiguous_roots() {
        let project = windows_project(r"\\?\E:\git\webcodex");
        for cwd in [
            r"..\outside",
            "../outside",
            r"E:\git\other",
            r"F:\git\webcodex",
            r"\Windows",
            r"\\server\share\repo",
            r"E:drive-relative",
            "bad\0cwd",
        ] {
            assert!(
                resolve_runner_cwd(&project, Some(cwd)).is_err(),
                "unsafe Windows cwd must fail closed: {cwd:?}"
            );
        }
    }

    /// Regression guard for the local-command infinite hang: a shell that exits
    /// immediately after backgrounding a long-lived process which inherits the
    /// stdout/stderr pipes must NOT make `run_command_sync` block on pipe EOF.
    /// Before the process-group reap this returned only after the background
    /// `sleep` exited (~5s); now the group is killed and it returns promptly.
    #[cfg(unix)]
    #[test]
    #[ignore = "manual real-process timing: validates OS process-group pipe-holder reap"]
    fn tool_runtime_real_process_run_command_sync_does_not_hang_on_backgrounded_pipe_holder() {
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

    /// Large test-process output must not turn into a fake timeout because the
    /// parent waited for exit before draining a bounded OS pipe.
    #[cfg(unix)]
    #[test]
    fn run_command_sync_captures_output_larger_than_pipe_capacity() {
        let dir = std::env::temp_dir();
        let (code, stdout, stderr, _ms) =
            run_command_sync("printf '%98304s' x; printf '%98304s' y >&2", &dir, 5);
        assert_eq!(
            code,
            0,
            "stderr tail: {}",
            &stderr[stderr.len().saturating_sub(200)..]
        );
        assert_eq!(stdout.len(), 98_304);
        assert_eq!(stderr.len(), 98_304);
    }

    /// A genuinely slow foreground command still hits the timeout path.
    #[cfg(unix)]
    #[test]
    #[ignore = "manual real-process timing: validates a real foreground shell timeout"]
    fn tool_runtime_real_process_run_command_sync_times_out_foreground_command() {
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
