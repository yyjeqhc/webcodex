use super::wire_models::sanitize_value;
use super::*;
use crate::db::{
    ConnectorExecutionContinuationIntent, ConnectorExecutionFailure, ConnectorExecutionObservation,
};
use crate::shell_client::{ShellClientRegistry, ShellJobStartMetadata};
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentPollRequest, ShellAgentProjectSummary,
    ShellAgentResultRequest, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellJobOpRequest, ShellJobValidationProgress,
    ShellJobValidationStep, VALIDATION_STEP_WAIT_FAILED_CODE,
};
use crate::tool_runtime::validation_profile::{RecipeId, SemanticCheck};
use crate::tool_runtime::ApplyFileChangeInput;
use salvo::test::ResponseExt;
use std::time::{Duration, Instant};

#[tokio::test]
async fn another_project_grant_cannot_observe_or_use_a_task_id() {
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
    assert_eq!(outcome.http_status, 403);
    assert_eq!(outcome.body["error"]["code"], "project_credential_rejected");
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

/// Fixture for the console HTTP tests: the runtime plus the client id the
/// registry knows, so a test can write activity for a client that is genuinely
/// visible to grant A and also live for grant B.
pub(crate) struct ConsoleFixture {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) runtime: Arc<ConnectorRuntime>,
    /// Registered under grant A only.
    pub(crate) own_client_id: String,
    /// Registered under grant B, so grant A can see the id exists without
    /// that meaning it may read grant B's history.
    pub(crate) shared_client_id: String,
}

pub(crate) async fn console_fixture() -> ConsoleFixture {
    let fixture = fixture(20).await;
    // The same registered client is visible to both grants in these tests,
    // which is exactly the situation a client id must not authorize.
    let grant_b = tests::auth("u2");
    fixture
        .registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "laptop".into(),
                agent_instance_id: "instance-b".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                host_context: None,
                capabilities: Some(crate::test_support::current_runner_capabilities(
                    ShellClientCapabilities::default(),
                )),
                projects: None,
                agent_protocol_version: Some("polling-v1".into()),
                policy: None,
            },
            Some(&grant_b),
        )
        .await
        .unwrap();
    ConsoleFixture {
        _temp: fixture._temp,
        runtime: fixture.connector,
        own_client_id: "hosted".to_string(),
        shared_client_id: "laptop".to_string(),
    }
}

async fn fixture(yield_ms: u64) -> Fixture {
    fixture_configured(yield_ms, |service| service).await
}

/// Restricted-authority fixture for the lanes that protect the human-approval
/// machinery; the default fixture runs under trusted_agent like production.
async fn fixture_restricted(yield_ms: u64) -> Fixture {
    fixture_built(yield_ms, |service| service, true).await
}

async fn fixture_configured(
    yield_ms: u64,
    configure: impl FnOnce(execution::ExecutionService) -> execution::ExecutionService,
) -> Fixture {
    fixture_built(yield_ms, configure, false).await
}

async fn fixture_built(
    yield_ms: u64,
    configure: impl FnOnce(execution::ExecutionService) -> execution::ExecutionService,
    restricted: bool,
) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let state = temp.path().join("state");
    tests::init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    let owner = tests::auth("u1");
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                host_context: None,
                capabilities: Some(crate::test_support::current_runner_capabilities(
                    ShellClientCapabilities {
                        shell: true,
                        internal_posix_script: true,
                        ..Default::default()
                    },
                )),
                projects: Some(vec![project_summary("project", &project)]),
                agent_protocol_version: Some("polling-v1".into()),
                policy: None,
            },
            Some(&owner),
        )
        .await
        .unwrap();
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let mut tools = ToolRuntime::new_for_tests_with_shell_clients(registry.clone());
    if restricted {
        tools = tools.with_permission_evaluator(
            crate::tool_runtime::permissions::PermissionEvaluator::with_mode(
                crate::tool_runtime::permissions::AuthorityMode::Restricted,
            ),
        );
    }
    let tools = Arc::new(tools);
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
            project_grant_id: tests::PROJECT_GRANT_ID.into(),
        },
        tests::credential(),
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
        revision: None,
        git_branch: Some("main".into()),
        git_head: None,
        git_dirty: Some(false),
        updated_at: 1,
        shell_profile: None,
    }
}

async fn next_request(registry: &ShellClientRegistry) -> ShellAgentShellRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(request) = poll(registry).await {
            return request;
        }
        if Instant::now() >= deadline {
            panic!("Connector agent dispatch readiness failed: no request dispatched within 10 seconds");
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
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

fn latest_execution(fixture: &Fixture) -> crate::db::ConnectorExecution {
    fixture
        .connector
        .db
        .latest_connector_execution(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            tests::PROJECT_SUBJECT_ID,
            None,
        )
        .unwrap()
        .expect("connector execution should exist")
}

fn execution_by_id(fixture: &Fixture, execution_id: &str) -> crate::db::ConnectorExecution {
    fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap()
}

async fn wait_for_execution(
    fixture: &Fixture,
    execution_id: Option<&str>,
    timeout: Duration,
    description: &str,
    predicate: impl Fn(&crate::db::ConnectorExecution) -> bool,
) -> crate::db::ConnectorExecution {
    let deadline = Instant::now() + timeout;
    loop {
        let current = match execution_id {
            Some(execution_id) => execution_by_id(fixture, execution_id),
            None => latest_execution(fixture),
        };
        if predicate(&current) {
            return current;
        }
        if Instant::now() >= deadline {
            panic!(
                "{description} did not become observable within {timeout:?}; last state={} status_failure={:?} executor_reference={:?}",
                current.state, current.status_failure_code, current.executor_reference
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn wait_for_monitor_count(
    fixture: &Fixture,
    expected: usize,
    timeout: Duration,
    description: &str,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let current = fixture.connector.executions.active_monitor_count();
        if current == expected {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "{description} did not reach active monitor count {expected} within {timeout:?}; last count={current}"
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn wait_for_workspace_slot_state(
    fixture: &Fixture,
    expected: &str,
    timeout: Duration,
    description: &str,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let resources = workspace::WorkspaceManager::resource_status(
            Path::new(&fixture.connector.context.runs_root),
            fixture._temp.path().join("cargo-target").as_path(),
        );
        if resources.slot_state == expected {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "{description} did not reach workspace slot state {expected} within {timeout:?}; last state={}",
                resources.slot_state
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn executor_status_observation<'a>(
    executor_status: &'a str,
    stdout_cursor: usize,
    stderr_cursor: usize,
    started_at: Option<i64>,
    now: i64,
) -> ConnectorExecutionObservation<'a> {
    ConnectorExecutionObservation {
        executor_status,
        stdout_cursor,
        stderr_cursor,
        exit_code: None,
        started_at,
        finished_at: None,
        check_completed: None,
        failed_check: None,
        assertion_evidence: None,
        validated_workspace_sha256: None,
        executor_failure_code: None,
        mcp_task_output_tail: None,
        now,
    }
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
            tests::PROJECT_SUBJECT_ID,
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

fn command_arguments(fixture: &Fixture, operation_id: &str, command: &str) -> Value {
    json!({
        "task_id": fixture.task_id,
        "operation_id": operation_id,
        "command": command,
        "timeout_secs": 30
    })
}

fn checks(fixture: &Fixture, operation_id: &str, plan: &[&str]) -> Value {
    json!({
        "task_id": fixture.task_id,
        "operation_id": operation_id,
        "checks": plan,
        "timeout_secs": 30
    })
}

fn check_progress(
    completed: usize,
    current: Option<&str>,
    failed: Option<&str>,
) -> ShellJobValidationProgress {
    ShellJobValidationProgress {
        completed,
        current_step: current.map(str::to_string),
        failed_step: failed.map(str::to_string),
    }
}

fn job_start_request() -> ShellJobOpRequest {
    ShellJobOpRequest {
        op: "start".into(),
        client_id: Some("hosted".into()),
        cwd: None,
        command: Some("true".into()),
        timeout_secs: Some(30),
        job_id: None,
        since_stdout_line: None,
        since_stderr_line: None,
        tail_lines: None,
        limit: None,
        codex: None,
    }
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
            update_seq: None,
            job_id: job_id.into(),
            request_id: None,
            status: status.into(),
            stdout_chunk: stdout.map(str::to_string),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code,
            duration_ms: Some(1),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: matches!(status, "completed" | "failed" | "stopped"),
        })
        .await
        .unwrap();
}

async fn update_validation_job(
    registry: &ShellClientRegistry,
    job_id: &str,
    status: &str,
    stdout: Option<&str>,
    exit_code: Option<i32>,
    progress: ShellJobValidationProgress,
) {
    let mut update = validation_job_update(job_id, status, progress);
    update.stdout_chunk = stdout.map(str::to_string);
    update.exit_code = exit_code;
    registry.update_job(update).await.unwrap();
}

fn validation_job_update(
    job_id: &str,
    status: &str,
    progress: ShellJobValidationProgress,
) -> ShellAgentJobUpdateRequest {
    ShellAgentJobUpdateRequest {
        client_id: "hosted".into(),
        agent_instance_id: "instance".into(),
        update_seq: None,
        job_id: job_id.into(),
        request_id: None,
        status: status.into(),
        stdout_chunk: None,
        stderr_chunk: None,
        stdout_tail: None,
        stderr_tail: None,
        log_snapshot: None,
        exit_code: None,
        duration_ms: Some(1),
        error: None,
        command_execution_state: None,
        validation_progress: Some(progress),
        finished: matches!(status, "completed" | "failed" | "stopped"),
    }
}

async fn terminal_check(
    fixture: &Fixture,
    operation_id: &str,
    plan: &[&str],
    status: &str,
    exit_code: i32,
    stdout: Option<String>,
    progress: ShellJobValidationProgress,
) -> ConnectorCallOutcome {
    let registry = fixture.registry.clone();
    let status = status.to_string();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            &status,
            stdout.as_deref(),
            Some(exit_code),
            progress,
        )
        .await;
    });
    let outcome = fixture
        .call("checks_run", checks(fixture, operation_id, plan))
        .await;
    responder.await.unwrap();
    outcome
}

async fn finish(fixture: &Fixture, summary: &str) -> ConnectorCallOutcome {
    fixture
        .call(
            "task_finish",
            json!({"task_id": fixture.task_id, "summary": summary}),
        )
        .await
}

async fn complete_create_edit(
    fixture: &Fixture,
    request: ShellAgentShellRequest,
    path: &str,
    content: &str,
) {
    std::fs::write(Path::new(&task(fixture).execution_root).join(path), content).unwrap();
    fixture
        .registry
        .complete(ShellAgentResultRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                json!({
                    "dry_run": false,
                    "applied_count": 1,
                    "changed": true,
                    "would_change": true,
                    "files": [{"index": 0, "kind": "create", "path": path}],
                    "changed_paths": [path]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn normal_task_finish_requires_structured_checks() {
    let fixture = fixture(1_000).await;
    let outcome = finish(&fixture, "unchecked result").await;
    assert_eq!(outcome.body["error"]["code"], "checks_required");
    assert_eq!(outcome.body["error"]["retryable"], false);
    assert_eq!(outcome.body["error"]["user_action_required"], true);
    assert_eq!(
        outcome.body["error"]["suggested_action"],
        "Call checks_run with a new operation_id, then retry task_finish."
    );
}

#[tokio::test]
async fn connector_readiness_uses_registered_agent_capabilities() {
    let fixture = fixture(1_000).await;
    let ready = fixture.connector.readiness(&fixture.owner).await.unwrap();
    assert!(ready.ready);

    fixture
        .registry
        .reconcile_disconnect("hosted", "instance")
        .await;
    let offline = fixture.connector.readiness(&fixture.owner).await.unwrap();
    assert!(offline
        .findings
        .iter()
        .any(|finding| finding.code == "agent_offline"));
}

#[tokio::test]
async fn quick_yield_arms_terminal_continuation_before_return_and_replay_keeps_single_intent() {
    let fixture = fixture(20).await;
    let arguments = command_arguments(&fixture, "continuation-yield-1", "sleep 30");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let call =
        tokio::spawn(
            async move { call(&connector, &owner, "commands_run", arguments.clone()).await },
        );
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.kind, "start_job");
    let job_id = request.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;

    let yielded = call.await.unwrap();
    assert!(yielded.ok, "{}", yielded.body);
    assert!(matches!(
        yielded.body["data"]["execution"]["execution_status"].as_str(),
        Some("queued" | "running")
    ));
    let execution_id = yielded.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap()
        .to_string();
    let armed = execution_by_id(&fixture, &execution_id);
    assert_eq!(
        armed.continuation_intent,
        ConnectorExecutionContinuationIntent::ArmedForTerminal
    );
    let armed_at = armed
        .continuation_armed_at
        .expect("active yielded execution must be durably armed");

    let reopened = Database::open(&fixture._temp.path().join("connector.db")).unwrap();
    let durable = reopened.connector_execution(&execution_id).unwrap();
    assert_eq!(durable.continuation_armed_at, Some(armed_at));
    drop(reopened);

    let replay = fixture
        .call(
            "commands_run",
            command_arguments(&fixture, "continuation-yield-1", "sleep 30"),
        )
        .await;
    assert!(replay.ok, "{}", replay.body);
    assert_eq!(
        replay.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert!(poll(&fixture.registry).await.is_none());
    assert_eq!(
        execution_by_id(&fixture, &execution_id).continuation_armed_at,
        Some(armed_at)
    );

    update_job(&fixture.registry, &job_id, "completed", None, Some(0)).await;
    let completed = wait_for_execution(
        &fixture,
        Some(&execution_id),
        Duration::from_secs(10),
        "armed yielded execution terminal continuation readiness",
        |execution| execution.state == "succeeded",
    )
    .await;
    assert_eq!(
        completed.continuation_intent,
        ConnectorExecutionContinuationIntent::ArmedForTerminal
    );
    assert_eq!(completed.continuation_armed_at, Some(armed_at));
    let ready = fixture
        .connector
        .db
        .terminal_ready_connector_executions()
        .unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].execution_id, execution_id);
}

#[tokio::test]
async fn mcp_task_polling_quick_yield_replays_exact_armed_execution() {
    let fixture = fixture(20).await;
    let arguments = command_arguments(&fixture, "mcp-task-yield-1", "sleep 30");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let call_arguments = arguments.clone();
    let call = tokio::spawn(async move {
        connector
            .call_for_window_with_task_polling(
                "commands_run",
                call_arguments,
                Some(&owner),
                ConnectorTransport::Mcp,
                None,
            )
            .await
    });
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.kind, "start_job");
    let job_id = request.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;

    let yielded = call.await.unwrap();
    assert!(yielded.ok, "{}", yielded.body);
    let execution_id = yielded.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap()
        .to_string();
    let armed = execution_by_id(&fixture, &execution_id);
    assert!(armed.is_active());
    assert!(armed.terminal_continuation_is_armed());

    let before_materialization = fixture
        .connector
        .execution_task_result_for_auth(&execution_id, &fixture.owner)
        .await;
    let Err(before_materialization) = before_materialization else {
        panic!("an armed execution is not an MCP Task until materialization");
    };
    assert_eq!(before_materialization.http_status, 404);
    let materialized_execution = fixture
        .connector
        .materialize_execution_task_for_auth(&execution_id, &fixture.owner)
        .unwrap();
    assert!(materialized_execution.mcp_task_is_materialized());

    let (_task, materialized, working_outcome) = fixture
        .connector
        .execution_task_result_for_auth(&execution_id, &fixture.owner)
        .await
        .unwrap();
    assert_eq!(materialized.execution_id, execution_id);
    assert!(materialized.is_active());
    assert_eq!(
        working_outcome.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert_eq!(working_outcome.body["blocking"], true);

    let foreign = tests::auth("foreign-grant");
    let denied = fixture
        .connector
        .execution_task_result_for_auth(&execution_id, &foreign)
        .await;
    let Err(denied) = denied else {
        panic!("foreign project credential must not resolve an execution task id");
    };
    assert_eq!(denied.http_status, 404);
    assert!(!denied.body.to_string().contains(&execution_id));

    let replay = fixture
        .connector
        .call_for_window_with_task_polling(
            "commands_run",
            arguments,
            Some(&fixture.owner),
            ConnectorTransport::Mcp,
            None,
        )
        .await;
    assert!(replay.ok, "{}", replay.body);
    assert_eq!(
        replay.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert!(poll(&fixture.registry).await.is_none());
    assert!(execution_by_id(&fixture, &execution_id).terminal_continuation_is_armed());

    update_job(
        &fixture.registry,
        &job_id,
        "completed",
        Some("durable task tail\n"),
        Some(0),
    )
    .await;
    let completed = wait_for_execution(
        &fixture,
        Some(&execution_id),
        Duration::from_secs(10),
        "MCP task-polling execution completion",
        |execution| execution.state == "succeeded" && execution.mcp_task_result_is_finalized(),
    )
    .await;
    let (_task, terminal, terminal_outcome) = fixture
        .connector
        .execution_task_result_for_auth(&execution_id, &fixture.owner)
        .await
        .unwrap();
    assert_eq!(terminal.execution_id, completed.execution_id);
    assert!(!terminal.is_active());
    assert_eq!(terminal_outcome.body["blocking"], false);
    assert_eq!(
        terminal.mcp_task_output_tail.as_ref().unwrap()["stdout"],
        "durable task tail\n"
    );
    assert!(fixture
        .connector
        .db
        .terminal_ready_connector_executions()
        .unwrap()
        .iter()
        .all(|ready| ready.execution_id != execution_id));
}

#[tokio::test]
async fn mcp_tools_call_tasks_extension_switches_only_active_command_results_to_tasks() {
    const PROJECT_CREDENTIAL: &str =
        "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PROTOCOL_VERSION: &str = "2026-07-28";
    const TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";

    let fixture = fixture(20).await;
    let mcp_runtime = Arc::new(
        ToolRuntime::new_for_tests_with_shell_clients(fixture.registry.clone())
            .with_runtime_exposure(crate::model_surface::RuntimeExposure::ProjectConnector),
    );
    let service = Arc::new(salvo::Service::new(
        salvo::Router::new()
            .hoop(salvo::affix_state::inject(
                crate::test_support::test_config(Some("secret")),
            ))
            .hoop(salvo::affix_state::inject(fixture.connector.db.clone()))
            .hoop(salvo::affix_state::inject(mcp_runtime))
            .hoop(salvo::affix_state::inject(ConnectorRuntimeSlot(Some(
                fixture.connector.clone(),
            ))))
            .push(
                salvo::Router::with_path("mcp")
                    .hoop(crate::AuthMiddleware)
                    .post(crate::mcp::mcp_post),
            ),
    ));
    let arguments = command_arguments(&fixture, "mcp-http-task-yield-1", "sleep 30");

    let service_for_call = service.clone();
    let first_arguments = arguments.clone();
    let first_call = tokio::spawn(async move {
        salvo::test::TestClient::post("http://localhost/mcp")
            .bearer_auth(PROJECT_CREDENTIAL)
            .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
            .add_header("mcp-method", "tools/call", true)
            .add_header("mcp-name", "commands_run", true)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 401,
                "method": "tools/call",
                "params": {
                    "name": "commands_run",
                    "arguments": first_arguments,
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                        "io.modelcontextprotocol/clientCapabilities": {
                            "extensions": { TASKS_EXTENSION: {} }
                        }
                    }
                }
            }))
            .send(service_for_call.as_ref())
            .await
    });
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.kind, "start_job");
    let job_id = request.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;

    let mut first = first_call.await.unwrap();
    assert_eq!(
        first.status_code.unwrap_or(salvo::http::StatusCode::OK),
        salvo::http::StatusCode::OK
    );
    let first_body: Value = first.take_json().await.unwrap();
    assert_eq!(first_body["result"]["resultType"], "task");
    assert_eq!(first_body["result"]["status"], "working");
    let execution_id = first_body["result"]["taskId"]
        .as_str()
        .expect("active task-augmented call must expose durable execution id")
        .to_string();
    assert_eq!(
        execution_by_id(&fixture, &execution_id).terminal_continuation_is_armed(),
        true
    );
    assert!(execution_by_id(&fixture, &execution_id).mcp_task_is_materialized());

    let mut no_capability = salvo::test::TestClient::post("http://localhost/mcp")
        .bearer_auth(PROJECT_CREDENTIAL)
        .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
        .add_header("mcp-method", "tools/call", true)
        .add_header("mcp-name", "commands_run", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 402,
            "method": "tools/call",
            "params": {
                "name": "commands_run",
                "arguments": arguments.clone(),
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }))
        .send(service.as_ref())
        .await;
    let no_capability_body: Value = no_capability.take_json().await.unwrap();
    assert_eq!(no_capability_body["result"]["resultType"], "complete");
    assert_eq!(
        no_capability_body["result"]["structuredContent"]["data"]["execution"]["execution_id"],
        execution_id
    );
    assert_eq!(
        no_capability_body["result"]["structuredContent"]["blocking"],
        true
    );
    let guided = fixture.connector.host_guide(
        &fixture.task_id,
        "task polling must not consume this guidance",
    );
    assert!(guided.ok, "{}", guided.body);

    let mut replay = salvo::test::TestClient::post("http://localhost/mcp")
        .bearer_auth(PROJECT_CREDENTIAL)
        .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
        .add_header("mcp-method", "tools/call", true)
        .add_header("mcp-name", "commands_run", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 403,
            "method": "tools/call",
            "params": {
                "name": "commands_run",
                "arguments": arguments.clone(),
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {
                        "extensions": { TASKS_EXTENSION: {} }
                    }
                }
            }
        }))
        .send(service.as_ref())
        .await;
    let replay_body: Value = replay.take_json().await.unwrap();
    assert_eq!(replay_body["result"]["resultType"], "task");
    assert_eq!(replay_body["result"]["taskId"], execution_id);
    assert!(poll(&fixture.registry).await.is_none());

    update_job(
        &fixture.registry,
        &job_id,
        "completed",
        Some("http durable final\n"),
        Some(0),
    )
    .await;
    wait_for_execution(
        &fixture,
        Some(&execution_id),
        Duration::from_secs(10),
        "MCP task-augmented HTTP execution completion",
        |execution| execution.state == "succeeded" && execution.mcp_task_result_is_finalized(),
    )
    .await;

    let mut terminal = salvo::test::TestClient::post("http://localhost/mcp")
        .bearer_auth(PROJECT_CREDENTIAL)
        .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
        .add_header("mcp-method", "tools/call", true)
        .add_header("mcp-name", "commands_run", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 404,
            "method": "tools/call",
            "params": {
                "name": "commands_run",
                "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {
                        "extensions": { TASKS_EXTENSION: {} }
                    }
                }
            }
        }))
        .send(service.as_ref())
        .await;
    let terminal_body: Value = terminal.take_json().await.unwrap();
    assert_eq!(terminal_body["result"]["resultType"], "task");
    assert_eq!(terminal_body["result"]["taskId"], execution_id);
    assert_eq!(terminal_body["result"]["status"], "completed");
    assert!(poll(&fixture.registry).await.is_none());

    let mut polled = salvo::test::TestClient::post("http://localhost/mcp")
        .bearer_auth(PROJECT_CREDENTIAL)
        .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
        .add_header("mcp-method", "tasks/get", true)
        .add_header("mcp-name", &execution_id, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 405,
            "method": "tasks/get",
            "params": {
                "taskId": execution_id,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {
                        "extensions": { TASKS_EXTENSION: {} }
                    }
                }
            }
        }))
        .send(service.as_ref())
        .await;
    let polled_body: Value = polled.take_json().await.unwrap();
    assert_eq!(polled_body["result"]["status"], "completed");
    assert_eq!(
        polled_body["result"]["result"]["structuredContent"]["data"]["execution"]["output_tail"]
            ["stdout"],
        "http durable final\n"
    );
    let review = fixture
        .call("task_review", json!({ "task_id": fixture.task_id }))
        .await;
    assert!(review.ok, "{}", review.body);
    assert_eq!(
        review.body["data"]["guidance"][0]["message"],
        "task polling must not consume this guidance"
    );
}

