use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use webcodex_core::runner_operation::RunnerProjectOperation;
use webcodex_runner_config::paths::paths_equal;

use super::super::config::RunnerPolicy;
use super::super::shell::canonicalize_existing;
use super::catalog::{
    effective_registration_source, parse_runner_project_toml, project_lineage, project_revision,
    project_root_fingerprint, project_wire_kind, run_git_bounded,
    AUTO_REGISTERED_REGISTRATION_SOURCE,
};
use super::registration::{
    bounded_project_name, build_project_toml_with_registration_source, choose_auto_project_id,
    load_project_files_for_path_resolution, projects_matching_canonical_path,
    resolve_managed_source_project, sanitized_project_basename, toml_basic_string,
    validate_model_network_project_ingress_authority, validate_project_op_id,
    validate_project_path_policy, validate_windows_project_root, write_project_toml_atomic,
    ProjectTomlWriteError,
};
use super::{project_registry_write_lock, structured_project_error_cmd, RunnerProjectFile};
use crate::{ok_cmd, CommandResult};

const MANAGED_WORKTREE_GIT_TIMEOUT: Duration = Duration::from_secs(20);

/// The slug is routing text, not recovery identity. Only a registered exact
/// operation establishes whether an occupied path is ours or a collision.
fn choose_managed_destination(
    root: &Path,
    source: &Path,
    operation_id: &str,
    projects: &[RunnerProjectFile],
) -> Result<PathBuf, &'static str> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"webcodex/managed-worktree-routing/v1\0");
    hash.update(operation_id.as_bytes());
    let digest = format!("{:x}", hash.finalize());
    for length in [8, 12, 16, 24, 32, 48, 64] {
        let candidate = root.join(format!(
            "{}-{}",
            sanitized_project_basename(source),
            &digest[..length]
        ));
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(_) => return Err("managed_worktree_recovery_conflict"),
            Ok(_) => {}
        }
        let existing =
            canonicalize_existing(&candidate).map_err(|_| "managed_worktree_recovery_conflict")?;
        let matches = projects_matching_canonical_path(projects, &existing);
        let [project] = matches.as_slice() else {
            return Err("managed_worktree_recovery_conflict");
        };
        let other = project
            .managed_operation_id
            .as_deref()
            .filter(|id| project.managed_worktree && uuid::Uuid::parse_str(id).is_ok())
            .ok_or("managed_worktree_recovery_conflict")?;
        if other == operation_id {
            return Ok(candidate);
        }
    }
    Err("managed_worktree_recovery_conflict")
}

fn managed_worktree_error(
    start: Instant,
    error_kind: &'static str,
    state_changed: bool,
    base_ref: Option<&str>,
    base_sha: Option<&str>,
    source_dirty: Option<bool>,
) -> CommandResult {
    structured_project_error_cmd(
        start,
        error_kind,
        state_changed,
        serde_json::json!({
            "base_ref": base_ref,
            "base_sha": base_sha,
            "source_dirty": source_dirty,
        }),
    )
}

fn managed_worktree_git_text(path: &Path, args: &[&str]) -> Result<String, &'static str> {
    let output = run_git_bounded(path, args, MANAGED_WORKTREE_GIT_TIMEOUT, None)
        .map_err(|_| "worktree_git_failed")?;
    if !output.status.success() || output.stdout_capped || output.stderr_capped {
        return Err("worktree_git_failed");
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|_| "worktree_git_failed")
}

fn exact_git_commit(source: &Path, base_ref: &str) -> Result<String, &'static str> {
    let commit_ref = format!("{base_ref}^{{commit}}");
    let output = run_git_bounded(
        source,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            commit_ref.as_str(),
        ],
        MANAGED_WORKTREE_GIT_TIMEOUT,
        None,
    )
    .map_err(|_| "base_ref_resolution_failed")?;
    if !output.status.success() || output.stdout_capped {
        return Err("base_ref_resolution_failed");
    }
    let sha = String::from_utf8(output.stdout)
        .map_err(|_| "base_ref_resolution_failed")?
        .trim()
        .to_string();
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("base_ref_resolution_failed");
    }
    Ok(sha.to_ascii_lowercase())
}

