//! Session guard tests: read-only sessions, guard denial, deny_write/deny_shell.

use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use crate::tool_runtime::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use serde_json::json;
use std::fs;
use std::path::Path;

async fn runtime_with_two_agent_projects(
    root_a: &Path,
    root_b: &Path,
) -> (ToolRuntime, String, String) {
    let runtime = test_runtime();
    let alpha = register_agent_project_at_path(&runtime, "alpha-client", "alpha", root_a).await;
    let bravo = register_agent_project_at_path(&runtime, "bravo-client", "bravo", root_b).await;
    (runtime, alpha, bravo)
}

fn latest_finished_event<'a>(
    summary: &'a sessions::SessionSummary,
    tool_name: &str,
) -> &'a sessions::SessionEvent {
    summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == tool_name)
        .unwrap_or_else(|| panic!("missing finished event for {tool_name}"))
}

#[tokio::test]
async fn unknown_session_id_fails_before_execution_or_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("README.md"), "hello\n").unwrap();
    let runtime = runtime_with_project(root, "demo");

    let read = runtime
        .dispatch(ToolCall::ReadFile {
            project: "demo".to_string(),
            path: "README.md".to_string(),
            session_id: Some("wc_sess_missing".to_string()),
            start_line: None,
            limit: None,
            with_line_numbers: None,
        })
        .await;
    assert!(!read.success);
    assert_eq!(read.output["error_kind"], "unknown_session_id");
    assert_eq!(read.output["session_id"], "wc_sess_missing");
    assert!(read
        .error
        .as_deref()
        .unwrap()
        .contains("unknown_session_id"));

    let write = runtime
        .dispatch(ToolCall::WriteProjectFile {
            project: "demo".to_string(),
            path: "should-not-exist.txt".to_string(),
            content: "nope".to_string(),
            session_id: Some("wc_sess_missing".to_string()),
            overwrite: None,
            expected_sha256: None,
            expected_content_prefix: None,
        })
        .await;
    assert!(!write.success);
    assert_eq!(write.output["error_kind"], "unknown_session_id");
    assert!(!root.join("should-not-exist.txt").exists());
}

#[tokio::test]
async fn same_project_session_records_without_project_mismatch_warning() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    fs::write(tmp_a.path().join("README.md"), "alpha\n").unwrap();
    fs::write(tmp_b.path().join("README.md"), "bravo\n").unwrap();
    let (runtime, alpha, _bravo) =
        runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("same".to_string()));

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let alpha = alpha.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: alpha,
                        path: "README.md".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "alpha-client")
        .await
        .expect("read_file should enqueue an agent request");
    complete_patch_agent_request(&runtime, "alpha-client", &req.request_id, 0, "alpha\n", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("warning_kind").is_none());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = latest_finished_event(&summary, "read_file");
    assert!(event.warning_kind.is_none());
}

#[tokio::test]
async fn read_only_cross_project_session_succeeds_with_structured_warning() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    fs::write(tmp_a.path().join("README.md"), "alpha\n").unwrap();
    fs::write(tmp_b.path().join("README.md"), "bravo\n").unwrap();
    let (runtime, alpha, bravo) = runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("read mismatch".to_string()));

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let bravo = bravo.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: bravo,
                        path: "README.md".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "bravo-client")
        .await
        .expect("read_file should enqueue an agent request");
    complete_patch_agent_request(&runtime, "bravo-client", &req.request_id, 0, "bravo\n", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["warning_kind"], "session_project_mismatch");
    assert_eq!(result.output["session_project"], alpha);
    assert_eq!(result.output["request_project"], bravo);
    assert_eq!(result.output["allow_cross_project_session_required"], true);
    assert_eq!(result.output["allow_cross_project_session"], false);
    let warning = result.output["warnings"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(warning["warning_kind"], "session_project_mismatch");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = latest_finished_event(&summary, "read_file");
    assert_eq!(
        event.warning_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(event.session_project.as_deref(), Some(alpha.as_str()));
    assert_eq!(event.request_project.as_deref(), Some(bravo.as_str()));
}

#[tokio::test]
async fn mutation_cross_project_session_fails_before_write() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let (runtime, alpha, bravo) = runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("write mismatch".to_string()));

    let result = runtime
        .dispatch_with_auth(
            ToolCall::WriteProjectFile {
                project: bravo.clone(),
                path: "blocked.txt".to_string(),
                content: "nope\n".to_string(),
                session_id: Some(session.session_id.clone()),
                overwrite: None,
                expected_sha256: None,
                expected_content_prefix: None,
            },
            Some(&auth),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "session_project_mismatch");
    assert_eq!(result.output["session_project"], alpha);
    assert_eq!(result.output["request_project"], bravo);
    assert_eq!(result.output["allow_cross_project_session_required"], true);
    assert!(!tmp_b.path().join("blocked.txt").exists());

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = latest_finished_event(&summary, "write_project_file");
    assert_eq!(
        event.failure_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(
        event.error_kind.as_deref(),
        Some("session_project_mismatch")
    );
}

