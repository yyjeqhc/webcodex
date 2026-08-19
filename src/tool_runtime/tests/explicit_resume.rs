use super::reconnect::dispatch_start_coding_task_in_window;
use super::support::*;
use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::shell_protocol::ShellClientCapabilities;
use crate::tool_runtime::sessions::{SessionEvent, SessionGuards, SessionTransport};
use crate::tool_runtime::{
    registered_tool_specs, SessionMode, StartupDetail, ToolCall, ToolResult, ToolRuntime,
};
use serde_json::{json, Value};
use std::path::Path;

fn coding_call(
    project: &str,
    instruction: &str,
    mode: SessionMode,
    resume_session_id: Option<&str>,
    bind_current: bool,
    new_session: bool,
) -> ToolCall {
    ToolCall::StartCodingTask {
        project: project.to_string(),
        client_id: None,
        path: None,
        temporary_project_name: None,
        title: Some(instruction.to_string()),
        mode,
        deny_write_tools: false,
        deny_shell_tools: false,
        detail: StartupDetail::Full,
        resume_session_id: resume_session_id.map(str::to_string),
        bind_current,
        new_session,
        execution_context: None,
    }
}

fn create_workflow_session(
    runtime: &ToolRuntime,
    project: &str,
    title: &str,
    mode: SessionMode,
) -> String {
    runtime
        .sessions
        .start_session_with_guards(
            Some(project.to_string()),
            Some(title.to_string()),
            mode,
            SessionGuards::default(),
        )
        .session_id
}

async fn dispatch_start_coding_task_without_window(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        async move {
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_sandbox(
                    call,
                    auth.as_ref(),
                    SessionTransport::Mcp,
                    true,
                    false,
                    Default::default(),
                    None,
                    None,
                )
                .await
        }
    });

    for _ in 0..5_000 {
        if task.is_finished() {
            break;
        }
        if let Some(request) = runtime
            .shell_clients
            .poll(crate::shell_protocol::ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            let (exit_code, stdout, stderr) = run_agent_shell_request_locally(&request);
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                exit_code,
                &stdout,
                &stderr,
            )
            .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    }
    assert!(task.is_finished(), "start_coding_task did not finish");
    task.await.unwrap()
}

async fn current_session_in_window(
    runtime: &ToolRuntime,
    project: &str,
    auth: Option<&AuthContext>,
    window_id: &str,
) -> ToolResult {
    let window = ClientWindow::for_test(window_id);
    runtime
        .dispatch_with_auth_transport_options_and_metadata_with_sandbox(
            ToolCall::CurrentSession {
                project: project.to_string(),
            },
            auth,
            SessionTransport::Mcp,
            true,
            false,
            Default::default(),
            None,
            Some(&window),
        )
        .await
}

fn session_value(runtime: &ToolRuntime, session_id: &str) -> Value {
    serde_json::to_value(runtime.sessions.summary(session_id, Some(200)).unwrap()).unwrap()
}

fn instruction_events(runtime: &ToolRuntime, session_id: &str) -> Vec<SessionEvent> {
    runtime
        .sessions
        .summary(session_id, Some(200))
        .unwrap()
        .events
        .into_iter()
        .filter(|event| event.kind == "task_instruction")
        .collect()
}

fn persisted_session_record(path: &Path, session_id: &str) -> Value {
    let ledger: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    ledger["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["session_id"] == session_id)
        .unwrap_or_else(|| panic!("missing persisted session {session_id}"))
        .clone()
}

#[tokio::test]
async fn explicit_resume_without_window_continues_unbound_and_preserves_root_title() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "explicit-no-window", "demo", root.path()).await;
    let session_id = create_workflow_session(
        &runtime,
        &project,
        "original root title",
        SessionMode::Inspect,
    );
    let auth = auth_context(None, true);

    let resumed = dispatch_start_coding_task_without_window(
        &runtime,
        "explicit-no-window",
        coding_call(
            &project,
            "continue without a window",
            SessionMode::Normal,
            Some(&session_id),
            true,
            false,
        ),
        Some(&auth),
    )
    .await;

    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session"]["session_id"], session_id);
    assert_eq!(
        resumed.output["session"]["continuation"],
        "resumed_explicitly"
    );
    assert_eq!(resumed.output["session"]["reused"], true);
    assert_eq!(resumed.output["session"]["resume_requested"], true);
    assert_eq!(resumed.output["session"]["current_binding"]["bound"], false);
    assert_eq!(
        resumed.output["session"]["current_binding"]["reason_code"],
        "stable_window_identity_unavailable"
    );
    assert_eq!(
        resumed.output["session"]["explicit_session_id_required_for_continuity"],
        true
    );
    assert!(resumed.output["startup_brief"]["warnings"]
        .as_array()
        .is_some_and(|warnings| warnings
            .iter()
            .any(|warning| warning == "current_binding_unavailable")));
    assert!(!serde_json::to_string(&resumed.output)
        .unwrap()
        .contains("current_session_unavailable"));
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 0);
    assert_eq!(runtime.sessions.status().durable_binding_count, 0);

    let summary = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("original root title"));
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    let events = instruction_events(&runtime, &session_id);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].instruction.as_deref(),
        Some("continue without a window")
    );
    assert_eq!(
        events[0].input_summary.as_ref().unwrap()["explicit_resume"],
        true
    );
    assert_eq!(
        events[0].input_summary.as_ref().unwrap()["current_binding_established"],
        false
    );
}

