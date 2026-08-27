use super::model::{SessionMessageClosureKind, DEFAULT_MAX_MESSAGES_PER_SESSION};
use super::*;
use serde_json::Value;

fn post(
    store: &SessionStore,
    session_id: &str,
    kind: SessionMessageKind,
    message: &str,
    priority: SessionMessagePriority,
) -> SessionMessage {
    store
        .post_message(PostSessionMessageInput {
            session_id: session_id.to_string(),
            kind,
            message: message.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority,
        })
        .unwrap()
}

fn post_ack_guidance(store: &SessionStore, session_id: &str, message: &str) -> SessionMessage {
    store
        .post_message_with_ack(
            PostSessionMessageInput {
                session_id: session_id.to_string(),
                kind: SessionMessageKind::Guidance,
                message: message.to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::High,
            },
            true,
        )
        .unwrap()
}

fn replace_input(session_id: &str, message_id: &str, message: &str) -> ReplaceSessionMessageInput {
    ReplaceSessionMessageInput {
        session_id: session_id.to_string(),
        message_id: message_id.to_string(),
        message: message.to_string(),
    }
}

fn completion_input(session_id: &str, message_id: &str) -> CompleteSessionMessageInput {
    CompleteSessionMessageInput {
        session_id: session_id.to_string(),
        message_id: message_id.to_string(),
        answer: "completed answer".to_string(),
        tags: Vec::new(),
        priority: SessionMessagePriority::Normal,
        completion_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        author_session_id: None,
        expected_assignment_fence: None,
    }
}

fn exact(store: &SessionStore, session_id: &str, message_id: &str) -> SessionMessage {
    store
        .list_messages(
            session_id,
            ListSessionMessagesFilter {
                message_id: Some(message_id.to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .pop()
        .expect("retained message")
}

async fn baseline(store: &SessionStore, session_id: &str) -> String {
    store
        .observe_messages(session_id, None, None, Some(100))
        .await
        .unwrap()
        .observation_token
}

fn persisted_session<'a>(raw: &'a Value, session_id: &str) -> &'a Value {
    raw["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["session_id"] == session_id)
        .expect("persisted session")
}

#[test]
fn withdraw_open_message_is_history_preserving() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "mistyped retained body",
        SessionMessagePriority::Normal,
    );

    let outcome = store
        .withdraw_message(&session.session_id, &original.message_id)
        .unwrap();
    assert!(!outcome.replayed);
    assert_eq!(outcome.message.message_id, original.message_id);
    assert_eq!(outcome.message.message, "mistyped retained body");
    assert_eq!(outcome.message.status, SessionMessageStatus::Resolved);
    assert_eq!(
        outcome.message.closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
    assert!(outcome.message.resolved_at.is_some());
    assert!(outcome.message.resolution.is_none());
    assert_eq!(
        exact(&store, &session.session_id, &original.message_id).message,
        original.message
    );
}

#[test]
fn withdraw_before_ack_makes_stale_ack_ignored() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let guidance = post_ack_guidance(&store, &session.session_id, "withdraw before ACK");
    store
        .withdraw_message(&session.session_id, &guidance.message_id)
        .unwrap();

    let ack = store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&guidance.message_id),
    );
    assert_eq!(ack.accepted_count, 0);
    assert_eq!(ack.ignored_count, 1);
    let retained = exact(&store, &session.session_id, &guidance.message_id);
    assert_eq!(
        retained.closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
    assert!(retained.first_ack_observed_at.is_none());
    assert!(store
        .ack_required_guidance(&session.session_id, &[])
        .messages
        .is_empty());
}

#[test]
fn ack_before_withdraw_preserves_first_ack_history() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let guidance = post_ack_guidance(&store, &session.session_id, "ACK then withdraw");
    let ack = store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&guidance.message_id),
    );
    assert_eq!(ack.accepted_count, 1);
    let first_ack = exact(&store, &session.session_id, &guidance.message_id)
        .first_ack_observed_at
        .expect("ACK timestamp");

    let withdrawn = store
        .withdraw_message(&session.session_id, &guidance.message_id)
        .unwrap();
    assert_eq!(withdrawn.message.first_ack_observed_at, Some(first_ack));
    assert_eq!(
        withdrawn.message.closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
}

