use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sha2::{Digest, Sha256};
use webcodex_core::runner_operation::RunnerProjectOperation;
use webcodex_runner_config::paths::paths_equal;

use super::super::config::RunnerPolicy;
use super::super::shell::canonicalize_existing;
use super::catalog::{
    effective_registration_source, parse_runner_project_toml, project_lineage, project_revision,
    project_root_fingerprint, project_wire_kind, AUTO_REGISTERED_REGISTRATION_SOURCE,
};
use super::{project_registry_write_lock, structured_project_error_cmd, RunnerProjectFile};
use crate::webcodex_runner::output::{ok_cmd, CommandResult};

const AUTO_PROJECT_HASH_PREFIX_LENGTHS: &[usize] = &[8, 12, 16, 24, 32, 48, 64];

/// Windows-only raw/canonical namespace fence. Local disks plus UNC and
/// verbatim-UNC shares may proceed; device namespaces and generic verbatim
/// namespaces fail closed with a stable error before filesystem access.
///
/// The shared `webcodex_runner_config::paths::validate_project_path_ingress`
/// owns the grammar-based prefix rule; it never falls back to a string
/// `starts_with` check.
pub(super) fn validate_windows_project_root(path: &Path) -> Result<(), &'static str> {
    if webcodex_runner_config::paths::project_path_has_parent_traversal(path) {
        return Err("path_outside_allowed_roots");
    }
    webcodex_runner_config::paths::validate_project_path_ingress(path)
        .map_err(|_| "windows_project_path_unsupported")
}

/// Before a model-facing request dereferences a raw Windows network path, require
/// it to fall under Runner authority that was already configured by the user.
/// This prevents an untrusted `\\server\share` spelling from triggering SMB I/O
/// (and possible OS authentication) merely to discover that policy rejects it.
pub(super) fn validate_model_network_project_ingress_authority(
    policy: &RunnerPolicy,
    path: &Path,
) -> Result<(), &'static str> {
    #[cfg(windows)]
    {
        if !webcodex_runner_config::paths::is_windows_network_share_path(path) {
            return Ok(());
        }
        // Containment below is lexical: a parent component could escape an
        // authorized directory before the canonical policy gets a chance to run.
        if webcodex_runner_config::paths::project_path_has_parent_traversal(path) {
            return Err("path_outside_allowed_roots");
        }
        if policy.allowed_roots.iter().any(|root| {
            root.is_absolute() && webcodex_runner_config::paths::path_is_within(path, root)
        }) {
            return Ok(());
        }
        // A configured mapped drive may canonicalize to the same UNC share. It is
        // safe to resolve configured roots here because those roots are already
        // user-authorized; never canonicalize the untrusted target before this gate.
        let canonical_roots =
            webcodex_runner_config::paths::canonicalize_usable_allowed_roots(&policy.allowed_roots);
        if canonical_roots
            .iter()
            .any(|root| webcodex_runner_config::paths::path_is_within(path, root))
        {
            return Ok(());
        }
        return Err("path_outside_allowed_roots");
    }

    #[cfg(not(windows))]
    {
        let _ = (policy, path);
        Ok(())
    }
}

