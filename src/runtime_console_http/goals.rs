use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    exact_console_project_for_auth, prepared, render_error, require_communication_read,
    require_project_read, workflow_session_detail_with_windows, workflow_session_locate_for_auth,
    RuntimeConsoleError,
};

const GOAL_LIST_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalsInput {
    #[serde(default)]
    project: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalInput {
    goal_id: String,
}

fn tool_output(result: crate::tool_runtime::ToolResult) -> Result<Value, RuntimeConsoleError> {
    if result.success {
        return Ok(result.output);
    }
    match result
        .output
        .get("error_kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "goal_not_found" | "agent_task_not_found" | "agent_not_found" | "agent_wait_not_found" => {
            Err(RuntimeConsoleError::NotFound)
        }
        "communication_principal_unavailable" => Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read access required",
        }),
        _ => Err(RuntimeConsoleError::Internal),
    }
}

fn goal_id_from(value: &Value) -> Option<String> {
    value
        .get("goal_id")
        .and_then(Value::as_str)
        .filter(|goal_id| goal_id.starts_with("wc_goal_"))
        .map(str::to_string)
}

async fn raw_goal(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal_id: &str,
) -> Result<Value, RuntimeConsoleError> {
    let output = tool_output(runtime.get_goal(Some(auth), goal_id.to_string()))?;
    output
        .get("goal")
        .cloned()
        .ok_or(RuntimeConsoleError::Internal)
}

async fn goal_plan(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal_id: &str,
) -> Result<Value, RuntimeConsoleError> {
    let output = tool_output(
        runtime
            .present_goal_plan(Some(auth), goal_id.to_string())
            .await,
    )?;
    output
        .get("goal_plan")
        .cloned()
        .ok_or(RuntimeConsoleError::Internal)
}

async fn task_value(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    task_id: &str,
) -> Option<Value> {
    let result = runtime.read_agent_task(Some(auth), task_id.to_string());
    result
        .success
        .then(|| result.output.get("task").cloned())
        .flatten()
}

fn correlation_rows(goal: &Value) -> impl Iterator<Item = (&str, &str)> {
    goal.get("correlations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some((
                row.get("kind")?.as_str()?,
                row.get("reference_id")?.as_str()?,
            ))
        })
}

