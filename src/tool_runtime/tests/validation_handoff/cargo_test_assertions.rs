//! Cargo test-count postcondition coverage across synchronous, Job, Session,
//! capability-fence, and incomplete-evidence paths.

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
        ShellClientCapabilities {
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
        .shell_clients
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
        ShellClientCapabilities {
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
    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["job_id"], job_id);

    runtime
        .shell_clients
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

#[cfg(unix)]
#[tokio::test]
async fn local_sync_cargo_test_enforces_explicit_count_without_changing_zero_test_default() {
    let tmp = tempfile::tempdir().unwrap();
    write_local_validation_crate(
        tmp.path(),
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn one() {}

    #[test]
    fn two() {}

    #[test]
    #[ignore]
    fn ignored_only() {}
}
"#,
    );
    let runtime = runtime_with_project(tmp.path(), "demo");
    let config = local_project_config(&tmp.path().to_string_lossy());
    let compatible_zero = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test does_not_exist",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions {
                filter: Some("does_not_exist".to_string()),
                ..Default::default()
            },
            ExecutionPurpose::Test,
            30,
            5,
            None,
        )
        .await;
    assert!(compatible_zero.success, "{:?}", compatible_zero.error);
    assert_eq!(compatible_zero.output["exit_code"], 0);
    assert_eq!(compatible_zero.output["passed"], true);
    assert_eq!(compatible_zero.output["tests_run_count"], 0);
    assert_eq!(compatible_zero.output["zero_tests_run"], true);
    assert!(compatible_zero.output.get("test_count_assertion").is_none());
    assert_cargo_result_matches_schema("cargo_test", &compatible_zero);

    let ignored_default = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test ignored_only",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions {
                filter: Some("ignored_only".to_string()),
                ..Default::default()
            },
            ExecutionPurpose::Test,
            30,
            5,
            None,
        )
        .await;
    assert!(ignored_default.success, "{:?}", ignored_default.error);
    assert_eq!(ignored_default.output["exit_code"], 0);
    assert_eq!(ignored_default.output["passed"], true);
    assert_eq!(ignored_default.output["tests_run_count"], 0);
    assert_eq!(ignored_default.output["zero_tests_run"], true);
    assert!(ignored_default.output.get("test_count_assertion").is_none());

    let ignored_required = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test ignored_only",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions {
                filter: Some("ignored_only".to_string()),
                ..Default::default()
            },
            ExecutionPurpose::Test,
            30,
            5,
            Some(1),
        )
        .await;
    assert!(!ignored_required.success);
    assert_eq!(ignored_required.output["exit_code"], 0);
    assert_eq!(ignored_required.output["passed"], false);
    assert_eq!(ignored_required.output["tests_run_count"], 0);
    assert_eq!(ignored_required.output["zero_tests_run"], true);
    assert_eq!(
        ignored_required.output["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
    assert_eq!(
        ignored_required.output["test_count_assertion"]["actual_tests_run"],
        0
    );
    assert_cargo_result_matches_schema("cargo_test", &ignored_required);

    let rejected_zero = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test does_not_exist",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions {
                filter: Some("does_not_exist".to_string()),
                ..Default::default()
            },
            ExecutionPurpose::Test,
            30,
            5,
            Some(1),
        )
        .await;
    assert!(!rejected_zero.success);
    assert_eq!(rejected_zero.output["exit_code"], 0);
    assert_eq!(rejected_zero.output["passed"], false);
    assert_eq!(rejected_zero.output["failure_kind"], "validation_failed");
    assert_eq!(
        rejected_zero.output["test_count_assertion"]["minimum_tests"],
        1
    );
    assert_eq!(
        rejected_zero.output["test_count_assertion"]["actual_tests_run"],
        0
    );
    assert_eq!(
        rejected_zero.output["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
    assert_cargo_result_matches_schema("cargo_test", &rejected_zero);

    let exact_minimum = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions::default(),
            ExecutionPurpose::Test,
            30,
            5,
            Some(2),
        )
        .await;
    assert!(exact_minimum.success, "{:?}", exact_minimum.error);
    assert_eq!(exact_minimum.output["tests_run_count"], 2);
    assert_eq!(
        exact_minimum.output["test_count_assertion"]["status"],
        "passed"
    );
    assert_eq!(
        exact_minimum.output["test_count_assertion"]["minimum_tests"],
        2
    );

    let cargo_failure = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test -p missing-package",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions {
                package: Some("missing-package".to_string()),
                ..Default::default()
            },
            ExecutionPurpose::Test,
            30,
            5,
            Some(1),
        )
        .await;
    assert!(!cargo_failure.success);
    assert_ne!(cargo_failure.output["exit_code"], 0);
    assert_eq!(cargo_failure.output["failure_kind"], "validation_failed");
    assert!(
        cargo_failure.output.get("test_count_assertion").is_none(),
        "the postcondition must not overwrite a real Cargo failure"
    );
    assert_cargo_result_matches_schema("cargo_test", &cargo_failure);
}

#[tokio::test]
async fn local_job_status_fails_closed_when_bounded_logs_cannot_prove_test_count() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let job_id = "bounded-local-validation";
    let dir = tmp.path().join(".codex/jobs").join(job_id);
    std::fs::create_dir_all(&dir).unwrap();
    let (record, _) = crate::tool_runtime::local_jobs::LocalJobRecord::initialize(
        "demo".to_string(),
        dir.clone(),
    )
    .unwrap();
    std::fs::write(
        dir.join("metadata.json"),
        serde_json::to_string(&json!({
            "job_id": job_id,
            "project": "demo",
            "command": "cargo test",
            "kind": "validation",
            "created_at": 1,
            "started_at": 1,
            "validation_tool": "cargo_test",
            "validation_kind": "test",
            "minimum_tests": 6,
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("status"), "completed").unwrap();
    std::fs::write(dir.join("exit_code"), "0").unwrap();
    std::fs::write(dir.join("finished_at"), "2").unwrap();
    let mut stdout = (0..(crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES + 100))
        .map(|index| format!("progress line {index}\n"))
        .collect::<String>();
    stdout.push_str("test result: ok. 7 passed; 0 failed; 0 ignored\n");
    std::fs::write(dir.join("stdout.log"), stdout).unwrap();
    std::fs::write(dir.join("stderr.log"), "").unwrap();
    record.observe().unwrap();
    record.mark_terminal();
    runtime
        .local_jobs
        .lock()
        .await
        .insert(job_id.to_string(), record.clone());

    let bounded = record.read_log_lines(
        "stdout.log",
        None,
        Some(crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES),
    );
    assert!(bounded.3);
    assert!(bounded.0.lines().count() <= crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES);

    let status = runtime.job_status(job_id.to_string()).await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["status"], "completed");
    assert_eq!(status.output["exit_code"], 0);
    assert_eq!(status.output["validation"]["passed"], false);
    assert_eq!(status.output["validation"]["truncated"], true);
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["status"],
        "unproven"
    );
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["reason_code"],
        "test_count_unproven"
    );
    for field in [
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        assert!(status.output["validation"][field].is_null(), "{field}");
    }
}
