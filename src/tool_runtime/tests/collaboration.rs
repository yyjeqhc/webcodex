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
use std::sync::Arc;

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

fn start_authorized_project_session(
    runtime: &ToolRuntime,
    project: &str,
    title: Option<&str>,
    auth: &AuthContext,
) -> sessions::SessionSummary {
    let fingerprint =
        super::super::session_context::workflow_session_authority_fingerprint(Some(auth))
            .expect("test authority must have a stable identity");
    runtime
        .sessions
        .start_session_with_options(
            sessions::SessionCreateOptions::new(
                Some(project.to_string()),
                title.map(str::to_string),
                super::super::SessionMode::Normal,
                sessions::SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(fingerprint)),
        )
        .unwrap()
}

#[tokio::test]
async fn observe_session_messages_collaboration_recorder_target_scope_fences() {
    let runtime = runtime_with_resolver_projects().await;
    let auth = auth_context(None, true);
    let coordinator = start_authorized_project_session(
        &runtime,
        "agent:workstation:my-repo",
        Some("observation coordinator"),
        &auth,
    );
    let same_project_worker = start_authorized_project_session(
        &runtime,
        "agent:workstation:my-repo",
        Some("same-project worker"),
        &auth,
    );
    let other_project_worker = start_authorized_project_session(
        &runtime,
        "agent:workstation:other-repo",
        Some("other-project worker"),
        &auth,
    );

    let allowed = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator.session_id}),
        Some(&same_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(allowed.success, "{:?}", allowed.error);
    assert!(allowed.output["messages"].as_array().unwrap().is_empty());
    assert!(allowed.output["observation_token"].as_str().is_some());
    let baseline_token = allowed.output["observation_token"]
        .as_str()
        .unwrap()
        .to_string();
    let wait_without_token = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator.session_id, "wait_secs": 1}),
        Some(&same_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(!wait_without_token.success);
    assert_eq!(
        wait_without_token.output["error_kind"],
        "invalid_session_message_observation_request"
    );

    let cross_project = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator.session_id}),
        Some(&other_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(!cross_project.success);
    assert_eq!(
        cross_project.output["error_kind"],
        "session_project_mismatch"
    );
    assert!(cross_project.output.get("observation_token").is_none());

    let unscoped_worker = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "unscoped observation worker"}),
        None,
        &auth,
        None,
    )
    .await;
    assert!(unscoped_worker.success, "{:?}", unscoped_worker.error);
    let unscoped_worker_id = unscoped_worker.output["session_id"].as_str().unwrap();
    let unscoped_to_scoped = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator.session_id}),
        Some(unscoped_worker_id),
        &auth,
        None,
    )
    .await;
    assert!(!unscoped_to_scoped.success);
    assert_eq!(
        unscoped_to_scoped.output["error_kind"],
        "session_project_mismatch"
    );

    let unscoped_coordinator = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "unscoped observation coordinator"}),
        None,
        &auth,
        None,
    )
    .await;
    assert!(
        unscoped_coordinator.success,
        "{:?}",
        unscoped_coordinator.error
    );
    let unscoped_coordinator_id = unscoped_coordinator.output["session_id"].as_str().unwrap();
    let scoped_to_unscoped = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": unscoped_coordinator_id}),
        Some(&same_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(!scoped_to_unscoped.success);
    assert_eq!(
        scoped_to_unscoped.output["error_kind"],
        "session_project_mismatch"
    );
    let closed = call_with_recorder(
        &runtime,
        "close_session",
        json!({"session_id": coordinator.session_id}),
        Some(&same_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(closed.success, "{:?}", closed.error);
    let closed_observation = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({
            "session_id": coordinator.session_id,
            "after_observation_token": baseline_token
        }),
        Some(&same_project_worker.session_id),
        &auth,
        None,
    )
    .await;
    assert!(closed_observation.success, "{:?}", closed_observation.error);
    assert_eq!(closed_observation.output["changed"], false);
}

#[tokio::test]
async fn observe_session_messages_collaboration_projectless_owner_and_foreign_recorder_fence() {
    let runtime = test_runtime();
    let alice = shared_key_auth_context("observation-projectless-alice");
    let bob = shared_key_auth_context("observation-projectless-bob");

    let alice_coordinator = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "Alice observation coordinator"}),
        None,
        &alice,
        None,
    )
    .await;
    let alice_worker = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "Alice observation worker"}),
        None,
        &alice,
        None,
    )
    .await;
    let bob_worker = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "Bob observation worker"}),
        None,
        &bob,
        None,
    )
    .await;
    assert!(alice_coordinator.success && alice_worker.success && bob_worker.success);
    let coordinator_id = alice_coordinator.output["session_id"].as_str().unwrap();
    let alice_worker_id = alice_worker.output["session_id"].as_str().unwrap();
    let bob_worker_id = bob_worker.output["session_id"].as_str().unwrap();

    let same_owner = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator_id}),
        Some(alice_worker_id),
        &alice,
        None,
    )
    .await;
    assert!(same_owner.success, "{:?}", same_owner.error);

    let foreign_recorder = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator_id}),
        Some(bob_worker_id),
        &alice,
        None,
    )
    .await;
    assert!(!foreign_recorder.success);
    assert_eq!(
        foreign_recorder.output["error_kind"],
        "session_authority_denied"
    );
    assert!(foreign_recorder.output.get("observation_token").is_none());

    let foreign_target = call_with_recorder(
        &runtime,
        "observe_session_messages",
        json!({"session_id": coordinator_id}),
        None,
        &bob,
        None,
    )
    .await;
    assert!(!foreign_target.success);
    assert_eq!(
        foreign_target.output["error_kind"],
        "session_authority_denied"
    );
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
    let coordinator =
        start_authorized_project_session(&runtime, &project, Some("coordinator C"), &auth);
    let worker = start_authorized_project_session(&runtime, &project, Some("worker W"), &auth);
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
async fn collaboration_completion_without_recording_or_current_binding_has_null_author() {
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
    let coordinator = start_authorized_project_session(&runtime, &project, None, &auth);
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
        None,
        &auth,
        None,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["answer"]["author_session_id"], Value::Null);
}

