//! One SQLite writer with bounded batching and exact successful commit fences.
use super::{SessionStoreInner, SessionWrite};
use serde::Serialize;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const ORDINARY_COALESCE: Duration = Duration::from_millis(20);

pub(super) struct LedgerWriterGuard {
    shared: Arc<LedgerWriterShared>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}
struct LedgerWriterShared {
    state: Mutex<LedgerWriterState>,
    cvar: Condvar,
    completed: tokio::sync::Notify,
}
#[derive(Default)]
struct LedgerWriterState {
    dirty_since: Option<Instant>,
    dirty_generation: u64,
    writes_completed: u64,
    attempted_generation: u64,
    flush_generation: u64,
    shutdown: bool,
    write_attempts: u64,
    failed_attempts: u64,
    ordinary_notifications: u64,
    durability_barriers: u64,
    last_attempt: Option<WriteAttemptObservation>,
}
impl std::fmt::Debug for LedgerWriterGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionDatabaseWriter")
            .finish_non_exhaustive()
    }
}
impl LedgerWriterGuard {
    pub(super) fn spawn(
        inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
    ) -> Option<Arc<Self>> {
        Self::spawn_with_window(inner, write_mutex, ORDINARY_COALESCE)
    }
    fn spawn_with_window(
        inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
        window: Duration,
    ) -> Option<Arc<Self>> {
        let generation = inner.lock().ok()?.sessions.generation;
        let shared = Arc::new(LedgerWriterShared {
            state: Mutex::new(LedgerWriterState {
                dirty_generation: generation,
                writes_completed: generation,
                ..Default::default()
            }),
            cvar: Condvar::new(),
            completed: tokio::sync::Notify::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("session-sqlite-writer".into())
            .spawn(move || writer_loop(thread_shared, inner, write_mutex, window))
            .ok()?;
        Some(Arc::new(Self {
            shared,
            join: Mutex::new(Some(join)),
        }))
    }
    #[cfg(test)]
    fn write_cycles(&self) -> u64 {
        self.shared.state.lock().unwrap().write_attempts
    }
    pub(super) fn notify_generation(&self, generation: u64) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("Session writer state poisoned");
        state.ordinary_notifications = state.ordinary_notifications.saturating_add(1);
        state.dirty_generation = state.dirty_generation.max(generation);
        if generation > state.writes_completed {
            state.dirty_since.get_or_insert_with(Instant::now);
        }
        self.shared.cvar.notify_all();
    }
    pub(super) async fn flush_through(&self, generation: u64) -> Result<(), ()> {
        let first_attempt = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("Session writer state poisoned");
            state.durability_barriers = state.durability_barriers.saturating_add(1);
            state.write_attempts
        };
        let mut requested = false;
        loop {
            let completed = self.shared.completed.notified();
            tokio::pin!(completed);
            completed.as_mut().enable();
            {
                let mut state = self
                    .shared
                    .state
                    .lock()
                    .expect("Session writer state poisoned");
                if state.writes_completed >= generation {
                    return Ok(());
                }
                if state.write_attempts > first_attempt && state.attempted_generation >= generation
                {
                    return Err(());
                }
                if !requested {
                    state.dirty_generation = state.dirty_generation.max(generation);
                    state.flush_generation = state.flush_generation.max(generation);
                    state.dirty_since.get_or_insert_with(Instant::now);
                    requested = true;
                    self.shared.cvar.notify_all();
                }
            }
            completed.await;
        }
    }
    #[cfg(any(test, feature = "root-test-support"))]
    pub(super) fn flush(&self) {
        let mut state = self.shared.state.lock().unwrap();
        let generation = state.dirty_generation;
        let attempts = state.write_attempts;
        if state.writes_completed >= generation {
            return;
        }
        state.flush_generation = generation;
        state.dirty_since.get_or_insert_with(Instant::now);
        self.shared.cvar.notify_all();
        while state.writes_completed < generation
            && (state.write_attempts == attempts || state.attempted_generation < generation)
        {
            state = self.shared.cvar.wait(state).unwrap();
        }
    }
}
impl Drop for LedgerWriterGuard {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("Session writer state poisoned");
            state.shutdown = true;
            if state.dirty_generation > state.writes_completed {
                state.dirty_since.get_or_insert_with(Instant::now);
            }
            self.shared.cvar.notify_all();
        }
        if let Some(join) = self
            .join
            .lock()
            .expect("Session writer join poisoned")
            .take()
        {
            let _ = join.join();
        }
    }
}

