//! Budgeted optional projections on the canonical writer, not a second owner.
//! Try-lock avoids queueing; SQLite progress interrupts expensive queries. Every
//! cursor write is transactional and accepted for output BEFORE commit. Required
//! explicit acknowledgements use the same operation with no optional budget.
use crate::connection_observation::{try_lock_connection, StoreConnectionGuard};
use crate::{Database, StoreDomain};
use rusqlite::{Connection, ErrorCode};
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct ProjectionDeadline;
impl std::fmt::Display for ProjectionDeadline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("optional projection deadline")
    }
}
impl std::error::Error for ProjectionDeadline {}

pub(crate) fn check_deadline(deadline: Option<Instant>) -> anyhow::Result<()> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ProjectionDeadline.into());
    }
    Ok(())
}

struct OptionalConnection<'a> {
    connection: StoreConnectionGuard<'a>,
    busy_timeout: Duration,
}
impl Deref for OptionalConnection<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl DerefMut for OptionalConnection<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}
impl Drop for OptionalConnection<'_> {
    fn drop(&mut self) {
        let _ = self.connection.progress_handler(0, None::<fn() -> bool>);
        let _ = self.connection.busy_timeout(self.busy_timeout);
    }
}

impl Database {
    pub(crate) fn with_projection_connection<T>(
        &self,
        domain: StoreDomain,
        deadline: Option<Instant>,
        work: impl FnOnce(&mut Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<Option<T>> {
        let Some(deadline) = deadline else {
            return work(&mut self.lock_connection(domain)).map(Some);
        };
        if Instant::now() >= deadline {
            tracing::debug!(target:"webcodex_store::projection",domain=domain.as_str(),outcome="budget_exhausted", "optional projection omitted without cursor mutation");
            return Ok(None);
        }
        let Some(connection) =
            try_lock_connection(&self.conn, self.connection_observer.as_ref(), domain)
        else {
            tracing::debug!(target:"webcodex_store::projection",domain=domain.as_str(),outcome="skipped_due_to_contention", "optional projection omitted without cursor mutation");
            return Ok(None);
        };
        let busy_ms: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        let busy_ms = u64::try_from(busy_ms)?;
        let mut connection = OptionalConnection {
            connection,
            busy_timeout: Duration::from_millis(busy_ms),
        };
        connection.busy_timeout(Duration::ZERO)?;
        let mut interrupted = false;
        connection.progress_handler(
            200,
            Some(move || {
                // Interrupt at most once: rollback must remain possible after the
                // interrupted statement propagates through the transaction owner.
                if !interrupted && Instant::now() >= deadline {
                    interrupted = true;
                    true
                } else {
                    false
                }
            }),
        )?;
        let result = work(&mut connection);
        // Restore connection policy on errors and panic as well as success.
        drop(connection);
        match result {
            Ok(value)=>Ok(Some(value)), // Never discard an already committed projection.
            Err(error) if error.is::<ProjectionDeadline>() || error.downcast_ref::<rusqlite::Error>().is_some_and(|e|
                matches!(e,rusqlite::Error::SqliteFailure(code,_) if matches!(code.code,ErrorCode::OperationInterrupted|ErrorCode::DatabaseBusy|ErrorCode::DatabaseLocked))) => {
                tracing::debug!(target:"webcodex_store::projection",domain=domain.as_str(),outcome="deadline_or_busy", "optional projection rolled back");
                Ok(None)
            }
            Err(error)=>Err(error),
        }
    }
}

#[cfg(test)]
#[path = "optional_projection_tests.rs"]
mod tests;
