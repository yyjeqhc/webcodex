use super::*;

#[tokio::test]
async fn global_maintenance_is_atomic_with_runtime_call_admission_and_not_stolen_on_expiry() {
    let registry = RunnerRegistry::default();
    let bootstrap = auth_context(None, true);
    let call = registry.admit_runtime_call().await.unwrap();
    assert!(registry
        .begin_maintenance(
            MaintenanceScope::AllRuntimes,
            "bootstrap",
            &"a".repeat(64),
            &bootstrap
        )
        .await
        .unwrap_err()
        .contains("active"));
    drop(call);
    let grant = registry
        .begin_maintenance(
            MaintenanceScope::AllRuntimes,
            "bootstrap",
            &"a".repeat(64),
            &bootstrap,
        )
        .await
        .unwrap();
    assert_eq!(
        registry
            .begin_maintenance(
                MaintenanceScope::AllRuntimes,
                "bootstrap",
                &"a".repeat(64),
                &bootstrap
            )
            .await
            .unwrap()
            .token(),
        grant.token()
    );
    assert!(registry.admit_runtime_call().await.is_err());
    {
        let mut inner = registry.inner.lock().await;
        inner.maintenance.as_mut().unwrap().expires = std::time::Instant::now();
    }
    assert!(registry
        .renew_maintenance("bootstrap", grant.token())
        .await
        .is_err());
    assert!(registry
        .begin_maintenance(
            MaintenanceScope::AllRuntimes,
            "another",
            &"b".repeat(64),
            &bootstrap
        )
        .await
        .is_err());
    assert!(registry.admit_runtime_call().await.is_err());
    assert!(registry
        .end_maintenance("another", grant.token())
        .await
        .is_err());
    registry
        .end_maintenance("bootstrap", grant.token())
        .await
        .unwrap();
    assert!(registry.admit_runtime_call().await.is_ok());
}

#[tokio::test]
async fn runner_maintenance_blocks_enqueue_and_requires_exact_owner() {
    let registry = RunnerRegistry::default();
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            computer_session_availability: None,
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "target".into(),
            runner_instance_id: "first".into(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".into()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                RunnerCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    assert!(registry
        .begin_maintenance(
            MaintenanceScope::Runner("target".into()),
            "bob-key",
            &"b".repeat(64),
            &auth_context(Some("bob"), false)
        )
        .await
        .is_err());
    let grant = registry
        .begin_maintenance(
            MaintenanceScope::Runner("target".into()),
            "alice-key",
            &"a".repeat(64),
            &auth_context(Some("alice"), false),
        )
        .await
        .unwrap();
    let request = ShellRunRequest {
        login: false,
        client_id: "target".into(),
        cwd: None,
        command: "echo hi".into(),
        stdin: None,
        timeout_secs: 5,
        wait_timeout_secs: 0,
    };
    let error = registry
        .enqueue_run(request.clone(), "tester".into())
        .await
        .unwrap_err();
    assert!(error.contains("maintenance"), "{error}");
    registry
        .end_maintenance("alice-key", grant.token())
        .await
        .unwrap();
    registry
        .enqueue_run(request, "tester".into())
        .await
        .unwrap();
    assert!(registry
        .begin_maintenance(
            MaintenanceScope::Runner("target".into()),
            "alice-key",
            &"a".repeat(64),
            &auth_context(Some("alice"), false)
        )
        .await
        .unwrap_err()
        .contains("active"));
}