#[test]
fn replace_creates_fresh_message_id() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Question,
        "wrong question",
        SessionMessagePriority::Normal,
    );
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "correct question",
        ))
        .unwrap();

    assert_ne!(
        replaced.original.message_id,
        replaced.replacement.message_id
    );
    assert_eq!(replaced.original.message, "wrong question");
    assert_eq!(replaced.replacement.message, "correct question");
    assert_eq!(replaced.original.status, SessionMessageStatus::Resolved);
    assert_eq!(replaced.replacement.status, SessionMessageStatus::Open);
    assert_eq!(
        replaced.original.closure_kind,
        Some(SessionMessageClosureKind::Superseded)
    );
    assert_eq!(
        replaced.original.superseded_by_message_id.as_deref(),
        Some(replaced.replacement.message_id.as_str())
    );
    assert_eq!(
        replaced.replacement.supersedes_message_id.as_deref(),
        Some(original.message_id.as_str())
    );
}

#[test]
fn replace_preserves_kind_priority_tags_reply_to_and_requires_ack() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let parent = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "parent",
        SessionMessagePriority::Normal,
    );
    let original = store
        .post_message_with_ack(
            PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Guidance,
                message: "old guidance".to_string(),
                tags: vec!["constraint".to_string(), "operator".to_string()],
                reply_to: Some(parent.message_id.clone()),
                priority: SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new guidance",
        ))
        .unwrap();

    assert_eq!(replaced.replacement.kind, original.kind);
    assert_eq!(replaced.replacement.priority, original.priority);
    assert_eq!(replaced.replacement.tags, original.tags);
    assert_eq!(replaced.replacement.reply_to, original.reply_to);
    assert_eq!(replaced.replacement.requires_ack, original.requires_ack);
}

#[test]
fn replace_never_carries_first_ack_to_new_message() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post_ack_guidance(&store, &session.session_id, "old guidance");
    assert_eq!(
        store
            .observe_message_acks(
                &session.session_id,
                std::slice::from_ref(&original.message_id)
            )
            .accepted_count,
        1
    );
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "replacement guidance",
        ))
        .unwrap();
    assert!(replaced.original.first_ack_observed_at.is_some());
    assert!(replaced.replacement.first_ack_observed_at.is_none());
}

#[test]
fn stale_old_ack_does_not_ack_replacement() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post_ack_guidance(&store, &session.session_id, "replace first");
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "fresh replacement",
        ))
        .unwrap();

    let ack = store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&original.message_id),
    );
    assert_eq!(ack.accepted_count, 0);
    assert_eq!(ack.ignored_count, 1);
    assert!(exact(
        &store,
        &session.session_id,
        &replaced.replacement.message_id
    )
    .first_ack_observed_at
    .is_none());
}

#[test]
fn replacement_requires_fresh_ack() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post_ack_guidance(&store, &session.session_id, "old ACK target");
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new ACK target",
        ))
        .unwrap();
    let attention = store.ack_required_guidance(&session.session_id, &[]);
    assert_eq!(attention.messages.len(), 1);
    assert_eq!(
        attention.messages[0].message_id,
        replaced.replacement.message_id
    );

    let ack = store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&replaced.replacement.message_id),
    );
    assert_eq!(ack.accepted_count, 1);
    assert!(exact(
        &store,
        &session.session_id,
        &replaced.replacement.message_id
    )
    .first_ack_observed_at
    .is_some());
}

