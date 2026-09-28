//! Deterministic transient readiness and local/runtime orchestration harness.
use super::*;
use webcodex_tool_contracts::tool_call::JobReadinessMode::{All, Any};

async fn ready(
    runtime: &ToolRuntime,
    ids: Vec<String>,
    mode: webcodex_tool_contracts::tool_call::JobReadinessMode,
    secs: u64,
    auth: &crate::auth::AuthContext,
) -> ToolResult {
    runtime
        .dispatch_with_auth(
            ToolCall::WaitForJobReadiness {
                job_ids: ids,
                mode,
                wait_secs: secs,
            },
            Some(auth),
        )
        .await
}

#[tokio::test(start_paused = true)]
async fn readiness_already_terminal_sparse_and_stable_dedup() {
    let runtime = test_runtime();
    let (a, ra, auth) = register_and_start_agent_job(&runtime, "ready-a").await;
    let (b, rb, _) = register_and_start_agent_job(&runtime, "ready-b").await;
    for (client, request) in [("ready-a", &ra), ("ready-b", &rb)] {
        update_observed_job(
            &runtime,
            client,
            request,
            "completed",
            Some("SECRET LOG"),
            None,
            true,
        )
        .await;
    }
    let before = tokio::time::Instant::now();
    let result = ready(
        &runtime,
        vec![a.clone(), a.clone(), b.clone(), a.clone()],
        All,
        45,
        &auth,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(tokio::time::Instant::now(), before);
    assert_eq!(
        result.output,
        json!({"wait_state":"ready", "mode":"all", "waited_ms":0,
        "ready":[{"job_id":a,"status":"completed","outcome":"succeeded"},{"job_id":b,"status":"completed","outcome":"succeeded"}],"pending_job_ids":[]})
    );
}

#[tokio::test(start_paused = true)]
async fn readiness_runtime_dogfood_any_consumes_a_before_b_without_redispatch() {
    let runtime = test_runtime();
    let (a, ra, auth) = register_and_start_agent_job(&runtime, "dag-a").await;
    let (b, rb, _) = register_and_start_agent_job(&runtime, "dag-b").await;
    // Independent work completes before the DAG reaches its wait barrier.
    let independent = runtime
        .runner_registry
        .list_jobs_for_auth(None, Some(100))
        .await;
    assert_eq!(independent.len(), 2);
    let wait = ready(
        &runtime,
        vec![a.clone(), a.clone(), b.clone()],
        Any,
        12,
        &auth,
    );
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    tokio::time::advance(Duration::from_millis(7314)).await;
    update_observed_job(&runtime, "dag-a", &ra, "completed", None, None, true).await;
    // Manual polling proves event wake without advancing a heartbeat timer.
    let std::task::Poll::Ready(result) = futures_util::poll!(&mut wait) else {
        panic!("A must unblock without B or timer progress")
    };
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait_state"], "ready");
    assert_eq!(result.output["ready"][0]["job_id"], a);
    assert_eq!(result.output["pending_job_ids"], json!([b]));
    assert_eq!(result.output["waited_ms"], 7314);
    // Consume A-dependent state in the same executor activation.
    let consumed_a = runtime
        .runner_registry
        .job_terminal_registration_snapshot_for_auth(None, &a)
        .await
        .unwrap();
    assert_eq!(consumed_a.terminal_event.unwrap().outcome, "succeeded");
    let remaining = ready(&runtime, vec![a.clone(), b.clone()], All, 12, &auth);
    tokio::pin!(remaining);
    assert!(futures_util::poll!(&mut remaining).is_pending());
    update_observed_job(&runtime, "dag-b", &rb, "completed", None, None, true).await;
    let std::task::Poll::Ready(result) = futures_util::poll!(&mut remaining) else {
        panic!("B event must finish all")
    };
    assert_eq!(result.output["ready"].as_array().unwrap().len(), 2);
    assert_eq!(
        runtime
            .runner_registry
            .list_jobs_for_auth(None, Some(100))
            .await
            .len(),
        2
    );
    println!("readiness dogfood: 2 Jobs, independent work, A consumed at 7314ms while B pending, remaining wait event-woken; 2 logical executions");
}

#[tokio::test(start_paused = true)]
async fn readiness_all_retains_remaining_wait_and_progress_never_resets_deadline() {
    let runtime = test_runtime();
    let (a, ra, auth) = register_and_start_agent_job(&runtime, "deadline-a").await;
    let (b, rb, _) = register_and_start_agent_job(&runtime, "deadline-b").await;
    let wait = ready(&runtime, vec![a.clone(), b.clone()], All, 4, &auth);
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    update_observed_job(&runtime, "deadline-a", &ra, "completed", None, None, true).await;
    assert!(futures_util::poll!(&mut wait).is_pending());
    for _ in 0..3 {
        tokio::time::advance(Duration::from_secs(1)).await;
        update_observed_job(
            &runtime,
            "deadline-b",
            &rb,
            "running",
            Some("progress\n"),
            Some(process_activity()),
            false,
        )
        .await;
        assert!(futures_util::poll!(&mut wait).is_pending());
    }
    tokio::time::advance(Duration::from_secs(1)).await;
    let result = wait.await;
    assert!(result.success);
    assert_eq!(result.output["wait_state"], "deadline");
    assert_eq!(result.output["waited_ms"], 4000);
    assert_eq!(result.output["ready"][0]["job_id"], a);
    assert_eq!(result.output["pending_job_ids"], json!([b]));
}

#[tokio::test(start_paused = true)]
async fn readiness_all_wakes_only_after_second_terminal_event() {
    let runtime = test_runtime();
    let (a, ra, auth) = register_and_start_agent_job(&runtime, "all-ready-a").await;
    let (b, rb, _) = register_and_start_agent_job(&runtime, "all-ready-b").await;
    let wait = ready(&runtime, vec![a, b], All, 12, &auth);
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    update_observed_job(&runtime, "all-ready-a", &ra, "completed", None, None, true).await;
    assert!(futures_util::poll!(&mut wait).is_pending());
    update_observed_job(&runtime, "all-ready-b", &rb, "completed", None, None, true).await;
    let std::task::Poll::Ready(result) = futures_util::poll!(&mut wait) else {
        panic!("both terminal")
    };
    assert_eq!(result.output["wait_state"], "ready");
    assert_eq!(result.output["ready"].as_array().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn readiness_invalid_set_fails_closed_before_any_wait() {
    let runtime = test_runtime();
    let (a, _, auth) = register_and_start_agent_job(&runtime, "denied-ready").await;
    let unknown = ready(
        &runtime,
        vec![a.clone(), "wc_job_missing".into()],
        Any,
        12,
        &auth,
    )
    .await;
    assert!(!unknown.success);
    assert!(unknown.output.is_null());
    let foreign = shared_key_auth_context(&"d".repeat(64));
    let denied = ready(&runtime, vec![a], Any, 12, &foreign).await;
    assert_eq!(denied.success, unknown.success);
    assert_eq!(denied.error, unknown.error);
    assert_eq!(denied.output, unknown.output);
    for ids in [vec![], (0..9).map(|n| format!("job-{n}")).collect()] {
        assert!(!ready(&runtime, ids, Any, 1, &auth).await.success);
    }
    for secs in [0, 46, u64::MAX] {
        assert!(
            !ready(&runtime, vec!["wc_job_missing".into()], Any, secs, &auth)
                .await
                .success
        );
    }
}

#[tokio::test(start_paused = true)]
async fn readiness_failure_statuses_are_terminal_ready() {
    for (status, outcome) in [
        ("failed", "failed"),
        ("stopped", "cancelled"),
        ("lost", "failed"),
        ("timed_out", "timed_out"),
    ] {
        let runtime = test_runtime();
        let (id, request, auth) = register_and_start_agent_job(&runtime, "failed-ready").await;
        update_observed_job(&runtime, "failed-ready", &request, status, None, None, true).await;
        let result = ready(&runtime, vec![id.clone()], Any, 12, &auth).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["wait_state"], "ready");
        assert_eq!(
            result.output["ready"],
            json!([{"job_id":id,"status":status,"outcome":outcome}])
        );
    }
}

#[tokio::test(start_paused = true)]
async fn readiness_cancel_drops_transient_wait_but_job_survives_runtime_recreation() {
    let runtime = test_runtime();
    let (id, request, auth) = register_and_start_agent_job(&runtime, "cancel-ready").await;
    {
        let wait = ready(&runtime, vec![id.clone()], Any, 45, &auth);
        tokio::pin!(wait);
        assert!(futures_util::poll!(&mut wait).is_pending());
    }
    // No terminal-attention DB/controller is configured: transient waits need neither.
    assert!(runtime.job_terminal_db.is_none());
    update_observed_job(
        &runtime,
        "cancel-ready",
        &request,
        "completed",
        None,
        None,
        true,
    )
    .await;
    let registry = runtime.runner_registry.clone();
    drop(runtime);
    let restarted = ToolRuntime::new(registry, Arc::new(RuntimeInfo::default()));
    let result = ready(&restarted, vec![id], Any, 1, &auth).await;
    assert_eq!(result.output["wait_state"], "ready");
    assert_eq!(result.output["waited_ms"], 0);
}

#[tokio::test(start_paused = true)]
async fn readiness_terminal_between_snapshot_and_registration_is_not_lost() {
    let mut runtime = test_runtime();
    let hook = crate::tool_runtime::job_terminal_wait::JobTerminalRegistrationTestHook::new();
    runtime.job_terminal_registration_test_hook = Some(hook.clone());
    let (id, request, auth) = register_and_start_agent_job(&runtime, "register-ready").await;
    let wait = ready(&runtime, vec![id], Any, 45, &auth);
    let transition = async {
        hook.first_snapshot.wait().await;
        update_observed_job(
            &runtime,
            "register-ready",
            &request,
            "completed",
            None,
            None,
            true,
        )
        .await;
        hook.resume_after_terminal.wait().await;
    };
    let (result, ()) = tokio::join!(wait, transition);
    assert_eq!(result.output["wait_state"], "ready");
    assert_eq!(result.output["waited_ms"], 0);
}

#[tokio::test]
async fn readiness_logical_identity_excludes_detached_instance_but_not_owner() {
    let runtime = test_runtime();
    let (id, _, _) = register_and_start_agent_job(&runtime, "identity-ready").await;
    let snapshot = runtime
        .runner_registry
        .job_terminal_registration_snapshot_for_auth(None, &id)
        .await
        .unwrap();
    let mut transferred = snapshot.clone();
    transferred.runner_instance_id = "replacement-detached-instance".into();
    use crate::job_terminal_attention::source_from_snapshot;
    assert_eq!(
        source_from_snapshot(&snapshot),
        source_from_snapshot(&transferred)
    );
    transferred.owner_at_admission = Some("different-owner".into());
    assert_ne!(
        source_from_snapshot(&snapshot),
        source_from_snapshot(&transferred)
    );
}
