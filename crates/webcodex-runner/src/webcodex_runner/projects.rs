use super::config::{
    default_true, project_registry_dir, validate_shell_profile_name, RunnerConfig, RunnerPolicy,
};
use super::shell::canonicalize_existing;
use crate::shell_protocol::{ShellAgentProjectSummary, ShellAgentShellRequest};
use crate::{err_cmd, ok_cmd, write_created_file};
use crate::{CommandResult, CreatedProjectPaths};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use webcodex_process::{GracefulTermination, ManagedChild};
use webcodex_runner_config::paths::paths_equal;

const PROJECT_SCAN_CACHE_MS: u64 = 5000;
const PROJECT_GIT_TIMEOUT: Duration = Duration::from_secs(2);
const PROJECT_GIT_CLEANUP_TIMEOUT: Duration = Duration::from_millis(500);
const PROJECT_GIT_OUTPUT_MAX_BYTES: usize = 64 * 1024;
const MANAGED_TEMPORARY_PROJECT_KIND: &str = "managed_temporary";
const AUTO_REGISTERED_PROJECT_KIND: &str = "auto_registered";
const DEFAULT_MANAGED_TEMPORARY_PROJECT_NAME: &str = "Temporary Project";
const MANAGED_TEMPORARY_PROJECT_ID_PREFIX: &str = "temporary";
const MANAGED_TEMPORARY_PROJECT_CREATE_ATTEMPTS: usize = 16;
const AUTO_PROJECT_HASH_PREFIX_LENGTHS: &[usize] = &[8, 12, 16, 24, 32, 48, 64];
static PROJECT_REGISTRY_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn project_registry_write_lock() -> &'static Mutex<()> {
    PROJECT_REGISTRY_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn project_error_cmd(start: Instant, error_code: &'static str) -> CommandResult {
    CommandResult {
        exit_code: Some(1),
        stdout: Some(
            serde_json::to_string(&serde_json::json!({"error_code": error_code}))
                .unwrap_or_else(|_| r#"{"error_code":"operation_failed"}"#.to_string()),
        ),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

fn structured_project_error_cmd(
    start: Instant,
    error_kind: &'static str,
    state_changed: bool,
    fields: serde_json::Value,
) -> CommandResult {
    let mut output = serde_json::json!({
        "error_code": error_kind,
        "error_kind": error_kind,
        "failure_kind": error_kind,
        "state_changed": state_changed,
    });
    if let (Some(output), Some(fields)) = (output.as_object_mut(), fields.as_object()) {
        output.extend(fields.clone());
    }
    CommandResult {
        exit_code: Some(1),
        stdout: Some(
            serde_json::to_string(&output)
                .unwrap_or_else(|_| r#"{"error_code":"operation_failed"}"#.to_string()),
        ),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunnerProjectFile {
    pub(crate) id: String,
    pub(crate) path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shell_profile: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) allow_patch: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) hooks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RunnerProjectCache {
    projects: Vec<ShellAgentProjectSummary>,
    refreshed_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) struct RunnerProjectShellContext {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) shell_profile: Option<String>,
}

fn validate_project_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id == "." || id == ".." {
        return Err("id cannot be '.' or '..'".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("id may only contain ASCII letters, digits, '-', '_', and '.'".to_string());
    }
    Ok(())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn runner_project_server_format_hint(content: &str, err: &str) -> Option<String> {
    let normalized = err.replace('`', "");
    if normalized.contains("missing field id") && content.contains("[projects.") {
        Some(
            "looks like a server projects.toml entry. Runner project registration records must use top-level fields:\n\
             id = \"smoke\"\n\
             path = \"/path/to/repo\""
                .to_string(),
        )
    } else {
        None
    }
}

pub(crate) fn parse_runner_project_toml(content: &str) -> Result<RunnerProjectFile, String> {
    let mut project: RunnerProjectFile = toml::from_str(content).map_err(|e| {
        let err = e.to_string();
        let base = format!("failed to parse project toml: {}", err);
        match runner_project_server_format_hint(content, &err) {
            Some(hint) => format!("{}; {}", base, hint),
            None => base,
        }
    })?;
    project.id = project.id.trim().to_string();
    validate_project_id(&project.id)?;
    project.path = project.path.trim().to_string();
    if project.path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    project.name = trim_optional(project.name);
    project.kind = trim_optional(project.kind);
    project.description = trim_optional(project.description);
    if let Some(shell_profile) = &project.shell_profile {
        validate_shell_profile_name("project.shell_profile", shell_profile)?;
    }
    let mut hooks = HashMap::new();
    for (name, commands) in project.hooks {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("hook name cannot be empty".to_string());
        }
        hooks.insert(name, commands);
    }
    project.hooks = hooks;
    Ok(project)
}

fn load_runner_project_shell_contexts_from_dir(dir: &Path) -> Vec<RunnerProjectShellContext> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(project) = parse_runner_project_toml(&content) else {
            continue;
        };
        if project.disabled || !seen.insert(project.id.clone()) {
            continue;
        }
        projects.push(RunnerProjectShellContext {
            id: project.id,
            path: project.path,
            shell_profile: project.shell_profile,
        });
    }
    projects
}

pub(crate) fn find_project_shell_context(
    project_registry_dir: &Path,
    cwd_path: &Path,
) -> Option<RunnerProjectShellContext> {
    let cwd = cwd_path.canonicalize().ok()?;
    load_runner_project_shell_contexts_from_dir(project_registry_dir)
        .into_iter()
        .filter_map(|project| {
            let project_path = PathBuf::from(&project.path).canonicalize().ok()?;
            // Windows filesystems are case-insensitive and `canonicalize` may
            // return `\\?\`-prefixed paths, so containment uses the shared
            // path identity rules instead of raw `==`/`starts_with`.
            if webcodex_runner_config::paths::path_is_within(&cwd, &project_path) {
                Some((project_path.components().count(), project))
            } else {
                None
            }
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, project)| project)
}

/// Resolve one enabled project by its Runner-local id. Persistent shells use
/// the id from the authenticated runtime-project binding rather than choosing
/// a project solely from a caller-controlled cwd.
pub(crate) fn find_project_shell_context_by_id(
    project_registry_dir: &Path,
    project_id: &str,
) -> Option<RunnerProjectShellContext> {
    load_runner_project_shell_contexts_from_dir(project_registry_dir)
        .into_iter()
        .find(|project| project.id == project_id)
}

struct BoundedGitOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_capped: bool,
    stderr_capped: bool,
}

fn spawn_bounded_git_reader(
    mut pipe: impl Read + Send + 'static,
) -> (mpsc::Receiver<(Vec<u8>, bool)>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let mut retained = Vec::with_capacity(PROJECT_GIT_OUTPUT_MAX_BYTES.min(8192));
        let mut chunk = [0_u8; 8192];
        let mut capped = false;
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let remaining = PROJECT_GIT_OUTPUT_MAX_BYTES.saturating_sub(retained.len());
                    let keep = remaining.min(read);
                    retained.extend_from_slice(&chunk[..keep]);
                    capped |= keep < read;
                }
                Err(_) => break,
            }
        }
        let _ = tx.send((retained, capped));
    });
    (rx, handle)
}

/// Terminate the whole Git process tree within one shared cleanup deadline,
/// then reap the direct child and confirm the complete tree exited.
///
/// The platform tree isolation lives in [`ManagedChild`]: a private process
/// group on Unix, a kill-on-close Job Object on Windows. Phase 1 (Unix only)
/// requests graceful tree termination and gives the tree a short bounded grace
/// to exit on its own; Windows reports [`GracefulTermination::Unsupported`]
/// and skips straight to phase 2. Phase 2 forcefully terminates any tree that
/// is still alive. Then the direct child is reaped and the complete tree (not
/// just the direct child) is confirmed exited — all within `deadline`. The
/// direct child's `ExitStatus`, when it can still be obtained, is returned;
/// failures are joined into one error string, but cleanup never gives up early
/// because a graceful request failed.
fn terminate_project_git_tree(
    child: &mut ManagedChild,
    deadline: Instant,
) -> Result<Option<ExitStatus>, String> {
    let mut errors = Vec::new();

    match child.request_terminate_tree() {
        Ok(GracefulTermination::Requested) => {
            // The whole tree received a graceful termination request. Give it
            // a short bounded grace to exit on its own; the grace never
            // extends past the overall cleanup deadline.
            let grace_deadline = deadline.min(Instant::now() + Duration::from_millis(50));
            let remaining = grace_deadline.saturating_duration_since(Instant::now());
            match child.wait_tree_exit(remaining) {
                Ok(_) => {}
                Err(error) => {
                    errors.push(format!("git graceful termination wait failed: {error}"));
                }
            }
        }
        Ok(GracefulTermination::AlreadyExited) => {
            // The owned tree was already fully gone; nothing to signal or wait for.
        }
        Ok(GracefulTermination::Unsupported) => {
            // Windows: no generic graceful tree termination. Escalate below.
        }
        Err(error) => {
            errors.push(format!("git graceful termination request failed: {error}"));
        }
    }

    // Forceful phase: any tree still alive is terminated as a whole.
    let tree_alive = match child.try_tree_exit() {
        Ok(exited) => !exited,
        Err(error) => {
            errors.push(format!("git tree liveness probe failed: {error}"));
            true
        }
    };
    if tree_alive {
        if let Err(error) = child.terminate_tree() {
            errors.push(format!("git tree termination failed: {error}"));
        }
    }

    // Reap the direct child within the remaining deadline.
    let mut status = None;
    loop {
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                status = Some(exit_status);
                break;
            }
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    errors.push("git child reap timed out".to_string());
                    break;
                }
                thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) => {
                errors.push(format!("git child reap failed: {error}"));
                break;
            }
        }
    }

    // Confirm the complete tree exited, not just the direct child. Forceful
    // termination can complete asynchronously (notably Job Object teardown on
    // Windows), so use the remaining shared cleanup budget rather than a
    // single instantaneous probe.
    let remaining = deadline.saturating_duration_since(Instant::now());
    match child.wait_tree_exit(remaining) {
        Ok(true) => {}
        Ok(false) => errors.push("git process tree did not exit before deadline".to_string()),
        Err(error) => errors.push(format!("git tree exit wait failed: {error}")),
    }

    if errors.is_empty() {
        Ok(status)
    } else {
        Err(errors.join("; "))
    }
}