#[tokio::test]
async fn replace_allocates_two_ordered_observation_revisions() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "old",
        SessionMessagePriority::Normal,
    );
    let token = baseline(&store, &session.session_id).await;
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new",
        ))
        .unwrap();

    let first = store
        .observe_messages(&session.session_id, Some(&token), None, Some(1))
        .await
        .unwrap();
    assert!(first.has_more);
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].message_id, original.message_id);
    let second = store
        .observe_messages(
            &session.session_id,
            Some(&first.observation_token),
            None,
            Some(1),
        )
        .await
        .unwrap();
    assert!(!second.has_more);
    assert_eq!(second.messages.len(), 1);
    assert_eq!(
        second.messages[0].message_id,
        replaced.replacement.message_id
    );
}

#[tokio::test]
async fn withdraw_allocates_one_observation_revision() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let message = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "withdraw",
        SessionMessagePriority::Normal,
    );
    let token = baseline(&store, &session.session_id).await;
    store
        .withdraw_message(&session.session_id, &message.message_id)
        .unwrap();
    let delta = store
        .observe_messages(&session.session_id, Some(&token), None, Some(100))
        .await
        .unwrap();
    assert!(delta.changed);
    assert!(!delta.has_more);
    assert_eq!(delta.messages.len(), 1);
    assert_eq!(delta.messages[0].message_id, message.message_id);
    assert_eq!(
        delta.messages[0].closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
}

#[test]
fn replace_retention_keeps_old_and_new_transactionally() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "protected source",
        SessionMessagePriority::Normal,
    );
    for index in 0..(DEFAULT_MAX_MESSAGES_PER_SESSION - 1) {
        post(
            &store,
            &session.session_id,
            SessionMessageKind::Note,
            &format!("filler {index}"),
            SessionMessagePriority::Normal,
        );
    }
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "protected replacement",
        ))
        .unwrap();
    assert_eq!(
        exact(&store, &session.session_id, &original.message_id).closure_kind,
        Some(SessionMessageClosureKind::Superseded)
    );
    assert_eq!(
        exact(
            &store,
            &session.session_id,
            &replaced.replacement.message_id
        )
        .message,
        "protected replacement"
    );
    let replay = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "protected replacement",
        ))
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        replay.replacement.message_id,
        replaced.replacement.message_id
    );
}

#[test]
fn withdraw_replay_is_idempotent() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Question,
        "withdraw once",
        SessionMessagePriority::Normal,
    );
    let first = store
        .withdraw_message(&session.session_id, &original.message_id)
        .unwrap();
    let replay = store
        .withdraw_message(&session.session_id, &original.message_id)
        .unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(replay.message.message_id, first.message.message_id);
    assert_eq!(replay.message.resolved_at, first.message.resolved_at);
}

#[test]
fn replace_same_body_replay_returns_same_new_message() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "old",
        SessionMessagePriority::Normal,
    );
    let first = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new",
        ))
        .unwrap();
    let replay = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new",
        ))
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.replacement.message_id, first.replacement.message_id);
}

#[test]
fn replace_different_body_after_supersede_conflicts() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "old",
        SessionMessagePriority::Normal,
    );
    let first = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new A",
        ))
        .unwrap();
    assert!(matches!(
        store.replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "new B"
        )),
        Err(SessionMessageError::IdempotencyConflict)
    ));
    let retained = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(first.replacement.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(retained.len(), 1);
}

#[test]
fn completion_first_then_replace_fails_without_mutation() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "complete first",
        SessionMessagePriority::Normal,
    );
    let completion = store
        .complete_message(completion_input(&session.session_id, &todo.message_id))
        .unwrap();
    assert!(matches!(
        store.replace_message(replace_input(
            &session.session_id,
            &todo.message_id,
            "late edit"
        )),
        Err(SessionMessageError::MessageNotOpen)
    ));
    let retained = exact(&store, &session.session_id, &todo.message_id);
    assert_eq!(
        retained.resolved_by_message_id,
        Some(completion.answer.message_id)
    );
    assert!(retained.closure_kind.is_none());
}

