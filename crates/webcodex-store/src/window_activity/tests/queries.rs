use super::*;

#[test]
fn window_queries_preserve_projection_and_principal_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("projections.db")).unwrap();
    seed_session(&db);
    let principal = ("oauth2", "alice");
    let composition = serde_json::json!({"calls": [{"operation": "read_files"}]});
    let mut item = event("details", "w", "alice", "p", 10);
    item.principal_kind = Some("oauth2".into());
    item.principal_correlation_kind = Some("oauth2".into());
    item.principal_user_id = Some("user-a".into());
    item.oauth_client_id = Some("client-a".into());
    item.request_observed_at_ms = Some(9);
    item.response_handed_at_ms = Some(12);
    item.window_transition_kind = Some("continued".into());
    item.response_streaming = Some(true);
    item.window_continuity_eligible = Some(false);
    item.recorder_gap_session_id = Some("gap-session".into());
    item.ids_json = serde_json::json!({
        "async_job_id": "wc_job_background_123",
        "business_session_id": "wc_sess_6666666666666666"
    })
    .to_string();
    item.summary_json = serde_json::json!({
        "failure_expectation_result": "matched_expected_result",
        "code_mode_composition": composition
    })
    .to_string();
    append(&db, item, &[("s1", "recording"), ("s2", "work_on_project")]);
    // The same correlation id under another principal kind is a different scope.
    let mut foreign = event("foreign", "w", "alice", "q", 20);
    foreign.action_name = "resourcesRead".into();
    foreign.window_meaningful = false;
    append(&db, foreign, &[]);
    let detail = db
        .latest_successful_window_action("w", principal, "read_files")
        .unwrap()
        .unwrap();
    assert_eq!(
        detail.managed_oauth_identity,
        Some(("user-a".into(), "client-a".into()))
    );
    assert_eq!(
        detail.failure_expectation_result.as_deref(),
        Some("matched_expected_result")
    );
    assert!(detail.code_mode_composition.is_none());

    for scope in [None, Some(principal)] {
        let rows = db.list_window_activity_events("w", scope, 20).unwrap();
        let expected_ids: &[&str] = if scope.is_some() {
            &["details"]
        } else {
            &["foreign", "details"]
        };
        assert_eq!(
            rows.iter()
                .map(|row| row.event_id.as_str())
                .collect::<Vec<_>>(),
            expected_ids
        );
        let mut composed = db
            .list_window_activity_events_with_code_mode_composition("w", scope, 20)
            .unwrap();
        for row in &mut composed {
            assert_eq!(
                row.code_mode_composition.take(),
                (row.event_id == "details").then(|| composition.clone())
            );
        }
        assert_eq!(composed, rows);
        assert_eq!(
            db.list_goal_window_activity_events("w", scope, 20).unwrap(),
            rows
        );
        for row in rows {
            assert_eq!(
                db.get_window_activity_event_by_trace(
                    "w",
                    scope,
                    row.server_trace_id.as_deref().unwrap()
                )
                .unwrap()
                .unwrap(),
                row
            );
            if row.event_id == "details" {
                assert_eq!(row, detail);
            }
        }
        let summary = WindowActivitySummaryRecord {
            client_window_key: "w".into(),
            client_window_source: "openai-session".into(),
            first_seen_at_ms: 11,
            last_seen_at_ms: if scope.is_some() { 11 } else { 21 },
            last_tool_call_at_ms: Some(11),
            last_meaningful_activity_at_ms: Some(11),
            linked_session_count: 2,
            recorder_gap_count: 1,
        };
        assert_eq!(
            db.get_window_activity_summary("w", scope).unwrap(),
            Some(summary.clone())
        );
        assert_eq!(
            db.list_window_activity_summaries(scope, 20).unwrap(),
            [summary]
        );
    }
    assert!(db
        .get_window_activity_event_by_trace("w", Some(("username", "alice")), "trace-details")
        .unwrap()
        .is_none());
}

#[test]
fn latest_successful_window_action_filters_before_the_activity_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("presentation.db")).unwrap();
    seed_session(&db);
    let mut presented = event("presented", "w", "alice", "p", 1);
    presented.operation = Some("present_work_result".into());
    append(&db, presented, &[]);
    for index in 0..=MAX_WINDOW_ACTIVITY_LIMIT {
        append(
            &db,
            event(
                &format!("noise-{index}"),
                "w",
                "alice",
                "p",
                index as i64 + 2,
            ),
            &[],
        );
    }
    for (id, window, principal, status) in [
        ("failed", "w", "alice", "tool_error"),
        ("other-principal", "w", "bob", "success"),
        ("other-window", "other", "alice", "success"),
    ] {
        let mut item = event(id, window, principal, "q", 10_000);
        item.operation = Some("present_work_result".into());
        item.status = status.into();
        append(&db, item, &[]);
    }
    assert_eq!(
        db.latest_successful_window_action("w", ("username", "alice"), "present_work_result")
            .unwrap()
            .unwrap()
            .event_id,
        "presented"
    );
    assert!(db
        .latest_successful_window_action("missing", ("username", "alice"), "present_work_result")
        .unwrap()
        .is_none());
    let mut newest = event("newest", "w", "alice", "new-project", 20_000);
    newest.operation = Some("present_work_result".into());
    append(&db, newest, &[]);
    assert_eq!(
        db.latest_successful_window_action("w", ("username", "alice"), "present_work_result")
            .unwrap()
            .unwrap()
            .project
            .as_deref(),
        Some("new-project")
    );
}

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