fn run_git_bounded(
    path: &Path,
    args: &[&str],
    timeout: Duration,
    shutdown: Option<&AtomicBool>,
) -> Result<BoundedGitOutput, String> {
    run_git_bounded_with_program("git", path, args, timeout, shutdown)
}

/// Test seam over `run_git_bounded`: the program name is passed in instead of
/// being hardcoded to `"git"`, so lifecycle tests can drive a cross-platform
/// fixture binary through the same bounded tree lifecycle. Production always
/// calls [`run_git_bounded`], which passes `"git"`.
fn run_git_bounded_with_program(
    program: &str,
    path: &Path,
    args: &[&str],
    timeout: Duration,
    shutdown: Option<&AtomicBool>,
) -> Result<BoundedGitOutput, String> {
    if shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("git stopped during runner shutdown".to_string());
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // ManagedChild owns the whole Git process tree: a private process group on
    // Unix, a kill-on-close Job Object on Windows. Spawn remains direct process
    // spawning with the standard Command spawn failure semantics.
    let mut child = match ManagedChild::spawn(&mut command) {
        Ok(child) => child,
        Err(error) => return Err(format!("failed to spawn git: {error}")),
    };
    let Some(stdout) = child.child_mut().stdout.take() else {
        let cleanup_deadline = Instant::now() + PROJECT_GIT_CLEANUP_TIMEOUT;
        let _ = terminate_project_git_tree(&mut child, cleanup_deadline);
        return Err("git stdout pipe was unavailable".to_string());
    };
    let Some(stderr) = child.child_mut().stderr.take() else {
        drop(stdout);
        let cleanup_deadline = Instant::now() + PROJECT_GIT_CLEANUP_TIMEOUT;
        let _ = terminate_project_git_tree(&mut child, cleanup_deadline);
        return Err("git stderr pipe was unavailable".to_string());
    };
    let (stdout_rx, stdout_reader) = spawn_bounded_git_reader(stdout);
    let (stderr_rx, stderr_reader) = spawn_bounded_git_reader(stderr);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let stopping = shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst));
                if stopping || Instant::now() >= deadline {
                    // Cleanup and then report the stopping cause: the cleanup
                    // outcome is deliberately not allowed to replace the
                    // user-visible timeout/shutdown error.
                    let _ = terminate_project_git_tree(
                        &mut child,
                        Instant::now() + PROJECT_GIT_CLEANUP_TIMEOUT,
                    );
                    return Err(if stopping {
                        "git stopped during runner shutdown".to_string()
                    } else {
                        "git command timed out".to_string()
                    });
                }
                thread::sleep(
                    Duration::from_millis(10)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) => {
                let _ = terminate_project_git_tree(
                    &mut child,
                    Instant::now() + PROJECT_GIT_CLEANUP_TIMEOUT,
                );
                return Err(format!("failed to wait for git: {error}"));
            }
        }
    };

    // A helper descendant must not keep either pipe open after Git itself
    // exits. Direct-child exit alone is not tree exit: if descendants remain,
    // clean up the surviving tree, then drain the bounded readers — all within
    // one shared cleanup deadline so no operation gets a fresh independent one.
    let cleanup_deadline = Instant::now() + PROJECT_GIT_CLEANUP_TIMEOUT;
    match child.try_tree_exit() {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            let _ = terminate_project_git_tree(&mut child, cleanup_deadline);
        }
    }
    let stdout = stdout_rx
        .recv_timeout(cleanup_deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "git stdout reader timed out".to_string())?;
    let stderr = stderr_rx
        .recv_timeout(cleanup_deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "git stderr reader timed out".to_string())?;
    if stdout_reader.is_finished() {
        let _ = stdout_reader.join();
    }
    if stderr_reader.is_finished() {
        let _ = stderr_reader.join();
    }
    Ok(BoundedGitOutput {
        status,
        stdout: stdout.0,
        stderr: stderr.0,
        stdout_capped: stdout.1,
        stderr_capped: stderr.1,
    })
}

fn run_git_capture(path: &str, args: &[&str], shutdown: Option<&AtomicBool>) -> Option<String> {
    let output = run_git_bounded(Path::new(path), args, PROJECT_GIT_TIMEOUT, shutdown).ok()?;
    if !output.status.success() || output.stdout_capped {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn project_revision(project: &RunnerProjectFile) -> String {
    let normalized = toml::to_string(project).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(normalized.as_bytes()))
}

fn runner_project_summary_with_shutdown(
    project: &RunnerProjectFile,
    updated_at: i64,
    include_git: bool,
    shutdown: Option<&AtomicBool>,
) -> ShellAgentProjectSummary {
    let mut hooks = project.hooks.keys().cloned().collect::<Vec<_>>();
    hooks.sort();
    // The server uses the reported path as part of its repository continuity
    // identity. Report the actual root, not a mutable symlink alias, so a
    // retargeted project registration cannot inherit another repository's
    // current Workflow Session.
    let resolved_path = canonicalize_existing(Path::new(&project.path))
        .ok()
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from(&project.path));
    let resolved_path = resolved_path.to_string_lossy().to_string();
    let (git_branch, git_head, git_dirty) = if include_git {
        let branch = run_git_capture(
            &resolved_path,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            shutdown,
        );
        let head = run_git_capture(
            &resolved_path,
            &["log", "-1", "--pretty=format:%h"],
            shutdown,
        );
        let dirty = run_git_capture(&resolved_path, &["status", "--short"], shutdown)
            .map(|status| !status.trim().is_empty());
        (branch, head, dirty)
    } else {
        (None, None, None)
    };
    ShellAgentProjectSummary {
        id: project.id.clone(),
        name: project.name.clone().or_else(|| Some(project.id.clone())),
        path: resolved_path,
        allow_patch: project.allow_patch,
        kind: project.kind.clone(),
        description: project.description.clone(),
        hooks,
        disabled: project.disabled,
        revision: Some(project_revision(project)),
        git_branch,
        git_head,
        git_dirty,
        updated_at,
        shell_profile: project.shell_profile.clone(),
    }
}

#[cfg(test)]
pub(crate) fn runner_project_summary(
    project: &RunnerProjectFile,
    updated_at: i64,
    include_git: bool,
) -> ShellAgentProjectSummary {
    runner_project_summary_with_shutdown(project, updated_at, include_git, None)
}

fn warn_empty_hook_commands(source: &Path, project: &RunnerProjectFile) {
    for (hook, commands) in &project.hooks {
        for (idx, command) in commands.iter().enumerate() {
            if command.trim().is_empty() {
                eprintln!(
                    "webcodex-runner project warning: {} hook {} command {} is empty",
                    source.display(),
                    hook,
                    idx
                );
            }
        }
    }
}

fn load_runner_project_summaries_from_dir_with_shutdown(
    dir: &Path,
    shutdown: Option<&AtomicBool>,
) -> Vec<ShellAgentProjectSummary> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!(
                "webcodex-runner project warning: failed to read {}: {}",
                dir.display(),
                e
            );
            return Vec::new();
        }
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();

    let updated_at = chrono::Utc::now().timestamp();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        if shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            break;
        }
        let content = match std::fs::read_to_string(&file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "webcodex-runner project warning: failed to read {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        let project = match parse_runner_project_toml(&content) {
            Ok(project) => project,
            Err(e) => {
                eprintln!(
                    "webcodex-runner project warning: skipping {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        if !seen.insert(project.id.clone()) {
            eprintln!(
                "webcodex-runner project warning: duplicate project id {} in {}; skipping",
                project.id,
                file.display()
            );
            continue;
        }
        warn_empty_hook_commands(&file, &project);
        projects.push(runner_project_summary_with_shutdown(
            &project, updated_at, true, shutdown,
        ));
    }
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    projects
}

pub(crate) fn load_runner_project_summaries_from_dir(dir: &Path) -> Vec<ShellAgentProjectSummary> {
    load_runner_project_summaries_from_dir_with_shutdown(dir, None)
}

fn load_runner_project_summaries(
    cfg: &RunnerConfig,
    shutdown: Option<&AtomicBool>,
) -> Vec<ShellAgentProjectSummary> {
    // Loaded configs always carry a materialized project_registry_dir; a bare
    // test-built config that cannot derive one reports the error instead of
    // silently scanning a relative path.
    let dir = match project_registry_dir(cfg) {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("webcodex-runner: {error}");
            return Vec::new();
        }
    };
    load_runner_project_summaries_from_dir_with_shutdown(&dir, shutdown)
}

impl RunnerProjectCache {
    #[cfg(test)]
    pub(crate) fn get(&mut self, cfg: &RunnerConfig) -> Vec<ShellAgentProjectSummary> {
        self.get_with_shutdown(cfg, None)
    }

    pub(crate) fn get_with_shutdown(
        &mut self,
        cfg: &RunnerConfig,
        shutdown: Option<&AtomicBool>,
    ) -> Vec<ShellAgentProjectSummary> {
        if self.refreshed_at.is_some_and(|refreshed_at| {
            refreshed_at.elapsed() < Duration::from_millis(PROJECT_SCAN_CACHE_MS)
        }) {
            return self.projects.clone();
        }
        self.projects = load_runner_project_summaries(cfg, shutdown);
        self.refreshed_at = Some(Instant::now());
        self.projects.clone()
    }

    pub(crate) fn needs_refresh(&self) -> bool {
        self.refreshed_at.is_none()
    }

    pub(crate) fn invalidate(&mut self) {
        self.projects.clear();
        self.refreshed_at = None;
    }
}