#[tokio::test]
async fn collaboration_current_binding_remains_author_fallback_without_recording_session() {
    let client_id = "collaboration-current-author-fallback";
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
    let coordinator =
        start_authorized_project_session(&runtime, &project, Some("coordinator"), &auth);
    let worker_current =
        start_authorized_project_session(&runtime, &project, Some("current worker"), &auth);
    let todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "exercise current binding fallback".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();
    let window = ClientWindow::for_test("current-author-fallback-window");
    bind_worker_window(
        &runtime,
        &auth,
        &project,
        "/tmp/agent-proj",
        &window,
        &worker_current.session_id,
    );

    let completed = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id,
            "answer": "done by current fallback",
            "completion_key": "current-fallback-v1"
        }),
        None,
        &auth,
        Some(&window),
    )
    .await;
    assert!(completed.success, "{:?}", completed.error);
    assert_eq!(
        completed.output["answer"]["author_session_id"],
        worker_current.session_id
    );
}

#[tokio::test]
async fn collaboration_recording_session_wins_over_different_current_window_binding() {
    let client_id = "collaboration-recorder-author";
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
    let coordinator =
        start_authorized_project_session(&runtime, &project, Some("coordinator"), &auth);
    let worker_recording =
        start_authorized_project_session(&runtime, &project, Some("worker W1"), &auth);
    let worker_current =
        start_authorized_project_session(&runtime, &project, Some("worker W2"), &auth);
    let todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "prove recorder provenance precedence".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    let window = ClientWindow::for_test("recorder-vs-current-window");
    bind_worker_window(
        &runtime,
        &auth,
        &project,
        "/tmp/agent-proj",
        &window,
        &worker_current.session_id,
    );

    let completed = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": coordinator.session_id,
            "message_id": todo.message_id,
            "answer": "completed by the recording Session",
            "completion_key": "recorder-wins-v1"
        }),
        Some(&worker_recording.session_id),
        &auth,
        Some(&window),
    )
    .await;
    assert!(completed.success, "{:?}", completed.error);
    assert_eq!(
        completed.output["answer"]["author_session_id"],
        worker_recording.session_id
    );
    assert_ne!(
        completed.output["answer"]["author_session_id"],
        worker_current.session_id
    );

    let recording_tools = tool_names(&runtime, &worker_recording.session_id);
    assert!(recording_tools
        .iter()
        .any(|tool| tool == "complete_session_message"));
    let current_tools = tool_names(&runtime, &worker_current.session_id);
    assert!(!current_tools
        .iter()
        .any(|tool| tool == "complete_session_message"));
    let coordinator_tools = tool_names(&runtime, &coordinator.session_id);
    assert!(!coordinator_tools
        .iter()
        .any(|tool| tool == "complete_session_message"));

    let discussion = runtime
        .sessions
        .discussion_summary(&coordinator.session_id, Some(10))
        .unwrap();
    assert_eq!(discussion.recent_completions.len(), 1);
    assert_eq!(
        discussion.recent_completions[0]
            .author_session_id
            .as_deref(),
        Some(worker_recording.session_id.as_str())
    );
}

