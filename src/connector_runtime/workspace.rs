//! Managed Git execution workspaces and human result application.
//!
//! A writable connector task runs in a detached worktree outside the user's
//! checkout. The final patch is content-addressed and can only be applied when
//! the target checkout still matches the captured base on every changed path.

use super::ConnectorContext;
use crate::db::{ConnectorPreservedWorkspace, ConnectorTaskResult, ConnectorTaskSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

const MAX_RESULT_PATCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESULT_CHANGED_PATHS: usize = 1_000;
const WRITE_SLOT_NAME: &str = "write-slot-01";
const WRITE_SLOT_PROJECT_ID: &str = "wc-slot-write-01";
const WORKSPACE_LEASE_VERSION: u32 = 1;
const RESOURCE_SCAN_ENTRY_LIMIT: usize = 250_000;

#[derive(Debug, Clone)]
pub(crate) struct PreparedWorkspace {
    pub run_id: String,
    pub agent_client_id: String,
    pub agent_project_id: String,
    pub execution_executor_ref: String,
    pub execution_root: String,
    pub baseline_commit: Option<String>,
    pub baseline_tree: Option<String>,
    pub isolated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WorkspaceLease {
    schema_version: u32,
    slot: String,
    task_id: String,
    run_id: String,
    baseline_commit: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedResult {
    pub patch_artifact: Option<String>,
    pub patch_sha256: Option<String>,
    pub patch_bytes: usize,
    pub changed_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PatchPreview {
    pub text: String,
    pub shown_bytes: usize,
    pub total_bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionOutcome {
    pub cleanup_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DirectoryUsage {
    pub bytes: u64,
    pub entries: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceResourceStatus {
    pub writable_slot: String,
    pub slot_state: String,
    pub occupied_task_id: Option<String>,
    pub occupied_run_id: Option<String>,
    pub checkout: DirectoryUsage,
    pub cargo_cache: DirectoryUsage,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceManager {
    runs_root: PathBuf,
    results_root: PathBuf,
    projects_dir: PathBuf,
}

impl WorkspaceManager {
    pub(crate) fn new(context: &ConnectorContext) -> Result<Self, String> {
        let manager = Self {
            runs_root: PathBuf::from(&context.runs_root),
            results_root: PathBuf::from(&context.results_root),
            projects_dir: PathBuf::from(&context.projects_dir),
        };
        for (label, path) in [
            ("runs root", &manager.runs_root),
            ("results root", &manager.results_root),
            ("agent projects directory", &manager.projects_dir),
        ] {
            if !path.is_absolute() || path == Path::new("/") {
                return Err(format!(
                    "connector {label} must be an absolute non-root path"
                ));
            }
        }
        let target = lexical_normalize(Path::new(&context.executor_root));
        for (label, path) in [
            ("runs root", &manager.runs_root),
            ("results root", &manager.results_root),
            ("agent projects directory", &manager.projects_dir),
        ] {
            if lexical_normalize(path).starts_with(&target) {
                return Err(format!(
                    "connector {label} must be outside the target checkout"
                ));
            }
        }
        Ok(manager)
    }

    pub(crate) fn prepare(
        &self,
        context: &ConnectorContext,
        task_id: &str,
        run_id: &str,
        read_only: bool,
    ) -> Result<PreparedWorkspace, String> {
        let (client_id, _) = parse_agent_executor_ref(&context.executor_project)?;
        let baseline_commit = git_text(
            Path::new(&context.executor_root),
            ["rev-parse", "--verify", "HEAD^{commit}"],
        )
        .ok();
        let baseline_tree = baseline_commit.as_deref().and_then(|commit| {
            git_text(
                Path::new(&context.executor_root),
                ["rev-parse", &format!("{commit}^{{tree}}")],
            )
            .ok()
        });
        if read_only {
            return Ok(PreparedWorkspace {
                run_id: run_id.to_string(),
                agent_client_id: client_id,
                agent_project_id: String::new(),
                execution_executor_ref: context.executor_project.clone(),
                execution_root: context.executor_root.clone(),
                baseline_commit,
                baseline_tree,
                isolated: false,
            });
        }
        let baseline_commit = baseline_commit.ok_or_else(|| {
            "writable tasks require a Git project with a valid HEAD commit".to_string()
        })?;
        let baseline_tree = baseline_tree
            .ok_or_else(|| "writable tasks require a readable Git baseline tree".to_string())?;
        create_private_dir(&self.runs_root)?;
        create_private_dir(&self.results_root)?;
        create_private_dir(&self.projects_dir)?;
        let execution_root = self.runs_root.join(WRITE_SLOT_NAME);
        ensure_direct_child(&self.runs_root, &execution_root)?;
        let lease_path = workspace_lease_path(&self.runs_root, WRITE_SLOT_NAME);
        claim_workspace_lease(
            &lease_path,
            &WorkspaceLease {
                schema_version: WORKSPACE_LEASE_VERSION,
                slot: WRITE_SLOT_NAME.to_string(),
                task_id: task_id.to_string(),
                run_id: run_id.to_string(),
                baseline_commit: baseline_commit.clone(),
            },
        )?;
        let preparation = (|| {
            let target_root = Path::new(&context.executor_root);
            let _ = git_output(target_root, [OsStr::new("worktree"), OsStr::new("prune")]);
            if execution_root.exists() {
                reset_managed_slot(target_root, &execution_root, &baseline_commit)
            } else {
                let output = git_output(
                    target_root,
                    [
                        OsStr::new("worktree"),
                        OsStr::new("add"),
                        OsStr::new("--detach"),
                        execution_root.as_os_str(),
                        OsStr::new(&baseline_commit),
                    ],
                )?;
                require_success(output, "create reusable execution worktree")?;
                verify_managed_slot(target_root, &execution_root)
            }
        })();
        if let Err(error) = preparation {
            let _ = fs::remove_file(&lease_path);
            return Err(error);
        }
        Ok(PreparedWorkspace {
            run_id: run_id.to_string(),
            agent_client_id: client_id.clone(),
            agent_project_id: WRITE_SLOT_PROJECT_ID.to_string(),
            execution_executor_ref: format!("agent:{client_id}:{WRITE_SLOT_PROJECT_ID}"),
            execution_root: execution_root.to_string_lossy().to_string(),
            baseline_commit: Some(baseline_commit),
            baseline_tree: Some(baseline_tree),
            isolated: true,
        })
    }

    pub(crate) fn discard_prepared(
        &self,
        target_root: &str,
        prepared: &PreparedWorkspace,
    ) -> Option<String> {
        if !prepared.isolated {
            return None;
        }
        release_workspace_slot(
            Path::new(target_root),
            Path::new(&prepared.execution_root),
            &self.runs_root,
            &self.projects_dir,
            &prepared.agent_project_id,
            &prepared.run_id,
        )
    }

    pub(crate) fn capture_result(
        &self,
        task: &ConnectorTaskSnapshot,
    ) -> Result<CapturedResult, String> {
        if !task.isolated {
            return Ok(CapturedResult {
                patch_artifact: None,
                patch_sha256: None,
                patch_bytes: 0,
                changed_paths: Vec::new(),
                warnings: vec!["read_only task has no isolated writable result patch".to_string()],
            });
        }
        let execution_root = Path::new(&task.execution_root);
        create_private_dir(&self.results_root)?;
        ensure_direct_child(&self.runs_root, execution_root)?;
        let baseline = task
            .baseline_commit
            .as_deref()
            .ok_or_else(|| "isolated run is missing its baseline commit".to_string())?;

        require_success(
            git_output(
                execution_root,
                [OsStr::new("add"), OsStr::new("-A"), OsStr::new("--")],
            )?,
            "stage isolated result snapshot",
        )?;
        let names = require_success(
            git_output(
                execution_root,
                [
                    OsStr::new("diff"),
                    OsStr::new("--cached"),
                    OsStr::new("--name-only"),
                    OsStr::new("-z"),
                    OsStr::new(baseline),
                    OsStr::new("--"),
                ],
            )?,
            "enumerate isolated result paths",
        )?;
        let changed_paths = parse_nul_paths(&names.stdout)?;
        if changed_paths.len() > MAX_RESULT_CHANGED_PATHS {
            return Err(format!(
                "result changes {} paths; maximum is {MAX_RESULT_CHANGED_PATHS}",
                changed_paths.len()
            ));
        }
        if let Some(path) = changed_paths
            .iter()
            .find(|path| sensitive_result_path(path))
        {
            return Err(format!(
                "result contains protected path '{path}'; remove it before task_finish"
            ));
        }
        let patch = require_success(
            git_output(
                execution_root,
                [
                    OsStr::new("diff"),
                    OsStr::new("--cached"),
                    OsStr::new("--binary"),
                    OsStr::new("--full-index"),
                    OsStr::new(baseline),
                    OsStr::new("--"),
                ],
            )?,
            "capture isolated result patch",
        )?
        .stdout;
        if patch.len() > MAX_RESULT_PATCH_BYTES {
            return Err(format!(
                "result patch is {} bytes; maximum is {MAX_RESULT_PATCH_BYTES}",
                patch.len()
            ));
        }
        if patch.is_empty() {
            return Ok(CapturedResult {
                patch_artifact: None,
                patch_sha256: None,
                patch_bytes: 0,
                changed_paths,
                warnings: vec!["task finished without code changes".to_string()],
            });
        }
        let patch_sha256 = format!("{:x}", Sha256::digest(&patch));
        let artifact = self.results_root.join(format!("{}.patch", task.task_id));
        ensure_direct_child(&self.results_root, &artifact)?;
        write_private_atomic(&artifact, &patch)?;
        Ok(CapturedResult {
            patch_artifact: Some(artifact.to_string_lossy().to_string()),
            patch_sha256: Some(patch_sha256),
            patch_bytes: patch.len(),
            changed_paths,
            warnings: Vec::new(),
        })
    }

    /// Release the execution slot only after the stable Result has committed.
    /// The lease binds cleanup to one run, so an older Result can never clean a
    /// slot that has already been reused by a newer task.
    pub(crate) fn release_task_workspace(&self, task: &ConnectorTaskSnapshot) -> Option<String> {
        if !task.isolated {
            return None;
        }
        let (_, project_id) = match parse_agent_executor_ref(&task.execution_executor_ref) {
            Ok(value) => value,
            Err(error) => return Some(error),
        };
        let execution_root = Path::new(&task.execution_root);
        if execution_root.file_name() == Some(OsStr::new(WRITE_SLOT_NAME)) {
            return release_workspace_slot(
                Path::new(&task.target_root),
                execution_root,
                &self.runs_root,
                &self.projects_dir,
                &project_id,
                &task.run_id,
            );
        }
        cleanup_workspace(
            Path::new(&task.target_root),
            execution_root,
            &self.runs_root,
            &self.projects_dir,
            &project_id,
        )
    }

    /// Preserve interrupted task workspaces and reclaim only connector-owned
    /// leftovers. Reusable slots remain checked out but idle; legacy per-run
    /// worktrees are removed once no interrupted Run owns them.
    pub(crate) fn recover(
        &self,
        context: &ConnectorContext,
        preserved: &[ConnectorPreservedWorkspace],
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Err(error) = create_private_dir(&self.runs_root) {
            warnings.push(error);
            return warnings;
        }
        if let Err(error) = create_private_dir(&self.projects_dir) {
            warnings.push(error);
            return warnings;
        }
        let target_root = Path::new(&context.executor_root);
        let preserved_roots = preserved
            .iter()
            .map(|workspace| lexical_normalize(Path::new(&workspace.execution_root)))
            .collect::<HashSet<_>>();
        let preserved_projects = preserved
            .iter()
            .filter_map(|workspace| {
                parse_agent_executor_ref(&workspace.execution_executor_ref)
                    .ok()
                    .map(|(_, project_id)| project_id)
            })
            .collect::<HashSet<_>>();

        let slot_root = self.runs_root.join(WRITE_SLOT_NAME);
        let lease_path = workspace_lease_path(&self.runs_root, WRITE_SLOT_NAME);
        let preserved_slot = preserved.iter().find(|workspace| {
            lexical_normalize(Path::new(&workspace.execution_root)) == lexical_normalize(&slot_root)
        });
        if let Some(workspace) = preserved_slot {
            if !lease_path.exists() {
                match workspace.baseline_commit.as_deref() {
                    Some(baseline_commit) => {
                        if let Err(error) = claim_workspace_lease(
                            &lease_path,
                            &WorkspaceLease {
                                schema_version: WORKSPACE_LEASE_VERSION,
                                slot: WRITE_SLOT_NAME.to_string(),
                                task_id: workspace.task_id.clone(),
                                run_id: workspace.run_id.clone(),
                                baseline_commit: baseline_commit.to_string(),
                            },
                        ) {
                            warnings.push(format!(
                                "could not restore interrupted workspace lease: {error}"
                            ));
                        }
                    }
                    None => warnings.push(
                        "interrupted writable task is missing its baseline commit".to_string(),
                    ),
                }
            } else if let Ok(lease) = read_workspace_lease(&lease_path) {
                if lease.run_id != workspace.run_id {
                    warnings.push(
                        "reusable workspace lease does not match its interrupted run".to_string(),
                    );
                }
            }
        } else if !preserved_roots.contains(&lexical_normalize(&slot_root)) {
            if lease_path.exists() {
                match read_workspace_lease(&lease_path) {
                    Ok(lease) => {
                        if let Some(warning) = release_workspace_slot(
                            target_root,
                            &slot_root,
                            &self.runs_root,
                            &self.projects_dir,
                            WRITE_SLOT_PROJECT_ID,
                            &lease.run_id,
                        ) {
                            warnings.push(warning);
                        }
                    }
                    Err(error) => {
                        if slot_root.exists() {
                            let baseline =
                                git_text(target_root, ["rev-parse", "--verify", "HEAD^{commit}"]);
                            if let Err(cleanup) = baseline.and_then(|baseline| {
                                reset_managed_slot(target_root, &slot_root, &baseline)
                            }) {
                                warnings.push(format!(
                                    "could not reclaim malformed workspace lease ({error}): {cleanup}"
                                ));
                                return warnings;
                            }
                        }
                        let config = self
                            .projects_dir
                            .join(format!("{WRITE_SLOT_PROJECT_ID}.toml"));
                        if let Err(cleanup) = remove_file_if_exists(&config) {
                            warnings.push(cleanup);
                        } else if let Err(cleanup) = fs::remove_file(&lease_path) {
                            warnings.push(format!(
                                "could not remove malformed workspace lease: {cleanup}"
                            ));
                        }
                    }
                }
            } else if slot_root.exists() {
                match git_text(target_root, ["rev-parse", "--verify", "HEAD^{commit}"])
                    .and_then(|baseline| reset_managed_slot(target_root, &slot_root, &baseline))
                {
                    Ok(()) => {
                        let config = self
                            .projects_dir
                            .join(format!("{WRITE_SLOT_PROJECT_ID}.toml"));
                        if let Err(error) = remove_file_if_exists(&config) {
                            warnings.push(error);
                        }
                    }
                    Err(error) => warnings.push(error),
                }
            }
        }

        if let Ok(entries) = fs::read_dir(&self.runs_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !name.starts_with("wc_run_")
                    || preserved_roots.contains(&lexical_normalize(&path))
                {
                    continue;
                }
                let suffix = name.strip_prefix("wc_run_").unwrap_or(name);
                let short = suffix.chars().take(24).collect::<String>();
                let project_id = format!("wc-run-{short}");
                if let Some(warning) = cleanup_workspace(
                    target_root,
                    &path,
                    &self.runs_root,
                    &self.projects_dir,
                    &project_id,
                ) {
                    warnings.push(warning);
                }
            }
        }

        if let Ok(entries) = fs::read_dir(&self.projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if !(stem.starts_with("wc-run-") || stem.starts_with("wc-slot-"))
                    || preserved_projects.contains(stem)
                {
                    continue;
                }
                if let Err(error) = remove_file_if_exists(&path) {
                    warnings.push(error);
                }
            }
        }
        let _ = git_output(target_root, [OsStr::new("worktree"), OsStr::new("prune")]);
        warnings
    }

    pub(crate) fn resource_status(
        runs_root: &Path,
        cargo_target: &Path,
    ) -> WorkspaceResourceStatus {
        let slot_root = runs_root.join(WRITE_SLOT_NAME);
        let lease_path = workspace_lease_path(runs_root, WRITE_SLOT_NAME);
        let lease = read_workspace_lease(&lease_path).ok();
        let slot_state = if lease_path.exists() {
            "occupied"
        } else if slot_root.exists() {
            "idle"
        } else {
            "uninitialized"
        };
        WorkspaceResourceStatus {
            writable_slot: WRITE_SLOT_NAME.to_string(),
            slot_state: slot_state.to_string(),
            occupied_task_id: lease.as_ref().map(|lease| lease.task_id.clone()),
            occupied_run_id: lease.as_ref().map(|lease| lease.run_id.clone()),
            checkout: directory_usage(&slot_root, RESOURCE_SCAN_ENTRY_LIMIT),
            cargo_cache: directory_usage(cargo_target, RESOURCE_SCAN_ENTRY_LIMIT),
        }
    }

    pub(crate) fn action_precondition(
        &self,
        task: &ConnectorTaskSnapshot,
    ) -> Result<String, String> {
        let execution_root = Path::new(&task.execution_root);
        if task.isolated {
            ensure_direct_child(&self.runs_root, execution_root)?;
        } else if execution_root != Path::new(&task.target_root) {
            return Err("task execution root does not match its recorded workspace".to_string());
        }
        let head = git_text(execution_root, ["rev-parse", "--verify", "HEAD^{commit}"])?;
        let index_tree = git_text(execution_root, ["write-tree"])?;
        create_private_dir(&self.results_root)?;
        let index = self
            .results_root
            .join(format!("approval-index-{}", uuid::Uuid::new_v4().simple()));
        ensure_direct_child(&self.results_root, &index)?;
        let capture = (|| {
            require_success(
                git_output_with_index(
                    execution_root,
                    &index,
                    [OsStr::new("read-tree"), OsStr::new("HEAD")],
                )?,
                "initialize approval precondition",
            )?;
            require_success(
                git_output_with_index(
                    execution_root,
                    &index,
                    [OsStr::new("add"), OsStr::new("-A"), OsStr::new("--")],
                )?,
                "capture approval precondition",
            )?;
            let tree = require_success(
                git_output_with_index(execution_root, &index, [OsStr::new("write-tree")])?,
                "finalize approval precondition",
            )?;
            String::from_utf8(tree.stdout)
                .map(|value| value.trim().to_string())
                .map_err(|_| "Git returned a non-UTF-8 approval precondition".to_string())
        })();
        let _ = fs::remove_file(&index);
        let worktree = capture?;
        if worktree.is_empty() {
            return Err("Git returned an empty approval precondition".to_string());
        }
        let mut hasher = Sha256::new();
        for value in [&head, &index_tree, &worktree] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub(crate) fn accept(
        task: &ConnectorTaskSnapshot,
        result: &ConnectorTaskResult,
    ) -> Result<DecisionOutcome, String> {
        if result.decision_status != "pending" || task.task_status != "ready_for_review" {
            return Err("task result is not pending human review".to_string());
        }
        if !task.isolated {
            if read_verified_patch(result)?.is_some() || !result.changed_paths.is_empty() {
                return Err("read-only task result unexpectedly contains changes".to_string());
            }
            return Ok(DecisionOutcome {
                cleanup_warning: None,
            });
        }
        let baseline = task
            .baseline_commit
            .as_deref()
            .ok_or_else(|| "task result has no baseline commit".to_string())?;
        let target_root = Path::new(&task.target_root);
        let current_head = git_text(target_root, ["rev-parse", "--verify", "HEAD^{commit}"])?;
        if current_head != baseline {
            return Err(format!(
                "target HEAD changed since task start (expected {}, found {}); result was not applied",
                short_oid(baseline),
                short_oid(&current_head)
            ));
        }
        if !result.changed_paths.is_empty() {
            let mut args = vec![
                OsStr::new("status"),
                OsStr::new("--porcelain=v1"),
                OsStr::new("-z"),
                OsStr::new("--untracked-files=all"),
                OsStr::new("--"),
            ];
            args.extend(result.changed_paths.iter().map(OsStr::new));
            let status = require_success(
                git_output(target_root, args)?,
                "check target changed-path preconditions",
            )?;
            if !status.stdout.is_empty() {
                return Err(
                    "target checkout has local changes on result paths; result was not applied"
                        .to_string(),
                );
            }
        }
        if let Some(patch) = read_verified_patch(result)? {
            for mode in ["--check", "--apply"] {
                let mut command = Command::new("git");
                command
                    .arg("-C")
                    .arg(target_root)
                    .arg("apply")
                    .arg("--binary");
                if mode == "--check" {
                    command.arg("--check");
                }
                command.arg("-");
                let mut child = command
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|error| format!("cannot start git apply: {error}"))?;
                child
                    .stdin
                    .as_mut()
                    .ok_or_else(|| "git apply stdin was unavailable".to_string())?
                    .write_all(&patch)
                    .map_err(|error| format!("cannot send result patch to git apply: {error}"))?;
                let output = child
                    .wait_with_output()
                    .map_err(|error| format!("cannot wait for git apply: {error}"))?;
                require_success(
                    output,
                    if mode == "--check" {
                        "check result patch"
                    } else {
                        "apply result patch"
                    },
                )?;
            }
        }
        Ok(DecisionOutcome {
            cleanup_warning: cleanup_task_workspace(task),
        })
    }

    pub(crate) fn patch_preview(
        result: &ConnectorTaskResult,
        max_bytes: usize,
    ) -> Result<Option<PatchPreview>, String> {
        if max_bytes == 0 || max_bytes > MAX_RESULT_PATCH_BYTES {
            return Err("patch preview bound is invalid".to_string());
        }
        let Some(patch) = read_verified_patch(result)? else {
            return Ok(None);
        };
        let shown_bytes = patch.len().min(max_bytes);
        Ok(Some(PatchPreview {
            text: String::from_utf8_lossy(&patch[..shown_bytes]).to_string(),
            shown_bytes,
            total_bytes: patch.len(),
            truncated: shown_bytes < patch.len(),
        }))
    }

    pub(crate) fn reject(
        task: &ConnectorTaskSnapshot,
        result: &ConnectorTaskResult,
    ) -> Result<DecisionOutcome, String> {
        if result.decision_status != "pending" || task.task_status != "ready_for_review" {
            return Err("task result is not pending human review".to_string());
        }
        Ok(DecisionOutcome {
            cleanup_warning: cleanup_task_workspace(task),
        })
    }

    pub(crate) fn release_owned_task_workspace(task: &ConnectorTaskSnapshot) -> DecisionOutcome {
        DecisionOutcome {
            cleanup_warning: cleanup_task_workspace(task),
        }
    }

    pub(crate) fn validate_resume(
        task: &ConnectorTaskSnapshot,
        runs_root: &Path,
        projects_dir: &Path,
    ) -> Result<(), String> {
        if task.run_status != "interrupted" || task.task_status != "needs_attention" {
            return Err("only an interrupted task can be resumed".to_string());
        }
        let execution_root = Path::new(&task.execution_root);
        if task.isolated {
            ensure_direct_child(runs_root, execution_root)?;
            if !execution_root.is_dir() {
                return Err("interrupted execution worktree is no longer available".to_string());
            }
            let (_, project_id) = parse_agent_executor_ref(&task.execution_executor_ref)?;
            if !projects_dir.join(format!("{project_id}.toml")).is_file() {
                return Err(
                    "interrupted execution project registration is no longer available".to_string(),
                );
            }
            let inside = git_text(execution_root, ["rev-parse", "--is-inside-work-tree"])?;
            if inside != "true" {
                return Err("interrupted execution root is not a Git worktree".to_string());
            }
        } else if execution_root != Path::new(&task.target_root) || !execution_root.is_dir() {
            return Err("read-only task workspace is no longer available".to_string());
        }
        Ok(())
    }
}

fn read_verified_patch(result: &ConnectorTaskResult) -> Result<Option<Vec<u8>>, String> {
    let Some(artifact) = result.patch_artifact.as_deref() else {
        if result.patch_bytes == 0 && result.patch_sha256.is_none() {
            return Ok(None);
        }
        return Err("result patch metadata is incomplete".to_string());
    };
    let patch = fs::read(artifact)
        .map_err(|error| format!("cannot read result patch artifact: {error}"))?;
    if patch.len() != result.patch_bytes {
        return Err("result patch size no longer matches its recorded value".to_string());
    }
    let hash = format!("{:x}", Sha256::digest(&patch));
    if result.patch_sha256.as_deref() != Some(hash.as_str()) {
        return Err("result patch hash no longer matches its recorded value".to_string());
    }
    Ok(Some(patch))
}

fn cleanup_task_workspace(task: &ConnectorTaskSnapshot) -> Option<String> {
    if !task.isolated {
        return None;
    }
    let (_, project_id) = parse_agent_executor_ref(&task.execution_executor_ref).ok()?;
    let execution_root = Path::new(&task.execution_root);
    let runs_root = execution_root.parent()?;
    let projects_dir = runs_root.parent()?.join("agent/projects.d");
    if execution_root.file_name() == Some(OsStr::new(WRITE_SLOT_NAME)) {
        return release_workspace_slot(
            Path::new(&task.target_root),
            execution_root,
            runs_root,
            &projects_dir,
            &project_id,
            &task.run_id,
        );
    }
    cleanup_workspace(
        Path::new(&task.target_root),
        execution_root,
        runs_root,
        &projects_dir,
        &project_id,
    )
}

fn workspace_lease_path(runs_root: &Path, slot_name: &str) -> PathBuf {
    runs_root.join(format!(".{slot_name}.lease.json"))
}

fn claim_workspace_lease(path: &Path, lease: &WorkspaceLease) -> Result<(), String> {
    let bytes = serde_json::to_vec(lease)
        .map_err(|error| format!("cannot serialize workspace lease: {error}"))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(
                "the reusable writable workspace slot is occupied; finish or resume its task before starting another writable task"
                    .to_string(),
            )
        }
        Err(error) => return Err(format!("cannot claim writable workspace slot: {error}")),
    };
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(path);
        return Err(format!("cannot persist workspace lease: {error}"));
    }
    Ok(())
}

fn read_workspace_lease(path: &Path) -> Result<WorkspaceLease, String> {
    let bytes = fs::read(path).map_err(|error| format!("cannot read workspace lease: {error}"))?;
    let lease = serde_json::from_slice::<WorkspaceLease>(&bytes)
        .map_err(|error| format!("workspace lease is malformed: {error}"))?;
    if lease.schema_version != WORKSPACE_LEASE_VERSION
        || lease.slot != WRITE_SLOT_NAME
        || !lease.task_id.starts_with("wc_task_")
        || !lease.run_id.starts_with("wc_run_")
        || lease.baseline_commit.is_empty()
    {
        return Err("workspace lease contains invalid ownership metadata".to_string());
    }
    Ok(lease)
}

fn release_workspace_slot(
    target_root: &Path,
    execution_root: &Path,
    runs_root: &Path,
    projects_dir: &Path,
    agent_project_id: &str,
    expected_run_id: &str,
) -> Option<String> {
    if ensure_direct_child(runs_root, execution_root).is_err()
        || execution_root.file_name() != Some(OsStr::new(WRITE_SLOT_NAME))
    {
        return Some("refused to release a workspace outside the managed write slot".to_string());
    }
    let lease_path = workspace_lease_path(runs_root, WRITE_SLOT_NAME);
    if !lease_path.exists() {
        return None;
    }
    let lease = match read_workspace_lease(&lease_path) {
        Ok(lease) => lease,
        Err(error) => return Some(error),
    };
    if lease.run_id != expected_run_id {
        return None;
    }
    if execution_root.exists() {
        if let Err(error) = reset_managed_slot(target_root, execution_root, &lease.baseline_commit)
        {
            return Some(error);
        }
    }
    if !safe_agent_project_id(agent_project_id) {
        return Some("execution project registration id is invalid".to_string());
    }
    let config = projects_dir.join(format!("{agent_project_id}.toml"));
    if let Err(error) = remove_file_if_exists(&config) {
        return Some(error);
    }
    if let Err(error) = fs::remove_file(&lease_path) {
        return Some(format!("could not release workspace lease: {error}"));
    }
    None
}

fn reset_managed_slot(
    target_root: &Path,
    execution_root: &Path,
    baseline_commit: &str,
) -> Result<(), String> {
    verify_managed_slot(target_root, execution_root)?;
    require_success(
        git_output(
            execution_root,
            [
                OsStr::new("checkout"),
                OsStr::new("--detach"),
                OsStr::new("--force"),
                OsStr::new(baseline_commit),
            ],
        )?,
        "detach and reset reusable execution worktree",
    )?;
    require_success(
        git_output(execution_root, [OsStr::new("clean"), OsStr::new("-ffdx")])?,
        "clean reusable execution worktree",
    )?;
    Ok(())
}

fn verify_managed_slot(target_root: &Path, execution_root: &Path) -> Result<(), String> {
    if execution_root.file_name() != Some(OsStr::new(WRITE_SLOT_NAME)) {
        return Err("execution worktree is not a recognized managed slot".to_string());
    }
    let target_common = git_common_dir(target_root)?;
    let execution_common = git_common_dir(execution_root)?;
    if target_common != execution_common {
        return Err("managed slot does not belong to the target Git repository".to_string());
    }
    let inside = git_text(execution_root, ["rev-parse", "--is-inside-work-tree"])?;
    if inside != "true" {
        return Err("managed slot is not a Git worktree".to_string());
    }
    Ok(())
}

fn git_common_dir(root: &Path) -> Result<PathBuf, String> {
    let value = git_text(root, ["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.canonicalize()
        .map_err(|error| format!("cannot resolve Git common directory: {error}"))
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not remove managed state file: {error}")),
    }
}

fn cleanup_workspace(
    target_root: &Path,
    execution_root: &Path,
    runs_root: &Path,
    projects_dir: &Path,
    agent_project_id: &str,
) -> Option<String> {
    let mut warnings = Vec::new();
    if ensure_direct_child(runs_root, execution_root).is_ok() && execution_root.exists() {
        match git_output(
            target_root,
            [
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                execution_root.as_os_str(),
            ],
        )
        .and_then(|output| require_success(output, "remove execution worktree"))
        {
            Ok(_) => {}
            Err(error) => warnings.push(error),
        }
    }
    if safe_agent_project_id(agent_project_id) {
        let config = projects_dir.join(format!("{agent_project_id}.toml"));
        if config.exists() {
            if let Err(error) = fs::remove_file(&config) {
                warnings.push(format!(
                    "could not remove execution project registration: {error}"
                ));
            }
        }
    }
    let _ = git_output(target_root, [OsStr::new("worktree"), OsStr::new("prune")]);
    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    }
}

fn parse_agent_executor_ref(value: &str) -> Result<(String, String), String> {
    let value = value
        .strip_prefix("agent:")
        .ok_or_else(|| "connector executor is not agent-backed".to_string())?;
    let (client_id, project_id) = value
        .split_once(':')
        .ok_or_else(|| "connector executor reference is malformed".to_string())?;
    if client_id.is_empty() || !safe_agent_project_id(project_id) {
        return Err("connector executor reference is malformed".to_string());
    }
    Ok((client_id.to_string(), project_id.to_string()))
}

fn safe_agent_project_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn sensitive_result_path(path: &str) -> bool {
    if crate::tool_runtime::files::is_sensitive_artifact_path(path) {
        return true;
    }
    Path::new(path).components().any(|component| {
        let Component::Normal(name) = component else {
            return true;
        };
        let name = name.to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            "credentials" | "id_rsa" | "id_ed25519" | "agent.toml" | "webcodex.env"
        ) || name.ends_with(".key")
            || name.ends_with(".p12")
            || name.ends_with(".pfx")
    })
}

fn git_text<const N: usize>(root: &Path, args: [&str; N]) -> Result<String, String> {
    let output = git_output(root, args.map(OsStr::new))?;
    let output = require_success(output, "inspect Git workspace")?;
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|_| "Git returned non-UTF-8 metadata".to_string())
}

fn git_output<I, S>(root: &Path, args: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("cannot start Git operation: {error}"))
}

