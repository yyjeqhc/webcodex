use super::*;
use crate::db::ConnectorExecutionObservation;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentPollRequest, ShellAgentProjectSummary,
    ShellAgentResultRequest, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest,
};
use std::time::{Duration, Instant};

#[tokio::test]
async fn another_user_cannot_observe_or_use_a_task_id() {
    let (_temp, connector) = tests::connector();
    let started = connector
        .call(
            "task_start",
            json!({ "goal": "private work", "mode": "read_only" }),
            Some(&tests::auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap();
    let outcome = connector
        .call(
            "files_read",
            json!({ "task_id": task_id, "files": [{ "path": "src/lib.rs" }] }),
            Some(&tests::auth("u2")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.http_status, 404);
    assert_eq!(outcome.body["error"]["code"], "task_not_found");
    assert!(outcome.body["task_id"].is_null());
}

#[test]
fn executor_ids_are_recursively_replaced() {
    let mut value = json!({
        "project": "agent:hosted:demo",
        "client_id": "hosted-secret-routing-id",
        "request_id": "transport-request-id",
        "message": "failed in agent:hosted:demo at /workspace/demo/src/lib.rs",
        "nested": ["agent:hosted:demo"]
    });
    sanitize_value(
        &mut value,
        "agent:hosted:demo",
        "wc_proj_demo123456",
        "/workspace/demo",
    );
    let serialized = serde_json::to_string(&value).unwrap();
    for secret in [
        "agent:hosted:demo",
        "/workspace/demo",
        "hosted-secret-routing-id",
        "transport-request-id",
    ] {
        assert!(!serialized.contains(secret));
    }
    assert!(serialized.contains("wc_proj_demo123456"));
}

struct Fixture {
    _temp: tempfile::TempDir,
    connector: Arc<ConnectorRuntime>,
    registry: Arc<ShellClientRegistry>,
    owner: AuthContext,
    task_id: String,
}

impl Fixture {
    async fn call(&self, capability: &str, arguments: Value) -> ConnectorCallOutcome {
        call(&self.connector, &self.owner, capability, arguments).await
    }
}

async fn fixture(yield_ms: u64) -> Fixture {
    fixture_configured(yield_ms, |service| service).await
}

async fn fixture_configured(
    yield_ms: u64,
    configure: impl FnOnce(execution::ExecutionService) -> execution::ExecutionService,
) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let state = temp.path().join("state");
    tests::init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            display_name: None,
            owner: Some("owner".into()),
            hostname: None,
            capabilities: Some(ShellClientCapabilities {
                jobs: true,
                async_jobs: true,
                async_shell_jobs: true,
                ..Default::default()
            }),
            projects: Some(vec![project_summary("project", &project)]),
            agent_protocol_version: Some("test".into()),
            policy: None,
        })
        .await
        .unwrap();
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let tools = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
        registry.clone(),
    ));
    let mut connector = ConnectorRuntime::new(
        tools,
        db,
        ConnectorContext {
            project_id: "wc_proj_1234567890".into(),
            project_name: "project".into(),
            workspace_id: "wc_ws_1234567890".into(),
            executor_project: "agent:hosted:project".into(),
            executor_root: project.to_string_lossy().into_owned(),
            runs_root: state.join("runs").to_string_lossy().into_owned(),
            results_root: state.join("results").to_string_lossy().into_owned(),
            projects_dir: state
                .join("agent/projects.d")
                .to_string_lossy()
                .into_owned(),
            profile: "personal".into(),
        },
    )
    .unwrap();
    connector.executions = configure(connector.executions.clone().with_yield_ms(yield_ms));
    let registration_registry = registry.clone();
    let registration = tokio::spawn(async move {
        let request = next_request(&registration_registry).await;
        assert_eq!(request.kind, "register_project");
        let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        registration_registry
            .complete(ShellAgentResultRequest {
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(
                    json!({
                        "agent_project_id": payload["id"],
                        "client_id": "hosted",
                        "name": payload["name"],
                        "path": payload["path"],
                        "allow_patch": true
                    })
                    .to_string(),
                ),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    });
    let owner = tests::auth("u1");
    let started = connector
        .call(
            "task_start",
            json!({"goal": "exercise durable execution", "mode": "normal"}),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    registration.await.unwrap();
    assert!(started.ok, "{}", started.body);
    Fixture {
        _temp: temp,
        connector: Arc::new(connector),
        registry,
        owner,
        task_id: started.body["task_id"].as_str().unwrap().to_string(),
    }
}

fn project_summary(id: &str, path: &Path) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.into(),
        name: Some(id.into()),
        path: path.to_string_lossy().into_owned(),
        allow_patch: true,
        kind: Some("auto".into()),
        description: None,
        hooks: Vec::new(),
        disabled: false,
        git_branch: Some("main".into()),
        git_head: None,
        git_dirty: Some(false),
        updated_at: 1,
        shell_profile: None,
    }
}

