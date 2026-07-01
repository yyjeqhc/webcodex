//! apply_text_edits tests for tool_runtime.

use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellClientCapabilities,
};
use serde_json::Value;

#[test]
fn apply_text_edits_replace_exact_large_block() {
    let original = "mod foo {\n    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }\n}\n";
    let old_block =
        "    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }";
    let new_block =
        "    fn new_a() -> u32 {\n        1\n    }\n    fn new_b() -> u32 {\n        2\n    }";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some(old_block),
        Some(new_block),
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/foo.rs", &edits, None, false).unwrap();
    assert!(updated.contains("fn new_a() -> u32 {"));
    assert!(updated.contains("fn new_b() -> u32 {"));
    assert!(!updated.contains("old_a"));
    assert!(!updated.contains("old_b"));
    assert_eq!(out["path"], "src/foo.rs");
    assert_eq!(out["applied_count"], 1);
    assert_eq!(out["changed"], true);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["edits"][0]["kind"], "replace_exact");
    assert_eq!(out["changed_paths"][0], "src/foo.rs");
}

#[test]
fn apply_text_edits_multiple_edits_atomic() {
    let original = "alpha\nbeta\ngamma\ndelta\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("beta"),
            Some("BETA"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("delta"),
            Some("DELTA"),
            None,
        ),
    ];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "alpha\nBETA\ngamma\nDELTA\n");
    assert_eq!(out["applied_count"], 2);
    assert_eq!(out["edits"].as_array().unwrap().len(), 2);
}

#[test]
fn apply_text_edits_rejects_missing_match() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("nonexistent"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("not found"));
    assert!(err.contains("No files were modified"));
    // Original is untouched (pure function never mutates input).
    assert_eq!(original, "alpha\nbeta\n");
}

#[test]
fn apply_text_edits_rejects_ambiguous_match() {
    let original = "dup\ndup\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
    assert!(err.contains("ambiguous"));
}

#[test]
fn apply_text_edits_expected_file_sha256_guard() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("beta"),
        Some("BETA"),
        None,
    )];
    // Wrong sha → rejected before any edit is applied.
    let err = files::apply_text_edits_to_string(
        original,
        "src/x.rs",
        &edits,
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        false,
    )
    .unwrap_err();
    assert!(err.contains("expected_file_sha256 mismatch"));
    // Correct sha → succeeds.
    let real_sha = files::sha256_hex_bytes(original.as_bytes());
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, Some(&real_sha), false)
            .unwrap();
    assert_eq!(updated, "alpha\nBETA\n");
}

#[test]
fn apply_text_edits_insert_before_after_unique_anchor() {
    let original = "header\nbody\nfooter\n";
    // insert_before unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("// before body\n"),
        Some("body"),
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\n// before body\nbody\nfooter\n");
    assert_eq!(out["edits"][0]["kind"], "insert_before");

    // insert_after unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertAfter,
        None,
        Some("// after body\n"),
        Some("body\n"),
    )];
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\nbody\n// after body\nfooter\n");

    // Ambiguous anchor → rejected.
    let dup = "tag\ntag\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("x"),
        Some("tag"),
    )];
    let err = files::apply_text_edits_to_string(dup, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
}

#[test]
fn apply_text_edits_delete_exact_removes_block() {
    let original = "keep1\ndelete_me\nkeep2\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::DeleteExact,
        Some("delete_me\n"),
        None,
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "keep1\nkeep2\n");
    assert_eq!(out["edits"][0]["kind"], "delete_exact");
}

#[test]
fn apply_text_edits_rejects_overlapping_edits() {
    let original = "abcdefghij\n";
    // Two replace_exact ops whose ranges overlap.
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("cde"),
            Some("X"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("def"),
            Some("Y"),
            None,
        ),
    ];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("overlap"));
}

