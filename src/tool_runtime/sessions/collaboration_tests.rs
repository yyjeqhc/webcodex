use super::messages::encode_observation_token;
use super::model::{
    SessionMessageObservationOutcome, DEFAULT_MAX_MESSAGES_PER_SESSION,
    MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS,
};
use super::*;
use serde_json::Value;

fn completion_id(byte: char) -> String {
    byte.to_string()
        .repeat(MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS)
}

async fn baseline(store: &SessionStore, session_id: &str) -> SessionMessageObservationOutcome {
    store
        .observe_messages(session_id, None, None, None)
        .await
        .unwrap()
}

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

#[tokio::test]
async fn observe_session_messages_baseline_append_resolve_completion_and_replay() {
    let store = SessionStore::default();
    let session = store.start_session(Some("proj".to_string()), None);

    let empty_baseline = baseline(&store, &session.session_id).await;
    assert!(empty_baseline.messages.is_empty());
    assert!(!empty_baseline.changed);
    assert_eq!(empty_baseline.wait_outcome, "immediate");
    assert_eq!(empty_baseline.waited_ms, 0);
    assert!(!empty_baseline.history_lost);
    assert!(!empty_baseline.has_more);

    let note = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "delta note",
        SessionMessagePriority::Normal,
    );
    let appended = store
        .observe_messages(
            &session.session_id,
            Some(&empty_baseline.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(appended.changed);
    assert_eq!(appended.messages.len(), 1);
    assert_eq!(appended.messages[0].message_id, note.message_id);
    let caught_up = store
        .observe_messages(
            &session.session_id,
            Some(&appended.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!caught_up.changed);
    assert!(caught_up.messages.is_empty());

    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "resolve me",
        SessionMessagePriority::High,
    );
    let before_resolve = baseline(&store, &session.session_id).await;
    let resolved = store
        .resolve_message(
            &session.session_id,
            &todo.message_id,
            Some("done".to_string()),
        )
        .unwrap();
    assert_eq!(resolved.status, SessionMessageStatus::Resolved);
    let resolution_delta = store
        .observe_messages(
            &session.session_id,
            Some(&before_resolve.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(resolution_delta.messages.len(), 1);
    assert_eq!(resolution_delta.messages[0].message_id, todo.message_id);
    assert_eq!(
        resolution_delta.messages[0].resolution.as_deref(),
        Some("done")
    );
    let unchanged_token = resolution_delta.observation_token.clone();
    let no_op = store
        .resolve_message(
            &session.session_id,
            &todo.message_id,
            Some("done".to_string()),
        )
        .unwrap();
    assert_eq!(no_op.resolution.as_deref(), Some("done"));
    let after_no_op = store
        .observe_messages(&session.session_id, Some(&unchanged_token), None, None)
        .await
        .unwrap();
    assert!(!after_no_op.changed);
    assert!(after_no_op.messages.is_empty());

    let completion_todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "complete me",
        SessionMessagePriority::High,
    );
    let before_completion = baseline(&store, &session.session_id).await;
    let input = CompleteSessionMessageInput {
        session_id: session.session_id.clone(),
        message_id: completion_todo.message_id.clone(),
        answer: "completed".to_string(),
        tags: vec!["done".to_string()],
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id('a'),
        author_session_id: None,
    };
    let completion = store.complete_message(input.clone()).unwrap();
    assert!(!completion.replayed);
    let completion_delta = store
        .observe_messages(
            &session.session_id,
            Some(&before_completion.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(completion_delta.messages.len(), 2);
    assert_eq!(
        completion_delta.messages[0].message_id,
        completion_todo.message_id
    );
    assert_eq!(
        completion_delta.messages[0].status,
        SessionMessageStatus::Resolved
    );
    assert_eq!(
        completion_delta.messages[1].message_id,
        completion.answer.message_id
    );

    let replay = store.complete_message(input).unwrap();
    assert!(replay.replayed);
    let after_replay = store
        .observe_messages(
            &session.session_id,
            Some(&completion_delta.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!after_replay.changed);
    assert!(after_replay.messages.is_empty());
}

#[tokio::test]
async fn observe_session_messages_pagination_advances_only_through_returned_changes() {
    let store = SessionStore::default();
    let session = store.start_session(Some("proj".to_string()), None);
    let first = baseline(&store, &session.session_id).await;
    let mut expected = Vec::new();
    for index in 0..7 {
        expected.push(
            post(
                &store,
                &session.session_id,
                SessionMessageKind::Note,
                &format!("page-{index}"),
                SessionMessagePriority::Normal,
            )
            .message_id,
        );
    }

    let mut token = first.observation_token;
    let mut observed = Vec::new();
    loop {
        let page = store
            .observe_messages(&session.session_id, Some(&token), None, Some(2))
            .await
            .unwrap();
        observed.extend(
            page.messages
                .iter()
                .map(|message| message.message_id.clone()),
        );
        token = page.observation_token;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(observed, expected);
    let final_page = store
        .observe_messages(&session.session_id, Some(&token), None, Some(2))
        .await
        .unwrap();
    assert!(!final_page.changed);
    assert!(final_page.messages.is_empty());
}

#[tokio::test]
async fn observe_session_messages_retention_reports_history_loss_for_protected_todo_hole() {
    let store = SessionStore::default();
    let session = store.start_session(Some("proj".to_string()), None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "old protected todo",
        SessionMessagePriority::High,
    );
    let before_fill = baseline(&store, &session.session_id).await;
    let mut filler_ids = Vec::new();
    for index in 0..(DEFAULT_MAX_MESSAGES_PER_SESSION - 1) {
        filler_ids.push(
            post(
                &store,
                &session.session_id,
                SessionMessageKind::Note,
                &format!("filler-{index}"),
                SessionMessagePriority::Normal,
            )
            .message_id,
        );
    }
    let completion = store
        .complete_message(CompleteSessionMessageInput {
            session_id: session.session_id.clone(),
            message_id: todo.message_id.clone(),
            answer: "answer preserving old todo".to_string(),
            tags: Vec::new(),
            priority: SessionMessagePriority::Normal,
            completion_id: completion_id('b'),
            author_session_id: None,
        })
        .unwrap();

    let retained_todo = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(todo.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(retained_todo.len(), 1);
    let retained_answer = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(completion.answer.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(retained_answer.len(), 1);
    let evicted_filler = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(filler_ids[0].clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(evicted_filler.is_empty());

    let delta = store
        .observe_messages(
            &session.session_id,
            Some(&before_fill.observation_token),
            None,
            Some(100),
        )
        .await
        .unwrap();
    assert!(delta.changed);
    assert!(delta.history_lost);
    assert!(delta.has_more);
    assert!(delta
        .messages
        .iter()
        .all(|message| message.message_id != filler_ids[0]));
}

#[tokio::test]
async fn observe_session_messages_token_survives_restart_and_legacy_restore_baselines_safely() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(Some("proj".to_string()), None);
    post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "before baseline",
        SessionMessagePriority::Normal,
    );
    let before_restart = baseline(&store, &session.session_id).await;
    // Observation-token issuance fences the current message-observation revision
    // itself. A fresh store opened while the first store is still alive must
    // therefore accept the token without relying on graceful Drop/test flush.
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let unchanged = restored
        .observe_messages(
            &session.session_id,
            Some(&before_restart.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!unchanged.changed);
    assert!(unchanged.messages.is_empty());
    let new_message = post(
        &restored,
        &session.session_id,
        SessionMessageKind::Progress,
        "after restart",
        SessionMessagePriority::Normal,
    );
    let after_restart = restored
        .observe_messages(
            &session.session_id,
            Some(&before_restart.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(after_restart.messages.len(), 1);
    assert_eq!(after_restart.messages[0].message_id, new_message.message_id);

    restored.flush_persistence();
    drop(restored);
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    record.remove("message_observation_revision");
    record.remove("message_observation_floor");
    record.remove("message_observation_revisions");
    std::fs::write(&ledger, serde_json::to_vec(&raw).unwrap()).unwrap();
    let legacy = SessionStore::with_persistence(&ledger, 10, 50);
    let legacy_baseline = baseline(&legacy, &session.session_id).await;
    assert!(legacy_baseline.messages.is_empty());
    assert!(!legacy_baseline.changed);
    let post_legacy = post(
        &legacy,
        &session.session_id,
        SessionMessageKind::Note,
        "after legacy restore",
        SessionMessagePriority::Normal,
    );
    let legacy_delta = legacy
        .observe_messages(
            &session.session_id,
            Some(&legacy_baseline.observation_token),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(legacy_delta.messages.len(), 1);
    assert_eq!(legacy_delta.messages[0].message_id, post_legacy.message_id);
}

#[tokio::test]
async fn observe_session_messages_duplicate_persisted_positive_revisions_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(Some("proj".to_string()), None);
    let before = baseline(&store, &session.session_id).await;
    let first = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "first distinct revision",
        SessionMessagePriority::Normal,
    );
    let second = post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "second distinct revision",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    let revisions = record["message_observation_revisions"]
        .as_object_mut()
        .unwrap();
    revisions.insert(first.message_id.clone(), Value::from(1));
    revisions.insert(second.message_id.clone(), Value::from(1));
    std::fs::write(&ledger, serde_json::to_vec(&raw).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let recovered = restored
        .observe_messages(
            &session.session_id,
            Some(&before.observation_token),
            None,
            Some(1),
        )
        .await
        .unwrap();
    assert!(recovered.changed);
    assert!(recovered.history_lost);
    assert!(!recovered.has_more);
    assert!(recovered.messages.is_empty());

    let caught_up = restored
        .observe_messages(
            &session.session_id,
            Some(&recovered.observation_token),
            None,
            Some(1),
        )
        .await
        .unwrap();
    assert!(!caught_up.changed);
    assert!(!caught_up.history_lost);
    assert!(caught_up.messages.is_empty());

    let after_restore = post(
        &restored,
        &session.session_id,
        SessionMessageKind::Progress,
        "new revision after conservative restore",
        SessionMessagePriority::Normal,
    );
    let delta = restored
        .observe_messages(
            &session.session_id,
            Some(&recovered.observation_token),
            None,
            Some(1),
        )
        .await
        .unwrap();
    assert_eq!(delta.messages.len(), 1);
    assert_eq!(delta.messages[0].message_id, after_restore.message_id);
}

#[tokio::test]
async fn observe_session_messages_unexplained_persisted_revision_gap_reports_history_loss() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(Some("proj".to_string()), None);
    let before = baseline(&store, &session.session_id).await;
    post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "known revision one",
        SessionMessagePriority::Normal,
    );
    post(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "known revision two",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    record.insert("message_observation_revision".to_string(), Value::from(3));
    std::fs::write(&ledger, serde_json::to_vec(&raw).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let recovered = restored
        .observe_messages(
            &session.session_id,
            Some(&before.observation_token),
            None,
            Some(1),
        )
        .await
        .unwrap();
    assert!(recovered.changed);
    assert!(recovered.history_lost);
    assert!(!recovered.has_more);
    assert!(recovered.messages.is_empty());
}

#[tokio::test]
async fn observe_session_messages_rejects_malformed_oversized_wrong_session_and_future_tokens() {
    let store = SessionStore::default();
    let first = store.start_session(Some("proj".to_string()), None);
    let second = store.start_session(Some("proj".to_string()), None);
    let token = baseline(&store, &first.session_id).await.observation_token;

    assert_eq!(
        store
            .observe_messages(&first.session_id, Some("not-a-token"), None, None)
            .await
            .unwrap_err(),
        SessionMessageObservationError::MalformedToken
    );
    assert_eq!(
        store
            .observe_messages(&first.session_id, Some(&"x".repeat(193)), None, None)
            .await
            .unwrap_err(),
        SessionMessageObservationError::OversizedToken
    );
    assert_eq!(
        store
            .observe_messages(&second.session_id, Some(&token), None, None)
            .await
            .unwrap_err(),
        SessionMessageObservationError::WrongSession
    );
    let future = encode_observation_token(&first.session_id, 1).unwrap();
    assert_eq!(
        store
            .observe_messages(&first.session_id, Some(&future), None, None)
            .await
            .unwrap_err(),
        SessionMessageObservationError::FutureRevision
    );
}

#[tokio::test]
async fn observe_session_messages_wait_is_target_scoped_race_safe_and_wakes_two_waiters() {
    let store = SessionStore::default();
    let target = store.start_session(Some("proj".to_string()), None);
    let unrelated = store.start_session(Some("proj".to_string()), None);
    let token = baseline(&store, &target.session_id).await.observation_token;

    let first_store = store.clone();
    let first_session = target.session_id.clone();
    let first_token = token.clone();
    let first_waiter = tokio::spawn(async move {
        first_store
            .observe_messages(&first_session, Some(&first_token), Some(2), None)
            .await
            .unwrap()
    });
    let second_store = store.clone();
    let second_session = target.session_id.clone();
    let second_token = token.clone();
    let second_waiter = tokio::spawn(async move {
        second_store
            .observe_messages(&second_session, Some(&second_token), Some(2), None)
            .await
            .unwrap()
    });
    tokio::task::yield_now().await;

    // An unrelated mutation can wake the shared watch internally but must not
    // satisfy either target waiter. It also proves no SessionStore mutex is held
    // across the await because this mutation can acquire the store immediately.
    post(
        &store,
        &unrelated.session_id,
        SessionMessageKind::Progress,
        "spurious wake",
        SessionMessagePriority::Normal,
    );
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(!first_waiter.is_finished());
    assert!(!second_waiter.is_finished());

    let target_message = post(
        &store,
        &target.session_id,
        SessionMessageKind::Progress,
        "wake both",
        SessionMessagePriority::Normal,
    );
    let first = first_waiter.await.unwrap();
    let second = second_waiter.await.unwrap();
    for observed in [first, second] {
        assert!(observed.changed);
        assert_eq!(observed.wait_outcome, "updated");
        assert_eq!(observed.messages.len(), 1);
        assert_eq!(observed.messages[0].message_id, target_message.message_id);
    }
}

#[tokio::test]
async fn observe_session_messages_wait_timeout_is_successful_unchanged_snapshot() {
    let store = SessionStore::default();
    let session = store.start_session(Some("proj".to_string()), None);
    let token = baseline(&store, &session.session_id)
        .await
        .observation_token;
    let timed_out = store
        .observe_messages(&session.session_id, Some(&token), Some(1), None)
        .await
        .unwrap();
    assert!(!timed_out.changed);
    assert!(timed_out.messages.is_empty());
    assert_eq!(timed_out.wait_outcome, "timeout");
    assert!(timed_out.waited_ms >= 900);
}

#[test]
fn collaboration_session_message_atomic_completion_is_correlated_and_idempotent() {
    let store = SessionStore::default();
    let coordinator =
        store.start_session(Some("proj".to_string()), Some("coordinator".to_string()));
    let worker = store.start_session(Some("proj".to_string()), Some("worker".to_string()));
    let todo = post(
        &store,
        &coordinator.session_id,
        SessionMessageKind::Todo,
        "Independent review this exact synthetic change; report findings.",
        SessionMessagePriority::High,
    );
    let first_completion_id = completion_id('a');
    let input = CompleteSessionMessageInput {
        session_id: coordinator.session_id.clone(),
        message_id: todo.message_id.clone(),
        answer: "No blocker found; source state revalidated.".to_string(),
        tags: vec!["review".to_string(), "done".to_string()],
        priority: SessionMessagePriority::High,
        completion_id: first_completion_id.clone(),
        author_session_id: Some(worker.session_id.clone()),
    };

    let completed = store.complete_message(input.clone()).unwrap();
    assert!(!completed.replayed);
    assert_eq!(completed.todo.status, SessionMessageStatus::Resolved);
    assert_eq!(
        completed.todo.resolved_by_message_id.as_deref(),
        Some(completed.answer.message_id.as_str())
    );
    assert_eq!(
        completed.todo.completion_id.as_deref(),
        Some(first_completion_id.as_str())
    );
    assert_eq!(completed.answer.kind, SessionMessageKind::Answer);
    assert_eq!(
        completed.answer.reply_to.as_deref(),
        Some(todo.message_id.as_str())
    );
    assert_eq!(
        completed.answer.author_session_id.as_deref(),
        Some(worker.session_id.as_str())
    );

    let exact = store
        .list_messages(
            &coordinator.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Resolved),
                message_id: Some(todo.message_id.clone()),
                reply_to: None,
                limit: Some(1),
            },
        )
        .unwrap();
    assert_eq!(exact.len(), 1);
    let impossible_intersection = store
        .list_messages(
            &coordinator.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Open),
                message_id: Some(todo.message_id.clone()),
                reply_to: None,
                limit: Some(1),
            },
        )
        .unwrap();
    assert!(impossible_intersection.is_empty());
    let replies = store
        .list_messages(
            &coordinator.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Answer),
                reply_to: Some(todo.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].message_id, completed.answer.message_id);

    let replay = store.complete_message(input.clone()).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, completed.answer.message_id);
    assert_eq!(replay.todo.resolved_at, completed.todo.resolved_at);
    let answers = store
        .list_messages(
            &coordinator.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Answer),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(answers.len(), 1, "retry must not duplicate answers");

    let conflict = store.complete_message(CompleteSessionMessageInput {
        answer: "different answer".to_string(),
        ..input.clone()
    });
    assert!(matches!(
        conflict,
        Err(SessionMessageError::IdempotencyConflict)
    ));
    let already = store.complete_message(CompleteSessionMessageInput {
        completion_id: completion_id('b'),
        ..input
    });
    assert!(matches!(
        already,
        Err(SessionMessageError::AlreadyCompleted {
            answer_message_id: Some(_),
            completion_id: Some(_),
        })
    ));

    let discussion = store
        .discussion_summary(&coordinator.session_id, Some(20))
        .unwrap();
    assert_eq!(discussion.counts.todo, 1);
    assert_eq!(discussion.counts.open_todos, 0);
    assert_eq!(discussion.counts.answer, 1);
    assert!(discussion.high_priority_open_todos.is_empty());
    assert_eq!(discussion.recent_answers.len(), 1);
    assert_eq!(discussion.recent_completions.len(), 1);
    assert_eq!(
        discussion.recent_completions[0]
            .author_session_id
            .as_deref(),
        Some(worker.session_id.as_str())
    );
}

#[test]
fn collaboration_session_message_completion_rejects_invalid_targets_and_author() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let question = post(
        &store,
        &session.session_id,
        SessionMessageKind::Question,
        "question",
        SessionMessagePriority::Normal,
    );
    let base = CompleteSessionMessageInput {
        session_id: session.session_id.clone(),
        message_id: question.message_id.clone(),
        answer: "answer".to_string(),
        tags: Vec::new(),
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id('c'),
        author_session_id: None,
    };
    assert!(matches!(
        store.complete_message(base.clone()),
        Err(SessionMessageError::NotTodo)
    ));
    assert!(matches!(
        store.complete_message(CompleteSessionMessageInput {
            message_id: "wc_msg_missing".to_string(),
            ..base.clone()
        }),
        Err(SessionMessageError::UnknownMessage)
    ));

    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "todo",
        SessionMessagePriority::Normal,
    );
    assert!(matches!(
        store.complete_message(CompleteSessionMessageInput {
            message_id: todo.message_id.clone(),
            author_session_id: Some("forged-author".to_string()),
            ..base.clone()
        }),
        Err(SessionMessageError::InvalidInput(_))
    ));
    assert!(matches!(
        store.complete_message(CompleteSessionMessageInput {
            message_id: todo.message_id.clone(),
            answer: "x".repeat(8001),
            ..base.clone()
        }),
        Err(SessionMessageError::InvalidInput(_))
    ));
    assert!(matches!(
        store.complete_message(CompleteSessionMessageInput {
            message_id: todo.message_id.clone(),
            tags: (0..17).map(|index| format!("tag-{index}")).collect(),
            ..base.clone()
        }),
        Err(SessionMessageError::InvalidInput(_))
    ));
    let still_open = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(todo.message_id.clone()),
                status: Some(SessionMessageStatus::Open),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        still_open.len(),
        1,
        "failed completion must not partially resolve todo"
    );
    store.close_session(&session.session_id).unwrap();
    assert!(matches!(
        store.complete_message(CompleteSessionMessageInput {
            message_id: todo.message_id,
            ..base
        }),
        Err(SessionMessageError::SessionClosed { .. })
    ));
}

#[test]
fn collaboration_session_message_exact_lookup_finds_old_retained_todo() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let target = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "old but retained exact assignment",
        SessionMessagePriority::High,
    );
    for index in 0..150 {
        post(
            &store,
            &session.session_id,
            SessionMessageKind::Note,
            &format!("noise {index}"),
            SessionMessagePriority::Normal,
        );
    }
    let exact = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(target.message_id.clone()),
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].message_id, target.message_id);
    let missing = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some("wc_msg_missing_exact".to_string()),
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(missing.is_empty());
}