/// Windows-only fail-closed rule: project roots must be on a local disk drive
/// (`C:\repo`, `D:\repo`, or the canonicalized `\\?\C:\repo` form). UNC
/// (`\\server\share\repo`), verbatim-UNC (`\\?\UNC\...`), device-namespace
/// (`\\.\...`) and every other non-disk Windows path prefix is rejected with
/// the stable `unc_project_path_unsupported` error before any filesystem
/// access happens.
///
/// The shared `webcodex_runner_config::paths::validate_project_path_ingress`
/// owns the grammar-based prefix rule; it never falls back to a string
/// `starts_with` check.
fn validate_windows_project_root(path: &Path) -> Result<(), &'static str> {
    webcodex_runner_config::paths::validate_project_path_ingress(path)
        .map_err(|_| "unc_project_path_unsupported")
}

/// Escape a string for use as a TOML basic string (double-quoted). NUL is
/// rejected up front by validation, so we only handle backslash, quote, and
/// common control characters.
fn toml_basic_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Build a deterministic project TOML string compatible with the existing
/// `parse_runner_project_toml` parser. The field order is fixed so the output
/// is reproducible.
fn build_project_toml(
    id: &str,
    name: &str,
    path: &str,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    build_project_toml_with_kind(id, name, path, None, description, allow_patch)
}

fn build_project_toml_with_kind(
    id: &str,
    name: &str,
    path: &str,
    kind: Option<&str>,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    let mut toml = String::new();
    toml.push_str(&format!("id = {}\n", toml_basic_string(id)));
    toml.push_str(&format!("name = {}\n", toml_basic_string(name)));
    toml.push_str(&format!("path = {}\n", toml_basic_string(path)));
    if let Some(kind) = kind {
        toml.push_str(&format!("kind = {}\n", toml_basic_string(kind)));
    }
    if let Some(desc) = description {
        toml.push_str(&format!("description = {}\n", toml_basic_string(desc)));
    }
    toml.push_str(&format!("allow_patch = {}\n", allow_patch));
    toml
}

/// Validate the project `id` for project-management operations. Stricter than
/// the existing `validate_project_id`: no dots (prevents any path-like
/// interpretation), only ASCII letters/digits/dash/underscore.
fn validate_project_op_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id.contains('\0') {
        return Err("id must not contain NUL".to_string());
    }
    if id.len() > 64 {
        return Err("id must be at most 64 characters".to_string());
    }
    if id.contains('/') || id.contains('\\') {
        return Err("id must not contain slash or backslash".to_string());
    }
    if id == ".." || id == "." || id.contains("..") {
        return Err("id must not contain dot-dot traversal".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(())
}

/// Validate the project `name`: non-empty after trim, <= 120 chars, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 characters".to_string());
    }
    Ok(())
}

/// A managed temporary project name is persisted as display metadata, never
/// used as a filesystem path. Still reject path-looking input at the Runner
/// boundary so callers cannot mistake it for a directory selector.
fn validate_managed_temporary_project_name(name: &str) -> Result<(), String> {
    validate_project_op_name(name)?;
    let name = name.trim();
    if name == "." || name == ".." || name.contains("..") {
        return Err("name must not contain dot-dot traversal".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("name must not contain slash or backslash".to_string());
    }
    Ok(())
}

/// Validate the optional `description`: <= 500 chars, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 characters".to_string());
    }
    Ok(())
}

/// Thin Runner adapter over the shared authoritative project-path policy.
/// Runner config loading has already materialized HOME-derived effective roots;
/// this layer only canonicalizes currently usable roots before applying the
/// same pure path semantics used by local onboarding.
pub(crate) fn validate_project_path_policy(
    policy: &RunnerPolicy,
    canonical_path: &Path,
) -> Result<(), String> {
    let canonical_roots = policy
        .allowed_roots
        .iter()
        .filter_map(|root| canonicalize_existing(root).ok())
        .collect::<Vec<_>>();
    webcodex_runner_config::paths::validate_project_path_policy(
        canonical_path,
        &canonical_roots,
        policy.allow_cwd_anywhere,
    )
}

#[derive(Debug, Clone)]
struct ProjectTomlWriteResult {
    config_path: PathBuf,
    created_config: bool,
    overwritten: bool,
}

#[derive(Debug)]
enum ProjectTomlWriteError {
    BeforeRename,
    AfterRename,
}

#[derive(Debug)]
enum ProjectUnregisterError {
    BeforeRename,
    AfterRename,
}

#[cfg(test)]
thread_local! {
    static FAIL_PARENT_SYNC_AFTER_PROJECT_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_PROJECT_PUBLISH_BEFORE_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn fail_next_project_parent_sync_after_rename() {
    FAIL_PARENT_SYNC_AFTER_PROJECT_RENAME.set(true);
}

#[cfg(test)]
pub(crate) fn fail_next_project_publish_before_rename() {
    FAIL_PROJECT_PUBLISH_BEFORE_RENAME.set(true);
}

fn sync_project_parent_after_rename(path: &Path) -> Result<(), String> {
    #[cfg(test)]
    if FAIL_PARENT_SYNC_AFTER_PROJECT_RENAME.replace(false) {
        return Err("injected parent directory sync failure".to_string());
    }
    sync_parent_dir(path)
}

/// Write a project TOML file atomically into `project_registry_dir`. Creates
/// `project_registry_dir` if missing. Returns write metadata on success.
/// The temp file is written and fsynced, then atomically published as
/// `<id>.toml`.
fn sync_parent_dir(path: &Path) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "project config has no parent".to_string())?;
    sync_dir(dir)
}

fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Opening a directory with `File::open` fails on Windows (it needs
        // FILE_FLAG_BACKUP_SEMANTICS, which std does not expose), and NTFS
        // metadata durability does not rely on directory fsync the way
        // POSIX filesystems do. The rename is already atomic; skip the
        // directory sync here.
        let _ = dir;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open(dir)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("failed to sync project registry directory: {e}"))
    }
}

fn unique_registry_temp(dir: &Path, id: &str, suffix: &str) -> PathBuf {
    dir.join(format!(".{id}.{}.{}", uuid::Uuid::new_v4(), suffix))
}

fn write_project_toml_atomic(
    project_registry_dir: &Path,
    id: &str,
    toml_content: &str,
    overwrite: bool,
) -> Result<ProjectTomlWriteResult, ProjectTomlWriteError> {
    std::fs::create_dir_all(project_registry_dir)
        .map_err(|_| ProjectTomlWriteError::BeforeRename)?;
    let canonical_dir = canonicalize_existing(project_registry_dir)
        .map_err(|_| ProjectTomlWriteError::BeforeRename)?;
    let config_path = canonical_dir.join(format!("{id}.toml"));
    if !config_path.starts_with(&canonical_dir) {
        return Err(ProjectTomlWriteError::BeforeRename);
    }
    let existed_before = config_path.exists();
    if existed_before && !overwrite {
        return Err(ProjectTomlWriteError::BeforeRename);
    }
    let temp_path = unique_registry_temp(&canonical_dir, id, "toml.tmp");
    let mut published = false;
    let before = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(toml_content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        #[cfg(test)]
        if FAIL_PROJECT_PUBLISH_BEFORE_RENAME.replace(false) {
            return Err("injected project publish failure".to_string());
        }
        if overwrite {
            std::fs::rename(&temp_path, &config_path).map_err(|e| e.to_string())?;
            published = true;
        } else {
            // Publish a complete, synced same-directory temp file without the
            // overwrite-on-rename race. A concurrent creator wins cleanly and
            // the caller can rescan the registry to converge.
            std::fs::hard_link(&temp_path, &config_path).map_err(|e| e.to_string())?;
            published = true;
            std::fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if before.is_err() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(if published {
            ProjectTomlWriteError::AfterRename
        } else {
            ProjectTomlWriteError::BeforeRename
        });
    }
    sync_project_parent_after_rename(&config_path)
        .map_err(|_| ProjectTomlWriteError::AfterRename)?;
    Ok(ProjectTomlWriteResult {
        config_path,
        created_config: !existed_before,
        overwritten: existed_before && overwrite,
    })
}

fn load_project_files_for_path_resolution(
    project_registry_dir: &Path,
) -> Result<Vec<RunnerProjectFile>, &'static str> {
    let entries = match std::fs::read_dir(project_registry_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("project_registry_unavailable"),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "project_registry_unavailable")?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();

    let mut projects = Vec::with_capacity(files.len());
    for file in files {
        let content = std::fs::read_to_string(&file).map_err(|_| "project_registry_unavailable")?;
        let project =
            parse_runner_project_toml(&content).map_err(|_| "project_registry_unavailable")?;
        projects.push(project);
    }
    Ok(projects)
}

fn projects_matching_canonical_path(
    projects: &[RunnerProjectFile],
    canonical_path: &Path,
) -> Vec<RunnerProjectFile> {
    projects
        .iter()
        .filter_map(|project| {
            let registered_path = canonicalize_existing(Path::new(&project.path)).ok()?;
            (registered_path.is_dir() && paths_equal(&registered_path, canonical_path))
                .then(|| project.clone())
        })
        .collect()
}

fn bounded_project_name(canonical_path: &Path) -> String {
    let raw = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Project")
        .trim();
    let mut name = String::new();
    for character in raw.chars() {
        if name.len() + character.len_utf8() > 120 {
            break;
        }
        name.push(character);
    }
    if name.is_empty() {
        "Project".to_string()
    } else {
        name
    }
}

fn sanitized_project_basename(canonical_path: &Path) -> String {
    let raw = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let mut sanitized = String::new();
    let mut separator_pending = false;
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !sanitized.is_empty() {
                sanitized.push('-');
            }
            sanitized.push(character.to_ascii_lowercase());
            separator_pending = false;
        } else {
            separator_pending = true;
        }
    }
    if sanitized.is_empty() {
        "project".to_string()
    } else {
        sanitized
    }
}

fn canonical_project_path_hash(canonical_path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        format!(
            "{:x}",
            Sha256::digest(canonical_path.as_os_str().as_bytes())
        )
    }
    #[cfg(not(unix))]
    format!(
        "{:x}",
        Sha256::digest(
            webcodex_runner_config::paths::normalize_path_identity(canonical_path).as_bytes()
        )
    )
}