#[tokio::test]
async fn explicit_resume_binds_durably_and_default_start_reuses_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    init_git_repo(&root);
    let auth = auth_context(None, true);
    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "explicit-restart", "demo", &root).await;
    let session_id = create_workflow_session(
        &runtime1,
        &project,
        "durable original title",
        SessionMode::Normal,
    );

    let resumed = dispatch_start_coding_task_in_window(
        &runtime1,
        "explicit-restart",
        coding_call(
            &project,
            "bind this existing session",
            SessionMode::Normal,
            Some(&session_id),
            true,
            false,
        ),
        Some(&auth),
        "explicit-restart-window",
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session"]["session_id"], session_id);
    assert_eq!(resumed.output["session"]["current_binding"]["bound"], true);
    assert_eq!(runtime1.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime1.sessions.status().durable_binding_count, 1);
    let first_event = instruction_events(&runtime1, &session_id).pop().unwrap();
    assert_eq!(first_event.input_summary.unwrap()["explicit_resume"], true);

    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "explicit-restart", "demo", &root).await;
    assert_eq!(runtime2.sessions.process_local_binding_count_for_test(), 0);
    assert_eq!(runtime2.sessions.status().restored_binding_count, 1);
    let continued = dispatch_start_coding_task_in_window(
        &runtime2,
        "explicit-restart",
        coding_call(
            &project,
            "default continuation after restart",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-restart-window",
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert_eq!(continued.output["session"]["session_id"], session_id);
    assert_eq!(continued.output["session"]["continuation"], "continued");
    assert_eq!(continued.output["session"]["resume_requested"], false);
    let summary = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("durable original title"));
    assert_eq!(instruction_events(&runtime2, &session_id).len(), 2);
}

#[tokio::test]
async fn explicit_resume_replaces_old_current_binding_without_rewriting_old_session() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    init_git_repo(&root);
    let auth = auth_context(None, true);
    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path(&runtime1, "explicit-rebind", "demo", &root).await;

    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "explicit-rebind",
        coding_call(
            &project,
            "session A root",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-rebind-window",
    )
    .await;
    assert!(first.success);
    let session_a = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session_b =
        create_workflow_session(&runtime1, &project, "session B root", SessionMode::Normal);
    runtime1.sessions.flush_persistence();
    let session_a_before = persisted_session_record(&ledger, &session_a);

    let resumed = dispatch_start_coding_task_in_window(
        &runtime1,
        "explicit-rebind",
        coding_call(
            &project,
            "continue session B",
            SessionMode::Normal,
            Some(&session_b),
            true,
            false,
        ),
        Some(&auth),
        "explicit-rebind-window",
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session"]["session_id"], session_b);
    let current =
        current_session_in_window(&runtime1, &project, Some(&auth), "explicit-rebind-window").await;
    assert_eq!(current.output["session_id"], session_b);
    assert_eq!(
        runtime1
            .sessions
            .summary(&session_a, None)
            .unwrap()
            .lifecycle
            .as_str(),
        "active"
    );
    runtime1.sessions.flush_persistence();
    assert_eq!(
        persisted_session_record(&ledger, &session_a),
        session_a_before,
        "rebinding must not rewrite Session A's ledger record"
    );
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "explicit-rebind", "demo", &root).await;
    let continued = dispatch_start_coding_task_in_window(
        &runtime2,
        "explicit-rebind",
        coding_call(
            &project,
            "continue rebound session",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-rebind-window",
    )
    .await;
    assert!(continued.success);
    assert_eq!(continued.output["session"]["session_id"], session_b);
    assert!(runtime2.sessions.summary(&session_a, None).is_some());
}

