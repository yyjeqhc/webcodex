//! Console-specific bounded projection over the canonical typed Job query.
//! No model ToolResult/JSON round trip, raw record serialization, or extra
//! lifecycle refresh. Endpoint authorization stays with the HTTP adapter.
use super::{
    bounded_text_str, is_valid_session_id, valid_project_id, RuntimeConsoleError,
    MAX_PROJECT_ID_CHARS,
};
use crate::auth::{AuthContext, SCOPE_RUNTIME_READ};
use crate::tool_runtime::ToolRuntime;
use serde::Serialize;
use std::collections::HashMap;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runner_protocol::{ShellJobActivityPhase, ShellJobActivityState, ShellJobInfo};

#[derive(Debug, Default)]
pub(super) struct RunningJobSnapshot {
    counts: HashMap<(String, String), usize>,
    pub(super) truncated: bool,
}

impl RunningJobSnapshot {
    pub(super) fn count(&self, project: &str, session_id: &str) -> usize {
        self.counts
            .get(&(project.to_string(), session_id.to_string()))
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct RuntimeConsoleSessionJob {
    pub(super) job_id: String,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) terminal: bool,
    pub(super) created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) activity_state: Option<ShellJobActivityState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) activity_phase: Option<ShellJobActivityPhase>,
}

fn session_job(job: &ShellJobInfo) -> Option<RuntimeConsoleSessionJob> {
    let status = bounded_text_str(&job.status, 80)?;
    Some(RuntimeConsoleSessionJob {
        job_id: bounded_text_str(&job.job_id, 160)?,
        kind: bounded_text_str(&job.kind, 80)?,
        terminal: RunnerJobLifecycle::from_wire(&status).is_ok_and(RunnerJobLifecycle::is_terminal),
        status,
        created_at: job.created_at,
        started_at: job.started_at,
        ended_at: job.ended_at,
        activity_state: job.activity.map(|activity| activity.state),
        activity_phase: job.activity.map(|activity| activity.phase),
    })
}

pub(super) async fn running_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: Option<&str>,
) -> Result<RunningJobSnapshot, RuntimeConsoleError> {
    if !auth.has_scope(SCOPE_RUNTIME_READ) {
        return Ok(RunningJobSnapshot::default());
    }
    let page = runtime
        .query_job_inventory_for_auth(Some(100), Some("running"), project, None, Some(auth))
        .await
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let mut snapshot = RunningJobSnapshot {
        truncated: page.truncated(),
        ..Default::default()
    };
    for job in &page.jobs {
        let Some(project) = job
            .project_id
            .as_deref()
            .and_then(|value| bounded_text_str(value, MAX_PROJECT_ID_CHARS))
        else {
            continue;
        };
        let Some(session_id) = job
            .session_id
            .as_deref()
            .and_then(|value| bounded_text_str(value, 160))
        else {
            continue;
        };
        if !valid_project_id(&project) || !is_valid_session_id(&session_id) {
            continue;
        }
        snapshot
            .counts
            .entry((project, session_id))
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
    Ok(snapshot)
}

pub(super) async fn session_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
) -> Result<(Vec<RuntimeConsoleSessionJob>, bool), RuntimeConsoleError> {
    let page = runtime
        .query_job_inventory_for_auth(Some(100), None, Some(project), Some(session_id), Some(auth))
        .await
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok((
        page.jobs.iter().filter_map(session_job).collect(),
        page.truncated(),
    ))
}

#[cfg(test)]
mod tests;