#[test]
fn replace_first_then_old_completion_fails_without_answer() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "replace first",
        SessionMessagePriority::Normal,
    );
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &todo.message_id,
            "new work",
        ))
        .unwrap();
    assert!(matches!(
        store.complete_message(completion_input(&session.session_id, &todo.message_id)),
        Err(SessionMessageError::AlreadyCompleted {
            answer_message_id: None,
            completion_id: None,
        })
    ));
    let replies = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Answer),
                reply_to: Some(todo.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(replies.is_empty());
    assert_eq!(
        exact(
            &store,
            &session.session_id,
            &replaced.replacement.message_id
        )
        .status,
        SessionMessageStatus::Open
    );
}

#[test]
fn completion_first_then_withdraw_does_not_overwrite_completion() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "completed",
        SessionMessagePriority::Normal,
    );
    let completion = store
        .complete_message(completion_input(&session.session_id, &todo.message_id))
        .unwrap();
    assert!(matches!(
        store.withdraw_message(&session.session_id, &todo.message_id),
        Err(SessionMessageError::MessageNotOpen)
    ));
    let retained = exact(&store, &session.session_id, &todo.message_id);
    assert_eq!(
        retained.resolved_by_message_id,
        Some(completion.answer.message_id)
    );
    assert!(retained.closure_kind.is_none());
}

#[test]
fn ordinary_resolve_first_then_replace_fails() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let note = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "resolved normally",
        SessionMessagePriority::Normal,
    );
    store
        .resolve_message(
            &session.session_id,
            &note.message_id,
            Some("handled".to_string()),
        )
        .unwrap();
    assert!(matches!(
        store.replace_message(replace_input(
            &session.session_id,
            &note.message_id,
            "edited"
        )),
        Err(SessionMessageError::MessageNotOpen)
    ));
    assert_eq!(
        exact(&store, &session.session_id, &note.message_id)
            .resolution
            .as_deref(),
        Some("handled")
    );
}

#[test]
fn ordinary_resolve_first_then_withdraw_does_not_relabel_resolution() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let note = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "resolved normally",
        SessionMessagePriority::Normal,
    );
    store
        .resolve_message(
            &session.session_id,
            &note.message_id,
            Some("normal resolution".to_string()),
        )
        .unwrap();
    assert!(matches!(
        store.withdraw_message(&session.session_id, &note.message_id),
        Err(SessionMessageError::MessageNotOpen)
    ));
    let retained = exact(&store, &session.session_id, &note.message_id);
    assert_eq!(retained.resolution.as_deref(), Some("normal resolution"));
    assert!(retained.closure_kind.is_none());
}

#[test]
fn ack_first_then_replace_requires_fresh_new_ack() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let original = post_ack_guidance(&store, &session.session_id, "ACK old first");
    store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&original.message_id),
    );
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &original.message_id,
            "fresh body",
        ))
        .unwrap();
    assert!(replaced.original.first_ack_observed_at.is_some());
    assert!(replaced.replacement.first_ack_observed_at.is_none());
    assert_eq!(
        store
            .ack_required_guidance(&session.session_id, &[])
            .messages[0]
            .message_id,
        replaced.replacement.message_id
    );
}

#[test]
fn replace_first_then_stale_ack_is_ignored() {
    stale_old_ack_does_not_ack_replacement();
}

#[test]
fn ack_first_then_withdraw_preserves_ack_history() {
    ack_before_withdraw_preserves_first_ack_history();
}

#[test]
fn withdraw_first_then_ack_is_ignored() {
    withdraw_before_ack_makes_stale_ack_ignored();
}

#[tokio::test]
async fn observation_delta_for_replace_contains_old_then_new_and_pages_without_duplicates() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let old = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "before",
        SessionMessagePriority::Normal,
    );
    let token = baseline(&store, &session.session_id).await;
    let replaced = store
        .replace_message(replace_input(&session.session_id, &old.message_id, "after"))
        .unwrap();
    let first = store
        .observe_messages(&session.session_id, Some(&token), None, Some(1))
        .await
        .unwrap();
    assert!(first.has_more);
    assert_eq!(first.messages[0].message_id, old.message_id);
    let second = store
        .observe_messages(
            &session.session_id,
            Some(&first.observation_token),
            None,
            Some(100),
        )
        .await
        .unwrap();
    assert!(!second.has_more);
    assert_eq!(second.messages.len(), 1);
    assert_eq!(
        second.messages[0].message_id,
        replaced.replacement.message_id
    );
}