#[tokio::test]
async fn allow_cross_project_session_allows_mutation_and_records_warning() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let (runtime, alpha, bravo) = runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("allow mismatch".to_string()));

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let bravo = bravo.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth_transport_options(
                    ToolCall::WriteProjectFile {
                        project: bravo,
                        path: "allowed.txt".to_string(),
                        content: "allowed\n".to_string(),
                        session_id: Some(session_id),
                        overwrite: None,
                        expected_sha256: None,
                        expected_content_prefix: None,
                    },
                    Some(&auth),
                    sessions::SessionTransport::Api,
                    true,
                    true,
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "bravo-client")
        .await
        .expect("write_project_file should enqueue a native file-op request");
    assert_eq!(req.kind, "file_write_project_file");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("file-op payload")).unwrap();
    assert_eq!(payload["path"], "allowed.txt");
    assert_eq!(payload["content"], "allowed\n");
    complete_patch_agent_request(
        &runtime,
        "bravo-client",
        &req.request_id,
        0,
        r#"{"path":"allowed.txt","bytes_written":8,"sha256":"abc","changed":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["warning_kind"], "session_project_mismatch");
    assert_eq!(result.output["allow_cross_project_session"], true);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = latest_finished_event(&summary, "write_project_file");
    assert_eq!(
        event.warning_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(event.allow_cross_project_session, Some(true));
}

#[tokio::test]
async fn recording_session_id_obeys_project_boundary() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let (runtime, alpha, bravo) = runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("record mismatch".to_string()));

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "write_project_file".to_string(),
                arguments: json!({
                    "project": bravo.clone(),
                    "path": "recording-blocked.txt",
                    "content": "nope\n"
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: Some(&session.session_id),
                auth: Some(&auth),
                record_oauth_scope_denials: true,
            },
        )
        .await;

    assert!(!outcome.success);
    let result = outcome.result.unwrap();
    assert_eq!(result.output["failure_kind"], "session_project_mismatch");
    assert!(!tmp_b.path().join("recording-blocked.txt").exists());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = latest_finished_event(&summary, "write_project_file");
    assert_eq!(
        event.failure_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(event.request_project.as_deref(), Some(bravo.as_str()));
}

#[tokio::test]
async fn current_session_binding_cannot_cross_project_boundary() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    fs::write(tmp_b.path().join("README.md"), "bravo\n").unwrap();
    let (runtime, alpha, bravo) = runtime_with_two_agent_projects(tmp_a.path(), tmp_b.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(alpha.clone()), Some("current mismatch".to_string()));

    let bind = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": bravo.clone(), "session_id": session.session_id}),
            )
            .unwrap(),
            Some(&auth),
        )
        .await;
    assert!(!bind.success);
    assert_eq!(bind.output["failure_kind"], "session_project_mismatch");
    assert_eq!(bind.output["session_project"], alpha);
    assert_eq!(bind.output["request_project"], bravo);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let bravo = bravo.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: bravo,
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "bravo-client")
        .await
        .expect("read_file should enqueue an agent request");
    complete_patch_agent_request(&runtime, "bravo-client", &req.request_id, 0, "bravo\n", "").await;
    let read = task.await.unwrap();
    assert!(read.success, "{:?}", read.error);
    assert!(read.output.get("session_recorded").is_none());
}

