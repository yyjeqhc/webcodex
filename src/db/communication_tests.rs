use super::communication::*;
use super::Database;

fn principal(kind: &str, hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: kind.to_string(),
        digest: format!("wc_commprincipal_{}", hex.to_string().repeat(64)),
    }
}

fn new_agent(handle: &str, display_name: &str, key: &str) -> NewAgentIdentity {
    NewAgentIdentity {
        handle: handle.to_string(),
        display_name: display_name.to_string(),
        description: format!("{display_name} durable profile"),
        specialty_labels: vec!["architecture".to_string(), "rust".to_string()],
        idempotency_key: key.to_string(),
    }
}

fn endpoint(agent_id: &str, host: &str, key: &str) -> NewAgentEndpoint {
    NewAgentEndpoint {
        agent_id: agent_id.to_string(),
        host: host.to_string(),
        client_attachment_id: Some(format!("attachment-{key}")),
        wake_capable: false,
        controller_generation: Some("manual-v1".to_string()),
        idempotency_key: key.to_string(),
    }
}

fn conversation(agent_ids: Vec<String>, key: &str) -> NewConversation {
    NewConversation {
        title: Some("Architecture room".to_string()),
        agent_ids,
        idempotency_key: key.to_string(),
    }
}

fn human_message(
    conversation_id: &str,
    body: &str,
    recipient_agent_ids: Option<Vec<String>>,
    reply_to: Option<String>,
    key: &str,
) -> NewConversationMessage {
    NewConversationMessage {
        conversation_id: conversation_id.to_string(),
        body: body.to_string(),
        author_agent_id: None,
        endpoint_id: None,
        recipient_agent_ids,
        reply_to,
        idempotency_key: key.to_string(),
    }
}

fn agent_message(
    conversation_id: &str,
    agent_id: &str,
    endpoint_id: &str,
    body: &str,
    recipient_agent_ids: Option<Vec<String>>,
    reply_to: Option<String>,
    key: &str,
) -> NewConversationMessage {
    NewConversationMessage {
        conversation_id: conversation_id.to_string(),
        body: body.to_string(),
        author_agent_id: Some(agent_id.to_string()),
        endpoint_id: Some(endpoint_id.to_string()),
        recipient_agent_ids,
        reply_to,
        idempotency_key: key.to_string(),
    }
}

#[test]
fn durable_agent_identity_profile_collision_owner_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("communication.db");
    let owner = principal("user", 'a');
    let other = principal("user", 'b');
    let db = Database::open(&path).unwrap();

    let first = db
        .create_agent_identity(&owner, new_agent("reviewer", "Reviewer", "agent-a"))
        .unwrap();
    assert!(first.created && first.state_changed && !first.replayed);
    assert!(first.agent.agent_id.starts_with(DURABLE_AGENT_ID_PREFIX));
    assert_eq!(first.agent.profile_revision, 1);

    let replay = db
        .create_agent_identity(&owner, new_agent("reviewer", "Reviewer", "agent-a"))
        .unwrap();
    assert!(replay.replayed && !replay.created && !replay.state_changed);
    assert_eq!(replay.agent.agent_id, first.agent.agent_id);

    let conflict = db
        .create_agent_identity(&owner, new_agent("builder", "Builder", "agent-a"))
        .unwrap_err();
    assert_eq!(conflict.code(), "communication_idempotency_conflict");

    // Self-description collisions never define canonical identity.
    let second = db
        .create_agent_identity(&owner, new_agent("reviewer", "Reviewer", "agent-b"))
        .unwrap();
    assert_ne!(second.agent.agent_id, first.agent.agent_id);
    assert_eq!(second.agent.handle, first.agent.handle);
    assert_eq!(second.agent.display_name, first.agent.display_name);

    let updated = db
        .update_agent_identity(
            &owner,
            &first.agent.agent_id,
            1,
            AgentProfilePatch {
                display_name: Some("Architecture Reviewer".to_string()),
                description: Some("Reviews durable boundaries".to_string()),
                specialty_labels: Some(vec!["review".to_string(), "sqlite".to_string()]),
                ..AgentProfilePatch::default()
            },
        )
        .unwrap();
    assert!(updated.state_changed);
    assert_eq!(updated.agent.agent_id, first.agent.agent_id);
    assert_eq!(updated.agent.profile_revision, 2);
    assert_eq!(updated.agent.display_name, "Architecture Reviewer");

    assert_eq!(
        db.update_agent_identity(
            &owner,
            &first.agent.agent_id,
            1,
            AgentProfilePatch {
                description: Some("stale update".to_string()),
                ..AgentProfilePatch::default()
            },
        )
        .unwrap_err()
        .code(),
        "agent_profile_changed"
    );
    assert_eq!(
        db.update_agent_identity(
            &other,
            &first.agent.agent_id,
            2,
            AgentProfilePatch {
                description: Some("not allowed".to_string()),
                ..AgentProfilePatch::default()
            },
        )
        .unwrap_err()
        .code(),
        "agent_not_owned"
    );

    assert_eq!(
        db.list_agent_identities(&owner, None, 0, 10)
            .unwrap()
            .total_count,
        2
    );
    assert_eq!(
        db.list_agent_identities(&other, None, 0, 10)
            .unwrap()
            .total_count,
        0
    );
    assert!(db
        .list_agent_identities(&other, Some(&first.agent.agent_id), 0, 10)
        .unwrap()
        .agents
        .is_empty());

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let durable = reopened
        .list_agent_identities(&owner, Some(&first.agent.agent_id), 0, 10)
        .unwrap()
        .agents
        .pop()
        .unwrap();
    assert_eq!(durable.agent_id, first.agent.agent_id);
    assert_eq!(durable.profile_revision, 2);
    assert_eq!(durable.display_name, "Architecture Reviewer");
}

