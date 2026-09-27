use crate::model::MAX_INPUT_ARRAY_ITEMS;
use crate::*;
use serde_json::{json, Value};

fn session_tool_contract(tool_name: &str) -> SessionToolContract {
    let write_like = matches!(tool_name, "edit_project_files" | "run_process");
    SessionToolContract {
        risk_class: if write_like { "write" } else { "read" },
        read_like: !write_like,
        write_like,
        shell_like: tool_name == "run_process",
        git_like: false,
        change_summary_like: false,
        project_write: tool_name == "edit_project_files",
        path_hint: SessionPathHint::None,
    }
}

fn project_edit_contract(path_hint: SessionPathHint) -> SessionToolContract {
    SessionToolContract {
        risk_class: "write",
        read_like: false,
        write_like: true,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        project_write: true,
        path_hint,
    }
}

fn record_result(
    store: &SessionStore,
    session_id: &str,
    tool_name: &str,
    success: bool,
    output: Value,
) -> String {
    let arguments = json!({"project": "proj"});
    let start = store
        .record_tool_call_started_with_metadata(
            Some(session_id),
            SessionTransport::Mcp,
            tool_name,
            &arguments,
            Some("proj".to_string()),
            ToolCallRecorderMetadata::default(),
            session_tool_contract(tool_name),
        )
        .expect("recorded call start");
    store
        .record_tool_call_finished(
            Some(start),
            success,
            &output,
            (!success).then_some("business failure"),
            (!success).then_some("business_failure"),
        )
        .expect("recorded tool result")
}

#[test]
fn dry_run_project_edits_do_not_record_session_changed_paths() {
    let store = SessionStore::new(10, 100);
    let session = store.start_session(Some("proj".to_string()), Some("dry run paths".to_string()));

    let dry_text_start = store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Mcp,
            "edit_project_files",
            &json!({
                "project": "proj",
                "dry_run": true,
                "changes": [{"kind": "create", "path": "src/would_only.rs", "content": "x"}]
            }),
            project_edit_contract(SessionPathHint::PathList),
        )
        .expect("dry-run apply_text_edits start");
    assert!(dry_text_start.changed_paths.is_empty());
    store
        .record_tool_call_finished(
            Some(dry_text_start),
            true,
            &json!({
                "dry_run": true,
                "state_changed": false,
                "changed_paths": ["src/would_only.rs"]
            }),
            None,
            None,
        )
        .expect("dry-run apply_text_edits finish");

    let dry_patch_start = store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Mcp,
            "apply_patch",
            &json!({"project": "proj", "dry_run": true, "patch": "*** Begin Patch\n*** End Patch"}),
            project_edit_contract(SessionPathHint::Patch),
        )
        .expect("dry-run apply_patch start");
    store
        .record_tool_call_finished(
            Some(dry_patch_start),
            true,
            &json!({
                "dry_run": true,
                "state_changed": false,
                "changed_paths": ["src/would_patch.rs"]
            }),
            None,
            None,
        )
        .expect("dry-run apply_patch finish");

    let live_start = store
        .record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Mcp,
            "edit_project_files",
            &json!({
                "project": "proj",
                "dry_run": false,
                "changes": [{"kind": "create", "path": "src/live.rs", "content": "x"}]
            }),
            project_edit_contract(SessionPathHint::PathList),
        )
        .expect("live apply_text_edits start");
    assert_eq!(live_start.changed_paths, vec!["src/live.rs"]);
    store
        .record_tool_call_finished(
            Some(live_start),
            true,
            &json!({"dry_run": false, "state_changed": true}),
            None,
            None,
        )
        .expect("live apply_text_edits finish");

    let summary = store
        .summary(&session.session_id, Some(100))
        .expect("session summary");
    let finished = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished")
        .collect::<Vec<_>>();
    assert_eq!(finished.len(), 3);
    assert!(finished[0].changed_paths.is_empty());
    assert!(finished[1].changed_paths.is_empty());
    assert_eq!(finished[2].changed_paths, vec!["src/live.rs"]);
    assert!(summary.repository_edit_observed);
}