#[tokio::test]
async fn mcp_tools_call_tasks_extension_keeps_terminal_before_yield_ordinary() {
    const PROJECT_CREDENTIAL: &str =
        "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PROTOCOL_VERSION: &str = "2026-07-28";
    const TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";

    let fixture = fixture(1_000).await;
    let mcp_runtime = Arc::new(
        ToolRuntime::new_for_tests_with_shell_clients(fixture.registry.clone())
            .with_runtime_exposure(crate::model_surface::RuntimeExposure::ProjectConnector),
    );
    let service = Arc::new(salvo::Service::new(
        salvo::Router::new()
            .hoop(salvo::affix_state::inject(
                crate::test_support::test_config(Some("secret")),
            ))
            .hoop(salvo::affix_state::inject(fixture.connector.db.clone()))
            .hoop(salvo::affix_state::inject(mcp_runtime))
            .hoop(salvo::affix_state::inject(ConnectorRuntimeSlot(Some(
                fixture.connector.clone(),
            ))))
            .push(
                salvo::Router::with_path("mcp")
                    .hoop(crate::AuthMiddleware)
                    .post(crate::mcp::mcp_post),
            ),
    ));
    let arguments = command_arguments(&fixture, "mcp-http-sync-terminal-1", "printf sync");
    let service_for_call = service.clone();
    let call = tokio::spawn(async move {
        salvo::test::TestClient::post("http://localhost/mcp")
            .bearer_auth(PROJECT_CREDENTIAL)
            .add_header("mcp-protocol-version", PROTOCOL_VERSION, true)
            .add_header("mcp-method", "tools/call", true)
            .add_header("mcp-name", "commands_run", true)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 406,
                "method": "tools/call",
                "params": {
                    "name": "commands_run",
                    "arguments": arguments,
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                        "io.modelcontextprotocol/clientCapabilities": {
                            "extensions": { TASKS_EXTENSION: {} }
                        }
                    }
                }
            }))
            .send(service_for_call.as_ref())
            .await
    });
    let request = next_request(&fixture.registry).await;
    let job_id = request.job_id.unwrap();
    let guided = fixture.connector.host_guide(
        &fixture.task_id,
        "ordinary fallback must carry this guidance",
    );
    assert!(guided.ok, "{}", guided.body);
    update_job(
        &fixture.registry,
        &job_id,
        "completed",
        Some("sync terminal\n"),
        Some(0),
    )
    .await;

    let mut response = call.await.unwrap();
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["result"]["resultType"], "complete");
    assert!(body["result"].get("taskId").is_none());
    assert_eq!(
        body["result"]["structuredContent"]["data"]["execution"]["execution_status"],
        "succeeded"
    );
    assert_eq!(
        body["result"]["structuredContent"]["data"]["execution"]["output_tail"]["stdout"],
        "sync terminal\n"
    );
    assert_eq!(
        body["result"]["structuredContent"]["data"]["guidance"][0]["message"],
        "ordinary fallback must carry this guidance"
    );
    let execution_id = body["result"]["structuredContent"]["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    let execution = execution_by_id(&fixture, execution_id);
    assert!(!execution.mcp_task_is_materialized());
    assert!(!execution.mcp_task_result_is_finalized());
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn terminal_before_yield_boundary_is_not_newly_armed() {
    let fixture = fixture(1_000).await;
    let arguments = command_arguments(&fixture, "continuation-terminal-1", "printf done");
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_job(&registry, &job_id, "completed", Some("done\n"), Some(0)).await;
    });
    let outcome = fixture.call("commands_run", arguments).await;
    responder.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);
    assert_eq!(
        outcome.body["data"]["execution"]["execution_status"],
        "succeeded"
    );
    let execution = latest_execution(&fixture);
    assert_eq!(
        execution.continuation_intent,
        ConnectorExecutionContinuationIntent::None
    );
    assert_eq!(execution.continuation_armed_at, None);
    assert!(fixture
        .connector
        .db
        .terminal_ready_connector_executions()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn short_command_returns_terminal_and_precise_retry_does_not_spawn() {
    let fixture = fixture(1_000).await;
    let arguments = command_arguments(&fixture, "short-command-1", "printf short");
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
async fn check_plan_returns_terminal_persists_kind_and_precise_retry_does_not_spawn() {
    let fixture = fixture(1_000).await;
    let arguments = checks(&fixture, "short-check-1", &["format", "check"]);
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        assert_eq!(request.kind, "start_validation_job");
        let steps: Vec<ShellJobValidationStep> = serde_json::from_str(&request.command).unwrap();
        assert_eq!(
            steps
                .iter()
                .map(|step| step.name.as_str())
                .collect::<Vec<_>>(),
            ["format", "check"]
        );
        assert_eq!(steps[0].program, "cargo");
        assert_eq!(steps[0].args, ["fmt", "--", "--check"]);
        assert_eq!(steps[1].program, "cargo");
        assert_eq!(steps[1].args, ["check", "--all-targets"]);
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            "running",
            Some("Finished format\n"),
            None,
            check_progress(1, Some("check"), None),
        )
        .await;
        update_validation_job(
            &registry,
            &job_id,
            "completed",
            Some("Finished check\n"),
            Some(0),
            check_progress(2, None, None),
        )
        .await;
    });
    let first = fixture.call("checks_run", arguments.clone()).await;
    responder.await.unwrap();
    assert!(first.ok, "{}", first.body);
    let execution = &first.body["data"]["execution"];
    assert_eq!(execution["kind"], "check");
    assert_eq!(execution["submission_status"], "accepted");
    assert_eq!(execution["execution_status"], "succeeded");
    assert_eq!(execution["assertion_status"], "passed");
    assert_eq!(
        execution["checks"],
        json!([
            {"check": "format", "status": "passed"},
            {"check": "check", "status": "passed"}
        ])
    );
    assert_eq!(execution["exit_code"], 0);
    let execution_id = execution["execution_id"].as_str().unwrap();
    let durable = fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap();
    assert_eq!(durable.kind, "check");
    assert_eq!(durable.check_plan, vec!["format", "check"]);
    assert_eq!(durable.check_completed, 2);
    assert_eq!(durable.check_recipe.as_ref().unwrap()["recipe_id"], "rust");
    assert_eq!(
        validation_projection(Some(&durable)),
        json!({
            "status": "passed",
            "execution_id": execution_id,
            "checks": [
                {"check": "format", "status": "passed"},
                {"check": "check", "status": "passed"}
            ],
            "recipe": {
                "id": "rust",
                "version": 1,
                "root": ".",
                "checks": ["format", "check"]
            },
            "assertion_evidence": null
        })
    );

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("retry-drift"),
        "changed",
    )
    .unwrap();
    let retry = fixture.call("checks_run", arguments).await;
    assert_eq!(
        retry.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn check_operation_conflict_and_new_key_fail_fast_with_assertion_result() {
    let fixture = fixture(1_000).await;
    let first_arguments = checks(&fixture, "check-attempt-1", &["format", "test"]);
    let first_registry = fixture.registry.clone();
    let first_responder = tokio::spawn(async move {
        let request = next_request(&first_registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &first_registry,
            &job_id,
            "failed",
            Some("format failed\n"),
            Some(7),
            check_progress(0, None, Some("format")),
        )
        .await;
    });
    let first = fixture.call("checks_run", first_arguments).await;
    first_responder.await.unwrap();
    let execution = &first.body["data"]["execution"];
    assert_eq!(execution["execution_status"], "failed");
    assert_eq!(execution["assertion_status"], "failed");
    assert_eq!(execution["exit_code"], 7);
    assert_eq!(execution["failure_source"], "check");
    assert_eq!(
        execution["assertion_evidence"]["failure_kind"],
        "process_exit"
    );
    assert_eq!(
        execution["checks"],
        json!([
            {"check": "format", "status": "failed"},
            {"check": "test", "status": "not_run"}
        ])
    );

    let conflict = fixture
        .call(
            "checks_run",
            checks(&fixture, "check-attempt-1", &["check"]),
        )
        .await;
    assert_eq!(conflict.body["error"]["code"], "operation_id_conflict");
    assert!(poll(&fixture.registry).await.is_none());

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("workspace-change"),
        "changed",
    )
    .unwrap();
    let second_arguments = checks(&fixture, "check-attempt-2", &["format", "test"]);
    let second_registry = fixture.registry.clone();
    let second_responder = tokio::spawn(async move {
        let request = next_request(&second_registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &second_registry,
            &job_id,
            "running",
            None,
            None,
            check_progress(1, Some("test"), None),
        )
        .await;
        update_validation_job(
            &second_registry,
            &job_id,
            "completed",
            None,
            Some(0),
            check_progress(2, None, None),
        )
        .await;
    });
    let second = fixture.call("checks_run", second_arguments).await;
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
async fn checks_run_hash_binds_recipe_version_invocation_cwd_and_filter() {
    let fixture = fixture(1_000).await;
    let task = task(&fixture);
    let base = resolve_validation_recipe(
        Path::new(&task.execution_root),
        None,
        Some(RecipeId::Rust),
        &[SemanticCheck::Test],
        None,
    )
    .unwrap()
    .durable_identity();
    let base_hash = check_request_hash(&task, &base, None, None, 30);
    for changed in [
        {
            let mut changed = base.clone();
            changed["recipe_version"] = json!(2);
            check_request_hash(&task, &changed, None, None, 30)
        },
        {
            let mut changed = base.clone();
            changed["recipe_id"] = json!("node");
            check_request_hash(&task, &changed, None, None, 30)
        },
        {
            let mut changed = base.clone();
            changed["invocation_digest"] = json!("upgraded-invocation");
            check_request_hash(&task, &changed, None, None, 30)
        },
        check_request_hash(&task, &base, Some("."), None, 30),
        check_request_hash(&task, &base, None, Some("unicode-筛选"), 30),
    ] {
        assert_ne!(base_hash, changed);
    }
}

#[tokio::test]
async fn node_lockfile_change_makes_successful_checks_run_stale() {
    let fixture = fixture(1_000).await;
    let root = Path::new(&task(&fixture).execution_root).join("frontend");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"packageManager":"npm@10","scripts":{"check":"eslint ."}}"#,
    )
    .unwrap();
    std::fs::write(root.join("package-lock.json"), "{}").unwrap();
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let steps: Vec<ShellJobValidationStep> = serde_json::from_str(&request.command).unwrap();
        assert_eq!(steps[0].program, "npm");
        update_validation_job(
            &registry,
            request.job_id.as_deref().unwrap(),
            "completed",
            None,
            Some(0),
            check_progress(1, None, None),
        )
        .await;
    });
    let checked = fixture
        .call(
            "checks_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "node-lock-check",
                "checks": ["check"],
                "cwd": "frontend"
            }),
        )
        .await;
    responder.await.unwrap();
    assert_eq!(checked.body["data"]["execution"]["recipe"]["id"], "node");
    std::fs::write(root.join("package-lock.json"), "{\"changed\":true}").unwrap();
    let finish = finish(&fixture, "must revalidate changed package-manager evidence").await;
    assert_eq!(finish.body["error"]["code"], "checks_stale");
}

