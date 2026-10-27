use super::tests::{append_read_fixture, persistent_fixture};
use super::*;
use crate::{
    CodingSessionRequest, SessionGuards, SessionTransport,
    TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
};
use webcodex_core::workflow_session_contract::SessionMode;

#[test]
fn long_history_is_not_rewritten_and_cold_entries_do_not_own_it() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    for n in 0..5_100 {
        append_read_fixture(&store, &id, n);
    }
    store.flush_persistence();
    let connection = rusqlite::Connection::open(temporary.path().join("sessions.sqlite3")).unwrap();
    let before: i64 = connection
        .query_row(
            "SELECT count(*) FROM workflow_session_events WHERE session_id=?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, 10_200);
    let protected = connection
        .query_row(
            "SELECT sum(length(payload)) FROM workflow_session_events",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    connection.execute_batch("CREATE TRIGGER forbid_history_update BEFORE UPDATE ON workflow_session_events BEGIN SELECT RAISE(ABORT, 'unrelated history rewritten'); END;").unwrap();
    append_read_fixture(&store, &id, 6_000);
    store.flush_persistence();
    assert!(store.status().last_persist_error.is_none());
    let attempt = store
        .persistence_observation()
        .writer
        .unwrap()
        .last_attempt
        .unwrap();
    assert!(
        attempt.logical_payload_bytes.unwrap() < 20_000,
        "{:?}",
        attempt.logical_payload_bytes
    );
    assert!(protected > 1_000_000);
    {
        let mut inner = store.inner.lock().unwrap();
        let record = inner.sessions.get(&id).unwrap().hot().unwrap();
        assert!(record.pending_events.is_empty());
        let cold = record.durable.clone().unwrap();
        assert!(cold.raw.get().len() < 4_000);
        inner
            .sessions
            .replace_residency(id.clone(), super::super::StoredSession::Cold(cold));
    }
    assert!(store.cold_payload_bytes_for_test(&id).unwrap() < 4_000);
    let summary = store.summary(&id, Some(5)).unwrap();
    assert_eq!(summary.events_total, 10_202);
    assert_eq!(summary.events.len(), 5);
    drop(store);
    let restored =
        super::super::SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert_eq!(restored.summary(&id, Some(5)).unwrap().events_total, 10_202);
    assert_eq!(restored.status().hot_sessions, 0);
}

#[tokio::test]
async fn durable_observation_of_unchanged_state_does_not_write() {
    let (_temporary, store) = persistent_fixture(Duration::from_secs(60));
    store.start_session(None, None);
    store.flush().await.unwrap();
    let before = store
        .persistence_observation()
        .writer
        .unwrap()
        .write_attempts;
    for _ in 0..100 {
        store.flush().await.unwrap();
    }
    assert_eq!(
        store
            .persistence_observation()
            .writer
            .unwrap()
            .write_attempts,
        before
    );
}

#[test]
fn latest_instruction_and_closed_identity_survive_history_and_session_churn() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let request = CodingSessionRequest {
        project: "agent:scale:populated".into(),
        authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.into(),
        resume_session_id: None,
        instruction: Some("Keep this instruction across ten thousand operations".into()),
        mode: SessionMode::Normal,
        guards: SessionGuards::default(),
        execution_context: None,
        project_instructions: None,
        transport: SessionTransport::Api,
        context_refreshed: true,
        write_scope_verified: true,
    };
    let id = store
        .ensure_coding_session(request)
        .unwrap()
        .summary
        .session_id;
    for n in 0..80 {
        append_read_fixture(&store, &id, n);
    }
    store.close_session(&id).unwrap();
    for _ in 0..150 {
        let newer = store.start_session(None, None).session_id;
        store.close_session(&newer).unwrap();
    }
    store.flush_persistence();
    drop(store);
    let restored =
        super::super::SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    let summary = restored.summary(&id, Some(5)).unwrap();
    assert_eq!(summary.lifecycle, super::super::SessionLifecycle::Closed);
    assert_eq!(
        summary
            .current_instruction
            .as_ref()
            .unwrap()
            .instruction
            .as_deref(),
        Some("Keep this instruction across ten thousand operations")
    );
    assert_eq!(restored.status().retained_sessions, 151);
}

