use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    PersistentShellResult, ShellAgentPersistentShellResultRequest, ShellAgentProjectSummary,
    ShellClientCapabilities,
};

const CLIENT: &str = "persistent-agent";
const PROJECT_ID: &str = "demo";

async fn setup(
    root: &std::path::Path,
    persistent_shell: bool,
    mode: SessionMode,
    context: Option<sessions::SessionExecutionContext>,
) -> (ToolRuntime, String, sessions::SessionSummary) {
    let runtime = test_runtime();
    register_agent_with_projects(
        &runtime,
        CLIENT,
        None,
        ShellClientCapabilities {
            shell: true,
            persistent_shell,
            ..Default::default()
        },
        vec![ShellAgentProjectSummary {
            id: PROJECT_ID.to_string(),
            name: Some(PROJECT_ID.to_string()),
            path: root.to_string_lossy().to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let project = crate::tool_runtime::agent_project_runtime_id(CLIENT, PROJECT_ID);
    let mut options = sessions::SessionCreateOptions::new(
        Some(project.clone()),
        Some("persistent shell".to_string()),
        mode,
        sessions::SessionGuards::default(),
    );
    if let Some(context) = context {
        options = options.with_execution_context(context);
    }
    let session = runtime
        .sessions
        .start_session_with_options(options)
        .unwrap();
    (runtime, project, session)
}

async fn next_persistent_request(
    runtime: &ToolRuntime,
) -> crate::shell_protocol::ShellAgentShellRequest {
    let request = wait_for_patch_agent_request(runtime, CLIENT).await;
    assert_eq!(request.kind, "persistent_shell");
    request
}

async fn complete(
    runtime: &ToolRuntime,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    shell_state: &str,
    execution_state: &str,
    stdout: &str,
    exit_code: Option<i32>,
    error_code: Option<&str>,
) {
    let operation = request.persistent_shell.as_ref().unwrap();
    let cwd = operation.cwd.as_deref();
    let initial_cwd = if operation.action == "open" {
        cwd
    } else {
        None
    };
    complete_with_cwds(
        runtime,
        request,
        shell_state,
        execution_state,
        stdout,
        exit_code,
        error_code,
        cwd,
        initial_cwd,
    )
    .await;
}

async fn complete_with_cwds(
    runtime: &ToolRuntime,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    shell_state: &str,
    execution_state: &str,
    stdout: &str,
    exit_code: Option<i32>,
    error_code: Option<&str>,
    cwd: Option<&str>,
    initial_cwd: Option<&str>,
) {
    let operation = request.persistent_shell.as_ref().unwrap();
    runtime
        .runner_registry
        .complete_persistent_shell(ShellAgentPersistentShellResultRequest {
            client_id: CLIENT.to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id.clone(),
            result: PersistentShellResult {
                shell_id: operation.shell_id.clone(),
                workflow_session_id: operation.workflow_session_id.clone(),
                runtime_project_id: operation.runtime_project_id.clone(),
                shell_state: shell_state.to_string(),
                execution_state: execution_state.to_string(),
                command_started: operation.action == "exec",
                command_completed: operation.action == "exec" && exit_code.is_some(),
                exit_code,
                stdout: stdout.to_string(),
                stderr: String::new(),
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: 3,
                cwd: cwd.map(str::to_string),
                initial_cwd: initial_cwd.map(str::to_string),
                shell: operation.shell.clone().or_else(|| Some("bash".to_string())),
                profile: None,
                created_at: Some(1),
                last_activity_at: Some(2),
                busy: false,
                already_closed: execution_state == "already_closed",
                close_reason: (shell_state == "closed").then(|| "explicit_close".to_string()),
                error_code: error_code.map(str::to_string),
                error: error_code.map(|code| format!("{code}: operation failed")),
            },
        })
        .await
        .unwrap();
}

fn dispatch(runtime: ToolRuntime, call: ToolCall) -> tokio::task::JoinHandle<ToolResult> {
    tokio::spawn(async move {
        let auth = auth_context(None, true);
        runtime.dispatch_with_auth(call, Some(&auth)).await
    })
}

#[tokio::test]
async fn server_lifecycle_uses_distinct_ids_and_never_routes_through_run_shell() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;

    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: Some(ExecutionShell::Bash),
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    let open = open_request.persistent_shell.as_ref().unwrap();
    assert_eq!(open.action, "open");
    assert!(open.shell_id.starts_with("wc_shell_"));
    assert_eq!(open.workflow_session_id, session.session_id);
    assert_eq!(open.runtime_project_id, project);
    assert_eq!(
        open_request.cwd.as_deref(),
        Some(temp.path().to_string_lossy().as_ref())
    );
    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    let opened = open_task.await.unwrap();
    assert!(opened.success, "{opened:?}");
    let first_shell_id = opened.output["shell_id"].as_str().unwrap().to_string();

    let exec_task = dispatch(
        runtime.clone(),
        ToolCall::SessionShellExec {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: first_shell_id.clone(),
            command: "printf ready".to_string(),
            timeout_secs: Some(5),
            purpose: Some(ExecutionPurpose::Diagnostic),
        },
    );
    let exec_request = next_persistent_request(&runtime).await;
    assert_eq!(
        exec_request.persistent_shell.as_ref().unwrap().action,
        "exec"
    );
    complete(
        &runtime,
        &exec_request,
        "running",
        "completed",
        "ready",
        Some(0),
        None,
    )
    .await;
    let exec = exec_task.await.unwrap();
    assert!(exec.success, "{exec:?}");
    assert_eq!(exec.output["stdout"], "ready");
    let ledger = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let exec_started = ledger
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started" && event.tool_name == "session_shell_exec")
        .unwrap();
    let exec_finished = ledger
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "session_shell_exec")
        .unwrap();
    let evidence = exec_finished.persistent_shell.as_ref().unwrap();
    assert_eq!(evidence.action, "exec");
    assert_eq!(evidence.shell_id.as_deref(), Some(first_shell_id.as_str()));
    assert_eq!(evidence.shell_state.as_deref(), Some("running"));
    assert_eq!(evidence.execution_state.as_deref(), Some("completed"));
    assert_eq!(evidence.command_started, Some(true));
    assert_eq!(evidence.command_completed, Some(true));
    let audit_json = serde_json::to_string(&(exec_started, exec_finished)).unwrap();
    assert!(!audit_json.contains("printf ready"));
    assert!(!audit_json.contains("\"stdout\""));
    assert!(!audit_json.contains("\"stderr\""));

    let duplicate_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let duplicate_status = next_persistent_request(&runtime).await;
    assert_eq!(
        duplicate_status.persistent_shell.as_ref().unwrap().action,
        "status"
    );
    complete(
        &runtime,
        &duplicate_status,
        "running",
        "idle",
        "",
        None,
        None,
    )
    .await;
    let duplicate = duplicate_task.await.unwrap();
    assert!(!duplicate.success);
    assert!(duplicate
        .error
        .as_deref()
        .unwrap()
        .contains("persistent_shell_already_open"));

    let close_task = dispatch(
        runtime.clone(),
        ToolCall::CloseSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: first_shell_id.clone(),
        },
    );
    let close_request = next_persistent_request(&runtime).await;
    complete(&runtime, &close_request, "closed", "closed", "", None, None).await;
    assert!(close_task.await.unwrap().success);

    let idempotent = {
        let auth = auth_context(None, true);
        runtime
            .dispatch_with_auth(
                ToolCall::CloseSessionShell {
                    project: project.clone(),
                    session_id: session.session_id.clone(),
                    shell_id: first_shell_id.clone(),
                },
                Some(&auth),
            )
            .await
    };
    assert!(idempotent.success);
    assert_eq!(idempotent.output["already_closed"], true);

    let reopen_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let reopen_request = next_persistent_request(&runtime).await;
    complete(
        &runtime,
        &reopen_request,
        "running",
        "opened",
        "",
        None,
        None,
    )
    .await;
    let reopened = reopen_task.await.unwrap();
    assert!(reopened.success);
    assert_ne!(reopened.output["shell_id"], first_shell_id);

    let stale = {
        let auth = auth_context(None, true);
        runtime
            .dispatch_with_auth(
                ToolCall::SessionShellExec {
                    project,
                    session_id: session.session_id,
                    shell_id: first_shell_id,
                    command: "printf stale".to_string(),
                    timeout_secs: None,
                    purpose: None,
                },
                Some(&auth),
            )
            .await
    };
    assert!(!stale.success);
    assert!(stale
        .error
        .as_deref()
        .unwrap()
        .contains("persistent_shell_stale"));
}