async fn next_request(registry: &ShellClientRegistry) -> ShellAgentShellRequest {
    for _ in 0..2_000 {
        if let Some(request) = poll(registry).await {
            return request;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("agent request was not dispatched");
}

async fn poll(registry: &ShellClientRegistry) -> Option<ShellAgentShellRequest> {
    registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            projects: None,
        })
        .await
        .unwrap()
}

fn created(reservation: ConnectorExecutionReservation) -> crate::db::ConnectorExecution {
    match reservation {
        ConnectorExecutionReservation::Created(execution) => execution,
        ConnectorExecutionReservation::Existing(_) => unreachable!(),
    }
}

fn task(fixture: &Fixture) -> ConnectorTaskSnapshot {
    fixture
        .connector
        .db
        .connector_task(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            "user:u1",
        )
        .unwrap()
}

async fn call(
    connector: &ConnectorRuntime,
    owner: &AuthContext,
    capability: &str,
    arguments: Value,
) -> ConnectorCallOutcome {
    connector
        .call(capability, arguments, Some(owner), ConnectorTransport::Mcp)
        .await
}

async fn approve(fixture: &Fixture, operation_id: &str, command: &str) -> Value {
    let arguments = json!({
        "task_id": fixture.task_id,
        "operation_id": operation_id,
        "command": command,
        "timeout_secs": 30
    });
    let waiting = fixture.call("commands_run", arguments.clone()).await;
    assert_eq!(waiting.body["error"]["code"], "approval_required");
    fixture
        .connector
        .db
        .decide_connector_approval(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            waiting.body["data"]["approval"]["approval_id"]
                .as_str()
                .unwrap(),
            true,
            "local_cli",
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    arguments
}

async fn update_job(
    registry: &ShellClientRegistry,
    job_id: &str,
    status: &str,
    stdout: Option<&str>,
    exit_code: Option<i32>,
) {
    registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            job_id: job_id.into(),
            request_id: None,
            status: status.into(),
            stdout_chunk: stdout.map(str::to_string),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code,
            duration_ms: Some(1),
            error: None,
            finished: matches!(status, "completed" | "failed" | "stopped"),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn short_command_returns_terminal_and_precise_retry_does_not_spawn() {
    let fixture = fixture(1_000).await;
    let arguments = approve(&fixture, "short-command-1", "printf short").await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        assert_eq!(request.kind, "start_job");
        let job_id = request.job_id.unwrap();
        update_job(&registry, &job_id, "running", Some("short\n"), None).await;
        update_job(&registry, &job_id, "completed", None, Some(0)).await;
    });
    let first = fixture.call("commands_run", arguments.clone()).await;
    responder.await.unwrap();
    assert!(first.ok, "{}", first.body);
    let execution = &first.body["data"]["execution"];
    assert_eq!(execution["submission_status"], "accepted");
    assert_eq!(execution["execution_status"], "succeeded");
    assert_eq!(execution["exit_code"], 0);
    assert_eq!(execution["assertion_status"], "not_run");
    assert_eq!(execution["capability_outcome"], "completed");
    assert!(execution["output_tail"]["stdout"]
        .as_str()
        .unwrap()
        .contains("short"));

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("retry-drift"),
        "changed",
    )
    .unwrap();
    let retry = fixture.call("commands_run", arguments).await;
    assert_eq!(
        retry.body["data"]["execution"]["execution_id"],
        execution["execution_id"]
    );
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn operation_id_conflict_is_stable_and_does_not_spawn() {
    let fixture = fixture(1_000).await;
    let arguments = approve(&fixture, "stable-operation", "printf first").await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_job(&registry, &job_id, "completed", Some("first\n"), Some(0)).await;
    });
    let first = fixture.call("commands_run", arguments).await;
    responder.await.unwrap();
    assert!(first.ok, "{}", first.body);

    let conflicting = json!({
        "task_id": fixture.task_id,
        "operation_id": "stable-operation",
        "command": "printf different",
        "timeout_secs": 30
    });
    for _ in 0..2 {
        let conflict = fixture.call("commands_run", conflicting.clone()).await;
        assert!(!conflict.ok, "{}", conflict.body);
        assert_eq!(conflict.http_status, 409);
        assert_eq!(conflict.body["error"]["code"], "operation_id_conflict");
        assert_eq!(conflict.body["data"]["operation_id"], "stable-operation");
        assert!(poll(&fixture.registry).await.is_none());
    }
}

