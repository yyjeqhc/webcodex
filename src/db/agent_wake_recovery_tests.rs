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
            recipient_agent_ids: Some(vec![fixture.receiver_agent_id.clone()]),
            reply_to: None,
            idempotency_key: format!("migration-message-{index}"),
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

fn wake_id_for(db: &Database, agent_id: &str) -> String {
    db.conn_for_tests()
        .query_row(
            "SELECT wake_id FROM wc_agent_wakes WHERE target_agent_id = ?1
             ORDER BY created_at_unix_ms, wake_id LIMIT 1",
            [agent_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn wake_count_for(db: &Database, agent_id: &str) -> i64 {
    db.conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_wakes WHERE target_agent_id = ?1",
            [agent_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn queued_delivery_facts(db: &Database, agent_id: &str) -> Vec<(i64, String, String, String)> {
    let conn = db.conn_for_tests();
    let mut statement = conn
        .prepare(
            "SELECT delivery_order, delivery_id, conversation_id, message_id
             FROM wc_agent_deliveries
             WHERE recipient_agent_id = ?1 AND state = 'queued'
             ORDER BY delivery_order",
        )
        .unwrap();
    statement
        .query_map([agent_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn migration_marker_count(db: &Database) -> i64 {
    db.conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_wake_migrations
             WHERE migration_key = 'a1_queued_delivery_backfill_v1'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn strip_a2_wake_schema(db: &Database) {
    db.conn_for_tests()
        .execute_batch(
            "DROP TABLE wc_agent_wake_attempts;
             DROP TABLE wc_agent_wakes;
             DROP TABLE wc_agent_wake_migrations;",
        )
        .unwrap();
}

#[test]
fn a1_queued_deliveries_backfill_once_with_exact_backlog_facts() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a1-upgrade.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, 'a');
    let message_ids = (0..3)
        .map(|index| post_to_receiver(&db, &fixture, index))
        .collect::<Vec<_>>();
    let delivery_facts = queued_delivery_facts(&db, &fixture.receiver_agent_id);
    assert_eq!(delivery_facts.len(), 3);
    let message_count_before: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM wc_conversation_messages", [], |row| {
            row.get(0)
        })
        .unwrap();
    let delivery_count_before: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM wc_agent_deliveries", [], |row| {
            row.get(0)
        })
        .unwrap();

    // Remove only A2 Wake schema/results. The remaining Agent, Conversation,
    // Message, and Delivery tables are the durable A1 upgrade state.
    strip_a2_wake_schema(&db);
    drop(db);

    let upgraded = Database::open(&path).unwrap();
    assert_eq!(wake_count_for(&upgraded, &fixture.receiver_agent_id), 1);
    assert_eq!(migration_marker_count(&upgraded), 1);
    let wake_id = wake_id_for(&upgraded, &fixture.receiver_agent_id);
    let wake = upgraded.agent_wake(&wake_id).unwrap().unwrap();
    assert_eq!(wake.state, AgentWakeState::Pending);
    assert_eq!(wake.first_triggering_delivery_id, delivery_facts[0].1);
    assert_eq!(wake.latest_triggering_delivery_id, delivery_facts[2].1);
    assert_eq!(wake.latest_conversation_id, delivery_facts[2].2);
    assert_eq!(wake.latest_message_id, delivery_facts[2].3);
    assert_eq!(wake.latest_message_id, message_ids[2]);
    assert_eq!(wake.inbox_high_watermark, delivery_facts[2].0);
    assert_eq!(wake.queued_delivery_count_snapshot, 3);
    assert_eq!(
        upgraded
            .conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_conversation_messages", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        message_count_before
    );
    assert_eq!(
        upgraded
            .conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_deliveries", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        delivery_count_before
    );

    drop(upgraded);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(wake_count_for(&reopened, &fixture.receiver_agent_id), 1);
    drop(reopened);
    let reopened_again = Database::open(&path).unwrap();
    assert_eq!(
        wake_count_for(&reopened_again, &fixture.receiver_agent_id),
        1
    );

    let endpoint = attach_wake_endpoint(&reopened_again, &fixture, "migration-consume");
    let claim = reopened_again
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    reopened_again
        .prepare_agent_wake_dispatch(
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
    reopened_again
        .complete_agent_wake_delivery(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            &claim.wake.wake_id,
            &claim.attempt.attempt_id,
            &claim.claim_fence,
        )
        .unwrap();
    reopened_again
        .consume_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            &claim.wake.wake_id,
            &claim.consume_token,
        )
        .unwrap();
    assert_eq!(
        queued_delivery_facts(&reopened_again, &fixture.receiver_agent_id).len(),
        3
    );
    drop(reopened_again);

    let after_consumed_reopen = Database::open(&path).unwrap();
    assert_eq!(
        wake_count_for(&after_consumed_reopen, &fixture.receiver_agent_id),
        1
    );
    assert_eq!(
        after_consumed_reopen
            .agent_wake(&wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Consumed
    );
}

#[test]
fn buggy_a2_tables_without_marker_still_run_backfill_once() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("buggy-a2-upgrade.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, 'b');
    post_to_receiver(&db, &fixture, 0);
    post_to_receiver(&db, &fixture, 1);
    {
        let conn = db.conn_for_tests();
        conn.execute("DELETE FROM wc_agent_wakes", []).unwrap();
        conn.execute("DELETE FROM wc_agent_wake_migrations", [])
            .unwrap();
    }
    drop(db);

    let upgraded = Database::open(&path).unwrap();
    assert_eq!(wake_count_for(&upgraded, &fixture.receiver_agent_id), 1);
    assert_eq!(migration_marker_count(&upgraded), 1);
    drop(upgraded);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(wake_count_for(&reopened, &fixture.receiver_agent_id), 1);
}

#[test]
fn buggy_a2_without_marker_does_not_rewake_backlog_covered_by_historical_wake() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("buggy-a2-covered-backlog.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, '1');
    post_to_receiver(&db, &fixture, 0);
    let delivery_facts = queued_delivery_facts(&db, &fixture.receiver_agent_id);
    assert_eq!(delivery_facts.len(), 1);
    let wake_id = wake_id_for(&db, &fixture.receiver_agent_id);
    let endpoint = attach_wake_endpoint(&db, &fixture, "covered-history");
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
    .unwrap();
    db.complete_agent_wake_delivery(
        &fixture.owner,
        &fixture.receiver_agent_id,
        &endpoint.endpoint_id,
        endpoint.controller_generation,
        &claim.wake.wake_id,
        &claim.attempt.attempt_id,
        &claim.claim_fence,
    )
    .unwrap();
    db.consume_agent_wake(
        &fixture.owner,
        &fixture.receiver_agent_id,
        &endpoint.endpoint_id,
        endpoint.controller_generation,
        &claim.wake.wake_id,
        &claim.consume_token,
    )
    .unwrap();
    let consumed = db.agent_wake(&wake_id).unwrap().unwrap();
    assert_eq!(consumed.state, AgentWakeState::Consumed);
    assert_eq!(consumed.inbox_high_watermark, delivery_facts[0].0);
    assert_eq!(
        queued_delivery_facts(&db, &fixture.receiver_agent_id).len(),
        1
    );
    db.conn_for_tests()
        .execute("DELETE FROM wc_agent_wake_migrations", [])
        .unwrap();
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(migration_marker_count(&reopened), 1);
    assert_eq!(wake_count_for(&reopened, &fixture.receiver_agent_id), 1);
    assert_eq!(
        reopened.agent_wake(&wake_id).unwrap().unwrap().state,
        AgentWakeState::Consumed
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
