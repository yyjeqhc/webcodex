//! Cargo test-count postcondition coverage across synchronous, Job, Session,
//! and incomplete-evidence paths.

use super::*;

#[tokio::test]
async fn fast_cargo_test_require_tests_rejects_ignored_only_and_records_failed_session_evidence() {
    let client_id = "vhandoff-fast-test-count";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(300));
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_cargo_test_count_assertion: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: Some("focused".to_string()),
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        require_tests: Some(true),
                        min_tests: None,
                        timeout_secs: Some(600),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "running 1 test\n\ntest ignored_only ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["exit_code"], 0);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["passed"], false);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert_eq!(result.output["promoted_to_job"], false);
    assert_eq!(
        result.output["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
    assert_eq!(
        result.output["test_count_assertion"]["evidence_reason_code"],
        "complete_summary"
    );
    assert_eq!(result.output["tests_run_count"], 0);
    assert_eq!(result.output["zero_tests_run"], true);
    assert_eq!(result.output["test_count_assertion"]["actual_tests_run"], 0);
    assert_cargo_result_matches_schema("cargo_test", &result);
    assert!(
        runtime.list_jobs_for_auth(None, None, None).await.output["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest"]["exit_code"], 0);
    assert_eq!(
        validation["latest"]["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
}

#[tokio::test]
async fn handoff_cargo_test_count_failure_preserves_completed_job_and_failed_session_validation() {
    let client_id = "vhandoff-test-count";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_cargo_test_count_assertion: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: Some("focused".to_string()),
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        require_tests: Some(true),
                        min_tests: Some(2),
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    let validation = request
        .job_context
        .as_ref()
        .and_then(|context| context.validation.as_ref())
        .expect("durable validation metadata");
    assert_eq!(validation.minimum_tests, Some(2));
    assert_eq!(validation.require_tests, Some(true));
    assert_eq!(validation.no_run, None);
    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["job_id"], job_id);

    runtime
        .runner_registry
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "running 1 test\n\ntest ignored_only ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let status = runtime
        .job_status_for_auth(job_id.clone(), false, Some(&auth))
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["status"], "completed");
    assert_eq!(status.output["exit_code"], 0);
    assert_eq!(status.output["validation"]["passed"], false);
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["evidence_reason_code"],
        "complete_summary"
    );
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["minimum_tests"],
        2
    );
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["actual_tests_run"],
        0
    );

    let log = runtime
        .job_log_for_auth(job_id.clone(), None, Some(200), Some(&auth), None, None)
        .await;
    assert!(log.success, "{:?}", log.error);
    assert_eq!(log.output["status"], "completed");
    assert_eq!(log.output["exit_code"], 0);
    assert_eq!(log.output["validation"]["passed"], false);
    assert_eq!(
        log.output["validation"]["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&summary, 50, Some(&auth))
        .await;
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest"]["exit_code"], 0);
    assert_eq!(
        validation["latest"]["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
}

#[tokio::test]
async fn durable_cargo_test_explicit_zero_opt_out_survives_job_reconciliation() {
    let client_id = "vhandoff-zero-opt-out";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_cargo_test_count_assertion: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: Some("focused".to_string()),
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        require_tests: Some(false),
                        min_tests: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    let durable = request
        .job_context
        .as_ref()
        .and_then(|context| context.validation.as_ref())
        .expect("durable validation metadata");
    assert_eq!(durable.minimum_tests, None);
    assert_eq!(durable.require_tests, Some(false));
    assert_eq!(durable.no_run, None);

    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["job_id"], job_id);

    runtime
        .runner_registry
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let status = runtime
        .job_status_for_auth(job_id, false, Some(&auth))
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["validation"]["passed"], true);
    assert_eq!(status.output["validation"]["tests_run_count"], 0);
    assert_eq!(status.output["validation"]["zero_tests_run"], true);
    assert_eq!(status.output["validation"]["require_tests"], false);

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&summary, 50, Some(&auth))
        .await;
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["successes"], 1);
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["latest_success"]["require_tests"], false);
    assert_eq!(validation["latest_success"]["zero_tests_run"], true);
}
