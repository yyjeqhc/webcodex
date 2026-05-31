use crate::projects::{ProjectConfig, SshConfig};
use std::time::Instant;

const SSH_START_MARKER: &str = "__PRIVATE_DROP_SSH_COMMAND_STARTED__";

/// Build ordered SSH target strings, preserving legacy host compatibility.
pub(super) fn build_ssh_targets(proj: &ProjectConfig) -> Result<Vec<String>, String> {
    let targets = proj.ssh_targets();
    if targets.is_empty() {
        Err("SSH executor requires 'host' or non-empty 'ssh_hosts' in projects.toml".to_string())
    } else {
        Ok(targets)
    }
}

pub(super) fn ssh_option_args(config: Option<&SshConfig>) -> Vec<String> {
    let Some(config) = config else {
        return Vec::new();
    };
    let mut args = Vec::new();
    if config.batch_mode || config.control_master {
        args.push("-o".to_string());
        args.push("BatchMode=yes".to_string());
    }
    if let Some(secs) = config.connect_timeout_secs {
        args.push("-o".to_string());
        args.push(format!("ConnectTimeout={secs}"));
    }
    if config.control_master {
        args.push("-o".to_string());
        args.push("ControlMaster=auto".to_string());
        if let Some(v) = &config.control_persist {
            args.push("-o".to_string());
            args.push(format!("ControlPersist={v}"));
        }
        if let Some(v) = &config.control_path {
            args.push("-o".to_string());
            args.push(format!("ControlPath={v}"));
        }
    }
    if let Some(secs) = config.server_alive_interval {
        args.push("-o".to_string());
        args.push(format!("ServerAliveInterval={secs}"));
    }
    if let Some(max) = config.server_alive_count_max {
        args.push("-o".to_string());
        args.push(format!("ServerAliveCountMax={max}"));
    }
    args
}

pub(super) fn build_ssh_command(
    ssh_target: &str,
    remote_cmd: &str,
    config: Option<&SshConfig>,
) -> std::process::Command {
    let mut command = std::process::Command::new("ssh");
    for arg in ssh_option_args(config) {
        command.arg(arg);
    }
    command.arg(ssh_target).arg("--").arg(remote_cmd);
    command
}

fn with_started_marker(remote_cmd: &str) -> String {
    format!("printf '{}\\n'; {}", SSH_START_MARKER, remote_cmd)
}

fn strip_started_marker(stdout: String) -> (bool, String) {
    let mut started = false;
    let mut lines = Vec::new();
    for line in stdout.lines() {
        if !started && line == SSH_START_MARKER {
            started = true;
            continue;
        }
        lines.push(line);
    }
    if stdout.ends_with('\n') && !lines.is_empty() {
        (started, format!("{}\n", lines.join("\n")))
    } else {
        (started, lines.join("\n"))
    }
}

pub(super) fn is_pre_start_ssh_connect_failure(code: i32, stdout: &str, stderr: &str) -> bool {
    if code != 255 && code != -1 {
        return false;
    }
    if stdout.lines().any(|line| line == SSH_START_MARKER) {
        return false;
    }
    let err = stderr.to_ascii_lowercase();
    code == -1
        || err.contains("connection refused")
        || err.contains("connection timed out")
        || err.contains("operation timed out")
        || err.contains("connection reset")
        || err.contains("connection closed")
        || err.contains("connection aborted")
        || err.contains("no route to host")
        || err.contains("could not resolve hostname")
        || err.contains("name or service not known")
        || err.contains("temporary failure in name resolution")
        || err.contains("kex_exchange_identification")
        || err.contains("failed to execute ssh")
        || err.contains("failed to execute ssh command")
        || err.contains("failed to execute ssh patch")
}

fn append_attempt_error(errors: &mut Vec<String>, target: &str, code: i32, stderr: &str) {
    let msg = stderr.trim();
    if msg.is_empty() {
        errors.push(format!("{target}: exit {code}"));
    } else {
        errors.push(format!("{target}: exit {code}: {msg}"));
    }
}

fn combine_fallback_errors(errors: &[String]) -> String {
    format!(
        "All SSH endpoints failed before command start: {}",
        errors.join(" | ")
    )
}