#[tokio::test]
async fn new_operation_id_reruns_same_command_after_workspace_change() {
    let fixture = fixture(1_000).await;
    let command = "cargo test";
    let first_arguments = approve(&fixture, "test-attempt-1", command).await;
    let first_registry = fixture.registry.clone();
    let first_responder = tokio::spawn(async move {
        let request = next_request(&first_registry).await;
        let job_id = request.job_id.unwrap();
        update_job(
            &first_registry,
            &job_id,
            "failed",
            Some("test failed\n"),
            Some(1),
        )
        .await;
    });
    let first = fixture.call("commands_run", first_arguments).await;
    first_responder.await.unwrap();
    assert_eq!(
        first.body["data"]["execution"]["execution_status"],
        "failed"
    );

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("fixed-source"),
        "fixed",
    )
    .unwrap();
    let second_arguments = approve(&fixture, "test-attempt-2", command).await;
    let second_registry = fixture.registry.clone();
    let second_responder = tokio::spawn(async move {
        let request = next_request(&second_registry).await;
        let job_id = request.job_id.unwrap();
        update_job(
            &second_registry,
            &job_id,
            "completed",
            Some("test passed\n"),
            Some(0),
        )
        .await;
    });
    let second = fixture.call("commands_run", second_arguments).await;
    second_responder.await.unwrap();
    assert_eq!(
        second.body["data"]["execution"]["execution_status"],
        "succeeded"
    );
    assert_ne!(
        first.body["data"]["execution"]["execution_id"],
        second.body["data"]["execution"]["execution_id"]
    );
}

