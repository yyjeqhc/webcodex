use super::*;
use crate::state::JobLifecycleState;

async fn register_owned_runner(
    registry: &RunnerRegistry,
    client_id: &str,
    instance: &str,
    owner: &str,
) -> RunnerView {
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some(owner.to_string()),
            ..runner_registration(client_id, instance, Vec::new())
        }))
        .await
        .unwrap()
}

async fn age_past_online_window(registry: &RunnerRegistry, client_id: &str) {
    registry
        .set_last_seen_for_test(client_id, now_ts() - RUNNER_ONLINE_WINDOW_SECS - 1)
        .await;
}

#[tokio::test]
async fn stale_registration_removes_every_server_projection() {
    let registry = RunnerRegistry::default();
    register_owned_runner(&registry, "stale", "stale-instance", "alice").await;
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "stale",
        "stale-instance",
        vec![
            project_summary("enabled-project", "/tmp/enabled"),
            project_summary("second-project", "/tmp/second"),
        ],
    )
    .await;
    {
        let mut inner = registry.inner.lock().await;
        inner.retired_instances.insert(
            "stale".to_string(),
            std::collections::VecDeque::from(["older-instance".to_string()]),
        );
        inner
            .unregistering_projects
            .insert("agent:stale:enabled-project".to_string(), 1);
    }
    age_past_online_window(&registry, "stale").await;

    let alice = auth_context(Some("alice"), false);
    let outcome = registry
        .remove_stale_runner_for_auth(Some(&alice), "stale")
        .await
        .expect("a stale, idle registration is removable");
    assert_eq!(outcome.client_id, "stale");
    assert_eq!(outcome.runner_instance_id, "stale-instance");
    assert_eq!(outcome.projects_removed, 2);

    {
        let inner = registry.inner.lock().await;
        assert!(inner.runners.is_empty());
        assert!(inner.queues_by_runner.is_empty());
        assert!(inner.notifiers.is_empty());
        assert!(inner.retired_instances.is_empty());
        assert!(inner.unregistering_projects.is_empty());
    }
    assert!(registry.list_runners().await.is_empty());
    assert!(registry
        .get_runner_view_for_auth("stale", Some(&alice))
        .await
        .is_none());
    // Absence is the target state: a repeated removal converges instead of
    // failing, so no journal is required for retry safety.
    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&alice), "stale")
            .await,
        Err(RunnerRemovalRefusal::UnknownRunner)
    );
}

#[tokio::test]
async fn removal_fails_closed_for_online_connected_and_foreign_registrations() {
    let registry = RunnerRegistry::default();
    let bootstrap = auth_context(None, true);

    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&bootstrap), "never-registered")
            .await,
        Err(RunnerRemovalRefusal::UnknownRunner)
    );

    register_owned_runner(&registry, "fresh", "fresh-instance", "alice").await;
    assert!(matches!(
        registry
            .remove_stale_runner_for_auth(Some(&bootstrap), "fresh")
            .await,
        Err(RunnerRemovalRefusal::StillOnline { .. })
    ));

    // A live long-lived transport keeps the registration even after the
    // liveness window lapses, because the Server can still address the Runner.
    let connected_auth = shared_key_access("removal-connected");
    registry
        .register_streaming_session(
            runner_registration("connected", "connected-instance", Vec::new()),
            Some(&connected_auth),
            "connected-connection",
            RunnerTransport::WebSocket,
            Arc::new(Notify::new()),
        )
        .await
        .unwrap();
    age_past_online_window(&registry, "connected").await;
    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&connected_auth), "connected")
            .await,
        Err(RunnerRemovalRefusal::Connected)
    );

    // Visibility is concealment: a foreign owner cannot observe, let alone
    // remove, another owner's registration.
    let bob = auth_context(Some("bob"), false);
    age_past_online_window(&registry, "fresh").await;
    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&bob), "fresh")
            .await,
        Err(RunnerRemovalRefusal::UnknownRunner)
    );
    assert!(registry
        .get_runner_view_for_auth("fresh", Some(&auth_context(Some("alice"), false)))
        .await
        .is_some());
}

#[tokio::test]
async fn active_jobs_refuse_removal_and_terminal_history_survives_it() {
    let registry = RunnerRegistry::default();
    register_owned_runner(&registry, "busy", "busy-instance", "alice").await;
    let job = registry
        .start_job(
            shell_job_start_request("busy", "sleep 60", Some(60)),
            "test".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
    age_past_online_window(&registry, "busy").await;
    let alice = auth_context(Some("alice"), false);

    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&alice), "busy")
            .await,
        Err(RunnerRemovalRefusal::ActiveJobs { active: 1 })
    );
    assert!(registry
        .get_runner_view_for_auth("busy", Some(&alice))
        .await
        .is_some());

    {
        let mut inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get_mut(&job.job_id).unwrap();
        record.lifecycle = JobLifecycleState::Completed;
        record.ended_at = Some(now_ts());
        record.exit_code = Some(0);
    }
    // A terminal Job is no longer active, but its dispatch request is still
    // outstanding until the Runner reports the result: the registration stays.
    assert_eq!(
        registry
            .remove_stale_runner_for_auth(Some(&alice), "busy")
            .await,
        Err(RunnerRemovalRefusal::PendingRequests { pending: 1 })
    );

    {
        let mut inner = registry.inner.lock().await;
        let request_id = inner
            .jobs_by_id
            .get(&job.job_id)
            .and_then(|record| record.request_id.clone())
            .expect("an admitted Job keeps its dispatch request id");
        crate::requests::remove_pending_request_locked(&mut inner, &request_id);
    }
    registry
        .remove_stale_runner_for_auth(Some(&alice), "busy")
        .await
        .expect("a reported terminal Job no longer blocks removal");

    let retained = registry
        .get_job_for_auth(Some(&alice), &job.job_id)
        .await
        .expect("terminal Job evidence is retained with its own authorization snapshot");
    assert_eq!(retained.status, "completed");
    assert_eq!(retained.client_id, "busy");
}
