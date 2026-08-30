use super::model::{
    DEFAULT_MAX_MESSAGES_PER_SESSION, MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES,
    SESSION_LEDGER_V1_VERSION,
};
use super::*;
use serde_json::Value;
use std::sync::Arc;

fn post_message(
    store: &SessionStore,
    session_id: &str,
    kind: SessionMessageKind,
    body: &str,
    reply_to: Option<&str>,
    priority: SessionMessagePriority,
    requires_ack: bool,
) -> SessionMessage {
    store
        .post_message_with_ack(
            PostSessionMessageInput {
                session_id: session_id.to_string(),
                kind,
                message: body.to_string(),
                tags: Vec::new(),
                reply_to: reply_to.map(str::to_string),
                priority,
            },
            requires_ack,
        )
        .unwrap()
}

fn todo(store: &SessionStore, session_id: &str, body: &str) -> SessionMessage {
    post_message(
        store,
        session_id,
        SessionMessageKind::Todo,
        body,
        None,
        SessionMessagePriority::High,
        false,
    )
}

fn fenced_completion(
    session_id: &str,
    message_id: &str,
    completion_id: &str,
    fence: String,
) -> CompleteSessionMessageInput {
    CompleteSessionMessageInput {
        session_id: session_id.to_string(),
        message_id: message_id.to_string(),
        answer: "assignment complete".to_string(),
        tags: vec!["e3".to_string()],
        priority: SessionMessagePriority::Normal,
        completion_id: completion_id.to_string(),
        author_session_id: None,
        expected_assignment_fence: fence,
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
        .expect("retained exact message")
}

fn answer_count(store: &SessionStore, session_id: &str, todo_id: &str) -> usize {
    store
        .list_messages(
            session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Answer),
                reply_to: Some(todo_id.to_string()),
                limit: Some(100),
                ..Default::default()
            },
        )
        .unwrap()
        .len()
}

#[test]
fn assignment_snapshot_is_atomic_and_unrelated_traffic_and_ack_do_not_stale() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "one read assignment");
    let guidance = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "direct high-priority guidance",
        Some(&todo.message_id),
        SessionMessagePriority::High,
        true,
    );

    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    assert_eq!(snapshot.todo.message_id, todo.message_id);
    assert_eq!(snapshot.direct_replies, vec![guidance.clone()]);
    assert!(snapshot.assignment_fence.starts_with("wsa1_"));
    assert_eq!(snapshot.assignment_fence.len(), 48);

    // Unrelated Session traffic advances the Session-wide observation high-water
    // but is not assignment-local meaning.
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "unrelated progress",
        None,
        SessionMessagePriority::Normal,
        false,
    );
    let ack = store.observe_message_acks(&session.session_id, &[guidance.message_id.clone()]);
    assert_eq!(ack.first_observed_count, 1);
    assert!(exact(&store, &session.session_id, &guidance.message_id)
        .first_ack_observed_at
        .is_some());

    let completed = store
        .complete_message(fenced_completion(
            &session.session_id,
            &todo.message_id,
            &"a".repeat(64),
            snapshot.assignment_fence,
        ))
        .unwrap();
    assert!(!completed.replayed);
    assert_eq!(
        answer_count(&store, &session.session_id, &todo.message_id),
        1
    );
}

#[test]
fn unrelated_retention_eviction_does_not_invalidate_assignment() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    for index in 0..150 {
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Note,
            &format!("old-unrelated-{index}"),
            None,
            SessionMessagePriority::Normal,
            false,
        );
    }
    let todo = todo(&store, &session.session_id, "survive unrelated eviction");
    let direct = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "retained direct reply",
        Some(&todo.message_id),
        SessionMessagePriority::Normal,
        false,
    );
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();

    // Cross the global message cap, but only evict messages older than this todo.
    for index in 0..75 {
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            &format!("new-unrelated-{index}"),
            None,
            SessionMessagePriority::Normal,
            false,
        );
    }
    assert_eq!(
        exact(&store, &session.session_id, &direct.message_id),
        direct
    );
    store
        .complete_message(fenced_completion(
            &session.session_id,
            &todo.message_id,
            &"b".repeat(64),
            snapshot.assignment_fence,
        ))
        .unwrap();
}