#[tokio::test]
async fn open_reconciles_a_stale_runner_shell_before_reserving_a_new_id() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;
    let first_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let first_request = next_persistent_request(&runtime).await;
    complete(
        &runtime,
        &first_request,
        "running",
        "opened",
        "",
        None,
        None,
    )
    .await;
    let first = first_task.await.unwrap();
    assert!(first.success, "{first:?}");
    let old_shell_id = first.output["shell_id"].as_str().unwrap().to_string();

    let reopen_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id,
            cwd: None,
            shell: None,
        },
    );
    let stale_status = next_persistent_request(&runtime).await;
    assert_eq!(
        stale_status.persistent_shell.as_ref().unwrap().action,
        "status"
    );
    assert_eq!(
        stale_status.persistent_shell.as_ref().unwrap().shell_id,
        old_shell_id
    );
    complete(
        &runtime,
        &stale_status,
        "lost",
        "rejected",
        "",
        None,
        Some("persistent_shell_not_found"),
    )
    .await;
    let reopened_request = next_persistent_request(&runtime).await;
    assert_eq!(
        reopened_request.persistent_shell.as_ref().unwrap().action,
        "open"
    );
    assert_ne!(
        reopened_request.persistent_shell.as_ref().unwrap().shell_id,
        old_shell_id
    );
    complete(
        &runtime,
        &reopened_request,
        "running",
        "opened",
        "",
        None,
        None,
    )
    .await;
    let reopened = reopen_task.await.unwrap();
    assert!(reopened.success, "{reopened:?}");
}

