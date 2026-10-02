use super::*;
use crate::{WindowInventoryPage, WindowInventoryQuery};

fn inventory(
    db: &Database,
    allowed: &[&str],
    principal: Option<(&str, &str)>,
    management: bool,
    projects: Option<&[String]>,
    offset: usize,
    limit: usize,
) -> WindowInventoryPage {
    let allowed = allowed.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    db.read_window_inventory(WindowInventoryQuery {
        visible_projects: &allowed,
        principal,
        caller: Some(("username", "alice")),
        management,
        projects,
        window_key: None,
        query: "",
        live: &[],
        offset,
        limit,
    })
    .unwrap()
}

#[test]
fn window_activity_inventory_preserves_exact_authority_partitions_and_paging() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("partitions.db")).unwrap();
    seed_session(&db);
    append(
        &db,
        event("a", "mixed", "alice", "a", 100),
        &[("s1", "recording")],
    );
    append(
        &db,
        event("b", "mixed", "bob", "b", 900),
        &[("s2", "recording")],
    );
    append(&db, event("older", "old", "alice", "a", 1), &[]);
    let mut free = event("free", "free", "alice", "a", 200);
    free.project = None;
    append(&db, free, &[]);
    let mut foreign = event("foreign", "foreign", "bob", "a", 1000);
    foreign.project = None;
    append(&db, foreign, &[]);
    let page = inventory(&db, &["a"], None, true, None, 0, 2);
    assert_eq!(page.total, 3);
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0].client_window_key, "free");
    let mixed = &page.rows[1];
    assert_eq!(mixed.client_window_key, "mixed");
    assert_eq!(mixed.last_seen_at_ms, 101);
    assert_eq!(mixed.first_seen_at_ms, 101);
    assert_eq!(mixed.last_project.as_deref(), Some("a"));
    assert_eq!(mixed.linked_session_count, 1);
    let next = inventory(&db, &["a"], None, true, None, 2, 2);
    assert_eq!(next.total, 3);
    assert_eq!(next.rows[0].client_window_key, "old");
    let empty = inventory(&db, &["a"], None, true, None, 10, 2);
    assert_eq!(empty.total, 3);
    assert!(empty.rows.is_empty());
    let revoked = inventory(&db, &[], None, true, None, 0, 20);
    assert_eq!(revoked.total, 1);
    assert_eq!(revoked.rows[0].client_window_key, "free");
    let scoped = inventory(
        &db,
        &["a", "b"],
        Some(("username", "alice")),
        false,
        None,
        0,
        20,
    );
    assert_eq!(scoped.total, 3);
    let selected = vec!["a".to_string()];
    assert_eq!(
        inventory(&db, &["a"], None, true, Some(&selected), 0, 20).total,
        2
    );
}

#[test]
fn window_activity_inventory_links_cannot_override_explicit_project_or_scoped_principal() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("links.db")).unwrap();
    seed_session(&db);
    let explicit = event("explicit", "denied", "alice", "denied", 100);
    let mut linked = event("linked", "linked", "bob", "denied", 200);
    linked.project = None;
    for ev in [explicit, linked] {
        let links = vec![ActionEventWorkflowLinkRecord {
            event_id: ev.event_id.clone(),
            workflow_session_id: "s".into(),
            workflow_session_relation: "recording".into(),
            project: Some("a".into()),
            linked_at_ms: 201,
        }];
        db.append_action_event_and_update_session(&ev, &links, 1, 0, 0, 0, 1, 0, 0)
            .unwrap();
    }
    let rows = inventory(&db, &["a"], None, true, None, 0, 20);
    assert_eq!(rows.total, 1);
    assert_eq!(rows.rows[0].client_window_key, "linked");
    assert_eq!(
        inventory(&db, &["a"], Some(("username", "alice")), false, None, 0, 20).total,
        0
    );
    assert_eq!(inventory(&db, &[], None, true, None, 0, 20).total, 0);
}

#[test]
fn window_activity_inventory_handles_out_of_order_ties_internal_calls_and_retention() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("updates.db")).unwrap();
    seed_session(&db);
    let mut latest = event("latest", "w", "alice", "a", 300);
    latest.recorder_gap_session_id = Some("s".into());
    latest.operation = Some("write".into());
    append(&db, latest, &[]);
    append(&db, event("old", "w", "alice", "a", 100), &[]);
    let mut internal = event("internal", "w", "alice", "a", 900);
    internal.operation = Some("present_work_result".into());
    append(&db, internal, &[]);
    let page = inventory(&db, &["a"], None, true, None, 0, 20);
    assert_eq!(page.rows[0].first_seen_at_ms, 101);
    assert_eq!(page.rows[0].last_seen_at_ms, 301);
    assert_eq!(page.rows[0].last_activity_name.as_deref(), Some("write"));
    assert_eq!(page.rows[0].recorder_gap_count, 1);
    db.conn_for_tests()
        .execute("DELETE FROM action_events WHERE event_id='latest'", [])
        .unwrap();
    let page = inventory(&db, &["a"], None, true, None, 0, 20);
    assert_eq!(page.rows[0].last_seen_at_ms, 101);
    assert_eq!(page.rows[0].recorder_gap_count, 0);
    db.conn_for_tests()
        .execute(
            "UPDATE action_events SET project='denied' WHERE event_id='old'",
            [],
        )
        .unwrap();
    assert_eq!(inventory(&db, &["a"], None, true, None, 0, 20).total, 0);
}

