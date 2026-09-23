//! Passive, non-authoritative Job state attention on ordinary coding results.
use super::{ToolResult, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_RUNTIME_READ};
use crate::client_window::ClientWindow;
use crate::runner_protocol::ShellJobInfo;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

const MAX_CURSOR_KEYS: usize = 128;
const MAX_JOBS_PER_KEY: usize = 32;
const MAX_ITEMS: usize = 8;

#[derive(Clone, Hash, PartialEq, Eq)]
struct AttentionKey {
    principal_kind: String,
    principal_id: String,
    window_key: String,
    project: String,
    session_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct JobState {
    status: String,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    reconciled_at: Option<i64>,
}

impl From<&ShellJobInfo> for JobState {
    fn from(job: &ShellJobInfo) -> Self {
        Self {
            status: job.status.clone(),
            started_at: job.started_at,
            ended_at: job.ended_at,
            exit_code: job.exit_code,
            reconciled_at: job.reconciled_at,
        }
    }
}

#[derive(Default)]
struct CursorEntry {
    last_used: u64,
    states: BTreeMap<String, JobState>,
}

#[derive(Default)]
struct CursorInner {
    tick: u64,
    entries: HashMap<AttentionKey, CursorEntry>,
}

#[derive(Default)]
pub(crate) struct JobAttentionCursor(Mutex<CursorInner>);

impl JobAttentionCursor {
    fn project_result(&self, result: &mut ToolResult, key: AttentionKey, jobs: &[ShellJobInfo]) {
        let Ok(mut inner) = self.0.lock() else {
            return;
        };
        let prior = inner.entries.get(&key);
        let items: Vec<Value> = jobs
            .iter()
            .filter(|job| {
                let previous = prior.and_then(|entry| entry.states.get(&job.job_id));
                match previous {
                    Some(previous) => previous != &JobState::from(*job),
                    None => webcodex_runner_registry::job_status_is_active(&job.status),
                }
            })
            .take(MAX_ITEMS)
            .map(|job| {
                json!({
                    "job_id": job.job_id,
                    "kind": job.kind,
                    "status": job.status,
                    "created_at": job.created_at,
                    "started_at": job.started_at,
                    "ended_at": job.ended_at,
                    "exit_code": job.exit_code,
                    "reconciled_at": job.reconciled_at,
                })
            })
            .collect();
        let emitted: HashSet<String> = items
            .iter()
            .filter_map(|item| item["job_id"].as_str().map(str::to_string))
            .collect();
        if !items.is_empty() {
            let Some(output) = result.output.as_object_mut() else {
                return;
            };
            if output.contains_key("job_attention") {
                return;
            }
            output.insert(
                "job_attention".to_string(),
                json!({"changed": true, "items": items}),
            );
            if !crate::json_measurement::serialized_json_len(result).is_ok_and(|size| {
                size <= webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
            }) {
                result
                    .output
                    .as_object_mut()
                    .unwrap()
                    .remove("job_attention");
                return;
            }
        }
        inner.tick = inner.tick.wrapping_add(1);
        let tick = inner.tick;
        let initial = !inner.entries.contains_key(&key);
        let entry = inner.entries.entry(key).or_default();
        entry.last_used = tick;
        let in_snapshot: HashSet<&str> = jobs.iter().map(|job| job.job_id.as_str()).collect();
        entry
            .states
            .retain(|id, _| in_snapshot.contains(id.as_str()));
        for job in jobs.iter().take(MAX_JOBS_PER_KEY) {
            // Old terminal Jobs establish a baseline on a fresh Window. When
            // more than eight states change, leave unprojected revisions ready
            // for the next ordinary result instead of silently consuming them.
            if emitted.contains(&job.job_id)
                || (initial && !webcodex_runner_registry::job_status_is_active(&job.status))
                || !entry.states.contains_key(&job.job_id)
                    && !webcodex_runner_registry::job_status_is_active(&job.status)
            {
                entry.states.insert(job.job_id.clone(), JobState::from(job));
            }
        }
        if inner.entries.len() > MAX_CURSOR_KEYS {
            if let Some(oldest) = inner
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            {
                inner.entries.remove(&oldest);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.0.lock().unwrap();
            panic!("simulate unavailable passive attention cursor");
        }));
    }
}

impl ToolRuntime {
    /// Add a small post-result sidecar only after the exact business relation is
    /// proven by canonical dispatch. Absence or failure is silent and never
    /// changes the main ToolResult or advances the cursor.
    pub(crate) async fn add_passive_job_attention(
        &self,
        result: &mut ToolResult,
        tool_name: &str,
        project: Option<&str>,
        business_session_id: Option<&str>,
        window: Option<&ClientWindow>,
        auth: Option<&AuthContext>,
    ) {
        if tool_name == "current_window_activity" || !result.success {
            return;
        }
        let (Some(project), Some(session_id), Some(window), Some(auth)) =
            (project, business_session_id, window, auth)
        else {
            return;
        };
        if auth.is_open_anonymous()
            || !auth.has_scope(SCOPE_RUNTIME_READ)
            || !self.exact_project_visible_to_auth(auth, project).await
            || self
                .sessions
                .session_project(session_id)
                .flatten()
                .as_deref()
                != Some(project)
        {
            return;
        }
        let Ok((principal_kind, principal_id)) =
            super::session_context::runtime_observation_principal(Some(auth))
        else {
            return;
        };
        let key = AttentionKey {
            principal_kind,
            principal_id,
            window_key: window.key().to_string(),
            project: project.to_string(),
            session_id: session_id.to_string(),
        };
        let jobs = self
            .runner_registry
            .snapshot_jobs_for_auth_filtered(
                crate::runner_http::runner_access_from_auth(Some(auth)).as_ref(),
                project,
                session_id,
                MAX_JOBS_PER_KEY,
            )
            .await;
        self.job_attention_cursor.project_result(result, key, &jobs);
    }
}
