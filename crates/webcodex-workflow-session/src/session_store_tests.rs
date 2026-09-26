use crate::model::{CodingSessionOutcome, MAX_SUMMARY_LIMIT, MESSAGE_ID_PREFIX};
use crate::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use webcodex_core::workflow_session_contract::{ExecutionShell, SessionMode};

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

fn session_tool_contract(tool_name: &str) -> SessionToolContract {
    let (read_like, write_like, shell_like, path_hint) = match tool_name {
        "read_file" => (true, false, false, SessionPathHint::SinglePath),
        "write_project_file" => (false, true, false, SessionPathHint::SinglePath),
        "apply_text_edits" => (false, true, false, SessionPathHint::PathList),
        "run_shell" | "session_shell_exec" | "cargo_check" => {
            (false, false, true, SessionPathHint::None)
        }
        _ => (true, false, false, SessionPathHint::None),
    };
    SessionToolContract {
        risk_class: if write_like { "write" } else { "read" },
        read_like,
        write_like,
        shell_like,
        git_like: false,
        change_summary_like: false,
        project_write: write_like,
        path_hint,
    }
}

#[test]
fn compound_search_observation_uses_nested_successful_matches() {
    let paths = crate::events::observed_paths_for_successful_result(
        "search_and_read",
        Vec::new(),
        &json!({
            "search": {"matches": [
                {"path": "src/lib.rs", "line": 2},
                {"path": "src/lib.rs", "line": 4},
                {"path": "../outside.rs", "line": 1}
            ]},
            "reads": {"items": []}
        }),
    );
    assert_eq!(paths, vec!["src/lib.rs"]);
    let empty = crate::events::observed_paths_for_successful_result(
        "search_and_read",
        Vec::new(),
        &json!({"search": {"matches": []}, "reads": []}),
    );
    assert!(empty.is_empty());
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
            session_tool_contract("write_project_file"),
        );
        store.record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let summary = store.summary(&summary.session_id, Some(50)).unwrap();
    assert_eq!(summary.events.len(), 3);
    assert_eq!(summary.counts.tool_calls, 2);
    assert_eq!(summary.events_retained, 3);
    assert_eq!(summary.events_evicted, summary.events_total - 3);
    assert!(summary.retention_truncated);
    assert_eq!(
        summary.ledger_first_retained_sequence,
        summary.events_evicted
    );
}

#[test]
fn default_session_retention_exceeds_model_summary_window_and_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(
        &ledger,
        DEFAULT_MAX_SESSIONS,
        DEFAULT_MAX_EVENTS_PER_SESSION,
    );
    assert_eq!(store.status().max_events_per_session, 2000);
    assert!(DEFAULT_MAX_EVENTS_PER_SESSION > MAX_SUMMARY_LIMIT);

    let session = store.start_session(Some("agent:test:long-session".to_string()), None);
    for index in 0..160 {
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "runtime_status",
            &json!({"probe": index}),
            session_tool_contract("runtime_status"),
        );
        store.record_tool_call_finished(start, true, &json!({"ok": true}), None, None);
    }
    store.flush_persistence();

    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let persisted = raw["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["session_id"] == session.session_id)
        .unwrap();
    let persisted_events = persisted["events"].as_array().unwrap().len();
    assert!(persisted_events > MAX_SUMMARY_LIMIT, "{persisted_events}");
    assert!(persisted_events <= DEFAULT_MAX_EVENTS_PER_SESSION);

    let summary = store
        .summary(&session.session_id, Some(usize::MAX))
        .unwrap();
    assert_eq!(summary.events_returned, MAX_SUMMARY_LIMIT);
    assert_eq!(summary.events.len(), MAX_SUMMARY_LIMIT);
    assert_eq!(summary.events_total, persisted_events);
    assert_eq!(summary.events_retained, persisted_events);
    assert_eq!(summary.events_evicted, 0);
    assert!(!summary.retention_truncated);
    assert_eq!(summary.ledger_first_retained_sequence, 0);
    assert!(summary.events_truncated);
    assert_eq!(
        summary.first_retained_sequence,
        persisted_events - MAX_SUMMARY_LIMIT
    );

    drop(store);
    let restored = SessionStore::with_persistence(
        &ledger,
        DEFAULT_MAX_SESSIONS,
        DEFAULT_MAX_EVENTS_PER_SESSION,
    );
    let restored_summary = restored
        .summary(&session.session_id, Some(usize::MAX))
        .unwrap();
    assert_eq!(restored_summary.events_total, persisted_events);
    assert_eq!(restored_summary.events_returned, MAX_SUMMARY_LIMIT);
    assert_eq!(restored.status().max_events_per_session, 2000);
}

#[test]
fn retained_task_instruction_identity_survives_summary_tail_and_fences_later_attempt() {
    let store = SessionStore::new(10, DEFAULT_MAX_EVENTS_PER_SESSION);
    let request = |resume_session_id: Option<String>, instruction: &str| CodingSessionRequest {
        project: "agent:test:attempt-fence".to_string(),
        authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.to_string(),
        resume_session_id,
        instruction: Some(instruction.to_string()),
        mode: SessionMode::Normal,
        guards: SessionGuards::default(),
        execution_context: None,
        project_instructions: None,
        transport: SessionTransport::Api,
        context_refreshed: true,
        write_scope_verified: true,
    };
    let created = store
        .ensure_coding_session_with_git_baseline(
            request(None, "first attempt"),
            Some("a".repeat(40)),
        )
        .unwrap();
    let session_id = created.summary.session_id.clone();
    let first_instruction_id = created
        .summary
        .events
        .iter()
        .find(|event| event.kind == "task_instruction")
        .unwrap()
        .event_id
        .clone();

    for index in 0..110 {
        let start = store.record_tool_call_started(
            Some(&session_id),
            SessionTransport::Api,
            "runtime_status",
            &json!({"probe": index}),
            session_tool_contract("runtime_status"),
        );
        store.record_tool_call_finished(start, true, &json!({"ok": true}), None, None);
    }
    let truncated = store.summary(&session_id, Some(usize::MAX)).unwrap();
    assert!(truncated.events_truncated);
    assert!(truncated
        .events
        .iter()
        .all(|event| event.kind != "task_instruction"));
    let first_attempt_events_total = truncated.events_total;
    assert_eq!(
        store.retained_task_instruction_event_id_at(&session_id, first_attempt_events_total),
        Some(first_instruction_id.clone())
    );

    let resumed = store
        .ensure_coding_session_with_git_baseline(
            request(Some(session_id.clone()), "second attempt"),
            Some("b".repeat(40)),
        )
        .unwrap();
    let second_instruction_id = resumed
        .summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "task_instruction")
        .unwrap()
        .event_id
        .clone();
    assert_ne!(second_instruction_id, first_instruction_id);
    assert_eq!(
        store.retained_task_instruction_event_id_at(&session_id, first_attempt_events_total),
        Some(first_instruction_id)
    );
    assert_eq!(
        store.retained_task_instruction_event_id_at(&session_id, resumed.summary.events_total),
        Some(second_instruction_id)
    );
}

