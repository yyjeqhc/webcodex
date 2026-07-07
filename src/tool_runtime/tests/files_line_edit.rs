//! Line edit tests for tool_runtime (replace_line_range, insert_at_line, delete_line_range).

use super::super::files::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellClientCapabilities,
};

#[test]
fn replace_line_range_content_replaces_middle_multiline() {
    let (updated, out) = files::apply_line_edit_content(
        "one\ntwo\nthree\nfour\n",
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(3),
        None,
        "TWO\nTHREE",
        None,
        None,
    )
    .unwrap();
    assert_eq!(updated, "one\nTWO\nTHREE\nfour\n");
    assert_eq!(out["path"], "src/example.rs");
    assert_eq!(out["start_line"], 2);
    assert_eq!(out["end_line"], 3);
    assert_eq!(out["old_line_count"], 2);
    assert_eq!(out["new_line_count"], 2);
    assert_eq!(out["changed"], true);
}

#[test]
fn replace_line_range_content_rejects_sha_mismatch_without_write() {
    let original = "one\ntwo\nthree\n";
    let err = files::apply_line_edit_content(
        original,
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(2),
        None,
        "TWO",
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        None,
    )
    .unwrap_err();
    assert!(err.contains("Rejected before write"));
    assert!(err.contains("expected_old_sha256 mismatch"));
    assert!(err.contains("No files were modified"));
    assert!(err.contains("Retry guidance"));
    assert_eq!(original, "one\ntwo\nthree\n");
}

#[test]
fn line_edit_guard_failure_reports_no_files_modified_and_retry_guidance() {
    let err = files::apply_line_edit_content(
        "one\ntwo\nthree\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "inserted",
        None,
        Some("not-the-anchor"),
    )
    .unwrap_err();
    assert!(err.contains("Rejected before write"));
    assert!(err.contains("expected_anchor_prefix mismatch"));
    assert!(err.contains("No files were modified"));
    assert!(err.contains("Retry guidance"));
    assert!(err.contains("read the file again"));
}

#[test]
fn replace_line_range_content_rejects_out_of_range() {
    let err = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(3),
        None,
        "x",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, "invalid line range");
}

#[test]
fn insert_at_line_content_inserts_start_middle_and_eof() {
    let (start, out) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(1),
        "zero",
        None,
        None,
    )
    .unwrap();
    assert_eq!(start, "zero\none\ntwo\n");
    assert_eq!(out["line"], 1);
    assert_eq!(out["old_line_count"], 1);

    let (middle, _) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "middle\n",
        None,
        None,
    )
    .unwrap();
    assert_eq!(middle, "one\nmiddle\ntwo\n");

    let (eof, out) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(3),
        "three",
        None,
        None,
    )
    .unwrap();
    assert_eq!(eof, "one\ntwo\nthree\n");
    assert_eq!(out["old_line_count"], 0);
}

#[test]
fn insert_at_line_content_rejects_anchor_prefix_mismatch() {
    let err = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "middle",
        None,
        Some("three"),
    )
    .unwrap_err();
    assert!(err.contains("expected_anchor_prefix mismatch"));
    assert!(err.contains("No files were modified"));
}

#[test]
fn delete_line_range_content_deletes_single_and_multiple_lines() {
    let (single, out) = files::apply_line_edit_content(
        "one\ntwo\nthree\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(2),
        Some(2),
        None,
        "",
        None,
        None,
    )
    .unwrap();
    assert_eq!(single, "one\nthree\n");
    assert_eq!(out["old_line_count"], 1);
    assert_eq!(out["new_line_count"], 0);

    let (multi, _) = files::apply_line_edit_content(
        "one\ntwo\nthree\nfour\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(2),
        Some(3),
        None,
        "",
        None,
        None,
    )
    .unwrap();
    assert_eq!(multi, "one\nfour\n");
}

#[test]
fn delete_line_range_content_rejects_out_of_range() {
    let err = files::apply_line_edit_content(
        "one\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(1),
        Some(2),
        None,
        "",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, "invalid line range");
}

#[tokio::test]
async fn line_edit_tools_reject_oversized_expected_prefix_before_agent_dispatch() {
    let runtime = test_runtime();
    let big_prefix = "x".repeat(MAX_EXPECTED_PREFIX_BYTES + 1);

    let result = runtime
        .replace_line_range(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            1,
            "new".to_string(),
            None,
            Some(big_prefix.clone()),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));

    let result = runtime
        .insert_at_line(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            "new".to_string(),
            None,
            Some(big_prefix.clone()),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));

    let result = runtime
        .delete_line_range(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            1,
            None,
            Some(big_prefix),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));
}

