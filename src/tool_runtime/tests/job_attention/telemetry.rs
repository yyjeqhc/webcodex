use super::*;
use crate::tool_runtime::model_ergonomics_telemetry::{
    job_convergence::JobEventKind, ModelErgonomicsTimer,
};
use crate::tool_runtime::window_activity::ToolCallCorrelation;

#[tokio::test]
async fn job_convergence_correlates_only_exact_authorized_relations_without_payloads() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("telemetry-secret-principal");
    let client = "telemetry-client";
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let project = format!("agent:{client}:repo");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("telemetry-window");
    let job_id =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    let context = ToolCallCorrelation {
        resolved_project: Some(project.clone()),
        business_session_id: Some(session.clone()),
        ..Default::default()
    };
    let pending = ToolResult::ok(
        json!({"execution_state": "pending", "continuation": super::super::super::jobs::observe_job_continuation(&job_id, None)}),
    );
    let facts = runtime
        .job_convergence_record(
            "run_process",
            &pending,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(facts.pending_handoff_count, 1);
    assert_eq!(facts.events.len(), 1);
    assert_eq!(facts.events[0].kind, JobEventKind::PendingHandoff);
    assert!(facts.events[0].terminal_observed_at_ms.is_none());
    assert!(facts.correlation_complete);
    let observe = ToolResult::ok(
        json!({"items": [{"job_id": job_id, "success": true, "output": {"stdout": "SECRET_OUTPUT", "stderr": "SECRET_ERROR"}}], "command": "SECRET_COMMAND", "source": "SECRET_SOURCE", "env": "SECRET_ENV"}),
    );
    let observed = runtime
        .job_convergence_record(
            "observe_jobs",
            &observe,
            &ToolCallCorrelation::default(),
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(observed.events[0].relation, facts.events[0].relation, "exact fallback without explicit Session still uses durable Job attribution only for telemetry");
    let sparse_observe = ToolResult::ok(json!({"items": [{"job_id": job_id, "terminal": true}]}));
    assert_eq!(
        runtime
            .job_convergence_record(
                "observe_jobs",
                &sparse_observe,
                &context,
                Some(&auth),
                Some(&window)
            )
            .unwrap()
            .events[0]
            .relation,
        facts.events[0].relation
    );
    let failed_observe = ToolResult::ok(json!({"items": [{"job_id": job_id, "success": false}]}));
    let incomplete = runtime
        .job_convergence_record(
            "observe_jobs",
            &failed_observe,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert!(!incomplete.correlation_complete);
    assert!(
        incomplete.events.is_empty(),
        "failed observation is unknown, not evidence of no observation"
    );
    let other_window = ClientWindow::for_test("other-window");
    assert_ne!(
        runtime
            .job_convergence_record(
                "observe_jobs",
                &observe,
                &context,
                Some(&auth),
                Some(&other_window)
            )
            .unwrap()
            .events[0]
            .relation,
        facts.events[0].relation
    );
    for mismatch in [
        ToolCallCorrelation {
            resolved_project: Some("wrong-project".into()),
            ..context.clone()
        },
        ToolCallCorrelation {
            business_session_id: Some("wrong-business-session".into()),
            ..context.clone()
        },
    ] {
        let other = runtime
            .job_convergence_record(
                "observe_jobs",
                &observe,
                &mismatch,
                Some(&auth),
                Some(&window),
            )
            .unwrap();
        assert!(other.events.is_empty());
        assert!(!other.correlation_complete);
    }
    let foreign = shared_key_auth_context("wrong-principal");
    assert!(runtime
        .job_convergence_record(
            "observe_jobs",
            &observe,
            &context,
            Some(&foreign),
            Some(&window)
        )
        .unwrap()
        .events
        .is_empty());
    let mut no_scope = auth.clone();
    no_scope
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_RUNTIME_READ);
    assert!(runtime
        .job_convergence_record(
            "observe_jobs",
            &observe,
            &context,
            Some(&no_scope),
            Some(&window)
        )
        .unwrap()
        .events
        .is_empty());
    let second_job =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    let unrelated = ToolResult::ok(json!({"items": [{"job_id": second_job, "success": true}]}));
    assert_ne!(
        runtime
            .job_convergence_record(
                "observe_jobs",
                &unrelated,
                &context,
                Some(&auth),
                Some(&window)
            )
            .unwrap()
            .events[0]
            .relation,
        facts.events[0].relation
    );
    let bytes = serde_json::to_string(&observed).unwrap();
    for private in [
        &job_id,
        &project,
        &session,
        "telemetry-window",
        "telemetry-secret-principal",
        "SECRET_OUTPUT",
        "SECRET_ERROR",
        "SECRET_COMMAND",
        "SECRET_SOURCE",
        "SECRET_ENV",
        "stdout",
        "stderr",
        "command",
        "source",
        "env",
    ] {
        assert!(!bytes.contains(private), "{private}");
    }
    assert!(bytes.len() < 2048);
    let before = serde_json::to_value(&observe).unwrap();
    let missing = runtime
        .job_convergence_record("observe_jobs", &observe, &context, None, None)
        .unwrap();
    assert!(!missing.correlation_complete);
    assert_eq!(
        serde_json::to_value(&observe).unwrap(),
        before,
        "telemetry omission cannot change ToolResult"
    );
    assert!(runtime
        .job_convergence_record(
            "read_files",
            &ToolResult::ok(json!({})),
            &context,
            Some(&auth),
            Some(&window)
        )
        .is_none());
}

#[tokio::test]
async fn job_convergence_terminal_delivery_uses_existing_server_timestamp() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("telemetry-terminal-owner");
    let client = "telemetry-terminal";
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let project = format!("agent:{client}:repo");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("telemetry-terminal-window");
    let id =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    mark_next_agent_job_running(&runtime, client).await;
    attention(&runtime, &project, Some(&session), &window, &auth).await;
    let job = runtime
        .runner_registry
        .get_job_for_auth(Some(&crate::test_support::runner_access(&auth)), &id)
        .await
        .unwrap();
    let terminal = runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client.into(),
            runner_instance_id: "inst".into(),
            job_id: id.clone(),
            request_id: job.request_id,
            update_seq: Some(2),
            status: "failed".into(),
            stdout_chunk: Some("SECRET".into()),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(1),
            duration_ms: Some(17),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
    let result = attention(&runtime, &project, Some(&session), &window, &auth).await;
    let facts = runtime
        .job_convergence_record(
            "git_status",
            &result,
            &ToolCallCorrelation::default(),
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(facts.passive_terminal_delivery_count, 1);
    assert_eq!(facts.passive_failure_delivery_count, 1);
    assert_eq!(
        facts.events[0].terminal_observed_at_ms,
        terminal.ended_at.map(|seconds| seconds * 1000)
    );
    let again = attention(&runtime, &project, Some(&session), &window, &auth).await;
    assert!(runtime
        .job_convergence_record(
            "git_status",
            &again,
            &ToolCallCorrelation::default(),
            Some(&auth),
            Some(&window)
        )
        .is_none());
    let mut completion = ModelErgonomicsTimer::start("git_status").unwrap().finish();
    completion.job_convergence = Some(facts);
    let record = completion.record_for_tool_result(&result).unwrap();
    assert!(serde_json::to_value(record).unwrap()["job_convergence"].is_object());
    assert!(result.output.get("job_convergence").is_none());
}

#[test]
fn job_convergence_wait_counts_even_pre_result_rejection_without_business_fields() {
    let record = ModelErgonomicsTimer::start("wait_for_job_terminal")
        .unwrap()
        .finish()
        .record_for_pre_result_failure("invalid_arguments");
    assert_eq!(
        record.job_convergence.unwrap().wait_for_job_terminal_count,
        1
    );
    let rejected_observe = ModelErgonomicsTimer::start("observe_jobs")
        .unwrap()
        .finish()
        .record_for_pre_result_failure("invalid_arguments");
    assert!(
        !rejected_observe
            .job_convergence
            .unwrap()
            .correlation_complete
    );
}