#[tokio::test]
async fn explicit_resume_with_bind_current_false_preserves_existing_binding() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "explicit-no-bind", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-no-bind",
        coding_call(
            &project,
            "bound session A",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-no-bind-window",
    )
    .await;
    let session_a = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session_b =
        create_workflow_session(&runtime, &project, "unbound session B", SessionMode::Normal);

    let resumed = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-no-bind",
        coding_call(
            &project,
            "continue B without rebinding",
            SessionMode::Normal,
            Some(&session_b),
            false,
            false,
        ),
        Some(&auth),
        "explicit-no-bind-window",
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session"]["session_id"], session_b);
    assert_eq!(resumed.output["session"]["current_binding"]["bound"], false);
    assert_eq!(
        resumed.output["session"]["current_binding"]["reason_code"],
        "binding_disabled"
    );
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);
    let current =
        current_session_in_window(&runtime, &project, Some(&auth), "explicit-no-bind-window").await;
    assert!(current.success);
    assert_eq!(current.output["session_id"], session_a);
    let events = instruction_events(&runtime, &session_b);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].input_summary.as_ref().unwrap()["current_binding_established"],
        false
    );
}

#[tokio::test]
async fn explicit_resume_invalid_unknown_and_new_session_conflict_are_non_mutating() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "explicit-invalid", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-invalid",
        coding_call(
            &project,
            "stable session",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-invalid-window",
    )
    .await;
    let session_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let before = session_value(&runtime, &session_id);

    for malformed in [
        "   ",
        "not-a-workflow-session",
        " wc_sess_valid_but_surrounded ",
    ] {
        let result = dispatch_start_coding_task_in_window(
            &runtime,
            "explicit-invalid",
            coding_call(
                &project,
                "must not append",
                SessionMode::Normal,
                Some(malformed),
                true,
                false,
            ),
            Some(&auth),
            "explicit-invalid-window",
        )
        .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_resume_session_id");
    }

    let unknown = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-invalid",
        coding_call(
            &project,
            "must not create",
            SessionMode::Normal,
            Some("wc_sess_missing"),
            true,
            false,
        ),
        Some(&auth),
        "explicit-invalid-window",
    )
    .await;
    assert!(!unknown.success);
    assert_eq!(unknown.output["error_kind"], "unknown_session_id");

    let conflict = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-invalid",
        coding_call(
            &project,
            "must reject conflict",
            SessionMode::Normal,
            Some(&session_id),
            true,
            true,
        ),
        Some(&auth),
        "explicit-invalid-window",
    )
    .await;
    assert!(!conflict.success);
    assert_eq!(conflict.output["error_kind"], "invalid_arguments");
    assert_eq!(
        conflict.output["constraint"],
        "resume_session_id_mutually_exclusive_with_new_session"
    );

    assert_eq!(session_value(&runtime, &session_id), before);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);
    let current =
        current_session_in_window(&runtime, &project, Some(&auth), "explicit-invalid-window").await;
    assert_eq!(current.output["session_id"], session_id);
}

#[tokio::test]
async fn explicit_resume_rejects_closed_and_project_mismatch_without_binding_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root_a = dir.path().join("a");
    let root_b = dir.path().join("b");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    init_git_repo(&root_a);
    init_git_repo(&root_b);
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "explicit-boundaries",
        None,
        ShellClientCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            ..Default::default()
        },
        vec![
            registered_project("a", &root_a.to_string_lossy()),
            registered_project("b", &root_b.to_string_lossy()),
        ],
    )
    .await;
    let project_a = crate::tool_runtime::agent_project_runtime_id("explicit-boundaries", "a");
    let project_b = crate::tool_runtime::agent_project_runtime_id("explicit-boundaries", "b");
    let auth = auth_context(None, true);

    let active = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-boundaries",
        coding_call(
            &project_a,
            "active project A",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-boundaries-window",
    )
    .await;
    let active_id = active.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let closed_id = create_workflow_session(
        &runtime,
        &project_a,
        "closed project A",
        SessionMode::Normal,
    );
    runtime.sessions.close_session(&closed_id).unwrap();
    let active_before = session_value(&runtime, &active_id);
    let closed_before = session_value(&runtime, &closed_id);

    let closed = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-boundaries",
        coding_call(
            &project_a,
            "must not reopen",
            SessionMode::Normal,
            Some(&closed_id),
            true,
            false,
        ),
        Some(&auth),
        "explicit-boundaries-window",
    )
    .await;
    assert!(!closed.success);
    assert_eq!(closed.output["error_kind"], "session_closed");
    assert_eq!(closed.output["lifecycle"], "closed");

    let mismatch = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-boundaries",
        coding_call(
            &project_b,
            "must not cross projects",
            SessionMode::Normal,
            Some(&active_id),
            true,
            false,
        ),
        Some(&auth),
        "explicit-boundaries-window",
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    assert_eq!(mismatch.output["session_project"], project_a);
    assert_eq!(mismatch.output["request_project"], project_b);
    assert_eq!(session_value(&runtime, &active_id), active_before);
    assert_eq!(session_value(&runtime, &closed_id), closed_before);
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);
    let current_a = current_session_in_window(
        &runtime,
        &project_a,
        Some(&auth),
        "explicit-boundaries-window",
    )
    .await;
    assert_eq!(current_a.output["session_id"], active_id);
    let current_b = current_session_in_window(
        &runtime,
        &project_b,
        Some(&auth),
        "explicit-boundaries-window",
    )
    .await;
    assert_eq!(current_b.output["found"], false);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project_b)),
        0
    );
}