#[test]
fn stale_second_writer_fails_closed_instead_of_overwriting_a_newer_commit() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    store.flush_persistence();
    let stale =
        super::super::SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    append_read_fixture(&store, &id, 1);
    store.flush_persistence();
    // The stale cache refuses hydration of a different disk version.
    assert!(stale.summary(&id, Some(5)).is_none());
    assert!(stale.contains_session(&id));
    assert!(stale.session_created_at(&id).is_some());
    assert_eq!(store.summary(&id, Some(5)).unwrap().events_total, 2);
}

#[test]
fn mutation_after_snapshot_remains_dirty_until_its_own_commit() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    append_read_fixture(&store, &id, 1);
    let captured = store.inner.lock().unwrap().sessions.generation;
    let (generation, committed, _, _) =
        persist_batch_observed(&store.inner, &store.persistence_write_mutex, |writes| {
            assert_eq!(writes[0].events.len(), 2);
            // Deterministically interleave a later mutation after releasing the
            // snapshot lock but before committing/acknowledging the earlier batch.
            append_read_fixture(&store, &id, 2);
            assert_eq!(writes[0].events.len(), 2);
        });
    assert_eq!(generation, captured);
    committed.unwrap();
    let connection = rusqlite::Connection::open(temporary.path().join("sessions.sqlite3")).unwrap();
    let count = || {
        connection
            .query_row("SELECT count(*) FROM workflow_session_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    assert_eq!(count(), 2);
    {
        let inner = store.inner.lock().unwrap();
        assert!(inner.sessions.dirty[&id] > captured);
        assert_eq!(
            inner
                .sessions
                .get(&id)
                .unwrap()
                .hot()
                .unwrap()
                .pending_events
                .len(),
            2
        );
    }
    store.flush_persistence();
    assert_eq!(count(), 4);
    assert!(store.inner.lock().unwrap().sessions.dirty.is_empty());
    drop(store);
    let restored =
        super::super::SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert_eq!(restored.summary(&id, Some(8)).unwrap().events_total, 4);
}

#[tokio::test]
async fn a_failed_multi_session_transaction_publishes_nothing_and_can_retry() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let mut ids = [
        store.start_session(None, None).session_id,
        store.start_session(None, None).session_id,
    ];
    ids.sort();
    for id in &ids {
        append_read_fixture(&store, id, 1);
    }
    let connection = rusqlite::Connection::open(temporary.path().join("sessions.sqlite3")).unwrap();
    connection.execute_batch(&format!("CREATE TRIGGER fail_second BEFORE INSERT ON workflow_sessions WHEN NEW.session_id='{}' BEGIN SELECT RAISE(ABORT, 'injected second row failure'); END;", ids[1])).unwrap();
    assert!(store.flush().await.is_err());
    for table in ["workflow_sessions", "workflow_session_events"] {
        assert_eq!(
            connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(store.inner.lock().unwrap().sessions.dirty.len(), 2);
    connection
        .execute_batch("DROP TRIGGER fail_second")
        .unwrap();
    store.flush().await.unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM workflow_session_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        4
    );
}

#[tokio::test]
async fn message_delivery_identity_survives_five_hundred_newer_messages_and_restart() {
    use crate::{
        ListSessionMessagesFilter, PostSessionMessageInput, SessionMessageDelivery,
        SessionMessageKind, SessionMessagePriority,
    };
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let id = store.start_session(None, None).session_id;
    let input = PostSessionMessageInput {
        session_id: id.clone(),
        kind: SessionMessageKind::Note,
        message: "original durable note".into(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    };
    let delivery = || {
        Some(SessionMessageDelivery {
            sender_scope: "a".repeat(64),
            delivery_key: "stable-first-delivery".into(),
        })
    };
    let first = store
        .post_message_with_ack_and_delivery(input.clone(), false, delivery())
        .await
        .unwrap();
    for n in 0..500 {
        let mut newer = input.clone();
        newer.message = format!("newer note {n}");
        store.post_message(newer).unwrap();
    }
    store.flush().await.unwrap();
    drop(store);
    let restored =
        super::super::SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    let replay = restored
        .post_message_with_ack_and_delivery(input, false, delivery())
        .await
        .unwrap();
    assert!(replay.replayed);
    assert!(!replay.state_changed);
    assert_eq!(replay.message.message_id, first.message.message_id);
    let exact = restored
        .list_messages(
            &id,
            ListSessionMessagesFilter {
                message_id: Some(first.message.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(exact.len(), 1);
    let connection = rusqlite::Connection::open(temporary.path().join("sessions.sqlite3")).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM workflow_session_messages",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        501
    );
}

#[test]
fn import_is_atomic_once_and_preserves_the_original_source() {
    let temporary = tempfile::tempdir().unwrap();
    let legacy = temporary.path().join("sessions.json");
    let source = super::super::SessionStore::new(1, 20);
    let id = source.start_session(None, None).session_id;
    let original =
        serde_json::to_string(&source.inner.lock().unwrap().to_persisted_ledger()).unwrap();
    std::fs::write(&legacy, &original).unwrap();
    let store = super::super::SessionStore::with_persistence(&legacy, 1, 20);
    append_read_fixture(&store, &id, 1);
    store.flush_persistence();
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), original);
    drop(store);
    std::fs::write(&legacy, "now corrupt: must not be imported twice").unwrap();
    let restored = super::super::SessionStore::with_persistence(&legacy, 1, 20);
    assert_eq!(restored.summary(&id, Some(5)).unwrap().events_total, 2);
}

#[test]
fn mismatched_indexed_authority_is_rejected_on_restart() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    store.start_session(None, None);
    store.flush_persistence();
    drop(store);
    let path = temporary.path().join("sessions.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute("UPDATE workflow_sessions SET owner=?1", ["a".repeat(64)])
        .unwrap();
    assert!(super::super::SessionStore::open_database(&path, None, 1, 20).is_err());
}

#[test]
fn abrupt_process_exit_recovers_committed_wal_and_excludes_unconfirmed_queue() {
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "store::writer::sqlite_tests::abrupt_exit_child",
            "--ignored",
            "--nocapture",
        ])
        .env("WEBCODEX_SESSION_CRASH_FIXTURE", directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("committed-session:"))
        .expect("child confirmed its exact commit");
    let wal = directory.path().join("sessions.sqlite3-wal");
    assert!(
        std::fs::metadata(wal).unwrap().len() > 0,
        "child must exit without closing/checkpointing SQLite"
    );
    let store =
        super::super::SessionStore::with_persistence(directory.path().join("sessions.json"), 1, 20);
    let summary = store.summary(id, Some(10)).unwrap();
    assert_eq!(summary.session_id, id);
    assert_eq!(summary.events_total, 2);
    assert_eq!(summary.events.len(), 2);
    assert_eq!(store.status().retained_sessions, 1);
}

#[test]
#[ignore = "subprocess fixture; invoked only by abrupt_process_exit_recovers_committed_wal_and_excludes_unconfirmed_queue"]
fn abrupt_exit_child() {
    let path =
        std::env::var_os("WEBCODEX_SESSION_CRASH_FIXTURE").expect("isolated child fixture path");
    let mut store = super::super::SessionStore::with_persistence(
        std::path::PathBuf::from(path).join("sessions.json"),
        1,
        20,
    );
    drop(store.writer.take());
    store.writer = LedgerWriterGuard::spawn_with_window(
        store.inner.clone(),
        store.persistence_write_mutex.clone(),
        Duration::from_secs(60),
    );
    let id = store.start_session(None, None).session_id;
    append_read_fixture(&store, &id, 1);
    store.flush_persistence();
    assert!(store.status().last_persist_error.is_none());
    println!("committed-session:{id}");
    append_read_fixture(&store, &id, 2);
    // Intentionally skip all Rust destructors and the final Session writer drain.
    // This exercises process death, not a claim about hardware power-loss safety.
    std::process::exit(0);
}
