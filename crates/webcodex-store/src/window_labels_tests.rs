use crate::window_activity::tests::{append, event, seed_session};
use crate::{Database, WindowInventoryQuery};

#[test]
fn window_labels_normalize_bound_unicode_and_preserve_noop_timestamp() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("labels.db")).unwrap();
    let principal = ("username", "alice");
    let label = "窗".repeat(80);
    assert_eq!(
        db.set_window_label(principal, "key", Some(&format!("  {label}  ")), 10)
            .unwrap(),
        Some(label.clone())
    );
    db.set_window_label(principal, "key", Some(&label), 20)
        .unwrap();
    let timestamp: i64 = db
        .conn_for_tests()
        .query_row("SELECT updated_at_ms FROM window_labels", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timestamp, 10);
    for invalid in [
        "窗".repeat(81),
        "bad\nname".into(),
        "bad\u{007f}name".into(),
        "bad\u{0085}name".into(),
    ] {
        assert!(db
            .set_window_label(principal, "key", Some(&invalid), 30)
            .is_err());
        assert_eq!(
            db.get_window_label(principal, "key").unwrap(),
            Some(label.clone())
        );
    }
    assert_eq!(
        db.set_window_label(principal, "key", Some("  "), 40)
            .unwrap(),
        None
    );
    assert_eq!(db.get_window_label(principal, "key").unwrap(), None);
    db.set_window_label(principal, "key", Some("restored"), 50)
        .unwrap();
    db.clear_window_label(principal, "key").unwrap();
    db.clear_window_label(principal, "key").unwrap();
    assert_eq!(db.get_window_label(principal, "key").unwrap(), None);
}

#[test]
fn window_labels_inventory_search_is_principal_scoped_and_retention_removes_only_expired_labels() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("labels.db");
    let db = Database::open(&path).unwrap();
    seed_session(&db);
    append(&db, event("a", "shared", "alice", "agent:r:p", 1000), &[]);
    append(&db, event("b", "shared", "bob", "agent:r:p", 2000), &[]);
    append(&db, event("c", "other", "alice", "agent:r:p", 3000), &[]);
    for (principal, key, name) in [
        ("alice", "shared", "Research"),
        ("bob", "shared", "Review"),
        ("alice", "other", "Keep"),
    ] {
        db.set_window_label(("username", principal), key, Some(name), 1)
            .unwrap();
    }
    assert_eq!(
        db.get_window_label(("username", "alice"), "shared")
            .unwrap()
            .as_deref(),
        Some("Research")
    );
    assert_eq!(
        db.get_window_label(("username", "bob"), "shared")
            .unwrap()
            .as_deref(),
        Some("Review")
    );
    let read = |principal, query| {
        db.read_window_inventory(WindowInventoryQuery {
            principal: Some(("username", principal)),
            caller: None,
            managed_oauth_identity: None,
            management: false,
            visible_projects: &["agent:r:p".into()],
            projects: None,
            window_key: None,
            query,
            live: &[],
            offset: 0,
            limit: 10,
        })
        .unwrap()
    };
    let page = read("alice", "reSEARCH");
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].label.as_deref(), Some("Research"));
    assert_eq!(read("bob", "Research").total, 0);
    db.conn_for_tests()
        .execute("DELETE FROM action_events WHERE event_id='a'", [])
        .unwrap();
    let page = read("bob", "Review");
    assert_eq!(page.total, 1);
    assert_eq!(
        db.get_window_label(("username", "alice"), "shared")
            .unwrap(),
        None
    );
    assert_eq!(
        db.get_window_label(("username", "bob"), "shared")
            .unwrap()
            .as_deref(),
        Some("Review")
    );
    assert_eq!(
        db.get_window_label(("username", "alice"), "other")
            .unwrap()
            .as_deref(),
        Some("Keep")
    );
    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .get_window_label(("username", "bob"), "shared")
            .unwrap()
            .as_deref(),
        Some("Review")
    );
}
