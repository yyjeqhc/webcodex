use super::*;

#[tokio::test]
async fn registry_allows_session_scoped_run_without_ssh_resource() {
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
            policy: None,
        }))
        .await
        .unwrap();

    let (request_id, _rx) = registry
        .enqueue_run_with_ssh(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo local".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            None,
            Some("wc_sess_local".to_string()),
        )
        .await
        .unwrap();
    assert!(!registry.cancel_request(&request_id).await);

    let error = registry
        .enqueue_run_with_ssh(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo remote".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            Some("tmp".to_string()),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("ssh_session_required"), "{error}");
}

#[tokio::test]
async fn registry_rejects_unknown_client_run() {
    let registry = ShellClientRegistry::default();
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "missing".to_string(),
                cwd: None,
                command: "pwd".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("unknown shell client"));
}
