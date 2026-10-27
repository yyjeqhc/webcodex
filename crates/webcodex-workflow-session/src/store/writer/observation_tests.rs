use super::super::SessionStore;
use super::*;

fn fixture() -> (tempfile::TempDir, SessionStore) {
    super::tests::persistent_fixture(Duration::from_secs(60))
}

#[test]
fn persistence_observation_never_flushes_marks_dirty_or_exposes_paths() {
    let (tmp, store) = fixture();
    store.start_session(None, None);
    store.start_session(None, None);
    let before = store.persistence_observation();
    assert_eq!(before.mode, "sqlite");
    assert_eq!(before.storage_observation, "available");
    let writer = before.writer.unwrap();
    assert_eq!(writer.pending_notifications, 2);
    assert_eq!(writer.write_attempts, 0);
    for _ in 0..8 {
        let value = serde_json::to_value(store.persistence_observation()).unwrap();
        assert!(!value.to_string().contains(tmp.path().to_str().unwrap()));
    }
    assert!(!tmp.path().join("sessions.json").exists());
    store.flush_persistence();
    let observed = store.persistence_observation();
    assert_eq!(observed.storage_observation, "available");
    assert_eq!(
        observed.database_bytes,
        Some(
            std::fs::metadata(tmp.path().join("sessions.sqlite3"))
                .unwrap()
                .len()
        )
    );
    let writer = observed.writer.unwrap();
    assert_eq!(writer.write_attempts, 1);
    assert_eq!(writer.failed_attempts, 0);
    assert_eq!(writer.pending_notifications, 0);
    let attempt = writer.last_attempt.unwrap();
    assert!(attempt.success);
    assert_eq!(attempt.dirty_notifications, 2);
    assert!(attempt.logical_payload_bytes.unwrap() > 0);
    assert!(attempt.snapshot_lock_hold_ms >= 0.0);
    assert_eq!(
        store
            .persistence_observation()
            .writer
            .unwrap()
            .write_attempts,
        1
    );
}

#[tokio::test]
async fn persistence_observation_distinguishes_failed_attempt_from_committed_bytes() {
    let (tmp, store) = fixture();
    let connection = rusqlite::Connection::open(tmp.path().join("sessions.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_session_write BEFORE INSERT ON workflow_sessions BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    let id = store.start_session(None, None).session_id;
    store.flush_persistence();
    let observed = store.persistence_observation();
    assert!(observed.database_bytes.is_some());
    let writer = observed.writer.unwrap();
    assert_eq!(writer.write_attempts, 1);
    assert_eq!(writer.failed_attempts, 1);
    assert_eq!(
        writer.pending_notifications, 1,
        "failed writes remain uncommitted"
    );
    let attempt = writer.last_attempt.unwrap();
    assert!(!attempt.success);
    assert!(attempt.logical_payload_bytes.is_none());
    assert!(store.contains_session(&id));
    connection
        .execute_batch("DROP TRIGGER fail_session_write")
        .unwrap();
    assert!(store.persist_after_mutation_durable().await.is_ok());
    let recovered = store.persistence_observation().writer.unwrap();
    assert_eq!(recovered.failed_attempts, 1);
    assert!(recovered.last_attempt.unwrap().success);
}

#[test]
fn persistence_observation_memory_and_sync_fallback_are_explicit() {
    let store = SessionStore::new(1, 10);
    let observed = store.persistence_observation();
    assert_eq!(observed.mode, "memory");
    assert_eq!(observed.storage_observation, "disabled");
    assert!(observed.writer.is_none());
    assert!(observed.database_bytes.is_none());
    let (_tmp, mut store) = fixture();
    drop(store.writer.take());
    store.start_session(None, None);
    let observed = store.persistence_observation();
    assert_eq!(observed.mode, "sqlite");
    assert!(observed.database_bytes.unwrap() > 0);
    assert!(observed.writer.is_none());
}
