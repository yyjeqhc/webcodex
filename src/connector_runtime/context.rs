//! Connector-side environment configuration.
//!
//! `ConnectorContext` is the resolved set of environment-supplied identity
//! and path fields that bind the runtime to a single project/workspace. It is
//! read once during server startup and thereafter
//! treated as immutable by every capability method via `self.context`. The
//! `required_env` / `nonempty_env` helpers and the surface-name constants live
//! here so the rest of the runtime module reads as orchestration rather than
//! env plumbing.

use super::validate_opaque_id;
use std::path::Path;

pub(crate) const CONNECTOR_SURFACE_ENV: &str = "WEBCODEX_CONNECTOR_SURFACE";
pub(crate) const CONNECTOR_SURFACE_TASK_V1: &str = "task-v1";
pub(crate) const PROJECT_CREDENTIAL_FILE_ENV: &str = "WEBCODEX_PROJECT_CREDENTIAL_FILE";
pub(crate) const PROJECT_AGENT_TOKEN_FILE_ENV: &str = "WEBCODEX_PROJECT_AGENT_TOKEN_FILE";
const PROJECT_REGISTRY_DIR_ENV: &str = "WEBCODEX_CONNECTOR_PROJECT_REGISTRY_DIR";
const LEGACY_PROJECTS_DIR_ENV: &str = "WEBCODEX_CONNECTOR_PROJECTS_DIR";

const CONNECTOR_CONFIGURATION_ENV_NAMES: &[&str] = &[
    "WEBCODEX_CONNECTOR_PROJECT_ID",
    "WEBCODEX_CONNECTOR_PROJECT_NAME",
    "WEBCODEX_CONNECTOR_WORKSPACE_ID",
    "WEBCODEX_CONNECTOR_EXECUTOR_PROJECT",
    "WEBCODEX_CONNECTOR_EXECUTOR_ROOT",
    "WEBCODEX_CONNECTOR_RUNS_ROOT",
    "WEBCODEX_CONNECTOR_RESULTS_ROOT",
    PROJECT_REGISTRY_DIR_ENV,
    LEGACY_PROJECTS_DIR_ENV,
    "WEBCODEX_CONNECTOR_PROFILE",
    "WEBCODEX_CONNECTOR_PROJECT_GRANT_ID",
    PROJECT_CREDENTIAL_FILE_ENV,
    PROJECT_AGENT_TOKEN_FILE_ENV,
];

pub(crate) fn required_env(name: &str) -> Result<String, String> {
    nonempty_env(name)
        .ok_or_else(|| format!("{name} is required when connector surface is enabled"))
}

pub(crate) fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn project_registry_dir_env() -> Result<String, String> {
    match (
        nonempty_env(PROJECT_REGISTRY_DIR_ENV),
        nonempty_env(LEGACY_PROJECTS_DIR_ENV),
    ) {
        (Some(_), Some(_)) => Err(format!(
            "{PROJECT_REGISTRY_DIR_ENV} and legacy {LEGACY_PROJECTS_DIR_ENV} cannot both be configured"
        )),
        (Some(value), None) | (None, Some(value)) => Ok(value),
        (None, None) => Err(format!(
            "{PROJECT_REGISTRY_DIR_ENV} is required when connector surface is enabled"
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectorContext {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) workspace_id: String,
    pub(crate) executor_project: String,
    pub(crate) executor_root: String,
    pub(crate) runs_root: String,
    pub(crate) results_root: String,
    pub(crate) project_registry_dir: String,
    pub(crate) profile: String,
    pub(crate) project_grant_id: String,
}

impl ConnectorContext {
    pub(crate) fn from_env() -> Result<Option<Self>, String> {
        let surface = nonempty_env(CONNECTOR_SURFACE_ENV);
        let Some(surface) = surface else {
            if let Some(name) = CONNECTOR_CONFIGURATION_ENV_NAMES
                .iter()
                .copied()
                .find(|name| nonempty_env(name).is_some())
            {
                return Err(format!(
                    "{name} is set but {CONNECTOR_SURFACE_ENV} is not configured"
                ));
            }
            return Ok(None);
        };
        if surface != CONNECTOR_SURFACE_TASK_V1 {
            return Err(format!(
                "unsupported {CONNECTOR_SURFACE_ENV} '{surface}'; expected {CONNECTOR_SURFACE_TASK_V1}"
            ));
        }
        let context = Self {
            project_id: required_env("WEBCODEX_CONNECTOR_PROJECT_ID")?,
            project_name: required_env("WEBCODEX_CONNECTOR_PROJECT_NAME")?,
            workspace_id: required_env("WEBCODEX_CONNECTOR_WORKSPACE_ID")?,
            executor_project: required_env("WEBCODEX_CONNECTOR_EXECUTOR_PROJECT")?,
            executor_root: required_env("WEBCODEX_CONNECTOR_EXECUTOR_ROOT")?,
            runs_root: required_env("WEBCODEX_CONNECTOR_RUNS_ROOT")?,
            results_root: required_env("WEBCODEX_CONNECTOR_RESULTS_ROOT")?,
            project_registry_dir: project_registry_dir_env()?,
            profile: required_env("WEBCODEX_CONNECTOR_PROFILE")?,
            project_grant_id: required_env("WEBCODEX_CONNECTOR_PROJECT_GRANT_ID")?,
        };
        context.validate()?;
        Ok(Some(context))
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_opaque_id(&self.project_id, "wc_proj_", "connector project id")?;
        validate_opaque_id(&self.workspace_id, "wc_ws_", "connector workspace id")?;
        if !self.executor_project.starts_with("agent:") {
            return Err(
                "WEBCODEX_CONNECTOR_EXECUTOR_PROJECT must be an agent-backed runtime id".into(),
            );
        }
        if !Path::new(&self.executor_root).is_absolute() || self.executor_root == "/" {
            return Err(
                "WEBCODEX_CONNECTOR_EXECUTOR_ROOT must be an absolute non-root project path".into(),
            );
        }
        if self.project_name.trim().is_empty() || self.project_name.len() > 200 {
            return Err("WEBCODEX_CONNECTOR_PROJECT_NAME must be 1..=200 bytes".into());
        }
        if self.profile.trim().is_empty() || self.profile.len() > 100 {
            return Err("WEBCODEX_CONNECTOR_PROFILE must be 1..=100 bytes".into());
        }
        validate_opaque_id(
            &self.project_grant_id,
            "wc_pgrant_",
            "connector project grant id",
        )?;
        Ok(())
    }

    pub(crate) fn executor_client_id(&self) -> Result<&str, String> {
        self.executor_project
            .strip_prefix("agent:")
            .and_then(|value| value.split_once(':'))
            .map(|(client_id, _)| client_id)
            .filter(|client_id| !client_id.is_empty())
            .ok_or_else(|| "connector executor reference is malformed".to_string())
    }
}
