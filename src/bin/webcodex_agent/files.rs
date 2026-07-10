use super::config::AgentPolicy;
use super::output::CommandResult;
use super::shell::cwd_allowed;
use crate::agent_init::DEFAULT_MAX_OUTPUT_BYTES;
use crate::project_overview::build_project_overview;
use crate::shell_protocol::ShellAgentShellRequest;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn resolve_requested_path(
    policy: &AgentPolicy,
    cwd: Option<&str>,
    path: &str,
) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    let raw_path = PathBuf::from(path);
    let resolved = if raw_path.is_absolute() {
        raw_path
    } else {
        let base = cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        base.join(raw_path)
    };
    let mut parent_for_policy = if resolved.exists() {
        resolved.clone()
    } else {
        resolved
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| resolved.clone())
    };
    while !parent_for_policy.exists() {
        let Some(parent) = parent_for_policy.parent() else {
            break;
        };
        parent_for_policy = parent.to_path_buf();
    }
    cwd_allowed(policy, &parent_for_policy)?;
    Ok(resolved)
}

pub(crate) fn is_basic_file_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "file_read" | "file_write" | "file_list" | "file_project_overview"
    )
}

pub(crate) fn handle_basic_file_request(
    policy: &AgentPolicy,
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    match request.kind.as_str() {
        "file_read" => handle_file_read_request(policy, request, resolved, start),
        "file_write" => handle_file_write_request(policy, request, resolved, start),
        "file_list" => handle_file_list_request(resolved, start),
        "file_project_overview" => handle_project_overview_request(request, start),
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("unknown file request kind: {}", request.kind)),
        },
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProjectOverviewAgentOptions {
    #[serde(default)]
    max_depth: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

fn handle_project_overview_request(
    request: &ShellAgentShellRequest,
    start: Instant,
) -> CommandResult {
    let Some(project_root) = request.cwd.as_deref() else {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("project_overview request missing project root".to_string()),
        };
    };
    let requested_path = request.path.as_deref().unwrap_or(".");
    let options = match request.content.as_deref() {
        Some(payload) => match serde_json::from_str::<ProjectOverviewAgentOptions>(payload) {
            Ok(options) => options,
            Err(error) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!("invalid project_overview options: {error}")),
                }
            }
        },
        None => ProjectOverviewAgentOptions::default(),
    };
    match build_project_overview(
        Path::new(project_root),
        requested_path,
        options.max_depth,
        options.limit,
    ) {
        Ok(output) => CommandResult {
            exit_code: Some(0),
            stdout: Some(output.to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(error) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(error),
        },
    }
}

fn handle_file_read_request(
    policy: &AgentPolicy,
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let max = request
        .max_bytes
        .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES)
        .min(policy.max_output_bytes);
    if let (Some(start_line), Some(end_line)) = (request.start_line, request.end_line) {
        return handle_file_read_range_request(resolved, start_line, end_line, max, start);
    }

    match std::fs::read(resolved) {
        Ok(bytes) => {
            if bytes.len() > max {
                CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "file too large: {} bytes exceeds max_bytes {}",
                        bytes.len(),
                        max
                    )),
                }
            } else {
                CommandResult {
                    exit_code: Some(0),
                    stdout: Some(String::from_utf8_lossy(&bytes).to_string()),
                    stderr: Some(String::new()),
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: None,
                }
            }
        }
        Err(e) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("failed to read {}: {}", resolved.display(), e)),
        },
    }
}

fn handle_file_read_range_request(
    resolved: &Path,
    start_line: usize,
    end_line: usize,
    max: usize,
    start: Instant,
) -> CommandResult {
    if start_line == 0 || end_line < start_line {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("invalid line range for file_read".to_string()),
        };
    }

    let file = match std::fs::File::open(resolved) {
        Ok(file) => file,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to read {}: {}", resolved.display(), e)),
            };
        }
    };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut content = String::new();
    let mut total_lines = 0usize;
    let mut wrote_any_line = false;

    loop {
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(bytes) => bytes,
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "failed to read UTF-8 text {}: {}",
                        resolved.display(),
                        e
                    )),
                };
            }
        };
        if bytes_read == 0 {
            break;
        }
        total_lines = total_lines.saturating_add(1);
        if total_lines >= start_line && total_lines <= end_line {
            let line_content = line.strip_suffix('\n').unwrap_or(&line);
            let line_content = line_content.strip_suffix('\r').unwrap_or(line_content);
            let additional_len = line_content.len() + usize::from(wrote_any_line);
            if content.len().saturating_add(additional_len) > max {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "range output too large: {} bytes exceeds max_bytes {}",
                        content.len().saturating_add(additional_len),
                        max
                    )),
                };
            }
            if wrote_any_line {
                content.push('\n');
            }
            content.push_str(line_content);
            wrote_any_line = true;
        }
    }

    let limit = end_line.saturating_sub(start_line).saturating_add(1);
    let out = serde_json::json!({
        "format": "webcodex.file_read_range.v1",
        "content": content,
        "total_lines": total_lines,
        "start_line": start_line,
        "limit": limit,
    });
    CommandResult {
        exit_code: Some(0),
        stdout: Some(out.to_string()),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

fn handle_file_write_request(
    policy: &AgentPolicy,
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let content = request.content.clone().unwrap_or_default();
    if content.len() > policy.max_output_bytes {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!(
                "content too large: {} bytes exceeds max_output_bytes {}",
                content.len(),
                policy.max_output_bytes
            )),
        };
    }
    if let Some(expected) = request.expected_sha256.as_deref() {
        match std::fs::read(resolved) {
            Ok(existing) => {
                let actual = sha256_hex_bytes(&existing);
                if !actual.eq_ignore_ascii_case(expected) {
                    return CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(format!(
                            "expected_sha256 mismatch: expected {}, actual {}",
                            expected, actual
                        )),
                    };
                }
            }
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "failed to read existing file for expected_sha256 {}: {}",
                        resolved.display(),
                        e
                    )),
                };
            }
        }
    }
    if request.create_dirs {
        if let Some(parent) = resolved.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!(
                        "failed to create parent directory {}: {}",
                        parent.display(),
                        e
                    )),
                };
            }
        }
    }
    match std::fs::write(resolved, content.as_bytes()) {
        Ok(()) => CommandResult {
            exit_code: Some(0),
            stdout: Some(content.len().to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(e) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("failed to write {}: {}", resolved.display(), e)),
        },
    }
}

fn handle_file_list_request(resolved: &Path, start: Instant) -> CommandResult {
    match std::fs::read_dir(resolved) {
        Ok(entries) => {
            let mut names = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let suffix = entry
                    .file_type()
                    .ok()
                    .filter(|t| t.is_dir())
                    .map(|_| "/")
                    .unwrap_or("");
                names.push(format!("{}{}", name, suffix));
            }
            names.sort();
            CommandResult {
                exit_code: Some(0),
                stdout: Some(format!("{}\n", names.join("\n"))),
                stderr: Some(String::new()),
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            }
        }
        Err(e) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("failed to list {}: {}", resolved.display(), e)),
        },
    }
}
