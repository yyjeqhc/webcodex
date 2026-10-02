//! Bounded retention tombstones for Closed Workflow Sessions.

use std::path::Path;

use serde_json::{json, Value};
use webcodex_core::workflow_session_contract::{SessionMode, SESSION_ID_PREFIX};

use crate::{
    workflow_session_incarnation_fingerprint, CodingSessionError, CodingSessionRequest,
    SessionCloseError, SessionGuards, SessionLifecycle, SessionStore, SessionTransport,
    TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
};

const FOREIGN_OWNER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn close_new(store: &SessionStore, title: &str) -> String {
    let summary = store.start_session(
        Some("agent:test:retention".to_string()),
        Some(title.to_string()),
    );
    store.close_session(&summary.session_id).unwrap();
    summary.session_id
}

fn coding_request(resume: Option<String>, owner: &str) -> CodingSessionRequest {
    CodingSessionRequest {
        project: "agent:test:retention".to_string(),
        authority_fingerprint: owner.to_string(),
        resume_session_id: resume,
        instruction: None,
        mode: SessionMode::Normal,
        guards: SessionGuards::default(),
        execution_context: None,
        project_instructions: None,
        transport: SessionTransport::Api,
        context_refreshed: false,
        write_scope_verified: false,
    }
}

fn read_ledger(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_ledger(path: &Path, ledger: &Value) {
    std::fs::write(path, serde_json::to_string(ledger).unwrap()).unwrap();
}

fn canonical_id(n: u8) -> String {
    format!("wc_sess_{n:032x}")
}

fn tombstone_row(session_id: &str, ordinal: u64) -> Value {
    tombstone_row_with(session_id, ordinal, &"b".repeat(64))
}

fn tombstone_row_with(session_id: &str, ordinal: u64, incarnation: &str) -> Value {
    json!({
        "retention_tombstone": {
            "session_id": session_id,
            "owner_authority_fingerprint": TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
            "incarnation_fingerprint": incarnation,
            "expiry_ordinal": ordinal
        }
    })
}

#[test]
fn historical_retention_keeps_a_tombstone_and_the_newer_closed_session() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let oldest = close_new(&store, "oldest");
    let oldest_summary = store.summary(&oldest, Some(5)).unwrap();
    let (project, owner) = store.session_target_authority(&oldest).unwrap();
    let retained = close_new(&store, "retained");

    assert!(!store.contains_session(&oldest));
    assert!(store.contains_session(&retained));
    let retained_summary = store.summary(&retained, Some(5)).unwrap();
    assert_eq!(retained_summary.lifecycle, SessionLifecycle::Closed);
    assert_eq!(retained_summary.title.as_deref(), Some("retained"));

    let tombstone = store.retention_tombstone_for_test(&oldest).unwrap();
    assert_eq!(tombstone.session_id, oldest);
    assert_eq!(tombstone.owner_authority_fingerprint, owner);
    assert_eq!(tombstone.expiry_ordinal, 1);
    assert_eq!(
        tombstone.incarnation_fingerprint,
        workflow_session_incarnation_fingerprint(
            &oldest,
            oldest_summary.created_at,
            project.as_deref(),
            &owner,
        )
    );
    let status = store.status();
    assert_eq!(status.retained_sessions, 1);
    assert_eq!(status.closed_sessions, 1);
    assert_eq!(status.retention_tombstones, 1);
    assert_eq!(status.capacity_evictions, 1);
    assert_eq!(status.hot_sessions, 0);
    assert_eq!(status.cold_sessions, 1);
}

