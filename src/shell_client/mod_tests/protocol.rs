use super::*;

#[test]
fn protocol_generation_v2_is_canonical_while_inventory_labels_remain_orthogonal() {
    let labels = [
        (
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            crate::shell_protocol::AgentProjectInventoryStrategy::Inline,
        ),
        (
            AGENT_PROTOCOL_VERSION_POLLING_V2,
            crate::shell_protocol::AgentProjectInventoryStrategy::Paged,
        ),
        (
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            crate::shell_protocol::AgentProjectInventoryStrategy::Inline,
        ),
        (
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V2,
            crate::shell_protocol::AgentProjectInventoryStrategy::Paged,
        ),
        (
            AGENT_PROTOCOL_VERSION_QUIC_V1,
            crate::shell_protocol::AgentProjectInventoryStrategy::Inline,
        ),
        (
            AGENT_PROTOCOL_VERSION_QUIC_V2,
            crate::shell_protocol::AgentProjectInventoryStrategy::Paged,
        ),
    ];

    for (label, inventory) in labels {
        let accepted = AcceptedRunnerProtocol::try_from_registration(
            label,
            Some(AGENT_PROTOCOL_GENERATION_V2),
        )
        .unwrap();
        assert_eq!(accepted.project_inventory(), inventory, "{label}");
    }
}

#[test]
fn missing_or_unsupported_generation_and_unknown_label_fail_closed() {
    let missing =
        AcceptedRunnerProtocol::try_from_registration(AGENT_PROTOCOL_VERSION_POLLING_V1, None)
            .unwrap_err();
    assert_eq!(missing, "agent_protocol_generation is required");

    for raw in [0, 1, 3, u16::MAX] {
        let error = AcceptedRunnerProtocol::try_from_registration(
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            Some(AgentProtocolGenerationNumber::new(raw)),
        )
        .unwrap_err();
        assert_eq!(error, "agent_protocol_generation is unsupported");
    }

    let error = AcceptedRunnerProtocol::try_from_registration(
        "future-v2",
        Some(AGENT_PROTOCOL_GENERATION_V2),
    )
    .unwrap_err();
    assert_eq!(error, "agent_protocol_version is unsupported");
}

fn generation_registration(
    client_id: &str,
    instance_id: &str,
    generation: Option<AgentProtocolGenerationNumber>,
) -> ShellClientRegisterRequest {
    let mut registration = runner_registration(client_id, instance_id, Vec::new());
    let mut capabilities = v2_baseline_capabilities();
    capabilities.agent_protocol_generation = generation;
    registration.capabilities = Some(capabilities);
    registration
}

#[tokio::test]
async fn registration_rejects_non_v2_generation_before_creating_a_record() {
    for (suffix, generation, expected) in [
        ("missing", None, "agent_protocol_generation is required"),
        (
            "generation-one",
            Some(AgentProtocolGenerationNumber::new(1)),
            "agent_protocol_generation is unsupported",
        ),
        (
            "unknown",
            Some(AgentProtocolGenerationNumber::new(3)),
            "agent_protocol_generation is unsupported",
        ),
    ] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("non-v2-{suffix}");
        let error = registry
            .register(generation_registration(&client_id, "inst-a", generation))
            .await
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(registry.get_client_view(&client_id).await.is_none());
    }
}

#[tokio::test]
async fn registration_rejects_v2_baseline_contradiction_before_creating_a_record() {
    let registry = ShellClientRegistry::default();
    let mut registration = generation_registration(
        "v2-contradiction",
        "inst-a",
        Some(AGENT_PROTOCOL_GENERATION_V2),
    );
    registration
        .capabilities
        .as_mut()
        .unwrap()
        .structured_process_argv = false;

    let error = registry.register(registration).await.unwrap_err();
    assert_eq!(
        error,
        "runner generation baseline capability mismatch: structured_process_argv"
    );
    assert!(registry.get_client_view("v2-contradiction").await.is_none());
}