#[test]
fn input_summary_redacts_sensitive_keys() {
    let store = SessionStore::default();
    let summary = store.start_session(None, None);
    store.record_tool_call_started(
        Some(&summary.session_id),
        SessionTransport::Api,
        "runtime_status",
        &json!({
            "token": "super-secret-token",
            "command": "curl -H 'Authorization: Bearer wc_pat_never_store'"
        }),
        session_tool_contract("runtime_status"),
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

#[test]
fn coding_instruction_redacts_reusable_credentials_without_truncating_normal_goals() {
    assert_eq!(
        super::util::redact_and_bound_instruction(
            "continue with wc_pat_never_persist_this_value",
            super::model::MAX_CODING_INSTRUCTION_CHARS,
        ),
        "[redacted]"
    );
    let goal = "x".repeat(1_000);
    assert_eq!(
        super::util::redact_and_bound_instruction(
            &goal,
            super::model::MAX_CODING_INSTRUCTION_CHARS,
        ),
        goal
    );
}

fn persistent_store(path: PathBuf) -> SessionStore {
    SessionStore::with_persistence(path, 10, 10)
}

/// Flush deferred ledger writes, then open a fresh store from the same path.
/// Required because persistent stores write on a background thread.
fn flush_and_restore(store: &SessionStore, path: PathBuf) -> SessionStore {
    store.flush_persistence();
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

    store.flush_persistence();
    let raw = std::fs::read_to_string(&ledger).unwrap();
    assert!(
        !raw.contains('\n'),
        "production ledger should use compact JSON"
    );
    let restored = SessionStore::with_persistence(ledger, 10, 10);
    let status = restored.status();
    assert_eq!(status.persistence, "enabled");
    assert_eq!(status.restored_sessions, 1);
    assert_eq!(status.last_persist_error, None);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.session_id, session.session_id);
    assert_eq!(summary.project.as_deref(), Some("agent:oe:private-drop"));
    assert_eq!(summary.title.as_deref(), Some("persistent work"));
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
    assert_eq!(
        summary.execution_context,
        SessionExecutionContext::default()
    );
}

#[test]
fn coding_git_baseline_and_repository_edit_fact_persist_resume_and_default_legacy_false() {
    fn request(resume_session_id: Option<String>) -> CodingSessionRequest {
        CodingSessionRequest {
            project: "agent:oe:private-drop".to_string(),
            authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.to_string(),
            resume_session_id,
            instruction: Some("final changes persistence".to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        }
    }

    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let baseline = "a".repeat(40);
    let replacement_candidate = "b".repeat(40);
    let created = store
        .ensure_coding_session_with_git_baseline(request(None), Some(baseline.clone()))
        .unwrap();
    assert_eq!(
        created.summary.git_baseline_tree.as_deref(),
        Some(baseline.as_str())
    );
    assert!(!created.summary.repository_edit_observed);
    let model_facing_summary = serde_json::to_value(&created.summary).unwrap();
    assert!(model_facing_summary.get("git_baseline_tree").is_none());
    assert!(model_facing_summary
        .get("repository_edit_observed")
        .is_none());

    let session_id = created.summary.session_id.clone();
    let edit = store
        .record_tool_call_started(
            Some(&session_id),
            SessionTransport::Mcp,
            "apply_text_edits",
            &json!({"project": "agent:oe:private-drop", "path": "src/lib.rs"}),
            session_tool_contract("apply_text_edits"),
        )
        .unwrap();
    store
        .record_tool_call_finished(
            Some(edit),
            true,
            &json!({"state_changed": true}),
            None,
            None,
        )
        .unwrap();

    let restored = flush_and_restore(&store, ledger.clone());
    let restored_summary = restored.summary(&session_id, Some(20)).unwrap();
    assert_eq!(
        restored_summary.git_baseline_tree.as_deref(),
        Some(baseline.as_str())
    );
    assert!(restored_summary.repository_edit_observed);

    let resumed = restored
        .ensure_coding_session_with_git_baseline(
            request(Some(session_id.clone())),
            Some(replacement_candidate),
        )
        .unwrap();
    assert!(resumed.reused);
    assert_eq!(
        resumed.summary.git_baseline_tree.as_deref(),
        Some(baseline.as_str())
    );
    assert!(resumed.summary.repository_edit_observed);

    restored.flush_persistence();
    let mut legacy: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    let row = legacy["sessions"][0].as_object_mut().unwrap();
    row.remove("git_baseline_tree");
    row.remove("repository_edit_observed");
    std::fs::write(&ledger, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let legacy_restored = SessionStore::with_persistence(ledger, 10, 10);
    let legacy_summary = legacy_restored.summary(&session_id, Some(20)).unwrap();
    assert_eq!(legacy_summary.git_baseline_tree, None);
    assert!(!legacy_summary.repository_edit_observed);
}

#[test]
fn session_execution_context_persistence_matrix() {
    let cases = [
        (
            "local-normalized",
            SessionExecutionContext {
                default_cwd: Some("frontend/./src".to_string()),
                default_shell: Some(ExecutionShell::Bash),
                resource: None,
            },
            SessionExecutionContext {
                default_cwd: Some("frontend/src".to_string()),
                default_shell: Some(ExecutionShell::Bash),
                resource: None,
            },
            json!({"default_cwd": "frontend/src", "default_shell": "bash"}),
        ),
        (
            "remote-resource",
            SessionExecutionContext {
                default_cwd: Some("/opt/webcodex-edge".to_string()),
                default_shell: None,
                resource: Some("tmp".to_string()),
            },
            SessionExecutionContext {
                default_cwd: Some("/opt/webcodex-edge".to_string()),
                default_shell: None,
                resource: Some("tmp".to_string()),
            },
            json!({"default_cwd": "/opt/webcodex-edge", "resource": "tmp"}),
        ),
    ];

    for (label, input, expected, persisted_context) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store
            .start_session_with_options(
                SessionCreateOptions::new(
                    Some("agent:oe:private-drop".to_string()),
                    Some(format!("persistent context {label}")),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_execution_context(input),
            )
            .unwrap();
        assert_eq!(session.execution_context, expected, "{label}");

        store.flush_persistence();
        let value: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
        assert_eq!(
            value["sessions"][0]["execution_context"], persisted_context,
            "{label}"
        );
        let restored = SessionStore::with_persistence(ledger.clone(), 10, 10);
        assert_eq!(
            restored
                .summary(&session.session_id, None)
                .unwrap()
                .execution_context,
            expected,
            "{label}"
        );
    }
}

#[test]
fn coding_session_context_precommit_failure_leaves_memory_unchanged() {
    fn request(
        resume_session_id: Option<String>,
        execution_context: Option<SessionExecutionContext>,
    ) -> CodingSessionRequest {
        CodingSessionRequest {
            project: "agent:oe:private-drop".to_string(),
            authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.to_string(),
            resume_session_id,
            instruction: Some("continue".to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        }
    }

    let store = SessionStore::default();
    let initial = SessionExecutionContext {
        default_cwd: Some("frontend".to_string()),
        default_shell: Some(ExecutionShell::Bash),
        resource: None,
    };
    let created = store
        .ensure_coding_session(request(None, Some(initial.clone())))
        .unwrap();
    assert!(!created.reused);
    assert!(created.execution_context_changed);
    assert_eq!(created.summary.execution_context, initial);

    let session_id = created.summary.session_id.clone();
    let preserved = store
        .ensure_coding_session(request(Some(session_id.clone()), None))
        .unwrap();
    assert!(preserved.reused);
    assert!(!preserved.execution_context_changed);
    assert_eq!(preserved.summary.execution_context, initial);

    let replacement = SessionExecutionContext {
        default_cwd: Some("backend".to_string()),
        default_shell: Some(ExecutionShell::Sh),
        resource: None,
    };
    let replaced = store
        .ensure_coding_session(request(Some(session_id.clone()), Some(replacement.clone())))
        .unwrap();
    assert!(replaced.execution_context_changed);
    assert_eq!(replaced.summary.execution_context, replacement);
    let event = replaced.summary.events.last().unwrap();
    assert_eq!(event.execution_context, Some(replacement.clone()));
    assert_eq!(event.previous_execution_context, Some(initial.clone()));

    let event_count = replaced.summary.events_total;
    store.fail_next_coding_continuity_precommit_for_test();
    let error = store
        .ensure_coding_session(request(
            Some(session_id.clone()),
            Some(SessionExecutionContext::default()),
        ))
        .unwrap_err();
    assert_eq!(error, CodingSessionError::CommitFailed);
    let after_failure = store.summary(&session_id, None).unwrap();
    assert_eq!(after_failure.execution_context, replacement);
    assert_eq!(after_failure.events_total, event_count);
}

#[test]
fn update_session_execution_context_sets_clears_and_rejects_invalid_states() {
    let store = SessionStore::default();
    let session = store.start_session(
        Some("agent:oe:private-drop".to_string()),
        Some("context update".to_string()),
    );
    let replacement = SessionExecutionContext {
        default_cwd: Some("frontend".to_string()),
        default_shell: Some(ExecutionShell::Bash),
        resource: None,
    };
    let set = store
        .update_execution_context(
            &session.session_id,
            replacement.clone(),
            SessionTransport::Api,
        )
        .unwrap();
    assert!(set.changed);
    assert_eq!(
        set.previous_execution_context,
        SessionExecutionContext::default()
    );
    assert_eq!(set.summary.execution_context, replacement);
    assert_eq!(
        set.summary.events.last().unwrap().kind,
        "session_execution_context_updated"
    );

    let cleared = store
        .update_execution_context(
            &session.session_id,
            SessionExecutionContext::default(),
            SessionTransport::Mcp,
        )
        .unwrap();
    assert!(cleared.changed);
    assert_eq!(
        cleared.summary.execution_context,
        SessionExecutionContext::default()
    );

    let before_invalid = cleared.summary.events_total;
    let invalid = store
        .update_execution_context(
            &session.session_id,
            SessionExecutionContext {
                default_cwd: Some("../outside".to_string()),
                default_shell: None,
                resource: None,
            },
            SessionTransport::Api,
        )
        .unwrap_err();
    assert!(matches!(
        invalid,
        SessionExecutionContextUpdateError::InvalidExecutionContext(_)
    ));
    assert_eq!(
        store
            .summary(&session.session_id, None)
            .unwrap()
            .events_total,
        before_invalid
    );

    assert_eq!(
        store
            .update_execution_context(
                "wc_sess_missingcontext01",
                SessionExecutionContext::default(),
                SessionTransport::Api,
            )
            .unwrap_err(),
        SessionExecutionContextUpdateError::UnknownSession
    );
    let unscoped = store.start_session(None, Some("unscoped".to_string()));
    assert_eq!(
        store
            .update_execution_context(
                &unscoped.session_id,
                SessionExecutionContext::default(),
                SessionTransport::Api,
            )
            .unwrap_err(),
        SessionExecutionContextUpdateError::SessionHasNoProject
    );
    store.close_session(&session.session_id).unwrap();
    assert_eq!(
        store
            .update_execution_context(
                &session.session_id,
                SessionExecutionContext::default(),
                SessionTransport::Api,
            )
            .unwrap_err(),
        SessionExecutionContextUpdateError::SessionNotActive {
            lifecycle: SessionLifecycle::Closed
        }
    );
}

#[test]
fn persistent_shell_evidence_survives_restore_without_command_or_output() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("persistent shell evidence".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "session_shell_exec",
        &json!({
            "project": "agent:oe:private-drop",
            "session_id": session.session_id.clone(),
            "shell_id": "wc_shell_evidence",
            "command": "export PRIVATE_LEDGER_VALUE=secret",
            "command_summary": "export PRIVATE_LEDGER_VALUE=secret",
            "command_present": true
        }),
        session_tool_contract("session_shell_exec"),
    );
    store.record_tool_call_finished(
        start,
        true,
        &json!({
            "shell_id": "wc_shell_evidence",
            "shell_state": "running",
            "execution_state": "completed",
            "command_started": true,
            "command_completed": true,
            "exit_code": 0,
            "stdout": "PRIVATE_LEDGER_VALUE=secret",
            "stderr": ""
        }),
        None,
        None,
    );
    store.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(!persisted.contains("PRIVATE_LEDGER_VALUE"));
    assert!(!persisted.contains("\"stdout\""));
    assert!(!persisted.contains("\"stderr\""));

    let restored = SessionStore::with_persistence(ledger, 10, 10);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    let evidence = summary.events[1].persistent_shell.as_ref().unwrap();
    assert_eq!(evidence.action, "exec");
    assert_eq!(evidence.shell_id.as_deref(), Some("wc_shell_evidence"));
    assert_eq!(evidence.shell_state.as_deref(), Some("running"));
    assert_eq!(evidence.execution_state.as_deref(), Some("completed"));
    assert_eq!(evidence.command_started, Some(true));
    assert_eq!(evidence.command_completed, Some(true));
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
        session_tool_contract("cargo_check"),
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

    let restored = flush_and_restore(&store, ledger);
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
            session_tool_contract(tool_name),
        );
        store.record_tool_call_finished(
            start,
            false,
            &json!({"exit_code": 101}),
            Some("tool failed"),
            None,
        );
    }

    store.flush_persistence();
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

    let restored = flush_and_restore(&store, ledger);
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
    for output_summary in [
        cargo_finished.validation_output_summary.as_ref().unwrap(),
        run_shell_finished
            .validation_output_summary
            .as_ref()
            .unwrap(),
    ] {
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
    }
}

#[test]
fn tool_call_start_and_finish_share_one_call_id() {
    let store = SessionStore::new_in_memory(10, 20);
    let session = store.start_session(
        Some("agent:eval:demo".to_string()),
        Some("correlation".to_string()),
    );
    let start = store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "agent:eval:demo", "path": "src/lib.rs"}),
            session_tool_contract("read_file"),
        )
        .expect("start recorded");
    let call_id = start.call_id.clone();
    assert!(call_id.starts_with("wc_call_"));
    store.record_tool_call_finished(
        Some(start),
        true,
        &json!({"content": "omitted"}),
        None,
        None,
    );
    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    let tool_events = summary
        .events
        .iter()
        .filter(|event| event.kind.starts_with("tool_call_"))
        .collect::<Vec<_>>();
    assert_eq!(tool_events.len(), 2);
    assert_eq!(tool_events[0].call_id.as_deref(), Some(call_id.as_str()));
    assert_eq!(tool_events[1].call_id.as_deref(), Some(call_id.as_str()));
}