#[test]
fn withdraw_and_replace_observation_revisions_are_durable_and_consecutive() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let withdraw = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "withdraw revision",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    let before: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let before_revision = persisted_session(&before, &session.session_id)
        ["message_observation_revision"]
        .as_u64()
        .unwrap();
    store
        .withdraw_message(&session.session_id, &withdraw.message_id)
        .unwrap();
    let after_withdraw: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = persisted_session(&after_withdraw, &session.session_id);
    assert_eq!(
        record["message_observation_revision"].as_u64(),
        Some(before_revision + 1)
    );
    assert_eq!(
        record["message_observation_revisions"][&withdraw.message_id].as_u64(),
        Some(before_revision + 1)
    );

    let source = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "replace revision",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    let before_replace: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let base = persisted_session(&before_replace, &session.session_id)
        ["message_observation_revision"]
        .as_u64()
        .unwrap();
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &source.message_id,
            "replacement",
        ))
        .unwrap();
    let after_replace: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = persisted_session(&after_replace, &session.session_id);
    assert_eq!(
        record["message_observation_revision"].as_u64(),
        Some(base + 2)
    );
    assert_eq!(
        record["message_observation_revisions"][&source.message_id].as_u64(),
        Some(base + 1)
    );
    assert_eq!(
        record["message_observation_revisions"][&replaced.replacement.message_id].as_u64(),
        Some(base + 2)
    );
}

#[test]
fn withdraw_durable_restart_stays_withdrawn_and_out_of_attention() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let guidance = post_ack_guidance(&store, &session.session_id, "withdraw durably");
    store
        .withdraw_message(&session.session_id, &guidance.message_id)
        .unwrap();
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let message = exact(&restored, &session.session_id, &guidance.message_id);
    assert_eq!(
        message.closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
    assert_eq!(message.status, SessionMessageStatus::Resolved);
    assert!(restored
        .ack_required_guidance(&session.session_id, &[])
        .messages
        .is_empty());
}

#[test]
fn replace_durable_restart_preserves_links_and_fresh_ack_requirement() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let guidance = post_ack_guidance(&store, &session.session_id, "old durable guidance");
    store.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&guidance.message_id),
    );
    let replaced = store
        .replace_message(replace_input(
            &session.session_id,
            &guidance.message_id,
            "new durable guidance",
        ))
        .unwrap();
    let replacement_id = replaced.replacement.message_id.clone();
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let old = exact(&restored, &session.session_id, &guidance.message_id);
    let new = exact(&restored, &session.session_id, &replacement_id);
    assert_eq!(
        old.closure_kind,
        Some(SessionMessageClosureKind::Superseded)
    );
    assert_eq!(
        old.superseded_by_message_id.as_deref(),
        Some(replacement_id.as_str())
    );
    assert!(old.first_ack_observed_at.is_some());
    assert_eq!(
        new.supersedes_message_id.as_deref(),
        Some(guidance.message_id.as_str())
    );
    assert_eq!(new.status, SessionMessageStatus::Open);
    assert!(new.first_ack_observed_at.is_none());
    assert_eq!(
        restored
            .ack_required_guidance(&session.session_id, &[])
            .messages[0]
            .message_id,
        replacement_id
    );
}

#[test]
fn withdraw_persistence_failure_is_uncertain_then_exact_replay_recovers() {
    let root = tempfile::tempdir().unwrap();
    let ledger_dir = root.path().join("ledger");
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let ledger = ledger_dir.join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let message = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "uncertain withdraw",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    std::fs::remove_dir_all(&ledger_dir).unwrap();
    std::fs::write(&ledger_dir, b"block directory recreation").unwrap();

    assert!(matches!(
        store.withdraw_message(&session.session_id, &message.message_id),
        Err(SessionMessageError::PersistenceUncertain)
    ));
    assert_eq!(
        exact(&store, &session.session_id, &message.message_id).closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );

    std::fs::remove_file(&ledger_dir).unwrap();
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let replay = store
        .withdraw_message(&session.session_id, &message.message_id)
        .unwrap();
    assert!(replay.replayed);
    drop(store);
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    assert_eq!(
        exact(&restored, &session.session_id, &message.message_id).closure_kind,
        Some(SessionMessageClosureKind::Withdrawn)
    );
}

