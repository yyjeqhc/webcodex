use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use webcodex_core::runner_operation::{RunnerProjectOperation, RunnerProjectOperationKind};
use webcodex_runner_config::paths::paths_equal;

use super::super::config::RunnerPolicy;
use super::super::shell::canonicalize_existing;
use super::catalog::{
    effective_registration_source, parse_runner_project_toml, project_lineage, project_revision,
    project_root_fingerprint, run_git_bounded, EXPLICIT_REGISTRATION_SOURCE,
};
use super::created_paths::{write_created_file, CreatedProjectPaths};
use super::registration::{
    build_project_toml, sync_dir, sync_parent_dir, sync_project_parent_after_rename,
    unique_registry_temp, validate_model_network_project_ingress_authority,
    validate_project_op_description, validate_project_op_id, validate_project_op_name,
    validate_project_path_policy, validate_windows_project_root, write_project_toml_atomic,
    ProjectTomlWriteError,
};
use super::{
    project_error_cmd, project_registry_write_lock, structured_project_error_cmd, RunnerProjectFile,
};
use crate::webcodex_runner::output::{err_cmd, ok_cmd, CommandResult};

#[derive(Debug)]
enum ProjectUnregisterError {
    BeforeRename,
    AfterRename,
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
pub(crate) fn handle_project_lifecycle_operation(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    operation: &RunnerProjectOperation,
) -> CommandResult {
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return project_error_cmd(Instant::now(), "operation_failed"),
    };
    let start = Instant::now();
    let action = match operation.kind {
        RunnerProjectOperationKind::LifecycleEnable => "enable",
        RunnerProjectOperationKind::LifecycleDisable => "disable",
        RunnerProjectOperationKind::LifecycleUnregister => "unregister",
        _ => return project_error_cmd(start, "unsupported_runner_version"),
    };
    let payload: serde_json::Value = match serde_json::from_str(&operation.payload).ok() {
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
                "name": project.name, "kind": project.kind,
                "registration_source": effective_registration_source(&project).as_str(),
                "description": project.description,
                "allow_patch": project.allow_patch,
                "root_fingerprint": canonicalize_existing(Path::new(&project.path)).ok()
                    .filter(|path| path.is_dir()).as_deref().map(project_root_fingerprint),
                "lineage": project_lineage(&project)
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
            "name": project.name, "kind": project.kind,
            "registration_source": effective_registration_source(&project).as_str(),
            "description": project.description,
            "allow_patch": project.allow_patch,
            "root_fingerprint": canonicalize_existing(Path::new(&project.path)).ok()
                .filter(|path| path.is_dir()).as_deref().map(project_root_fingerprint),
            "lineage": project_lineage(&project)
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
    Ok(())
}

fn recovered_project_result(
    create: bool,
    runtime_id: &str,
    client_id: &str,
    project: &RunnerProjectFile,
    template: Option<&str>,
    git_init: bool,
) -> serde_json::Value {
    let root_fingerprint = canonicalize_existing(Path::new(&project.path))
        .ok()
        .filter(|path| path.is_dir())
        .as_deref()
        .map(project_root_fingerprint);
    serde_json::json!({
        "id": runtime_id, "agent_project_id": project.id, "client_id": client_id,
        "name": project.name, "path": project.path, "kind": project.kind,
        "registration_source": effective_registration_source(project).as_str(),
        "description": project.description,
        "created_directory": false, "created_config": false, "overwritten": false,
        "allow_patch": project.allow_patch, "template": template,
        "git_initialized": git_init, "recovered": true, "changed": false,
        "operation": if create { "create" } else { "register" },
        "outcome": if create { "created" } else { "registered" },
        "revision": project_revision(project),
        "root_fingerprint": root_fingerprint,
        "lineage": project_lineage(project),
    })
}

/// Handle `register_project` / `create_project` agent requests. Parses the
/// JSON payload from `request.stdin`, validates fields and path against
/// policy, writes `project_registry_dir/<id>.toml` atomically (and for
/// `create_project` creates the directory / templates / optional git init),
/// and returns structured JSON in `CommandResult.stdout`.
pub(crate) fn handle_project_operation(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    client_id: &str,
    operation: &RunnerProjectOperation,
) -> CommandResult {
    let _registry_guard = match project_registry_write_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return project_error_cmd(Instant::now(), "operation_failed"),
    };
    let start = Instant::now();
    let kind = operation.kind.wire_kind();
    let create = operation.kind == RunnerProjectOperationKind::Create;
    let payload = match operation.payload.as_str() {
        s if !s.is_empty() => s,
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
    if json.get("managed_temporary_project").is_some() {
        return project_error_cmd(start, "managed_temporary_projects_retired");
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
    // Existing project registration accepts local disks and network shares;
    // special Windows namespaces still fail before filesystem access.
    if let Err(error_kind) = validate_windows_project_root(Path::new(&path)) {
        return project_error_cmd(start, error_kind);
    }
    if !create {
        if let Err(error_kind) =
            validate_model_network_project_ingress_authority(policy, Path::new(&path))
        {
            return project_error_cmd(start, error_kind);
        }
    }
    #[cfg(windows)]
    if create && webcodex_runner_config::paths::is_windows_network_share_path(Path::new(&path)) {
        // Network project creation is deliberately outside this phase. Existing
        // network directories can be registered when RunnerPolicy already grants them.
        return project_error_cmd(start, "windows_project_path_unsupported");
    }

    let client_id = client_id.to_string();
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
    let adopt_existing_empty = json
        .get("adopt_existing_empty")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if create && template != "empty" && template != "basic" {
        return project_error_cmd(start, "invalid_request");
    }

    if !create {
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
                            create,
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

            "created_config": write_result.created_config,
            "overwritten": write_result.overwritten,
            "allow_patch": allow_patch,
            "registration_source": EXPLICIT_REGISTRATION_SOURCE,
            "revision": project_revision(&parse_runner_project_toml(&toml_content).expect("generated project TOML must parse")),
            "root_fingerprint": project_root_fingerprint(&canonical),
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
    #[cfg(windows)]
    if webcodex_runner_config::paths::is_windows_network_share_path(&canonical_for_policy) {
        // A mapped drive may canonicalize to VerbatimUNC. Keep create_project
        // local-only even when the raw spelling looked like a drive letter.
        return project_error_cmd(start, "windows_project_path_unsupported");
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
                if let Err(code) =
                    validate_recovered_create_side_effects(&path_buf, &template, git_init)
                {
                    return project_error_cmd(start, code);
                }
                return ok_cmd(
                    start,
                    recovered_project_result(
                        create,
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
        if !adopt_existing_empty {
            return project_error_cmd(start, "path_exists");
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
    }
    // `empty` itself generates no project files. Description stays registration
    // metadata; `git_init` remains a separate explicit filesystem side effect.

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

        "created_directory": created_directory,
        "created_config": write_result.created_config,
        "overwritten": write_result.overwritten,
        "allow_patch": allow_patch,
        "registration_source": EXPLICIT_REGISTRATION_SOURCE,
        "template": template,
        "revision": project_revision(&parse_runner_project_toml(&toml_content).expect("generated project TOML must parse")),
        "root_fingerprint": path_buf.canonicalize().ok().filter(|path| path.is_dir()).as_deref().map(project_root_fingerprint),
        "git_initialized": git_initialized,
        "operation": "create", "outcome": "created", "changed": true, "recovered": false,
    });
    ok_cmd(start, result)
}
