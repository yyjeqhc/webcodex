use super::*;

#[tokio::test]
async fn registry_enqueues_polls_and_completes_shell_request() {
    let registry = ShellClientRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        }))
        .await
        .unwrap();
    let (request_id, rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: Some("/tmp".to_string()),
                command: "echo hello".to_string(),
                stdin: Some("hello stdin".to_string()),
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.command, "echo hello");
    assert_eq!(polled.stdin.as_deref(), Some("hello stdin"));
    registry
        .complete(ShellAgentResultRequest {
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(12),
            error: None,
        })
        .await
        .unwrap();
    let response = rx.await.unwrap();
    assert!(response.success);
    assert_eq!(response.stdout.as_deref(), Some("hello\n"));
}

#[tokio::test]
async fn polling_out_of_order_results_resolve_only_their_original_waiters() {
    let registry = ShellClientRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        }))
        .await
        .unwrap();
    let (request_a, waiter_a) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "ordered".to_string(),
                cwd: None,
                command: "slow-a".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let (request_b, waiter_b) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "ordered".to_string(),
                cwd: None,
                command: "fast-b".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let polled_a = registry
        .poll(ShellAgentPollRequest {
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    let polled_b = registry
        .poll(ShellAgentPollRequest {
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled_a.request_id, request_a);
    assert_eq!(polled_b.request_id, request_b);

    for (request_id, stdout) in [(&request_b, "result-b\n"), (&request_a, "result-a\n")] {
        registry
            .complete(ShellAgentResultRequest {
                client_id: "ordered".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some(stdout.to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    }

    let response_b = waiter_b.await.unwrap();
    let response_a = waiter_a.await.unwrap();
    assert_eq!(response_b.request_id, request_b);
    assert_eq!(response_b.stdout.as_deref(), Some("result-b\n"));
    assert_eq!(response_a.request_id, request_a);
    assert_eq!(response_a.stdout.as_deref(), Some("result-a\n"));
}
