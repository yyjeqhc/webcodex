//! Navigation-only overview. Historical jobs, Workflow Sessions and complete
//! Project rows belong to explicit page inventories/full progressive hydration.
use super::{
    active_window_count_for_auth, empty_console_aggregate, finalize_recent_sessions,
    project_read_available, require_runtime_read, safe_string, AuthContext, RuntimeConsoleError,
    RuntimeConsoleOverview, RuntimeConsoleRunnerSummary, RuntimeConsoleWorkflowAggregate,
    ToolRuntime,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) fn family_count(rows: &[super::RuntimeConsoleProject]) -> usize {
    rows.iter()
        .map(|project| match &project.lineage {
            Some(super::RuntimeConsoleProjectLineage::ManagedWorktreeSource {
                source_project_id,
                ..
            }) => format!("agent:{}:{source_project_id}", project.client_id),
            None => project.id.clone(),
        })
        .collect::<HashSet<_>>()
        .len()
}

pub(super) async fn primary_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<RuntimeConsoleOverview, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    let access = crate::runner_http::runner_access_from_auth(Some(auth));
    let project_access = project_read_available(auth);
    let snapshot = runtime
        .runner_registry
        .console_registry_snapshot_for_auth(access.as_ref(), project_access)
        .await;
    let jobs = runtime
        .runner_registry
        .active_job_summary_by_runner_for_auth(access.as_ref())
        .await;
    let compatibility = runtime.console_runner_alignment(&snapshot.runners);
    let alignment_by_runner: HashMap<_, _> = compatibility
        .get("runners")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value
                .get("client_id")
                .and_then(Value::as_str)
                .map(|id| (id, value))
        })
        .collect();
    let build = crate::build_info::runtime_build_info();
    let now = chrono::Utc::now().timestamp();
    let mut rows = Vec::with_capacity(snapshot.runners.len());
    let visible_projects = snapshot.projects_by_runner.values().sum();
    for client in &snapshot.runners {
        let aligned = alignment_by_runner
            .get(client.client_id.as_str())
            .copied()
            .unwrap_or(&Value::Null);
        let active = jobs.get(&client.client_id);
        let project_count = snapshot
            .projects_by_runner
            .get(&client.client_id)
            .copied()
            .unwrap_or(0);
        let mut sessions = empty_console_aggregate();
        sessions.sessions_truncated = project_count > 0;
        rows.push(RuntimeConsoleRunnerSummary {
            computer_session_availability: client.computer_session_availability,
            client_id: client.client_id.clone(),
            connected: client.connected,
            status: Some(client.status.clone()),
            transport: Some(client.transport.clone()),
            runner_protocol_generation: Some(u64::from(client.runner_protocol_generation.get())),
            last_seen_age_secs: Some(now.saturating_sub(client.last_seen)),
            version: client.build.as_ref().and_then(|b| b.version.clone()),
            build_git_commit: client.build.as_ref().and_then(|b| b.git_commit.clone()),
            build_git_dirty: client.build.as_ref().and_then(|b| b.git_dirty),
            source_alignment: safe_string(aligned.pointer("/source_alignment/status"), 40),
            protocol_compatibility: safe_string(aligned.get("protocol_compatibility"), 40)
                .unwrap_or_else(|| "unknown".into()),
            build_alignment: safe_string(aligned.get("build_alignment"), 40),
            version_matches_server: aligned
                .get("version_matches_server")
                .and_then(Value::as_bool),
            active_jobs: active.map_or(0, |j| j.active),
            job_concurrency_limit: client.job_concurrency_limit.map(|limit| limit as u64),
            jobs_running: active.map_or(0, |j| j.running),
            jobs_queued: active.map_or(0, |j| j.queued),
            projects_scanned: 0,
            projects_scan_partial: project_count > 0,
            sessions,
        });
    }
    let online = rows.iter().filter(|row| row.connected).count();
    let stale = rows
        .iter()
        .filter(|row| row.status.as_deref() == Some("stale"))
        .count();
    let source_mismatched = rows
        .iter()
        .filter(|row| row.source_alignment.as_deref() == Some("different"))
        .count();
    let mixed = rows.iter().any(|row| {
        row.version_matches_server == Some(false)
            || row.source_alignment.as_deref() == Some("different")
    });
    Ok(RuntimeConsoleOverview {
        detail_level: "primary",
        projects_included: false,
        visible_project_families: snapshot.project_families,
        authenticated_user: auth.username.clone(),
        effective_config: runtime.effective_config_status(),
        service: Some("webcodex".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        build_git_commit: build.git_commit.map(str::to_string),
        build_git_dirty: build.git_dirty,
        runner_count: rows.len(),
        runners_online: online,
        runners_stale: stale,
        runners_unavailable: 0,
        source_mismatched_runners: source_mismatched,
        mixed_builds_present: mixed,
        active_jobs: jobs.values().map(|j| j.active).sum(),
        active_windows: active_window_count_for_auth(runtime, auth).await?,
        projects_available: project_access,
        visible_projects,
        projects_truncated: false,
        workflow_sessions: RuntimeConsoleWorkflowAggregate {
            projects_total: visible_projects,
            truncated: visible_projects > 0,
            ..Default::default()
        },
        recent_sessions: finalize_recent_sessions(Vec::new(), visible_projects > 0),
        runners: rows,
        projects: Vec::new(),
    })
}