#[tokio::test]
async fn collaboration_cross_project_recorder_fails_closed_before_completion() {
    let runtime = runtime_with_resolver_projects().await;
    let auth = auth_context(None, true);
    let coordinator =
        start_authorized_project_session(&runtime, "agent:workstation:my-repo", Some("C"), &auth);
    let worker = start_authorized_project_session(
        &runtime,
        "agent:workstation:other-repo",
        Some("W"),
        &auth,
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
async fn foreign_recording_session_is_denied_before_ordinary_tool_recording() {
    let runtime = test_runtime();
    let alice = shared_key_auth_context("ordinary-recorder-alice");
    let bob = shared_key_auth_context("ordinary-recorder-bob");
    let started = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "Alice recorder"}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();
    let before = runtime.sessions.summary(&session_id, Some(100)).unwrap();

    let denied = call_with_recorder(
        &runtime,
        "list_projects",
        json!({}),
        Some(&session_id),
        &bob,
        None,
    )
    .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "session_authority_denied");
    let after_denial = runtime.sessions.summary(&session_id, Some(100)).unwrap();
    assert_eq!(after_denial.events.len(), before.events.len());
    assert!(!tool_names(&runtime, &session_id)
        .iter()
        .any(|tool| tool == "list_projects"));

    let allowed = call_with_recorder(
        &runtime,
        "list_projects",
        json!({}),
        Some(&session_id),
        &alice,
        None,
    )
    .await;
    assert!(allowed.success, "{:?}", allowed.error);
    let after_allowed = runtime.sessions.summary(&session_id, Some(100)).unwrap();
    assert_eq!(after_allowed.events.len(), before.events.len() + 2);
    assert!(tool_names(&runtime, &session_id)
        .iter()
        .any(|tool| tool == "list_projects"));
}