fn git_output_with_index<I, S>(root: &Path, index: &Path, args: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .arg("-C")
        .arg(root)
        .env("GIT_INDEX_FILE", index)
        .args(args)
        .output()
        .map_err(|error| format!("cannot start Git precondition operation: {error}"))
}

fn require_success(output: Output, action: &str) -> Result<Output, String> {
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = stderr
        .lines()
        .next()
        .unwrap_or("Git operation failed")
        .trim();
    Err(format!("{action} failed: {summary}"))
}

fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        let path =
            std::str::from_utf8(raw).map_err(|_| "result contains a non-UTF-8 path".to_string())?;
        if Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("Git returned an unsafe result path".to_string());
        }
        paths.push(path.to_string());
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                output.pop();
            }
            other => output.push(other.as_os_str()),
        }
    }
    output
}

fn directory_usage(root: &Path, entry_limit: usize) -> DirectoryUsage {
    if entry_limit == 0 || !root.exists() {
        return DirectoryUsage {
            bytes: 0,
            entries: 0,
            truncated: false,
        };
    }
    let mut bytes = 0_u64;
    let mut entries = 0_usize;
    let mut truncated = false;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(children) = fs::read_dir(path) else {
            continue;
        };
        for child in children.flatten() {
            if entries >= entry_limit {
                truncated = true;
                pending.clear();
                break;
            }
            entries += 1;
            let Ok(metadata) = child.path().symlink_metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(child.path());
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    }
    DirectoryUsage {
        bytes,
        entries,
        truncated,
    }
}

