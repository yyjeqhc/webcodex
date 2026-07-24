use super::*;
use crate::connector_runtime::{ConnectorContext, ConnectorRuntime, ConnectorRuntimeSlot};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentPollRequest, ShellAgentProjectSummary,
    ShellAgentResultRequest, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellJobValidationProgress,
};
use crate::tool_runtime::ToolRuntime;
use salvo::prelude::{affix_state, handler, Depot, Request, Router, Service, StatusCode};
use salvo::test::{ResponseExt, TestClient};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(name);
    let state = temp.path().join("state");
    fs::create_dir(&root).unwrap();
    git(&["init", "-q"], &root);
    fs::write(root.join("README.md"), "fixture\n").unwrap();
    git(&["add", "README.md"], &root);
    git(
        &[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
        &root,
    );
    (temp, root, state)
}

fn options(root: PathBuf, state: PathBuf) -> ProjectCommandOptions {
    ProjectCommandOptions {
        root,
        profile: "personal".to_string(),
        state_dir: Some(state),
        json: false,
    }
}

fn fact<'a>(readiness: &'a ProjectReadiness, code: &str) -> &'a ReadinessFact {
    readiness
        .findings
        .iter()
        .find(|finding| finding.code == code)
        .unwrap_or_else(|| panic!("missing readiness fact {code}: {readiness:?}"))
}

struct AuthenticatedProjectFixture {
    _temp: tempfile::TempDir,
    service: Service,
    db: Arc<crate::Database>,
    registry: Arc<ShellClientRegistry>,
    connector: Arc<ConnectorRuntime>,
    agent_auth: crate::auth::AuthContext,
    credential: String,
    client_id: String,
    root: PathBuf,
    state: PathBuf,
    recorded_requests: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct AuthenticatedRequestRecorder(Arc<Mutex<Vec<String>>>);

#[handler]
async fn record_authenticated_connector_request(req: &mut Request, depot: &mut Depot) {
    if let Ok(recorder) = depot.obtain::<AuthenticatedRequestRecorder>() {
        recorder
            .0
            .lock()
            .unwrap()
            .push(req.uri().path().to_string());
    }
}

async fn authenticated_project_fixture() -> AuthenticatedProjectFixture {
    authenticated_project_fixture_for("rust").await
}

async fn authenticated_project_fixture_for(recipe: &str) -> AuthenticatedProjectFixture {
    let (temp, root, state) = repo("authenticated");
    match recipe {
        "rust" => fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='golden-fixture'\nversion='0.1.0'\n",
        )
        .unwrap(),
        "node" => {
            fs::write(
                root.join("package.json"),
                r#"{"packageManager":"npm@10","scripts":{"check":"eslint ."}}"#,
            )
            .unwrap();
            fs::write(root.join("package-lock.json"), "{}").unwrap();
        }
        "python" => {
            fs::write(root.join("pyproject.toml"), "[tool.ruff]\nline-length=88\n").unwrap()
        }
        "go" => fs::write(
            root.join("go.mod"),
            "module example.test/golden\n\ngo 1.22\n",
        )
        .unwrap(),
        _ => unreachable!(),
    }
    git(&["add", "."], &root);
    git(
        &[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "add validation manifest",
        ],
        &root,
    );
    let options = options(root.clone(), state.clone());
    setup(&options).unwrap();
    let (config, paths) = ProjectConfig::resolve(&options).unwrap();
    let connector_key = read_private_value(&paths.connector_key).unwrap();
    let bootstrap_key = read_private_value(&paths.bootstrap_key).unwrap();
    let grant_id = config.project_grant_id(&paths);
    let credential_verifier =
        crate::auth::ProjectCredentialVerifier::new(grant_id.clone(), &connector_key).unwrap();
    let agent_auth = credential_verifier.authenticate(&connector_key).unwrap();
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: config.executor_client_id.clone(),
                agent_instance_id: "project-agent-instance".to_string(),
                display_name: Some("configured project Agent".to_string()),
                owner: Some("local-owner".to_string()),
                hostname: Some("private-host".to_string()),
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    structured_validation_argv: true,
                    ..Default::default()
                }),
                projects: Some(vec![ShellAgentProjectSummary {
                    id: config.executor_project_id.clone(),
                    name: Some(config.project_name.clone()),
                    path: config.root.to_string_lossy().into_owned(),
                    allow_patch: true,
                    kind: Some("auto".to_string()),
                    description: None,
                    hooks: Vec::new(),
                    disabled: false,
                    git_branch: Some("main".to_string()),
                    git_head: None,
                    git_dirty: Some(false),
                    updated_at: 1,
                    shell_profile: None,
                }]),
                agent_protocol_version: Some("test".to_string()),
                policy: None,
            },
            Some(&agent_auth),
        )
        .await
        .unwrap();
    let db = Arc::new(crate::Database::open(&state.join("data/webcodex.db")).unwrap());
    let tools = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
        registry.clone(),
    ));
    let runtime_project_id = config.runtime_project_id();
    let connector = Arc::new(
        ConnectorRuntime::new(
            tools.clone(),
            db.clone(),
            ConnectorContext {
                project_id: config.logical_project_id.clone(),
                project_name: config.project_name.clone(),
                workspace_id: config.workspace_id.clone(),
                executor_project: runtime_project_id,
                executor_root: config.root.to_string_lossy().into_owned(),
                runs_root: paths.runs.to_string_lossy().into_owned(),
                results_root: paths.results.to_string_lossy().into_owned(),
                projects_dir: paths.projects.to_string_lossy().into_owned(),
                profile: config.profile.clone(),
                project_grant_id: grant_id,
            },
            credential_verifier,
        )
        .unwrap(),
    );
    let recorded_requests = Arc::new(Mutex::new(Vec::new()));
    let http_config = Arc::new(crate::Config {
        addr: "127.0.0.1:0".to_string(),
        data_dir: state.join("data"),
        token: Some(bootstrap_key),
        max_text_size: 2 * 1024 * 1024,
        max_file_size: 100 * 1024 * 1024,
        codex: crate::CodexConfig::default(),
        oauth2: crate::OAuth2Config::default(),
    });
    let router = Router::new()
        .hoop(affix_state::inject(http_config))
        .hoop(affix_state::inject(db.clone()))
        .hoop(affix_state::inject(tools))
        .hoop(affix_state::inject(ConnectorRuntimeSlot(Some(
            connector.clone(),
        ))))
        .hoop(affix_state::inject(AuthenticatedRequestRecorder(
            recorded_requests.clone(),
        )))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .hoop(record_authenticated_connector_request)
                .push(crate::connector_runtime::http::routes()),
        );
    AuthenticatedProjectFixture {
        _temp: temp,
        service: Service::new(router),
        db,
        registry,
        connector,
        agent_auth,
        credential: connector_key,
        client_id: config.executor_client_id,
        root,
        state,
        recorded_requests,
    }
}

