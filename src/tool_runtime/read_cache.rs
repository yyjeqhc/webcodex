//! Authorized read-only singleflight and SHA-validated Session range snapshots.
//!
//! A revision is not a filesystem generation. Even a fenced cache hit must ask
//! the owning Runner for fresh full-file SHA evidence; metadata/TTL cannot prove
//! that an external writer has not changed the file. This saves range transfer,
//! not the Runner's full-file scan. No Session means no retained content cache.

use super::project_resolution::ResolvedProject;
use super::read_revisions::ReadRevisionTarget;
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use futures_util::future::{BoxFuture, Shared, WeakShared};
use futures_util::FutureExt;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_range::EffectiveRange;

const MAX_SNAPSHOTS: usize = 64;
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_FLIGHTS: usize = 128;
const PHYSICAL_READ_GRACE: Duration = Duration::from_secs(2);

tokio::task_local! {
    // Installed only after the canonical dispatcher has authorized the call.
    // Direct internal callers without a scope retain the uncached read path.
    pub(crate) static READ_SCOPE: ReadScope;
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReadScope {
    authority: String,
    session: Option<String>,
}

impl ReadScope {
    pub(crate) fn new(auth: Option<&AuthContext>, session: Option<&str>) -> Self {
        // Include the full authority, not just principal identity. This digest
        // is private process-local partitioning, never an authorization grant.
        let authority = auth.map(|auth| {
            json!([
                format!("{:?}", auth.kind),
                auth.user_id,
                auth.username,
                auth.api_key_id,
                auth.role,
                auth.scopes,
                auth.is_bootstrap,
                auth.token_kind,
                auth.allowed_client_id,
                auth.shared_key_hash,
                auth.project_grant_id
            ])
        });
        Self {
            authority: super::files::sha256_hex_bytes(
                &serde_json::to_vec(&authority).expect("authority JSON"),
            ),
            session: session.map(str::to_owned),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct SnapshotKey {
    scope: ReadScope,
    target: ReadRevisionTarget,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ReadKey {
    snapshot: SnapshotKey,
    start: usize,
    limit: usize,
    expected_sha256: Option<String>,
}

type ReadFlight = Shared<BoxFuture<'static, Arc<ToolResult>>>;

struct ReadFlightEntry {
    physical_deadline: Instant,
    flight: WeakShared<BoxFuture<'static, Arc<ToolResult>>>,
}

struct Snapshot {
    key: SnapshotKey,
    output: Value,
    bytes: usize,
}

#[derive(Default)]
struct State {
    snapshots: VecDeque<Snapshot>,
    bytes: usize,
    flights: HashMap<ReadKey, ReadFlightEntry>,
}

#[derive(Default)]
pub(crate) struct ReadCache {
    state: Mutex<State>,
}

impl ReadCache {
    fn snapshot(&self, key: &ReadKey) -> Option<Value> {
        if key.snapshot.scope.session.is_none() {
            return None;
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshots.iter().rev().find_map(|entry| {
            (entry.key == key.snapshot)
                .then(|| {
                    super::files::slice_read_file_success_output(
                        &entry.output,
                        Some(key.start),
                        Some(key.limit),
                        false,
                        &key.snapshot.target.path,
                    )
                })
                .flatten()
        })
    }

    fn invalidate(&self, key: &SnapshotKey) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshots.retain(|entry| &entry.key != key);
        state.bytes = state.snapshots.iter().map(|entry| entry.bytes).sum();
    }

    fn remember(&self, key: SnapshotKey, output: Value) {
        if key.scope.session.is_none() {
            return;
        }
        let Some(sha) = output.get("sha256").and_then(Value::as_str) else {
            return;
        };
        let bytes = crate::json_measurement::serialized_json_len(&output)
            .unwrap_or(usize::MAX)
            .saturating_add(key.scope.authority.len())
            .saturating_add(key.scope.session.as_ref().map_or(0, String::len))
            .saturating_add(key.target.project_id.len())
            .saturating_add(key.target.path.len())
            .saturating_add(key.target.client_id.len())
            .saturating_add(key.target.runner_instance_id.len())
            .saturating_add(key.target.project_root.len())
            .saturating_add(key.target.root_fingerprint.as_ref().map_or(0, String::len));
        if bytes > MAX_SNAPSHOT_BYTES {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Keep other ranges only for the same observed full-file snapshot.
        state.snapshots.retain(|entry| {
            entry.key != key
                || (entry.output["sha256"].as_str() == Some(sha)
                    && (entry.output["start_line"] != output["start_line"]
                        || entry.output["limit"] != output["limit"]))
        });
        state.bytes = state.snapshots.iter().map(|entry| entry.bytes).sum();
        while state.snapshots.len() >= MAX_SNAPSHOTS || state.bytes + bytes > MAX_SNAPSHOT_BYTES {
            if let Some(evicted) = state.snapshots.pop_front() {
                state.bytes -= evicted.bytes;
            } else {
                break;
            }
        }
        state.bytes += bytes;
        state.snapshots.push_back(Snapshot { key, output, bytes });
    }

    fn flight(
        &self,
        key: ReadKey,
        caller_deadline: Instant,
        physical_deadline: Instant,
        work: BoxFuture<'static, ToolResult>,
    ) -> ReadFlight {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Weak ownership: the last departing waiter drops the physical future.
        // Completed successes AND errors must never become unfenced cache hits.
        state.flights.retain(|_, entry| {
            entry
                .flight
                .upgrade()
                .is_some_and(|flight| flight.peek().is_none())
        });
        if let Some(existing) = state
            .flights
            .get(&key)
            // Sharing is only an optimization: a late waiter may join an older
            // flight only when that flight is guaranteed to live through the
            // waiter's own caller deadline.
            .filter(|entry| caller_deadline <= entry.physical_deadline)
            .and_then(|entry| entry.flight.upgrade())
            .filter(|flight| flight.peek().is_none())
        {
            return existing;
        }
        let flight = work.map(Arc::new).boxed().shared();
        // Replacing an older same-key flight does not increase the bounded index.
        // The displaced flight remains alive only through its existing waiters.
        if state.flights.contains_key(&key) || state.flights.len() < MAX_FLIGHTS {
            state.flights.insert(
                key,
                ReadFlightEntry {
                    physical_deadline,
                    flight: flight.downgrade().expect("new flight"),
                },
            );
        }
        flight
    }
}

impl ToolRuntime {
    pub(crate) async fn read_project_snapshot(
        &self,
        resolved: &ResolvedProject,
        runner_project_id: &str,
        runner_instance_id: &str,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        expected_sha256: Option<&str>,
        deadline: Instant,
    ) -> ToolResult {
        let Ok(scope) = READ_SCOPE.try_with(Clone::clone) else {
            return self
                .read_one_resolved_project_file(
                    &resolved.config,
                    runner_project_id,
                    runner_instance_id,
                    path,
                    start_line,
                    limit,
                    false,
                    deadline,
                )
                .await;
        };
        if Instant::now() >= deadline {
            return timeout(&path);
        }
        let range = EffectiveRange::new(start_line, limit);
        let key = ReadKey {
            snapshot: SnapshotKey {
                scope,
                target: ReadRevisionTarget {
                    project_id: resolved.resolved_id.clone(),
                    path: path.clone(),
                    client_id: resolved.config.client_id.clone(),
                    runner_instance_id: runner_instance_id.to_owned(),
                    project_root: resolved.config.path.clone(),
                    root_fingerprint: resolved.root_fingerprint.clone(),
                },
            },
            start: range.start_line,
            limit: range.limit,
            expected_sha256: expected_sha256.map(str::to_owned),
        };
        // Keep physical work bounded while allowing the starter caller to time
        // out without immediately tearing down work still safe for another waiter.
        let physical_deadline = deadline + PHYSICAL_READ_GRACE;
        let runtime = self.clone();
        let project = resolved.config.clone();
        let runner_project_id = runner_project_id.to_owned();
        let work_key = key.clone();
        let work = async move {
            let key = work_key;
            let target = &key.snapshot.target;
            let read = |start, limit| {
                runtime.read_one_resolved_project_file(
                    &project,
                    &runner_project_id,
                    &target.runner_instance_id,
                    target.path.clone(),
                    Some(start),
                    Some(limit),
                    false,
                    physical_deadline,
                )
            };
            if let Some(cached) = runtime.read_cache.snapshot(&key) {
                // Pick a line inside the requested range, so the probe cannot
                // fail just because an unrelated first line is oversized.
                let probe = read(key.start, 1).await;
                if probe.success
                    && key
                        .expected_sha256
                        .as_deref()
                        .is_some_and(|expected| probe.output["sha256"].as_str() != Some(expected))
                {
                    runtime.read_cache.invalidate(&key.snapshot);
                    return super::read_files::stale_read_revision_failure(&target.path);
                }
                if probe.success && probe.output["sha256"] == cached["sha256"] {
                    return ToolResult::ok(cached);
                }
                runtime.read_cache.invalidate(&key.snapshot);
                if !probe.success && probe.output["reason_code"] != "range_too_large" {
                    return probe;
                }
            }
            let result = read(key.start, key.limit).await;
            if result.success {
                runtime
                    .read_cache
                    .remember(key.snapshot, result.output.clone());
            } else {
                runtime.read_cache.invalidate(&key.snapshot);
            }
            result
        }
        .boxed();
        let flight = self
            .read_cache
            .flight(key, deadline, physical_deadline, work);
        match tokio::time::timeout_at(deadline, flight).await {
            Ok(result) => ToolResult {
                success: result.success,
                output: result.output.clone(),
                error: result.error.clone(),
            },
            Err(_) => timeout(&path),
        }
    }
}

fn timeout(path: &str) -> ToolResult {
    ToolResult::err_with_output(
        "read_file failed: timeout",
        json!({
            "error_kind": "read_file_failed", "reason_code": "timeout",
            "path": path, "state_changed": false,
        }),
    )
}

#[cfg(test)]
#[path = "tests/read_cache.rs"]
mod tests;
