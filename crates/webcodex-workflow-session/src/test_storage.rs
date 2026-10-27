//! Fixtures for the retired JSON input format. Runtime round trips use the real
//! SQLite store; corrupt JSON fixtures explicitly import into a fresh database.
use crate::{model::PersistedSessionLedger, SessionStore};
use std::{io, path::Path};

pub fn read_ledger(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();
    if path.exists() {
        return std::fs::read_to_string(path);
    }
    let database =
        crate::persistence::database::SessionDatabase::open(&path.with_extension("sqlite3"))?;
    let mut rows = Vec::new();
    for cold in database.inventory()? {
        rows.push(crate::model::PersistedSessionSnapshot::Hot(
            database.load(&cold, usize::MAX)?,
        ));
    }
    for tombstone in database.tombstones()? {
        rows.push(crate::model::PersistedSessionSnapshot::Tombstone(
            crate::model::PersistedRetentionTombstoneRow {
                retention_tombstone: tombstone,
            },
        ));
    }
    serde_json::to_string(&PersistedSessionLedger {
        version: crate::model::SESSION_LEDGER_VERSION,
        sessions: rows,
    })
    .map_err(io::Error::other)
}

pub fn read_ledger_bytes(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    read_ledger(path).map(String::into_bytes)
}

pub fn import_ledger(path: impl AsRef<Path>, _hot: usize, recent: usize) -> SessionStore {
    let path = path.as_ref();
    SessionStore::open_database(
        path.with_file_name(format!("import-{}.sqlite3", uuid::Uuid::new_v4().simple())),
        Some(path),
        crate::model::DEFAULT_SESSION_CACHE_BYTES,
        recent,
    )
    .unwrap()
}

#[cfg(test)]
pub fn fail_writes(store: &SessionStore, fail: bool) {
    store.set_persistence_failure_for_test(fail);
}

#[cfg(test)]
pub fn assert_import_rejected(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let source = std::fs::read(path).unwrap();
    let database = path.with_file_name(format!(
        "rejected-{}.sqlite3",
        uuid::Uuid::new_v4().simple()
    ));
    assert!(SessionStore::open_database(&database, Some(path), 10, 20).is_err());
    let connection = rusqlite::Connection::open(database).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM workflow_sessions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        count, 0,
        "invalid migration must not publish valid neighboring rows either"
    );
    let imported: bool = connection
        .query_row(
            "SELECT imported FROM workflow_session_storage WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!imported);
    assert_eq!(std::fs::read(path).unwrap(), source);
}