fn response_status(response: &salvo::Response) -> StatusCode {
    response.status_code.unwrap_or(StatusCode::OK)
}

async fn post_connector(
    fixture: &AuthenticatedProjectFixture,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let mut response = TestClient::post(&format!("http://localhost{path}"))
        .bearer_auth(token)
        .json(&body)
        .send(&fixture.service)
        .await;
    let status = response_status(&response);
    let body = response
        .take_json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    (status, body)
}

const PROJECT_AGENT_INSTANCE: &str = "project-agent-instance";

async fn next_project_agent_request(
    registry: &ShellClientRegistry,
    client_id: &str,
) -> ShellAgentShellRequest {
    for _ in 0..2_000 {
        if let Some(request) = registry
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: PROJECT_AGENT_INSTANCE.to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            return request;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("configured project Agent did not receive a request");
}

async fn complete_project_agent_request(
    registry: &ShellClientRegistry,
    client_id: &str,
    request: ShellAgentShellRequest,
    exit_code: i32,
    stdout: String,
    stderr: String,
) {
    registry
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: PROJECT_AGENT_INSTANCE.to_string(),
            request_id: request.request_id,
            exit_code: Some(exit_code),
            stdout: Some(stdout),
            stderr: Some(stderr),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

fn record_agent_request(recorder: &Arc<Mutex<Vec<String>>>, request: &ShellAgentShellRequest) {
    recorder.lock().unwrap().push(request.kind.clone());
}

fn run_agent_shell_request(request: &ShellAgentShellRequest) -> (i32, String, String) {
    let mut command = Command::new("sh");
    command.args(["-lc", &request.command]);
    if let Some(cwd) = request.cwd.as_deref() {
        command.current_dir(cwd);
    }
    let output = command.output().unwrap();
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

async fn complete_project_job(
    registry: &ShellClientRegistry,
    client_id: &str,
    request: ShellAgentShellRequest,
    validation: bool,
) {
    let job_id = request.job_id.expect("job dispatch must include job_id");
    registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: client_id.to_string(),
            agent_instance_id: PROJECT_AGENT_INSTANCE.to_string(),
            job_id,
            request_id: None,
            status: "completed".to_string(),
            stdout_chunk: Some(if validation {
                "check passed\n".to_string()
            } else {
                "command completed\n".to_string()
            }),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: Some(0),
            duration_ms: Some(1),
            error: None,
            validation_progress: validation.then_some(ShellJobValidationProgress {
                completed: 1,
                current_step: None,
                failed_step: None,
            }),
            finished: true,
        })
        .await
        .unwrap();
}

struct GoldenPathEvidence {
    request_paths: Vec<String>,
    event_kinds: Vec<String>,
    agent_request_kinds: Vec<String>,
    accepted_output: String,
    accepted_content: String,
    execution_count: i64,
    recipe_id: String,
}

async fn run_authenticated_golden_path(recipe: &str) -> GoldenPathEvidence {
    let fixture = authenticated_project_fixture_for(recipe).await;
    let agent_requests = Arc::new(Mutex::new(Vec::new()));

    let (status, readiness) = post_connector(
        &fixture,
        "/api/connector/readiness",
        &fixture.credential,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(readiness["ready"], true);

    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let registration = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "register_project");
        let payload: serde_json::Value =
            serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        let stdout = serde_json::json!({
            "agent_project_id": payload["id"],
            "client_id": client_id,
            "name": payload["name"],
            "path": payload["path"],
            "allow_patch": true
        })
        .to_string();
        complete_project_agent_request(&registry, &client_id, request, 0, stdout, String::new())
            .await;
    });
    let (status, started) = post_connector(
        &fixture,
        "/api/connector/task/start",
        &fixture.credential,
        serde_json::json!({
            "goal": "exercise the authenticated project golden path",
            "mode": "normal"
        }),
    )
    .await;
    registration.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(started["ok"], true, "{started}");
    let task_id = started["task_id"].as_str().unwrap().to_string();

    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let reader = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "file_read");
        let stdout = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "path": "README.md",
            "content": "fixture\n",
            "start_line": 1,
            "total_lines": 1,
            "truncated": false
        })
        .to_string();
        complete_project_agent_request(&registry, &client_id, request, 0, stdout, String::new())
            .await;
    });
    let (status, read) = post_connector(
        &fixture,
        "/api/connector/files/read",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "files": [{"path": "README.md"}]
        }),
    )
    .await;
    reader.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["ok"], true, "{read}");

    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let searcher = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "run_shell");
        let (exit_code, stdout, stderr) = run_agent_shell_request(&request);
        complete_project_agent_request(&registry, &client_id, request, exit_code, stdout, stderr)
            .await;
    });
    let (status, searched) = post_connector(
        &fixture,
        "/api/connector/files/search",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "pattern": "fixture",
            "limit": 10
        }),
    )
    .await;
    searcher.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(searched["ok"], true, "{searched}");

    let task = fixture
        .db
        .local_connector_task(&task_id, &fixture.connector.context().project_id)
        .unwrap();
    let execution_root = PathBuf::from(&task.execution_root);
    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let editor = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "file_apply_text_edits");
        fs::write(execution_root.join("golden.txt"), "accepted\n").unwrap();
        let stdout = serde_json::json!({
            "dry_run": false,
            "applied_count": 1,
            "changed": true,
            "would_change": true,
            "files": [{"index": 0, "kind": "create", "path": "golden.txt"}],
            "changed_paths": ["golden.txt"]
        })
        .to_string();
        complete_project_agent_request(&registry, &client_id, request, 0, stdout, String::new())
            .await;
    });
    let (status, edited) = post_connector(
        &fixture,
        "/api/connector/edits/apply",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "operation_id": "golden-edit-1",
            "changes": [{
                "kind": "create",
                "path": "golden.txt",
                "content": "accepted\n"
            }]
        }),
    )
    .await;
    editor.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["ok"], true, "{edited}");

    let command = serde_json::json!({
        "task_id": task_id,
        "operation_id": "golden-command-1",
        "command": "printf golden-command",
        "timeout_secs": 30
    });
    let (status, waiting) = post_connector(
        &fixture,
        "/api/connector/commands/run",
        &fixture.credential,
        command.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(waiting["error"]["code"], "approval_required");
    let approval_id = waiting["data"]["approval"]["approval_id"].as_str().unwrap();
    let approval = crate::task_cli::parse(&[
        "approve".to_string(),
        task_id.clone(),
        approval_id.to_string(),
        "--root".to_string(),
        fixture.root.to_string_lossy().into_owned(),
        "--state-dir".to_string(),
        fixture.state.to_string_lossy().into_owned(),
    ])
    .unwrap();
    assert!(crate::task_cli::run(approval).unwrap().contains("Approved"));

    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let commander = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "start_job");
        complete_project_job(&registry, &client_id, request, false).await;
    });
    let (status, commanded) = post_connector(
        &fixture,
        "/api/connector/commands/run",
        &fixture.credential,
        command,
    )
    .await;
    commander.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(commanded["ok"], true, "{commanded}");

    let registry = fixture.registry.clone();
    let client_id = fixture.client_id.clone();
    let recorder = agent_requests.clone();
    let expected_program = match recipe {
        "rust" => "cargo",
        "node" => "npm",
        "python" => "python",
        "go" => "go",
        _ => unreachable!(),
    }
    .to_string();
    let checker = tokio::spawn(async move {
        let request = next_project_agent_request(&registry, &client_id).await;
        record_agent_request(&recorder, &request);
        assert_eq!(request.kind, "start_validation_job");
        let steps: Vec<crate::shell_protocol::ShellJobValidationStep> =
            serde_json::from_str(&request.command).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].program, expected_program);
        complete_project_job(&registry, &client_id, request, true).await;
    });
    let (status, checked) = post_connector(
        &fixture,
        "/api/connector/checks/run",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "operation_id": "golden-check-1",
            "checks": ["check"],
            "recipe": recipe,
            "timeout_secs": 30
        }),
    )
    .await;
    checker.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["data"]["execution"]["assertion_status"], "passed");

    let (status, finished) = post_connector(
        &fixture,
        "/api/connector/task/finish",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "summary": "authenticated golden path result"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finished["ok"], true, "{finished}");
    assert_eq!(finished["data"]["status"], "ready_for_review");

    let (status, reviewed) = post_connector(
        &fixture,
        "/api/connector/task/review",
        &fixture.credential,
        serde_json::json!({
            "task_id": task_id,
            "include_diff": true
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reviewed["ok"], true, "{reviewed}");
    assert!(reviewed["data"]["changes"]["diff_preview"]["text"]
        .as_str()
        .unwrap()
        .contains("accepted"));

    let accept = crate::task_cli::parse(&[
        "accept".to_string(),
        task_id.clone(),
        "--root".to_string(),
        fixture.root.to_string_lossy().into_owned(),
        "--state-dir".to_string(),
        fixture.state.to_string_lossy().into_owned(),
    ])
    .unwrap();
    let accepted_output = crate::task_cli::run(accept).unwrap();
    let accepted_content = fs::read_to_string(fixture.root.join("golden.txt")).unwrap();
    let event_kinds = fixture
        .db
        .local_connector_task_events(&task_id, &fixture.connector.context().project_id, 100)
        .unwrap()
        .into_iter()
        .map(|event| event.kind)
        .collect();
    let execution_count = fixture
        .db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM wc_executions", [], |row| row.get(0))
        .unwrap();

    let request_paths = fixture.recorded_requests.lock().unwrap().clone();
    let agent_request_kinds = agent_requests.lock().unwrap().clone();
    GoldenPathEvidence {
        request_paths,
        event_kinds,
        agent_request_kinds,
        accepted_output,
        accepted_content,
        execution_count,
        recipe_id: checked["data"]["execution"]["recipe"]["id"]
            .as_str()
            .unwrap()
            .to_string(),
    }
}

#[tokio::test]
async fn configured_project_credential_can_complete_connector_golden_path() {
    let evidence = run_authenticated_golden_path("rust").await;
    assert!(evidence.accepted_output.contains("Accepted"));
    assert_eq!(evidence.accepted_content, "accepted\n");
    assert!(evidence.execution_count >= 2);
    for capability in [
        "task_started",
        "files_read",
        "files_search",
        "edits_apply",
        "approval_requested",
        "approval_granted",
        "approval_consumed",
        "execution_succeeded",
        "task_finished",
        "workspace_release",
        "task_accepted",
    ] {
        assert!(
            evidence.event_kinds.iter().any(|kind| kind == capability),
            "durable task ledger did not record {capability}: {:?}",
            evidence.event_kinds
        );
    }
}

#[tokio::test]
async fn authenticated_golden_path_emits_no_discovery_or_session_calls() {
    let evidence = run_authenticated_golden_path("rust").await;
    let canonical_paths = [
        "/api/connector/readiness",
        "/api/connector/task/start",
        "/api/connector/files/read",
        "/api/connector/files/search",
        "/api/connector/edits/apply",
        "/api/connector/commands/run",
        "/api/connector/checks/run",
        "/api/connector/task/finish",
        "/api/connector/task/review",
    ];
    assert!(evidence
        .request_paths
        .iter()
        .all(|path| canonical_paths.contains(&path.as_str())));
    assert_eq!(
        evidence.request_paths,
        [
            "/api/connector/readiness",
            "/api/connector/task/start",
            "/api/connector/files/read",
            "/api/connector/files/search",
            "/api/connector/edits/apply",
            "/api/connector/commands/run",
            "/api/connector/commands/run",
            "/api/connector/checks/run",
            "/api/connector/task/finish",
            "/api/connector/task/review",
        ]
    );
    assert_eq!(
        evidence
            .request_paths
            .iter()
            .filter(|path| path.as_str() == "/api/connector/commands/run")
            .count(),
        2,
        "the authenticated recorder must include approval request and exact approved retry"
    );
    for forbidden in [
        "list_projects",
        "runtime_status",
        "tool_manifest",
        "start_session",
        "current_session",
        "list_agents",
    ] {
        assert!(!evidence
            .request_paths
            .iter()
            .chain(evidence.event_kinds.iter())
            .chain(evidence.agent_request_kinds.iter())
            .any(|record| record.contains(forbidden)));
    }
}

#[tokio::test]
async fn checks_run_project_aware_golden_paths_cover_rust_node_python_and_go() {
    for recipe in ["rust", "node", "python", "go"] {
        let evidence = run_authenticated_golden_path(recipe).await;
        assert_eq!(evidence.recipe_id, recipe);
        assert!(evidence.accepted_output.contains("Accepted"));
        assert_eq!(evidence.accepted_content, "accepted\n");
        assert!(evidence
            .agent_request_kinds
            .iter()
            .any(|kind| kind == "start_validation_job"));
    }
}

#[test]
fn fresh_setup_is_minimal_idempotent_and_does_not_expose_internal_ids() {
    let (_temp, root, state) = repo("demo");
    let options = options(root, state.clone());

    let first = setup(&options).unwrap();
    assert_eq!(first.status, "configured");
    assert_eq!(
        first.changed,
        ["Connection", "Agent", "Project registration"]
    );
    let agent = fs::read_to_string(state.join("agent/agent.toml")).unwrap();
    let registration = fs::read_to_string(
        fs::read_dir(state.join("agent/projects.d"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    let agent_toml: toml::Value = toml::from_str(&agent).unwrap();
    let registration_toml: toml::Value = toml::from_str(&registration).unwrap();
    assert_eq!(
        registration_toml["path"].as_str(),
        Some(options.root.to_string_lossy().as_ref())
    );
    assert_eq!(
        agent_toml["projects_dir"].as_str(),
        Some(state.join("agent/projects.d").to_string_lossy().as_ref())
    );
    let before = (agent, registration);

    let second = setup(&options).unwrap();
    assert_eq!(second.status, "already_configured");
    assert!(second.changed.is_empty());
    assert_eq!(
        fs::read_to_string(state.join("agent/agent.toml")).unwrap(),
        before.0
    );
    let project_file = fs::read_dir(state.join("agent/projects.d"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(fs::read_to_string(project_file).unwrap(), before.1);

    let output = render_setup_text(&second);
    for forbidden in [
        "client_id",
        "runtime project",
        "executor_ref",
        "workflow session",
        "agent:local",
        "wc_proj_",
        "token",
        "credentials/",
    ] {
        assert!(
            !output.to_ascii_lowercase().contains(forbidden),
            "default setup output leaked {forbidden}: {output}"
        );
    }
    assert!(output.contains("Next:\n  webcodex doctor"));
}

#[test]
fn setup_preserves_a_valid_legacy_client_identity() {
    let (_temp, root, state) = repo("legacy-client");
    let options = options(root, state.clone());
    setup(&options).unwrap();

    let config_path = state.join("project.toml");
    let mut config: ProjectConfig =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config.executor_client_id = "local-legacy-client".to_string();
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let agent_path = state.join("agent/agent.toml");
    let mut agent: toml::Value = toml::from_str(&fs::read_to_string(&agent_path).unwrap()).unwrap();
    agent["client_id"] = toml::Value::String(config.executor_client_id);
    fs::write(&agent_path, toml::to_string_pretty(&agent).unwrap()).unwrap();
    let before = (
        fs::read(&config_path).unwrap(),
        fs::read(&agent_path).unwrap(),
    );

    let report = setup(&options).unwrap();
    assert_eq!(report.status, "already_configured");
    assert!(report.changed.is_empty());
    assert_eq!(fs::read(config_path).unwrap(), before.0);
    assert_eq!(fs::read(agent_path).unwrap(), before.1);
}

#[test]
fn setup_repairs_only_missing_components_and_preserves_existing_config() {
    let (_temp, root, state) = repo("repair");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let agent_path = state.join("agent/agent.toml");
    let original_agent = fs::read_to_string(&agent_path).unwrap();
    let project_path = fs::read_dir(state.join("agent/projects.d"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&project_path).unwrap();

    let report = setup(&options).unwrap();
    assert_eq!(report.changed, ["Project registration"]);
    assert_eq!(fs::read_to_string(agent_path).unwrap(), original_agent);
    assert!(project_path.is_file());
}

#[test]
fn setup_conflict_and_project_root_collision_fail_closed() {
    let (temp, root, state) = repo("first");
    let first = options(root, state.clone());
    setup(&first).unwrap();
    let agent_path = state.join("agent/agent.toml");
    let before = fs::read_to_string(&agent_path).unwrap();
    let mut conflicting = before.replace("server_url = ", "server_url = \"http://invalid\" # ");
    if conflicting == before {
        conflicting.push_str("\nserver_url = \"http://invalid\"\n");
    }
    fs::write(&agent_path, conflicting).unwrap();

    let error = setup(&first).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("server_url"));

    let second_root = temp.path().join("second");
    fs::create_dir(&second_root).unwrap();
    git(&["init", "-q"], &second_root);
    let collision = options(second_root, state);
    let error = setup(&collision).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("project root"));
}

#[test]
fn setup_client_ids_include_project_grant_identity() {
    let (temp, root, _) = repo("grant-scoped-client");
    let first = options(root.clone(), temp.path().join("state-a"));
    let second = options(root, temp.path().join("state-b"));
    let (first_config, first_paths) = ProjectConfig::resolve(&first).unwrap();
    let (second_config, second_paths) = ProjectConfig::resolve(&second).unwrap();

    assert_eq!(
        first_config.logical_project_id,
        second_config.logical_project_id
    );
    assert_ne!(
        first_config.project_grant_id(&first_paths),
        second_config.project_grant_id(&second_paths)
    );
    assert_ne!(
        first_config.executor_client_id,
        second_config.executor_client_id
    );
}

#[test]
fn doctor_and_status_share_table_driven_readiness_facts_and_stay_read_only() {
    let (_temp, root, state) = repo("readiness");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let agent_path = state.join("agent/agent.toml");
    let before = fs::read(&agent_path).unwrap();

    let cases = [
        (RemoteProbe::Ready, true, "ready"),
        (RemoteProbe::Unreachable, false, "server_unreachable"),
        (
            RemoteProbe::CredentialRejected,
            false,
            "project_credential_rejected",
        ),
        (RemoteProbe::AgentOffline, false, "agent_offline"),
        (
            RemoteProbe::ProjectMissing,
            false,
            "project_registration_invalid",
        ),
        (
            RemoteProbe::RequiredCapabilityMissing,
            false,
            "required_capability_unavailable",
        ),
        (
            RemoteProbe::StructuredValidationMissing,
            false,
            "structured_validation_unavailable",
        ),
    ];
    for (probe, expected_ready, expected_code) in cases {
        let readiness = readiness_with_probe(&options, probe);
        assert_eq!(readiness.ready, expected_ready);
        assert!(readiness
            .findings
            .iter()
            .any(|finding| finding.code == expected_code));
        let status = render_status_text(&readiness);
        let doctor = render_doctor_text(&readiness);
        assert_eq!(
            status.contains("Coding access: ready"),
            expected_ready,
            "{status}"
        );
        assert!(status.contains(&format!("Capabilities: {}", readiness.capabilities)));
        assert!(doctor.contains(&fact(&readiness, expected_code).summary));
        for output in [status, doctor] {
            assert!(!output.contains("agent:"));
            assert!(!output.contains("client_id"));
            assert!(!output.contains("wc_proj_"));
        }
    }
    assert_eq!(fs::read(agent_path).unwrap(), before);
}

#[test]
fn doctor_reports_not_setup_and_invalid_workspace_with_stable_actions() {
    let (_temp, root, state) = repo("invalid");
    let options = options(root.clone(), state);
    let missing = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(missing.connection, "not configured");
    let finding = fact(&missing, "project_not_configured");
    assert_eq!(finding.status, ReadinessStatus::Fail);
    assert_eq!(finding.next_action.as_deref(), Some("webcodex setup"));

    setup(&options).unwrap();
    fs::remove_dir_all(root).unwrap();
    let invalid = readiness_with_probe(&options, RemoteProbe::Ready);
    assert_eq!(
        fact(&invalid, "workspace_unavailable").status,
        ReadinessStatus::Fail
    );
}

#[tokio::test]
async fn arbitrary_shared_key_cannot_access_project_connector() {
    // Expected pre-fix failure: project mode currently accepts every unknown
    // non-wc_* bearer as a new shared-key principal.
    let auth_env = crate::auth::AuthEnvGuard::new();
    auth_env.enable_direct_shared_key();
    let fixture = authenticated_project_fixture().await;

    let readiness = TestClient::post("http://localhost/api/connector/readiness")
        .bearer_auth("unconfigured-random-project-token")
        .json(&serde_json::json!({}))
        .send(&fixture.service)
        .await;
    let task_start = TestClient::post("http://localhost/api/connector/task/start")
        .bearer_auth("unconfigured-random-project-token")
        .json(&serde_json::json!({
            "goal": "must never be persisted",
            "mode": "read_only"
        }))
        .send(&fixture.service)
        .await;

    let conn = fixture.db.conn_for_tests();
    let task_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_tasks", [], |row| row.get(0))
        .unwrap();
    let execution_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_executions", [], |row| row.get(0))
        .unwrap();
    let binding_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_connector_grants", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(conn);
    let pending = fixture
        .registry
        .get_client_view_for_auth(&fixture.client_id, Some(&fixture.agent_auth))
        .await
        .unwrap()
        .pending_requests;

    assert_eq!(
        (
            response_status(&readiness),
            response_status(&task_start),
            task_count,
            execution_count,
            binding_count,
            pending,
        ),
        (
            StatusCode::UNAUTHORIZED,
            StatusCode::UNAUTHORIZED,
            0,
            0,
            0,
            0,
        ),
        "an unconfigured token must be rejected before connector state or Agent requests are created"
    );
    assert!(
        fixture.recorded_requests.lock().unwrap().is_empty(),
        "an arbitrary token must not reach the authenticated Connector adapter"
    );
}

#[tokio::test]
async fn connector_credential_cannot_cross_agent_auth_group() {
    // Expected pre-fix failure: a credential generated by another project is
    // accepted as a fresh shared-key group before ConnectorRuntime runs.
    let auth_env = crate::auth::AuthEnvGuard::new();
    auth_env.enable_direct_shared_key();
    let fixture = authenticated_project_fixture().await;
    let (_other_temp, other_root, other_state) = repo("other-authenticated-project");
    let other_options = options(other_root, other_state.clone());
    setup(&other_options).unwrap();
    let wrong_project_credential =
        read_private_value(&other_state.join("credentials/connector-key")).unwrap();
    let opaque_task_id = format!("wc_task_{}", "0".repeat(32));

    let calls = [
        ("/api/connector/readiness", serde_json::json!({})),
        (
            "/api/connector/task/start",
            serde_json::json!({"goal": "cross-project request", "mode": "read_only"}),
        ),
        (
            "/api/connector/files/read",
            serde_json::json!({"task_id": opaque_task_id, "files": [{"path": "README.md"}]}),
        ),
        (
            "/api/connector/files/search",
            serde_json::json!({"task_id": opaque_task_id, "query": "forbidden"}),
        ),
        (
            "/api/connector/edits/apply",
            serde_json::json!({
                "task_id": opaque_task_id,
                "operation_id": "cross-edit",
                "changes": [{"kind": "create", "path": "forbidden.txt", "content": "no"}]
            }),
        ),
        (
            "/api/connector/commands/run",
            serde_json::json!({
                "task_id": opaque_task_id,
                "operation_id": "cross-command",
                "command": "true"
            }),
        ),
        (
            "/api/connector/checks/run",
            serde_json::json!({
                "task_id": opaque_task_id,
                "operation_id": "cross-check",
                "checks": ["check"]
            }),
        ),
        (
            "/api/connector/task/cancel",
            serde_json::json!({"task_id": opaque_task_id, "reason": "cross-cancel"}),
        ),
        (
            "/api/connector/task/review",
            serde_json::json!({"task_id": opaque_task_id}),
        ),
    ];
    let mut responses = Vec::new();
    for (path, body) in calls {
        responses.push(post_connector(&fixture, path, &wrong_project_credential, body).await);
    }

    let conn = fixture.db.conn_for_tests();
    let task_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_tasks", [], |row| row.get(0))
        .unwrap();
    let execution_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_executions", [], |row| row.get(0))
        .unwrap();
    let binding_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_connector_grants", [], |row| {
            row.get(0)
        })
        .unwrap();
    let edit_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM wc_edit_operations", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(conn);
    let agent = fixture
        .registry
        .get_client_view_for_auth(&fixture.client_id, Some(&fixture.agent_auth))
        .await
        .unwrap();
    let serialized =
        serde_json::to_string(&responses.iter().map(|(_, body)| body).collect::<Vec<_>>()).unwrap();
    for private in [
        fixture.client_id.as_str(),
        "private-host",
        "project-agent-instance",
    ] {
        assert!(
            !serialized.contains(private),
            "cross-group response leaked private Agent registration data"
        );
    }
    assert!(
        responses
            .iter()
            .all(|(status, _)| *status == StatusCode::UNAUTHORIZED),
        "wrong-project credential statuses: {:?}",
        responses
            .iter()
            .map(|(status, _)| status.as_u16())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        (task_count, execution_count, binding_count, edit_count),
        (0, 0, 0, 0)
    );
    assert_eq!(agent.pending_requests, 0);
    assert!(!fixture.root.join("forbidden.txt").exists());
    assert!(
        fixture.recorded_requests.lock().unwrap().is_empty(),
        "a different project's credential must not reach Connector dispatch"
    );
}

#[tokio::test]
async fn connector_readiness_is_scoped_to_request_principal() {
    // Expected pre-fix failure: readiness ignores the request AuthContext, so
    // another project's credential can distinguish online from offline.
    let auth_env = crate::auth::AuthEnvGuard::new();
    auth_env.enable_direct_shared_key();
    let fixture = authenticated_project_fixture().await;
    let (_other_temp, other_root, other_state) = repo("readiness-other-project");
    let other_options = options(other_root, other_state.clone());
    setup(&other_options).unwrap();
    let wrong_project_credential =
        read_private_value(&other_state.join("credentials/connector-key")).unwrap();

    let online = post_connector(
        &fixture,
        "/api/connector/readiness",
        &wrong_project_credential,
        serde_json::json!({}),
    )
    .await;
    fixture
        .registry
        .set_last_seen_for_test(&fixture.client_id, 0)
        .await;
    let offline = post_connector(
        &fixture,
        "/api/connector/readiness",
        &wrong_project_credential,
        serde_json::json!({}),
    )
    .await;

    assert_eq!(online.0, StatusCode::UNAUTHORIZED);
    assert_eq!(offline.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        online.1, offline.1,
        "an unauthorized principal must not receive an Agent liveness oracle"
    );
}

#[test]
fn doctor_reports_malformed_registration() {
    let (_temp, root, state) = repo("malformed-registration");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let project_path = fs::read_dir(state.join("agent/projects.d"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&project_path, "this is not = valid TOML [").unwrap();
    let before = fs::read(&project_path).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_registration_invalid").status,
        ReadinessStatus::Fail
    );
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_not_configured"));
    assert_eq!(fs::read(project_path).unwrap(), before);
}

#[test]
fn doctor_reports_conflicting_registration() {
    let (_temp, root, state) = repo("conflicting-registration");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let project_path = fs::read_dir(state.join("agent/projects.d"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let before = fs::read_to_string(&project_path).unwrap();
    let conflicting = before.replace(
        &format!("path = \"{}\"", options.root.display()),
        "path = \"/different/project\"",
    );
    assert_ne!(conflicting, before);
    fs::write(&project_path, &conflicting).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_registration_invalid").status,
        ReadinessStatus::Fail
    );
    assert_eq!(fs::read_to_string(project_path).unwrap(), conflicting);
}

#[test]
fn doctor_reports_missing_private_credential() {
    let (_temp, root, state) = repo("missing-credential");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    fs::remove_file(state.join("credentials/connector-key")).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_credential_invalid").status,
        ReadinessStatus::Fail
    );
}

#[test]
fn doctor_reports_unreadable_private_credential() {
    let (_temp, root, state) = repo("unreadable-credential");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let credential = state.join("credentials/connector-key");
    fs::remove_file(&credential).unwrap();
    fs::create_dir(&credential).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_credential_invalid").status,
        ReadinessStatus::Fail
    );
}

#[test]
fn status_does_not_turn_invalid_config_into_not_configured() {
    let (_temp, root, state) = repo("malformed-config");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let config_path = state.join("project.toml");
    fs::write(&config_path, "version = [broken").unwrap();
    let before = fs::read(&config_path).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(readiness.connection, "invalid");
    assert!(readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_registration_invalid"));
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_not_configured"));
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[tokio::test]
async fn doctor_reports_rejected_project_credential_without_agent_offline() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (_temp, root, state) = repo("rejected-credential");
    let options = options(root, state);
    setup(&options).unwrap();
    let (config, _) = ProjectConfig::resolve(&options).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", config.port))
        .await
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 24\r\nconnection: close\r\n\r\n{\"error\":\"Unauthorized\"}",
            )
            .await
            .unwrap();
    });

    let readiness = collect_readiness(&options).await;
    server.await.unwrap();
    assert_eq!(readiness.connection, "connected");
    assert_eq!(
        fact(&readiness, "project_credential_rejected").status,
        ReadinessStatus::Fail
    );
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "server_unreachable"));
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "agent_offline"));
}
