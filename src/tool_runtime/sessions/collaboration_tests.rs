use super::model::MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS;
use super::*;
use serde_json::Value;

fn completion_id(byte: char) -> String {
    byte.to_string()
        .repeat(MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS)
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