#[tokio::test]
async fn read_only_current_session_guard_blocks_write_before_enqueue() {
    let runtime = runtime_with_agent_project("current-guard");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    caps.file_write = true;
    register_agent(&runtime, "current-guard", None, caps).await;
    let project = agent_test_project_id("current-guard");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("readonly current".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let bind = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": session.session_id}),
            )
            .unwrap(),
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);

    let result = runtime
        .dispatch(ToolCall::WriteProjectFile {
            project: project.clone(),
            path: "blocked.txt".to_string(),
            content: "nope".to_string(),
            session_id: None,
            overwrite: None,
            expected_sha256: None,
            expected_content_prefix: None,
        })
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["session_id"], session.session_id);
    assert_eq!(result.output["session_recorded"], true);
    assert!(
        next_agent_request_for_instance(&runtime, "current-guard", "inst")
            .await
            .is_none(),
        "guard denial must happen before an agent request is enqueued"
    );
}

#[tokio::test]
async fn start_session_defaults_to_normal_without_guards() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::from_tool_name("start_session", json!({})).unwrap())
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["mode"], "normal");
    assert_eq!(result.output["guards"]["deny_write_tools"], false);
    assert_eq!(result.output["guards"]["deny_shell_tools"], false);
    let session_id = result.output["session_id"].as_str().unwrap();
    let summary = runtime.sessions.summary(session_id, None).unwrap();
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
}

#[tokio::test]
async fn start_session_read_only_enables_write_and_shell_guards() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "start_session",
                json!({"mode": "read_only", "deny_shell_tools": false}),
            )
            .unwrap(),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["mode"], "read_only");
    assert_eq!(result.output["guards"]["deny_write_tools"], true);
    assert_eq!(result.output["guards"]["deny_shell_tools"], true);
    let session_id = result.output["session_id"].as_str().unwrap();
    let summary = runtime.sessions.summary(session_id, None).unwrap();
    assert_eq!(summary.mode, SessionMode::ReadOnly);
    assert!(summary.guards.deny_write_tools);
    assert!(summary.guards.deny_shell_tools);
}

#[tokio::test]
async fn read_only_session_allows_read_file_and_records_success() {
    let runtime = runtime_with_agent_project("guard-read");
    register_agent(
        &runtime,
        "guard-read",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("guard-read");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "guard-read", "inst")
        .await
        .expect("read_file should be allowed in read_only session");
    assert_eq!(req.kind, "file_read");
    complete_patch_agent_request(&runtime, "guard-read", &req.request_id, 0, "hello\n", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.read_like, 1);
    assert_eq!(
        finished_event(&summary, "read_file").status.as_deref(),
        Some("succeeded")
    );
}

#[tokio::test]
async fn read_only_session_rejects_write_project_file_before_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let session = runtime.sessions.start_session_with_guards(
        Some("demo".to_string()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let message_text = "guard risk message must stay out of hint";
    runtime
        .sessions
        .post_message(sessions::PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: sessions::SessionMessageKind::Risk,
            message: message_text.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: sessions::SessionMessagePriority::High,
        })
        .unwrap();

    let result = runtime
        .dispatch(ToolCall::WriteProjectFile {
            project: "demo".to_string(),
            path: "should-not-exist.txt".to_string(),
            content: "nope".to_string(),
            session_id: Some(session.session_id.clone()),
            overwrite: None,
            expected_sha256: None,
            expected_content_prefix: None,
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["guard"], "deny_write_tools");
    assert_eq!(result.output["mode"], "read_only");
    assert_eq!(result.output["session_recorded"], true);
    assert!(result.output["session_event_id"].as_str().is_some());
    assert_eq!(result.output["session_hint"]["has_open_messages"], true);
    assert_eq!(result.output["session_hint"]["open_counts"]["risk"], 1);
    assert_eq!(result.output["session_hint"]["highest_priority"], "high");
    let serialized_hint = serde_json::to_string(&result.output["session_hint"]).unwrap();
    assert!(
        !serialized_hint.contains(message_text),
        "session_hint leaked message text: {serialized_hint}"
    );
    assert!(!tmp.path().join("should-not-exist.txt").exists());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "write_project_file");
    assert_eq!(event.status.as_deref(), Some("failed"));
    assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
}