fn choose_managed_worktree_root(
    policy: &RunnerPolicy,
    source: &Path,
) -> Result<PathBuf, &'static str> {
    let mut roots =
        webcodex_runner_config::paths::canonicalize_usable_allowed_roots(&policy.allowed_roots);
    roots.sort_by_key(|root| std::cmp::Reverse(root.components().count()));
    for root in roots {
        if !webcodex_runner_config::paths::path_is_within(source, &root) {
            continue;
        }
        let candidate = root.join(".webpi-managed-worktrees");
        if webcodex_runner_config::paths::path_is_within(&candidate, source) {
            continue;
        }
        return Ok(candidate);
    }
    if policy.allow_cwd_anywhere {
        if let Some(parent) = source.parent() {
            let candidate = parent.join(".webpi-managed-worktrees");
            if !webcodex_runner_config::paths::path_is_within(&candidate, source) {
                return Ok(candidate);
            }
        }
    }
    Err("managed_worktree_root_unavailable")
}

/// Render an already-authorized managed-worktree destination for Git's CLI.
///
/// Rust canonicalization commonly returns `\\?\C:\...` on Windows. Win32
/// filesystem APIs accept that identity, but Git for Windows does not reliably
/// accept the verbatim-disk spelling as a `git worktree add` destination. Keep
/// canonical PathBuf values for policy, registry, recovery, and identity checks;
/// only the child-process argv gets the equivalent ordinary local-disk spelling.
pub(super) fn managed_worktree_git_cli_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            if let Prefix::VerbatimDisk(drive) = prefix.kind() {
                let mut rendered = PathBuf::from(format!("{}:\\", char::from(drive)));
                for component in components {
                    match component {
                        Component::RootDir => {}
                        Component::Normal(part) => rendered.push(part),
                        _ => return path.to_string_lossy().into_owned(),
                    }
                }
                return rendered.to_string_lossy().into_owned();
            }
        }
    }
    path.to_string_lossy().into_owned()
}

fn source_mentions_worktree_path(source: &Path, worktree: &Path) -> Result<bool, &'static str> {
    let listing = managed_worktree_git_text(source, &["worktree", "list", "--porcelain"])?;
    Ok(listing
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|path| paths_equal(Path::new(path), worktree)))
}

fn managed_worktree_project_toml(
    id: &str,
    name: &str,
    worktree: &str,
    source: &str,
    source_project_id: &str,
    source_root_fingerprint: &str,
    base_ref: &str,
    base_sha: &str,
    operation_id: &str,
) -> String {
    let mut content = build_project_toml_with_registration_source(
        id,
        name,
        worktree,
        Some(AUTO_REGISTERED_REGISTRATION_SOURCE),
        &None,
        true,
    );
    content.push_str("managed_worktree = true\n");
    content.push_str(&format!("managed_source = {}\n", toml_basic_string(source)));
    content.push_str(&format!(
        "managed_source_project_id = {}\n",
        toml_basic_string(source_project_id)
    ));
    content.push_str(&format!(
        "managed_source_root_fingerprint = {}\n",
        toml_basic_string(source_root_fingerprint)
    ));
    content.push_str(&format!(
        "managed_base_ref = {}\n",
        toml_basic_string(base_ref)
    ));
    content.push_str(&format!(
        "managed_base_sha = {}\n",
        toml_basic_string(base_sha)
    ));
    content.push_str(&format!(
        "managed_operation_id = {}\n",
        toml_basic_string(operation_id)
    ));
    content
}

fn managed_worktree_success(
    start: Instant,
    client_id: &str,
    project: &RunnerProjectFile,
    worktree: &Path,
    base_ref: &str,
    base_sha: &str,
    source_dirty: bool,
    outcome: &'static str,
    registered: bool,
    changed: bool,
) -> CommandResult {
    ok_cmd(
        start,
        serde_json::json!({
            "id": format!("agent:{}:{}", client_id, project.id),
            "agent_project_id": project.id,
            "client_id": client_id,
            "name": project.name,
            "path": worktree.to_string_lossy(),
            "kind": project_wire_kind(project),
            "registration_source": effective_registration_source(project).as_str(),
            "description": project.description,
            "allow_patch": project.allow_patch,
            "disabled": project.disabled,
            "revision": project_revision(project),
            "root_fingerprint": project_root_fingerprint(worktree),
            "lineage": project_lineage(project),
            "source": "managed_worktree",
            "outcome": outcome,
            "registered": registered,
            "created_config": registered,
            "changed": changed,
            "recovered": outcome == "managed_worktree_recovered",
            "managed": true,
            "base_ref": base_ref,
            "base_sha": base_sha,
            "source_dirty": source_dirty,
        }),
    )
}

