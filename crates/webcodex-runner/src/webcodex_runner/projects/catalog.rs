use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use webcodex_process::{GracefulTermination, ManagedChild};

use super::super::config::{project_registry_dir, validate_shell_profile_name, RunnerConfig};
use super::super::shell::canonicalize_existing;
use super::{RunnerProjectCache, RunnerProjectFile, RunnerProjectShellContext};
use crate::runner_protocol::{
    RunnerProjectLineage, RunnerProjectSummary, PROJECT_ROOT_FINGERPRINT_PREFIX,
    PROJECT_ROOT_IDENTITY_DOMAIN,
};

const PROJECT_SCAN_CACHE_MS: u64 = 5000;
const PROJECT_GIT_TIMEOUT: Duration = Duration::from_secs(2);
// Tree shutdown also has to let the bounded stdout/stderr readers observe EOF.
// Darwin process-group teardown and reader scheduling can legitimately take
// longer than 500ms on loaded native CI hosts, so keep a short but realistic
// bounded cleanup budget rather than turning successful direct-child exit into
// a spurious reader-timeout failure.
const PROJECT_GIT_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const PROJECT_GIT_OUTPUT_MAX_BYTES: usize = 64 * 1024;
pub(super) const EXPLICIT_REGISTRATION_SOURCE: &str = "explicit";
pub(super) const AUTO_REGISTERED_REGISTRATION_SOURCE: &str = "auto_registered";
pub(super) const LEGACY_AUTO_REGISTERED_PROJECT_KIND: &str = "auto_registered";

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
    project.registration_source = trim_optional(project.registration_source);
    project.description = trim_optional(project.description);
    project.managed_source = trim_optional(project.managed_source);
    project.managed_source_project_id = trim_optional(project.managed_source_project_id);
    project.managed_source_root_fingerprint =
        trim_optional(project.managed_source_root_fingerprint);
    project.managed_base_ref = trim_optional(project.managed_base_ref);
    project.managed_base_sha = trim_optional(project.managed_base_sha);
    project.managed_operation_id = trim_optional(project.managed_operation_id);
    match (
        project.managed_source_project_id.as_deref(),
        project.managed_source_root_fingerprint.as_deref(),
    ) {
        (Some(source_project_id), Some(source_root_fingerprint)) => {
            if !project.managed_worktree {
                return Err("managed lineage requires managed_worktree = true".to_string());
            }
            validate_project_id(source_project_id)?;
            if source_project_id == project.id {
                return Err("managed source project cannot equal target project".to_string());
            }
            if !valid_project_root_fingerprint(source_root_fingerprint) {
                return Err("managed source root fingerprint is invalid".to_string());
            }
            if project
                .managed_base_sha
                .as_deref()
                .is_none_or(|sha| !valid_git_sha(sha))
            {
                return Err("managed lineage requires a valid managed_base_sha".to_string());
            }
        }
        (None, None) => {}
        _ => return Err("managed source lineage is incomplete".to_string()),
    }
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

#[derive(Debug)]
pub(super) struct BoundedGitOutput {
    pub(super) status: std::process::ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) stdout_capped: bool,
    pub(super) stderr_capped: bool,
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

