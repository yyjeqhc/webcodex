//! Model surface selection for model-facing MCP tool exposure.
//!
//! WebCodex exposes exactly one model surface per process, selected at
//! startup from `WEBCODEX_CONNECTOR_SURFACE` and `WEBCODEX_MCP_MODEL_SURFACE`:
//!
//! - A complete Connector configuration (`WEBCODEX_CONNECTOR_SURFACE=task-v1`)
//!   selects `canonical_connector`.
//! - Without Connector configuration, an unset `WEBCODEX_MCP_MODEL_SURFACE`
//!   selects the focused `local_coding` surface; the explicit
//!   `local-coding-v1` / `full-operator-v1` values select `local_coding` /
//!   `full_operator_runtime` respectively.
//! - Setting Connector configuration and `WEBCODEX_MCP_MODEL_SURFACE` together,
//!   or using an unsupported value, is a startup configuration error and never
//!   falls through to another surface.

use crate::connector_runtime::ConnectorContext;
use crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES;
use crate::tool_runtime::{registered_tool_specs, ToolSpec};

pub(crate) const MODEL_SURFACE_LOCAL_CODING: &str = "local_coding";
pub(crate) const MODEL_SURFACE_FULL_OPERATOR_RUNTIME: &str = "full_operator_runtime";
pub(crate) const MODEL_SURFACE_CANONICAL_CONNECTOR: &str = "canonical_connector";

pub(crate) const MCP_MODEL_SURFACE_ENV: &str = "WEBCODEX_MCP_MODEL_SURFACE";
pub(crate) const MCP_MODEL_SURFACE_LOCAL_CODING_V1: &str = "local-coding-v1";
pub(crate) const MCP_MODEL_SURFACE_FULL_OPERATOR_V1: &str = "full-operator-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelSurface {
    LocalCoding,
    FullOperatorRuntime,
    CanonicalConnector,
}

impl ModelSurface {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::LocalCoding => MODEL_SURFACE_LOCAL_CODING,
            Self::FullOperatorRuntime => MODEL_SURFACE_FULL_OPERATOR_RUNTIME,
            Self::CanonicalConnector => MODEL_SURFACE_CANONICAL_CONNECTOR,
        }
    }
}