fn auto_project_id_candidate(
    canonical_path: &Path,
    hash_prefix_length: usize,
) -> Result<String, &'static str> {
    let digest = canonical_project_path_hash(canonical_path);
    let hash_prefix = digest
        .get(..hash_prefix_length.min(digest.len()))
        .ok_or("project_id_collision")?;
    let max_basename_length = 64usize.saturating_sub(hash_prefix.len() + 1);
    if max_basename_length == 0 {
        return Err("project_id_collision");
    }
    let basename = sanitized_project_basename(canonical_path);
    let basename = basename
        .chars()
        .take(max_basename_length)
        .collect::<String>();
    let candidate = format!("{basename}-{hash_prefix}");
    validate_project_op_id(&candidate).map_err(|_| "project_id_collision")?;
    Ok(candidate)
}

fn choose_auto_project_id(
    project_registry_dir: &Path,
    projects: &[RunnerProjectFile],
    canonical_path: &Path,
) -> Result<String, &'static str> {
    let configured_ids = projects
        .iter()
        .map(|project| project.id.as_str())
        .collect::<HashSet<_>>();
    for &prefix_length in AUTO_PROJECT_HASH_PREFIX_LENGTHS {
        let candidate = auto_project_id_candidate(canonical_path, prefix_length)?;
        if configured_ids.contains(candidate.as_str())
            || project_registry_dir
                .join(format!("{candidate}.toml"))
                .exists()
        {
            continue;
        }
        return Ok(candidate);
    }
    Err("project_id_collision")
}

fn path_resolution_success(
    request: &ShellAgentShellRequest,
    project: &RunnerProjectFile,
    canonical_path: &Path,
    outcome: &'static str,
    registered: bool,
    project_record_path: Option<&Path>,
) -> serde_json::Value {
    serde_json::json!({
        "id": format!("agent:{}:{}", request.client_id, project.id),
        "agent_project_id": project.id,
        "client_id": request.client_id,
        "name": project.name,
        "path": canonical_path.to_string_lossy(),
        "kind": project.kind,
        "description": project.description,
        "allow_patch": project.allow_patch,
        "disabled": project.disabled,
        "revision": project_revision(project),
        "source": "path",
        "outcome": outcome,
        "registered": registered,
        "created_config": registered,
        "changed": registered,
        "recovered": !registered,
        "project_record_path": project_record_path.map(|path| path.to_string_lossy().to_string()),
        "projects_config_path": project_record_path.map(|path| path.to_string_lossy().to_string()),
    })
}

fn existing_path_resolution_result(
    start: Instant,
    request: &ShellAgentShellRequest,
    canonical_path: &Path,
    matches: Vec<RunnerProjectFile>,
) -> Option<CommandResult> {
    if matches.len() > 1 {
        let mut matching_project_ids = matches
            .iter()
            .map(|project| project.id.clone())
            .collect::<Vec<_>>();
        matching_project_ids.sort();
        matching_project_ids.dedup();
        return Some(structured_project_error_cmd(
            start,
            "ambiguous_project_path",
            false,
            serde_json::json!({"matching_project_ids": matching_project_ids}),
        ));
    }
    let project = matches.into_iter().next()?;
    if project.disabled {
        return Some(structured_project_error_cmd(
            start,
            "project_disabled",
            false,
            serde_json::json!({"matching_project_id": project.id}),
        ));
    }
    Some(ok_cmd(
        start,
        path_resolution_success(
            request,
            &project,
            canonical_path,
            "reused_existing_registration",
            false,
            None,
        ),
    ))
}

/// Resolve an existing Runner registration by canonical path or atomically
/// persist a new one. This is an internal Server↔Runner operation, not a
/// model-visible runtime tool.
pub(crate) fn handle_resolve_or_register_project(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let start = Instant::now();
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return structured_project_error_cmd(
                start,
                "operation_failed",
                false,
                serde_json::json!({}),
            )
        }
    };
    let payload = match request
        .stdin
        .as_deref()
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
        .and_then(|payload| payload.as_object().cloned())
    {
        Some(payload) => payload,
        None => {
            return structured_project_error_cmd(
                start,
                "invalid_request",
                false,
                serde_json::json!({}),
            )
        }
    };
    if payload.len() != 1 {
        return structured_project_error_cmd(
            start,
            "invalid_request",
            false,
            serde_json::json!({}),
        );
    }
    let path = match payload.get("path").and_then(serde_json::Value::as_str) {
        Some(path) if !path.is_empty() && !path.contains('\0') && Path::new(path).is_absolute() => {
            path
        }
        _ => {
            return structured_project_error_cmd(
                start,
                "invalid_project_path",
                false,
                serde_json::json!({"field": "path"}),
            )
        }
    };
    // The raw input path is checked before any filesystem access so a UNC
    // path is rejected as `unc_project_path_unsupported` even when the share
    // is unreachable (which would otherwise surface as `project_path_not_found`).
    if let Err(error_kind) = validate_windows_project_root(Path::new(path)) {
        return structured_project_error_cmd(
            start,
            error_kind,
            false,
            serde_json::json!({"field": "path"}),
        );
    }
    let canonical_path = match canonicalize_existing(Path::new(path)) {
        Ok(path) => path,
        Err(_) => {
            return structured_project_error_cmd(
                start,
                "project_path_not_found",
                false,
                serde_json::json!({"field": "path"}),
            )
        }
    };
    // The canonical form is checked too: Windows canonicalization rewrites
    // reachable UNC paths into `\\?\UNC\...`, which the raw check may not
    // have seen verbatim.
    if let Err(error_kind) = validate_windows_project_root(&canonical_path) {
        return structured_project_error_cmd(
            start,
            error_kind,
            false,
            serde_json::json!({"field": "path"}),
        );
    }
    if !canonical_path.is_dir() {
        return structured_project_error_cmd(
            start,
            "project_path_not_directory",
            false,
            serde_json::json!({"field": "path"}),
        );
    }
    if canonical_path.to_str().is_none() {
        return structured_project_error_cmd(
            start,
            "invalid_project_path",
            false,
            serde_json::json!({"field": "path"}),
        );
    }
    if validate_project_path_policy(policy, &canonical_path).is_err() {
        return structured_project_error_cmd(
            start,
            "path_outside_allowed_roots",
            false,
            serde_json::json!({"field": "path"}),
        );
    }

    let projects = match load_project_files_for_path_resolution(project_registry_dir) {
        Ok(projects) => projects,
        Err(error_kind) => {
            return structured_project_error_cmd(start, error_kind, false, serde_json::json!({}))
        }
    };
    let matches = projects_matching_canonical_path(&projects, &canonical_path);
    if let Some(result) = existing_path_resolution_result(start, request, &canonical_path, matches)
    {
        return result;
    }

    let project_id = match choose_auto_project_id(project_registry_dir, &projects, &canonical_path)
    {
        Ok(project_id) => project_id,
        Err(error_kind) => {
            return structured_project_error_cmd(start, error_kind, false, serde_json::json!({}))
        }
    };
    let canonical_path_string = canonical_path
        .to_str()
        .expect("validated UTF-8 canonical project path")
        .to_string();
    let name = bounded_project_name(&canonical_path);
    let description = None;
    let toml_content = build_project_toml_with_kind(
        &project_id,
        &name,
        &canonical_path_string,
        Some(AUTO_REGISTERED_PROJECT_KIND),
        &description,
        true,
    );
    let write_result =
        match write_project_toml_atomic(project_registry_dir, &project_id, &toml_content, false) {
            Ok(result) => result,
            Err(ProjectTomlWriteError::BeforeRename) => {
                // A different process may have won publication. Rescan under
                // our process-local lock and converge if it registered the
                // same canonical directory.
                if let Ok(projects) = load_project_files_for_path_resolution(project_registry_dir) {
                    let matches = projects_matching_canonical_path(&projects, &canonical_path);
                    if let Some(result) =
                        existing_path_resolution_result(start, request, &canonical_path, matches)
                    {
                        return result;
                    }
                }
                return structured_project_error_cmd(
                    start,
                    "operation_failed",
                    false,
                    serde_json::json!({}),
                );
            }
            Err(ProjectTomlWriteError::AfterRename) => {
                return structured_project_error_cmd(
                    start,
                    "operation_indeterminate",
                    true,
                    serde_json::json!({}),
                )
            }
        };
    let project = match parse_runner_project_toml(&toml_content) {
        Ok(project) => project,
        Err(_) => {
            return structured_project_error_cmd(
                start,
                "operation_indeterminate",
                true,
                serde_json::json!({}),
            )
        }
    };
    ok_cmd(
        start,
        path_resolution_success(
            request,
            &project,
            &canonical_path,
            "auto_registered",
            true,
            Some(&write_result.config_path),
        ),
    )
}

fn lifecycle_config_path(project_registry_dir: &Path, id: &str) -> Result<PathBuf, String> {
    validate_project_op_id(id)?;
    let canonical_dir = canonicalize_existing(project_registry_dir)?;
    let path = canonical_dir.join(format!("{id}.toml"));
    if !path.starts_with(&canonical_dir) {
        return Err("project config path would escape project_registry_dir".to_string());
    }
    Ok(path)
}

