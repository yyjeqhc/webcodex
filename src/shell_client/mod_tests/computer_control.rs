use super::*;

#[tokio::test]
async fn computer_control_enqueue_requires_independent_capability() {
    // CU-AX2 control remains independently fenced from CU-AX3 text input.
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test","action":"focus"}"#;
    let error = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_control"));

    register_computer_test_client(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-control".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer control request");
    assert_eq!(request.kind, "computer_control");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_scroll_to_element_requires_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-scroll-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-scroll-control-only".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_scroll_to_element"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-scroll-capable".to_string(),
            agent_instance_id: "computer-scroll-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                computer_control: true,
                computer_scroll_to_element: true,
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
            "computer-scroll-capable".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-scroll-capable".to_string(),
            agent_instance_id: "computer-scroll-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer scroll request");
    assert_eq!(request.kind, "computer_scroll_to_element");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_key_input_requires_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-key-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","key":"tab","modifiers":["shift"]}"#;
    let error = registry
        .enqueue_computer(
            "computer-key-control-only".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_key_input"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-key-capable".to_string(),
            agent_instance_id: "computer-key-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                computer_control: true,
                computer_key_input: true,
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
            "computer-key-capable".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-key-capable".to_string(),
            agent_instance_id: "computer-key-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer key-input request");
    assert_eq!(request.kind, "computer_key_input");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_window_activation_requires_its_own_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-activate",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_window_activate"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-activate".to_string(),
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
                computer_control: true,
                computer_window_activate: true,
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
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-activate".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer window activation request");
    assert_eq!(request.kind, "computer_activate_window");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}