#[tokio::test]
async fn same_instance_generation_v2_reconnects_remain_valid() {
    let registry = ShellClientRegistry::default();
    registry
        .register(generation_registration(
            "v2-stable",
            "inst-a",
            Some(AGENT_PROTOCOL_GENERATION_V2),
        ))
        .await
        .unwrap();
    registry
        .register(generation_registration(
            "v2-stable",
            "inst-a",
            Some(AGENT_PROTOCOL_GENERATION_V2),
        ))
        .await
        .unwrap();

    let inner = registry.inner.lock().await;
    assert_eq!(inner.clients["v2-stable"].agent_instance_id, "inst-a");
}

#[tokio::test]
async fn same_instance_inventory_strategy_change_remains_valid() {
    let registry = ShellClientRegistry::default();
    let mut inline = generation_registration(
        "inventory-change",
        "inst-a",
        Some(AGENT_PROTOCOL_GENERATION_V2),
    );
    inline.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V1.to_string());
    registry.register(inline).await.unwrap();

    let mut paged = generation_registration(
        "inventory-change",
        "inst-a",
        Some(AGENT_PROTOCOL_GENERATION_V2),
    );
    paged.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V2.to_string());
    registry.register(paged).await.unwrap();

    let inner = registry.inner.lock().await;
    let record = inner.clients.get("inventory-change").unwrap();
    assert_eq!(
        record.accepted_protocol.project_inventory(),
        crate::shell_protocol::AgentProjectInventoryStrategy::Paged
    );
}

#[tokio::test]
async fn replacement_cannot_bypass_generation_v2_admission() {
    let registry = ShellClientRegistry::default();
    registry
        .register(generation_registration(
            "replacement-v2",
            "inst-a",
            Some(AGENT_PROTOCOL_GENERATION_V2),
        ))
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("replacement-v2", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;

    let error = registry
        .register(generation_registration("replacement-v2", "inst-b", None))
        .await
        .unwrap_err();
    assert_eq!(error, "agent_protocol_generation is required");
    assert_eq!(
        registry
            .get_client_view("replacement-v2")
            .await
            .unwrap()
            .agent_instance_id,
        "inst-a"
    );

    registry
        .register(generation_registration(
            "replacement-v2",
            "inst-b",
            Some(AGENT_PROTOCOL_GENERATION_V2),
        ))
        .await
        .unwrap();
    assert_eq!(
        registry
            .get_client_view("replacement-v2")
            .await
            .unwrap()
            .agent_instance_id,
        "inst-b"
    );
}

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
            "agent_protocol_version": "polling-v1",
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
            "agent_protocol_version": "polling-v1",
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
fn protocol_serde_preserves_missing_version_for_centralized_validation() {
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
    // Missing wire data is preserved as None so the shared registration
    // validator can return the same stable error across every transport.
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
async fn missing_generation_is_rejected_across_all_runner_transports() {
    for (protocol, transport, transport_label) in [
        (
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            AgentTransport::Polling,
            TRANSPORT_POLLING,
        ),
        (
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            AgentTransport::WebSocket,
            TRANSPORT_WEBSOCKET,
        ),
        (
            AGENT_PROTOCOL_VERSION_QUIC_V1,
            AgentTransport::Quic,
            TRANSPORT_QUIC,
        ),
    ] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("missing-generation-{transport_label}");
        let mut registration = generation_registration(&client_id, "inst-a", None);
        registration.agent_protocol_version = Some(protocol.to_string());
        let error = match transport {
            AgentTransport::Polling => registry.register(registration).await.unwrap_err(),
            AgentTransport::WebSocket | AgentTransport::Quic => registry
                .register_streaming_session(
                    registration,
                    None,
                    &format!("connection-{transport_label}"),
                    transport,
                    Arc::new(Notify::new()),
                )
                .await
                .unwrap_err(),
        };
        assert_eq!(error, "agent_protocol_generation is required");
        assert!(registry.get_client_view(&client_id).await.is_none());
    }
}

#[tokio::test]
async fn register_without_protocol_version_is_rejected() {
    let registry = ShellClientRegistry::default();
    let mut registration = runner_registration("oe", "inst", Vec::new());
    registration.agent_protocol_version = None;
    let error = registry.register(registration).await.unwrap_err();
    assert_eq!(error, "agent_protocol_version is required");
    assert!(registry.list_clients().await.is_empty());
}

#[tokio::test]
async fn polling_http_register_requires_explicit_protocol_version() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(ShellClientRegistry::default());
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/register").post(shell_agent_register)),
    );
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&json!({
            "client_id": "polling-missing-protocol",
            "agent_instance_id": "inst"
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::BAD_REQUEST
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "agent_protocol_version is required");
    assert!(registry.list_clients().await.is_empty());
}

