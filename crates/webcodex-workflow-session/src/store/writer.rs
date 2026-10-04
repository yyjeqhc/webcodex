//! Background full-ledger persistence. Ordinary dirty notifications coalesce
//! behind one cost-aware fixed deadline; explicit flush and shutdown bypass it.
//! This changes write scheduling only, not ledger format or durable identity.
use super::{bound_summary_string, SessionStoreInner};
use crate::persistence::write_ledger_atomic_measured;
use serde::Serialize;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const ORDINARY_COALESCE: Duration = Duration::from_millis(20);
const LARGE_LEDGER_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COST_COALESCE: Duration = Duration::from_secs(1);

fn cost_coalesce(bytes: u64, duration: Duration) -> Duration {
    if bytes < LARGE_LEDGER_BYTES {
        return ORDINARY_COALESCE;
    }
    duration
        .saturating_mul(4)
        .max(Duration::from_millis(250))
        .min(MAX_COST_COALESCE)
}

pub(super) struct LedgerWriterGuard {
    shared: Arc<LedgerWriterShared>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

struct LedgerWriterShared {
    state: Mutex<LedgerWriterState>,
    cvar: Condvar,
}

#[derive(Default)]
struct LedgerWriterState {
    // First pending mark owns the deadline; later progress never extends it.
    dirty_since: Option<Instant>,
    // Captured by the first mark; later work and cost updates cannot extend it.
    dirty_cost: Duration,
    next_cost: Duration,

    dirty_generation: u64,
    // A completed cycle is an attempted write, not necessarily a successful one.
    // SessionPersistence.last_persist_error remains the success/error authority.
    writes_completed: u64,
    flush_generation: u64,
    shutdown: bool,
    write_attempts: u64,
    failed_attempts: u64,
    last_attempt: Option<WriteAttemptObservation>,
    #[cfg(test)]
    write_cycles: usize,
}

impl LedgerWriterState {
    fn mark_dirty(&mut self, now: Instant) -> u64 {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(now);
            self.dirty_cost = self.next_cost;
        }
        self.dirty_generation = self.dirty_generation.saturating_add(1);
        self.dirty_generation
    }

    fn coalescing_wait(&self, now: Instant, window: Duration) -> Option<Duration> {
        if self.shutdown || self.flush_generation > self.writes_completed {
            return None;
        }
        self.dirty_since.and_then(|since| {
            window
                .max(self.dirty_cost)
                .checked_sub(now.saturating_duration_since(since))
                .filter(|remaining| !remaining.is_zero())
        })
    }
}

impl std::fmt::Debug for LedgerWriterGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerWriterGuard").finish_non_exhaustive()
    }
}

impl LedgerWriterGuard {
    pub(super) fn spawn(
        store_inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
    ) -> Option<Arc<Self>> {
        Self::spawn_with_window(store_inner, write_mutex, ORDINARY_COALESCE)
    }

    // The window is private policy, not a deployment knob. Tests inject a long
    // window to prove flush/drop bypass without short timing assertions.
    fn spawn_with_window(
        store_inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
        window: Duration,
    ) -> Option<Arc<Self>> {
        Self::spawn_with_policy(store_inner, write_mutex, window, true)
    }

    // Private comparison seam: production always enables cost-aware scheduling.
    fn spawn_with_policy(
        store_inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
        window: Duration,
        cost_aware: bool,
    ) -> Option<Arc<Self>> {
        let path = store_inner
            .lock()
            .ok()
            .and_then(|inner| inner.persistence.as_ref().map(|p| p.path.clone()));
        let bytes = path
            .and_then(|path| std::fs::metadata(path).ok())
            .map_or(0, |meta| meta.len());
        let shared = Arc::new(LedgerWriterShared {
            state: Mutex::new(LedgerWriterState {
                next_cost: if cost_aware {
                    cost_coalesce(bytes, Duration::ZERO)
                } else {
                    Duration::ZERO
                },
                ..Default::default()
            }),
            cvar: Condvar::new(),
        });
        let shared_thread = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("session-ledger-writer".to_string())
            .spawn(move || {
                ledger_writer_loop(shared_thread, store_inner, write_mutex, window, cost_aware)
            })
            .ok()?;
        Some(Arc::new(Self {
            shared,
            join: Mutex::new(Some(join)),
        }))
    }

