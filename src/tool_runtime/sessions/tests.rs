use super::super::project_instructions::ProjectInstructionsSnapshot;
use super::super::tool_inputs::{ExecutionShell, SessionMode};
use super::events::{
    changed_paths_for_tool, normalize_observed_project_path, observed_paths_for_successful_result,
    session_input_summary_for_tool, SessionToolClassification,
};
use super::model::{
    PersistedCurrentBindings, PersistedSessionLedger, PersistedSessionRecord, SessionLifecycle,
    MAX_OBSERVED_PATHS_PER_EVENT, MAX_VALIDATION_EXCERPT_CHARS, MESSAGE_ID_PREFIX,
    SESSION_ID_PREFIX, SESSION_LEDGER_VERSION,
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
        ("run_process", "job_run"),
        ("run_script", "job_run"),
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
            "apply_text_edits",
            &json!({
                "project": "demo",
                "changes": [
                    {"kind": "edit", "path": "src/lib.rs"},
                    {"kind": "rename", "path": "old.rs", "to_path": "new.rs"}
                ]
            }),
        ),
        vec![
            "src/lib.rs".to_string(),
            "old.rs".to_string(),
            "new.rs".to_string()
        ]
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
fn exploration_paths_are_normalized_and_reject_escape_absolute_and_uri_forms() {
    for (raw, expected) in [
        (
            " src\\tool_runtime\\mod.rs ",
            Some("src/tool_runtime/mod.rs"),
        ),
        ("src/./lib.rs", Some("src/lib.rs")),
        ("", None),
        (".", None),
        ("/etc/passwd", None),
        ("../secret", None),
        ("src/../../secret", None),
        ("C:\\repo\\file.rs", None),
        ("\\\\server\\share\\file.rs", None),
        ("\\repo\\file.rs", None),
        ("file:///root/git/repo/src/lib.rs", None),
        ("https://example.test/source.rs", None),
        ("src/\0secret.rs", None),
        ("src/\nsecret.rs", None),
    ] {
        assert_eq!(
            normalize_observed_project_path(raw).as_deref(),
            expected,
            "{raw:?}"
        );
    }
}

#[test]
fn successful_reads_record_input_paths_but_failed_reads_do_not_finish_with_evidence() {
    let store = SessionStore::new(10, 20);
    let session = store.start_session(None, Some("read evidence".to_string()));

    let successful = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "read_file",
        &json!({"project": "demo", "path": "src\\lib.rs"}),
    );
    store.record_tool_call_finished(
        successful,
        true,
        &json!({"content": "not persisted"}),
        None,
        None,
    );

    let failed = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "read_file",
        &json!({"project": "demo", "path": "src/failed.rs"}),
    );
    store.record_tool_call_finished(
        failed,
        false,
        &json!({"content": "failed body must not become evidence"}),
        Some("read failed"),
        None,
    );

    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    let finished = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished")
        .collect::<Vec<_>>();
    assert_eq!(finished[0].observed_paths, ["src/lib.rs"]);
    assert!(finished[1].observed_paths.is_empty());
    assert!(!serde_json::to_string(&summary)
        .unwrap()
        .contains("not persisted"));
    assert!(!serde_json::to_string(&summary)
        .unwrap()
        .contains("failed body must not become evidence"));
}

#[test]
fn search_result_modes_extract_only_known_structured_file_paths() {
    let cases = [
        (
            "matches",
            json!({
                "matches": [
                    {
                        "path": "src/matches.rs",
                        "line": 7,
                        "preview": "SEARCH_PREVIEW_MUST_NOT_PERSIST",
                        "context_before": ["SEARCH_CONTEXT_BEFORE_MUST_NOT_PERSIST"],
                        "context_after": ["SEARCH_CONTEXT_AFTER_MUST_NOT_PERSIST"]
                    },
                    {"path": "src/shared.rs", "line": 9, "preview": "duplicate"}
                ],
                "unrelated": {"path": "src/not-evidence.rs"}
            }),
            vec!["src/matches.rs", "src/shared.rs"],
        ),
        (
            "files_with_matches",
            json!({
                "files": [
                    {"path": "src/files.rs", "match_count": 4},
                    {"path": "src/shared.rs", "match_count": 1}
                ],
                "matches": []
            }),
            vec!["src/files.rs", "src/shared.rs"],
        ),
        (
            "count",
            json!({
                "files": [
                    {"path": "src/count.rs", "match_count": 8},
                    {"path": "../escape.rs", "match_count": 1}
                ],
                "total_match_count": 9
            }),
            vec!["src/count.rs"],
        ),
    ];

    for (mode, output, expected) in cases {
        let paths =
            observed_paths_for_successful_result("search_project_text", Vec::new(), &output);
        assert_eq!(paths, expected, "{mode}");
    }

    let bounded = observed_paths_for_successful_result(
        "search_project_text",
        Vec::new(),
        &json!({
            "files": (0..MAX_OBSERVED_PATHS_PER_EVENT + 5)
                .map(|index| json!({"path": format!("src/file-{index:03}.rs"), "match_count": 1}))
                .collect::<Vec<_>>()
        }),
    );
    assert_eq!(bounded.len(), MAX_OBSERVED_PATHS_PER_EVENT);
}