#[tokio::test]
async fn validation_executor_failures_preserve_codes_without_assertion_evidence() {
    let cases = [
        ("spawn-failure", "validation_step_spawn_failed", true, false),
        (
            "tool-unavailable",
            "validation_tool_unavailable",
            false,
            false,
        ),
        (
            "wait-failure",
            VALIDATION_STEP_WAIT_FAILED_CODE,
            false,
            true,
        ),
    ];

    for (operation_id, failure_code, format_completed, assert_wait_failed_step_none) in cases {
        let fixture = fixture(1_000).await;
        let registry = fixture.registry.clone();
        let responder_failure_code = failure_code.to_string();
        let responder = tokio::spawn(async move {
            let request = next_request(&registry).await;
            assert_eq!(request.kind, "start_validation_job");
            let job_id = request.job_id.unwrap();
            if format_completed {
                update_validation_job(
                    &registry,
                    &job_id,
                    "running",
                    Some("format completed\n"),
                    None,
                    check_progress(1, Some("check"), None),
                )
                .await;
            }
            let completed = if format_completed { 1 } else { 0 };
            let mut failed =
                validation_job_update(&job_id, "failed", check_progress(completed, None, None));
            failed.error = Some(responder_failure_code);
            let updated = registry.update_job(failed).await.unwrap();
            assert_eq!(updated.status, "failed");
            if assert_wait_failed_step_none {
                assert!(updated
                    .validation_progress
                    .is_some_and(|progress| progress.failed_step.is_none()));
            }
        });

        let plan: &[&str] = if format_completed {
            &["format", "check"]
        } else {
            &["check"]
        };
        let outcome = fixture
            .call("checks_run", checks(&fixture, operation_id, plan))
            .await;
        responder.await.unwrap();
        assert!(outcome.ok, "{failure_code}: {}", outcome.body);
        let execution = &outcome.body["data"]["execution"];
        assert_eq!(execution["execution_status"], "failed", "{failure_code}");
        assert_eq!(execution["failure_source"], "executor", "{failure_code}");
        assert_eq!(execution["failure_code"], failure_code, "{failure_code}");
        assert_ne!(execution["assertion_status"], "failed", "{failure_code}");
        assert!(execution["assertion_evidence"].is_null(), "{failure_code}");
        let projected_checks = execution["checks"].as_array().unwrap();
        assert!(
            projected_checks
                .iter()
                .all(|check| check["status"] != "failed"),
            "{failure_code}: {projected_checks:?}"
        );
        let durable = fixture
            .connector
            .db
            .connector_execution(execution["execution_id"].as_str().unwrap())
            .unwrap();
        assert!(durable.failed_check.is_none(), "{failure_code}");
        assert!(durable.assertion_evidence.is_none(), "{failure_code}");
        assert!(
            durable.validated_workspace_sha256.is_none(),
            "{failure_code}"
        );

        if format_completed {
            assert_eq!(
                execution["checks"],
                json!([
                    {"check": "format", "status": "passed"},
                    {"check": "check", "status": "not_run"}
                ])
            );
            assert_eq!(durable.check_plan, vec!["format", "check"]);
            assert_eq!(durable.check_completed, 1);
        }
    }
}

#[tokio::test]
async fn passed_check_does_not_validate_a_later_workspace_state() {
    let fixture = fixture(1_000).await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            "completed",
            None,
            Some(0),
            check_progress(1, None, None),
        )
        .await;
    });
    let checked = fixture
        .call(
            "checks_run",
            checks(&fixture, "workspace-provenance-1", &["check"]),
        )
        .await;
    responder.await.unwrap();
    assert_eq!(
        checked.body["data"]["execution"]["execution_status"],
        "succeeded"
    );
    let execution_id = checked.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap()
        .to_string();
    let provenance = fixture
        .connector
        .db
        .connector_execution(&execution_id)
        .unwrap()
        .validated_workspace_sha256
        .unwrap();
    fixture
        .connector
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_executions SET validated_workspace_sha256 = NULL WHERE id = ?1",
            [&execution_id],
        )
        .unwrap();
    let legacy = finish(&fixture, "legacy provenance").await;
    assert_eq!(legacy.body["error"]["code"], "checks_stale");
    fixture
        .connector
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_executions SET validated_workspace_sha256 = ?1 WHERE id = ?2",
            [&provenance, &execution_id],
        )
        .unwrap();

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("changed-after-check"),
        "not validated",
    )
    .unwrap();
    let stale = finish(&fixture, "must rerun checks").await;
    assert_eq!(stale.http_status, 409);
    assert_eq!(stale.body["error"]["code"], "checks_stale");
    assert_eq!(
        stale.body["data"]["execution_id"].as_str(),
        Some(execution_id.as_str())
    );

    let retry = fixture
        .call(
            "checks_run",
            checks(&fixture, "workspace-provenance-1", &["check"]),
        )
        .await;
    assert_eq!(
        retry.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert!(poll(&fixture.registry).await.is_none());
    let still_stale = finish(&fixture, "still stale").await;
    assert_eq!(still_stale.body["error"]["code"], "checks_stale");

    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            "completed",
            None,
            Some(0),
            check_progress(1, None, None),
        )
        .await;
    });
    let rechecked = fixture
        .call(
            "checks_run",
            checks(&fixture, "workspace-provenance-2", &["check"]),
        )
        .await;
    responder.await.unwrap();
    let rechecked_id = rechecked.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    assert_ne!(rechecked_id, execution_id);
    let reopened = Database::open(&fixture._temp.path().join("connector.db")).unwrap();
    assert!(reopened
        .connector_execution(rechecked_id)
        .unwrap()
        .validated_workspace_sha256
        .is_some());

    let finished = finish(&fixture, "fresh validation").await;
    assert!(finished.ok, "{}", finished.body);
    assert_eq!(
        finished.body["data"]["result"]["validation"]["status"],
        "passed"
    );
}

#[tokio::test]
async fn edits_apply_after_a_passed_check_makes_finish_stale() {
    let fixture = fixture(1_000).await;
    let parallel = fixture
        .call(
            "task_start",
            json!({"goal": "parallel finish", "mode": "read_only"}),
        )
        .await;
    let parallel_task = parallel.body["task_id"].as_str().unwrap();
    let checked = terminal_check(
        &fixture,
        "before-edit-1",
        &["check"],
        "completed",
        0,
        None,
        check_progress(1, None, None),
    )
    .await;
    assert_eq!(
        checked.body["data"]["execution"]["assertion_status"],
        "passed"
    );

    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let task_id = fixture.task_id.clone();
    let mut edit_call = tokio::spawn(async move {
        call(
            &connector,
            &owner,
            "edits_apply",
            json!({
                "task_id": task_id,
                "operation_id": "edit-after-check-1",
                "changes": [{
                    "kind": "create",
                    "path": "edit-after-check.txt",
                    "content": "changed"
                }]
            }),
        )
        .await
    });
    let request = tokio::select! {
        result = &mut edit_call => panic!("edit returned before dispatch: {}", result.unwrap().body),
        request = next_request(&fixture.registry) => request,
    };
    assert_eq!(request.kind, "file_apply_text_edits");
    let other_finish = fixture
        .call(
            "task_finish",
            json!({"task_id": parallel_task, "summary": "not globally blocked"}),
        )
        .await;
    assert!(other_finish.ok, "{}", other_finish.body);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let task_id = fixture.task_id.clone();
    let finish_call = tokio::spawn(async move {
        call(
            &connector,
            &owner,
            "task_finish",
            json!({"task_id": task_id, "summary": "stale after edit"}),
        )
        .await
    });
    complete_create_edit(&fixture, request, "edit-after-check.txt", "changed").await;
    assert!(edit_call.await.unwrap().ok);

    let finish = finish_call.await.unwrap();
    assert_eq!(finish.body["error"]["code"], "checks_stale");
}

