use super::*;
use rusqlite::{params, StatementStatus};

fn fixture(old: usize) -> (tempfile::TempDir, Database) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("peers.db")).unwrap();
    seed_session(&db);
    if old > 0 {
        db.conn_for_tests().execute_batch(&format!("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i+1<{old})
            INSERT INTO action_events(event_id,session_id,started_at,ended_at,duration_ms,endpoint,action_name,operation,project,status,changed_files_json,ids_json,summary_json,client_window_key,client_window_source,principal_correlation_kind,principal_correlation_id,window_started_at_ms,window_ended_at_ms,window_meaningful)
            SELECT 'old-'||i,'audit-session',0,1,1,'/mcp','toolsCall','read_files','a','success','[]','{{}}','{{}}','w-'||(i%500),'openai-session','username','alice',1,2,1 FROM n;")).unwrap();
    }
    for index in 0..8 {
        append(
            &db,
            event(
                &format!("recent-{index}"),
                &format!("peer-{index}"),
                "alice",
                "a",
                10_000 + index,
            ),
            &[],
        );
    }
    (tmp, db)
}

fn measure(db: &Database) -> (i32, usize, f64) {
    let conn = db.conn_for_tests();
    let mut stmt = conn
        .prepare(crate::peer_collaboration::RECENT_PEERS_SQL)
        .unwrap();
    let start = std::time::Instant::now();
    let rows = stmt
        .query_map(
            params!["observer", "username", "alice", "a", 10_000, 8],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let elapsed = start.elapsed().as_secs_f64() * 1000.;
    // MATERIALIZED recent rows can add a small counted scan, never history.
    assert!(stmt.get_status(StatementStatus::FullscanStep) <= 8);
    (
        stmt.get_status(StatementStatus::VmStep),
        rows.len(),
        elapsed,
    )
}

#[test]
fn peer_recent_query_work_depends_on_recent_rows_not_retained_action_history() {
    let (_small, small) = fixture(0);
    let a = measure(&small);
    let (_large, large) = fixture(100_000);
    let b = measure(&large);
    // A nonempty older index range adds one terminating range-bound opcode;
    // doubling retained history must add none. This is operation-count proof,
    // not a timing tolerance or a weakened visibility assertion.
    let (_doubled, doubled) = fixture(200_000);
    let c = measure(&doubled);
    assert!(b.0 <= a.0 + 1);
    assert_eq!((b.0, b.1), (c.0, c.1));
    assert_eq!(a.1, 8);
    let conn = large.conn_for_tests();
    let mut stmt = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            crate::peer_collaboration::RECENT_PEERS_SQL
        ))
        .unwrap();
    let plan = stmt
        .query_map(
            params!["observer", "username", "alice", "a", 10_000, 8],
            |r| r.get::<_, String>(3),
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(plan.contains("idx_action_events_recent_peer"), "{plan}");
    assert!(plan.contains("window_ended_at_ms>?"), "{plan}");
    assert!(!plan.contains("SCAN e"), "{plan}");
    eprintln!("PEER_INDEX old_rows=0 vm_steps={} lookup_ms={:.3}; old_rows=100000 vm_steps={} lookup_ms={:.3}; recent=8\n{plan}",a.0,a.2,b.0,b.2);
}

#[test]
fn peer_recent_query_preserves_principal_project_cutoff_and_exact_once_projection() {
    let (_tmp, db) = fixture(0);
    append(&db, event("foreign", "secret", "bob", "a", 20_000), &[]);
    append(
        &db,
        event("other-project", "other", "alice", "b", 30_000),
        &[],
    );
    append(
        &db,
        event("observer", "observer", "alice", "a", 40_000),
        &[],
    );
    let mut quiet = event("quiet", "quiet", "alice", "a", 50_000);
    quiet.window_meaningful = false;
    append(&db, quiet, &[]);
    let first = db
        .take_new_recent_project_peers("username", "alice", "observer", "a", 10_000, 60_000, 3)
        .unwrap();
    assert_eq!(
        first
            .iter()
            .map(|r| r.client_window_key.as_str())
            .collect::<Vec<_>>(),
        vec!["peer-7", "peer-6", "peer-5"]
    );
    let second = db
        .take_new_recent_project_peers("username", "alice", "observer", "a", 10_000, 60_001, 8)
        .unwrap();
    assert_eq!(second.len(), 5);
    assert!(db
        .take_new_recent_project_peers("username", "alice", "observer", "a", 10_000, 60_002, 8)
        .unwrap()
        .is_empty());
    let ids = first
        .iter()
        .filter_map(|r| r.discovery_rowid)
        .collect::<Vec<_>>();
    db.rollback_peer_discoveries("username", "alice", "observer", "a", &ids)
        .unwrap();
    assert_eq!(
        db.take_new_recent_project_peers("username", "alice", "observer", "a", 10_000, 60_003, 8)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        db.take_new_recent_project_peers(
            "username",
            "alice",
            "another-window",
            "a",
            10_000,
            60_004,
            8
        )
        .unwrap()
        .len(),
        8
    );
    assert!(db
        .take_new_recent_project_peers("username", "alice", "new-observer", "a", 50_002, 60_005, 8)
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "manual large peer query baseline/index benchmark"]
fn recent_peer_query_benchmark() {
    for old in [10_000, 100_000, 400_000] {
        let (_tmp, db) = fixture(old);
        let (steps, rows, indexed) = measure(&db);
        db.conn_for_tests()
            .execute("DROP INDEX idx_action_events_recent_peer", [])
            .unwrap();
        let conn = db.conn_for_tests();
        let mut stmt = conn
            .prepare(crate::peer_collaboration::RECENT_PEERS_SQL)
            .unwrap();
        let start = std::time::Instant::now();
        let baseline = stmt
            .query_map(
                params!["observer", "username", "alice", "a", 10_000, 8],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(baseline.len(), rows);
        eprintln!("PEER_BENCH retained={old} recent=8 without_index_ms={:.3} indexed_ms={indexed:.3} baseline_vm_steps={} indexed_vm_steps={steps}",start.elapsed().as_secs_f64()*1000.,stmt.get_status(StatementStatus::VmStep));
    }
}
