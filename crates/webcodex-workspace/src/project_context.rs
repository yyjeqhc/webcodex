//! Lightweight repository-context fingerprinting.
//!
//! Fingerprints contain identities and hashes only, never repository file
//! contents. They are suitable for durable continuity records and let callers
//! report exactly which context slices changed between chat turns.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};
pub use webcodex_core::project_context_contract::{
    ContextFileFingerprint, FingerprintCompleteness, GitContextFingerprint,
    ProjectContextFingerprint,
};

const FINGERPRINT_SCHEMA_VERSION: u32 = 2;
pub const MAX_UNTRACKED_FILE_COUNT: usize = 512;
pub const MAX_BYTES_PER_UNTRACKED_FILE: u64 = 256 * 1024;
pub const MAX_TOTAL_UNTRACKED_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_TRACKED_DIFF_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MANIFEST_CANDIDATES: usize = 128;
pub const MAX_CONTEXT_SCAN_ENTRIES: usize = 100_000;
pub const MAX_CONTEXT_SCAN_ELAPSED: Duration = Duration::from_secs(3);

/// Compile-time invariants for the bounded-context budgets above: they must
/// stay positive and ordered, so any future edit that violates the budgets is
/// caught at build time instead of at runtime.
const _: () = {
    assert!(MAX_UNTRACKED_FILE_COUNT > 0);
    assert!(MAX_BYTES_PER_UNTRACKED_FILE > 0);
    assert!(MAX_TOTAL_UNTRACKED_BYTES >= MAX_BYTES_PER_UNTRACKED_FILE);
    assert!(MAX_TRACKED_DIFF_BYTES > 0);
    assert!(MAX_MANIFEST_CANDIDATES > 0);
    assert!(MAX_CONTEXT_SCAN_ENTRIES >= MAX_MANIFEST_CANDIDATES);
    assert!(MAX_CONTEXT_SCAN_ELAPSED.as_nanos() > 0);
};

const MAX_GIT_STATUS_BYTES: usize = 1024 * 1024;
const MAX_GIT_LIST_BYTES: usize = 1024 * 1024;
const MAX_GIT_IDENTITY_BYTES: usize = 4096;
const MAX_CONTEXT_FILE_BYTES: u64 = 512 * 1024;
const MAX_FINGERPRINT_WARNINGS: usize = 16;
const RULE_CANDIDATES: &[&str] = &[
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    ".codex/AGENTS.md",
    ".github/copilot-instructions.md",
];

#[derive(Debug, Clone)]
struct ProjectContextBudget {
    max_untracked_file_count: usize,
    max_bytes_per_untracked_file: u64,
    max_total_untracked_bytes: u64,
    max_tracked_diff_bytes: usize,
    max_manifest_candidates: usize,
    max_scan_entries: usize,
    max_elapsed: Duration,
    max_git_status_bytes: usize,
    max_git_list_bytes: usize,
    max_context_file_bytes: u64,
}

impl Default for ProjectContextBudget {
    fn default() -> Self {
        Self {
            max_untracked_file_count: MAX_UNTRACKED_FILE_COUNT,
            max_bytes_per_untracked_file: MAX_BYTES_PER_UNTRACKED_FILE,
            max_total_untracked_bytes: MAX_TOTAL_UNTRACKED_BYTES,
            max_tracked_diff_bytes: MAX_TRACKED_DIFF_BYTES,
            max_manifest_candidates: MAX_MANIFEST_CANDIDATES,
            max_scan_entries: MAX_CONTEXT_SCAN_ENTRIES,
            max_elapsed: MAX_CONTEXT_SCAN_ELAPSED,
            max_git_status_bytes: MAX_GIT_STATUS_BYTES,
            max_git_list_bytes: MAX_GIT_LIST_BYTES,
            max_context_file_bytes: MAX_CONTEXT_FILE_BYTES,
        }
    }
}

#[derive(Debug)]
struct CaptureState {
    budget: ProjectContextBudget,
    started: Instant,
    partial_slices: BTreeSet<String>,
    warnings: BTreeSet<String>,
}

impl CaptureState {
    fn new(budget: ProjectContextBudget) -> Self {
        Self {
            budget,
            started: Instant::now(),
            partial_slices: BTreeSet::new(),
            warnings: BTreeSet::new(),
        }
    }

    fn deadline(&self) -> Instant {
        self.started + self.budget.max_elapsed
    }

    fn elapsed(&self) -> bool {
        Instant::now() >= self.deadline()
    }

    fn mark_partial(&mut self, slice: &str, warning: &str) {
        self.partial_slices.insert(slice.to_string());
        if self.warnings.len() < MAX_FINGERPRINT_WARNINGS {
            self.warnings.insert(warning.to_string());
        }
    }

