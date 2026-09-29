use super::*;
use std::collections::VecDeque;

#[test]
fn recency_matches_reference_order_and_closed_eligibility() {
    let mut index = SessionRecency::default();
    let mut reference = VecDeque::<(String, bool)>::new();
    let mut random = 17_u64;
    for step in 0..10_000 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let id = format!("session-{}", (random >> 32) % 97);
        let closed = step % 3 == 0;
        reference.retain(|(candidate, _)| candidate != &id);
        if step % 7 == 0 {
            index.remove(&id);
        } else {
            index.touch(&id, closed);
            reference.push_back((id, closed));
        }
        assert_eq!(
            index.iter().cloned().collect::<Vec<_>>(),
            reference
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            index.closed_count(),
            reference.iter().filter(|(_, closed)| *closed).count()
        );
        assert_eq!(
            index.oldest_closed(),
            reference
                .iter()
                .find(|(_, closed)| *closed)
                .map(|(id, _)| id.as_str())
        );
        assert_eq!(index.positions.len(), reference.len());
        assert_eq!(index.ordered.len(), reference.len());
    }
}

#[test]
fn recency_counter_rollover_keeps_order_and_lifecycle_partition() {
    let mut index = SessionRecency::default();
    index.touch("active", false);
    index.touch("older-closed", true);
    index.touch("newer-closed", true);
    index.next = u64::MAX;
    index.touch("older-closed", true);
    assert_eq!(
        index.iter().map(String::as_str).collect::<Vec<_>>(),
        ["active", "newer-closed", "older-closed"]
    );
    assert_eq!(index.oldest_closed(), Some("newer-closed"));
    assert_eq!(index.closed_count(), 2);
    index.remove("newer-closed");
    assert_eq!(index.oldest_closed(), Some("older-closed"));
    index.touch("active", false);
    assert_eq!(index.closed_count(), 1);
}

#[test]
fn session_retention_uses_access_order_only_among_closed_identities() {
    use super::super::{SessionLifecycle, SessionStore};
    let store = SessionStore::new_in_memory_with_limits(1, 2, 8);
    let active = store.start_session(None, None).session_id;
    let first = store.start_session(None, None).session_id;
    let second = store.start_session(None, None).session_id;
    store.close_session(&first).unwrap();
    store.close_session(&second).unwrap();
    let before = store.summary(&first, Some(8)).unwrap();
    // Reading older Closed history makes it more recent, without reopening it
    // or generating events. Active identities never compete for Closed quota.
    assert_eq!(
        store.summary(&first, Some(8)).unwrap().events_total,
        before.events_total
    );
    let third = store.start_session(None, None).session_id;
    store.close_session(&third).unwrap();
    assert!(store.contains_session(&first));
    assert!(!store.contains_session(&second));
    assert_eq!(
        store.lifecycle_state(&active),
        Some(SessionLifecycle::Active)
    );
    assert_eq!(
        store.lifecycle_state(&first),
        Some(SessionLifecycle::Closed)
    );
    let inner = store.inner.lock().unwrap();
    assert_eq!(inner.lru.closed_count(), 2);
    assert_eq!(inner.lru.iter().count(), inner.sessions.len());
}

#[test]
fn unknown_query_and_repeated_close_do_not_create_recency_entries() {
    use super::super::SessionStore;
    let store = SessionStore::new_in_memory_with_limits(1, 2, 8);
    let id = store.start_session(None, None).session_id;
    store.close_session(&id).unwrap();
    store.close_session(&id).unwrap();
    assert!(store.summary("wc_sess_unknown", None).is_none());
    let inner = store.inner.lock().unwrap();
    assert_eq!(inner.lru.iter().cloned().collect::<Vec<_>>(), [id]);
    assert_eq!(inner.lru.closed_count(), 1);
}