#[tokio::test]
async fn collaboration_mixed_project_scope_fails_closed_in_both_directions() {
    let runtime = runtime_with_resolver_projects().await;
    let auth = auth_context(None, true);

    let scoped_coordinator = start_authorized_project_session(
        &runtime,
        "agent:workstation:my-repo",
        Some("scoped coordinator"),
        &auth,
    );
    let unscoped_worker = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "unscoped worker"}),
        None,
        &auth,
        None,
    )
    .await;
    assert!(unscoped_worker.success, "{:?}", unscoped_worker.error);
    let unscoped_worker_id = unscoped_worker.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let scoped_todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: scoped_coordinator.session_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "unscoped worker must not bridge into scoped coordinator".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    for (tool_name, arguments) in [
        (
            "list_session_messages",
            json!({"session_id": scoped_coordinator.session_id, "message_id": scoped_todo.message_id}),
        ),
        (
            "complete_session_message",
            json!({
                "session_id": scoped_coordinator.session_id,
                "message_id": scoped_todo.message_id,
                "answer": "must not complete",
                "completion_key": "mixed-unscoped-to-scoped"
            }),
        ),
    ] {
        let denied = call_with_recorder(
            &runtime,
            tool_name,
            arguments,
            Some(&unscoped_worker_id),
            &auth,
            None,
        )
        .await;
        assert!(!denied.success, "{tool_name} unexpectedly succeeded");
        assert_eq!(denied.output["error_kind"], "session_project_mismatch");
    }
    assert_eq!(
        runtime
            .sessions
            .list_messages(
                &scoped_coordinator.session_id,
                sessions::ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Answer),
                    ..Default::default()
                },
            )
            .unwrap()
            .len(),
        0
    );

    let unscoped_coordinator = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "unscoped coordinator"}),
        None,
        &auth,
        None,
    )
    .await;
    assert!(
        unscoped_coordinator.success,
        "{:?}",
        unscoped_coordinator.error
    );
    let unscoped_coordinator_id = unscoped_coordinator.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let scoped_worker = start_authorized_project_session(
        &runtime,
        "agent:workstation:my-repo",
        Some("scoped worker"),
        &auth,
    );
    let unscoped_todo = runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: unscoped_coordinator_id.clone(),
            kind: SessionMessageKind::Todo,
            message: "scoped worker must not bridge into unscoped coordinator".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    for (tool_name, arguments) in [
        (
            "list_session_messages",
            json!({"session_id": unscoped_coordinator_id, "message_id": unscoped_todo.message_id}),
        ),
        (
            "complete_session_message",
            json!({
                "session_id": unscoped_coordinator_id,
                "message_id": unscoped_todo.message_id,
                "answer": "must not complete",
                "completion_key": "mixed-scoped-to-unscoped"
            }),
        ),
    ] {
        let denied = call_with_recorder(
            &runtime,
            tool_name,
            arguments,
            Some(&scoped_worker.session_id),
            &auth,
            None,
        )
        .await;
        assert!(!denied.success, "{tool_name} unexpectedly succeeded");
        assert_eq!(denied.output["error_kind"], "session_project_mismatch");
    }
    assert_eq!(
        runtime
            .sessions
            .list_messages(
                &unscoped_coordinator_id,
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
async fn project_scoped_session_authority_rejects_recycled_project_identity() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let shell_clients = Arc::new(
        crate::shell_client::ShellClientRegistry::with_shared_key_limits_for_test(4, 8, 1),
    );
    let runtime = ToolRuntime::new_for_tests_with_shell_clients(shell_clients.clone())
        .with_session_ledger(&ledger);
    let alice = shared_key_auth_context("recycled-authority-a");
    let alice_oauth = oauth_bridge_auth_context(
        "recycled-authority-a",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_PROJECT_WRITE,
            crate::auth::SCOPE_JOB_RUN,
            crate::auth::SCOPE_AGENT_REGISTER,
        ],
    );
    let bob = shared_key_auth_context("recycled-authority-b");
    assert_eq!(
        super::super::session_context::workflow_session_authority_fingerprint(Some(&alice))
            .unwrap(),
        super::super::session_context::workflow_session_authority_fingerprint(Some(&alice_oauth))
            .unwrap(),
        "direct shared-key and OAuth bridge must be one canonical authority group"
    );

    let project_summary = named_registered_project(
        "recycled-client",
        "recycled-project",
        "Recycled Project",
        "/tmp/recycled-project",
        1,
    );
    register_agent_projects_for_auth(
        &runtime,
        "recycled-client",
        &alice,
        ShellClientCapabilities::default(),
        vec![project_summary.clone()],
    )
    .await;
    let project = "agent:recycled-client:recycled-project".to_string();
    let started = call_with_recorder(
        &runtime,
        "start_session",
        json!({"project": project, "title": "recycling fence"}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();
    let posted = call_with_recorder(
        &runtime,
        "post_session_message",
        json!({
            "session_id": session_id,
            "kind": "todo",
            "message": "authority recycling must not resolve this todo"
        }),
        None,
        &alice,
        None,
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);
    let todo_id = posted.output["message_id"].as_str().unwrap().to_string();
    runtime.sessions.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(persisted.contains("owner_authority_fingerprint"));
    assert!(!persisted.contains("recycled-authority-a"));

    let oauth_read = call_with_recorder(
        &runtime,
        "session_summary",
        json!({"session_id": session_id}),
        None,
        &alice_oauth,
        None,
    )
    .await;
    assert!(oauth_read.success, "{:?}", oauth_read.error);

    let expired_at = chrono::Utc::now().timestamp() - 100;
    shell_clients
        .set_last_seen_for_test("recycled-client", expired_at)
        .await;
    let _ = shell_clients.list_clients_for_auth(Some(&alice)).await;
    assert!(shell_clients
        .get_client_view("recycled-client")
        .await
        .is_none());

    register_agent_projects_for_auth(
        &runtime,
        "recycled-client",
        &bob,
        ShellClientCapabilities::default(),
        vec![project_summary.clone()],
    )
    .await;
    let before = runtime.sessions.summary(&session_id, Some(100)).unwrap();
    let denied_calls = [
        ("session_summary", json!({"session_id": session_id})),
        (
            "list_session_messages",
            json!({"session_id": session_id, "message_id": todo_id}),
        ),
        (
            "post_session_message",
            json!({"session_id": session_id, "kind": "note", "message": "Bob must not write"}),
        ),
        (
            "complete_session_message",
            json!({
                "session_id": session_id,
                "message_id": todo_id,
                "answer": "Bob must not complete",
                "completion_key": "recycled-bob"
            }),
        ),
        (
            "session_discussion_summary",
            json!({"session_id": session_id}),
        ),
        (
            "session_handoff_summary",
            json!({
                "session_id": session_id,
                "include_workspace": false,
                "include_checkpoints": false,
                "include_validation": false,
                "summary_only": true
            }),
        ),
        ("close_session", json!({"session_id": session_id})),
    ];
    for (tool_name, arguments) in denied_calls {
        let denied = call_with_recorder(&runtime, tool_name, arguments, None, &bob, None).await;
        assert!(!denied.success, "{tool_name} unexpectedly succeeded");
        assert_eq!(
            denied.output["error_kind"], "session_authority_denied",
            "{tool_name}: {:?}",
            denied.output
        );
        let denied_state = runtime.sessions.summary(&session_id, Some(100)).unwrap();
        assert_eq!(
            denied_state.events.len(),
            before.events.len(),
            "{tool_name} denial must not append Session evidence: {:?}",
            tool_names(&runtime, &session_id)
        );
        assert_eq!(
            denied_state.messages.total, before.messages.total,
            "{tool_name} denial must not mutate Session messages"
        );
        assert_eq!(
            denied_state.lifecycle, before.lifecycle,
            "{tool_name} denial must not mutate Session lifecycle"
        );
    }
    let after = runtime.sessions.summary(&session_id, Some(100)).unwrap();
    assert_eq!(after.lifecycle, before.lifecycle);
    assert_eq!(after.messages.total, before.messages.total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(
        runtime
            .sessions
            .list_messages(
                &session_id,
                sessions::ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Answer),
                    ..Default::default()
                },
            )
            .unwrap()
            .len(),
        0
    );

    shell_clients
        .set_last_seen_for_test("recycled-client", expired_at)
        .await;
    let _ = shell_clients.list_clients_for_auth(Some(&bob)).await;
    assert!(shell_clients
        .get_client_view("recycled-client")
        .await
        .is_none());
    register_agent_projects_for_auth(
        &runtime,
        "recycled-client",
        &alice,
        ShellClientCapabilities::default(),
        vec![project_summary],
    )
    .await;
    let restored_owner = call_with_recorder(
        &runtime,
        "session_summary",
        json!({"session_id": session_id}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(restored_owner.success, "{:?}", restored_owner.error);
}

#[tokio::test]
async fn projectless_session_owner_authority_blocks_known_ids_from_foreign_principal() {
    let runtime = test_runtime();
    let alice = shared_key_auth_context("projectless-owner-alice");
    let bob = shared_key_auth_context("projectless-owner-bob");

    let started = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "Alice project-less Session"}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();
    assert_eq!(started.output["project"], Value::Null);

    let posted = call_with_recorder(
        &runtime,
        "post_session_message",
        json!({
            "session_id": session_id,
            "kind": "todo",
            "message": "Alice-owned project-less todo",
            "priority": "high"
        }),
        None,
        &alice,
        None,
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);
    let todo_id = posted.output["message_id"].as_str().unwrap().to_string();

    let denied_calls = [
        ("session_summary", json!({"session_id": session_id})),
        (
            "post_session_message",
            json!({"session_id": session_id, "kind": "note", "message": "Bob note"}),
        ),
        (
            "list_session_messages",
            json!({"session_id": session_id, "message_id": todo_id}),
        ),
        (
            "resolve_session_message",
            json!({"session_id": session_id, "message_id": todo_id}),
        ),
        (
            "complete_session_message",
            json!({
                "session_id": session_id,
                "message_id": todo_id,
                "answer": "Bob forged completion",
                "completion_key": "bob-forged"
            }),
        ),
        (
            "session_discussion_summary",
            json!({"session_id": session_id}),
        ),
        (
            "session_handoff_summary",
            json!({
                "session_id": session_id,
                "include_workspace": false,
                "include_checkpoints": false,
                "include_validation": false,
                "summary_only": true
            }),
        ),
        ("close_session", json!({"session_id": session_id})),
    ];
    for (tool_name, arguments) in denied_calls {
        let denied = call_with_recorder(&runtime, tool_name, arguments, None, &bob, None).await;
        assert!(!denied.success, "{tool_name} unexpectedly succeeded");
        assert_eq!(
            denied.output["error_kind"], "session_authority_denied",
            "{tool_name}: {:?}",
            denied.output
        );
    }

    let alice_summary = call_with_recorder(
        &runtime,
        "session_summary",
        json!({"session_id": session_id}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(alice_summary.success, "{:?}", alice_summary.error);
    let alice_complete = call_with_recorder(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": session_id,
            "message_id": todo_id,
            "answer": "Alice completion",
            "completion_key": "alice-complete"
        }),
        None,
        &alice,
        None,
    )
    .await;
    assert!(alice_complete.success, "{:?}", alice_complete.error);
    let alice_close = call_with_recorder(
        &runtime,
        "close_session",
        json!({"session_id": session_id}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(alice_close.success, "{:?}", alice_close.error);
}

#[tokio::test]
async fn projectless_owner_fingerprint_survives_restart_without_raw_principal_material() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let alice_identity = "projectless-persisted-alice-raw-principal";
    let alice = shared_key_auth_context(alice_identity);
    let bob = shared_key_auth_context("projectless-persisted-bob");
    let runtime = test_runtime().with_session_ledger(&ledger);

    let started = call_with_recorder(
        &runtime,
        "start_session",
        json!({"title": "durable project-less owner"}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();
    runtime.sessions.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(persisted.contains("owner_authority_fingerprint"));
    assert!(!persisted.contains(alice_identity));
    drop(runtime);

    let restored = test_runtime().with_session_ledger(&ledger);
    let alice_read = call_with_recorder(
        &restored,
        "session_summary",
        json!({"session_id": session_id}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(alice_read.success, "{:?}", alice_read.error);
    let bob_read = call_with_recorder(
        &restored,
        "session_summary",
        json!({"session_id": session_id}),
        None,
        &bob,
        None,
    )
    .await;
    assert!(!bob_read.success);
    assert_eq!(bob_read.output["error_kind"], "session_authority_denied");
}

#[tokio::test]
async fn legacy_persisted_projectless_session_without_owner_fails_closed_for_authenticated_caller()
{
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = sessions::SessionStore::with_persistence(
        &ledger,
        sessions::DEFAULT_MAX_SESSIONS,
        sessions::DEFAULT_MAX_EVENTS_PER_SESSION,
    );
    let legacy = store.start_session(None, Some("legacy project-less".to_string()));
    store.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(!persisted.contains("owner_authority_fingerprint"));
    drop(store);

    let runtime = test_runtime().with_session_ledger(&ledger);
    let alice = shared_key_auth_context("legacy-owner-alice");
    let denied = call_with_recorder(
        &runtime,
        "session_summary",
        json!({"session_id": legacy.session_id}),
        None,
        &alice,
        None,
    )
    .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "session_authority_denied");

    // The trusted local/dev path retains compatibility for legacy project-less
    // records; the new authenticated boundary is the fail-closed change.
    assert!(runtime.sessions.summary(&legacy.session_id, None).is_some());
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
    let coordinator = start_authorized_project_session(&runtime, &project, None, &alice);
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
