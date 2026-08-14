use super::projections::{paginate_search_output, parse_search_cursor, search_cursor_signature};
use super::wire_models::{CodeNavigateInput, FilesSearchInput};
use super::*;
use crate::auth::{AuthKind, SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ};
use crate::lsp_bridge::{
    AgentLspRequest, AgentLspResultEnvelope, CallHierarchyDirection, CallHierarchyResult,
    LspAvailabilityStatus, LspServerStatusEntry, LspStatusResult, PublicCallHierarchySymbol,
    PublicPosition, PublicRange, PublicWorkspaceSymbol, WorkspaceSymbolsResult,
    AGENT_LSP_REQUEST_KIND,
};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};

pub(super) const PROJECT_GRANT_ID: &str = "wc_pgrant_1111111111111111";
pub(super) const PROJECT_SUBJECT_ID: &str = "project:wc_pgrant_1111111111111111";
const PROJECT_CREDENTIAL: &str =
    "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) fn credential() -> ProjectCredentialVerifier {
    ProjectCredentialVerifier::new(PROJECT_GRANT_ID.to_string(), PROJECT_CREDENTIAL).unwrap()
}

async fn register_agent(registry: &ShellClientRegistry, project_id: &str, path: &str) {
    register_agent_with_lsp(registry, project_id, path, false).await;
}

async fn register_agent_with_lsp(
    registry: &ShellClientRegistry,
    project_id: &str,
    path: &str,
    lsp_read_only_navigation: bool,
) {
    register_agent_with_lsp_capabilities(
        registry,
        project_id,
        path,
        lsp_read_only_navigation,
        lsp_read_only_navigation,
    )
    .await;
}

async fn register_agent_with_lsp_capabilities(
    registry: &ShellClientRegistry,
    project_id: &str,
    path: &str,
    lsp_read_only_navigation: bool,
    lsp_call_hierarchy: bool,
) {
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                client_id: "hosted".to_string(),
                agent_instance_id: "instance".to_string(),
                display_name: None,
                owner: Some("owner".to_string()),
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    structured_file_delete: false,
                    git: true,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    ssh_shell: false,
                    persistent_shell: false,
                    ssh_persistent_shell: false,
                    structured_validation_argv: true,
                    structured_go_test_json: true,
                    structured_go_test_tool: true,
                    structured_go_test_packages: true,
                    structured_process_argv: true,
                    structured_script_payload: false,
                    structured_execution_jobs: false,
                    lsp_read_only_navigation,
                    lsp_call_hierarchy,
                    sandbox_inspect_commands: false,
                    project_lifecycle: false,
                    project_path_registration: false,
                    computer_observe: false,
                    computer_accessibility_observe: false,
                    job_state_reconciliation: false,
                }),
                projects: Some(vec![ShellAgentProjectSummary {
                    id: project_id.to_string(),
                    name: Some(project_id.to_string()),
                    path: path.to_string(),
                    allow_patch: true,
                    kind: Some("auto".to_string()),
                    description: None,
                    hooks: Vec::new(),
                    disabled: false,
                    revision: None,
                    git_branch: Some("main".to_string()),
                    git_head: None,
                    git_dirty: Some(false),
                    updated_at: 1,
                    shell_profile: None,
                }]),
                agent_protocol_version: Some("test".to_string()),
                policy: None,
            },
            Some(&auth("u1")),
        )
        .await
        .unwrap();
}

