//! Runner-owned project build planning and admission re-plan fence.

use super::config::RunnerPolicy;
use super::output::CommandResult;
use super::projects::load_runner_project_summaries_from_dir;
use super::shell::cwd_allowed;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildPlanningResult, ProjectBuildProvenance, ProjectBuildRequest,
};
use webcodex_core::runner_operation::RunnerJobOperation;
use webcodex_workspace::project_recipe::{
    digest_project_recipe_files, project_recipe_provenance_files, resolve_project_recipe_root,
    ProjectRecipeId, ProjectRecipeResolutionError,
};

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
    };
    let resolved = resolve_project_recipe_root(&root, request.cwd.as_deref(), hint)
        .map_err(|error| unavailable(map_recipe_error(error), None))?;
    let backend = resolved.recipe;
    if matches!(backend, ProjectRecipeId::Node | ProjectRecipeId::Python) {
        return Err(unavailable(
            "build_adapter_unavailable",
            Some(backend.as_str()),
        ));
    }

    let process = canonical_project_build_process(backend.as_str(), request)
        .map_err(|_| unavailable("build_scope_invalid", Some(backend.as_str())))?;
    let provenance_files = project_recipe_provenance_files(&resolved)
        .map_err(|error| unavailable(map_recipe_error(error), Some(backend.as_str())))?
        .expect("project build supports only Rust/Go");
    let manifest_digest =
        digest_project_recipe_files(&resolved.execution_root, provenance_files)
            .map_err(|error| unavailable(map_recipe_error(error), Some(backend.as_str())))?;

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