#[test]
fn reused_logical_invocation_metadata_does_not_suppress_ambiguous_evidence() {
    let store = SessionStore::new_in_memory(10, 20);
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let mut business = ToolCallRecorderMetadata::default();
    business.assign_logical_invocation();
    business.mark_business_execution();

    for path in ["src/one.rs", "src/two.rs"] {
        let start = store.record_tool_call_started_with_metadata(
            Some(&session.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "agent:eval:demo", "path": path}),
            Some("agent:eval:demo".to_string()),
            business.clone(),
            session_tool_contract("read_file"),
        );
        store.record_tool_call_finished(start, true, &json!({"content": "omitted"}), None, None);
    }

    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    assert_eq!(
        summary.counts.tool_calls, 2,
        "a duplicated valid-looking business correlation is ambiguous and must stay conservative"
    );
    let canonical = super::events::canonical_tool_call_finished_events(&summary.events);
    assert_eq!(canonical.len(), 2);
}

#[test]
fn legacy_session_events_without_observed_paths_restore_with_empty_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("legacy exploration".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "read_file",
        &json!({"project": "demo", "path": "src/legacy.rs"}),
        session_tool_contract("read_file"),
    );
    store.record_tool_call_finished(start, true, &json!({"content": "omitted"}), None, None);
    store.flush_persistence();

    let mut ledger_value: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    for event in ledger_value["sessions"][0]["events"]
        .as_array_mut()
        .unwrap()
    {
        event.as_object_mut().unwrap().remove("observed_paths");
    }
    std::fs::write(&ledger, serde_json::to_vec_pretty(&ledger_value).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(ledger, 10, 10);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert!(summary
        .events
        .iter()
        .all(|event| event.observed_paths.is_empty()));
    assert_eq!(restored.status().restored_sessions, 1);
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
        session_tool_contract("cargo_check"),
    );
    store.record_tool_call_finished(start, true, &json!({"exit_code": 0}), None, None);

    store.flush_persistence();
    let ledger_text = std::fs::read_to_string(&ledger).unwrap();
    assert!(
        !ledger_text.contains("validation_output_summary"),
        "legacy fixture should omit validation_output_summary: {ledger_text}"
    );
    let restored = flush_and_restore(&store, ledger);
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

    let restored = flush_and_restore(&store, ledger);
    let messages = restored
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Resolved),
                message_id: None,
                reply_to: None,
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
                message_id: None,
                reply_to: None,
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
                message_id: None,
                reply_to: None,
                limit: Some(usize::MAX),
            },
        )
        .unwrap();
    assert_eq!(open.len(), 3);
    assert_eq!(open[0].message, "r1");
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
    let via_options = store
        .start_session_with_options(SessionCreateOptions::new(
            Some("proj-a".to_string()),
            Some("t3".to_string()),
            SessionMode::Normal,
            SessionGuards::default(),
        ))
        .unwrap();
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
        .guard_denial(
            &via_read_only.session_id,
            session_tool_contract("write_project_file")
        )
        .is_some());
    assert!(store
        .guard_denial(
            &via_start.session_id,
            session_tool_contract("write_project_file")
        )
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
            session_tool_contract("read_file"),
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

#[test]
fn active_session_survives_capacity_churn_and_keeps_exact_identity_and_fences() {
    let store = SessionStore::new(1, 10);
    let owner_authority = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let guards = SessionGuards {
        deny_write_tools: true,
        deny_shell_tools: true,
    };
    let first = store
        .start_session_with_options(
            SessionCreateOptions::new(
                Some("agent:test:durable-a".to_string()),
                Some("first".to_string()),
                SessionMode::ReadOnly,
                guards,
            )
            .with_owner_authority_fingerprint(Some(owner_authority.to_string())),
        )
        .unwrap();
    let canonical_id = first.session_id.clone();

    for index in 0..4 {
        store.start_session(
            Some(format!("agent:test:churn-{index}")),
            Some(format!("churn {index}")),
        );
    }

    assert!(store.contains_session(&canonical_id));
    let retained = store.summary(&canonical_id, Some(10)).unwrap();
    assert_eq!(retained.session_id, canonical_id);
    assert_eq!(retained.lifecycle, SessionLifecycle::Active);
    assert_eq!(retained.project.as_deref(), Some("agent:test:durable-a"));
    assert_eq!(retained.mode, SessionMode::ReadOnly);
    assert_eq!(retained.guards, guards);

    let start = store
        .record_tool_call_started(
            Some(&canonical_id),
            SessionTransport::Api,
            "read_file",
            &json!({"project": "agent:test:durable-a", "path": "a.rs"}),
            session_tool_contract("read_file"),
        )
        .expect("capacity pressure must not turn an Active Session into unknown");
    store.record_tool_call_finished(Some(start), true, &json!({"ok": true}), None, None);

    let resume = |project: &str, authority_fingerprint: &str| CodingSessionRequest {
        project: project.to_string(),
        authority_fingerprint: authority_fingerprint.to_string(),
        resume_session_id: Some(canonical_id.clone()),
        instruction: Some("resume exact identity".to_string()),
        mode: SessionMode::ReadOnly,
        guards,
        execution_context: None,
        project_instructions: None,
        transport: SessionTransport::Api,
        context_refreshed: true,
        write_scope_verified: true,
    };
    let resumed = store
        .ensure_coding_session_with_git_baseline(
            resume("agent:test:durable-a", owner_authority),
            None,
        )
        .unwrap();
    assert!(resumed.reused);
    assert_eq!(resumed.summary.session_id, canonical_id);

    assert!(matches!(
        store.ensure_coding_session_with_git_baseline(
            resume("agent:test:wrong-project", owner_authority,),
            None,
        ),
        Err(CodingSessionError::ResumeProjectMismatch { .. })
    ));
    assert!(matches!(
        store.ensure_coding_session_with_git_baseline(
            resume(
                "agent:test:durable-a",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            None,
        ),
        Err(CodingSessionError::ResumeAuthorityMismatch { .. })
    ));

    let status = store.status();
    assert_eq!(status.hot_session_capacity_target, 1);
    assert!(status.retained_sessions > status.hot_session_capacity_target);
    assert_eq!(status.active_sessions, status.retained_sessions);
    assert_eq!(status.capacity_evictions, 0);
}

#[test]
fn active_session_survives_persistence_restart_after_capacity_churn() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let owner_authority = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let request =
        |resume_session_id: Option<String>, project: &str, authority: &str| CodingSessionRequest {
            project: project.to_string(),
            authority_fingerprint: authority.to_string(),
            resume_session_id,
            instruction: Some("durable active".to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        };

    let store = SessionStore::with_persistence(&ledger, 1, 10);
    let first = store
        .ensure_coding_session_with_git_baseline(
            request(None, "agent:test:persistent-active", owner_authority),
            None,
        )
        .unwrap();
    let canonical_id = first.summary.session_id.clone();
    for index in 0..4 {
        store.start_session(
            Some(format!("agent:test:persist-churn-{index}")),
            Some(format!("churn {index}")),
        );
    }
    assert!(store.contains_session(&canonical_id));
    store.flush_persistence();
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 1, 10);
    assert!(restored.contains_session(&canonical_id));
    assert_eq!(
        restored.lifecycle_state(&canonical_id),
        Some(SessionLifecycle::Active)
    );
    assert!(restored.status().restored_sessions > 1);
    assert!(matches!(
        restored.ensure_coding_session_with_git_baseline(
            request(
                Some(canonical_id.clone()),
                "agent:test:wrong-project",
                owner_authority,
            ),
            None,
        ),
        Err(CodingSessionError::ResumeProjectMismatch { .. })
    ));
    assert!(matches!(
        restored.ensure_coding_session_with_git_baseline(
            request(
                Some(canonical_id.clone()),
                "agent:test:persistent-active",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            None,
        ),
        Err(CodingSessionError::ResumeAuthorityMismatch { .. })
    ));
    let resumed = restored
        .ensure_coding_session_with_git_baseline(
            request(
                Some(canonical_id.clone()),
                "agent:test:persistent-active",
                owner_authority,
            ),
            None,
        )
        .unwrap();
    assert!(resumed.reused);
    assert_eq!(resumed.summary.session_id, canonical_id);
}

#[test]
fn restore_boundary_preserves_older_active_sessions_beyond_hot_target() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 10);
    let oldest = store.start_session(
        Some("agent:test:restore-boundary".to_string()),
        Some("oldest active".to_string()),
    );
    let _middle = store.start_session(
        Some("agent:test:restore-boundary".to_string()),
        Some("middle active".to_string()),
    );
    let newest = store.start_session(
        Some("agent:test:restore-boundary".to_string()),
        Some("newest active".to_string()),
    );
    store.flush_persistence();
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 1, 10);
    assert!(restored.contains_session(&oldest.session_id));
    assert!(restored.contains_session(&newest.session_id));
    assert_eq!(
        restored.lifecycle_state(&oldest.session_id),
        Some(SessionLifecycle::Active)
    );
    let status = restored.status();
    assert_eq!(status.hot_session_capacity_target, 1);
    assert_eq!(status.retained_sessions, 3);
    assert_eq!(status.active_sessions, 3);
    assert_eq!(status.closed_sessions, 0);
    assert_eq!(status.capacity_evictions, 0);
}