#[test]
fn direct_reply_append_replace_withdraw_and_resolve_stale_without_completion() {
    enum Mutation {
        Append,
        Replace,
        Withdraw,
        Resolve,
    }
    for mutation in [
        Mutation::Append,
        Mutation::Replace,
        Mutation::Withdraw,
        Mutation::Resolve,
    ] {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        let todo = todo(&store, &session.session_id, "mutate direct reply");
        let direct = post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Guidance,
            "original direct reply",
            Some(&todo.message_id),
            SessionMessagePriority::Normal,
            false,
        );
        let snapshot = store
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap();
        match mutation {
            Mutation::Append => {
                post_message(
                    &store,
                    &session.session_id,
                    SessionMessageKind::Note,
                    "new direct reply",
                    Some(&todo.message_id),
                    SessionMessagePriority::Normal,
                    false,
                );
            }
            Mutation::Replace => {
                store
                    .replace_message(ReplaceSessionMessageInput {
                        session_id: session.session_id.clone(),
                        message_id: direct.message_id.clone(),
                        message: "replacement direct reply".to_string(),
                    })
                    .unwrap();
            }
            Mutation::Withdraw => {
                store
                    .withdraw_message(&session.session_id, &direct.message_id)
                    .unwrap();
            }
            Mutation::Resolve => {
                store
                    .resolve_message(
                        &session.session_id,
                        &direct.message_id,
                        Some("resolved direct reply".to_string()),
                    )
                    .unwrap();
            }
        }
        // Extra unrelated body must never leak into the assignment-local stale payload.
        let unrelated = post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            "private unrelated progress",
            None,
            SessionMessagePriority::Normal,
            false,
        );
        let error = store
            .complete_message(fenced_completion(
                &session.session_id,
                &todo.message_id,
                &"c".repeat(64),
                snapshot.assignment_fence,
            ))
            .unwrap_err();
        let (current, fresh_assignment_fence) = match error {
            SessionMessageError::AssignmentStale {
                current,
                fresh_assignment_fence,
            } => (current, fresh_assignment_fence),
            other => panic!("expected assignment stale, got {other:?}"),
        };
        assert!(fresh_assignment_fence
            .as_deref()
            .is_some_and(|fence| fence.starts_with("wsa1_")));
        assert_eq!(current.todo.message_id, todo.message_id);
        assert!(!current
            .direct_replies
            .iter()
            .any(|message| message.message_id == unrelated.message_id));
        assert_eq!(
            exact(&store, &session.session_id, &todo.message_id).status,
            SessionMessageStatus::Open
        );
        assert_eq!(
            answer_count(&store, &session.session_id, &todo.message_id),
            0
        );
    }
}

#[test]
fn todo_replace_withdraw_and_resolve_reject_old_fence() {
    enum Mutation {
        Replace,
        Withdraw,
        Resolve,
    }
    for mutation in [Mutation::Replace, Mutation::Withdraw, Mutation::Resolve] {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        let todo = todo(&store, &session.session_id, "mutate exact todo");
        let snapshot = store
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap();
        match mutation {
            Mutation::Replace => {
                let replacement = store
                    .replace_message(ReplaceSessionMessageInput {
                        session_id: session.session_id.clone(),
                        message_id: todo.message_id.clone(),
                        message: "replacement todo".to_string(),
                    })
                    .unwrap();
                assert_ne!(replacement.replacement.message_id, todo.message_id);
                let replacement_assignment = store
                    .get_assignment(&session.session_id, &replacement.replacement.message_id)
                    .unwrap();
                assert_ne!(
                    replacement_assignment.assignment_fence,
                    snapshot.assignment_fence
                );
            }
            Mutation::Withdraw => {
                store
                    .withdraw_message(&session.session_id, &todo.message_id)
                    .unwrap();
            }
            Mutation::Resolve => {
                store
                    .resolve_message(&session.session_id, &todo.message_id, None)
                    .unwrap();
            }
        }
        assert!(matches!(
            store.complete_message(fenced_completion(
                &session.session_id,
                &todo.message_id,
                &"d".repeat(64),
                snapshot.assignment_fence,
            )),
            Err(SessionMessageError::AssignmentStale { .. })
        ));
        assert_eq!(
            answer_count(&store, &session.session_id, &todo.message_id),
            0
        );
    }
}

