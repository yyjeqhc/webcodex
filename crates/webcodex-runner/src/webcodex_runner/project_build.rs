//! Runner-owned project build planning and admission re-plan fence.

use super::config::RunnerPolicy;
use super::output::CommandResult;
use super::projects::load_runner_project_summaries_from_dir;
use super::shell::cwd_allowed;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use webcodex_core::project_build::{
    project_build_invocation_digest, ProjectBuildAdapter, ProjectBuildPlan,
    ProjectBuildPlanningResult, ProjectBuildProvenance, ProjectBuildRequest,
};
use webcodex_core::runner_operation::RunnerJobOperation;
use webcodex_workspace::project_recipe::{
    digest_project_recipe_files, project_recipe_provenance_files, resolve_project_recipe_root,
    ProjectRecipeId, ProjectRecipeResolutionError,
};

#[path = "project_build/rust.rs"]
mod rust;
pub(crate) use rust::run_rust_build;

fn unavailable(code: &str, backend: Option<&str>) -> ProjectBuildPlanningResult {
    ProjectBuildPlanningResult::Unavailable {
        code: code.into(),
        detected_backend: backend.map(str::to_string),
    }
}

fn map_recipe_error(error: ProjectRecipeResolutionError) -> &'static str {
    match error {
        ProjectRecipeResolutionError::ExecutionRootUnavailable
        | ProjectRecipeResolutionError::NotFound => "build_recipe_not_found",
        ProjectRecipeResolutionError::CwdMismatch
        | ProjectRecipeResolutionError::ExplicitNotFound { .. }
        | ProjectRecipeResolutionError::ExplicitMismatch { .. } => "build_recipe_mismatch",
        ProjectRecipeResolutionError::Ambiguous { .. } => "build_recipe_ambiguous",
        ProjectRecipeResolutionError::SourceFileInvalid => "build_manifest_invalid",
    }
}

fn resolve_runner_project(project_registry_dir: &Path, project_id: &str) -> Result<PathBuf, ()> {
    load_runner_project_summaries_from_dir(project_registry_dir)
        .into_iter()
        .find(|project| project.id == project_id)
        .map(|project| PathBuf::from(project.path))
        .ok_or(())
}

fn validate_project_root(policy: &RunnerPolicy, path: &Path) -> Result<PathBuf, ()> {
    cwd_allowed(policy, path).map_err(|_| ())?;
    std::fs::canonicalize(path).map_err(|_| ())
}

