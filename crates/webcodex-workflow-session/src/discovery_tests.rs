use crate::{SessionCreateOptions, SessionLifecycle, SessionStore};
use webcodex_core::workflow_session_contract::SessionMode;

fn create(store: &SessionStore, project: &str, owner: &str, title: &str) -> String {
    store
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.to_string()),
                Some(title.to_string()),
                SessionMode::Normal,
                Default::default(),
            )
            .with_owner_authority_fingerprint(Some(owner.to_string())),
        )
        .unwrap()
        .session_id
}

#[test]
fn discovery_filters_authority_before_counting_and_pagination() {
    let store = SessionStore::new_in_memory(1, 10);
    let owner = "a".repeat(64);
    let foreign = "b".repeat(64);
    for _ in 0..25 {
        create(&store, "project", &foreign, "foreign secret title");
    }
    let ids = (0..4)
        .map(|index| create(&store, "project", &owner, &format!("task {index}")))
        .collect::<Vec<_>>();
    create(&store, "other-project", &owner, "unrelated");
    let all = store.discover_sessions("project", &owner, None, 0, 1000);
    assert_eq!(all.total, 4);
    assert_eq!(all.sessions.len(), 4);
    assert_eq!(all.next_offset, None);
    assert!(all.sessions.iter().all(|row| ids.contains(&row.session_id)));
    let first = store.discover_sessions("project", &owner, None, 0, 2);
    let second = store.discover_sessions("project", &owner, None, 2, 2);
    assert_eq!(first.next_offset, Some(2));
    assert_eq!(second.next_offset, None);
    assert_eq!([first.sessions, second.sessions].concat(), all.sessions);
    let past = store.discover_sessions("project", &owner, None, usize::MAX, usize::MAX);
    assert!(past.sessions.is_empty());
    assert_eq!(past.total, 4);
    assert_eq!(past.next_offset, None);
    let absent = store.discover_sessions("missing", &owner, None, 0, 10);
    assert_eq!(absent.total, 0);
    assert!(absent.sessions.is_empty());
}

#[test]
fn discovery_redacts_bounds_titles_and_caps_inventory_rows() {
    let store = SessionStore::new_in_memory(100, 10);
    let owner = "a".repeat(64);
    let secret = create(
        &store,
        "project",
        &owner,
        "Authorization: Bearer super-secret-value",
    );
    let long = create(&store, "project", &owner, &"界".repeat(1000));
    let page = store.discover_sessions("project", &owner, None, 0, 10);
    let secret_row = page
        .sessions
        .iter()
        .find(|row| row.session_id == secret)
        .unwrap();
    assert_eq!(secret_row.title.as_deref(), Some("[redacted]"));
    assert!(secret_row.title_truncated);
    let long_row = page
        .sessions
        .iter()
        .find(|row| row.session_id == long)
        .unwrap();
    assert_eq!(long_row.title.as_ref().unwrap().chars().count(), 240);
    assert!(long_row.title_truncated);
    for _ in 0..25 {
        create(&store, "project", &owner, "ordinary");
    }
    let capped = store.discover_sessions("project", &owner, None, 0, usize::MAX);
    assert_eq!(capped.sessions.len(), 20);
    assert_eq!(capped.next_offset, Some(20));
    assert_eq!(
        store
            .discover_sessions("project", &owner, None, 0, 0)
            .sessions
            .len(),
        1
    );
}

#[test]
fn discovery_preserves_closed_cold_history_and_restart_order() {
    let root = tempfile::tempdir().unwrap();
    let ledger = root.path().join("sessions.json");
    let owner = "a".repeat(64);
    let store = SessionStore::with_persistence(ledger.clone(), 1, 10);
    let active = create(&store, "project", &owner, "active");
    let closed = create(&store, "project", &owner, "closed");
    store.close_session(&closed).unwrap();
    assert_eq!(store.status().cold_sessions, 1);
    let before = store.discover_sessions("project", &owner, None, 0, 10);
    let closed_page =
        store.discover_sessions("project", &owner, Some(SessionLifecycle::Closed), 0, 10);
    assert_eq!(closed_page.total, 1);
    assert_eq!(closed_page.sessions[0].session_id, closed);
    assert_eq!(
        store.status().cold_sessions,
        1,
        "discovery must not warm Closed identities"
    );
    let active_page =
        store.discover_sessions("project", &owner, Some(SessionLifecycle::Active), 0, 10);
    assert_eq!(active_page.total, 1);
    assert_eq!(active_page.sessions[0].session_id, active);
    store.flush_persistence();
    let restored = SessionStore::with_persistence(ledger, 1, 10);
    assert_eq!(
        restored.discover_sessions("project", &owner, None, 0, 10),
        before
    );
    assert_eq!(restored.status().cold_sessions, 1);
    assert_eq!(
        restored
            .discover_sessions("project", &"b".repeat(64), None, 0, 10)
            .total,
        0
    );
}
