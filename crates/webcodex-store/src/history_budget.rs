//! A narrowing execution budget for one synchronous historical read. Bound by
//! the admitted Console blocking task, never by principal or transport identity.
//! Cancellation belongs to that task: no shared sqlite3_interrupt handle can
//! accidentally cancel the next user of the connection.
use crate::connection_observation::{lock_connection, try_lock_connection, StoreConnectionGuard};
use crate::{Database, StoreDomain};
use rusqlite::Connection;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

/// Numeric diagnostics only: no SQL text, parameters, Window or principal IDs.
/// SQL profiles include transaction control and repair statements, excluding
/// progress/busy/trace setup. SQLite profile durations have millisecond resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryReadDiagnostics {
    pub sql_statements: u64,
    pub sql_duration: Duration,
    pub connection_wait: Duration,
    pub connection_attempts: u64,
    pub dirty_repair_attempts: u64,
    pub dirty_windows_repaired: u64,
    pub dirty_repair_duration: Duration,
}
#[derive(Default)]
struct Counters {
    sql_statements: AtomicU64,
    sql_nanos: AtomicU64,
    connection_wait_nanos: AtomicU64,
    connection_attempts: AtomicU64,
    dirty_repair_attempts: AtomicU64,
    dirty_windows_repaired: AtomicU64,
    dirty_repair_nanos: AtomicU64,
}
fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Clone)]
pub struct HistoryReadBudget {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    counters: Arc<Counters>,
    sql_profile: bool,
}
impl HistoryReadBudget {
    pub fn until(deadline: Instant) -> Self {
        Self {
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            counters: Arc::new(Counters::default()),
            sql_profile: true,
        }
    }
    /// Numeric SQL profiles are enabled by default for explicit Store callers.
    /// A caller that will not observe them can skip the SQLite callback without
    /// changing the deadline, interruption, connection or repair policy.
    pub fn with_sql_profile(mut self, enabled: bool) -> Self {
        self.sql_profile = enabled;
        self
    }
    pub fn diagnostics(&self) -> HistoryReadDiagnostics {
        let c = &self.counters;
        HistoryReadDiagnostics {
            sql_statements: c.sql_statements.load(Ordering::Relaxed),
            sql_duration: Duration::from_nanos(c.sql_nanos.load(Ordering::Relaxed)),
            connection_wait: Duration::from_nanos(c.connection_wait_nanos.load(Ordering::Relaxed)),
            connection_attempts: c.connection_attempts.load(Ordering::Relaxed),
            dirty_repair_attempts: c.dirty_repair_attempts.load(Ordering::Relaxed),
            dirty_windows_repaired: c.dirty_windows_repaired.load(Ordering::Relaxed),
            dirty_repair_duration: Duration::from_nanos(
                c.dirty_repair_nanos.load(Ordering::Relaxed),
            ),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn exhausted(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline
    }
    fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.exhausted(), "historical read budget exhausted");
        Ok(())
    }
}

thread_local! {
    // Exact Database address scopes this only to the requested store. The guard
    // restores on unwind before this blocking thread returns to Tokio's pool.
    static BUDGET: RefCell<Option<(usize, HistoryReadBudget)>> = const { RefCell::new(None) };
}
fn profile(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Profile(_, duration) = event {
        // Deliberately never access StmtRef::sql or expanded_sql.
        BUDGET.with(|slot| {
            if let Some((_, budget)) = slot.borrow().as_ref() {
                budget
                    .counters
                    .sql_statements
                    .fetch_add(1, Ordering::Relaxed);
                budget
                    .counters
                    .sql_nanos
                    .fetch_add(nanos(duration), Ordering::Relaxed);
            }
        });
    }
}

pub(crate) fn record_dirty_repair(db: &Database, duration: Duration, windows: usize) {
    BUDGET.with(|slot| {
        if let Some((_, budget)) = slot
            .borrow()
            .as_ref()
            .filter(|(id, _)| *id == db as *const Database as usize)
        {
            budget
                .counters
                .dirty_repair_attempts
                .fetch_add(1, Ordering::Relaxed);
            budget
                .counters
                .dirty_windows_repaired
                .fetch_add(windows as u64, Ordering::Relaxed);
            budget
                .counters
                .dirty_repair_nanos
                .fetch_add(nanos(duration), Ordering::Relaxed);
        }
    });
}