#[tokio::test]
async fn explicit_resume_mode_upgrade_rechecks_write_scope_atomically() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    commit_file(
        root.path(),
        "src/read_only.rs",
        "pub fn observed_before_resume() {}\n",
        "add read-only source",
    );
    let runtime = ToolRuntime::new_for_tests();
    let read_auth = oauth_bridge_auth_context(
        "explicit-upgrade-subject",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
        ],
    );
    let write_auth = oauth_bridge_auth_context(
        "explicit-upgrade-subject",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_PROJECT_WRITE,
        ],
    );
    let project = register_agent_project_at_path_with_auth(
        &runtime,
        "oauth-client",
        "demo",
        root.path(),
        &read_auth,
    )
    .await;
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_call(
            &project,
            "bound session A",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&read_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert!(first.success);
    let session_a = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session_b = create_workflow_session(
        &runtime,
        &project,
        "read-only session B",
        SessionMode::ReadOnly,
    );
    let read = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        ToolCall::ReadFile {
            project: project.clone(),
            path: "src/read_only.rs".to_string(),
            session_id: Some(session_b.clone()),
            start_line: None,
            limit: None,
            with_line_numbers: None,
        },
        Some(&read_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert!(read.success, "{:?}", read.error);
    let before = session_value(&runtime, &session_b);

    let denied = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_call(
            &project,
            "enable writes",
            SessionMode::Normal,
            Some(&session_b),
            true,
            false,
        ),
        Some(&read_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert!(!denied.success);
    assert_eq!(
        denied.output["error_kind"],
        "session_capability_upgrade_denied"
    );
    assert_eq!(session_value(&runtime, &session_b), before);
    let current = current_session_in_window(
        &runtime,
        &project,
        Some(&read_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert_eq!(current.output["session_id"], session_a);

    let upgraded = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_call(
            &project,
            "enable writes",
            SessionMode::Normal,
            Some(&session_b),
            true,
            false,
        ),
        Some(&write_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert!(upgraded.success, "{:?}", upgraded.error);
    assert_eq!(upgraded.output["session"]["session_id"], session_b);
    assert_eq!(upgraded.output["session"]["mode"], "normal");
    assert_eq!(
        upgraded.output["session"]["capability"]["previous_mode"],
        "read_only"
    );
    assert_eq!(upgraded.output["session"]["capability"]["changed"], true);
    assert_eq!(
        upgraded.output["session"]["capability"]["write_scope_verified"],
        true
    );
    assert_eq!(
        upgraded.output["continuation_feedback"]["attempt"]["exploration"]["observed_paths"],
        json!(["src/read_only.rs"])
    );
    assert_eq!(
        upgraded.output["startup_brief"]["continuation"]["exploration"]["paths"]["items"],
        json!(["src/read_only.rs"])
    );
    assert_eq!(
        upgraded.output["continuation_feedback"]["attempt"]["exploration"]["read_count"],
        1
    );
    assert_eq!(
        upgraded.output["continuation_feedback"]["attempt"]["exploration"]["complete"],
        true
    );
    let summary = runtime.sessions.summary(&session_b, Some(20)).unwrap();
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert_eq!(instruction_events(&runtime, &session_b).len(), 1);
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| {
                event.kind == "tool_call_finished"
                    && event.tool_name == "read_file"
                    && event.status.as_deref() == Some("succeeded")
            })
            .count(),
        1,
        "explicit resume must not reread an explored source file"
    );
    let current = current_session_in_window(
        &runtime,
        &project,
        Some(&write_auth),
        "explicit-upgrade-window",
    )
    .await;
    assert_eq!(current.output["session_id"], session_b);
}

#[tokio::test]
async fn explicit_resume_fault_rolls_back_session_event_and_both_binding_layers() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    init_git_repo(&root);
    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path(&runtime, "explicit-fault", "demo", &root).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-fault",
        coding_call(
            &project,
            "bound session A",
            SessionMode::Normal,
            None,
            true,
            false,
        ),
        Some(&auth),
        "explicit-fault-window",
    )
    .await;
    let session_a = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session_b = create_workflow_session(
        &runtime,
        &project,
        "read-only session B",
        SessionMode::ReadOnly,
    );
    let session_a_before = session_value(&runtime, &session_a);
    let session_b_before = session_value(&runtime, &session_b);
    runtime.sessions.flush_persistence();
    let ledger_before = std::fs::read(&ledger).unwrap();

    runtime
        .sessions
        .fail_next_coding_continuity_precommit_for_test();
    let failed = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-fault",
        coding_call(
            &project,
            "retryable explicit instruction",
            SessionMode::Normal,
            Some(&session_b),
            true,
            false,
        ),
        Some(&auth),
        "explicit-fault-window",
    )
    .await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["error_kind"],
        "coding_continuity_commit_failed"
    );
    assert_eq!(session_value(&runtime, &session_a), session_a_before);
    assert_eq!(session_value(&runtime, &session_b), session_b_before);
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);
    let current =
        current_session_in_window(&runtime, &project, Some(&auth), "explicit-fault-window").await;
    assert_eq!(current.output["session_id"], session_a);
    runtime.sessions.flush_persistence();
    assert_eq!(std::fs::read(&ledger).unwrap(), ledger_before);

    let retried = dispatch_start_coding_task_in_window(
        &runtime,
        "explicit-fault",
        coding_call(
            &project,
            "retryable explicit instruction",
            SessionMode::Normal,
            Some(&session_b),
            true,
            false,
        ),
        Some(&auth),
        "explicit-fault-window",
    )
    .await;
    assert!(retried.success, "{:?}", retried.error);
    assert_eq!(retried.output["session"]["session_id"], session_b);
    let events = instruction_events(&runtime, &session_b);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].instruction.as_deref(),
        Some("retryable explicit instruction")
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| {
                event.instruction.as_deref() == Some("retryable explicit instruction")
            })
            .count(),
        1
    );
    let current =
        current_session_in_window(&runtime, &project, Some(&auth), "explicit-fault-window").await;
    assert_eq!(current.output["session_id"], session_b);
}

