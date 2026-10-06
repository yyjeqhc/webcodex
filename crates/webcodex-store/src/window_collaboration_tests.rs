use super::window_collaboration::*;
use crate::{Database, NewPeerMessage};

fn input() -> NewWindowOperatorMessage {
    NewWindowOperatorMessage {
        principal_kind: "managed_user".into(),
        principal_id: "alice".into(),
        recipient_window_key: "a".repeat(64),
        context_session_id: None,
        context_project: None,
        kind: "guidance".into(),
        priority: "normal".into(),
        message: "Check the tests first".into(),
        tags: vec![],
        requires_ack: true,
        created_at_ms: 10,
    }
}
fn delivered(outcome: WindowOperatorDeliveryOutcome, expected_replay: bool) -> String {
    match outcome {
        WindowOperatorDeliveryOutcome::Delivered {
            message_id,
            replayed,
        } => {
            assert_eq!(replayed, expected_replay);
            message_id
        }
        _ => panic!("expected delivered"),
    }
}

#[test]
fn window_history_encoded_pages_are_bounded_without_cutting_message_bodies() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("bounded-history.db")).unwrap();
    let body = "😀\n\u{0000}".repeat(2666);
    for i in 0..20 {
        let mut message = input();
        message.created_at_ms = 100 + i;
        message.message = body.clone();
        db.post_window_operator_message(message, &format!("large-{i}"))
            .unwrap();
    }
    let mut before = None;
    let mut ids = std::collections::HashSet::new();
    let mut pages = 0;
    loop {
        let (rows, more) = db
            .window_collaboration_page(
                "managed_user",
                "alice",
                &"a".repeat(64),
                100,
                before.as_deref(),
            )
            .unwrap()
            .unwrap();
        assert!(!rows.is_empty());
        assert!(serde_json::to_vec(&rows).unwrap().len() <= 128 * 1024);
        assert!(rows.iter().all(|row| row.message == body));
        before = Some(rows[0].message_id.clone());
        pages += 1;
        for row in rows {
            assert!(ids.insert(row.message_id));
        }
        if !more {
            break;
        }
    }
    assert_eq!(ids.len(), 20);
    assert!(pages > 1);
}

#[test]
fn window_history_attention_query_bounds_the_source_before_window_filtering() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("plan.db")).unwrap();
    let conn = db.conn_for_tests();
    let mut statement = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            include_str!("window_operator_attention.sql")
        ))
        .unwrap();
    let plan = statement
        .query_map(
            rusqlite::params!["managed_user", "alice", "a".repeat(64), 4],
            |row| row.get::<_, String>(3),
        )
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
    assert!(plan.contains("idx_operator_attention_recent"), "{plan}");
    assert!(
        !plan.contains("idx_operator_transcript_page"),
        "must not scan permanent Window history: {plan}"
    );
    assert!(
        plan.contains("recent_operator_messages"),
        "the bounded source must precede Window filtering: {plan}"
    );
}

#[test]
fn window_history_does_not_turn_old_transcripts_into_an_unbounded_attention_queue() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("history.db")).unwrap();
    let original = delivered(
        db.post_window_operator_message(input(), "old-window")
            .unwrap(),
        false,
    );
    for i in 0..520 {
        let mut value = input();
        value.recipient_window_key = "b".repeat(64);
        value.created_at_ms = 100 + i;
        db.post_window_operator_message(value, &format!("busy-window-{i}"))
            .unwrap();
    }
    let attention = db
        .take_window_operator_attention("managed_user", "alice", &"a".repeat(64), &[], 1000, 4)
        .unwrap();
    assert!(
        attention.messages.is_empty(),
        "history must not widen the existing bounded projection queue"
    );
    let (history, more) = db
        .window_collaboration_page("managed_user", "alice", &"a".repeat(64), 10, None)
        .unwrap()
        .unwrap();
    assert!(!more);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].message_id, original);
}

#[test]
fn window_history_preserves_model_replies_and_reply_targets_beyond_the_old_cap() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.db");
    let db = Database::open(&path).unwrap();
    let original = delivered(
        db.post_window_operator_message(input(), "original")
            .unwrap(),
        false,
    );
    db.take_window_operator_attention("managed_user", "alice", &"a".repeat(64), &[], 11, 4)
        .unwrap();
    for i in 0..520 {
        let reply = NewWindowModelReply {
            principal_kind: "managed_user".into(),
            principal_id: "alice".into(),
            window_key: "a".repeat(64),
            reply_to_message_id: original.clone(),
            message: format!("reply-{i}"),
            created_at_ms: 20 + i,
        };
        assert!(matches!(
            db.post_window_model_reply(reply, &format!("reply-key-{i}"))
                .unwrap(),
            WindowModelReplyDeliveryOutcome::Delivered {
                replayed: false,
                ..
            }
        ));
    }
    drop(db);
    let db = Database::open(&path).unwrap();
    let mut cursor = None;
    let mut rows = Vec::new();
    loop {
        let (page, more) = db
            .window_collaboration_page(
                "managed_user",
                "alice",
                &"a".repeat(64),
                100,
                cursor.as_deref(),
            )
            .unwrap()
            .unwrap();
        cursor = page.first().map(|row| row.message_id.clone());
        rows.extend(page);
        if !more {
            break;
        }
    }
    assert_eq!(rows.len(), 521);
    assert!(rows
        .iter()
        .filter(|row| row.source == "window")
        .all(|row| row.reply_to_message_id.as_deref() == Some(original.as_str())));
    assert!(rows.iter().any(|row| row.message == "reply-0"));
}