struct LockWait {
    started: Instant,
    budget: HistoryReadBudget,
    domain: StoreDomain,
    outcome: &'static str,
}
impl Drop for LockWait {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        self.budget
            .counters
            .connection_attempts
            .fetch_add(1, Ordering::Relaxed);
        self.budget
            .counters
            .connection_wait_nanos
            .fetch_add(nanos(elapsed), Ordering::Relaxed);
        tracing::debug!(target: "webcodex_store::connection", phase="history_connection_wait",
            domain=self.domain.as_str(), outcome=self.outcome,
            elapsed_ms=elapsed.as_secs_f64()*1000.0, "history connection admission");
    }
}

struct Scope(Option<(usize, HistoryReadBudget)>);
impl Drop for Scope {
    fn drop(&mut self) {
        BUDGET.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(crate) struct HistoryConnectionGuard<'a> {
    connection: StoreConnectionGuard<'a>,
    old_busy_timeout: Option<Duration>,
    sql_profile: bool,
}
impl Deref for HistoryConnectionGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl DerefMut for HistoryConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}
impl Drop for HistoryConnectionGuard<'_> {
    fn drop(&mut self) {
        if let Some(timeout) = self.old_busy_timeout {
            if self.sql_profile {
                self.connection
                    .trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
            }
            let _ = self.connection.progress_handler(0, None::<fn() -> bool>);
            let _ = self.connection.busy_timeout(timeout);
        }
    }
}

impl Database {
    pub fn with_history_read_budget<T>(
        &self,
        budget: HistoryReadBudget,
        read: impl FnOnce(&Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.history_budget_reads.fetch_add(1, Ordering::Relaxed);
        budget.check()?;
        anyhow::ensure!(
            BUDGET.with(|slot| slot.borrow().is_none()),
            "nested historical read budget is not supported"
        );
        let _scope = Scope(
            BUDGET.with(|slot| slot.replace(Some((self as *const Self as usize, budget.clone())))),
        );
        let result = read(self);
        budget.check()?;
        result
    }

    pub(crate) fn history_connection<'a>(
        &'a self,
        connection: &'a Mutex<Connection>,
        domain: StoreDomain,
    ) -> anyhow::Result<HistoryConnectionGuard<'a>> {
        let budget = BUDGET.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|(id, _)| *id == self as *const Self as usize)
                .map(|(_, budget)| budget.clone())
        });
        let Some(budget) = budget else {
            return Ok(HistoryConnectionGuard {
                connection: lock_connection(connection, self.connection_observer.as_ref(), domain),
                old_busy_timeout: None,
                sql_profile: false,
            });
        };
        let mut wait = LockWait {
            started: Instant::now(),
            budget: budget.clone(),
            domain,
            outcome: "budget_exhausted",
        };
        let mut connection = loop {
            budget.check()?;
            if let Some(connection) =
                try_lock_connection(connection, self.connection_observer.as_ref(), domain)
            {
                break connection;
            }
            // This method is invoked on an admitted blocking task, not a Tokio worker.
            std::thread::sleep(Duration::from_millis(1));
        };
        connection.set_wait_duration(wait.started.elapsed());
        wait.outcome = "acquired";
        drop(wait);
        let timeout: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        let timeout = u64::try_from(timeout)?;
        let connection = HistoryConnectionGuard {
            connection,
            old_busy_timeout: Some(Duration::from_millis(timeout)),
            sql_profile: budget.sql_profile,
        };
        connection.busy_timeout(Duration::ZERO)?;
        let mut interrupted = false;
        connection.progress_handler(
            500,
            Some(move || {
                // Rollback must still execute after an interrupted statement.
                if !interrupted && budget.exhausted() {
                    interrupted = true;
                    true
                } else {
                    false
                }
            }),
        )?;
        if connection.sql_profile {
            connection.trace_v2(
                rusqlite::trace::TraceEventCodes::SQLITE_TRACE_PROFILE,
                Some(profile),
            );
        }
        Ok(connection)
    }

    pub(crate) fn lock_history_repair_connection(
        &self,
        domain: StoreDomain,
    ) -> anyhow::Result<HistoryConnectionGuard<'_>> {
        self.history_connection(&self.conn, domain)
    }
}

#[cfg(test)]
mod tests;
