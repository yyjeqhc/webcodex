//! Background full-ledger persistence. Ordinary dirty notifications coalesce
//! behind one fixed short deadline; explicit flush and shutdown bypass it.
//! This changes write scheduling only, not ledger format or durable identity.
use super::{bound_summary_string, write_ledger_atomic, SessionStoreInner};
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
}

#[derive(Default)]
struct LedgerWriterState {
    // First pending mark owns the deadline; later progress never extends it.
    dirty_since: Option<Instant>,
    dirty_generation: u64,
    // A completed cycle is an attempted write, not necessarily a successful one.
    // SessionPersistence.last_persist_error remains the success/error authority.
    writes_completed: u64,
    flush_generation: u64,
    shutdown: bool,
    #[cfg(test)]
    write_cycles: usize,
}

impl LedgerWriterState {
    fn mark_dirty(&mut self, now: Instant) -> u64 {
        self.dirty_since.get_or_insert(now);
        self.dirty_generation = self.dirty_generation.saturating_add(1);
        self.dirty_generation
    }

    fn coalescing_wait(&self, now: Instant, window: Duration) -> Option<Duration> {
        if self.shutdown || self.flush_generation > self.writes_completed {
            return None;
        }
        self.dirty_since.and_then(|since| {
            window
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
        let shared = Arc::new(LedgerWriterShared {
            state: Mutex::new(LedgerWriterState::default()),
            cvar: Condvar::new(),
        });
        let shared_thread = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("session-ledger-writer".to_string())
            .spawn(move || ledger_writer_loop(shared_thread, store_inner, write_mutex, window))
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
        let snapshot = {
            let inner = store_inner.lock().expect("session store mutex poisoned");
            inner
                .persistence
                .as_ref()
                .map(|persistence| (persistence.path.clone(), inner.to_persisted_ledger()))
        };
        let result = match snapshot {
            Some((path, ledger)) => write_ledger_atomic(&path, &ledger).map_err(|err| {
                bound_summary_string(&format!("persist_failed: {}: {err}", path.display()))
            }),
            None => Ok(()),
        };
        {
            let mut inner = store_inner.lock().expect("session store mutex poisoned");
            if let Some(persistence) = inner.persistence.as_mut() {
                match &result {
                    Ok(()) => persistence.last_persist_error = None,
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
            state.writes_completed = generation;
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

#[cfg(test)]
mod tests;
