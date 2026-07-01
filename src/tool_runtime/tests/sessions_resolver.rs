//! Project resolver tests for tool_runtime.

use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellAgentResultRequest;

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
