use super::*;

#[tokio::test]
async fn registration_retains_reported_job_concurrency_and_preserves_missing_limit() {
    let registry = ShellClientRegistry::default();
    for limit in [1, 4, 8, 64] {
        let client_id = format!("current-limit-{limit}");
        let instance_id = format!("inst-current-{limit}");
        let mut current = runner_registration(&client_id, &instance_id, Vec::new());
        current.job_concurrency_limit = Some(limit);
        let current_view = registry.register(current).await.unwrap();
        assert_eq!(current_view.job_concurrency_limit, Some(limit));
        assert_eq!(
            registry
                .get_client_view(&client_id)
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
    let registry = ShellClientRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: Some("XRH".to_string()),
            owner: Some("yyjeqhc".to_string()),
            hostname: Some("fineserver".to_string()),
            host_context: None,
            capabilities: None,
            policy: None,
        }))
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].client_id, "xrh");
    assert!(clients[0].connected);
    assert_eq!(clients[0].pending_requests, 0);
}
