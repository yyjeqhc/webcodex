//! Bounded reuse of presentation-only Git observations. Every caller reauthorizes
//! first. Canonical mutation fences are NOT filesystem revisions: external writes
//! require a new observation, so a snapshot is reused for at most 30 seconds.
//! Explicit refresh, presentation and closeout never read this cache.
use super::validation_source::PresentationSourceFence;
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use webcodex_runner_registry::WorkspaceObservationIdentity;

pub(crate) const REUSE_LEASE: Duration = Duration::from_secs(30);
const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 256 * 1024;
type Key = (String, String, String);

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    target: WorkspaceObservationIdentity,
    source: PresentationSourceFence,
}
struct Entry {
    stamp: Stamp,
    observed: Instant,
    output: serde_json::Value,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    bytes: usize,
}
#[derive(Default)]
pub(crate) struct WorkResultWorkspaceCache(Mutex<Cache>);

impl WorkResultWorkspaceCache {
    fn get(&self, key: &Key, stamp: &Stamp) -> Option<ToolResult> {
        let cache = self.0.lock().ok()?;
        let entry = cache.entries.get(key)?;
        (entry.stamp == *stamp && entry.observed.elapsed() < REUSE_LEASE)
            .then(|| ToolResult::ok(entry.output.clone()))
    }
    fn remove(&self, key: &Key) {
        if let Ok(mut cache) = self.0.lock() {
            if let Some(old) = cache.entries.remove(key) {
                cache.bytes -= old.bytes;
            }
        }
    }
    fn put(&self, key: Key, stamp: Stamp, observed: Instant, result: &ToolResult) {
        let Ok(bytes) = crate::json_measurement::serialized_json_len(result) else {
            return;
        };
        if !result.success || bytes > MAX_ENTRY_BYTES || observed.elapsed() >= REUSE_LEASE {
            return;
        }
        let Ok(mut cache) = self.0.lock() else {
            return;
        };
        // A slower older probe must not replace a newer observation.
        if cache
            .entries
            .get(&key)
            .is_some_and(|entry| entry.observed > observed)
        {
            return;
        }
        if let Some(old) = cache.entries.remove(&key) {
            cache.bytes -= old.bytes;
        }
        while cache.entries.len() >= MAX_ENTRIES || cache.bytes + bytes > MAX_BYTES {
            let Some(oldest) = cache
                .entries
                .iter()
                .min_by_key(|(_, e)| e.observed)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes -= old.bytes;
            }
        }
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                stamp,
                observed,
                output: result.output.clone(),
                bytes,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn expire_for_test(&self) {
        for entry in self.0.lock().unwrap().entries.values_mut() {
            entry.observed = Instant::now() - REUSE_LEASE;
        }
    }
}

impl ToolRuntime {
    async fn work_result_observation_stamp(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Option<Stamp> {
        let mut source = self.validation_sources.capture_presentation(project)?;
        if !source.pending_jobs.is_empty() {
            if !self.runner_registry.observation_jobs_ended_for_project(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                project,
                &source.pending_jobs,
            ) || !self
                .validation_sources
                .resolve_presentation_jobs(project, &source)
            {
                tracing::debug!(target: "webcodex::phase", phase="workspace_reuse", outcome="pending_or_unknown_job", "workspace observation cache miss");
                return None;
            }
            source = self.validation_sources.capture_presentation(project)?;
            if !source.pending_jobs.is_empty() {
                return None;
            }
        }
        let target = self
            .runner_registry
            .workspace_observation_identity_for_auth(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                project,
            )
            .await?;
        Some(Stamp { target, source })
    }

    pub(crate) async fn work_result_workspace_observation(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
        automatic: bool,
    ) -> (ToolResult, bool) {
        let key = super::runtime_observation_principal(auth)
            .ok()
            .map(|(kind, id)| (kind, id, project.to_string()));
        let before = self.work_result_observation_stamp(project, auth).await;
        if automatic {
            if let (Some(key), Some(stamp)) = (&key, &before) {
                if let Some(result) = self.work_result_workspace_cache.get(key, stamp) {
                    tracing::debug!(target: "webcodex::phase", phase="workspace_reuse", outcome="hit", "reused presentation snapshot");
                    return (result, true);
                }
            }
        }
        // Remove even on probe failure/cancellation so an older success cannot
        // be revived after an explicit refresh or a crossed source/target fence.
        if let Some(key) = &key {
            self.work_result_workspace_cache.remove(key);
        }
        let observed = Instant::now();
        let result = self
            .workspace_metadata_for_presentation(project.to_string())
            .await;
        let after = self.work_result_observation_stamp(project, auth).await;
        crate::tool_request_trace::record_phase_latency(
            "workspace_observation",
            observed,
            if !result.success {
                "failed"
            } else if before.is_none() || after.is_none() {
                "uncacheable"
            } else if before != after {
                "crossed_fence"
            } else {
                "fresh"
            },
        );

        if let (Some(key), Some(before), Some(after)) = (key, before, after) {
            if before == after {
                self.work_result_workspace_cache
                    .put(key, after, observed, &result);
            }
        }
        (result, false)
    }
}
