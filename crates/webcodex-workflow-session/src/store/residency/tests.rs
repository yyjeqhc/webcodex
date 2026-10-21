use super::*;
use crate::model::SessionMessageKind;
use serde_json::json;
use webcodex_core::project_instructions::{
    InstructionSourceScope, LoadedInstructionCandidate, ProjectInstructionsSnapshot,
};

fn message(id: &str) -> PostSessionMessageInput {
    PostSessionMessageInput {
        session_id: id.into(),
        kind: SessionMessageKind::Note,
        message: "retained message".into(),
        tags: vec![],
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    }
}
fn churn(store: &SessionStore) {
    store.start_session(None, Some("other target".into()));
}
fn encoded(store: &SessionStore, id: &str) -> Value {
    store
        .with_record_for_query(id, |record, _| {
            serde_json::to_value(PersistedSessionRecord::from_record(record, 2000)).unwrap()
        })
        .unwrap()
}

#[test]
fn retained_permission_updates_preserve_hot_cold_evidence_and_snapshots() {
    for closed in [false, true] {
        for cold in [false, true] {
            let store = SessionStore::new(1, 20);
            let id = store.start_session(Some("project".into()), None).session_id;
            let mut start = store
                .record_tool_call_started(
                    Some(&id),
                    SessionTransport::Api,
                    "read_file",
                    &json!({"project":"project", "path":"src/lib.rs"}),
                    SessionToolContract {
                        risk_class: "read",
                        read_like: true,
                        write_like: false,
                        shell_like: false,
                        git_like: false,
                        change_summary_like: false,
                        project_write: false,
                        path_hint: crate::events::SessionPathHint::SinglePath,
                    },
                )
                .unwrap();
            if closed {
                store.close_session(&id).unwrap();
                if !cold {
                    // Exercise the transient Closed/Hot branch before coldification.
                    let record = store
                        .with_record_for_query(&id, |record, _| record.clone())
                        .unwrap();
                    store
                        .inner
                        .lock()
                        .unwrap()
                        .sessions
                        .insert(id.clone(), StoredSession::Hot(record));
                }
            } else if cold {
                churn(&store);
            }
            assert_eq!(store.cold_payload_bytes_for_test(&id).is_some(), cold);
            let before = encoded(&store, &id);
            let old_event = store
                .with_record_for_query(&id, |record, _| {
                    record
                        .events
                        .iter()
                        .find(|event| event.event_id == start.event_id)
                        .unwrap()
                        .clone()
                })
                .unwrap();
            let permission = PermissionDecision {
                required: true,
                policy: "approval_required".into(),
                request_id: "test-request".into(),
                status: "approved".into(),
                reason: "test approval".into(),
                risk: "read".into(),
                tool_name: "read_file".into(),
                project: Some("project".into()),
            };
            store.record_permission_decision(&mut start, permission.clone());
            assert_eq!(start.permission.as_ref(), Some(&permission));
            assert!(
                old_event.permission.is_none(),
                "retained Arc snapshot was mutated"
            );
            let mut expected = before;
            let event = expected["events"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|event| event["event_id"] == start.event_id)
                .unwrap();
            event["permission"] = serde_json::to_value(&permission).unwrap();
            assert_eq!(
                encoded(&store, &id),
                expected,
                "closed={closed}, cold={cold}"
            );
            assert_eq!(store.cold_payload_bytes_for_test(&id).is_some(), closed);
            assert!(!store.set_event_permission(&id, "missing-event", permission.clone()));
            assert_eq!(encoded(&store, &id), expected);
            assert!(!store.set_event_permission("missing-session", &start.event_id, permission));
        }
    }
}

#[test]
fn active_residency_capacity_keeps_identity_and_queries_do_not_promote() {
    let store = SessionStore::new(2, 20);
    let ids: Vec<_> = (0..12)
        .map(|i| {
            store
                .start_session(None, Some(format!("session {i}")))
                .session_id
        })
        .collect();
    assert_eq!(store.status().hot_sessions, 2);
    assert_eq!(store.status().active_cold_sessions, 10);
    assert!(store.status().cold_payload_bytes > 0);
    for id in &ids {
        assert_eq!(store.lifecycle_state(id), Some(SessionLifecycle::Active));
        assert!(store.summary(id, Some(5)).is_some());
    }
    assert_eq!(store.status().hot_sessions, 2);
    store.post_message(message(&ids[0])).unwrap();
    assert!(store.hot_payload_entry_count_for_test(&ids[0]).is_some());
    assert_eq!(store.status().hot_sessions, 2);
    assert_eq!(store.status().retained_sessions, 12);
    assert_eq!(store.status().capacity_evictions, 0);
}

