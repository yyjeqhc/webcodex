use super::*;

#[tokio::test]
async fn reconcile_disconnect_marks_running_jobs_lost() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
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
    // Job is "queued" with its request sitting in the client's queue.
    let before = registry.get_client_view("oe").await.unwrap();
    assert_eq!(before.pending_requests, 1);
    // Transport disconnects (e.g. WebSocket dropped).
    registry.reconcile_disconnect("oe", "inst").await;
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert!(lost.error.unwrap().contains("disconnected"));
    // Pending request was dropped: no dangling waiter / queue entry.
    let after = registry.get_client_view("oe").await.unwrap();
    assert_eq!(after.pending_requests, 0);
}
#[tokio::test]
async fn reconcile_disconnect_fails_pending_sync_requests_fast() {
    // Regression guard for the MCP "no reply" hang: a synchronous tool
    // request (run_shell/read_file/... with job_id: None) whose agent drops
    // mid-flight must be resolved immediately, not parked until the caller's
    // wait timeout.
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "oe".to_string(),
                cwd: Some("/tmp".to_string()),
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 30,
                wait_timeout_secs: 30,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let before = registry.get_client_view("oe").await.unwrap();
    assert_eq!(before.pending_requests, 1);

    // Agent transport drops before returning a result.
    registry.reconcile_disconnect("oe", "inst").await;

    // Waiter resolves promptly with a disconnect error rather than parking
    // for the full 30s wait timeout. The short timeout turns a regression
    // (unbounded park) into a fast test failure instead of a hang.
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .expect("waiter must resolve promptly, not park until the caller timeout")
        .expect("waiter must be resolved, not dropped");
    assert!(!response.success);
    let error = response.error.expect("disconnect must set an error");
    assert!(
        error.contains("offline"),
        "error should classify as agent_offline: {error}"
    );
    assert!(
        !error.to_ascii_lowercase().contains("command"),
        "generic sync disconnect errors must remain request-neutral: {error}"
    );
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(response.command_execution_state, None);
    // No dangling waiter or queue entry remains.
    let after = registry.get_client_view("oe").await.unwrap();
    assert_eq!(after.pending_requests, 0);
}
#[tokio::test]
async fn dispatched_file_request_disconnect_remains_request_neutral() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;
    let (_request_id, rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("file request should be dispatched");

    registry.reconcile_disconnect("oe", "inst").await;

    let response = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .expect("waiter must resolve promptly")
        .expect("waiter must receive a response");
    let error = response.error.as_deref().unwrap_or_default();
    assert!(
        !error.to_ascii_lowercase().contains("command"),
        "generic sync disconnect errors must not invent command lifecycle prose: {error}"
    );
    assert_eq!(response.request_dispatched, Some(true));
    assert_eq!(response.command_execution_state, None);
}
#[tokio::test]
async fn reconcile_disconnect_releases_active_lease_immediately() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;

    registry.reconcile_disconnect("oe", "inst-a").await;

    let offline = registry.get_client_view("oe").await.unwrap();
    assert!(
        !offline.connected,
        "active disconnect must immediately leave online window"
    );
    assert!(now_ts().saturating_sub(offline.last_seen) > CLIENT_ONLINE_WINDOW_SECS);

    let new_view = register_with_instance(&registry, "oe", "inst-b").await;
    assert_eq!(new_view.agent_instance_id, "inst-b");
    assert!(
        new_view.connected,
        "new instance should register without waiting 60 seconds"
    );
}