#[test]
fn lsp_observations_use_path_metadata_and_known_typed_result_locations_only() {
    let store = SessionStore::new(10, 20);
    let session = store.start_session(None, Some("lsp evidence".to_string()));
    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "goto_definition",
        &json!({
            "project": "demo",
            "path": "src/caller.rs",
            "line": 4,
            "column": 9
        }),
    );
    store.record_tool_call_finished(
        start,
        true,
        &json!({
            "project": "demo",
            "path": "src/caller.rs",
            "query_position": {"line": 4, "column": 9},
            "locations": [
                {
                    "path": "src/definition.rs",
                    "range": {
                        "start": {"line": 1, "column": 1},
                        "end": {"line": 1, "column": 7}
                    }
                },
                {
                    "path": "src/shared.rs",
                    "range": {
                        "start": {"line": 2, "column": 1},
                        "end": {"line": 2, "column": 7}
                    }
                }
            ],
            "total_results": 2,
            "returned_count": 2,
            "truncated": false,
            "external_results_omitted": 0,
            "invalid_results_omitted": 0,
            "arbitrary": {"path": "src/not-evidence.rs"}
        }),
        None,
        None,
    );

    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();
    assert_eq!(
        finished.observed_paths,
        ["src/caller.rs", "src/definition.rs", "src/shared.rs"]
    );
    assert!(!finished
        .observed_paths
        .iter()
        .any(|path| path == "src/not-evidence.rs"));
}

#[test]
fn exploration_input_audit_omits_queries_and_shell_commands() {
    let search = session_input_summary_for_tool(
        "search_project_text",
        &json!({
            "project": "demo",
            "pattern": "RAW_SEARCH_PATTERN wc_pat_PRIVATE_TOKEN",
            "pattern_present": true,
            "path": "src"
        }),
    );
    assert_eq!(search["pattern_present"], true);
    assert!(search.get("pattern").is_none());

    let symbols = session_input_summary_for_tool(
        "workspace_symbols",
        &json!({"project": "demo", "query": "RAW_SYMBOL_QUERY", "limit": 20}),
    );
    assert!(symbols.get("query").is_none());

    let shell = session_input_summary_for_tool(
        "run_shell",
        &json!({
            "project": "demo",
            "command": "printf RAW_SHELL_COMMAND",
            "command_summary": "printf RAW_SHELL_COMMAND",
            "command_present": true
        }),
    );
    assert_eq!(shell["command_present"], true);
    assert!(shell.get("command").is_none());
    assert!(shell.get("command_summary").is_none());

    let process = session_input_summary_for_tool(
        "run_process",
        &json!({
            "project": "demo",
            "executable": "RAW_EXECUTABLE",
            "args": ["RAW_ARG", "secret"],
            "stdin": "RAW_STDIN",
            "process_summary": "RAW_EXECUTABLE RAW_ARG",
            "executable_present": true,
            "arg_count": 2,
            "stdin_present": true
        }),
    );
    assert_eq!(process["executable_present"], true);
    assert_eq!(process["arg_count"], 2);
    assert_eq!(process["stdin_present"], true);
    assert!(process.get("executable").is_none());
    assert!(process.get("args").is_none());
    assert!(process.get("stdin").is_none());
    assert!(process.get("process_summary").is_none());

    let script = session_input_summary_for_tool(
        "run_script",
        &json!({
            "project": "demo",
            "language": "bash",
            "script": "RAW_SCRIPT_BODY",
            "args": ["RAW_ARG", "secret"],
            "stdin": "RAW_STDIN",
            "script_summary": "bash script (15 bytes, 2 args)",
            "script_bytes": 15,
            "arg_count": 2,
            "stdin_present": true
        }),
    );
    assert_eq!(script["language"], "bash");
    assert_eq!(script["script_bytes"], 15);
    assert_eq!(script["arg_count"], 2);
    assert_eq!(script["stdin_present"], true);
    assert!(script.get("script").is_none());
    assert!(script.get("args").is_none());
    assert!(script.get("stdin").is_none());
    assert!(script.get("script_summary").is_none());

    let persistent = session_input_summary_for_tool(
        "session_shell_exec",
        &json!({
            "project": "demo",
            "session_id": "wc_sess_demo",
            "shell_id": "wc_shell_demo",
            "command": "export PRIVATE_TOKEN=secret",
            "command_summary": "export PRIVATE_TOKEN=secret",
            "command_present": true
        }),
    );
    assert_eq!(persistent["command_present"], true);
    assert!(persistent.get("command").is_none());
    assert!(persistent.get("command_summary").is_none());
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
fn write_ledger_atomic_cleans_up_temp_file_when_rename_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger_path = tmp.path().join("sessions.json");
    std::fs::create_dir(&ledger_path).unwrap();
    let ledger = PersistedSessionLedger {
        version: SESSION_LEDGER_VERSION,
        sessions: Vec::new(),
        durable_current_bindings: PersistedCurrentBindings::default(),
    };

    let err = write_ledger_atomic(&ledger_path, &ledger).unwrap_err();
    assert_ne!(err.kind(), std::io::ErrorKind::NotFound);

    let temp_files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".sessions.json.tmp-")
        })
        .collect();
    assert!(
        temp_files.is_empty(),
        "failed write left a temporary ledger"
    );
}

