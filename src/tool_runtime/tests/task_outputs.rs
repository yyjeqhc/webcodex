use super::super::task_outputs::{
    attach_task_outputs, retained_task_outputs, task_output_observation, validate_task_output_paths,
};
use super::super::{sessions::SessionTransport, ToolResult};
use super::support::test_runtime;
use serde_json::json;
use webcodex_core::task_outputs::{TaskOutput, TaskOutputStatus, TaskOutputs};

fn observed(path: &str) -> TaskOutput {
    task_output_observation(
        path.to_string(),
        &ToolResult::ok(json!({
            "path": path, "exists": true, "bytes": 12, "sha256": "a".repeat(64), "mime_type": "text/plain"
        })),
    )
}

#[test]
fn task_outputs_reject_ambiguous_protected_and_unbounded_paths() {
    assert!(validate_task_output_paths(&["data/report.csv".to_string()]));
    for paths in [
        vec!["../outside".to_string()],
        vec!["/absolute".to_string()],
        vec![".env".to_string()],
        vec!["a".to_string(), "a".to_string()],
        vec!["a".repeat(513)],
        vec!["a/./b".to_string()],
        (0..17).map(|i| format!("{i}.csv")).collect(),
    ] {
        assert!(!validate_task_output_paths(&paths), "{paths:?}");
    }
}

#[test]
fn task_outputs_require_complete_exact_metadata_and_never_trust_failure_prose() {
    let item = observed("result.csv");
    assert_eq!(item.status, TaskOutputStatus::Verified);
    let unknown_mime = task_output_observation(
        "big.bin".into(),
        &ToolResult::ok(json!({
            "path":"big.bin", "exists":true, "bytes":10 * 1024 * 1024 + 1,
            "sha256":"a".repeat(64), "mime_type":null
        })),
    );
    assert_eq!(unknown_mime.status, TaskOutputStatus::Verified);
    assert_eq!(
        unknown_mime.mime_type.as_deref(),
        Some("application/octet-stream")
    );
    for result in [
        ToolResult::ok(
            json!({"path":"other.csv","exists":true,"bytes":12,"sha256":"a".repeat(64),"mime_type":"text/plain"}),
        ),
        ToolResult::ok(json!({"path":"result.csv","exists":true,"bytes":12})),
        ToolResult::err_with_output(
            "secret stdout",
            json!({"path":"result.csv","exists":true,"bytes":12,"sha256":"a".repeat(64),"mime_type":"text/plain"}),
        ),
    ] {
        let item = task_output_observation("result.csv".into(), &result);
        assert_eq!(item.status, TaskOutputStatus::Unavailable);
        assert!(item.sha256.is_none());
        assert!(!serde_json::to_string(&item).unwrap().contains("secret"));
    }
    assert_eq!(
        task_output_observation(
            "result.csv".into(),
            &ToolResult::ok(json!({"path":"result.csv","exists":false}))
        )
        .status,
        TaskOutputStatus::Missing
    );
}

#[test]
fn task_outputs_block_missing_files_without_erasing_existing_task_blockers() {
    let outputs = TaskOutputs {
        items: vec![TaskOutput {
            path: "missing.csv".into(),
            status: TaskOutputStatus::Missing,
            file_bytes: None,
            sha256: None,
            mime_type: None,
        }],
        verified_count: 0,
        missing_count: 1,
        unavailable_count: 0,
        observed_at: 1,
    };
    let mut decision = json!({"task_outcome":{"status":"fail","blocking":true,"blocking_reasons":["validation_failed"],"warning_reasons":[]},"hard_blockers":["validation_failed"],"suggested_next_actions":[]});
    attach_task_outputs(&mut decision, &outputs);
    assert_eq!(
        decision["task_outcome"]["blocking_reasons"],
        json!(["validation_failed", "task_outputs_unverified"])
    );
    assert_eq!(decision["task_outcome"]["blocking"], true);
    assert!(decision["task_outcome"].get("reasons").is_none());
}

#[test]
fn task_outputs_ledger_preserves_sixteen_long_paths_and_latest_finish_invalidates_old_results() {
    let runtime = test_runtime();
    let project = "agent:fixture:files";
    let session = runtime.sessions.start_session(Some(project.into()), None);
    let outputs = TaskOutputs {
        items: (0..16)
            .map(|i| observed(&format!("{}/report{i}.csv", "x".repeat(180))))
            .collect(),
        verified_count: 16,
        missing_count: 0,
        unavailable_count: 0,
        observed_at: 1,
    };
    assert!(outputs.valid());
    let record = |output| {
        let start = runtime.sessions.record_tool_call_started_with_options(
            Some(&session.session_id),
            SessionTransport::Api,
            "finish_coding_task",
            &json!({"project":project,"session_id":session.session_id}),
            Some(project.into()),
            crate::tool_runtime::sessions::session_tool_contract("finish_coding_task"),
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &output, None, None);
    };
    record(json!({"task_outputs":outputs}));
    let summary = runtime.sessions.summary(&session.session_id, None).unwrap();
    let retained = retained_task_outputs(&summary).unwrap();
    assert_eq!(retained["items"].as_array().unwrap().len(), 16);
    assert_eq!(retained["items"][0]["path"], outputs.items[0].path);
    // A malformed later result cannot revive the prior manifest.
    record(json!({"task_outputs":{"items":[],"raw_stdout":"PRIVATE"}}));
    assert!(
        retained_task_outputs(&runtime.sessions.summary(&session.session_id, None).unwrap())
            .is_none()
    );
    record(json!({}));
    assert!(
        retained_task_outputs(&runtime.sessions.summary(&session.session_id, None).unwrap())
            .is_none()
    );
}

#[test]
fn task_outputs_do_not_attribute_another_business_session_to_its_recorder() {
    let runtime = test_runtime();
    let project = "agent:fixture:files";
    let business = runtime.sessions.start_session(Some(project.into()), None);
    let recorder = runtime.sessions.start_session(Some(project.into()), None);
    let outputs = TaskOutputs {
        items: vec![observed("result.csv")],
        verified_count: 1,
        missing_count: 0,
        unavailable_count: 0,
        observed_at: 1,
    };
    let start = runtime.sessions.record_tool_call_started_with_options(
        Some(&recorder.session_id),
        SessionTransport::Api,
        "finish_coding_task",
        &json!({"project":project,"session_id":business.session_id}),
        Some(project.into()),
        crate::tool_runtime::sessions::session_tool_contract("finish_coding_task"),
    );
    runtime.sessions.record_tool_call_finished(
        start,
        true,
        &json!({"task_outputs": outputs}),
        None,
        None,
    );
    assert!(retained_task_outputs(
        &runtime
            .sessions
            .summary(&recorder.session_id, None)
            .unwrap()
    )
    .is_none());
}