#[test]
fn restore_prunes_only_old_closed_history_and_keeps_active_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence_limits(&ledger, 10, 10, 10);
    let active = store.start_session(
        Some("agent:test:mixed-restore".to_string()),
        Some("active".to_string()),
    );
    let old_closed = store.start_session(
        Some("agent:test:mixed-restore".to_string()),
        Some("old closed".to_string()),
    );
    store.close_session(&old_closed.session_id).unwrap();
    let new_closed = store.start_session(
        Some("agent:test:mixed-restore".to_string()),
        Some("new closed".to_string()),
    );
    store.close_session(&new_closed.session_id).unwrap();
    store.flush_persistence();
    drop(store);

    let restored = SessionStore::with_persistence_limits(&ledger, 1, 1, 10);
    assert!(restored.contains_session(&active.session_id));
    assert_eq!(
        restored.lifecycle_state(&active.session_id),
        Some(SessionLifecycle::Active)
    );
    assert!(
        !restored.contains_session(&old_closed.session_id),
        "restore-time historical retention should prune the oldest Closed row"
    );
    assert!(restored.contains_session(&new_closed.session_id));
    assert_eq!(
        restored.lifecycle_state(&new_closed.session_id),
        Some(SessionLifecycle::Closed)
    );
    let status = restored.status();
    assert_eq!(status.retained_sessions, 2);
    assert_eq!(status.restored_sessions, 2);
    assert_eq!(status.active_sessions, 1);
    assert_eq!(status.closed_sessions, 1);
    assert_eq!(status.hot_sessions, 1);
    assert_eq!(status.cold_sessions, 1);
    assert_eq!(status.hot_session_capacity_target, 1);
    assert_eq!(status.historical_session_retention_limit, 1);
    assert_eq!(status.capacity_evictions, 1);
}