fn write_existing_project_atomic(path: &Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "project config has no parent".to_string())?;
    let id = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("project");
    let temp = unique_registry_temp(dir, id, "toml.tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&temp)
            .map_err(|e| format!("failed to create lifecycle temp file: {e}"))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write lifecycle temp file: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync lifecycle temp file: {e}"))?;
        std::fs::rename(&temp, path)
            .map_err(|e| format!("failed to atomically replace project config: {e}"))?;
        sync_parent_dir(path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn cleanup_unregister_tombstones(project_registry_dir: &Path, id: &str) -> Result<(), String> {
    let prefix = format!(".{id}.");
    let suffix = ".toml.unregistering";
    let mut changed = false;
    for entry in std::fs::read_dir(project_registry_dir)
        .map_err(|e| format!("failed to inspect project registry tombstones: {e}"))?
    {
        let entry = entry.map_err(|e| format!("failed to inspect project registry entry: {e}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(suffix) {
            std::fs::remove_file(entry.path())
                .map_err(|e| format!("failed to remove stale unregister tombstone: {e}"))?;
            changed = true;
        }
    }
    if changed {
        sync_dir(project_registry_dir)?;
    }
    Ok(())
}

fn unregister_project_config(path: &Path) -> Result<(), ProjectUnregisterError> {
    let dir = path.parent().ok_or(ProjectUnregisterError::BeforeRename)?;
    let id = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("project");
    let tombstone = unique_registry_temp(dir, id, "toml.unregistering");
    std::fs::rename(path, &tombstone).map_err(|_| ProjectUnregisterError::BeforeRename)?;
    sync_project_parent_after_rename(path).map_err(|_| ProjectUnregisterError::AfterRename)?;
    std::fs::remove_file(&tombstone).map_err(|_| ProjectUnregisterError::AfterRename)?;
    sync_project_parent_after_rename(path).map_err(|_| ProjectUnregisterError::AfterRename)
}

/// Structured, non-shell project lifecycle mutation. Unregister only removes
/// the registry TOML and never touches the project path or Git data.
pub(crate) fn handle_project_lifecycle_op(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return project_error_cmd(Instant::now(), "operation_failed"),
    };
    let start = Instant::now();
    let action = request
        .kind
        .strip_prefix("project_lifecycle_")
        .unwrap_or("");
    if !matches!(action, "enable" | "disable" | "unregister") {
        return project_error_cmd(start, "unsupported_runner_version");
    }
    let payload: serde_json::Value = match request
        .stdin
        .as_deref()
        .and_then(|v| serde_json::from_str(v).ok())
    {
        Some(v) => v,
        None => return project_error_cmd(start, "invalid_request"),
    };
    let id = match payload.get("project_id").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return project_error_cmd(start, "invalid_request"),
    };
    let expected_revision = match payload.get("expected_revision").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return project_error_cmd(start, "invalid_request"),
    };
    let config_path = match lifecycle_config_path(project_registry_dir, id) {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    if !config_path.exists() {
        if action == "unregister" {
            if cleanup_unregister_tombstones(project_registry_dir, id).is_err() {
                return project_error_cmd(start, "operation_failed");
            }
            return ok_cmd(
                start,
                serde_json::json!({
                    "operation": action, "agent_project_id": id,
                    "outcome": "already_unregistered", "changed": false,
                    "revision": serde_json::Value::Null
                }),
            );
        }
        return project_error_cmd(start, "project_not_found");
    }
    let content = match std::fs::read_to_string(&config_path) {
        Ok(v) => v,
        Err(_) => return project_error_cmd(start, "operation_failed"),
    };
    let mut project = match parse_runner_project_toml(&content) {
        Ok(v) => v,
        Err(_) => return project_error_cmd(start, "operation_failed"),
    };
    let current_revision = project_revision(&project);
    let desired_disabled = action == "disable";
    if action != "unregister" && project.disabled == desired_disabled {
        return ok_cmd(
            start,
            serde_json::json!({
                "operation": action, "agent_project_id": id,
                "outcome": if desired_disabled {"already_disabled"} else {"already_enabled"},
                "changed": false, "revision": current_revision,
                "disabled": project.disabled, "path": project.path,
                "name": project.name, "description": project.description,
                "allow_patch": project.allow_patch
            }),
        );
    }
    if expected_revision != current_revision {
        return project_error_cmd(start, "revision_conflict");
    }
    if action == "unregister" {
        match unregister_project_config(&config_path) {
            Ok(()) => {}
            Err(ProjectUnregisterError::BeforeRename) => {
                return project_error_cmd(start, "operation_failed")
            }
            Err(ProjectUnregisterError::AfterRename) => {
                return structured_project_error_cmd(
                    start,
                    "operation_indeterminate",
                    true,
                    serde_json::json!({}),
                )
            }
        }
        return ok_cmd(
            start,
            serde_json::json!({
                "operation": action, "agent_project_id": id,
                "outcome": "unregistered", "changed": true,
                "revision": serde_json::Value::Null
            }),
        );
    }
    if !desired_disabled {
        let canonical = match canonicalize_existing(Path::new(&project.path)) {
            Ok(v) if v.is_dir() => v,
            _ => return project_error_cmd(start, "project_not_found"),
        };
        if let Err(error_kind) = validate_windows_project_root(&canonical) {
            return project_error_cmd(start, error_kind);
        }
        if validate_project_path_policy(policy, &canonical).is_err() {
            return project_error_cmd(start, "path_outside_allowed_roots");
        }
    }
    project.disabled = desired_disabled;
    let serialized = match toml::to_string_pretty(&project) {
        Ok(v) => v,
        Err(_) => return project_error_cmd(start, "operation_failed"),
    };
    if write_existing_project_atomic(&config_path, &serialized).is_err() {
        return project_error_cmd(start, "operation_failed");
    }
    let revision = project_revision(&project);
    ok_cmd(
        start,
        serde_json::json!({
            "operation": action, "agent_project_id": id,
            "outcome": if desired_disabled {"disabled"} else {"enabled"},
            "changed": true, "revision": revision,
            "disabled": project.disabled, "path": project.path,
            "name": project.name, "description": project.description,
            "allow_patch": project.allow_patch
        }),
    )
}

fn matching_existing_project(
    project_registry_dir: &Path,
    id: &str,
    name: &str,
    path: &str,
    description: Option<&str>,
    allow_patch: bool,
) -> Result<Option<RunnerProjectFile>, &'static str> {
    let config_path = project_registry_dir.join(format!("{id}.toml"));
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&config_path).map_err(|_| "operation_failed")?;
    let project = parse_runner_project_toml(&content).map_err(|_| "operation_failed")?;
    let matches = project.id == id
        && paths_equal(Path::new(&project.path), Path::new(path))
        && project.name.as_deref() == Some(name)
        && project.description.as_deref() == description
        && project.allow_patch == allow_patch
        && !project.disabled;
    if matches {
        Ok(Some(project))
    } else {
        Err("project_already_exists")
    }
}

fn validate_recovered_create_side_effects(
    path: &Path,
    template: &str,
    description: Option<&str>,
    git_init: bool,
) -> Result<(), &'static str> {
    if !path.is_dir() {
        return Err("project_already_exists");
    }
    if git_init && !path.join(".git").is_dir() {
        return Err("project_already_exists");
    }
    if template == "basic"
        && (!path.join("README.md").is_file() || !path.join(".gitignore").is_file())
    {
        return Err("project_already_exists");
    }
    if template == "empty" && description.is_some() && !path.join("README.md").is_file() {
        return Err("project_already_exists");
    }
    Ok(())
}

fn recovered_project_result(
    kind: &str,
    runtime_id: &str,
    client_id: &str,
    project: &RunnerProjectFile,
    template: Option<&str>,
    git_init: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": runtime_id, "agent_project_id": project.id, "client_id": client_id,
        "name": project.name, "path": project.path, "description": project.description,
        "created_directory": false, "created_config": false, "overwritten": false,
        "allow_patch": project.allow_patch, "template": template,
        "git_initialized": git_init, "recovered": true, "changed": false,
        "operation": if kind == "create_project" { "create" } else { "register" },
        "outcome": if kind == "create_project" { "created" } else { "registered" },
        "revision": project_revision(project),
    })
}

/// Create and persist one Runner-managed temporary project. The directory name
/// and project id are generated here, never accepted from the server, and the
/// canonical result must be exactly one direct child of the configured root.
///
/// TODO: add an explicit retention policy plus a safe managed-project deletion
/// path that re-verifies this kind and root before removing anything.
fn handle_managed_temporary_project(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    temporary_projects_root: Option<&Path>,
    request: &ShellAgentShellRequest,
    json: &serde_json::Value,
    start: Instant,
) -> CommandResult {
    // This internal request accepts no caller-selected directory/id or
    // create-project behavior. Rejecting those fields makes the generated
    // direct-child invariant explicit even if a future caller bypasses the
    // public start_coding_task schema.
    if [
        "id",
        "path",
        "description",
        "allow_patch",
        "template",
        "git_init",
        "allow_existing_empty",
        "overwrite",
    ]
    .iter()
    .any(|field| json.get(*field).is_some())
    {
        return project_error_cmd(start, "invalid_request");
    }
    let name = match json.get("name") {
        None | Some(serde_json::Value::Null) => DEFAULT_MANAGED_TEMPORARY_PROJECT_NAME.to_string(),
        Some(serde_json::Value::String(value)) => {
            if validate_managed_temporary_project_name(value).is_err() {
                return project_error_cmd(start, "invalid_request");
            }
            value.trim().to_string()
        }
        Some(_) => return project_error_cmd(start, "invalid_request"),
    };
    let Some(temporary_projects_root) = temporary_projects_root else {
        return project_error_cmd(start, "temporary_projects_not_configured");
    };
    let canonical_root = match canonicalize_existing(temporary_projects_root) {
        Ok(root) if root.is_dir() => root,
        _ => return project_error_cmd(start, "temporary_projects_root_unavailable"),
    };
    if let Err(error_kind) = validate_windows_project_root(&canonical_root) {
        return project_error_cmd(start, error_kind);
    }
    if validate_project_path_policy(policy, &canonical_root).is_err() {
        return project_error_cmd(start, "temporary_projects_root_outside_allowed_roots");
    }

    for _ in 0..MANAGED_TEMPORARY_PROJECT_CREATE_ATTEMPTS {
        let id = format!(
            "{MANAGED_TEMPORARY_PROJECT_ID_PREFIX}-{}",
            uuid::Uuid::new_v4()
        );
        if project_registry_dir.join(format!("{id}.toml")).exists() {
            continue;
        }
        let requested_path = canonical_root.join(&id);
        match std::fs::create_dir(&requested_path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return project_error_cmd(start, "temporary_project_create_failed"),
        }
        let canonical_path = match canonicalize_existing(&requested_path) {
            Ok(path) if path.is_dir() && path.parent() == Some(canonical_root.as_path()) => path,
            _ => return project_error_cmd(start, "temporary_project_path_escape"),
        };
        let path = canonical_path.to_string_lossy().to_string();
        match run_git_bounded(&canonical_path, &["init"], Duration::from_secs(5), None) {
            Ok(output) if output.status.success() => {}
            Ok(_) | Err(_) => {
                let _ = std::fs::remove_dir_all(&canonical_path);
                return project_error_cmd(start, "temporary_project_git_init_failed");
            }
        }
        let description = None;
        let toml_content = build_project_toml_with_kind(
            &id,
            &name,
            &path,
            Some(MANAGED_TEMPORARY_PROJECT_KIND),
            &description,
            true,
        );
        let write_result =
            match write_project_toml_atomic(project_registry_dir, &id, &toml_content, false) {
                Ok(result) => result,
                Err(ProjectTomlWriteError::BeforeRename) => {
                    // The directory is a newly created direct child of the managed
                    // root and only contains the Git metadata initialized above.
                    let _ = std::fs::remove_dir_all(&canonical_path);
                    return project_error_cmd(start, "operation_failed");
                }
                Err(ProjectTomlWriteError::AfterRename) => {
                    return project_error_cmd(start, "operation_indeterminate");
                }
            };
        let project = parse_runner_project_toml(&toml_content)
            .expect("generated managed temporary project TOML must parse");
        return ok_cmd(
            start,
            serde_json::json!({
                "id": format!("agent:{}:{}", request.client_id, id),
                "agent_project_id": id,
                "client_id": request.client_id,
                "name": name,
                "path": path,
                "description": serde_json::Value::Null,
                "kind": MANAGED_TEMPORARY_PROJECT_KIND,
                "source": MANAGED_TEMPORARY_PROJECT_KIND,
                "managed_temporary": true,
                "project_record_path": write_result.config_path.to_string_lossy(),
                "projects_config_path": write_result.config_path.to_string_lossy(),
                "created_directory": true,
                "created_config": write_result.created_config,
                "overwritten": false,
                "allow_patch": true,
                "template": "empty",
                "git_initialized": true,
                "revision": project_revision(&project),
                "operation": "create",
                "outcome": "created",
                "changed": true,
                "recovered": false,
            }),
        );
    }
    project_error_cmd(start, "temporary_project_name_collision")
}

