use super::super::{SessionLifecycle, SessionPersistence, SessionStore};
use super::*;
use std::sync::mpsc;

fn persistent_fixture(window: Duration) -> (tempfile::TempDir, SessionStore) {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = SessionStore::new_in_memory_with_limits(16, 16, 20);
    store.inner.lock().unwrap().persistence = Some(SessionPersistence {
        path: temporary.path().join("sessions.json"),
        restored_sessions: 0,
        last_persist_error: None,
    });
    store.writer = LedgerWriterGuard::spawn_with_window(
        store.inner.clone(),
        store.persistence_write_mutex.clone(),
        window,
    );
    assert!(store.writer.is_some());
    (temporary, store)
}

#[test]
fn large_ledger_cost_is_bounded_and_never_extends_an_existing_dirty_deadline() {
    assert_eq!(
        cost_coalesce(1024, Duration::from_secs(10)),
        ORDINARY_COALESCE
    );
    assert_eq!(
        cost_coalesce(73 * 1024 * 1024, Duration::from_millis(100)),
        Duration::from_millis(400)
    );
    assert_eq!(
        cost_coalesce(73 * 1024 * 1024, Duration::from_secs(10)),
        MAX_COST_COALESCE
    );
    let now = Instant::now();
    let mut state = LedgerWriterState {
        next_cost: Duration::from_millis(400),
        ..Default::default()
    };
    state.mark_dirty(now);
    state.next_cost = MAX_COST_COALESCE;
    state.mark_dirty(now + Duration::from_millis(100));
    assert_eq!(
        state.coalescing_wait(now + Duration::from_millis(100), ORDINARY_COALESCE),
        Some(Duration::from_millis(300))
    );
    assert_eq!(
        state.coalescing_wait(now + Duration::from_millis(400), ORDINARY_COALESCE),
        None
    );
    state.flush_generation = state.dirty_generation;
    assert_eq!(state.coalescing_wait(now, ORDINARY_COALESCE), None);
    state.flush_generation = 0;
    state.shutdown = true;
    assert_eq!(state.coalescing_wait(now, ORDINARY_COALESCE), None);
    eprintln!("SESSION_WRITE_COALESCE small_ms=20 large_min_ms=250 large_max_ms=1000 durable_flush_bypasses=true fixed_first_mark=true");
}

#[test]
fn dirty_progress_never_extends_first_mark_deadline() {
    let now = Instant::now();
    let mut state = LedgerWriterState::default();
    state.mark_dirty(now);
    for elapsed in 1..20 {
        state.mark_dirty(now + Duration::from_millis(elapsed));
        assert_eq!(
            state.coalescing_wait(now + Duration::from_millis(elapsed), ORDINARY_COALESCE),
            Some(Duration::from_millis(20 - elapsed))
        );
    }
    assert_eq!(state.dirty_generation, 20);
    assert_eq!(
        state.coalescing_wait(now + ORDINARY_COALESCE, ORDINARY_COALESCE),
        None
    );
    assert_eq!(
        state.coalescing_wait(now + Duration::from_secs(1), ORDINARY_COALESCE),
        None
    );
}

#[test]
fn flush_urgency_and_inflight_dirty_marks_keep_exact_generation_fences() {
    let now = Instant::now();
    let mut state = LedgerWriterState::default();
    let first = state.mark_dirty(now);
    state.flush_generation = first;
    assert_eq!(state.coalescing_wait(now, ORDINARY_COALESCE), None);
    state.dirty_since = None; // writer owns the first snapshot generation
    let second = state.mark_dirty(now + Duration::from_millis(1));
    state.writes_completed = first;
    assert!(state.writes_completed < second);
    assert_eq!(
        state.coalescing_wait(now + Duration::from_millis(2), ORDINARY_COALESCE),
        Some(Duration::from_millis(19))
    );
    // A flusher for first is done even with a newer pending generation.
    assert!(state.writes_completed >= first);
    state.flush_generation = second;
    assert_eq!(
        state.coalescing_wait(now + Duration::from_millis(2), ORDINARY_COALESCE),
        None
    );
    state.flush_generation = first;
    state.shutdown = true;
    assert_eq!(
        state.coalescing_wait(now + Duration::from_millis(2), ORDINARY_COALESCE),
        None
    );
}

