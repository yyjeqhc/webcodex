//! Sessions tests for tool_runtime.

use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{ShellAgentResultRequest, ShellClientCapabilities};
use serde_json::{json, Value};
use std::fs;

#[tokio::test]
async fn read_file_with_session_id_records_event_without_content() {
    let runtime = runtime_with_agent_project("telemetry-read");
    register_agent(
        &runtime,
        "telemetry-read",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("telemetry-read");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("read telemetry".to_string()));
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
                        with_line_numbers: Some(true),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "telemetry-read", "inst")
        .await
        .expect("read_file should enqueue an agent request");
    assert_eq!(req.kind, "file_read");
    complete_patch_agent_request(
        &runtime,
        "telemetry-read",
        &req.request_id,
        0,
        "secret line\nsecond\n",
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    assert_eq!(result.output["session_id"], session.session_id);
    assert!(result.output["session_event_id"].as_str().is_some());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.read_like, 1);
    let event = finished_event(&summary, "read_file");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    assert!(event.read_like);
    assert!(!event.write_like);
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains("secret line"),
        "session event leaked read_file content: {serialized}"
    );
}

#[tokio::test]
async fn git_status_with_session_id_records_git_read_event() {
    let runtime = runtime_with_agent_project("telemetry-git");
    let mut caps = ShellClientCapabilities::default();
    caps.git = true;
    caps.shell = false;
    register_agent(&runtime, "telemetry-git", None, caps).await;
    let project = agent_test_project_id("telemetry-git");
    let session = runtime.sessions.start_session(None, None);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitStatus {
                        project,
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-git")
        .await
        .expect("git_status should enqueue an agent shell request");
    complete_patch_agent_request(&runtime, "telemetry-git", &req.request_id, 0, "", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    assert_eq!(summary.counts.git_like, 1);
    let event = finished_event(&summary, "git_status");
    assert!(event.git_like);
    assert!(event.read_like);
}

#[tokio::test]
async fn git_log_parses_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    commit_file(root, "a.txt", "two\n", "second commit");
    let stdout = git_log_stdout(root, 20, 0);
    let runtime = runtime_with_agent_project("git-log-parse");
    let mut caps = ShellClientCapabilities::default();
    caps.git = true;
    register_agent(&runtime, "git-log-parse", None, caps).await;
    let project = agent_test_project_id("git-log-parse");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        limit: None,
                        skip: None,
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "git-log-parse")
        .await
        .expect("git_log should enqueue an agent shell request");
    assert!(req.command.contains("git log"));
    assert!(req.command.contains("-n 21"));
    complete_patch_agent_request(&runtime, "git-log-parse", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["limit"], 20);
    assert_eq!(result.output["skip"], 0);
    assert_eq!(result.output["count"], 2);
    let commits = result.output["commits"].as_array().unwrap();
    assert_eq!(commits[0]["subject"], "second commit");
    assert!(commits[0]["hash"].as_str().is_some_and(|s| s.len() >= 40));
    assert!(commits[0]["short_hash"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(commits[0]["author_date"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert_eq!(commits[0]["author_name"], "WebCodex Test");
    assert_eq!(commits[0]["author_email"], "webcodex-test@example.com");
    assert!(commits[0]["refs"].as_array().is_some());
}

#[tokio::test]
async fn git_log_limit_and_skip_returns_second_recent_and_truncated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    commit_file(root, "a.txt", "two\n", "second commit");
    commit_file(root, "a.txt", "three\n", "third commit");
    let stdout = git_log_stdout(root, 1, 1);
    let runtime = runtime_with_agent_project("git-log-page");
    let mut caps = ShellClientCapabilities::default();
    caps.git = true;
    register_agent(&runtime, "git-log-page", None, caps).await;
    let project = agent_test_project_id("git-log-page");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        limit: Some(1),
                        skip: Some(1),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "git-log-page")
        .await
        .expect("git_log should enqueue an agent shell request");
    assert!(req.command.contains("-n 2"));
    assert!(req.command.contains("--skip 1"));
    complete_patch_agent_request(&runtime, "git-log-page", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["limit"], 1);
    assert_eq!(result.output["skip"], 1);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["truncated"], true);
    let commits = result.output["commits"].as_array().unwrap();
    assert_eq!(commits[0]["subject"], "second commit");
}

#[tokio::test]
async fn git_log_unknown_project_and_unknown_session_are_structured_errors() {
    let runtime = runtime_with_agent_project("git-log-errors");
    let mut caps = ShellClientCapabilities::default();
    caps.git = true;
    register_agent(&runtime, "git-log-errors", None, caps).await;
    let project = agent_test_project_id("git-log-errors");

    let unknown_project = runtime
        .dispatch(ToolCall::GitLog {
            project: "ghost".to_string(),
            limit: None,
            skip: None,
            session_id: None,
        })
        .await;
    assert!(!unknown_project.success);
    assert_eq!(unknown_project.output["error_kind"], "unknown_project");

    let unknown_session = runtime
        .dispatch(ToolCall::GitLog {
            project,
            limit: None,
            skip: None,
            session_id: Some("wc_sess_missing".to_string()),
        })
        .await;
    assert!(!unknown_session.success);
    assert_eq!(unknown_session.output["error_kind"], "unknown_session_id");
}

#[tokio::test]
async fn git_log_read_only_session_allowed_and_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    let stdout = git_log_stdout(root, 5, 0);
    let runtime = runtime_with_agent_project("git-log-readonly");
    let mut caps = ShellClientCapabilities::default();
    caps.git = true;
    register_agent(&runtime, "git-log-readonly", None, caps).await;
    let project = agent_test_project_id("git-log-readonly");
    let session_result = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "start_session",
                json!({"project": project, "mode": "read_only"}),
            )
            .unwrap(),
        )
        .await;
    assert!(session_result.success, "{:?}", session_result.error);
    let session_id = session_result.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        limit: Some(5),
                        skip: Some(0),
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "git-log-readonly")
        .await
        .expect("git_log should be allowed in read_only session");
    complete_patch_agent_request(
        &runtime,
        "git-log-readonly",
        &req.request_id,
        0,
        &stdout,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    let summary = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    assert_eq!(summary.counts.git_like, 1);
    let event = finished_event(&summary, "git_log");
    assert!(event.read_like);
    assert!(event.git_like);
    assert!(!event.write_like);
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
async fn no_session_id_keeps_old_behavior_without_telemetry_hint() {
    let runtime = runtime_with_agent_project("telemetry-nosession");
    register_agent(
        &runtime,
        "telemetry-nosession",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("telemetry-nosession");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "telemetry-nosession", "inst")
        .await
        .expect("read_file should enqueue without session_id");
    complete_patch_agent_request(
        &runtime,
        "telemetry-nosession",
        &req.request_id,
        0,
        "hello\n",
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["content"], "hello");
    assert!(result.output.get("session_recorded").is_none());
}

#[tokio::test]
async fn bind_current_session_success_and_lookup() {
    let runtime = runtime_with_agent_project("current-bind");
    register_agent(
        &runtime,
        "current-bind",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("current-bind");
    let bootstrap = auth_context(None, true);
    let started = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_session",
                json!({"project": project, "title": "current"}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();

    let bound = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": session_id}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(bound.success, "{:?}", bound.error);
    assert_eq!(bound.output["bound"], true);
    assert_eq!(bound.output["session_id"], session_id);
    assert_eq!(bound.output["resolved_project"], project);

    let current = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(current.success, "{:?}", current.error);
    assert_eq!(current.output["found"], true);
    assert_eq!(current.output["session_id"], session_id);
}

#[tokio::test]
async fn bind_current_session_rejects_unknown_session() {
    let runtime = runtime_with_agent_project("current-unknown");
    register_agent(
        &runtime,
        "current-unknown",
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id("current-unknown");
    let result = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": "wc_sess_missing"}),
            )
            .unwrap(),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_session_id");
}

#[tokio::test]
async fn bind_current_session_rejects_project_mismatch() {
    let runtime = runtime_with_resolver_projects().await;
    let project_a = "agent:workstation:my-repo";
    let project_b = "agent:workstation:other-repo";
    let session = runtime
        .sessions
        .start_session(Some(project_a.to_string()), Some("a".to_string()));
    let result = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project_b, "session_id": session.session_id}),
            )
            .unwrap(),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    assert_eq!(result.output["session_project"], project_a);
    assert_eq!(result.output["resolved_project"], project_b);
}

