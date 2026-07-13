use super::super::project_instructions::ProjectInstructionsSnapshot;
use super::super::tool_inputs::SessionMode;
use super::events::{changed_paths_for_tool, SessionToolClassification};
use super::model::{
    PersistedSessionLedger, PersistedSessionRecord, SessionLifecycle, MAX_VALIDATION_EXCERPT_CHARS,
    MESSAGE_ID_PREFIX, SESSION_ID_PREFIX, SESSION_LEDGER_VERSION,
};
use super::persistence::write_ledger_atomic;
use super::*;
use serde_json::{json, Value};
use std::path::PathBuf;

#[test]
fn session_tool_classification_uses_definition_policy() {
    for (tool, risk_class) in [
        ("show_changes", "read_only"),
        ("start_session", "read_only"),
        ("write_project_file", "project_write"),
        ("apply_patch_checked", "project_write"),
        ("run_shell", "job_run"),
        ("cargo_test", "job_run"),
        ("definitely_not_a_tool", "unknown"),
    ] {
        assert_eq!(
            SessionToolClassification::for_tool(tool).risk_class,
            risk_class,
            "{tool}"
        );
    }
}

#[test]
fn changed_paths_single_path_and_path_list_from_metadata() {
    assert_eq!(
        changed_paths_for_tool(
            "write_project_file",
            &json!({"project": "demo", "path": " src/lib.rs "}),
        ),
        vec!["src/lib.rs".to_string()]
    );
    assert_eq!(
        changed_paths_for_tool(
            "delete_project_files",
            &json!({"project": "demo", "paths": ["src/lib.rs", "", "src/lib.rs", "README.md"]}),
        ),
        vec!["src/lib.rs".to_string(), "README.md".to_string()]
    );
    assert_eq!(
        changed_paths_for_tool(
            "save_project_artifact",
            &json!({"project": "demo", "path": "out/image.png"}),
        ),
        vec!["out/image.png".to_string()]
    );
    assert!(changed_paths_for_tool(
        "read_file",
        &json!({"project": "demo", "path": "src/lib.rs"}),
    )
    .is_empty());
    assert!(changed_paths_for_tool(
        "apply_patch_checked",
        &json!({"project": "demo", "patch": "--- a/src/lib.rs\n+++ b/src/lib.rs\n"}),
    )
    .is_empty());
}

#[test]
fn session_store_bounds_event_limit() {
    let store = SessionStore::new(10, 3);
    let summary = store.start_session(None, None);
    for idx in 0..5 {
        let args = json!({"project": "demo", "path": format!("file{idx}.rs")});
        let start = store.record_tool_call_started(
            Some(&summary.session_id),
            SessionTransport::Api,
            "write_project_file",
            &args,
        );
        store.record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let summary = store.summary(&summary.session_id, Some(50)).unwrap();
    assert_eq!(summary.events.len(), 3);
    assert_eq!(summary.counts.tool_calls, 2);
}

#[test]
fn input_summary_redacts_sensitive_keys() {
    let store = SessionStore::default();
    let summary = store.start_session(None, None);
    store.record_tool_call_started(
        Some(&summary.session_id),
        SessionTransport::Api,
        "read_file",
        &json!({
            "project": "demo",
            "token": "super-secret-token",
            "command": "curl -H 'Authorization: Bearer wc_pat_never_store'"
        }),
    );
    let summary = store.summary(&summary.session_id, Some(10)).unwrap();
    assert_eq!(
        summary.events[0].input_summary.as_ref().unwrap()["token"],
        "[redacted]"
    );
    assert_eq!(
        summary.events[0].input_summary.as_ref().unwrap()["command"],
        "[redacted]"
    );
}

fn persistent_store(path: PathBuf) -> SessionStore {
    SessionStore::with_persistence(path, 10, 10)
}

#[test]
fn session_store_persists_and_restores_basic_session() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(
        Some("agent:oe:private-drop".to_string()),
        Some("persistent work".to_string()),
    );

    let restored = persistent_store(ledger);
    let status = restored.status();
    assert_eq!(status.persistence, "enabled");
    assert_eq!(status.restored_sessions, 1);
    assert_eq!(status.last_persist_error, None);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.session_id, session.session_id);
    assert_eq!(summary.project.as_deref(), Some("agent:oe:private-drop"));
    assert_eq!(summary.title.as_deref(), Some("persistent work"));
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
}