#[tokio::test]
async fn active_review_surfaces_applied_paths_without_diff() {
    let fixture = fixture(20).await;
    // Apply one edit so the task has durable changed paths on record.
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let task_id = fixture.task_id.clone();
    let edit_call = tokio::spawn(async move {
        call(
            &connector,
            &owner,
            "edits_apply",
            json!({
                "task_id": task_id,
                "operation_id": "active-review-edit-1",
                "changes": [{
                    "kind": "create",
                    "path": "active-review.txt",
                    "content": "x"
                }]
            }),
        )
        .await
    });
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.kind, "file_apply_text_edits");
    complete_create_edit(&fixture, request, "active-review.txt", "x").await;
    assert!(edit_call.await.unwrap().ok);

    // Start a long command and let it reach running.
    let arguments = command_arguments(&fixture, "active-review-cmd-1", "sleep 30");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_job");
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;
    let quick_yield = command_call.await.unwrap();
    assert!(quick_yield.ok, "{}", quick_yield.body);

    // Review during the active execution: diff stays deferred, but the
    // applied paths and the enriched edits_apply event are visible.
    let review = fixture
        .call(
            "task_review",
            json!({"task_id": fixture.task_id, "include_diff": true}),
        )
        .await;
    assert!(review.ok, "{}", review.body);
    let changes = &review.body["data"]["changes"];
    assert_eq!(changes["source"], "live_workspace_deferred");
    assert_eq!(changes["changed_paths_source"], "applied_edits");
    assert_eq!(changes["changed_paths"], json!(["active-review.txt"]));
    assert!(changes["diff_preview"].is_null());
    let events = review.body["data"]["recent_events"].as_array().unwrap();
    let edit_event = events
        .iter()
        .find(|event| event["kind"] == "edits_apply")
        .expect("edits_apply event in timeline");
    assert_eq!(
        edit_event["payload"]["changed_paths"],
        json!(["active-review.txt"])
    );

    // Release the workspace slot.
    let stop_registry = fixture.registry.clone();
    let stop_job = job_id.clone();
    let stopper = tokio::spawn(async move {
        let stop = next_request(&stop_registry).await;
        assert_eq!(stop.kind, "stop_job");
        update_job(&stop_registry, &stop_job, "stopped", None, Some(-1)).await;
    });
    let cancelled = fixture
        .call("task_cancel", json!({"task_id": fixture.task_id}))
        .await;
    assert!(cancelled.ok, "{}", cancelled.body);
    stopper.await.unwrap();
}

#[tokio::test]
async fn finish_fingerprint_and_result_capture_exclude_a_concurrent_edit() {
    let fixture = fixture(1_000).await;
    let command_arguments = command_arguments(&fixture, "atomic-finish-command-1", "printf late");
    let second = fixture
        .connector
        .call(
            "task_start",
            json!({"goal": "parallel read-only finish", "mode": "read_only"}),
            Some(&fixture.owner),
            ConnectorTransport::Mcp,
        )
        .await;
    let second_task_id = second.body["task_id"].as_str().unwrap().to_string();
    terminal_check(
        &fixture,
        "atomic-finish-check-1",
        &["check"],
        "completed",
        0,
        None,
        check_progress(1, None, None),
    )
    .await;
    let reached = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *fixture.connector.finish_after_fingerprint.lock().unwrap() =
        Some((reached.clone(), resume.clone()));
    let mutation_entered = Arc::new(tokio::sync::Semaphore::new(0));
    *fixture.connector.mutation_before_task_lock.lock().unwrap() = Some(mutation_entered.clone());
    let task_lock = fixture.connector.task_lock(&fixture.task_id);

    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let task_id = fixture.task_id.clone();
    let finish_call = tokio::spawn(async move {
        connector
            .task_finish(
                json!({"task_id": task_id, "summary": "atomic finish"}),
                tests::PROJECT_SUBJECT_ID,
                &owner,
                ConnectorTransport::Mcp,
                chrono::Utc::now().timestamp(),
            )
            .await
    });
    reached.notified().await;
    let parallel_finish = fixture
        .call(
            "task_finish",
            json!({"task_id": second_task_id, "summary": "parallel finish"}),
        )
        .await;
    assert!(parallel_finish.ok, "{}", parallel_finish.body);

    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let task_id = fixture.task_id.clone();
    let edit_call = tokio::spawn(async move {
        connector
            .edits_apply(
                json!({
                    "task_id": task_id,
                    "operation_id": "atomic-finish-edit-1",
                    "changes": [{
                        "kind": "create",
                        "path": "atomic-finish-edit.txt",
                        "content": "state B"
                    }]
                }),
                tests::PROJECT_SUBJECT_ID,
                &owner,
                ConnectorTransport::Mcp,
                chrono::Utc::now().timestamp(),
            )
            .await
    });
    mutation_entered.acquire().await.unwrap().forget();

    assert!(
        task_lock.try_lock().is_err(),
        "finish must own the task lock"
    );
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(
            async move { call(&connector, &owner, "commands_run", command_arguments).await },
        );
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let arguments = checks(&fixture, "atomic-finish-late-check-1", &["check"]);
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    mutation_entered.acquire_many(2).await.unwrap().forget();
    resume.notify_one();
    let edit = edit_call.await.unwrap();
    let finished = finish_call.await.unwrap();
    let command = command_call.await.unwrap();
    let check = check_call.await.unwrap();
    assert!(finished.ok, "{}", finished.body);
    assert_eq!(edit.body["error"]["code"], "task_not_active");
    assert_eq!(command.body["error"]["code"], "task_not_active");
    assert_eq!(check.body["error"]["code"], "task_not_active");
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn command_and_check_reservations_block_finish_before_dispatch_completes() {
    let fixture = fixture(1_000).await;
    let arguments = command_arguments(&fixture, "reservation-command-1", "printf reserved");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let command_request = next_request(&fixture.registry).await;
    assert_eq!(command_request.kind, "start_job");
    let blocked = finish(&fixture, "command reservation is active").await;
    assert_eq!(blocked.body["error"]["code"], "execution_not_terminal");
    update_job(
        &fixture.registry,
        command_request.job_id.as_deref().unwrap(),
        "completed",
        None,
        Some(0),
    )
    .await;
    assert!(command_call.await.unwrap().ok);

    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let arguments = checks(&fixture, "reservation-check-1", &["check"]);
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let check_request = next_request(&fixture.registry).await;
    assert_eq!(check_request.kind, "start_validation_job");
    let blocked = finish(&fixture, "check reservation is active").await;
    assert_eq!(blocked.body["error"]["code"], "execution_not_terminal");
    update_validation_job(
        &fixture.registry,
        check_request.job_id.as_deref().unwrap(),
        "completed",
        None,
        Some(0),
        check_progress(1, None, None),
    )
    .await;
    assert!(check_call.await.unwrap().ok);
}

#[tokio::test]
async fn mutating_command_after_a_passed_check_makes_finish_stale() {
    let fixture = fixture(1_000).await;
    terminal_check(
        &fixture,
        "before-command-1",
        &["test"],
        "completed",
        0,
        None,
        check_progress(1, None, None),
    )
    .await;
    let command = "printf changed > command-after-check.txt";
    let arguments = command_arguments(&fixture, "mutating-command-1", command);
    let registry = fixture.registry.clone();
    let execution_root = task(&fixture).execution_root;
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        std::fs::write(
            Path::new(&execution_root).join("command-after-check.txt"),
            "changed",
        )
        .unwrap();
        update_job(&registry, &job_id, "completed", None, Some(0)).await;
    });
    let command_outcome = fixture.call("commands_run", arguments).await;
    responder.await.unwrap();
    assert_eq!(
        command_outcome.body["data"]["execution"]["execution_status"],
        "succeeded"
    );

    let finish = finish(&fixture, "stale after command").await;
    assert_eq!(finish.body["error"]["code"], "checks_stale");
}

#[tokio::test]
async fn project_stdout_cannot_forge_passed_check_progress() {
    let fixture = fixture(1_000).await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            "failed",
            Some("__WEBCODEX_CHECK_STEP__:passed:test\n"),
            Some(101),
            check_progress(0, None, Some("test")),
        )
        .await;
    });
    let outcome = fixture
        .call(
            "checks_run",
            checks(&fixture, "forged-progress-1", &["test"]),
        )
        .await;
    responder.await.unwrap();
    let execution = &outcome.body["data"]["execution"];
    assert_eq!(execution["execution_status"], "failed");
    assert_eq!(
        execution["checks"],
        json!([{"check": "test", "status": "failed"}])
    );
    assert_eq!(execution["assertion_evidence"]["failed_check"], "test");
    assert!(execution["assertion_evidence"]["failure_kind"].is_string());
}

#[tokio::test]
async fn terminal_validation_success_without_progress_fails_closed() {
    let fixture = fixture(1_000).await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        let job_id = request.job_id.unwrap();
        update_job(&registry, &job_id, "completed", None, Some(0)).await;
    });
    let outcome = fixture
        .call(
            "checks_run",
            checks(
                &fixture,
                "missing-terminal-progress-1",
                &["format", "check", "test"],
            ),
        )
        .await;
    responder.await.unwrap();
    let execution = &outcome.body["data"]["execution"];
    assert_ne!(execution["execution_status"], "succeeded");
    assert_ne!(execution["assertion_status"], "passed");
    let execution_id = execution["execution_id"].as_str().unwrap();
    let db = &fixture.connector.db;
    let durable = db.connector_execution(execution_id).unwrap();
    assert_eq!(durable.check_completed, 0);
    assert!(durable.validated_workspace_sha256.is_none());
    let direct = created(
        db.reserve_connector_execution(
            &task(&fixture),
            "check",
            "missing-provenance-direct",
            "missing-provenance-hash",
            &["check".to_string()],
            Some(&json!({"recipe_id":"rust"})),
            Some("expected-workspace"),
            30,
            2,
        )
        .unwrap(),
    );
    db.attach_connector_executor(&direct.execution_id, "direct-job", "running", 3)
        .unwrap();
    let error = db
        .observe_connector_execution(
            &direct.execution_id,
            ConnectorExecutionObservation {
                executor_status: "completed",
                stdout_cursor: 0,
                stderr_cursor: 0,
                exit_code: Some(0),
                started_at: Some(3),
                finished_at: Some(4),
                check_completed: Some(1),
                failed_check: None,
                assertion_evidence: None,
                validated_workspace_sha256: None,
                executor_failure_code: None,
                mcp_task_output_tail: None,
                now: 4,
            },
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("successful check requires complete progress and matching workspace provenance"));
    db.finish_connector_execution(
        &direct.execution_id,
        ConnectorExecutionFailure::Unknown("protocol_test"),
        5,
    )
    .unwrap();
    let finished = finish(&fixture, "missing progress must not pass").await;
    assert_ne!(
        finished.body["data"]["result"]["validation"]["status"],
        "passed"
    );
}

#[tokio::test]
async fn failed_check_has_durable_bounded_sanitized_evidence_without_passed_provenance() {
    let fixture = fixture(1_000).await;
    let mut output = [
        "thread 'tests::fails' panicked at /private/workspace/secret.rs:9:2:",
        "assertion failed",
        "test tests::fails ... FAILED",
        "test result: FAILED. 0 passed; 1 failed; 0 ignored",
    ]
    .join("\n");
    for index in 0..240 {
        output.push_str(&format!("\npost-diagnostic line {index}"));
    }
    let failed = terminal_check(
        &fixture,
        "durable-evidence-1",
        &["test"],
        "failed",
        101,
        Some(output),
        check_progress(0, None, Some("test")),
    )
    .await;
    let execution_id = failed.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    let durable = fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap();
    assert!(durable.validated_workspace_sha256.is_none());
    assert_eq!(durable.failed_check.as_deref(), Some("test"));
    let evidence = durable.assertion_evidence.as_ref().unwrap();
    assert_eq!(evidence["failure_kind"], "test_failure");
    assert_eq!(evidence["parser_version"], 3);
    let serialized = serde_json::to_vec(evidence).unwrap();
    assert!(serialized.len() <= crate::db::MAX_ASSERTION_EVIDENCE_BYTES);
    assert!(!String::from_utf8(serialized)
        .unwrap()
        .contains("/private/workspace"));

    let without_tail = fixture
        .connector
        .executions
        .projection(&durable, &fixture.owner, false)
        .await;
    assert!(without_tail["output_tail"].is_null());
    assert_eq!(without_tail["assertion_evidence"]["failed_check"], "test");
    let unavailable_logs = execution::ExecutionService::new(
        Arc::new(ToolRuntime::new_for_tests_with_shell_clients(Arc::new(
            ShellClientRegistry::default(),
        ))),
        fixture.connector.db.clone(),
        fixture.connector.workspace.clone(),
    )
    .projection(&durable, &fixture.owner, true)
    .await;
    assert!(unavailable_logs["output_tail"].is_null());
    assert_eq!(
        unavailable_logs["assertion_evidence"],
        without_tail["assertion_evidence"]
    );
    let reopened = Database::open(&fixture._temp.path().join("connector.db")).unwrap();
    let reopened_execution = reopened.connector_execution(execution_id).unwrap();
    assert_eq!(
        reopened_execution.assertion_evidence,
        durable.assertion_evidence
    );
    assert_eq!(reopened_execution.check_recipe, durable.check_recipe);

    std::fs::write(
        Path::new(&task(&fixture).execution_root).join("changed-after-failure"),
        "changed",
    )
    .unwrap();
    let finish = finish(&fixture, "failed validation remains failed").await;
    assert!(finish.ok, "{}", finish.body);
    assert_eq!(
        finish.body["data"]["result"]["validation"]["status"],
        "failed"
    );
    assert_eq!(
        finish.body["data"]["result"]["validation"]["assertion_evidence"]["failed_check"],
        "test"
    );
}