#[test]
fn assignment_fence_is_bound_to_exact_session_and_todo() {
    let store = SessionStore::default();
    let first = store.start_session(None, None);
    let second = store.start_session(None, None);
    let first_todo = todo(&store, &first.session_id, "first");
    let second_todo = todo(&store, &first.session_id, "second");
    let other_todo = todo(&store, &second.session_id, "other session");
    let snapshot = store
        .get_assignment(&first.session_id, &first_todo.message_id)
        .unwrap();

    for (session_id, todo_id) in [
        (&first.session_id, &second_todo.message_id),
        (&second.session_id, &other_todo.message_id),
    ] {
        assert!(matches!(
            store.complete_message(fenced_completion(
                session_id,
                todo_id,
                &"e".repeat(64),
                snapshot.assignment_fence.clone(),
            )),
            Err(SessionMessageError::AssignmentStale { .. })
        ));
    }
    assert!(matches!(
        store.complete_message(fenced_completion(
            &first.session_id,
            &first_todo.message_id,
            &"e".repeat(64),
            format!("wsm1_{}", "A".repeat(43)),
        )),
        Err(SessionMessageError::InvalidAssignmentFence)
    ));
}

#[test]
fn fenced_completion_replays_same_key_and_conflicts_on_same_key_body_change() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "replay fenced completion");
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "unrelated traffic leaves assignment semantics unchanged",
        None,
        SessionMessagePriority::Normal,
        false,
    );
    let same_semantics_new_fence = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    assert_eq!(
        same_semantics_new_fence.assignment_fence,
        snapshot.assignment_fence
    );
    let different_todo = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "different replay fence",
        None,
        SessionMessagePriority::High,
        false,
    );
    let different_valid_fence = store
        .get_assignment(&session.session_id, &different_todo.message_id)
        .unwrap()
        .assignment_fence;
    assert_ne!(different_valid_fence, snapshot.assignment_fence);
    let input = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"f".repeat(64),
        snapshot.assignment_fence.clone(),
    );
    let first = store.complete_message(input.clone()).unwrap();
    let replay = store.complete_message(input.clone()).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, first.answer.message_id);

    let mut different_fence = input.clone();
    different_fence.expected_assignment_fence = different_valid_fence;
    assert!(matches!(
        store.complete_message(different_fence),
        Err(SessionMessageError::IdempotencyConflict)
    ));

    let mut different_author = input.clone();
    different_author.author_session_id = Some("wc_sess_other_worker".to_string());
    assert!(matches!(
        store.complete_message(different_author),
        Err(SessionMessageError::IdempotencyConflict)
    ));

    let mut different_tags = input.clone();
    different_tags.tags.push("different-tag".to_string());
    assert!(matches!(
        store.complete_message(different_tags),
        Err(SessionMessageError::IdempotencyConflict)
    ));

    let mut different_priority = input.clone();
    different_priority.priority = SessionMessagePriority::High;
    assert!(matches!(
        store.complete_message(different_priority),
        Err(SessionMessageError::IdempotencyConflict)
    ));

    // Later assignment-local traffic cannot rewrite the identity of the already
    // committed idempotent call; the original exact fence still replays.
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Note,
        "post-completion direct reply",
        Some(&todo.message_id),
        SessionMessagePriority::Normal,
        false,
    );
    let replay_after_thread_change = store.complete_message(input.clone()).unwrap();
    assert!(replay_after_thread_change.replayed);
    assert_eq!(
        replay_after_thread_change.answer.message_id,
        first.answer.message_id
    );

    let mut conflict = input;
    conflict.answer = "different body".to_string();
    assert!(matches!(
        store.complete_message(conflict),
        Err(SessionMessageError::IdempotencyConflict)
    ));
    assert_eq!(
        answer_count(&store, &session.session_id, &todo.message_id),
        1
    );
}

