use super::*;
use std::time::{Duration, Instant};

const CLIENT: &str = "workspace-cache";

async fn revision(runtime: &ToolRuntime, revision: Option<&str>) {
    let mut row = runtime
        .runner_registry
        .list_runner_projects(CLIENT)
        .await
        .unwrap()
        .remove(0);
    row.revision = revision.map(|hex| format!("sha256:{hex}"));
    row.root_fingerprint = Some(format!("wc_projroot_{}", "a".repeat(64)));
    runtime
        .runner_registry
        .upsert_runner_project_for_instance(CLIENT, "inst", row)
        .await
        .unwrap();
}

async fn fixture() -> (
    tempfile::TempDir,
    ToolRuntime,
    String,
    String,
    crate::auth::AuthContext,
) {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "initial\n", "initial");
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, CLIENT, "demo", tmp.path()).await;
    revision(&runtime, Some(&"a".repeat(64))).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    (tmp, runtime, project, session, auth_context(None, true))
}

async fn poll(
    runtime: &ToolRuntime,
    project: &str,
    session: &str,
    auth: &crate::auth::AuthContext,
    automatic: bool,
    expected_git: usize,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let (project, session, auth) = (project.to_string(), session.to_string(), auth.clone());
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch_with_auth(
                ToolCall::WorkResultState {
                    collaboration: None,
                    project,
                    session_id: Some(session),
                    files: None,
                    automatic,
                },
                Some(&auth),
            )
            .await
    });
    let until = Instant::now() + Duration::from_secs(15);
    let mut git = 0;
    while !task.is_finished() {
        assert!(Instant::now() < until, "Work Result fixture stalled");
        if let Some(request) = probe_patch_agent_request(runtime, CLIENT).await {
            assert_eq!(request.kind, "run_internal_posix_script");
            git += 1;
            complete_agent_request_by_running_locally(runtime, CLIENT, request).await;
        } else {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    let result = task.await.unwrap();
    assert_eq!(
        git, expected_git,
        "unexpected Git observation count: {:?}",
        result.error
    );
    result
}

#[tokio::test]
async fn work_result_automatic_workspace_reuse_keeps_validation_and_jobs_live() {
    let (_tmp, runtime, project, session, auth) = fixture().await;
    let first = poll(&runtime, &project, &session, &auth, true, 1).await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(
        first.output["work_result"]["workspace_observation"]["reused"],
        false
    );
    for _ in 0..8 {
        let next = poll(&runtime, &project, &session, &auth, true, 0).await;
        assert!(next.success);
        assert_eq!(
            next.output["work_result"]["workspace_observation"]["reused"],
            true
        );
        assert_eq!(
            next.output["work_result"]["state_version"],
            first.output["work_result"]["state_version"]
        );
    }
    let start = runtime.sessions.record_tool_call_started_with_options(
        Some(&session),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "cargo_check",
        &json!({"project":project}),
        Some(project.clone()),
        crate::tool_runtime::sessions::session_tool_contract("cargo_check"),
    );
    runtime.sessions.record_tool_call_finished(start,true,&json!({"execution_state":"completed",
        "command_started":true,"command_completed":true,"exit_code":0,"stdout_tail":"","stderr_tail":""}),None,None);
    let updated = poll(&runtime, &project, &session, &auth, true, 0).await;
    assert_ne!(
        updated.output["work_result"]["state_version"],
        first.output["work_result"]["state_version"]
    );
    assert_eq!(
        updated.output["work_result"]["workspace_observation"]["reused"],
        true
    );
    // A Server Job state transition alone is not a new source mutation. The
    // test admits a no-filesystem-effect Job directly into the canonical registry.
    let job = runtime
        .runner_registry
        .start_job_with_metadata(
            crate::runner_protocol::ShellJobOpRequest {
                login: false,
                op: "start".into(),
                client_id: Some(CLIENT.into()),
                cwd: None,
                command: Some("echo observation-only".into()),
                timeout_secs: Some(60),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "alice".into(),
            webcodex_runner_registry::ShellJobStartMetadata {
                project_id: Some(project.clone()),
                session_id: Some(session.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        crate::tool_runtime::tests::jobs::mark_next_agent_job_running(&runtime, CLIENT).await,
        job.job_id
    );
    let changed_job = poll(&runtime, &project, &session, &auth, true, 0).await;
    assert_eq!(changed_job.output["work_result"]["jobs"]["active"], true);
    assert_eq!(
        changed_job.output["work_result"]["workspace_observation"]["reused"],
        true
    );
    eprintln!("WORK_RESULT_REUSE initial_git=1 unchanged_polls=8 unchanged_git=0 validation_change_git=0 job_change_git=0");
}

#[tokio::test]
async fn work_result_workspace_mutation_lease_expiry_explicit_refresh_and_closed_session_reobserve()
{
    let (tmp, runtime, project, session, auth) = fixture().await;
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    for (name, body) in [
        ("README.md", "changed\n"),
        ("another.txt", "changed again\n"),
    ] {
        let mutation = runtime.validation_sources.begin(&project).unwrap();
        std::fs::write(tmp.path().join(name), body).unwrap();
        mutation.finish(&ToolResult::ok(json!({"state_changed":true})));
        let changed = poll(&runtime, &project, &session, &auth, true, 1).await;
        assert!(changed.success);
        assert_eq!(changed.output["work_result"]["workspace"]["clean"], false);
        assert_eq!(
            changed.output["work_result"]["workspace_observation"]["reused"],
            false
        );
    }
    // External filesystem activity is not covered by the dispatch fence. Force
    // refresh/expiry must actually observe it, never turn reuse into freshness proof.
    std::fs::write(tmp.path().join("outside.txt"), "outside\n").unwrap();
    assert!(
        poll(&runtime, &project, &session, &auth, false, 1)
            .await
            .success
    );
    runtime.work_result_workspace_cache.expire_for_test();
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    runtime.sessions.close_session(&session).unwrap();
    let closed = poll(&runtime, &project, &session, &auth, true, 1).await;
    assert!(closed.success, "{:?}", closed.error);
    assert_eq!(
        closed.output["work_result"]["workspace_observation"]["reused"],
        false
    );
}

#[tokio::test]
async fn work_result_workspace_reconnect_revision_loss_and_authority_change_never_reuse() {
    let (tmp, runtime, project, session, auth) = fixture().await;
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    register_runner_project_at_path(&runtime, CLIENT, "demo", tmp.path()).await;
    revision(&runtime, Some(&"a".repeat(64))).await;
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    revision(&runtime, Some(&"b".repeat(64))).await;
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    revision(&runtime, None).await;
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    let foreign = shared_key_auth_context("foreign-work-result");
    assert!(
        !poll(&runtime, &project, &session, &foreign, true, 0)
            .await
            .success
    );
    runtime
        .runner_registry
        .remove_runner_project_for_instance(CLIENT, "inst", "demo")
        .await
        .unwrap();
    assert!(
        !poll(&runtime, &project, &session, &auth, true, 0)
            .await
            .success
    );
}

#[tokio::test]
async fn work_result_pending_job_completion_recovers_presentation_but_not_validation_proof() {
    let (_tmp, runtime, project, session, auth) = fixture().await;
    let runtime = runtime.with_structured_execution_sync_wait(Duration::from_millis(20));
    let task = tokio::spawn({
        let (runtime, project, session, auth) = (
            runtime.clone(),
            project.clone(),
            session.clone(),
            auth.clone(),
        );
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "echo pending-writer".into(),
                        timeout_secs: Some(120),
                        sync_wait_secs: None,
                        cwd: None,
                        purpose: None,
                        shell: None,
                        session_id: Some(session),
                        login: false,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, CLIENT).await;
    let job_id = request.job_id.clone().expect("canonical Job");
    let update = |seq, status: &str, finished, exit_code| {
        serde_json::from_value(json!({
            "client_id": CLIENT, "agent_instance_id": "inst", "job_id": job_id,
            "request_id": request.request_id, "update_seq": seq, "status": status,
            "finished": finished, "exit_code": exit_code,
        }))
        .unwrap()
    };
    runtime
        .runner_registry
        .update_job(update(1, "running", false, None::<i32>))
        .await
        .unwrap();
    assert!(task.await.unwrap().success);
    assert!(
        !runtime
            .validation_sources
            .capture(&project)
            .unwrap()
            .quiescent
    );
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    runtime
        .runner_registry
        .update_job(update(2, "completed", true, Some(0)))
        .await
        .unwrap();
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    let reused = poll(&runtime, &project, &session, &auth, true, 0).await;
    assert!(reused.success);
    assert_eq!(
        reused.output["work_result"]["workspace_observation"]["reused"],
        true
    );
    assert!(
        !runtime
            .validation_sources
            .capture(&project)
            .unwrap()
            .quiescent,
        "presentation observation must never turn uncertain validation into proof"
    );
    assert!(!runtime.runner_registry.observation_jobs_ended_for_project(
        None,
        "wrong-project",
        &[job_id.clone()]
    ));
    assert!(!runtime.runner_registry.observation_jobs_ended_for_project(
        None,
        &project,
        &["wc_job_missing0123456".into()]
    ));
    assert!(!runtime.runner_registry.observation_jobs_ended_for_project(
        None,
        &project,
        &vec![job_id; 129]
    ));
    eprintln!("WORK_RESULT_HANDOFF active_job_git=1 first_terminal_git=1 unchanged_terminal_git=0 validation_quiescent=false");
}

#[tokio::test]
async fn work_result_unknown_non_job_writer_never_reuses_presentation() {
    let (_tmp, runtime, project, session, auth) = fixture().await;
    runtime
        .validation_sources
        .begin(&project)
        .unwrap()
        .finish(&ToolResult::ok(
            json!({"execution_state":"outcome_unknown"}),
        ));
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
}

#[tokio::test]
async fn work_result_workspace_mutation_during_probe_is_not_cached() {
    let (tmp, runtime, project, session, auth) = fixture().await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session = session.clone();
        let auth = auth.clone();
        async move {
            runtime
                .work_result_state_for_window_with_refresh(
                    project,
                    Some(session),
                    Some(&auth),
                    None,
                    true,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, CLIENT).await;
    let mutation = runtime.validation_sources.begin(&project).unwrap();
    std::fs::write(tmp.path().join("README.md"), "raced\n").unwrap();
    mutation.finish(&ToolResult::ok(json!({"state_changed":true})));
    complete_agent_request_by_running_locally(&runtime, CLIENT, request).await;
    assert!(task.await.unwrap().success);
    assert!(
        poll(&runtime, &project, &session, &auth, true, 1)
            .await
            .success
    );
    assert!(
        poll(&runtime, &project, &session, &auth, true, 0)
            .await
            .success
    );
}
