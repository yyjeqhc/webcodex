use super::*;

#[test]
fn accepted_runner_protocol_requires_exact_generation_two() {
    let accepted = AcceptedRunnerProtocol::try_from_registration(AGENT_PROTOCOL_GENERATION_V2)
        .expect("generation 2");
    assert_eq!(accepted.generation(), AGENT_PROTOCOL_GENERATION_V2);
    for raw in [0, 1, 3, u16::MAX] {
        assert_eq!(
            AcceptedRunnerProtocol::try_from_registration(AgentProtocolGenerationNumber::new(raw))
                .unwrap_err(),
            "agent_protocol_generation is unsupported"
        );
    }
}

fn generation_registration(
    client_id: &str,
    instance_id: &str,
    generation: AgentProtocolGenerationNumber,
) -> ShellClientRegisterRequest {
    let mut registration = runner_registration(client_id, instance_id, Vec::new());
    registration.agent_protocol_generation = generation;
    registration.capabilities = v2_baseline_capabilities();
    registration
}

#[tokio::test]
async fn registration_rejects_non_v2_generation_before_creating_a_record() {
    for (suffix, generation) in [
        ("generation-one", AgentProtocolGenerationNumber::new(1)),
        ("future", AgentProtocolGenerationNumber::new(3)),
    ] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("non-v2-{suffix}");
        let error = registry
            .register(generation_registration(&client_id, "inst-a", generation))
            .await
            .unwrap_err();
        assert_eq!(error, "agent_protocol_generation is unsupported");
        assert!(registry.get_client_view(&client_id).await.is_none());
    }
}

#[tokio::test]
async fn registration_rejects_v2_baseline_contradiction_before_creating_a_record() {
    let registry = ShellClientRegistry::default();
    let mut registration =
        generation_registration("v2-contradiction", "inst-a", AGENT_PROTOCOL_GENERATION_V2);
    registration.capabilities.structured_process_argv = false;
    let error = registry.register(registration).await.unwrap_err();
    assert_eq!(
        error,
        "runner generation baseline capability mismatch: structured_process_argv"
    );
    assert!(registry.get_client_view("v2-contradiction").await.is_none());
}

#[tokio::test]
async fn same_instance_generation_two_reconnects_remain_valid() {
    let registry = ShellClientRegistry::default();
    for _ in 0..2 {
        registry
            .register(generation_registration(
                "v2-stable",
                "inst-a",
                AGENT_PROTOCOL_GENERATION_V2,
            ))
            .await
            .unwrap();
    }
    let view = registry.get_client_view("v2-stable").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
    assert_eq!(view.agent_protocol_generation, AGENT_PROTOCOL_GENERATION_V2);
    assert_eq!(view.transport, TRANSPORT_POLLING);
    assert_eq!(
        view.project_inventory
            .as_ref()
            .map(|status| status.sync_state.as_str()),
        Some("pending")
    );
}

#[tokio::test]
async fn replacement_cannot_bypass_generation_two_admission() {
    let registry = ShellClientRegistry::default();
    registry
        .register(generation_registration(
            "replacement-v2",
            "inst-a",
            AGENT_PROTOCOL_GENERATION_V2,
        ))
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("replacement-v2", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;

    let error = registry
        .register(generation_registration(
            "replacement-v2",
            "inst-b",
            AgentProtocolGenerationNumber::new(3),
        ))
        .await
        .unwrap_err();
    assert_eq!(error, "agent_protocol_generation is unsupported");
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
            AGENT_PROTOCOL_GENERATION_V2,
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
fn registration_wire_requires_generation_capabilities_and_explicit_shell() {
    let capabilities = ShellClientCapabilities::default();
    assert!(!capabilities.async_jobs);
    assert!(!capabilities.async_shell_jobs);
    assert!(!capabilities.structured_validation_argv);
    assert!(!capabilities.structured_go_test_json);
    assert!(!capabilities.structured_go_test_tool);
    assert!(!capabilities.structured_go_test_packages);

    let missing_generation = serde_json::from_str::<ShellClientRegisterRequest>(
        r#"{"client_id":"oe","agent_instance_id":"inst-1","capabilities":{"shell":true}}"#,
    )
    .unwrap_err();
    assert!(missing_generation
        .to_string()
        .contains("agent_protocol_generation"));

    let missing_capabilities = serde_json::from_str::<ShellClientRegisterRequest>(
        r#"{"client_id":"oe","agent_instance_id":"inst-1","agent_protocol_generation":2}"#,
    )
    .unwrap_err();
    assert!(missing_capabilities.to_string().contains("capabilities"));

    let missing_shell = serde_json::from_str::<ShellClientRegisterRequest>(
        r#"{"client_id":"oe","agent_instance_id":"inst-1","agent_protocol_generation":2,"capabilities":{}}"#,
    )
    .unwrap_err();
    assert!(missing_shell.to_string().contains("shell"));

    let request: ShellClientRegisterRequest = serde_json::from_str(
        r#"{"client_id":"oe","agent_instance_id":"inst-1","agent_protocol_generation":2,"capabilities":{"shell":true}}"#,
    )
    .unwrap();
    assert_eq!(
        request.agent_protocol_generation,
        AGENT_PROTOCOL_GENERATION_V2
    );
    assert!(request.capabilities.shell);
    assert!(!request.capabilities.async_jobs);
}

#[tokio::test]
async fn polling_http_register_requires_generation() {
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
            "client_id": "polling-missing-generation",
            "agent_instance_id": "inst",
            "capabilities": {"shell": true}
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::BAD_REQUEST
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("agent_protocol_generation")),
        "{body:?}"
    );
    assert!(registry.list_clients().await.is_empty());
}

#[tokio::test]
async fn polling_http_register_accepts_generation_two() {
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
            "client_id": "polling-generation-two",
            "agent_instance_id": "inst",
            "agent_protocol_generation": AGENT_PROTOCOL_GENERATION_V2.get(),
            "capabilities": crate::test_support::current_runner_capabilities(ShellClientCapabilities::default())
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
        .get_client_view("polling-generation-two")
        .await
        .unwrap();
    assert_eq!(view.agent_protocol_generation, AGENT_PROTOCOL_GENERATION_V2);
    assert_eq!(view.transport, TRANSPORT_POLLING);
    assert!(view.projects.is_empty());
    assert_eq!(
        view.project_inventory
            .as_ref()
            .map(|status| status.sync_state.as_str()),
        Some("pending")
    );
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
            agent_protocol_generation: AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: caps,
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
                agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: {
                    let mut capabilities = v2_baseline_capabilities();
                    capabilities.coding_agent_runs = true;
                    capabilities
                },
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
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: {
                let mut capabilities = v2_baseline_capabilities();
                capabilities.coding_agent_runs = true;
                capabilities
            },
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
            agent_protocol_generation: AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write: true,
                artifact_export_chunk_read: true,
                artifact_export_streaming_metadata: true,
                structured_file_delete: true,
                apply_text_edit_occurrence: true,
                apply_text_edit_line_scope: true,
                apply_patch: true,
                apply_patch_match_metadata: true,
                apply_patch_strict_matching: true,
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
            },
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
