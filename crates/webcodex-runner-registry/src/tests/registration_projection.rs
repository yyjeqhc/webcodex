use super::*;

#[tokio::test]
async fn registration_retains_reported_job_concurrency_and_preserves_missing_limit() {
    let registry = RunnerRegistry::default();
    for limit in [1, 4, 8, 64] {
        let client_id = format!("current-limit-{limit}");
        let instance_id = format!("inst-current-{limit}");
        let mut current = runner_registration(&client_id, &instance_id, Vec::new());
        current.job_concurrency_limit = Some(limit);
        let current_view = registry.register(current).await.unwrap();
        assert_eq!(current_view.job_concurrency_limit, Some(limit));
        assert_eq!(
            registry
                .get_runner_view(&client_id)
                .await
                .unwrap()
                .job_concurrency_limit,
            Some(limit)
        );
    }

    let without_limit = registry
        .register(runner_registration(
            "missing-limit",
            "inst-missing-limit",
            Vec::new(),
        ))
        .await
        .unwrap();
    assert_eq!(without_limit.job_concurrency_limit, None);

    for (client_id, limit) in [("invalid-limit-zero", 0), ("invalid-limit-high", 65)] {
        let mut invalid = runner_registration(client_id, "inst-invalid", Vec::new());
        invalid.job_concurrency_limit = Some(limit);
        assert_eq!(
            registry.register(invalid).await.unwrap_err(),
            "job_concurrency_limit must be between 1 and 64"
        );
    }
}

#[tokio::test]
async fn registry_registers_and_lists_client() {
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
            client_id: "xrh".to_string(),
            runner_instance_id: "inst".to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: Some("XRH".to_string()),
            owner: Some("yyjeqhc".to_string()),
            hostname: Some("fineserver".to_string()),
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                RunnerCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    let runners = registry.list_runners().await;
    assert_eq!(runners.len(), 1);
    assert_eq!(runners[0].client_id, "xrh");
    assert!(runners[0].connected);
    assert_eq!(runners[0].pending_requests, 0);
}

#[tokio::test]
async fn hot_job_capacity_is_bounded_and_fenced_by_instance_connection_and_generation() {
    use webcodex_core::runner_protocol::RunnerJobCapacity;
    for transport in [
        RunnerTransport::WebSocket,
        RunnerTransport::Quic,
        RunnerTransport::Polling,
    ] {
        let registry = RunnerRegistry::default();
        let mut request = runner_registration("capacity", "instance", Vec::new());
        request.job_concurrency_limit = Some(2);
        let connection = if transport == RunnerTransport::Polling {
            registry.register(request.clone()).await.unwrap();
            None
        } else {
            registry
                .register_streaming_session(
                    request.clone(),
                    None,
                    "current",
                    transport,
                    Arc::new(Notify::new()),
                )
                .await
                .unwrap();
            Some("current")
        };
        let capacity = |generation, limit| Some(RunnerJobCapacity { generation, limit });
        registry
            .update_job_capacity("capacity", "instance", connection, capacity(2, 6))
            .await
            .unwrap();
        for (instance, conn, generation, limit) in [
            ("old", connection, 3, 32),
            ("instance", Some("old"), 3, 32),
            ("instance", connection, 1, 32),
            ("instance", connection, 2, 32),
            ("instance", connection, 3, 0),
            ("instance", connection, 3, 65),
        ] {
            assert!(registry
                .update_job_capacity("capacity", instance, conn, capacity(generation, limit))
                .await
                .is_err());
            assert_eq!(
                registry
                    .get_runner_view("capacity")
                    .await
                    .unwrap()
                    .job_concurrency_limit,
                Some(6)
            );
        }
        // A delayed poll cannot mutate a same-instance stream lease, even if
        // it claims a higher generation than the current stream has reported.
        if connection.is_some() {
            assert!(registry
                .update_job_capacity("capacity", "instance", None, capacity(3, 32))
                .await
                .is_err());
            assert_eq!(
                registry
                    .get_runner_view("capacity")
                    .await
                    .unwrap()
                    .job_concurrency_limit,
                Some(6)
            );
        }
        // An obsolete registration from this same process must not regress capacity.
        assert!(registry.register(request).await.is_err());
        registry
            .update_job_capacity("capacity", "instance", connection, None)
            .await
            .unwrap();
        registry
            .update_job_capacity("capacity", "instance", connection, capacity(3, 2))
            .await
            .unwrap();
        assert_eq!(
            registry.list_runners().await[0].job_concurrency_limit,
            Some(2)
        );
        registry.reconcile_disconnect("capacity", "instance").await;
        assert!(registry
            .update_job_capacity("capacity", "instance", connection, capacity(4, 32))
            .await
            .is_err());
        assert_eq!(
            registry
                .get_runner_view("capacity")
                .await
                .unwrap()
                .job_concurrency_limit,
            Some(2)
        );
    }
}
