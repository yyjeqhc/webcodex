use super::agent_wake::{AgentWakeAttemptState, AgentWakeState};
use super::communication::{
    CommunicationPrincipal, ConversationAccess, NewAgentEndpoint, NewAgentIdentity,
    NewConversation, NewConversationMessage,
};
use super::Database;
use crate::agent_wake::{
    dispatch_next_agent_wake, AgentWakeDispatchReport, ContinuationAdapter,
    ContinuationDispatchOutcome, ContinuationPreflight, ContinuationPreflightError,
};
use crate::db::AgentWakeEnvelope;
use rusqlite::params;
use std::sync::Mutex;

#[derive(Clone)]
struct Fixture {
    owner: CommunicationPrincipal,
    sender_agent_id: String,
    receiver_agent_id: String,
    conversation_id: String,
}

fn principal(hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: "user".to_string(),
        digest: format!("wc_commprincipal_{}", hex.to_string().repeat(64)),
    }
}

fn create_fixture(db: &Database, hex: char) -> Fixture {
    let owner = principal(hex);
    let sender = db
        .create_agent_identity(
            &owner,
            NewAgentIdentity {
                handle: "wake-sender".to_string(),
                display_name: "Wake Sender".to_string(),
                description: "sender durable description".to_string(),
                specialty_labels: vec!["sender-specialty".to_string()],
                idempotency_key: "wake-sender-agent".to_string(),
            },
        )
        .unwrap()
        .agent;
    let receiver = db
        .create_agent_identity(
            &owner,
            NewAgentIdentity {
                handle: "wake-receiver".to_string(),
                display_name: "Wake Receiver".to_string(),
                description: "receiver durable description".to_string(),
                specialty_labels: vec!["receiver-specialty".to_string()],
                idempotency_key: "wake-receiver-agent".to_string(),
            },
        )
        .unwrap()
        .agent;
    let conversation_id = db
        .create_conversation(
            &owner,
            NewConversation {
                title: Some("Wake architecture room".to_string()),
                agent_ids: vec![sender.agent_id.clone(), receiver.agent_id.clone()],
                idempotency_key: "wake-conversation".to_string(),
            },
        )
        .unwrap()
        .conversation
        .conversation
        .conversation_id;
    Fixture {
        owner,
        sender_agent_id: sender.agent_id,
        receiver_agent_id: receiver.agent_id,
        conversation_id,
    }
}