fn run_ssh_single(
    ssh_target: &str,
    remote_cmd: &str,
    _timeout_secs: u64,
    ssh_config: Option<&SshConfig>,
) -> (i32, String, String, u64, bool) {
    let start = Instant::now();
    let result =
        build_ssh_command(ssh_target, &with_started_marker(remote_cmd), ssh_config).output();

    match result {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let (started, stdout) = strip_started_marker(raw_stdout);
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed, started)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to execute SSH command: {}", e),
                elapsed,
                false,
            )
        }
    }
}

/// Run a command using ordered fallback endpoints. Retries only when SSH fails before
/// the remote command start marker is observed.
pub(super) fn run_ssh_targets(
    ssh_targets: &[String],
    remote_cmd: &str,
    timeout_secs: u64,
    ssh_config: Option<&SshConfig>,
) -> (i32, String, String, u64) {
    let mut errors = Vec::new();
    let mut total_ms = 0;
    for (idx, target) in ssh_targets.iter().enumerate() {
        let (code, stdout, stderr, elapsed, started) =
            run_ssh_single(target, remote_cmd, timeout_secs, ssh_config);
        total_ms += elapsed;
        if code == 0 || started || !is_pre_start_ssh_connect_failure(code, &stdout, &stderr) {
            return (code, stdout, stderr, total_ms);
        }
        append_attempt_error(&mut errors, target, code, &stderr);
        if idx + 1 == ssh_targets.len() {
            return (
                -1,
                String::new(),
                combine_fallback_errors(&errors),
                total_ms,
            );
        }
    }
    (
        -1,
        String::new(),
        "No SSH endpoints configured".to_string(),
        total_ms,
    )
}

fn run_ssh_patch_target(
    ssh_target: &str,
    patch: &str,
    remote_cmd_template: &str,
    ssh_config: Option<&SshConfig>,
) -> (i32, String, String, u64, bool) {
    let patch_id = uuid::Uuid::new_v4();
    let remote_patch = format!("/tmp/private-drop-patch-{}.diff", patch_id);
    let remote_cmd = format!(
        "printf '{}\\n'; cat > '{}' && {} && rm -f '{}'",
        SSH_START_MARKER,
        remote_patch,
        remote_cmd_template.replace("__PATCH__", &remote_patch),
        remote_patch
    );
    let start = Instant::now();
    let result = build_ssh_command(ssh_target, &remote_cmd, ssh_config)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(patch.as_bytes());
            }
            child.wait_with_output()
        });

    match result {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let (started, stdout) = strip_started_marker(raw_stdout);
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed, started)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to execute SSH patch: {}", e),
                elapsed,
                false,
            )
        }
    }
}

pub(super) fn run_ssh_patch_targets(
    ssh_targets: &[String],
    _project_path: &str,
    patch: &str,
    remote_cmd_template: &str,
    ssh_config: Option<&SshConfig>,
) -> (i32, String, String, u64) {
    let mut errors = Vec::new();
    let mut total_ms = 0;
    for (idx, target) in ssh_targets.iter().enumerate() {
        let (code, stdout, stderr, elapsed, started) =
            run_ssh_patch_target(target, patch, remote_cmd_template, ssh_config);
        total_ms += elapsed;
        if code == 0 || started || !is_pre_start_ssh_connect_failure(code, &stdout, &stderr) {
            return (code, stdout, stderr, total_ms);
        }
        append_attempt_error(&mut errors, target, code, &stderr);
        if idx + 1 == ssh_targets.len() {
            return (
                -1,
                String::new(),
                combine_fallback_errors(&errors),
                total_ms,
            );
        }
    }
    (
        -1,
        String::new(),
        "No SSH endpoints configured".to_string(),
        total_ms,
    )
}

pub(super) fn parse_ssh_batch_blocks(stdout: &str, count: usize, nonce: &str) -> Vec<String> {
    let mut blocks = vec![String::new(); count];
    let mut current: Option<usize> = None;
    let start_prefix = format!("__PDCTX_{}_START_", nonce);
    let end_prefix = format!("__PDCTX_{}_END_", nonce);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix(&start_prefix) {
            if let Some(idx) = rest
                .strip_suffix("__")
                .and_then(|s| s.parse::<usize>().ok())
            {
                current = if idx < count { Some(idx) } else { None };
            }
            continue;
        }
        if line.starts_with(&end_prefix) {
            current = None;
            continue;
        }
        if let Some(idx) = current {
            blocks[idx].push_str(line);
            blocks[idx].push('\n');
        }
    }
    blocks
}
