//! Git-related session tests for tool_runtime.

use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::json;

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