#[test]
fn window_activity_inventory_additive_backfill_is_idempotent_and_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("backfill.db");
    let db = Database::open(&path).unwrap();
    seed_session(&db);
    append(
        &db,
        event("a", "w", "alice", "a", 100),
        &[("s", "recording")],
    );
    // Simulate a database predating the derived projection, preserving evidence.
    db.conn_for_tests().execute_batch("DROP TABLE window_inventory_cells; DROP TABLE window_inventory_links; DROP TABLE window_inventory_dirty; DROP TABLE window_inventory_meta;").unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        inventory(&db, &["a"], None, true, None, 0, 20).rows[0].linked_session_count,
        1
    );
    drop(db);
    let db = Database::open(&path).unwrap();
    let before = inventory(&db, &["a"], None, true, None, 0, 20);
    db.conn_for_tests().execute_batch("CREATE TEMP TRIGGER fail_inventory_fixture BEFORE INSERT ON action_event_workflow_links WHEN NEW.event_id='bad' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let ev = event("bad", "w", "alice", "a", 500);
    let bad = ActionEventWorkflowLinkRecord {
        event_id: ev.event_id.clone(),
        workflow_session_id: "s".into(),
        workflow_session_relation: "invalid".into(),
        project: Some("a".into()),
        linked_at_ms: 500,
    };
    assert!(db
        .append_action_event_and_update_session(&ev, &[bad], 1, 0, 0, 0, 1, 0, 0)
        .is_err());
    assert_eq!(
        inventory(&db, &["a"], None, true, None, 0, 20).rows[0].last_seen_at_ms,
        before.rows[0].last_seen_at_ms
    );
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM action_events WHERE event_id='bad'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn window_activity_inventory_read_plan_never_opens_event_history() {
    let db = Database::open(&tempfile::tempdir().unwrap().path().join("plan.db")).unwrap();
    let conn = db.conn_for_tests();
    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        include_str!("../../window_inventory/read.sql")
    );
    let mut stmt = conn.prepare(&sql).unwrap();
    let plan=stmt.query_map(rusqlite::named_params! {":allowed":"[]",":projects":Option::<String>::None,":pk":Option::<String>::None,":pi":Option::<String>::None,":ck":"username",":ci":"alice",":management":true,":live":"[]",":key":Option::<String>::None,":query":"",":limit":50,":offset":0},|r|r.get::<_,String>(3)).unwrap().collect::<Result<Vec<_>,_>>().unwrap().join("\n");
    assert!(!plan.contains("action_events"), "{plan}");
    assert!(!plan.contains("action_event_workflow_links"), "{plan}");
    assert!(plan.contains("window_inventory"), "{plan}");
    // Exact lookup uses the derived table's key, not a full history scan.
    let mut stmt = conn
        .prepare("EXPLAIN QUERY PLAN SELECT * FROM window_inventory_cells WHERE window_key=?1")
        .unwrap();
    let key_plan = stmt
        .query_map(["w"], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(key_plan.contains("PRIMARY KEY"), "{key_plan}");
    assert!(!key_plan.contains("TEMP B-TREE"));
    eprintln!("WINDOW_INVENTORY_PLAN\n{plan}\nEXACT_LOOKUP {key_plan}");
}

#[test]
#[ignore = "manual inventory scalability benchmark; no wall-clock assertion"]
fn window_inventory_query_benchmark() {
    // Fixture population and repair are outside both measured paths. Bulk SQL
    // simulates historical DB import and exercises the real backfill projection.
    for windows in [100, 500, 2000] {
        for history in [10, 200] {
            let tmp = tempfile::tempdir().unwrap();
            let db = Database::open(&tmp.path().join("bench.db")).unwrap();
            seed_session(&db);
            {
                let conn = db.conn_for_tests();
                conn.execute_batch(&format!("WITH RECURSIVE w(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM w WHERE n+1<{windows}), h(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM h WHERE n+1<{history})
                    INSERT INTO action_events(event_id,session_id,started_at,ended_at,duration_ms,endpoint,action_name,operation,project,status,changed_files_json,ids_json,summary_json,client_window_key,client_window_source,principal_correlation_kind,principal_correlation_id,window_started_at_ms,window_ended_at_ms,window_meaningful)
                    SELECT printf('event-%d-%d',w.n,h.n),'audit-session',0,1,1,'/mcp','toolsCall','read_files','a','success','[]','{{}}','{{}}',printf('w%06d',w.n),'openai-session','username','alice',h.n,h.n+1,1 FROM w CROSS JOIN h;" )).unwrap();
            }
            db.window_inventory_project_anchors(None).unwrap();
            let cell_count: i64 = db
                .conn_for_tests()
                .query_row("SELECT COUNT(*) FROM window_inventory_cells", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(cell_count, windows);
            let start = std::time::Instant::now();
            let candidates = db.list_window_activity_summaries(None, 2000).unwrap();
            for row in candidates {
                db.list_window_activity_events(&row.client_window_key, None, 2000)
                    .unwrap();
                db.list_window_workflow_sessions(&row.client_window_key, None, 2000)
                    .unwrap();
            }
            let before = start.elapsed();
            let start = std::time::Instant::now();
            let page = inventory(&db, &["a"], None, true, None, 0, 50);
            assert_eq!(page.total, windows as usize);
            assert_eq!(page.rows.len(), 50);
            let after = start.elapsed();
            eprintln!("WINDOW_INVENTORY windows={windows} events_per_window={history} cells={cell_count} legacy_history_queries={} after_history_queries=0 inventory_statements=1 before_ms={:.3} after_ms={:.3}",windows*3,before.as_secs_f64()*1000.,after.as_secs_f64()*1000.);
        }
    }
}