#[test]
fn retained_changed_path_evidence_uses_proven_effects_and_fails_closed_at_durable_bound() {
    let store = SessionStore::new(10, 100);
    let session = store.start_session(Some("proj".to_string()), Some("path evidence".to_string()));
    let contract = project_edit_contract(SessionPathHint::PathList);

    let record = |paths: Vec<String>, success: bool, state_changed: bool| {
        let changes = paths
            .iter()
            .map(|path| json!({"kind": "create", "path": path, "content": "x"}))
            .collect::<Vec<_>>();
        let start = store
            .record_tool_call_started(
                Some(&session.session_id),
                SessionTransport::Mcp,
                "edit_project_files",
                &json!({"project": "proj", "changes": changes}),
                contract,
            )
            .expect("tool start");
        store
            .record_tool_call_finished(
                Some(start),
                success,
                &json!({"state_changed": state_changed}),
                (!success).then_some("failed"),
                None,
            )
            .expect("tool finish");
    };

    record(vec!["src/noop.rs".to_string()], true, false);
    record(vec!["src/failed.rs".to_string()], false, false);
    record(vec!["src/live.rs".to_string()], true, true);
    let (paths, complete) = store
        .retained_changed_path_evidence(&session.session_id)
        .unwrap();
    assert_eq!(paths, vec!["src/live.rs"]);
    assert!(complete);

    record(
        (0..MAX_INPUT_ARRAY_ITEMS)
            .map(|index| format!("src/bound-{index}.rs"))
            .collect(),
        true,
        true,
    );
    let (_, complete) = store
        .retained_changed_path_evidence(&session.session_id)
        .unwrap();
    assert!(
        !complete,
        "hitting the durable path bound cannot certify completeness"
    );
}

#[test]
fn repository_edit_observed_is_canonical_sticky_and_survives_event_eviction() {
    let store = SessionStore::new(10, 4);
    let session = store.start_session(Some("proj".to_string()), Some("sticky edit".to_string()));

    let record = |tool_name: &str, success: bool, state_changed: bool| {
        let contract = if tool_name == "edit_project_files" {
            project_edit_contract(SessionPathHint::PathList)
        } else {
            session_tool_contract(tool_name)
        };
        let start = store
            .record_tool_call_started(
                Some(&session.session_id),
                SessionTransport::Mcp,
                tool_name,
                &json!({"project": "proj"}),
                contract,
            )
            .expect("tool start");
        store
            .record_tool_call_finished(
                Some(start),
                success,
                &json!({"state_changed": state_changed}),
                (!success).then_some("failed"),
                None,
            )
            .expect("tool finish");
    };

    record("edit_project_files", true, false);
    assert!(
        !store
            .summary(&session.session_id, None)
            .unwrap()
            .repository_edit_observed
    );

    // Shell/process writes are intentionally ineligible even when their generic
    // effect evidence reports a state change.
    record("run_process", true, true);
    assert!(
        !store
            .summary(&session.session_id, None)
            .unwrap()
            .repository_edit_observed
    );

    record("edit_project_files", false, true);
    assert!(
        !store
            .summary(&session.session_id, None)
            .unwrap()
            .repository_edit_observed
    );

    record("edit_project_files", true, true);
    assert!(
        store
            .summary(&session.session_id, None)
            .unwrap()
            .repository_edit_observed
    );

    // Push enough later events to evict the successful Edit event itself. The
    // monotonic Session fact must not be reconstructed from retained history.
    for index in 0..8 {
        let start = store
            .record_tool_call_started(
                Some(&session.session_id),
                SessionTransport::Api,
                "read_files",
                &json!({"project": "proj", "path": format!("src/{index}.rs")}),
                session_tool_contract("read_files"),
            )
            .expect("read start");
        store
            .record_tool_call_finished(Some(start), true, &json!({}), None, None)
            .expect("read finish");
    }
    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    assert!(summary.repository_edit_observed);
    assert!(summary
        .events
        .iter()
        .all(|event| event.tool_name != "edit_project_files"));
}

