use super::*;
use crate::*;

fn post(store: &SessionStore, session: &str, kind: SessionMessageKind) -> SessionMessage {
    store
        .post_message(PostSessionMessageInput {
            session_id: session.into(),
            kind,
            message: "message".into(),
            tags: vec![],
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap()
}

#[test]
fn compact_session_and_message_allocators_retry_without_overwrite() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let message = post(&store, &session.session_id, SessionMessageKind::Note);
    assert_eq!(session.session_id.len(), 24);
    assert_eq!(message.message_id.len(), 23);
    let inner = store.inner.lock().unwrap();
    let mut suffixes = [
        session.session_id[8..].to_string(),
        "abcdefghijklmnop".into(),
    ]
    .into_iter();
    assert_eq!(
        inner
            .allocate_session_id(|| suffixes.next().unwrap())
            .unwrap(),
        "wc_sess_abcdefghijklmnop"
    );
    assert!(inner
        .allocate_session_id(|| session.session_id[8..].to_string())
        .is_none());
    let record = inner
        .sessions
        .get(&session.session_id)
        .unwrap()
        .hot()
        .unwrap();
    let mut suffixes = [
        message.message_id[7..].to_string(),
        "abcdefghijklmnop".into(),
    ]
    .into_iter();
    assert_eq!(
        allocate_message_id(record, || suffixes.next().unwrap()).unwrap(),
        "wc_msg_abcdefghijklmnop"
    );
    assert!(allocate_message_id(record, || message.message_id[7..].to_string()).is_err());
    assert_eq!(record.messages.len(), 1);
}

#[test]
fn persisted_message_ids_accept_only_compact_or_legacy_canonical_forms() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let message = post(&store, &session.session_id, SessionMessageKind::Note);

    let mut malformed_primary = message.clone();
    malformed_primary.message_id = "wc_msg_garbage".to_string();
    assert!(
        crate::persistence::sanitize_persisted_message(malformed_primary, &session.session_id)
            .is_none()
    );

    let mut malformed_links = message;
    malformed_links.reply_to = Some("wc_msg_garbage".to_string());
    malformed_links.resolved_by_message_id = Some("wc_msg_also_bad".to_string());
    let sanitized =
        crate::persistence::sanitize_persisted_message(malformed_links, &session.session_id)
            .expect("canonical primary message id remains restorable");
    assert!(sanitized.reply_to.is_none());
    assert!(sanitized.resolved_by_message_id.is_none());
}

#[tokio::test]
async fn legacy_ledger_restores_all_links_and_resumes_with_compact_messages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&path, 10, 100);
    let session = store.start_session(Some("demo".into()), None);
    let note = post(&store, &session.session_id, SessionMessageKind::Note);
    let replacement = store
        .replace_message(ReplaceSessionMessageInput {
            session_id: session.session_id.clone(),
            message_id: note.message_id.clone(),
            message: "replacement".into(),
        })
        .await
        .unwrap()
        .replacement;
    let todo = post(&store, &session.session_id, SessionMessageKind::Todo);
    let fence = store
        .get_assignment(&session.session_id, &todo.message_id)
        .await
        .unwrap()
        .assignment_fence;
    let answer = store
        .complete_message(CompleteSessionMessageInput {
            session_id: session.session_id.clone(),
            message_id: todo.message_id.clone(),
            answer: "done".into(),
            tags: vec![],
            priority: SessionMessagePriority::Normal,
            completion_id: "a".repeat(64),
            author_session_id: Some(session.session_id.clone()),
            expected_assignment_fence: fence,
        })
        .await
        .unwrap()
        .answer;
    drop(store);
    // Construct a historical fixture; production never rewrites an old ledger.
    let legacy_session = format!("wc_sess_{}", "1".repeat(32));
    let mut legacy = crate::test_storage::read_ledger(&path)
        .unwrap()
        .replace(&session.session_id, &legacy_session);
    let mut ids = Vec::new();
    for (index, id) in [
        &note.message_id,
        &replacement.message_id,
        &todo.message_id,
        &answer.message_id,
    ]
    .into_iter()
    .enumerate()
    {
        let old = format!("wc_msg_{index:032x}");
        legacy = legacy.replace(id, &old);
        ids.push(old);
    }
    std::fs::write(&path, &legacy).unwrap();
    let restored = crate::test_storage::import_ledger(&path, 10, 100);
    assert!(restored.summary(&legacy_session, None).is_some());
    assert_eq!(
        crate::test_storage::read_ledger(&path).unwrap(),
        legacy,
        "restoration does not rewrite the ledger"
    );
    let messages = restored
        .list_messages(
            &legacy_session,
            ListSessionMessagesFilter {
                limit: Some(100),
                ..Default::default()
            },
        )
        .unwrap();
    let find = |id: &str| messages.iter().find(|m| m.message_id == id).unwrap();
    assert_eq!(
        find(&ids[0]).superseded_by_message_id.as_deref(),
        Some(ids[1].as_str())
    );
    assert_eq!(
        find(&ids[1]).supersedes_message_id.as_deref(),
        Some(ids[0].as_str())
    );
    assert_eq!(
        find(&ids[2]).resolved_by_message_id.as_deref(),
        Some(ids[3].as_str())
    );
    assert_eq!(find(&ids[3]).reply_to.as_deref(), Some(ids[2].as_str()));
    assert_eq!(
        find(&ids[3]).author_session_id.as_deref(),
        Some(legacy_session.as_str())
    );
    let resumed = restored
        .ensure_coding_session(CodingSessionRequest {
            project: "demo".into(),
            authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.into(),
            resume_session_id: Some(legacy_session.clone()),
            instruction: Some("continue".into()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: false,
            write_scope_verified: true,
        })
        .unwrap();
    assert!(resumed.reused);
    assert_eq!(resumed.summary.session_id, legacy_session);
    assert_eq!(
        post(&restored, &legacy_session, SessionMessageKind::Note)
            .message_id
            .len(),
        23
    );
}
