use super::*;

#[tokio::test]
async fn computer_enqueue_requires_exact_owner_and_distinct_capability() {
    let registry = ShellClientRegistry::default();
    register_computer_test_client(
        &registry,
        "computer-owned",
        "alice",
        false,
        false,
        false,
        false,
    )
    .await;
    let alice = auth_context(Some("alice"), false);
    let error = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_list_windows",
            r#"{"limit":1}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_observe"));

    register_computer_test_client(
        &registry,
        "computer-owned",
        "alice",
        true,
        false,
        false,
        false,
    )
    .await;
    let bob = auth_context(Some("bob"), false);
    let error = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_list_windows",
            r#"{"limit":1}"#.to_string(),
            "bob".to_string(),
            Some(&bob),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("owned by alice"));

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_snapshot",
            r#"{"surface_id":"surface_test"}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-owned".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer request");
    assert_eq!(request.kind, "computer_snapshot");
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_snapshot_region_requires_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-region-old",
        "alice",
        true,
        false,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","region":{"x":0,"y":0,"width":10,"height":10},"max_width":null,"max_height":null}"#;
    let error = registry
        .enqueue_computer(
            "computer-region-old".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_snapshot_region"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-region-only".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: false,
                computer_snapshot_region: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let error = registry
        .enqueue_computer(
            "computer-region-only".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_observe"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-region-new".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_snapshot_region: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-region-new".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-region-new".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued region snapshot request");
    assert_eq!(request.kind, "computer_snapshot_region");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
}