#[tokio::test]
async fn go_test_failure_has_durable_structured_assertion_evidence() {
    let fixture = fixture(1_000).await;
    let root = Path::new(&task(&fixture).execution_root).join("go-fixture");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("go.mod"),
        "module example.test/go-fixture\n\ngo 1.22\n",
    )
    .unwrap();
    let output = [
        json!({
            "Action": "pass",
            "Package": "example.test/go-fixture/ok",
            "Test": "TestPass"
        })
        .to_string(),
        json!({
            "Action": "output",
            "Package": "example.test/go-fixture/failing",
            "Test": "TestParent/subtest",
            "Output": "panic payload at /private/workspace/secret.go executor-private-id\n"
        })
        .to_string(),
        json!({
            "Action": "fail",
            "Package": "example.test/go-fixture/failing",
            "Test": "TestParent/subtest"
        })
        .to_string(),
        json!({
            "Action": "fail",
            "Package": "example.test/go-fixture/failing"
        })
        .to_string(),
    ]
    .join("\n");
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        assert_eq!(request.kind, "start_validation_job");
        let steps: Vec<ShellJobValidationStep> = serde_json::from_str(&request.command).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].name, "test");
        assert_eq!(steps[0].program, "go");
        assert_eq!(steps[0].args, ["test", "-json", "./..."]);
        assert!(steps[0].is_canonical());
        update_validation_job(
            &registry,
            request.job_id.as_deref().unwrap(),
            "failed",
            Some(&output),
            Some(1),
            check_progress(0, None, Some("test")),
        )
        .await;
    });
    let outcome = fixture
        .call(
            "checks_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "go-structured-failure-1",
                "checks": ["test"],
                "recipe": "go",
                "cwd": "go-fixture",
                "timeout_secs": 30
            }),
        )
        .await;
    responder.await.unwrap();

    assert!(outcome.ok, "{}", outcome.body);
    let execution = &outcome.body["data"]["execution"];
    assert_eq!(execution["execution_status"], "failed");
    assert_eq!(execution["failure_source"], "check");
    assert_eq!(execution["assertion_status"], "failed");
    assert_eq!(execution["assertion_evidence"]["failed_check"], "test");
    assert_eq!(
        execution["assertion_evidence"]["failure_kind"],
        "test_failure"
    );
    assert_eq!(
        execution["assertion_evidence"]["parser"],
        "structured_validation_parser"
    );
    let diagnostics = &execution["assertion_evidence"]["diagnostics"];
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["test_summary"]["passed"], 1);
    assert_eq!(diagnostics["test_summary"]["failed"], 1);
    assert!(diagnostics["failed_test_details"]
        .as_array()
        .unwrap()
        .iter()
        .any(|detail| { detail["name"] == "example.test/go-fixture/failing::TestParent/subtest" }));

    let execution_id = execution["execution_id"].as_str().unwrap();
    let durable = fixture
        .connector
        .db
        .connector_execution(execution_id)
        .unwrap();
    assert_eq!(durable.failed_check.as_deref(), Some("test"));
    assert_eq!(
        durable.assertion_evidence.as_ref().unwrap()["failure_kind"],
        "test_failure"
    );
    let serialized = serde_json::to_string(durable.assertion_evidence.as_ref().unwrap()).unwrap();
    assert!(
        serialized.len() <= crate::db::MAX_ASSERTION_EVIDENCE_BYTES,
        "{serialized}"
    );
    for forbidden in [
        "panic payload",
        "/private/workspace",
        "secret.go",
        "executor-private-id",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "durable evidence leaked {forbidden:?}: {serialized}"
        );
    }
}

#[tokio::test]
async fn structured_progress_rejects_invalid_order_and_preserves_fail_fast_plan() {
    let fixture = fixture(1_000).await;
    let plan = || ShellJobStartMetadata {
        project_id: None,
        session_id: None,
        ssh_resource: None,
        project_cwd: None,
        purpose: Some("validation".into()),
        shell: Some("configured".into()),
        validation_steps: ["format", "check", "test"]
            .into_iter()
            .map(|name| ShellJobValidationStep {
                name: name.into(),
                program: match name {
                    "format" | "check" | "test" => "cargo".into(),
                    _ => unreachable!(),
                },
                args: match name {
                    "format" => vec!["fmt".into(), "--".into(), "--check".into()],
                    "check" => vec!["check".into(), "--all-targets".into()],
                    "test" => vec!["test".into()],
                    _ => unreachable!(),
                },
                env: Vec::new(),
            })
            .collect(),
        validation: None,
        visibility: crate::shell_client::ShellJobVisibility::Public,
        validation_identity: None,
        validation_tool: None,
        assertion_name: None,
        structured_execution: None,
        stdin: None,
        detached_idempotency_key: None,
    };
    let duplicate = fixture
        .registry
        .start_job_with_metadata(job_start_request(), "tester".into(), plan())
        .await
        .unwrap();
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.job_id.as_deref(), Some(duplicate.job_id.as_str()));
    for _ in 0..2 {
        let repeated = fixture
            .registry
            .update_job(validation_job_update(
                &duplicate.job_id,
                "running",
                check_progress(0, Some("format"), None),
            ))
            .await
            .unwrap();
        assert_eq!(repeated.status, "running");
    }

    let cases = vec![
        (
            vec![],
            "failed",
            Some(101),
            None,
            "validation_progress_missing",
        ),
        (
            vec![check_progress(0, Some("format"), None)],
            "completed",
            Some(0),
            Some(check_progress(1, None, None)),
            "validation_progress_incomplete",
        ),
        (
            vec![],
            "running",
            None,
            Some(check_progress(0, Some("test"), None)),
            "validation_progress_invalid",
        ),
        (
            vec![check_progress(0, Some("format"), None)],
            "completed",
            Some(0),
            Some(check_progress(1, Some("check"), None)),
            "validation_progress_incomplete",
        ),
        (
            vec![],
            "failed",
            Some(7),
            Some(check_progress(0, None, Some("test"))),
            "validation_progress_invalid",
        ),
        (
            vec![
                check_progress(0, Some("format"), None),
                check_progress(1, Some("check"), None),
            ],
            "running",
            None,
            Some(check_progress(0, Some("format"), None)),
            "validation_progress_invalid",
        ),
        (
            vec![],
            "running",
            None,
            Some(check_progress(2, Some("test"), None)),
            "validation_progress_invalid",
        ),
        (
            vec![],
            "running",
            None,
            Some(check_progress(4, None, None)),
            "validation_progress_invalid",
        ),
    ];
    for (setup, status, exit_code, progress, code) in cases {
        let job = fixture
            .registry
            .start_job_with_metadata(job_start_request(), "tester".into(), plan())
            .await
            .unwrap();
        let request = next_request(&fixture.registry).await;
        assert_eq!(request.job_id.as_deref(), Some(job.job_id.as_str()));
        for progress in setup {
            let update = fixture
                .registry
                .update_job(validation_job_update(&job.job_id, "running", progress))
                .await
                .unwrap();
            assert_eq!(update.status, "running");
        }
        let malformed = ShellAgentJobUpdateRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            update_seq: None,
            job_id: job.job_id,
            request_id: None,
            status: status.into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code,
            duration_ms: Some(1),
            error: None,
            command_execution_state: None,
            validation_progress: progress,
            finished: matches!(status, "completed" | "failed"),
        };
        let failed = fixture.registry.update_job(malformed).await.unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.error.as_deref().unwrap().contains(code));
    }
}

#[tokio::test]
async fn ordinary_jobs_reject_validation_progress_without_changing_normal_updates() {
    let fixture = fixture(1_000).await;
    let job = fixture
        .registry
        .start_job(job_start_request(), "tester".into())
        .await
        .unwrap();
    let request = next_request(&fixture.registry).await;
    assert_eq!(request.kind, "start_job");
    let normal = fixture
        .registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            update_seq: None,
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    assert_eq!(normal.status, "running");
    let rejected = fixture
        .registry
        .update_job(validation_job_update(
            &job.job_id,
            "running",
            check_progress(0, Some("format"), None),
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status, "failed");
    assert!(rejected
        .error
        .as_deref()
        .unwrap()
        .contains("validation_progress_unexpected"));
}

#[tokio::test]
async fn invalid_check_plan_is_rejected_before_durable_reservation() {
    let fixture = fixture(1_000).await;
    let outcome = fixture
        .call(
            "checks_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "invalid-check-plan-1",
                "checks": ["test"],
                "test_filter": "contains\0nul"
            }),
        )
        .await;
    assert_eq!(outcome.body["error"]["code"], "test_filter_unsupported");
    assert!(fixture
        .connector
        .db
        .latest_connector_execution(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            tests::PROJECT_SUBJECT_ID,
            None,
        )
        .unwrap()
        .is_none());
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn same_normalized_test_filter_retries_and_different_filter_conflicts() {
    let fixture = fixture(1_000).await;
    let filtered = |operation_id: &str, filter: &str| {
        json!({
            "task_id": fixture.task_id,
            "operation_id": operation_id,
            "checks": ["test"],
            "test_filter": filter,
            "timeout_secs": 30
        })
    };

    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        update_validation_job(
            &registry,
            request.job_id.as_deref().unwrap(),
            "completed",
            None,
            Some(0),
            check_progress(1, None, None),
        )
        .await;
    });
    let first = fixture
        .call(
            "checks_run",
            filtered("filter-identity-1", "module::inner_test"),
        )
        .await;
    responder.await.unwrap();
    assert!(first.ok, "{}", first.body);
    let execution_id = first.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Whitespace-only difference normalizes to the same executed value: same
    // operation id is an exact retry with no new dispatch.
    let retry = fixture
        .call(
            "checks_run",
            filtered("filter-identity-1", "  module::inner_test  "),
        )
        .await;
    assert_eq!(
        retry.body["data"]["execution"]["execution_id"],
        execution_id
    );
    assert!(poll(&fixture.registry).await.is_none());

    // A genuinely different filter under the same operation id conflicts.
    let conflict = fixture
        .call(
            "checks_run",
            filtered("filter-identity-1", "module::other_test"),
        )
        .await;
    assert_eq!(conflict.body["error"]["code"], "operation_id_conflict");
    assert!(poll(&fixture.registry).await.is_none());
}

#[tokio::test]
async fn operation_id_conflict_is_stable_and_does_not_spawn() {
    let fixture = fixture(1_000).await;
    let arguments = command_arguments(&fixture, "stable-operation", "printf first");
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
    let first_arguments = command_arguments(&fixture, "test-attempt-1", command);
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
    let second_arguments = command_arguments(&fixture, "test-attempt-2", command);
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
    // Generous yield budget: after the late attach the response must include the
    // monitor-driven cancel_requested -> cancelled transition, which can lag far
    // beyond a few milliseconds when the whole suite runs in parallel.
    let fixture = fixture_configured(2_000, {
        let gate = gate.clone();
        move |service| service.with_monitor_timing(80, 5).with_attach_gate(gate)
    })
    .await;
    let arguments = command_arguments(&fixture, "starting-race-1", "sleep 30");
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
    assert_eq!(latest_execution(&fixture).state, "cancel_requested");

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
    let durable = execution_by_id(&fixture, execution_id);
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
    wait_for_workspace_slot_state(
        &fixture,
        "idle",
        Duration::from_secs(10),
        "late-attach cancellation",
    )
    .await;
}

#[tokio::test]
async fn retry_and_cancel_share_one_execution_monitor() {
    let fixture = fixture_configured(20, |service| service.with_monitor_timing(500, 5)).await;
    let arguments = command_arguments(&fixture, "one-monitor-1", "sleep 30");
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
    let cancelled_id = cancelled.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    assert!(execution_by_id(&fixture, cancelled_id)
        .validated_workspace_sha256
        .is_none());
    assert_eq!(fixture.connector.executions.monitor_start_count(), 1);
    wait_for_monitor_count(
        &fixture,
        0,
        Duration::from_secs(10),
        "cancelled execution monitor shutdown",
    )
    .await;
}