#[test]
fn session_messages_survive_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("discussion".to_string()));
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "keep OpenAPI operation count stable",
    );
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "ledger snapshot wired",
    );

    let restored = persistent_store(ledger);
    let messages = restored
        .list_messages(&session.session_id, ListSessionMessagesFilter::default())
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].message, "ledger snapshot wired");
    assert_eq!(messages[1].kind, SessionMessageKind::Guidance);
    let discussion = restored
        .discussion_summary(&session.session_id, Some(10))
        .unwrap();
    assert_eq!(discussion.counts.total, 2);
    assert_eq!(discussion.counts.guidance, 1);
    assert_eq!(discussion.counts.progress, 1);
}

#[test]
fn session_events_survive_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("events".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "git_log",
        &json!({"project": "agent:oe:private-drop", "limit": 1}),
    );
    store.record_tool_call_finished(start, true, &json!({}), None, None);

    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.events.len(), 2);
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.git_like, 1);
    assert_eq!(summary.events[1].tool_name, "git_log");
}

#[test]
fn validation_output_summary_survives_restore_sanitized() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("validation output".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({"project": "agent:eval:demo"}),
    );
    store.record_tool_call_finished(
        start,
        false,
        &json!({
            "exit_code": 101,
            "stdout": "full stdout body must not persist",
            "stderr": "full stderr body must not persist",
            "stdout_tail": "token=supersecret\nsafe stdout line\n",
            "stderr_tail": "Authorization: Bearer supersecret\nerror[E0308]: mismatched types\n --> src/lib.rs:12:5\n",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
        Some("tool failed"),
        None,
    );

    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();
    let output_summary = finished.validation_output_summary.as_ref().unwrap();
    let stdout_excerpt = output_summary["stdout_tail_excerpt"].as_str().unwrap();
    let stderr_excerpt = output_summary["stderr_tail_excerpt"].as_str().unwrap();

    assert_eq!(output_summary["tool_name"], "cargo_check");
    assert!(stdout_excerpt.contains("safe stdout line"));
    assert!(stderr_excerpt.contains("error[E0308]"));
    assert!(stderr_excerpt.contains("--> src/lib.rs:12:5"));
    for leaked in [
        "full stdout body must not persist",
        "full stderr body must not persist",
        "token=supersecret",
        "Authorization: Bearer supersecret",
    ] {
        assert!(
            !serde_json::to_string(output_summary)
                .unwrap()
                .contains(leaked),
            "restored validation_output_summary leaked {leaked}: {output_summary}"
        );
    }
    assert!(stdout_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert!(stderr_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert_eq!(output_summary["stdout_truncated"], true);
    assert_eq!(output_summary["stderr_truncated"], true);
}

#[test]
fn malicious_persisted_validation_output_summary_is_resanitized_on_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("malicious validation".to_string()));
    for tool_name in ["cargo_check", "run_shell"] {
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            tool_name,
            &json!({"project": "agent:eval:demo"}),
        );
        store.record_tool_call_finished(
            start,
            false,
            &json!({"exit_code": 101}),
            Some("tool failed"),
            None,
        );
    }

    let mut ledger_value: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let events = ledger_value["sessions"][0]["events"]
        .as_array_mut()
        .unwrap();
    for event in events {
        if event["kind"] != "tool_call_finished" {
            continue;
        }
        let tool_name = event["tool_name"].clone();
        event["validation_output_summary"] = json!({
            "tool_name": tool_name,
            "stdout_tail_excerpt": format!(
                "token=abc\nsecret=abc\npassword=abc\napi_key=abc\n{}STDOUT_SAFE_END",
                "x".repeat(MAX_VALIDATION_EXCERPT_CHARS + 64)
            ),
            "stderr_tail_excerpt": format!(
                "authorization: basic abc\nbearer abc\nprivate key abc\naccess key abc\n{}STDERR_SAFE_END",
                "y".repeat(MAX_VALIDATION_EXCERPT_CHARS + 64)
            ),
            "stdout_truncated": false,
            "stderr_truncated": false,
            "max_excerpt_chars": 999999,
        });
    }
    std::fs::write(&ledger, serde_json::to_vec_pretty(&ledger_value).unwrap()).unwrap();

    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    let cargo_finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "cargo_check")
        .unwrap();
    let run_shell_finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "run_shell")
        .unwrap();
    let output_summary = cargo_finished.validation_output_summary.as_ref().unwrap();
    let stdout_excerpt = output_summary["stdout_tail_excerpt"].as_str().unwrap();
    let stderr_excerpt = output_summary["stderr_tail_excerpt"].as_str().unwrap();
    let serialized = serde_json::to_string(output_summary).unwrap();

    assert!(stdout_excerpt.contains("STDOUT_SAFE_END"));
    assert!(stderr_excerpt.contains("STDERR_SAFE_END"));
    assert!(stdout_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert!(stderr_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert_eq!(
        output_summary["max_excerpt_chars"],
        MAX_VALIDATION_EXCERPT_CHARS
    );
    assert_eq!(output_summary["stdout_truncated"], true);
    assert_eq!(output_summary["stderr_truncated"], true);
    for leaked in [
        "token=abc",
        "secret=abc",
        "password=abc",
        "api_key=abc",
        "authorization: basic abc",
        "bearer abc",
        "private key abc",
        "access key abc",
    ] {
        assert!(
            !serialized.contains(leaked),
            "restored validation_output_summary leaked {leaked}: {serialized}"
        );
    }
    assert!(
        run_shell_finished.validation_output_summary.is_none(),
        "non-cargo tool validation_output_summary must be discarded"
    );
}

#[test]
fn legacy_session_events_without_validation_output_summary_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("legacy validation".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({"project": "agent:eval:demo"}),
    );
    store.record_tool_call_finished(start, true, &json!({"exit_code": 0}), None, None);

    let ledger_text = std::fs::read_to_string(&ledger).unwrap();
    assert!(
        !ledger_text.contains("validation_output_summary"),
        "legacy fixture should omit validation_output_summary: {ledger_text}"
    );
    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();

    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(finished.tool_name, "cargo_check");
    assert!(finished.validation_output_summary.is_none());
}