fn post_to_receiver(db: &Database, fixture: &Fixture, body: &str, key: &str) -> String {
    db.post_conversation_message(
        &fixture.owner,
        NewConversationMessage {
            conversation_id: fixture.conversation_id.clone(),
            body: body.to_string(),
            author_agent_id: None,
            endpoint_id: None,
            expected_controller_generation: None,
            recipient_agent_ids: Some(vec![fixture.receiver_agent_id.clone()]),
            reply_to: None,
            idempotency_key: Some(key.to_string()),
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
    fixture: &Fixture,
    host: &str,
    key: &str,
) -> super::communication::AgentEndpointRecord {
    db.attach_agent_endpoint(
        &fixture.owner,
        NewAgentEndpoint {
            agent_id: fixture.receiver_agent_id.clone(),
            host: host.to_string(),
            client_attachment_id: Some(format!("attachment-{key}")),
            wake_capable: true,
            idempotency_key: key.to_string(),
        },
    )
    .unwrap()
    .endpoint
}

fn wake_id_for(db: &Database, agent_id: &str) -> String {
    db.conn_for_tests()
        .query_row(
            "SELECT wake_id FROM wc_agent_wakes
             WHERE target_agent_id = ?1
             ORDER BY created_at_unix_ms, wake_id LIMIT 1",
            [agent_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn queued_delivery_ids(db: &Database, agent_id: &str) -> Vec<String> {
    let conn = db.conn_for_tests();
    let mut statement = conn
        .prepare(
            "SELECT delivery_id FROM wc_agent_deliveries
             WHERE recipient_agent_id = ?1 AND state = 'queued'
             ORDER BY delivery_order",
        )
        .unwrap();
    statement
        .query_map([agent_id], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap()
}

#[derive(Debug)]
struct FakeContinuationAdapter {
    preflight_error: Option<&'static str>,
    outcome: ContinuationDispatchOutcome,
    preflights: Mutex<Vec<ContinuationPreflight>>,
    envelopes: Mutex<Vec<AgentWakeEnvelope>>,
}

impl FakeContinuationAdapter {
    fn new(preflight_error: Option<&'static str>, outcome: ContinuationDispatchOutcome) -> Self {
        Self {
            preflight_error,
            outcome,
            preflights: Mutex::new(Vec::new()),
            envelopes: Mutex::new(Vec::new()),
        }
    }

    fn dispatch_count(&self) -> usize {
        self.envelopes.lock().unwrap().len()
    }

    fn latest_envelope(&self) -> AgentWakeEnvelope {
        self.envelopes.lock().unwrap().last().unwrap().clone()
    }
}

impl ContinuationAdapter for FakeContinuationAdapter {
    fn adapter_kind(&self) -> &'static str {
        "deterministic_fake"
    }

    fn preflight(
        &self,
        continuation: &ContinuationPreflight,
    ) -> Result<(), ContinuationPreflightError> {
        self.preflights.lock().unwrap().push(continuation.clone());
        match self.preflight_error {
            Some(kind) => Err(ContinuationPreflightError::new(kind)),
            None => Ok(()),
        }
    }

    fn dispatch(&self, envelope: &AgentWakeEnvelope) -> ContinuationDispatchOutcome {
        self.envelopes.lock().unwrap().push(envelope.clone());
        self.outcome
    }
}

#[test]
fn offline_fifty_message_burst_preserves_facts_and_coalesces_wake() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("coalescing.db")).unwrap();
    let fixture = create_fixture(&db, '1');

    for index in 0..50 {
        post_to_receiver(
            &db,
            &fixture,
            &format!("offline durable message {index}"),
            &format!("burst-message-{index}"),
        );
    }

    let (message_count, delivery_count, wake_count): (i64, i64, i64) = {
        let conn = db.conn_for_tests();
        (
            conn.query_row("SELECT COUNT(*) FROM wc_conversation_messages", [], |row| {
                row.get(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM wc_agent_deliveries", [], |row| {
                row.get(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM wc_agent_wakes", [], |row| row.get(0))
                .unwrap(),
        )
    };
    assert_eq!((message_count, delivery_count, wake_count), (50, 50, 1));

    let wake_id = wake_id_for(&db, &fixture.receiver_agent_id);
    let wake = db.agent_wake(&wake_id).unwrap().unwrap();
    assert_eq!(wake.state, AgentWakeState::Pending);
    assert_eq!(wake.queued_delivery_count_snapshot, 50);
    assert!(wake.inbox_high_watermark >= 50);
    assert_ne!(
        wake.first_triggering_delivery_id,
        wake.latest_triggering_delivery_id
    );

    let agent = db
        .list_agent_identities(&fixture.owner, Some(&fixture.receiver_agent_id), 0, 10)
        .unwrap()
        .agents
        .pop()
        .unwrap();
    assert_eq!(agent.active_endpoint_count, 0);
    assert_eq!(agent.queued_delivery_count, 50);
    assert_eq!(agent.unresolved_wake_count, 1);
    assert_eq!(agent.latest_wake_id.as_deref(), Some(wake_id.as_str()));
    assert_eq!(agent.latest_wake_state.as_deref(), Some("pending"));

    let endpoint = attach_wake_endpoint(&db, &fixture, "fake-host", "burst-endpoint");
    let inbox = db
        .list_agent_inbox(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            0,
            100,
        )
        .unwrap();
    assert_eq!(inbox.total_queued_count, 50);
    assert_eq!(inbox.deliveries.len(), 50);

    let first_delivery_id = inbox.deliveries[0].delivery_id.clone();
    db.consume_agent_deliveries(
        &fixture.owner,
        &fixture.receiver_agent_id,
        &endpoint.endpoint_id,
        endpoint.controller_generation,
        vec![first_delivery_id],
    )
    .unwrap();
    assert_eq!(
        db.agent_wake(&wake_id).unwrap().unwrap().state,
        AgentWakeState::Pending,
        "Delivery consume must not consume or rewrite the logical Wake"
    );
    let transcript = db
        .read_conversation(
            &fixture.owner,
            &ConversationAccess::Human,
            &fixture.conversation_id,
            0,
            100,
        )
        .unwrap();
    assert_eq!(transcript.messages.len(), 50);
    assert_eq!(transcript.conversation.message_count, 50);
}

#[test]
fn replacement_generation_fences_old_claim_dispatch_wake_consume_and_inbox_consume() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("generation.db")).unwrap();
    let fixture = create_fixture(&db, '2');
    post_to_receiver(
        &db,
        &fixture,
        "generation-fenced work",
        "generation-message",
    );
    let delivery_id = queued_delivery_ids(&db, &fixture.receiver_agent_id)
        .pop()
        .unwrap();

    let generation_one = attach_wake_endpoint(&db, &fixture, "host-one", "endpoint-one");
    assert_eq!(generation_one.controller_generation, 1);
    let claim_one = db
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();

    let generation_two = attach_wake_endpoint(&db, &fixture, "host-two", "endpoint-two");
    assert_eq!(generation_two.controller_generation, 2);
    let lifecycle: String = db
        .conn_for_tests()
        .query_row(
            "SELECT lifecycle FROM wc_agent_endpoints WHERE endpoint_id = ?1",
            [&generation_one.endpoint_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lifecycle, "expired");
    assert_eq!(
        db.agent_wake(&claim_one.wake.wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Pending
    );
    assert_eq!(
        db.agent_wake_attempts(&claim_one.wake.wake_id).unwrap()[0].state,
        AgentWakeAttemptState::Revoked
    );

    let stale_claim = db
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            "deterministic_fake",
        )
        .unwrap_err();
    assert_eq!(stale_claim.code(), "endpoint_expired");
    assert_eq!(
        db.prepare_agent_wake_dispatch(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            &claim_one.wake.wake_id,
            &claim_one.attempt.attempt_id,
            &claim_one.claim_fence,
            &claim_one.consume_token,
        )
        .unwrap_err()
        .code(),
        "endpoint_expired"
    );
    assert_eq!(
        db.consume_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            &claim_one.wake.wake_id,
            &claim_one.consume_token,
        )
        .unwrap_err()
        .code(),
        "endpoint_expired"
    );
    assert_eq!(
        db.consume_agent_deliveries(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            vec![delivery_id],
        )
        .unwrap_err()
        .code(),
        "endpoint_expired"
    );

    let claim_two = db
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    assert_eq!(claim_two.wake.wake_id, claim_one.wake.wake_id);
    db.prepare_agent_wake_dispatch(
        &fixture.owner,
        &fixture.receiver_agent_id,
        &generation_two.endpoint_id,
        generation_two.controller_generation,
        &claim_two.wake.wake_id,
        &claim_two.attempt.attempt_id,
        &claim_two.claim_fence,
        &claim_two.consume_token,
    )
    .unwrap();
    let generation_three = attach_wake_endpoint(&db, &fixture, "host-three", "endpoint-three");
    assert_eq!(generation_three.controller_generation, 3);
    assert_eq!(
        db.verify_agent_wake_dispatch_binding(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            &claim_two.wake.wake_id,
            &claim_two.attempt.attempt_id,
            &claim_two.claim_fence,
        )
        .unwrap_err()
        .code(),
        "endpoint_expired",
        "a callback reaching the controller after replacement is fenced before Host invocation"
    );
    assert_eq!(
        db.agent_wake(&claim_two.wake.wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::DeliveryUnknown
    );
}

#[test]
fn fake_adapter_recovers_same_wake_before_fence_and_exact_consume_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("adapter.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, '3');
    let secret_body = "secret-message-body-must-not-enter-wake-payload";
    post_to_receiver(&db, &fixture, secret_body, "secret-message-idempotency-key");
    let logical_wake_id = wake_id_for(&db, &fixture.receiver_agent_id);
    let generation_one = attach_wake_endpoint(&db, &fixture, "host-one", "adapter-one");

    let rejected = FakeContinuationAdapter::new(
        Some("host_bridge_unavailable"),
        ContinuationDispatchOutcome::Delivered,
    );
    let report = dispatch_next_agent_wake(
        &db,
        &fixture.owner,
        &fixture.receiver_agent_id,
        &generation_one.endpoint_id,
        generation_one.controller_generation,
        &rejected,
    )
    .unwrap();
    match report {
        AgentWakeDispatchReport::ReleasedBeforeDispatch {
            wake,
            adapter_error_kind,
        } => {
            assert_eq!(wake.wake_id, logical_wake_id);
            assert_eq!(wake.state, AgentWakeState::Pending);
            assert_eq!(adapter_error_kind, "host_bridge_unavailable");
        }
        other => panic!("unexpected pre-dispatch report: {other:?}"),
    }
    assert_eq!(rejected.dispatch_count(), 0);

    let generation_two = attach_wake_endpoint(&db, &fixture, "host-two", "adapter-two");
    let delivered = FakeContinuationAdapter::new(None, ContinuationDispatchOutcome::Delivered);
    let report = dispatch_next_agent_wake(
        &db,
        &fixture.owner,
        &fixture.receiver_agent_id,
        &generation_two.endpoint_id,
        generation_two.controller_generation,
        &delivered,
    )
    .unwrap();
    match report {
        AgentWakeDispatchReport::Delivered { wake } => {
            assert_eq!(wake.wake_id, logical_wake_id);
            assert_eq!(wake.state, AgentWakeState::Delivered);
        }
        other => panic!("unexpected delivery report: {other:?}"),
    }
    let envelope = delivered.latest_envelope();
    assert_eq!(envelope.wake_id, logical_wake_id);
    assert_eq!(envelope.agent_id, fixture.receiver_agent_id);
    assert_eq!(envelope.endpoint_id, generation_two.endpoint_id);
    assert_eq!(envelope.controller_generation, 2);
    assert!(envelope
        .resume_hint
        .contains("read the authoritative Agent Inbox"));
    assert!(!envelope.resume_hint.contains(secret_body));
    assert!(!envelope
        .resume_hint
        .contains("receiver durable description"));
    assert!(!envelope.resume_hint.contains("receiver-specialty"));
    assert!(!envelope
        .resume_hint
        .contains("secret-message-idempotency-key"));
    assert!(!envelope.resume_hint.contains(&fixture.owner.digest));

    let first_consume = db
        .consume_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            &logical_wake_id,
            &envelope.consume_token,
        )
        .unwrap();
    assert!(first_consume.state_changed);
    assert!(!first_consume.already_consumed);
    let retry_consume = db
        .consume_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            &logical_wake_id,
            &envelope.consume_token,
        )
        .unwrap();
    assert!(!retry_consume.state_changed);
    assert!(retry_consume.already_consumed);
    assert_eq!(
        retry_consume.consumed_at_unix_ms,
        first_consume.consumed_at_unix_ms
    );
    assert_eq!(
        queued_delivery_ids(&db, &fixture.receiver_agent_id).len(),
        1
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let after_restart = reopened
        .consume_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            &logical_wake_id,
            &envelope.consume_token,
        )
        .unwrap();
    assert!(after_restart.already_consumed);
    assert!(!after_restart.state_changed);

    let generation_three = attach_wake_endpoint(&reopened, &fixture, "host-three", "adapter-three");
    assert_eq!(generation_three.controller_generation, 3);
    assert_eq!(
        reopened
            .consume_agent_wake(
                &fixture.owner,
                &fixture.receiver_agent_id,
                &generation_two.endpoint_id,
                generation_two.controller_generation,
                &logical_wake_id,
                &envelope.consume_token,
            )
            .unwrap_err()
            .code(),
        "endpoint_expired"
    );

    let delivery_ids = queued_delivery_ids(&reopened, &fixture.receiver_agent_id);
    reopened
        .consume_agent_deliveries(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_three.endpoint_id,
            generation_three.controller_generation,
            delivery_ids,
        )
        .unwrap();
    assert_eq!(
        reopened
            .agent_wake(&logical_wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Consumed,
        "Inbox consumption must not rewrite consumed Wake state"
    );
}

#[test]
fn dispatch_uncertainty_blocks_duplicate_and_preserves_successor_across_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("unknown.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, '4');
    post_to_receiver(&db, &fixture, "first durable work", "unknown-one");
    let first_wake_id = wake_id_for(&db, &fixture.receiver_agent_id);
    let generation_one =
        attach_wake_endpoint(&db, &fixture, "unknown-host-one", "unknown-endpoint-one");
    let unknown = FakeContinuationAdapter::new(None, ContinuationDispatchOutcome::OutcomeUnknown);
    match dispatch_next_agent_wake(
        &db,
        &fixture.owner,
        &fixture.receiver_agent_id,
        &generation_one.endpoint_id,
        generation_one.controller_generation,
        &unknown,
    )
    .unwrap()
    {
        AgentWakeDispatchReport::DeliveryUnknown { wake } => {
            assert_eq!(wake.wake_id, first_wake_id);
            assert_eq!(wake.state, AgentWakeState::DeliveryUnknown);
        }
        other => panic!("unexpected unknown report: {other:?}"),
    }
    assert_eq!(unknown.dispatch_count(), 1);

    post_to_receiver(&db, &fixture, "successor durable work", "unknown-two");
    let wake_states: Vec<(String, String)> = {
        let conn = db.conn_for_tests();
        let mut statement = conn
            .prepare(
                "SELECT wake_id, state FROM wc_agent_wakes
                 WHERE target_agent_id = ?1 ORDER BY created_at_unix_ms, wake_id",
            )
            .unwrap();
        statement
            .query_map([&fixture.receiver_agent_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(wake_states.len(), 2);
    assert!(wake_states
        .iter()
        .any(|(wake_id, state)| wake_id == &first_wake_id && state == "delivery_unknown"));
    assert!(wake_states.iter().any(|(_, state)| state == "pending"));
    assert_eq!(
        queued_delivery_ids(&db, &fixture.receiver_agent_id).len(),
        2
    );

    assert!(matches!(
        dispatch_next_agent_wake(
            &db,
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_one.endpoint_id,
            generation_one.controller_generation,
            &unknown,
        )
        .unwrap(),
        AgentWakeDispatchReport::NoPendingWake
    ));
    assert_eq!(
        unknown.dispatch_count(),
        1,
        "unknown dispatch must not be retried"
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened.agent_wake(&first_wake_id).unwrap().unwrap().state,
        AgentWakeState::DeliveryUnknown
    );
    let generation_two = attach_wake_endpoint(
        &reopened,
        &fixture,
        "unknown-host-two",
        "unknown-endpoint-two",
    );
    let replacement_adapter =
        FakeContinuationAdapter::new(None, ContinuationDispatchOutcome::Delivered);
    assert!(matches!(
        dispatch_next_agent_wake(
            &reopened,
            &fixture.owner,
            &fixture.receiver_agent_id,
            &generation_two.endpoint_id,
            generation_two.controller_generation,
            &replacement_adapter,
        )
        .unwrap(),
        AgentWakeDispatchReport::NoPendingWake
    ));
    assert_eq!(replacement_adapter.dispatch_count(), 0);
    assert_eq!(
        reopened.agent_wake_attempts(&first_wake_id).unwrap().len(),
        1
    );
}

#[test]
fn generic_reopen_preserves_live_pre_dispatch_claim_without_takeover() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("offline-restart.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, '5');
    post_to_receiver(
        &db,
        &fixture,
        "offline before attachment",
        "offline-message",
    );
    let wake_id = wake_id_for(&db, &fixture.receiver_agent_id);
    let endpoint = attach_wake_endpoint(&db, &fixture, "restart-host", "restart-endpoint");
    let first_claim = db
        .claim_next_agent_wake(
            &fixture.owner,
            &fixture.receiver_agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            "deterministic_fake",
        )
        .unwrap()
        .unwrap();
    assert_eq!(first_claim.wake.wake_id, wake_id);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        queued_delivery_ids(&reopened, &fixture.receiver_agent_id).len(),
        1
    );
    assert_eq!(
        reopened.agent_wake(&wake_id).unwrap().unwrap().state,
        AgentWakeState::Claimed,
        "opening the database must not assert that a live Wake owner died"
    );
    let attempts = reopened.agent_wake_attempts(&wake_id).unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].state, AgentWakeAttemptState::Claimed);
    assert_eq!(attempts[0].attempt_id, first_claim.attempt.attempt_id);
}

#[test]
fn generic_reopen_preserves_prepared_dispatch_without_inventing_takeover() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("prepared-restart.db");
    let db = Database::open(&path).unwrap();
    let fixture = create_fixture(&db, '6');
    post_to_receiver(&db, &fixture, "prepared before restart", "prepared-message");
    let endpoint = attach_wake_endpoint(&db, &fixture, "prepared-host", "prepared-endpoint");
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

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .agent_wake(&claim.wake.wake_id)
            .unwrap()
            .unwrap()
            .state,
        AgentWakeState::Prepared
    );
    assert_eq!(
        reopened.agent_wake_attempts(&claim.wake.wake_id).unwrap()[0].state,
        AgentWakeAttemptState::Prepared
    );
    assert!(
        reopened
            .claim_next_agent_wake(
                &fixture.owner,
                &fixture.receiver_agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                "deterministic_fake",
            )
            .unwrap()
            .is_none(),
        "a prepared Wake remains fenced until explicit authoritative recovery"
    );
}

#[test]
fn wake_storage_keeps_stable_refs_and_hashes_without_communication_payload() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("privacy.db")).unwrap();
    let fixture = create_fixture(&db, '7');
    let body = "private-body-not-a-wake-fact";
    post_to_receiver(&db, &fixture, body, "private-idempotency-key");
    let endpoint = attach_wake_endpoint(&db, &fixture, "privacy-host", "privacy-endpoint");
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

    let columns: Vec<String> = {
        let conn = db.conn_for_tests();
        let mut statement = conn.prepare("PRAGMA table_info(wc_agent_wakes)").unwrap();
        statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    for forbidden in [
        "body",
        "description",
        "specialty_labels",
        "principal_digest",
        "idempotency_key",
        "wake_payload",
        "credential",
    ] {
        assert!(!columns.iter().any(|column| column == forbidden));
    }

    let (claim_fence_hash, consume_token_hash): (String, String) = db
        .conn_for_tests()
        .query_row(
            "SELECT claim_fence_hash, consume_token_hash
             FROM wc_agent_wake_attempts WHERE attempt_id = ?1",
            params![claim.attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_ne!(claim_fence_hash, claim.claim_fence);
    assert_ne!(consume_token_hash, claim.consume_token);
    assert!(!claim_fence_hash.contains(&claim.claim_fence));
    assert!(!consume_token_hash.contains(&claim.consume_token));
    assert_eq!(claim.wake.trigger_kind, "inbox_changed");
    assert_eq!(claim.wake.state.as_str(), "claimed");
    assert!(!format!("{:?}", claim.wake).contains(body));
}