#[test]
fn active_residency_is_lossless_not_a_second_disk_sanitization() {
    let store = SessionStore::new(1, 20);
    let instructions = ProjectInstructionsSnapshot::from_candidates(
        vec![LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Project,
            path: "AGENTS.md".into(),
            content: "retained rule body".into(),
            total_lines: 1,
            full_sha256: None,
        }],
        true,
    );
    let mut options = SessionCreateOptions::new(
        Some("project".into()),
        Some("long title ".repeat(180)),
        SessionMode::ReadOnly,
        SessionGuards::default(),
    );
    options.project_instructions = Some(instructions);
    let id = store
        .start_session_with_options(options)
        .unwrap()
        .session_id;
    {
        let mut inner = store.inner.lock().unwrap();
        let record = inner.sessions.get_mut(&id).unwrap().hot_mut().unwrap();
        record.git_baseline_tree = Some("a".repeat(40));
        record.repository_edit_observed = true;
        record
            .materialized_validation_job_ids
            .push_back("wc_job_1234567890123456".into());
        record
            .completion_assignment_fence_fingerprints
            .insert("historical-no-fence".into(), None);
    }
    let before = encoded(&store, &id);
    churn(&store);
    assert_eq!(encoded(&store, &id), before);
    assert!(store.cold_payload_bytes_for_test(&id).is_some());
    store
        .with_record_for_query(&id, |record, _| {
            assert_eq!(
                record.project_instructions.as_ref().unwrap().files[0].content,
                "retained rule body"
            );
            assert_eq!(
                record
                    .completion_assignment_fence_fingerprints
                    .get("historical-no-fence"),
                Some(&None)
            );
        })
        .unwrap();
    let context = store.execution_context_for_project(&id, "project").unwrap();
    assert!(store.execution_context_for_project(&id, "wrong").is_none());
    store
        .update_execution_context(&id, context, SessionTransport::Api)
        .unwrap();
    assert!(store.hot_payload_entry_count_for_test(&id).is_some());
    assert_eq!(store.guard_state(&id).unwrap().0, SessionMode::ReadOnly);
}

#[tokio::test]
async fn active_residency_restart_restores_cold_and_keyed_replay_ack_survive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&path, 1, 20);
    let id = store.start_session(None, None).session_id;
    let delivery = SessionMessageDelivery {
        sender_scope: "b".repeat(64),
        delivery_key: "same-send".into(),
    };
    let first = store
        .post_message_with_ack_and_delivery(message(&id), true, Some(delivery.clone()))
        .await
        .unwrap();
    churn(&store);
    let ack = store.observe_message_acks(&id, &[first.message.message_id.clone()]);
    assert_eq!(ack.first_observed_count, 1);
    churn(&store);
    assert!(
        store
            .post_message_with_ack_and_delivery(message(&id), true, Some(delivery.clone()))
            .await
            .unwrap()
            .replayed
    );
    store.flush_persistence();
    drop(store);
    let bytes = std::fs::read(&path).unwrap();
    let reopened = SessionStore::with_persistence(&path, 1, 20);
    assert_eq!(reopened.status().hot_sessions, 0);
    assert_eq!(reopened.status().active_sessions, 3);
    assert_eq!(reopened.status().active_cold_sessions, 3);
    reopened.summary(&id, None).unwrap();
    assert_eq!(reopened.status().hot_sessions, 0);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "restore/query cannot rewrite the ledger"
    );
    let replay = reopened
        .post_message_with_ack_and_delivery(message(&id), true, Some(delivery))
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.message.message_id, first.message.message_id);
    assert!(replay.message.first_ack_observed_at.is_some());
    assert_eq!(reopened.status().hot_sessions, 1);
    churn(&reopened);
    reopened.close_session(&id).unwrap();
    assert!(matches!(
        reopened.post_message(message(&id)),
        Err(SessionMessageError::SessionClosed { .. })
    ));
}

#[test]
fn active_residency_inflight_call_keeps_terminal_and_sticky_edit_evidence() {
    let store = SessionStore::new(1, 20);
    let id = store.start_session(Some("project".into()), None).session_id;
    let contract = SessionToolContract {
        risk_class: "write",
        read_like: false,
        write_like: true,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        project_write: true,
        path_hint: crate::events::SessionPathHint::PathList,
    };
    let start = store
        .record_tool_call_started(
            Some(&id),
            SessionTransport::Api,
            "edit_project_files",
            &json!({"project":"project","changes":[]}),
            contract,
        )
        .unwrap();
    churn(&store);
    assert!(store.cold_payload_bytes_for_test(&id).is_some());
    store
        .record_tool_call_finished(
            Some(start),
            true,
            &json!({"changed":true,"state_changed":true}),
            None,
            None,
        )
        .unwrap();
    let value = encoded(&store, &id);
    assert_eq!(value["events_observed"], 2);
    assert_eq!(value["repository_edit_observed"], true);
    assert_eq!(value["events"][1]["kind"], "tool_call_finished");
    churn(&store);
    assert_eq!(encoded(&store, &id), value);
}