/// Escape a string for use as a TOML basic string (double-quoted). NUL is
/// rejected up front by validation, so we only handle backslash, quote, and
/// common control characters.
pub(super) fn toml_basic_string(value: &str) -> String {
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
pub(super) fn build_project_toml(
    id: &str,
    name: &str,
    path: &str,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    build_project_toml_with_registration_source(id, name, path, None, description, allow_patch)
}

pub(super) fn build_project_toml_with_registration_source(
    id: &str,
    name: &str,
    path: &str,
    registration_source: Option<&str>,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    let mut toml = String::new();
    toml.push_str(&format!("id = {}\n", toml_basic_string(id)));
    toml.push_str(&format!("name = {}\n", toml_basic_string(name)));
    toml.push_str(&format!("path = {}\n", toml_basic_string(path)));
    if let Some(registration_source) = registration_source {
        toml.push_str(&format!(
            "registration_source = {}\n",
            toml_basic_string(registration_source)
        ));
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
pub(super) fn validate_project_op_id(id: &str) -> Result<(), String> {
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

/// Validate the project `name`: non-empty after trim, <= 120 UTF-8 bytes, no NUL.
pub(super) fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 UTF-8 bytes".to_string());
    }
    Ok(())
}

/// Validate the optional `description`: <= 500 UTF-8 bytes, no NUL.
pub(super) fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 UTF-8 bytes".to_string());
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
    let canonical_roots =
        webcodex_runner_config::paths::canonicalize_usable_allowed_roots(&policy.allowed_roots);
    webcodex_runner_config::paths::validate_project_path_policy(
        canonical_path,
        &canonical_roots,
        policy.allow_cwd_anywhere,
    )
}

#[derive(Debug, Clone)]
pub(super) struct ProjectTomlWriteResult {
    pub(super) config_path: PathBuf,
    pub(super) created_config: bool,
    pub(super) overwritten: bool,
}

#[derive(Debug)]
pub(super) enum ProjectTomlWriteError {
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

pub(super) fn sync_project_parent_after_rename(path: &Path) -> Result<(), String> {
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
pub(super) fn sync_parent_dir(path: &Path) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "project config has no parent".to_string())?;
    sync_dir(dir)
}

pub(super) fn sync_dir(dir: &Path) -> Result<(), String> {
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

pub(super) fn unique_registry_temp(dir: &Path, id: &str, suffix: &str) -> PathBuf {
    dir.join(format!(".{id}.{}.{}", uuid::Uuid::new_v4(), suffix))
}

pub(super) fn write_project_toml_atomic(
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

pub(super) fn load_project_files_for_path_resolution(
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

pub(super) fn projects_matching_canonical_path(
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

pub(super) fn resolve_managed_source_project(
    projects: &[RunnerProjectFile],
    source_root: &Path,
) -> Result<(String, String), &'static str> {
    let matches = projects_matching_canonical_path(projects, source_root);
    if matches.len() > 1 {
        return Err("ambiguous_project_path");
    }
    let Some(source) = matches.into_iter().next() else {
        return Err("managed_worktree_source_project_unavailable");
    };
    if source.disabled {
        return Err("managed_worktree_source_project_unavailable");
    }
    Ok((source.id, project_root_fingerprint(source_root)))
}

pub(super) fn bounded_project_name(canonical_path: &Path) -> String {
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

pub(super) fn sanitized_project_basename(canonical_path: &Path) -> String {
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

pub(super) fn choose_auto_project_id(
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
    client_id: &str,
    project: &RunnerProjectFile,
    canonical_path: &Path,
    outcome: &'static str,
    registered: bool,
    project_record_path: Option<&Path>,
) -> serde_json::Value {
    serde_json::json!({
        "id": format!("agent:{}:{}", client_id, project.id),
        "agent_project_id": project.id,
        "client_id": client_id,
        "name": project.name,
        "path": canonical_path.to_string_lossy(),
        "kind": project_wire_kind(project),
        "registration_source": effective_registration_source(project).as_str(),
        "description": project.description,
        "allow_patch": project.allow_patch,
        "disabled": project.disabled,
        "revision": project_revision(project),
        "root_fingerprint": project_root_fingerprint(canonical_path),
        "lineage": project_lineage(project),
        "source": "path",
        "outcome": outcome,
        "registered": registered,
        "created_config": registered,
        "changed": registered,
        "recovered": !registered,
        "project_record_path": project_record_path.map(|path| path.to_string_lossy().to_string()),
    })
}

fn existing_path_resolution_result(
    start: Instant,
    client_id: &str,
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
            client_id,
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
pub(crate) fn handle_resolve_or_register_project_operation(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    client_id: &str,
    operation: &RunnerProjectOperation,
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
    let payload = match serde_json::from_str::<serde_json::Value>(&operation.payload)
        .ok()
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
    // Reject unsupported namespaces before filesystem access. Raw UNC/VerbatimUNC
    // inputs must also be covered by pre-existing Runner authority before they may
    // trigger SMB I/O; local CLI/Desktop onboarding owns any authority extension.
    if let Err(error_kind) = validate_windows_project_root(Path::new(path)) {
        return structured_project_error_cmd(
            start,
            error_kind,
            false,
            serde_json::json!({"field": "path"}),
        );
    }
    if let Err(error_kind) =
        validate_model_network_project_ingress_authority(policy, Path::new(path))
    {
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
    // Re-check the canonical form so canonicalization cannot introduce a device
    // or other unsupported Windows namespace. VerbatimUNC remains supported.
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
    if let Some(result) =
        existing_path_resolution_result(start, client_id, &canonical_path, matches)
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
    let toml_content = build_project_toml_with_registration_source(
        &project_id,
        &name,
        &canonical_path_string,
        Some(AUTO_REGISTERED_REGISTRATION_SOURCE),
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
                        existing_path_resolution_result(start, client_id, &canonical_path, matches)
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
            client_id,
            &project,
            &canonical_path,
            "auto_registered",
            true,
            Some(&write_result.config_path),
        ),
    )
}