#[test]
fn resolved_message_survives_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, None);
    let message = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "finish persistence tests",
    );
    store
        .resolve_message(
            &session.session_id,
            &message.message_id,
            Some("covered".to_string()),
        )
        .unwrap();

    let restored = persistent_store(ledger);
    let messages = restored
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Resolved),
                limit: Some(10),
            },
        )
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].status, SessionMessageStatus::Resolved);
    assert_eq!(messages[0].resolution.as_deref(), Some("covered"));
    assert!(messages[0].resolved_at.is_some());
}

#[test]
fn corrupted_ledger_does_not_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    std::fs::write(&ledger, "{not valid json").unwrap();

    let store = persistent_store(ledger);
    let status = store.status();
    assert_eq!(status.persistence, "enabled");
    assert_eq!(status.restored_sessions, 0);
    assert!(status
        .last_persist_error
        .as_deref()
        .unwrap()
        .contains("restore_failed"));
    assert!(store.summary("wc_sess_missing", None).is_none());
}

#[test]
fn concurrent_persistence_reloads_current_snapshot_before_write() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("ordered writes".to_string()));
    let (old_snapshot_ready_tx, old_snapshot_ready_rx) = std::sync::mpsc::channel();
    let (allow_old_write_tx, allow_old_write_rx) = std::sync::mpsc::channel();

    let delayed_store = store.clone();
    let delayed_write = std::thread::spawn(move || {
        delayed_store.persist_after_mutation_with(|path, ledger| {
            old_snapshot_ready_tx.send(()).unwrap();
            allow_old_write_rx.recv().unwrap();
            write_ledger_atomic(path, ledger)
        });
    });
    old_snapshot_ready_rx.recv().unwrap();

    let newer_store = store.clone();
    let newer_session_id = session.session_id.clone();
    let newer_mutation = std::thread::spawn(move || {
        post_message(
            &newer_store,
            &newer_session_id,
            SessionMessageKind::Progress,
            "newer mutation",
        );
    });

    let mut newer_message_visible = false;
    for _ in 0..100 {
        let messages = store
            .list_messages(&session.session_id, ListSessionMessagesFilter::default())
            .unwrap();
        if messages
            .iter()
            .any(|message| message.message == "newer mutation")
        {
            newer_message_visible = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(newer_message_visible);

    allow_old_write_tx.send(()).unwrap();
    delayed_write.join().unwrap();
    newer_mutation.join().unwrap();

    let restored = persistent_store(ledger);
    let messages = restored
        .list_messages(&session.session_id, ListSessionMessagesFilter::default())
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message, "newer mutation");
}