#[test]
fn collaboration_session_message_completion_replays_after_restart_and_legacy_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let coordinator = store.start_session(Some("proj".to_string()), None);
    let worker = store.start_session(Some("proj".to_string()), None);
    let todo = post(
        &store,
        &coordinator.session_id,
        SessionMessageKind::Todo,
        "persist completion",
        SessionMessagePriority::Normal,
    );
    let input = CompleteSessionMessageInput {
        session_id: coordinator.session_id.clone(),
        message_id: todo.message_id.clone(),
        answer: "persisted answer".to_string(),
        tags: vec!["done".to_string()],
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id('d'),
        author_session_id: Some(worker.session_id.clone()),
    };
    let first = store.complete_message(input.clone()).unwrap();
    // Completion success itself fences the async writer, so a fresh store can
    // replay it without a test-only flush or graceful Drop of the first store.
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let replay = restored.complete_message(input.clone()).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, first.answer.message_id);
    assert_eq!(
        restored
            .list_messages(
                &coordinator.session_id,
                ListSessionMessagesFilter {
                    reply_to: Some(todo.message_id.clone()),
                    ..Default::default()
                },
            )
            .unwrap()
            .len(),
        1
    );

    restored.flush_persistence();
    drop(restored);
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let messages = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|session| session["session_id"] == coordinator.session_id)
        .unwrap()["messages"]
        .as_array_mut()
        .unwrap();
    let answer = messages
        .iter_mut()
        .find(|message| message["message_id"] == first.answer.message_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    answer.remove("author_session_id");
    answer.remove("resolved_by_message_id");
    answer.remove("completion_id");
    std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
    let legacy = SessionStore::with_persistence(&ledger, 10, 50);
    let legacy_answer = legacy
        .list_messages(
            &coordinator.session_id,
            ListSessionMessagesFilter {
                message_id: Some(first.answer.message_id),
                ..Default::default()
            },
        )
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(legacy_answer.author_session_id, None);
    assert_eq!(legacy_answer.resolved_by_message_id, None);
    assert_eq!(legacy_answer.completion_id, None);
}