pub(super) fn persist_batch(
    inner: &Arc<Mutex<SessionStoreInner>>,
    write_mutex: &Arc<Mutex<()>>,
) -> (u64, Result<u64, String>, f64, f64) {
    persist_batch_observed(inner, write_mutex, |_| {})
}

fn persist_batch_observed(
    inner: &Arc<Mutex<SessionStoreInner>>,
    write_mutex: &Arc<Mutex<()>>,
    after_snapshot: impl FnOnce(&[SessionWrite]),
) -> (u64, Result<u64, String>, f64, f64) {
    let _write = write_mutex.lock().expect("Session writer mutex poisoned");
    {
        let mut inner = inner.lock().expect("Session store mutex poisoned");
        if inner.persistence.is_none() {
            // The memory-only fixture/embedder has no deferred durability queue.
            for record in inner
                .sessions
                .records
                .values_mut()
                .filter_map(super::StoredSession::hot_mut)
            {
                record.pending_events.clear();
            }
            inner.sessions.dirty.clear();
            return (inner.sessions.generation, Ok(0), 0.0, 0.0);
        }
    }
    let waiting = Instant::now();
    let (database, generation, writes, wait_ms, hold_ms) = {
        let inner = inner.lock().expect("Session store mutex poisoned");
        let acquired = Instant::now();
        let writes = inner
            .sessions
            .dirty
            .iter()
            .map(|(id, revision)| {
                let record = inner
                    .sessions
                    .get(id)
                    .and_then(super::StoredSession::hot)
                    .ok_or_else(|| "dirty Session has no mutable state".to_owned())?;
                Ok(SessionWrite {
                    revision: *revision,
                    expected_revision: record.durable.as_ref().map(|cold| cold.database_revision),
                    previous_message_revision: record
                        .durable
                        .as_ref()
                        .map_or(0, |cold| cold.retained_message_revision),
                    record: super::PersistedSessionRecord::database_delta(
                        record,
                        record
                            .durable
                            .as_ref()
                            .map_or(0, |cold| cold.retained_message_revision),
                    ),
                    events: record.pending_events.iter().cloned().collect(),
                })
            })
            .collect::<Result<Vec<_>, String>>();
        (
            inner
                .persistence
                .as_ref()
                .map(|state| Arc::clone(&state.database)),
            inner.sessions.generation,
            writes,
            acquired.duration_since(waiting).as_secs_f64() * 1000.0,
            acquired.elapsed().as_secs_f64() * 1000.0,
        )
    };
    if let Ok(writes) = &writes {
        after_snapshot(writes);
    }
    let committed = writes.and_then(|writes| {
        let database = database.as_ref().expect("persistent batch has database");
        let cold = writes
            .iter()
            .map(|write| database.cold(write))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        let bytes = if writes.is_empty() {
            0
        } else {
            database.write(&writes).map_err(|error| {
                super::bound_summary_string(&format!("Session persistence failed: {error}"))
            })?
        };
        Ok((bytes, writes, cold))
    });
    let result = committed
        .as_ref()
        .map(|(bytes, _, _)| *bytes)
        .map_err(Clone::clone);
    let mut inner = inner.lock().expect("Session store mutex poisoned");
    if let Some(state) = inner.persistence.as_mut() {
        state.last_persist_error = result.as_ref().err().cloned();
    }
    if let Ok((_, writes, cold)) = committed {
        for (write, mut cold) in writes.iter().zip(cold) {
            if let Some(record) = inner
                .sessions
                .get(&write.record.session_id)
                .and_then(super::StoredSession::hot)
            {
                cold.resident_instructions = record.project_instructions.clone().map(Arc::new);
                cold.project_instructions = record
                    .project_instructions
                    .as_ref()
                    .map(|instructions| instructions.to_summary());
                cold.unfenced_completion_ids = record
                    .completion_assignment_fence_fingerprints
                    .iter()
                    .filter(|(_, f)| f.is_none())
                    .map(|(id, _)| id.clone())
                    .collect();
            }
            inner.sessions.acknowledge(
                &write.record.session_id,
                write.revision,
                &write.events,
                cold,
            );
        }
        inner.compact_hot_sessions("");
    }
    (generation, result, wait_ms, hold_ms)
}

