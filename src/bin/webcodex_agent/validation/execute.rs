//! Bounded process execution for validation adapters.

use crate::validation_bridge::{MAX_VALIDATION_STDERR_SUMMARY_CHARS, MAX_VALIDATION_STDOUT_BYTES};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct CapturedProcess {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stdout_capped: bool,
    pub(crate) stderr_summary: Option<String>,
    pub(crate) duration_ms: u64,
    pub(crate) timed_out: bool,
    pub(crate) spawn_error: Option<String>,
}

/// Run argv with bounded stdout capture. When stdout exceeds the hard byte cap,
/// `stdout_capped` is true and `stdout` is empty (complete JSON only — never a
/// truncated body intended for parsing).
pub(crate) fn run_bounded(
    program: &Path,
    args: &[String],
    cwd: &Path,
    timeout_secs: u64,
) -> CapturedProcess {
    let start = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("PYTHONSTARTUP")
        .env_remove("PYTHONPATH");

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CapturedProcess {
                exit_code: None,
                stdout: Vec::new(),
                stdout_capped: false,
                stderr_summary: Some(bound_stderr(&format!("spawn failed: {error}"))),
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
                spawn_error: Some(format!("spawn failed: {error}")),
            };
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (stdout_tx, stdout_rx) = mpsc::channel::<(Vec<u8>, bool)>();
    if let Some(mut out) = stdout {
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut collected = Vec::new();
            let mut capped = false;
            loop {
                match out.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if collected.len() + n > MAX_VALIDATION_STDOUT_BYTES {
                            capped = true;
                            let mut discard = [0u8; 8192];
                            while let Ok(m) = out.read(&mut discard) {
                                if m == 0 {
                                    break;
                                }
                            }
                            break;
                        }
                        collected.extend_from_slice(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
            let _ = stdout_tx.send((if capped { Vec::new() } else { collected }, capped));
        });
    } else {
        let _ = stdout_tx.send((Vec::new(), false));
    }

    let (stderr_tx, stderr_rx) = mpsc::channel::<Vec<u8>>();
    if let Some(mut err) = stderr {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            let _ = stderr_tx.send(buf);
        });
    } else {
        let _ = stderr_tx.send(Vec::new());
    }

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return CapturedProcess {
                    exit_code: None,
                    stdout: Vec::new(),
                    stdout_capped: false,
                    stderr_summary: Some(bound_stderr(&format!("wait failed: {error}"))),
                    duration_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    spawn_error: Some(format!("wait failed: {error}")),
                };
            }
        }
    };

    let exit_code = if timed_out {
        Some(-1)
    } else {
        child.wait().ok().and_then(|s| s.code())
    };

    // Drain reader threads (bounded wait after kill/exit).
    let (stdout_bytes, stdout_capped) = stdout_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or((Vec::new(), false));
    let stderr_bytes = stderr_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes);

    if timed_out {
        return CapturedProcess {
            exit_code,
            stdout: Vec::new(),
            stdout_capped: false,
            stderr_summary: Some(bound_stderr(&format!(
                "command timed out after {timeout_secs} seconds"
            ))),
            duration_ms: start.elapsed().as_millis() as u64,
            timed_out: true,
            spawn_error: None,
        };
    }

    CapturedProcess {
        exit_code,
        stdout: stdout_bytes,
        stdout_capped,
        stderr_summary: if stderr_text.trim().is_empty() {
            None
        } else {
            Some(bound_stderr(&stderr_text))
        },
        duration_ms: start.elapsed().as_millis() as u64,
        timed_out: false,
        spawn_error: None,
    }
}

fn bound_stderr(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(MAX_VALIDATION_STDERR_SUMMARY_CHARS)
        .collect()
}

/// Resolve an executable by env override then PATH search. Callers must not
/// expose the absolute executable path across the bridge.
pub(crate) fn resolve_executable(env_override: &str, executable_name: &str) -> Option<PathBuf> {
    if let Ok(value) = std::env::var(env_override) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    which_in_path(executable_name)
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
