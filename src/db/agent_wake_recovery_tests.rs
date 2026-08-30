use super::agent_wake::{AgentWakeAttemptState, AgentWakeState};
use super::communication::{
    CommunicationPrincipal, NewAgentEndpoint, NewAgentIdentity, NewConversation,
    NewConversationMessage,
};
use super::Database;
use crate::server_instance::ServerInstanceGuard;
use rusqlite::params;

#[derive(Clone)]
struct RecoveryFixture {
    owner: CommunicationPrincipal,
    receiver_agent_id: String,
    conversation_id: String,
}

fn principal(hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: "user".to_string(),
        digest: format!("wc_commprincipal_{}", hex.to_string().repeat(64)),
    }
}

fn create_fixture(db: &Database, hex: char) -> RecoveryFixture {
    let owner = principal(hex);
    let sender = db
        .create_agent_identity(
            &owner,
            NewAgentIdentity {
                handle: format!("migration-sender-{hex}"),
                display_name: "Migration Sender".to_string(),
                description: "sender description must stay outside Wake storage".to_string(),
                specialty_labels: vec!["sender-specialty".to_string()],
                idempotency_key: format!("migration-sender-agent-{hex}"),
            },
        )
        .unwrap()
        .agent;
    let receiver = db
        .create_agent_identity(
            &owner,
            NewAgentIdentity {
                handle: format!("migration-receiver-{hex}"),
                display_name: "Migration Receiver".to_string(),
                description: "receiver description must stay outside Wake storage".to_string(),
                specialty_labels: vec!["receiver-specialty".to_string()],
                idempotency_key: format!("migration-receiver-agent-{hex}"),
            },
        )
        .unwrap()
        .agent;
    let conversation_id = db
        .create_conversation(
            &owner,
            NewConversation {
                title: Some("Wake migration room".to_string()),
                agent_ids: vec![sender.agent_id, receiver.agent_id.clone()],
                idempotency_key: format!("migration-conversation-{hex}"),
            },
        )
        .unwrap()
        .conversation
        .conversation
        .conversation_id;
    RecoveryFixture {
        owner,
        receiver_agent_id: receiver.agent_id,
        conversation_id,
    }
}

fn post_to_receiver(db: &Database, fixture: &RecoveryFixture, index: usize) -> String {
    db.post_conversation_message(
        &fixture.owner,
        NewConversationMessage {
            conversation_id: fixture.conversation_id.clone(),
            body: format!("private migration body {index}"),
            author_agent_id: None,
            endpoint_id: None,
            expected_controller_generation: None,
            recipient_agent_ids: Some(vec![fixture.receiver_agent_id.clone()]),
            reply_to: None,
            idempotency_key: Some(format!("migration-message-{index}")),
            wake_reply_id: None,
            reply_operation_index: None,
        },
    )
    .unwrap()
    .message
    .message_id
}

fn attach_wake_endpoint(
    db: &Database,
    fixture: &RecoveryFixture,
    suffix: &str,
) -> super::communication::AgentEndpointRecord {
    db.attach_agent_endpoint(
        &fixture.owner,
        NewAgentEndpoint {
            agent_id: fixture.receiver_agent_id.clone(),
            host: format!("recovery-host-{suffix}"),
            client_attachment_id: Some(format!("recovery-attachment-{suffix}")),
            wake_capable: true,
            idempotency_key: format!("recovery-endpoint-{suffix}"),
        },
    )
    .unwrap()
    .endpoint
}

#[test]
fn pre_wake_delivery_state_is_rejected_instead_of_backfilled() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pre-wake.db");
    {
        let db = Database::open(&path).unwrap();
        let fixture = create_fixture(&db, 'a');
        post_to_receiver(&db, &fixture, 0);
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "DROP TABLE wc_agent_wake_attempts;
             DROP TABLE wc_agent_wakes;",
        )
        .unwrap();
    }

    let error = match Database::open(&path) {
        Ok(_) => panic!("wake-less persisted deliveries must be rejected"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("existing deliveries are not backfilled"),
        "{error:#}"
    );
    let conn = rusqlite::Connection::open(&path).unwrap();
    let wake_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('wc_agent_wakes', 'wc_agent_wake_attempts')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        wake_tables, 0,
        "rejected state must not be partially initialized"
    );
}