#[test]
fn session_execution_context_persists_and_legacy_ledgers_default_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let expected = SessionExecutionContext {
        default_cwd: Some("frontend/./src".to_string()),
        default_shell: Some(ExecutionShell::Bash),
        resource: None,
    };
    let session = store
        .start_session_with_options(
            SessionCreateOptions::new(
                Some("agent:oe:private-drop".to_string()),
                Some("persistent context".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_execution_context(expected),
        )
        .unwrap();
    assert_eq!(
        session.execution_context,
        SessionExecutionContext {
            default_cwd: Some("frontend/src".to_string()),
            default_shell: Some(ExecutionShell::Bash),
            resource: None,
        }
    );

    store.flush_persistence();
    let raw = std::fs::read_to_string(&ledger).unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        value["sessions"][0]["execution_context"],
        json!({"default_cwd": "frontend/src", "default_shell": "bash"})
    );
    let restored = SessionStore::with_persistence(ledger.clone(), 10, 10);
    assert_eq!(
        restored
            .summary(&session.session_id, None)
            .unwrap()
            .execution_context,
        session.execution_context
    );

    value["sessions"][0]
        .as_object_mut()
        .unwrap()
        .remove("execution_context");
    std::fs::write(&ledger, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let legacy = SessionStore::with_persistence(ledger, 10, 10);
    assert_eq!(
        legacy
            .summary(&session.session_id, None)
            .unwrap()
            .execution_context,
        SessionExecutionContext::default()
    );
}

#[test]
fn session_ssh_resource_and_remote_default_cwd_persist_without_connection_state() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let context = SessionExecutionContext {
        default_cwd: Some("/opt/webcodex-edge".to_string()),
        default_shell: None,
        resource: Some("tmp".to_string()),
    };
    let session = store
        .start_session_with_options(
            SessionCreateOptions::new(
                Some("agent:oe:private-drop".to_string()),
                Some("remote context".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_execution_context(context.clone()),
        )
        .unwrap();
    assert_eq!(session.execution_context, context);
    store.flush_persistence();
    let value: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    assert_eq!(
        value["sessions"][0]["execution_context"],
        json!({"default_cwd": "/opt/webcodex-edge", "resource": "tmp"})
    );
    let restored = SessionStore::with_persistence(ledger, 10, 10);
    assert_eq!(
        restored
            .summary(&session.session_id, None)
            .unwrap()
            .execution_context,
        context
    );
}

#[test]
fn coding_session_context_precommit_failure_leaves_memory_unchanged() {
    fn request(
        resume_session_id: Option<String>,
        execution_context: Option<SessionExecutionContext>,
    ) -> CodingSessionRequest {
        CodingSessionRequest {
            key: None,
            project: "agent:oe:private-drop".to_string(),
            resume_session_id,
            instruction: Some("continue".to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context,
            project_instructions: None,
            transport: SessionTransport::Api,
            bind_current: false,
            new_session: false,
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
fn archived_session_rejects_execution_context_update() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    std::fs::write(
        &ledger,
        serde_json::to_vec_pretty(&json!({
            "version": SESSION_LEDGER_VERSION,
            "sessions": [{
                "session_id": "wc_sess_archivedcontext01",
                "project": "agent:oe:private-drop",
                "title": "archived",
                "mode": "normal",
                "guards": {
                    "deny_write_tools": false,
                    "deny_shell_tools": false
                },
                "execution_context": {"default_cwd": "frontend"},
                "lifecycle": "archived",
                "created_at": 1,
                "updated_at": 1,
                "events": [],
                "messages": []
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let store = SessionStore::with_persistence(ledger, 10, 10);
    assert_eq!(
        store
            .update_execution_context(
                "wc_sess_archivedcontext01",
                SessionExecutionContext::default(),
                SessionTransport::Api,
            )
            .unwrap_err(),
        SessionExecutionContextUpdateError::SessionNotActive {
            lifecycle: SessionLifecycle::Archived
        }
    );
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

    let restored = flush_and_restore(&store, ledger);
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

    let restored = flush_and_restore(&store, ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.events.len(), 2);
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.git_like, 1);
    assert_eq!(summary.events[1].tool_name, "git_log");
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
fn exploration_ledger_persists_only_bounded_relative_paths_and_safe_metadata() {
    const PRIVATE_ROOT: &str = "/root/git/private-drop";
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 30);
    let session = store.start_session(None, Some("private exploration".to_string()));

    let search = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "search_project_text",
        &json!({
            "project": "demo",
            "pattern": "RAW_SEARCH_PATTERN wc_pat_PRIVATE_TOKEN",
            "pattern_present": true,
            "context_before": 2,
            "context_after": 2
        }),
    );
    store.record_tool_call_finished(
        search,
        true,
        &json!({
            "matches": [{
                "path": "src/search.rs",
                "line": 3,
                "preview": "RAW_SEARCH_PREVIEW Authorization: Bearer PRIVATE_SECRET",
                "context_before": ["RAW_CONTEXT_BEFORE"],
                "context_after": ["RAW_CONTEXT_AFTER"]
            }, {
                "path": format!("{PRIVATE_ROOT}/src/absolute.rs"),
                "line": 4,
                "preview": "ABSOLUTE_PATH_PREVIEW"
            }]
        }),
        None,
        None,
    );

    let hover = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "hover",
        &json!({
            "project": "demo",
            "path": "src/hover.rs",
            "line": 1,
            "column": 1
        }),
    );
    store.record_tool_call_finished(
        hover,
        true,
        &json!({
            "project": "demo",
            "path": "src/hover.rs",
            "position": {"line": 1, "column": 1},
            "hover": {"kind": "markdown", "value": "RAW_HOVER_BODY"},
            "truncated": false,
            "range_omitted": false
        }),
        None,
        None,
    );

    let symbols = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "workspace_symbols",
        &json!({
            "project": "demo",
            "query": "RAW_SYMBOL_QUERY",
            "query_present": true,
            "limit": 10
        }),
    );
    store.record_tool_call_finished(
        symbols,
        true,
        &json!({
            "project": "demo",
            "query": "RAW_SYMBOL_QUERY",
            "symbols": [{
                "name": "RAW_SYMBOL_NAME",
                "kind": "struct",
                "kind_code": 23,
                "container_name": null,
                "path": "src/symbol.rs",
                "range": null
            }],
            "total_results": 1,
            "returned_count": 1,
            "truncated": false,
            "external_results_omitted": 0,
            "invalid_results_omitted": 0
        }),
        None,
        None,
    );

    let diagnostics = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "document_diagnostics",
        &json!({"project": "demo", "path": "src/diagnostics.rs", "limit": 10}),
    );
    store.record_tool_call_finished(
        diagnostics,
        true,
        &json!({
            "project": "demo",
            "path": "src/diagnostics.rs",
            "language": "rust",
            "diagnostics": [{
                "range": {
                    "start": {"line": 1, "column": 1},
                    "end": {"line": 1, "column": 2}
                },
                "severity": "warning",
                "severity_code": 2,
                "code": "unused",
                "source": "rust-analyzer",
                "message": "RAW_DIAGNOSTIC_BODY",
                "tags": []
            }],
            "total_count": 1,
            "returned_count": 1,
            "truncated": false,
            "status": "complete",
            "clean": false,
            "published_version": 1,
            "invalid_results_omitted": 0,
            "related_information_omitted": 0
        }),
        None,
        None,
    );

    for (tool, arguments, output) in [
        (
            "list_project_files",
            json!({"project": "demo", "path": "src"}),
            json!({"entries": [{"path": "src/listed.rs", "kind": "file"}]}),
        ),
        (
            "run_shell",
            json!({
                "project": "demo",
                "command": "printf RAW_SHELL_COMMAND",
                "command_summary": "printf RAW_SHELL_COMMAND",
                "command_present": true
            }),
            json!({
                "stdout": "RAW_SHELL_STDOUT",
                "stderr": "RAW_SHELL_STDERR",
                "path": "src/from-shell.rs"
            }),
        ),
    ] {
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            tool,
            &arguments,
        );
        store.record_tool_call_finished(start, true, &output, None, None);
    }

    store.flush_persistence();
    let serialized = std::fs::read_to_string(&ledger).unwrap();
    for forbidden in [
        "RAW_SEARCH_PATTERN",
        "wc_pat_PRIVATE_TOKEN",
        "RAW_SEARCH_PREVIEW",
        "Authorization: Bearer PRIVATE_SECRET",
        "RAW_CONTEXT_BEFORE",
        "RAW_CONTEXT_AFTER",
        "RAW_HOVER_BODY",
        "RAW_SYMBOL_QUERY",
        "RAW_SYMBOL_NAME",
        "RAW_DIAGNOSTIC_BODY",
        "RAW_SHELL_COMMAND",
        "RAW_SHELL_STDOUT",
        "RAW_SHELL_STDERR",
        "src/listed.rs",
        "src/from-shell.rs",
        PRIVATE_ROOT,
    ] {
        assert!(
            !serialized.contains(forbidden),
            "ledger leaked {forbidden}: {serialized}"
        );
    }
    assert!(serialized.contains("src/search.rs"));
    assert!(serialized.contains("src/hover.rs"));
    assert!(serialized.contains("src/symbol.rs"));
    assert!(serialized.contains("src/diagnostics.rs"));

    let restored = SessionStore::with_persistence(ledger, 10, 30);
    let summary = restored.summary(&session.session_id, Some(30)).unwrap();
    let successful_observations = summary
        .events
        .iter()
        .filter(|event| {
            event.kind == "tool_call_finished"
                && event.status.as_deref() == Some("succeeded")
                && !event.observed_paths.is_empty()
        })
        .flat_map(|event| event.observed_paths.iter().map(String::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        successful_observations,
        [
            "src/search.rs",
            "src/hover.rs",
            "src/symbol.rs",
            "src/diagnostics.rs"
        ]
    );
}

#[test]
fn search_project_texts_nested_patterns_are_removed_from_session_input_summary() {
    let summary = super::events::session_input_summary_for_tool(
        "search_project_texts",
        &json!({
            "project": "agent:oe:demo",
            "queries": [
                {
                    "pattern": "RAW_BATCH_PATTERN_ALPHA wc_pat_PRIVATE_TOKEN",
                    "path": "src",
                    "result_mode": "matches"
                },
                {
                    "pattern": "RAW_BATCH_PATTERN_BETA Authorization: Bearer PRIVATE",
                    "path": "tests",
                    "result_mode": "files_with_matches"
                }
            ]
        }),
    );
    assert_eq!(summary["queries"][0]["path"], "src");
    assert_eq!(summary["queries"][1]["path"], "tests");
    assert!(summary["queries"][0].get("pattern").is_none());
    assert!(summary["queries"][1].get("pattern").is_none());
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains("RAW_BATCH_PATTERN_ALPHA"));
    assert!(!serialized.contains("RAW_BATCH_PATTERN_BETA"));
    assert!(!serialized.contains("PRIVATE_TOKEN"));
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
fn persistence_snapshot_shares_payload_and_stays_stable_across_message_cow() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("shared snapshot".to_string()));
    assert!(store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({
                "project": "demo",
                "path": "src/lib.rs",
                "query": "snapshot payload"
            }),
        )
        .is_some());
    let message = post_message(
        &store,
        &session.session_id,
        SessionMessageKind::Todo,
        "snapshot message payload",
    );
    store.flush_persistence();

    let (snapshot_ready_tx, snapshot_ready_rx) = std::sync::mpsc::channel();
    let (allow_old_write_tx, allow_old_write_rx) = std::sync::mpsc::channel();
    let snapshot_session_id = session.session_id.clone();
    let delayed_store = store.clone();
    let delayed_write = std::thread::spawn(move || {
        delayed_store.persist_after_mutation_with(|path, ledger| {
            let snapshot = ledger
                .sessions
                .iter()
                .find(|record| record.session_id == snapshot_session_id)
                .unwrap();
            let event = snapshot.events.last().unwrap();
            let message = snapshot.messages.last().unwrap();
            assert_eq!(std::sync::Arc::strong_count(event), 2);
            assert_eq!(std::sync::Arc::strong_count(message), 2);
            assert_eq!(message.status, SessionMessageStatus::Open);

            snapshot_ready_tx.send(()).unwrap();
            allow_old_write_rx.recv().unwrap();

            assert_eq!(std::sync::Arc::strong_count(message), 1);
            assert_eq!(message.status, SessionMessageStatus::Open);
            assert_eq!(message.resolution, None);
            write_ledger_atomic(path, ledger)
        });
    });
    snapshot_ready_rx.recv().unwrap();

    let resolved = store
        .resolve_message(
            &session.session_id,
            &message.message_id,
            Some("resolved after snapshot".to_string()),
        )
        .unwrap();
    assert_eq!(resolved.status, SessionMessageStatus::Resolved);
    assert_eq!(
        resolved.resolution.as_deref(),
        Some("resolved after snapshot")
    );

    allow_old_write_tx.send(()).unwrap();
    delayed_write.join().unwrap();

    let restored = flush_and_restore(&store, ledger);
    let messages = restored
        .list_messages(&session.session_id, ListSessionMessagesFilter::default())
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].status, SessionMessageStatus::Resolved);
    assert_eq!(
        messages[0].resolution.as_deref(),
        Some("resolved after snapshot")
    );
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

    let restored = flush_and_restore(&store, ledger);
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
    let session = store
        .start_session_with_options(
            SessionCreateOptions::new(
                Some("agent:oe:private-drop".to_string()),
                Some("instructions".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_project_instructions(Some(
                ProjectInstructionsSnapshot::from_single_file(
                    "AGENTS.md",
                    secret_body.to_string(),
                    1,
                ),
            )),
        )
        .unwrap();

    store.flush_persistence();
    let serialized = std::fs::read_to_string(&ledger).unwrap();
    assert!(!serialized.contains(secret_body));
    assert!(!serialized.contains("project_instructions"));
    let restored = flush_and_restore(&store, ledger);
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
        window_key: "window-1".to_string(),
        resolved_project: project.to_string(),
        repository_root_key: format!("root:{project}"),
    }
}

#[test]
fn durable_current_binding_key_is_exact_domain_separated_sha256() {
    let base = test_binding_key("proj");
    let digest = base.durable_binding_key();
    assert_eq!(digest.len(), 64);
    assert!(digest
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)));

    let variants = [
        CurrentSessionKey {
            principal_kind: "other-kind".to_string(),
            ..base.clone()
        },
        CurrentSessionKey {
            principal_id: "other-principal".to_string(),
            ..base.clone()
        },
        CurrentSessionKey {
            transport: SessionTransport::Mcp.as_str().to_string(),
            ..base.clone()
        },
        CurrentSessionKey {
            window_key: "other-window".to_string(),
            ..base.clone()
        },
        CurrentSessionKey {
            resolved_project: "other-project".to_string(),
            ..base.clone()
        },
        CurrentSessionKey {
            repository_root_key: "other-root".to_string(),
            ..base.clone()
        },
    ];
    for variant in variants {
        assert_ne!(variant.durable_binding_key(), digest);
    }
}

