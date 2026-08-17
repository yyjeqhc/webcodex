use super::*;

#[test]
fn protocol_async_capability_defaults_false() {
    let capabilities = ShellClientCapabilities::default();
    assert!(!capabilities.async_jobs);
    assert!(!capabilities.async_shell_jobs);
    assert!(!capabilities.structured_validation_argv);
    assert!(!capabilities.structured_go_test_json);
    assert!(!capabilities.structured_go_test_tool);
    assert!(!capabilities.structured_go_test_packages);

    let request: ShellClientRegisterRequest = serde_json::from_str(
        r#"{
            "client_id": "oe",
            "agent_instance_id": "inst-1",
            "capabilities": {"shell": true}
        }"#,
    )
    .unwrap();
    let capabilities = request.capabilities.unwrap();
    assert!(!capabilities.async_jobs);
    assert!(!capabilities.async_shell_jobs);
    assert!(!capabilities.structured_validation_argv);
    assert!(!capabilities.structured_go_test_json);
    assert!(!capabilities.structured_go_test_tool);
    assert!(!capabilities.structured_go_test_packages);

    let old_go_runner: ShellClientRegisterRequest = serde_json::from_str(
        r#"{
            "client_id": "oe",
            "agent_instance_id": "inst-old-go",
            "capabilities": {"shell": true, "structured_go_test_json": true}
        }"#,
    )
    .unwrap();
    let capabilities = old_go_runner.capabilities.unwrap();
    assert!(capabilities.structured_go_test_json);
    assert!(!capabilities.structured_go_test_tool);
    assert!(!capabilities.structured_go_test_packages);
    let serialized = serde_json::to_string(&capabilities).unwrap();
    assert!(!serialized.contains("structured_go_test_tool"));
    assert!(!serialized.contains("structured_go_test_packages"));
}

#[test]
fn protocol_serde_keeps_old_register_compatible() {
    let request: ShellClientRegisterRequest = serde_json::from_str(
        r#"{
            "client_id": "oe",
            "agent_instance_id": "inst-1",
            "capabilities": {"shell": true, "file_read": true}
        }"#,
    )
    .unwrap();
    assert_eq!(request.client_id, "oe");
    assert!(request.projects.is_none());
    // Old agents omit agent_protocol_version; the field deserializes as None.
    assert!(request.agent_protocol_version.is_none());
}

#[test]
fn protocol_serde_parses_agent_protocol_version() {
    let request: ShellClientRegisterRequest = serde_json::from_str(
        r#"{
            "client_id": "oe",
            "agent_instance_id": "inst-1",
            "agent_protocol_version": "polling-v1"
        }"#,
    )
    .unwrap();
    assert_eq!(
        request.agent_protocol_version.as_deref(),
        Some("polling-v1")
    );
}

#[tokio::test]
async fn register_without_protocol_version_defaults_to_unknown() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients[0].agent_protocol_version, "unknown");
}

#[tokio::test]
async fn register_with_protocol_version_is_exposed_in_view() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].client_id, "xrh");
    assert_eq!(clients[0].agent_protocol_version, "polling-v1");
    let view = registry.get_client_view("xrh").await.unwrap();
    assert_eq!(view.agent_protocol_version, "polling-v1");
}

#[tokio::test]
async fn register_blank_protocol_version_falls_back_to_unknown() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("   ".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients[0].agent_protocol_version, "unknown");
}

#[tokio::test]
async fn client_supports_reflects_registered_capabilities() {
    let registry = ShellClientRegistry::default();
    let caps = ShellClientCapabilities {
        shell: true,
        file_read: true,
        async_shell_jobs: true,
        project_path_registration: true,
        structured_go_test_json: true,
        structured_go_test_tool: true,
        structured_go_test_packages: true,
        ..Default::default()
    };
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_SHELL)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_FILE_READ)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL)
        .await
        .unwrap());
    assert!(registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES)
        .await
        .unwrap());
    let view = registry.get_client_view("oe").await.unwrap();
    assert!(view.capabilities.structured_go_test_json);
    assert!(view.capabilities.structured_go_test_tool);
    assert!(view.capabilities.structured_go_test_packages);
    assert!(!registry
        .client_supports("oe", SHELL_CLIENT_CAPABILITY_GIT)
        .await
        .unwrap());
    // Unknown capability name is false, not an error.
    assert!(!registry.client_supports("oe", "teleport").await.unwrap());
    // Unknown client is a structured error.
    let err = registry
        .client_supports("ghost", SHELL_CLIENT_CAPABILITY_SHELL)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ShellClientLookupError::UnknownClient {
            client_id: "ghost".to_string()
        }
    );
    let err = registry.get_client_capabilities("ghost").await.unwrap_err();
    assert_eq!(
        err,
        ShellClientLookupError::UnknownClient {
            client_id: "ghost".to_string()
        }
    );
}

#[tokio::test]
async fn client_supports_recognizes_all_protocol_capability_names() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: Some(crate::shell_protocol::ShellJobInventory {
                active_complete: true,
                jobs: Vec::new(),
            }),
            client_id: "all".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write: true,
                artifact_export_chunk_read: true,
                artifact_export_streaming_metadata: true,
                structured_file_delete: true,
                git: true,
                jobs: true,
                async_jobs: true,
                async_shell_jobs: true,
                ssh_shell: true,
                persistent_shell: true,
                ssh_persistent_shell: true,
                structured_validation_argv: true,
                structured_go_test_json: true,
                structured_go_test_tool: true,
                structured_go_test_packages: true,
                structured_process_argv: true,
                structured_script_payload: true,
                internal_posix_script: true,
                structured_execution_jobs: true,
                lsp_read_only_navigation: true,
                lsp_call_hierarchy: true,
                sandbox_inspect_commands: true,
                project_lifecycle: true,
                project_path_registration: true,
                computer_observe: true,
                computer_snapshot_region: true,
                computer_accessibility_observe: true,
                computer_element_state: true,
                computer_control: true,
                computer_scroll_to_element: true,
                computer_key_input: true,
                computer_window_activate: true,
                computer_text_input: true,
                job_state_reconciliation: true,
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    for capability in SHELL_CLIENT_CAPABILITY_NAMES {
        assert!(
            registry.client_supports("all", capability).await.unwrap(),
            "shell client matcher must recognize protocol capability {capability}"
        );
    }
}