#[tokio::test]
async fn session_close_transitions_first_and_cleans_the_active_runner_shell() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;
    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project,
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    assert!(open_task.await.unwrap().success);

    let close_session_task = dispatch(
        runtime.clone(),
        ToolCall::CloseSession {
            session_id: session.session_id.clone(),
        },
    );
    let cleanup = next_persistent_request(&runtime).await;
    assert_eq!(cleanup.persistent_shell.as_ref().unwrap().action, "close");
    assert_eq!(
        runtime
            .sessions
            .lifecycle_state(&session.session_id)
            .unwrap(),
        sessions::SessionLifecycle::Closed
    );
    complete(&runtime, &cleanup, "closed", "closed", "", None, None).await;
    let closed = close_session_task.await.unwrap();
    assert!(closed.success, "{closed:?}");
    assert_eq!(closed.output["persistent_shells_closed"], 1);
    assert_eq!(closed.output["lifecycle"], "closed");
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let close_event = summary
        .events
        .iter()
        .find(|event| event.kind == "session_closed")
        .unwrap();
    let evidence = close_event.persistent_shell.as_ref().unwrap();
    assert_eq!(evidence.action, "close");
    assert_eq!(
        evidence.shell_id.as_deref(),
        Some(cleanup.persistent_shell.as_ref().unwrap().shell_id.as_str())
    );
    assert_eq!(evidence.shell_state.as_deref(), Some("closed"));
    assert_eq!(evidence.execution_state.as_deref(), Some("closed"));
    assert_eq!(evidence.error_code, None);
}

#[tokio::test]
async fn session_close_racing_open_releases_the_late_runner_shell() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;
    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project,
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let open_request = next_persistent_request(&runtime).await;

    let close_session_task = dispatch(
        runtime.clone(),
        ToolCall::CloseSession {
            session_id: session.session_id.clone(),
        },
    );
    let early_cleanup = next_persistent_request(&runtime).await;
    assert_eq!(
        early_cleanup.persistent_shell.as_ref().unwrap().action,
        "close"
    );
    complete(
        &runtime,
        &early_cleanup,
        "lost",
        "rejected",
        "",
        None,
        Some("persistent_shell_not_found"),
    )
    .await;
    assert!(close_session_task.await.unwrap().success);

    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    let late_cleanup = next_persistent_request(&runtime).await;
    assert_eq!(
        late_cleanup.persistent_shell.as_ref().unwrap().action,
        "close"
    );
    complete(&runtime, &late_cleanup, "closed", "closed", "", None, None).await;
    let opened = open_task.await.unwrap();
    assert!(!opened.success);
    assert_eq!(
        opened.output["error_code"],
        "persistent_shell_session_inactive"
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let evidence = summary
        .events
        .iter()
        .find(|event| event.kind == "session_closed")
        .and_then(|event| event.persistent_shell.as_ref())
        .unwrap();
    assert_eq!(evidence.shell_state.as_deref(), Some("closed"));
    assert_eq!(evidence.execution_state.as_deref(), Some("closed"));
    assert_eq!(evidence.error_code, None);
}