#[test]
fn session_message_create_list_and_resolve_contract() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let posted = store
        .post_message(PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "do the thing".to_string(),
            tags: vec!["work".to_string(), "constraint".to_string()],
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    assert!(posted.message_id.starts_with(MESSAGE_ID_PREFIX));
    assert_eq!(posted.session_id, session.session_id);
    assert_eq!(posted.kind, SessionMessageKind::Todo);
    assert_eq!(posted.status, SessionMessageStatus::Open);
    assert_eq!(posted.priority, SessionMessagePriority::High);
    assert_eq!(posted.message, "do the thing");
    assert_eq!(posted.tags, vec!["work", "constraint"]);

    let listed = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: Some(SessionMessageKind::Todo),
                status: Some(SessionMessageStatus::Open),
                message_id: None,
                reply_to: None,
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

    let idempotent = store
        .resolve_message(&session.session_id, &posted.message_id, None)
        .unwrap();
    assert_eq!(idempotent.status, SessionMessageStatus::Resolved);
    assert_eq!(idempotent.resolved_at, Some(first_resolved_at));
    assert_eq!(idempotent.resolution.as_deref(), Some("shipped"));

    let updated = store
        .resolve_message(
            &session.session_id,
            &posted.message_id,
            Some("still done".to_string()),
        )
        .unwrap();
    assert_eq!(updated.status, SessionMessageStatus::Resolved);
    assert_eq!(updated.resolved_at, Some(first_resolved_at));
    assert_eq!(updated.resolution.as_deref(), Some("still done"));

    let open = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: None,
                status: Some(SessionMessageStatus::Open),
                message_id: None,
                reply_to: None,
                limit: None,
            },
        )
        .unwrap();
    assert!(open.is_empty());
}

#[test]
fn requires_ack_is_kind_priority_independent_and_survives_restore() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("general-ack-session.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session(None, None);
    let question = store
        .post_message_with_ack(
            PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Question,
                message: "which implementation path should we take?".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            },
            true,
        )
        .unwrap();
    assert!(question.requires_ack);

    let hint = store.inbox_hint(&session.session_id).unwrap();
    assert_eq!(hint.attention_required, Some(true));
    assert_eq!(
        hint.attention_reason,
        Some(SESSION_INBOX_ACK_REQUIRED_ATTENTION_REASON)
    );
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let attention = restored.ack_required_messages(&session.session_id, &[]);
    assert_eq!(attention.total_open_requires_ack, 1);
    assert_eq!(attention.messages.len(), 1);
    assert_eq!(attention.messages[0].message_id, question.message_id);
    assert!(attention.messages[0].requires_ack);

    let ack = restored.observe_message_acks(
        &session.session_id,
        std::slice::from_ref(&question.message_id),
    );
    assert_eq!(ack.accepted_ids, vec![question.message_id.clone()]);
    let suppressed = restored.ack_required_messages(&session.session_id, &ack.accepted_ids);
    assert_eq!(suppressed.total_open_requires_ack, 1);
    assert!(suppressed.messages.is_empty());
}

