use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

pub(super) fn sanitize_tail(s: &str, max_len: usize) -> (String, bool) {
    let bytes = s.as_bytes();
    if bytes.len() <= max_len {
        (s.to_string(), false)
    } else {
        // Find a valid UTF-8 boundary near max_len
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        (s[end..].to_string(), true)
    }
}
pub(super) fn run_command(cmd: &str, cwd: &Path, timeout_secs: u64) -> (i32, String, String, u64) {
    let start = Instant::now();
    let spawn_result = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(child) => child,
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            return (
                -1,
                String::new(),
                format!("Failed to execute command: {}", e),
                elapsed,
            );
        }
    };

    let timeout = (timeout_secs > 0).then(|| Duration::from_secs(timeout_secs));
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let elapsed = start.elapsed().as_millis() as u64;
                    return match output {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                            let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
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
                let elapsed = start.elapsed().as_millis() as u64;
                return (
                    -1,
                    String::new(),
                    format!("Failed to wait for command: {}", e),
                    elapsed,
                );
            }
        }
    }

    match child.wait_with_output() {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to collect command output: {}", e),
                elapsed,
            )
        }
    }
}

/// Escape a string for safe use as a shell argument via `ssh -- arg`.
/// Uses single-quote wrapping with proper escaping.
pub(super) fn shell_escape(s: &str) -> String {
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

pub(super) fn shell_join_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| shell_escape(p))
        .collect::<Vec<_>>()
        .join(" ")
}
