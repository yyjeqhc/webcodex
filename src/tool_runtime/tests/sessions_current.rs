//! Current-session binding tests: bind, unbind, isolation, stale eviction.

use super::super::{current_session_principal, kernel, sessions, ToolCall};
use super::support::*;
use crate::shell_protocol::{ShellClientCapabilities, ShellClientRegisterRequest};
use serde_json::json;

#[test]
fn open_anonymous_current_session_principal_is_stable() {
    let open = open_auth_context();
    assert_eq!(open.user_id, None);
    assert_eq!(open.api_key_id, None);
    assert_eq!(open.shared_key_hash, None);

    let (principal_kind, principal_id) = current_session_principal(Some(&open)).unwrap();
    assert_eq!(principal_kind, "open");
    assert_eq!(principal_id, "open-anonymous");
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
async fn open_anonymous_can_bind_current_session_and_record_project_read() {
    let runtime = test_runtime();
    let open = open_auth_context();
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: "open-current".to_string(),
                agent_instance_id: "inst-open-current".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    file_read: true,
                    ..Default::default()
                }),
                projects: Some(vec![registered_project("agent-proj", "/tmp/open-current")]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            Some(&open),
        )
        .await
        .unwrap();
    let project = agent_test_project_id("open-current");

    let started = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_session",
                json!({"project": project, "title": "open current"}),
            )
            .unwrap(),
            Some(&open),
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
            Some(&open),
        )
        .await;
    assert!(bound.success, "{:?}", bound.error);
    assert_eq!(bound.output["bound"], true);

    let current = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("current_session", json!({"project": project})).unwrap(),
            Some(&open),
        )
        .await;
    assert!(current.success, "{:?}", current.error);
    assert_eq!(current.output["found"], true);
    assert_eq!(current.output["session_id"], session_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let open = open.clone();
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
                    Some(&open),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "open-current", "inst-open-current")
        .await
        .expect("open read_file should enqueue with current session");
    complete_patch_agent_request_for_instance(
        &runtime,
        "open-current",
        "inst-open-current",
        &req.request_id,
        0,
        "hello\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    assert_eq!(result.output["session_id"], session_id);
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