pub(super) fn run_git_bounded(
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
pub(super) fn run_git_bounded_with_program(
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

pub(super) fn project_revision(project: &RunnerProjectFile) -> String {
    let normalized = toml::to_string(project).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(normalized.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectRegistrationSource {
    Explicit,
    AutoRegistered,
}

impl ProjectRegistrationSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => EXPLICIT_REGISTRATION_SOURCE,
            Self::AutoRegistered => AUTO_REGISTERED_REGISTRATION_SOURCE,
        }
    }
}

/// Interpret registration provenance without changing the parsed persisted
/// representation. Keeping this compatibility projection separate is
/// important because `project_revision` hashes the raw normalized record.
pub(super) fn effective_registration_source(
    project: &RunnerProjectFile,
) -> ProjectRegistrationSource {
    match project.registration_source.as_deref() {
        Some(AUTO_REGISTERED_REGISTRATION_SOURCE) => ProjectRegistrationSource::AutoRegistered,
        // A present new field is authoritative. `explicit` and unknown future
        // values therefore fail closed to ordinary explicit registration rather
        // than allowing a legacy `kind` value to override newer semantics.
        Some(_) => ProjectRegistrationSource::Explicit,
        None if project.kind.as_deref() == Some(LEGACY_AUTO_REGISTERED_PROJECT_KIND) => {
            ProjectRegistrationSource::AutoRegistered
        }
        None => ProjectRegistrationSource::Explicit,
    }
}

/// Preserve the historical auto-registration sentinel only at Runner→Server
/// compatibility boundaries. Persisted `kind` remains genuine project metadata.
pub(super) fn project_wire_kind(project: &RunnerProjectFile) -> Option<String> {
    if project.kind.is_none()
        && project.registration_source.as_deref() == Some(AUTO_REGISTERED_REGISTRATION_SOURCE)
    {
        Some(LEGACY_AUTO_REGISTERED_PROJECT_KIND.to_string())
    } else {
        project.kind.clone()
    }
}

pub(super) fn valid_git_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn valid_project_root_fingerprint(value: &str) -> bool {
    value
        .strip_prefix(PROJECT_ROOT_FINGERPRINT_PREFIX)
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

pub(crate) fn project_root_fingerprint(canonical_root: &Path) -> String {
    let identity = webcodex_runner_config::paths::normalize_path_identity(canonical_root);
    let mut hasher = Sha256::new();
    hasher.update(PROJECT_ROOT_IDENTITY_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update(identity.as_bytes());
    format!("{PROJECT_ROOT_FINGERPRINT_PREFIX}{:x}", hasher.finalize())
}

pub(super) fn project_lineage(project: &RunnerProjectFile) -> Option<RunnerProjectLineage> {
    let source_project_id = project.managed_source_project_id.as_ref()?;
    let source_root_fingerprint = project.managed_source_root_fingerprint.as_ref()?;
    let base_sha = project.managed_base_sha.as_ref()?;
    Some(RunnerProjectLineage::ManagedWorktreeSource {
        source_project_id: source_project_id.clone(),
        source_root_fingerprint: source_root_fingerprint.clone(),
        base_sha: base_sha.clone(),
    })
}

fn runner_project_summary_with_shutdown(
    project: &RunnerProjectFile,
    updated_at: i64,
    include_git: bool,
    shutdown: Option<&AtomicBool>,
) -> RunnerProjectSummary {
    let mut hooks = project.hooks.keys().cloned().collect::<Vec<_>>();
    hooks.sort();
    // The server uses the reported path as part of its repository continuity
    // identity. Report the actual root, not a mutable symlink alias, so a
    // retargeted project registration cannot inherit another repository's
    // current Workflow Session.
    let canonical_root = canonicalize_existing(Path::new(&project.path))
        .ok()
        .filter(|path| path.is_dir());
    let root_fingerprint = canonical_root.as_deref().map(project_root_fingerprint);
    let resolved_path = canonical_root
        .unwrap_or_else(|| PathBuf::from(&project.path))
        .to_string_lossy()
        .to_string();
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
    let registration_source = effective_registration_source(project);
    // Rolling-upgrade shim: old Servers only know the historical `kind`
    // sentinel. New auto-registered records keep persisted `kind` empty, but
    // temporarily project that sentinel on the wire when there is no genuine
    // project kind. New Servers ignore it in favor of `registration_source`.
    RunnerProjectSummary {
        id: project.id.clone(),
        name: project.name.clone().or_else(|| Some(project.id.clone())),
        path: resolved_path,
        allow_patch: project.allow_patch,
        kind: project_wire_kind(project),
        registration_source: Some(registration_source.as_str().to_string()),
        description: project.description.clone(),
        hooks,
        disabled: project.disabled,
        revision: Some(project_revision(project)),
        root_fingerprint,
        lineage: project_lineage(project),
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
) -> RunnerProjectSummary {
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
) -> Vec<RunnerProjectSummary> {
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

pub(crate) fn load_runner_project_summaries_from_dir(dir: &Path) -> Vec<RunnerProjectSummary> {
    load_runner_project_summaries_from_dir_with_shutdown(dir, None)
}

fn load_runner_project_summaries(
    cfg: &RunnerConfig,
    shutdown: Option<&AtomicBool>,
) -> Vec<RunnerProjectSummary> {
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
    pub(crate) fn get(&mut self, cfg: &RunnerConfig) -> Vec<RunnerProjectSummary> {
        self.get_with_shutdown(cfg, None)
    }

    #[cfg(test)]
    pub(crate) fn mark_fresh_for_test(&mut self) {
        self.refreshed_at = Some(Instant::now());
    }

    pub(crate) fn get_with_shutdown(
        &mut self,
        cfg: &RunnerConfig,
        shutdown: Option<&AtomicBool>,
    ) -> Vec<RunnerProjectSummary> {
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

#[cfg(test)]
mod git_lifecycle_tests {
    use super::*;
    #[cfg(feature = "runner-real-process-tests")]
    use std::path::PathBuf;
    #[cfg(feature = "runner-real-process-tests")]
    use std::sync::{Arc, OnceLock};
    #[cfg(feature = "runner-real-process-tests")]
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
    #[cfg(feature = "runner-real-process-tests")]
    struct GitTreeHelper {
        _temp: tempfile::TempDir,
        path: PathBuf,
    }

    #[cfg(feature = "runner-real-process-tests")]
    static GIT_TREE_HELPER: OnceLock<Arc<GitTreeHelper>> = OnceLock::new();

    #[cfg(feature = "runner-real-process-tests")]
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

    #[cfg(feature = "runner-real-process-tests")]
    fn str_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// A unique temp file, removed on drop.
    #[cfg(feature = "runner-real-process-tests")]
    struct CleanupPath(PathBuf);

    #[cfg(feature = "runner-real-process-tests")]
    impl std::ops::Deref for CleanupPath {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    #[cfg(feature = "runner-real-process-tests")]
    impl Drop for CleanupPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[cfg(feature = "runner-real-process-tests")]
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

    #[cfg(feature = "runner-real-process-tests")]
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
    #[cfg(feature = "runner-real-process-tests")]
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

    #[cfg(feature = "runner-real-process-tests")]
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

    #[cfg(feature = "runner-real-process-tests")]
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

    #[cfg(feature = "runner-real-process-tests")]
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
    #[cfg(feature = "runner-real-process-tests")]
    const BOUNDEDNESS_LIMIT: Duration = Duration::from_secs(15);

    /// A. Normal completion: a short-lived process exits successfully, its
    /// stdout/stderr are collected, and no cleanup stall occurs.
    #[test]
    #[cfg(feature = "runner-real-process-tests")]
    #[ignore = "runner real-process lane: spawns the Git ManagedChild process-tree fixture"]
    fn runner_real_process_git_normal_completion_collects_output_and_returns_bounded() {
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
    #[cfg(feature = "runner-real-process-tests")]
    #[ignore = "runner real-process lane: spawns the Git ManagedChild process-tree fixture"]
    fn runner_real_process_git_timeout_terminates_whole_tree() {
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
    #[cfg(feature = "runner-real-process-tests")]
    #[ignore = "runner real-process lane: spawns the Git ManagedChild process-tree fixture"]
    fn runner_real_process_git_runner_shutdown_terminates_whole_tree() {
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
    #[cfg(feature = "runner-real-process-tests")]
    #[ignore = "runner real-process lane: spawns the Git ManagedChild process-tree fixture"]
    fn runner_real_process_git_parent_exit_alone_does_not_finish_cleanup() {
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
    #[cfg(feature = "runner-real-process-tests")]
    #[ignore = "runner real-process lane: spawns the Git ManagedChild process-tree fixture"]
    fn runner_real_process_git_sigterm_resistant_tree_is_forcefully_escalated() {
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