#[test]
fn project_instructions_content_not_persisted_or_leaked_after_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let secret_body = "secret project rule that must not persist";
    let store = persistent_store(ledger.clone());
    let session = store.start_session_with_options(SessionCreateOptions {
        project: Some("agent:oe:private-drop".to_string()),
        title: Some("instructions".to_string()),
        mode: SessionMode::Normal,
        guards: SessionGuards::default(),
        project_instructions: Some(ProjectInstructionsSnapshot::from_single_file(
            "AGENTS.md",
            secret_body.to_string(),
            1,
        )),
    });

    let serialized = std::fs::read_to_string(&ledger).unwrap();
    assert!(!serialized.contains(secret_body));
    assert!(!serialized.contains("project_instructions"));
    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert!(summary.project_instructions.is_none());
    let summary_json = serde_json::to_string(&summary).unwrap();
    assert!(!summary_json.contains(secret_body));
}

fn post_message(
    store: &SessionStore,
    session_id: &str,
    kind: SessionMessageKind,
    message: &str,
) -> SessionMessage {
    store
        .post_message(PostSessionMessageInput {
            session_id: session_id.to_string(),
            kind,
            message: message.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap()
}

#[test]
fn post_session_message_creates_message() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let message = store
        .post_message(PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: SessionMessageKind::Guidance,
            message: "Keep this behind callRuntimeTool.".to_string(),
            tags: vec!["openapi".to_string(), "constraint".to_string()],
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();

    assert!(message.message_id.starts_with(MESSAGE_ID_PREFIX));
    assert_eq!(message.session_id, session.session_id);
    assert_eq!(message.kind, SessionMessageKind::Guidance);
    assert_eq!(message.status, SessionMessageStatus::Open);
    assert_eq!(message.priority, SessionMessagePriority::High);
    assert_eq!(message.message, "Keep this behind callRuntimeTool.");
    assert_eq!(message.tags, vec!["openapi", "constraint"]);
}

#[test]
fn list_session_messages_filters_and_clamps_limit() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "g1",
    );
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "p1",
    );
    post_message(&store, &session.session_id, SessionMessageKind::Risk, "r1");

    let guidance = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Guidance),
                status: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(guidance.len(), 1);
    assert_eq!(guidance[0].kind, SessionMessageKind::Guidance);

    let open = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: None,
                status: Some(SessionMessageStatus::Open),
                limit: Some(usize::MAX),
            },
        )
        .unwrap();
    assert_eq!(open.len(), 3);
    assert_eq!(open[0].message, "r1");
}

