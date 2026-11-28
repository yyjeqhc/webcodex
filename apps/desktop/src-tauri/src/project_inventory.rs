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

pub(crate) fn revision_changed() -> DesktopError {
    DesktopError::new(
        "project_revision_changed",
        "The project changed since confirmation",
        "Review the current project again before removing it.",
    )
}

fn uncertain() -> DesktopError {
    DesktopError::new("project_unregister_uncertain", "The unregister outcome could not be confirmed", "Refresh the Runner inventory before taking further action. No automatic retry or folder deletion was attempted.")
}

pub async fn observe(
    runtime: &StoredRuntime,
    project: &str,
) -> DesktopResult<UnregisterObservation> {
    observe_many(runtime, &[project.to_owned()])
        .await?
        .pop()
        .ok_or_else(unavailable)
}

pub async fn observe_many(
    runtime: &StoredRuntime,
    projects: &[String],
) -> DesktopResult<Vec<UnregisterObservation>> {
    let target = settings::target(runtime)?;
    if projects.is_empty() || projects.len() > 16 {
        return Err(unavailable());
    }
    let mut seen = std::collections::HashSet::new();
    for project in projects {
        if project.len() > 512
            || project.chars().any(char::is_control)
            || !project.starts_with(&format!("agent:{}:", target.client_id))
            || !seen.insert(project.as_str())
        {
            return Err(unavailable());
        }
    }
    let exact = |project: &str| json!({"project":project,"limit":1,"summary_only":false});
    let params = if projects.len() == 1 {
        exact(&projects[0])
    } else {
        json!({"client_id":target.client_id,"limit":100,"summary_only":false})
    };
    let page = list_for_removal(runtime, params).await?;
    let rows = page
        .get("projects")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    let mut observations = Vec::with_capacity(projects.len());
    for project in projects {
        let exact_page;
        let row = match rows.iter().find(|row| row["id"].as_str() == Some(project)) {
            Some(row) => row,
            None if projects.len() > 1 && page["truncated"].as_bool() == Some(true) => {
                exact_page = list_for_removal(runtime, exact(project)).await?;
                exact_page["projects"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["id"].as_str() == Some(project)))
                    .ok_or_else(unavailable)?
            }
            None => return Err(unavailable()),
        };
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
        observations.push(UnregisterObservation {
            target: target.clone(),
            project: project.clone(),
            expected_revision: revision.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(observations)
}

async fn list_for_removal(runtime: &StoredRuntime, params: Value) -> DesktopResult<Value> {
    let mut value = crate::workspace::post(
        runtime,
        "/api/tools/call",
        json!({"tool":"list_projects","params":params}),
        Duration::from_secs(12),
    )
    .await?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(unavailable());
    }
    Ok(value["output"].take())
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
        return Err(revision_changed());
    }
    let value = crate::workspace::post(runtime, "/api/tools/call", json!({"tool":"unregister_project", "params":{"project":request.project,"expected_revision":request.expected_revision}}), Duration::from_secs(40)).await.map_err(|_| uncertain())?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(
            match value.pointer("/output/error/code").and_then(Value::as_str) {
                Some("project_projection_reconcile_required")
                    if value.pointer("/output/project").and_then(Value::as_str)
                        == Some(&request.project)
                        && matches!(
                            value
                                .pointer("/output/authoritative_outcome")
                                .and_then(Value::as_str),
                            Some("unregistered" | "already_unregistered")
                        ) =>
                {
                    DesktopError::new(
                        "project_projection_reconcile_required",
                        "Runner confirmed removal but Server inventory is pending",
                        "Refresh the inventory before taking further action.",
                    )
                }
                Some("active_jobs_conflict") => DesktopError::new(
                    "active_jobs_conflict",
                    "The project still has active jobs",
                    "Wait for those jobs to finish before removing the project.",
                ),
                Some("revision_conflict") => revision_changed(),
                _ => uncertain(),
            },
        );
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
        let overview = crate::workspace::query(
            runtime,
            crate::workspace::WorkspaceRequest::RunnerDetails {},
        )
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchUnregisterRequest {
    pub items: Vec<UnregisterRequest>,
}

#[derive(Serialize)]
pub struct BatchUnregisterResult {
    pub items: Vec<BatchUnregisterItem>,
    pub state: crate::models::DesktopStateSnapshot,
}

#[derive(Serialize)]
pub struct BatchUnregisterItem {
    pub project: String,
    pub success: bool,
    pub status: String,
}

pub async fn unregister_batch(
    runtime: &StoredRuntime,
    requests: &[UnregisterRequest],
) -> DesktopResult<Vec<BatchUnregisterItem>> {
    if requests.is_empty() || requests.len() > 16 {
        return Err(unavailable());
    }
    let mut ids = std::collections::HashSet::new();
    for request in requests {
        settings::verify_target(runtime, &request.target)?;
        if !request.confirmed
            || !ids.insert(request.project.as_str())
            || request.project.len() > 512
            || !request
                .project
                .starts_with(&format!("agent:{}:", request.target.client_id))
            || request.expected_revision.len() != 71
            || !request.expected_revision.starts_with("sha256:")
            || !request.expected_revision[7..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(unavailable());
        }
    }
    let items: Vec<Value> = requests
        .iter()
        .map(|item| json!({"project":item.project,"expected_revision":item.expected_revision}))
        .collect();
    let response = crate::workspace::post(runtime, "/api/tools/call",
        json!({"tool":"unregister_projects","params":{"items":items,"dry_run":false,"confirm":true}}),
        Duration::from_secs(120)).await.map_err(|_| uncertain())?;
    let results = response
        .pointer("/output/items")
        .and_then(Value::as_array)
        .ok_or_else(uncertain)?;
    if results.len() != requests.len() {
        return Err(uncertain());
    }
    let mut parsed = Vec::with_capacity(requests.len());
    for (index, (item, request)) in results.iter().zip(requests.iter()).enumerate() {
        if item.get("project").and_then(Value::as_str) != Some(request.project.as_str())
            || item.get("index").and_then(Value::as_u64) != Some(index as u64)
        {
            return Err(uncertain());
        }
        let success = item
            .get("success")
            .and_then(Value::as_bool)
            .ok_or_else(uncertain)?;
        let status = if success {
            let outcome = item
                .pointer("/output/outcome")
                .and_then(Value::as_str)
                .ok_or_else(uncertain)?;
            if !matches!(outcome, "unregistered" | "already_unregistered") {
                return Err(uncertain());
            }
            outcome
        } else {
            match item
                .pointer("/output/error/code")
                .and_then(Value::as_str)
                .or_else(|| item.get("error").and_then(Value::as_str))
            {
                Some("not_attempted_after_uncertain_outcome") => {
                    "not_attempted_after_uncertain_outcome"
                }
                Some("project_projection_reconcile_required") => {
                    "project_projection_reconcile_required"
                }
                Some("active_jobs_conflict") => "active_jobs_conflict",
                Some("revision_conflict") => "revision_conflict",
                Some("project_not_found") => "project_not_found",
                _ => "failed_or_unconfirmed",
            }
        };
        parsed.push(BatchUnregisterItem {
            project: request.project.clone(),
            success,
            status: status.to_owned(),
        });
    }
    Ok(parsed)
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