#[test]
fn durable_current_binding_explicit_bind_restores_without_raw_key_material() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(Some("proj".to_string()), None);
    let key = test_binding_key("proj");

    store
        .bind_current_session(key.clone(), &session.session_id)
        .expect("bind known active session");
    assert_eq!(store.status().durable_binding_count, 1);
    store.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(persisted.contains(&key.durable_binding_key()));
    assert!(!persisted.contains(&key.principal_id));
    assert!(!persisted.contains(&key.window_key));
    assert!(!persisted.contains(&key.repository_root_key));

    drop(store);
    let restored = persistent_store(ledger);
    let status = restored.status();
    assert_eq!(status.restored_binding_count, 1);
    assert_eq!(status.durable_binding_count, 1);
    assert_eq!(status.discarded_binding_count, 0);
    assert_eq!(restored.process_local_binding_count_for_test(), 0);
    assert_eq!(
        restored.current_session_id(&key).as_deref(),
        Some(session.session_id.as_str())
    );
    assert_eq!(restored.process_local_binding_count_for_test(), 1);
}

#[test]
fn durable_current_binding_unbind_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(Some("proj".to_string()), None);
    let key = test_binding_key("proj");
    store
        .bind_current_session(key.clone(), &session.session_id)
        .unwrap();

    assert!(store.unbind_current_session(&key));
    assert_eq!(store.status().durable_binding_count, 0);
    let restored = flush_and_restore(&store, ledger);
    assert!(restored.current_session(&key).is_none());
    assert_eq!(restored.status().restored_binding_count, 0);
}