#[test]
fn explicit_flush_coalesces_burst_and_preserves_all_retained_identities() {
    // A long injected window eliminates reliance on scheduler speed. The flush
    // must interrupt it, not sleep until it expires. Small ledgers use 20ms; large ledgers use a bounded cost-aware window.
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

#[test]
fn failed_durable_write_is_reported_and_later_mutation_can_recover() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let blocker = temporary.path().join("parent-is-file");
    std::fs::write(&blocker, b"block directory creation").unwrap();
    store
        .inner
        .lock()
        .unwrap()
        .persistence
        .as_mut()
        .unwrap()
        .path = blocker.join("sessions.json");
    let id = store.start_session(None, None).session_id;
    assert!(store.persist_after_mutation_durable().is_err());
    assert!(store.status().last_persist_error.is_some());
    std::fs::remove_file(&blocker).unwrap();
    assert!(store.persist_after_mutation_durable().is_ok());
    assert!(store.status().last_persist_error.is_none());
    let restored = SessionStore::with_persistence(blocker.join("sessions.json"), 1, 20);
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

#[test]
fn multiple_flush_waiters_and_new_dirty_generations_all_complete() {
    let (temporary, store) = persistent_fixture(Duration::from_secs(60));
    let mut joins = Vec::new();
    let (done, completed) = mpsc::channel();
    for _ in 0..8 {
        let worker = store.clone();
        let done = done.clone();
        joins.push(std::thread::spawn(move || {
            let id = worker.start_session(None, None).session_id;
            worker.persist_after_mutation_durable().unwrap();
            done.send(id).unwrap();
        }));
    }
    let ids: Vec<_> = (0..8)
        .map(|_| {
            completed
                .recv_timeout(Duration::from_secs(10))
                .expect("all flush waiters must wake")
        })
        .collect();
    for join in joins {
        join.join().unwrap();
    }
    drop(store);
    let restored = SessionStore::with_persistence(temporary.path().join("sessions.json"), 1, 20);
    assert!(ids.iter().all(|id| restored.contains_session(id)));
}

fn append_read_fixture(store: &SessionStore, id: &str, ordinal: usize) {
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

/// Same implementation and populated source data, only the ordinary scheduling
/// policy differs. Counts are completed full-file write cycles, not disk sectors.
#[test]
#[ignore = "manual populated-ledger write scheduling comparison; temporary files, serial"]
fn populated_ledger_write_coalescing_experiment() {
    use crate::{PostSessionMessageInput, SessionMessageKind, SessionMessagePriority};
    let seed = SessionStore::new_in_memory_with_limits(16, 16, 128);
    let ids: Vec<_> = (0..256)
        .map(|_| {
            seed.start_session(Some("agent:scale:populated".into()), None)
                .session_id
        })
        .collect();
    for id in &ids {
        for ordinal in 0..16 {
            append_read_fixture(&seed, id, ordinal);
        }
        for ordinal in 0..2 {
            seed.post_message(PostSessionMessageInput {
                session_id: id.clone(),
                kind: SessionMessageKind::Progress,
                message: format!(
                    "synthetic progress {ordinal}: {}",
                    "bounded fixture ".repeat(16)
                ),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        }
    }
    let ledger = seed.inner.lock().unwrap().to_persisted_ledger();
    let seed_bytes = serde_json::to_vec(&ledger).unwrap();
    drop(seed);
    for (label, window, cost_aware) in [
        ("immediate_reference", Duration::ZERO, false),
        ("fixed_20ms_reference", ORDINARY_COALESCE, false),
        ("cost_aware", ORDINARY_COALESCE, true),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("sessions.json");
        std::fs::write(&path, &seed_bytes).unwrap();
        let mut store = SessionStore::with_persistence_limits(&path, 16, 16, 128);
        // Replace only the idle fixture writer before any mutation. This is not
        // a production configuration or additional persistence implementation.
        store.writer = None;
        store.writer = LedgerWriterGuard::spawn_with_policy(
            store.inner.clone(),
            store.persistence_write_mutex.clone(),
            window,
            cost_aware,
        );
        let begin = Instant::now();
        for ordinal in 0..128 {
            append_read_fixture(&store, &ids[ordinal % ids.len()], 1000 + ordinal);
            std::thread::sleep(Duration::from_millis(5));
        }
        store.flush_persistence();
        let flush_ms = begin.elapsed().as_secs_f64() * 1000.0;
        let cycles = store.writer.as_ref().unwrap().write_cycles();
        assert!(store.status().last_persist_error.is_none());
        let final_bytes = std::fs::metadata(&path).unwrap().len();
        drop(store);
        let restored = SessionStore::with_persistence_limits(&path, 16, 16, 128);
        for (ordinal, id) in ids.iter().enumerate() {
            let summary = restored.summary(id, Some(128)).unwrap();
            assert_eq!(summary.lifecycle, SessionLifecycle::Active);
            // Each synthetic read has one start and one finish; messages may
            // have their own ledger events, so compare against the seed below.
            let original = ledger
                .sessions
                .iter()
                .filter_map(|row| row.hot())
                .find(|row| &row.session_id == id)
                .unwrap();
            assert_eq!(
                summary.events_total as u64,
                original.events_observed + if ordinal < 128 { 2 } else { 0 }
            );
        }
        println!(
            "SESSION_WRITE_BURST {}",
            serde_json::json!({
                "mode": label, "sessions": ids.len(), "seed_tool_events_per_session": 32,
                "seed_messages_per_session": 2, "additional_tool_calls": 128,
                "seed_ledger_bytes": seed_bytes.len(), "final_ledger_bytes": final_bytes,
                "full_file_write_cycles": cycles, "mutate_and_flush_ms": flush_ms,
                "scope": "synthetic_populated_ledger_unoptimized_single_run_not_disk_bytes_or_power_loss"
            })
        );
    }
}