#[test]
fn concurrent_workers_with_same_fence_create_at_most_one_answer() {
    let store = Arc::new(SessionStore::default());
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "concurrent workers");
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    let first = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"1".repeat(64),
        snapshot.assignment_fence.clone(),
    );
    let second = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"2".repeat(64),
        snapshot.assignment_fence,
    );
    let a = Arc::clone(&store);
    let b = Arc::clone(&store);
    let left = std::thread::spawn(move || a.complete_message(first));
    let right = std::thread::spawn(move || b.complete_message(second));
    let results = [left.join().unwrap(), right.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results
        .iter()
        .filter(|result| result.is_err())
        .all(|result| {
            matches!(
                result,
                Err(SessionMessageError::AlreadyCompleted { .. })
                    | Err(SessionMessageError::AssignmentStale { .. })
            )
        }));
    assert_eq!(
        answer_count(&store, &session.session_id, &todo.message_id),
        1
    );
}

#[test]
fn assignment_snapshot_and_fenced_replay_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "restart assignment");
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "persisted direct reply",
        Some(&todo.message_id),
        SessionMessagePriority::Normal,
        false,
    );
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    let input = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"3".repeat(64),
        snapshot.assignment_fence,
    );

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let first = restored.complete_message(input.clone()).unwrap();
    let restarted_again = SessionStore::with_persistence(&ledger, 10, 50);
    let replay = restarted_again.complete_message(input).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, first.answer.message_id);
}

#[test]
fn sanitized_relevant_history_and_corrupt_observation_metadata_fail_closed() {
    for corrupt_observation in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("sessions.json");
        let store = SessionStore::with_persistence(&ledger, 10, 50);
        let session = store.start_session(None, None);
        let todo = todo(&store, &session.session_id, "retention proof");
        let direct = post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Guidance,
            "direct reply that must remain provable",
            Some(&todo.message_id),
            SessionMessagePriority::Normal,
            false,
        );
        store.flush_persistence();
        drop(store);

        let mut raw: Value =
            serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
        let record = raw["sessions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["session_id"] == session.session_id)
            .unwrap();
        if corrupt_observation {
            record["message_observation_revision"] = Value::from(u64::MAX - 1);
        } else {
            let direct_raw = record["messages"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|message| message["message_id"] == direct.message_id)
                .unwrap();
            direct_raw["session_id"] = Value::String("wc_sess_wrong".to_string());
        }
        std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

        let restored = SessionStore::with_persistence(&ledger, 10, 50);
        assert!(matches!(
            restored.get_assignment(&session.session_id, &todo.message_id),
            Err(SessionMessageError::AssignmentHistoryLost { .. })
        ));
    }
}

#[test]
fn relevant_change_then_sanitization_cannot_look_safe_again() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "old assignment");
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    let direct = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "relevant change after snapshot",
        Some(&todo.message_id),
        SessionMessagePriority::Normal,
        false,
    );
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap();
    record["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|message| message["message_id"] == direct.message_id)
        .unwrap()["session_id"] = Value::String("wc_sess_wrong".to_string());
    std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    assert!(matches!(
        restored.complete_message(fenced_completion(
            &session.session_id,
            &todo.message_id,
            &"4".repeat(64),
            snapshot.assignment_fence,
        )),
        Err(SessionMessageError::AssignmentHistoryLost { .. })
    ));
    assert_eq!(
        answer_count(&restored, &session.session_id, &todo.message_id),
        0
    );
}