#[test]
fn active_residency_zero_target_and_failed_persistence_never_discard_dirty_state() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "block").unwrap();
    let store = SessionStore::with_persistence(blocker.join("sessions.json"), 0, 20);
    let id = store.start_session(None, Some("dirty".into())).session_id;
    let first = store.post_message(message(&id)).unwrap();
    churn(&store);
    store.flush_persistence();
    assert!(store.status().last_persist_error.is_some());
    assert_eq!(store.status().hot_sessions, 1);
    assert_eq!(store.status().capacity_evictions, 0);
    assert_eq!(
        encoded(&store, &id)["messages"][0]["message_id"],
        first.message_id
    );
    assert!(store.post_message(message(&id)).is_ok());
}

#[tokio::test]
async fn active_residency_assignment_fences_and_exact_completion_replay_survive_churn() {
    let store = SessionStore::new(1, 20);
    let id = store.start_session(None, None).session_id;
    let mut input = message(&id);
    input.kind = SessionMessageKind::Todo;
    let todo = store.post_message(input).unwrap();
    let assignment = store.get_assignment(&id, &todo.message_id).await.unwrap();
    churn(&store);
    assert_eq!(
        store
            .get_assignment(&id, &todo.message_id)
            .await
            .unwrap()
            .assignment_fence,
        assignment.assignment_fence
    );
    let completion = crate::model::CompleteSessionMessageInput {
        session_id: id.clone(),
        message_id: todo.message_id.clone(),
        answer: "done".into(),
        tags: vec![],
        priority: SessionMessagePriority::Normal,
        completion_id: "a".repeat(64),
        author_session_id: None,
        expected_assignment_fence: assignment.assignment_fence,
    };
    let first = store.complete_message(completion.clone()).await.unwrap();
    assert!(!first.replayed);
    churn(&store);
    let replay = store.complete_message(completion).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.answer.message_id, first.answer.message_id);
    assert_eq!(
        replay.todo.resolved_by_message_id,
        Some(first.answer.message_id)
    );
}

#[test]
fn active_residency_parallel_mutation_and_churn_preserve_every_message() {
    let store = SessionStore::new(1, 20);
    let id = store.start_session(None, None).session_id;
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let store = &store;
            let id = &id;
            scope.spawn(move || {
                for index in 0..8 {
                    churn(store);
                    let mut input = message(id);
                    input.message = format!("worker-{worker}-message-{index}");
                    store.post_message(input).unwrap();
                }
            });
        }
    });
    let messages = store
        .list_messages(&id, crate::model::ListSessionMessagesFilter::default())
        .unwrap();
    let texts: std::collections::BTreeSet<_> = messages.iter().map(|m| m.message.clone()).collect();
    assert_eq!(texts.len(), 32);
    assert_eq!(store.status().active_sessions, 33);
    assert_eq!(store.status().hot_sessions, 1);
    assert_eq!(store.status().capacity_evictions, 0);
}

#[test]
fn active_residency_corrupt_payload_fails_closed_without_replacing_identity() {
    let store = SessionStore::new(1, 20);
    let id = store.start_session(None, None).session_id;
    churn(&store);
    {
        let mut inner = store.inner.lock().unwrap();
        let StoredSession::Cold(record) = inner.sessions.get_mut(&id).unwrap() else {
            panic!("cold fixture");
        };
        record.raw = Arc::from(serde_json::value::RawValue::from_string("{}".into()).unwrap());
    }
    assert!(store.summary(&id, None).is_none());
    assert!(store.post_message(message(&id)).is_err());
    assert!(store.contains_session(&id));
    assert_eq!(store.lifecycle_state(&id), Some(SessionLifecycle::Active));
    assert_eq!(store.status().capacity_evictions, 0);
}