#[tokio::test]
async fn unbind_current_session_removes_binding_and_is_idempotent() {
    let runtime = runtime_with_agent_project("current-unbind");
    register_agent(
        &runtime,
        "current-unbind",
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id("current-unbind");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("unbind".to_string()));
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

    let first = runtime
        .dispatch(
            ToolCall::from_tool_name("unbind_current_session", json!({"project": project}))
                .unwrap(),
        )
        .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["unbound"], true);
    assert_eq!(first.output["had_binding"], true);

    let current = runtime
        .dispatch(ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap())
        .await;
    assert!(current.success, "{:?}", current.error);
    assert_eq!(current.output["found"], false);

    let second = runtime
        .dispatch(
            ToolCall::from_tool_name("unbind_current_session", json!({"project": project}))
                .unwrap(),
        )
        .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["had_binding"], false);
}

#[tokio::test]
async fn bound_current_session_records_project_tool_without_session_id() {
    let runtime = runtime_with_agent_project("current-read");
    register_agent(
        &runtime,
        "current-read",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("current-read");
    let bootstrap = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("current read".to_string()));
    let bind = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": session.session_id}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let bootstrap = bootstrap.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "current-read", "inst")
        .await
        .expect("read_file should enqueue with current session");
    complete_patch_agent_request(&runtime, "current-read", &req.request_id, 0, "hello\n", "").await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    assert_eq!(result.output["session_id"], session.session_id);

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(
        finished_event(&summary, "read_file").status.as_deref(),
        Some("succeeded")
    );
}

