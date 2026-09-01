use super::*;

#[tokio::test]
async fn terminal_observed_poll_complete_and_log() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: Some("/tmp".to_string()),
                command: Some("printf hello".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: Some(ShellJobCodexMetadata {
                    project: Some("demo".to_string()),
                    goal_id: Some("goal-1".to_string()),
                    client_request_id: Some("crid-1".to_string()),
                    command: Some("printf hello".to_string()),
                    kind: Some("command".to_string()),
                    suite: None,
                    script_path: None,
                    reason: Some("test job".to_string()),
                    max_runtime_secs: Some(10),
                }),
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(job.status, "queued");
    assert_eq!(
        job.codex
            .as_ref()
            .and_then(|codex| codex.client_request_id.as_deref()),
        Some("crid-1")
    );
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.command, "printf hello");
    let running = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(running.status, "agent_queued");
    registry
        .complete(ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: polled.request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(20),
            error: None,
        })
        .await
        .unwrap();
    let done = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.exit_code, Some(0));
    {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        assert!(record.terminal_observed_at.is_some());
        assert_eq!(record.terminal_observed_at, record.ended_at);
    }
    assert_eq!(
        done.codex
            .as_ref()
            .and_then(|codex| codex.project.as_deref()),
        Some("demo")
    );
    let listed = registry.list_jobs(Some(10)).await;
    assert_eq!(
        listed
            .iter()
            .find(|listed| listed.job_id == job.job_id)
            .and_then(|listed| listed.codex.as_ref())
            .and_then(|codex| codex.goal_id.as_deref()),
        Some("goal-1")
    );
    let (_info, stdout, stderr, next_stdout, next_stderr) = registry
        .job_log(&job.job_id, Some(1), Some(1), None)
        .await
        .unwrap();
    assert_eq!(stdout.as_deref(), Some("hello\n"));
    assert_eq!(stderr.as_deref(), Some(""));
    assert_eq!(next_stdout, 2);
    assert_eq!(next_stderr, 1);
}

#[tokio::test]
async fn terminal_observed_queued_stop_records_server_time() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
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
            "test".to_string(),
        )
        .await
        .unwrap();
    let stopped = registry
        .stop_job(&job.job_id, "test".to_string())
        .await
        .unwrap();
    assert_eq!(stopped.status, "stopped");
    {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        assert!(record.terminal_observed_at.is_some());
        assert_eq!(record.terminal_observed_at, record.ended_at);
    }
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap();
    assert!(polled.is_none());
}

#[tokio::test]
async fn registry_shell_job_stop_running_delivers_stop_to_client() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
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
            "test".to_string(),
        )
        .await
        .unwrap();
    let started = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.kind, "start_job");

    let stop_requested = registry
        .stop_job(&job.job_id, "test".to_string())
        .await
        .unwrap();
    assert_eq!(stop_requested.status, "stop_requested");
    let stop = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stop.kind, "stop_job");
    assert_eq!(stop.job_id.as_deref(), Some(job.job_id.as_str()));
}

#[tokio::test]
async fn registry_marks_running_job_lost_when_client_stale() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
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
            "test".to_string(),
        )
        .await
        .unwrap();
    let _ = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        let client = inner.clients.get_mut("oe").unwrap();
        client.last_seen = now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1;
    }
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert!(lost.error.unwrap().contains("stale"));
}
