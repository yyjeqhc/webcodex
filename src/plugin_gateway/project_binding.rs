//! Exact, reauthorized Project observations for project-bound Plugin tools.
//!
//! Provider cwd is checked by the Runner against this target at invocation.
//! Neither this observation nor the opaque outer binding grants filesystem isolation.
use super::{GatewayError, PluginProjectTarget, ToolRuntime};
use crate::auth::AuthContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoundPluginProject {
    pub(super) canonical_id: String,
    pub(super) target: PluginProjectTarget,
}

/// Preflight the bound business Project before selecting permission policy.
/// Call dispatch rechecks it again after governance; a recorder never supplies it.
pub(super) async fn invocation_project(
    runtime: &ToolRuntime,
    request: &super::PluginToolCall,
    auth: Option<&AuthContext>,
) -> Result<Option<BoundPluginProject>, GatewayError> {
    if !matches!(
        super::PluginOperation::from(request.action),
        super::PluginOperation::Call
    ) {
        return Ok(None);
    }
    let Some(binding) = request
        .binding
        .as_deref()
        .and_then(|id| runtime.plugin_gateway.binding(id))
    else {
        return Ok(None);
    };
    let Some(project) = binding.project else {
        return Ok(None);
    };
    let current = resolve_bound_project(
        runtime,
        &project.canonical_id,
        &binding.client_id,
        crate::auth::SCOPE_PROJECT_WRITE,
        auth,
    )
    .await?;
    if current != project {
        return Err(GatewayError::local(
            "plugin_project_changed",
            "The described Project root changed; the call was not dispatched",
        ));
    }
    Ok(Some(current))
}

pub(super) async fn resolve_bound_project(
    runtime: &ToolRuntime,
    project: &str,
    runner_id: &str,
    scope: &str,
    auth: Option<&AuthContext>,
) -> Result<BoundPluginProject, GatewayError> {
    if !auth.is_some_and(|auth| auth.has_scope(scope)) {
        return Err(GatewayError::local(
            "insufficient_scope",
            "Project-bound Plugin access requires the matching Project scope",
        ));
    }
    let resolved = runtime
        .resolve_project_input_for_auth(project, auth)
        .await
        .map_err(|_| {
            GatewayError::local(
                "plugin_project_unavailable",
                "The exact authorized Project is unavailable",
            )
        })?;
    if resolved.config.client_id != runner_id {
        return Err(GatewayError::local(
            "plugin_project_mismatch",
            "Project and Plugin Runner must match",
        ));
    }
    let project_id = crate::tool_runtime::runner_local_project_id(&resolved.resolved_id)
        .ok_or_else(|| {
            GatewayError::local(
                "plugin_project_unavailable",
                "The exact Runner Project is unavailable",
            )
        })?;
    let root_fingerprint = resolved.root_fingerprint.ok_or_else(|| {
        GatewayError::local(
            "plugin_project_unavailable",
            "The Project root identity is unavailable",
        )
    })?;
    Ok(BoundPluginProject {
        target: PluginProjectTarget {
            project_id: project_id.to_string(),
            root_fingerprint,
        },
        canonical_id: resolved.resolved_id,
    })
}