#[test]
fn oversized_direct_reply_set_fails_closed_without_partial_fence() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "bounded assignment");
    for index in 0..=MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES {
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Note,
            &format!("direct-{index}"),
            Some(&todo.message_id),
            SessionMessagePriority::Normal,
            false,
        );
    }
    match store.get_assignment(&session.session_id, &todo.message_id) {
        Err(SessionMessageError::AssignmentTooLarge {
            reply_count,
            max_replies,
            current,
        }) => {
            assert_eq!(reply_count, MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES + 1);
            assert_eq!(max_replies, MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES);
            assert_eq!(
                current.direct_replies.len(),
                MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES
            );
            assert!(current.direct_replies_truncated);
        }
        other => panic!("expected bounded assignment failure, got {other:?}"),
    }
}

#[test]
fn v039_no_fence_completion_restores_for_query_but_cannot_replay() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "v0.3.9 completion");
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    let input = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"5".repeat(64),
        snapshot.assignment_fence,
    );
    let first = store.complete_message(input.clone()).unwrap();
    store.flush_persistence();
    drop(store);

    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    raw["version"] = Value::from(SESSION_LEDGER_V1_VERSION);
    raw.as_object_mut().unwrap().insert(
        "durable_current_bindings".to_string(),
        serde_json::json!([]),
    );
    let record = raw["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap()
        .as_object_mut()
        .unwrap();
    for field in [
        "assignment_history_floors",
        "assignment_history_tracking_complete",
        "completion_assignment_fence_fingerprints",
        "completion_assignment_fence_tracking_complete",
    ] {
        record.remove(field);
    }
    std::fs::write(&ledger, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
    let before_replay = std::fs::read(&ledger).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let restored_todo = exact(&restored, &session.session_id, &todo.message_id);
    assert_eq!(restored_todo.status, SessionMessageStatus::Resolved);
    assert_eq!(
        restored_todo.resolved_by_message_id.as_deref(),
        Some(first.answer.message_id.as_str())
    );
    assert_eq!(
        answer_count(&restored, &session.session_id, &todo.message_id),
        1
    );
    assert!(matches!(
        restored.complete_message(input),
        Err(SessionMessageError::IdempotencyConflict)
    ));
    assert_eq!(std::fs::read(&ledger).unwrap(), before_replay);
}

#[test]
fn fenced_completion_retains_direct_replies_for_idempotent_replay_at_message_cap() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let todo = todo(&store, &session.session_id, "oldest fenced todo");
    let direct = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "direct reply must survive completion cleanup",
        Some(&todo.message_id),
        SessionMessagePriority::Normal,
        false,
    );
    let snapshot = store
        .get_assignment(&session.session_id, &todo.message_id)
        .unwrap();
    for index in 0..(DEFAULT_MAX_MESSAGES_PER_SESSION - 2) {
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            &format!("unrelated-after-assignment-{index}"),
            None,
            SessionMessagePriority::Normal,
            false,
        );
    }

    let input = fenced_completion(
        &session.session_id,
        &todo.message_id,
        &"6".repeat(64),
        snapshot.assignment_fence,
    );
    let first = store.complete_message(input.clone()).unwrap();
    assert!(!first.replayed);
    assert_eq!(
        exact(&store, &session.session_id, &direct.message_id),
        direct
    );
    let replay = store.complete_message(input).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, first.answer.message_id);
}

#[test]
fn assignment_test_message_budget_assumption_matches_store_cap() {
    assert!(150 + 2 + 75 > DEFAULT_MAX_MESSAGES_PER_SESSION);
    assert!(150 + 2 < DEFAULT_MAX_MESSAGES_PER_SESSION);
}