/// Handle `register_project` / `create_project` agent requests. Parses the
/// JSON payload from `request.stdin`, validates fields and path against
/// policy, writes `project_registry_dir/<id>.toml` atomically (and for
/// `create_project` creates the directory / templates / optional git init),
/// and returns structured JSON in `CommandResult.stdout`.
#[cfg(test)]
pub(crate) fn handle_project_op(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    handle_project_op_with_temporary_projects_root(policy, project_registry_dir, None, request)
}

pub(crate) fn handle_project_op_with_temporary_projects_root(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    temporary_projects_root: Option<&Path>,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return project_error_cmd(Instant::now(), "operation_failed"),
    };
    let start = Instant::now();
    let kind = request.kind.as_str();
    let payload = match request.stdin.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("{} request missing stdin payload", kind)),
            };
        }
    };
    let json: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to parse {} payload: {}", kind, e)),
            };
        }
    };
    if kind == "create_project"
        && json
            .get("managed_temporary_project")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return handle_managed_temporary_project(
            policy,
            project_registry_dir,
            temporary_projects_root,
            request,
            &json,
            start,
        );
    }
    let get_str = |key: &str| -> Result<String, String> {
        json.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{} missing required field '{}'", kind, key))
    };
    let id = match get_str("id") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let name = match get_str("name") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let path = match get_str("path") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let description = json
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let allow_patch = json
        .get("allow_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let overwrite = json
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if let Err(e) = validate_project_op_id(&id) {
        return err_cmd(start, e);
    }
    if let Err(e) = validate_project_op_name(&name) {
        return err_cmd(start, e);
    }
    if let Some(ref desc) = description {
        if let Err(e) = validate_project_op_description(desc) {
            return err_cmd(start, e);
        }
    }
    // `Path::is_absolute` is platform-correct: drive-letter and UNC paths
    // (`C:\foo`, `\\server\share`) are absolute on Windows; bare `foo` or
    // drive-relative `/foo` are not.
    if path.is_empty() || path.contains('\0') || !Path::new(&path).is_absolute() {
        return err_cmd(start, "path must be a non-empty absolute path".to_string());
    }
    // Windows supports local-drive project roots only; UNC and other
    // non-disk prefixes fail closed here, before the directory is touched,
    // so an unreachable share cannot masquerade as a missing directory.
    if let Err(error_kind) = validate_windows_project_root(Path::new(&path)) {
        return project_error_cmd(start, error_kind);
    }

    let client_id = request.client_id.clone();
    let runtime_id = format!("agent:{}:{}", client_id, id);

    let toml_content = build_project_toml(&id, &name, &path, &description, allow_patch);
    let template = json
        .get("template")
        .and_then(|v| v.as_str())
        .unwrap_or("empty")
        .to_string();
    let git_init = json
        .get("git_init")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let allow_existing_empty = json
        .get("allow_existing_empty")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if kind == "create_project" && template != "empty" && template != "basic" {
        return project_error_cmd(start, "invalid_request");
    }

    if kind == "register_project" {
        // The directory must exist and be a directory.
        let path_buf = PathBuf::from(&path);
        let canonical = match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "path does not exist or cannot be canonicalized: {}: {}",
                        path, e
                    ),
                );
            }
        };
        if !canonical.is_dir() {
            return err_cmd(start, format!("path {} is not a directory", path));
        }
        if let Err(error_kind) = validate_windows_project_root(&canonical) {
            return project_error_cmd(start, error_kind);
        }
        if validate_project_path_policy(policy, &canonical).is_err() {
            return project_error_cmd(start, "path_outside_allowed_roots");
        }
        if !overwrite {
            match matching_existing_project(
                project_registry_dir,
                &id,
                &name,
                &path,
                description.as_deref(),
                allow_patch,
            ) {
                Ok(Some(project)) => {
                    return ok_cmd(
                        start,
                        recovered_project_result(
                            kind,
                            &runtime_id,
                            &client_id,
                            &project,
                            None,
                            false,
                        ),
                    )
                }
                Ok(None) => {}
                Err(code) => return project_error_cmd(start, code),
            }
        }
        let write_result =
            match write_project_toml_atomic(project_registry_dir, &id, &toml_content, overwrite) {
                Ok(p) => p,
                Err(ProjectTomlWriteError::BeforeRename) => {
                    return project_error_cmd(start, "operation_failed")
                }
                Err(ProjectTomlWriteError::AfterRename) => {
                    return project_error_cmd(start, "operation_indeterminate")
                }
            };
        let result = serde_json::json!({
            "id": runtime_id,
            "agent_project_id": id,
            "client_id": client_id,
            "name": name,
            "path": path,
            "description": description,
            "project_record_path": write_result.config_path.to_string_lossy(),
            "projects_config_path": write_result.config_path.to_string_lossy(),
            "created_config": write_result.created_config,
            "overwritten": write_result.overwritten,
            "allow_patch": allow_patch,
            "revision": project_revision(&parse_runner_project_toml(&toml_content).expect("generated project TOML must parse")),
            "operation": "register", "outcome": "registered", "changed": true, "recovered": false,
        });
        return ok_cmd(start, result);
    }

    // create_project
    let path_buf = PathBuf::from(&path);
    let mut created_directory = false;
    let mut created_paths = CreatedProjectPaths::default();

    // Determine the canonical parent for policy validation. If the path exists,
    // canonicalize it directly. If not, canonicalize the existing ancestor.
    let canonical_for_policy = if path_buf.exists() {
        match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!("path cannot be canonicalized: {}: {}", path, e),
                );
            }
        }
    } else {
        // Find the nearest existing ancestor and canonicalize it.
        let mut ancestor = path_buf.clone();
        while !ancestor.exists() {
            if let Some(parent) = ancestor.parent() {
                ancestor = parent.to_path_buf();
            } else {
                break;
            }
        }
        match ancestor.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "parent path cannot be canonicalized: {}: {}",
                        ancestor.display(),
                        e
                    ),
                );
            }
        }
    };
    if let Err(error_kind) = validate_windows_project_root(&canonical_for_policy) {
        return project_error_cmd(start, error_kind);
    }
    if validate_project_path_policy(policy, &canonical_for_policy).is_err() {
        return project_error_cmd(start, "path_outside_allowed_roots");
    }
    if !overwrite {
        match matching_existing_project(
            project_registry_dir,
            &id,
            &name,
            &path,
            description.as_deref(),
            allow_patch,
        ) {
            Ok(Some(project)) => {
                if let Err(code) = validate_recovered_create_side_effects(
                    &path_buf,
                    &template,
                    description.as_deref(),
                    git_init,
                ) {
                    return project_error_cmd(start, code);
                }
                return ok_cmd(
                    start,
                    recovered_project_result(
                        kind,
                        &runtime_id,
                        &client_id,
                        &project,
                        Some(&template),
                        git_init,
                    ),
                );
            }
            Ok(None) => {}
            Err(code) => return project_error_cmd(start, code),
        }
    }

    // Handle existing vs new directory.
    if path_buf.exists() {
        let meta = match std::fs::metadata(&path_buf) {
            Ok(m) => m,
            Err(e) => return err_cmd(start, format!("failed to stat path {}: {}", path, e)),
        };
        if !meta.is_dir() {
            return err_cmd(
                start,
                format!("path {} exists but is not a directory", path),
            );
        }
        // Check if the directory is empty.
        let is_empty = match std::fs::read_dir(&path_buf) {
            Ok(mut it) => it.next().is_none(),
            Err(e) => {
                return err_cmd(start, format!("failed to read directory {}: {}", path, e));
            }
        };
        if !is_empty {
            return project_error_cmd(start, "path_not_empty");
        }
        if !allow_existing_empty {
            return project_error_cmd(start, "path_not_empty");
        }
    } else {
        // Create the directory.
        if let Err(e) = std::fs::create_dir_all(&path_buf) {
            return err_cmd(start, format!("failed to create directory {}: {}", path, e));
        }
        created_directory = true;
        created_paths.mark_project_dir_created(path_buf.clone());
    }

    // Apply template.
    if template == "basic" {
        let readme = if let Some(ref desc) = description {
            format!("# {}\n\n{}\n", name, desc)
        } else {
            format!("# {}\n", name)
        };
        let readme_path = path_buf.join("README.md");
        if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths) {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write README.md: {}", e));
        }
        let gitignore = "target/\nnode_modules/\n.env\n*.log\n";
        let gitignore_path = path_buf.join(".gitignore");
        if let Err(e) =
            write_created_file(&gitignore_path, gitignore.as_bytes(), &mut created_paths)
        {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write .gitignore: {}", e));
        }
    } else if template == "empty" {
        // For empty template, optionally create README.md if description is provided.
        if let Some(ref desc) = description {
            let readme = format!("# {}\n\n{}\n", name, desc);
            let readme_path = path_buf.join("README.md");
            if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths)
            {
                created_paths.cleanup();
                return err_cmd(start, format!("failed to write README.md: {}", e));
            }
        }
    }

    // git init.
    let mut git_initialized = false;
    if git_init {
        match run_git_bounded(&path_buf, &["init"], Duration::from_secs(5), None) {
            Ok(output) if output.status.success() => {
                git_initialized = true;
                created_paths.track(path_buf.join(".git"));
            }
            Ok(output) => {
                created_paths.cleanup();
                let stderr = String::from_utf8_lossy(&output.stderr);
                let suffix = if output.stderr_capped {
                    " [stderr truncated]"
                } else {
                    ""
                };
                return err_cmd(
                    start,
                    format!("git init failed: {}{}", stderr.trim(), suffix),
                );
            }
            Err(e) => {
                created_paths.cleanup();
                return err_cmd(start, format!("git init failed (is git installed?): {}", e));
            }
        }
    }

    // Write project TOML.
    let write_result =
        match write_project_toml_atomic(project_registry_dir, &id, &toml_content, overwrite) {
            Ok(p) => p,
            Err(ProjectTomlWriteError::BeforeRename) => {
                created_paths.cleanup();
                return project_error_cmd(start, "operation_failed");
            }
            Err(ProjectTomlWriteError::AfterRename) => {
                return project_error_cmd(start, "operation_indeterminate");
            }
        };
    let result = serde_json::json!({
        "id": runtime_id,
        "agent_project_id": id,
        "client_id": client_id,
        "name": name,
        "path": path,
        "description": description,
        "project_record_path": write_result.config_path.to_string_lossy(),
        "projects_config_path": write_result.config_path.to_string_lossy(),
        "created_directory": created_directory,
        "created_config": write_result.created_config,
        "overwritten": write_result.overwritten,
        "allow_patch": allow_patch,
        "template": template,
        "revision": project_revision(&parse_runner_project_toml(&toml_content).expect("generated project TOML must parse")),
        "git_initialized": git_initialized,
        "operation": "create", "outcome": "created", "changed": true, "recovered": false,
    });
    ok_cmd(start, result)
}