#[test]
fn operator_durability_replay_identity_isolation_and_read_only_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("collaboration.db");
    let db = Database::open(&path).unwrap();
    let id = delivered(
        db.post_window_operator_message(input(), "send-1").unwrap(),
        false,
    );
    for _ in 0..3 {
        let (rows, truncated) = db
            .window_collaboration_transcript("managed_user", "alice", &"a".repeat(64), 10)
            .unwrap();
        assert!(!truncated);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].first_projected_at_ms.is_none());
    }
    let count: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT projection_count FROM window_operator_messages",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert!(db
        .list_window_operator_messages("managed_user", "bob", &"a".repeat(64), 10)
        .unwrap()
        .is_empty());
    assert!(db
        .take_window_operator_attention(
            "managed_user",
            "bob",
            &"a".repeat(64),
            &[id.clone()],
            20,
            4
        )
        .unwrap()
        .messages
        .is_empty());
    drop(db);
    let db = Database::open(&path).unwrap();
    let mut later = input();
    later.created_at_ms = 99;
    assert_eq!(
        delivered(
            db.post_window_operator_message(later, "send-1").unwrap(),
            true
        ),
        id
    );
    for changed in 0..4 {
        let mut value = input();
        match changed {
            0 => value.message = "different".into(),
            1 => value.recipient_window_key = "b".repeat(64),
            2 => value.context_session_id = Some("wc_sess_context".into()),
            _ => value.context_project = Some("project".into()),
        }
        assert!(matches!(
            db.post_window_operator_message(value, "send-1").unwrap(),
            WindowOperatorDeliveryOutcome::DeliveryKeyConflict
        ));
    }
    let batch = db
        .take_window_operator_attention("managed_user", "alice", &"a".repeat(64), &[], 30, 4)
        .unwrap();
    assert_eq!(batch.messages[0].message_id, id);
    db.rollback_window_operator_attention(
        "managed_user",
        "alice",
        &"a".repeat(64),
        30,
        &batch.projection_rollbacks,
    )
    .unwrap();
    assert!(db
        .list_window_operator_messages("managed_user", "alice", &"a".repeat(64), 10)
        .unwrap()[0]
        .first_projected_at_ms
        .is_none());
    let batch = db
        .take_window_operator_attention("managed_user", "alice", &"a".repeat(64), &[], 40, 4)
        .unwrap();
    assert_eq!(batch.messages[0].first_projected_at_ms, Some(40));
    let ack = db
        .take_window_operator_attention(
            "managed_user",
            "alice",
            &"a".repeat(64),
            &[id.clone()],
            50,
            4,
        )
        .unwrap();
    assert_eq!(ack.accepted_ack_ids, vec![id]);
    assert!(ack.messages.is_empty());
    assert!(db
        .take_window_operator_attention("managed_user", "alice", &"a".repeat(64), &[], 60, 4)
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn window_transcript_merges_both_peer_directions_with_operator_context_and_bounds_retention() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("db")).unwrap();
    for (sender, recipient, at) in [('a', 'b', 20), ('b', 'a', 30)] {
        db.post_peer_message(NewPeerMessage {
            principal_kind: "managed_user".into(),
            principal_id: "alice".into(),
            sender_window_key: sender.to_string().repeat(64),
            recipient_window_key: recipient.to_string().repeat(64),
            sender_peer_id: format!("wc_peer_{}", sender.to_string().repeat(32)),
            recipient_peer_id: format!("wc_peer_{}", recipient.to_string().repeat(32)),
            kind: "progress".into(),
            priority: "normal".into(),
            message: "peer update".into(),
            tags: vec![],
            requires_ack: false,
            sender_session_id: Some("wc_sess_context".into()),
            sender_project: Some("project".into()),
            created_at_ms: at,
        })
        .unwrap();
    }
    db.post_window_operator_message(input(), "one").unwrap();
    let (rows, truncated) = db
        .window_collaboration_transcript("managed_user", "alice", &"a".repeat(64), 3)
        .unwrap();
    assert!(!truncated);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        (&*rows[0].source, &*rows[1].direction, &*rows[2].direction),
        ("operator", "outbound", "inbound")
    );
    assert_eq!(rows[1].context_project.as_deref(), Some("project"));
    assert_eq!(
        rows[1].context_session_id.as_deref(),
        Some("wc_sess_context")
    );
    // Sender provenance is retained for its own outbound history, not shared
    // merely because the other Window can receive a peer message.
    assert!(rows[2].context_project.is_none());
    assert!(rows[2].context_session_id.is_none());
    let inbound = serde_json::to_value(&rows[2]).unwrap();
    assert!(inbound.get("context_project").is_none());
    assert!(inbound.get("context_session_id").is_none());
    let (other_rows, _) = db
        .window_collaboration_transcript("managed_user", "alice", &"b".repeat(64), 3)
        .unwrap();
    assert!(other_rows[0].context_project.is_none());
    assert!(other_rows[0].context_session_id.is_none());
    assert_eq!(other_rows[1].context_project.as_deref(), Some("project"));
    assert_eq!(
        other_rows[1].context_session_id.as_deref(),
        Some("wc_sess_context")
    );
    let (rows, truncated) = db
        .window_collaboration_transcript("managed_user", "alice", &"a".repeat(64), 2)
        .unwrap();
    assert!(truncated);
    assert_eq!(rows[0].created_at_ms, 20);
    for i in 0..515 {
        let mut value = input();
        value.created_at_ms = 100 + i;
        db.post_window_operator_message(value, &format!("key-{i}"))
            .unwrap();
    }
    let count: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM window_operator_messages", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        count, 516,
        "new traffic must not delete durable operator history"
    );
}

