//! Core session lifecycle and summary tests.

use super::super::sessions::{SessionMessageKind, SessionMessagePriority};
use super::super::types::*;
use super::super::{ToolCall, ToolResult, ToolRuntime};
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::Value;

async fn post_session_message(
    runtime: &ToolRuntime,
    session_id: &str,
    kind: SessionMessageKind,
    priority: SessionMessagePriority,
    message: &str,
) -> String {
    let result = runtime
        .dispatch(ToolCall::PostSessionMessage {
            session_id: session_id.to_string(),
            kind,
            message: message.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    result.output["message_id"].as_str().unwrap().to_string()
}

async fn read_agent_file_for_session(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    session_id: Option<String>,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(runtime, client_id, "inst")
        .await
        .expect("read_file should enqueue an agent request");
    complete_patch_agent_request(runtime, client_id, &req.request_id, 0, "hello\n", "").await;
    task.await.unwrap()
}

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
    assert!(result.output.get("session_hint").is_none());
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
    assert!(result.output.get("session_hint").is_none());
}

#[tokio::test]
async fn session_inbox_hint_reports_open_guidance_without_text() {
    let client_id = "inbox-guidance";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("hint".to_string()));
    let message_text = "sensitive guidance body should not be in the hint";
    post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Guidance,
        SessionMessagePriority::High,
        message_text,
    )
    .await;

    let result = read_agent_file_for_session(
        &runtime,
        client_id,
        project,
        Some(session.session_id.clone()),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let hint = &result.output["session_hint"];
    assert_eq!(hint["has_open_messages"], true);
    assert_eq!(hint["open_counts"]["guidance"], 1);
    assert_eq!(hint["open_counts"]["question"], 0);
    assert_eq!(hint["open_counts"]["todo"], 0);
    assert_eq!(hint["open_counts"]["risk"], 0);
    assert_eq!(hint["highest_priority"], "high");
    assert_eq!(hint["suggested_next_tool"], "session_discussion_summary");
    let serialized_hint = serde_json::to_string(hint).unwrap();
    assert!(
        !serialized_hint.contains(message_text),
        "session_hint leaked message text: {serialized_hint}"
    );
}

#[tokio::test]
async fn session_inbox_hint_counts_question_todo_and_risk() {
    let client_id = "inbox-counts";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("counts".to_string()));
    post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Todo,
        SessionMessagePriority::Low,
        "todo body",
    )
    .await;
    post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Question,
        SessionMessagePriority::Normal,
        "question body",
    )
    .await;
    post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Risk,
        SessionMessagePriority::Low,
        "risk body",
    )
    .await;
    post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Note,
        SessionMessagePriority::High,
        "note body must not affect hint",
    )
    .await;

    let result = read_agent_file_for_session(
        &runtime,
        client_id,
        project,
        Some(session.session_id.clone()),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let hint = &result.output["session_hint"];
    assert_eq!(hint["has_open_messages"], true);
    assert_eq!(hint["open_counts"]["guidance"], 0);
    assert_eq!(hint["open_counts"]["question"], 1);
    assert_eq!(hint["open_counts"]["todo"], 1);
    assert_eq!(hint["open_counts"]["risk"], 1);
    assert_eq!(hint["highest_priority"], "normal");
    assert_eq!(hint["suggested_next_tool"], "session_discussion_summary");
}

#[tokio::test]
async fn session_inbox_hint_disappears_after_message_resolved() {
    let client_id = "inbox-resolve";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("resolve".to_string()));
    let message_id = post_session_message(
        &runtime,
        &session.session_id,
        SessionMessageKind::Todo,
        SessionMessagePriority::High,
        "resolve me",
    )
    .await;

    let before = read_agent_file_for_session(
        &runtime,
        client_id,
        project.clone(),
        Some(session.session_id.clone()),
    )
    .await;
    assert!(before.output.get("session_hint").is_some());

    let resolved = runtime
        .dispatch(ToolCall::ResolveSessionMessage {
            session_id: session.session_id.clone(),
            message_id,
            resolution: Some("done".to_string()),
        })
        .await;
    assert!(resolved.success, "{:?}", resolved.error);

    let after = read_agent_file_for_session(
        &runtime,
        client_id,
        project,
        Some(session.session_id.clone()),
    )
    .await;
    assert!(after.success, "{:?}", after.error);
    assert!(after.output.get("session_hint").is_none());
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
