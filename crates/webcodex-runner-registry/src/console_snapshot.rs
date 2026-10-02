//! Ephemeral console aggregates. No Project bodies, history, log buffers, or
//! independent visibility cache. All identities come from one authorized snapshot.
use crate::state::{RunnerRecord, RunnerRegistryInner};
use crate::{RunnerAccess, RunnerRegistry};
use std::collections::{HashMap, HashSet};
use webcodex_core::runner_protocol::{RunnerProjectLineage, RunnerView};

#[derive(Debug, Default)]
pub struct ActiveJobAggregate {
    pub active: usize,
    pub running: usize,
    pub queued: usize,
}

pub struct ConsoleRegistrySnapshot {
    /// Existing canonical Runner projection, with its Project body deliberately omitted.
    pub runners: Vec<RunnerView>,
    pub projects_by_runner: HashMap<String, usize>,
    pub project_families: usize,
}

impl RunnerRegistry {
    /// Batched form of exact_project_visible_for_auth_snapshot. Sharing one
    /// authorized Runner snapshot avoids O(anchors × registered Projects).
    pub async fn visible_project_ids_for_auth_snapshot(
        &self,
        auth: Option<&RunnerAccess>,
        requested: &[String],
    ) -> HashSet<String> {
        let requested: HashSet<&str> = requested.iter().map(String::as_str).collect();
        if requested.is_empty() {
            return HashSet::new();
        }
        let now = crate::now_ts();
        let inner = self.inner.lock().await;
        let mut visible = HashSet::new();
        for runner in inner.runners.values() {
            if !self.runner_visible_for_snapshot(auth, &inner, runner, now) {
                continue;
            }
            for project in &runner.projects {
                let id = format!("agent:{}:{}", runner.client_id, project.id);
                if requested.contains(id.as_str()) {
                    visible.insert(id);
                }
            }
        }
        visible
    }

    /// Same read-only TTL/authority fence used by exact Project observations.
    /// This does not prune expired records (which could scan historical Jobs).
    pub(crate) fn runner_visible_for_snapshot(
        &self,
        auth: Option<&RunnerAccess>,
        inner: &RunnerRegistryInner,
        runner: &RunnerRecord,
        now: i64,
    ) -> bool {
        if !crate::access_control::runner_visible_to_access(auth, runner) {
            return false;
        }
        if matches!(
            runner.auth_group,
            Some(crate::RunnerAccessGroup::SharedKey(_))
        ) {
            let connected = inner.notifiers.contains_key(&runner.client_id);
            let recent = now.saturating_sub(runner.last_seen) <= crate::RUNNER_ONLINE_WINDOW_SECS;
            let offline = runner.disconnected_at.unwrap_or(runner.last_seen);
            if !connected
                && !recent
                && now.saturating_sub(offline) > self.shared_key_limits.offline_ttl_secs
            {
                return false;
            }
        }
        true
    }

    /// O(R + P); no full Project serialization/capability-per-Project or Job scan.
    /// Families use explicit lineage only, never filesystem/path inference.
    pub async fn console_registry_snapshot_for_auth(
        &self,
        auth: Option<&RunnerAccess>,
        include_project_counts: bool,
    ) -> ConsoleRegistrySnapshot {
        let inner = self.inner.lock().await;
        let now = crate::now_ts();
        let mut runners = Vec::new();
        let mut projects_by_runner = HashMap::new();
        let mut project_families = 0;
        for runner in inner.runners.values() {
            if !self.runner_visible_for_snapshot(auth, &inner, runner, now) {
                continue;
            }
            if include_project_counts {
                let mut count = 0;
                let mut families = HashSet::new();
                for project in runner.projects.iter().filter(|project| !project.disabled) {
                    count += 1;
                    let family = match &project.lineage {
                        Some(RunnerProjectLineage::ManagedWorktreeSource {
                            source_project_id,
                            ..
                        }) => source_project_id,
                        None => &project.id,
                    };
                    families.insert(family);
                }
                project_families += families.len();
                projects_by_runner.insert(runner.client_id.clone(), count);
            }
            if let Some(view) =
                Self::runner_view_with_projects_locked(&inner, &runner.client_id, false)
            {
                runners.push(view);
            }
        }
        runners.sort_by(|a, b| a.client_id.cmp(&b.client_id));
        ConsoleRegistrySnapshot {
            runners,
            projects_by_runner,
            project_families,
        }
    }
}
