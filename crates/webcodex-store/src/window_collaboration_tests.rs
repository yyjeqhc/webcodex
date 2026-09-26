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
    assert_eq!(count, 512);
}