#[tokio::test]
async fn apply_text_edits_dry_run_does_not_write() {
    let runtime = runtime_with_agent_project("ate-dry");
    register_agent(
        &runtime,
        "ate-dry",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-dry");

    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .apply_text_edits(
                project_for_task,
                "EDIT_PROBE.txt".to_string(),
                vec![text_edit(
                    ApplyTextEditKind::ReplaceExact,
                    Some("old"),
                    Some("new"),
                    None,
                )],
                Some(true),
                None,
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "ate-dry".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("apply_text_edits should enqueue an agent file op");
    assert_eq!(req.kind, "file_apply_text_edits");
    // The payload carries dry_run and the edits.
    let payload: Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["edits"][0]["kind"], "replace_exact");

    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-dry".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"path\":\"EDIT_PROBE.txt\",\"dry_run\":true,\"applied_count\":1,\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\"changed\":false,\
                     \"would_change\":true,\"edits\":[],\"changed_paths\":[\"EDIT_PROBE.txt\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["dry_run"], true);
    assert_eq!(result.output["would_change"], true);
    assert_eq!(result.output["changed"], false);
}

#[tokio::test]
async fn apply_text_edits_read_only_session_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let session = runtime.sessions.start_session_with_guards(
        Some("demo".to_string()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );

    let result = runtime
        .dispatch(ToolCall::ApplyTextEdits {
            project: "demo".to_string(),
            path: "should-not-exist.txt".to_string(),
            edits: vec![text_edit(
                ApplyTextEditKind::ReplaceExact,
                Some("old"),
                Some("new"),
                None,
            )],
            dry_run: None,
            expected_file_sha256: None,
            session_id: Some(session.session_id.clone()),
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["guard"], "deny_write_tools");
    assert_eq!(result.output["mode"], "read_only");
    assert!(!tmp.path().join("should-not-exist.txt").exists());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("failed"));
    assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
}

#[tokio::test]
async fn apply_text_edits_session_event_summary() {
    let runtime = runtime_with_agent_project("ate-sess");
    register_agent(
        &runtime,
        "ate-sess",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-sess");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("apply_text_edits session".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );

    let bootstrap = auth_context(None, true);
    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let session_id = session.session_id.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch_with_auth(
                ToolCall::ApplyTextEdits {
                    project: project_for_task,
                    path: "src/lib.rs".to_string(),
                    edits: vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        Some("SECRET_OLD_BLOCK"),
                        Some("SECRET_NEW_BLOCK"),
                        None,
                    )],
                    dry_run: None,
                    expected_file_sha256: None,
                    session_id: Some(session_id),
                },
                Some(&bootstrap),
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "ate-sess".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("apply_text_edits should enqueue an agent file op");
    assert_eq!(req.kind, "file_apply_text_edits");
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-sess".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"path\":\"src/lib.rs\",\"dry_run\":false,\"applied_count\":1,\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\"changed\":true,\
                     \"would_change\":true,\"edits\":[{\"index\":0,\"kind\":\"replace_exact\",\
                     \"old_start_line\":1,\"old_end_line\":1,\"new_line_count\":1}],\
                     \"changed_paths\":[\"src/lib.rs\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["changed_paths"][0], "src/lib.rs");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    // changed_paths recorded from the input path.
    assert!(event.changed_paths.iter().any(|p| p == "src/lib.rs"));
    // input_summary lives on the tool_call_started event; it must NOT leak
    // old_text/new_text contents.
    let started = summary
        .events
        .iter()
        .rev()
        .find(|e| e.kind == "tool_call_started" && e.tool_name == "apply_text_edits")
        .expect("started event for apply_text_edits");
    let input_summary = started
        .input_summary
        .as_ref()
        .expect("input_summary present on started event");
    let summary_str = serde_json::to_string(input_summary).unwrap();
    assert!(summary_str.contains("edit_count"));
    assert!(summary_str.contains("src/lib.rs"));
    assert!(
        !summary_str.contains("SECRET_OLD_BLOCK"),
        "input_summary must not leak old_text content: {}",
        summary_str
    );
    assert!(
        !summary_str.contains("SECRET_NEW_BLOCK"),
        "input_summary must not leak new_text content: {}",
        summary_str
    );
    assert_eq!(input_summary["old_text_present"], true);
    assert_eq!(input_summary["new_text_present"], true);
}