#[test]
fn resolve_session_message_is_idempotent() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let message = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "fix it",
    );

    let resolved = store
        .resolve_message(
            &session.session_id,
            &message.message_id,
            Some("Done".to_string()),
        )
        .unwrap();
    assert_eq!(resolved.status, SessionMessageStatus::Resolved);
    assert!(resolved.resolved_at.is_some());
    assert_eq!(resolved.resolution.as_deref(), Some("Done"));

    let resolved_again = store
        .resolve_message(&session.session_id, &message.message_id, None)
        .unwrap();
    assert_eq!(resolved_again.status, SessionMessageStatus::Resolved);
    assert_eq!(resolved_again.resolution.as_deref(), Some("Done"));
}

#[test]
fn session_message_unknown_errors_are_explicit() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let unknown_session = store.post_message(PostSessionMessageInput {
        session_id: "wc_sess_missing".to_string(),
        kind: SessionMessageKind::Note,
        message: "hello".to_string(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    });
    assert!(matches!(
        unknown_session,
        Err(SessionMessageError::UnknownSession)
    ));

    let unknown_message = store.resolve_message(&session.session_id, "wc_msg_missing", None);
    assert!(matches!(
        unknown_message,
        Err(SessionMessageError::UnknownMessage)
    ));
}

#[test]
fn session_summary_includes_bounded_message_summary() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Guidance,
        "g1",
    );
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "p1",
    );
    post_message(&store, &session.session_id, SessionMessageKind::Risk, "r1");
    post_message(&store, &session.session_id, SessionMessageKind::Todo, "t1");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    assert_eq!(summary.messages.total, 4);
    assert_eq!(summary.messages.open, 4);
    assert_eq!(summary.messages.pending_guidance, 1);
    assert_eq!(summary.messages.open_risks, 1);
    assert_eq!(summary.messages.open_todos, 1);
    assert_eq!(summary.messages.recent_progress.len(), 1);
    assert!(serde_json::to_value(summary)
        .unwrap()
        .get("messages")
        .is_some());
}

fn test_binding_key(project: &str) -> CurrentSessionKey {
    CurrentSessionKey {
        principal_kind: "test".to_string(),
        principal_id: "principal-1".to_string(),
        transport: SessionTransport::Api.as_str().to_string(),
        resolved_project: project.to_string(),
    }
}

/// Create-entry funnel: convenience wrappers must produce the same shape as
/// the authoritative `start_session_with_options` path.
#[test]
fn start_session_wrappers_funnel_to_single_create_entry() {
    let store = SessionStore::default();

    let via_start = store.start_session(Some("proj-a".to_string()), Some("t1".to_string()));
    let via_guards = store.start_session_with_guards(
        Some("proj-a".to_string()),
        Some("t2".to_string()),
        SessionMode::Normal,
        SessionGuards::default(),
    );
    let via_options = store.start_session_with_options(SessionCreateOptions {
        project: Some("proj-a".to_string()),
        title: Some("t3".to_string()),
        mode: SessionMode::Normal,
        guards: SessionGuards::default(),
        project_instructions: None,
    });
    let via_read_only = store.start_session_with_guards(
        Some("proj-a".to_string()),
        Some("ro".to_string()),
        SessionMode::ReadOnly,
        SessionGuards::default(),
    );

    for summary in [&via_start, &via_guards, &via_options, &via_read_only] {
        assert!(summary.session_id.starts_with("wc_sess_"));
        assert!(store.contains_session(&summary.session_id));
        assert_eq!(summary.project.as_deref(), Some("proj-a"));
    }
    assert_eq!(via_start.mode, SessionMode::Normal);
    assert!(!via_start.guards.deny_write_tools);
    assert!(!via_start.guards.deny_shell_tools);
    assert_eq!(via_read_only.mode, SessionMode::ReadOnly);
    assert!(via_read_only.guards.deny_write_tools);
    assert!(via_read_only.guards.deny_shell_tools);
    assert!(store
        .guard_denial(&via_read_only.session_id, "write_project_file")
        .is_some());
    assert!(store
        .guard_denial(&via_start.session_id, "write_project_file")
        .is_none());
}