async fn project_ids_for_goal(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal: &Value,
) -> BTreeSet<String> {
    let mut projects = BTreeSet::new();
    for (kind, reference_id) in correlation_rows(goal) {
        match kind {
            "workflow_session" => {
                if let Ok(session) =
                    workflow_session_locate_for_auth(runtime, auth, reference_id).await
                {
                    projects.insert(session.project_id);
                }
            }
            "agent_task" => {
                if let Some(task) = task_value(runtime, auth, reference_id).await {
                    if let Some(project_id) = task
                        .pointer("/summary/referenced_project_id")
                        .and_then(Value::as_str)
                    {
                        if exact_console_project_for_auth(runtime, auth, project_id)
                            .await
                            .is_ok()
                        {
                            projects.insert(project_id.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    projects
}

fn goal_list_row(goal: &Value, projects: BTreeSet<String>) -> Result<Value, RuntimeConsoleError> {
    let summary = goal.get("summary").ok_or(RuntimeConsoleError::Internal)?;
    let plan = goal.get("plan").ok_or(RuntimeConsoleError::Internal)?;
    let steps = plan
        .get("steps")
        .and_then(Value::as_array)
        .ok_or(RuntimeConsoleError::Internal)?;
    let completed = steps
        .iter()
        .filter(|step| step.get("status").and_then(Value::as_str) == Some("completed"))
        .count();
    let current = steps
        .iter()
        .find(|step| step.get("status").and_then(Value::as_str) == Some("in_progress"));
    Ok(json!({
        "goal_id": summary.get("goal_id"),
        "title": summary.get("title"),
        "lifecycle": summary.get("lifecycle"),
        "revision": summary.get("revision"),
        "updated_at_unix_ms": summary.get("updated_at_unix_ms"),
        "agent_task_count": summary.get("agent_task_count"),
        "workflow_session_count": summary.get("workflow_session_count"),
        "total_step_count": steps.len(),
        "completed_step_count": completed,
        "current_step_id": current.and_then(|step| step.get("id")),
        "current_step_title": current.and_then(|step| step.get("title")),
        "progress_summary": plan.get("progress_summary"),
        "checkpoint_at_unix_ms": plan.get("checkpoint_at_unix_ms"),
        "project_ids": projects.into_iter().collect::<Vec<_>>(),
    }))
}

async fn goals_for_auth(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    project: Option<&str>,
) -> Result<Value, RuntimeConsoleError> {
    if let Some(project) = project {
        let _ = exact_console_project_for_auth(runtime, auth, project).await?;
    }
    let page = tool_output(runtime.list_goals(Some(auth), None, Some(0), Some(GOAL_LIST_LIMIT)))?;
    let rows = page
        .get("goals")
        .and_then(Value::as_array)
        .ok_or(RuntimeConsoleError::Internal)?;
    let mut goals = Vec::new();
    for row in rows {
        let Some(goal_id) = goal_id_from(row) else {
            continue;
        };
        let goal = raw_goal(runtime, auth, &goal_id).await?;
        let projects = project_ids_for_goal(runtime, auth, &goal).await;
        if project.is_some_and(|wanted| !projects.contains(wanted)) {
            continue;
        }
        goals.push(goal_list_row(&goal, projects)?);
    }
    Ok(json!({
        "goals": goals,
        "total": goals.len(),
        "source_total": page.get("total_count").cloned().unwrap_or(json!(rows.len())),
        "truncated": page.get("truncated").and_then(Value::as_bool).unwrap_or(false),
    }))
}

async fn exact_agent(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    agent_id: &str,
) -> Option<Value> {
    let result =
        runtime.list_agent_identities(Some(auth), Some(agent_id.to_string()), Some(0), Some(1));
    if !result.success {
        return None;
    }
    result
        .output
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.first())
        .cloned()
}

async fn goal_detail_for_auth(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal_id: &str,
) -> Result<Value, RuntimeConsoleError> {
    let goal = raw_goal(runtime, auth, goal_id).await?;
    let plan = goal_plan(runtime, auth, goal_id).await?;
    let project_ids = project_ids_for_goal(runtime, auth, &goal).await;

    let mut projects = Vec::new();
    for project_id in &project_ids {
        if let Ok(project) = exact_console_project_for_auth(runtime, auth, project_id).await {
            projects
                .push(serde_json::to_value(project).map_err(|_| RuntimeConsoleError::Internal)?);
        }
    }

    let mut sessions = Vec::new();
    let mut tasks = Vec::new();
    let mut agent_ids = BTreeSet::new();
    let mut windows: BTreeMap<String, Value> = BTreeMap::new();

    if let Some(controller) = goal.get("controller_agent_id").and_then(Value::as_str) {
        agent_ids.insert(controller.to_string());
    }

    for (kind, reference_id) in correlation_rows(&goal) {
        match kind {
            "workflow_session" => {
                let Ok(located) =
                    workflow_session_locate_for_auth(runtime, auth, reference_id).await
                else {
                    continue;
                };
                let project_id = located.project_id.clone();
                let session_id = located.session.session_id.clone();
                sessions.push(json!({
                    "session_id": session_id,
                    "title": located.session.title,
                    "lifecycle": located.session.lifecycle,
                    "mode": located.session.mode,
                    "updated_at": located.session.updated_at,
                    "running_jobs": located.session.running_jobs,
                    "project_id": project_id,
                    "project_name": located.project_name,
                    "client_id": located.client_id,
                }));
                if let Ok(observed) = workflow_session_detail_with_windows(
                    runtime,
                    auth,
                    &project_id,
                    &session_id,
                    Some(1),
                )
                .await
                {
                    for window in observed.linked_windows {
                        let entry = windows.entry(window.client_window_key.clone()).or_insert_with(|| json!({
                            "client_window_key": window.client_window_key,
                            "source": window.source,
                            "last_seen_at_ms": window.last_seen_at_ms,
                            "last_meaningful_activity_at_ms": window.last_meaningful_activity_at_ms,
                            "active_count": window.active_count,
                            "session_ids": [],
                        }));
                        if let Some(existing) = entry.get("last_seen_at_ms").and_then(Value::as_i64)
                        {
                            entry["last_seen_at_ms"] = json!(existing.max(window.last_seen_at_ms));
                        }
                        let latest_meaningful = entry
                            .get("last_meaningful_activity_at_ms")
                            .and_then(Value::as_i64)
                            .into_iter()
                            .chain(window.last_meaningful_activity_at_ms)
                            .max();
                        entry["last_meaningful_activity_at_ms"] = json!(latest_meaningful);
                        let active = entry
                            .get("active_count")
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                            .max(window.active_count as u64);
                        entry["active_count"] = json!(active);
                        if let Some(ids) =
                            entry.get_mut("session_ids").and_then(Value::as_array_mut)
                        {
                            if !ids.iter().any(|value| value.as_str() == Some(&session_id)) {
                                ids.push(json!(session_id));
                            }
                        }
                    }
                }
            }
            "agent_task" => {
                if let Some(task) = task_value(runtime, auth, reference_id).await {
                    if let Some(agent_id) = task
                        .pointer("/summary/assignee_agent_id")
                        .and_then(Value::as_str)
                    {
                        agent_ids.insert(agent_id.to_string());
                    }
                    tasks.push(task);
                }
            }
            _ => {}
        }
    }

    let waits_output =
        tool_output(runtime.list_goal_agent_waits_for_console(Some(auth), goal_id.to_string()))?;
    let waits = waits_output
        .get("waits")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(RuntimeConsoleError::Internal)?;
    let waits_truncated = waits_output
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or(RuntimeConsoleError::Internal)?;

    for wait in &waits {
        if let Some(agent_id) = wait.get("target_agent_id").and_then(Value::as_str) {
            agent_ids.insert(agent_id.to_string());
        }
    }

    let mut agents = Vec::new();
    for agent_id in agent_ids {
        if let Some(agent) = exact_agent(runtime, auth, &agent_id).await {
            agents.push(agent);
        }
    }

    Ok(json!({
        "goal": goal,
        "goal_plan": plan,
        "projects": projects,
        "sessions": sessions,
        "tasks": tasks,
        "waits": waits,
        "waits_truncated": waits_truncated,
        "agents": agents,
        "windows": windows.into_values().collect::<Vec<_>>(),
    }))
}

#[handler]
pub(super) async fn goals_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth).and_then(|_| require_project_read(&auth))
    {
        return render_error(res, error);
    }
    let input = match req.parse_json::<GoalsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match goals_for_auth(&runtime, &auth, input.project.as_deref()).await {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn goal_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth).and_then(|_| require_project_read(&auth))
    {
        return render_error(res, error);
    }
    let input = match req.parse_json::<GoalInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match goal_detail_for_auth(&runtime, &auth, &input.goal_id).await {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, error),
    }
}

#[cfg(test)]
pub(super) async fn goals_for_auth_test(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    project: Option<&str>,
) -> Result<Value, RuntimeConsoleError> {
    goals_for_auth(runtime, auth, project).await
}

#[cfg(test)]
pub(super) async fn goal_detail_for_auth_test(
    runtime: &crate::tool_runtime::ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal_id: &str,
) -> Result<Value, RuntimeConsoleError> {
    goal_detail_for_auth(runtime, auth, goal_id).await
}