#[tokio::test]
async fn polling_http_register_accepts_supported_paged_protocol_label() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(ShellClientRegistry::default());
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/register").post(shell_agent_register)),
    );
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&json!({
            "client_id": "polling-supported-protocol",
            "agent_instance_id": "inst",
            "agent_protocol_version": AGENT_PROTOCOL_VERSION_POLLING_V2,
            "capabilities": crate::test_support::current_runner_capabilities(ShellClientCapabilities::default()),
            "projects": []
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::OK
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], true);

    let view = registry
        .get_client_view("polling-supported-protocol")
        .await
        .expect("supported polling registration");
    assert_eq!(
        view.agent_protocol_version,
        AGENT_PROTOCOL_VERSION_POLLING_V2
    );
    assert_eq!(view.transport, TRANSPORT_POLLING);
    assert_eq!(
        view.agent_protocol_semantics.compatibility,
        crate::shell_protocol::AgentProtocolCompatibility::V1
    );
    assert_eq!(
        view.agent_protocol_semantics.project_inventory,
        crate::shell_protocol::AgentProjectInventoryStrategy::Paged
    );
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
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            )),
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
async fn register_blank_protocol_version_is_rejected() {
    for (client_id, version) in [("empty", ""), ("whitespace", "   ")] {
        let registry = ShellClientRegistry::default();
        let mut registration = runner_registration(client_id, "inst", Vec::new());
        registration.agent_protocol_version = Some(version.to_string());
        let error = registry.register(registration).await.unwrap_err();
        assert_eq!(error, "agent_protocol_version is required");
        assert!(registry.list_clients().await.is_empty());
    }
}

#[tokio::test]
async fn register_protocol_version_bounds_are_enforced() {
    let cases = [
        (
            "oversized",
            "x".repeat(65),
            "agent_protocol_version is too long; maximum is 64 bytes",
        ),
        (
            "nul",
            "polling-v1\0".to_string(),
            "agent_protocol_version cannot contain control characters",
        ),
        (
            "control",
            "polling-v1\n".to_string(),
            "agent_protocol_version cannot contain control characters",
        ),
    ];
    for (client_id, version, expected_error) in cases {
        let registry = ShellClientRegistry::default();
        let mut registration = runner_registration(client_id, "inst", Vec::new());
        registration.agent_protocol_version = Some(version);
        let error = registry.register(registration).await.unwrap_err();
        assert_eq!(error, expected_error);
        assert!(registry.list_clients().await.is_empty());
    }
}

#[tokio::test]
async fn register_unknown_protocol_versions_are_rejected_without_suffix_guessing() {
    for (client_id, protocol) in [
        ("future", "future-v2"),
        ("websocket-next", "websocket-next"),
        ("quic-next", "quic-next"),
        ("random", "totally-random"),
    ] {
        let registry = ShellClientRegistry::default();
        let mut registration = runner_registration(client_id, "inst", Vec::new());
        registration.agent_protocol_version = Some(protocol.to_string());
        let error = registry.register(registration).await.unwrap_err();
        assert_eq!(error, "agent_protocol_version is unsupported");
        assert!(registry.list_clients().await.is_empty());
    }
}