fn ensure_direct_child(root: &Path, path: &Path) -> Result<(), String> {
    if path.parent() != Some(root) || path.file_name().is_none() {
        return Err("managed path escaped its configured root".to_string());
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("cannot create connector state directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot secure connector state directory: {error}"))?;
    }
    Ok(())
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("cannot create result artifact: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot write result artifact: {error}"));
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot publish result artifact: {error}")
    })
}

fn short_oid(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn fixture() -> (tempfile::TempDir, ConnectorContext, WorkspaceManager) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        fs::write(root.join("README.md"), "before\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(
            &root,
            &[
                "-c",
                "user.name=WebCodex Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "initial",
            ],
        );
        let state = temp.path().join("state");
        let context = ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "project".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:project".to_string(),
            executor_root: root.to_string_lossy().to_string(),
            runs_root: state.join("runs").to_string_lossy().to_string(),
            results_root: state.join("results").to_string_lossy().to_string(),
            projects_dir: state.join("agent/projects.d").to_string_lossy().to_string(),
            profile: "personal".to_string(),
        };
        let manager = WorkspaceManager::new(&context).unwrap();
        (temp, context, manager)
    }

    fn task(context: &ConnectorContext, prepared: &PreparedWorkspace) -> ConnectorTaskSnapshot {
        ConnectorTaskSnapshot {
            task_id: "wc_task_0123456789abcdef0123456789abcdef".to_string(),
            run_id: prepared.run_id.clone(),
            project_id: context.project_id.clone(),
            workspace_id: context.workspace_id.clone(),
            owner_subject_id: "user:owner".to_string(),
            goal: "edit the readme".to_string(),
            mode: "normal".to_string(),
            task_status: "ready_for_review".to_string(),
            run_status: "completed".to_string(),
            event_cursor: 2,
            target_executor_ref: context.executor_project.clone(),
            execution_executor_ref: prepared.execution_executor_ref.clone(),
            target_root: context.executor_root.clone(),
            execution_root: prepared.execution_root.clone(),
            baseline_commit: prepared.baseline_commit.clone(),
            baseline_tree: prepared.baseline_tree.clone(),
            isolated: prepared.isolated,
        }
    }

    fn result(task: &ConnectorTaskSnapshot, captured: &CapturedResult) -> ConnectorTaskResult {
        ConnectorTaskResult {
            result_id: "wc_result_0123456789abcdef0123456789abcdef".to_string(),
            task_id: task.task_id.clone(),
            run_id: task.run_id.clone(),
            summary: "updated readme".to_string(),
            patch_artifact: captured.patch_artifact.clone(),
            patch_sha256: captured.patch_sha256.clone(),
            patch_bytes: captured.patch_bytes,
            changed_paths: captured.changed_paths.clone(),
            validation: serde_json::json!({"status": "not_run"}),
            warnings: captured.warnings.clone(),
            decision_status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
            cleanup_warning: None,
            created_at: 1,
        }
    }

    #[test]
    fn isolated_result_only_reaches_target_after_acceptance() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_0123456789abcdef0123456789abcdef",
                "wc_run_0123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        assert!(prepared.isolated);
        fs::write(
            Path::new(&prepared.execution_root).join("README.md"),
            "after\n",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&context.executor_root).join("README.md")).unwrap(),
            "before\n"
        );
        let task = task(&context, &prepared);
        let captured = manager.capture_result(&task).unwrap();
        assert_eq!(captured.changed_paths, vec!["README.md"]);
        assert!(captured.patch_bytes > 0);
        assert_eq!(manager.release_task_workspace(&task), None);
        assert!(Path::new(&prepared.execution_root).exists());

        let outcome = WorkspaceManager::accept(&task, &result(&task, &captured)).unwrap();
        assert_eq!(outcome.cleanup_warning, None);
        assert_eq!(
            fs::read_to_string(Path::new(&context.executor_root).join("README.md")).unwrap(),
            "after\n"
        );
        assert!(Path::new(&prepared.execution_root).exists());
    }

    #[test]
    fn acceptance_fails_closed_when_target_path_changed() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_1123456789abcdef0123456789abcdef",
                "wc_run_1123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        fs::write(
            Path::new(&prepared.execution_root).join("README.md"),
            "agent change\n",
        )
        .unwrap();
        let task = task(&context, &prepared);
        let captured = manager.capture_result(&task).unwrap();
        fs::write(
            Path::new(&context.executor_root).join("README.md"),
            "human change\n",
        )
        .unwrap();

        let error = WorkspaceManager::accept(&task, &result(&task, &captured)).unwrap_err();
        assert!(error.contains("local changes"));
        assert_eq!(
            fs::read_to_string(Path::new(&context.executor_root).join("README.md")).unwrap(),
            "human change\n"
        );
        WorkspaceManager::reject(&task, &result(&task, &captured)).unwrap();
    }

    #[test]
    fn command_precondition_tracks_content_without_staging_target() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_2123456789abcdef0123456789abcdef",
                "wc_run_2123456789abcdef0123456789abcdef",
                true,
            )
            .unwrap();
        let task = task(&context, &prepared);
        fs::write(
            Path::new(&context.executor_root).join("generated.txt"),
            "first\n",
        )
        .unwrap();
        let first = manager.action_precondition(&task).unwrap();
        fs::write(
            Path::new(&context.executor_root).join("generated.txt"),
            "second\n",
        )
        .unwrap();
        let second = manager.action_precondition(&task).unwrap();
        assert_ne!(first, second);
        require_success(
            git_output(
                Path::new(&context.executor_root),
                [
                    OsStr::new("rm"),
                    OsStr::new("--cached"),
                    OsStr::new("--quiet"),
                    OsStr::new("README.md"),
                ],
            )
            .unwrap(),
            "change fixture index",
        )
        .unwrap();
        let index_changed = manager.action_precondition(&task).unwrap();
        assert_ne!(second, index_changed);
        require_success(
            git_output(
                Path::new(&context.executor_root),
                [
                    OsStr::new("reset"),
                    OsStr::new("-q"),
                    OsStr::new("HEAD"),
                    OsStr::new("--"),
                    OsStr::new("README.md"),
                ],
            )
            .unwrap(),
            "restore fixture index",
        )
        .unwrap();
        let staged = require_success(
            git_output(
                Path::new(&context.executor_root),
                [
                    OsStr::new("diff"),
                    OsStr::new("--cached"),
                    OsStr::new("--quiet"),
                ],
            )
            .unwrap(),
            "check fixture index",
        );
        assert!(
            staged.is_ok(),
            "approval fingerprint must not mutate the real index"
        );
    }

    #[test]
    fn result_preview_rejects_tampered_patch_artifact() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_3123456789abcdef0123456789abcdef",
                "wc_run_3123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        fs::write(
            Path::new(&prepared.execution_root).join("README.md"),
            "preview me\n",
        )
        .unwrap();
        let task = task(&context, &prepared);
        let captured = manager.capture_result(&task).unwrap();
        let result = result(&task, &captured);
        let preview = WorkspaceManager::patch_preview(&result, 1024)
            .unwrap()
            .unwrap();
        assert!(preview.text.contains("preview me"));
        fs::write(result.patch_artifact.as_deref().unwrap(), "tampered\n").unwrap();
        let error = WorkspaceManager::patch_preview(&result, 1024).unwrap_err();
        assert!(error.contains("size") || error.contains("hash"));
        WorkspaceManager::reject(&task, &result).unwrap();
    }

    #[test]
    fn writable_slot_is_exclusive_then_reused_cleanly() {
        let (_temp, context, manager) = fixture();
        let first = manager
            .prepare(
                &context,
                "wc_task_5123456789abcdef0123456789abcdef",
                "wc_run_5123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        fs::write(
            Path::new(&first.execution_root).join("generated.txt"),
            "temporary\n",
        )
        .unwrap();
        git(
            Path::new(&first.execution_root),
            &["checkout", "-qb", "agent-branch"],
        );
        let occupied = manager
            .prepare(
                &context,
                "wc_task_6123456789abcdef0123456789abcdef",
                "wc_run_6123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap_err();
        assert!(occupied.contains("occupied"));
        let first_task = task(&context, &first);
        assert_eq!(manager.release_task_workspace(&first_task), None);
        assert_eq!(
            git_text(
                Path::new(&first.execution_root),
                ["rev-parse", "--abbrev-ref", "HEAD"]
            )
            .unwrap(),
            "HEAD"
        );

        let second = manager
            .prepare(
                &context,
                "wc_task_6123456789abcdef0123456789abcdef",
                "wc_run_6123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        assert_eq!(first.execution_root, second.execution_root);
        assert!(!Path::new(&second.execution_root)
            .join("generated.txt")
            .exists());
        assert_eq!(
            fs::read_to_string(Path::new(&second.execution_root).join("README.md")).unwrap(),
            "before\n"
        );
        let second_task = task(&context, &second);
        assert_eq!(manager.release_task_workspace(&second_task), None);
    }

    #[test]
    fn recovery_restores_missing_lease_for_interrupted_slot() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_7123456789abcdef0123456789abcdef",
                "wc_run_7123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        let lease_path = workspace_lease_path(&manager.runs_root, WRITE_SLOT_NAME);
        fs::remove_file(&lease_path).unwrap();
        let preserved = ConnectorPreservedWorkspace {
            task_id: "wc_task_7123456789abcdef0123456789abcdef".to_string(),
            run_id: prepared.run_id.clone(),
            execution_root: prepared.execution_root.clone(),
            execution_executor_ref: prepared.execution_executor_ref.clone(),
            baseline_commit: prepared.baseline_commit.clone(),
        };
        assert!(manager.recover(&context, &[preserved]).is_empty());
        assert!(lease_path.is_file());
        let occupied = manager
            .prepare(
                &context,
                "wc_task_8123456789abcdef0123456789abcdef",
                "wc_run_8123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap_err();
        assert!(occupied.contains("occupied"));
        let interrupted = task(&context, &prepared);
        assert_eq!(manager.release_task_workspace(&interrupted), None);
    }

    #[test]
    fn recovery_reclaims_unowned_slot_after_finish_crash_window() {
        let (_temp, context, manager) = fixture();
        let prepared = manager
            .prepare(
                &context,
                "wc_task_9123456789abcdef0123456789abcdef",
                "wc_run_9123456789abcdef0123456789abcdef",
                false,
            )
            .unwrap();
        fs::write(
            Path::new(&prepared.execution_root).join("leftover.txt"),
            "captured before crash\n",
        )
        .unwrap();

        assert!(manager.recover(&context, &[]).is_empty());
        let cargo_target = manager
            .runs_root
            .parent()
            .unwrap()
            .join("cache/cargo-target");
        let resources = WorkspaceManager::resource_status(&manager.runs_root, &cargo_target);
        assert_eq!(resources.slot_state, "idle");
        assert!(!Path::new(&prepared.execution_root)
            .join("leftover.txt")
            .exists());
    }
}