#[test]
fn wrapper_resolution_requires_ack_rejects_todo_and_replays_idempotently() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let guidance = store
        .post_message_with_ack(
            PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Guidance,
                message: "apply the reviewed direction".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();

    let missing_ack = store.resolve_message_from_wrapper(
        &session.session_id,
        &guidance.message_id,
        "handled".to_string(),
        false,
    );
    assert!(matches!(
        missing_ack,
        Err(SessionMessageError::InvalidInput(_))
    ));
    let still_open = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(guidance.message_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(still_open[0].status, SessionMessageStatus::Open);

    let resolved = store
        .resolve_message_from_wrapper(
            &session.session_id,
            &guidance.message_id,
            "handled".to_string(),
            true,
        )
        .unwrap();
    assert_eq!(resolved.status, SessionMessageStatus::Resolved);
    assert_eq!(resolved.resolution.as_deref(), Some("handled"));
    let resolved_at = resolved.resolved_at;

    let replay = store
        .resolve_message_from_wrapper(
            &session.session_id,
            &guidance.message_id,
            "handled".to_string(),
            false,
        )
        .unwrap();
    assert_eq!(replay.resolved_at, resolved_at);
    let conflict = store.resolve_message_from_wrapper(
        &session.session_id,
        &guidance.message_id,
        "different completion".to_string(),
        true,
    );
    assert!(matches!(
        conflict,
        Err(SessionMessageError::IdempotencyConflict)
    ));

    let todo = store
        .post_message(PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "finish the delegated task".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();
    let todo_resolution = store.resolve_message_from_wrapper(
        &session.session_id,
        &todo.message_id,
        "done".to_string(),
        true,
    );
    assert!(matches!(
        todo_resolution,
        Err(SessionMessageError::InvalidInput(_))
    ));
    let retained_todo = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                message_id: Some(todo.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(retained_todo[0].status, SessionMessageStatus::Open);
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
        .guard_denial(
            &normal.session_id,
            session_tool_contract("write_project_file")
        )
        .is_none());
    assert!(store
        .guard_denial(&normal.session_id, session_tool_contract("run_shell"))
        .is_none());

    let write_denial = store
        .guard_denial(
            &read_only.session_id,
            session_tool_contract("write_project_file"),
        )
        .expect("write denied");
    assert_eq!(write_denial.guard, "deny_write_tools");
    assert_eq!(write_denial.mode, SessionMode::ReadOnly);

    let shell_denial = store
        .guard_denial(&read_only.session_id, session_tool_contract("run_shell"))
        .expect("shell denied");
    assert_eq!(shell_denial.guard, "deny_shell_tools");

    // Reads remain allowed under read_only.
    assert!(store
        .guard_denial(&read_only.session_id, session_tool_contract("read_file"))
        .is_none());
}

#[test]
fn validation_job_terminal_projects_typed_runner_lifecycle_without_absorbing_active_or_recovery() {
    let store = SessionStore::default();
    let project = "agent:workflow-session:runner-lifecycle".to_string();
    let session = store.start_session(Some(project.clone()), Some("runner lifecycle".to_string()));
    let target = "target:0123456789abcdef01234567";
    let cases = [
        ("completed", Some(0), Some(true), "succeeded", None),
        (
            "completed",
            Some(0),
            Some(false),
            "failed",
            Some("validation_failed"),
        ),
        (
            "failed",
            Some(1),
            Some(false),
            "failed",
            Some("command_exit_nonzero"),
        ),
        ("timeout", None, Some(false), "failed", Some("timeout")),
        ("timed_out", None, Some(false), "failed", Some("timeout")),
        ("stopped", None, Some(false), "failed", Some("cancelled")),
        ("cancelled", None, Some(false), "failed", Some("cancelled")),
        ("lost", None, Some(false), "failed", Some("execution_lost")),
    ];

    for (index, (job_status, exit_code, validation_passed, status, failure_kind)) in
        cases.into_iter().enumerate()
    {
        let job_id = format!("job-lifecycle-{index}");
        assert!(store.record_validation_job_terminal(
            &session.session_id,
            &job_id,
            &[job_id.as_str()],
            "cargo_check",
            session_tool_contract("cargo_check"),
            Some(project.clone()),
            target,
            None,
            job_status,
            exit_code,
            validation_passed,
            Some(90),
            Some(100 + index as i64),
            Some(10_000),
            None,
        ));
        let summary = store.summary(&session.session_id, None).unwrap();
        let event = summary
            .events
            .iter()
            .rev()
            .find(|event| event.job_id.as_deref() == Some(job_id.as_str()))
            .expect("terminal validation event");
        assert_eq!(event.status.as_deref(), Some(status), "{job_status}");
        assert_eq!(event.failure_kind.as_deref(), failure_kind, "{job_status}");
    }

    for (index, status) in [
        "queued",
        "agent_queued",
        "started",
        "running",
        "stop_requested",
        "recovering",
        "unknown",
    ]
    .into_iter()
    .enumerate()
    {
        let job_id = format!("nonterminal-lifecycle-{index}");
        assert!(
            !store.record_validation_job_terminal(
                &session.session_id,
                &job_id,
                &[job_id.as_str()],
                "cargo_check",
                session_tool_contract("cargo_check"),
                Some(project.clone()),
                target,
                None,
                status,
                None,
                None,
                Some(90),
                Some(200 + index as i64),
                Some(10_000),
                None,
            ),
            "{status} must not materialize terminal validation evidence"
        );
    }
}

fn instruction_observation(
    runner: Option<&str>,
    project: Option<&str>,
    runner_complete: bool,
    project_complete: bool,
    instance: &str,
    generation: u64,
    started_at: std::time::Instant,
) -> webcodex_core::project_instructions::ProjectInstructionsSnapshot {
    use webcodex_core::project_instructions::*;
    let candidate = |scope, path: &str, content: &str| LoadedInstructionCandidate {
        source_scope: scope,
        path: path.into(),
        content: content.into(),
        total_lines: 1,
        full_sha256: None,
    };
    let runner = ProjectInstructionsSnapshot::from_candidates(
        runner
            .map(|body| candidate(InstructionSourceScope::Runner, "runner/0/rules.md", body))
            .into_iter()
            .collect(),
        runner_complete,
    );
    let project = ProjectInstructionsSnapshot::from_candidates(
        project
            .map(|body| candidate(InstructionSourceScope::Project, "AGENTS.md", body))
            .into_iter()
            .collect(),
        project_complete,
    );
    let mut combined =
        ProjectInstructionsSnapshot::with_runner_files(runner.files, project, runner_complete);
    combined.scan.as_mut().unwrap().runner = Some(RunnerInstructionObservation {
        instance_id: instance.into(),
        generation: Some(generation),
        started_at,
        instance_verified_at: started_at,
    });
    combined.scan.as_mut().unwrap().project_started_at = Some(started_at);
    combined
}

fn commit_instruction_observation(
    store: &SessionStore,
    session_id: Option<&str>,
    snapshot: webcodex_core::project_instructions::ProjectInstructionsSnapshot,
) -> CodingSessionOutcome {
    store
        .ensure_coding_session(CodingSessionRequest {
            project: "agent:instructions:demo".into(),
            authority_fingerprint: TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.into(),
            resume_session_id: session_id.map(str::to_string),
            instruction: Some("observe instructions".into()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: Some(snapshot),
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap()
}

#[test]
fn instruction_scopes_refresh_and_remove_independently_without_persisting_bodies() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let at = std::time::Instant::now();
    let observe = |runner, project, rc, pc| {
        instruction_observation(runner, project, rc, pc, "instance", 1, at)
    };
    let first = commit_instruction_observation(
        &store,
        None,
        observe(
            Some("global private body"),
            Some("local private body"),
            true,
            true,
        ),
    );
    let id = first.summary.session_id;
    let resumed = commit_instruction_observation(
        &store,
        Some(&id),
        observe(None, Some("local edited body"), false, true),
    );
    let snapshot = resumed.project_instructions.unwrap();
    assert!(!snapshot.scan_complete);
    assert_eq!(snapshot.files[0].content, "global private body");
    assert_eq!(snapshot.files[1].content, "local edited body");
    assert_eq!(
        resumed.summary.project_instructions.unwrap().files[1].fingerprint,
        snapshot.files[1].fingerprint
    );
    let removed =
        commit_instruction_observation(&store, Some(&id), observe(None, None, true, false));
    let snapshot = removed.project_instructions.unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].content, "local edited body");
    let removed = commit_instruction_observation(
        &store,
        Some(&id),
        observe(Some("global updated body"), None, true, true),
    );
    let snapshot = removed.project_instructions.unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].content, "global updated body");
    // Confirm the converse: a project failure cannot block a global edit.
    let updated = commit_instruction_observation(
        &store,
        Some(&id),
        observe(Some("global newest body"), None, true, false),
    );
    assert_eq!(
        updated.project_instructions.unwrap().files[0].content,
        "global newest body"
    );
    store.flush_persistence();
    let serialized = std::fs::read_to_string(&ledger).unwrap();
    for body in [
        "global private body",
        "local private body",
        "local edited body",
        "global updated body",
        "global newest body",
    ] {
        assert!(!serialized.contains(body));
    }
    assert!(!serialized.contains("project_instructions"));
}