#[tokio::test]
async fn starting_cancel_late_attach_binds_job_and_dispatches_compensating_stop() {
    let gate = Arc::new(execution::ExecutionAttachGate::new());
    let fixture = fixture_configured(20, {
        let gate = gate.clone();
        move |service| service.with_monitor_timing(80, 5).with_attach_gate(gate)
    })
    .await;
    let arguments = approve(&fixture, "starting-race-1", "sleep 30").await;
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });

    gate.wait_until_job_created().await;
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_job");
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;

    let cancellation = fixture
        .call(
            "task_cancel",
            json!({"task_id": fixture.task_id, "reason": "cancel during dispatch"}),
        )
        .await;
    assert!(cancellation.ok, "{}", cancellation.body);
    assert_eq!(
        cancellation.body["data"]["execution"]["execution_status"],
        "cancel_requested"
    );
    assert_eq!(fixture.connector.executions.active_monitor_count(), 0);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        fixture
            .connector
            .db
            .latest_connector_execution(
                &fixture.task_id,
                &fixture.connector.context.project_id,
                "user:u1",
                None,
            )
            .unwrap()
            .unwrap()
            .state,
        "cancel_requested"
    );

    let stop_registry = fixture.registry.clone();
    let expected_job_id = job_id.clone();
    let stopper = tokio::spawn(async move {
        let stop = next_request(&stop_registry).await;
        assert_eq!(stop.kind, "stop_job");
        assert_eq!(stop.job_id.as_deref(), Some(expected_job_id.as_str()));
        update_job(&stop_registry, &expected_job_id, "stopped", None, Some(-1)).await;
    });
    gate.release_attach().await;
    let completed = command_call.await.unwrap();
    stopper.await.unwrap();
    assert_eq!(
        completed.body["data"]["execution"]["execution_status"],
        "cancelled"
    );
    let execution_id = completed.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    let durable = fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap();
    assert_eq!(durable.executor_reference.as_deref(), Some(job_id.as_str()));
    assert_eq!(durable.state, "cancelled");
    assert_eq!(
        fixture.registry.get_job(&job_id).await.unwrap().status,
        "stopped"
    );
    assert!(fixture
        .registry
        .list_jobs(Some(10))
        .await
        .iter()
        .all(|job| !matches!(
            job.status.as_str(),
            "queued" | "agent_queued" | "running" | "stop_requested"
        )));
    for _ in 0..100 {
        let resources = workspace::WorkspaceManager::resource_status(
            Path::new(&fixture.connector.context.runs_root),
            fixture._temp.path().join("cargo-target").as_path(),
        );
        if resources.slot_state == "idle" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("cancelled workspace slot was not released");
}