#[test]
fn durable_current_binding_close_removes_all_session_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(Some("proj".to_string()), None);
    let first_key = test_binding_key("proj");
    let mut second_key = first_key.clone();
    second_key.window_key = "window-2".to_string();
    store
        .bind_current_session(first_key.clone(), &session.session_id)
        .unwrap();
    store
        .bind_current_session(second_key.clone(), &session.session_id)
        .unwrap();
    assert_eq!(store.status().durable_binding_count, 2);

    store.close_session(&session.session_id).unwrap();
    assert_eq!(store.status().durable_binding_count, 0);
    assert_eq!(store.process_local_binding_count_for_test(), 0);
    let restored = flush_and_restore(&store, ledger);
    assert!(restored.current_session(&first_key).is_none());
    assert!(restored.current_session(&second_key).is_none());
    assert_eq!(
        restored.lifecycle_state(&session.session_id),
        Some(SessionLifecycle::Closed)
    );
}

#[test]
fn durable_current_binding_eviction_removes_stale_target() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 1, 10);
    let first = store.start_session(Some("proj".to_string()), None);
    let key = test_binding_key("proj");
    store
        .bind_current_session(key.clone(), &first.session_id)
        .unwrap();

    let second = store.start_session(Some("proj".to_string()), None);
    assert!(!store.contains_session(&first.session_id));
    assert!(store.contains_session(&second.session_id));
    assert_eq!(store.status().durable_binding_count, 0);

    store.flush_persistence();
    let restored = SessionStore::with_persistence(ledger, 1, 10);
    assert!(restored.current_session(&key).is_none());
    assert_eq!(restored.status().restored_binding_count, 0);
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
    let inspect = store.start_session_with_guards(
        None,
        None,
        SessionMode::Inspect,
        SessionGuards {
            deny_write_tools: false,
            deny_shell_tools: true,
        },
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
    assert_eq!(
        store
            .guard_denial(&inspect.session_id, "write_project_file")
            .expect("inspect write denied")
            .guard,
        "deny_write_tools"
    );
    assert!(store
        .guard_denial(&inspect.session_id, "run_shell")
        .is_none());
}