#[test]
fn instruction_observation_cannot_revive_obsolete_generation_or_runner() {
    let store = SessionStore::default();
    let at = std::time::Instant::now();
    let next = at + std::time::Duration::from_secs(1);
    let replacement_at = next + std::time::Duration::from_secs(1);
    let observe = |body, complete, instance, generation, time| {
        instruction_observation(
            body,
            Some("local"),
            complete,
            true,
            instance,
            generation,
            time,
        )
    };
    let first =
        commit_instruction_observation(&store, None, observe(Some("old global"), true, "a", 1, at));
    let id = first.summary.session_id;
    // Failure under a newly observed config retires the old config's body.
    let failed =
        commit_instruction_observation(&store, Some(&id), observe(None, false, "a", 2, next));
    assert!(failed
        .project_instructions
        .unwrap()
        .files
        .iter()
        .all(|file| file.content != "old global"));
    let late = commit_instruction_observation(
        &store,
        Some(&id),
        observe(Some("old global"), true, "a", 1, next),
    );
    assert!(late
        .project_instructions
        .unwrap()
        .files
        .iter()
        .all(|file| file.content != "old global"));
    let replacement = commit_instruction_observation(
        &store,
        Some(&id),
        observe(Some("new runner"), true, "b", 1, replacement_at),
    );
    assert_eq!(
        replacement.project_instructions.unwrap().files[0].content,
        "new runner"
    );
    let late = commit_instruction_observation(
        &store,
        Some(&id),
        observe(Some("old runner"), true, "a", 3, at),
    );
    assert_eq!(
        late.project_instructions.unwrap().files[0].content,
        "new runner"
    );
    // A transient failure from that same instance/config preserves its rule.
    let failed = commit_instruction_observation(
        &store,
        Some(&id),
        observe(None, false, "b", 1, replacement_at),
    );
    assert_eq!(
        failed.project_instructions.unwrap().files[0].content,
        "new runner"
    );
    let replacement_failed = commit_instruction_observation(
        &store,
        Some(&id),
        observe(
            None,
            false,
            "c",
            1,
            replacement_at + std::time::Duration::from_secs(1),
        ),
    );
    assert_eq!(
        replacement_failed.project_instructions.unwrap().files.len(),
        1
    );
}

#[test]
fn newer_instruction_generation_wins_even_when_request_started_first() {
    use webcodex_core::project_instructions::InstructionSourceScope;
    let at = std::time::Instant::now();
    let later = at + std::time::Duration::from_secs(1);
    for complete in [false, true] {
        let store = SessionStore::default();
        let first = commit_instruction_observation(
            &store,
            None,
            instruction_observation(
                Some("revoked"),
                Some("new local"),
                true,
                true,
                "a",
                1,
                later,
            ),
        );
        let id = first.summary.session_id;
        let refreshed = commit_instruction_observation(
            &store,
            Some(&id),
            instruction_observation(
                complete.then_some("generation two"),
                Some("old local"),
                complete,
                true,
                "a",
                2,
                at,
            ),
        )
        .project_instructions
        .unwrap();
        assert_eq!(
            refreshed
                .scan
                .as_ref()
                .unwrap()
                .runner
                .as_ref()
                .unwrap()
                .generation,
            Some(2)
        );
        assert!(!refreshed.scope_complete(InstructionSourceScope::Project));
        assert_eq!(refreshed.files.last().unwrap().content, "new local");
        assert!(refreshed.files.iter().all(|file| file.content != "revoked"));
        assert_eq!(
            refreshed.scope_complete(InstructionSourceScope::Runner),
            complete
        );
        if complete {
            assert_eq!(refreshed.files[0].content, "generation two");
        }
        // A later-started lower generation and an unknown generation cannot revive it.
        for generation in [Some(1), None] {
            let mut late = instruction_observation(
                Some("revoked"),
                Some("new local"),
                true,
                true,
                "a",
                1,
                later,
            );
            late.scan
                .as_mut()
                .unwrap()
                .runner
                .as_mut()
                .unwrap()
                .generation = generation;
            let late = commit_instruction_observation(&store, Some(&id), late)
                .project_instructions
                .unwrap();
            assert!(!late.scope_complete(InstructionSourceScope::Runner));
            assert_eq!(
                late.scan
                    .as_ref()
                    .unwrap()
                    .runner
                    .as_ref()
                    .unwrap()
                    .generation,
                Some(2)
            );
            assert!(late.files.iter().all(|file| file.content != "revoked"));
        }
    }
}

#[test]
fn replacement_instruction_instance_uses_verification_order_not_request_start() {
    use webcodex_core::project_instructions::InstructionSourceScope;
    let at = std::time::Instant::now();
    let later = at + std::time::Duration::from_secs(1);
    let verified = later + std::time::Duration::from_secs(1);
    for complete in [false, true] {
        let store = SessionStore::default();
        let first = commit_instruction_observation(
            &store,
            None,
            instruction_observation(Some("retired"), None, true, true, "a", 9, later),
        );
        let id = first.summary.session_id;
        let mut replacement = instruction_observation(
            complete.then_some("replacement"),
            None,
            complete,
            true,
            "b",
            1,
            at,
        );
        replacement
            .scan
            .as_mut()
            .unwrap()
            .runner
            .as_mut()
            .unwrap()
            .instance_verified_at = verified;
        let replacement = commit_instruction_observation(&store, Some(&id), replacement)
            .project_instructions
            .unwrap();
        assert_eq!(
            replacement
                .scan
                .as_ref()
                .unwrap()
                .runner
                .as_ref()
                .unwrap()
                .instance_id,
            "b"
        );
        assert_eq!(
            replacement.scope_complete(InstructionSourceScope::Runner),
            complete
        );
        assert!(replacement
            .files
            .iter()
            .all(|file| file.content != "retired"));
        // This old-instance request started after B's request, but verified A before B.
        let late = commit_instruction_observation(
            &store,
            Some(&id),
            instruction_observation(Some("retired"), None, true, true, "a", 10, later),
        )
        .project_instructions
        .unwrap();
        assert_eq!(
            late.scan
                .as_ref()
                .unwrap()
                .runner
                .as_ref()
                .unwrap()
                .instance_id,
            "b"
        );
        assert!(!late.scope_complete(InstructionSourceScope::Runner));
        assert!(late.files.iter().all(|file| file.content != "retired"));
        if complete {
            assert_eq!(late.files[0].content, "replacement");
        }
    }
}