#[tokio::test]
async fn transient_check_status_recovers_within_grace() {
    // Keep readiness comfortably inside the test-only grace so recovery can be
    // injected before the monitor is allowed to finalize the execution unknown.
    let monitor_grace_ms = 3_000;
    let readiness = Duration::from_secs(2);
    let fixture = fixture_configured(20, move |service| {
        service.with_monitor_timing(monitor_grace_ms, 5)
    })
    .await;
    let arguments = checks(&fixture, "transient-status-1", &["check"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    fixture
        .registry
        .update_job(validation_job_update(
            &job_id,
            "future-agent-state",
            check_progress(0, Some("check"), None),
        ))
        .await
        .unwrap();

    let degraded = wait_for_execution(
        &fixture,
        None,
        readiness,
        "monitor degraded observation",
        |current| current.status_failure_code.as_deref() == Some("executor_status_unrecognized"),
    )
    .await;
    assert!(degraded.is_active());
    assert_eq!(
        degraded.status_failure_code.as_deref(),
        Some("executor_status_unrecognized")
    );
    let projection = execution::execution_projection(&degraded, 10, None);
    assert_eq!(projection["observation_status"], "degraded");

    update_validation_job(
        &fixture.registry,
        &job_id,
        "running",
        None,
        None,
        check_progress(0, Some("check"), None),
    )
    .await;
    let recovered = wait_for_execution(
        &fixture,
        Some(&degraded.execution_id),
        readiness,
        "monitor recovery observation",
        |current| current.state == "running" && current.status_failure_code.is_none(),
    )
    .await;
    assert_eq!(recovered.status_failure_code, None);

    update_validation_job(
        &fixture.registry,
        &job_id,
        "completed",
        None,
        Some(0),
        check_progress(1, None, None),
    )
    .await;
    let _quick_yield = check_call.await.unwrap();
    let completed = wait_for_execution(
        &fixture,
        Some(&degraded.execution_id),
        Duration::from_secs(10),
        "recovered execution success",
        |current| current.state == "succeeded",
    )
    .await;
    assert_eq!(completed.status_failure_code, None);
}

#[tokio::test]
async fn check_transport_failure_becomes_unknown_only_after_grace() {
    // The degraded observation must arrive inside the grace; terminal `unknown`
    // is allowed only after that grace has elapsed from transport loss.
    let monitor_grace = Duration::from_millis(3_000);
    let degraded_readiness = Duration::from_secs(2);
    let unknown_readiness = Duration::from_secs(6);
    let fixture = fixture_configured(5, move |service| {
        service.with_monitor_timing(monitor_grace.as_millis() as u64, 5)
    })
    .await;
    let arguments = checks(&fixture, "transport-grace-1", &["test"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    update_validation_job(
        &fixture.registry,
        &job_id,
        "running",
        None,
        None,
        check_progress(0, Some("test"), None),
    )
    .await;
    let started = check_call.await.unwrap();
    let execution_id = started.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    fixture
        .registry
        .reconcile_disconnect("hosted", "instance")
        .await;

    let degraded = wait_for_execution(
        &fixture,
        Some(execution_id),
        degraded_readiness,
        "executor transport degradation",
        |current| current.status_failure_code.as_deref() == Some("executor_status_unavailable"),
    )
    .await;
    let degraded_observed_at = Instant::now();
    assert!(degraded.is_active());
    assert_ne!(degraded.state, "unknown");
    assert_eq!(
        degraded.status_failure_code.as_deref(),
        Some("executor_status_unavailable")
    );

    let unknown = wait_for_execution(
        &fixture,
        Some(execution_id),
        unknown_readiness,
        "executor transport grace expiry",
        |current| current.state == "unknown",
    )
    .await;
    assert_eq!(unknown.executor_reference.as_deref(), Some(job_id.as_str()));
    assert!(
        degraded_observed_at.elapsed() + Duration::from_millis(100) >= monitor_grace,
        "execution became unknown before the configured {monitor_grace:?} transport grace"
    );
}

#[tokio::test]
async fn running_check_allows_review_wait_cancel_and_releases_slot() {
    let fixture = fixture(1_000).await;
    let arguments = checks(&fixture, "running-check-1", &["test"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_validation_job");
    let job_id = start.job_id.unwrap();
    update_validation_job(
        &fixture.registry,
        &job_id,
        "running",
        None,
        None,
        check_progress(0, Some("test"), None),
    )
    .await;

    let review_started = Instant::now();
    let initial = fixture
        .call(
            "task_review",
            json!({"task_id": fixture.task_id, "include_diff": false, "include_output_tail": true}),
        )
        .await;
    assert!(review_started.elapsed() < Duration::from_millis(500));
    assert!(initial.ok, "{}", initial.body);
    assert!(matches!(
        initial.body["data"]["active_execution"]["execution_status"].as_str(),
        Some("queued" | "running")
    ));
    assert_eq!(initial.body["data"]["active_execution"]["kind"], "check");
    // Active execution with no applied edits: diff deferred, empty path list.
    let changes = &initial.body["data"]["changes"];
    assert_eq!(changes["source"], "live_workspace_deferred");
    assert_eq!(changes["changed_paths"], json!([]));
    assert!(changes["diff_preview"].is_null());

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
                "include_diff": false,
                "include_output_tail": true
            }),
        )
        .await
    });
    update_validation_job(
        &fixture.registry,
        &job_id,
        "running",
        Some("progress\n"),
        None,
        check_progress(0, Some("test"), None),
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

    let finish = finish(&fixture, "too early").await;
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
    let check = check_call.await.unwrap();
    assert!(cancelled.ok, "{}", cancelled.body);
    assert_eq!(cancelled.body["data"]["status"], "cancelled");
    assert_eq!(
        cancelled.body["data"]["execution"]["execution_status"],
        "cancelled"
    );
    assert_eq!(
        check.body["data"]["execution"]["execution_id"],
        cancelled.body["data"]["execution"]["execution_id"]
    );
    wait_for_monitor_count(
        &fixture,
        0,
        Duration::from_secs(10),
        "running validation cancellation monitor shutdown",
    )
    .await;
    wait_for_workspace_slot_state(
        &fixture,
        "idle",
        Duration::from_secs(10),
        "running validation cancellation",
    )
    .await;
}

#[tokio::test]
async fn queued_cancel_never_dispatches_and_restart_is_fail_closed() {
    let queued_fixture = fixture(50).await;
    let arguments = checks(&queued_fixture, "queued-check-1", &["test"]);
    let queued = queued_fixture.call("checks_run", arguments).await;
    assert_eq!(
        queued.body["data"]["execution"]["execution_status"],
        "queued"
    );
    assert_eq!(
        queued.body["data"]["execution"]["assertion_status"],
        "in_progress"
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
            .reserve(
                &task(&second),
                "check",
                "restart-operation",
                "restart-hash",
                &["test".to_string()],
                Some(&json!({"recipe_id":"rust"})),
                Some("restart-workspace"),
                30,
                10,
            )
            .unwrap(),
    );
    let recovery = second
        .connector
        .db
        .reconcile_connector_executions(&second.connector.context.project_id, 11)
        .unwrap();
    assert_eq!(recovery.1, 1);
    let interrupted = execution_by_id(&second, &execution.execution_id);
    assert_eq!(interrupted.state, "interrupted");
    assert!(interrupted.validated_workspace_sha256.is_none());
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
            .reserve(
                &resumed,
                "check",
                "unknown-operation",
                "unknown-hash",
                &["check".to_string()],
                Some(&json!({"recipe_id":"rust"})),
                Some("unknown-workspace"),
                30,
                13,
            )
            .unwrap(),
    );
    let unknown = second
        .connector
        .db
        .finish_connector_execution(
            &unknown.execution_id,
            ConnectorExecutionFailure::Unknown("transport_lost"),
            14,
        )
        .unwrap();
    assert!(unknown.validated_workspace_sha256.is_none());
    let finish = finish(&second, "must not finish").await;
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
            .reserve(
                &task(&fixture),
                "command",
                "cancel-transport-1",
                "cancel-transport-hash",
                &[],
                None,
                None,
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
    let durable = execution_by_id(&fixture, &execution.execution_id);
    assert_eq!(durable.executor_reference.as_deref(), Some(job_id));
    let finish = finish(&fixture, "must stay blocked").await;
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
    let released = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(
        released.slot_state, "idle",
        "a reset failure must not retain the writable-slot lease"
    );

    let retried = fixture
        .connector
        .workspace
        .prepare(
            &fixture.connector.context,
            "wc_task_retry_cancelled_workspace",
            "wc_run_retry_cancelled_workspace",
            false,
        )
        .expect("the verified managed slot must be reusable by the next task");
    assert_eq!(retried.execution_root, good_task.execution_root);
    assert_eq!(
        fixture
            .connector
            .workspace
            .discard_prepared(&fixture.connector.context.executor_root, &retried,),
        None
    );
    let idle = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(idle.slot_state, "idle");
}

#[tokio::test]
async fn wait_for_terminal_propagates_store_error_without_panicking() {
    let fixture = fixture(20).await;
    let execution = created(
        fixture
            .connector
            .executions
            .reserve(
                &task(&fixture),
                "command",
                "store-error-1",
                "store-error-hash",
                &[],
                None,
                None,
                30,
                2,
            )
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
        db.reserve_connector_execution(
            &task(&fixture),
            "command",
            "nonzero-operation",
            "nonzero-hash",
            &[],
            None,
            None,
            30,
            2,
        )
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
                    check_completed: None,
                    failed_check: None,
                    assertion_evidence: None,
                    validated_workspace_sha256: None,
                    executor_failure_code: None,
                    mcp_task_output_tail: None,
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
async fn connector_execution_recovering_status_remains_active_without_duplicate_or_success() {
    let fixture = fixture(20).await;
    let db = &fixture.connector.db;
    let task = task(&fixture);
    let execution = created(
        db.reserve_connector_execution(
            &task,
            "command",
            "recovering-operation",
            "recovering-request-hash",
            &[],
            None,
            None,
            30,
            2,
        )
        .unwrap(),
    );
    db.attach_connector_executor(&execution.execution_id, "job-recovering", "queued", 3)
        .unwrap();
    let queued_recovery = db
        .observe_connector_execution(
            &execution.execution_id,
            executor_status_observation("recovering", 1, 1, None, 4),
        )
        .unwrap();
    assert_eq!(queued_recovery.state, "queued");
    assert!(queued_recovery.is_active());
    assert!(queued_recovery.failure_code.is_none());
    assert!(queued_recovery.terminal_reason.is_none());

    let running = db
        .observe_connector_execution(
            &execution.execution_id,
            executor_status_observation("running", 2, 1, Some(5), 5),
        )
        .unwrap();
    assert_eq!(running.state, "running");
    let running_recovery = db
        .observe_connector_execution(
            &execution.execution_id,
            executor_status_observation("recovering", 2, 1, Some(5), 6),
        )
        .unwrap();
    assert_eq!(running_recovery.state, "running");
    assert!(running_recovery.is_active());

    match db
        .reserve_connector_execution(
            &task,
            "command",
            "recovering-operation",
            "recovering-request-hash",
            &[],
            None,
            None,
            30,
            7,
        )
        .unwrap()
    {
        ConnectorExecutionReservation::Existing(existing) => {
            assert_eq!(existing.execution_id, execution.execution_id);
        }
        ConnectorExecutionReservation::Created(_) => {
            panic!("recovering observation must not trigger duplicate execution")
        }
    }
}

#[tokio::test]
async fn unrecognized_executor_status_is_degraded_instead_of_running() {
    let fixture = fixture(50).await;
    let db = &fixture.connector.db;
    let execution = created(
        db.reserve_connector_execution(
            &task(&fixture),
            "command",
            "unrecognized-status",
            "unrecognized-hash",
            &[],
            None,
            None,
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
            executor_status_observation("future-agent-state", 1, 1, None, 4),
        )
        .unwrap();
    assert_eq!(observed.state, "queued");
    assert!(observed.is_active());
    assert!(observed.failure_code.is_none());
    assert!(observed.terminal_reason.is_none());
    assert_eq!(
        observed.status_failure_code.as_deref(),
        Some("executor_status_unrecognized")
    );
}

/// A `read_only` task has no shell, so `files_list` is its only way to learn
/// what the project contains. It must reach the agent as a Git-index listing
/// and come back rolled up, not as a filesystem walk.
#[tokio::test]
async fn read_only_files_list_reaches_the_agent_as_a_git_index_listing() {
    let fixture = fixture(20).await;
    let started = fixture
        .call(
            "task_start",
            json!({ "goal": "understand the project", "mode": "read_only" }),
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    let list_connector = fixture.connector.clone();
    let list_owner = fixture.owner.clone();
    let list_task = task_id.clone();
    let listing = tokio::spawn(async move {
        call(
            &list_connector,
            &list_owner,
            "files_list",
            json!({ "task_id": list_task, "depth": 1 }),
        )
        .await
    });

    let request = next_request(&fixture.registry).await;
    // The agent is asked for the index, never for a directory walk: that is
    // what keeps .venv and target out of the answer.
    assert!(
        request.command.contains("git ls-files -z --cached"),
        "{}",
        request.command
    );
    assert!(
        request.command.contains("git rev-parse --git-dir"),
        "a non-repository must be distinguishable from an empty project: {}",
        request.command
    );
    fixture
        .registry
        .complete(ShellAgentResultRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some("README.md\0src/main.rs\0src/db/mod.rs\0".to_string()),
            stderr: None,
            duration_ms: Some(2),
            error: None,
        })
        .await
        .unwrap();

    let outcome = listing.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);
    let data = &outcome.body["data"];
    assert_eq!(data["source"], "git_index");
    assert_eq!(data["total_files"], 3);
    assert_eq!(
        data["entries"],
        json!([
            { "path": "README.md", "kind": "file" },
            { "path": "src/", "kind": "dir", "file_count": 2 },
        ])
    );
}

/// Bad input must be refused before an agent request exists, so a malformed
/// call cannot occupy the executor.
#[tokio::test]
async fn files_list_rejects_out_of_range_input_without_reaching_the_agent() {
    let fixture = fixture(20).await;
    let started = fixture
        .call(
            "task_start",
            json!({ "goal": "inspect", "mode": "read_only" }),
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    for arguments in [
        json!({ "task_id": task_id, "depth": 0 }),
        json!({ "task_id": task_id, "limit": 5000 }),
        json!({ "task_id": task_id, "path": "../escape" }),
        json!({ "task_id": task_id, "globs": [""] }),
    ] {
        let outcome = fixture.call("files_list", arguments.clone()).await;
        assert!(
            !outcome.ok,
            "{arguments} should be rejected: {}",
            outcome.body
        );
    }
    assert!(
        poll(&fixture.registry).await.is_none(),
        "rejected input must not reach the agent"
    );
}

#[tokio::test]
async fn persisted_read_only_isolated_task_cannot_finish_or_capture_result() {
    let fixture = fixture(20).await;
    let before = std::fs::read_to_string(
        Path::new(&fixture.connector.context.executor_root).join("README.md"),
    )
    .unwrap();
    let execution_root = task(&fixture).execution_root;
    std::fs::write(
        Path::new(&execution_root).join("README.md"),
        "malformed unvalidated patch\n",
    )
    .unwrap();
    fixture
        .connector
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_tasks SET mode = 'read_only' WHERE id = ?1",
            [&fixture.task_id],
        )
        .unwrap();
    let malformed = task(&fixture);
    assert_eq!(malformed.mode, "read_only");
    assert!(malformed.isolated);
    assert_ne!(malformed.execution_root, malformed.target_root);

    let outcome = finish(&fixture, "must not capture malformed writable state").await;
    assert!(!outcome.ok, "{}", outcome.body);
    assert_eq!(outcome.http_status, 409);
    assert_eq!(outcome.body["error"]["code"], "task_state_invalid");
    let result_count: i64 = fixture
        .connector
        .db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_task_results WHERE task_id = ?1",
            [&fixture.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        result_count, 0,
        "malformed task must not capture a stable result"
    );
    assert_eq!(
        std::fs::read_to_string(
            Path::new(&fixture.connector.context.executor_root).join("README.md")
        )
        .unwrap(),
        before
    );
}

#[tokio::test]
async fn persisted_legacy_inspect_task_is_observable_but_never_executable() {
    let fixture = fixture(20).await;
    fixture
        .connector
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_tasks SET mode = 'inspect' WHERE id = ?1",
            [&fixture.task_id],
        )
        .unwrap();
    assert_eq!(task(&fixture).mode, "inspect");

    let listed = fixture.call("task_list", json!({})).await;
    assert!(listed.ok, "{}", listed.body);
    let listed_json = serde_json::to_string(&listed.body).unwrap();
    assert!(listed_json.contains(&fixture.task_id));

    let reviewed = fixture
        .call(
            "task_review",
            json!({ "task_id": fixture.task_id, "include_diff": false }),
        )
        .await;
    assert!(reviewed.ok, "{}", reviewed.body);
    assert_eq!(reviewed.body["data"]["mode"], "inspect");

    let denied = [
        (
            "edits_apply",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "legacy-inspect-edit",
                "changes": [{
                    "kind": "create",
                    "path": "legacy-inspect.txt",
                    "content": "must not be created"
                }]
            }),
        ),
        (
            "commands_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "legacy-inspect-command",
                "command": "echo must-not-run",
                "timeout_secs": 30
            }),
        ),
        (
            "checks_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "legacy-inspect-check",
                "checks": ["check"],
                "timeout_secs": 30
            }),
        ),
        (
            "task_finish",
            json!({ "task_id": fixture.task_id, "summary": "must not finish" }),
        ),
        ("task_resume", json!({ "task_id": fixture.task_id })),
    ];
    for (capability, arguments) in denied {
        let outcome = fixture.call(capability, arguments).await;
        assert!(
            !outcome.ok,
            "{capability} unexpectedly succeeded: {}",
            outcome.body
        );
        assert_eq!(outcome.http_status, 409, "{capability}: {}", outcome.body);
        assert_eq!(
            outcome.body["error"]["code"], "inspect_mode_retired",
            "{capability}: {}",
            outcome.body
        );
    }

    assert!(
        poll(&fixture.registry).await.is_none(),
        "a persisted inspect task must not dispatch work to the Runner"
    );
    assert!(!Path::new(&task(&fixture).execution_root)
        .join("legacy-inspect.txt")
        .exists());
    assert_eq!(task(&fixture).mode, "inspect");
}

