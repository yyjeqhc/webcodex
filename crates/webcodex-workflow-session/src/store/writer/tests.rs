use super::super::{SessionLifecycle, SessionStore};
use super::*;
use std::sync::mpsc;

pub(super) fn persistent_fixture(window: Duration) -> (tempfile::TempDir, SessionStore) {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = SessionStore::with_persistence(temporary.path().join("sessions.json"), 16, 20);
    drop(store.writer.take());
    store.writer = LedgerWriterGuard::spawn_with_window(
        store.inner.clone(),
        store.persistence_write_mutex.clone(),
        window,
    );
    assert!(store.writer.is_some());
    (temporary, store)
}

#[test]
fn explicit_flush_coalesces_burst_and_preserves_all_retained_identities() {
    // A long injected window eliminates reliance on scheduler speed. The flush
    // must interrupt it, not sleep until it expires. Ordinary batches use 20 ms.
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let ids: Vec<_> = (0..64)
        .map(|_| store.start_session(None, None).session_id)
        .collect();
    let (done, completed) = mpsc::channel();
    let worker = store.clone();
    let join = std::thread::spawn(move || {
        worker.flush_persistence();
        done.send(()).unwrap();
    });
    completed
        .recv_timeout(Duration::from_secs(10))
        .expect("flush must bypass coalescing");
    join.join().unwrap();
    assert_eq!(store.writer.as_ref().unwrap().write_cycles(), 1);
    assert!(store.status().last_persist_error.is_none());
    drop(store);
    let restored =
        SessionStore::with_persistence_limits(temporary.path().join("sessions.json"), 1, 16, 20);
    for id in &ids {
        assert_eq!(restored.lifecycle_state(id), Some(SessionLifecycle::Active));
    }
    assert_eq!(restored.status().retained_sessions, ids.len());
}

#[test]
fn final_owner_drop_drains_pending_data_without_coalescing_delay() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    let clone = store.clone();
    drop(store);
    let (done, completed) = mpsc::channel();
    let join = std::thread::spawn(move || {
        drop(clone);
        done.send(()).unwrap();
    });
    completed
        .recv_timeout(Duration::from_secs(10))
        .expect("shutdown must bypass coalescing");
    join.join().unwrap();
    let restored = SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert!(restored.contains_session(&id));
}

#[tokio::test]
async fn failed_durable_write_is_reported_and_later_mutation_can_recover() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let connection = rusqlite::Connection::open(temporary.path().join("sessions.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_session_write BEFORE INSERT ON workflow_sessions BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
    let id = store.start_session(None, None).session_id;
    assert!(store.persist_after_mutation_durable().await.is_err());
    assert!(store.status().last_persist_error.is_some());
    connection
        .execute_batch("DROP TRIGGER fail_session_write")
        .unwrap();
    assert!(store.persist_after_mutation_durable().await.is_ok());
    assert!(store.status().last_persist_error.is_none());
    let restored = SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert!(restored.contains_session(&id));
}

#[test]
fn exact_queries_do_not_schedule_persistence_or_change_event_history() {
    let (_temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    store.flush_persistence();
    let events = store.summary(&id, Some(8)).unwrap().events_total;
    let cycles = store.writer.as_ref().unwrap().write_cycles();
    for _ in 0..100 {
        assert_eq!(store.summary(&id, Some(8)).unwrap().events_total, events);
    }
    store.flush_persistence();
    assert_eq!(store.writer.as_ref().unwrap().write_cycles(), cycles);
}

#[tokio::test]
async fn multiple_flush_waiters_and_new_dirty_generations_all_complete() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let mut joins = Vec::new();
    for _ in 0..8 {
        let worker = store.clone();
        joins.push(tokio::spawn(async move {
            let id = worker.start_session(None, None).session_id;
            worker.persist_after_mutation_durable().await.unwrap();
            id
        }));
    }
    let mut ids = Vec::new();
    for join in joins {
        ids.push(
            tokio::time::timeout(Duration::from_secs(10), join)
                .await
                .expect("all flush waiters must wake")
                .unwrap(),
        );
    }
    drop(store);
    let restored = SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert!(ids.iter().all(|id| restored.contains_session(id)));
}

pub(super) fn append_read_fixture(store: &SessionStore, id: &str, ordinal: usize) {
    use crate::{SessionPathHint, SessionToolContract, SessionTransport};
    let started = store.record_tool_call_started(
        Some(id), SessionTransport::Api, "read_files",
        &serde_json::json!({"project": "agent:scale:populated", "items": [{"path": format!("src/file-{ordinal}.rs"), "limit": 8}]}),
        SessionToolContract { risk_class: "read", read_like: true, write_like: false,
            shell_like: false, git_like: false, change_summary_like: false,
            project_write: false, path_hint: SessionPathHint::None },
    );
    assert!(started.is_some());
    store.record_tool_call_finished(started, true, &serde_json::json!({"requested_count": 1, "returned_count": 1, "failed_count": 0, "items": []}), None, None);
}