#[test]
fn ledger_round_trip_preserves_session_events_and_messages() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 50);
    let session = store.start_session_with_guards(
        Some("proj".to_string()),
        Some("persist".to_string()),
        SessionMode::Inspect,
        SessionGuards::default(),
    );
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

    store.flush_persistence();
    let restored = SessionStore::with_persistence(&ledger, 10, 50);
    let summary = restored.summary(&session.session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some("proj"));
    assert_eq!(summary.title.as_deref(), Some("persist"));
    assert_eq!(summary.mode, SessionMode::Inspect);
    assert!(summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
    assert_eq!(summary.counts.tool_calls, 1);
    assert!(summary
        .events
        .iter()
        .any(|event| event.kind == "tool_call_finished"));
    assert_eq!(summary.messages.total, 1);
    assert_eq!(summary.messages.progress, 1);

    // This session was never bound, so the additive ledger field stays empty.
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

    store.flush_persistence();
    let raw = std::fs::read_to_string(&ledger).unwrap();
    let ledger_value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(ledger_value["version"], SESSION_LEDGER_VERSION);
    assert_eq!(ledger_value["sessions"][0]["lifecycle"], "active");
    assert_eq!(
        ledger_value["sessions"][0]["session_id"].as_str().unwrap(),
        session.session_id
    );

    let restored = flush_and_restore(&store, ledger);
    let summary = restored.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Active);
}