#[cfg(test)]
mod durability_tests {
    use super::*;

    /// Unix-only: verifies the POSIX directory-fsync contract (opening a
    /// directory as a file). On Windows directory sync is intentionally
    /// skipped because std cannot open directories with the required
    /// FILE_FLAG_BACKUP_SEMANTICS.
    #[cfg(unix)]
    #[test]
    fn registry_parent_sync_failures_are_not_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing").join("demo.toml");
        let error = sync_parent_dir(&missing).unwrap_err();
        assert!(error.contains("sync project registry directory"));
    }

    #[test]
    fn registry_loader_ignores_temp_and_unregister_tombstones() {
        let tmp = tempfile::tempdir().unwrap();
        let project_registry_dir = tmp.path().join("project-registry");
        let source = tmp.path().join("source");
        std::fs::create_dir_all(&project_registry_dir).unwrap();
        std::fs::create_dir_all(&source).unwrap();
        let content = build_project_toml("demo", "Demo", source.to_str().unwrap(), &None, true);
        std::fs::write(project_registry_dir.join("demo.toml"), &content).unwrap();
        std::fs::write(project_registry_dir.join(".demo.random.toml.tmp"), &content).unwrap();
        std::fs::write(
            project_registry_dir.join(".demo.random.toml.unregistering"),
            &content,
        )
        .unwrap();
        let projects = load_runner_project_summaries_from_dir(&project_registry_dir);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "demo");
    }

    #[cfg(unix)]
    #[test]
    fn project_summary_reports_retargeted_symlinks_as_distinct_canonical_roots() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        let link = tmp.path().join("current");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        symlink(&first, &link).unwrap();
        let project = RunnerProjectFile {
            id: "demo".to_string(),
            path: link.to_string_lossy().to_string(),
            shell_profile: None,
            allow_patch: true,
            name: None,
            kind: None,
            description: None,
            disabled: false,
            hooks: HashMap::new(),
        };

        let first_summary = runner_project_summary(&project, 1, false);
        assert_eq!(
            Path::new(&first_summary.path),
            first.canonicalize().unwrap()
        );
        std::fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();
        let second_summary = runner_project_summary(&project, 2, false);
        assert_eq!(
            Path::new(&second_summary.path),
            second.canonicalize().unwrap()
        );
        assert_ne!(first_summary.path, second_summary.path);
    }
}

