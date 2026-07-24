use super::config::{validate_shell_config, AgentPolicy, ShellConfig, ShellProfileConfig};
use super::output::CommandResult;
use super::projects::find_project_shell_context;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SHELL_PROFILE_PREPARE_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreparedShellProfileKey {
    project_key: String,
    profile_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedShellProfile {
    pub(crate) profile_name: String,
    program: String,
    args: Vec<String>,
    env_snapshot: HashMap<String, String>,
}

/// Lazily prepared shell environment snapshots. Snapshots are keyed by
/// project/cwd plus profile name because inline init scripts such as
/// `. .venv/bin/activate` are intentionally resolved from the project cwd.
/// Profile config changes require restarting the agent in this phase.
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedShellProfileCache {
    profiles: Arc<Mutex<HashMap<PreparedShellProfileKey, Arc<PreparedShellProfile>>>>,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn should_inherit_env_key(key: &str) -> bool {
    !matches!(
        key,
        "WEBCODEX_TOKEN" | "WEBCODEX_AGENT_TOKEN" | "WEBCODEX_USER_TOKEN" | "AUTHORIZATION"
    )
}

fn shell_command_text(shell: &ShellConfig, command: &str) -> String {
    match shell.init_script.as_ref() {
        Some(path) => format!(
            ". {} && (\n{}\n)",
            shell_quote(&path.to_string_lossy()),
            command
        ),
        None => command.to_string(),
    }
}

fn apply_shell_environment(cmd: &mut Command, shell: &ShellConfig) -> Result<(), String> {
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
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
        cmd.env(key, value);
    }
    Ok(())
}

fn apply_env_snapshot(cmd: &mut Command, env_snapshot: &HashMap<String, String>) {
    cmd.env_clear();
    for (key, value) in env_snapshot {
        cmd.env(key, value);
    }
}

fn configured_shell_command(shell: &ShellConfig, command: &str) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let mut cmd = Command::new(&shell.program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, command));
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
    cmd.arg(command);
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

pub(crate) fn configured_shell_job_command(
    shell: &ShellConfig,
    command: &str,
) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let mut cmd = Command::new("setsid");
    cmd.arg(&shell.program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, command));
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

pub(crate) fn configured_prepared_shell_job_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new("setsid");
    cmd.arg(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(command);
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
    configure_direct_process_group(&mut cmd);
    match profile {
        Some(profile) => apply_env_snapshot(&mut cmd, &profile.env_snapshot),
        None => {
            validate_shell_config(shell)?;
            apply_shell_environment(&mut cmd, shell)?;
        }
    }
    Ok(cmd)
}

