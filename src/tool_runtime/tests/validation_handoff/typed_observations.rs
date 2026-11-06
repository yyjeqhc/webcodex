use super::super::jobs::{register_job_agent_for_auth, start_agent_runtime_job_in_session};
use super::*;

#[tokio::test]
async fn typed_validation_queries_preserve_authority_complete_candidates_and_output_privacy() {
    let runtime = ToolRuntime::new_for_tests();
    let owner = shared_key_auth_context("validation-query-owner");
    let foreign = shared_key_auth_context("validation-query-foreign");
    let client = "validation-query-own";
    let other_client = "validation-query-other";
    register_job_agent_for_auth(&runtime, client, "demo", &owner).await;
    register_job_agent_for_auth(&runtime, other_client, "demo", &foreign).await;
    let project = format!("agent:{client}:demo");
    let other_project = format!("agent:{other_client}:demo");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let unselected = runtime.sessions.start_session(Some(project.clone()), None);
    let other_session = runtime
        .sessions
        .start_session(Some(other_project.clone()), None);
    let mut expected = Vec::new();
    for ordinal in 0..2 {
        expected.push(
            seed_retained_terminal_validation_job(
                &runtime,
                client,
                &project,
                &session.session_id,
                ordinal,
            )
            .await
            .job_id,
        );
    }
    seed_retained_terminal_validation_job(&runtime, client, &project, &unselected.session_id, 10)
        .await;
    let foreign_job = seed_retained_terminal_validation_job(
        &runtime,
        other_client,
        &other_project,
        &other_session.session_id,
        20,
    )
    .await;
    // Ordinary shell Jobs without admission-derived validation identity must
    // not enter the durable validation marker ledger.
    start_agent_runtime_job_in_session(&runtime, client, "demo", Some(&session.session_id), &owner)
        .await;
    let events_before = runtime
        .sessions
        .summary(&session.session_id, None)
        .unwrap()
        .events_total;
    let selected = vec![session.session_id.clone(), other_session.session_id.clone()];
    let candidates = runtime
        .validation_job_candidates_for_sessions(&project, &selected, Some(&owner))
        .await;
    assert_eq!(candidates.len(), 1);
    let jobs = &candidates[&session.session_id];
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|job| expected.contains(&job.job_id)));
    assert!(jobs
        .windows(2)
        .all(|pair| pair[0].created_at > pair[1].created_at
            || (pair[0].created_at == pair[1].created_at && pair[0].job_id < pair[1].job_id)));
    let page = runtime
        .query_job_inventory_for_auth(
            Some(1),
            None,
            Some(&project),
            Some(&session.session_id),
            None,
            Some(&owner),
        )
        .await
        .unwrap();
    assert!(page.truncated());
    assert_eq!(
        page.jobs.len(),
        1,
        "model page bounds must not truncate reconciliation candidates"
    );
    assert!(runtime
        .validation_job_candidates_for_sessions(&project, &selected, Some(&foreign),)
        .await
        .is_empty());
    assert!(runtime
        .validation_job_candidates_for_sessions(&other_project, &selected, Some(&owner),)
        .await
        .is_empty());
    assert!(runtime
        .query_job_status_for_auth(&foreign_job.job_id, Some(&owner))
        .await
        .is_err());

    let observation = runtime
        .query_job_status_for_auth(&expected[0], Some(&owner))
        .await
        .unwrap();
    assert_eq!(
        observation.job.project_id.as_deref(),
        Some(project.as_str())
    );
    assert_eq!(
        observation.job.session_id.as_deref(),
        Some(session.session_id.as_str())
    );
    assert_eq!(observation.job.exit_code, Some(0));
    assert_eq!(
        observation.validation.as_ref().unwrap().passed(),
        Some(true)
    );
    let public = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                page: false,
                job_id: expected[0].clone(),
                after_observation_token: None,
                observation_ref: None,
            }],
            200,
            None,
            ObserveJobsWakeOn::Change,
            Some(&owner),
        )
        .await;
    assert!(public.success);
    let item = &public.output["items"][0];
    assert_eq!(item["success"], true);
    let output = &item["output"];
    assert_eq!(
        output["validation"],
        observation.validation.unwrap().into_value()
    );
    for field in [
        "request_id",
        "command_preview",
        "test_count_evidence",
        "validation_identity",
    ] {
        assert!(output.get(field).is_none(), "internal fact leaked: {field}");
    }
    assert_eq!(
        runtime
            .sessions
            .summary(&session.session_id, None)
            .unwrap()
            .events_total,
        events_before,
        "typed observations must not append Session evidence"
    );
}

#[tokio::test]
async fn typed_validation_terminal_materialization_preserves_log_evidence_and_idempotency() {
    let client = "validation-query-evidence";
    let runtime = runtime_with_agent_project(client);
    register_async_validation_agent(&runtime, client).await;
    let project = agent_test_project_id(client);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let job =
        seed_retained_terminal_validation_job(&runtime, client, &project, &session.session_id, 0)
            .await;
    let public_log = runtime
        .job_log_for_auth(job.job_id.clone(), None, Some(200), None, None, None)
        .await;
    assert!(public_log.success);
    let expected_log = crate::tool_runtime::sessions::execution_output_summary_for_tool_result(
        "cargo_check",
        &public_log.output,
    )
    .unwrap();
    runtime
        .materialize_validation_job_terminals_for_sessions(
            &project,
            std::slice::from_ref(&session.session_id),
            None,
        )
        .await;
    let recorded = runtime.sessions.summary(&session.session_id, None).unwrap();
    let terminal = recorded
        .events
        .iter()
        .find(|event| event.kind == "validation_job_terminal")
        .unwrap();
    assert_eq!(terminal.job_id.as_deref(), Some(job.job_id.as_str()));
    assert_eq!(terminal.status.as_deref(), Some("succeeded"));
    let evidence = terminal.validation_output_summary.as_ref().unwrap();
    for field in [
        "stdout_tail_excerpt",
        "stderr_tail_excerpt",
        "stdout_truncated",
        "stderr_truncated",
        "stdout_lines",
        "stderr_lines",
        "command_summary",
        "purpose",
        "cwd",
        "shell",
        "executor",
    ] {
        assert_eq!(
            evidence[field], expected_log[field],
            "changed persisted evidence: {field}"
        );
    }
    runtime
        .materialize_validation_job_terminals_for_sessions(
            &project,
            std::slice::from_ref(&session.session_id),
            None,
        )
        .await;
    let repeated = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(repeated.events_total, recorded.events_total);
    assert_eq!(repeated.updated_at, recorded.updated_at);
}