#[test]
fn tombstone_payload_omits_session_body() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence_limits(&path, 10, 1, 10);
    let oldest = close_new(&store, "body");
    let _retained = close_new(&store, "kept");
    store.flush_persistence();

    let ledger = read_ledger(&path);
    let row = ledger["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("retention_tombstone").is_some())
        .expect("ledger keeps the tombstone row");
    assert_eq!(row.as_object().unwrap().len(), 1);
    let tombstone = row["retention_tombstone"].as_object().unwrap();
    let keys = tombstone
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        [
            "expiry_ordinal",
            "incarnation_fingerprint",
            "owner_authority_fingerprint",
            "session_id",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    assert_eq!(tombstone["session_id"], oldest);
    let encoded = serde_json::to_string(tombstone).unwrap();
    for forbidden in [
        "messages",
        "events",
        "title",
        "instruction",
        "project",
        "mode",
        "guards",
        "created_at",
    ] {
        assert!(
            !encoded.contains(&format!("\"{forbidden}\"")),
            "tombstone leaked {forbidden}"
        );
    }
}

#[test]
fn never_created_id_has_no_tombstone() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let _ = close_new(&store, "oldest");
    let _ = close_new(&store, "retained");
    let missing = canonical_id(9);
    assert!(store.retention_tombstone_for_test(&missing).is_none());
    assert!(store
        .owned_retention_incarnation(&missing, TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT)
        .is_none());
    assert!(!store.contains_session(&missing));
}

#[test]
fn tombstone_survives_flush_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let expired = {
        let store = SessionStore::with_persistence_limits(&path, 10, 1, 10);
        let expired = close_new(&store, "expired");
        let _retained = close_new(&store, "retained");
        store.flush_persistence();
        expired
    };
    let restored = SessionStore::with_persistence_limits(&path, 10, 1, 10);
    let tombstone = restored.retention_tombstone_for_test(&expired).unwrap();
    assert_eq!(tombstone.expiry_ordinal, 1);
    assert_eq!(
        restored
            .owned_retention_incarnation(&expired, TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT)
            .as_deref(),
        Some(tombstone.incarnation_fingerprint.as_str())
    );
    assert!(restored
        .owned_retention_incarnation(&expired, FOREIGN_OWNER)
        .is_none());
    assert!(restored.summary(&expired, Some(1)).is_none());
}

#[test]
fn tombstone_ceiling_replaces_the_oldest_ordinal_without_another_eviction() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let first = close_new(&store, "first");
    let second = close_new(&store, "second");
    let third = close_new(&store, "third");

    assert!(store.retention_tombstone_for_test(&first).is_none());
    let tombstone = store.retention_tombstone_for_test(&second).unwrap();
    assert_eq!(tombstone.expiry_ordinal, 2);
    assert!(store.contains_session(&third));
    assert!(!store.contains_session(&second));
    assert_eq!(store.status().retention_tombstones, 1);
    assert_eq!(store.status().capacity_evictions, 2);
}

#[test]
fn restored_expiry_ordinal_continues_above_the_greatest_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let (expired_a, expired_b, expired_c) = {
        let store = SessionStore::with_persistence_limits(&path, 10, 2, 10);
        let expired_a = close_new(&store, "a");
        let expired_b = close_new(&store, "b");
        let retained_c = close_new(&store, "c");
        assert_eq!(
            store
                .retention_tombstone_for_test(&expired_a)
                .unwrap()
                .expiry_ordinal,
            1
        );
        assert!(store.contains_session(&expired_b));
        assert!(store.contains_session(&retained_c));
        let retained_d = close_new(&store, "d");
        assert_eq!(
            store
                .retention_tombstone_for_test(&expired_b)
                .unwrap()
                .expiry_ordinal,
            2
        );
        assert!(store.contains_session(&retained_c));
        assert!(store.contains_session(&retained_d));
        store.flush_persistence();
        (expired_a, expired_b, retained_c)
    };

    let store = SessionStore::with_persistence_limits(&path, 10, 2, 10);
    assert_eq!(
        store
            .retention_tombstone_for_test(&expired_a)
            .unwrap()
            .expiry_ordinal,
        1
    );
    assert_eq!(
        store
            .retention_tombstone_for_test(&expired_b)
            .unwrap()
            .expiry_ordinal,
        2
    );
    let _retained_e = close_new(&store, "e");
    assert!(store.retention_tombstone_for_test(&expired_a).is_none());
    assert_eq!(
        store
            .retention_tombstone_for_test(&expired_b)
            .unwrap()
            .expiry_ordinal,
        2
    );
    assert_eq!(
        store
            .retention_tombstone_for_test(&expired_c)
            .unwrap()
            .expiry_ordinal,
        3
    );
    assert!(!store.contains_session(&expired_c));
    assert_eq!(store.status().capacity_evictions, 1);
    assert_eq!(store.status().retention_tombstones, 2);
}

