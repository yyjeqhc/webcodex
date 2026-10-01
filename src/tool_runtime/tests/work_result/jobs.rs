use super::*;
use crate::tool_runtime::tests::jobs::{
    mark_next_agent_job_running, register_job_agent_for_auth_with_reconciliation,
    start_agent_runtime_job_in_session,
};

#[tokio::test]
async fn work_result_jobs_are_exact_authorized_bounded_server_snapshots() {
    let runtime = test_runtime();
    let auth = shared_key_auth_context("work-jobs-owner");
    let foreign = shared_key_auth_context("work-jobs-foreign");
    let client = "work-jobs";
    let project = format!("agent:{client}:repo");
    register_job_agent_for_auth_with_reconciliation(&runtime, client, "repo", &auth, true).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let other = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let job =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    assert_eq!(mark_next_agent_job_running(&runtime, client).await, job);
    let excluded =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&other), &auth).await;
    assert_eq!(
        mark_next_agent_job_running(&runtime, client).await,
        excluded
    );
    let running = runtime
        .work_result_jobs(&project, Some(&session), Some(&auth))
        .await;
    assert_eq!(running["active"], true);
    assert_eq!(running["items"].as_array().unwrap().len(), 1);
    assert_eq!(running["items"][0]["job_id"], job);
    assert_eq!(running["items"][0]["status"], "running");
    assert_eq!(
        runtime.work_result_jobs(&project, None, Some(&auth)).await,
        json!({"available": false})
    );
    for (target, caller) in [(&project[..], &foreign), ("agent:other:repo", &auth)] {
        assert_eq!(
            runtime
                .work_result_jobs(target, Some(&session), Some(caller))
                .await["items"],
            json!([])
        );
    }
    let mut no_scope = auth.clone();
    no_scope
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_RUNTIME_READ);
    assert_eq!(
        runtime
            .work_result_jobs(&project, Some(&session), Some(&no_scope))
            .await,
        json!({"available": false})
    );
    assert!(probe_agent_request_for_instance(&runtime, client, "inst")
        .await
        .is_none());

    // The UI read does not consume the model's independent attention baseline.
    let window = crate::client_window::ClientWindow::for_test("work-jobs-window");
    let mut baseline = ToolResult::ok(json!({}));
    runtime
        .add_passive_job_attention(
            &mut baseline,
            "get_git_status",
            Some(&project),
            Some(&session),
            Some(&window),
            Some(&auth),
        )
        .await;
    runtime
        .runner_registry
        .reconcile_disconnect(client, "inst")
        .await;
    let recovering = runtime
        .work_result_jobs(&project, Some(&session), Some(&auth))
        .await;
    assert_eq!(recovering["items"][0]["status"], "recovering");
    assert_eq!(
        runtime
            .work_result_jobs(&project, Some(&session), Some(&auth))
            .await,
        recovering
    );
    let mut attention = ToolResult::ok(json!({}));
    runtime
        .add_passive_job_attention(
            &mut attention,
            "get_git_status",
            Some(&project),
            Some(&session),
            Some(&window),
            Some(&auth),
        )
        .await;
    assert_eq!(attention.output["job_attention"]["items"][0]["job_id"], job);

    let mut snapshot = runtime
        .runner_registry
        .snapshot_jobs_for_auth_filtered(
            crate::runner_http::runner_access_from_auth(Some(&auth)).as_ref(),
            &project,
            &session,
            1,
            1,
        )
        .await
        .remove(0);
    snapshot.job.status = "completed".into();
    snapshot.job.exit_code = Some(0);
    snapshot.job.recovery_state = Some("recovered".into());
    snapshot.job.recovered_after_server_restart = true;
    let recovered = runtime.work_result_job(&snapshot);
    assert_eq!(recovered["state"], "terminal");
    assert_eq!(recovered["outcome"], "passed");
    assert_eq!(recovered["recovery_state"], "recovered");
    for private in [
        "command",
        "stdout",
        "stderr",
        "observation_token",
        "details",
        "client_id",
    ] {
        assert!(recovered.get(private).is_none());
    }
}

#[tokio::test]
async fn work_result_job_terminal_transition_is_visible_without_model_activity() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = test_runtime();
    let client = "work-terminal";
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client,
        "repo",
        root.path(),
        crate::test_support::current_runner_capabilities(
            crate::runner_protocol::RunnerCapabilities {
                async_shell_jobs: true,
                shell: true,
                git: true,
                file_read: true,
                ..Default::default()
            },
        ),
    )
    .await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    for exit_code in [0, 1] {
        let job =
            start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth)
                .await;
        let request = wait_for_runner_request_for_instance(&runtime, client, "inst").await;
        let before = refresh_once(&runtime, client, &project, &session, &auth).await;
        assert!(before.success, "{before:?}");
        let events_before = runtime
            .sessions
            .summary(&session, None)
            .unwrap()
            .events
            .len();
        complete_patch_agent_request(
            &runtime,
            client,
            &request.request_id,
            exit_code,
            "PRIVATE_OUTPUT",
            "PRIVATE_ERROR",
        )
        .await;
        let after = refresh_once(&runtime, client, &project, &session, &auth).await;
        assert!(after.success, "{after:?}");
        let jobs = &after.output["work_result"]["jobs"];
        let item = jobs["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["job_id"] == job)
            .unwrap();
        assert_eq!(item["state"], "terminal");
        assert_eq!(
            item["outcome"],
            if exit_code == 0 { "passed" } else { "failed" }
        );
        assert_ne!(
            before.output["work_result"]["state_version"],
            after.output["work_result"]["state_version"]
        );
        assert_eq!(
            runtime
                .sessions
                .summary(&session, None)
                .unwrap()
                .events
                .len(),
            events_before
        );
        assert!(!jobs.to_string().contains("PRIVATE_"));
        let again = refresh_once(&runtime, client, &project, &session, &auth).await;
        assert_eq!(
            after.output["work_result"]["state_version"],
            again.output["work_result"]["state_version"]
        );
    }
    for _ in 0..10 {
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
        mark_next_agent_job_running(&runtime, client).await;
    }
    let bounded = runtime
        .work_result_jobs(&project, Some(&session), Some(&auth))
        .await;
    assert_eq!(bounded["items"].as_array().unwrap().len(), 8);
    assert_eq!(bounded["truncated"], true);
}