#[cfg(test)]
mod git_lifecycle_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::SystemTime;

    // -----------------------------------------------------------------------
    // Git lifecycle regression coverage for the run_git_bounded ManagedChild
    // migration. The scenarios run the real `validation_tree_helper` fixture
    // (compiled at test time with rustc, exactly like the validation and job
    // tree tests) through the `run_git_bounded_with_program` seam, so the same
    // tests run on Windows and Unix without cmd, PowerShell, or bash. Each
    // test tracks the real parent/descendant pids written to marker files and
    // probes them with platform-native APIs, and every test reaps the tree it
    // starts before returning.
    // -----------------------------------------------------------------------

    /// Compiled copy of the `validation_tree_helper` fixture, kept alive for
    /// the whole test process so its binary path never disappears under a
    /// running descendant.
    struct GitTreeHelper {
        _temp: tempfile::TempDir,
        path: PathBuf,
    }

    static GIT_TREE_HELPER: OnceLock<Arc<GitTreeHelper>> = OnceLock::new();

    fn helper_binary() -> PathBuf {
        GIT_TREE_HELPER
            .get_or_init(|| {
                let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/webcodex_runner/validation/validation_tree_helper.rs");
                let temp = tempfile::tempdir().unwrap();
                let output = temp
                    .path()
                    .join(format!("git-tree-helper{}", std::env::consts::EXE_SUFFIX));
                let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
                let result = Command::new(rustc)
                    .arg("--edition=2021")
                    .arg("--crate-name=webcodex_git_tree_helper")
                    .arg(&source)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .expect("run rustc for git tree helper");
                assert!(
                    result.status.success(),
                    "git tree helper compilation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                Arc::new(GitTreeHelper {
                    _temp: temp,
                    path: output,
                })
            })
            .path
            .clone()
    }

    fn str_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// A unique temp file, removed on drop.
    struct CleanupPath(PathBuf);

    impl std::ops::Deref for CleanupPath {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    impl Drop for CleanupPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn unique_temp_path(tag: &str) -> CleanupPath {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wc-project-git-{tag}-{}-{nanos}",
            std::process::id()
        ));
        CleanupPath(path)
    }

    fn wait_until_file(path: &Path, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if path.exists() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Parse `KEY=<pid>` from a marker file written by the helper.
    fn read_pid(marker: &Path, key: &str) -> u32 {
        let text = std::fs::read_to_string(marker).expect("read pid marker");
        text.lines()
            .find_map(|line| {
                line.strip_prefix(key)
                    .and_then(|rest| rest.strip_prefix('='))
                    .and_then(|value| value.trim().parse().ok())
            })
            .unwrap_or_else(|| panic!("marker {marker:?} missing {key}: {text}"))
    }

    #[cfg(windows)]
    fn process_alive(pid: u32) -> bool {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: OpenProcess returns a handle or NULL; NULL means the pid no
        // longer exists (or is inaccessible, which also means not ours).
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0u32;
        // SAFETY: `handle` is valid; `exit_code` is a valid out-param.
        let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
        // SAFETY: close the handle we opened.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        ok == 1 && exit_code == 259 // 259 == STILL_ACTIVE
    }

    #[cfg(target_os = "linux")]
    fn process_alive(pid: u32) -> bool {
        // `kill(pid, 0)` also succeeds for zombies, while ManagedChild's Linux
        // tree-liveness contract deliberately treats zombies as unable to run.
        // Use /proc to align this test probe with that contract, but fall back
        // conservatively if procfs cannot be read or parsed.
        // SAFETY: signal 0 is an existence probe; the pid comes from our own
        // test helper.
        if (unsafe { libc::kill(pid as i32, 0) }) != 0 {
            return false;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return true;
        };
        let Some((_, rest)) = stat.rsplit_once(')') else {
            return true;
        };
        let state = rest.split_whitespace().next().unwrap_or("");
        state != "Z" && state != "X"
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn process_alive(pid: u32) -> bool {
        // SAFETY: signal 0 is an existence probe; the pid comes from our own
        // test helper. Non-Linux Unix test hosts reap orphaned descendants
        // promptly, so a successful probe represents a live process here.
        (unsafe { libc::kill(pid as i32, 0) }) == 0
    }

    /// Upper bound for the whole test body including cleanup; the fixture
    /// sleeps far longer (600s), so any run exceeding this is a cleanup hang,
    /// not a slow exit.
    const BOUNDEDNESS_LIMIT: Duration = Duration::from_secs(15);

    /// A. Normal completion: a short-lived process exits successfully, its
    /// stdout/stderr are collected, and no cleanup stall occurs.
    #[test]
    fn normal_completion_collects_output_and_returns_bounded() {
        let cwd = tempfile::tempdir().unwrap();
        let program = helper_binary();
        let started = Instant::now();
        let output = run_git_bounded_with_program(
            &program.to_string_lossy(),
            cwd.path(),
            &["sleep", "0", "7"],
            Duration::from_secs(10),
            None,
        )
        .expect("normal completion must succeed");
        assert_eq!(output.status.code(), Some(7));
        assert!(!output.stdout_capped);
        assert!(!output.stderr_capped);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("VALIDATION_HELPER_STDOUT"), "{stdout}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("VALIDATION_HELPER_STDERR"), "{stderr}");
        assert!(
            started.elapsed() < BOUNDEDNESS_LIMIT,
            "normal completion was not bounded"
        );
    }

    /// B. Explicit timeout kills the whole tree: the direct Git process and
    /// its pipe-holding descendant must both die, with the timeout error
    /// unchanged.
    #[test]
    fn timeout_terminates_whole_tree() {
        let parent_marker = unique_temp_path("timeout-parent");
        let alive_marker = unique_temp_path("timeout-desc");
        let cwd = tempfile::tempdir().unwrap();
        let program = helper_binary();
        let args = str_args(&[
            "spawn-descendant-keepalive",
            parent_marker.to_str().unwrap(),
            alive_marker.to_str().unwrap(),
            "600",
        ]);
        let started = Instant::now();
        let result = thread::scope(|scope| {
            let handle = scope.spawn(|| {
                run_git_bounded_with_program(
                    &program.to_string_lossy(),
                    cwd.path(),
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    Duration::from_secs(2),
                    None,
                )
            });
            assert!(
                wait_until_file(&parent_marker, Duration::from_secs(5)),
                "parent marker never appeared"
            );
            assert!(
                wait_until_file(&alive_marker, Duration::from_secs(5)),
                "descendant marker never appeared"
            );
            let parent_pid = read_pid(&parent_marker, "PARENT_PID");
            let descendant_pid = read_pid(&parent_marker, "DESCENDANT_PID");
            // Both sleep 600s while the timeout is 2s, so both must still be
            // alive when the timeout fires.
            assert!(process_alive(parent_pid), "parent not alive before timeout");
            assert!(
                process_alive(descendant_pid),
                "descendant not alive before timeout"
            );
            handle.join().expect("run_git_bounded panicked")
        });
        let error = match result {
            Ok(_) => panic!("run_git_bounded must report a timeout, not success"),
            Err(error) => error,
        };
        assert_eq!(error, "git command timed out");
        assert!(
            started.elapsed() < BOUNDEDNESS_LIMIT,
            "timeout cleanup not bounded"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "PARENT_PID")),
            "Git parent survived timeout cleanup"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "DESCENDANT_PID")),
            "Git descendant survived timeout cleanup"
        );
    }

    /// C. Runner shutdown terminates the whole tree with the shutdown error
    /// unchanged. Works on Windows and Linux.
    #[test]
    fn runner_shutdown_terminates_whole_tree() {
        let parent_marker = unique_temp_path("shutdown-parent");
        let alive_marker = unique_temp_path("shutdown-desc");
        let cwd = tempfile::tempdir().unwrap();
        let program = helper_binary();
        let args = str_args(&[
            "spawn-descendant-keepalive",
            parent_marker.to_str().unwrap(),
            alive_marker.to_str().unwrap(),
            "600",
        ]);
        let shutdown = AtomicBool::new(false);
        let started = Instant::now();
        let result = thread::scope(|scope| {
            let handle = scope.spawn(|| {
                run_git_bounded_with_program(
                    &program.to_string_lossy(),
                    cwd.path(),
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    Duration::from_secs(60),
                    Some(&shutdown),
                )
            });
            assert!(
                wait_until_file(&parent_marker, Duration::from_secs(5)),
                "parent marker never appeared"
            );
            assert!(
                wait_until_file(&alive_marker, Duration::from_secs(5)),
                "descendant marker never appeared"
            );
            let parent_pid = read_pid(&parent_marker, "PARENT_PID");
            let descendant_pid = read_pid(&parent_marker, "DESCENDANT_PID");
            assert!(
                process_alive(parent_pid),
                "parent not alive before shutdown"
            );
            assert!(
                process_alive(descendant_pid),
                "descendant not alive before shutdown"
            );
            shutdown.store(true, Ordering::SeqCst);
            handle.join().expect("run_git_bounded panicked")
        });
        let error = match result {
            Ok(_) => panic!("run_git_bounded must report shutdown, not success"),
            Err(error) => error,
        };
        assert_eq!(error, "git stopped during runner shutdown");
        assert!(
            started.elapsed() < BOUNDEDNESS_LIMIT,
            "shutdown cleanup not bounded"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "PARENT_PID")),
            "Git parent survived runner shutdown"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "DESCENDANT_PID")),
            "Git descendant survived runner shutdown"
        );
    }

    /// D. The direct Git process exits while its descendant survives and holds
    /// the captured pipes. Direct-child exit alone must not finish cleanup:
    /// the surviving tree is terminated, the readers reach EOF, and
    /// run_git_bounded returns without an indefinite reader wait.
    #[test]
    fn parent_exit_alone_does_not_finish_cleanup() {
        let parent_marker = unique_temp_path("parent-first");
        let alive_marker = unique_temp_path("parent-first-desc");
        let cwd = tempfile::tempdir().unwrap();
        let program = helper_binary();
        let args = str_args(&[
            "spawn-descendant",
            parent_marker.to_str().unwrap(),
            alive_marker.to_str().unwrap(),
            "600",
        ]);
        let started = Instant::now();
        let output = thread::scope(|scope| {
            let handle = scope.spawn(|| {
                run_git_bounded_with_program(
                    &program.to_string_lossy(),
                    cwd.path(),
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    Duration::from_secs(30),
                    None,
                )
            });
            // The direct child exits almost immediately after spawning its
            // descendant. The descendant's marker appears only if it actually
            // ran, so its existence proves the descendant was alive after the
            // direct child exited.
            assert!(
                wait_until_file(&alive_marker, Duration::from_secs(5)),
                "descendant marker never appeared"
            );
            handle.join().expect("run_git_bounded panicked")
        })
        .expect("direct-parent exit must not turn into an error");
        assert!(
            output.status.success(),
            "direct child exited 0; tree cleanup must not change its status"
        );
        // The captured stdout contains the helper's pid line only when the
        // reader hit EOF, which requires every descendant holding the pipe to
        // be gone. A cleanup that stops at the direct child leaves stdout
        // stuck at the un-flushed line or empty.
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("DESCENDANT_PID="),
            "stdout reader never reached EOF: {stdout}"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "DESCENDANT_PID")),
            "descendant survived cleanup after direct child exit"
        );
        assert!(
            started.elapsed() < BOUNDEDNESS_LIMIT,
            "parent-exit cleanup not bounded"
        );
    }

    /// E. A SIGTERM-resistant tree is escalated to force: the graceful request
    /// gets a short bounded grace, then the whole tree is killed. Never
    /// unbounded. (Windows has no generic graceful tree termination, so there
    /// is nothing to escalate from there.)
    #[cfg(unix)]
    #[test]
    fn sigterm_resistant_tree_is_forcefully_escalated() {
        let parent_marker = unique_temp_path("resist-parent");
        let alive_marker = unique_temp_path("resist-desc");
        let cwd = tempfile::tempdir().unwrap();
        let program = helper_binary();
        let args = str_args(&[
            "ignore-term-keepalive",
            parent_marker.to_str().unwrap(),
            alive_marker.to_str().unwrap(),
            "600",
        ]);
        let started = Instant::now();
        let result = thread::scope(|scope| {
            let handle = scope.spawn(|| {
                run_git_bounded_with_program(
                    &program.to_string_lossy(),
                    cwd.path(),
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    Duration::from_secs(2),
                    None,
                )
            });
            assert!(
                wait_until_file(&parent_marker, Duration::from_secs(5)),
                "parent marker never appeared"
            );
            assert!(
                wait_until_file(&alive_marker, Duration::from_secs(5)),
                "descendant marker never appeared"
            );
            let parent_pid = read_pid(&parent_marker, "PARENT_PID");
            let descendant_pid = read_pid(&parent_marker, "DESCENDANT_PID");
            assert!(process_alive(parent_pid), "parent not alive before timeout");
            assert!(
                process_alive(descendant_pid),
                "descendant not alive before timeout"
            );
            handle.join().expect("run_git_bounded panicked")
        });
        let error = match result {
            Ok(_) => panic!("run_git_bounded must report a timeout, not success"),
            Err(error) => error,
        };
        assert_eq!(error, "git command timed out");
        // Both processes ignore SIGTERM (inherited SIG_IGN), so only the
        // forceful escalation can have ended them.
        assert!(
            !process_alive(read_pid(&parent_marker, "PARENT_PID")),
            "SIGTERM-resistant parent survived escalation"
        );
        assert!(
            !process_alive(read_pid(&parent_marker, "DESCENDANT_PID")),
            "SIGTERM-resistant descendant survived escalation"
        );
        assert!(
            started.elapsed() < BOUNDEDNESS_LIMIT,
            "SIGTERM-resistant cleanup not bounded"
        );
    }

    /// F. Spawn failure keeps the standard direct-spawn failure semantics with
    /// the existing user-visible error prefix.
    #[test]
    fn spawn_failure_reports_spawn_error() {
        let cwd = tempfile::tempdir().unwrap();
        let error = match run_git_bounded_with_program(
            "webcodex-git-command-that-does-not-exist-xyz",
            cwd.path(),
            &["--version"],
            Duration::from_secs(5),
            None,
        ) {
            Ok(_) => panic!("spawn of a nonexistent executable must fail"),
            Err(error) => error,
        };
        assert!(
            error.starts_with("failed to spawn git"),
            "unexpected spawn error: {error}"
        );
    }

    /// Keep at least one real Git smoke path: production `run_git_bounded`
    /// with the hardcoded `"git"` program.
    #[test]
    fn real_git_smoke_runs_through_managed_spawn() {
        let cwd = tempfile::tempdir().unwrap();
        let output = run_git_bounded(cwd.path(), &["--version"], Duration::from_secs(5), None)
            .expect("real git must run through the managed spawn");
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("git version"),
            "unexpected git --version output"
        );
    }
}