#[test]
fn explicit_resume_schema_metadata_and_business_input_are_distinct() {
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "start_coding_task")
        .unwrap();
    let property = &spec.input_schema["properties"]["resume_session_id"];
    assert_eq!(property["type"], "string");
    assert_eq!(property["pattern"], "^wc_sess_[A-Za-z0-9_]+$");
    let description = property["description"].as_str().unwrap();
    let description_lower = description.to_lowercase();
    for phrase in [
        "failure never falls back",
        "no current binding",
        "recording_session_id",
        "mutually exclusive",
    ] {
        assert!(
            description_lower.contains(phrase),
            "missing {phrase}: {description}"
        );
    }
    assert_eq!(
        spec.input_schema["not"]["required"],
        json!(["resume_session_id", "new_session"])
    );
    assert_eq!(
        spec.input_schema["not"]["properties"]["new_session"]["const"],
        true
    );
    assert!(spec.description.contains("resume_session_id"));
    let accepted = crate::tool_runtime::registry::accepted_flattened_args_for_spec(spec);
    assert!(accepted.contains(&"resume_session_id".to_string()));
    assert!(accepted.contains(&"session_id".to_string()));
    assert!(accepted.contains(&"recording_session_id".to_string()));
    let full_output = spec.output_schema["properties"]["output"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["properties"]["detail"]["const"] == "full")
        .expect("full startup schema");
    let output_description = full_output["properties"]["session"]["description"]
        .as_str()
        .unwrap();
    assert!(output_description.contains("explicitly resumed"));

    let call = ToolCall::from_tool_name(
        "start_coding_task",
        json!({
            "project": "agent:test:demo",
            "resume_session_id": "wc_sess_target",
            "session_id": "wc_sess_project_tool_business_input",
            "recording_session_id": "wc_sess_wrapper_recorder"
        }),
    )
    .unwrap();
    match &call {
        ToolCall::StartCodingTask {
            resume_session_id, ..
        } => assert_eq!(resume_session_id.as_deref(), Some("wc_sess_target")),
        _ => panic!("expected start_coding_task"),
    }
    assert!(call.session_id().is_none());
    let audit = call.session_log_arguments();
    assert_eq!(audit["resume_session_id"], "wc_sess_target");
    assert!(audit.get("session_id").is_none());
    assert!(audit.get("recording_session_id").is_none());
}