#[test]
fn endpoint_attachment_is_principal_bound_and_detach_preserves_agent() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("endpoint.db")).unwrap();
    let owner = principal("shared-key", 'c');
    let other = principal("shared-key", 'd');
    let agent = db
        .create_agent_identity(&owner, new_agent("worker", "Worker", "agent"))
        .unwrap()
        .agent;

    assert_eq!(
        db.attach_agent_endpoint(&other, endpoint(&agent.agent_id, "ChatGPT", "wrong"))
            .unwrap_err()
            .code(),
        "agent_not_owned"
    );
    let attached = db
        .attach_agent_endpoint(&owner, endpoint(&agent.agent_id, "ChatGPT", "window-a"))
        .unwrap();
    assert!(attached.created && attached.state_changed && !attached.replayed);
    assert!(attached
        .endpoint
        .endpoint_id
        .starts_with(AGENT_ENDPOINT_ID_PREFIX));

    let replay = db
        .attach_agent_endpoint(&owner, endpoint(&agent.agent_id, "ChatGPT", "window-a"))
        .unwrap();
    assert!(replay.replayed && !replay.state_changed);
    assert_eq!(replay.endpoint.endpoint_id, attached.endpoint.endpoint_id);

    assert_eq!(
        db.detach_agent_endpoint(&other, &attached.endpoint.endpoint_id)
            .unwrap_err()
            .code(),
        "endpoint_not_owned"
    );
    let detached = db
        .detach_agent_endpoint(&owner, &attached.endpoint.endpoint_id)
        .unwrap();
    assert!(detached.state_changed);
    assert!(detached.endpoint.detached_at_unix_ms.is_some());
    let desired_state_retry = db
        .detach_agent_endpoint(&owner, &attached.endpoint.endpoint_id)
        .unwrap();
    assert!(!desired_state_retry.state_changed);

    let after_detach = db
        .list_agent_identities(&owner, Some(&agent.agent_id), 0, 10)
        .unwrap()
        .agents
        .pop()
        .unwrap();
    assert_eq!(after_detach.agent_id, agent.agent_id);
    assert_eq!(after_detach.active_endpoint_count, 0);

    let replacement = db
        .attach_agent_endpoint(&owner, endpoint(&agent.agent_id, "ChatGPT", "window-b"))
        .unwrap();
    assert_ne!(
        replacement.endpoint.endpoint_id,
        attached.endpoint.endpoint_id
    );
    assert_eq!(replacement.endpoint.agent_id, agent.agent_id);
}

