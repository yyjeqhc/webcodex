//! Untrusted external observations, deliberately separate from native Job truth.
use super::Database;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;

pub const MAX_EXTERNAL_OBSERVATIONS_PER_SESSION: usize = 256;
const MAX_EXTERNAL_OBSERVATIONS: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalObservation {
    pub adapter_id: String,
    pub event_id: String,
    pub tool: String,
    pub exit_code: Option<i32>,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalObservationError {
    InvalidInput,
    Conflict,
    Capacity,
    Storage,
}

fn db_error(_: rusqlite::Error) -> ExternalObservationError {
    ExternalObservationError::Storage
}

fn digest_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Database {
    /// The Runtime must authorize the exact Session and Project first. IDs are
    /// scoped to that association, not credentials or execution authority.
    pub fn record_external_observation(
        &self,
        session: &str,
        project: &str,
        input: ExternalObservation,
    ) -> Result<(ExternalObservation, bool), ExternalObservationError> {
        if !session.starts_with("wc_sess_")
            || session.len() > 128
            || project.is_empty()
            || project.len() > 1024
            || !digest_id(&input.adapter_id)
            || !digest_id(&input.event_id)
            || input.tool.is_empty()
            || input.tool.len() > 64
            || !input
                .tool
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
            || input.recorded_at <= 0
        {
            return Err(ExternalObservationError::InvalidInput);
        }
        let mut conn = self.lock_connection(crate::StoreDomain::Communication);
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let previous = tx
            .query_row(
                "SELECT project, tool, exit_code, recorded_at FROM wc_external_observations
             WHERE session_id=?1 AND adapter_id=?2 AND event_id=?3",
                params![session, input.adapter_id, input.event_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i32>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        if let Some((stored_project, tool, code, recorded_at)) = previous {
            if stored_project != project || tool != input.tool || code != input.exit_code {
                return Err(ExternalObservationError::Conflict);
            }
            return Ok((
                ExternalObservation {
                    recorded_at,
                    ..input
                },
                false,
            ));
        }
        let session_count: i64 = tx
            .query_row(
                "SELECT count(*) FROM wc_external_observations WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let total: i64 = tx
            .query_row("SELECT count(*) FROM wc_external_observations", [], |r| {
                r.get(0)
            })
            .map_err(db_error)?;
        if session_count >= MAX_EXTERNAL_OBSERVATIONS_PER_SESSION as i64
            || total >= MAX_EXTERNAL_OBSERVATIONS as i64
        {
            // No silent eviction of deduplication identities. Do not acknowledge
            // an event that cannot be retained or turn a replay into a new event.
            return Err(ExternalObservationError::Capacity);
        }
        tx.execute("INSERT INTO wc_external_observations (session_id, project, adapter_id, event_id, tool, exit_code, recorded_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![session, project, input.adapter_id, input.event_id, input.tool, input.exit_code, input.recorded_at]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok((input, true))
    }

    pub fn list_external_observations(
        &self,
        session: &str,
        project: &str,
    ) -> Result<Vec<ExternalObservation>, ExternalObservationError> {
        let conn = self.lock_connection(crate::StoreDomain::Communication);
        let mut statement = conn.prepare("SELECT adapter_id, event_id, tool, exit_code, recorded_at FROM wc_external_observations WHERE session_id=?1 AND project=?2 ORDER BY recorded_at, adapter_id, event_id LIMIT 256").map_err(db_error)?;
        let rows = statement
            .query_map(params![session, project], |r| {
                Ok(ExternalObservation {
                    adapter_id: r.get(0)?,
                    event_id: r.get(1)?,
                    tool: r.get(2)?,
                    exit_code: r.get(3)?,
                    recorded_at: r.get(4)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }
}
