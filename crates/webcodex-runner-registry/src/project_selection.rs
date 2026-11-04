//! Bounded read-only Project selector facts. The caller owns search semantics
//! and its wire allowlist; no Runner RPC, Job reconciliation or Project clone.
use crate::state::RunnerRecord;
use crate::{RunnerAccess, RunnerRegistry};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use webcodex_core::runner_protocol::RunnerProjectSummary;

pub const PROJECT_SELECTION_MAX_RESULTS: usize = 2_000;

pub struct ProjectSelection {
    pub projects: Vec<ProjectSelectionRow>,
    pub matched_count: usize,
}

pub struct ProjectSelectionRow {
    pub id: String,
    pub client_id: String,
    pub name: Option<String>,
    pub path: String,
    pub root_fingerprint: Option<String>,
    pub connected: bool,
    pub runner_status: String,
}

struct Candidate<'a> {
    id: String,
    runner: &'a RunnerRecord,
    project: &'a RunnerProjectSummary,
}
impl PartialEq for Candidate<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Candidate<'_> {}
impl PartialOrd for Candidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl RunnerRegistry {
    /// Count all authorized matches but retain only the first ID-sorted page.
    /// A maximum of `limit` borrowed candidates is held; only that page's
    /// selector fields are cloned. The canonical snapshot TTL/authority fence
    /// excludes expired shared-key Runners without pruning state or Jobs.
    pub async fn select_projects_for_auth(
        &self,
        auth: Option<&RunnerAccess>,
        client_id: Option<&str>,
        project_id: Option<&str>,
        limit: usize,
        matches: impl Fn(&str, &RunnerProjectSummary) -> bool,
    ) -> ProjectSelection {
        let limit = limit.min(PROJECT_SELECTION_MAX_RESULTS);
        let inner = self.inner.read().await;
        let now = crate::now_ts();
        let mut selected = BinaryHeap::<Candidate<'_>>::with_capacity(limit);
        let mut matched_count = 0;
        for runner in inner.runners.values() {
            if client_id.is_some_and(|expected| runner.client_id != expected)
                || !self.runner_visible_for_snapshot(auth, &inner, runner, now)
            {
                continue;
            }
            for project in &runner.projects {
                let id = format!("agent:{}:{}", runner.client_id, project.id);
                if project_id.is_some_and(|expected| id != expected) || !matches(&id, project) {
                    continue;
                }
                matched_count += 1;
                if limit == 0
                    || (selected.len() == limit
                        && selected.peek().is_some_and(|last| id >= last.id))
                {
                    continue;
                }
                if selected.len() == limit {
                    selected.pop();
                }
                selected.push(Candidate {
                    id,
                    runner,
                    project,
                });
            }
        }
        let projects = selected
            .into_sorted_vec()
            .into_iter()
            .map(|candidate| {
                let runner = candidate.runner;
                let project = candidate.project;
                // Match canonical RunnerView liveness for both polling and
                // streaming Runners; only streaming transports own a notifier.
                let connected =
                    now.saturating_sub(runner.last_seen) <= crate::RUNNER_ONLINE_WINDOW_SECS;
                ProjectSelectionRow {
                    id: candidate.id,
                    client_id: runner.client_id.clone(),
                    name: project.name.clone(),
                    path: project.path.clone(),
                    root_fingerprint: project.root_fingerprint.clone(),
                    connected,
                    runner_status: if connected { "online" } else { "stale" }.into(),
                }
            })
            .collect();
        ProjectSelection {
            projects,
            matched_count,
        }
    }
}