fn configure_direct_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid` is async-signal-safe and touches no Rust-managed
        // memory in the post-fork child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
}

fn base_shell_env(
    shell: &ShellConfig,
    profile: &ShellProfileConfig,
) -> Result<HashMap<String, String>, String> {
    let mut env: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| should_inherit_env_key(key))
        .collect();
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        if let Some(current) = env.get("PATH") {
            paths.extend(std::env::split_paths(current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        env.insert("PATH".to_string(), joined.to_string_lossy().to_string());
    }
    for (key, value) in &shell.env {
        env.insert(key.clone(), value.clone());
    }
    for (key, value) in &profile.env {
        env.insert(key.clone(), value.clone());
    }
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
        env.remove(key);
    }
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

fn run_prepare_command(
    mut cmd: Command,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn profile prepare command: {}", e))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "profile prepare stdout pipe missing".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "profile prepare stderr pipe missing".to_string())?;
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare stdout: {}", e))
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stderr
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare stderr: {}", e))
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _stdout = stdout_handle
                        .join()
                        .map_err(|_| "profile prepare stdout reader panicked".to_string())??;
                    let stderr = stderr_handle
                        .join()
                        .map_err(|_| "profile prepare stderr reader panicked".to_string())??;
                    return Err(format!(
                        "profile prepare timed out after {} seconds; stderr tail: {}",
                        timeout.as_secs(),
                        stderr_tail(&stderr)
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed to wait profile prepare command: {}", e));
            }
        }
    };
    let stdout = stdout_handle
        .join()
        .map_err(|_| "profile prepare stdout reader panicked".to_string())??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| "profile prepare stderr reader panicked".to_string())??;
    Ok((status, stdout, stderr))
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

fn capture_profile_env_snapshot(
    profile_name: &str,
    profile: &ShellProfileConfig,
    program: &str,
    args: &[String],
    prepare_cwd: &Path,
    initial_env: HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    let Some(init_script) = profile.init_script.as_deref() else {
        return Ok(initial_env);
    };
    let marker = format!("__WEBCODEX_ENV_START_{}__", uuid::Uuid::new_v4().simple());
    let prepare_script = format!(
        "set -e\n{}\nprintf '\\n{}\\n'\nenv -0\n",
        init_script, marker
    );
    let mut cmd = Command::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(prepare_script).current_dir(prepare_cwd).env_clear();
    for (key, value) in initial_env {
        cmd.env(key, value);
    }
    let (status, stdout, stderr) =
        run_prepare_command(cmd, Duration::from_secs(SHELL_PROFILE_PREPARE_TIMEOUT_SECS)).map_err(
            |e| {
                format!(
                    "failed to prepare shell profile '{}' at {}: {}",
                    profile_name,
                    prepare_cwd.display(),
                    e
                )
            },
        )?;
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
    for key in [
        "WEBCODEX_TOKEN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_USER_TOKEN",
        "AUTHORIZATION",
    ] {
        snapshot.remove(key);
    }
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
        shell: &ShellConfig,
        profile_name: &str,
        project_key: String,
        prepare_cwd: &Path,
    ) -> Result<Arc<PreparedShellProfile>, String> {
        let key = PreparedShellProfileKey {
            project_key,
            profile_name: profile_name.to_string(),
        };
        if let Some(prepared) = self.profiles.lock().unwrap().get(&key).cloned() {
            return Ok(prepared);
        }
        let profile = shell.profiles.get(profile_name).ok_or_else(|| {
            format!(
                "shell profile '{}' is not configured for project/cwd {}",
                profile_name,
                prepare_cwd.display()
            )
        })?;
        let program = profile
            .program
            .clone()
            .unwrap_or_else(|| shell.program.clone());
        let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
        let initial_env = base_shell_env(shell, profile)?;
        let env_snapshot = capture_profile_env_snapshot(
            profile_name,
            profile,
            &program,
            &args,
            prepare_cwd,
            initial_env,
        )?;
        let prepared = Arc::new(PreparedShellProfile {
            profile_name: profile_name.to_string(),
            program,
            args,
            env_snapshot,
        });
        self.profiles.lock().unwrap().insert(key, prepared.clone());
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
    shell: &ShellConfig,
    projects_dir: &Path,
    cwd_path: &Path,
    request_has_cwd: bool,
    cache: &PreparedShellProfileCache,
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
        .get_or_prepare(shell, profile_name, project_key, &prepare_cwd)
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
        if cwd == root || cwd.starts_with(&root) {
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

fn read_pipes(
    mut child: std::process::Child,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout pipe missing".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr pipe missing".to_string())?;
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stdout.read_to_end(&mut buf).map(|_| buf);
        result.map_err(|e| format!("failed to read stdout: {}", e))
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stderr.read_to_end(&mut buf).map(|_| buf);
        result.map_err(|e| format!("failed to read stderr: {}", e))
    });
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait command: {}", e))?;
    let stdout = stdout_handle
        .join()
        .map_err(|_| "stdout reader panicked".to_string())??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| "stderr reader panicked".to_string())??;
    Ok((status, stdout, stderr))
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
    )
}

pub(crate) fn run_shell_with_profiles(
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
    run_shell_impl(
        policy,
        shell,
        Some((projects_dir, cache)),
        cwd,
        command,
        stdin,
        timeout_secs,
        stop_requested,
    )
}

fn run_shell_impl(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    profiles: Option<(&Path, &PreparedShellProfileCache)>,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    if !policy.allow_raw_shell {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("raw shell is disabled by local agent policy".to_string()),
        };
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(e) = cwd_allowed(policy, &cwd_path) {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(e),
        };
    }
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let start = Instant::now();
    let mut prepared_profile_name = None;
    let mut cmd = match profiles {
        Some((projects_dir, cache)) => {
            match resolve_prepared_shell_profile(
                shell,
                projects_dir,
                &cwd_path,
                cwd.is_some(),
                cache,
            ) {
                Ok(Some(profile)) => match configured_prepared_shell_command(&profile, command) {
                    Ok(cmd) => {
                        prepared_profile_name = Some(profile.profile_name.clone());
                        cmd
                    }
                    Err(e) => {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to configure shell profile '{}': {}",
                                profile.profile_name, e
                            )),
                        };
                    }
                },
                Ok(None) => match configured_shell_command(shell, command) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        return CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(e),
                        };
                    }
                },
                Err(e) => {
                    return CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(e),
                    };
                }
            }
        }
        None => match configured_shell_command(shell, command) {
            Ok(cmd) => cmd,
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(e),
                };
            }
        },
    };
    cmd.current_dir(&cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let spawn = cmd.spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            let error = prepared_profile_name
                .as_deref()
                .map(|profile_name| {
                    format!("failed to spawn shell profile '{}': {}", profile_name, e)
                })
                .unwrap_or_else(|| format!("failed to spawn command: {}", e));
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
            };
        }
    };
    if let Some(input) = stdin {
        match child.stdin.take() {
            Some(mut child_stdin) => {
                if let Err(e) = child_stdin.write_all(input.as_bytes()) {
                    let _ = child.kill();
                    return CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(format!("failed to write command stdin: {}", e)),
                    };
                }
            }
            None => {
                let _ = child.kill();
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some("stdin pipe missing".to_string()),
                };
            }
        }
    }
    loop {
        if stop_requested
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            let _ = child.kill();
            let duration_ms = start.elapsed().as_millis() as u64;
            return match read_pipes(child) {
                Ok((_status, stdout, stderr)) => CommandResult {
                    exit_code: Some(-1),
                    stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
                    stderr: Some(format!(
                        "{}{}job stopped by request",
                        truncate_bytes(&stderr, policy.max_output_bytes),
                        if stderr.is_empty() { "" } else { "\n" },
                    )),
                    duration_ms: Some(duration_ms),
                    error: Some("job stopped".to_string()),
                },
                Err(e) => CommandResult {
                    exit_code: Some(-1),
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(duration_ms),
                    error: Some(format!("job stopped; failed to collect output: {}", e)),
                },
            };
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return match read_pipes(child) {
                        Ok((_status, stdout, stderr)) => CommandResult {
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
                        },
                        Err(e) => CommandResult {
                            exit_code: Some(-1),
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(duration_ms),
                            error: Some(format!(
                                "command timed out; failed to collect output: {}",
                                e
                            )),
                        },
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(format!("failed to wait command: {}", e)),
                };
            }
        }
    }
    match read_pipes(child) {
        Ok((status, stdout, stderr)) => CommandResult {
            exit_code: Some(status.code().unwrap_or(-1)),
            stdout: Some(truncate_bytes(&stdout, policy.max_output_bytes)),
            stderr: Some(truncate_bytes(&stderr, policy.max_output_bytes)),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(e) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(e),
        },
    }
}