/// Unknown sessions never accept events or messages, and are not recreated.
#[test]
fn unknown_session_mutations_do_not_recreate_session() {
    let store = SessionStore::default();
    let missing = "wc_sess_does_not_exist";

    assert!(!store.contains_session(missing));
    assert!(store.summary(missing, None).is_none());
    assert!(store
        .record_tool_call_started(
            Some(missing),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "demo", "path": "a.rs"}),
        )
        .is_none());
    assert!(!store.contains_session(missing));

    let post = store.post_message(PostSessionMessageInput {
        session_id: missing.to_string(),
        kind: SessionMessageKind::Note,
        message: "nope".to_string(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    });
    assert!(matches!(post, Err(SessionMessageError::UnknownSession)));
    assert!(!store.contains_session(missing));

    let resolve = store.resolve_message(missing, "wc_msg_x", None);
    assert!(matches!(resolve, Err(SessionMessageError::UnknownSession)));
}

/// Evicted (capacity-bound) sessions stay gone: events must not revive them.
#[test]
fn evicted_session_is_not_reactivated_by_events_or_messages() {
    let store = SessionStore::new(1, 10);
    let first = store.start_session(None, Some("first".to_string()));
    let second = store.start_session(None, Some("second".to_string()));

    assert!(!store.contains_session(&first.session_id));
    assert!(store.contains_session(&second.session_id));

    assert!(store
        .record_tool_call_started(
            Some(&first.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "demo", "path": "a.rs"}),
        )
        .is_none());
    assert!(!store.contains_session(&first.session_id));
    assert!(store.summary(&first.session_id, None).is_none());

    let post = store.post_message(PostSessionMessageInput {
        session_id: first.session_id.clone(),
        kind: SessionMessageKind::Note,
        message: "revive?".to_string(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    });
    assert!(matches!(post, Err(SessionMessageError::UnknownSession)));
    assert!(!store.contains_session(&first.session_id));
}

#[test]
fn bind_unbind_current_session_is_consistent() {
    let store = SessionStore::default();
    let session = store.start_session(Some("proj".to_string()), None);
    let key = test_binding_key("proj");

    assert!(store.current_session(&key).is_none());
    let bound = store
        .bind_current_session(key.clone(), &session.session_id)
        .expect("bind known session");
    assert_eq!(bound.session_id, session.session_id);
    assert_eq!(
        store.current_session_id(&key).as_deref(),
        Some(session.session_id.as_str())
    );

    assert!(store.unbind_current_session(&key));
    assert!(!store.unbind_current_session(&key));
    assert!(store.current_session(&key).is_none());

    // Unknown session cannot be bound.
    assert!(store
        .bind_current_session(key.clone(), "wc_sess_missing")
        .is_none());
    assert!(store.current_session(&key).is_none());
}

#[test]
fn stale_binding_is_cleared_when_session_missing() {
    let store = SessionStore::new(1, 10);
    let first = store.start_session(Some("proj".to_string()), None);
    let key = test_binding_key("proj");
    store
        .bind_current_session(key.clone(), &first.session_id)
        .unwrap();

    // Evict first by creating a second session (max_sessions = 1).
    let _second = store.start_session(Some("proj".to_string()), None);
    assert!(!store.contains_session(&first.session_id));

    // Lookup must clear the stale binding rather than returning a ghost id.
    assert!(store.current_session(&key).is_none());
    assert!(store.current_session_id(&key).is_none());
}