#[tokio::test]
async fn retry_and_cancel_share_one_execution_monitor() {
    let fixture = fixture_configured(20, |service| service.with_monitor_timing(500, 5)).await;
    let arguments = approve(&fixture, "one-monitor-1", "sleep 30").await;
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let first_arguments = arguments.clone();
    let command_call =
        tokio::spawn(
            async move { call(&connector, &owner, "commands_run", first_arguments).await },
        );
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;
    let first = command_call.await.unwrap();
    assert!(first.ok, "{}", first.body);
    assert_eq!(fixture.connector.executions.monitor_start_count(), 1);
    assert_eq!(fixture.connector.executions.active_monitor_count(), 1);

    let retry = fixture.call("commands_run", arguments).await;
    assert!(retry.ok, "{}", retry.body);
    assert_eq!(fixture.connector.executions.monitor_start_count(), 1);
    assert_eq!(fixture.connector.executions.active_monitor_count(), 1);

    let stop_registry = fixture.registry.clone();
    let stop_job_id = job_id.clone();
    let stopper = tokio::spawn(async move {
        let stop = next_request(&stop_registry).await;
        assert_eq!(stop.kind, "stop_job");
        update_job(&stop_registry, &stop_job_id, "stopped", None, Some(-1)).await;
    });
    let cancelled = fixture
        .call("task_cancel", json!({"task_id": fixture.task_id}))
        .await;
    stopper.await.unwrap();
    assert_eq!(
        cancelled.body["data"]["execution"]["execution_status"],
        "cancelled"
    );
    assert_eq!(fixture.connector.executions.monitor_start_count(), 1);
    for _ in 0..100 {
        if fixture.connector.executions.active_monitor_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(fixture.connector.executions.active_monitor_count(), 0);
}

#[tokio::test]
async fn transient_unrecognized_status_recovers_within_grace() {
    let fixture = fixture_configured(20, |service| service.with_monitor_timing(200, 5)).await;
    let arguments = approve(&fixture, "transient-status-1", "sleep 30").await;
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "future-agent-state", None, None).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let degraded = fixture
        .connector
        .db
        .latest_connector_execution(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            "user:u1",
            None,
        )
        .unwrap()
        .unwrap();
    assert!(degraded.is_active());
    assert_ne!(degraded.state, "running");
    assert_eq!(
        degraded.status_failure_code.as_deref(),
        Some("executor_status_unrecognized")
    );
    let projection = execution::execution_projection(&degraded, 10, None);
    assert_eq!(projection["observation_status"], "degraded");

    update_job(&fixture.registry, &job_id, "running", None, None).await;
    for _ in 0..100 {
        let recovered = fixture
            .connector
            .db
            .connector_execution(&degraded.execution_id)
            .unwrap();
        if recovered.status_failure_code.is_none() && recovered.state == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let recovered = fixture
        .connector
        .db
        .connector_execution(&degraded.execution_id)
        .unwrap();
    assert_eq!(recovered.state, "running");
    assert_eq!(recovered.status_failure_code, None);
    update_job(&fixture.registry, &job_id, "completed", None, Some(0)).await;
    let _quick_yield = command_call.await.unwrap();
    for _ in 0..100 {
        let completed = fixture
            .connector
            .db
            .connector_execution(&degraded.execution_id)
            .unwrap();
        if completed.state == "succeeded" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("recovered execution did not reach succeeded");
}

#[tokio::test]
async fn status_transport_failure_becomes_unknown_only_after_grace() {
    let fixture = fixture_configured(5, |service| service.with_monitor_timing(80, 5)).await;
    let arguments = approve(&fixture, "transport-grace-1", "sleep 30").await;
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;
    let started = command_call.await.unwrap();
    let execution_id = started.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    fixture
        .registry
        .reconcile_disconnect("hosted", "instance")
        .await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let degraded = fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap();
    assert!(degraded.is_active());
    assert_eq!(
        degraded.status_failure_code.as_deref(),
        Some("executor_status_unavailable")
    );
    for _ in 0..100 {
        let current = fixture
            .connector
            .db
            .connector_execution(execution_id)
            .unwrap();
        if current.state == "unknown" {
            assert_eq!(current.executor_reference.as_deref(), Some(job_id.as_str()));
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("execution did not become unknown after the configured grace");
}

#[tokio::test]
async fn running_command_allows_review_wait_cancel_and_releases_slot() {
    let fixture = fixture(1_000).await;
    let arguments = approve(&fixture, "running-command-1", "sleep 30").await;
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_job");
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;

    let review_started = Instant::now();
    let initial = fixture
        .call(
            "task_review",
            json!({"task_id": fixture.task_id, "include_diff": false}),
        )
        .await;
    assert!(review_started.elapsed() < Duration::from_millis(500));
    assert!(initial.ok, "{}", initial.body);
    assert!(matches!(
        initial.body["data"]["active_execution"]["execution_status"].as_str(),
        Some("queued" | "running")
    ));

    let waiting_connector = fixture.connector.clone();
    let waiting_owner = fixture.owner.clone();
    let waiting_task = fixture.task_id.clone();
    let cursor = initial.body["event_cursor"].as_i64().unwrap();
    let waiting = tokio::spawn(async move {
        call(
            &waiting_connector,
            &waiting_owner,
            "task_review",
            json!({
                "task_id": waiting_task,
                "after_cursor": cursor,
                "wait_ms": 1_000,
                "include_diff": false
            }),
        )
        .await
    });
    update_job(
        &fixture.registry,
        &job_id,
        "running",
        Some("progress\n"),
        None,
    )
    .await;
    let progressed = waiting.await.unwrap();
    assert_eq!(progressed.body["data"]["heartbeat"], false);
    assert!(
        progressed.body["data"]["recent_execution"]["stdout_cursor"]
            .as_u64()
            .unwrap()
            > initial.body["data"]["recent_execution"]["stdout_cursor"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(
        progressed.body["data"]["recent_execution"]["output_tail"]["stdout"],
        "progress\n"
    );

    let finish = fixture
        .call(
            "task_finish",
            json!({"task_id": fixture.task_id, "summary": "too early"}),
        )
        .await;
    assert_eq!(finish.body["error"]["code"], "execution_not_terminal");

    let stop_registry = fixture.registry.clone();
    let stop_job_id = job_id.clone();
    let stopper = tokio::spawn(async move {
        let stop = next_request(&stop_registry).await;
        assert_eq!(stop.kind, "stop_job");
        assert_eq!(stop.job_id.as_deref(), Some(stop_job_id.as_str()));
        update_job(&stop_registry, &stop_job_id, "stopped", None, Some(-1)).await;
    });
    let cancelled = fixture
        .call(
            "task_cancel",
            json!({"task_id": fixture.task_id, "reason": "user stopped the task"}),
        )
        .await;
    stopper.await.unwrap();
    let command = command_call.await.unwrap();
    assert!(cancelled.ok, "{}", cancelled.body);
    assert_eq!(cancelled.body["data"]["status"], "cancelled");
    assert_eq!(
        cancelled.body["data"]["execution"]["execution_status"],
        "cancelled"
    );
    assert_eq!(
        command.body["data"]["execution"]["execution_id"],
        cancelled.body["data"]["execution"]["execution_id"]
    );
    let resources = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(resources.slot_state, "idle");
}

#[tokio::test]
async fn queued_cancel_never_dispatches_and_restart_is_fail_closed() {
    let queued_fixture = fixture(50).await;
    let arguments = approve(&queued_fixture, "queued-command-1", "sleep 30").await;
    let queued = queued_fixture.call("commands_run", arguments).await;
    assert_eq!(
        queued.body["data"]["execution"]["execution_status"],
        "queued"
    );
    assert_eq!(
        queued.body["data"]["execution"]["queue_reason"],
        "executor_queue"
    );
    let heartbeat = queued_fixture
        .call(
            "task_review",
            json!({
                "task_id": queued_fixture.task_id,
                "after_cursor": queued.body["event_cursor"],
                "wait_ms": 30,
                "include_diff": false
            }),
        )
        .await;
    assert_eq!(heartbeat.body["data"]["heartbeat"], true);
    let cancelled = queued_fixture
        .call("task_cancel", json!({"task_id": queued_fixture.task_id}))
        .await;
    assert_eq!(
        cancelled.body["data"]["execution"]["execution_status"],
        "cancelled"
    );
    assert!(poll(&queued_fixture.registry).await.is_none());

    let second = fixture(50).await;
    let execution = created(
        second
            .connector
            .executions
            .reserve_command(&task(&second), "restart-operation", "restart-hash", 30, 10)
            .unwrap(),
    );
    let recovery = second
        .connector
        .db
        .reconcile_connector_executions(&second.connector.context.project_id, 11)
        .unwrap();
    assert_eq!(recovery.1, 1);
    assert_eq!(
        second
            .connector
            .db
            .connector_execution(&execution.execution_id)
            .unwrap()
            .state,
        "interrupted"
    );
    assert_eq!(task(&second).task_status, "needs_attention");

    let resumed = second
        .connector
        .db
        .resume_connector_task(
            &second.task_id,
            &second.connector.context.project_id,
            "local_cli",
            12,
        )
        .unwrap();
    let unknown = created(
        second
            .connector
            .executions
            .reserve_command(&resumed, "unknown-operation", "unknown-hash", 30, 13)
            .unwrap(),
    );
    second
        .connector
        .db
        .finish_connector_execution(
            &unknown.execution_id,
            crate::db::ConnectorExecutionFailure::Unknown("transport_lost"),
            14,
        )
        .unwrap();
    let finish = second
        .call(
            "task_finish",
            json!({"task_id": second.task_id, "summary": "must not finish"}),
        )
        .await;
    assert_eq!(finish.body["error"]["code"], "execution_not_terminal");
    assert_eq!(
        finish.body["data"]["execution"]["execution_status"],
        "unknown"
    );
}

#[tokio::test]
async fn cancellation_transport_unknown_preserves_executor_reference_and_blocks_finish() {
    let fixture = fixture(20).await;
    let execution = created(
        fixture
            .connector
            .executions
            .reserve_command(
                &task(&fixture),
                "cancel-transport-1",
                "cancel-transport-hash",
                30,
                2,
            )
            .unwrap(),
    );
    let job_id = "22222222-2222-4222-8222-222222222222";
    fixture
        .connector
        .db
        .attach_connector_executor(&execution.execution_id, job_id, "running", 3)
        .unwrap();
    let cancelled = fixture
        .call("task_cancel", json!({"task_id": fixture.task_id}))
        .await;
    assert_eq!(
        cancelled.body["data"]["execution"]["execution_status"],
        "unknown"
    );
    let durable = fixture
        .connector
        .db
        .connector_execution(&execution.execution_id)
        .unwrap();
    assert_eq!(durable.executor_reference.as_deref(), Some(job_id));
    let finish = fixture
        .call(
            "task_finish",
            json!({"task_id": fixture.task_id, "summary": "must stay blocked"}),
        )
        .await;
    assert_eq!(finish.body["error"]["code"], "execution_not_terminal");
}

#[tokio::test]
async fn failed_cancelled_workspace_release_can_be_retried() {
    let fixture = fixture(20).await;
    let good_task = task(&fixture);
    let mut bad_task = good_task.clone();
    bad_task.target_root = fixture
        ._temp
        .path()
        .join("not-a-git-checkout")
        .to_string_lossy()
        .into_owned();
    fixture
        .connector
        .executions
        .release_cancelled_workspace(bad_task)
        .await;
    let occupied = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(occupied.slot_state, "occupied");

    fixture
        .connector
        .executions
        .release_cancelled_workspace(good_task)
        .await;
    let released = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(released.slot_state, "idle");
}

#[tokio::test]
async fn wait_for_terminal_propagates_store_error_without_panicking() {
    let fixture = fixture(20).await;
    let execution = created(
        fixture
            .connector
            .executions
            .reserve_command(&task(&fixture), "store-error-1", "store-error-hash", 30, 2)
            .unwrap(),
    );
    fixture
        .connector
        .db
        .conn_for_tests()
        .execute("DROP TABLE wc_executions", [])
        .unwrap();
    let error = fixture
        .connector
        .executions
        .wait_for_terminal(&execution.execution_id, 20)
        .await
        .unwrap_err();
    assert!(matches!(error, ConnectorTaskStoreError::Storage(_)));
}

#[tokio::test]
async fn nonzero_exit_keeps_submission_and_execution_outcomes_separate() {
    let fixture = fixture(50).await;
    let db = &fixture.connector.db;
    let execution = created(
        db.reserve_connector_execution(&task(&fixture), "nonzero-operation", "nonzero-hash", 30, 2)
            .unwrap(),
    );
    db.attach_connector_executor(&execution.execution_id, "job-1", "running", 4)
        .unwrap();
    let mut observed = None;
    for (status, exit_code, finished_at, now) in [
        ("running", None, None, 4),
        ("running", None, None, 4),
        ("completed", Some(7), Some(5), 5),
    ] {
        observed = Some(
            db.observe_connector_execution(
                &execution.execution_id,
                ConnectorExecutionObservation {
                    executor_status: status,
                    stdout_cursor: 2,
                    stderr_cursor: 1,
                    exit_code,
                    started_at: Some(4),
                    finished_at,
                    now,
                },
            )
            .unwrap(),
        );
    }
    let failed = observed.unwrap();
    let projection = execution::execution_projection(
        &failed,
        5,
        Some(json!({"stdout": "once\n", "stderr": "", "bounded": true})),
    );
    assert_eq!(projection["submission_status"], "accepted");
    assert_eq!(projection["execution_status"], "failed");
    assert_eq!(projection["exit_code"], 7);
    assert_eq!(projection["capability_outcome"], "failed");
    assert_eq!(projection["output_tail"]["stdout"], "once\n");
}

#[tokio::test]
async fn unrecognized_executor_status_is_degraded_instead_of_running() {
    let fixture = fixture(50).await;
    let db = &fixture.connector.db;
    let execution = created(
        db.reserve_connector_execution(
            &task(&fixture),
            "unrecognized-status",
            "unrecognized-hash",
            30,
            2,
        )
        .unwrap(),
    );
    db.attach_connector_executor(&execution.execution_id, "job-unknown", "queued", 3)
        .unwrap();
    let observed = db
        .observe_connector_execution(
            &execution.execution_id,
            ConnectorExecutionObservation {
                executor_status: "future-agent-state",
                stdout_cursor: 1,
                stderr_cursor: 1,
                exit_code: None,
                started_at: None,
                finished_at: None,
                now: 4,
            },
        )
        .unwrap();
    assert_eq!(observed.state, "queued");
    assert_eq!(
        observed.status_failure_code.as_deref(),
        Some("executor_status_unrecognized")
    );
}

#[tokio::test]
async fn read_only_task_denies_consequential_capability_before_executor_dispatch() {
    let (_temp, connector) = tests::connector();
    let owner = tests::auth("u1");
    let started = connector
        .call(
            "task_start",
            json!({ "goal": "inspect only", "mode": "read_only" }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap();
    let outcome = connector
        .call(
            "edits_apply",
            json!({
                "task_id": task_id,
                "operation_id": "read-only-probe",
                "changes": [{
                    "kind": "edit",
                    "path": "src/lib.rs",
                    "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "edits": [{
                        "kind": "replace_exact",
                        "old_text": "old",
                        "new_text": "new"
                    }]
                }]
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.http_status, 403);
    assert_eq!(outcome.body["error"]["code"], "read_only_task");
    assert_eq!(outcome.body["event_cursor"], 2);
}

#[tokio::test]
async fn edits_apply_replays_durable_result_without_executor_dispatch() {
    let (_temp, connector) = tests::connector();
    let owner = tests::auth("u1");
    let now = chrono::Utc::now().timestamp();
    connector
        .db
        .ensure_connector_binding(ConnectorBinding {
            project_id: &connector.context.project_id,
            project_name: &connector.context.project_name,
            workspace_id: &connector.context.workspace_id,
            executor_ref: &connector.context.executor_project,
            subject_id: "user:u1",
            profile: &connector.context.profile,
            now,
        })
        .unwrap();
    let task_id = "wc_task_abcdef0123456789abcdef0123456789";
    let run_id = "wc_run_abcdef0123456789abcdef0123456789";
    let prepared = connector
        .workspace
        .prepare(&connector.context, task_id, run_id, false)
        .unwrap();
    let task = connector
        .db
        .start_connector_task(NewConnectorTask {
            task_id,
            run_id,
            project_id: &connector.context.project_id,
            workspace_id: &connector.context.workspace_id,
            subject_id: "user:u1",
            goal: "replay one edit",
            mode: "normal",
            target_executor_ref: &connector.context.executor_project,
            execution_executor_ref: &prepared.execution_executor_ref,
            target_root: &connector.context.executor_root,
            execution_root: &prepared.execution_root,
            baseline_commit: prepared.baseline_commit.as_deref(),
            baseline_tree: prepared.baseline_tree.as_deref(),
            isolated: true,
            now,
        })
        .unwrap();
    let changes_json = json!([{
        "kind": "edit",
        "path": "README.md",
        "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "edits": [{"kind": "replace_exact", "old_text": "fixture", "new_text": "updated"}]
    }]);
    let changes: Vec<ApplyFileChangeInput> = serde_json::from_value(changes_json.clone()).unwrap();
    let request_sha256 = edit_operation_hash(&task, &changes, false);
    assert_eq!(
        connector
            .db
            .begin_connector_edit_operation(
                task_id,
                &connector.context.project_id,
                "user:u1",
                "device-retry-1",
                &request_sha256,
                now,
            )
            .unwrap(),
        ConnectorEditOperationGate::Started
    );
    connector
        .db
        .complete_connector_edit_operation(
            task_id,
            &connector.context.project_id,
            "user:u1",
            "device-retry-1",
            &request_sha256,
            &json!({"changed": true, "changed_paths": ["README.md"]}),
            now,
        )
        .unwrap();

    let outcome = connector
        .call(
            "edits_apply",
            json!({
                "task_id": task_id,
                "operation_id": "device-retry-1",
                "changes": changes_json
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(outcome.ok, "{}", outcome.body);
    assert_eq!(outcome.body["data"]["idempotent_replay"], true);
    assert_eq!(outcome.body["data"]["changed_paths"], json!(["README.md"]));
    assert_eq!(
        connector
            .workspace
            .discard_prepared(&connector.context.executor_root, &prepared),
        None
    );
}
