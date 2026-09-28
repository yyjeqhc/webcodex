//! Runner-owned planning; admission revalidates the fence before enqueue.
use super::*;
use sha2::{Digest, Sha256};
use webcodex_core::project_validation::*;
use webcodex_validation::{
    detect_validation_recipe, project_validation_operation,
    resolve_validation_recipe_with_packages, RecipeId, SemanticCheck,
};

pub(crate) fn plan(
    policy: &RunnerPolicy,
    registry: &Path,
    request: &ProjectValidationRequest,
) -> Result<(ProjectValidationPlan, PathBuf), ProjectValidationPlanningResult> {
    let unavailable =
        |code: &str, backend: Option<&str>| ProjectValidationPlanningResult::Unavailable {
            code: code.into(),
            detected_backend: backend.map(str::to_string),
        };
    request
        .validate()
        .map_err(|_| unavailable("invalid_arguments", None))?;
    let root = resolve_runner_project(registry, &request.project_id)
        .map_err(|_| unavailable("unknown_project", None))?;
    let root = validate_project_root(policy, &root)
        .map_err(|_| unavailable("invalid_project_path", None))?;
    let hint = match request.adapter {
        ProjectValidationAdapter::Auto => None,
        ProjectValidationAdapter::Rust => Some(RecipeId::Rust),
        ProjectValidationAdapter::Go => Some(RecipeId::Go),
    };
    let backend = detect_validation_recipe(&root, request.cwd.as_deref(), hint)
        .map_err(|e| unavailable(e.code, None))?;
    if matches!(backend, RecipeId::Node | RecipeId::Python) {
        return Err(unavailable(
            "validation_adapter_unavailable",
            Some(backend.as_str()),
        ));
    }
    let action = match request.action {
        ProjectValidationAction::FormatCheck => SemanticCheck::Format,
        ProjectValidationAction::Check => SemanticCheck::Check,
        ProjectValidationAction::Test => SemanticCheck::Test,
    };
    let packages = request.scope.as_ref().map(|scope| scope.packages.clone());
    let operation = project_validation_operation(backend.as_str(), action, packages)
        .map_err(|code| unavailable(code, Some(backend.as_str())))?;
    let adapter = operation.adapter();
    let resolved = resolve_validation_recipe_with_packages(
        &root,
        request.cwd.as_deref(),
        hint,
        &[action],
        None,
        request
            .scope
            .as_ref()
            .map(|scope| scope.packages.as_slice()),
    )
    .map_err(|e| unavailable(e.code, Some(backend.as_str())))?;
    let identity = operation
        .validation_target_id(Some(&resolved.recipe_root_relative))
        .ok_or_else(|| unavailable("validation_scope_invalid", Some(backend.as_str())))?;
    debug_assert_eq!(
        operation
            .build_readonly_plan()
            .ok()
            .map(|plan| plan.structured_step),
        resolved.steps.first().cloned()
    );
    let cwd = root.join(&resolved.recipe_root_relative);
    Ok((
        ProjectValidationPlan {
            provenance: ProjectValidationProvenance {
                request: request.clone(),
                backend: backend.as_str().into(),
                recipe_root: resolved.recipe_root_relative,
                root_digest: format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes())),
                manifest_digest: resolved.manifest_digest,
                invocation_digest: resolved.invocation_digest,
            },
            adapter: adapter.tool_identity().into(),
            step: resolved
                .steps
                .into_iter()
                .next()
                .expect("one requested action"),
            validation_target_id: identity,
        },
        cwd,
    ))
}

pub(crate) fn handle(
    policy: &RunnerPolicy,
    registry: &Path,
    request: &ProjectValidationRequest,
) -> CommandResult {
    let result = match plan(policy, registry, request) {
        Ok((plan, _)) => ProjectValidationPlanningResult::Ready { plan },
        Err(error) => error,
    };
    CommandResult {
        exit_code: Some(0),
        stdout: Some(serde_json::to_string(&result).expect("typed planning response")),
        stderr: None,
        duration_ms: Some(0),
        error: None,
    }
}

pub(crate) fn fence(
    policy: &RunnerPolicy,
    registry: &Path,
    operation: &webcodex_core::runner_operation::RunnerJobOperation,
) -> Result<(), String> {
    let webcodex_core::runner_operation::RunnerJobOperation::StartValidation(op) = operation else {
        return Ok(());
    };
    let Some(metadata) = &op.context.validation else {
        return Ok(());
    };
    let Some(provenance) = &metadata.project_validation else {
        return Ok(());
    };
    let registered_id = op
        .context
        .runtime_project_id
        .as_deref()
        .and_then(|id| id.strip_prefix("agent:"))
        .and_then(|id| id.split_once(':'))
        .map(|(_, id)| id);
    if registered_id != Some(provenance.request.project_id.as_str()) {
        return Err("validation project identity mismatch".into());
    }
    let (resolved, cwd) = plan(policy, registry, &provenance.request)
        .map_err(|_| "validation_plan_stale: resolve project validation again".to_string())?;
    if &resolved.provenance != provenance
        || resolved.adapter != metadata.adapter
        || op.steps != [resolved.step]
        || op
            .cwd
            .as_deref()
            .and_then(|p| Path::new(p).canonicalize().ok())
            .as_ref()
            != Some(
                &cwd.canonicalize()
                    .map_err(|_| "validation cwd unavailable")?,
            )
        || metadata.validation_target_id.as_deref() != Some(&resolved.validation_target_id)
    {
        return Err("validation_plan_stale: resolve project validation again".into());
    }
    Ok(())
}