#[tokio::test]
async fn generic_tool_call_uses_bound_current_session_without_session_id() {
    let runtime = runtime_with_agent_project("current-generic");
    register_agent(
        &runtime,
        "current-generic",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("current-generic");
    let bootstrap = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("generic current".to_string()));
    let bind = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": session.session_id}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let bootstrap = bootstrap.clone();
        async move {
            runtime
                .call_tool_with_context(
                    kernel::ToolCallRequest {
                        tool_name: "read_file".to_string(),
                        arguments: json!({
                            "project": project,
                            "path": "README.md",
                            "limit": 1
                        }),
                    },
                    kernel::ToolCallContext {
                        transport: kernel::ToolTransport::Api,
                        session_id: None,
                        auth: Some(&bootstrap),
                        record_oauth_scope_denials: true,
                    },
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "current-generic", "inst")
        .await
        .expect("generic read_file should enqueue with current session");
    complete_patch_agent_request(
        &runtime,
        "current-generic",
        &req.request_id,
        0,
        "hello\n",
        "",
    )
    .await;
    let outcome = task.await.unwrap();
    assert!(outcome.success);
    let result = outcome.result.unwrap();
    assert_eq!(result.output["session_recorded"], true);
    assert_eq!(result.output["session_id"], session.session_id);
    assert_eq!(
        runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap()
            .counts
            .tool_calls,
        1
    );
}

#[tokio::test]
async fn explicit_session_id_wins_over_current_session() {
    let runtime = runtime_with_agent_project("current-explicit");
    register_agent(
        &runtime,
        "current-explicit",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("current-explicit");
    let bootstrap = auth_context(None, true);
    let current = runtime
        .sessions
        .start_session(Some(project.clone()), Some("current".to_string()));
    let explicit = runtime
        .sessions
        .start_session(Some(project.clone()), Some("explicit".to_string()));
    let bind = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": current.session_id}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let explicit_id = explicit.session_id.clone();
        let bootstrap = bootstrap.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: Some(explicit_id),
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "current-explicit", "inst")
        .await
        .expect("read_file should enqueue with explicit session");
    complete_patch_agent_request(
        &runtime,
        "current-explicit",
        &req.request_id,
        0,
        "hello\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_id"], explicit.session_id);
    assert_eq!(
        runtime
            .sessions
            .summary(&current.session_id, Some(20))
            .unwrap()
            .counts
            .tool_calls,
        0
    );
    assert_eq!(
        runtime
            .sessions
            .summary(&explicit.session_id, Some(20))
            .unwrap()
            .counts
            .tool_calls,
        1
    );
}