    pub(super) fn mark_dirty(&self) -> u64 {
        let generation = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned")
            .mark_dirty(Instant::now());
        // The same condition variable has writer and flush waiters. notify_one
        // can wake a flusher instead of the only thread able to perform I/O.
        self.shared.cvar.notify_all();
        generation
    }

    /// Wait for the exact requested generation, not later concurrent mutations.
    /// Errors also release the barrier; the caller checks last_persist_error.
    pub(super) fn flush_through(&self, generation: u64) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned");
        state.flush_generation = state.flush_generation.max(generation);
        self.shared.cvar.notify_all();
        while state.writes_completed < generation {
            state = self
                .shared
                .cvar
                .wait(state)
                .expect("session ledger writer state poisoned");
        }
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub(super) fn flush(&self) {
        let generation = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned")
            .dirty_generation;
        self.flush_through(generation);
    }

    #[cfg(test)]
    pub(super) fn write_cycles(&self) -> usize {
        self.shared.state.lock().unwrap().write_cycles
    }
}

impl Drop for LedgerWriterGuard {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            state.shutdown = true;
            self.shared.cvar.notify_all();
        }
        if let Some(join) = self
            .join
            .lock()
            .expect("session ledger writer join mutex poisoned")
            .take()
        {
            let _ = join.join();
        }
    }
}

fn ledger_writer_loop(
    shared: Arc<LedgerWriterShared>,
    store_inner: Arc<Mutex<SessionStoreInner>>,
    write_mutex: Arc<Mutex<()>>,
    window: Duration,
    cost_aware: bool,
) {
    loop {
        let generation = {
            let mut state = shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            while state.dirty_since.is_none() && !state.shutdown {
                state = shared
                    .cvar
                    .wait(state)
                    .expect("session ledger writer state poisoned");
            }
            if state.dirty_since.is_none() {
                break;
            }
            while let Some(remaining) = state.coalescing_wait(Instant::now(), window) {
                (state, _) = shared
                    .cvar
                    .wait_timeout(state, remaining)
                    .expect("session ledger writer state poisoned");
            }
            let generation = state.dirty_generation;
            state.dirty_since = None;
            generation
        };

        // Preserve the shared write lock and snapshot order: no old background
        // write may race past a synchronous write or durable commit barrier.
        let _write_guard = write_mutex
            .lock()
            .expect("session persistence mutex poisoned");
        let waiting = Instant::now();
        let (snapshot, lock_wait, lock_hold) = {
            let inner = store_inner.lock().expect("session store mutex poisoned");
            let acquired = Instant::now();
            let snapshot = inner
                .persistence
                .as_ref()
                .map(|persistence| (persistence.path.clone(), inner.to_persisted_ledger()));
            (
                snapshot,
                acquired.duration_since(waiting),
                acquired.elapsed(),
            )
        };
        tracing::debug!(target: "webcodex::session_cost", phase="snapshot",
            wait_ms=lock_wait.as_secs_f64()*1000.0, hold_ms=lock_hold.as_secs_f64()*1000.0,
            rows=snapshot.as_ref().map_or(0, |(_, ledger)| ledger.sessions.len()),
            "session snapshot lock cost");
        let write_started = Instant::now();
        let result = match snapshot {
            Some((path, ledger)) => write_ledger_atomic_measured(&path, &ledger).map_err(|err| {
                bound_summary_string(&format!("persist_failed: {}: {err}", path.display()))
            }),
            None => Ok(0),
        };
        let write_duration = write_started.elapsed();
        {
            let mut inner = store_inner.lock().expect("session store mutex poisoned");
            if let Some(persistence) = inner.persistence.as_mut() {
                match &result {
                    Ok(_) => persistence.last_persist_error = None,
                    Err(error) => {
                        tracing::warn!("session ledger persistence failed: {}", error);
                        persistence.last_persist_error = Some(error.clone());
                    }
                }
            }
        }
        {
            let mut state = shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            state.write_attempts = state.write_attempts.saturating_add(1);
            if result.is_err() {
                state.failed_attempts = state.failed_attempts.saturating_add(1);
            }
            state.last_attempt = Some(WriteAttemptObservation {
                success: result.is_ok(),
                written_bytes: result.as_ref().ok().copied(),
                write_ms: write_duration.as_secs_f64() * 1000.0,
                snapshot_lock_wait_ms: lock_wait.as_secs_f64() * 1000.0,
                snapshot_lock_hold_ms: lock_hold.as_secs_f64() * 1000.0,
                dirty_notifications: generation.saturating_sub(state.writes_completed),
            });
            state.writes_completed = generation;
            if cost_aware {
                if let Ok(bytes) = &result {
                    state.next_cost = cost_coalesce(*bytes, write_duration);
                }
            }

            #[cfg(test)]
            {
                state.write_cycles += 1;
            }
            shared.cvar.notify_all();
            if state.shutdown && state.dirty_since.is_none() {
                break;
            }
        }
        drop(_write_guard);
    }
}