#[test]
fn conversation_transcript_delivery_replay_offline_and_restart_are_durable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("conversation.db");
    let owner = principal("user", 'e');
    let other = principal("user", 'f');
    let db = Database::open(&path).unwrap();
    let agent_a = db
        .create_agent_identity(&owner, new_agent("agent-a", "Agent A", "agent-a"))
        .unwrap()
        .agent;
    let agent_b = db
        .create_agent_identity(&owner, new_agent("agent-b", "Agent B", "agent-b"))
        .unwrap()
        .agent;

    assert_eq!(
        db.create_conversation(
            &other,
            conversation(vec![agent_a.agent_id.clone()], "foreign-room")
        )
        .unwrap_err()
        .code(),
        "agent_not_owned"
    );

    let created = db
        .create_conversation(
            &owner,
            conversation(
                vec![agent_b.agent_id.clone(), agent_a.agent_id.clone()],
                "room",
            ),
        )
        .unwrap();
    assert!(created.created && created.state_changed && !created.replayed);
    assert!(created
        .conversation
        .conversation
        .conversation_id
        .starts_with(CONVERSATION_ID_PREFIX));
    assert_eq!(created.conversation.participants.len(), 3);
    let conversation_id = created.conversation.conversation.conversation_id.clone();

    let room_replay = db
        .create_conversation(
            &owner,
            conversation(
                vec![agent_a.agent_id.clone(), agent_b.agent_id.clone()],
                "room",
            ),
        )
        .unwrap();
    assert!(room_replay.replayed && !room_replay.state_changed);
    assert_eq!(
        room_replay.conversation.conversation.conversation_id,
        conversation_id
    );

    let human = db
        .post_conversation_message(
            &owner,
            human_message(&conversation_id, "Human to both Agents", None, None, "m1"),
        )
        .unwrap();
    assert_eq!(human.message.seq, 1);
    assert_eq!(human.message.author.participant_kind, "human");
    assert_eq!(human.message.deliveries.len(), 2);

    let exact_retry = db
        .post_conversation_message(
            &owner,
            human_message(&conversation_id, "Human to both Agents", None, None, "m1"),
        )
        .unwrap();
    assert!(exact_retry.replayed && !exact_retry.state_changed);
    assert_eq!(exact_retry.message.message_id, human.message.message_id);
    assert_eq!(
        db.post_conversation_message(
            &owner,
            human_message(&conversation_id, "Changed payload", None, None, "m1"),
        )
        .unwrap_err()
        .code(),
        "communication_idempotency_conflict"
    );

    let endpoint_a = db
        .attach_agent_endpoint(&owner, endpoint(&agent_a.agent_id, "ChatGPT", "endpoint-a"))
        .unwrap()
        .endpoint;
    let endpoint_b = db
        .attach_agent_endpoint(&owner, endpoint(&agent_b.agent_id, "ChatGPT", "endpoint-b"))
        .unwrap()
        .endpoint;

    let from_a = db
        .post_conversation_message(
            &owner,
            agent_message(
                &conversation_id,
                &agent_a.agent_id,
                &endpoint_a.endpoint_id,
                "Agent A to Agent B",
                Some(vec![agent_b.agent_id.clone()]),
                Some(human.message.message_id.clone()),
                "m2",
            ),
        )
        .unwrap();
    assert_eq!(from_a.message.seq, 2);
    assert_eq!(
        from_a.message.author.agent_id.as_deref(),
        Some(agent_a.agent_id.as_str())
    );
    assert_eq!(from_a.message.deliveries.len(), 1);

    let from_b_to_room = db
        .post_conversation_message(
            &owner,
            agent_message(
                &conversation_id,
                &agent_b.agent_id,
                &endpoint_b.endpoint_id,
                "Agent B to Human / room",
                Some(Vec::new()),
                Some(from_a.message.message_id.clone()),
                "m3",
            ),
        )
        .unwrap();
    assert_eq!(from_b_to_room.message.seq, 3);
    assert!(from_b_to_room.message.deliveries.is_empty());

    let transcript = db
        .read_conversation(&owner, &ConversationAccess::Human, &conversation_id, 0, 10)
        .unwrap();
    assert_eq!(
        transcript
            .messages
            .iter()
            .map(|message| message.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(transcript.conversation.message_count, 3);
    assert_eq!(transcript.conversation.last_seq, 3);

    let inbox_a = db
        .list_agent_inbox(&owner, &agent_a.agent_id, &endpoint_a.endpoint_id, 0, 10)
        .unwrap();
    assert_eq!(inbox_a.total_queued_count, 1);
    assert_eq!(
        inbox_a.deliveries[0].message.message_id,
        human.message.message_id
    );

    let inbox_b = db
        .list_agent_inbox(&owner, &agent_b.agent_id, &endpoint_b.endpoint_id, 0, 10)
        .unwrap();
    assert_eq!(inbox_b.total_queued_count, 2);
    assert_eq!(
        inbox_b
            .deliveries
            .iter()
            .map(|item| item.message.seq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(inbox_b.deliveries[0].message.deliveries[0].delivery_order > 0);

    let b_delivery_ids = inbox_b
        .deliveries
        .iter()
        .map(|item| item.delivery_id.clone())
        .collect::<Vec<_>>();
    let mut expected_b_delivery_ids = b_delivery_ids.clone();
    expected_b_delivery_ids.sort();
    let consumed = db
        .consume_agent_deliveries(
            &owner,
            &agent_b.agent_id,
            &endpoint_b.endpoint_id,
            b_delivery_ids.clone(),
        )
        .unwrap();
    assert!(consumed.state_changed);
    assert_eq!(consumed.consumed_delivery_ids, expected_b_delivery_ids);
    let consume_retry = db
        .consume_agent_deliveries(
            &owner,
            &agent_b.agent_id,
            &endpoint_b.endpoint_id,
            b_delivery_ids.clone(),
        )
        .unwrap();
    assert!(!consume_retry.state_changed);
    assert_eq!(
        consume_retry.already_consumed_delivery_ids,
        expected_b_delivery_ids
    );
    assert_eq!(
        db.list_agent_inbox(&owner, &agent_a.agent_id, &endpoint_a.endpoint_id, 0, 10)
            .unwrap()
            .total_queued_count,
        1,
        "Agent B consumption must not mutate Agent A delivery state"
    );

    db.detach_agent_endpoint(&owner, &endpoint_b.endpoint_id)
        .unwrap();
    let offline = db
        .post_conversation_message(
            &owner,
            human_message(
                &conversation_id,
                "Queued while Agent B is offline",
                Some(vec![agent_b.agent_id.clone()]),
                None,
                "m4",
            ),
        )
        .unwrap();
    assert_eq!(offline.message.seq, 4);
    assert_eq!(offline.message.deliveries.len(), 1);

    let replacement_b = db
        .attach_agent_endpoint(
            &owner,
            endpoint(&agent_b.agent_id, "ChatGPT", "endpoint-b2"),
        )
        .unwrap()
        .endpoint;
    let recovered_inbox = db
        .list_agent_inbox(&owner, &agent_b.agent_id, &replacement_b.endpoint_id, 0, 10)
        .unwrap();
    assert_eq!(recovered_inbox.total_queued_count, 1);
    assert_eq!(
        recovered_inbox.deliveries[0].message.message_id,
        offline.message.message_id
    );

    let other_conversation = db
        .create_conversation(
            &owner,
            conversation(vec![agent_a.agent_id.clone()], "other-room"),
        )
        .unwrap()
        .conversation
        .conversation
        .conversation_id;
    assert_eq!(
        db.post_conversation_message(
            &owner,
            human_message(
                &other_conversation,
                "Invalid cross-room reply",
                None,
                Some(human.message.message_id.clone()),
                "cross-reply",
            ),
        )
        .unwrap_err()
        .code(),
        "reply_conversation_mismatch"
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let durable = reopened
        .read_conversation(&owner, &ConversationAccess::Human, &conversation_id, 0, 10)
        .unwrap();
    assert_eq!(durable.conversation.message_count, 4);
    assert_eq!(
        durable.messages.last().unwrap().message_id,
        offline.message.message_id
    );
    let agent_view = reopened
        .read_conversation(
            &owner,
            &ConversationAccess::Agent {
                agent_id: agent_b.agent_id.clone(),
                endpoint_id: replacement_b.endpoint_id.clone(),
            },
            &conversation_id,
            0,
            10,
        )
        .unwrap();
    assert_eq!(agent_view.conversation.queued_delivery_count, Some(1));
}

#[test]
fn exact_message_replay_survives_endpoint_detach_without_duplicate_delivery() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("detached-replay.db")).unwrap();
    let owner = principal("user", '8');
    let agent_a = db
        .create_agent_identity(&owner, new_agent("author", "Author", "author"))
        .unwrap()
        .agent;
    let agent_b = db
        .create_agent_identity(&owner, new_agent("recipient", "Recipient", "recipient"))
        .unwrap()
        .agent;
    let conversation_id = db
        .create_conversation(
            &owner,
            conversation(
                vec![agent_a.agent_id.clone(), agent_b.agent_id.clone()],
                "room",
            ),
        )
        .unwrap()
        .conversation
        .conversation
        .conversation_id;
    let endpoint_a = db
        .attach_agent_endpoint(
            &owner,
            endpoint(&agent_a.agent_id, "ChatGPT", "author-endpoint"),
        )
        .unwrap()
        .endpoint;

    let first = db
        .post_conversation_message(
            &owner,
            agent_message(
                &conversation_id,
                &agent_a.agent_id,
                &endpoint_a.endpoint_id,
                "Committed before the response was lost",
                Some(vec![agent_b.agent_id.clone()]),
                None,
                "detached-replay-message",
            ),
        )
        .unwrap();
    assert_eq!(first.message.seq, 1);
    assert_eq!(first.message.deliveries.len(), 1);

    db.detach_agent_endpoint(&owner, &endpoint_a.endpoint_id)
        .unwrap();
    let replay = db
        .post_conversation_message(
            &owner,
            agent_message(
                &conversation_id,
                &agent_a.agent_id,
                &endpoint_a.endpoint_id,
                "Committed before the response was lost",
                Some(vec![agent_b.agent_id.clone()]),
                None,
                "detached-replay-message",
            ),
        )
        .unwrap();
    assert!(replay.replayed);
    assert!(!replay.state_changed);
    assert_eq!(replay.message.message_id, first.message.message_id);
    assert_eq!(replay.message.deliveries, first.message.deliveries);

    assert_eq!(
        db.post_conversation_message(
            &owner,
            agent_message(
                &conversation_id,
                &agent_a.agent_id,
                &endpoint_a.endpoint_id,
                "Changed request must not bypass detached Endpoint validation",
                Some(vec![agent_b.agent_id.clone()]),
                None,
                "detached-replay-message",
            ),
        )
        .unwrap_err()
        .code(),
        "communication_idempotency_conflict"
    );

    let transcript = db
        .read_conversation(&owner, &ConversationAccess::Human, &conversation_id, 0, 10)
        .unwrap();
    assert_eq!(transcript.conversation.message_count, 1);
    assert_eq!(transcript.messages.len(), 1);
    assert_eq!(transcript.messages[0].message_id, first.message.message_id);
    let recipient = db
        .list_agent_identities(&owner, Some(&agent_b.agent_id), 0, 10)
        .unwrap()
        .agents
        .pop()
        .unwrap();
    assert_eq!(recipient.active_endpoint_count, 0);
    assert_eq!(recipient.queued_delivery_count, 1);
}

#[test]
fn message_and_deliveries_commit_atomically_after_dispatch_uncertainty() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("atomic.db")).unwrap();
    let owner = principal("user", '9');
    let agent = db
        .create_agent_identity(&owner, new_agent("atomic", "Atomic", "agent"))
        .unwrap()
        .agent;
    let conversation_id = db
        .create_conversation(&owner, conversation(vec![agent.agent_id.clone()], "room"))
        .unwrap()
        .conversation
        .conversation
        .conversation_id;

    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_delivery_insert
             BEFORE INSERT ON wc_agent_deliveries
             BEGIN SELECT RAISE(ABORT, 'forced delivery failure'); END;",
        )
        .unwrap();
    assert_eq!(
        db.post_conversation_message(
            &owner,
            human_message(&conversation_id, "must rollback", None, None, "message"),
        )
        .unwrap_err()
        .code(),
        "communication_store_unavailable"
    );
    {
        let conn = db.conn_for_tests();
        let message_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wc_conversation_messages", [], |row| {
                row.get(0)
            })
            .unwrap();
        let delivery_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wc_agent_deliveries", [], |row| {
                row.get(0)
            })
            .unwrap();
        let next_seq: i64 = conn
            .query_row(
                "SELECT next_seq FROM wc_conversations WHERE conversation_id = ?1",
                [&conversation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((message_count, delivery_count, next_seq), (0, 0, 1));
    }
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_delivery_insert;")
        .unwrap();

    let retry = db
        .post_conversation_message(
            &owner,
            human_message(&conversation_id, "must rollback", None, None, "message"),
        )
        .unwrap();
    assert_eq!(retry.message.seq, 1);
    assert_eq!(retry.message.deliveries.len(), 1);
}
