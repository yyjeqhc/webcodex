use super::*;

#[tokio::test]
async fn computer_accessibility_enqueue_requires_distinct_capability() {
    let registry = ShellClientRegistry::default();
    register_computer_test_client(&registry, "computer-ax", "alice", true, false, false, false)
        .await;
    let alice = auth_context(Some("alice"), false);
    let error = registry
        .enqueue_computer(
            "computer-ax".to_string(),
            "computer_accessibility_status",
            "{}".to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_accessibility_observe"));

    register_computer_test_client(&registry, "computer-ax", "alice", true, true, false, false)
        .await;
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-ax".to_string(),
            "computer_accessibility_tree",
            r#"{"surface_id":"surface_test","max_depth":2,"max_nodes":8}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-ax".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued accessibility request");
    assert_eq!(request.kind, "computer_accessibility_tree");
    assert!(request.command.is_empty());
}

#[tokio::test]
async fn computer_element_state_requires_its_own_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-state",
        "alice",
        true,
        true,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-state".to_string(),
            "computer_element_state",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_element_state"));

    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "computer-state-capable".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_accessibility_observe: true,
                computer_element_state: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-state-capable".to_string(),
            "computer_element_state",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-state-capable".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer element-state request");
    assert_eq!(request.kind, "computer_element_state");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
}