pub(crate) fn plan(
    policy: &RunnerPolicy,
    registry: &Path,
    request: &ProjectBuildRequest,
) -> Result<(ProjectBuildPlan, PathBuf), ProjectBuildPlanningResult> {
    request
        .validate()
        .map_err(|_| unavailable("invalid_arguments", None))?;

    let root = resolve_runner_project(registry, &request.project_id)
        .map_err(|_| unavailable("unknown_project", None))?;
    let root = validate_project_root(policy, &root)
        .map_err(|_| unavailable("invalid_project_path", None))?;

    let hint = match request.adapter {
        ProjectBuildAdapter::Auto => None,
        ProjectBuildAdapter::Rust => Some(ProjectRecipeId::Rust),
        ProjectBuildAdapter::Go => Some(ProjectRecipeId::Go),
        ProjectBuildAdapter::Node => Some(ProjectRecipeId::Node),
        ProjectBuildAdapter::Python => Some(ProjectRecipeId::Python),
    };
    if matches!(
        request.adapter,
        ProjectBuildAdapter::Auto | ProjectBuildAdapter::Node
    ) {
        webcodex_workspace::project_recipe::validate_node_project_marker_chain(
            &root,
            request.cwd.as_deref(),
        )
        .map_err(|_| unavailable("build_manifest_invalid", Some("node")))?;
    }
    let resolved = resolve_project_recipe_root(&root, request.cwd.as_deref(), hint)
        .map_err(|error| unavailable(map_recipe_error(error), None))?;
    let backend = resolved.recipe;
    if request.has_rust_options() && backend != ProjectRecipeId::Rust {
        return Err(unavailable(
            "build_options_unsupported",
            Some(backend.as_str()),
        ));
    }
    if request.dependency_resolution_policy.is_some() && backend != ProjectRecipeId::Rust {
        return Err(unavailable(
            "dependency_resolution_policy_unsupported",
            Some(backend.as_str()),
        ));
    }
    let excluded_packages = request
        .scope
        .as_ref()
        .map(webcodex_core::project_build::ProjectBuildScope::normalized_exclude_packages)
        .transpose()
        .map_err(|_| unavailable("build_scope_invalid", None))?
        .unwrap_or_default();
    if !excluded_packages.is_empty() && backend != ProjectRecipeId::Rust {
        return Err(unavailable(
            "workspace_exclusions_unsupported",
            Some(backend.as_str()),
        ));
    }

    if backend == ProjectRecipeId::Python {
        // Default `build` output is dist/. Never follow a pre-existing redirected
        // output directory; the native backend remains trusted project code.
        validate_python_dist(&resolved.absolute_root)
            .map_err(|_| unavailable("build_artifact_path_invalid", Some("python")))?;
    }

    let node = if backend == ProjectRecipeId::Node {
        if request.scope.is_some() {
            return Err(unavailable("build_scope_unsupported", Some("node")));
        }
        if request.dependency_policy.is_some() {
            return Err(unavailable("dependency_policy_unsupported", Some("node")));
        }
        Some(
            webcodex_validation::resolve_node_project_build(&root, request.cwd.as_deref())
                .map_err(|error| unavailable(error.code, Some("node")))?,
        )
    } else {
        None
    };
    let process = webcodex_core::project_build::canonical_project_build_process_with_manager(
        backend.as_str(),
        request,
        node.as_ref().map_or("npm", |(manager, _)| manager.as_str()),
    )
    .map_err(|_| unavailable("build_scope_invalid", Some(backend.as_str())))?;
    let manifest_digest = if let Some((_, digest)) = node {
        digest
    } else if backend == ProjectRecipeId::Rust
        && request
            .scope
            .as_ref()
            .is_some_and(webcodex_core::project_build::ProjectBuildScope::selects_all_packages)
    {
        if !excluded_packages.is_empty() {
            let (digest, members) =
                webcodex_workspace::project_recipe::project_cargo_all_packages_provenance(
                    &resolved,
                )
                .map_err(|_| unavailable("build_scope_unavailable", Some(backend.as_str())))?;
            request
                .scope
                .as_ref()
                .expect("exclusions require scope")
                .validate_excluded_members(&members)
                .map_err(|_| unavailable("build_scope_invalid", Some(backend.as_str())))?;
            digest
        } else {
            webcodex_workspace::project_recipe::digest_project_cargo_all_packages_provenance(
                &resolved,
            )
            .map_err(|_| unavailable("build_scope_unavailable", Some(backend.as_str())))?
        }
    } else {
        let provenance_files = if backend == ProjectRecipeId::Python {
            // Packaging requires a manifest even for explicit Python. These
            // backend inputs are fenced without executing build hooks.
            if !resolved.marker_path().is_file() {
                return Err(unavailable("build_recipe_not_found", Some("python")));
            }
            Some(vec![
                resolved.marker_path(),
                resolved.absolute_root.join("setup.cfg"),
                resolved.absolute_root.join("setup.py"),
            ])
        } else {
            project_recipe_provenance_files(&resolved)
                .map_err(|error| unavailable(map_recipe_error(error), Some(backend.as_str())))?
        };
        let provenance_files = provenance_files
            .ok_or_else(|| unavailable("build_adapter_unavailable", Some(backend.as_str())))?;
        digest_project_recipe_files(&resolved.execution_root, provenance_files)
            .map_err(|error| unavailable(map_recipe_error(error), Some(backend.as_str())))?
    };

    let plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request: request.clone(),
            backend: backend.as_str().into(),
            recipe_root: resolved.relative_root,
            root_digest: format!(
                "{:x}",
                Sha256::digest(resolved.execution_root.to_string_lossy().as_bytes())
            ),
            manifest_digest,
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    };
    debug_assert!(plan.is_valid());
    Ok((plan, resolved.absolute_root))
}

pub(crate) fn handle(
    policy: &RunnerPolicy,
    registry: &Path,
    request: &ProjectBuildRequest,
) -> CommandResult {
    let result = match plan(policy, registry, request) {
        Ok((plan, _)) => ProjectBuildPlanningResult::Ready { plan },
        Err(error) => error,
    };
    CommandResult {
        exit_code: Some(0),
        stdout: Some(serde_json::to_string(&result).expect("typed build planning response")),
        stderr: None,
        duration_ms: Some(0),
        error: None,
    }
}