#[test]
fn cold_revision_snapshot_tracks_active_closed_and_rejects_replaced_raw() {
    let store = SessionStore::new(1, 2_000);
    let id = store.start_session(Some("project".into()), None).session_id;
    store.post_message(message(&id)).unwrap();
    let original = store
        .with_record_for_query(&id, |record, _| {
            (record.events_observed, record.message_observation_revision)
        })
        .unwrap();
    assert!(original.1 > 0);
    assert_eq!(store.handoff_revision(&id), Some(original));

    churn(&store);
    assert!(store.cold_payload_bytes_for_test(&id).is_some());
    assert_eq!(store.retained_revisions(&id), Some(original));
    let old = store.with_record_for_query(&id, |record, _| {
        (record.events_observed, record.message_observation_revision)
    });
    assert_eq!(store.retained_revisions(&id), old);

    // A legitimate mutation hydrates an Active Cold identity before updating
    // its observed revisions. The lightweight path must never return its stale
    // pre-mutation snapshot.
    store.post_message(message(&id)).unwrap();
    let changed = store
        .with_record_for_query(&id, |record, _| {
            (record.events_observed, record.message_observation_revision)
        })
        .unwrap();
    assert!(changed.1 > original.1);
    assert_eq!(store.handoff_revision(&id), Some(changed));

    store.close_session(&id).unwrap();
    let closed = store
        .with_record_for_query(&id, |record, _| {
            (record.events_observed, record.message_observation_revision)
        })
        .unwrap();
    assert_eq!(store.handoff_revision(&id), Some(closed));
    assert!(closed.0 >= changed.0);

    {
        let mut inner = store.inner.lock().unwrap();
        let StoredSession::Cold(cold) = inner.sessions.get_mut(&id).unwrap() else {
            panic!("Closed identity must be Cold");
        };
        cold.raw = Arc::from(serde_json::value::to_raw_value(&json!({"corrupted":true})).unwrap());
    }
    assert_eq!(store.retained_revisions(&id), None);
    assert_eq!(store.handoff_revision(&id), None);
}

#[test]
fn cold_revision_snapshot_survives_durable_restart_without_raw_decode() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("revisions.json");
    let store = SessionStore::with_persistence_limits(&ledger, 1, 20, 40);
    let id = store.start_session(Some("project".into()), None).session_id;
    store.post_message(message(&id)).unwrap();
    let expected = store.handoff_revision(&id).unwrap();
    store.flush_persistence();
    drop(store);

    let reopened = SessionStore::with_persistence_limits(&ledger, 1, 20, 40);
    assert_eq!(reopened.handoff_revision(&id), Some(expected));
    assert!(reopened.cold_payload_bytes_for_test(&id).is_some());
    assert_eq!(
        reopened.with_record_for_query(&id, |record, _| {
            (record.events_observed, record.message_observation_revision)
        }),
        Some(expected)
    );
}

#[test]
#[ignore = "manual synthetic Cold Session revision read comparison"]
fn cold_revision_snapshot_cost_comparison() {
    use std::time::Instant;
    let store = SessionStore::new(1, 2_000);
    let id = store.start_session(Some("project".into()), None).session_id;
    let contract = SessionToolContract {
        risk_class: "read_only",
        read_like: true,
        write_like: false,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        project_write: false,
        path_hint: crate::events::SessionPathHint::SinglePath,
    };
    for i in 0..1_000 {
        let start = store.record_tool_call_started(
            Some(&id),
            SessionTransport::Api,
            "read_files",
            &json!({"project":"project", "items":[{"path":format!("file_{i}.rs")}]}),
            contract,
        );
        store.record_tool_call_finished(start, true, &json!({}), None, None);
    }
    churn(&store);
    assert!(store.cold_payload_bytes_for_test(&id).is_some());
    let reference = store
        .with_record_for_query(&id, |record, _| {
            (record.events_observed, record.message_observation_revision)
        })
        .unwrap();
    assert_eq!(store.retained_revisions(&id), Some(reference));
    let loops = 20_u128;
    let mut baseline = 0_u128;
    let mut fast = 0_u128;
    for i in 0..loops {
        if i % 2 == 0 {
            let t = Instant::now();
            std::hint::black_box(
                store
                    .with_record_for_query(&id, |record, _| {
                        (record.events_observed, record.message_observation_revision)
                    })
                    .unwrap(),
            );
            baseline += t.elapsed().as_nanos();
        }
        let t = Instant::now();
        std::hint::black_box(store.handoff_revision(&id).unwrap());
        fast += t.elapsed().as_nanos();
        if i % 2 != 0 {
            let t = Instant::now();
            std::hint::black_box(
                store
                    .with_record_for_query(&id, |record, _| {
                        (record.events_observed, record.message_observation_revision)
                    })
                    .unwrap(),
            );
            baseline += t.elapsed().as_nanos();
        }
    }
    println!(
        "cold_revision events=2000 legacy_avg_ms={:.3} indexed_avg_ms={:.5} ratio={:.1}x",
        baseline as f64 / loops as f64 / 1e6,
        fast as f64 / loops as f64 / 1e6,
        baseline as f64 / fast as f64
    );
}