#[test]
fn active_capacity_churn_creates_no_tombstone() {
    let store = SessionStore::new(1, 10);
    let mut ids = Vec::new();
    for index in 0..4 {
        ids.push(
            store
                .start_session(None, Some(format!("active {index}")))
                .session_id,
        );
    }
    for id in &ids {
        assert!(store.contains_session(id));
        assert_eq!(store.lifecycle_state(id), Some(SessionLifecycle::Active));
        assert!(store.retention_tombstone_for_test(id).is_none());
    }
    let status = store.status();
    assert_eq!(status.retention_tombstones, 0);
    assert_eq!(status.capacity_evictions, 0);
    assert_eq!(status.active_sessions, 4);
    assert_eq!(status.closed_sessions, 0);
}

#[test]
fn retained_closed_session_stays_queryable() {
    let store = SessionStore::new_in_memory_with_limits(10, 2, 10);
    let closed = close_new(&store, "still here");
    let summary = store.summary(&closed, Some(5)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Closed);
    assert!(store.retention_tombstone_for_test(&closed).is_none());
    assert_eq!(store.status().retention_tombstones, 0);
    assert_eq!(store.status().closed_sessions, 1);
}

#[test]
fn limit_zero_removes_the_closed_row_and_keeps_no_tombstone() {
    let store = SessionStore::new_in_memory_with_limits(10, 0, 10);
    let removed = close_new(&store, "gone");
    assert!(!store.contains_session(&removed));
    assert!(store.retention_tombstone_for_test(&removed).is_none());
    assert_eq!(store.status().retention_tombstones, 0);
    assert_eq!(store.status().capacity_evictions, 1);
    assert_eq!(store.status().closed_sessions, 0);
}

#[test]
fn live_session_wins_over_a_same_id_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let session_id = {
        let store = SessionStore::with_persistence_limits(&path, 10, 10, 10);
        let summary = store.start_session(
            Some("agent:test:retention".to_string()),
            Some("live".to_string()),
        );
        store.flush_persistence();
        summary.session_id
    };
    let mut ledger = read_ledger(&path);
    ledger["sessions"]
        .as_array_mut()
        .unwrap()
        .push(tombstone_row(&session_id, 7));
    write_ledger(&path, &ledger);

    let restored = SessionStore::with_persistence_limits(&path, 10, 10, 10);
    assert!(restored.contains_session(&session_id));
    assert!(restored.retention_tombstone_for_test(&session_id).is_none());
    assert_eq!(restored.status().retention_tombstones, 0);
    assert!(restored.status().last_persist_error.is_none());
    assert_eq!(
        restored.lifecycle_state(&session_id),
        Some(SessionLifecycle::Active)
    );
}

#[test]
fn malformed_tombstone_drops_only_that_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let session_id = {
        let store = SessionStore::with_persistence_limits(&path, 10, 10, 10);
        let summary = store.start_session(None, Some("neighbor".to_string()));
        store.flush_persistence();
        summary.session_id
    };
    let mut ledger = read_ledger(&path);
    ledger["sessions"].as_array_mut().unwrap().push(json!({
        "retention_tombstone": {
            "session_id": "not-a-session",
            "owner_authority_fingerprint": TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
            "incarnation_fingerprint": "b".repeat(64),
            "expiry_ordinal": 1,
            "title": "leak"
        }
    }));
    ledger["sessions"].as_array_mut().unwrap().push(json!({
        "session_id": canonical_id(4),
        "lifecycle": "closed"
    }));
    write_ledger(&path, &ledger);

    let restored = SessionStore::with_persistence_limits(&path, 10, 10, 10);
    assert!(restored.contains_session(&session_id));
    assert!(restored
        .retention_tombstone_for_test(&canonical_id(4))
        .is_none());
    assert_eq!(restored.status().retention_tombstones, 0);
    assert!(restored.status().last_persist_error.is_none());
    assert_eq!(
        restored
            .summary(&session_id, Some(1))
            .unwrap()
            .title
            .as_deref(),
        Some("neighbor")
    );
}

