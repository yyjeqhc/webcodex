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
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct HistoryReadBudget {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}
impl HistoryReadBudget {
    pub fn until(deadline: Instant) -> Self {
        Self {
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
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
struct Scope(Option<(usize, HistoryReadBudget)>);
impl Drop for Scope {
    fn drop(&mut self) {
        BUDGET.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(crate) struct HistoryConnectionGuard<'a> {
    connection: StoreConnectionGuard<'a>,
    old_busy_timeout: Option<Duration>,
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
            });
        };
        let started = Instant::now();
        let connection = loop {
            budget.check()?;
            if let Some(connection) =
                try_lock_connection(connection, self.connection_observer.as_ref(), domain)
            {
                break connection;
            }
            // This method is invoked on an admitted blocking task, not a Tokio worker.
            std::thread::sleep(Duration::from_millis(1));
        };
        let timeout: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        let timeout = u64::try_from(timeout)?;
        let connection = HistoryConnectionGuard {
            connection,
            old_busy_timeout: Some(Duration::from_millis(timeout)),
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
        tracing::debug!(target: "webcodex_store::connection", phase="history_admission", wait_ms=started.elapsed().as_secs_f64()*1000.0, "budgeted Store admission");
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
