use super::super::{SessionPersistence, SessionStore};
use super::*;

fn fixture() -> (tempfile::TempDir, SessionStore) {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = SessionStore::new(16, 20);
    store.inner.lock().unwrap().persistence = Some(SessionPersistence {
        path: tmp.path().join("sessions.json"),
        restored_sessions: 0,
        last_persist_error: None,
    });
    store.writer = LedgerWriterGuard::spawn_with_window(
        store.inner.clone(),
        store.persistence_write_mutex.clone(),
        Duration::from_secs(60),
    );
    (tmp, store)
}

#[test]
fn persistence_observation_never_flushes_marks_dirty_or_exposes_paths() {
    let (tmp, store) = fixture();
    store.start_session(None, None);
    store.start_session(None, None);
    let before = store.persistence_observation();
    assert_eq!(before.mode, "background");
    assert_eq!(before.ledger_observation, "unavailable");
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
    assert_eq!(observed.ledger_observation, "available");
    assert_eq!(
        observed.ledger_bytes,
        Some(
            std::fs::metadata(tmp.path().join("sessions.json"))
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
    assert!(attempt.written_bytes.unwrap() > 0);
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

#[test]
fn persistence_observation_distinguishes_failed_attempt_from_committed_bytes() {
    let (tmp, store) = fixture();
    let blocker = tmp.path().join("blocked");
    std::fs::write(&blocker, "file").unwrap();
    store
        .inner
        .lock()
        .unwrap()
        .persistence
        .as_mut()
        .unwrap()
        .path = blocker.join("sessions.json");
    let id = store.start_session(None, None).session_id;
    store.flush_persistence();
    let observed = store.persistence_observation();
    assert!(observed.ledger_bytes.is_none());
    let writer = observed.writer.unwrap();
    assert_eq!(writer.write_attempts, 1);
    assert_eq!(writer.failed_attempts, 1);
    assert_eq!(
        writer.pending_notifications, 0,
        "pending=0 is not durability success"
    );
    let attempt = writer.last_attempt.unwrap();
    assert!(!attempt.success);
    assert!(attempt.written_bytes.is_none());
    assert!(store.contains_session(&id));
    std::fs::remove_file(&blocker).unwrap();
    assert!(store.persist_after_mutation_durable().is_ok());
    let recovered = store.persistence_observation().writer.unwrap();
    assert_eq!(recovered.failed_attempts, 1);
    assert!(recovered.last_attempt.unwrap().success);
}

#[test]
fn persistence_observation_memory_and_sync_fallback_are_explicit() {
    let store = SessionStore::new(1, 10);
    let observed = store.persistence_observation();
    assert_eq!(observed.mode, "memory");
    assert_eq!(observed.ledger_observation, "disabled");
    assert!(observed.writer.is_none());
    assert!(observed.ledger_bytes.is_none());
    let tmp = tempfile::tempdir().unwrap();
    store.inner.lock().unwrap().persistence = Some(SessionPersistence {
        path: tmp.path().join("sessions.json"),
        restored_sessions: 0,
        last_persist_error: None,
    });
    store.start_session(None, None);
    let observed = store.persistence_observation();
    assert_eq!(observed.mode, "synchronous_fallback");
    assert!(observed.ledger_bytes.unwrap() > 0);
    assert!(observed.writer.is_none());
}