#[test]
fn legacy_v2_ledger_without_tombstones_restores_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let session_id = {
        let store = SessionStore::with_persistence_limits(&path, 10, 10, 10);
        let session_id = close_new(&store, "legacy");
        store.flush_persistence();
        session_id
    };
    let ledger = read_ledger(&path);
    assert!(ledger["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row.get("retention_tombstone").is_none()));
    let restored = SessionStore::with_persistence_limits(&path, 10, 10, 10);
    assert_eq!(restored.status().retention_tombstones, 0);
    assert_eq!(
        restored.summary(&session_id, Some(1)).unwrap().lifecycle,
        SessionLifecycle::Closed
    );
}

#[test]
fn duplicate_ordinals_fail_closed_and_the_next_ordinal_stays_above_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let first = canonical_id(1);
    let second = canonical_id(2);
    let third = canonical_id(3);
    write_ledger(
        &path,
        &json!({
            "version": 2,
            "sessions": [
                tombstone_row(&first, 5),
                tombstone_row(&second, 5),
                tombstone_row(&first, 4),
                tombstone_row(&third, 9),
            ]
        }),
    );
    let store = SessionStore::with_persistence_limits(&path, 10, 10, 10);
    assert_eq!(
        store
            .retention_tombstone_for_test(&first)
            .unwrap()
            .expiry_ordinal,
        5
    );
    assert!(store.retention_tombstone_for_test(&second).is_none());
    assert_eq!(
        store
            .retention_tombstone_for_test(&third)
            .unwrap()
            .expiry_ordinal,
        9
    );
    store.flush_persistence();
    drop(store);

    let store = SessionStore::with_persistence_limits(&path, 10, 1, 10);
    assert!(store.retention_tombstone_for_test(&first).is_none());
    assert_eq!(
        store
            .retention_tombstone_for_test(&third)
            .unwrap()
            .expiry_ordinal,
        9
    );
    let removed = close_new(&store, "later");
    let _kept = close_new(&store, "kept");
    assert_eq!(
        store
            .retention_tombstone_for_test(&removed)
            .unwrap()
            .expiry_ordinal,
        10
    );
    assert!(store.retention_tombstone_for_test(&third).is_none());
}

