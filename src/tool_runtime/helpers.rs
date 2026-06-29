use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) fn run_command_sync(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    let start = Instant::now();
    let spawn = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawn {
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
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let elapsed = start.elapsed().as_millis() as u64;
                    return match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                            let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();
                            if !stderr.is_empty() && !stderr.ends_with('\n') {
                                stderr.push('\n');
                            }
                            stderr.push_str(&format!(
                                "Command timed out after {} seconds",
                                timeout_secs
                            ));
                            (-1, stdout, stderr, elapsed)
                        }
                        Err(e) => (
                            -1,
                            String::new(),
                            format!(
                                "Command timed out after {} seconds; failed to collect output: {}",
                                timeout_secs, e
                            ),
                            elapsed,
                        ),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for command: {}", e),
                    start.elapsed().as_millis() as u64,
                );
            }
        }
    }
    match child.wait_with_output() {
        Ok(out) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let code = out.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => (
            -1,
            String::new(),
            format!("Failed to collect command output: {}", e),
            start.elapsed().as_millis() as u64,
        ),
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
        "queued" | "running" | "completed" | "failed" | "stopped" => raw.trim().to_string(),
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