#[tokio::test]
async fn read_only_current_session_guard_blocks_write_before_enqueue() {
    let runtime = runtime_with_agent_project("current-guard");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
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
async fn stale_current_session_is_cleared_and_project_tool_runs_without_session() {
    let runtime = runtime_with_agent_project("current-stale");
    register_agent(
        &runtime,
        "current-stale",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("current-stale");
    let bootstrap = auth_context(None, true);
    let stale = runtime
        .sessions
        .start_session(Some(project.clone()), Some("stale".to_string()));
    let bind = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": stale.session_id}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);
    for idx in 0..101 {
        runtime
            .sessions
            .start_session(Some(project.clone()), Some(format!("evict-{idx}")));
    }
    assert!(runtime.sessions.summary(&stale.session_id, None).is_none());

    let current = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(current.success, "{:?}", current.error);
    assert_eq!(current.output["found"], false);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let bootstrap = bootstrap.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "current-stale", "inst")
        .await
        .expect("stale current session should not block no-session call");
    complete_patch_agent_request(&runtime, "current-stale", &req.request_id, 0, "hello\n", "")
        .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_recorded").is_none());
}

#[tokio::test]
async fn current_session_binding_is_principal_and_transport_isolated() {
    let runtime = runtime_with_agent_project("current-isolation");
    register_agent(
        &runtime,
        "current-isolation",
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id("current-isolation");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("isolated".to_string()));
    let alice = auth_context(Some("alice"), false);
    let bob = auth_context(Some("bob"), false);
    let bind = runtime
        .dispatch_with_auth_transport(
            ToolCall::from_tool_name(
                "bind_current_session",
                json!({"project": project, "session_id": session.session_id}),
            )
            .unwrap(),
            Some(&alice),
            sessions::SessionTransport::Api,
        )
        .await;
    assert!(bind.success, "{:?}", bind.error);

    let alice_api = runtime
        .dispatch_with_auth_transport(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&alice),
            sessions::SessionTransport::Api,
        )
        .await;
    assert_eq!(alice_api.output["found"], true);

    let bob_api = runtime
        .dispatch_with_auth_transport(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&bob),
            sessions::SessionTransport::Api,
        )
        .await;
    assert_eq!(bob_api.output["found"], false);

    let alice_mcp = runtime
        .dispatch_with_auth_transport(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&alice),
            sessions::SessionTransport::Mcp,
        )
        .await;
    assert_eq!(alice_mcp.output["found"], false);
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
async fn read_only_session_rejects_run_shell_before_agent_enqueue() {
    let runtime = runtime_with_agent_project("guard-shell");
    register_agent(
        &runtime,
        "guard-shell",
        None,
        ShellClientCapabilities {
            shell: true,
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
    complete_patch_agent_request(
        &runtime,
        "guard-shell-only",
        &req.request_id,
        0,
        r#"{"path":"allowed.txt","bytes_written":1,"sha256":"abc","changed":true}"#,
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
            !spec.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "session_id"),
            "{name} schema must not require session_id"
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
    }
}

#[tokio::test]
async fn project_resolver_resolves_full_id() {
    let runtime = runtime_with_resolver_projects().await;
    let resolved = runtime
        .resolve_project_input("agent:workstation:my-repo")
        .await
        .unwrap();
    assert_eq!(resolved.resolved_id, "agent:workstation:my-repo");
    assert_eq!(resolved.config.agent_client_id().unwrap(), "workstation");
    assert_eq!(resolved.config.path, "/root/git/workstation-my-repo");
}

#[tokio::test]
async fn project_resolver_resolves_client_project_shorthand() {
    let runtime = runtime_with_resolver_projects().await;
    let resolved = runtime
        .resolve_project_input("workstation:my-repo")
        .await
        .unwrap();
    assert_eq!(resolved.resolved_id, "agent:workstation:my-repo");
}

#[tokio::test]
async fn project_resolver_resolves_unique_short_id() {
    let runtime = runtime_with_resolver_projects().await;
    let resolved = runtime.resolve_project_input("other-repo").await.unwrap();
    assert_eq!(resolved.resolved_id, "agent:workstation:other-repo");
}

#[tokio::test]
async fn project_resolver_ambiguous_short_id_returns_candidates() {
    let runtime = runtime_with_resolver_projects().await;
    let err = runtime.resolve_project_input("my-repo").await.unwrap_err();
    assert_eq!(err.kind, ProjectResolverErrorKind::AmbiguousProject);
    assert_eq!(err.project, "my-repo");
    let ids: Vec<String> = err
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            "agent:laptop:my-repo".to_string(),
            "agent:workstation:my-repo".to_string(),
        ]
    );
}