#[tokio::test]
async fn runner_disconnect_converges_server_status_to_lost() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;
    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    let opened = open_task.await.unwrap();
    let shell_id = opened.output["shell_id"].as_str().unwrap().to_string();

    runtime
        .runner_registry
        .reconcile_disconnect(CLIENT, "inst")
        .await;
    let auth = auth_context(None, true);
    let unavailable = runtime
        .dispatch_with_auth(
            ToolCall::SessionShellStatus {
                project: project.clone(),
                session_id: session.session_id.clone(),
                shell_id: shell_id.clone(),
            },
            Some(&auth),
        )
        .await;
    assert!(!unavailable.success);

    let status = runtime
        .dispatch_with_auth(
            ToolCall::SessionShellStatus {
                project,
                session_id: session.session_id,
                shell_id,
            },
            Some(&auth),
        )
        .await;
    assert!(status.success, "{status:?}");
    assert_eq!(status.output["shell_state"], "lost");
}

#[tokio::test]
async fn cancelled_exec_releases_server_busy_without_mixing_runner_requests() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup(temp.path(), true, SessionMode::Normal, None).await;
    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: None,
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    let opened = open_task.await.unwrap();
    let shell_id = opened.output["shell_id"].as_str().unwrap().to_string();

    let cancelled = dispatch(
        runtime.clone(),
        ToolCall::SessionShellExec {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: shell_id.clone(),
            command: "printf first".to_string(),
            timeout_secs: Some(5),
            purpose: None,
        },
    );
    let first_request = next_persistent_request(&runtime).await;
    cancelled.abort();
    let _ = cancelled.await;
    complete(
        &runtime,
        &first_request,
        "running",
        "completed",
        "first",
        Some(0),
        None,
    )
    .await;

    let second = dispatch(
        runtime.clone(),
        ToolCall::SessionShellExec {
            project,
            session_id: session.session_id,
            shell_id,
            command: "printf second".to_string(),
            timeout_secs: Some(5),
            purpose: None,
        },
    );
    let second_request = next_persistent_request(&runtime).await;
    assert_eq!(
        second_request
            .persistent_shell
            .as_ref()
            .and_then(|operation| operation.command.as_deref()),
        Some("printf second")
    );
    complete(
        &runtime,
        &second_request,
        "running",
        "completed",
        "second",
        Some(0),
        None,
    )
    .await;
    let result = second.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["stdout"], "second");
}

#[tokio::test]
async fn capability_modes_and_ssh_resource_fail_closed_without_enqueue() {
    let temp = tempfile::tempdir().unwrap();
    let (legacy_runtime, legacy_project, legacy_session) =
        setup(temp.path(), false, SessionMode::Normal, None).await;
    let legacy = {
        let auth = auth_context(None, true);
        legacy_runtime
            .dispatch_with_auth(
                ToolCall::OpenSessionShell {
                    project: legacy_project,
                    session_id: legacy_session.session_id,
                    cwd: None,
                    shell: None,
                },
                Some(&auth),
            )
            .await
    };
    assert!(!legacy.success);
    assert_eq!(legacy.output["error_kind"], "agent_capability_unavailable");
    assert_eq!(legacy.output["recovery_kind"], "none");
    assert!(legacy.output.get("recovery_tool").is_none());
    assert!(legacy
        .error
        .as_deref()
        .unwrap()
        .contains("agent_capability_unavailable"));

    let (ssh_runtime, ssh_project, ssh_session) =
        setup_ssh_with_capabilities(temp.path(), "prod", true, false).await;
    let ssh = {
        let auth = auth_context(None, true);
        ssh_runtime
            .dispatch_with_auth(
                ToolCall::OpenSessionShell {
                    project: ssh_project,
                    session_id: ssh_session.session_id,
                    cwd: None,
                    shell: None,
                },
                Some(&auth),
            )
            .await
    };
    assert!(!ssh.success);
    assert_eq!(ssh.output["error_kind"], "agent_capability_unavailable");
    assert_eq!(ssh.output["recovery_kind"], "none");
    assert!(ssh.output.get("recovery_tool").is_none());
    // A legacy Runner may support one-shot SSH plus persistent shells while
    // predating the additive SSH-persistent capability. It still fails closed
    // before enqueue rather than falling back to a local persistent shell.
    assert!(ssh
        .error
        .as_deref()
        .unwrap()
        .contains("ssh_persistent_shell"));
    assert!(probe_patch_agent_request(&ssh_runtime, CLIENT)
        .await
        .is_none());

    for mode in [SessionMode::ReadOnly] {
        let (runtime, project, session) = setup(temp.path(), true, mode, None).await;
        let result = {
            let auth = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::OpenSessionShell {
                        project,
                        session_id: session.session_id,
                        cwd: None,
                        shell: None,
                    },
                    Some(&auth),
                )
                .await
        };
        assert!(!result.success, "mode {mode:?} unexpectedly opened a shell");
        assert_eq!(result.output["error_kind"], "session_guard_denied");
        assert!(result.output.get("guard").is_some());
    }
}