#[test]
fn same_instruction_generation_uses_observation_order() {
    let store = SessionStore::default();
    let at = std::time::Instant::now();
    let later = at + std::time::Duration::from_secs(1);
    let first = commit_instruction_observation(
        &store,
        None,
        instruction_observation(Some("new body"), None, true, true, "a", 1, later),
    );
    let mut late = instruction_observation(Some("old body"), None, true, true, "a", 1, at);
    late.scan
        .as_mut()
        .unwrap()
        .runner
        .as_mut()
        .unwrap()
        .instance_verified_at = later;
    let late = commit_instruction_observation(&store, Some(&first.summary.session_id), late)
        .project_instructions
        .unwrap();
    assert!(!late.scan_complete);
    assert_eq!(late.files[0].content, "new body");
}

#[test]
fn resumed_project_observation_has_an_independent_freshness_fence() {
    use webcodex_core::project_instructions::InstructionSourceScope;
    let at = std::time::Instant::now();
    let time = |seconds| at + std::time::Duration::from_secs(seconds);
    // Exercise an unavailable Runner, no Runner capability, and a new generation
    // that must update the global scope without rolling back the local scope.
    for runner_case in 0..3 {
        let store = SessionStore::default();
        let observe = |body, complete, seconds| {
            let mut snapshot = instruction_observation(
                None,
                body,
                runner_case == 1,
                complete,
                "a",
                1,
                time(seconds),
            );
            if runner_case < 2 {
                snapshot.scan.as_mut().unwrap().runner = None;
            }
            snapshot
        };
        let mut delayed = observe(Some("old local"), true, 0);
        let first =
            commit_instruction_observation(&store, None, observe(Some("new local"), true, 1));
        let id = first.summary.session_id;
        if runner_case == 2 {
            delayed = instruction_observation(
                Some("new global"),
                Some("old local"),
                true,
                true,
                "a",
                2,
                at,
            );
        }
        let delayed = commit_instruction_observation(&store, Some(&id), delayed)
            .project_instructions
            .unwrap();
        assert!(!delayed.scan_complete);
        assert!(!delayed.scope_complete(InstructionSourceScope::Project));
        assert_eq!(delayed.files.last().unwrap().content, "new local");
        if runner_case == 2 {
            assert_eq!(delayed.files[0].content, "new global");
            assert!(delayed.scope_complete(InstructionSourceScope::Runner));
        }
        let failed = commit_instruction_observation(
            &store,
            Some(&id),
            observe(Some("partial local"), false, 3),
        );
        assert_eq!(
            failed
                .project_instructions
                .unwrap()
                .files
                .last()
                .unwrap()
                .content,
            "new local"
        );
        // The incomplete refresh must not lose the fence when retaining text.
        let late =
            commit_instruction_observation(&store, Some(&id), observe(Some("late local"), true, 2));
        assert_eq!(
            late.project_instructions
                .unwrap()
                .files
                .last()
                .unwrap()
                .content,
            "new local"
        );
        let recovered = commit_instruction_observation(
            &store,
            Some(&id),
            observe(Some("recovered local"), true, 4),
        )
        .project_instructions
        .unwrap();
        assert!(recovered.scope_complete(InstructionSourceScope::Project));
        assert_eq!(recovered.files.last().unwrap().content, "recovered local");
    }
}

#[test]
fn retained_project_selection_preserves_global_edit_and_only_persists_metadata() {
    use webcodex_core::project_instructions::{InstructionSourceScope, MAX_TOTAL_CHARS};
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let at = std::time::Instant::now();
    let observe = |runner, project, rc, pc, seconds| {
        instruction_observation(
            runner,
            project,
            rc,
            pc,
            "a",
            1,
            at + std::time::Duration::from_secs(seconds),
        )
    };
    let first = commit_instruction_observation(
        &store,
        None,
        observe(
            Some("old global private"),
            Some("short local private"),
            true,
            true,
            0,
        ),
    );
    let id = first.summary.session_id;
    let large = "界".repeat(MAX_TOTAL_CHARS);
    let partial = observe(Some("edited global private"), Some(&large), true, false, 1);
    assert!(partial.files[0].content.is_empty());
    let selected = commit_instruction_observation(&store, Some(&id), partial)
        .project_instructions
        .unwrap();
    assert_eq!(selected.files[0].content, "edited global private");
    assert_eq!(selected.files[1].content, "short local private");
    assert!(!selected.scan_complete);
    // A successful large Project can hide global text; a later local shrink
    // must restore its bounded source even if the new global read fails.
    let expanded = commit_instruction_observation(
        &store,
        Some(&id),
        observe(None, Some(&large), false, true, 2),
    )
    .project_instructions
    .unwrap();
    assert!(expanded.files[0].content.is_empty());
    assert_eq!(expanded.total_chars, MAX_TOTAL_CHARS);
    let recovered = commit_instruction_observation(
        &store,
        Some(&id),
        observe(None, Some("recovered local private"), false, true, 3),
    )
    .project_instructions
    .unwrap();
    assert_eq!(recovered.files[0].content, "edited global private");
    assert_eq!(recovered.files[1].content, "recovered local private");
    assert!(!recovered.files[0].truncated);
    for snapshot in [&selected, &expanded, &recovered] {
        assert!(snapshot.total_chars <= MAX_TOTAL_CHARS);
        assert_eq!(
            snapshot.total_chars,
            snapshot
                .files
                .iter()
                .map(|file| file.content.chars().count())
                .sum::<usize>()
        );
        assert_eq!(
            snapshot.files[0].source_scope,
            InstructionSourceScope::Runner
        );
        assert!(snapshot.files[0].read_more.is_none());
        assert!(
            snapshot
                .scan
                .as_ref()
                .unwrap()
                .runner_source_files
                .iter()
                .map(|file| file.chars)
                .sum::<usize>()
                <= MAX_TOTAL_CHARS
        );
        let public = serde_json::to_value(snapshot).unwrap();
        assert!(public.get("scan").is_none());
        let summary = serde_json::to_string(&snapshot.to_summary()).unwrap();
        for hidden in [
            "private",
            "started_at",
            "instance_verified_at",
            "runner_source_files",
        ] {
            assert!(!summary.contains(hidden));
        }
    }
    // Even an entirely hidden source body cannot leak through snapshot serialization.
    let public = serde_json::to_string(&expanded).unwrap();
    assert!(!public.contains("edited global private"));
    store.flush_persistence();
    let durable = std::fs::read_to_string(&ledger).unwrap();
    for hidden in [
        "private",
        "project_instructions",
        "instance_verified_at",
        "project_started_at",
        "runner_source_files",
    ] {
        assert!(!durable.contains(hidden));
    }
}