#[tokio::test]
async fn project_resolver_unknown_id_returns_candidates() {
    let runtime = runtime_with_resolver_projects().await;
    let err = runtime
        .resolve_project_input("missing-repo")
        .await
        .unwrap_err();
    assert_eq!(err.kind, ProjectResolverErrorKind::UnknownProject);
    assert_eq!(err.project, "missing-repo");
    assert!(err.candidates.len() >= 3);
    assert!(err
        .candidates
        .iter()
        .any(|candidate| candidate.id == "agent:workstation:other-repo"));
}

#[tokio::test]
async fn start_session_without_project_is_allowed() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: None,
                title: Some("probe".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], Value::Null);
    assert_eq!(result.output["project_input"], Value::Null);
    assert_eq!(result.output["resolved_project"], Value::Null);
    // No project => project_instructions must be null (not loaded=false).
    assert_eq!(result.output["project_instructions"], Value::Null);
}

#[tokio::test]
async fn start_session_valid_full_id_stores_resolved_project() {
    let runtime = runtime_with_resolver_projects().await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some("agent:workstation:my-repo".to_string()),
                title: Some("probe".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], "agent:workstation:my-repo");
    assert_eq!(result.output["project_input"], "agent:workstation:my-repo");
    assert_eq!(
        result.output["resolved_project"],
        "agent:workstation:my-repo"
    );
}

#[tokio::test]
async fn start_session_valid_short_id_stores_resolved_project() {
    let runtime = runtime_with_resolver_projects().await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some("other-repo".to_string()),
                title: Some("probe".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], "agent:workstation:other-repo");
    assert_eq!(result.output["project_input"], "other-repo");
    assert_eq!(
        result.output["resolved_project"],
        "agent:workstation:other-repo"
    );
}

