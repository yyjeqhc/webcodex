//! Authorized read-only singleflight and SHA-validated Session range snapshots.
//!
//! A revision is not a filesystem generation. Even a fenced cache hit must ask
//! the owning Runner for fresh full-file SHA evidence; metadata/TTL cannot prove
//! that an external writer has not changed the file. This saves range transfer,
//! not the Runner's full-file scan. No Session means no retained content cache.

use super::files::{slice_read_file_range, FileReadFailure, FileReadOutcome, ProjectFileReader};
use super::project_resolution::ResolvedProject;
use super::read_revisions::ReadRevisionTarget;
use crate::auth::AuthContext;
use futures_util::future::{BoxFuture, Shared, WeakShared};
use futures_util::FutureExt;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_range::{EffectiveRange, FileReadRange, ReadFileReason};

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

type ReadFlight = Shared<BoxFuture<'static, Arc<FileReadOutcome>>>;

struct ReadFlightEntry {
    physical_deadline: Instant,
    flight: WeakShared<BoxFuture<'static, Arc<FileReadOutcome>>>,
}

struct Snapshot {
    key: SnapshotKey,
    range: FileReadRange,
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

/// Charge the retained typed allocation, plus conservative JSON-escaping cost
/// and fixed projection metadata. This preserves the old serialized-payload
/// bound without constructing or retaining a model-facing output object.
fn snapshot_bytes(key: &SnapshotKey, range: &FileReadRange) -> usize {
    let escaped_content_bytes =
        crate::json_measurement::serialized_json_len(&range.content).unwrap_or(usize::MAX);
    std::mem::size_of::<Snapshot>()
        .saturating_add(512) // Canonical field names, numeric metadata and JSON delimiters.
        .saturating_add(range.content.capacity().max(escaped_content_bytes))
        .saturating_add(range.sha256.capacity())
        .saturating_add(key.scope.authority.capacity())
        .saturating_add(key.scope.session.as_ref().map_or(0, String::capacity))
        .saturating_add(key.target.project_id.capacity())
        .saturating_add(key.target.path.capacity().saturating_mul(7))
        .saturating_add(key.target.client_id.capacity())
        .saturating_add(key.target.runner_instance_id.capacity())
        .saturating_add(key.target.project_root.capacity())
        .saturating_add(
            key.target
                .root_fingerprint
                .as_ref()
                .map_or(0, String::capacity),
        )
}

fn enforce_expected_sha(result: FileReadOutcome, expected_sha256: Option<&str>) -> FileReadOutcome {
    result.and_then(|range| {
        if expected_sha256.is_some_and(|expected| range.sha256 != expected) {
            Err(FileReadFailure::StaleRevision)
        } else {
            Ok(range)
        }
    })
}

impl ReadCache {
    fn snapshot(&self, key: &ReadKey) -> Option<FileReadRange> {
        if key.snapshot.scope.session.is_none() {
            return None;
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshots.iter().rev().find_map(|entry| {
            (entry.key == key.snapshot)
                .then(|| {
                    slice_read_file_range(
                        &entry.range,
                        Some(key.start),
                        Some(key.limit),
                        false,
                        &key.snapshot.target.path,
                    )
                    .ok()
                })
                .flatten()
        })
    }

    fn invalidate(&self, key: &SnapshotKey) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshots.retain(|entry| &entry.key != key);
        state.bytes = state.snapshots.iter().map(|entry| entry.bytes).sum();
    }

    fn remember(&self, key: SnapshotKey, range: FileReadRange) {
        if key.scope.session.is_none() {
            return;
        }
        let bytes = snapshot_bytes(&key, &range);
        if bytes > MAX_SNAPSHOT_BYTES {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Keep other ranges only for the same observed full-file snapshot.
        state.snapshots.retain(|entry| {
            entry.key != key
                || (entry.range.sha256 == range.sha256
                    && (entry.range.start_line != range.start_line
                        || entry.range.limit != range.limit))
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
        state.snapshots.push_back(Snapshot { key, range, bytes });
    }

    fn flight(
        &self,
        key: ReadKey,
        caller_deadline: Instant,
        physical_deadline: Instant,
        work: BoxFuture<'static, FileReadOutcome>,
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

impl ReadCache {
    pub(crate) async fn read_project_snapshot(
        self: &Arc<Self>,
        reader: &ProjectFileReader,
        resolved: &ResolvedProject,
        runner_project_id: &str,
        runner_instance_id: &str,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        expected_sha256: Option<&str>,
        deadline: Instant,
    ) -> FileReadOutcome {
        let Ok(scope) = READ_SCOPE.try_with(Clone::clone) else {
            let result = reader
                .read_one_resolved_project_file(
                    &resolved.config,
                    runner_project_id,
                    runner_instance_id,
                    path,
                    start_line,
                    limit,
                    deadline,
                )
                .await;
            return enforce_expected_sha(result, expected_sha256);
        };
        if Instant::now() >= deadline {
            return Err(ReadFileReason::Timeout.into());
        }
        let range = EffectiveRange::new(start_line, limit);
        let key = ReadKey {
            snapshot: SnapshotKey {
                scope,
                target: ReadRevisionTarget::from_resolved(resolved, &path, runner_instance_id),
            },
            start: range.start_line,
            limit: range.limit,
            expected_sha256: expected_sha256.map(str::to_owned),
        };
        // Keep physical work bounded while allowing the starter caller to time
        // out without immediately tearing down work still safe for another waiter.
        let physical_deadline = deadline + PHYSICAL_READ_GRACE;
        let cache = self.clone();
        let reader = reader.clone();
        let project = resolved.config.clone();
        let runner_project_id = runner_project_id.to_owned();
        let work_key = key.clone();
        let work = async move {
            let key = work_key;
            let target = &key.snapshot.target;
            let read = |start, limit| {
                reader.read_one_resolved_project_file(
                    &project,
                    &runner_project_id,
                    &target.runner_instance_id,
                    target.path.clone(),
                    Some(start),
                    Some(limit),
                    physical_deadline,
                )
            };
            if let Some(cached) = cache.snapshot(&key) {
                // Pick a line inside the requested range, so the probe cannot
                // fail just because an unrelated first line is oversized.
                let probe = read(key.start, 1).await;
                match enforce_expected_sha(probe, key.expected_sha256.as_deref()) {
                    Ok(probe) if probe.sha256 == cached.sha256 => return Ok(cached),
                    Ok(_) | Err(FileReadFailure::Read(ReadFileReason::RangeTooLarge)) => {}
                    Err(failure) => {
                        cache.invalidate(&key.snapshot);
                        return Err(failure);
                    }
                }
                cache.invalidate(&key.snapshot);
            }
            let result = enforce_expected_sha(
                read(key.start, key.limit).await,
                key.expected_sha256.as_deref(),
            );
            match &result {
                Ok(range) => cache.remember(key.snapshot, range.clone()),
                Err(_) => cache.invalidate(&key.snapshot),
            }
            result
        }
        .boxed();
        let flight = self.flight(key, deadline, physical_deadline, work);
        match tokio::time::timeout_at(deadline, flight).await {
            Ok(result) => (*result).clone(),
            Err(_) => Err(ReadFileReason::Timeout.into()),
        }
    }
}

#[cfg(test)]
#[path = "tests/read_cache.rs"]
mod tests;