async fn next_lsp_request(registry: &ShellClientRegistry) -> ShellAgentShellRequest {
    for _ in 0..200 {
        if let Some(request) = registry
            .poll(ShellAgentPollRequest {
                client_id: "hosted".to_string(),
                agent_instance_id: "instance".to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            assert_eq!(request.kind, AGENT_LSP_REQUEST_KIND);
            assert!(request.command.is_empty());
            return request;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    panic!("connector did not dispatch an LSP request");
}

async fn complete_lsp_request(
    registry: &ShellClientRegistry,
    request: &ShellAgentShellRequest,
    result: impl serde::Serialize,
) {
    registry
        .complete(ShellAgentResultRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            request_id: request.request_id.clone(),
            exit_code: Some(0),
            stdout: Some(AgentLspResultEnvelope::ok(result).to_stdout_json()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

pub(crate) fn init_repo(project: &Path) {
    std::fs::create_dir(project).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(project)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q"]);
    std::fs::write(project.join("README.md"), "fixture\n").unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"connector-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    run(&["add", "README.md", "Cargo.toml"]);
    run(&[
        "-c",
        "user.name=WebCodex Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "initial",
    ]);
}

pub(crate) fn auth(user_id: &str) -> AuthContext {
    let project_grant_id = if user_id == "u1" {
        PROJECT_GRANT_ID.to_string()
    } else {
        "wc_pgrant_2222222222222222".to_string()
    };
    AuthContext {
        role: Some("project".to_string()),
        scopes: vec![
            SCOPE_RUNTIME_READ.to_string(),
            SCOPE_PROJECT_READ.to_string(),
            SCOPE_PROJECT_WRITE.to_string(),
            SCOPE_JOB_RUN.to_string(),
        ],
        token_kind: Some("project".to_string()),
        project_grant_id: Some(project_grant_id),
        ..AuthContext::new(AuthKind::ProjectCredential)
    }
}

pub(super) fn connector() -> (tempfile::TempDir, ConnectorRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let runtime = Arc::new(ToolRuntime::new_for_tests());
    let connector = ConnectorRuntime::new(
        runtime,
        db,
        ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "demo".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:demo".to_string(),
            executor_root: project.to_string_lossy().to_string(),
            runs_root: temp.path().join("runs").to_string_lossy().to_string(),
            results_root: temp.path().join("results").to_string_lossy().to_string(),
            projects_dir: temp
                .path()
                .join("agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        credential(),
    )
    .unwrap();
    (temp, connector)
}

async fn connector_with_lsp(
    lsp_read_only_navigation: bool,
) -> (
    tempfile::TempDir,
    Arc<ConnectorRuntime>,
    Arc<ShellClientRegistry>,
) {
    connector_with_lsp_capabilities(lsp_read_only_navigation, lsp_read_only_navigation).await
}

async fn connector_with_lsp_capabilities(
    lsp_read_only_navigation: bool,
    lsp_call_hierarchy: bool,
) -> (
    tempfile::TempDir,
    Arc<ConnectorRuntime>,
    Arc<ShellClientRegistry>,
) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    register_agent_with_lsp_capabilities(
        &registry,
        "demo",
        &project.to_string_lossy(),
        lsp_read_only_navigation,
        lsp_call_hierarchy,
    )
    .await;
    let connector = Arc::new(
        ConnectorRuntime::new(
            Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
                registry.clone(),
            )),
            Arc::new(Database::open(&temp.path().join("connector.db")).unwrap()),
            ConnectorContext {
                project_id: "wc_proj_1234567890".to_string(),
                project_name: "demo".to_string(),
                workspace_id: "wc_ws_1234567890".to_string(),
                executor_project: "agent:hosted:demo".to_string(),
                executor_root: project.to_string_lossy().to_string(),
                runs_root: temp.path().join("runs").to_string_lossy().to_string(),
                results_root: temp.path().join("results").to_string_lossy().to_string(),
                projects_dir: temp
                    .path()
                    .join("agent/projects.d")
                    .to_string_lossy()
                    .to_string(),
                profile: "personal".to_string(),
                project_grant_id: PROJECT_GRANT_ID.to_string(),
            },
            credential(),
        )
        .unwrap(),
    );
    (temp, connector, registry)
}

async fn start_read_only_task(connector: &ConnectorRuntime, goal: &str) -> String {
    start_task_mode(connector, goal, "read_only").await
}

async fn start_task_mode(connector: &ConnectorRuntime, goal: &str, mode: &str) -> String {
    let started = connector
        .call(
            "task_start",
            json!({ "goal": goal, "mode": mode }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(started.ok, "{}", started.body);
    started.body["task_id"].as_str().unwrap().to_string()
}

fn connector_call_hierarchy_result(path: &str, line: usize, column: usize) -> CallHierarchyResult {
    let range = PublicRange {
        start: PublicPosition { line: 1, column: 1 },
        end: PublicPosition { line: 1, column: 5 },
    };
    CallHierarchyResult {
        project: "private-agent-project".to_string(),
        path: path.to_string(),
        language: "typescript".to_string(),
        query_position: PublicPosition { line, column },
        direction: CallHierarchyDirection::Both,
        depth: 2,
        roots: vec![PublicCallHierarchySymbol {
            name: "root".to_string(),
            kind: "function".to_string(),
            kind_code: 12,
            path: path.to_string(),
            range: range.clone(),
            selection_range: range,
        }],
        root_total_count: 1,
        root_returned_count: 1,
        edges: Vec::new(),
        returned_count: 0,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
        call_site_ranges_omitted: 0,
    }
}

#[tokio::test]
async fn same_window_reuses_task_and_appends_instruction_history() {
    let (_temp, connector) = connector();
    let owner = auth("u1");
    let window = ClientWindow::for_test("window-a");
    let first = connector
        .call_for_window(
            "task_start",
            json!({"goal": "inspect the parser", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(first.ok, "{}", first.body);
    assert_eq!(first.body["data"]["continuation"], "created");
    let task_id = first.body["task_id"].as_str().unwrap().to_string();
    let first_task = connector
        .db
        .connector_task(&task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
        .unwrap();
    connector
        .record_event(
            &first_task,
            "analysis_finding",
            json!({"summary": "the parser keeps the original error context"}),
            chrono::Utc::now().timestamp(),
        )
        .unwrap();

    let second = connector
        .call_for_window(
            "task_start",
            json!({"goal": "check the error path too", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(second.ok, "{}", second.body);
    assert_eq!(second.body["task_id"], task_id);
    assert_eq!(second.body["data"]["continuation"], "continued");
    assert_eq!(
        second.body["data"]["project_switch"]["restored_previous_context"],
        false
    );
    assert_eq!(second.body["data"]["history"]["preserved"], true);
    assert!(
        second.body["data"]["history"]["event_cursor_after"]
            .as_i64()
            .unwrap()
            > second.body["data"]["history"]["event_cursor_before"]
                .as_i64()
                .unwrap()
    );

    let tasks = connector
        .db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap();
    assert_eq!(
        tasks.len(),
        1,
        "same window/project must not duplicate tasks"
    );
    let events = connector
        .db
        .connector_task_events(
            &task_id,
            &connector.context.project_id,
            PROJECT_SUBJECT_ID,
            20,
        )
        .unwrap();
    assert_eq!(events[0].kind, "task_started");
    let appended = events
        .iter()
        .find(|event| event.kind == "task_instruction")
        .expect("follow-up instruction event");
    assert_eq!(appended.payload["instruction"], "check the error path too");
    assert!(events.iter().any(|event| {
        event.kind == "analysis_finding"
            && event.payload["summary"] == "the parser keeps the original error context"
    }));
    assert_eq!(
        connector
            .db
            .connector_task(&task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
            .unwrap()
            .goal,
        "inspect the parser",
        "the first goal remains the durable task root"
    );
    let stored_window_key: String = connector
        .db
        .conn_for_tests()
        .query_row(
            "SELECT window_key FROM wc_window_project_contexts WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_window_key.len(), 64);
    assert_ne!(stored_window_key, "window-a");
    let fingerprint_json: String = connector
        .db
        .conn_for_tests()
        .query_row(
            "SELECT fingerprint_json FROM wc_window_project_contexts WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!fingerprint_json.contains(&connector.context.executor_root));
}

#[tokio::test]
async fn simultaneous_first_turns_create_one_window_project_context() {
    let (_temp, connector) = connector();
    let owner = auth("u1");
    let window = ClientWindow::for_test("concurrent-window");
    let first = connector.call_for_window(
        "task_start",
        json!({"goal": "inspect parser branch one", "mode": "read_only"}),
        Some(&owner),
        ConnectorTransport::Mcp,
        Some(&window),
    );
    let second = connector.call_for_window(
        "task_start",
        json!({"goal": "inspect parser branch two", "mode": "read_only"}),
        Some(&owner),
        ConnectorTransport::Mcp,
        Some(&window),
    );
    let (first, second) = tokio::join!(first, second);
    assert!(first.ok && second.ok);
    assert_eq!(first.body["task_id"], second.body["task_id"]);
    let continuations = [
        first.body["data"]["continuation"].as_str().unwrap(),
        second.body["data"]["continuation"].as_str().unwrap(),
    ];
    assert!(continuations.contains(&"created"));
    assert!(continuations.contains(&"continued"));
    let tasks = connector
        .db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap();
    assert_eq!(tasks.len(), 1);
}

#[tokio::test]
async fn different_windows_on_one_project_keep_independent_contexts() {
    let (_temp, connector) = connector();
    let owner = auth("u1");
    let first_window = ClientWindow::for_test("window-one");
    let second_window = ClientWindow::for_test("window-two");
    let first = connector
        .call_for_window(
            "task_start",
            json!({"goal": "window one work", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&first_window),
        )
        .await;
    let second = connector
        .call_for_window(
            "task_start",
            json!({"goal": "window two work", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&second_window),
        )
        .await;
    assert!(first.ok && second.ok);
    assert_ne!(first.body["task_id"], second.body["task_id"]);

    let first_again = connector
        .call_for_window(
            "task_start",
            json!({"goal": "window one follow-up", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&first_window),
        )
        .await;
    assert_eq!(first_again.body["task_id"], first.body["task_id"]);
    assert_ne!(first_again.body["task_id"], second.body["task_id"]);
}

#[tokio::test]
async fn explicit_recovery_moves_context_without_sharing_it_between_windows() {
    let (_temp, connector) = connector();
    let owner = auth("u1");
    let original_window = ClientWindow::for_test("recovery-original");
    let recovered_window = ClientWindow::for_test("recovery-new");
    let started = connector
        .call_for_window(
            "task_start",
            json!({"goal": "inspect before reconnect", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&original_window),
        )
        .await;
    assert!(started.ok, "{}", started.body);
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    let resumed = connector
        .call_for_window(
            "task_resume",
            json!({"task_id": task_id}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&recovered_window),
        )
        .await;
    assert!(resumed.ok, "{}", resumed.body);
    assert_eq!(resumed.body["data"]["continuity"]["window_rebound"], true);

    let recovered_follow_up = connector
        .call_for_window(
            "task_start",
            json!({"goal": "continue in the rebuilt connection", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&recovered_window),
        )
        .await;
    assert!(recovered_follow_up.ok, "{}", recovered_follow_up.body);
    assert_eq!(recovered_follow_up.body["task_id"], task_id);

    let original_follow_up = connector
        .call_for_window(
            "task_start",
            json!({"goal": "independent work in the old window", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&original_window),
        )
        .await;
    assert!(original_follow_up.ok, "{}", original_follow_up.body);
    assert_ne!(original_follow_up.body["task_id"], task_id);
    let tasks = connector
        .db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap();
    assert_eq!(tasks.len(), 2);
}

#[tokio::test]
async fn one_window_switches_projects_and_restores_each_task() {
    let temp = tempfile::tempdir().unwrap();
    let project_a = temp.path().join("same-name-a");
    let project_b = temp.path().join("same-name-b");
    init_repo(&project_a);
    init_repo(&project_b);
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let runtime = Arc::new(ToolRuntime::new_for_tests());
    let make = |project_id: &str, workspace_id: &str, name: &str, root: &Path, state: &str| {
        ConnectorRuntime::new(
            runtime.clone(),
            db.clone(),
            ConnectorContext {
                project_id: project_id.to_string(),
                project_name: name.to_string(),
                workspace_id: workspace_id.to_string(),
                executor_project: format!("agent:hosted:{name}"),
                executor_root: root.to_string_lossy().to_string(),
                runs_root: temp
                    .path()
                    .join(format!("{state}/runs"))
                    .to_string_lossy()
                    .to_string(),
                results_root: temp
                    .path()
                    .join(format!("{state}/results"))
                    .to_string_lossy()
                    .to_string(),
                projects_dir: temp
                    .path()
                    .join(format!("{state}/projects.d"))
                    .to_string_lossy()
                    .to_string(),
                profile: "personal".to_string(),
                project_grant_id: PROJECT_GRANT_ID.to_string(),
            },
            credential(),
        )
        .unwrap()
    };
    let connector_a = make(
        "wc_proj_aaaaaaaaaaaaaaaa",
        "wc_ws_aaaaaaaaaaaaaaaa",
        "same-name",
        &project_a,
        "state-a",
    );
    let connector_b = make(
        "wc_proj_aaaaaaaaaaaaaaaa",
        "wc_ws_bbbbbbbbbbbbbbbb",
        "same-name",
        &project_b,
        "state-b",
    );
    let owner = auth("u1");
    let window = ClientWindow::for_test("switching-window");
    let a_first = connector_a
        .call_for_window(
            "task_start",
            json!({"goal": "inspect A", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    let failed_b = connector_b
        .call_for_window(
            "task_start",
            json!({"goal": "attempt B write", "mode": "normal"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(!failed_b.ok, "B has no registered writable executor");
    let a_after_failed_b = connector_a
        .call_for_window(
            "task_start",
            json!({"goal": "continue A after failed switch", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(a_after_failed_b.ok, "{}", a_after_failed_b.body);
    assert_eq!(a_after_failed_b.body["task_id"], a_first.body["task_id"]);
    assert_eq!(
        a_after_failed_b.body["data"]["project_switch"]["switched"], false,
        "a failed project start must not mutate successful navigation state"
    );
    let b = connector_b
        .call_for_window(
            "task_start",
            json!({"goal": "inspect B", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    let cross_root_resume = connector_b
        .call_for_window(
            "task_resume",
            json!({"task_id": a_first.body["task_id"]}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    let a_again = connector_a
        .call_for_window(
            "task_start",
            json!({"goal": "continue A", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(a_first.ok && b.ok && a_again.ok);
    assert!(!cross_root_resume.ok);
    assert_eq!(
        cross_root_resume.body["error"]["code"],
        "project_context_mismatch"
    );
    assert_ne!(a_first.body["task_id"], b.body["task_id"]);
    assert_eq!(a_again.body["task_id"], a_first.body["task_id"]);
    assert_eq!(a_again.body["data"]["continuation"], "continued");
    assert_eq!(
        a_again.body["data"]["project_switch"]["restored_previous_context"],
        true
    );
    assert_eq!(b.body["data"]["project_switch"]["switched"], true);
    assert_eq!(a_again.body["data"]["project_switch"]["switched"], true);
    assert_eq!(
        connector_a
            .db
            .connector_task(
                a_first.body["task_id"].as_str().unwrap(),
                &connector_a.context.project_id,
                PROJECT_SUBJECT_ID,
            )
            .unwrap()
            .task_status,
        "active"
    );
}

#[tokio::test]
async fn context_refresh_reports_only_changed_rules_and_worktree() {
    let (_temp, connector) = connector();
    let root = Path::new(&connector.context.executor_root);
    std::fs::create_dir_all(root.join("src/nested")).unwrap();
    let owner = auth("u1");
    let window = ClientWindow::for_test("refresh-window");
    let first = connector
        .call_for_window(
            "task_start",
            json!({
                "goal": "inspect nested code",
                "mode": "read_only",
                "target_path": "src/nested"
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(first.ok, "{}", first.body);
    std::fs::write(root.join("src/AGENTS.md"), "nested rules\n").unwrap();
    let second = connector
        .call_for_window(
            "task_start",
            json!({
                "goal": "continue under the new rule",
                "mode": "read_only",
                "target_path": "src/nested"
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(second.ok, "{}", second.body);
    assert_eq!(
        second.body["data"]["context"]["rules"]["refreshed"],
        json!(["src/AGENTS.md"])
    );
    assert!(second.body["data"]["context"]["refreshed"]
        .as_array()
        .unwrap()
        .contains(&json!("worktree")));
    assert!(second.body["data"]["context"]["manifests"]["refreshed"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        second.body["data"]["context"]["manifests"]["reused_count"],
        1
    );
    assert!(second.body["data"]["context"]["manifests"]
        .get("reused")
        .is_none());
}

#[tokio::test]
async fn inspect_to_write_keeps_task_and_rechecks_write_authority() {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    register_agent(&registry, "project", &project.to_string_lossy()).await;
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let connector = ConnectorRuntime::new(
        Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
            registry.clone(),
        )),
        db,
        ConnectorContext {
            project_id: "wc_proj_upgrade123456".to_string(),
            project_name: "upgrade".to_string(),
            workspace_id: "wc_ws_upgrade123456".to_string(),
            executor_project: "agent:hosted:project".to_string(),
            executor_root: project.to_string_lossy().to_string(),
            runs_root: temp.path().join("state/runs").to_string_lossy().to_string(),
            results_root: temp
                .path()
                .join("state/results")
                .to_string_lossy()
                .to_string(),
            projects_dir: temp
                .path()
                .join("state/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        credential(),
    )
    .unwrap();
    let owner = auth("u1");
    let window = ClientWindow::for_test("upgrade-window");
    let inspected = connector
        .call_for_window(
            "task_start",
            json!({"goal": "inspect first", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(inspected.ok, "{}", inspected.body);
    let task_id = inspected.body["task_id"].as_str().unwrap().to_string();

    let mut read_only_credential = owner.clone();
    read_only_credential
        .scopes
        .retain(|scope| scope != SCOPE_PROJECT_WRITE);
    let denied = connector
        .call_for_window(
            "task_start",
            json!({"goal": "now fix it", "mode": "normal"}),
            Some(&read_only_credential),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(!denied.ok);
    assert_eq!(denied.body["error"]["code"], "insufficient_scope");
    let still_read_only = connector
        .db
        .connector_task(&task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
        .unwrap();
    assert_eq!(still_read_only.mode, "read_only");
    assert!(!still_read_only.isolated);

    let responder_registry = registry.clone();
    let responder = tokio::spawn(async move {
        for _ in 0..1_000 {
            if let Some(request) = responder_registry
                .poll(ShellAgentPollRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                assert_eq!(request.kind, "register_project");
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                responder_registry
                    .complete(ShellAgentResultRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
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
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("workspace upgrade did not register the isolated project");
    });
    let upgraded = connector
        .call_for_window(
            "task_start",
            json!({"goal": "now fix it", "mode": "normal"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    responder.await.unwrap();
    assert!(upgraded.ok, "{}", upgraded.body);
    assert_eq!(upgraded.body["task_id"], task_id);
    assert_eq!(upgraded.body["data"]["continuation"], "continued");
    assert_eq!(
        upgraded.body["data"]["capability"]["previous_mode"],
        "read_only"
    );
    assert_eq!(upgraded.body["data"]["capability"]["mode"], "normal");
    assert_eq!(
        upgraded.body["data"]["capability"]["write_scope_verified"],
        true
    );
    assert_eq!(
        upgraded.body["data"]["capability"]["workspace_upgraded"],
        true
    );
    let task = connector
        .db
        .connector_task(&task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
        .unwrap();
    assert_eq!(task.mode, "normal");
    assert!(task.isolated);
    assert_ne!(task.execution_root, task.target_root);
    let events = connector
        .db
        .connector_task_events(
            &task_id,
            &connector.context.project_id,
            PROJECT_SUBJECT_ID,
            20,
        )
        .unwrap();
    assert!(events.iter().any(|event| {
        event.kind == "task_instruction"
            && event.payload["instruction"] == "now fix it"
            && event.payload["workspace_upgraded"] == true
    }));
}

#[tokio::test]
async fn restart_recovers_history_without_guessing_execution_state() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let db_path = temp.path().join("connector.db");
    let context = ConnectorContext {
        project_id: "wc_proj_restart123456".to_string(),
        project_name: "restart".to_string(),
        workspace_id: "wc_ws_restart123456".to_string(),
        executor_project: "agent:hosted:restart".to_string(),
        executor_root: project.to_string_lossy().to_string(),
        runs_root: temp.path().join("runs").to_string_lossy().to_string(),
        results_root: temp.path().join("results").to_string_lossy().to_string(),
        projects_dir: temp.path().join("projects.d").to_string_lossy().to_string(),
        profile: "personal".to_string(),
        project_grant_id: PROJECT_GRANT_ID.to_string(),
    };
    let owner = auth("u1");
    let window = ClientWindow::for_test("restart-window");
    let task_id = {
        let db = Arc::new(Database::open(&db_path).unwrap());
        let connector = ConnectorRuntime::new(
            Arc::new(ToolRuntime::new_for_tests()),
            db,
            context.clone(),
            credential(),
        )
        .unwrap();
        let started = connector
            .call_for_window(
                "task_start",
                json!({"goal": "persistent inspection", "mode": "read_only"}),
                Some(&owner),
                ConnectorTransport::Mcp,
                Some(&window),
            )
            .await;
        assert!(started.ok, "{}", started.body);
        started.body["task_id"].as_str().unwrap().to_string()
    };

    let db = Arc::new(Database::open(&db_path).unwrap());
    let connector = ConnectorRuntime::new(
        Arc::new(ToolRuntime::new_for_tests()),
        db,
        context,
        credential(),
    )
    .unwrap();
    let recovered = connector
        .call_for_window(
            "task_start",
            json!({"goal": "continue after reconnect", "mode": "read_only"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(!recovered.ok);
    assert_eq!(recovered.body["task_id"], task_id);
    assert_eq!(recovered.body["error"]["code"], "task_interrupted");
    assert_eq!(recovered.body["data"]["continuation"], "recovered");
    assert_eq!(recovered.body["data"]["instruction_appended"], true);
    assert_eq!(
        recovered.body["data"]["project_switch"]["restored_previous_context"],
        true
    );
    let tasks = connector
        .db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap();
    assert_eq!(tasks.len(), 1, "restart must not duplicate the context");
    let events = connector
        .db
        .connector_task_events(
            &task_id,
            &connector.context.project_id,
            PROJECT_SUBJECT_ID,
            20,
        )
        .unwrap();
    assert!(events.iter().any(|event| event.kind == "task_started"));
    assert!(events.iter().any(|event| {
        event.kind == "task_instruction"
            && event.payload["instruction"] == "continue after reconnect"
            && event.payload["blocked_by"] == "task_interrupted"
    }));
}

#[tokio::test]
async fn writable_start_registers_and_releases_a_reusable_git_worktree() {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    register_agent(&registry, "project", &project.to_string_lossy()).await;
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let connector = ConnectorRuntime::new(
        Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
            registry.clone(),
        )),
        db,
        ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "project".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:project".to_string(),
            executor_root: project.to_string_lossy().to_string(),
            runs_root: temp.path().join("state/runs").to_string_lossy().to_string(),
            results_root: temp
                .path()
                .join("state/results")
                .to_string_lossy()
                .to_string(),
            projects_dir: temp
                .path()
                .join("state/agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        credential(),
    )
    .unwrap();
    let agent_registry = registry.clone();
    let responder = tokio::spawn(async move {
        for _ in 0..1_000 {
            if let Some(request) = agent_registry
                .poll(ShellAgentPollRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                assert_eq!(request.kind, "register_project");
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                assert_eq!(payload["id"], "wc-slot-write-01");
                assert!(Path::new(payload["path"].as_str().unwrap()).is_dir());
                let stdout = json!({
                    "agent_project_id": payload["id"],
                    "client_id": "hosted",
                    "name": payload["name"],
                    "path": payload["path"],
                    "allow_patch": true
                })
                .to_string();
                agent_registry
                    .complete(ShellAgentResultRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(stdout),
                        stderr: Some(String::new()),
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("connector did not register its isolated execution project");
    });
    let owner = auth("u1");
    let outcome = connector
        .call(
            "task_start",
            json!({ "goal": "make an isolated change", "mode": "normal" }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    responder.await.unwrap();
    assert!(outcome.ok, "{}", outcome.body);
    assert_eq!(outcome.body["data"]["brief"]["workspace"]["isolated"], true);
    assert_eq!(outcome.body["data"]["brief"]["languages"], json!(["rust"]));
    assert_eq!(outcome.body["data"]["brief"]["git"]["dirty"], false);
    let task_id = outcome.body["task_id"].as_str().unwrap();
    let task = connector
        .db
        .connector_task(task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
        .unwrap();
    assert_ne!(task.execution_root, task.target_root);
    assert!(Path::new(&task.execution_root).is_dir());
    assert!(task.baseline_commit.is_some());
    std::fs::write(
        Path::new(&task.execution_root).join("README.md"),
        "isolated result\n",
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("README.md")).unwrap(),
        "fixture\n"
    );
    let check_registry = registry.clone();
    let check_responder = tokio::spawn(async move {
        for _ in 0..1_000 {
            if let Some(request) = check_registry
                .poll(ShellAgentPollRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                assert_eq!(request.kind, "start_validation_job");
                check_registry
                    .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        update_seq: None,
                        job_id: request.job_id.unwrap(),
                        request_id: Some(request.request_id),
                        status: "completed".to_string(),
                        stdout_chunk: None,
                        stderr_chunk: None,
                        stdout_tail: None,
                        stderr_tail: None,
                        log_snapshot: None,
                        exit_code: Some(0),
                        duration_ms: Some(1),
                        error: None,
                        command_execution_state: None,
                        validation_progress: Some(
                            crate::shell_protocol::ShellJobValidationProgress {
                                completed: 1,
                                current_step: None,
                                failed_step: None,
                            },
                        ),
                        finished: true,
                    })
                    .await
                    .unwrap();
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("connector did not dispatch structured validation");
    });
    let checked = connector
        .call(
            "checks_run",
            json!({
                "task_id": task_id,
                "operation_id": "worktree-check-1",
                "checks": ["check"]
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    check_responder.await.unwrap();
    assert!(checked.ok, "{}", checked.body);
    let finished = connector
        .call(
            "task_finish",
            json!({ "task_id": task_id, "summary": "updated the readme" }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(finished.ok, "{}", finished.body);
    assert_eq!(finished.body["data"]["status"], "ready_for_review");
    assert_eq!(finished.body["data"]["workspace"]["released"], true);
    assert_eq!(
        finished.body["data"]["result"]["changed_paths"],
        json!(["README.md"])
    );
    assert_eq!(
        std::fs::read_to_string(project.join("README.md")).unwrap(),
        "fixture\n"
    );
    let review = connector
        .call(
            "task_review",
            json!({ "task_id": task_id, "include_diff": true }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(review.ok, "{}", review.body);
    assert_eq!(
        review.body["data"]["changes"]["source"],
        "stable_task_result"
    );
    assert!(review.body["data"]["changes"]["diff_preview"]["text"]
        .as_str()
        .unwrap()
        .contains("isolated result"));
    let result_id = connector
        .db
        .connector_task_result(task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
        .unwrap()
        .unwrap()
        .result_id;
    connector
        .host_decide(
            task_id,
            Some(&result_id),
            LocalResultDecision::Reject,
            Some("the diff touches files outside the agreed scope"),
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    assert!(Path::new(&task.execution_root).exists());
    let resources = workspace::WorkspaceManager::resource_status(
        Path::new(&connector.context.runs_root),
        temp.path().join("cargo-target").as_path(),
    );
    assert_eq!(resources.slot_state, "idle");

    // The rejection reason travels the same guidance channel as task
    // guide: claimed exactly once, on the model's next capability
    // response for this task.
    let review = connector
        .call(
            "task_review",
            json!({ "task_id": task_id }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(review.ok, "{}", review.body);
    let message = review.body["data"]["guidance"][0]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.contains("rejected") && message.contains("outside the agreed scope"),
        "guidance must carry the rejection reason: {}",
        review.body
    );
    let review_again = connector
        .call(
            "task_review",
            json!({ "task_id": task_id }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(review_again.ok, "{}", review_again.body);
    assert!(
        review_again.body["data"]["guidance"].is_null(),
        "guidance is claimed exactly once: {}",
        review_again.body
    );
}

#[tokio::test]
async fn failed_task_binding_releases_prepared_workspace_for_retry() {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    register_agent(&registry, "project", &project.to_string_lossy()).await;
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let connector = ConnectorRuntime::new(
        Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
            registry.clone(),
        )),
        db.clone(),
        ConnectorContext {
            project_id: "wc_proj_compensation1".to_string(),
            project_name: "compensation".to_string(),
            workspace_id: "wc_ws_compensation1".to_string(),
            executor_project: "agent:hosted:project".to_string(),
            executor_root: project.to_string_lossy().to_string(),
            runs_root: temp.path().join("state/runs").to_string_lossy().to_string(),
            results_root: temp
                .path()
                .join("state/results")
                .to_string_lossy()
                .to_string(),
            projects_dir: temp
                .path()
                .join("state/agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        credential(),
    )
    .unwrap();
    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER wc_test_fail_connector_binding
             BEFORE INSERT ON wc_window_project_contexts
             BEGIN
               SELECT RAISE(ABORT, 'injected connector binding failure');
             END;",
        )
        .unwrap();

    let responder_registry = registry.clone();
    let responder = tokio::spawn(async move {
        let mut registrations = 0;
        for _ in 0..2_000 {
            if let Some(request) = responder_registry
                .poll(ShellAgentPollRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                assert_eq!(request.kind, "register_project");
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                responder_registry
                    .complete(ShellAgentResultRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
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
                registrations += 1;
                if registrations == 2 {
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("connector did not issue both workspace registrations");
    });

    let owner = auth("u1");
    let window = ClientWindow::for_test("compensation-window");
    let failed = connector
        .call_for_window(
            "task_start",
            json!({"goal": "first attempt", "mode": "normal"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    assert!(!failed.ok);
    assert!(db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap()
        .is_empty());
    let lease = temp.path().join("state/runs/.wc-slot-write-01.lease.json");
    let registration = temp
        .path()
        .join("state/agent/projects.d/wc-slot-write-01.toml");
    assert!(!lease.exists(), "failed start retained the workspace lease");
    assert!(
        !registration.exists(),
        "failed start retained the managed project registration"
    );

    db.conn_for_tests()
        .execute_batch("DROP TRIGGER wc_test_fail_connector_binding;")
        .unwrap();
    let retried = connector
        .call_for_window(
            "task_start",
            json!({"goal": "retry after rollback", "mode": "normal"}),
            Some(&owner),
            ConnectorTransport::Mcp,
            Some(&window),
        )
        .await;
    responder.await.unwrap();
    assert!(retried.ok, "{}", retried.body);
    let tasks = db
        .connector_tasks_for_subject(&connector.context.project_id, PROJECT_SUBJECT_ID, 10)
        .unwrap();
    assert_eq!(tasks.len(), 1, "retry created duplicate active contexts");
}

#[tokio::test]
async fn canonical_read_reaches_bound_executor_and_advances_event_cursor() {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
    let registry = Arc::new(ShellClientRegistry::default());
    register_agent(&registry, "demo", &project.to_string_lossy()).await;
    let tool_runtime = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
        registry.clone(),
    ));
    let connector = ConnectorRuntime::new(
        tool_runtime,
        db,
        ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "demo".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:demo".to_string(),
            executor_root: project.to_string_lossy().to_string(),
            runs_root: temp.path().join("runs").to_string_lossy().to_string(),
            results_root: temp.path().join("results").to_string_lossy().to_string(),
            projects_dir: temp
                .path()
                .join("agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        credential(),
    )
    .unwrap();
    let owner = auth("u1");
    let started = connector
        .call(
            "task_start",
            json!({ "goal": "read the entry point", "mode": "read_only" }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    let task_id = started.body["task_id"].as_str().unwrap().to_string();

    let agent_registry = registry.clone();
    let responder = tokio::spawn(async move {
        for _ in 0..100 {
            if let Some(request) = agent_registry
                .poll(ShellAgentPollRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                assert_eq!(request.kind, "file_read");
                assert_eq!(request.path.as_deref(), Some("src/lib.rs"));
                agent_registry
                    .complete(ShellAgentResultRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(
                            json!({
                                "format": "webcodex.file_read_range.v1",
                                "path": "src/lib.rs",
                                "content": "fn entry() {}",
                                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                                "start_line": 1,
                                "limit": 50,
                                "total_lines": 1,
                                "truncated": false
                            })
                            .to_string(),
                        ),
                        stderr: Some(String::new()),
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("connector did not dispatch the read to its bound executor");
    });
    let outcome = connector
        .call(
            "files_read",
            json!({
                "task_id": task_id,
                "files": [{ "path": "src/lib.rs", "limit": 50 }]
            }),
            Some(&owner),
            ConnectorTransport::Mcp,
        )
        .await;
    if !outcome.ok {
        responder.abort();
        panic!(
            "files_read failed before executor dispatch: {}",
            outcome.body
        );
    }
    responder.await.unwrap();
    assert_eq!(outcome.body["event_cursor"], 2);
    assert!(outcome.body["data"]["files"][0]["text"]
        .as_str()
        .unwrap()
        .contains("fn entry"));
    assert!(!serde_json::to_string(&outcome.body)
        .unwrap()
        .contains("agent:hosted:demo"));
}

#[tokio::test]
async fn code_navigate_status_holds_the_task_lifecycle_lock() {
    let (_temp, connector, registry) = connector_with_lsp(true).await;
    let owner = auth("u1");
    let task_id = start_read_only_task(&connector, "inspect Python language services").await;

    let navigating_connector = connector.clone();
    let navigating_owner = owner.clone();
    let navigating_task = task_id.clone();
    let navigation = tokio::spawn(async move {
        navigating_connector
            .call(
                "code_navigate",
                json!({ "task_id": navigating_task, "operation": "status" }),
                Some(&navigating_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    let request = next_lsp_request(&registry).await;
    let payload = request.lsp.as_ref().expect("typed LSP payload");
    assert_eq!(payload.project_id, "demo");
    assert_eq!(payload.request, AgentLspRequest::Status);
    assert!(
        connector.task_lock(&task_id).try_lock().is_err(),
        "code_navigate must hold the task lifecycle lock while the LSP read is active"
    );

    let finishing_connector = connector.clone();
    let finishing_owner = owner.clone();
    let finishing_task = task_id.clone();
    let finish = tokio::spawn(async move {
        finishing_connector
            .call(
                "task_finish",
                json!({
                    "task_id": finishing_task,
                    "summary": "completed semantic inspection"
                }),
                Some(&finishing_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    tokio::task::yield_now().await;
    assert!(
        !finish.is_finished(),
        "task_finish must wait for the in-flight read-only navigation"
    );

    complete_lsp_request(
        &registry,
        &request,
        LspStatusResult {
            project: "private-agent-project".to_string(),
            detected_languages: vec!["python".to_string()],
            servers: vec![LspServerStatusEntry {
                language: "python".to_string(),
                server: "pyright".to_string(),
                available: true,
                running: true,
                status: LspAvailabilityStatus::Running,
                source: None,
                position_encoding: Some("utf-16".to_string()),
            }],
            warnings: Vec::new(),
        },
    )
    .await;
    let navigation = navigation.await.unwrap();
    let finish = finish.await.unwrap();
    assert!(navigation.ok, "{}", navigation.body);
    assert_eq!(navigation.body["event_cursor"], 2);
    assert_eq!(navigation.body["data"]["project"], "wc_proj_1234567890");
    assert_eq!(
        navigation.body["data"]["detected_languages"],
        json!(["python"])
    );
    assert_eq!(navigation.body["data"]["servers"][0]["server"], "pyright");
    assert!(finish.ok, "{}", finish.body);
}

#[tokio::test]
async fn code_navigate_preserves_non_rust_navigation_and_bounded_ledger_metadata() {
    let (_temp, connector, registry) = connector_with_lsp(true).await;
    let owner = auth("u1");
    let task_id = start_read_only_task(&connector, "navigate Python and TypeScript code").await;
    let private_root = connector.context.executor_root.clone();

    let symbols_connector = connector.clone();
    let symbols_owner = owner.clone();
    let symbols_task = task_id.clone();
    let symbols = tokio::spawn(async move {
        symbols_connector
            .call(
                "code_navigate",
                json!({
                    "task_id": symbols_task,
                    "operation": "document_symbols",
                    "path": "src/main.py",
                    "limit": 25
                }),
                Some(&symbols_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    let request = next_lsp_request(&registry).await;
    let payload = request.lsp.as_ref().expect("typed LSP payload");
    assert_eq!(payload.project_id, "demo");
    assert_eq!(
        payload.request,
        AgentLspRequest::DocumentSymbols {
            path: "src/main.py".to_string(),
            limit: 25,
        }
    );
    complete_lsp_request(
        &registry,
        &request,
        json!({
            "project": "agent:hosted:demo",
            "path": "src/main.py",
            "language": "python",
            "symbols": [{
                "name": "main",
                "kind": "function",
                "kind_code": 12,
                "range": {
                    "start": { "line": 1, "column": 1 },
                    "end": { "line": 2, "column": 1 }
                },
                "selection_range": {
                    "start": { "line": 1, "column": 5 },
                    "end": { "line": 1, "column": 9 }
                },
                "children": []
            }],
            "total_count": 1,
            "returned_count": 1,
            "truncated": false,
            "external_results_omitted": 0,
            "invalid_results_omitted": 0,
            "client_id": "hosted",
            "request_id": "req-private",
            "executor": "private-executor",
            "root": private_root.clone(),
            "raw_stderr": "stderr-private"
        }),
    )
    .await;
    let symbols = symbols.await.unwrap();
    assert!(symbols.ok, "{}", symbols.body);
    assert_eq!(symbols.body["data"]["language"], "python");
    assert_eq!(symbols.body["data"]["path"], "src/main.py");
    assert_eq!(symbols.body["data"]["symbols"][0]["name"], "main");
    let serialized = serde_json::to_string(&symbols.body).unwrap();
    for private in [
        "agent:hosted:demo",
        "hosted",
        private_root.as_str(),
        "req-private",
        "private-executor",
        "stderr-private",
    ] {
        assert!(
            !serialized.contains(private),
            "private executor metadata leaked: {serialized}"
        );
    }

    let workspace_connector = connector.clone();
    let workspace_owner = owner.clone();
    let workspace_task = task_id.clone();
    let workspace = tokio::spawn(async move {
        workspace_connector
            .call(
                "code_navigate",
                json!({
                    "task_id": workspace_task,
                    "operation": "workspace_symbols",
                    "query": "Widget"
                }),
                Some(&workspace_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    let request = next_lsp_request(&registry).await;
    let payload = request.lsp.as_ref().expect("typed LSP payload");
    assert_eq!(payload.project_id, "demo");
    assert_eq!(
        payload.request,
        AgentLspRequest::WorkspaceSymbols {
            query: "Widget".to_string(),
            limit: 50,
        }
    );
    complete_lsp_request(
        &registry,
        &request,
        WorkspaceSymbolsResult {
            project: "private-agent-project".to_string(),
            query: "Widget".to_string(),
            symbols: vec![PublicWorkspaceSymbol {
                name: "Widget".to_string(),
                kind: "class".to_string(),
                kind_code: 5,
                container_name: None,
                path: "src/widget.ts".to_string(),
                range: None,
            }],
            total_results: 1,
            returned_count: 1,
            truncated: false,
            external_results_omitted: 0,
            invalid_results_omitted: 0,
        },
    )
    .await;
    let workspace = workspace.await.unwrap();
    assert!(workspace.ok, "{}", workspace.body);
    assert_eq!(workspace.body["data"]["query"], "Widget");
    assert_eq!(
        workspace.body["data"]["symbols"][0]["path"],
        "src/widget.ts"
    );

    let navigation_events = connector
        .db
        .connector_task_events(
            &task_id,
            &connector.context.project_id,
            PROJECT_SUBJECT_ID,
            20,
        )
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "code_navigate")
        .collect::<Vec<_>>();
    assert_eq!(navigation_events.len(), 2);
    assert_eq!(
        navigation_events[0].payload,
        json!({ "ok": true, "operation": "document_symbols" })
    );
    assert_eq!(
        navigation_events[1].payload,
        json!({ "ok": true, "operation": "workspace_symbols" })
    );
    let ledger = serde_json::to_string(&navigation_events).unwrap();
    assert!(!ledger.contains("src/main.py"));
    assert!(!ledger.contains("Widget"));
}

#[tokio::test]
async fn code_navigate_fails_closed_without_runner_capability() {
    let (_temp, connector, registry) = connector_with_lsp(false).await;
    let task_id = start_read_only_task(&connector, "inspect semantic status").await;
    let outcome = connector
        .call(
            "code_navigate",
            json!({ "task_id": task_id, "operation": "status" }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.body["error"]["code"], "capability_failed");
    assert!(outcome.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("agent_capability_unavailable"));
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn code_navigate_requires_project_read_and_rejects_foreign_or_inactive_tasks() {
    let (_temp, connector, registry) = connector_with_lsp(true).await;
    let task_id = start_read_only_task(&connector, "inspect task ownership").await;

    let mut runtime_only = auth("u1");
    runtime_only.scopes = vec![SCOPE_RUNTIME_READ.to_string()];
    let denied = connector
        .call(
            "code_navigate",
            json!({ "task_id": task_id, "operation": "status" }),
            Some(&runtime_only),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(denied.http_status, 403);
    assert_eq!(denied.required_scope, Some(SCOPE_PROJECT_READ));
    assert_eq!(denied.body["error"]["code"], "insufficient_scope");

    let mut foreign = auth("u1");
    foreign.user_id = Some("foreign-user".to_string());
    let foreign_outcome = connector
        .call(
            "code_navigate",
            json!({ "task_id": task_id, "operation": "status" }),
            Some(&foreign),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(foreign_outcome.http_status, 404);
    assert_eq!(foreign_outcome.body["error"]["code"], "task_not_found");

    let finished = connector
        .call(
            "task_finish",
            json!({ "task_id": task_id, "summary": "inspection complete" }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(finished.ok, "{}", finished.body);
    let inactive = connector
        .call(
            "code_navigate",
            json!({ "task_id": task_id, "operation": "status" }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(inactive.http_status, 409);
    assert_eq!(inactive.body["error"]["code"], "task_not_active");
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn code_navigate_rejects_irrelevant_null_fields_before_dispatch() {
    let (_temp, connector, registry) = connector_with_lsp(true).await;
    let task_id = start_read_only_task(&connector, "validate strict navigation input").await;
    for arguments in [
        json!({ "task_id": task_id, "operation": "status", "path": null }),
        json!({
            "task_id": task_id,
            "operation": "hover",
            "path": "src/main.rs",
            "line": 1,
            "column": 1,
            "limit": null
        }),
    ] {
        let outcome = connector
            .call(
                "code_navigate",
                arguments,
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await;
        assert_eq!(outcome.http_status, 400);
        assert_eq!(outcome.body["error"]["code"], "invalid_arguments");
    }
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn code_impact_uses_bound_executor_holds_lifecycle_lock_and_bounds_ledger() {
    let (_temp, connector, registry) = connector_with_lsp_capabilities(true, true).await;
    let owner = auth("u1");
    let task_id = start_read_only_task(&connector, "inspect TypeScript change impact").await;
    let impact_connector = connector.clone();
    let impact_owner = owner.clone();
    let impact_task = task_id.clone();
    let impact = tokio::spawn(async move {
        impact_connector
            .call(
                "code_impact",
                json!({
                    "task_id": impact_task,
                    "path": "src/app.ts",
                    "line": 1,
                    "column": 4,
                    "direction": "both",
                    "depth": 2,
                    "limit": 25
                }),
                Some(&impact_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    let request = next_lsp_request(&registry).await;
    let payload = request.lsp.as_ref().expect("typed LSP payload");
    assert_eq!(payload.project_id, "demo");
    assert_eq!(
        payload.request,
        AgentLspRequest::CallHierarchy {
            path: "src/app.ts".to_string(),
            line: 1,
            column: 4,
            direction: CallHierarchyDirection::Both,
            depth: 2,
            limit: 25,
        }
    );
    assert!(
        connector.task_lock(&task_id).try_lock().is_err(),
        "code_impact must hold the lifecycle lock during semantic dispatch"
    );

    let finish_connector = connector.clone();
    let finish_owner = owner.clone();
    let finish_task = task_id.clone();
    let finish = tokio::spawn(async move {
        finish_connector
            .call(
                "task_finish",
                json!({"task_id": finish_task, "summary": "impact inspection complete"}),
                Some(&finish_owner),
                ConnectorTransport::Mcp,
            )
            .await
    });
    tokio::task::yield_now().await;
    assert!(!finish.is_finished());

    let mut raw_result =
        serde_json::to_value(connector_call_hierarchy_result("src/app.ts", 1, 4)).unwrap();
    raw_result["client_id"] = json!("hosted-private");
    raw_result["execution_executor_ref"] = json!("agent:hosted:demo");
    raw_result["roots"][0]["data"] = json!({"opaque": "file:///private/root"});
    raw_result["roots"][0]["uri"] = json!("file:///private/root");
    complete_lsp_request(&registry, &request, raw_result).await;

    let impact = impact.await.unwrap();
    let finish = finish.await.unwrap();
    assert!(impact.ok, "{}", impact.body);
    assert!(finish.ok, "{}", finish.body);
    assert_eq!(impact.body["data"]["project"], "wc_proj_1234567890");
    assert_eq!(impact.body["data"]["language"], "typescript");
    let serialized = serde_json::to_string(&impact.body).unwrap();
    for private in [
        "agent:hosted:demo",
        "hosted-private",
        "file:///private",
        "execution_executor_ref",
        "\"opaque\"",
        "\"uri\"",
    ] {
        assert!(
            !serialized.contains(private),
            "{private} leaked: {serialized}"
        );
    }

    let events = connector
        .db
        .connector_task_events(
            &task_id,
            &connector.context.project_id,
            PROJECT_SUBJECT_ID,
            20,
        )
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "code_impact")
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].payload,
        json!({"ok": true, "direction": "both", "depth": 2})
    );
    let ledger = serde_json::to_string(&events).unwrap();
    assert!(!ledger.contains("src/app.ts"));
    assert!(!ledger.contains("roots"));
    assert!(!ledger.contains("edges"));
}

#[tokio::test]
async fn code_impact_lifecycle_lock_blocks_task_cancel_until_read_completes() {
    let (_temp, connector, registry) = connector_with_lsp_capabilities(true, true).await;
    let task_id = start_read_only_task(&connector, "inspect impact before cancellation").await;
    let impact_connector = connector.clone();
    let impact_task_id = task_id.clone();
    let impact = tokio::spawn(async move {
        impact_connector
            .call(
                "code_impact",
                json!({
                    "task_id": impact_task_id,
                    "path": "src/main.rs",
                    "line": 1,
                    "column": 1
                }),
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await
    });
    let request = next_lsp_request(&registry).await;

    let cancel_connector = connector.clone();
    let cancel_task_id = task_id.clone();
    let cancel = tokio::spawn(async move {
        cancel_connector
            .call(
                "task_cancel",
                json!({"task_id": cancel_task_id}),
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await
    });
    tokio::task::yield_now().await;
    assert!(
        !cancel.is_finished(),
        "task_cancel must wait for the semantic read lifecycle lock"
    );

    let mut result = connector_call_hierarchy_result("src/main.rs", 1, 1);
    result.depth = 1;
    complete_lsp_request(&registry, &request, result).await;
    let impact = impact.await.unwrap();
    let cancel = cancel.await.unwrap();
    assert!(impact.ok, "{}", impact.body);
    assert!(cancel.ok, "{}", cancel.body);
}

#[tokio::test]
async fn code_impact_is_available_in_normal_inspect_and_read_only_tasks() {
    for mode in ["normal", "inspect", "read_only"] {
        let (_temp, connector, registry) = connector_with_lsp_capabilities(true, true).await;
        let registration = if mode == "normal" {
            let registration_registry = registry.clone();
            Some(tokio::spawn(async move {
                for _ in 0..1_000 {
                    if let Some(request) = registration_registry
                        .poll(ShellAgentPollRequest {
                            client_id: "hosted".to_string(),
                            agent_instance_id: "instance".to_string(),
                            projects: None,
                        })
                        .await
                        .unwrap()
                    {
                        assert_eq!(request.kind, "register_project");
                        let payload: Value =
                            serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                        let stdout = json!({
                            "agent_project_id": payload["id"],
                            "client_id": "hosted",
                            "name": payload["name"],
                            "path": payload["path"],
                            "allow_patch": true
                        })
                        .to_string();
                        registration_registry
                            .complete(ShellAgentResultRequest {
                                client_id: "hosted".to_string(),
                                agent_instance_id: "instance".to_string(),
                                request_id: request.request_id,
                                exit_code: Some(0),
                                stdout: Some(stdout),
                                stderr: Some(String::new()),
                                duration_ms: Some(1),
                                error: None,
                            })
                            .await
                            .unwrap();
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
                panic!("normal task did not register its isolated execution project");
            }))
        } else {
            None
        };
        let task_id = start_task_mode(&connector, "inspect call impact", mode).await;
        if let Some(registration) = registration {
            registration.await.unwrap();
        }
        let calling_connector = connector.clone();
        let calling_task = task_id.clone();
        let call = tokio::spawn(async move {
            calling_connector
                .call(
                    "code_impact",
                    json!({
                        "task_id": calling_task,
                        "path": "src/main.rs",
                        "line": 1,
                        "column": 1
                    }),
                    Some(&auth("u1")),
                    ConnectorTransport::Mcp,
                )
                .await
        });
        let request = next_lsp_request(&registry).await;
        assert_eq!(
            request.lsp.as_ref().unwrap().request,
            AgentLspRequest::CallHierarchy {
                path: "src/main.rs".to_string(),
                line: 1,
                column: 1,
                direction: CallHierarchyDirection::Both,
                depth: 1,
                limit: 50,
            }
        );
        let mut result = connector_call_hierarchy_result("src/main.rs", 1, 1);
        result.depth = 1;
        complete_lsp_request(&registry, &request, result).await;
        let outcome = call.await.unwrap();
        assert!(outcome.ok, "{mode}: {}", outcome.body);
    }
}

#[tokio::test]
async fn code_impact_requires_distinct_capability_scope_and_active_owned_task() {
    let (_temp, connector, registry) = connector_with_lsp_capabilities(true, false).await;
    let task_id = start_read_only_task(&connector, "inspect impact policy").await;
    let unavailable = connector
        .call(
            "code_impact",
            json!({
                "task_id": task_id,
                "path": "src/main.rs",
                "line": 1,
                "column": 1
            }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(!unavailable.ok);
    assert!(unavailable.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("lsp_call_hierarchy"));
    assert!(
        !unavailable.body.to_string().contains("hosted"),
        "agent client identity leaked: {}",
        unavailable.body
    );
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());

    let mut runtime_only = auth("u1");
    runtime_only.scopes = vec![SCOPE_RUNTIME_READ.to_string()];
    let denied = connector
        .call(
            "code_impact",
            json!({
                "task_id": task_id,
                "path": "src/main.rs",
                "line": 1,
                "column": 1
            }),
            Some(&runtime_only),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(denied.http_status, 403);
    assert_eq!(denied.required_scope, Some(SCOPE_PROJECT_READ));

    let mut foreign = auth("u1");
    foreign.user_id = Some("foreign-user".to_string());
    let foreign = connector
        .call(
            "code_impact",
            json!({
                "task_id": task_id,
                "path": "src/main.rs",
                "line": 1,
                "column": 1
            }),
            Some(&foreign),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(foreign.http_status, 404);
    assert_eq!(foreign.body["error"]["code"], "task_not_found");

    let finished = connector
        .call(
            "task_finish",
            json!({"task_id": task_id, "summary": "inspection complete"}),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert!(finished.ok, "{}", finished.body);
    let inactive = connector
        .call(
            "code_impact",
            json!({
                "task_id": task_id,
                "path": "src/main.rs",
                "line": 1,
                "column": 1
            }),
            Some(&auth("u1")),
            ConnectorTransport::Mcp,
        )
        .await;
    assert_eq!(inactive.http_status, 409);
    assert_eq!(inactive.body["error"]["code"], "task_not_active");
}

#[tokio::test]
async fn code_impact_rejects_null_malformed_and_schema_bypassing_inputs() {
    let (_temp, connector, registry) = connector_with_lsp_capabilities(true, true).await;
    let task_id = start_read_only_task(&connector, "validate impact input").await;
    for arguments in [
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "direction": null}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "depth": null}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "limit": null}),
        json!({"task_id": task_id, "path": "/private/main.rs", "line": 1, "column": 1}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 0, "column": 1}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "depth": 3}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "limit": 101}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "project": "agent:other:private"}),
        json!({"task_id": task_id, "path": "src/main.rs", "line": 1, "column": 1, "uri": "file:///private/main.rs"}),
    ] {
        let outcome = connector
            .call(
                "code_impact",
                arguments,
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await;
        assert_eq!(outcome.http_status, 400, "{}", outcome.body);
        assert_eq!(outcome.body["error"]["code"], "invalid_arguments");
    }
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "hosted".to_string(),
            agent_instance_id: "instance".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[test]
fn code_navigation_operation_parameters_are_strict() {
    let task_id = "wc_task_00000000000000000000000000000000";
    let invalid = [
        json!({ "task_id": task_id, "operation": "status", "path": "src/main.rs" }),
        json!({ "task_id": task_id, "operation": "document_symbols" }),
        json!({ "task_id": task_id, "operation": "document_symbols", "path": "/tmp/main.rs" }),
        json!({ "task_id": task_id, "operation": "workspace_symbols", "query": "  " }),
        json!({ "task_id": task_id, "operation": "workspace_symbols", "query": "Widget", "path": "src" }),
        json!({ "task_id": task_id, "operation": "definition", "path": "src/main.rs", "line": 1, "column": 1, "include_declaration": true }),
        json!({ "task_id": task_id, "operation": "definition", "path": "src/main.rs", "line": 1, "column": 1, "limit": 101 }),
        json!({ "task_id": task_id, "operation": "references", "path": "src/main.rs", "line": 1, "column": 1, "query": "main" }),
        json!({ "task_id": task_id, "operation": "diagnostics", "path": "src/main.rs", "line": 1 }),
        json!({ "task_id": task_id, "operation": "hover", "path": "src/main.rs", "line": 1, "column": 1, "limit": 1 }),
    ];
    for arguments in invalid {
        let input: CodeNavigateInput = serde_json::from_value(arguments.clone()).unwrap();
        assert!(
            code_navigation_tool_call(&input).is_err(),
            "arguments should be rejected: {arguments}"
        );
    }
    assert!(serde_json::from_value::<CodeNavigateInput>(json!({
        "task_id": task_id,
        "operation": "status",
        "project": "agent:hosted:demo"
    }))
    .is_err());

    let references: CodeNavigateInput = serde_json::from_value(json!({
        "task_id": task_id,
        "operation": "references",
        "path": "src/main.rs",
        "line": 2,
        "column": 3
    }))
    .unwrap();
    let (tool, arguments) = code_navigation_tool_call(&references).unwrap();
    assert_eq!(tool, "find_references");
    assert_eq!(arguments["include_declaration"], true);
}

#[test]
fn search_cursor_is_query_bound_and_pages_a_sorted_window() {
    let mut input = FilesSearchInput {
        task_id: "wc_task_0123456789abcdef0123456789abcdef".to_string(),
        pattern: "needle".to_string(),
        path: Some("src".to_string()),
        limit: Some(2),
        context_before: Some(0),
        context_after: Some(0),
        include_globs: Vec::new(),
        exclude_globs: Vec::new(),
        result_mode: Some(SearchResultMode::Matches),
        cursor: None,
    };
    let signature = search_cursor_signature(&input, 2);
    let first = paginate_search_output(
        json!({
            "matches": [
                {"path": "src/a.rs", "line": 1},
                {"path": "src/b.rs", "line": 2}
            ],
            "truncated": true
        }),
        SearchResultMode::Matches,
        0,
        2,
        &signature,
    );
    let cursor = first["page"]["next_cursor"].as_str().unwrap();
    assert_eq!(parse_search_cursor(cursor, &signature), Ok(2));
    assert_eq!(first["page"]["returned"], 2);
    let second = paginate_search_output(
        json!({
            "matches": [
                {"path": "src/a.rs", "line": 1},
                {"path": "src/b.rs", "line": 2},
                {"path": "src/c.rs", "line": 3},
                {"path": "src/d.rs", "line": 4}
            ],
            "truncated": false
        }),
        SearchResultMode::Matches,
        2,
        2,
        &signature,
    );
    assert_eq!(second["matches"][0]["path"], "src/c.rs");
    assert_eq!(second["matches"][1]["path"], "src/d.rs");
    assert!(second["page"]["next_cursor"].is_null());

    input.pattern = "different".to_string();
    let other_signature = search_cursor_signature(&input, 2);
    assert_eq!(parse_search_cursor(cursor, &other_signature), Err(()));
}