#[test]
fn persistence_restore_revalidates_bounded_context_result_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 100);
    let session = store.start_session(
        Some("proj".to_string()),
        Some("context sanitize".to_string()),
    );
    record_result(
        &store,
        &session.session_id,
        "show_changes",
        true,
        json!({
            "head": "demo-head",
            "branch": "b".repeat(500),
            "counts": {
                "modified": 21,
                "token": "wc_pat_must_not_survive"
            }
        }),
    );
    let live = store.summary(&session.session_id, Some(20)).unwrap();
    let live_summary = live
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "show_changes")
        .and_then(|event| event.context_result_summary.as_ref())
        .unwrap();
    assert_eq!(live_summary["counts"]["modified"], 21);
    assert_eq!(live_summary["counts"]["token"], "[redacted]");
    let live_branch = live_summary["branch"].as_str().unwrap();
    assert!(live_branch.chars().count() <= 123);
    assert!(live_branch.ends_with("..."));

    store.flush_persistence();
    drop(store);
    let mut persisted: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    let events = persisted["sessions"][0]["events"].as_array_mut().unwrap();
    let finished = events
        .iter_mut()
        .find(|event| event["kind"] == "tool_call_finished" && event["tool_name"] == "show_changes")
        .unwrap();
    finished["context_result_summary"] = json!({
        "head": "demo-head",
        "branch": "x".repeat(500),
        "counts": {
            "modified": 21,
            "token": "wc_pat_corrupt_ledger_secret"
        },
        "arbitrary_untrusted_body": "must not survive restore"
    });
    std::fs::write(&ledger, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 100);
    let restored_summary = restored.summary(&session.session_id, Some(20)).unwrap();
    let context = restored_summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "show_changes")
        .and_then(|event| event.context_result_summary.as_ref())
        .unwrap();
    assert!(context.get("arbitrary_untrusted_body").is_none());
    assert_eq!(context["counts"]["modified"], 21);
    assert_eq!(context["counts"]["token"], "[redacted]");
    let restored_branch = context["branch"].as_str().unwrap();
    assert!(restored_branch.chars().count() <= 123);
    assert!(restored_branch.ends_with("..."));
}

#[test]
fn retired_context_revision_fields_restore_but_are_never_reemitted() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 100);
    let session = store.start_session(
        Some("proj".to_string()),
        Some("legacy revision".to_string()),
    );
    record_result(
        &store,
        &session.session_id,
        "show_changes",
        true,
        json!({"head": "demo-head"}),
    );
    store.flush_persistence();
    drop(store);

    let fresh = std::fs::read_to_string(&ledger).unwrap();
    assert!(
        !fresh.contains("\"context_revision\""),
        "current writers must omit the retired field: {fresh}"
    );

    let mut persisted: Value = serde_json::from_str(&fresh).unwrap();
    persisted["sessions"][0]["context_revision"] = json!(123);
    let finished = persisted["sessions"][0]["events"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|event| event["kind"] == "tool_call_finished")
        .unwrap();
    finished["context_revision"] = json!(120);
    std::fs::write(&ledger, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 100);
    assert!(restored.summary(&session.session_id, Some(20)).is_some());
    assert_eq!(
        restored.handoff_revision(&session.session_id),
        Some((2, 0)),
        "retired persistence values must not enter the live handoff fence"
    );
    record_result(
        &restored,
        &session.session_id,
        "show_changes",
        true,
        json!({"head": "after-restore"}),
    );
    restored.flush_persistence();
    let rewritten = std::fs::read_to_string(&ledger).unwrap();
    assert!(
        !rewritten.contains("\"context_revision\""),
        "restored legacy fields must stay read-only: {rewritten}"
    );
}

#[test]
fn current_v2_without_retired_context_revision_restores() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 100);
    let session = store.start_session(
        Some("proj".to_string()),
        Some("no legacy field".to_string()),
    );
    record_result(
        &store,
        &session.session_id,
        "show_changes",
        true,
        json!({"head": "demo-head"}),
    );
    store.flush_persistence();
    drop(store);

    let raw = std::fs::read_to_string(&ledger).unwrap();
    assert!(!raw.contains("\"context_revision\""));
    let restored = SessionStore::with_persistence(&ledger, 10, 100);
    assert!(restored.summary(&session.session_id, Some(20)).is_some());
}

#[test]
fn unknown_current_v2_event_fields_still_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 100);
    let session = store.start_session(
        Some("proj".to_string()),
        Some("unknown event field".to_string()),
    );
    record_result(
        &store,
        &session.session_id,
        "show_changes",
        true,
        json!({"head": "demo-head"}),
    );
    store.flush_persistence();
    drop(store);

    let mut persisted: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    let finished = persisted["sessions"][0]["events"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|event| event["kind"] == "tool_call_finished")
        .unwrap();
    finished["definitely_unknown_v2_member"] = json!(true);
    std::fs::write(&ledger, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 100);
    assert_eq!(restored.status().restored_sessions, 0);
    assert!(restored.summary(&session.session_id, None).is_none());
}