#[tokio::test]
async fn polling_http_register_rejects_unknown_protocol_version() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(ShellClientRegistry::default());
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/register").post(shell_agent_register)),
    );
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&json!({
            "client_id": "polling-unknown-protocol",
            "agent_instance_id": "inst",
            "agent_protocol_version": "polling-v3"
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::BAD_REQUEST
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "agent_protocol_version is unsupported");
    assert!(registry.list_clients().await.is_empty());
}

#[tokio::test]
async fn client_supports_reflects_registered_capabilities() {
    let registry = ShellClientRegistry::default();
    let caps = crate::test_support::current_runner_capabilities(ShellClientCapabilities {
        shell: true,
        file_read: true,
        async_shell_jobs: true,
        project_path_registration: true,
        structured_go_test_json: true,
        structured_go_test_tool: true,
        structured_go_test_packages: true,
        ..Default::default()
    });
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
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
    let err = registry.get_client_feature_set("ghost").await.unwrap_err();
    assert_eq!(
        err,
        ShellClientLookupError::UnknownClient {
            client_id: "ghost".to_string()
        }
    );
}

#[tokio::test]
async fn coding_agent_run_lookup_is_exact_when_bound_and_ambiguous_when_unbound() {
    let registry = ShellClientRegistry::default();
    let run_id = "wc_agent_run_duplicate_123";
    for client_id in ["a", "b"] {
        let provider_instance_id = format!("provider_{client_id}");
        registry
            .register(ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: Some(vec![
                    webcodex_core::coding_agent::CodingAgentProvider {
                        provider_id: "codex".to_string(),
                        provider_instance_id: provider_instance_id.clone(),
                        name: "Codex".to_string(),
                    },
                ]),
                coding_agent_inventory: Some(
                    webcodex_core::coding_agent::CodingAgentRunInventory {
                        runs: vec![webcodex_core::coding_agent::CodingAgentRunSnapshot {
                            run_id: run_id.to_string(),
                            intent_fingerprint: "fingerprint".to_string(),
                            authority_fingerprint: "auth_test".to_string(),
                            runtime_project_id: format!("agent:{client_id}:demo"),
                            provider_id: "codex".to_string(),
                            provider_instance_id,
                            state: webcodex_core::coding_agent::CodingAgentRunState::Running,
                            execution_state:
                                webcodex_core::coding_agent::CodingAgentExecutionState::Started,
                            observation_revision: 1,
                            created_at: 1,
                            updated_at: 1,
                            terminal: None,
                        }],
                    },
                ),
                client_id: client_id.to_string(),
                agent_instance_id: format!("inst_{client_id}"),
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: Some({
                    let mut capabilities = v2_baseline_capabilities();
                    capabilities.coding_agent_runs = true;
                    capabilities
                }),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
    }

    assert!(registry
        .coding_agent_run_for_auth(None, run_id)
        .await
        .is_none());
    let (client, run) = registry
        .coding_agent_run_for_client_for_auth(None, "b", run_id)
        .await
        .expect("exact bound client lookup");
    assert_eq!(client.client_id, "b");
    assert_eq!(run.runtime_project_id, "agent:b:demo");
}

#[tokio::test]
async fn coding_agent_registration_rejects_semantically_contradictory_snapshot() {
    let registry = ShellClientRegistry::default();
    let register =
        |run: webcodex_core::coding_agent::CodingAgentRunSnapshot| ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: Some(vec![webcodex_core::coding_agent::CodingAgentProvider {
                provider_id: "codex".to_string(),
                provider_instance_id: "provider_test".to_string(),
                name: "Codex".to_string(),
            }]),
            coding_agent_inventory: Some(webcodex_core::coding_agent::CodingAgentRunInventory {
                runs: vec![run],
            }),
            client_id: "test".to_string(),
            agent_instance_id: "inst_test".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some({
                let mut capabilities = v2_baseline_capabilities();
                capabilities.coding_agent_runs = true;
                capabilities
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        };
    let base = webcodex_core::coding_agent::CodingAgentRunSnapshot {
        run_id: "wc_agent_run_registration_semantic".to_string(),
        intent_fingerprint: "fingerprint".to_string(),
        authority_fingerprint: "auth_test".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        provider_id: "codex".to_string(),
        provider_instance_id: "provider_test".to_string(),
        state: webcodex_core::coding_agent::CodingAgentRunState::Running,
        execution_state: webcodex_core::coding_agent::CodingAgentExecutionState::Started,
        observation_revision: 1,
        created_at: 1,
        updated_at: 1,
        terminal: None,
    };
    registry.register(register(base.clone())).await.unwrap();

    let mut completed_with_refusal = base.clone();
    completed_with_refusal.run_id = "wc_agent_run_registration_bad1".to_string();
    completed_with_refusal.state = webcodex_core::coding_agent::CodingAgentRunState::Completed;
    completed_with_refusal.execution_state =
        webcodex_core::coding_agent::CodingAgentExecutionState::Completed;
    completed_with_refusal.terminal = Some(webcodex_core::coding_agent::CodingAgentTerminal {
        stop_reason: Some("refusal".to_string()),
        error_code: Some("refusal".to_string()),
        message: None,
        completed_at: 1,
    });
    let error = registry
        .register(register(completed_with_refusal))
        .await
        .unwrap_err();
    assert!(
        error.contains("invalid coding-agent Run snapshot"),
        "{error}"
    );

    let mut unknown_stop = base;
    unknown_stop.run_id = "wc_agent_run_registration_bad2".to_string();
    unknown_stop.state = webcodex_core::coding_agent::CodingAgentRunState::Failed;
    unknown_stop.execution_state =
        webcodex_core::coding_agent::CodingAgentExecutionState::Completed;
    unknown_stop.terminal = Some(webcodex_core::coding_agent::CodingAgentTerminal {
        stop_reason: Some("future_stop_reason".to_string()),
        error_code: Some("future_stop_reason".to_string()),
        message: None,
        completed_at: 1,
    });
    let error = registry.register(register(unknown_stop)).await.unwrap_err();
    assert!(
        error.contains("invalid coding-agent Run snapshot"),
        "{error}"
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
            coding_agent_providers: Some(vec![webcodex_core::coding_agent::CodingAgentProvider {
                provider_id: "codex".to_string(),
                provider_instance_id: "provider_all".to_string(),
                name: "Codex".to_string(),
            }]),
            coding_agent_inventory: Some(
                webcodex_core::coding_agent::CodingAgentRunInventory::default(),
            ),
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
                apply_text_edit_occurrence: true,
                apply_text_edit_line_scope: true,
                git: true,
                jobs: true,
                async_jobs: true,
                async_shell_jobs: true,
                ssh_shell: true,
                persistent_shell: true,
                ssh_persistent_shell: true,
                structured_validation_argv: true,
                structured_cargo_test_count_assertion: true,
                structured_go_test_json: true,
                structured_go_test_tool: true,
                structured_go_test_packages: true,
                structured_process_argv: true,
                structured_script_payload: true,
                internal_posix_script: true,
                structured_execution_jobs: true,
                detached_process_jobs: true,
                lsp_read_only_navigation: true,
                lsp_call_hierarchy: true,
                project_lifecycle: true,
                project_path_registration: true,
                skill_store_read: true,
                skill_store_manage: true,
                computer_observe: true,
                computer_application_discovery: true,
                computer_application_launch: true,
                computer_display_observe: true,
                computer_pointer_control: true,
                computer_clipboard_read: true,
                computer_clipboard_write: true,
                computer_snapshot_region: true,
                computer_accessibility_observe: true,
                computer_element_state: true,
                computer_control: true,
                computer_scroll_to_element: true,
                computer_key_input: true,
                computer_window_activate: true,
                computer_text_input: true,
                job_state_reconciliation: true,
                coding_agent_runs: true,
                agent_protocol_generation: Some(AGENT_PROTOCOL_GENERATION_V2),
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
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
