use super::*;
use crate::auth::{AuthContext, AuthKind};
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellClientRegisterRequest, ShellCommandExecutionState,
};
use webcodex_core::skill_store::SkillStoreRequest;

fn skill_store_registration(
    instance: &str,
    read: bool,
    manage: bool,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        client_id: "skill-store-runner".to_string(),
        agent_instance_id: instance.to_string(),
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        capabilities: Some(ShellClientCapabilities {
            skill_store_read: read,
            skill_store_manage: manage,
            ..Default::default()
        }),
        host_context: None,
        projects: None,
        agent_protocol_version: Some("polling-v1".to_string()),
        policy: None,
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
    }
}

fn alice() -> AuthContext {
    let mut auth = AuthContext::new(AuthKind::ApiToken);
    auth.username = Some("alice".to_string());
    auth
}

#[tokio::test]
async fn skill_store_enqueue_requires_exact_instance_and_independent_capability() {
    let registry = ShellClientRegistry::default();
    registry
        .register(skill_store_registration("instance-a", false, false))
        .await
        .unwrap();
    let auth = alice();

    let read_error = registry
        .enqueue_skill_store(
            "skill-store-runner",
            "instance-a",
            SkillStoreRequest::ListActive,
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(read_error.contains("skill_store_capability_unavailable"));
    assert!(read_error.contains("skill_store_read"));

    let stale_error = registry
        .enqueue_skill_store(
            "skill-store-runner",
            "replacement-instance",
            SkillStoreRequest::ListActive,
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(stale_error.contains("stale Runner"));

    registry
        .register(skill_store_registration("instance-a", true, false))
        .await
        .unwrap();
    let manage_error = registry
        .enqueue_skill_store(
            "skill-store-runner",
            "instance-a",
            SkillStoreRequest::Versions {
                skill_key: "demo".to_string(),
                offset: 0,
                limit: 1,
            },
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(manage_error.contains("skill_store_capability_unavailable"));
    assert!(manage_error.contains("skill_store_manage"));

    let inner = registry.inner.lock().await;
    assert!(inner.pending_by_id.is_empty());
}

#[tokio::test]
async fn skill_store_dequeue_rejects_replacement_runner_before_dispatch() {
    let registry = ShellClientRegistry::default();
    registry
        .register(skill_store_registration("instance-a", true, true))
        .await
        .unwrap();
    let auth = alice();
    let (_request_id, receiver) = registry
        .enqueue_skill_store(
            "skill-store-runner",
            "instance-a",
            SkillStoreRequest::Versions {
                skill_key: "demo".to_string(),
                offset: 0,
                limit: 1,
            },
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap();

    // Normal replacement registration drains stale synchronous work. Mutate
    // only the exact process lease here to prove dequeue itself independently
    // fences a later process that recycles the same stable client_id.
    {
        let mut inner = registry.inner.lock().await;
        inner
            .clients
            .get_mut("skill-store-runner")
            .unwrap()
            .agent_instance_id = "instance-b".to_string();
    }
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "skill-store-runner".to_string(),
            agent_instance_id: "instance-b".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(polled.is_none());
    let response = receiver.await.unwrap();
    assert!(!response.success);
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(response
        .error
        .as_deref()
        .is_some_and(|error| error.contains("target Runner changed before dispatch")));

    let inner = registry.inner.lock().await;
    assert!(inner.pending_by_id.is_empty());
    assert!(inner
        .queues_by_client
        .get("skill-store-runner")
        .is_none_or(|queue| queue.is_empty()));
}
