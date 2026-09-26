use super::*;
use crate::runner_protocol::{
    RunnerPollRequest, RunnerRegisterRequest, ShellCommandExecutionState,
};
use webcodex_core::runner_skill::RunnerSkillRequest;

fn skill_registration(instance: &str, runtime: bool, management: bool) -> RunnerRegisterRequest {
    current_runner_registration(RunnerRegisterRequest {
        computer_session_availability: None,
        client_id: "skill-runner".to_string(),
        runner_instance_id: instance.to_string(),
        runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        capabilities: RunnerCapabilities {
            skill_runtime: runtime,
            skill_management: management,
            ..Default::default()
        },
        host_context: None,
        policy: None,
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
    })
}

fn alice() -> RunnerAccess {
    auth_context(Some("alice"), false)
}

#[tokio::test]
async fn skill_enqueue_requires_exact_instance_and_independent_runtime_management_capabilities() {
    let registry = RunnerRegistry::default();
    registry
        .register(skill_registration("instance-a", false, false))
        .await
        .unwrap();
    let auth = alice();

    let runtime_error = registry
        .enqueue_runner_skill_typed(
            "skill-runner",
            "instance-a",
            RunnerSkillRequest::List,
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        runtime_error,
        EnqueueRunnerSkillError::UnsupportedCapability {
            capability: "skill_runtime",
            ..
        }
    ));

    registry
        .register(skill_registration("instance-a", true, false))
        .await
        .unwrap();
    let stale_error = registry
        .enqueue_runner_skill_typed(
            "skill-runner",
            "replacement-instance",
            RunnerSkillRequest::List,
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        stale_error,
        EnqueueRunnerSkillError::RunnerChanged { .. }
    ));

    let management_error = registry
        .enqueue_runner_skill_typed(
            "skill-runner",
            "instance-a",
            RunnerSkillRequest::Versions {
                skill_key: "demo".to_string(),
                offset: 0,
                limit: 1,
            },
            Some(&auth),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        management_error,
        EnqueueRunnerSkillError::UnsupportedCapability {
            capability: "skill_management",
            ..
        }
    ));

    let inner = registry.inner.lock().await;
    assert!(inner.pending_by_id.is_empty());
}

#[tokio::test]
async fn skill_dequeue_rejects_replacement_runner_before_dispatch() {
    let registry = RunnerRegistry::default();
    registry
        .register(skill_registration("instance-a", true, true))
        .await
        .unwrap();
    let auth = alice();
    let (_request_id, receiver) = registry
        .enqueue_runner_skill_typed(
            "skill-runner",
            "instance-a",
            RunnerSkillRequest::Versions {
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
    // only the exact process lease here to prove dequeue independently fences
    // a later process that recycles the same stable client_id.
    {
        let mut inner = registry.inner.lock().await;
        inner
            .runners
            .get_mut("skill-runner")
            .unwrap()
            .runner_instance_id = "instance-b".to_string();
    }
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "skill-runner".to_string(),
            runner_instance_id: "instance-b".to_string(),
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
        .is_some_and(|error| error.contains("Skill target Runner changed before dispatch")));

    let inner = registry.inner.lock().await;
    assert!(inner.pending_by_id.is_empty());
    assert!(inner
        .queues_by_runner
        .get("skill-runner")
        .is_none_or(|queue| queue.is_empty()));
}