#[test]
fn message_post_and_resolve_round_trip_through_store() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let posted = store
        .post_message(PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "do the thing".to_string(),
            tags: vec!["work".to_string()],
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    assert_eq!(posted.status, SessionMessageStatus::Open);

    let listed = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Open),
                limit: Some(10),
            },
        )
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message_id, posted.message_id);

    let resolved = store
        .resolve_message(
            &session.session_id,
            &posted.message_id,
            Some("shipped".to_string()),
        )
        .unwrap();
    assert_eq!(resolved.status, SessionMessageStatus::Resolved);
    assert_eq!(resolved.resolution.as_deref(), Some("shipped"));
    let first_resolved_at = resolved.resolved_at.expect("resolved_at set");

    // Resolved messages are not reopened by a second resolve.
    let again = store
        .resolve_message(
            &session.session_id,
            &posted.message_id,
            Some("still done".to_string()),
        )
        .unwrap();
    assert_eq!(again.status, SessionMessageStatus::Resolved);
    assert_eq!(again.resolved_at, Some(first_resolved_at));
    assert_eq!(again.resolution.as_deref(), Some("still done"));

    let open = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: None,
                status: Some(SessionMessageStatus::Open),
                limit: None,
            },
        )
        .unwrap();
    assert!(open.is_empty());
}

#[test]
fn read_only_guards_block_write_and_shell_classifications() {
    let store = SessionStore::default();
    let normal =
        store.start_session_with_guards(None, None, SessionMode::Normal, SessionGuards::default());
    let read_only = store.start_session_with_guards(
        None,
        None,
        SessionMode::ReadOnly,
        SessionGuards::default(),
    );

    assert!(store
        .guard_denial(&normal.session_id, "write_project_file")
        .is_none());
    assert!(store
        .guard_denial(&normal.session_id, "run_shell")
        .is_none());

    let write_denial = store
        .guard_denial(&read_only.session_id, "write_project_file")
        .expect("write denied");
    assert_eq!(write_denial.guard, "deny_write_tools");
    assert_eq!(write_denial.mode, SessionMode::ReadOnly);

    let shell_denial = store
        .guard_denial(&read_only.session_id, "run_shell")
        .expect("shell denied");
    assert_eq!(shell_denial.guard, "deny_shell_tools");

    // Reads remain allowed under read_only.
    assert!(store
        .guard_denial(&read_only.session_id, "read_file")
        .is_none());
}

#[test]
fn ledger_round_trip_preserves_session_events_and_messages() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(Some("proj".to_string()), Some("persist".to_string()));
    let start = store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "proj", "path": "src/lib.rs"}),
        )
        .unwrap();
    store.record_tool_call_finished(Some(start), true, &json!({}), None, None);
    post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Progress,
        "checkpoint",
    );

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let summary = restored.summary(&session.session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some("proj"));
    assert_eq!(summary.title.as_deref(), Some("persist"));
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
    assert_eq!(summary.counts.tool_calls, 1);
    assert!(summary
        .events
        .iter()
        .any(|event| event.kind == "tool_call_finished"));
    assert_eq!(summary.messages.total, 1);
    assert_eq!(summary.messages.progress, 1);

    // Process-local bindings are intentionally not durable.
    let key = test_binding_key("proj");
    assert!(restored.current_session(&key).is_none());
}

// --- Workflow session lifecycle (Phase 1: field + ledger default only) ---

#[test]
fn new_session_defaults_lifecycle_to_active() {
    let store = SessionStore::default();
    let summary = store.start_session(None, Some("lifecycle".to_string()));
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);

    // Summary JSON exposes lifecycle for observability.
    let value = serde_json::to_value(&summary).unwrap();
    assert_eq!(value["lifecycle"], "active");
}