#[test]
fn restored_max_ordinal_compacts_in_order_and_still_allocates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let oldest = canonical_id(1);
    let newer = canonical_id(2);
    let oldest_incarnation = "a".repeat(64);
    let newer_incarnation = "c".repeat(64);
    write_ledger(
        &path,
        &json!({
            "version": 2,
            "sessions": [
                tombstone_row_with(&oldest, 10, &oldest_incarnation),
                tombstone_row_with(&newer, u64::MAX, &newer_incarnation),
            ]
        }),
    );
    let store = SessionStore::with_persistence_limits(&path, 10, 2, 10);
    let restored_oldest = store.retention_tombstone_for_test(&oldest).unwrap();
    let restored_newer = store.retention_tombstone_for_test(&newer).unwrap();
    assert_eq!(restored_oldest.expiry_ordinal, 10);
    assert_eq!(restored_newer.expiry_ordinal, u64::MAX);
    assert_eq!(restored_oldest.incarnation_fingerprint, oldest_incarnation);
    assert_eq!(
        restored_newer.owner_authority_fingerprint,
        restored_oldest.owner_authority_fingerprint
    );

    // Limit 2 keeps the first two Closed rows. The third close removes the
    // oldest Closed row and must still be able to mint its tombstone.
    let removed = close_new(&store, "removed");
    let oldest_closed = close_new(&store, "oldest closed");
    let _newer_closed = close_new(&store, "newer closed");
    assert!(store.retention_tombstone_for_test(&oldest).is_none());
    assert!(store.contains_session(&oldest_closed));
    let kept = store.retention_tombstone_for_test(&newer).unwrap();
    let fresh = store.retention_tombstone_for_test(&removed).unwrap();
    assert_eq!(kept.incarnation_fingerprint, newer_incarnation);
    assert_eq!(
        kept.owner_authority_fingerprint,
        restored_newer.owner_authority_fingerprint
    );
    assert_eq!(kept.session_id, newer);
    assert_eq!(fresh.session_id, removed);
    assert!(kept.expiry_ordinal < fresh.expiry_ordinal);
    assert_ne!(fresh.expiry_ordinal, 0);
    assert!(fresh.expiry_ordinal < u64::MAX);
    assert_eq!(store.status().retention_tombstones, 2);
    assert_eq!(store.status().capacity_evictions, 1);
    store.flush_persistence();
    let ledger = read_ledger(&path);
    assert_eq!(ledger["version"], 2);
    assert!(ledger.get("retention_tombstones").is_none());

    let reloaded = SessionStore::with_persistence_limits(&path, 10, 2, 10);
    assert_eq!(
        reloaded
            .retention_tombstone_for_test(&newer)
            .unwrap()
            .expiry_ordinal,
        kept.expiry_ordinal
    );
    assert_eq!(
        reloaded
            .retention_tombstone_for_test(&removed)
            .unwrap()
            .incarnation_fingerprint,
        fresh.incarnation_fingerprint
    );
    let _trigger = close_new(&reloaded, "trigger");
    assert!(reloaded.retention_tombstone_for_test(&newer).is_none());
    let survived = reloaded.retention_tombstone_for_test(&removed).unwrap();
    let successor = reloaded
        .retention_tombstone_for_test(&oldest_closed)
        .unwrap();
    assert_eq!(survived.expiry_ordinal, fresh.expiry_ordinal);
    assert_eq!(
        survived.incarnation_fingerprint,
        fresh.incarnation_fingerprint
    );
    assert!(survived.expiry_ordinal < successor.expiry_ordinal);
    assert_eq!(reloaded.status().retention_tombstones, 2);
}

#[test]
fn restored_near_max_ordinal_allocates_once_then_compacts_without_inversion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let oldest = canonical_id(3);
    let oldest_incarnation = "d".repeat(64);
    write_ledger(
        &path,
        &json!({
            "version": 2,
            "sessions": [tombstone_row_with(&oldest, u64::MAX - 1, &oldest_incarnation)]
        }),
    );
    let store = SessionStore::with_persistence_limits(&path, 10, 2, 10);
    assert_eq!(
        store
            .retention_tombstone_for_test(&oldest)
            .unwrap()
            .expiry_ordinal,
        u64::MAX - 1
    );

    let first_evicted = close_new(&store, "first evicted");
    let second_evicted = close_new(&store, "second evicted");
    let _retained = close_new(&store, "retained");
    let first = store.retention_tombstone_for_test(&first_evicted).unwrap();
    assert_eq!(
        store
            .retention_tombstone_for_test(&oldest)
            .unwrap()
            .expiry_ordinal,
        u64::MAX - 1
    );
    assert_eq!(first.expiry_ordinal, u64::MAX);
    assert_eq!(store.status().retention_tombstones, 2);

    let _another_retained = close_new(&store, "another retained");
    assert!(store.retention_tombstone_for_test(&oldest).is_none());
    let first_after = store.retention_tombstone_for_test(&first_evicted).unwrap();
    let second = store.retention_tombstone_for_test(&second_evicted).unwrap();
    assert_eq!(
        first_after.incarnation_fingerprint,
        first.incarnation_fingerprint
    );
    assert_eq!(first_after.session_id, first_evicted);
    assert!(first_after.expiry_ordinal < second.expiry_ordinal);
    assert!(second.expiry_ordinal < u64::MAX);
    assert_ne!(first_after.expiry_ordinal, 0);
    assert_eq!(store.status().retention_tombstones, 2);
    assert_eq!(store.status().capacity_evictions, 2);
}

