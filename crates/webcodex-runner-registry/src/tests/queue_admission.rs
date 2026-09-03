use super::*;

#[tokio::test]
async fn registry_rejects_enqueue_when_queue_full() {
    let registry = RunnerRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "full".to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    // Fill the queue to the limit without any consumer draining it.
    for _ in 0..MAX_QUEUED_REQUESTS_PER_RUNNER {
        registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "full".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
    }
    // The next enqueue must be rejected with a structured error instead
    // of growing the queue unboundedly.
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "full".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("too many pending requests"));
    assert!(err.contains("full"));
    // The queue is exactly at the cap; memory is bounded.
    let view = registry.get_runner_view("full").await.unwrap();
    assert_eq!(view.pending_requests, MAX_QUEUED_REQUESTS_PER_RUNNER);
}

#[tokio::test]
async fn registry_rejects_enqueue_when_client_offline() {
    // Registered-but-stale agents must fail fast at enqueue rather than
    // accepting work that can only time out (or fill the 256-deep queue).
    let registry = RunnerRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "stale".to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("stale", now_ts() - RUNNER_ONLINE_WINDOW_SECS - 1)
        .await;

    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "stale".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("offline"),
        "enqueue against a stale agent must fail fast as offline: {err}"
    );
    let view = registry.get_runner_view("stale").await.unwrap();
    assert_eq!(view.pending_requests, 0);
    assert!(!view.connected);
}