/// An SSH persistent shell enqueues to the Runner with a `job_context` carrying
/// the bound resource, and later exec/status/close route by the saved record
/// (not the current Session context). A context change after open must not
/// redirect an already-open shell.
async fn setup_ssh_with_capabilities(
    root: &std::path::Path,
    resource: &str,
    ssh_shell: bool,
    ssh_persistent_shell: bool,
) -> (ToolRuntime, String, sessions::SessionSummary) {
    let runtime = test_runtime();
    register_agent_with_projects(
        &runtime,
        CLIENT,
        None,
        ShellClientCapabilities {
            shell: true,
            async_jobs: true,
            persistent_shell: true,
            ssh_shell,
            ssh_persistent_shell,
            ..Default::default()
        },
        vec![ShellAgentProjectSummary {
            id: PROJECT_ID.to_string(),
            name: Some(PROJECT_ID.to_string()),
            path: root.to_string_lossy().to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let project = crate::tool_runtime::agent_project_runtime_id(CLIENT, PROJECT_ID);
    let context = sessions::SessionExecutionContext {
        default_cwd: None,
        default_shell: None,
        resource: Some(resource.to_string()),
    };
    let options = sessions::SessionCreateOptions::new(
        Some(project.clone()),
        Some("ssh persistent shell".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    )
    .with_execution_context(context);
    let session = runtime
        .sessions
        .start_session_with_options(options)
        .unwrap();
    (runtime, project, session)
}

async fn setup_ssh(
    root: &std::path::Path,
    resource: &str,
) -> (ToolRuntime, String, sessions::SessionSummary) {
    // Exact Windows Stage 2 capability shape: persistent SSH is available even
    // though the existing one-shot/background SSH capability remains absent.
    setup_ssh_with_capabilities(root, resource, false, true).await
}

#[tokio::test]
async fn ssh_persistent_shell_enqueues_with_bound_resource_and_routes_by_record() {
    let temp = tempfile::tempdir().unwrap();
    let (runtime, project, session) = setup_ssh(temp.path(), "prod").await;
    let auth = auth_context(None, true);

    let one_shot = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: project.clone(),
                command: "printf one-shot".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: Some(30),
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!one_shot.success, "{one_shot:?}");
    assert_eq!(
        one_shot.output["error_kind"],
        "agent_capability_unavailable"
    );
    assert!(one_shot.error.as_deref().unwrap().contains("ssh_shell"));

    let background = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "printf background".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: Some(30),
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!background.success, "{background:?}");
    assert_eq!(
        background.output["error_kind"],
        "agent_capability_unavailable"
    );
    assert!(background.error.as_deref().unwrap().contains("ssh_shell"));
    assert!(
        probe_patch_agent_request(&runtime, CLIENT).await.is_none(),
        "one-shot/background SSH work must not enqueue without ssh_shell"
    );

    // Open carries the bound SSH resource on job_context and must still enqueue
    // with persistent_shell=true + ssh_persistent_shell=true + ssh_shell=false.
    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: Some(ExecutionShell::Bash),
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    let open = open_request.persistent_shell.as_ref().unwrap();
    assert_eq!(open.action, "open");
    assert_eq!(
        open_request
            .job_context
            .as_ref()
            .and_then(|ctx| ctx.ssh_resource.as_deref()),
        Some("prod")
    );
    complete(&runtime, &open_request, "running", "opened", "", None, None).await;
    let opened = open_task.await.unwrap();
    assert!(opened.success, "{opened:?}");
    assert_eq!(opened.output["executor"], "ssh");
    assert_eq!(opened.output["resource"], "prod");
    let shell_id = opened.output["shell_id"].as_str().unwrap().to_string();

    // Exec routes by the saved record: the exec request carries the bound
    // resource from the record, not from the current Session context.
    let exec_task = dispatch(
        runtime.clone(),
        ToolCall::SessionShellExec {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: shell_id.clone(),
            command: "printf remote".to_string(),
            timeout_secs: Some(5),
            purpose: None,
        },
    );
    let exec_request = next_persistent_request(&runtime).await;
    assert_eq!(
        exec_request
            .job_context
            .as_ref()
            .and_then(|ctx| ctx.ssh_resource.as_deref()),
        Some("prod"),
        "exec must route by the saved binding, not the current Session context"
    );
    complete(
        &runtime,
        &exec_request,
        "running",
        "completed",
        "remote",
        Some(0),
        None,
    )
    .await;
    let exec = exec_task.await.unwrap();
    assert!(exec.success, "{exec:?}");
    assert_eq!(exec.output["stdout"], "remote");

    let status_task = dispatch(
        runtime.clone(),
        ToolCall::SessionShellStatus {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: shell_id.clone(),
        },
    );
    let status_request = next_persistent_request(&runtime).await;
    assert_eq!(
        status_request.persistent_shell.as_ref().unwrap().action,
        "status"
    );
    assert_eq!(
        status_request
            .job_context
            .as_ref()
            .and_then(|ctx| ctx.ssh_resource.as_deref()),
        Some("prod"),
        "status must route by the saved SSH resource binding"
    );
    complete(&runtime, &status_request, "running", "idle", "", None, None).await;
    assert!(status_task.await.unwrap().success);

    // Close also routes by the saved record.
    let close_task = dispatch(
        runtime.clone(),
        ToolCall::CloseSessionShell {
            project,
            session_id: session.session_id,
            shell_id,
        },
    );
    let close_request = next_persistent_request(&runtime).await;
    assert_eq!(
        close_request.persistent_shell.as_ref().unwrap().action,
        "close"
    );
    assert_eq!(
        close_request
            .job_context
            .as_ref()
            .and_then(|ctx| ctx.ssh_resource.as_deref()),
        Some("prod")
    );
    complete(&runtime, &close_request, "closed", "closed", "", None, None).await;
    let closed = close_task.await.unwrap();
    assert!(closed.success, "{closed:?}");
}

#[tokio::test]
async fn server_record_does_not_replace_initial_cwd_from_later_status() {
    let temp = tempfile::tempdir().unwrap();
    let login = temp.path().join("login");
    let current = temp.path().join("current");
    std::fs::create_dir(&login).unwrap();
    std::fs::create_dir(&current).unwrap();
    let login = login.to_string_lossy().into_owned();
    let current = current.to_string_lossy().into_owned();
    let (runtime, project, session) = setup_ssh(temp.path(), "prod").await;

    let open_task = dispatch(
        runtime.clone(),
        ToolCall::OpenSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            cwd: None,
            shell: Some(ExecutionShell::Bash),
        },
    );
    let open_request = next_persistent_request(&runtime).await;
    complete_with_cwds(
        &runtime,
        &open_request,
        "running",
        "opened",
        "",
        None,
        None,
        Some(&login),
        Some(&login),
    )
    .await;
    let opened = open_task.await.unwrap();
    assert!(opened.success, "{opened:?}");
    assert_eq!(opened.output["cwd"], "login");
    assert_eq!(opened.output["initial_cwd"], "login");
    let shell_id = opened.output["shell_id"].as_str().unwrap().to_string();

    let status_task = dispatch(
        runtime.clone(),
        ToolCall::SessionShellStatus {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: shell_id.clone(),
        },
    );
    let status_request = next_persistent_request(&runtime).await;
    complete_with_cwds(
        &runtime,
        &status_request,
        "running",
        "idle",
        "",
        None,
        None,
        Some(&current),
        Some(&current),
    )
    .await;
    assert!(status_task.await.unwrap().success);

    let close_task = dispatch(
        runtime.clone(),
        ToolCall::CloseSessionShell {
            project: project.clone(),
            session_id: session.session_id.clone(),
            shell_id: shell_id.clone(),
        },
    );
    let close_request = next_persistent_request(&runtime).await;
    complete_with_cwds(
        &runtime,
        &close_request,
        "closed",
        "closed",
        "",
        None,
        None,
        Some(&current),
        None,
    )
    .await;
    assert!(close_task.await.unwrap().success);

    let terminal_status = {
        let auth = auth_context(None, true);
        runtime
            .dispatch_with_auth(
                ToolCall::SessionShellStatus {
                    project,
                    session_id: session.session_id,
                    shell_id,
                },
                Some(&auth),
            )
            .await
    };
    assert!(terminal_status.success, "{terminal_status:?}");
    assert_eq!(terminal_status.output["cwd"], "current");
    assert_eq!(
        terminal_status.output["initial_cwd"], "login",
        "a later status result must not overwrite the authoritative initial cwd"
    );
}