#[test]
fn generic_database_open_does_not_take_over_claimed_or_prepared_wakes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("concurrent-open.db");
    let db1 = Database::open(&path).unwrap();
    let fixture = create_fixture(&db1, 'c');
    post_to_receiver(&db1, &fixture, 0);
    let endpoint = attach_wake_endpoint(&db1, &fixture, "concurrent");
    let claim = db1
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    let before_claim: (String, String, String, i64) = db1
        .conn_for_tests()
        .query_row(
            "SELECT w.state, a.claim_fence_hash, a.endpoint_id, a.controller_generation
             FROM wc_agent_wakes w JOIN wc_agent_wake_attempts a
               ON a.attempt_id = w.claimed_attempt_id
             WHERE w.wake_id = ?1",
            [claim.wake.wake_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    let db2 = Database::open(&path).unwrap();
    assert_eq!(
        db2.agent_wake(&claim.wake.wake_id).unwrap().unwrap().state,
        AgentWakeState::Claimed
    );
    assert_eq!(
        db2.agent_wake_attempts(&claim.wake.wake_id).unwrap()[0].state,
        AgentWakeAttemptState::Claimed
    );
    let after_claim: (String, String, String, i64) = db2
        .conn_for_tests()
        .query_row(
            "SELECT w.state, a.claim_fence_hash, a.endpoint_id, a.controller_generation
             FROM wc_agent_wakes w JOIN wc_agent_wake_attempts a
               ON a.attempt_id = w.claimed_attempt_id
             WHERE w.wake_id = ?1",
            [claim.wake.wake_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(after_claim, before_claim);
    drop(db2);

    db1.prepare_agent_wake_dispatch(
        &fixture.owner,
        &fixture.receiver_agent_id,
        &endpoint.endpoint_id,
        endpoint.controller_generation,
        &claim.wake.wake_id,
        &claim.attempt.attempt_id,
        &claim.claim_fence,
        &claim.consume_token,
    )
    .unwrap();
    let db3 = Database::open(&path).unwrap();
    assert_eq!(
        db3.agent_wake(&claim.wake.wake_id).unwrap().unwrap().state,
        AgentWakeState::Prepared
    );
    assert_eq!(
        db3.agent_wake_attempts(&claim.wake.wake_id).unwrap()[0].state,
        AgentWakeAttemptState::Prepared
    );
}

#[test]
fn explicit_server_takeover_recovers_once_after_new_owner_acquires_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("takeover.db");
    let db = Database::open(&path).unwrap();
    let owner_guard = ServerInstanceGuard::acquire(&db).unwrap();

    let claimed_fixture = create_fixture(&db, 'd');
    post_to_receiver(&db, &claimed_fixture, 0);
    let claimed_endpoint = attach_wake_endpoint(&db, &claimed_fixture, "claimed");
    let claimed = db
        .claim_next_agent_wake(
            &claimed_fixture.owner,
            &claimed_fixture.receiver_agent_id,
            &claimed_endpoint.endpoint_id,
            claimed_endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();

    let prepared_fixture = create_fixture(&db, 'e');
    post_to_receiver(&db, &prepared_fixture, 0);
    let prepared_endpoint = attach_wake_endpoint(&db, &prepared_fixture, "prepared");
    let prepared = db
        .claim_next_agent_wake(
            &prepared_fixture.owner,
            &prepared_fixture.receiver_agent_id,
            &prepared_endpoint.endpoint_id,
            prepared_endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    db.prepare_agent_wake_dispatch(
        &prepared_fixture.owner,
        &prepared_fixture.receiver_agent_id,
        &prepared_endpoint.endpoint_id,
        prepared_endpoint.controller_generation,
        &prepared.wake.wake_id,
        &prepared.attempt.attempt_id,
        &prepared.claim_fence,
        &prepared.consume_token,
    )
    .unwrap();

    let queued_before: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_deliveries WHERE state = 'queued'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let generations_before: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT SUM(current_controller_generation) FROM wc_agent_identities",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(owner_guard);
    drop(db);

    let successor = Database::open(&path).unwrap();
    assert_eq!(
        successor
            .agent_wake(&claimed.wake.wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Claimed
    );
    assert_eq!(
        successor
            .agent_wake(&prepared.wake.wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Prepared
    );
    let successor_guard = ServerInstanceGuard::acquire(&successor).unwrap();

    let unrelated = Database::open(&temp.path().join("unrelated-takeover.db")).unwrap();
    let unrelated_guard = ServerInstanceGuard::acquire(&unrelated).unwrap();
    assert_eq!(
        successor
            .recover_agent_wakes_for_server_takeover(
                &unrelated_guard,
                chrono::Utc::now().timestamp_millis()
            )
            .unwrap_err()
            .code(),
        "server_instance_ownership_mismatch"
    );

    let now = chrono::Utc::now().timestamp_millis();
    successor
        .recover_agent_wakes_for_server_takeover(&successor_guard, now)
        .unwrap();
    let claimed_after = successor
        .agent_wake(&claimed.wake.wake_id)
        .unwrap()
        .unwrap();
    let prepared_after = successor
        .agent_wake(&prepared.wake.wake_id)
        .unwrap()
        .unwrap();
    assert_eq!(claimed_after.state, AgentWakeState::Pending);
    assert_eq!(claimed_after.revision, claimed.wake.revision + 1);
    assert_eq!(
        successor
            .agent_wake_attempts(&claimed.wake.wake_id)
            .unwrap()[0]
            .state,
        AgentWakeAttemptState::Revoked
    );
    assert_eq!(prepared_after.state, AgentWakeState::DeliveryUnknown);
    assert_eq!(prepared_after.revision, prepared.wake.revision + 2);
    assert_eq!(
        successor
            .agent_wake_attempts(&prepared.wake.wake_id)
            .unwrap()[0]
            .state,
        AgentWakeAttemptState::DeliveryUnknown
    );
    assert_eq!(
        successor
            .conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_deliveries WHERE state = 'queued'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        queued_before
    );
    assert_eq!(
        successor
            .conn_for_tests()
            .query_row(
                "SELECT SUM(current_controller_generation) FROM wc_agent_identities",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        generations_before
    );

    let claimed_revision = claimed_after.revision;
    let prepared_revision = prepared_after.revision;
    let claimed_revoked_at = successor
        .agent_wake_attempts(&claimed.wake.wake_id)
        .unwrap()[0]
        .revoked_at_unix_ms;
    let prepared_unknown_at = successor
        .agent_wake_attempts(&prepared.wake.wake_id)
        .unwrap()[0]
        .delivery_unknown_at_unix_ms;
    successor
        .recover_agent_wakes_for_server_takeover(&successor_guard, now.saturating_add(1))
        .unwrap();
    assert_eq!(
        successor
            .agent_wake(&claimed.wake.wake_id)
            .unwrap()
            .unwrap()
            .revision,
        claimed_revision
    );
    assert_eq!(
        successor
            .agent_wake(&prepared.wake.wake_id)
            .unwrap()
            .unwrap()
            .revision,
        prepared_revision
    );
    assert_eq!(
        successor
            .agent_wake_attempts(&claimed.wake.wake_id)
            .unwrap()[0]
            .revoked_at_unix_ms,
        claimed_revoked_at
    );
    assert_eq!(
        successor
            .agent_wake_attempts(&prepared.wake.wake_id)
            .unwrap()[0]
            .delivery_unknown_at_unix_ms,
        prepared_unknown_at
    );
}

#[test]
fn expired_endpoint_fails_closed_then_replacement_lazily_materializes_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("lazy-lease.db")).unwrap();
    let fixture = create_fixture(&db, 'f');
    post_to_receiver(&db, &fixture, 0);
    let endpoint = attach_wake_endpoint(&db, &fixture, "expired");
    let claim = db
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_endpoints SET lease_expires_at_unix_ms = 0 WHERE endpoint_id = ?1",
            params![endpoint.endpoint_id],
        )
        .unwrap();

    assert_eq!(
        db.prepare_agent_wake_dispatch(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            &claim.wake.wake_id,
            &claim.attempt.attempt_id,
            &claim.claim_fence,
            &claim.consume_token,
        )
        .unwrap_err()
        .code(),
        "endpoint_expired"
    );
    let lifecycle_before: String = db
        .conn_for_tests()
        .query_row(
            "SELECT lifecycle FROM wc_agent_endpoints WHERE endpoint_id = ?1",
            [endpoint.endpoint_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lifecycle_before, "attached");
    assert_eq!(
        db.agent_wake(&claim.wake.wake_id).unwrap().unwrap().state,
        AgentWakeState::Claimed
    );

    let replacement = attach_wake_endpoint(&db, &fixture, "replacement");
    assert_eq!(
        replacement.controller_generation,
        endpoint.controller_generation + 1
    );
    let lifecycle_after: String = db
        .conn_for_tests()
        .query_row(
            "SELECT lifecycle FROM wc_agent_endpoints WHERE endpoint_id = ?1",
            [endpoint.endpoint_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lifecycle_after, "expired");
    assert_eq!(
        db.agent_wake(&claim.wake.wake_id).unwrap().unwrap().state,
        AgentWakeState::Pending
    );
    assert_eq!(
        db.agent_wake_attempts(&claim.wake.wake_id).unwrap()[0].state,
        AgentWakeAttemptState::Revoked
    );
}