#[test]
fn window_history_keyset_pages_survive_restart_ties_and_new_messages() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.db");
    let db = Database::open(&path).unwrap();
    let window = "a".repeat(64);
    let mut expected = Vec::new();
    for i in 0..520 {
        let mut message = input();
        message.created_at_ms = 100 + i / 7; // Deliberate timestamp ties.
        expected.push(delivered(
            db.post_window_operator_message(message, &format!("history-{i}"))
                .unwrap(),
            false,
        ));
    }
    let (latest, more) = db
        .window_collaboration_page("managed_user", "alice", &window, 17, None)
        .unwrap()
        .unwrap();
    assert!(more);
    let before = latest[0].message_id.clone();
    let mut newer = input();
    newer.created_at_ms = 9999;
    db.post_window_operator_message(newer, "newer-during-pagination")
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    let mut seen: Vec<_> = latest
        .iter()
        .map(|message| message.message_id.clone())
        .collect();
    let mut cursor = before;
    loop {
        let (page, more) = db
            .window_collaboration_page("managed_user", "alice", &window, 17, Some(&cursor))
            .unwrap()
            .unwrap();
        assert!(page.len() <= 17);
        assert!(page
            .windows(2)
            .all(|pair| (pair[0].created_at_ms, &pair[0].message_id)
                < (pair[1].created_at_ms, &pair[1].message_id)));
        seen.extend(page.iter().map(|message| message.message_id.clone()));
        if !more {
            break;
        }
        cursor = page[0].message_id.clone();
    }
    seen.sort();
    expected.sort();
    assert_eq!(seen, expected);
    assert!(db
        .window_collaboration_page("managed_user", "bob", &window, 10, Some(&cursor))
        .unwrap()
        .is_none());
    assert!(db
        .window_collaboration_page("managed_user", "alice", &"b".repeat(64), 10, Some(&cursor))
        .unwrap()
        .is_none());
    assert!(db
        .window_collaboration_page("managed_user", "alice", &window, 10, Some("wc_msg_missing"))
        .unwrap()
        .is_none());
    let mut retry = input();
    retry.created_at_ms = 90000;
    assert!(matches!(
        db.post_window_operator_message(retry, "history-0").unwrap(),
        WindowOperatorDeliveryOutcome::Delivered { replayed: true, .. }
    ));
}

#[test]
fn window_history_archives_peer_delivery_without_extending_routes_or_leaking_context() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.db");
    let db = Database::open(&path).unwrap();
    for i in 0..520 {
        db.post_peer_message(NewPeerMessage {
            principal_kind: "managed_user".into(),
            principal_id: "alice".into(),
            sender_window_key: "a".repeat(64),
            recipient_window_key: "b".repeat(64),
            sender_peer_id: format!("wc_peer_{}", "a".repeat(32)),
            recipient_peer_id: format!("wc_peer_{}", "b".repeat(32)),
            kind: "note".into(),
            priority: "normal".into(),
            message: format!("peer-{i}"),
            tags: vec![],
            requires_ack: false,
            sender_session_id: Some("wc_sess_context".into()),
            sender_project: Some("private-sender-project".into()),
            created_at_ms: i,
        })
        .unwrap();
    }
    let count = |table: &str| {
        db.conn_for_tests()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    assert_eq!(count("window_peer_messages"), 512);
    assert_eq!(count("window_peer_message_history"), 8);
    drop(db);
    let db = Database::open(&path).unwrap();
    let mut cursor = None;
    let mut rows = Vec::new();
    loop {
        let (page, more) = db
            .window_collaboration_page(
                "managed_user",
                "alice",
                &"b".repeat(64),
                100,
                cursor.as_deref(),
            )
            .unwrap()
            .unwrap();
        cursor = page.first().map(|message| message.message_id.clone());
        rows.extend(page);
        if !more {
            break;
        }
    }
    assert_eq!(rows.len(), 520);
    assert!(rows
        .iter()
        .all(|row| row.context_project.is_none() && row.context_session_id.is_none()));
    assert!(rows.iter().any(|row| row.message == "peer-0"));
}
