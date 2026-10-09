//! Console-specific bounded projection over the canonical typed Job query.
//! No model ToolResult/JSON round trip, raw record serialization, or extra
//! lifecycle refresh. Endpoint authorization stays with the HTTP adapter.
use super::{
    bounded_text_str, is_valid_session_id, valid_project_id, RuntimeConsoleError,
    MAX_PROJECT_ID_CHARS,
};
use crate::auth::{AuthContext, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ};
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

/// Bounded, credential-filtered Runner Job inventory for operator diagnostics.
/// This is a presentation of canonical Job state, not another lifecycle owner.
#[derive(Debug, Clone, Serialize)]
pub(super) struct RuntimeConsoleRunnerJob {
    pub(super) job_id: String,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) terminal: bool,
    pub(super) created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) session_id: Option<String>,
}

pub(super) async fn runner_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: &str,
) -> Result<(Vec<RuntimeConsoleRunnerJob>, bool), RuntimeConsoleError> {
    let page = runtime
        .query_job_inventory_for_auth(Some(100), None, None, None, Some(client_id), Some(auth))
        .await
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let truncated = page.truncated();
    // Job visibility is admission-time authority. Project/Session association
    // additionally requires current Project visibility so retained Jobs cannot
    // disclose an unregistered or otherwise no-longer-visible Project anchor.
    let visible_projects = if auth.has_scope(SCOPE_PROJECT_READ) {
        let requested = page
            .jobs
            .iter()
            .filter_map(|job| {
                job.project_id
                    .as_deref()
                    .and_then(|id| bounded_text_str(id, MAX_PROJECT_ID_CHARS))
            })
            .collect::<Vec<_>>();
        let access = crate::runner_http::runner_access_from_auth(Some(auth));
        runtime
            .runner_registry
            .visible_project_ids_for_auth_snapshot(access.as_ref(), &requested)
            .await
    } else {
        std::collections::HashSet::new()
    };
    let jobs = page
        .jobs
        .iter()
        .filter_map(|job| {
            let status = bounded_text_str(&job.status, 80)?;
            let project_id = job
                .project_id
                .as_deref()
                .filter(|id| visible_projects.contains(*id))
                .and_then(|id| bounded_text_str(id, MAX_PROJECT_ID_CHARS));
            Some(RuntimeConsoleRunnerJob {
                job_id: bounded_text_str(&job.job_id, 160)?,
                kind: bounded_text_str(&job.kind, 80)?,
                terminal: RunnerJobLifecycle::from_wire(&status)
                    .is_ok_and(RunnerJobLifecycle::is_terminal),
                status,
                created_at: job.created_at,
                started_at: job.started_at,
                elapsed_secs: job.elapsed_secs,
                session_id: project_id.as_ref().and_then(|_| {
                    job.session_id
                        .as_deref()
                        .and_then(|id| bounded_text_str(id, 160))
                }),
                project_id,
            })
        })
        .collect();
    Ok((jobs, truncated))
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
        .query_job_inventory_for_auth(Some(100), Some("running"), project, None, None, Some(auth))
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
        .query_job_inventory_for_auth(
            Some(100),
            None,
            Some(project),
            Some(session_id),
            None,
            Some(auth),
        )
        .await
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok((
        page.jobs.iter().filter_map(session_job).collect(),
        page.truncated(),
    ))
}

#[cfg(test)]
mod tests;
