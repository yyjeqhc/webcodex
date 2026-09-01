use super::*;

#[tokio::test]
async fn computer_snapshot_artifact_rechecks_current_target_project_and_authority_under_registry_lock(
) {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    let request = |client_id: &str| {
        ShellFileOpRequest {
        op: "save_project_artifact".to_string(),
        client_id: client_id.to_string(),
        path: "artifacts/ui.jpg".to_string(),
        cwd: Some("/tmp/project".to_string()),
        content: Some(
            r#"{"path":"artifacts/ui.jpg","content_base64":"/9j/4A==","mime_type":"image/jpeg","overwrite":false,"max_bytes":1048576}"#
                .to_string(),
        ),
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 60,
    }
    };
    let register = |client_id: &str, instance_id: &str| ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: instance_id.to_string(),
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        host_context: None,
        capabilities: Some(crate::test_support::current_runner_capabilities(
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )),
        policy: None,
    };

    registry
        .register(register("computer-artifact-write", "artifact-inst"))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-write",
        "artifact-inst",
        vec![project_summary("demo", "/tmp/project")],
    )
    .await;
    let (request_id, _rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-write"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    let queued = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-write".to_string(),
            agent_instance_id: "artifact-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("snapshot artifact request should be queued");
    assert_eq!(queued.request_id, request_id);
    assert_eq!(queued.kind, "file_save_project_artifact");
    assert_eq!(queued.path.as_deref(), Some("artifacts/ui.jpg"));
    assert_eq!(queued.cwd.as_deref(), Some("/tmp/project"));
    assert!(queued.command.is_empty());

    registry
        .register(register(
            "computer-artifact-poll-change",
            "artifact-poll-inst",
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-poll-change",
        "artifact-poll-inst",
        vec![project_summary("demo", "/tmp/project")],
    )
    .await;
    let (_request_id, response_rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-poll-change"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    // Authoritative inventory may change before dispatch. The pending placement
    // fence is checked against the new snapshot before dispatched=true.
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-poll-change",
        "artifact-poll-inst",
        vec![project_summary("demo", "/tmp/replaced")],
    )
    .await;
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-poll-change".to_string(),
            agent_instance_id: "artifact-poll-inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), response_rx)
        .await
        .expect("stale placement response timed out")
        .expect("stale placement response channel closed");
    assert!(!response.success);
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale_project"),
        "{response:?}"
    );

    registry
        .register(register(
            "computer-artifact-owner-change",
            "artifact-owner-inst",
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-owner-change",
        "artifact-owner-inst",
        vec![project_summary("demo", "/tmp/project")],
    )
    .await;
    let (_request_id, response_rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-owner-change"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    let mut changed_owner = register("computer-artifact-owner-change", "artifact-owner-inst");
    changed_owner.owner = Some("bob".to_string());
    registry.register(changed_owner).await.unwrap();
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-owner-change".to_string(),
            agent_instance_id: "artifact-owner-inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), response_rx)
        .await
        .expect("owner change response timed out")
        .expect("owner change response channel closed");
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale_authority"),
        "{response:?}"
    );

    registry
        .register(register(
            "computer-artifact-replaced",
            "artifact-replaced-inst",
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-replaced",
        "artifact-replaced-inst",
        vec![project_summary("demo", "/tmp/project")],
    )
    .await;
    // Model a placement captured before a reconnect: by admission time the same
    // Runner identity reports the project at a different path.
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "computer-artifact-replaced",
        "artifact-replaced-inst",
        vec![project_summary("demo", "/tmp/replaced")],
    )
    .await;
    let error = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-replaced"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap_err();
    assert!(error.contains("stale_project"), "{error}");
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-replaced".to_string(),
            agent_instance_id: "artifact-replaced-inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}