#[test]
fn durable_current_binding_legacy_ledger_without_bindings_loads_session_as_active() {
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
    assert_eq!(status.durable_binding_count, 0);
    assert_eq!(status.restored_binding_count, 0);
    assert_eq!(status.discarded_binding_count, 0);
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
fn durable_current_binding_corruption_is_discarded_without_losing_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger_path = tmp.path().join("sessions.json");
    let store = persistent_store(ledger_path.clone());
    let active = store.start_session(Some("proj-a".to_string()), Some("active".to_string()));
    let other_project = store.start_session(Some("proj-b".to_string()), Some("other".to_string()));
    let closed = store.start_session(Some("proj-a".to_string()), Some("closed".to_string()));
    store.close_session(&closed.session_id).unwrap();
    let valid_key = test_binding_key("proj-a");
    store
        .bind_current_session(valid_key.clone(), &active.session_id)
        .unwrap();
    store.flush_persistence();
    drop(store);

    let mut ledger: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger_path).unwrap()).unwrap();
    let bindings = ledger["durable_current_bindings"]
        .as_array_mut()
        .expect("durable binding array");
    let original = bindings[0].clone();
    let original_updated_at = original["updated_at"].as_i64().unwrap();
    bindings.push(json!({
        "binding_key_sha256": "not-a-sha256",
        "session_id": active.session_id,
        "updated_at": original_updated_at,
    }));
    bindings.push(json!({
        "binding_key_sha256": "a".repeat(64),
        "session_id": "not-a-workflow-session",
        "updated_at": original_updated_at,
    }));
    bindings.push(json!({
        "binding_key_sha256": "b".repeat(64),
        "session_id": "wc_sess_missing",
        "updated_at": original_updated_at,
    }));
    bindings.push(json!({
        "binding_key_sha256": "c".repeat(64),
        "session_id": closed.session_id,
        "updated_at": original_updated_at,
    }));
    bindings.push(json!({
        "binding_key_sha256": 7,
        "session_id": active.session_id,
        "updated_at": original_updated_at,
    }));
    let mut duplicate = original;
    duplicate["updated_at"] = json!(original_updated_at.saturating_add(1));
    bindings.push(duplicate);
    let mut mismatched_key = valid_key.clone();
    mismatched_key.window_key = "mismatched-project-window".to_string();
    bindings.push(json!({
        "binding_key_sha256": mismatched_key.durable_binding_key(),
        "session_id": other_project.session_id,
        "updated_at": original_updated_at,
    }));
    std::fs::write(&ledger_path, serde_json::to_vec_pretty(&ledger).unwrap()).unwrap();

    let restored = persistent_store(ledger_path);
    let status = restored.status();
    assert_eq!(status.restored_sessions, 3);
    assert_eq!(status.restored_binding_count, 2);
    assert_eq!(status.durable_binding_count, 2);
    assert_eq!(status.discarded_binding_count, 6);
    assert_eq!(status.last_persist_error, None);
    assert!(restored.summary(&active.session_id, None).is_some());
    assert!(restored.summary(&other_project.session_id, None).is_some());
    assert_eq!(
        restored.lifecycle_state(&closed.session_id),
        Some(SessionLifecycle::Closed)
    );

    // The composite key resolves project A, so a corrupt reference to an
    // otherwise active project-B Session is rejected lazily without exposing
    // the binding digest.
    assert!(restored.current_session(&mismatched_key).is_none());
    let after_lookup = restored.status();
    assert_eq!(after_lookup.durable_binding_count, 1);
    assert_eq!(after_lookup.discarded_binding_count, 7);
    let status_json = serde_json::to_string(&after_lookup).unwrap();
    assert!(!status_json.contains(&mismatched_key.durable_binding_key()));
    assert_eq!(
        restored.current_session_id(&valid_key).as_deref(),
        Some(active.session_id.as_str())
    );
}

#[test]
fn durable_current_binding_restore_enforces_bounded_count() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger_path = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger_path, 1, 10);
    let session = store.start_session(Some("proj".to_string()), None);
    store.flush_persistence();
    drop(store);

    let mut ledger: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger_path).unwrap()).unwrap();
    ledger["durable_current_bindings"] = Value::Array(
        (0..12)
            .map(|index| {
                json!({
                    "binding_key_sha256": format!("{index:064x}"),
                    "session_id": session.session_id,
                    "updated_at": index,
                })
            })
            .collect(),
    );
    std::fs::write(&ledger_path, serde_json::to_vec_pretty(&ledger).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(ledger_path, 1, 10);
    let status = restored.status();
    assert_eq!(status.max_durable_bindings, 8);
    assert_eq!(status.restored_binding_count, 8);
    assert_eq!(status.durable_binding_count, 8);
    assert_eq!(status.discarded_binding_count, 4);
    assert!(restored.summary(&session.session_id, None).is_some());
}

#[test]
fn lifecycle_ledger_round_trip_preserves_active() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(None, Some("round trip".to_string()));
    assert_eq!(session.lifecycle, SessionLifecycle::Active);

    let restored = flush_and_restore(&store, ledger);
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
    assert!(ledger.durable_current_bindings.records.is_empty());
    assert_eq!(ledger.durable_current_bindings.malformed_count, 0);
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

// --- Workflow session lifecycle (Phase 2: explicit close) ---

#[test]
fn active_to_closed_succeeds_and_emits_session_closed_event() {
    let store = SessionStore::default();
    let session = store.start_session(None, Some("close me".to_string()));
    assert_eq!(session.lifecycle, SessionLifecycle::Active);

    let outcome = store.close_session(&session.session_id).unwrap();
    assert!(!outcome.already_closed);
    assert_eq!(outcome.summary.lifecycle, SessionLifecycle::Closed);
    assert_eq!(outcome.summary.session_id, session.session_id);

    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Closed);
    let closed_events: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "session_closed")
        .collect();
    assert_eq!(closed_events.len(), 1);
    assert_eq!(closed_events[0].tool_name, "close_session");
    assert_eq!(closed_events[0].status.as_deref(), Some("succeeded"));
}