#[tokio::test]
async fn start_session_ambiguous_project_fails_with_candidates() {
    let runtime = runtime_with_resolver_projects().await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some("my-repo".to_string()),
                title: Some("probe".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            None,
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "ambiguous_project");
    let candidates = result.output["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["id"], "agent:laptop:my-repo");
    assert_eq!(candidates[1]["id"], "agent:workstation:my-repo");
}

#[tokio::test]
async fn start_session_unknown_project_fails_with_candidates() {
    let runtime = runtime_with_resolver_projects().await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some("missing-repo".to_string()),
                title: Some("probe".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            None,
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");
    assert_eq!(result.output["project"], "missing-repo");
    assert!(result.output["candidates"].as_array().unwrap().len() >= 3);
}

#[tokio::test]
async fn start_session_without_project_instructions_when_no_candidate_exists() {
    // The agent is registered (so the project resolves) but no instruction
    // candidate file exists on the agent host. Every candidate file_read is
    // answered with a not-found error, the loader skips them all, and
    // start_session still succeeds with project_instructions.loaded=false.
    let runtime = runtime_with_agent_project("instr-empty");
    register_agent(
        &runtime,
        "instr-empty",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-empty");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("empty".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    // Drive every candidate file_read in order; each fails with not-found.
    for expected_path in super::super::project_instructions::INSTRUCTION_CANDIDATE_PATHS {
        let req = next_agent_request_for_instance(&runtime, "instr-empty", "inst")
            .await
            .expect("each candidate should enqueue an agent file_read");
        assert_eq!(req.kind, "file_read");
        assert_eq!(
            req.path.as_deref(),
            Some(*expected_path),
            "candidates must be tried in fixed order"
        );
        complete_patch_agent_request(
            &runtime,
            "instr-empty",
            &req.request_id,
            1,
            "",
            "no such file or directory",
        )
        .await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], false);
    assert!(pi["files"].as_array().unwrap().is_empty());
    assert_eq!(pi["truncated"], false);
    assert_eq!(pi["max_total_chars"], 32 * 1024);
    assert_eq!(
        pi["candidate_paths"].as_array().unwrap().len(),
        super::super::project_instructions::INSTRUCTION_CANDIDATE_PATHS.len()
    );
    assert!(pi["note"]
        .as_str()
        .unwrap()
        .contains("project-local guidance only"));
}

#[tokio::test]
async fn start_session_loads_agents_md_from_agent_project() {
    let runtime = runtime_with_agent_project("instr-loader");
    register_agent(
        &runtime,
        "instr-loader",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-loader");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("load instructions".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    // The loader tries AGENTS.md first; drive that single file_read.
    let req = next_agent_request_for_instance(&runtime, "instr-loader", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    assert_eq!(req.kind, "file_read");
    assert_eq!(req.path.as_deref(), Some("AGENTS.md"));
    complete_patch_agent_request(
        &runtime,
        "instr-loader",
        &req.request_id,
        0,
        "# Agent Guide\n\nRespect the AGENTS.md rules.\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    let files = pi["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "AGENTS.md");
    assert_eq!(files[0]["truncated"], false);
    assert!(files[0]["read_more"].is_null());
    assert!(
        files[0]["content"]
            .as_str()
            .unwrap()
            .contains("Respect the AGENTS.md rules."),
        "content should carry AGENTS.md body"
    );
    assert_eq!(files[0]["limit"], 400);
    assert_eq!(files[0]["start_line"], 1);
    assert!(pi["note"]
        .as_str()
        .unwrap()
        .contains("project-local guidance only"));
}

#[tokio::test]
async fn start_session_truncates_large_instruction_file() {
    let runtime = runtime_with_agent_project("instr-trunc");
    register_agent(
        &runtime,
        "instr-trunc",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-trunc");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("trunc".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "instr-trunc", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    assert_eq!(req.kind, "file_read");
    assert_eq!(req.path.as_deref(), Some("AGENTS.md"));
    // Simulate the agent returning MAX_LINES_PER_FILE + 1 lines for a file
    // that is larger than the per-file line cap.
    let body = (0..(super::super::project_instructions::MAX_LINES_PER_FILE + 1))
        .map(|i| format!("rule line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    complete_patch_agent_request(&runtime, "instr-trunc", &req.request_id, 0, &body, "").await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    assert_eq!(pi["truncated"], true);
    let file = &pi["files"][0];
    assert_eq!(file["truncated"], true);
    let read_more = &file["read_more"];
    assert_eq!(read_more["path"], "AGENTS.md");
    assert_eq!(
        read_more["start_line"],
        super::super::project_instructions::MAX_LINES_PER_FILE + 1
    );
    assert_eq!(
        read_more["limit"],
        super::super::project_instructions::MAX_LINES_PER_FILE
    );
    // Kept content is capped at MAX_LINES_PER_FILE lines.
    assert_eq!(
        file["content"].as_str().unwrap().lines().count(),
        super::super::project_instructions::MAX_LINES_PER_FILE
    );
    assert_eq!(
        file["limit"],
        super::super::project_instructions::MAX_LINES_PER_FILE
    );
}

#[tokio::test]
async fn session_summary_returns_project_instructions_without_content() {
    let runtime = runtime_with_agent_project("instr-summary");
    register_agent(
        &runtime,
        "instr-summary",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-summary");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("summary".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "instr-summary", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    complete_patch_agent_request(
        &runtime,
        "instr-summary",
        &req.request_id,
        0,
        "secret project rule that must not leak into session_summary\n",
        "",
    )
    .await;
    let start_result = task.await.unwrap();
    assert!(start_result.success, "{:?}", start_result.error);
    let session_id = start_result.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let summary_result = runtime
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id: session_id.clone(),
                limit: Some(20),
            },
            None,
        )
        .await;
    assert!(summary_result.success, "{:?}", summary_result.error);
    let pi = &summary_result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    let files = pi["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "AGENTS.md");
    // Summary-only: content must NOT be present on the file entry.
    assert!(
        files[0].get("content").is_none(),
        "session_summary project_instructions file must be summary-only"
    );
    assert!(files[0]["chars"].as_u64().is_some());
    assert_eq!(files[0]["truncated"], false);
    assert_eq!(
        pi["total_chars"],
        start_result.output["project_instructions"]["total_chars"]
    );
    // The instruction body must not leak anywhere in the summary output.
    let serialized = serde_json::to_string(&summary_result.output).unwrap();
    assert!(
        !serialized.contains("secret project rule"),
        "session_summary leaked instruction content: {serialized}"
    );
}

#[tokio::test]
async fn load_project_instructions_first_match_wins_locally() {
    // Direct unit-style test of the loader against a local project root so
    // the local read path and first-match-wins ordering are exercised
    // without driving an agent.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("CLAUDE.md"),
        "# Claude\n\nclaude-local rules\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("agents.md"),
        "# lower agents\n\nignored because CLAUDE.md wins earlier? no, AGENTS.md is first\n",
    )
    .unwrap();
    // Note: AGENTS.md is absent, agents.md is present (2nd candidate),
    // CLAUDE.md is present (3rd candidate). First match wins => agents.md.
    let config = local_project_config(&dir.path().to_string_lossy());
    let runtime = test_runtime();
    let snapshot = runtime.load_project_instructions(&config).await;
    assert!(snapshot.loaded);
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "agents.md");
    assert!(snapshot.files[0].content.contains("lower agents"));
}

#[tokio::test]
async fn load_project_instructions_empty_when_no_candidates_exist() {
    let dir = tempfile::tempdir().unwrap();
    let config = local_project_config(&dir.path().to_string_lossy());
    let runtime = test_runtime();
    let snapshot = runtime.load_project_instructions(&config).await;
    assert!(!snapshot.loaded);
    assert!(snapshot.files.is_empty());
}

#[tokio::test]
async fn read_file_accepts_unique_short_id() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: "other-repo".to_string(),
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "workstation")
        .await
        .expect("read_file should enqueue an agent file_read request");
    assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "workstation".to_string(),
            agent_instance_id: "inst-workstation".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn git_status_accepts_unique_short_id() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GitStatus {
                        project: "other-repo".to_string(),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "workstation")
        .await
        .expect("git_status should enqueue an agent shell request");
    assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "workstation".to_string(),
            agent_instance_id: "inst-workstation".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(String::new()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn ambiguous_short_id_returns_candidates_for_project_tools() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "my-repo".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "ambiguous_project");
    assert_eq!(result.output["project"], "my-repo");
    assert_eq!(result.output["candidates"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn full_id_remains_compatible_for_project_tools() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: "agent:workstation:other-repo".to_string(),
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "workstation")
        .await
        .expect("full id should still enqueue an agent request");
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "workstation".to_string(),
            agent_instance_id: "inst-workstation".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}