#[tokio::test]
async fn line_edit_dispatch_uses_agent_native_file_op_not_python_helper() {
    let runtime = runtime_with_agent_project("editor");
    register_agent(&runtime, "editor", None, ShellClientCapabilities::default()).await;
    let project = agent_test_project_id("editor");

    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .replace_line_range(
                project,
                "EDIT_PROBE.txt".to_string(),
                2,
                2,
                "new".to_string(),
                None,
                Some("old".to_string()),
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "editor".to_string(),
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
    let req = req.expect("replace_line_range should enqueue an agent file op");
    assert_eq!(req.kind, "file_replace_line_range");
    assert_eq!(req.command, "");
    assert!(req.stdin.is_none());
    assert_eq!(req.path.as_deref(), Some("EDIT_PROBE.txt"));
    assert_eq!(req.content.as_deref(), Some("new"));
    assert_eq!(req.start_line, Some(2));
    assert_eq!(req.end_line, Some(2));
    assert_eq!(req.expected_prefix.as_deref(), Some("old"));

    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "editor".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"changed\":true,\"path\":\"EDIT_PROBE.txt\",\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\
                     \"old_line_count\":1,\"new_line_count\":1,\"bytes_written\":4,\
                     \"start_line\":2,\"end_line\":2}"
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
    assert_eq!(result.output["path"], "EDIT_PROBE.txt");
}

#[tokio::test]
async fn replace_in_file_rejects_invalid_input_before_agent_dispatch() {
    // Call the method directly (bypassing authorize_agent_tool, which would
    // otherwise resolve the project first) so we prove input validation
    // fires before any agent request is enqueued. A test_runtime() has no
    // registered agents, so a request that reached dispatch would hang;
    // these all return early with a validation error.
    let runtime = test_runtime();
    let cases: Vec<(String, String, String)> = vec![
        // empty old
        (
            "EDIT_PROBE.txt".to_string(),
            "".to_string(),
            "x".to_string(),
        ),
        // NUL in old
        (
            "EDIT_PROBE.txt".to_string(),
            "a\0b".to_string(),
            "x".to_string(),
        ),
        // NUL in new
        (
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "x\0y".to_string(),
        ),
        // sensitive path
        ("agent.toml".to_string(), "a".to_string(), "b".to_string()),
        // absolute path
        ("/etc/passwd".to_string(), "a".to_string(), "b".to_string()),
        // traversal path
        ("../x".to_string(), "a".to_string(), "b".to_string()),
    ];
    for (path, old, new) in cases {
        let result = runtime
            .replace_in_file("agent:c:p".to_string(), path, old, new, None, None)
            .await;
        assert!(!result.success, "expected validation failure");
        let err = result.error.unwrap();
        // Must NOT be the project-resolution error — proves early reject.
        assert!(
            !err.contains("shell client"),
            "should fail input validation before project resolution: {}",
            err
        );
    }
    // expected_replacements < 1 rejected.
    let result = runtime
        .replace_in_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "b".to_string(),
            Some(0),
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected_replacements"));

    // expected_replacements > 1 requires allow_multiple=true, otherwise
    // the caller's requested count would be ambiguous.
    let result = runtime
        .replace_in_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "b".to_string(),
            Some(2),
            Some(false),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("allow_multiple"));
}

#[tokio::test]
async fn edit_tools_rejected_without_required_capability() {
    let runtime = runtime_with_agent_project("editor");
    // Structured edit tools require file_write. Default caps have shell=true
    // and file_write=false, so both calls should fail the same capability gate.
    register_agent(&runtime, "editor", None, ShellClientCapabilities::default()).await;
    let auth = auth_context(None, true);

    // replace_in_file: requires file_write
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceInFile {
                project: agent_test_project_id("editor"),
                path: "EDIT_PROBE.txt".to_string(),
                old: "foo".to_string(),
                new: "bar".to_string(),
                session_id: None,
                expected_replacements: None,
                allow_multiple: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_write"));

    // write_project_file: requires file_write
    let result = runtime
        .dispatch_with_auth(
            ToolCall::WriteProjectFile {
                project: agent_test_project_id("editor"),
                path: "EDIT_PROBE.txt".to_string(),
                content: "new".to_string(),
                session_id: None,
                overwrite: None,
                expected_sha256: None,
                expected_content_prefix: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_write"));

    // line edit tools: requires file_write
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceLineRange {
                project: agent_test_project_id("editor"),
                path: "EDIT_PROBE.txt".to_string(),
                start_line: 1,
                end_line: 1,
                new_text: "new".to_string(),
                session_id: None,
                expected_old_sha256: None,
                expected_old_prefix: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_write"));
}