fn resume_managed_worktree(
    start: Instant,
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    client_id: &str,
    source_root: &Path,
    source_dirty: bool,
    requested_base_ref: Option<&str>,
    resume_project_id: &str,
) -> CommandResult {
    if validate_project_op_id(resume_project_id).is_err() {
        return managed_worktree_error(
            start,
            "invalid_request",
            false,
            requested_base_ref,
            None,
            Some(source_dirty),
        );
    }
    let projects = match load_project_files_for_path_resolution(project_registry_dir) {
        Ok(projects) => projects,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                requested_base_ref,
                None,
                Some(source_dirty),
            )
        }
    };
    let Some(project) = projects
        .iter()
        .find(|project| project.id == resume_project_id)
        .cloned()
    else {
        return managed_worktree_error(
            start,
            "managed_worktree_resume_mismatch",
            false,
            requested_base_ref,
            None,
            Some(source_dirty),
        );
    };
    let stored_base_sha = project.managed_base_sha.as_deref().filter(|sha| {
        matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let stored_source = project
        .managed_source
        .as_deref()
        .and_then(|path| canonicalize_existing(Path::new(path)).ok());
    if project.disabled
        || !project.managed_worktree
        || stored_base_sha.is_none()
        || stored_source
            .as_ref()
            .is_none_or(|source| !paths_equal(source, source_root))
    {
        return managed_worktree_error(
            start,
            "managed_worktree_resume_mismatch",
            false,
            requested_base_ref,
            stored_base_sha,
            Some(source_dirty),
        );
    }
    if let (Some(stored_source_project_id), Some(stored_source_root_fingerprint)) = (
        project.managed_source_project_id.as_deref(),
        project.managed_source_root_fingerprint.as_deref(),
    ) {
        let current_lineage = resolve_managed_source_project(&projects, source_root);
        if current_lineage.as_ref().is_err()
            || current_lineage
                .as_ref()
                .is_ok_and(|(source_project_id, source_root_fingerprint)| {
                    source_project_id != stored_source_project_id
                        || source_root_fingerprint != stored_source_root_fingerprint
                })
        {
            return managed_worktree_error(
                start,
                "managed_worktree_resume_mismatch",
                false,
                requested_base_ref,
                stored_base_sha,
                Some(source_dirty),
            );
        }
    }
    let base_sha = stored_base_sha.expect("validated managed base SHA");
    if let Some(base_ref) = requested_base_ref {
        match exact_git_commit(source_root, base_ref) {
            Ok(resolved) if resolved == base_sha => {}
            Ok(_) => {
                return managed_worktree_error(
                    start,
                    "managed_worktree_resume_base_mismatch",
                    false,
                    Some(base_ref),
                    Some(base_sha),
                    Some(source_dirty),
                )
            }
            Err(error) => {
                return managed_worktree_error(
                    start,
                    error,
                    false,
                    Some(base_ref),
                    Some(base_sha),
                    Some(source_dirty),
                )
            }
        }
    }
    let worktree = match canonicalize_existing(Path::new(&project.path)) {
        Ok(worktree) => worktree,
        Err(_) => {
            return managed_worktree_error(
                start,
                "managed_worktree_recovery_conflict",
                false,
                project.managed_base_ref.as_deref(),
                Some(base_sha),
                Some(source_dirty),
            )
        }
    };
    if validate_windows_project_root(&worktree).is_err()
        || validate_project_path_policy(policy, &worktree).is_err()
    {
        return managed_worktree_error(
            start,
            "managed_worktree_recovery_conflict",
            false,
            project.managed_base_ref.as_deref(),
            Some(base_sha),
            Some(source_dirty),
        );
    }
    let head = match managed_worktree_git_text(&worktree, &["rev-parse", "HEAD"]) {
        Ok(head) => head,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                project.managed_base_ref.as_deref(),
                Some(base_sha),
                Some(source_dirty),
            )
        }
    };
    let listed = match source_mentions_worktree_path(source_root, &worktree) {
        Ok(listed) => listed,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                project.managed_base_ref.as_deref(),
                Some(base_sha),
                Some(source_dirty),
            )
        }
    };
    if head != base_sha || !listed {
        return managed_worktree_error(
            start,
            "managed_worktree_recovery_conflict",
            false,
            project.managed_base_ref.as_deref(),
            Some(base_sha),
            Some(source_dirty),
        );
    }
    let projected_base_ref = project
        .managed_base_ref
        .as_deref()
        .unwrap_or(requested_base_ref.unwrap_or("HEAD"));
    managed_worktree_success(
        start,
        client_id,
        &project,
        &worktree,
        projected_base_ref,
        base_sha,
        source_dirty,
        "managed_worktree_recovered",
        false,
        false,
    )
}