#[test]
fn collaboration_session_message_persistence_failure_is_uncertain_then_same_key_recovers() {
    let root = tempfile::tempdir().unwrap();
    let ledger_dir = root.path().join("ledger");
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let ledger = ledger_dir.join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "durability failure",
        SessionMessagePriority::Normal,
    );
    store.flush_persistence();
    std::fs::remove_dir_all(&ledger_dir).unwrap();
    std::fs::write(&ledger_dir, b"block directory recreation").unwrap();

    let input = CompleteSessionMessageInput {
        session_id: session.session_id.clone(),
        message_id: todo.message_id.clone(),
        answer: "durable answer".to_string(),
        tags: Vec::new(),
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id('f'),
        author_session_id: None,
    };
    assert!(matches!(
        store.complete_message(input.clone()),
        Err(SessionMessageError::PersistenceUncertain)
    ));
    let in_memory = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                reply_to: Some(todo.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        in_memory.len(),
        1,
        "atomic in-memory completion remains exact"
    );

    std::fs::remove_file(&ledger_dir).unwrap();
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let replay = store.complete_message(input).unwrap();
    assert!(replay.replayed);
    store.flush_persistence();
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let restored_replies = restored
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                reply_to: Some(todo.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(restored_replies.len(), 1);
    assert_eq!(restored_replies[0].message_id, replay.answer.message_id);
}

#[test]
fn collaboration_session_message_partial_persisted_completion_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let todo = post(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "partial",
        SessionMessagePriority::Normal,
    );
    let input = CompleteSessionMessageInput {
        session_id: session.session_id.clone(),
        message_id: todo.message_id.clone(),
        answer: "answer".to_string(),
        tags: Vec::new(),
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id('e'),
        author_session_id: None,
    };
    store.complete_message(input.clone()).unwrap();
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let todo_raw = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|message| message["message_id"] == todo.message_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    todo_raw.remove("resolved_by_message_id");
    std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let err = restored.complete_message(input).unwrap_err();
    assert!(matches!(err, SessionMessageError::InvalidCompletionState));
    let persisted_todo = restored
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(todo.message_id),
                ..Default::default()
            },
        )
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(persisted_todo.status, SessionMessageStatus::Resolved);
}
