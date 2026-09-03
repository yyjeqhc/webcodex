use super::*;

#[tokio::test]
async fn registry_allows_quic_run_queueing() {
    let registry = RunnerRegistry::default();
    register_quic_v1_runner(&registry, "quic-run").await;

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
    let view = registry.get_runner_view("quic-run").await.unwrap();
    assert_eq!(view.transport, TRANSPORT_QUIC);
    assert_eq!(
        view.runner_protocol_generation,
        RUNNER_PROTOCOL_GENERATION_V2
    );
    assert_eq!(view.pending_requests, 1);
    assert!(view.capabilities.shell);
    assert!(view.capabilities.async_shell_jobs);
}

#[tokio::test]
async fn enqueue_file_op_allows_read_with_line_range() {
    let registry = RunnerRegistry::default();
    register_quic_v1_runner(&registry, "oe").await;

    let mut req = file_request("read");
    req.start_line = Some(7);
    req.end_line = Some(12);
    let (request_id, _rx) = registry
        .enqueue_file_op(req, "tester".to_string())
        .await
        .unwrap();

    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "oe".to_string(),
            runner_instance_id: "inst".to_string(),
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
async fn generic_file_enqueue_rejects_internal_skill_runtime_ops() {
    let registry = RunnerRegistry::default();

    let mut list = file_request("skill_list_packages");
    list.path = ".agents/skills".to_string();
    list.content = Some(r#"{"limit":257}"#.to_string());
    let error = registry
        .enqueue_file_op(list, "rest-like-caller".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("internal-only"));
    assert!(error.contains("skill_list_packages"));

    let mut read = file_request("skill_read_file");
    read.path = ".agents/skills/foo/SKILL.md".to_string();
    read.content =
        Some(r#"{"package_root":".agents/skills/foo","max_file_bytes":65536}"#.to_string());
    read.start_line = Some(1);
    read.end_line = Some(20);
    read.max_bytes = Some(48 * 1024);
    let error = registry
        .enqueue_file_op(read, "rest-like-caller".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("internal-only"));
    assert!(error.contains("skill_read_file"));
}

#[tokio::test]
async fn registry_allows_quic_v1_file_and_project_ops_queueing() {
    let registry = RunnerRegistry::default();
    register_quic_v1_runner(&registry, "quic-ops").await;

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

    let view = registry.get_runner_view("quic-ops").await.unwrap();
    assert_eq!(view.pending_requests, 2);
}

#[tokio::test]
async fn registry_allows_quic_v1_start_job_queueing() {
    let registry = RunnerRegistry::default();
    register_quic_v1_runner(&registry, "quic-job").await;

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

    let view = registry.get_runner_view("quic-job").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(job.status, "queued");
    assert_eq!(registry.list_jobs(Some(10)).await.len(), 1);
}

#[tokio::test]
async fn registry_allows_quic_v1_stop_job_delivery_queueing() {
    let registry = RunnerRegistry::default();
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "quic-stop".to_string(),
            runner_instance_id: "inst".to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: async_job_capabilities(),
            policy: None,
        }))
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
        .poll(RunnerPollRequest {
            client_id: "quic-stop".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    registry
        .set_transport("quic-stop", RunnerTransport::Quic)
        .await
        .unwrap();

    let stopped = registry
        .stop_job(&job.job_id, "tester".to_string())
        .await
        .unwrap();
    let view = registry.get_runner_view("quic-stop").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(stopped.status, "stop_requested");
}