pub(crate) fn fence(
    policy: &RunnerPolicy,
    registry: &Path,
    operation: &RunnerJobOperation,
) -> Result<(), String> {
    let RunnerJobOperation::StartBuild(operation) = operation else {
        return Ok(());
    };
    let provenance = &operation.provenance;

    let registered_id = operation
        .context
        .runtime_project_id
        .as_deref()
        .and_then(|id| id.strip_prefix("agent:"))
        .and_then(|id| id.split_once(':'))
        .map(|(_, id)| id);
    if registered_id != Some(provenance.request.project_id.as_str()) {
        return Err("build project identity mismatch".into());
    }

    let (resolved, cwd) = plan(policy, registry, &provenance.request)
        .map_err(|_| "build_plan_stale: resolve project build again".to_string())?;
    if &resolved.provenance != provenance
        || resolved.process != operation.process
        || operation
            .cwd
            .as_deref()
            .and_then(|path| Path::new(path).canonicalize().ok())
            .as_ref()
            != Some(&cwd.canonicalize().map_err(|_| "build cwd unavailable")?)
    {
        return Err("build_plan_stale: resolve project build again".into());
    }
    Ok(())
}

fn validate_python_dist(cwd: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(cwd.join("dist")) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("build_artifact_path_invalid".into()),
    }
}

/// Observe the existing bounded default output directory. Presence supplies no
/// package correctness or source-freshness proof and does not imply this run
/// created a pre-existing file. Metadata uses the canonical artifact reader.
pub(crate) fn observe_python_artifacts(
    policy: &RunnerPolicy,
    registry: &Path,
    request: &ProjectBuildRequest,
    cwd: &Path,
    deadline: std::time::Instant,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Vec<webcodex_core::project_build::ProjectBuildArtifact>, String> {
    use std::sync::atomic::Ordering;
    validate_python_dist(cwd)?;
    let root = resolve_runner_project(registry, &request.project_id)
        .map_err(|_| "build_artifact_project_unavailable")?;
    let root =
        validate_project_root(policy, &root).map_err(|_| "build_artifact_project_unavailable")?;
    let dist = cwd.join("dist");
    if !dist.exists() {
        return Err("build_artifacts_missing".into());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&dist).map_err(|_| "build_artifact_metadata_unavailable")? {
        if paths.len() >= webcodex_core::project_build::PROJECT_BUILD_ARTIFACTS_MAX {
            return Err("build_artifacts_limit_exceeded".into());
        }
        let entry = entry.map_err(|_| "build_artifact_metadata_unavailable")?;
        let kind = entry
            .file_type()
            .map_err(|_| "build_artifact_metadata_unavailable")?;
        if !kind.is_file() || kind.is_symlink() {
            return Err("build_artifact_path_invalid".into());
        }
        let path = entry
            .path()
            .strip_prefix(&root)
            .map_err(|_| "build_artifact_path_invalid")?
            .to_str()
            .ok_or("build_artifact_path_invalid")?
            .replace('\\', "/");
        paths.push(path);
    }
    paths.sort();
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    for path in paths {
        if stop.load(Ordering::SeqCst) {
            return Err("build_cancelled".into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("build_timeout".into());
        }
        let artifact = super::artifacts::observe_build_artifact(&root, &path)?;
        total = total
            .checked_add(artifact.bytes)
            .ok_or("build_artifacts_limit_exceeded")?;
        if total > 256 * 1024 * 1024 {
            return Err("build_artifacts_limit_exceeded".into());
        }
        artifacts.push(artifact);
    }
    if stop.load(Ordering::SeqCst) {
        return Err("build_cancelled".into());
    }
    if std::time::Instant::now() >= deadline {
        return Err("build_timeout".into());
    }
    if !webcodex_core::project_build::validate_build_artifacts(&artifacts) {
        return Err("build_artifact_metadata_invalid".into());
    }
    if !artifacts
        .iter()
        .any(|artifact| artifact.path.ends_with(".whl"))
        || !artifacts
            .iter()
            .any(|artifact| artifact.path.ends_with(".tar.gz"))
    {
        return Err("build_artifacts_missing".into());
    }
    Ok(artifacts)
}
