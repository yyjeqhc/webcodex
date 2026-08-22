//! Workflow Session collaboration tests: coordinator/worker isolation, provenance, and authority.

use super::super::kernel::{HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::session_context::current_session_key;
use super::super::sessions::{
    self, PostSessionMessageInput, SessionMessageKind, SessionMessagePriority,
};
use super::super::ToolRuntime;
use super::support::*;
use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::{json, Value};

async fn call_with_recorder(
    runtime: &ToolRuntime,
    tool_name: &str,
    arguments: Value,
    recorder_session_id: Option<&str>,
    auth: &AuthContext,
    window: Option<&ClientWindow>,
) -> super::super::ToolResult {
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: recorder_session_id,
                auth: Some(auth),
                window,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(
        outcome.error_status.is_none(),
        "unexpected transport error: {:?}",
        outcome.error_status
    );
    outcome.result.expect("tool result")
}

fn bind_worker_window(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    repository_root: &str,
    window: &ClientWindow,
    session_id: &str,
) {
    let key = current_session_key(
        Some(auth),
        sessions::SessionTransport::Api,
        project,
        repository_root,
        Some(window),
    )
    .unwrap();
    runtime
        .sessions
        .bind_current_session(key, session_id)
        .expect("bind current worker session");
}

fn tool_names(runtime: &ToolRuntime, session_id: &str) -> Vec<String> {
    runtime
        .sessions
        .summary(session_id, Some(100))
        .unwrap()
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished")
        .map(|event| event.tool_name.clone())
        .collect()
}

#[tokio::test]
async fn collaboration_two_sessions_keep_execution_history_independent_and_bind_provenance() {
    let client_id = "collaboration-runtime";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let coordinator = runtime
        .sessions
        .start_session(Some(project.clone()), Some("coordinator C".to_string()));
    let worker = runtime
        .sessions
        .start_session(Some(project.clone()), Some("worker W".to_string()));
    assert_ne!(coordinator.session_id, worker.session_id);

    let coordinator_window = ClientWindow::for_test("collaboration-coordinator-window");
    let worker_window = ClientWindow::for_test("collaboration-worker-window");
    bind_worker_window(
        &runtime,
        &auth,
        &project,
        "/tmp/agent-proj",
        &coordinator_window,
        &coordinator.session_id,
    );
    bind_worker_window(
        &runtime,
        &auth,
        &project,
        "/tmp/agent-proj",
        &worker_window,
        &worker.session_id,
    );

    let posted = call_with_recorder(
        &runtime,
        "post_session_message",
        json!({
            "session_id": coordinator.session_id,
            "kind": "todo",
            "message": "Independent review this exact synthetic change; report findings.",
            "tags": ["review"],
            "priority": "high"
        }),
        Some(&coordinator.session_id),
        &auth,
        Some(&coordinator_window),
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);
    let todo_id = posted.output["message_id"].as_str().unwrap().to_string();

    let handoff = call_with_recorder(
        &runtime,
        "session_handoff_summary",
        json!({
            "session_id": coordinator.session_id,
            "include_workspace": false,
            "include_checkpoints": false,
            "include_validation": false,
            "summary_only": true
        }),
        Some(&worker.session_id),
        &auth,
        Some(&worker_window),
    )
    .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["collaboration"]["open_todo_count"], 1);
    assert_eq!(
        handoff.output["collaboration"]["high_priority_open_todos"][0]["message_id"],
        todo_id
    );

    let exact = call_with_recorder(
        &runtime,
        "list_session_messages",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo_id,
            "kind": "todo",
            "status": "open",
            "limit": 1
        }),
        Some(&worker.session_id),
        &auth,
        Some(&worker_window),
    )
    .await;
    assert!(exact.success, "{:?}", exact.error);
    assert_eq!(exact.output["messages"].as_array().unwrap().len(), 1);

    let completed = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo_id,
            "answer": "No findings. Revalidated the authoritative synthetic source after review.",
            "completion_key": "worker-review-v1",
            "tags": ["review", "done"],
            "priority": "normal",
            "author_session_id": "wc_sess_forged_should_be_ignored"
        }),
        Some(&worker.session_id),
        &auth,
        Some(&worker_window),
    )
    .await;
    assert!(completed.success, "{:?}", completed.error);
    assert_eq!(
        completed.output["answer"]["author_session_id"],
        worker.session_id
    );
    assert_eq!(completed.output["answer"]["reply_to"], todo_id);
    assert_eq!(
        completed.output["todo"]["resolved_by_message_id"],
        completed.output["answer_message_id"]
    );

    let replay = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo_id,
            "answer": "No findings. Revalidated the authoritative synthetic source after review.",
            "completion_key": "worker-review-v1",
            "tags": ["review", "done"],
            "priority": "normal"
        }),
        Some(&worker.session_id),
        &auth,
        Some(&worker_window),
    )
    .await;
    assert!(replay.success, "{:?}", replay.error);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(
        replay.output["answer_message_id"],
        completed.output["answer_message_id"]
    );

    let reply = call_with_recorder(
        &runtime,
        "list_session_messages",
        json!({
            "session_id": coordinator.session_id,
            "reply_to": todo_id,
            "kind": "answer",
            "limit": 10
        }),
        Some(&coordinator.session_id),
        &auth,
        Some(&coordinator_window),
    )
    .await;
    assert!(reply.success, "{:?}", reply.error);
    assert_eq!(reply.output["messages"].as_array().unwrap().len(), 1);

    let todo_after = call_with_recorder(
        &runtime,
        "list_session_messages",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo_id,
            "kind": "todo",
            "status": "resolved",
            "limit": 1
        }),
        Some(&coordinator.session_id),
        &auth,
        Some(&coordinator_window),
    )
    .await;
    assert!(todo_after.success, "{:?}", todo_after.error);
    assert_eq!(todo_after.output["messages"].as_array().unwrap().len(), 1);

    let worker_tools = tool_names(&runtime, &worker.session_id);
    for expected in [
        "session_handoff_summary",
        "list_session_messages",
        "complete_session_message",
    ] {
        assert!(
            worker_tools.iter().any(|tool| tool == expected),
            "{expected}: {worker_tools:?}"
        );
    }
    let coordinator_tools = tool_names(&runtime, &coordinator.session_id);
    assert!(coordinator_tools
        .iter()
        .any(|tool| tool == "post_session_message"));
    assert!(!coordinator_tools
        .iter()
        .any(|tool| tool == "session_handoff_summary"));
    assert!(!coordinator_tools
        .iter()
        .any(|tool| tool == "complete_session_message"));
    let coordinator_events = runtime
        .sessions
        .summary(&coordinator.session_id, Some(100))
        .unwrap();
    let worker_events = runtime
        .sessions
        .summary(&worker.session_id, Some(100))
        .unwrap();
    let audit_text = format!(
        "{}\n{}",
        serde_json::to_string(&coordinator_events.events).unwrap(),
        serde_json::to_string(&worker_events.events).unwrap()
    );
    for private in [
        "Independent review this exact synthetic change; report findings.",
        "No findings. Revalidated the authoritative synthetic source after review.",
        "worker-review-v1",
        "wc_sess_forged_should_be_ignored",
        "collaboration-worker-window",
    ] {
        assert!(
            !audit_text.contains(private),
            "tool audit duplicated collaboration body or private provenance: {private}"
        );
    }

    let discussion = runtime
        .sessions
        .discussion_summary(&coordinator.session_id, Some(20))
        .unwrap();
    assert_eq!(discussion.counts.open_todos, 0);
    assert_eq!(discussion.recent_answers.len(), 1);
    assert_eq!(discussion.recent_completions.len(), 1);
    assert_eq!(
        discussion.recent_completions[0]
            .author_session_id
            .as_deref(),
        Some(worker.session_id.as_str())
    );
}