    fn completeness(&self) -> FingerprintCompleteness {
        FingerprintCompleteness {
            complete: self.partial_slices.is_empty(),
            partial_slices: self.partial_slices.iter().cloned().collect(),
            warnings: self.warnings.iter().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextRefreshSummary {
    pub reused: Vec<String>,
    pub refreshed: Vec<String>,
    pub rules: ContextFileRefresh,
    pub manifests: ContextFileRefresh,
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub unknown: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFileRefresh {
    pub reused: Vec<String>,
    pub refreshed: Vec<String>,
    pub removed: Vec<String>,
    #[serde(default)]
    pub unknown: Vec<String>,
}

pub fn capture_project_context(
    project_root: &Path,
    target_path: Option<&str>,
) -> Result<ProjectContextFingerprint, String> {
    capture_project_context_with_budget(project_root, target_path, ProjectContextBudget::default())
}

fn capture_project_context_with_budget(
    project_root: &Path,
    target_path: Option<&str>,
    budget: ProjectContextBudget,
) -> Result<ProjectContextFingerprint, String> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| format!("project root is unavailable: {error}"))?;
    if !canonical_root.is_dir() {
        return Err("project root is not a directory".to_string());
    }
    let normalized_target =
        crate::project_overview::normalize_project_overview_path(target_path.unwrap_or(""))?;
    let target_directory = target_directory(&canonical_root, &normalized_target);

    let root_identity = canonical_root.to_string_lossy();
    let project_root_sha256 = sha256_bytes(root_identity.as_bytes());
    let mut state = CaptureState::new(budget);
    let git = git_fingerprint(&canonical_root, &mut state);
    let rules = fingerprint_context_files(
        &canonical_root,
        rule_paths(&canonical_root, &target_directory),
        "rules",
        &mut state,
    );
    let manifest_paths = manifest_paths(&canonical_root, git.available, &mut state);
    let manifests =
        fingerprint_context_files(&canonical_root, manifest_paths, "manifests", &mut state);
    let completeness = state.completeness();

    Ok(ProjectContextFingerprint {
        schema_version: FINGERPRINT_SCHEMA_VERSION,
        project_root_sha256,
        target_directory,
        git,
        rules,
        manifests,
        completeness,
    })
}

fn fingerprint_context_files(
    root: &Path,
    paths: Vec<String>,
    slice: &str,
    state: &mut CaptureState,
) -> Vec<ContextFileFingerprint> {
    let budget = state.budget.clone();
    let mut fingerprints = Vec::with_capacity(paths.len());
    for path in paths {
        if state.elapsed() {
            state.mark_partial(slice, "scan_time_budget_exceeded");
            break;
        }
        match fingerprint_file(root, &path, &budget) {
            Ok(file) => {
                if !file.complete {
                    state.mark_partial(slice, "context_file_budget_exceeded");
                }
                fingerprints.push(file);
            }
            Err(_) => state.mark_partial(slice, "context_file_unavailable"),
        }
    }
    fingerprints
}

pub fn compare_project_context(
    previous: Option<&ProjectContextFingerprint>,
    current: &ProjectContextFingerprint,
) -> ContextRefreshSummary {
    let Some(previous) = previous else {
        return ContextRefreshSummary {
            reused: Vec::new(),
            refreshed: vec![
                "project_identity".to_string(),
                "git_head".to_string(),
                "worktree".to_string(),
                "target_directory".to_string(),
            ],
            rules: compare_files(&[], &current.rules),
            manifests: compare_files(&[], &current.manifests),
            partial: !current.completeness.complete,
            unknown: current.completeness.partial_slices.clone(),
            warnings: current.completeness.warnings.clone(),
        };
    };

    let mut reused = Vec::new();
    let mut refreshed = Vec::new();
    compare_scalar(
        "project_identity",
        previous.project_root_sha256 == current.project_root_sha256,
        &mut reused,
        &mut refreshed,
    );
    let mut unknown = Vec::new();
    compare_bounded_scalar(
        "git_head",
        previous.git.available == current.git.available
            && previous.git.branch == current.git.branch
            && previous.git.head == current.git.head,
        slice_complete(previous, "git_head") && slice_complete(current, "git_head"),
        &mut reused,
        &mut refreshed,
        &mut unknown,
    );
    compare_bounded_scalar(
        "worktree",
        previous.git.worktree_sha256 == current.git.worktree_sha256
            && previous.git.dirty == current.git.dirty,
        slice_complete(previous, "worktree") && slice_complete(current, "worktree"),
        &mut reused,
        &mut refreshed,
        &mut unknown,
    );
    compare_scalar(
        "target_directory",
        previous.target_directory == current.target_directory,
        &mut reused,
        &mut refreshed,
    );
    let rules_complete = slice_complete(previous, "rules") && slice_complete(current, "rules");
    let manifests_complete =
        slice_complete(previous, "manifests") && slice_complete(current, "manifests");
    let mut warnings = previous.completeness.warnings.clone();
    for warning in &current.completeness.warnings {
        if warnings.len() >= MAX_FINGERPRINT_WARNINGS {
            break;
        }
        if !warnings.contains(warning) {
            warnings.push(warning.clone());
        }
    }
    for slice in previous
        .completeness
        .partial_slices
        .iter()
        .chain(&current.completeness.partial_slices)
    {
        if !unknown.contains(slice) {
            unknown.push(slice.clone());
        }
    }
    ContextRefreshSummary {
        reused,
        refreshed,
        rules: compare_files_bounded(&previous.rules, &current.rules, rules_complete),
        manifests: compare_files_bounded(
            &previous.manifests,
            &current.manifests,
            manifests_complete,
        ),
        partial: !previous.completeness.complete || !current.completeness.complete,
        unknown,
        warnings,
    }
}

fn compare_scalar(
    name: &str,
    unchanged: bool,
    reused: &mut Vec<String>,
    refreshed: &mut Vec<String>,
) {
    if unchanged {
        reused.push(name.to_string());
    } else {
        refreshed.push(name.to_string());
    }
}

fn compare_bounded_scalar(
    name: &str,
    unchanged: bool,
    complete: bool,
    reused: &mut Vec<String>,
    refreshed: &mut Vec<String>,
    unknown: &mut Vec<String>,
) {
    if !unchanged {
        refreshed.push(name.to_string());
    } else if complete {
        reused.push(name.to_string());
    } else {
        unknown.push(name.to_string());
    }
}

fn slice_complete(fingerprint: &ProjectContextFingerprint, slice: &str) -> bool {
    !fingerprint
        .completeness
        .partial_slices
        .iter()
        .any(|partial| partial == slice)
}

fn compare_files(
    previous: &[ContextFileFingerprint],
    current: &[ContextFileFingerprint],
) -> ContextFileRefresh {
    compare_files_bounded(previous, current, true)
}

fn compare_files_bounded(
    previous: &[ContextFileFingerprint],
    current: &[ContextFileFingerprint],
    enumeration_complete: bool,
) -> ContextFileRefresh {
    let previous = previous
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut refresh = ContextFileRefresh::default();
    for (path, file) in &current {
        match previous.get(path) {
            Some(old)
                if old.sha256 == file.sha256
                    && old.bytes == file.bytes
                    && old.modified_unix_nanos == file.modified_unix_nanos
                    && old.complete
                    && file.complete =>
            {
                refresh.reused.push((*path).to_string())
            }
            Some(old)
                if old.sha256 == file.sha256
                    && old.bytes == file.bytes
                    && old.modified_unix_nanos == file.modified_unix_nanos =>
            {
                refresh.unknown.push((*path).to_string())
            }
            _ => refresh.refreshed.push((*path).to_string()),
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            if enumeration_complete {
                refresh.removed.push((*path).to_string());
            } else {
                refresh.unknown.push((*path).to_string());
            }
        }
    }
    if !enumeration_complete && !refresh.unknown.iter().any(|path| path == "*") {
        refresh.unknown.push("*".to_string());
    }
    refresh
}

fn target_directory(root: &Path, target: &str) -> String {
    if target.is_empty() {
        return String::new();
    }
    let candidate = root.join(target);
    if candidate.is_dir() {
        return target.to_string();
    }
    Path::new(target)
        .parent()
        .filter(|parent| *parent != Path::new(""))
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn git_fingerprint(root: &Path, state: &mut CaptureState) -> GitContextFingerprint {
    let head = match bounded_git_output(
        root,
        ["rev-parse", "--verify", "HEAD^{commit}"],
        MAX_GIT_IDENTITY_BYTES,
        state.deadline(),
    ) {
        Ok(output) => {
            if !output.complete {
                state.mark_partial("git_head", output.warning_code());
            }
            (output.status.success() && output.complete)
                .then(|| output_text(&output.stdout))
                .flatten()
        }
        Err(_) => {
            state.mark_partial("git_head", "git_identity_unavailable");
            None
        }
    };
    let branch = match bounded_git_output(
        root,
        ["symbolic-ref", "--short", "-q", "HEAD"],
        MAX_GIT_IDENTITY_BYTES,
        state.deadline(),
    ) {
        Ok(output) => {
            if !output.complete {
                state.mark_partial("git_head", output.warning_code());
            }
            (output.status.success() && output.complete)
                .then(|| output_text(&output.stdout))
                .flatten()
        }
        Err(_) => {
            state.mark_partial("git_head", "git_identity_unavailable");
            None
        }
    };
    let status = match bounded_git_output(
        root,
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        state.budget.max_git_status_bytes,
        state.deadline(),
    ) {
        Ok(output) => {
            if !output.complete {
                state.mark_partial("worktree", output.warning_code());
            }
            if !output.status.success() {
                if head.is_some() {
                    state.mark_partial("worktree", "git_status_unavailable");
                }
                None
            } else {
                Some(output.stdout)
            }
        }
        Err(_) => {
            state.mark_partial("worktree", "git_status_unavailable");
            None
        }
    };
    let worktree_sha256 = status
        .as_deref()
        .map(|status| worktree_sha256(root, status, head.is_some(), state));
    GitContextFingerprint {
        available: head.is_some() || status.is_some(),
        branch,
        head,
        worktree_sha256,
        dirty: status.as_ref().map(|bytes| !bytes.is_empty()),
    }
}

fn worktree_sha256(root: &Path, status: &[u8], has_head: bool, state: &mut CaptureState) -> String {
    let mut hasher = Sha256::new();
    update_hash_field(&mut hasher, b"status", status);

    // Clean fast path: porcelain proves there is no tracked or untracked
    // worktree state, so do not launch `git diff --binary` or open files.
    if !status.is_empty() {
        let mut remaining = state.budget.max_tracked_diff_bytes;
        let diff_commands: Vec<&[&str]> = if has_head {
            vec![&[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--binary",
                "HEAD",
                "--",
            ]]
        } else {
            vec![
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--cached",
                    "--",
                ],
                &["diff", "--no-ext-diff", "--no-textconv", "--binary", "--"],
            ]
        };
        for args in diff_commands {
            match bounded_git_output(root, args, remaining, state.deadline()) {
                Ok(output) => {
                    let digest = sha256_bytes(&output.stdout);
                    update_hash_field(&mut hasher, b"tracked", digest.as_bytes());
                    remaining = remaining.saturating_sub(output.stdout.len());
                    if !output.complete || !output.status.success() {
                        state.mark_partial("worktree", output.warning_code());
                    }
                }
                Err(_) => state.mark_partial("worktree", "tracked_diff_unavailable"),
            }
            if remaining == 0 {
                state.mark_partial("worktree", "tracked_diff_budget_exceeded");
                break;
            }
        }
        if status_has_untracked(status) {
            if let Some(untracked) = untracked_files_sha256(root, state) {
                update_hash_field(&mut hasher, b"untracked", untracked.as_bytes());
            } else {
                state.mark_partial("worktree", "untracked_scan_unavailable");
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug)]
struct BoundedGitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    complete: bool,
    timed_out: bool,
}

impl BoundedGitOutput {
    fn warning_code(&self) -> &'static str {
        if self.timed_out {
            "scan_time_budget_exceeded"
        } else if !self.complete {
            "git_output_budget_exceeded"
        } else {
            "git_command_failed"
        }
    }
}

fn bounded_git_output<I, S>(
    root: &Path,
    args: I,
    max_bytes: usize,
    deadline: Instant,
) -> std::io::Result<BoundedGitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("git stdout pipe unavailable"))?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let reader_exceeded = Arc::clone(&exceeded);
    let reader = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut captured = Vec::with_capacity(max_bytes.min(64 * 1024));
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = stdout.read(&mut buffer)?;
            if read == 0 {
                return Ok(captured);
            }
            let remaining = max_bytes.saturating_sub(captured.len());
            captured.extend_from_slice(&buffer[..read.min(remaining)]);
            if read > remaining {
                reader_exceeded.store(true, Ordering::SeqCst);
                return Ok(captured);
            }
        }
    });
    let mut timed_out = false;
    let status = loop {
        if exceeded.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let captured = reader
        .join()
        .map_err(|_| std::io::Error::other("git output reader panicked"))??;
    let complete = !timed_out && !exceeded.load(Ordering::SeqCst);
    Ok(BoundedGitOutput {
        status,
        stdout: captured,
        complete,
        timed_out,
    })
}

