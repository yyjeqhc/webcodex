//! Old tombstones remain recognizable, but ordinary lifecycle/cache pressure no
//! longer creates or ages out Session identities.
use crate::*;
use serde_json::json;

#[test]
fn closed_identity_does_not_expire_at_zero_retention_and_cannot_be_reallocated() {
    let store = SessionStore::new_in_memory_with_limits(0, 0, 8);
    let first = store.start_session(None, None);
    store.close_session(&first.session_id).unwrap();
    for _ in 0..150 {
        let newer = store.start_session(None, None);
        store.close_session(&newer.session_id).unwrap();
    }
    assert_eq!(
        store.summary(&first.session_id, None).unwrap().lifecycle,
        SessionLifecycle::Closed
    );
    let suffix = first.session_id.strip_prefix("wc_sess_").unwrap();
    assert!(store
        .allocate_fixed_session_suffix_for_test(suffix)
        .is_none());
    assert_eq!(store.status().retention_tombstones, 0);
}

#[test]
fn imported_tombstone_is_durable_non_reusable_and_never_retargets() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("sessions.json");
    let id = "wc_sess_0123456789abcdef";
    let owner = "a".repeat(64);
    let incarnation = "b".repeat(64);
    std::fs::write(&legacy, serde_json::to_vec(&json!({"version":2,"sessions":[{
        "retention_tombstone": {"session_id":id,"owner_authority_fingerprint":owner,"incarnation_fingerprint":incarnation,"expiry_ordinal":u64::MAX}
    }]})).unwrap()).unwrap();
    let source = std::fs::read(&legacy).unwrap();
    let path = dir.path().join("webcodex.db");
    let store = SessionStore::open_database(&path, Some(&legacy), 1, 8).unwrap();
    for _ in 0..150 {
        let newer = store.start_session(None, None);
        store.close_session(&newer.session_id).unwrap();
    }
    store.flush_persistence();
    assert!(!store.contains_session(id));
    assert_eq!(
        store.owned_retention_incarnation(id, &owner),
        Some(incarnation.clone())
    );
    assert!(store
        .owned_retention_incarnation(id, &"c".repeat(64))
        .is_none());
    assert!(store
        .allocate_fixed_session_suffix_for_test("0123456789abcdef")
        .is_none());
    drop(store);
    let restored = SessionStore::open_database(&path, Some(&legacy), 1, 8).unwrap();
    assert_eq!(
        restored.owned_retention_incarnation(id, &owner),
        Some(incarnation)
    );
    assert_eq!(std::fs::read(&legacy).unwrap(), source);
}
