use super::*;
use rusqlite::params;

fn seed_expired_grant(db: &Database) {
    db.conn_for_tests().execute_batch(
        "INSERT INTO users(id,username,created_at) VALUES ('operator-user','operator-user',0);
         INSERT INTO oauth_clients(id,client_id,client_secret_hash,name,owner_user_id,created_at)
         VALUES ('operator-client','operator-client','fixture-only','fixture','operator-user',0);
         INSERT INTO oauth_access_tokens(id,token_hash,client_id,subject_kind,subject_id,user_id,created_at,expires_at)
         VALUES ('z-known','fixture-only','operator-client','managed_user','operator-user','operator-user',0,1);",
    ).unwrap();
}

fn legacy() -> NewWindowOperatorMessage {
    NewWindowOperatorMessage {
        principal_kind: "oauth2".into(),
        principal_id: "z-known".into(),
        ..input()
    }
}

fn stable() -> NewWindowOperatorMessage {
    let (principal_kind, principal_id) =
        managed_oauth_operator_principal("operator-user", "operator-client").unwrap();
    NewWindowOperatorMessage {
        principal_kind,
        principal_id,
        ..input()
    }
}

fn reply(input: &NewWindowOperatorMessage, target: &str, index: usize) -> NewWindowModelReply {
    NewWindowModelReply {
        principal_kind: input.principal_kind.clone(),
        principal_id: input.principal_id.clone(),
        window_key: input.recipient_window_key.clone(),
        reply_to_message_id: target.into(),
        message: format!("reply-{index}"),
        created_at_ms: index as i64,
    }
}

#[test]
fn operator_migration_progress_passes_unattributed_mailboxes_before_grant_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("migration.db");
    let db = Database::open(&path).unwrap();
    seed_expired_grant(&db);
    // The former namespace LIMIT repeatedly selected these 4096 unrepairable
    // rows and never reached the valid recipient sorted after them.
    for index in 0..4096 {
        let mut missing = legacy();
        missing.principal_id = format!("a-unattributed-{index:04}");
        db.post_window_operator_message(missing, "retained-history")
            .unwrap();
    }
    let mut ids = Vec::new();
    for index in 0..130 {
        ids.push(delivered(
            db.post_window_operator_message(legacy(), &format!("key-{index}"))
                .unwrap(),
            false,
        ));
    }
    // Explicitly retained counters are state, not a reason to deliver again.
    db.conn_for_tests().execute(
        "UPDATE window_operator_messages SET first_projected_at_ms=11,last_projected_at_ms=12,projection_count=2,first_ack_observed_at_ms=13 WHERE message_id=?1",
        [&ids[0]],
    ).unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    let stable = stable();
    let rows = db
        .list_window_operator_messages(
            &stable.principal_kind,
            &stable.principal_id,
            &stable.recipient_window_key,
            512,
        )
        .unwrap();
    assert_eq!(rows.len(), ids.len());
    assert!(rows
        .iter()
        .any(|row| row.message_id == ids[0] && row.first_ack_observed_at_ms == Some(13)));
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM oauth_access_tokens", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM window_operator_messages WHERE principal_kind='oauth2'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        4096
    );
    let replay = db.post_window_operator_message(stable, "key-129").unwrap();
    assert_eq!(delivered(replay, true), ids[129]);
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
}