/// Internal Server↔Runner operation that owns Git/ref/path semantics for managed
/// worktrees. It is intentionally not model-visible; successful output is fed
/// back through the ordinary registered runtime Project authority path.
pub(crate) fn handle_prepare_managed_worktree_operation(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    client_id: &str,
    operation: &RunnerProjectOperation,
) -> CommandResult {
    let start = Instant::now();
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return managed_worktree_error(start, "operation_failed", false, None, None, None)
        }
    };
    let Some(payload) = serde_json::from_str::<serde_json::Value>(&operation.payload)
        .ok()
        .and_then(|payload| payload.as_object().cloned())
    else {
        return managed_worktree_error(start, "invalid_request", false, None, None, None);
    };
    if payload.len() != 4 {
        return managed_worktree_error(start, "invalid_request", false, None, None, None);
    }
    let Some(path) = payload
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty() && !path.contains('\0') && Path::new(path).is_absolute())
    else {
        return managed_worktree_error(start, "invalid_project_path", false, None, None, None);
    };
    let requested_base_ref = match payload.get("base_ref") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value))
            if !value.trim().is_empty() && value.len() <= 1024 && !value.contains('\0') =>
        {
            Some(value.clone())
        }
        _ => return managed_worktree_error(start, "invalid_base_ref", false, None, None, None),
    };
    let base_ref = requested_base_ref
        .clone()
        .unwrap_or_else(|| "HEAD".to_string());
    let resume_project_id = match payload.get("resume_project_id") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => Some(value.as_str()),
        _ => {
            return managed_worktree_error(
                start,
                "invalid_request",
                false,
                Some(&base_ref),
                None,
                None,
            )
        }
    };
    let Some(operation_id) = payload
        .get("operation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
    else {
        return managed_worktree_error(
            start,
            "invalid_request",
            false,
            Some(&base_ref),
            None,
            None,
        );
    };
    if validate_windows_project_root(Path::new(path)).is_err() {
        return managed_worktree_error(
            start,
            "invalid_project_path",
            false,
            Some(&base_ref),
            None,
            None,
        );
    }
    if let Err(error_kind) =
        validate_model_network_project_ingress_authority(policy, Path::new(path))
    {
        return managed_worktree_error(start, error_kind, false, Some(&base_ref), None, None);
    }
    let source = match canonicalize_existing(Path::new(path)) {
        Ok(source) if source.is_dir() && source.to_str().is_some() => source,
        _ => {
            return managed_worktree_error(
                start,
                "project_path_not_found",
                false,
                Some(&base_ref),
                None,
                None,
            )
        }
    };
    if validate_windows_project_root(&source).is_err()
        || validate_project_path_policy(policy, &source).is_err()
    {
        return managed_worktree_error(
            start,
            "path_outside_allowed_roots",
            false,
            Some(&base_ref),
            None,
            None,
        );
    }
    let source_root = match managed_worktree_git_text(&source, &["rev-parse", "--show-toplevel"])
        .ok()
        .and_then(|root| canonicalize_existing(Path::new(&root)).ok())
    {
        Some(root) if paths_equal(&root, &source) => root,
        _ => {
            return managed_worktree_error(
                start,
                "source_not_git_repository",
                false,
                Some(&base_ref),
                None,
                None,
            )
        }
    };
    let source_dirty = match managed_worktree_git_text(&source_root, &["status", "--porcelain"]) {
        Ok(status) => !status.is_empty(),
        Err(_) => {
            return managed_worktree_error(
                start,
                "source_git_status_failed",
                false,
                Some(&base_ref),
                None,
                None,
            )
        }
    };
    if let Some(resume_project_id) = resume_project_id {
        return resume_managed_worktree(
            start,
            policy,
            project_registry_dir,
            client_id,
            &source_root,
            source_dirty,
            requested_base_ref.as_deref(),
            resume_project_id,
        );
    }
    let source_projects = match load_project_files_for_path_resolution(project_registry_dir) {
        Ok(projects) => projects,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                Some(&base_ref),
                None,
                Some(source_dirty),
            )
        }
    };
    let (source_project_id, source_root_fingerprint) =
        match resolve_managed_source_project(&source_projects, &source_root) {
            Ok(lineage) => lineage,
            Err(error) => {
                return managed_worktree_error(
                    start,
                    error,
                    false,
                    Some(&base_ref),
                    None,
                    Some(source_dirty),
                )
            }
        };
    let base_sha = match exact_git_commit(&source_root, &base_ref) {
        Ok(sha) => sha,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                Some(&base_ref),
                None,
                Some(source_dirty),
            )
        }
    };
    let managed_root = match choose_managed_worktree_root(policy, &source_root) {
        Ok(root) => root,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                false,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
    };
    if std::fs::create_dir_all(&managed_root).is_err() {
        return managed_worktree_error(
            start,
            "worktree_creation_failed",
            false,
            Some(&base_ref),
            Some(&base_sha),
            Some(source_dirty),
        );
    }
    let managed_root = match canonicalize_existing(&managed_root) {
        Ok(root) => root,
        Err(_) => {
            return managed_worktree_error(
                start,
                "worktree_creation_failed",
                false,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
    };
    if validate_project_path_policy(policy, &managed_root).is_err() {
        return managed_worktree_error(
            start,
            "managed_worktree_root_unavailable",
            false,
            Some(&base_ref),
            Some(&base_sha),
            Some(source_dirty),
        );
    }
    let destination =
        match load_project_files_for_path_resolution(project_registry_dir).and_then(|projects| {
            choose_managed_destination(&managed_root, &source_root, operation_id, &projects)
        }) {
            Ok(path) => path,
            Err(error) => {
                return managed_worktree_error(
                    start,
                    error,
                    false,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
    let mut created_worktree = false;
    let canonical_worktree = if destination.exists() {
        let Ok(existing) = canonicalize_existing(&destination) else {
            return managed_worktree_error(
                start,
                "managed_worktree_recovery_conflict",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            );
        };
        let head = match managed_worktree_git_text(&existing, &["rev-parse", "HEAD"]) {
            Ok(head) => head,
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
        let listed = match source_mentions_worktree_path(&source_root, &existing) {
            Ok(listed) => listed,
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
        if head != base_sha || !listed {
            return managed_worktree_error(
                start,
                "managed_worktree_recovery_conflict",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            );
        }
        existing
    } else {
        match source_mentions_worktree_path(&source_root, &destination) {
            Ok(true) => {
                return managed_worktree_error(
                    start,
                    "managed_worktree_recovery_conflict",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
            Ok(false) => {}
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    false,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        }
        let destination_string = managed_worktree_git_cli_path(&destination);
        let add_result = run_git_bounded(
            &source_root,
            &[
                "worktree",
                "add",
                "--detach",
                destination_string.as_str(),
                base_sha.as_str(),
            ],
            MANAGED_WORKTREE_GIT_TIMEOUT,
            None,
        );
        match &add_result {
            Ok(output) if output.status.success() => created_worktree = true,
            Ok(_) if !destination.exists() => {
                return match source_mentions_worktree_path(&source_root, &destination) {
                    Ok(false) => managed_worktree_error(
                        start,
                        "worktree_creation_failed",
                        false,
                        Some(&base_ref),
                        Some(&base_sha),
                        Some(source_dirty),
                    ),
                    Ok(true) => managed_worktree_error(
                        start,
                        "managed_worktree_recovery_conflict",
                        true,
                        Some(&base_ref),
                        Some(&base_sha),
                        Some(source_dirty),
                    ),
                    Err(_) => managed_worktree_error(
                        start,
                        "operation_indeterminate",
                        false,
                        Some(&base_ref),
                        Some(&base_sha),
                        Some(source_dirty),
                    ),
                };
            }
            Err(_) if !destination.exists() => {
                return match source_mentions_worktree_path(&source_root, &destination) {
                    Ok(true) => managed_worktree_error(
                        start,
                        "managed_worktree_recovery_conflict",
                        true,
                        Some(&base_ref),
                        Some(&base_sha),
                        Some(source_dirty),
                    ),
                    Ok(false) | Err(_) => managed_worktree_error(
                        start,
                        "operation_indeterminate",
                        false,
                        Some(&base_ref),
                        Some(&base_sha),
                        Some(source_dirty),
                    ),
                };
            }
            Ok(_) | Err(_) => {}
        }
        let worktree = match canonicalize_existing(&destination) {
            Ok(worktree) => worktree,
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
        let head = match managed_worktree_git_text(&worktree, &["rev-parse", "HEAD"]) {
            Ok(head) => head,
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
        let listed = match source_mentions_worktree_path(&source_root, &worktree) {
            Ok(listed) => listed,
            Err(_) => {
                return managed_worktree_error(
                    start,
                    "operation_indeterminate",
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
        if head != base_sha || !listed {
            return managed_worktree_error(
                start,
                "managed_worktree_recovery_conflict",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            );
        }
        worktree
    };
    if validate_project_path_policy(policy, &canonical_worktree).is_err() {
        return managed_worktree_error(
            start,
            "managed_worktree_root_unavailable",
            true,
            Some(&base_ref),
            Some(&base_sha),
            Some(source_dirty),
        );
    }
    let projects = match load_project_files_for_path_resolution(project_registry_dir) {
        Ok(projects) => projects,
        Err(error) => {
            return managed_worktree_error(
                start,
                error,
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
    };
    let matches = projects_matching_canonical_path(&projects, &canonical_worktree);
    if matches.len() > 1 {
        return managed_worktree_error(
            start,
            "ambiguous_project_path",
            true,
            Some(&base_ref),
            Some(&base_sha),
            Some(source_dirty),
        );
    }
    if let Some(project) = matches.into_iter().next() {
        let same_operation = project.managed_worktree
            && project.managed_operation_id.as_deref() == Some(operation_id)
            && project.managed_base_sha.as_deref() == Some(base_sha.as_str())
            && project.managed_source.as_deref() == source_root.to_str()
            && project.managed_source_project_id.as_deref() == Some(source_project_id.as_str())
            && project.managed_source_root_fingerprint.as_deref()
                == Some(source_root_fingerprint.as_str());
        if project.disabled || !same_operation {
            return managed_worktree_error(
                start,
                "managed_worktree_recovery_conflict",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            );
        }
        return managed_worktree_success(
            start,
            client_id,
            &project,
            &canonical_worktree,
            &base_ref,
            &base_sha,
            source_dirty,
            "managed_worktree_recovered",
            false,
            false,
        );
    }
    let project_id =
        match choose_auto_project_id(project_registry_dir, &projects, &canonical_worktree) {
            Ok(id) => id,
            Err(error) => {
                return managed_worktree_error(
                    start,
                    error,
                    true,
                    Some(&base_ref),
                    Some(&base_sha),
                    Some(source_dirty),
                )
            }
        };
    let Some(worktree_string) = canonical_worktree.to_str() else {
        return managed_worktree_error(
            start,
            "invalid_project_path",
            true,
            Some(&base_ref),
            Some(&base_sha),
            Some(source_dirty),
        );
    };
    let source_string = source_root.to_str().expect("validated UTF-8 source path");
    let content = managed_worktree_project_toml(
        &project_id,
        &bounded_project_name(&canonical_worktree),
        worktree_string,
        source_string,
        &source_project_id,
        &source_root_fingerprint,
        &base_ref,
        &base_sha,
        operation_id,
    );
    match write_project_toml_atomic(project_registry_dir, &project_id, &content, false) {
        Ok(_) => {}
        Err(ProjectTomlWriteError::BeforeRename) => {
            return managed_worktree_error(
                start,
                "worktree_registration_failed",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
        Err(ProjectTomlWriteError::AfterRename) => {
            return managed_worktree_error(
                start,
                "operation_indeterminate",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
    }
    let project = match parse_runner_project_toml(&content) {
        Ok(project) => project,
        Err(_) => {
            return managed_worktree_error(
                start,
                "operation_indeterminate",
                true,
                Some(&base_ref),
                Some(&base_sha),
                Some(source_dirty),
            )
        }
    };
    managed_worktree_success(
        start,
        client_id,
        &project,
        &canonical_worktree,
        &base_ref,
        &base_sha,
        source_dirty,
        if created_worktree {
            "managed_worktree_created"
        } else {
            "managed_worktree_recovered"
        },
        true,
        true,
    )
}