#[test]
fn closed_lifecycle_persists_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = persistent_store(ledger.clone());
    let session = store.start_session(Some("proj".to_string()), Some("close persist".to_string()));
    store.close_session(&session.session_id).unwrap();

    store.flush_persistence();
    let raw = std::fs::read_to_string(&ledger).unwrap();
    let ledger_value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(ledger_value["version"], SESSION_LEDGER_VERSION);
    assert_eq!(ledger_value["sessions"][0]["lifecycle"], "closed");

    let restored = flush_and_restore(&store, ledger);
    let summary = restored.summary(&session.session_id, Some(20)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Closed);
    assert!(summary
        .events
        .iter()
        .any(|event| event.kind == "session_closed"));
}

#[test]
fn closed_session_denies_mutation_tools_allows_query() {
    let store = SessionStore::default();
    let session = store.start_session(None, Some("closed query".to_string()));
    store.close_session(&session.session_id).unwrap();

    // Write / shell blocked.
    let write_denial = store
        .lifecycle_denial(&session.session_id, "write_project_file")
        .expect("write denied on closed");
    assert_eq!(write_denial.lifecycle, SessionLifecycle::Closed);
    let shell_denial = store
        .lifecycle_denial(&session.session_id, "run_shell")
        .expect("shell denied on closed");
    assert_eq!(shell_denial.lifecycle, SessionLifecycle::Closed);
    assert!(store
        .lifecycle_denial(&session.session_id, "post_session_message")
        .is_some());
    assert!(store
        .lifecycle_denial(&session.session_id, "workspace_checkpoint_create")
        .is_some());

    // Query / pure read still allowed; close remains idempotent path.
    assert!(store
        .lifecycle_denial(&session.session_id, "read_file")
        .is_none());
    assert!(store
        .lifecycle_denial(&session.session_id, "session_summary")
        .is_none());
    assert!(store
        .lifecycle_denial(&session.session_id, "close_session")
        .is_none());

    // Message board mutations fail with SessionClosed; list still works.
    let post = store.post_message(PostSessionMessageInput {
        session_id: session.session_id.clone(),
        kind: SessionMessageKind::Note,
        message: "after close".to_string(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    });
    assert!(matches!(
        post,
        Err(SessionMessageError::SessionClosed {
            lifecycle: SessionLifecycle::Closed
        })
    ));
    let listed = store
        .list_messages(
            &session.session_id,
            ListSessionMessagesFilter {
                kind: None,
                status: None,
                limit: None,
            },
        )
        .unwrap();
    assert!(listed.is_empty());

    let summary = store.summary(&session.session_id, Some(10)).unwrap();
    assert_eq!(summary.lifecycle, SessionLifecycle::Closed);
    assert_eq!(summary.session_id, session.session_id);
}

#[test]
fn repeated_close_is_idempotent_without_duplicate_events() {
    let store = SessionStore::default();
    let session = store.start_session(None, Some("idempotent".to_string()));
    let first = store.close_session(&session.session_id).unwrap();
    assert!(!first.already_closed);
    let second = store.close_session(&session.session_id).unwrap();
    assert!(second.already_closed);
    assert_eq!(second.summary.lifecycle, SessionLifecycle::Closed);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let closed_count = summary
        .events
        .iter()
        .filter(|event| event.kind == "session_closed")
        .count();
    assert_eq!(
        closed_count, 1,
        "repeat close must not append another event"
    );
}

#[test]
fn unknown_session_close_fails_without_create() {
    let store = SessionStore::default();
    let missing = "wc_sess_missingclose01";
    let err = store.close_session(missing).unwrap_err();
    assert_eq!(err, SessionCloseError::UnknownSession);
    assert!(!store.contains_session(missing));
    assert!(store.summary(missing, None).is_none());
}

#[test]
fn eviction_does_not_produce_closed_lifecycle() {
    // Capacity eviction removes the record; it is not a Closed transition.
    let store = SessionStore::new(1, 10);
    let first = store.start_session(None, Some("evict me".to_string()));
    let _second = store.start_session(None, Some("survivor".to_string()));
    assert!(!store.contains_session(&first.session_id));
    assert!(store.summary(&first.session_id, None).is_none());
    // Evicted id is unknown, not Closed — close must not invent a session.
    assert_eq!(
        store.close_session(&first.session_id).unwrap_err(),
        SessionCloseError::UnknownSession
    );
    assert!(!store.contains_session(&first.session_id));
}

#[test]
fn closed_session_does_not_reopen() {
    let store = SessionStore::default();
    let session = store.start_session(None, Some("no reopen".to_string()));
    store.close_session(&session.session_id).unwrap();
    // Only path that could "reopen" would be inventing Active; close stays Closed.
    let again = store.close_session(&session.session_id).unwrap();
    assert!(again.already_closed);
    assert_eq!(again.summary.lifecycle, SessionLifecycle::Closed);
    assert!(!again.summary.lifecycle.allows_mutation());
}
