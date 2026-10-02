use super::*;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

#[test]
fn history_read_snapshot_does_not_block_receipt_reference_or_audit_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&tmp.path().join("lanes.db")).unwrap());
    seed_session(&db);
    append(&db, event("original", "w", "alice", "a", 1), &[]);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let reader_db = db.clone();
    let reader = std::thread::spawn(move || {
        let mut reader = reader_db.lock_history_connection(crate::StoreDomain::WindowActivity);
        let snapshot = reader.transaction().unwrap();
        let count = || {
            snapshot
                .query_row("SELECT COUNT(*) FROM action_events", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        };
        assert_eq!(count(), 1);
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // WAL gives this transaction its original view despite the committed append.
        assert_eq!(count(), 1);
        snapshot.commit().unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let writer_db = db.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        let started = Instant::now();
        let reference = writer_db
            .get_or_create_project_reference(
                "principal",
                "agent:r:p",
                &format!("wc_projroot_{}", "a".repeat(64)),
                1,
            )
            .unwrap();
        assert_eq!(
            writer_db
                .lookup_project_reference("principal", reference.ref_index)
                .unwrap(),
            Some(reference)
        );
        let now = chrono::Utc::now().timestamp();
        let receipt=webcodex_core::runner_job_receipt::RetainedJobReceipt {
            client_id:"r".into(),runner_instance_id:"instance".into(),auth_group:None,owner_at_admission:Some("alice".into()),kind:"shell".into(),terminal_observed_at:now,
            expires_at:now+webcodex_core::runner_protocol::JOB_TERMINAL_RETENTION_SECS,
            snapshot:serde_json::from_value(serde_json::json!({"job_id":"job-read-lane","request_id":"req-read-lane","status":"completed","update_seq":3,"created_at":now-2,"started_at":now-1,"ended_at":now,"exit_code":0,"context":{"command_preview":"echo done"}})).unwrap(),
        };
        writer_db.upsert_job_receipt(&receipt, now).unwrap();
        append(&writer_db, event("concurrent", "w", "alice", "a", 2), &[]);
        done_tx.send(started.elapsed()).unwrap();
    });
    let completed = done_rx.recv_timeout(Duration::from_secs(5));
    // Release/join even on failure; the timeout is a deadlock guard, not a CI
    // performance threshold. The invariant is completion BEFORE read release.
    release_tx.send(()).unwrap();
    reader.join().unwrap();
    writer.join().unwrap();
    let elapsed = completed.expect("critical operations serialized behind history reader");
    assert_eq!(
        db.list_window_activity_events("w", None, 20).unwrap().len(),
        2
    );
    assert_eq!(
        db.window_inventory_project_anchors(None).unwrap(),
        vec!["a"]
    );
    eprintln!("STORE_READ_ISOLATION history_snapshot=held writer_completed_before_release=true receipt_insert=1 project_reference_lookup=1 audit_append=1 critical_ms={:.3}",elapsed.as_secs_f64()*1000.);
}

#[test]
fn history_connection_is_fixed_read_only_and_reopens_after_migration() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readonly.db");
    for _ in 0..3 {
        let db = Database::open(&path).unwrap();
        let reader = db.lock_history_connection(crate::StoreDomain::WindowActivity);
        assert_eq!(
            reader
                .query_row("PRAGMA query_only", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            reader
                .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert!(reader
            .execute(
                "INSERT INTO window_inventory_dirty VALUES ('must-not-write')",
                []
            )
            .is_err());
        assert!(db.history_reader.get().is_some());
    }
}

#[test]
fn append_repairs_only_its_own_dirty_window_and_inventory_repairs_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("dirty.db")).unwrap();
    seed_session(&db);
    append(&db, event("old-a", "a", "alice", "p", 1), &[]);
    append(&db, event("old-b", "b", "alice", "p", 1), &[]);
    db.conn_for_tests()
        .execute("UPDATE action_events SET status='failed'", [])
        .unwrap();
    append(&db, event("new-a", "a", "alice", "p", 2), &[]);
    let dirty: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM window_inventory_dirty WHERE window_key='b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        dirty, 1,
        "unrelated history must not be repaired by a critical append"
    );
    db.window_inventory_project_anchors(None).unwrap();
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM window_inventory_dirty", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