fn writer_loop(
    shared: Arc<LedgerWriterShared>,
    inner: Arc<Mutex<SessionStoreInner>>,
    write_mutex: Arc<Mutex<()>>,
    window: Duration,
) {
    loop {
        {
            let mut state = shared.state.lock().expect("Session writer state poisoned");
            while state.dirty_since.is_none() && !state.shutdown {
                state = shared.cvar.wait(state).unwrap();
            }
            if state.dirty_since.is_none() {
                break;
            }
            while !state.shutdown && state.flush_generation <= state.writes_completed {
                let Some(remaining) = state
                    .dirty_since
                    .and_then(|since| window.checked_sub(since.elapsed()))
                    .filter(|d| !d.is_zero())
                else {
                    break;
                };
                state = shared.cvar.wait_timeout(state, remaining).unwrap().0;
            }
            state.dirty_since = None;
        }
        let started = Instant::now();
        let (generation, result, wait_ms, hold_ms) = persist_batch(&inner, &write_mutex);
        let mut state = shared.state.lock().expect("Session writer state poisoned");
        state.write_attempts = state.write_attempts.saturating_add(1);
        state.attempted_generation = generation;
        let notifications = generation.saturating_sub(state.writes_completed);
        match &result {
            Ok(_) => {
                state.writes_completed = generation;
                if state.dirty_generation <= generation {
                    state.dirty_since = None;
                }
            }
            Err(_) => state.failed_attempts = state.failed_attempts.saturating_add(1),
        }
        state.last_attempt = Some(WriteAttemptObservation {
            success: result.is_ok(),
            logical_payload_bytes: result.ok(),
            write_ms: started.elapsed().as_secs_f64() * 1000.0,
            snapshot_lock_wait_ms: wait_ms,
            snapshot_lock_hold_ms: hold_ms,
            dirty_notifications: notifications,
        });
        shared.cvar.notify_all();
        shared.completed.notify_waiters();
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteAttemptObservation {
    pub success: bool,
    /// Changed JSON payload only; excludes SQLite pages, indexes and checkpoints.
    pub logical_payload_bytes: Option<u64>,
    pub write_ms: f64,
    pub snapshot_lock_wait_ms: f64,
    pub snapshot_lock_hold_ms: f64,
    pub dirty_notifications: u64,
}
#[derive(Debug, Serialize)]
pub struct WriterObservation {
    pub write_attempts: u64,
    pub failed_attempts: u64,
    pub ordinary_notifications: u64,
    pub durability_barriers: u64,
    pub pending_notifications: u64,
    pub last_attempt: Option<WriteAttemptObservation>,
}
#[derive(Debug, Serialize)]
pub struct PersistenceObservation {
    pub mode: &'static str,
    pub storage_observation: &'static str,
    /// Sizes of the shared database and WAL, not Session-only disk attribution.
    pub database_bytes: Option<u64>,
    pub wal_bytes: Option<u64>,
    pub writer: Option<WriterObservation>,
}
impl super::SessionStore {
    pub fn persistence_observation(&self) -> PersistenceObservation {
        let path = self
            .inner
            .lock()
            .ok()
            .and_then(|inner| inner.persistence.as_ref().map(|state| state.path.clone()));
        let bytes = path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|m| m.len());
        let wal_bytes = path.as_ref().and_then(|path| {
            let mut wal = path.as_os_str().to_os_string();
            wal.push("-wal");
            std::fs::metadata(wal).ok().map(|m| m.len())
        });
        let writer = self.writer.as_ref().and_then(|writer| {
            writer
                .shared
                .state
                .lock()
                .ok()
                .map(|state| WriterObservation {
                    write_attempts: state.write_attempts,
                    failed_attempts: state.failed_attempts,
                    ordinary_notifications: state.ordinary_notifications,
                    durability_barriers: state.durability_barriers,
                    pending_notifications: state
                        .dirty_generation
                        .saturating_sub(state.writes_completed),
                    last_attempt: state.last_attempt.clone(),
                })
        });
        PersistenceObservation {
            mode: if path.is_some() { "sqlite" } else { "memory" },
            storage_observation: if bytes.is_some() {
                "available"
            } else if path.is_some() {
                "unavailable"
            } else {
                "disabled"
            },
            database_bytes: bytes,
            wal_bytes,
            writer,
        }
    }
}

#[cfg(test)]
mod async_tests;
#[cfg(test)]
mod observation_tests;
#[cfg(test)]
mod sqlite_tests;
#[cfg(test)]
mod tests;