#[test]
fn operator_migration_late_reply_conflict_rolls_back_earlier_pages_and_preserves_replays() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("reply-migration.db")).unwrap();
    seed_expired_grant(&db);
    let old = legacy();
    let target = delivered(
        db.post_window_operator_message(old.clone(), "old-target")
            .unwrap(),
        false,
    );
    db.take_window_operator_attention(
        &old.principal_kind,
        &old.principal_id,
        &old.recipient_window_key,
        &[],
        11,
        4,
    )
    .unwrap();
    let mut replies = Vec::new();
    for index in 0..130 {
        let key = format!("reply-{index}");
        let input = reply(&old, &target, index);
        match db.post_window_model_reply(input.clone(), &key).unwrap() {
            WindowModelReplyDeliveryOutcome::Delivered {
                message_id,
                replayed: false,
            } => replies.push((message_id, key, input)),
            _ => panic!("expected a new reply"),
        }
    }
    replies.sort_by(|a, b| a.0.cmp(&b.0));
    let stable = stable();
    let stable_target = delivered(
        db.post_window_operator_message(stable.clone(), "stable-target")
            .unwrap(),
        false,
    );
    db.take_window_operator_attention(
        &stable.principal_kind,
        &stable.principal_id,
        &stable.recipient_window_key,
        &[],
        11,
        4,
    )
    .unwrap();
    let conflicting = reply(&stable, &stable_target, 999);
    let last = replies.last().unwrap();
    let collision_id = match db.post_window_model_reply(conflicting, &last.1).unwrap() {
        WindowModelReplyDeliveryOutcome::Delivered { message_id, .. } => message_id,
        _ => panic!("expected a collision fixture"),
    };
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    assert_eq!(
        db.list_window_model_replies("oauth2", "z-known", &old.recipient_window_key, 512)
            .unwrap()
            .len(),
        130
    );
    assert_eq!(
        db.list_window_model_replies(
            &stable.principal_kind,
            &stable.principal_id,
            &stable.recipient_window_key,
            512
        )
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        db.list_window_operator_messages("oauth2", "z-known", &old.recipient_window_key, 10)
            .unwrap()[0]
            .message_id,
        target
    );
    // Clearing only the conflicting fixture allows an exact, idempotent repair.
    db.conn_for_tests()
        .execute(
            "DELETE FROM window_model_replies WHERE message_id=?1",
            [collision_id],
        )
        .unwrap();
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        1
    );
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    let mut retried = last.2.clone();
    retried.principal_kind = stable.principal_kind.clone();
    retried.principal_id = stable.principal_id.clone();
    retried.created_at_ms = 9999;
    assert!(
        matches!(db.post_window_model_reply(retried, &last.1).unwrap(),
        WindowModelReplyDeliveryOutcome::Delivered { message_id, replayed: true } if message_id == last.0)
    );
    assert_eq!(
        db.list_window_model_replies(
            &stable.principal_kind,
            &stable.principal_id,
            &stable.recipient_window_key,
            512
        )
        .unwrap()
        .len(),
        130
    );
}

#[test]
fn operator_migration_nonmanaged_or_invalid_grants_never_gain_managed_identity() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("subject.db")).unwrap();
    seed_expired_grant(&db);
    let old = legacy();
    db.post_window_operator_message(old.clone(), "operator")
        .unwrap();
    db.conn_for_tests()
        .execute(
            "UPDATE oauth_access_tokens SET subject_id='mismatch' WHERE id='z-known'",
            [],
        )
        .unwrap();
    assert!(db
        .oauth_window_operator_principal(&old.recipient_window_key, "z-known")
        .unwrap()
        .is_none());
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    db.conn_for_tests().execute(
        "UPDATE oauth_access_tokens SET subject_kind='shared_key',subject_id=?1,shared_key_hash=?1,user_id=NULL WHERE id='z-known'",
        [&"a".repeat(64)],
    ).unwrap();
    assert_eq!(
        db.oauth_window_operator_principal(&old.recipient_window_key, "z-known")
            .unwrap(),
        Some(("oauth2".into(), "z-known".into()))
    );
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    assert_eq!(
        delivered(
            db.post_window_operator_message(old.clone(), "operator")
                .unwrap(),
            true
        ),
        db.list_window_operator_messages("oauth2", "z-known", &old.recipient_window_key, 10)
            .unwrap()[0]
            .message_id
    );
    let share = webcodex_core::authority::project_share_subject_id(
        &format!("wc_pgrant_{}", "a".repeat(24)),
        &format!("wc_share_{}", "b".repeat(64)),
    )
    .unwrap();
    db.conn_for_tests().execute(
        "UPDATE oauth_access_tokens SET subject_kind='project_share',subject_id=?1,shared_key_hash=NULL WHERE id='z-known'",
        [&share],
    ).unwrap();
    assert_eq!(
        db.oauth_window_operator_principal(&old.recipient_window_key, "z-known")
            .unwrap(),
        Some(("oauth2".into(), "z-known".into()))
    );
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    db.conn_for_tests()
        .execute(
            "DELETE FROM oauth_access_tokens WHERE id=?1",
            params!["z-known"],
        )
        .unwrap();
    assert!(db
        .oauth_window_operator_principal(&old.recipient_window_key, "z-known")
        .unwrap()
        .is_none());
}
