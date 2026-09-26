mod failures;
mod telemetry;

use super::jobs::{
    mark_next_agent_job_running, register_job_agent_for_auth,
    register_job_agent_for_auth_with_reconciliation, start_agent_runtime_job_in_session,
};
use super::support::{probe_agent_request_for_instance, shared_key_auth_context};
use crate::client_window::ClientWindow;
use crate::runner_protocol::RunnerJobUpdateRequest;
use crate::tool_runtime::{ToolResult, ToolRuntime};
use serde_json::json;

async fn attention(
    runtime: &ToolRuntime,
    project: &str,
    session_id: Option<&str>,
    window: &ClientWindow,
    auth: &crate::auth::AuthContext,
) -> ToolResult {
    let mut result = ToolResult::ok(json!({"main":"preserved"}));
    runtime
        .add_passive_job_attention(
            &mut result,
            "git_status",
            Some(project),
            session_id,
            Some(window),
            Some(auth),
        )
        .await;
    result
}

#[tokio::test]
async fn passive_attention_requires_exact_business_relation_and_deduplicates_state() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("passive-owner");
    let foreign = shared_key_auth_context("passive-foreign");
    let client = "passive-job";
    let project = format!("agent:{client}:repo");
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let other_session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("passive-window-1");
    let another_window = ClientWindow::for_test("passive-window-2");
    let empty = attention(&runtime, &project, Some(&session), &window, &auth).await;
    assert!(empty.output.get("job_attention").is_none());
    let job_id =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    assert_eq!(mark_next_agent_job_running(&runtime, client).await, job_id);
    let mut diagnostic = ToolResult::ok(json!({"status":"available"}));
    runtime
        .add_passive_job_attention(
            &mut diagnostic,
            "current_window_activity",
            Some(&project),
            Some(&session),
            Some(&window),
            Some(&auth),
        )
        .await;
    assert!(diagnostic.output.get("job_attention").is_none());
    let first = attention(&runtime, &project, Some(&session), &window, &auth).await;
    assert!(first.output.get("job_attention").is_none());
    assert_eq!(first.output["main"], "preserved");
    for absent in [
        "stdout",
        "stderr",
        "command",
        "cwd",
        "client_id",
        "runner_instance_id",
        "session_id",
    ] {
        assert!(
            !first.output["job_attention"].to_string().contains(absent),
            "{absent}"
        );
    }
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    assert!(
        probe_agent_request_for_instance(&runtime, client, "inst")
            .await
            .is_none(),
        "passive attention must not request Runner status"
    );
    assert!(
        attention(&runtime, &project, Some(&other_session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    assert!(attention(&runtime, &project, None, &window, &auth)
        .await
        .output
        .get("job_attention")
        .is_none());
    assert!(
        attention(&runtime, &project, Some(&session), &window, &foreign)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    assert!(
        attention(&runtime, "agent:wrong:repo", Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    let mut no_runtime_read = auth.clone();
    no_runtime_read
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_RUNTIME_READ);
    assert!(attention(
        &runtime,
        &project,
        Some(&session),
        &window,
        &no_runtime_read
    )
    .await
    .output
    .get("job_attention")
    .is_none());
    let mut revoked_project = auth.clone();
    let mut anonymous = auth.clone();
    anonymous.kind = crate::auth::AuthKind::OpenAnonymous;
    assert!(
        attention(&runtime, &project, Some(&session), &window, &anonymous)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    revoked_project
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_PROJECT_READ);
    assert!(attention(
        &runtime,
        &project,
        Some(&session),
        &window,
        &revoked_project
    )
    .await
    .output
    .get("job_attention")
    .is_none());
    assert!(
        attention(&runtime, &project, Some(&session), &another_window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    let mut restarted = runtime.clone();
    restarted.job_attention_cursor =
        std::sync::Arc::new(crate::tool_runtime::job_attention::JobAttentionCursor::default());
    assert!(
        attention(&restarted, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    let mut failed_attention = runtime.clone();
    failed_attention.job_attention_cursor =
        std::sync::Arc::new(crate::tool_runtime::job_attention::JobAttentionCursor::default());
    failed_attention.job_attention_cursor.poison_for_test();
    let preserved = attention(&failed_attention, &project, Some(&session), &window, &auth).await;
    assert!(preserved.success);
    assert_eq!(preserved.output["main"], "preserved");
    assert!(preserved.output.get("job_attention").is_none());

    let job = runtime
        .runner_registry
        .get_job_for_auth(Some(&crate::test_support::runner_access(&auth)), &job_id)
        .await
        .unwrap();
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client.into(),
            runner_instance_id: "inst".into(),
            job_id: job_id.clone(),
            request_id: job.request_id,
            update_seq: Some(2),
            status: "completed".into(),
            stdout_chunk: Some("secret output".into()),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(25),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
    let terminal = attention(&runtime, &project, Some(&session), &window, &auth).await;
    assert_eq!(
        terminal.output["job_attention"]["items"][0]["status"],
        "completed"
    );
    assert_eq!(terminal.output["job_attention"]["items"][0]["exit_code"], 0);
    assert_eq!(
        attention(&runtime, &project, Some(&session), &another_window, &auth)
            .await
            .output["job_attention"]["items"][0]["job_id"],
        job_id
    );
    let mut restarted_after_terminal = runtime.clone();
    restarted_after_terminal.job_attention_cursor =
        std::sync::Arc::new(crate::tool_runtime::job_attention::JobAttentionCursor::default());
    assert!(
        attention(
            &restarted_after_terminal,
            &project,
            Some(&session),
            &window,
            &auth
        )
        .await
        .output
        .get("job_attention")
        .is_none(),
        "historical terminal must not replay"
    );
    let terminal_item = &terminal.output["job_attention"]["items"][0];
    assert_eq!(terminal_item["state"], "terminal");
    assert_eq!(terminal_item["outcome"], "passed");
    assert_eq!(terminal_item["command_ok"], true);
    assert_eq!(terminal_item["details"]["tool"], "observe_jobs");
    assert_eq!(
        terminal_item["details"]["arguments"]["items"][0]["job_id"],
        job_id
    );
    assert!(!terminal.output["job_attention"]
        .to_string()
        .contains("secret output"));
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    let mut failure = ToolResult::err("main failed");
    runtime
        .add_passive_job_attention(
            &mut failure,
            "git_status",
            Some(&project),
            Some(&session),
            Some(&window),
            Some(&auth),
        )
        .await;
    assert!(!failure.success);
    assert_eq!(failure.error.as_deref(), Some("main failed"));
    assert!(failure.output.get("job_attention").is_none());
}

#[tokio::test]
async fn initiating_handoff_is_cursor_baseline_then_terminal_is_delivered_once() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("passive-handoff-owner");
    let client = "passive-handoff";
    let project = format!("agent:{client}:repo");
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("passive-handoff-window");
    let job_id =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;

    let mut handoff = ToolResult::ok(json!({
        "execution_state": "pending",
        "continuation": super::super::jobs::observe_job_continuation(&job_id, None),
    }));
    runtime
        .add_passive_job_attention(
            &mut handoff,
            "run_process",
            Some(&project),
            Some(&session),
            Some(&window),
            Some(&auth),
        )
        .await;
    assert!(
        handoff.output.get("job_attention").is_none(),
        "initiating handoff must establish cursor baseline without duplicate active attention"
    );
    assert_eq!(mark_next_agent_job_running(&runtime, client).await, job_id);
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none(),
        "unchanged running execution must stay silent while independent work continues"
    );

    let job = runtime
        .runner_registry
        .get_job_for_auth(Some(&crate::test_support::runner_access(&auth)), &job_id)
        .await
        .unwrap();
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client.into(),
            runner_instance_id: "inst".into(),
            job_id: job_id.clone(),
            request_id: job.request_id,
            update_seq: Some(2),
            status: "completed".into(),
            stdout_chunk: Some("PRIVATE_TERMINAL_LOG".into()),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(42),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();

    let ordinary = attention(&runtime, &project, Some(&session), &window, &auth).await;
    let item = &ordinary.output["job_attention"]["items"][0];
    assert_eq!(item["job_id"], job_id);
    assert_eq!(item["state"], "terminal");
    assert_eq!(item["outcome"], "passed");
    assert_eq!(item["command_ok"], true);
    assert_eq!(item["exit_code"], 0);
    assert_eq!(item["details"]["tool"], "observe_jobs");
    assert!(
        !ordinary.output["job_attention"]
            .to_string()
            .contains("PRIVATE_TERMINAL_LOG"),
        "passive terminal truth must never inline Job logs"
    );
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none(),
        "same terminal transition is delivered at most once per exact cursor key"
    );
    assert!(
        probe_agent_request_for_instance(&runtime, client, "inst")
            .await
            .is_none(),
        "passive observation must never redispatch the execution"
    );
}

#[tokio::test]
async fn passive_attention_projects_process_failure_stop_and_timeout_without_logs() {
    for (suffix, status, exit_code, expected_outcome) in [
        ("nonzero", "completed", Some(7), "failed"),
        ("stopped", "stopped", None, "cancelled"),
        ("timeout", "timeout", None, "timed_out"),
    ] {
        let runtime = ToolRuntime::new_for_tests();
        let auth = shared_key_auth_context(&format!("passive-{suffix}-owner"));
        let client = format!("passive-{suffix}");
        let project = format!("agent:{client}:repo");
        register_job_agent_for_auth(&runtime, &client, "repo", &auth).await;
        let session = runtime
            .sessions
            .start_session(Some(project.clone()), None)
            .session_id;
        let window = ClientWindow::for_test(&format!("passive-{suffix}-window"));
        let job_id =
            start_agent_runtime_job_in_session(&runtime, &client, "repo", Some(&session), &auth)
                .await;
        assert_eq!(mark_next_agent_job_running(&runtime, &client).await, job_id);
        let _ = attention(&runtime, &project, Some(&session), &window, &auth).await;

        let job = runtime
            .runner_registry
            .get_job_for_auth(Some(&crate::test_support::runner_access(&auth)), &job_id)
            .await
            .unwrap();
        runtime
            .runner_registry
            .update_job(RunnerJobUpdateRequest {
                client_id: client.clone(),
                runner_instance_id: "inst".into(),
                job_id: job_id.clone(),
                request_id: job.request_id,
                update_seq: Some(2),
                status: status.into(),
                stdout_chunk: Some(format!("PRIVATE_{suffix}_LOG")),
                stderr_chunk: None,
                log_snapshot: None,
                exit_code,
                duration_ms: Some(25),
                error: None,
                command_execution_state: None,
                validation_progress: None,
                test_count_evidence: None,
                activity: None,
                finished: true,
            })
            .await
            .unwrap();

        let terminal = attention(&runtime, &project, Some(&session), &window, &auth).await;
        let item = &terminal.output["job_attention"]["items"][0];
        assert_eq!(item["state"], "terminal", "{suffix}");
        assert_eq!(item["outcome"], expected_outcome, "{suffix}");
        assert_eq!(item["command_ok"], false, "{suffix}");
        if let Some(exit_code) = exit_code {
            assert_eq!(item["exit_code"], exit_code, "{suffix}");
        } else {
            assert!(item["exit_code"].is_null(), "{suffix}");
        }
        assert!(
            !terminal.output["job_attention"]
                .to_string()
                .contains(&format!("PRIVATE_{suffix}_LOG")),
            "{suffix}"
        );
    }
}

#[tokio::test]
async fn passive_attention_projects_recovering_from_server_record_without_runner_poll() {
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("recovering-owner");
    let client = "recovering-passive";
    let project = format!("agent:{client}:repo");
    register_job_agent_for_auth_with_reconciliation(&runtime, client, "repo", &auth, true).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("recovering-passive-window");
    let job_id =
        start_agent_runtime_job_in_session(&runtime, client, "repo", Some(&session), &auth).await;
    assert_eq!(mark_next_agent_job_running(&runtime, client).await, job_id);
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    runtime
        .runner_registry
        .reconcile_disconnect(client, "inst")
        .await;
    let changed = attention(&runtime, &project, Some(&session), &window, &auth).await;
    assert_eq!(
        changed.output["job_attention"]["items"][0]["status"],
        "recovering"
    );
    assert!(
        attention(&runtime, &project, Some(&session), &window, &auth)
            .await
            .output
            .get("job_attention")
            .is_none()
    );
    assert!(probe_agent_request_for_instance(&runtime, client, "inst")
        .await
        .is_none());
}
