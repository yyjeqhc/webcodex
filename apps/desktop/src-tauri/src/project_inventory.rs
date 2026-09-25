//! Registry-only removal through the canonical runtime gate. No filesystem deletion.
use crate::error::{DesktopError, DesktopResult};
use crate::models::{ProjectSelection, StoredDesktopConfig, StoredRuntime};
use crate::webcodex::settings::{self, SettingsTarget};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

#[cfg(test)]
#[path = "project_inventory/tests.rs"]
pub(crate) mod tests;

#[derive(Serialize)]
pub struct UnregisterObservation {
    pub target: SettingsTarget,
    pub project: String,
    pub expected_revision: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnregisterRequest {
    pub target: SettingsTarget,
    pub project: String,
    pub expected_revision: String,
    pub confirmed: bool,
}

fn unavailable() -> DesktopError {
    DesktopError::new(
        "project_unregister_unavailable",
        "Project registration could not be verified",
        "Refresh the inventory and inspect the exact project again.",
    )
}

fn uncertain() -> DesktopError {
    DesktopError::new("project_unregister_uncertain", "The unregister outcome could not be confirmed", "Refresh the Runner inventory before taking further action. No automatic retry or folder deletion was attempted.")
}

pub async fn observe(
    runtime: &StoredRuntime,
    project: &str,
) -> DesktopResult<UnregisterObservation> {
    let target = settings::target(runtime)?;
    if project.len() > 512
        || project.chars().any(char::is_control)
        || !project.starts_with(&format!("agent:{}:", target.client_id))
    {
        return Err(unavailable());
    }
    let value = crate::workspace::post(
        runtime,
        "/api/tools/call",
        json!({"tool":"list_projects","params":{}}),
        Duration::from_secs(12),
    )
    .await?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(unavailable());
    }
    let row = value
        .pointer("/output/projects")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(project))
        })
        .ok_or_else(unavailable)?;
    let revision = row
        .get("revision")
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() == 71
                && s.starts_with("sha256:")
                && s[7..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .ok_or_else(unavailable)?;
    let path = row
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(unavailable)?;
    Ok(UnregisterObservation {
        target,
        project: project.to_owned(),
        expected_revision: revision.to_owned(),
        path: path.to_owned(),
    })
}

pub async fn unregister(
    runtime: &StoredRuntime,
    request: &UnregisterRequest,
) -> DesktopResult<String> {
    settings::verify_target(runtime, &request.target)?;
    if !request.confirmed {
        return Err(unavailable());
    }
    let observation = observe(runtime, &request.project).await?;
    if observation.expected_revision != request.expected_revision {
        return Err(unavailable());
    }
    let value = crate::workspace::post(runtime, "/api/tools/call", json!({"tool":"unregister_project", "params":{"project":request.project,"expected_revision":request.expected_revision}}), Duration::from_secs(40)).await.map_err(|_| uncertain())?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        // Do not expose arbitrary upstream text or infer that a failed response had no effects.
        return Err(uncertain());
    }
    let output = value.get("output").ok_or_else(uncertain)?;
    if output.get("project").and_then(Value::as_str) != Some(&request.project)
        || !matches!(
            output.get("outcome").and_then(Value::as_str),
            Some("unregistered" | "already_unregistered")
        )
    {
        return Err(uncertain());
    }
    if output.get("outcome").and_then(Value::as_str) == Some("already_unregistered") {
        let overview =
            crate::workspace::query(runtime, crate::workspace::WorkspaceRequest::Overview {})
                .await
                .map_err(|_| uncertain())?;
        let rows = complete_inventory(runtime, &overview).ok_or_else(uncertain)?;
        if rows
            .iter()
            .any(|row| row.get("id").and_then(Value::as_str) == Some(&request.project))
        {
            return Err(uncertain());
        }
    }
    Ok(observation.path)
}

pub fn forget(config: &mut StoredDesktopConfig, project: &str, path: &str) {
    let Some(runtime) = config.runtime.as_mut() else {
        return;
    };
    let matches = |saved: &ProjectSelection| match saved.runtime_project_id.as_deref() {
        Some(id) => id == project,
        None => webcodex_runner_config::paths::paths_equal(Path::new(&saved.path), Path::new(path)),
    };
    config.saved_projects.retain(|entry| {
        Some(&entry.runner_config) != runtime.runner_config.as_ref() || !matches(&entry.project)
    });
    if config.project.as_ref().is_some_and(matches) {
        config.project = None;
        runtime.project_id = None;
        runtime.runtime_project_id = None;
    }
}

/// A bounded, complete observation of this exact Runner is the only negative
/// evidence allowed to retire saved registration history.
fn complete_inventory<'a>(runtime: &StoredRuntime, overview: &'a Value) -> Option<&'a Vec<Value>> {
    let Some(client_id) = runtime.runner_client_id.as_deref() else {
        return None;
    };
    let Some(rows) = overview.get("projects").and_then(Value::as_array) else {
        return None;
    };
    if overview.get("client_id").and_then(Value::as_str) != Some(client_id)
        || overview.get("connected").and_then(Value::as_bool) != Some(true)
        || overview.get("projects_available").and_then(Value::as_bool) != Some(true)
        || overview.get("projects_truncated").and_then(Value::as_bool) != Some(false)
        || overview
            .get("visible_project_count")
            .and_then(Value::as_u64)
            != Some(rows.len() as u64)
        || rows.len() > 32
        || rows.iter().any(|row| {
            !row.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.starts_with(&format!("agent:{client_id}:")))
                || row.get("path").and_then(Value::as_str).is_none()
        })
    {
        return None;
    }
    Some(rows)
}

pub fn reconcile(config: &mut StoredDesktopConfig, overview: &Value) -> bool {
    let Some(runtime) = config.runtime.as_ref() else {
        return false;
    };
    let Some(scope) = runtime.runner_config.as_ref() else {
        return false;
    };
    let Some(rows) = complete_inventory(runtime, overview) else {
        return false;
    };
    let present = |saved: &ProjectSelection| {
        rows.iter()
            .any(|row| match saved.runtime_project_id.as_deref() {
                Some(id) => row.get("id").and_then(Value::as_str) == Some(id),
                None => row.get("path").and_then(Value::as_str).is_some_and(|path| {
                    webcodex_runner_config::paths::paths_equal(
                        Path::new(path),
                        Path::new(&saved.path),
                    )
                }),
            })
    };
    let before = config.saved_projects.len();
    config
        .saved_projects
        .retain(|saved| &saved.runner_config != scope || present(&saved.project));
    let stale_default = config
        .project
        .as_ref()
        .is_some_and(|project| !present(project));
    if stale_default {
        config.project = None;
        let runtime = config.runtime.as_mut().expect("scoped runtime");
        runtime.project_id = None;
        runtime.runtime_project_id = None;
    }
    before != config.saved_projects.len() || stale_default
}