#[test]
fn persisted_ledger_writes_and_reads_lifecycle_active() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(Some("proj".to_string()), Some("with lifecycle".to_string()));
    assert_eq!(session.lifecycle, SessionLifecycle::Active);

    let raw = std::fs::read_to_string(&ledger).unwrap();
    let ledger_value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(ledger_value["version"], SESSION_LEDGER_VERSION);
    assert_eq!(ledger_value["sessions"][0]["lifecycle"], "active");
    assert_eq!(
        ledger_value["sessions"][0]["session_id"].as_str().unwrap(),
        session.session_id
    );

    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
}

#[test]
fn legacy_ledger_without_lifecycle_loads_as_active() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger_path = tmp.path().join("sessions.json");
    // Pre-lifecycle JSON shape: no `lifecycle` key on the session record.
    let legacy = json!({
        "version": SESSION_LEDGER_VERSION,
        "sessions": [{
            "session_id": "wc_sess_legacylifecycle01",
            "project": "proj-legacy",
            "title": "old row",
            "mode": "normal",
            "guards": {
                "deny_write_tools": false,
                "deny_shell_tools": false
            },
            "created_at": 1_700_000_000,
            "updated_at": 1_700_000_100,
            "events": [],
            "messages": []
        }]
    });
    std::fs::write(&ledger_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

    // Missing field must not fail serde (#[serde(default)] → Active).
    let restored = persistent_store(ledger_path);
    let status = restored.status();
    assert_eq!(status.restored_sessions, 1);
    assert_eq!(status.last_persist_error, None);

    let summary = restored
        .summary("wc_sess_legacylifecycle01", Some(10))
        .unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
    assert_eq!(summary.project.as_deref(), Some("proj-legacy"));
    assert_eq!(summary.title.as_deref(), Some("old row"));
    assert_eq!(summary.mode, SessionMode::Normal);
}

#[test]
fn lifecycle_ledger_round_trip_preserves_active() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("round trip".to_string()));
    assert_eq!(session.lifecycle, SessionLifecycle::Active);

    let restored = persistent_store(ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
    assert_eq!(summary.session_id, session.session_id);
}

#[test]
fn persisted_session_record_serde_defaults_missing_lifecycle() {
    // Direct serde check: omit lifecycle entirely; deserialize succeeds as Active.
    let json = r#"{
        "session_id": "wc_sess_serde_default_ok",
        "project": null,
        "title": null,
        "mode": "normal",
        "guards": {"deny_write_tools": false, "deny_shell_tools": false},
        "created_at": 10,
        "updated_at": 20,
        "events": [],
        "messages": []
    }"#;
    let record: PersistedSessionRecord = serde_json::from_str(json).unwrap();
    assert_eq!(record.lifecycle, SessionLifecycle::Active);
    assert!(record.session_id.starts_with(SESSION_ID_PREFIX));

    let ledger_json = format!(
        r#"{{"version":{version},"sessions":[{record}]}}"#,
        version = SESSION_LEDGER_VERSION,
        record = json
    );
    let ledger: PersistedSessionLedger = serde_json::from_str(&ledger_json).unwrap();
    assert_eq!(ledger.version, SESSION_LEDGER_VERSION);
    assert_eq!(ledger.sessions.len(), 1);
    assert_eq!(ledger.sessions[0].lifecycle, SessionLifecycle::Active);
}

#[test]
fn session_lifecycle_wire_values_are_snake_case() {
    assert_eq!(
        serde_json::to_value(SessionLifecycle::Active).unwrap(),
        json!("active")
    );
    assert_eq!(
        serde_json::to_value(SessionLifecycle::Closed).unwrap(),
        json!("closed")
    );
    assert_eq!(
        serde_json::to_value(SessionLifecycle::Archived).unwrap(),
        json!("archived")
    );
    assert_eq!(
        serde_json::from_value::<SessionLifecycle>(json!("active")).unwrap(),
        SessionLifecycle::Active
    );
    assert_eq!(SessionLifecycle::default(), SessionLifecycle::Active);
}
