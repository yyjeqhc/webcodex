//! Disposable scale/recovery evidence. No production ledger or global env writes.
use super::*;
use serde_json::json;
use std::time::Instant;

fn timed<T>(run: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let value = run();
    (value, start.elapsed().as_secs_f64() * 1000.0)
}

fn fixture(count: usize) -> (SessionStore, Vec<String>) {
    let store = SessionStore::new_in_memory_with_limits(16, 16, 20);
    let ids = (0..count)
        .map(|index| {
            store
                .start_session(
                    Some("agent:scale:fixture".into()),
                    Some(format!("synthetic-{index}")),
                )
                .session_id
        })
        .collect();
    (store, ids)
}

fn scale_case(count: usize) -> Value {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("sessions.json");
    let ((store, ids), create_ms) = timed(|| fixture(count));
    let (ledger, snapshot_lock_ms) = timed(|| store.inner.lock().unwrap().to_persisted_ledger());
    let (_, serialize_replace_ms) = timed(|| write_ledger_atomic(&path, &ledger).unwrap());
    let ledger_bytes = std::fs::metadata(&path).unwrap().len();
    drop(ledger);
    drop(store);
    // Active identities are not evicted to meet a small hot-capacity target.
    let (restored, restore_ms) = timed(|| SessionStore::with_persistence_limits(&path, 16, 16, 20));
    assert_eq!(restored.status().active_sessions, count);
    assert_eq!(restored.status().capacity_evictions, 0);
    assert!(restored.status().last_persist_error.is_none());
    let (_, query_200_ms) = timed(|| {
        for index in 0..200 {
            assert!(restored.summary(&ids[index % count], Some(8)).is_some());
        }
    });
    let (_, one_close_flush_ms) = timed(|| {
        restored.close_session(&ids[0]).unwrap();
        restored.flush_persistence();
    });
    assert!(restored.status().last_persist_error.is_none());
    let after_transition_bytes = std::fs::metadata(&path).unwrap().len();
    let hot_sessions = restored.status().hot_sessions;
    let cold_sessions = restored.status().cold_sessions;
    drop(restored);
    let (reopened, recovery_ms) =
        timed(|| SessionStore::with_persistence_limits(&path, 16, 16, 20));
    assert!(reopened.status().last_persist_error.is_none());
    assert_eq!(
        reopened.lifecycle_state(&ids[0]),
        Some(SessionLifecycle::Closed)
    );
    for id in &ids[1..] {
        assert_eq!(reopened.lifecycle_state(id), Some(SessionLifecycle::Active));
    }
    assert_eq!(reopened.status().retained_sessions, count);
    json!({
        "sessions": count, "create_ms": create_ms, "snapshot_lock_ms": snapshot_lock_ms,
        "serialize_replace_ms": serialize_replace_ms, "restore_ms": restore_ms,
        "query_200_ms": query_200_ms, "one_close_flush_ms": one_close_flush_ms,
        "recovery_ms": recovery_ms, "ledger_bytes": ledger_bytes,
        "full_ledger_bytes_after_one_close": after_transition_bytes,
        "hot_sessions_after_close": hot_sessions, "cold_sessions_after_close": cold_sessions,
        "recovery": "exact_identities_and_closed_state_preserved",
        "scope": "synthetic_minimal_sessions_not_production_load_or_power_loss"
    })
}

#[test]
fn session_store_scale_smoke_preserves_identities_and_lifecycle() {
    let metrics = scale_case(4);
    assert_eq!(metrics["hot_sessions_after_close"], 3);
    assert_eq!(metrics["cold_sessions_after_close"], 1);
    assert!(metrics["ledger_bytes"].as_u64().unwrap() > 0);
}

#[test]
#[ignore = "manual retained-Session scale experiment; temporary files, run serially"]
fn session_store_scale_and_recovery() {
    for count in [100, 1_000, 10_000] {
        println!("SESSION_SCALE {}", scale_case(count));
    }
}

#[test]
fn session_store_scale_corrupt_copy_is_explicitly_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("truncated.json");
    let (store, ids) = fixture(4);
    let ledger = store.inner.lock().unwrap().to_persisted_ledger();
    let bytes = serde_json::to_vec(&ledger).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    let recovered = SessionStore::with_persistence_limits(&path, 16, 16, 20);
    assert!(recovered.status().last_persist_error.is_some());
    assert!(ids.iter().all(|id| !recovered.contains_session(id)));
    // An unsuccessful load does not replace the damaged file during observation.
    assert_eq!(std::fs::read(&path).unwrap(), &bytes[..bytes.len() / 2]);
}