/// Bounded diagnostics only; never consulted by write admission, flush or retry.
#[derive(Debug, Clone, Serialize)]
pub struct WriteAttemptObservation {
    pub success: bool,
    /// None on failure: an unsuccessful attempt cannot claim committed bytes.
    pub written_bytes: Option<u64>,
    pub write_ms: f64,
    pub snapshot_lock_wait_ms: f64,
    pub snapshot_lock_hold_ms: f64,
    /// Coalesced dirty marks, not distinct Sessions or a business-mutation count.
    pub dirty_notifications: u64,
}

#[derive(Debug, Serialize)]
pub struct WriterObservation {
    pub write_attempts: u64,
    pub failed_attempts: u64,
    /// Dirty marks not yet attempted. Zero does not imply successful durability.
    pub pending_notifications: u64,
    pub last_attempt: Option<WriteAttemptObservation>,
}

#[derive(Debug, Serialize)]
pub struct PersistenceObservation {
    pub mode: &'static str,
    pub ledger_observation: &'static str,
    pub ledger_bytes: Option<u64>,
    pub writer: Option<WriterObservation>,
}

impl super::SessionStore {
    /// Explicit detailed-status observation. Clone only the configured path under
    /// the store lock, then stat outside it. No payload read, flush or dirty mark.
    /// Counters/path metadata are sequential observations, not a commit receipt.
    pub fn persistence_observation(&self) -> PersistenceObservation {
        let path = match self.inner.lock() {
            Ok(inner) => inner.persistence.as_ref().map(|state| state.path.clone()),
            Err(_) => {
                return PersistenceObservation {
                    mode: "unavailable",
                    ledger_observation: "unavailable",
                    ledger_bytes: None,
                    writer: None,
                }
            }
        };
        let mode = match (&path, &self.writer) {
            (None, _) => "memory",
            (Some(_), Some(_)) => "background",
            (Some(_), None) => "synchronous_fallback",
        };
        let ledger_bytes = path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len());
        let writer = self.writer.as_ref().and_then(|writer| {
            writer
                .shared
                .state
                .lock()
                .ok()
                .map(|state| WriterObservation {
                    write_attempts: state.write_attempts,
                    failed_attempts: state.failed_attempts,
                    pending_notifications: state
                        .dirty_generation
                        .saturating_sub(state.writes_completed),
                    last_attempt: state.last_attempt.clone(),
                })
        });
        PersistenceObservation {
            mode,
            ledger_observation: if path.is_none() {
                "disabled"
            } else if ledger_bytes.is_some() {
                "available"
            } else {
                "unavailable"
            },
            ledger_bytes,
            writer,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod observation_tests;
