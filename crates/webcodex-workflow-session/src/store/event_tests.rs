use super::*;
use crate::events::SessionPathHint;
use serde_json::json;

#[test]
fn event_construction_preserves_tool_identity_wire_shape_and_retained_sequence() {
    let store = SessionStore::new_in_memory(4, 3);
    let session = store.start_session(Some("project-a".to_string()), None);
    let start = store
        .record_tool_call_started_with_metadata(
            Some(&session.session_id),
            SessionTransport::Mcp,
            "run_process",
            &json!({"project": "project-a", "executable": "true"}),
            Some("project-a".to_string()),
            ToolCallRecorderMetadata {
                logical_invocation_id: Some(format!("wc_inv_{}", "a".repeat(32))),
                logical_invocation_role: Some("business".to_string()),
                expectation: ToolCallExpectation {
                    result_expectation: Some("success".to_string()),
                    accepted_exit_codes: vec![0],
                    assertion_name: Some("event contract".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
            SessionToolContract {
                risk_class: "process_run",
                read_like: false,
                write_like: false,
                shell_like: true,
                git_like: false,
                change_summary_like: false,
                project_write: false,
                path_hint: SessionPathHint::None,
            },
        )
        .unwrap();
    let start_id = start.event_id.clone();
    let started =
        serde_json::to_value(&store.summary(&session.session_id, None).unwrap().events[0]).unwrap();
    let finish_id = store
        .record_tool_call_finished(
            Some(start),
            true,
            &json!({
                "exit_code": 0,
                "execution_state": "completed",
                "command_started": true,
                "command_completed": true,
            }),
            None,
            None,
        )
        .unwrap();
    let finished =
        serde_json::to_value(&store.summary(&session.session_id, None).unwrap().events[1]).unwrap();
    for field in [
        "session_id",
        "call_id",
        "logical_invocation_id",
        "logical_invocation_role",
        "transport",
        "tool_name",
        "project",
        "resolved_project",
        "risk_class",
        "read_like",
        "write_like",
        "shell_like",
        "git_like",
        "change_summary_like",
        "diff_review_like",
        "started_at",
        "result_expectation",
        "accepted_exit_codes",
        "assertion_name",
    ] {
        assert_eq!(started[field], finished[field], "{field}");
    }
    assert_eq!(started["event_id"], start_id);
    assert_eq!(finished["event_id"], finish_id);
    assert_ne!(start_id, finish_id);
    assert_eq!(started["kind"], "tool_call_started");
    assert!(started["finished_at"].is_null());
    assert!(started["status"].is_null());
    assert_eq!(finished["kind"], "tool_call_finished");
    assert_eq!(finished["status"], "succeeded");
    assert_eq!(finished["exit_code"], 0);
    assert!(finished["input_summary"].is_null());

    store
        .update_execution_context(
            &session.session_id,
            SessionExecutionContext::default(),
            SessionTransport::Api,
        )
        .unwrap();
    let closed = store.close_session(&session.session_id).unwrap();
    assert!(!closed.already_closed);
    let summary = store.summary(&session.session_id, Some(3)).unwrap();
    assert_eq!(summary.events_total, 4);
    assert_eq!(summary.events_evicted, 1);
    assert_eq!(summary.ledger_first_retained_sequence, 1);
    assert_eq!(summary.first_retained_sequence, 1);
    assert_eq!(summary.events[0].event_id, finish_id);
    assert_eq!(summary.events[1].kind, "session_execution_context_updated");
    let close = &summary.events[2];
    // Keep the existing distinction between nullable fields and omitted additive
    // evidence; a shared initializer must not change the persisted event shape.
    assert_eq!(
        serde_json::to_value(close).unwrap(),
        json!({
            "event_id": close.event_id,
            "session_id": session.session_id,
            "kind": "session_closed",
            "timestamp": close.timestamp,
            "transport": "system",
            "tool_name": "close_session",
            "project": null,
            "resolved_project": null,
            "risk_class": "read_only",
            "read_like": true,
            "write_like": false,
            "shell_like": false,
            "git_like": false,
            "change_summary_like": false,
            "diff_review_like": false,
            "started_at": close.timestamp,
            "finished_at": close.timestamp,
            "duration_ms": 0,
            "status": "succeeded",
            "exit_code": null,
            "failure_kind": null,
            "error_kind": null,
            "error_message_summary": null,
            "changed_paths": [],
            "job_id": null,
            "input_summary": null,
        }),
    );
}
