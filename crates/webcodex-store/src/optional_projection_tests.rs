use super::*;
use crate::{NewPeerMessage, NewWindowOperatorMessage};

fn database() -> (tempfile::TempDir, Database) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("optional.db")).unwrap();
    (tmp, db)
}

#[test]
fn optional_projection_busy_never_consumes_peer_or_operator_delivery() {
    let (_tmp, db) = database();
    let peer = db
        .post_peer_message(NewPeerMessage {
            principal_kind: "username".into(),
            principal_id: "alice".into(),
            sender_window_key: "a".repeat(64),
            recipient_window_key: "b".repeat(64),
            sender_peer_id: format!("wc_peer_{}", "a".repeat(32)),
            recipient_peer_id: format!("wc_peer_{}", "b".repeat(32)),
            kind: "note".into(),
            priority: "normal".into(),
            message: "retained".into(),
            tags: vec![],
            requires_ack: true,
            sender_session_id: None,
            sender_project: None,
            created_at_ms: 1,
        })
        .unwrap();
    db.post_window_operator_message(
        NewWindowOperatorMessage {
            principal_kind: "username".into(),
            principal_id: "alice".into(),
            recipient_window_key: "b".repeat(64),
            context_session_id: None,
            context_project: None,
            kind: "note".into(),
            priority: "normal".into(),
            message: "retained operator".into(),
            tags: vec![],
            requires_ack: true,
            created_at_ms: 1,
        },
        "operator-one",
    )
    .unwrap();
    let locked = db.conn_for_tests();
    let deadline = Instant::now() + Duration::from_secs(1);
    assert!(db
        .project_peer_attention(
            "username",
            "alice",
            &"b".repeat(64),
            &[],
            2,
            4,
            Some(deadline),
            |_| true
        )
        .unwrap()
        .is_none());
    assert!(db
        .project_window_operator_attention(
            "username",
            "alice",
            &"b".repeat(64),
            &[],
            2,
            4,
            Some(deadline),
            |_| true
        )
        .unwrap()
        .is_none());
    drop(locked);
    let rejected = db
        .project_peer_attention(
            "username",
            "alice",
            &"b".repeat(64),
            &[],
            3,
            4,
            Some(deadline),
            |_| false,
        )
        .unwrap()
        .unwrap();
    assert!(rejected.messages.is_empty());
    let first = db
        .take_peer_attention("username", "alice", &"b".repeat(64), &[], 4, 4)
        .unwrap();
    assert_eq!(first.messages[0].projection_count, 1);
    let operator = db
        .take_window_operator_attention("username", "alice", &"b".repeat(64), &[], 4, 4)
        .unwrap();
    assert_eq!(operator.messages.len(), 1);
    // Explicit ACK remains required even after the optional deadline expired.
    let expired = Instant::now() - Duration::from_secs(1);
    let ack = db
        .project_peer_attention(
            "username",
            "alice",
            &"b".repeat(64),
            &[peer.message_id.clone()],
            5,
            4,
            Some(expired),
            |_| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(ack.accepted_ack_ids, vec![peer.message_id]);
    let ack = db
        .project_window_operator_attention(
            "username",
            "alice",
            &"b".repeat(64),
            &[operator.messages[0].message_id.clone()],
            5,
            4,
            Some(expired),
            |_| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(ack.accepted_ack_ids.len(), 1);
    assert!(db
        .take_window_operator_attention("username", "alice", &"b".repeat(64), &[], 6, 4)
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn optional_projection_sql_deadline_rolls_back_and_restores_connection_policy() {
    let (_tmp, db) = database();
    db.conn_for_tests()
        .execute_batch("CREATE TABLE optional_probe(value INTEGER); PRAGMA busy_timeout=1234;")
        .unwrap();
    let started = Instant::now();
    let result=db.with_projection_connection(StoreDomain::Communication,Some(started+Duration::from_millis(10)),|conn| {
        let tx=conn.transaction()?;
        tx.execute("INSERT INTO optional_probe VALUES(1)",[])?;
        tx.query_row("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<100000000) SELECT SUM(n) FROM numbers",[],|r|r.get::<_,i64>(0))?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    assert!(result.is_none());
    let conn = db.conn_for_tests();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM optional_probe", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1234
    );
    assert!(conn.is_autocommit());
    eprintln!(
        "OPTIONAL_SQL deadline_ms=10 rollback=true elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.
    );
}

#[test]
fn optional_projection_external_writer_busy_does_not_commit_or_wait() {
    let (tmp, db) = database();
    db.conn_for_tests()
        .execute_batch("CREATE TABLE optional_probe(value INTEGER)")
        .unwrap();
    let external = Connection::open(tmp.path().join("optional.db")).unwrap();
    external.execute_batch("BEGIN IMMEDIATE").unwrap();
    let result = db
        .with_projection_connection(
            StoreDomain::Communication,
            Some(Instant::now() + Duration::from_secs(1)),
            |conn| {
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO optional_probe VALUES(1)", [])?;
                tx.commit()?;
                Ok(())
            },
        )
        .unwrap();
    assert!(result.is_none());
    external.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM optional_probe", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
