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
    let code_mode_pending = ToolResult::ok(json!({
        "effect_receipt": {
            "children": [{
                "outcome": "job_handoff",
                "job_id": job_id,
                "continuation": super::super::super::jobs::observe_job_continuation(&job_id, None),
            }]
        }
    }));
    let code_mode_facts = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &code_mode_pending,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(code_mode_facts.pending_handoff_count, 1);
    assert_eq!(code_mode_facts.events.len(), 1);
    assert_eq!(code_mode_facts.events[0].kind, JobEventKind::PendingHandoff);
    assert_eq!(code_mode_facts.events[0].relation, facts.events[0].relation);
    assert!(code_mode_facts.correlation_complete);
    let mut mixed_children = vec![json!({"outcome": "known_result"}); 9];
    mixed_children.push(json!({
        "outcome": "job_handoff",
        "job_id": job_id,
        "continuation": super::super::super::jobs::observe_job_continuation(&job_id, None),
    }));
    let mixed_receipt = ToolResult::ok(json!({
        "effect_receipt": {"children": mixed_children}
    }));
    let mixed_facts = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &mixed_receipt,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(mixed_facts.pending_handoff_count, 1);
    assert_eq!(mixed_facts.events.len(), 1);
    assert_eq!(mixed_facts.events[0].relation, facts.events[0].relation);
    assert!(
        mixed_facts.correlation_complete,
        "non-handoff receipt children do not consume the telemetry event budget",
    );
    let mut oversized_children = vec![json!({"outcome": "known_result"}); 32];
    oversized_children.push(json!({
        "outcome": "job_handoff",
        "job_id": job_id,
        "continuation": super::super::super::jobs::observe_job_continuation(&job_id, None),
    }));
    let oversized_receipt = ToolResult::ok(json!({
        "effect_receipt": {"children": oversized_children}
    }));
    let oversized_facts = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &oversized_receipt,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert!(oversized_facts.events.is_empty());
    assert!(
        !oversized_facts.correlation_complete,
        "receipt input beyond the canonical Code Mode child bound fails closed",
    );
    assert!(
        !serde_json::to_string(&code_mode_facts)
            .unwrap()
            .contains(&job_id),
        "payload-safe convergence telemetry must not persist raw nested Job ids",
    );

    let failed_code_mode_pending = ToolResult::err_with_output(
        "cell failed after handoff",
        json!({
            "effect_receipt": {
                "children": [{
                    "outcome": "job_handoff",
                    "job_id": job_id,
                    "continuation": super::super::super::jobs::observe_job_continuation(&job_id, None),
                }]
            }
        }),
    );
    let failed_parent_facts = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &failed_code_mode_pending,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(failed_parent_facts.pending_handoff_count, 1);
    assert_eq!(failed_parent_facts.events.len(), 1);
    assert_eq!(
        failed_parent_facts.events[0].relation, facts.events[0].relation,
        "canonical child handoff truth survives a failed Code Mode parent",
    );
    assert!(failed_parent_facts.correlation_complete);

    let malformed_code_mode_pending = ToolResult::ok(json!({
        "effect_receipt": {
            "children": [{
                "outcome": "job_handoff",
                "job_id": job_id,
                "continuation": {
                    "tool": "observe_jobs",
                    "arguments": {"items": [{"job_id": "different-job"}]}
                }
            }]
        }
    }));
    let malformed = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &malformed_code_mode_pending,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert!(!malformed.correlation_complete);
    assert!(malformed.events.is_empty());
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

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn job_convergence_marks_combined_code_mode_handoff_and_attention_overflow_incomplete() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("telemetry-overflow-owner");
    let client = "telemetry-overflow";
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let project = format!("agent:{client}:repo");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("telemetry-overflow-window");
    let context = ToolCallCorrelation {
        resolved_project: Some(project),
        business_session_id: Some(session.clone()),
        ..Default::default()
    };

    assert!(
        webcodex_tool_contracts::runtime_tool_supports_passive_job_attention(
            "execute_effectful_code_mode"
        )
    );
    let mut jobs = Vec::new();
    for _ in 0..10 {
        jobs.push(
            start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth)
                .await,
        );
    }
    let children: Vec<_> = jobs[..9]
        .iter()
        .map(|job_id| {
            json!({
                "outcome": "job_handoff",
                "job_id": job_id,
                "continuation": super::super::super::jobs::observe_job_continuation(job_id, None),
            })
        })
        .collect();
    let result = ToolResult::ok(json!({
        "effect_receipt": {"children": children},
        "job_attention": {
            "items": [{
                "job_id": jobs[9],
                "state": "terminal",
                "command_ok": true,
                "validation": {"passed": true}
            }]
        }
    }));

    let facts = runtime
        .job_convergence_record(
            "execute_effectful_code_mode",
            &result,
            &context,
            Some(&auth),
            Some(&window),
        )
        .unwrap();
    assert_eq!(facts.pending_handoff_count, 9);
    assert_eq!(facts.passive_terminal_delivery_count, 1);
    assert_eq!(facts.events.len(), 9);
    assert!(
        !facts.correlation_complete,
        "bounded event truncation must fail correlation completeness closed",
    );
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
            "get_git_status",
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
            "get_git_status",
            &again,
            &ToolCallCorrelation::default(),
            Some(&auth),
            Some(&window)
        )
        .is_none());
    let mut completion = ModelErgonomicsTimer::start("get_git_status")
        .unwrap()
        .finish();
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