#[tokio::test]
async fn read_only_session_rejects_all_artifact_upload_tools_without_base64_leak() {
    let runtime = runtime_with_agent_project("guard-artifact-upload");
    register_agent(
        &runtime,
        "guard-artifact-upload",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("guard-artifact-upload");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read only artifacts".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let content_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        "SECRET_UPLOAD_CHUNK_DO_NOT_LOG",
    );
    let bootstrap = auth_context(None, true);

    let calls = vec![
        ToolCall::ArtifactUploadBegin {
            project: project.clone(),
            path: "artifacts/imports/blocked.txt".to_string(),
            session_id: Some(session.session_id.clone()),
            expected_bytes: Some(3),
            expected_sha256: None,
            mime_type: Some("text/plain".to_string()),
            overwrite: None,
        },
        ToolCall::ArtifactUploadChunk {
            project: project.clone(),
            path: "artifacts/imports/blocked.txt".to_string(),
            upload_id: "wc_upload_test_1".to_string(),
            offset: 0,
            content_base64: content_base64.clone(),
            session_id: Some(session.session_id.clone()),
        },
        ToolCall::ArtifactUploadFinish {
            project: project.clone(),
            path: "artifacts/imports/blocked.txt".to_string(),
            upload_id: "wc_upload_test_1".to_string(),
            session_id: Some(session.session_id.clone()),
        },
        ToolCall::ArtifactUploadAbort {
            project,
            path: "artifacts/imports/blocked.txt".to_string(),
            upload_id: "wc_upload_test_1".to_string(),
            session_id: Some(session.session_id.clone()),
        },
    ];

    for call in calls {
        let tool_name = call.tool_name().to_string();
        let result = runtime.dispatch_with_auth(call, Some(&bootstrap)).await;
        assert!(!result.success, "{tool_name}");
        assert_eq!(result.output["error_kind"], "session_guard_denied");
        assert_eq!(result.output["guard"], "deny_write_tools");
        assert_eq!(result.output["mode"], "read_only");
        assert_eq!(result.output["session_recorded"], true);
    }
    assert!(
        next_patch_agent_request(&runtime, "guard-artifact-upload")
            .await
            .is_none(),
        "artifact upload guard denial must not enqueue an agent request"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 4);
    assert_eq!(summary.counts.write_like, 4);
    for tool_name in [
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        let event = finished_event(&summary, tool_name);
        assert_eq!(event.status.as_deref(), Some("failed"), "{tool_name}");
        assert_eq!(
            event.error_kind.as_deref(),
            Some("session_guard_denied"),
            "{tool_name}"
        );
    }

    let started = summary
        .events
        .iter()
        .rev()
        .find(|event| {
            event.kind == "tool_call_started" && event.tool_name == "artifact_upload_chunk"
        })
        .expect("started event for artifact_upload_chunk");
    let input_summary = started.input_summary.as_ref().unwrap();
    assert_eq!(input_summary["path"], "artifacts/imports/blocked.txt");
    assert_eq!(input_summary["upload_id"], "wc_upload_test_1");
    assert_eq!(input_summary["offset"], 0);
    assert_eq!(input_summary["content_base64_present"], true);
    assert!(input_summary.get("content_base64").is_none());
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains(&content_base64) && !serialized.contains("SECRET_UPLOAD_CHUNK"),
        "guard denial event leaked artifact chunk content: {serialized}"
    );
}

#[tokio::test]
async fn read_only_session_rejects_run_shell_before_agent_enqueue() {
    let runtime = runtime_with_agent_project("guard-shell");
    register_agent(
        &runtime,
        "guard-shell",
        None,
        ShellClientCapabilities {
            shell: true,
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("guard-shell");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );

    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project,
                command: "echo should-not-run".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: Some(30),
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["guard"], "deny_shell_tools");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["session_recorded"], true);
    assert!(
        next_patch_agent_request(&runtime, "guard-shell")
            .await
            .is_none(),
        "run_shell guard denial must not enqueue an agent request"
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.shell_like, 1);
    let event = finished_event(&summary, "run_shell");
    assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
}

#[tokio::test]
async fn deny_write_only_allows_read_and_shell_tools() {
    let runtime = runtime_with_agent_project("guard-write-only");
    register_agent(
        &runtime,
        "guard-write-only",
        None,
        ShellClientCapabilities {
            file_read: true,
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("guard-write-only");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        None,
        SessionMode::Normal,
        sessions::SessionGuards {
            deny_write_tools: true,
            deny_shell_tools: false,
        },
    );
    let bootstrap = auth_context(None, true);

    let denied = runtime
        .dispatch_with_auth(
            ToolCall::WriteProjectFile {
                project: project.clone(),
                path: "blocked.txt".to_string(),
                content: "x".to_string(),
                session_id: Some(session.session_id.clone()),
                overwrite: None,
                expected_sha256: None,
                expected_content_prefix: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["guard"], "deny_write_tools");

    let read_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "guard-write-only", "inst")
        .await
        .expect("read_file should be allowed with deny_write_tools only");
    complete_patch_agent_request(
        &runtime,
        "guard-write-only",
        &req.request_id,
        0,
        "hello\n",
        "",
    )
    .await;
    assert!(read_task.await.unwrap().success);

    let shell_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "exit 0".to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "guard-write-only")
        .await
        .expect("run_shell should be allowed when deny_shell_tools=false");
    complete_patch_agent_request(&runtime, "guard-write-only", &req.request_id, 0, "", "").await;
    assert!(shell_task.await.unwrap().success);
}