#[test]
fn replace_persistence_failure_replays_same_id_and_conflicts_on_different_body() {
    let root = tempfile::tempdir().unwrap();
    let ledger_dir = root.path().join("ledger");
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let ledger = ledger_dir.join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let message = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "uncertain replace",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    std::fs::remove_dir_all(&ledger_dir).unwrap();
    std::fs::write(&ledger_dir, b"block directory recreation").unwrap();

    assert!(matches!(
        store.replace_message(replace_input(
            &session.session_id,
            &message.message_id,
            "durable replacement",
        )),
        Err(SessionMessageError::PersistenceUncertain)
    ));
    let old = exact(&store, &session.session_id, &message.message_id);
    let replacement_id = old
        .superseded_by_message_id
        .clone()
        .expect("live replacement id");
    assert!(matches!(
        store.replace_message(replace_input(
            &session.session_id,
            &message.message_id,
            "different replacement",
        )),
        Err(SessionMessageError::IdempotencyConflict)
    ));

    std::fs::remove_file(&ledger_dir).unwrap();
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let replay = store
        .replace_message(replace_input(
            &session.session_id,
            &message.message_id,
            "durable replacement",
        ))
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.replacement.message_id, replacement_id);
    drop(store);
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    assert_eq!(
        exact(&restored, &session.session_id, &replacement_id).message,
        "durable replacement"
    );
}

#[test]
fn malformed_persisted_closure_metadata_is_not_replay_authority() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let message = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "retained despite malformed metadata",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let persisted = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|candidate| candidate["message_id"] == message.message_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    persisted.insert(
        "closure_kind".to_string(),
        Value::String("future_unknown".to_string()),
    );
    persisted.insert(
        "superseded_by_message_id".to_string(),
        Value::String("not-a-message-id".to_string()),
    );
    persisted.insert(
        "supersedes_message_id".to_string(),
        Value::String("wc_msg_evicted_but_syntactically_valid".to_string()),
    );
    std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let retained = exact(&restored, &session.session_id, &message.message_id);
    assert!(retained.closure_kind.is_none());
    assert!(retained.superseded_by_message_id.is_none());
    assert_eq!(
        retained.supersedes_message_id.as_deref(),
        Some("wc_msg_evicted_but_syntactically_valid")
    );
    assert_eq!(retained.message, "retained despite malformed metadata");
}

#[test]
fn resolve_cannot_relabel_withdrawn_or_superseded_closure() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let withdrawn = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "withdrawn",
        SessionMessagePriority::Normal,
    );
    store
        .withdraw_message(&session.session_id, &withdrawn.message_id)
        .unwrap();
    assert!(matches!(
        store.resolve_message(
            &session.session_id,
            &withdrawn.message_id,
            Some("should not overwrite".to_string()),
        ),
        Err(SessionMessageError::MessageNotOpen)
    ));
    assert!(exact(&store, &session.session_id, &withdrawn.message_id)
        .resolution
        .is_none());

    let source = post(
        &store,
        &session.session_id,
        SessionMessageKind::Question,
        "source",
        SessionMessagePriority::Normal,
    );
    store
        .replace_message(replace_input(
            &session.session_id,
            &source.message_id,
            "replacement",
        ))
        .unwrap();
    assert!(matches!(
        store.resolve_message(
            &session.session_id,
            &source.message_id,
            Some("should not overwrite".to_string()),
        ),
        Err(SessionMessageError::MessageNotOpen)
    ));
    assert!(exact(&store, &session.session_id, &source.message_id)
        .resolution
        .is_none());
}