/// Resolve the model surface from the process environment.
///
/// `connector_context` is present only after a complete Connector
/// configuration has been parsed and validated. Returns an error for a
/// conflict (Connector plus `WEBCODEX_MCP_MODEL_SURFACE`) or for an unsupported
/// `WEBCODEX_MCP_MODEL_SURFACE` value; the server must fail startup rather
/// than silently serving a different surface.
pub(crate) fn resolve_model_surface(
    connector_context: Option<&ConnectorContext>,
) -> Result<ModelSurface, String> {
    let configured = std::env::var(MCP_MODEL_SURFACE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match (connector_context, configured.as_deref()) {
        (Some(_), Some(value)) => Err(format!(
            "{MCP_MODEL_SURFACE_ENV}='{value}' cannot be combined with WEBCODEX_CONNECTOR_SURFACE; the Connector surface is authoritative"
        )),
        (Some(_), None) => Ok(ModelSurface::CanonicalConnector),
        (None, None) => Ok(ModelSurface::LocalCoding),
        (None, Some(MCP_MODEL_SURFACE_LOCAL_CODING_V1)) => Ok(ModelSurface::LocalCoding),
        (None, Some(MCP_MODEL_SURFACE_FULL_OPERATOR_V1)) => Ok(ModelSurface::FullOperatorRuntime),
        (None, Some(value)) => Err(format!(
            "unsupported {MCP_MODEL_SURFACE_ENV} '{value}'; expected {MCP_MODEL_SURFACE_LOCAL_CODING_V1} or {MCP_MODEL_SURFACE_FULL_OPERATOR_V1}"
        )),
    }
}

/// Registered ToolSpecs for the local_coding surface, in
/// `LOCAL_CODING_TOOL_NAMES` order. Every name must resolve to a registered
/// model-visible ToolSpec; the MCP `tools/list` surface is built from this.
pub(crate) fn local_coding_tool_specs() -> Vec<ToolSpec> {
    let mut by_name: std::collections::HashMap<String, ToolSpec> = registered_tool_specs()
        .into_iter()
        .map(|spec| (spec.name.clone(), spec))
        .collect();
    LOCAL_CODING_TOOL_NAMES
        .iter()
        .map(|name| {
            by_name.remove(*name).unwrap_or_else(|| {
                panic!("{name} local_coding tool is missing a registered ToolSpec")
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::admin_cli::TEST_ENV_LOCK.lock().unwrap()
    }

    #[test]
    fn local_coding_tool_names_are_ordered_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for name in LOCAL_CODING_TOOL_NAMES {
            assert!(
                seen.insert(*name),
                "{name} is duplicated in LOCAL_CODING_TOOL_NAMES"
            );
        }
        assert_eq!(
            LOCAL_CODING_TOOL_NAMES.len(),
            seen.len(),
            "local_coding tool set must be unique"
        );
    }

    #[test]
    fn local_coding_tools_are_fully_registered_in_order() {
        let specs = local_coding_tool_specs();
        let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(names, LOCAL_CODING_TOOL_NAMES);
        for spec in &specs {
            assert!(
                crate::tool_runtime::tool_definition::is_model_visible_tool_name(&spec.name),
                "{} must be model-visible",
                spec.name
            );
        }
    }

    #[test]
    fn computer_tools_are_full_operator_only() {
        let full = registered_tool_specs();
        let full_names = full
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();
        for name in [
            "computer_list_targets",
            "computer_list_windows",
            "computer_accessibility_status",
            "computer_accessibility_tree",
            "computer_find_elements",
            "computer_element_state",
            "computer_activate_window",
            "computer_control",
            "computer_input_text",
            "computer_snapshot",
        ] {
            assert!(
                full_names.contains(&name),
                "{name} must be in full_operator_runtime"
            );
            assert!(
                !LOCAL_CODING_TOOL_NAMES.contains(&name),
                "{name} must not expand local_coding"
            );
            assert!(
                !crate::connector_runtime::surface::CAPABILITY_NAMES.contains(&name),
                "{name} must not expand canonical_connector"
            );
        }
    }

    fn connector_context() -> ConnectorContext {
        ConnectorContext {
            project_id: "wc_proj_1111111111111111".to_string(),
            project_name: "demo".to_string(),
            workspace_id: "wc_ws_1111111111111111".to_string(),
            executor_project: "agent:special:webcodex".to_string(),
            executor_root: "/tmp/webcodex".to_string(),
            runs_root: "/tmp/webcodex-runs".to_string(),
            results_root: "/tmp/webcodex-results".to_string(),
            projects_dir: "/tmp/webcodex-projects".to_string(),
            profile: "default".to_string(),
            project_grant_id: "wc_pgrant_1111111111111111".to_string(),
        }
    }

    #[test]
    fn default_surface_is_local_coding_without_connector_or_env() {
        let _guard = env_guard();
        std::env::remove_var(MCP_MODEL_SURFACE_ENV);
        assert_eq!(resolve_model_surface(None), Ok(ModelSurface::LocalCoding));
    }

    #[test]
    fn explicit_local_coding_and_full_operator_values() {
        let _guard = env_guard();
        std::env::set_var(MCP_MODEL_SURFACE_ENV, MCP_MODEL_SURFACE_LOCAL_CODING_V1);
        assert_eq!(resolve_model_surface(None), Ok(ModelSurface::LocalCoding));
        std::env::set_var(MCP_MODEL_SURFACE_ENV, MCP_MODEL_SURFACE_FULL_OPERATOR_V1);
        assert_eq!(
            resolve_model_surface(None),
            Ok(ModelSurface::FullOperatorRuntime)
        );
        std::env::remove_var(MCP_MODEL_SURFACE_ENV);
    }

    #[test]
    fn connector_configured_selects_canonical_connector() {
        let _guard = env_guard();
        std::env::remove_var(MCP_MODEL_SURFACE_ENV);
        let context = connector_context();
        assert_eq!(
            resolve_model_surface(Some(&context)),
            Ok(ModelSurface::CanonicalConnector)
        );
    }

    #[test]
    fn invalid_and_conflicting_values_fail_resolution() {
        let _guard = env_guard();
        std::env::set_var(MCP_MODEL_SURFACE_ENV, "bogus-surface");
        let error = resolve_model_surface(None).expect_err("invalid value must fail");
        assert!(error.contains("unsupported"), "error: {error}");
        assert!(error.contains("bogus-surface"), "error: {error}");

        std::env::set_var(MCP_MODEL_SURFACE_ENV, MCP_MODEL_SURFACE_LOCAL_CODING_V1);
        let context = connector_context();
        let error = resolve_model_surface(Some(&context)).expect_err("conflict must fail");
        assert!(error.contains("cannot be combined"), "error: {error}");
        assert!(error.contains(MCP_MODEL_SURFACE_ENV), "error: {error}");
        std::env::remove_var(MCP_MODEL_SURFACE_ENV);
    }
}