/// A read_only task must refuse commands before any durable trace exists.
///
/// read_only promises no consequential command execution regardless of the
/// Runner's ordinary shell capability.
#[tokio::test]
async fn read_only_commands_run_is_denied_even_when_agent_supports_shell() {
    let fixture = fixture(20).await;
    fixture
        .registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                host_context: None,
                capabilities: Some(crate::test_support::current_runner_capabilities(
                    ShellClientCapabilities {
                        shell: true,
                        project_lifecycle: false,
                        project_path_registration: false,
                        internal_posix_script: true,
                        ..Default::default()
                    },
                )),
                projects: Some(vec![project_summary(
                    "project",
                    Path::new(&fixture.connector.context.executor_root),
                )]),
                agent_protocol_version: Some("polling-v1".into()),
                policy: None,
            },
            Some(&fixture.owner),
        )
        .await
        .unwrap();

    let started = fixture
        .call(
            "task_start",
            json!({ "goal": "inspect", "mode": "read_only" }),
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    let outcome = fixture
        .call(
            "commands_run",
            json!({
                "task_id": task_id,
                "operation_id": "read-only-shell",
                "command": "echo hello",
                "timeout_secs": 30
            }),
        )
        .await;
    assert!(!outcome.ok, "{}", outcome.body);
    assert_eq!(outcome.http_status, 403);
    assert_eq!(outcome.body["error"]["code"], "read_only_task");
}
/// The denial has to happen before anything is created — an approval, a
/// reservation, or an agent request would each be a durable trace of work that
/// read_only said would not happen.
#[tokio::test]
async fn read_only_denial_creates_no_approval_reservation_or_agent_request() {
    let fixture = fixture(20).await;
    let started = fixture
        .call(
            "task_start",
            json!({ "goal": "inspect", "mode": "read_only" }),
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    let outcome = fixture
        .call(
            "commands_run",
            json!({
                "task_id": task_id,
                "operation_id": "read-only-shell",
                "command": "echo hello",
                "timeout_secs": 30
            }),
        )
        .await;
    assert_eq!(outcome.body["error"]["code"], "read_only_task");

    // No approval was created for the human to accidentally grant.
    let approvals = fixture
        .connector
        .db
        .local_pending_connector_approvals(&fixture.connector.context.project_id, 10)
        .expect("approvals");
    assert!(approvals.is_empty(), "{approvals:?}");

    // No execution was reserved.
    let execution = fixture
        .connector
        .db
        .latest_connector_execution(
            &task_id,
            &fixture.connector.context.project_id,
            &super::stable_subject_id(&fixture.owner).unwrap(),
            Some("read-only-shell"),
        )
        .expect("execution lookup");
    assert!(execution.is_none(), "{execution:?}");

    // And nothing was queued for the agent to run.
    let queued = fixture
        .registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(queued.is_none(), "{queued:?}");
}

/// The normal path keeps its gate: the same command still needs one-time
/// host-local approval before it runs.
#[tokio::test]
async fn normal_commands_run_still_requires_exact_one_time_approval() {
    let fixture = fixture_restricted(20).await;
    let arguments = json!({
        "task_id": fixture.task_id,
        "operation_id": "needs-approval",
        "command": "echo hello",
        "timeout_secs": 30
    });
    let waiting = fixture.call("commands_run", arguments.clone()).await;
    assert_eq!(waiting.body["error"]["code"], "approval_required");
    let approval_id = waiting.body["data"]["approval"]["approval_id"]
        .as_str()
        .expect("approval id");

    // A different command cannot ride the same approval.
    let other = fixture
        .call(
            "commands_run",
            json!({
                "task_id": fixture.task_id,
                "operation_id": "different-command",
                "command": "echo goodbye",
                "timeout_secs": 30
            }),
        )
        .await;
    assert_eq!(other.body["error"]["code"], "approval_required");
    assert_ne!(
        other.body["data"]["approval"]["approval_id"]
            .as_str()
            .unwrap(),
        approval_id,
        "a second command reused the first command's approval"
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
            subject_id: tests::PROJECT_SUBJECT_ID,
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
            subject_id: tests::PROJECT_SUBJECT_ID,
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
                tests::PROJECT_SUBJECT_ID,
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
            tests::PROJECT_SUBJECT_ID,
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
                "changes": changes_json.clone()
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
            .db
            .begin_connector_edit_operation(
                task_id,
                &connector.context.project_id,
                tests::PROJECT_SUBJECT_ID,
                "device-pending-1",
                &request_sha256,
                now,
            )
            .unwrap(),
        ConnectorEditOperationGate::Started
    );
    let uncertain = connector
        .call(
            "edits_apply",
            json!({"task_id": task_id, "operation_id": "device-pending-1", "changes": changes_json}),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(uncertain.body["error"]["code"], "edit_operation_uncertain");
    assert_eq!(
        connector
            .workspace
            .discard_prepared(&connector.context.executor_root, &prepared),
        None
    );
}

/// Closest in-process replay of the manifestless Python acceptance path.
#[tokio::test]
async fn manifestless_python_unittest_checks_finish_with_clean_result() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let git = |args: &[&str]| {
        assert!(Command::new("git")
            .arg("-C")
            .arg(&project)
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    git(&["init", "-q"]);
    fs::write(
        project.join("calculator.py"),
        "def add(a, b):\n    return a - b\n",
    )
    .unwrap();
    fs::write(
        project.join("test_calculator.py"),
        "import unittest\nfrom calculator import add\nclass T(unittest.TestCase):\n    def test_sum(self):\n        self.assertEqual(add(2, 3), 5)\n",
    )
    .unwrap();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"polyglot-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "i",
    ]);
    let baseline = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&project)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let registry = Arc::new(ShellClientRegistry::default());
    let owner = tests::auth("u1");
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                host_context: None,
                capabilities: Some(crate::test_support::current_runner_capabilities(
                    ShellClientCapabilities {
                        shell: true,
                        file_read: true,
                        file_write: true,
                        jobs: true,
                        async_jobs: true,
                        async_shell_jobs: true,
                        structured_validation_argv: true,
                        ..Default::default()
                    },
                )),
                projects: Some(vec![project_summary("project", &project)]),
                agent_protocol_version: Some("polling-v1".into()),
                policy: None,
            },
            Some(&owner),
        )
        .await
        .unwrap();
    let state = temp.path().join("state");
    let connector = Arc::new(
        ConnectorRuntime::new(
            Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
                registry.clone(),
            )),
            Arc::new(Database::open(&temp.path().join("connector.db")).unwrap()),
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
                project_grant_id: tests::PROJECT_GRANT_ID.into(),
            },
            tests::credential(),
        )
        .unwrap(),
    );
    let reg = registry.clone();
    let registration = tokio::spawn(async move {
        let request = next_request(&reg).await;
        let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        reg.complete(ShellAgentResultRequest {
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
    let started = call(
        &connector,
        &owner,
        "task_start",
        json!({"goal": "fix add", "mode": "normal"}),
    )
    .await;
    registration.await.unwrap();
    assert!(started.ok, "{}", started.body);
    let task_id = started.body["task_id"].as_str().unwrap().to_string();
    let task = connector
        .db
        .connector_task(
            &task_id,
            &connector.context.project_id,
            tests::PROJECT_SUBJECT_ID,
        )
        .unwrap();
    let root = PathBuf::from(&task.execution_root);
    fs::write(
        root.join("calculator.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("__pycache__")).unwrap();
    fs::write(root.join("__pycache__/x.pyc"), b"junk").unwrap();

    let check_reg = registry.clone();
    let checker = tokio::spawn(async move {
        let request = next_request(&check_reg).await;
        let steps: Vec<ShellJobValidationStep> = serde_json::from_str(&request.command).unwrap();
        assert_eq!(request.kind, "start_validation_job");
        assert_eq!(steps[0].args, ["-B", "-m", "unittest", "discover", "-v"]);
        update_validation_job(
            &check_reg,
            request.job_id.as_deref().unwrap(),
            "completed",
            Some("OK"),
            Some(0),
            check_progress(1, None, None),
        )
        .await;
    });
    let checked = call(
        &connector,
        &owner,
        "checks_run",
        json!({
            "task_id": task_id,
            "operation_id": "py-unittest-1",
            "checks": ["test"],
            "recipe": "python",
            "timeout_secs": 30
        }),
    )
    .await;
    checker.await.unwrap();
    assert!(checked.ok, "{}", checked.body);
    assert_eq!(
        checked.body["data"]["execution"]["assertion_status"],
        "passed"
    );
    assert_eq!(checked.body["data"]["execution"]["recipe"]["id"], "python");

    let finished = call(
        &connector,
        &owner,
        "task_finish",
        json!({"task_id": task_id, "summary": "fixed add"}),
    )
    .await;
    assert!(finished.ok, "{}", finished.body);
    let data = &finished.body["data"];
    assert_eq!(data["status"], "ready_for_review");
    assert_eq!(data["result"]["validation"]["status"], "passed");
    assert_eq!(data["result"]["changed_paths"], json!(["calculator.py"]));
    assert_eq!(data["result"]["decision_status"], "pending");
    assert_eq!(data["workspace"]["released"], true);
    assert!(data["result"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w
            .as_str()
            .is_some_and(|s| s.contains("ignored_generated_paths"))));
    assert_eq!(
        fs::read_to_string(project.join("calculator.py")).unwrap(),
        "def add(a, b):\n    return a - b\n"
    );
    let head = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&project)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert_eq!(head, baseline);
    assert!(String::from_utf8_lossy(
        &Command::new("git")
            .arg("-C")
            .arg(&project)
            .args(["status", "--porcelain"])
            .output()
            .unwrap()
            .stdout
    )
    .trim()
    .is_empty());
}

#[tokio::test]
async fn guidance_is_delivered_once_inside_the_next_capability_response() {
    let fixture = fixture(20).await;
    // Keep an execution active so task_review stays on the deferred branch
    // (no workspace scan) for both polls.
    let arguments = command_arguments(&fixture, "guided-review-1", "sleep 30");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    update_job(&fixture.registry, &job_id, "running", None, None).await;
    assert!(command_call.await.unwrap().ok);

    let guided = fixture
        .connector
        .host_guide(&fixture.task_id, "focus on the parser first");
    assert!(guided.ok, "{}", guided.body);

    // The host console continuously reviews the selected task. That host-side
    // projection must not claim guidance intended for the model.
    let host = fixture
        .connector
        .host_review(
            &fixture.owner,
            TaskReviewInput {
                task_id: fixture.task_id.clone(),
                include_diff: None,
                after_cursor: None,
                wait_ms: None,
                max_events: None,
                include_output_tail: None,
            },
        )
        .await;
    assert!(host.ok, "{}", host.body);
    assert!(host.body["guidance"].is_null());

    // The host review surfaces the guidance read-state for the console
    // timeline — the watermark the model has claimed and the newest
    // still-pending guidance — without advancing the watermark itself.
    assert_eq!(host.body["guidance_seen_seq"].as_i64(), Some(0));
    let unread_seq = host.body["unread_guidance_seq"].as_i64();
    assert!(unread_seq.is_some() && unread_seq.unwrap() > 0);

    // The next model-facing capability response carries the guidance…
    let review = fixture
        .call("task_review", json!({"task_id": fixture.task_id}))
        .await;
    assert!(review.ok, "{}", review.body);
    let guidance = review.body["data"]["guidance"]
        .as_array()
        .expect("guidance list");
    assert_eq!(guidance.len(), 1);
    assert_eq!(guidance[0]["message"], "focus on the parser first");
    assert!(review.body["data"]["guidance_note"].is_string());

    // …and exactly once: the following response does not repeat it, while
    // the durable event stays visible in the timeline for humans.
    let second = fixture
        .call("task_review", json!({"task_id": fixture.task_id}))
        .await;
    assert!(second.ok, "{}", second.body);
    assert!(second.body["data"]["guidance"].is_null());
    let events = second.body["data"]["recent_events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["kind"] == "human_guidance"));

    // Release the workspace slot.
    let stop_registry = fixture.registry.clone();
    let stop_job = job_id.clone();
    let stopper = tokio::spawn(async move {
        let stop = next_request(&stop_registry).await;
        assert_eq!(stop.kind, "stop_job");
        update_job(&stop_registry, &stop_job, "stopped", None, Some(-1)).await;
    });
    let cancelled = fixture
        .call("task_cancel", json!({"task_id": fixture.task_id}))
        .await;
    assert!(cancelled.ok, "{}", cancelled.body);
    stopper.await.unwrap();
}

#[tokio::test]
async fn finished_command_response_carries_pending_guidance() {
    let fixture = fixture(1_000).await;
    let arguments = command_arguments(&fixture, "guided-cmd-1", "printf done");
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let command_call =
        tokio::spawn(async move { call(&connector, &owner, "commands_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    let job_id = start.job_id.unwrap();
    // Operator guidance lands while the command is still running; the
    // completed command's own response must carry it back to the model.
    assert!(
        fixture
            .connector
            .host_guide(&fixture.task_id, "stop after this and run the tests")
            .ok
    );
    update_job(&fixture.registry, &job_id, "completed", None, Some(0)).await;
    let completed = command_call.await.unwrap();
    assert!(completed.ok, "{}", completed.body);
    let guidance = completed.body["data"]["guidance"]
        .as_array()
        .expect("guidance on command completion");
    assert_eq!(guidance[0]["message"], "stop after this and run the tests");
}

#[tokio::test]
async fn denied_approval_reason_reaches_the_model() {
    let fixture = fixture_restricted(20).await;
    let arguments = json!({
        "task_id": fixture.task_id,
        "operation_id": "denied-cmd-1",
        "command": "rm -rf target && echo done",
        "timeout_secs": 30
    });
    let waiting = fixture.call("commands_run", arguments.clone()).await;
    assert_eq!(waiting.body["error"]["code"], "approval_required");
    let approval_id = waiting.body["data"]["approval"]["approval_id"]
        .as_str()
        .unwrap()
        .to_string();
    // The pending summary must show the human what would run (first line,
    // bounded preview) — approvals are informed consent, not blind signing.
    let summary = waiting.body["data"]["approval"]["action_summary"]
        .as_str()
        .unwrap();
    assert!(
        summary.contains("rm -rf target && echo done"),
        "summary must carry the command preview: {summary}"
    );
    let pending = fixture
        .connector
        .db
        .local_pending_connector_approvals(
            &fixture.connector.context.project_id,
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0.approval_id, approval_id);

    // Host denies with a reason; the model's retry carries it back.
    fixture
        .connector
        .db
        .decide_connector_approval(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            &approval_id,
            false,
            "host_console",
            Some("use cargo clean instead of rm"),
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    let retry = fixture.call("commands_run", arguments).await;
    assert_eq!(retry.body["error"]["code"], "approval_denied");
    let message = retry.body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("use cargo clean instead of rm"),
        "denial reason must reach the model: {message}"
    );
    assert_eq!(
        retry.body["data"]["approval"]["decision_reason"],
        "use cargo clean instead of rm"
    );
    // Decided approvals leave the pending queue.
    let pending = fixture
        .connector
        .db
        .local_pending_connector_approvals(
            &fixture.connector.context.project_id,
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn host_devices_returns_the_agent_projection() {
    let fixture = fixture(20).await;
    let devices = fixture.connector.host_devices(&fixture.owner).await;
    assert!(devices.success, "{:?}", devices.error);
    let agents = devices.output["agents"].as_array().expect("agents array");
    assert!(devices.output["count"].is_number());
    if let Some(agent) = agents.first() {
        assert!(agent["client_id"].is_string());
        assert!(agent.get("connected").is_some());
        assert!(agent.get("last_seen_age_secs").is_some());
        assert!(agent.get("capabilities").is_some());
    }
}

#[tokio::test]
async fn checks_run_steers_cargo_at_the_shared_target_cache() {
    let fixture = fixture(1_000).await;
    let arguments = checks(&fixture, "shared-cache-1", &["check", "test"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_validation_job");
    let steps: Vec<crate::shell_protocol::ShellJobValidationStep> =
        serde_json::from_str(&start.command).expect("validation steps json");
    assert!(!steps.is_empty());
    for step in &steps {
        assert!(step.is_canonical(), "{} must stay canonical", step.name);
        let target = step
            .env
            .iter()
            .find(|(key, _)| key == "CARGO_TARGET_DIR")
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("{} step missing CARGO_TARGET_DIR", step.name));
        assert!(
            target.ends_with("cache/cargo-target"),
            "shared cache path expected, got {target}"
        );
    }
    // Unblock the slot.
    let job_id = start.job_id.unwrap();
    update_validation_job(
        &fixture.registry,
        &job_id,
        "completed",
        None,
        Some(0),
        check_progress(2, None, None),
    )
    .await;
    assert!(check_call.await.unwrap().ok);
}

#[tokio::test]
async fn provenance_mismatch_fails_honestly_with_evidence() {
    let fixture = fixture(1_000).await;
    let arguments = checks(&fixture, "provenance-1", &["check"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_validation_job");
    let job_id = start.job_id.unwrap();
    // Simulate a check that generates an untracked build artifact.
    std::fs::write(
        std::path::Path::new(&task(&fixture).execution_root).join("build-artifact.tmp"),
        "junk",
    )
    .unwrap();
    update_validation_job(
        &fixture.registry,
        &job_id,
        "completed",
        None,
        Some(0),
        check_progress(1, None, None),
    )
    .await;
    let outcome = check_call.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);

    // Deterministic invariant failure: terminal quickly (no grace burn),
    // honestly categorized, with the evidence and the remedy in the message.
    let execution = wait_for_execution(
        &fixture,
        None,
        Duration::from_secs(10),
        "workspace provenance mismatch terminal state",
        |execution| execution.is_terminal(),
    )
    .await;
    assert_eq!(execution.state, "failed");
    assert_eq!(execution.failure_source.as_deref(), Some("workspace"));
    assert_eq!(
        execution.failure_code.as_deref(),
        Some("workspace_provenance_mismatch")
    );
    let projection = execution::execution_projection(&execution, 10, None);
    assert_eq!(
        projection["next_action"],
        "inspect_workspace_changes_then_rerun_checks"
    );
    let evidence = execution.assertion_evidence.expect("evidence must persist");
    assert_eq!(evidence["invariant"], "workspace_provenance");
    let detail = evidence["detail"].as_str().unwrap();
    assert!(detail.contains("build-artifact.tmp"), "{detail}");
    assert!(detail.contains(".gitignore"), "{detail}");
    assert!(
        serde_json::to_vec(&evidence).unwrap().len() <= crate::db::MAX_ASSERTION_EVIDENCE_BYTES
    );

    // The reviewer is never blinded by the wedged workspace: when the
    // show_changes scan errors, the review degrades to the durable
    // applied-path record instead of failing outright.
    let review_connector = fixture.connector.clone();
    let review_owner = fixture.owner.clone();
    let review_task = fixture.task_id.clone();
    let review = tokio::spawn(async move {
        call(
            &review_connector,
            &review_owner,
            "task_review",
            json!({"task_id": review_task}),
        )
        .await
    });
    let scan = next_request(&fixture.registry).await;
    fixture
        .registry
        .complete(ShellAgentResultRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            request_id: scan.request_id,
            // A branch header marks a real repository, so the non-zero exit is
            // a hard scan failure rather than the tolerated non-git degrade.
            exit_code: Some(1),
            stdout: Some("## main\n".into()),
            stderr: Some("fatal: unable to read tree".into()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let review = review.await.unwrap();
    assert!(review.ok, "{}", review.body);
    let changes = &review.body["data"]["changes"];
    assert_eq!(changes["source"], "workspace_scan_failed");
    assert_eq!(changes["changed_paths_source"], "applied_edits");
    assert_eq!(changes["changed_paths_complete"], true);
    assert_eq!(changes["changed_paths_total"], 0);
    assert!(changes["changed_paths"].as_array().is_some());
}

#[tokio::test]
async fn provenance_mismatch_from_tracked_changes_keeps_gitignore_out_of_the_remedy() {
    let fixture = fixture(1_000).await;
    let arguments = checks(&fixture, "provenance-tracked-1", &["check"]);
    let connector = fixture.connector.clone();
    let owner = fixture.owner.clone();
    let check_call =
        tokio::spawn(async move { call(&connector, &owner, "checks_run", arguments).await });
    let start = next_request(&fixture.registry).await;
    assert_eq!(start.kind, "start_validation_job");
    let job_id = start.job_id.unwrap();
    // Simulate a check that rewrites a tracked file (e.g. a formatter or a
    // build script touching committed sources) without creating anything
    // untracked. The fingerprint must still change, but the remedy must not
    // talk about .gitignore — ignoring a tracked file cannot fix this.
    let readme = std::path::Path::new(&task(&fixture).execution_root).join("README.md");
    assert!(readme.exists(), "fixture workspace must track README.md");
    std::fs::write(&readme, "fixture rewritten by the check\n").unwrap();
    update_validation_job(
        &fixture.registry,
        &job_id,
        "completed",
        None,
        Some(0),
        check_progress(1, None, None),
    )
    .await;
    let outcome = check_call.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);

    let execution = wait_for_execution(
        &fixture,
        None,
        Duration::from_secs(10),
        "tracked workspace provenance mismatch terminal state",
        |execution| execution.is_terminal(),
    )
    .await;
    assert_eq!(execution.state, "failed");
    assert_eq!(execution.failure_source.as_deref(), Some("workspace"));
    assert_eq!(
        execution.failure_code.as_deref(),
        Some("workspace_provenance_mismatch")
    );
    let projection = execution::execution_projection(&execution, 10, None);
    assert_eq!(
        projection["next_action"],
        "inspect_workspace_changes_then_rerun_checks"
    );
    let evidence = execution.assertion_evidence.expect("evidence must persist");
    assert_eq!(evidence["invariant"], "workspace_provenance");
    let detail = evidence["detail"].as_str().unwrap();
    assert!(
        detail.contains("no untracked files were detected"),
        "{detail}"
    );
    assert!(
        detail.contains("Inspect or revert workspace changes"),
        "{detail}"
    );
    assert!(!detail.contains(".gitignore"), "{detail}");
    assert!(
        serde_json::to_vec(&evidence).unwrap().len() <= crate::db::MAX_ASSERTION_EVIDENCE_BYTES
    );
}

#[tokio::test]
async fn scan_failure_review_caps_applied_paths_and_reports_the_true_total() {
    let fixture = fixture(20).await;
    let project_id = fixture.connector.context.project_id.clone();

    // Persist more distinct applied paths than the review will ever list,
    // through the same durable edits_apply events the runtime writes.
    let distinct = MAX_REVIEW_APPLIED_PATHS + 5;
    let paths: Vec<String> = (0..distinct)
        .map(|index| format!("src/gen/file-{index:03}.rs"))
        .collect();
    let mut now = 500;
    for chunk in paths.chunks(16) {
        fixture
            .connector
            .db
            .append_connector_task_event(
                &fixture.task_id,
                &project_id,
                tests::PROJECT_SUBJECT_ID,
                "edits_apply",
                &json!({ "ok": true, "dry_run": false, "changed_paths": chunk }),
                now,
            )
            .unwrap();
        now += 1;
    }
    // Re-applying paths after the cap is already crossed must not inflate
    // the total: the count is over distinct paths, not over edit events.
    fixture
        .connector
        .db
        .append_connector_task_event(
            &fixture.task_id,
            &project_id,
            tests::PROJECT_SUBJECT_ID,
            "edits_apply",
            &json!({ "ok": true, "dry_run": false, "changed_paths": &paths[..16] }),
            now,
        )
        .unwrap();
    // Bury the edit events under more noise than any recent-event window
    // holds: the review must read the persisted applied-edit query, not the
    // tail of the timeline.
    for index in 0..60 {
        now += 1;
        fixture
            .connector
            .db
            .append_connector_task_event(
                &fixture.task_id,
                &project_id,
                tests::PROJECT_SUBJECT_ID,
                "files_read",
                &json!({ "index": index }),
                now,
            )
            .unwrap();
    }

    let review_connector = fixture.connector.clone();
    let review_owner = fixture.owner.clone();
    let review_task = fixture.task_id.clone();
    let review = tokio::spawn(async move {
        call(
            &review_connector,
            &review_owner,
            "task_review",
            json!({"task_id": review_task}),
        )
        .await
    });
    let scan = next_request(&fixture.registry).await;
    fixture
        .registry
        .complete(ShellAgentResultRequest {
            client_id: "hosted".into(),
            agent_instance_id: "instance".into(),
            request_id: scan.request_id,
            exit_code: Some(1),
            stdout: Some("## main\n".into()),
            stderr: Some("fatal: unable to read tree".into()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let review = review.await.unwrap();
    assert!(review.ok, "{}", review.body);
    let changes = &review.body["data"]["changes"];
    assert_eq!(changes["source"], "workspace_scan_failed");
    assert_eq!(changes["changed_paths_source"], "applied_edits");
    assert_eq!(changes["changed_paths_complete"], false);
    assert_eq!(changes["changed_paths_total"], distinct);
    let listed = changes["changed_paths"].as_array().unwrap();
    assert_eq!(listed.len(), MAX_REVIEW_APPLIED_PATHS);
    // First-seen order, no duplicates, and nothing past the cap leaks in.
    assert_eq!(listed[0], "src/gen/file-000.rs");
    assert_eq!(
        listed[MAX_REVIEW_APPLIED_PATHS - 1],
        format!("src/gen/file-{:03}.rs", MAX_REVIEW_APPLIED_PATHS - 1)
    );
    let unique: std::collections::HashSet<&str> = listed.iter().filter_map(Value::as_str).collect();
    assert_eq!(unique.len(), MAX_REVIEW_APPLIED_PATHS);
    for overflow in &paths[MAX_REVIEW_APPLIED_PATHS..] {
        assert!(
            !unique.contains(overflow.as_str()),
            "path past the cap leaked into the bounded list: {overflow}"
        );
    }
}

#[tokio::test]
async fn task_list_and_task_resume_bootstrap_a_new_session() {
    let fixture = fixture(20).await;

    // A fresh session discovers durable work without knowing any task_id, so
    // the response carries no task binding.
    let listed = fixture.call("task_list", json!({})).await;
    assert!(listed.ok, "{}", listed.body);
    assert_eq!(listed.body["task_id"], Value::Null);
    let tasks = listed.body["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1, "{}", listed.body);
    assert_eq!(tasks[0]["task_id"], json!(fixture.task_id));
    assert_eq!(tasks[0]["goal"], "exercise durable execution");
    assert_eq!(tasks[0]["task_status"], "active");
    assert_eq!(tasks[0]["next_action"], "task_resume");

    for arguments in [json!({ "limit": 0 }), json!({ "limit": 21 })] {
        let outcome = fixture.call("task_list", arguments).await;
        assert_eq!(
            outcome.body["error"]["code"], "invalid_arguments",
            "{}",
            outcome.body
        );
    }

    // Resume rebinds the session: goal, state, and a next step; no guidance
    // is pending yet.
    let resumed = fixture
        .call("task_resume", json!({ "task_id": fixture.task_id }))
        .await;
    assert!(resumed.ok, "{}", resumed.body);
    assert_eq!(resumed.body["data"]["goal"], "exercise durable execution");
    assert_eq!(resumed.body["data"]["task_status"], "active");
    assert_eq!(resumed.body["data"]["applied_paths"], json!([]));
    assert_eq!(resumed.body["data"]["result"], Value::Null);
    assert!(
        resumed.body["data"]["guidance"].is_null(),
        "{}",
        resumed.body
    );

    // Guidance recorded between sessions is claimed by the next resume,
    // exactly once — the same channel task_review uses.
    let guided = fixture
        .connector
        .host_guide(&fixture.task_id, "focus on the parser first");
    assert!(guided.ok, "{}", guided.body);
    let resumed = fixture
        .call("task_resume", json!({ "task_id": fixture.task_id }))
        .await;
    assert!(resumed.ok, "{}", resumed.body);
    assert!(
        resumed.body["data"]["guidance"][0]["message"]
            .as_str()
            .unwrap()
            .contains("parser"),
        "{}",
        resumed.body
    );
    let resumed_again = fixture
        .call("task_resume", json!({ "task_id": fixture.task_id }))
        .await;
    assert!(
        resumed_again.body["data"]["guidance"].is_null(),
        "guidance is claimed exactly once: {}",
        resumed_again.body
    );

    // Each rebind of a running task is visible on the console timeline.
    let rebinds: i64 = fixture
        .connector
        .db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_task_events
             WHERE task_id = ?1 AND kind = 'task_resume'",
            [fixture.task_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rebinds, 3);

    let unknown = fixture
        .call(
            "task_resume",
            json!({ "task_id": "wc_task_00000000000000000000000000000000" }),
        )
        .await;
    assert_eq!(
        unknown.body["error"]["code"], "task_not_found",
        "{}",
        unknown.body
    );
}