fn status_has_untracked(status: &[u8]) -> bool {
    status
        .split(|byte| *byte == 0)
        .any(|entry| entry.starts_with(b"?? "))
}

fn untracked_files_sha256(root: &Path, state: &mut CaptureState) -> Option<String> {
    let output = bounded_git_output(
        root,
        ["ls-files", "--others", "--exclude-standard", "-z", "--"],
        state.budget.max_git_list_bytes,
        state.deadline(),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    if !output.complete {
        state.mark_partial("worktree", output.warning_code());
    }
    let mut hasher = Sha256::new();
    let mut total_read = 0u64;
    let mut count = 0usize;
    for raw_path in output.stdout.split(|byte| *byte == 0) {
        if raw_path.is_empty() {
            continue;
        }
        if count >= state.budget.max_untracked_file_count {
            state.mark_partial("worktree", "untracked_file_count_exceeded");
            break;
        }
        count += 1;
        update_hash_field(&mut hasher, b"path", raw_path);
        let Ok(relative) = std::str::from_utf8(raw_path) else {
            update_hash_field(&mut hasher, b"metadata", b"non_utf8_path");
            state.mark_partial("worktree", "untracked_path_unavailable");
            continue;
        };
        if !safe_worktree_relative_path(relative) {
            update_hash_field(&mut hasher, b"metadata", b"unsafe_path");
            state.mark_partial("worktree", "untracked_path_unavailable");
            continue;
        }
        let path = root.join(relative);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            update_hash_field(&mut hasher, b"metadata", b"unavailable");
            state.mark_partial("worktree", "untracked_metadata_unavailable");
            continue;
        };
        update_metadata_hash(&mut hasher, &metadata);
        if metadata.file_type().is_symlink() {
            // Hash only the link target bytes. Never canonicalize or open the
            // target, so an untracked symlink cannot make the scan leave root.
            match std::fs::read_link(&path) {
                Ok(target) => {
                    update_hash_field(&mut hasher, b"symlink", target.to_string_lossy().as_bytes())
                }
                Err(_) => {
                    update_hash_field(&mut hasher, b"symlink", b"unavailable");
                    state.mark_partial("worktree", "untracked_symlink_unavailable");
                }
            }
        } else if metadata.is_file() {
            hash_untracked_file(&path, &metadata, &mut hasher, &mut total_read, state);
        } else {
            update_hash_field(&mut hasher, b"metadata", b"unsupported_type");
            state.mark_partial("worktree", "untracked_type_unsupported");
        }
        if state.elapsed() {
            state.mark_partial("worktree", "scan_time_budget_exceeded");
            break;
        }
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn hash_untracked_file(
    path: &Path,
    metadata: &std::fs::Metadata,
    hasher: &mut Sha256,
    total_read: &mut u64,
    state: &mut CaptureState,
) {
    let remaining_total = state
        .budget
        .max_total_untracked_bytes
        .saturating_sub(*total_read);
    let per_file = state
        .budget
        .max_bytes_per_untracked_file
        .min(remaining_total);
    let size = metadata.len();
    if size <= per_file {
        match read_file_digest(path, size) {
            Ok((digest, read)) => {
                *total_read = total_read.saturating_add(read);
                update_hash_field(hasher, b"content", digest.as_bytes());
            }
            Err(_) => {
                update_hash_field(hasher, b"content", b"unavailable");
                state.mark_partial("worktree", "untracked_content_unavailable");
            }
        }
        return;
    }

    if size > state.budget.max_bytes_per_untracked_file {
        state.mark_partial("worktree", "untracked_file_budget_exceeded");
    }
    if size > remaining_total {
        state.mark_partial("worktree", "untracked_total_budget_exceeded");
    }
    match sample_file_digest(path, size, per_file) {
        Ok((digest, read)) => {
            *total_read = total_read.saturating_add(read);
            update_hash_field(hasher, b"sample", digest.as_bytes());
        }
        Err(_) => {
            update_hash_field(hasher, b"sample", b"unavailable");
            state.mark_partial("worktree", "untracked_content_unavailable");
        }
    }
}

fn read_file_digest(path: &Path, expected: u64) -> std::io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > expected {
            return Err(std::io::Error::other("file grew during fingerprint"));
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected {
        return Err(std::io::Error::other(
            "file size changed during fingerprint",
        ));
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn sample_file_digest(path: &Path, size: u64, max_read: u64) -> std::io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    hasher.update(size.to_be_bytes());
    let prefix_len = max_read.saturating_add(1) / 2;
    let suffix_len = max_read.saturating_sub(prefix_len).min(size);
    let mut total = 0u64;
    let mut buffer = vec![0u8; prefix_len.min(64 * 1024) as usize];
    let mut remaining = prefix_len.min(size);
    while remaining > 0 {
        let take = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..take])?;
        hasher.update(&buffer[..take]);
        total += take as u64;
        remaining -= take as u64;
    }
    if suffix_len > 0 {
        file.seek(SeekFrom::Start(size.saturating_sub(suffix_len)))?;
        let mut remaining = suffix_len;
        while remaining > 0 {
            let take = remaining.min(buffer.len().max(1) as u64) as usize;
            if buffer.len() < take {
                buffer.resize(take, 0);
            }
            file.read_exact(&mut buffer[..take])?;
            hasher.update(&buffer[..take]);
            total += take as u64;
            remaining -= take as u64;
        }
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn update_metadata_hash(hasher: &mut Sha256, metadata: &std::fs::Metadata) {
    update_hash_field(hasher, b"size", &metadata.len().to_be_bytes());
    let modified = modified_unix_nanos(metadata).unwrap_or(0);
    update_hash_field(hasher, b"mtime", &modified.to_be_bytes());
}

fn update_hash_field(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn rule_paths(root: &Path, target_directory: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(root_rule) = first_rule(root, "") {
        paths.push(root_rule);
    }
    if target_directory.is_empty() {
        return paths;
    }

    let mut current = PathBuf::new();
    for component in Path::new(target_directory).components() {
        current.push(component.as_os_str());
        let relative = current.to_string_lossy().replace('\\', "/");
        if let Some(local_rule) = first_rule(root, &relative) {
            if !paths.contains(&local_rule) {
                paths.push(local_rule);
            }
        }
    }
    paths
}

fn first_rule(root: &Path, directory: &str) -> Option<String> {
    RULE_CANDIDATES.iter().find_map(|candidate| {
        let path = if directory.is_empty() {
            (*candidate).to_string()
        } else {
            format!("{directory}/{candidate}")
        };
        root.join(&path).is_file().then_some(path)
    })
}

fn manifest_paths(root: &Path, git_available: bool, state: &mut CaptureState) -> Vec<String> {
    let mut args = vec![
        "ls-files".to_string(),
        "-co".to_string(),
        "--exclude-standard".to_string(),
        "-z".to_string(),
        "--".to_string(),
    ];
    for name in MANIFEST_FILE_NAMES {
        args.push(format!(":(glob){name}"));
        args.push(format!(":(glob)**/{name}"));
    }
    for suffix in ["*.sln", "*.csproj"] {
        args.push(format!(":(glob){suffix}"));
        args.push(format!(":(glob)**/{suffix}"));
    }

    match bounded_git_output(
        root,
        args,
        state.budget.max_git_list_bytes,
        state.deadline(),
    ) {
        Ok(output) if output.status.success() => {
            if !output.complete {
                state.mark_partial("manifests", output.warning_code());
            }
            manifest_paths_from_git_output(&output.stdout, state)
        }
        _ => {
            if git_available {
                state.mark_partial("manifests", "manifest_git_query_unavailable");
            }
            fallback_manifest_paths(root, state).into_iter().collect()
        }
    }
}

fn manifest_paths_from_git_output(output: &[u8], state: &mut CaptureState) -> Vec<String> {
    let mut manifests = BTreeSet::new();
    for raw_path in output.split(|byte| *byte == 0) {
        if raw_path.is_empty() {
            continue;
        }
        let Ok(path) = std::str::from_utf8(raw_path) else {
            state.mark_partial("manifests", "manifest_path_unavailable");
            continue;
        };
        if !manifest_name(path) || !safe_relative_path(path) || manifests.contains(path) {
            continue;
        }
        if manifests.len() >= state.budget.max_manifest_candidates {
            state.mark_partial("manifests", "manifest_candidate_budget_exceeded");
            break;
        }
        manifests.insert(path.to_string());
    }
    manifests.into_iter().collect()
}

fn fallback_manifest_paths(root: &Path, state: &mut CaptureState) -> BTreeSet<String> {
    let mut manifests = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(directory) = pending.pop() {
        if state.elapsed() {
            state.mark_partial("manifests", "scan_time_budget_exceeded");
            break;
        }
        if visited >= state.budget.max_scan_entries {
            state.mark_partial("manifests", "scan_entry_budget_exceeded");
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            state.mark_partial("manifests", "manifest_directory_unavailable");
            continue;
        };
        for entry in entries {
            if visited >= state.budget.max_scan_entries {
                state.mark_partial("manifests", "scan_entry_budget_exceeded");
                return manifests;
            }
            if state.elapsed() {
                state.mark_partial("manifests", "scan_time_budget_exceeded");
                return manifests;
            }
            visited += 1;
            let Ok(entry) = entry else {
                state.mark_partial("manifests", "manifest_directory_unavailable");
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                state.mark_partial("manifests", "manifest_metadata_unavailable");
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if excluded_path(&relative) {
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && manifest_name(&relative) {
                if !manifests.contains(&relative)
                    && manifests.len() >= state.budget.max_manifest_candidates
                {
                    state.mark_partial("manifests", "manifest_candidate_budget_exceeded");
                    return manifests;
                }
                manifests.insert(relative);
            }
        }
    }
    manifests
}

const MANIFEST_FILE_NAMES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "requirements.txt",
    "Pipfile",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Gemfile",
    "composer.json",
    "CMakeLists.txt",
    "meson.build",
];

fn manifest_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    MANIFEST_FILE_NAMES.contains(&name) || name.ends_with(".sln") || name.ends_with(".csproj")
}

fn excluded_path(path: &str) -> bool {
    path.split('/').any(|component| {
        matches!(
            component.to_ascii_lowercase().as_str(),
            ".git"
                | "target"
                | "node_modules"
                | "vendor"
                | "dist"
                | "build"
                | ".venv"
                | "venv"
                | "__pycache__"
                | ".cache"
                | "secrets"
                | "credentials"
        )
    })
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && !Path::new(path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && !excluded_path(path)
}

fn safe_worktree_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path).components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

fn fingerprint_file(
    root: &Path,
    relative: &str,
    budget: &ProjectContextBudget,
) -> Result<ContextFileFingerprint, String> {
    if !safe_relative_path(relative) {
        return Err("context file path is unsafe".to_string());
    }
    let path = root.join(relative);
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("context file is unavailable: {error}"))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err("context file escapes the project root".to_string());
    }
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("context file metadata failed: {error}"))?;
    let bytes = metadata.len();
    let modified_unix_nanos = modified_unix_nanos(&metadata);
    let (sha256, complete, hash_kind) = if bytes <= budget.max_context_file_bytes {
        let (sha256, _) = read_file_digest(&canonical, bytes)
            .map_err(|error| format!("context file fingerprint failed: {error}"))?;
        (sha256, true, "full")
    } else {
        let (sha256, _) = sample_file_digest(&canonical, bytes, budget.max_context_file_bytes)
            .map_err(|error| format!("context file fingerprint failed: {error}"))?;
        (sha256, false, "prefix_suffix")
    };
    Ok(ContextFileFingerprint {
        path: relative.to_string(),
        sha256,
        bytes,
        complete,
        hash_kind: hash_kind.to_string(),
        modified_unix_nanos,
    })
}

fn modified_unix_nanos(metadata: &std::fs::Metadata) -> Option<u64> {
    let nanos = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(u64::try_from(nanos).unwrap_or(u64::MAX))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn output_text(output: &[u8]) -> Option<String> {
    String::from_utf8(output.to_vec())
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repo(name: &str) -> tempfile::TempDir {
        let temp = tempfile::Builder::new().prefix(name).tempdir().unwrap();
        git(temp.path(), &["init", "-q"]);
        git(temp.path(), &["config", "user.name", "WebCodex Test"]);
        git(
            temp.path(),
            &["config", "user.email", "webcodex@example.invalid"],
        );
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname='demo'\n").unwrap();
        std::fs::write(temp.path().join("AGENTS.md"), "root rules\n").unwrap();
        std::fs::create_dir_all(temp.path().join("src/nested")).unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "pub fn demo() {}\n").unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-qm", "initial"]);
        temp
    }

    #[test]
    fn unchanged_context_is_fully_reused() {
        let repo = repo("context-reuse");
        let first = capture_project_context(repo.path(), Some("src")).unwrap();
        let second = capture_project_context(repo.path(), Some("src")).unwrap();
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.refreshed.is_empty());
        assert!(refresh.rules.refreshed.is_empty());
        assert!(refresh.manifests.refreshed.is_empty());
        assert_eq!(refresh.rules.reused, ["AGENTS.md"]);
    }

    #[test]
    fn head_and_worktree_refresh_independently() {
        let repo = repo("context-git");
        let first = capture_project_context(repo.path(), None).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        let dirty = capture_project_context(repo.path(), None).unwrap();
        let refresh = compare_project_context(Some(&first), &dirty);
        assert!(refresh.refreshed.contains(&"worktree".to_string()));
        assert!(refresh.reused.contains(&"git_head".to_string()));

        std::fs::write(repo.path().join("src/lib.rs"), "pub fn altered() {}\n").unwrap();
        let dirty_again = capture_project_context(repo.path(), None).unwrap();
        assert_ne!(
            dirty.git.worktree_sha256, dirty_again.git.worktree_sha256,
            "content changes must refresh the worktree even when porcelain status is unchanged"
        );

        git(repo.path(), &["add", "src/lib.rs"]);
        git(repo.path(), &["commit", "-qm", "change head"]);
        let committed = capture_project_context(repo.path(), None).unwrap();
        let refresh = compare_project_context(Some(&dirty_again), &committed);
        assert!(refresh.refreshed.contains(&"git_head".to_string()));
        assert!(refresh.refreshed.contains(&"worktree".to_string()));
    }

    #[test]
    fn untracked_content_changes_refresh_worktree_with_the_same_status_path() {
        let repo = repo("context-untracked");
        std::fs::write(repo.path().join("notes.tmp"), "one\n").unwrap();
        let first = capture_project_context(repo.path(), None).unwrap();
        std::fs::write(repo.path().join("notes.tmp"), "two\n").unwrap();
        let second = capture_project_context(repo.path(), None).unwrap();
        assert_ne!(first.git.worktree_sha256, second.git.worktree_sha256);
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.refreshed.contains(&"worktree".to_string()));
    }

    #[test]
    fn branch_change_refreshes_git_baseline_without_refreshing_worktree() {
        let repo = repo("context-branch");
        let first = capture_project_context(repo.path(), None).unwrap();
        git(repo.path(), &["switch", "-qc", "feature"]);
        let second = capture_project_context(repo.path(), None).unwrap();
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.refreshed.contains(&"git_head".to_string()));
        assert!(refresh.reused.contains(&"worktree".to_string()));
        assert_eq!(first.git.head, second.git.head);
        assert_ne!(first.git.branch, second.git.branch);
    }

    #[test]
    fn changed_and_new_local_rules_refresh_only_their_paths() {
        let repo = repo("context-rules");
        let first = capture_project_context(repo.path(), Some("src/nested")).unwrap();
        std::fs::write(repo.path().join("AGENTS.md"), "updated root rules\n").unwrap();
        std::fs::write(repo.path().join("src/AGENTS.md"), "local rules\n").unwrap();
        let second = capture_project_context(repo.path(), Some("src/nested")).unwrap();
        let refresh = compare_project_context(Some(&first), &second);
        assert_eq!(
            refresh.rules.refreshed,
            ["AGENTS.md".to_string(), "src/AGENTS.md".to_string()]
        );
        assert!(refresh.manifests.refreshed.is_empty());
        assert!(second.rules.iter().any(|rule| rule.path == "src/AGENTS.md"));
    }

    #[test]
    fn target_directory_change_discovers_only_newly_applicable_rules() {
        let repo = repo("context-target");
        std::fs::write(repo.path().join("src/AGENTS.md"), "source rules\n").unwrap();
        let root_target = capture_project_context(repo.path(), None).unwrap();
        let nested_target =
            capture_project_context(repo.path(), Some("src/nested/lib.rs")).unwrap();
        let refresh = compare_project_context(Some(&root_target), &nested_target);
        assert!(refresh.refreshed.contains(&"target_directory".to_string()));
        assert_eq!(refresh.rules.refreshed, ["src/AGENTS.md"]);
        assert_eq!(refresh.rules.reused, ["AGENTS.md"]);
        assert!(refresh.manifests.refreshed.is_empty());
    }

    #[test]
    fn manifest_changes_do_not_refresh_rules() {
        let repo = repo("context-manifest");
        let first = capture_project_context(repo.path(), None).unwrap();
        std::fs::write(
            repo.path().join("Cargo.toml"),
            "[package]\nname='changed'\n",
        )
        .unwrap();
        let second = capture_project_context(repo.path(), None).unwrap();
        let refresh = compare_project_context(Some(&first), &second);
        assert_eq!(refresh.manifests.refreshed, ["Cargo.toml"]);
        assert!(refresh.rules.refreshed.is_empty());
    }

    #[test]
    fn similar_repository_names_never_share_identity() {
        let first_repo = repo("same-name");
        let second_repo = repo("same-name");
        let first = capture_project_context(first_repo.path(), None).unwrap();
        let second = capture_project_context(second_repo.path(), None).unwrap();
        assert_ne!(first.project_root_sha256, second.project_root_sha256);
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.refreshed.contains(&"project_identity".to_string()));
    }

    #[test]
    fn clean_repository_skips_zero_byte_diff_and_untracked_budgets() {
        let repo = repo("context-clean-fast-path");
        let budget = ProjectContextBudget {
            max_tracked_diff_bytes: 0,
            max_untracked_file_count: 0,
            max_bytes_per_untracked_file: 0,
            max_total_untracked_bytes: 0,
            ..Default::default()
        };

        let first = capture_project_context_with_budget(repo.path(), None, budget.clone()).unwrap();
        let second = capture_project_context_with_budget(repo.path(), None, budget).unwrap();
        assert!(first.completeness.complete);
        assert!(second.completeness.complete);
        assert_eq!(first.git.worktree_sha256, second.git.worktree_sha256);
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.reused.contains(&"worktree".to_string()));
        assert!(!refresh.partial);
    }

    #[test]
    fn large_untracked_file_uses_bounded_sample_and_compares_as_unknown() {
        let repo = repo("context-large-untracked");
        let file = File::create(repo.path().join("large.bin")).unwrap();
        file.set_len(4096).unwrap();
        let budget = ProjectContextBudget {
            max_bytes_per_untracked_file: 32,
            max_total_untracked_bytes: 32,
            ..Default::default()
        };

        let first = capture_project_context_with_budget(repo.path(), None, budget.clone()).unwrap();
        let second = capture_project_context_with_budget(repo.path(), None, budget).unwrap();
        assert!(!first.completeness.complete);
        assert!(first
            .completeness
            .partial_slices
            .contains(&"worktree".to_string()));
        assert!(first
            .completeness
            .warnings
            .contains(&"untracked_file_budget_exceeded".to_string()));
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.partial);
        assert!(refresh.unknown.contains(&"worktree".to_string()));
        assert!(!refresh.reused.contains(&"worktree".to_string()));
    }

    #[test]
    fn untracked_file_count_and_total_bytes_are_bounded() {
        let repo = repo("context-untracked-count");
        for index in 0..3 {
            std::fs::write(
                repo.path().join(format!("note-{index}.tmp")),
                [index as u8; 32],
            )
            .unwrap();
        }
        let count_budget = ProjectContextBudget {
            max_untracked_file_count: 2,
            ..Default::default()
        };
        let count_limited =
            capture_project_context_with_budget(repo.path(), None, count_budget).unwrap();
        assert!(count_limited
            .completeness
            .warnings
            .contains(&"untracked_file_count_exceeded".to_string()));

        let byte_budget = ProjectContextBudget {
            max_bytes_per_untracked_file: 32,
            max_total_untracked_bytes: 40,
            ..Default::default()
        };
        let byte_limited =
            capture_project_context_with_budget(repo.path(), None, byte_budget).unwrap();
        assert!(byte_limited
            .completeness
            .warnings
            .contains(&"untracked_total_budget_exceeded".to_string()));
    }

    #[test]
    fn binary_tracked_change_has_a_bounded_complete_fingerprint() {
        let repo = repo("context-binary");
        let binary = repo.path().join("src/blob.bin");
        std::fs::write(&binary, [0, 1, 2, 0, 4, 5, 6, 7]).unwrap();
        git(repo.path(), &["add", "src/blob.bin"]);
        git(repo.path(), &["commit", "-qm", "add binary"]);
        let first = capture_project_context(repo.path(), None).unwrap();
        std::fs::write(&binary, [0, 1, 9, 0, 4, 5, 6, 7]).unwrap();
        let second = capture_project_context(repo.path(), None).unwrap();

        assert!(second.completeness.complete);
        assert_ne!(first.git.worktree_sha256, second.git.worktree_sha256);
    }

    #[test]
    fn large_tracked_diff_is_partial_instead_of_read_without_limit() {
        let repo = repo("context-large-diff");
        let generated = (0..512)
            .map(|index| format!("old-{index:04}\n"))
            .collect::<String>();
        std::fs::write(repo.path().join("src/generated.txt"), generated).unwrap();
        git(repo.path(), &["add", "src/generated.txt"]);
        git(repo.path(), &["commit", "-qm", "add generated"]);
        let changed = (0..512)
            .map(|index| format!("new-{index:04}\n"))
            .collect::<String>();
        std::fs::write(repo.path().join("src/generated.txt"), changed).unwrap();
        let budget = ProjectContextBudget {
            max_tracked_diff_bytes: 64,
            ..Default::default()
        };

        let first = capture_project_context_with_budget(repo.path(), None, budget.clone()).unwrap();
        let second = capture_project_context_with_budget(repo.path(), None, budget).unwrap();
        assert!(!first.completeness.complete);
        assert!(first
            .completeness
            .warnings
            .iter()
            .any(|warning| warning == "git_output_budget_exceeded"
                || warning == "tracked_diff_budget_exceeded"));
        let refresh = compare_project_context(Some(&first), &second);
        assert!(refresh.unknown.contains(&"worktree".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn untracked_symlink_hashes_only_its_target_path() {
        use std::os::unix::fs::symlink;

        let repo = repo("context-symlink");
        let external = tempfile::tempdir().unwrap();
        let first_target = external.path().join("first.txt");
        let second_target = external.path().join("second.txt");
        std::fs::write(&first_target, "outside one").unwrap();
        std::fs::write(&second_target, "outside two").unwrap();
        let link = repo.path().join("outside-link");
        symlink(&first_target, &link).unwrap();
        let first = capture_project_context(repo.path(), None).unwrap();

        std::fs::write(&first_target, "outside content changed").unwrap();
        let target_content_changed = capture_project_context(repo.path(), None).unwrap();
        assert_eq!(
            first.git.worktree_sha256,
            target_content_changed.git.worktree_sha256
        );

        std::fs::remove_file(&link).unwrap();
        symlink(&second_target, &link).unwrap();
        let link_changed = capture_project_context(repo.path(), None).unwrap();
        assert_ne!(
            target_content_changed.git.worktree_sha256,
            link_changed.git.worktree_sha256
        );
    }

    #[test]
    fn manifest_candidates_and_fallback_scan_entries_are_bounded() {
        let repo = repo("context-manifest-budget");
        for (directory, name) in [("one", "package.json"), ("two", "go.mod")] {
            std::fs::create_dir_all(repo.path().join(directory)).unwrap();
            std::fs::write(repo.path().join(directory).join(name), "{}\n").unwrap();
        }
        let candidate_budget = ProjectContextBudget {
            max_manifest_candidates: 2,
            ..Default::default()
        };
        let candidate_limited =
            capture_project_context_with_budget(repo.path(), None, candidate_budget).unwrap();
        assert_eq!(candidate_limited.manifests.len(), 2);
        assert!(candidate_limited
            .completeness
            .warnings
            .contains(&"manifest_candidate_budget_exceeded".to_string()));

        let non_git = tempfile::tempdir().unwrap();
        std::fs::create_dir(non_git.path().join("nested")).unwrap();
        std::fs::write(non_git.path().join("nested/package.json"), "{}\n").unwrap();
        let scan_budget = ProjectContextBudget {
            max_scan_entries: 1,
            ..Default::default()
        };
        let scan_limited =
            capture_project_context_with_budget(non_git.path(), None, scan_budget).unwrap();
        assert!(scan_limited
            .completeness
            .warnings
            .contains(&"scan_entry_budget_exceeded".to_string()));
    }

    #[test]
    fn elapsed_budget_marks_incomplete_without_exposing_paths() {
        let repo = repo("context-elapsed-budget");
        let budget = ProjectContextBudget {
            max_elapsed: Duration::ZERO,
            ..Default::default()
        };
        let fingerprint = capture_project_context_with_budget(repo.path(), None, budget).unwrap();
        assert!(!fingerprint.completeness.complete);
        assert!(fingerprint
            .completeness
            .warnings
            .contains(&"scan_time_budget_exceeded".to_string()));
        assert!(fingerprint
            .completeness
            .warnings
            .iter()
            .all(|warning| !warning.contains(repo.path().to_string_lossy().as_ref())));
    }

    #[test]
    fn legacy_complete_fingerprint_deserializes_with_safe_defaults() {
        let repo = repo("context-legacy-json");
        let fingerprint = capture_project_context(repo.path(), None).unwrap();
        let mut value = serde_json::to_value(&fingerprint).unwrap();
        value.as_object_mut().unwrap().remove("completeness");
        for collection in ["rules", "manifests"] {
            for file in value[collection].as_array_mut().unwrap() {
                let file = file.as_object_mut().unwrap();
                file.remove("complete");
                file.remove("hash_kind");
                file.remove("modified_unix_nanos");
            }
        }
        let restored: ProjectContextFingerprint = serde_json::from_value(value).unwrap();
        assert!(restored.completeness.complete);
        assert!(restored.rules.iter().all(|file| file.complete));
        assert!(restored
            .manifests
            .iter()
            .all(|file| file.hash_kind == "full"));
    }
}
