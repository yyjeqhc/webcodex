//! SQLite storage for Workflow Session state and independently indexed history.
//! Transactions contain changed Sessions only; events are append/upsert deltas.
//! The database is authoritative backing for clean cache entries, never a replay
//! engine. Loading history cannot dispatch a tool or change Session authority.
use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub(crate) struct SessionDatabase {
    connection: Mutex<Connection>,
}

pub(crate) struct SessionWrite {
    pub revision: u64,
    pub expected_revision: Option<u64>,
    pub previous_message_revision: u64,
    pub record: PersistedSessionRecord,
    pub events: Vec<Arc<SessionEvent>>,
}

fn error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

impl SessionDatabase {
    pub(crate) fn cold(self: &Arc<Self>, write: &SessionWrite) -> io::Result<ColdSessionRecord> {
        let state = state_only(&write.record);
        let mut cold = cold_session_from_persisted(&state, None).map_err(error)?;
        cold.database = Some(Arc::clone(self));
        cold.database_revision = write.revision;
        Ok(cold)
    }
    pub(crate) fn open(path: &Path) -> io::Result<Arc<Self>> {
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path).map_err(error)?;
        // Refuse future formats before changing even journal/schema settings.
        let has_storage: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_session_storage')", [], |row| row.get(0)).map_err(error)?;
        if has_storage {
            let version: i64 = connection
                .query_row(
                    "SELECT version FROM workflow_session_storage WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(error)?;
            if version != 1 {
                return Err(error("unsupported Workflow Session database version"));
            }
        }
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;
             PRAGMA cache_size=-4096;
             CREATE TABLE IF NOT EXISTS workflow_session_storage (
                 singleton INTEGER PRIMARY KEY CHECK(singleton=1), version INTEGER NOT NULL,
                 imported INTEGER NOT NULL DEFAULT 0
             );
             INSERT OR IGNORE INTO workflow_session_storage(singleton,version) VALUES(1,1);",
            )
            .map_err(error)?;
        let version: i64 = connection
            .query_row(
                "SELECT version FROM workflow_session_storage WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(error)?;
        if version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported Workflow Session database version",
            ));
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_sessions (
                 session_id TEXT PRIMARY KEY, project TEXT, owner TEXT NOT NULL,
                 lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active','closed')),
                 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                 revision INTEGER NOT NULL, state_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS workflow_sessions_recent ON workflow_sessions(project,updated_at DESC,session_id DESC);
             CREATE INDEX IF NOT EXISTS workflow_sessions_owner ON workflow_sessions(owner,project,updated_at DESC,session_id DESC);
             CREATE TABLE IF NOT EXISTS workflow_session_events (
                 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                 event_id TEXT NOT NULL UNIQUE, session_id TEXT NOT NULL REFERENCES workflow_sessions(session_id),
                 kind TEXT NOT NULL, call_id TEXT, ordinal INTEGER NOT NULL, payload TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS workflow_session_events_recent ON workflow_session_events(session_id,sequence DESC);
             CREATE INDEX IF NOT EXISTS workflow_session_events_kind ON workflow_session_events(session_id,kind,ordinal DESC);
             CREATE UNIQUE INDEX IF NOT EXISTS workflow_session_events_order ON workflow_session_events(session_id,ordinal);
             CREATE TABLE IF NOT EXISTS workflow_session_messages (
                 message_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES workflow_sessions(session_id),
                 revision INTEGER NOT NULL, payload TEXT NOT NULL, assignment_floor INTEGER, completion_fence TEXT
             );
             CREATE INDEX IF NOT EXISTS workflow_session_messages_revision ON workflow_session_messages(session_id,revision,message_id);
             CREATE TABLE IF NOT EXISTS workflow_session_deliveries (
                 session_id TEXT NOT NULL REFERENCES workflow_sessions(session_id), scope_key TEXT NOT NULL,
                 message_id TEXT NOT NULL REFERENCES workflow_session_messages(message_id), fingerprint TEXT NOT NULL,
                 PRIMARY KEY(session_id,scope_key)
             );
             CREATE TABLE IF NOT EXISTS workflow_session_tombstones (
                 session_id TEXT PRIMARY KEY, payload TEXT NOT NULL
             );",
        ).map_err(error)?;
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
        }))
    }

    pub(crate) fn write(&self, writes: &[SessionWrite]) -> io::Result<u64> {
        let mut connection = self.connection.lock().map_err(error)?;
        let transaction = connection.transaction().map_err(error)?;
        let mut bytes = 0_u64;
        for write in writes {
            bytes = bytes.saturating_add(write_row(&transaction, write)?);
        }
        transaction.commit().map_err(error)?;
        Ok(bytes)
    }

    pub(crate) fn inventory(self: &Arc<Self>) -> io::Result<Vec<ColdSessionRecord>> {
        let connection = self.connection.lock().map_err(error)?;
        let mut statement = connection.prepare("SELECT revision,state_json,session_id,project,owner,lifecycle,created_at,updated_at FROM workflow_sessions ORDER BY updated_at,session_id").map_err(error)?;
        let mut rows = statement.query([]).map_err(error)?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().map_err(error)? {
            let (revision, record) = state_from_row(row).map_err(error)?;
            let mut cold = cold_session_from_persisted(&record, None).map_err(error)?;
            cold.database = Some(Arc::clone(self));
            cold.database_revision = revision;
            result.push(cold);
        }
        Ok(result)
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub(crate) fn fail_writes(&self, fail: bool) {
        self.connection
            .lock()
            .unwrap()
            .pragma_update(None, "query_only", fail)
            .unwrap();
    }

    pub(crate) fn instruction_at(
        &self,
        session_id: &str,
        through: usize,
    ) -> io::Result<Option<String>> {
        self.connection.lock().map_err(error)?.query_row(
            "SELECT event_id FROM workflow_session_events WHERE session_id=?1 AND kind='task_instruction' AND ordinal<?2 ORDER BY ordinal DESC LIMIT 1",
            params![session_id, i64::try_from(through).map_err(error)?], |row| row.get(0)
        ).optional().map_err(error)
    }

    pub(crate) fn tombstones(&self) -> io::Result<Vec<SessionRetentionTombstone>> {
        let connection = self.connection.lock().map_err(error)?;
        let mut statement = connection.prepare("SELECT session_id,payload FROM workflow_session_tombstones WHERE session_id NOT IN (SELECT session_id FROM workflow_sessions) ORDER BY session_id").map_err(error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error)?;
        rows.map(|row| {
            let (id, payload) = row.map_err(error)?;
            let value: SessionRetentionTombstone = serde_json::from_str(&payload).map_err(error)?;
            if value.session_id != id
                || !is_valid_session_id(&value.session_id)
                || !is_lower_hex_sha256(&value.owner_authority_fingerprint)
                || !is_lower_hex_sha256(&value.incarnation_fingerprint)
            {
                return Err(error("invalid Session tombstone"));
            }
            Ok(value)
        })
        .collect()
    }

    pub(crate) fn load(
        &self,
        cold: &ColdSessionRecord,
        recent_limit: usize,
    ) -> io::Result<PersistedSessionRecord> {
        let mut connection = self.connection.lock().map_err(error)?;
        let transaction = connection.transaction().map_err(error)?;
        let found = transaction.query_row(
            "SELECT revision,state_json,session_id,project,owner,lifecycle,created_at,updated_at FROM workflow_sessions WHERE session_id=?1",
            [&cold.session_id], state_from_row,
        ).optional().map_err(error)?;
        let (revision, mut record) =
            found.ok_or_else(|| error("Session database row unavailable"))?;
        if revision != cold.database_revision {
            return Err(error("Session database revision changed"));
        }
        if record.session_id != cold.session_id
            || record.project != cold.project
            || record.owner_authority_fingerprint != cold.owner_authority_fingerprint
            || record.created_at != cold.created_at
        {
            return Err(error("Session database identity changed"));
        }
        record.events = read_events(&transaction, &cold.session_id, recent_limit)?;
        let mut statement = transaction.prepare(
            "SELECT payload,revision,assignment_floor,completion_fence,message_id FROM workflow_session_messages WHERE session_id=?1 ORDER BY rowid",
        ).map_err(error)?;
        let mut rows = statement.query([&cold.session_id]).map_err(error)?;
        while let Some(row) = rows.next().map_err(error)? {
            let payload: String = row.get(0).map_err(error)?;
            let message: SessionMessage = serde_json::from_str(&payload).map_err(error)?;
            if message.session_id != record.session_id
                || message.message_id != row.get::<_, String>(4).map_err(error)?
            {
                return Err(error("Session message identity mismatch"));
            }
            let revision: i64 = row.get(1).map_err(error)?;
            if u64::try_from(revision).map_err(error)? > record.message_observation_revision {
                return Err(error("Session message revision mismatch"));
            }
            record.message_observation_revisions.insert(
                message.message_id.clone(),
                u64::try_from(revision).map_err(error)?,
            );
            if let Some(floor) = row.get::<_, Option<i64>>(2).map_err(error)? {
                record.assignment_history_floors.insert(
                    message.message_id.clone(),
                    u64::try_from(floor).map_err(error)?,
                );
            }
            if let Some(fence) = row.get::<_, Option<String>>(3).map_err(error)? {
                record
                    .completion_assignment_fence_fingerprints
                    .insert(message.message_id.clone(), fence);
            }
            record.messages.push(Arc::new(message));
        }
        drop(rows);
        drop(statement);
        let mut statement = transaction.prepare("SELECT scope_key,message_id,fingerprint FROM workflow_session_deliveries WHERE session_id=?1").map_err(error)?;
        let mut rows = statement.query([&cold.session_id]).map_err(error)?;
        while let Some(row) = rows.next().map_err(error)? {
            record.message_delivery_replays.insert(
                row.get(0).map_err(error)?,
                super::super::model::SessionMessageDeliveryReplay {
                    message_id: row.get(1).map_err(error)?,
                    payload_fingerprint: row.get(2).map_err(error)?,
                },
            );
        }
        drop(rows);
        drop(statement);
        transaction.commit().map_err(error)?;
        // SQLite rows are persisted input, so use the same privacy/replay validation boundary.
        let history_start = record.event_history_start;
        let evidence_counts = (
            record.events.len(),
            record.messages.len(),
            record.message_delivery_replays.len(),
            record.completion_assignment_fence_fingerprints.len(),
        );
        let mut validated = record
            .into_record(recent_limit)
            .ok_or_else(|| error("invalid Workflow Session persisted state"))?;
        if evidence_counts
            != (
                validated.events.len(),
                validated.messages.len(),
                validated.message_delivery_replays.len(),
                validated.completion_assignment_fence_fingerprints.len(),
            )
        {
            return Err(error("invalid Session evidence or replay metadata"));
        }
        validated.history_is_durable = true;
        validated.event_history_start = history_start;
        Ok(PersistedSessionRecord::from_record(
            &validated,
            recent_limit,
        ))
    }

    /// Import once, with an all-or-nothing marker in the same transaction. The
    /// source is never overwritten or treated as a second live persistence path.
    pub(crate) fn import_legacy(&self, path: &Path) -> io::Result<()> {
        let mut connection = self.connection.lock().map_err(error)?;
        let imported: bool = connection
            .query_row(
                "SELECT imported FROM workflow_session_storage WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(error)?;
        if imported {
            return Ok(());
        }
        let transaction = connection.transaction().map_err(error)?;
        match fs::File::open(path) {
            Ok(file) => {
                super::stream::visit(io::BufReader::new(file), |row| {
                    match row {
                        super::stream::ImportRow::Session(record) => {
                            let events = record.events.clone();
                            write_row(&transaction, &SessionWrite { revision: 1, expected_revision: None, previous_message_revision: 0, record, events }).map(|_| ())
                        }
                        super::stream::ImportRow::Tombstone(tombstone) => {
                            transaction.execute("INSERT INTO workflow_session_tombstones(session_id,payload) VALUES(?1,?2)", params![tombstone.session_id, serde_json::to_string(&tombstone).map_err(error)?]).map_err(error)?;
                            Ok(())
                        }
                    }
                }).map_err(error)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        transaction
            .execute(
                "UPDATE workflow_session_storage SET imported=1 WHERE singleton=1",
                [],
            )
            .map_err(error)?;
        transaction.commit().map_err(error)
    }
}

fn read_events(
    connection: &Connection,
    session_id: &str,
    limit: usize,
) -> io::Result<Vec<Arc<SessionEvent>>> {
    let mut statement = connection.prepare(
        "SELECT payload,event_id,kind,call_id FROM (SELECT sequence,payload,event_id,kind,call_id FROM workflow_session_events WHERE session_id=?1 ORDER BY sequence DESC LIMIT ?2) ORDER BY sequence",
    ).map_err(error)?;
    let rows = statement
        .query_map(
            params![session_id, limit.max(1).min(i64::MAX as usize) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .map_err(error)?;
    rows.map(|row| {
        let (payload, id, kind, call_id) = row.map_err(error)?;
        let event: SessionEvent = serde_json::from_str(&payload).map_err(error)?;
        if event.session_id != session_id
            || event.event_id != id
            || event.kind != kind
            || event.call_id != call_id
        {
            return Err(error("Session event identity mismatch"));
        }
        Ok(Arc::new(event))
    })
    .collect()
}

fn write_row(connection: &Connection, write: &SessionWrite) -> io::Result<u64> {
    // Serialize the typed state with history detached. Never rewrite another
    // Session or encode all historical events to append one activity.
    let state = state_only(&write.record);
    let state = serde_json::to_string(&state).map_err(error)?;
    let record = &write.record;
    let actual: Option<i64> = connection
        .query_row(
            "SELECT revision FROM workflow_sessions WHERE session_id=?1",
            [&record.session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(error)?;
    let expected = write
        .expected_revision
        .map(i64::try_from)
        .transpose()
        .map_err(error)?;
    if actual != expected {
        return Err(error("Session database revision conflict"));
    }
    connection.execute(
        "INSERT INTO workflow_sessions(session_id,project,owner,lifecycle,created_at,updated_at,revision,state_json)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(session_id) DO UPDATE SET lifecycle=excluded.lifecycle,updated_at=excluded.updated_at,
             revision=excluded.revision,state_json=excluded.state_json
         WHERE workflow_sessions.owner=excluded.owner AND workflow_sessions.project IS excluded.project
             AND workflow_sessions.created_at=excluded.created_at",
        params![record.session_id,record.project,record.owner_authority_fingerprint,record.lifecycle.as_str(),record.created_at,record.updated_at,i64::try_from(write.revision).map_err(error)?,state],
    ).map_err(error)?;
    if connection.changes() != 1 {
        return Err(error("Session identity mismatch"));
    }
    let mut bytes = state.len() as u64;
    let last: Option<i64> = connection
        .query_row(
            "SELECT max(ordinal) FROM workflow_session_events WHERE session_id=?1",
            [&record.session_id],
            |row| row.get(0),
        )
        .map_err(error)?;
    let mut ordinal = match last {
        Some(last) => last
            .checked_add(1)
            .ok_or_else(|| error("Session event sequence exhausted"))?,
        None => i64::try_from(record.event_history_start).map_err(error)?,
    };
    for event in &write.events {
        if event.session_id != record.session_id {
            return Err(error("Session event identity mismatch"));
        }
        let existing: Option<(String, i64)> = connection
            .query_row(
                "SELECT session_id,ordinal FROM workflow_session_events WHERE event_id=?1",
                [&event.event_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(error)?;
        if existing
            .as_ref()
            .is_some_and(|(id, _)| id != &record.session_id)
        {
            return Err(error("event id collision"));
        }
        let event_ordinal = existing.as_ref().map_or(ordinal, |(_, ordinal)| *ordinal);
        if existing.is_none() {
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| error("Session event sequence exhausted"))?;
        }
        let payload = serde_json::to_string(event).map_err(error)?;
        connection.execute(
            "INSERT INTO workflow_session_events(event_id,session_id,kind,call_id,payload,ordinal) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(event_id) DO UPDATE SET payload=excluded.payload WHERE workflow_session_events.session_id=excluded.session_id AND workflow_session_events.payload<>excluded.payload",
            params![event.event_id,event.session_id,event.kind,event.call_id,payload,event_ordinal],
        ).map_err(error)?;
        if connection.changes() > 0 {
            bytes = bytes.saturating_add(payload.len() as u64);
        }
    }
    for message in &record.messages {
        let revision = record
            .message_observation_revisions
            .get(&message.message_id)
            .copied()
            .unwrap_or(0);
        if write.expected_revision.is_some() && revision <= write.previous_message_revision {
            continue;
        }
        if message.session_id != record.session_id {
            return Err(error("Session message identity mismatch"));
        }
        let existing: Option<String> = connection
            .query_row(
                "SELECT session_id FROM workflow_session_messages WHERE message_id=?1",
                [&message.message_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(error)?;
        if existing.as_ref().is_some_and(|id| id != &record.session_id) {
            return Err(error("Session message id collision"));
        }
        let payload = serde_json::to_string(message).map_err(error)?;
        connection.execute(
            "INSERT INTO workflow_session_messages(message_id,session_id,revision,payload,assignment_floor,completion_fence) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(message_id) DO UPDATE SET revision=excluded.revision,payload=excluded.payload,assignment_floor=excluded.assignment_floor,completion_fence=excluded.completion_fence
             WHERE workflow_session_messages.session_id=excluded.session_id AND (workflow_session_messages.revision<>excluded.revision OR workflow_session_messages.payload<>excluded.payload OR workflow_session_messages.assignment_floor IS NOT excluded.assignment_floor OR workflow_session_messages.completion_fence IS NOT excluded.completion_fence)",
            params![message.message_id,record.session_id,i64::try_from(revision).map_err(error)?,payload, record.assignment_history_floors.get(&message.message_id).copied().map(i64::try_from).transpose().map_err(error)?, record.completion_assignment_fence_fingerprints.get(&message.message_id)],
        ).map_err(error)?;
        if connection.changes() > 0 {
            bytes = bytes.saturating_add(payload.len() as u64);
        }
    }
    for (scope, replay) in &record.message_delivery_replays {
        if write.expected_revision.is_some()
            && record
                .message_observation_revisions
                .get(&replay.message_id)
                .copied()
                .unwrap_or(0)
                <= write.previous_message_revision
        {
            continue;
        }
        connection.execute("INSERT INTO workflow_session_deliveries(session_id,scope_key,message_id,fingerprint) VALUES(?1,?2,?3,?4) ON CONFLICT(session_id,scope_key) DO NOTHING", params![record.session_id,scope,replay.message_id,replay.payload_fingerprint]).map_err(error)?;
        let matches: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workflow_session_deliveries d JOIN workflow_session_messages m ON m.message_id=d.message_id WHERE d.session_id=?1 AND d.scope_key=?2 AND d.message_id=?3 AND d.fingerprint=?4 AND m.session_id=d.session_id)", params![record.session_id,scope,replay.message_id,replay.payload_fingerprint], |row| row.get(0)).map_err(error)?;
        if !matches {
            return Err(error("Session delivery identity conflict"));
        }
    }
    Ok(bytes)
}

fn state_only(record: &PersistedSessionRecord) -> PersistedSessionRecord {
    let mut state = record.clone();
    state.events.clear();
    state.messages.clear();
    state.message_delivery_replays.clear();
    state.message_observation_revisions.clear();
    state.assignment_history_floors.clear();
    state.completion_assignment_fence_fingerprints.clear();
    state
}

// Indexed authority and JSON projections must agree. Never admit a second
// identity, or accidentally reintroduce an embedded history in current-state RAM.
fn state_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(u64, PersistedSessionRecord)> {
    let decode = || -> io::Result<_> {
        let revision = u64::try_from(row.get::<_, i64>(0).map_err(error)?).map_err(error)?;
        let state: String = row.get(1).map_err(error)?;
        let record: PersistedSessionRecord = serde_json::from_str(&state).map_err(error)?;
        if record.session_id != row.get::<_, String>(2).map_err(error)?
            || record.project != row.get::<_, Option<String>>(3).map_err(error)?
            || record.owner_authority_fingerprint != row.get::<_, String>(4).map_err(error)?
            || record.lifecycle.as_str() != row.get::<_, String>(5).map_err(error)?
            || record.created_at != row.get::<_, i64>(6).map_err(error)?
            || record.updated_at != row.get::<_, i64>(7).map_err(error)?
            || !is_valid_session_id(&record.session_id)
            || !is_lower_hex_sha256(&record.owner_authority_fingerprint)
            || record.event_history_start > record.events_observed
            || !record.events.is_empty()
            || !record.messages.is_empty()
            || !record.message_delivery_replays.is_empty()
            || !record.message_observation_revisions.is_empty()
            || !record.assignment_history_floors.is_empty()
            || !record.completion_assignment_fence_fingerprints.is_empty()
        {
            return Err(error("invalid normalized Session state"));
        }
        Ok((revision, record))
    };
    decode().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })
}
