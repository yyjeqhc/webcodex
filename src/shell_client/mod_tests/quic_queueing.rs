use super::*;

#[tokio::test]
async fn registry_allows_quic_v1_run_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-run").await;

    let (_request_id, _rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "quic-run".to_string(),
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
    let view = registry.get_client_view("quic-run").await.unwrap();
    assert_eq!(view.transport, TRANSPORT_QUIC);
    assert_eq!(view.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
    assert_eq!(view.pending_requests, 1);
    assert!(view.capabilities.shell);
    assert!(view.capabilities.async_shell_jobs);
}

#[tokio::test]
async fn enqueue_file_op_allows_read_with_line_range() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;

    let mut req = file_request("read");
    req.start_line = Some(7);
    req.end_line = Some(12);
    let (request_id, _rx) = registry
        .enqueue_file_op(req, "tester".to_string())
        .await
        .unwrap();

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_read");
    assert_eq!(polled.path.as_deref(), Some("src/auth/scopes.rs"));
    assert_eq!(polled.start_line, Some(7));
    assert_eq!(polled.end_line, Some(12));
}

#[tokio::test]
async fn registry_allows_quic_v1_file_and_project_ops_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-ops").await;

    let (_file_request_id, _file_rx) = registry
        .enqueue_file_op(
            ShellFileOpRequest {
                op: "read".to_string(),
                client_id: "quic-ops".to_string(),
                path: "README.md".to_string(),
                cwd: None,
                content: None,
                max_bytes: None,
                old_text: None,
                pattern: None,
                expected_sha256: None,
                expected_prefix: None,
                start_line: None,
                end_line: None,
                line: None,
                create_dirs: false,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let (_project_request_id, _project_rx) = registry
        .enqueue_project_op(
            "quic-ops".to_string(),
            "register_project",
            "{}".to_string(),
            "tester".to_string(),
        )
        .await
        .unwrap();

    let view = registry.get_client_view("quic-ops").await.unwrap();
    assert_eq!(view.pending_requests, 2);
}

#[tokio::test]
async fn registry_allows_quic_v1_start_job_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-job").await;

    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("quic-job".to_string()),
                cwd: None,
                command: Some("sleep 1".to_string()),
                timeout_secs: Some(5),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let view = registry.get_client_view("quic-job").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(job.status, "queued");
    assert_eq!(registry.list_jobs(Some(10)).await.len(), 1);
}

#[tokio::test]
async fn registry_allows_quic_v1_stop_job_delivery_queueing() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "quic-stop".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("quic-stop".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let _ = registry
        .poll(ShellAgentPollRequest {
            client_id: "quic-stop".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    registry
        .set_transport("quic-stop", AgentTransport::Quic)
        .await
        .unwrap();

    let stopped = registry
        .stop_job(&job.job_id, "tester".to_string())
        .await
        .unwrap();
    let view = registry.get_client_view("quic-stop").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(stopped.status, "stop_requested");
}
