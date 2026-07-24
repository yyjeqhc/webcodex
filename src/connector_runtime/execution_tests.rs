use super::*;
use crate::db::{ConnectorExecutionFailure, ConnectorExecutionObservation};
use crate::shell_client::{ShellClientRegistry, ShellJobStartMetadata};
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentPollRequest, ShellAgentProjectSummary,
    ShellAgentResultRequest, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellJobOpRequest, ShellJobValidationProgress,
    ShellJobValidationStep,
};
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
    let owner = tests::auth("u1");
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    structured_validation_jobs: true,
                    ..Default::default()
                }),
                projects: Some(vec![project_summary("project", &project)]),
                agent_protocol_version: Some("test".into()),
                policy: None,
            },
            Some(&owner),
        )
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
        job_id: job_id.into(),
        request_id: None,
        status: status.into(),
        stdout_chunk: None,
        stderr_chunk: None,
        stdout_tail: None,
        stderr_tail: None,
        exit_code: None,
        duration_ms: Some(1),
        error: None,
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
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    structured_validation_jobs: false,
                    ..Default::default()
                }),
                projects: Some(vec![project_summary(
                    "project",
                    Path::new(&fixture.connector.context.executor_root),
                )]),
                agent_protocol_version: Some("old-agent".into()),
                policy: None,
            },
            Some(&fixture.owner),
        )
        .await
        .unwrap();
    let old_agent = fixture.connector.readiness(&fixture.owner).await.unwrap();
    assert!(!old_agent.ready);
    assert!(old_agent.findings.iter().any(|finding| {
        finding.code == "structured_validation_unavailable"
            && finding.status == crate::project_entry::ReadinessStatus::Fail
    }));

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
        assert!(steps[0].command.contains("cargo fmt -- --check"));
        assert!(steps[1].command.contains("cargo check --all-targets"));
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
    assert_eq!(
        validation_projection(Some(&durable)),
        json!({
            "status": "passed",
            "execution_id": execution_id,
            "checks": [
                {"check": "format", "status": "passed"},
                {"check": "check", "status": "passed"}
            ],
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
async fn validation_step_spawn_failure_is_executor_failure_without_assertion_evidence() {
    let fixture = fixture(1_000).await;
    let registry = fixture.registry.clone();
    let responder = tokio::spawn(async move {
        let request = next_request(&registry).await;
        assert_eq!(request.kind, "start_validation_job");
        let job_id = request.job_id.unwrap();
        update_validation_job(
            &registry,
            &job_id,
            "running",
            Some("format completed\n"),
            None,
            check_progress(1, Some("check"), None),
        )
        .await;
        let mut failed = validation_job_update(&job_id, "failed", check_progress(1, None, None));
        failed.error = Some("validation_step_spawn_failed".to_string());
        let updated = registry.update_job(failed).await.unwrap();
        assert_eq!(updated.status, "failed");
    });

    let outcome = fixture
        .call(
            "checks_run",
            checks(&fixture, "spawn-failure", &["format", "check"]),
        )
        .await;
    responder.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);
    let execution = &outcome.body["data"]["execution"];
    assert_eq!(execution["execution_status"], "failed");
    assert_eq!(execution["failure_source"], "executor");
    assert_eq!(execution["failure_code"], "validation_step_spawn_failed");
    assert_ne!(execution["assertion_status"], "failed");
    assert!(execution["assertion_evidence"].is_null());
    assert_eq!(execution["checks"][1]["status"], "not_run");

    let durable = fixture
        .connector
        .db
        .connector_execution(execution["execution_id"].as_str().unwrap())
        .unwrap();
    assert!(durable.failed_check.is_none());
    assert!(durable.assertion_evidence.is_none());
    assert!(durable.validated_workspace_sha256.is_none());
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
async fn finish_fingerprint_and_result_capture_exclude_a_concurrent_edit() {
    let fixture = fixture(1_000).await;
    let command_arguments = approve(&fixture, "atomic-finish-command-1", "printf late").await;
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
    let arguments = approve(&fixture, "reservation-command-1", "printf reserved").await;
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
    let arguments = approve(&fixture, "mutating-command-1", command).await;
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
    assert_eq!(
        reopened
            .connector_execution(execution_id)
            .unwrap()
            .assertion_evidence,
        durable.assertion_evidence
    );

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
async fn structured_progress_rejects_invalid_order_and_preserves_fail_fast_plan() {
    let fixture = fixture(1_000).await;
    let plan = || ShellJobStartMetadata {
        project_id: None,
        session_id: None,
        validation_steps: ["format", "check", "test"]
            .into_iter()
            .map(|name| ShellJobValidationStep {
                name: name.into(),
                command: "true".into(),
            })
            .collect(),
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
            job_id: job.job_id,
            request_id: None,
            status: status.into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code,
            duration_ms: Some(1),
            error: None,
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
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: None,
            duration_ms: None,
            error: None,
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
async fn old_agent_cannot_receive_a_structured_validation_job() {
    let fixture = fixture(1_000).await;
    fixture
        .registry
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: "hosted".into(),
                agent_instance_id: "instance".into(),
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    structured_validation_jobs: false,
                    ..Default::default()
                }),
                projects: None,
                agent_protocol_version: Some("old-test".into()),
                policy: None,
            },
            Some(&fixture.owner),
        )
        .await
        .unwrap();
    let outcome = fixture
        .call(
            "checks_run",
            checks(&fixture, "old-agent-check-1", &["check"]),
        )
        .await;
    assert_eq!(
        outcome.body["error"]["code"],
        "structured_validation_unavailable"
    );
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
    assert_eq!(outcome.body["error"]["code"], "invalid_arguments");
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
                tests::PROJECT_SUBJECT_ID,
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
    let cancelled_id = cancelled.body["data"]["execution"]["execution_id"]
        .as_str()
        .unwrap();
    assert!(fixture
        .connector
        .db
        .connector_execution(cancelled_id)
        .unwrap()
        .validated_workspace_sha256
        .is_none());
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
async fn transient_check_status_recovers_within_grace() {
    let fixture = fixture_configured(20, |service| service.with_monitor_timing(200, 5)).await;
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
    tokio::time::sleep(Duration::from_millis(30)).await;
    let degraded = fixture
        .connector
        .db
        .latest_connector_execution(
            &fixture.task_id,
            &fixture.connector.context.project_id,
            tests::PROJECT_SUBJECT_ID,
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

    update_validation_job(
        &fixture.registry,
        &job_id,
        "running",
        None,
        None,
        check_progress(0, Some("check"), None),
    )
    .await;
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
async fn check_transport_failure_becomes_unknown_only_after_grace() {
    let fixture = fixture_configured(5, |service| service.with_monitor_timing(80, 5)).await;
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
            json!({"task_id": fixture.task_id, "include_diff": false}),
        )
        .await;
    assert!(review_started.elapsed() < Duration::from_millis(500));
    assert!(initial.ok, "{}", initial.body);
    assert!(matches!(
        initial.body["data"]["active_execution"]["execution_status"].as_str(),
        Some("queued" | "running")
    ));
    assert_eq!(initial.body["data"]["active_execution"]["kind"], "check");

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
    let resources = workspace::WorkspaceManager::resource_status(
        Path::new(&fixture.connector.context.runs_root),
        fixture._temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(resources.slot_state, "idle");
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
    let interrupted = second
        .connector
        .db
        .connector_execution(&execution.execution_id)
        .unwrap();
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
            .reserve(
                &task(&fixture),
                "command",
                "store-error-1",
                "store-error-hash",
                &[],
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
            "command",
            "unrecognized-status",
            "unrecognized-hash",
            &[],
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
            ConnectorExecutionObservation {
                executor_status: "future-agent-state",
                stdout_cursor: 1,
                stderr_cursor: 1,
                exit_code: None,
                started_at: None,
                finished_at: None,
                check_completed: None,
                failed_check: None,
                assertion_evidence: None,
                validated_workspace_sha256: None,
                executor_failure_code: None,
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
