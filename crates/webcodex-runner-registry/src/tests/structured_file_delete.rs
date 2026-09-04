use super::*;

async fn register_structured_delete_runner(registry: &RunnerRegistry, client_id: &str) {
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: RunnerCapabilities::default(),
            policy: None,
        }))
        .await
        .unwrap();
}

async fn register_structured_delete_instance(
    registry: &RunnerRegistry,
    client_id: &str,
    instance: &str,
) -> Result<RunnerView, String> {
    register_instance_with_capabilities(
        registry,
        client_id,
        instance,
        RunnerCapabilities::default(),
    )
    .await
}

fn structured_delete_request(client_id: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: "delete_project_files".to_string(),
        client_id: client_id.to_string(),
        path: ".".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(r#"{"paths":["tmp.txt"]}"#.to_string()),
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 30,
    }
}

#[tokio::test]
async fn enqueue_structured_file_delete_queues_when_capability_advertised() {
    let registry = RunnerRegistry::default();
    register_structured_delete_runner(&registry, "structured-delete-on").await;
    let (request_id, _rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-on"),
            "tester".to_string(),
        )
        .await
        .expect("capable client should accept the structured delete request");

    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "structured-delete-on".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_delete_project_files");
    assert!(polled.command.is_empty());
    assert_eq!(polled.path.as_deref(), Some("."));
    assert_eq!(polled.content.as_deref(), Some(r#"{"paths":["tmp.txt"]}"#));
}

#[tokio::test]
async fn enqueue_structured_file_delete_unknown_runner_fails_closed() {
    let registry = RunnerRegistry::default();
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-ghost"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert_eq!(error, "unknown shell client: structured-delete-ghost");
    assert_structured_delete_runner_idle(&registry, "structured-delete-ghost").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_offline_runner_fails_closed() {
    let registry = RunnerRegistry::default();
    register_structured_delete_runner(&registry, "structured-delete-offline").await;
    registry
        .set_last_seen_for_test(
            "structured-delete-offline",
            chrono::Utc::now().timestamp() - 120,
        )
        .await;
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-offline"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("offline"), "error was: {error}");
    assert_structured_delete_runner_idle(&registry, "structured-delete-offline").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_rejects_other_ops_before_registry() {
    let registry = RunnerRegistry::default();
    register_structured_delete_runner(&registry, "structured-delete-op").await;
    let mut req = structured_delete_request("structured-delete-op");
    req.op = "write".to_string();
    let error = registry
        .enqueue_structured_file_delete(req, "tester".to_string())
        .await
        .unwrap_err();
    assert!(
        error.contains("op=delete_project_files"),
        "error was: {error}"
    );
    assert_structured_delete_runner_idle(&registry, "structured-delete-op").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_validates_request_before_locking() {
    let registry = RunnerRegistry::default();
    register_structured_delete_runner(&registry, "structured-delete-invalid").await;
    let mut req = structured_delete_request("structured-delete-invalid");
    req.path = "".to_string();
    let error = registry
        .enqueue_structured_file_delete(req, "tester".to_string())
        .await
        .unwrap_err();
    assert_eq!(error, "path cannot be empty");
    assert_structured_delete_runner_idle(&registry, "structured-delete-invalid").await;
}

#[tokio::test]
async fn same_instance_structured_file_delete_reconnect_allowed() {
    let registry = RunnerRegistry::default();
    register_structured_delete_instance(&registry, "monotonic-reconnect", "inst-a")
        .await
        .unwrap();
    register_structured_delete_instance(&registry, "monotonic-reconnect", "inst-a")
        .await
        .unwrap();
    let view = registry
        .get_runner_view("monotonic-reconnect")
        .await
        .unwrap();
    assert_eq!(view.runner_instance_id, "inst-a");
    assert!(view.capabilities.structured_file_delete);
}

#[tokio::test]
async fn same_instance_reconnect_keeps_queued_structured_delete_dispatchable() {
    let registry = RunnerRegistry::default();
    register_structured_delete_instance(&registry, "reconnect-keeps", "inst-a")
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("reconnect-keeps"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    // Same-instance transport reconnect with the capability still true: the
    // queued structured request remains valid and dispatchable to that
    // instance (never failed or re-enqueued).
    register_structured_delete_instance(&registry, "reconnect-keeps", "inst-a")
        .await
        .unwrap();
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "reconnect-keeps".to_string(),
            runner_instance_id: "inst-a".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_delete_project_files");
}

#[tokio::test]
async fn instance_replacement_drains_sync_requests_before_installing_new_lease() {
    let registry = RunnerRegistry::default();
    register_structured_delete_instance(&registry, "replace-drain", "inst-a")
        .await
        .unwrap();
    let (request_id, mut rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("replace-drain"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    // Replacement instance B satisfies the same generation-2 baseline. Its
    // successful registration is the lease transition and immediately drains
    // the old synchronous request; passive last_seen freshness is not a lock.
    register_structured_delete_instance(&registry, "replace-drain", "inst-b")
        .await
        .unwrap();

    // The old synchronous waiter resolves safely with request_dispatched=false
    // (the request was never polled by the replaced instance).
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut rx)
        .await
        .expect("replaced instance waiter must resolve promptly")
        .expect("replaced instance waiter must not be dropped");
    assert!(!response.success);
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.request_dispatched, Some(false));
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("replaced"),
        "error was: {:?}",
        response.error
    );

    // The replacement Runner polls no inherited file_delete_project_files.
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "replace-drain".to_string(),
            runner_instance_id: "inst-b".to_string(),
        })
        .await
        .unwrap();
    assert!(
        polled.is_none(),
        "replacement must not inherit the structured delete request: {polled:?}"
    );

    // No pending request or queue leak remains for the client.
    assert_structured_delete_runner_idle(&registry, "replace-drain").await;
}

#[tokio::test]
async fn instance_replacement_keeps_job_reconciliation_contract_unchanged() {
    let registry = RunnerRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "replace-job-sync",
        "inst-a",
        RunnerCapabilities {
            jobs: true,
            async_jobs: true,
            async_shell_jobs: true,
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("replace-job-sync".to_string()),
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
    let (request_id, mut rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("replace-job-sync"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    registry
        .set_last_seen_for_test("replace-job-sync", chrono::Utc::now().timestamp() - 120)
        .await;
    register_instance_with_capabilities(
        &registry,
        "replace-job-sync",
        "inst-b",
        RunnerCapabilities::default(),
    )
    .await
    .unwrap();

    // The synchronous structured delete is drained with its waiter resolved...
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut rx)
        .await
        .expect("waiter must resolve promptly")
        .expect("waiter must not be dropped");
    assert!(!response.success);
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.request_dispatched, Some(false));
    // ...while the Job keeps its existing replacement reconciliation contract:
    // terminated to `lost` with `runner_instance_replaced`, never drained as a
    // synchronous request.
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert_eq!(
        lost.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );
    // The replacement polls no inherited structured delete and nothing leaks.
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "replace-job-sync".to_string(),
            runner_instance_id: "inst-b".to_string(),
        })
        .await
        .unwrap();
    assert!(polled.is_none());
    assert_structured_delete_runner_idle(&registry, "replace-job-sync").await;
}