#[tokio::test]
async fn deny_shell_only_allows_write_tools() {
    let runtime = runtime_with_agent_project("guard-shell-only");
    register_agent(
        &runtime,
        "guard-shell-only",
        None,
        ShellClientCapabilities {
            shell: true,
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("guard-shell-only");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        None,
        SessionMode::Normal,
        sessions::SessionGuards {
            deny_write_tools: false,
            deny_shell_tools: true,
        },
    );
    let bootstrap = auth_context(None, true);

    let denied = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: project.clone(),
                command: "echo blocked".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: Some(30),
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["guard"], "deny_shell_tools");

    let write_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::WriteProjectFile {
                        project,
                        path: "allowed.txt".to_string(),
                        content: "x".to_string(),
                        session_id: Some(session_id),
                        overwrite: None,
                        expected_sha256: None,
                        expected_content_prefix: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "guard-shell-only")
        .await
        .expect("write_project_file should be allowed when deny_write_tools=false");
    assert_eq!(req.kind, "file_write_project_file");
    complete_patch_agent_request(
        &runtime,
        "guard-shell-only",
        &req.request_id,
        0,
        r#"{"path":"allowed.txt","created":true,"overwritten":false,"bytes_written":1,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","warning":null}"#,
        "",
    )
    .await;
    assert!(write_task.await.unwrap().success);
}

#[test]
fn project_tool_schemas_include_optional_session_id() {
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
    let start_session = spec_named(&specs, "start_session");
    assert_eq!(
        start_session.input_schema["properties"]["mode"]["enum"],
        json!(["normal", "read_only"])
    );
    assert!(start_session.input_schema["properties"]
        .get("deny_write_tools")
        .is_some());
    assert!(start_session.input_schema["properties"]
        .get("deny_shell_tools")
        .is_some());
    assert!(
        start_session.output_schema["properties"]["output"]["properties"]
            .get("mode")
            .is_some()
    );
    assert!(
        start_session.output_schema["properties"]["output"]["properties"]
            .get("guards")
            .is_some()
    );
    let session_summary = spec_named(&specs, "session_summary");
    assert!(
        session_summary.output_schema["properties"]["output"]["properties"]
            .get("mode")
            .is_some()
    );
    assert!(
        session_summary.output_schema["properties"]["output"]["properties"]
            .get("guards")
            .is_some()
    );
    for name in [
        "read_file",
        "run_shell",
        "write_project_file",
        "replace_line_range",
        "git_status",
        "git_log",
        "show_changes",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.input_schema["properties"].get("session_id").is_some(),
            "{name} schema missing session_id"
        );
        assert!(
            spec.input_schema["properties"]
                .get("allow_cross_project_session")
                .is_some(),
            "{name} schema missing allow_cross_project_session"
        );
        assert!(
            !spec.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "session_id"),
            "{name} schema must not require session_id"
        );
        assert!(
            !spec.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "allow_cross_project_session"),
            "{name} schema must not require allow_cross_project_session"
        );
    }
    for name in ["read_file", "run_shell", "write_project_file"] {
        let spec = spec_named(&specs, name);
        assert!(spec.output_schema["properties"]["output"]["properties"]
            .get("session_recorded")
            .is_some());
        assert!(spec.output_schema["properties"]["output"]["properties"]
            .get("session_event_id")
            .is_some());
        let session_hint =
            &spec.output_schema["properties"]["output"]["properties"]["session_hint"];
        assert_eq!(session_hint["type"], "object");
        assert_eq!(
            session_hint["properties"]["suggested_next_tool"]["enum"],
            json!(["session_discussion_summary"])
        );
    }
}
