use super::*;

#[test]
fn window_event_links_remain_exact_ordered_and_page_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("links.db")).unwrap();
    seed_session(&db);
    append(
        &db,
        event("old", "w", "alice", "p", 1),
        &[("old-session", "recording")],
    );
    append(
        &db,
        event("linked", "w", "alice", "p", 2),
        &[
            ("z", "recording"),
            ("a", "recording"),
            ("b", "work_on_project"),
        ],
    );
    append(&db, event("empty", "w", "alice", "p", 3), &[]);
    append(
        &db,
        event("foreign", "w", "bob", "q", 4),
        &[("foreign-session", "recording")],
    );
    append(
        &db,
        event("other", "other", "alice", "p", 5),
        &[("other-session", "recording")],
    );
    for composition in [false, true] {
        let rows = if composition {
            db.list_window_activity_events_with_code_mode_composition(
                "w",
                Some(("username", "alice")),
                2,
            )
        } else {
            db.list_window_activity_events("w", Some(("username", "alice")), 2)
        }
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.event_id.as_str())
                .collect::<Vec<_>>(),
            ["empty", "linked"]
        );
        assert!(rows[0].workflow_links.is_empty());
        assert_eq!(
            rows[1]
                .workflow_links
                .iter()
                .map(|link| (link.workflow_session_id.as_str(), link.relation.as_str()))
                .collect::<Vec<_>>(),
            [
                ("a", "recording"),
                ("z", "recording"),
                ("b", "work_on_project")
            ]
        );
        assert!(rows[1]
            .workflow_links
            .iter()
            .all(|link| link.project.as_deref() == Some("p") && link.linked_at_ms == 3));
    }
    let row = db
        .get_window_activity_event_by_trace("w", Some(("username", "alice")), "trace-linked")
        .unwrap()
        .unwrap();
    assert_eq!(row.workflow_links.len(), 3);
}

#[test]
#[ignore = "manual multi-window query performance comparison; no timing assertion"]
fn window_history_query_benchmark() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("window-history.db")).unwrap();
    seed_session(&db);
    for window in 0..12 {
        for index in 0..2_000 {
            append(
                &db,
                event(
                    &format!("event-{window}-{index}"),
                    &format!("w{window}"),
                    "alice",
                    "p",
                    index,
                ),
                &[("session", "recording")],
            );
        }
    }
    for sample in 0..3 {
        let started = std::time::Instant::now();
        for window in 0..12 {
            let rows = db
                .list_window_activity_events(&format!("w{window}"), None, 2_000)
                .unwrap();
            assert_eq!(rows.len(), 2_000);
            assert!(rows.iter().all(|row| row.workflow_links.len() == 1));
        }
        eprintln!(
            "12 windows × 2000 events, sample {sample}: {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn window_activity_order_uses_index_for_primary_and_full_pages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("order.db");
    let db = Database::open(&path).unwrap();
    seed_session(&db);
    append(&db, event("fallback", "w", "alice", "p", 300), &[]);
    let mut observed = event("observed", "w", "alice", "p", 100);
    observed.request_observed_at_ms = Some(400);
    append(&db, observed, &[]);
    let mut tie = event("tie", "w", "alice", "p", 200);
    tie.request_observed_at_ms = Some(400);
    append(&db, tie, &[]);
    for principal in [None, Some(("username", "alice"))] {
        let rows = db.list_window_activity_events("w", principal, 2).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.event_id.as_str())
                .collect::<Vec<_>>(),
            ["tie", "observed"]
        );
    }
    // Exercise opening an existing database, including recreation of the additive
    // index for installations created before this optimization.
    db.lock_connection(crate::StoreDomain::WindowActivity)
        .execute_batch("DROP INDEX idx_action_events_window_observed")
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    let conn = db.lock_connection(crate::StoreDomain::WindowActivity);
    for principal in [
        "",
        "AND principal_correlation_kind = 'username' AND principal_correlation_id = 'alice'",
    ] {
        let mut stmt = conn
            .prepare(&format!(
                "EXPLAIN QUERY PLAN SELECT event_id FROM action_events
             WHERE client_window_key = 'w' AND window_started_at_ms IS NOT NULL
               AND window_ended_at_ms IS NOT NULL {principal}
             ORDER BY COALESCE(request_observed_at_ms, window_started_at_ms) DESC, event_id DESC
             LIMIT 80"
            ))
            .unwrap();
        let plan = stmt
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        assert!(plan.contains("idx_action_events_window_observed"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
}