#[tokio::test]
async fn collaboration_completion_without_trusted_current_binding_has_null_author() {
    let client_id = "collaboration-null-author";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let coordinator = runtime.sessions.start_session(Some(project), None);
    let worker = runtime
        .sessions
        .start_session(Some(agent_test_project_id(client_id)), None);
    let todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "no stable window".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();

    let result = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id,
            "answer": "done",
            "completion_key": "no-window"
        }),
        Some(&worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["answer"]["author_session_id"], Value::Null);
}

#[tokio::test]
async fn collaboration_cross_project_recorder_fails_closed_before_completion() {
    let runtime = runtime_with_resolver_projects().await;
    let auth = auth_context(None, true);
    let coordinator = runtime.sessions.start_session(
        Some("agent:workstation:my-repo".to_string()),
        Some("C".to_string()),
    );
    let worker = runtime.sessions.start_session(
        Some("agent:workstation:other-repo".to_string()),
        Some("W".to_string()),
    );
    let todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "must not cross projects".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    let window = ClientWindow::for_test("cross-project-worker");
    bind_worker_window(
        &runtime,
        &auth,
        "agent:workstation:other-repo",
        "/root/git/workstation-other-repo",
        &window,
        &worker.session_id,
    );

    let result = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id,
            "answer": "should not commit",
            "completion_key": "cross-project"
        }),
        Some(&worker.session_id),
        &auth,
        Some(&window),
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    let still_open = runtime
        .sessions
        .list_messages(
            &coordinator.session_id,
            sessions::ListSessionMessagesFilter {
                message_id: Some(todo.message_id),
                status: Some(sessions::SessionMessageStatus::Open),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(still_open.len(), 1);
    assert_eq!(
        runtime
            .sessions
            .list_messages(
                &coordinator.session_id,
                sessions::ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Answer),
                    ..Default::default()
                },
            )
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn collaboration_foreign_owner_cannot_read_or_complete_known_session_and_todo_ids() {
    let runtime = test_runtime();
    let alice = shared_key_auth_context("collaboration-owner-a");
    let bob = shared_key_auth_context("collaboration-owner-b");
    register_agent_projects_for_auth(
        &runtime,
        "alice-host",
        &alice,
        ShellClientCapabilities::default(),
        vec![named_registered_project(
            "alice-host",
            "private",
            "Private",
            "/tmp/alice-private",
            1,
        )],
    )
    .await;
    let project = "agent:alice-host:private".to_string();
    let coordinator = runtime.sessions.start_session(Some(project), None);
    let todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "known ids are not capabilities".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();

    let read = call_with_recorder(
        &runtime,
        "list_session_messages",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id
        }),
        None,
        &bob,
        None,
    )
    .await;
    assert!(!read.success);

    let complete = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id,
            "answer": "forged completion",
            "completion_key": "foreign"
        }),
        None,
        &bob,
        None,
    )
    .await;
    assert!(!complete.success);
    assert_eq!(
        runtime
            .sessions
            .list_messages(
                &coordinator.session_id,
                sessions::ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Answer),
                    ..Default::default()
                },
            )
            .unwrap()
            .len(),
        0
    );
}