#[test]
fn unprovable_session_row_is_not_invented_as_a_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let (good, bad) = {
        let store = SessionStore::with_persistence_limits(&path, 10, 10, 10);
        let good = store
            .start_session(None, Some("good".to_string()))
            .session_id;
        let bad = close_new(&store, "bad");
        store.flush_persistence();
        (good, bad)
    };
    let mut ledger = read_ledger(&path);
    let bad_row = ledger["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["session_id"] == bad)
        .unwrap();
    bad_row["owner_authority_fingerprint"] = json!("not-a-fingerprint");
    write_ledger(&path, &ledger);

    let restored = SessionStore::with_persistence_limits(&path, 10, 0, 10);
    assert!(restored.contains_session(&good));
    assert!(!restored.contains_session(&bad));
    assert!(restored.retention_tombstone_for_test(&bad).is_none());
    assert!(restored.status().last_persist_error.is_none());
}

#[test]
fn tombstone_occupies_its_canonical_id_until_it_ages_out() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let occupied = close_new(&store, "occupied");
    let suffix = occupied
        .strip_prefix(SESSION_ID_PREFIX)
        .expect("canonical prefix");
    let _retained = close_new(&store, "retained");
    assert!(!store.contains_session(&occupied));
    assert!(store.retention_tombstone_for_test(&occupied).is_some());
    assert!(store
        .allocate_fixed_session_suffix_for_test(suffix)
        .is_none());
    let freed = close_new(&store, "frees the first tombstone");
    assert!(store.retention_tombstone_for_test(&occupied).is_none());
    assert_eq!(
        store
            .allocate_fixed_session_suffix_for_test(suffix)
            .as_deref(),
        Some(occupied.as_str())
    );
    assert!(store.contains_session(&freed));
}

#[test]
fn explicit_resume_of_an_expired_id_does_not_create_a_replacement() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let expired = close_new(&store, "expired");
    let _retained = close_new(&store, "retained");
    let before = store.status().retained_sessions;

    let expired_resume = store
        .ensure_coding_session(coding_request(
            Some(expired.clone()),
            TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
        ))
        .unwrap_err();
    assert_eq!(
        expired_resume,
        CodingSessionError::ResumeRetentionExpired {
            session_id: expired.clone(),
        }
    );
    assert_eq!(store.status().retained_sessions, before);

    let foreign = store
        .ensure_coding_session(coding_request(Some(expired.clone()), FOREIGN_OWNER))
        .unwrap_err();
    assert_eq!(
        foreign,
        CodingSessionError::UnknownResumeSession {
            session_id: expired.clone(),
        }
    );
    assert_eq!(store.status().retained_sessions, before);

    let created = store
        .ensure_coding_session(coding_request(
            None,
            TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
        ))
        .unwrap();
    assert_ne!(created.summary.session_id, expired);
    assert!(!created.reused);
    assert_eq!(store.status().retained_sessions, before + 1);
    assert!(store.contains_session(&created.summary.session_id));
}

#[test]
fn retained_closed_resume_stays_closed_and_close_of_a_tombstone_does_not_resurrect_it() {
    let store = SessionStore::new_in_memory_with_limits(10, 1, 10);
    let expired = close_new(&store, "expired");
    let retained = close_new(&store, "retained");
    let error = store
        .ensure_coding_session(coding_request(
            Some(retained.clone()),
            TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        CodingSessionError::ResumeSessionNotActive {
            lifecycle: SessionLifecycle::Closed,
            ..
        }
    ));
    assert_eq!(
        store.close_session(&expired).unwrap_err(),
        SessionCloseError::UnknownSession
    );
    assert!(store.retention_tombstone_for_test(&expired).is_some());
    assert_eq!(
        store.lifecycle_state(&retained),
        Some(SessionLifecycle::Closed)
    );
}
