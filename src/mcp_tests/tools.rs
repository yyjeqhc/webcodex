use super::*;

// The compact switch is read per tools/list request, so `WEBCODEX_MCP_COMPACT_SCHEMAS`
// must stay stable (and serialized against other env-mutating tests) for the whole
// async body below. The full-operator surface is passed explicitly instead of via env.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_list_returns_same_names_as_runtime() {
    // Name parity with the runtime registry must hold for stateless-2026 under
    // both full and compact schema modes. Legacy MCP intentionally omits the
    // stateless-only export_project_artifact transport adapter. Schema shape is
    // covered by dedicated tests:
    // `mcp_tools_list_default_retains_output_schema` and
    // `mcp_tools_list_compact_omits_output_schema_only`.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let runtime_names: Vec<String> = registered_tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let legacy_runtime_names: Vec<String> = runtime_names
        .iter()
        .filter(|name| name.as_str() != "export_project_artifact")
        .cloned()
        .collect();

    for compact in [false, true] {
        if compact {
            std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        } else {
            std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        }
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(3)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok (compact={compact}), got {other:?}"),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names, legacy_runtime_names,
            "legacy tools/list must equal runtime registry minus stateless-only tools (compact={compact})"
        );

        let stateless_outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/list",
                Some(Value::from(3003)),
                mcp_2026_params(json!({})),
            ),
            None,
        )
        .await;
        let stateless_value = match stateless_outcome {
            McpOutcome::Ok(value) => value,
            other => panic!("expected stateless Ok (compact={compact}), got {other:?}"),
        };
        let stateless_names: Vec<String> = stateless_value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            stateless_names, runtime_names,
            "stateless-2026 tools/list must match the full runtime registry (compact={compact})"
        );
        // Exercise the real env adapter, not just the pure renderer: compact
        // must change outputSchema shape while preserving the common fields.
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
            if compact {
                assert!(
                    tool.get("outputSchema").is_none(),
                    "compact env adapter must omit outputSchema for {}",
                    tool["name"]
                );
            } else {
                assert!(
                    tool["outputSchema"].is_object(),
                    "default env adapter must retain outputSchema for {}",
                    tool["name"]
                );
            }
        }
    }
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
}

#[test]
fn mcp_tools_list_adds_image_mode_without_changing_generic_artifact_schema() {
    // Explicit non-compact rendering: no env involvement, nothing to serialize.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let mcp_tool = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "read_project_artifact")
        .expect("MCP read_project_artifact");
    assert_eq!(
        mcp_tool["inputSchema"]["properties"]["as_image"]["type"],
        "boolean"
    );
    assert!(mcp_tool["description"]
        .as_str()
        .unwrap()
        .contains("as_image=true"));

    let generic_tool = registered_tool_specs()
        .into_iter()
        .find(|tool| tool.name == "read_project_artifact")
        .expect("generic read_project_artifact");
    assert!(
        generic_tool.input_schema["properties"]
            .get("as_image")
            .is_none(),
        "MCP image presentation must not change the generic REST/GPT Actions schema"
    );
}

#[test]
fn mcp_tools_list_exposes_host_file_params_for_conversation_import() {
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let tool = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "import_conversation_files_to_project")
        .expect("MCP conversation import tool");

    assert_eq!(
        tool["_meta"]["openai/fileParams"],
        json!(["openaiFileIdRefs"])
    );
    let refs = &tool["inputSchema"]["properties"]["openaiFileIdRefs"];
    assert_eq!(refs["type"], "array");
    assert_eq!(refs["minItems"], 1);
    assert_eq!(refs["maxItems"], 10);
    assert_eq!(
        refs["items"]["required"],
        json!(["download_url", "file_id"])
    );
    for property in ["download_url", "file_id", "mime_type", "file_name"] {
        assert_eq!(refs["items"]["properties"][property]["type"], "string");
    }
    assert!(tool["description"]
        .as_str()
        .unwrap()
        .contains("host file-reference mechanism"));
}

#[test]
fn mcp_file_params_keep_raw_object_shape_and_reject_model_mask_strings() {
    // ChatGPT masks openai/fileParams to string[] for the model, then rewrites
    // those selections back to the raw provided-file object[] below before the
    // MCP request reaches WebCodex. WebCodex intentionally accepts only that
    // post-host-rewrite object form; it never interprets model-facing strings.
    let _string_error = crate::tool_runtime::ToolCall::from_tool_name(
        "import_conversation_files_to_project",
        json!({
            "project": "agent:test:demo",
            "openaiFileIdRefs": ["file-model-selection"]
        }),
    )
    .expect_err("model-facing string[] must not deserialize at the server");

    let call = crate::tool_runtime::ToolCall::from_tool_name(
        "import_conversation_files_to_project",
        json!({
            "project": "agent:test:demo",
            "openaiFileIdRefs": [{
                "download_url": "https://download.example/file",
                "file_id": "file_host_rewritten",
                "mime_type": "application/pdf",
                "file_name": "paper.pdf"
            }]
        }),
    )
    .expect("post-host-rewrite provided-file object[] must deserialize");
    let crate::tool_runtime::ToolCall::ImportConversationFilesToProject {
        openai_file_id_refs,
        trusted_mcp_host_file_import,
        ..
    } = call
    else {
        unreachable!()
    };
    assert_eq!(openai_file_id_refs.len(), 1);
    assert_eq!(openai_file_id_refs[0].file_id, "file_host_rewritten");
    assert!(
        !trusted_mcp_host_file_import,
        "raw input cannot set provenance"
    );
}

#[test]
fn mcp_file_import_trust_requires_exact_configured_active_client_id() {
    const CALLBACK: &str = "https://chatgpt.example/connector/oauth/test";
    let mut config = (*test_config_oauth2(Some("secret"))).clone();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");

    let make_client = |name: &str, redirect_uris: &str| crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: crate::auth::hash_token("test-secret"),
        name: name.to_string(),
        owner_user_id: Some(user.id.clone()),
        owner_project_grant_id: None,
        redirect_uris: redirect_uris.to_string(),
        allowed_scopes: "project:write".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    };
    let auth_for = |client_id: &str| {
        let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.username = Some("alice".to_string());
        auth.token_kind = Some("oauth2".to_string());
        auth.scopes = vec![crate::auth::SCOPE_PROJECT_WRITE.to_string()];
        auth.allowed_client_id = Some(client_id.to_string());
        auth
    };

    let trusted = make_client("ChatGPT WebCodex", CALLBACK);
    db.insert_oauth_client(&trusted).unwrap();
    config.oauth2.trusted_mcp_file_client_ids = vec![trusted.client_id.clone()];
    let trusted_auth = auth_for(&trusted.client_id);
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&trusted_auth)),
        HostFileImportTrust::TrustedOAuthClient,
        "the exact configured active OAuth client ID is trusted"
    );

    let same_redirect = make_client("Different Client", CALLBACK);
    db.insert_oauth_client(&same_redirect).unwrap();
    assert_eq!(
        mcp_host_file_import_trust_from_state(
            &config,
            &db,
            Some(&auth_for(&same_redirect.client_id))
        ),
        HostFileImportTrust::Untrusted,
        "sharing a redirect URI must not grant authority"
    );
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&trusted_auth)),
        HostFileImportTrust::TrustedOAuthClient,
        "multiple active clients sharing the callback must not revoke explicit client-ID trust"
    );

    let same_name = make_client("ChatGPT WebCodex", "https://other.example/callback");
    db.insert_oauth_client(&same_name).unwrap();
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&auth_for(&same_name.client_id))),
        HostFileImportTrust::Untrusted,
        "sharing the display name must not grant authority"
    );

    let unknown_client_id = crate::auth::generate_oauth_client_id();
    let mut unknown_config = config.clone();
    unknown_config.oauth2.trusted_mcp_file_client_ids = vec![unknown_client_id.clone()];
    assert_eq!(
        mcp_host_file_import_trust_from_state(
            &unknown_config,
            &db,
            Some(&auth_for(&unknown_client_id))
        ),
        HostFileImportTrust::Untrusted,
        "a configured ID without an active OAuth client record must fail closed"
    );

    let mut empty_config = config.clone();
    empty_config.oauth2.trusted_mcp_file_client_ids.clear();
    assert_eq!(
        mcp_host_file_import_trust_from_state(&empty_config, &db, Some(&trusted_auth)),
        HostFileImportTrust::Untrusted,
        "empty operator trust config must fail closed"
    );

    db.revoke_oauth_client(&trusted.id, chrono::Utc::now().timestamp())
        .unwrap();
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&trusted_auth)),
        HostFileImportTrust::Untrusted,
        "revoked configured client registrations must fail closed"
    );

    let replacement = make_client("ChatGPT WebCodex", CALLBACK);
    db.insert_oauth_client(&replacement).unwrap();
    assert_ne!(replacement.client_id, trusted.client_id);
    assert_eq!(
        mcp_host_file_import_trust_from_state(
            &config,
            &db,
            Some(&auth_for(&replacement.client_id))
        ),
        HostFileImportTrust::Untrusted,
        "recreating a client with the same callback cannot inherit the configured client-ID trust"
    );

    let api_auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&api_auth)),
        HostFileImportTrust::Untrusted
    );
}

#[test]
fn ordinary_artifact_result_keeps_existing_text_and_structured_base64_shape() {
    let value = mcp_runtime_tool_result(
        "read_project_artifact",
        false,
        ToolResult::ok(json!({
            "path": "sample.pdf",
            "mime_type": "application/pdf",
            "file_bytes": 100_000,
            "sha256": "a".repeat(64),
            "offset": 0,
            "bytes_returned": 32_768,
            "content_base64": "JVBERg==",
            "next_offset": 32_768,
            "truncated": true,
            "eof": false,
        })),
    );
    assert_eq!(value["content"].as_array().unwrap().len(), 1);
    assert_eq!(value["content"][0]["type"], "text");
    assert_eq!(
        value["structuredContent"]["output"]["content_base64"],
        "JVBERg=="
    );
    assert_eq!(
        value["structuredContent"]["output"]["truncated"], true,
        "ordinary artifact reads must retain chunk continuation metadata"
    );
}

#[tokio::test]
async fn mcp_image_call_returns_native_image_for_remote_agent_project() {
    // read_project_artifact is an artifact tool outside the local_coding
    // surface; select the full operator surface for this call.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let client_id = "mcp-vision-agent";
    let agent_instance_id = "inst-mcp-vision";
    let project_name = "remote-images";
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            }),
            projects: Some(vec![ShellAgentProjectSummary {
                id: project_name.to_string(),
                name: Some(project_name.to_string()),
                path: "/remote/session-atlas".to_string(),
                allow_patch: true,
                kind: Some("repo".to_string()),
                description: None,
                hooks: Vec::new(),
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 1,
                shell_profile: None,
            }]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
        })
        .await
        .unwrap();
    let project = crate::tool_runtime::agent_project_runtime_id(client_id, project_name);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;

    // Larger than the ordinary 256 KiB runner stdout cap: this proves the
    // narrowly widened MCP image result path carries a real screenshot-sized
    // response without silently tail-truncating its JSON/base64.
    let mut image_bytes = vec![0u8; 300 * 1024];
    image_bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    let image_base64 = general_purpose::STANDARD.encode(&image_bytes);
    let sha256 = format!("{:x}", Sha256::digest(&image_bytes));
    let path = "docs/images/console-overview-dark.png";

    let call = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "tools/call",
                    Some(json!(77)),
                    json!({
                        "name": "read_project_artifact",
                        "arguments": {
                            "project": project,
                            "path": path,
                            "as_image": true
                        }
                    }),
                ),
                Some(&auth),
            )
            .await
        }
    });

    let mut request = None;
    for _ in 0..200 {
        request = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if request.is_some() || call.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let request = match request {
        Some(request) => request,
        None => {
            let outcome = call.await.unwrap();
            panic!("MCP image call should enqueue a remote artifact read, got {outcome:?}");
        }
    };
    assert_eq!(request.kind, "file_read_project_artifact");
    assert_eq!(request.cwd.as_deref(), Some("/remote/session-atlas"));
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["offset"], 0);
    assert_eq!(
        payload["length"],
        crate::artifact_policy::MAX_MCP_IMAGE_BYTES
    );
    assert_eq!(
        payload["max_file_bytes"],
        crate::artifact_policy::MAX_MCP_IMAGE_BYTES
    );
    assert_eq!(payload["mcp_image"], true);

    let stdout = json!({
        "path": path,
        "mime_type": "image/png",
        "file_bytes": image_bytes.len(),
        "sha256": sha256,
        "offset": 0,
        "bytes_returned": image_bytes.len(),
        "content_base64": &image_base64,
        "next_offset": image_bytes.len(),
        "truncated": false,
        "eof": true,
    })
    .to_string();
    assert!(stdout.len() > 256 * 1024);
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let outcome = call.await.unwrap();
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected MCP tool result, got {outcome:?}");
    };
    assert_eq!(value["result"]["isError"], false);
    let content = value["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert!(content[0]["text"].as_str().unwrap().len() < 256);
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert_eq!(content[1]["data"], image_base64);
    let structured = &value["result"]["structuredContent"];
    assert_eq!(structured["success"], true);
    assert_eq!(structured["output"]["content_delivery"], "mcp_image");
    assert!(
        structured["output"].get("content_base64").is_none(),
        "structuredContent must not duplicate the image base64"
    );
}

#[test]
fn computer_snapshot_frames_native_image_without_structured_base64() {
    let image_bytes = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46];
    let image_base64 = general_purpose::STANDARD.encode(&image_bytes);
    let result = ToolResult::ok(json!({
        "client_id": "msi",
        "surface": {
            "surface_id": "surface_test",
            "application": "Test App",
            "title": "Test Window",
            "width": 640,
            "height": 480,
            "focused": true,
            "active": true
        },
        "width": 640,
        "height": 480,
        "mime_type": "image/jpeg",
        "file_bytes": image_bytes.len(),
        "content_base64": image_base64
    }));

    let value = crate::mcp::mcp_runtime_tool_result("computer_snapshot", false, result);
    assert_eq!(value["isError"], false);
    let content = value["content"].as_array().expect("native content");
    assert_eq!(content.len(), 2);
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/jpeg");
    assert_eq!(content[1]["data"], image_base64);
    assert_eq!(
        value["structuredContent"]["output"]["content_delivery"],
        "mcp_image"
    );
    assert_eq!(value["structuredContent"]["output"]["client_id"], "msi");
    assert!(value["structuredContent"]["output"]
        .get("content_base64")
        .is_none());
}

#[test]
fn project_connector_tools_list_is_exact_canonical_surface() {
    // Explicit non-compact rendering: no env involvement.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::CanonicalConnector, false);
    let tools = payload["tools"].as_array().expect("tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, crate::connector_runtime::surface::CAPABILITY_NAMES);
    assert_eq!(tools.len(), 14);
    assert!(tools.iter().all(|tool| tool["inputSchema"].is_object()));
    assert!(tools.iter().all(|tool| tool["outputSchema"].is_object()));
    assert!(!names.contains(&"runtime_status"));
    assert!(!names.contains(&"list_projects"));
    assert!(!names.contains(&"start_session"));
    assert!(names.contains(&"code_navigate"));
    assert!(names.contains(&"code_impact"));
    for raw_name in [
        "lsp_status",
        "document_symbols",
        "workspace_symbols",
        "goto_definition",
        "find_references",
        "document_diagnostics",
        "hover",
        "call_hierarchy",
        "prepare_call_hierarchy",
        "incoming_calls",
        "outgoing_calls",
    ] {
        assert!(!names.contains(&raw_name));
    }
}

#[test]
fn mcp_tools_list_default_retains_output_schema() {
    // Pure renderer with the explicit default compact=false switch; the
    // env-adapter path for the default is covered end-to-end by
    // `mcp_tools_list_returns_same_names_as_runtime`.
    let value = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let tools = value["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(
            tool["outputSchema"].is_object(),
            "default mode must keep outputSchema for {}",
            tool["name"]
        );
        assert!(tool["annotations"].is_object() || tool.get("annotations").is_some());
    }
}

#[test]
fn explicit_resume_mcp_schema_and_metadata_are_exposed() {
    // Explicit non-compact rendering: no env involvement.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let tool = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "start_coding_task")
        .expect("start_coding_task MCP metadata");
    let property = &tool["inputSchema"]["properties"]["resume_session_id"];
    assert_eq!(property["type"], "string");
    assert_eq!(property["pattern"], "^wc_sess_[A-Za-z0-9_]+$");
    let description = property["description"].as_str().unwrap();
    assert!(description.contains("failure never falls back"));
    assert!(description.contains("no current binding"));
    assert!(description.contains("recording_session_id"));
    assert_eq!(
        tool["inputSchema"]["not"]["required"],
        json!(["resume_session_id", "new_session"])
    );
    assert!(tool["description"]
        .as_str()
        .unwrap()
        .contains("resume_session_id"));
}

#[test]
fn mcp_tools_list_compact_omits_output_schema_only() {
    // Pure renderer with the explicit compact=true switch; the env-adapter
    // path for compact mode is covered end-to-end by
    // `mcp_tools_list_returns_same_names_as_runtime`.
    let value = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, true);
    let tools = value["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string(), "{tool:?}");
        assert!(tool["description"].is_string(), "{tool:?}");
        assert!(tool["inputSchema"].is_object(), "{tool:?}");
        assert!(
            tool.get("outputSchema").is_none(),
            "compact mode must omit outputSchema for {}",
            tool["name"]
        );
        // First-version experiment keeps annotations to reduce variables.
        assert!(
            tool.get("annotations").is_some(),
            "compact mode keeps annotations for {}",
            tool["name"]
        );
    }
}

#[test]
fn mcp_tools_list_compact_is_smaller_than_full_serialized() {
    // Explicit compact switches on the pure renderer: no env involvement.
    let full = serde_json::to_vec(&mcp_tools_list_payload_with_compact(
        ModelSurface::FullOperatorRuntime,
        false,
    ))
    .expect("full serialize");
    let compact = serde_json::to_vec(&mcp_tools_list_payload_with_compact(
        ModelSurface::FullOperatorRuntime,
        true,
    ))
    .expect("compact serialize");
    assert!(
        compact.len() < full.len(),
        "compact={} full={}",
        compact.len(),
        full.len()
    );
    // Guard against accidental total collapse (must still list many tools).
    assert!(
        compact.len() > 10_000,
        "compact unexpectedly tiny: {}",
        compact.len()
    );
}

// The compact switch is the tested product behavior: `tools/call` must be
// unaffected while `WEBCODEX_MCP_COMPACT_SCHEMAS` is set, so the env must stay
// stable (and serialized against other env-mutating tests) for the whole call.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_call_still_returns_structured_content_under_compact_flag() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(3)),
            json!({"name": "list_projects", "arguments": {}}),
        ),
        None,
    )
    .await;
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Ok, got {outcome:?}");
    };
    assert!(value["result"]["content"].is_array());
    assert!(value["result"]["structuredContent"].is_object());
    assert!(value["result"]["structuredContent"]["success"].is_boolean());
}

#[tokio::test]
async fn session_tools_exposed_in_registry_and_mcp() {
    // Session tools live on the full operator surface, not local_coding.
    // Assertions cover names/descriptions/inputSchema only, which compact
    // mode keeps, so no env or lock is needed.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let specs = registered_tool_specs();
    let registry_names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(registry_names.contains(&"session_summary"));
    assert!(registry_names.contains(&"update_session_context"));
    assert!(registry_names.contains(&"validation_summary"));
    assert!(registry_names.contains(&"current_session"));
    assert!(registry_names.contains(&"unbind_current_session"));
    // `start_session` and `bind_current_session` are ModelHidden: the
    // model coding line is covered by `start_coding_task` (resume_session_id
    // + bind_current), and `current_session` is the query-only view. They
    // keep no public ToolSpec, so they must be absent from both the
    // registry and the MCP tools/list surface.
    assert!(!registry_names.contains(&"start_session"));
    assert!(!registry_names.contains(&"bind_current_session"));

    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(31)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let names: Vec<String> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.iter().any(|name| name == "session_summary"));
    assert!(names.iter().any(|name| name == "update_session_context"));
    assert!(names.iter().any(|name| name == "validation_summary"));
    assert!(names.iter().any(|name| name == "current_session"));
    assert!(names.iter().any(|name| name == "unbind_current_session"));
    assert!(!names.iter().any(|name| name == "start_session"));
    assert!(!names.iter().any(|name| name == "bind_current_session"));
    let tools = value["result"]["tools"].as_array().unwrap();
    let tool_description = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"))["description"]
            .as_str()
            .unwrap()
            .to_lowercase()
    };
    assert!(tool_description("session_summary").contains("session ledger"));
    assert!(tool_description("update_session_context").contains("authorized project"));
    assert!(tool_description("update_session_context").contains("background writer"));
    assert!(tool_description("update_session_context").contains("success does not mean"));
    assert!(tool_description("validation_summary").contains("does not run cargo"));
    assert!(tool_description("session_handoff_summary")
        .contains("does not depend on current-session binding"));
    for name in ["current_session", "unbind_current_session"] {
        let description = tool_description(name);
        assert!(
                description.contains("process-local")
                    && description.contains("hashed durable"),
                "MCP {name} description should distinguish the exact cache and hashed durable projection: {description}"
            );
    }
    let validation_summary = tools
        .iter()
        .find(|tool| tool["name"] == "validation_summary")
        .expect("missing MCP validation_summary tool");
    assert_eq!(
        validation_summary["inputSchema"]["required"],
        json!(["project", "session_id"])
    );
    assert_eq!(
        validation_summary["inputSchema"]["additionalProperties"],
        false
    );
    for name in ["read_file", "run_shell", "write_project_file"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"));
        assert!(
            tool["inputSchema"]["properties"]
                .get("session_id")
                .is_some(),
            "MCP tools/list schema missing session_id for {name}"
        );
        assert!(
            !tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "session_id"),
            "MCP tools/list must not require session_id for {name}"
        );
    }
}

#[tokio::test]
async fn mcp_tools_call_list_projects_returns_content_blocks() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(4)),
            json!({"name": "list_projects", "arguments": {}}),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    assert_eq!(value["id"], 4);
    assert!(value["result"]["content"].is_array());
    assert_eq!(value["result"]["content"][0]["type"], "text");
    assert!(value["result"]["content"][0]["text"].is_string());
    assert!(value["result"]["structuredContent"].is_object());
    // No server-side project config is normal; without registered agents,
    // list_projects succeeds with an empty project array.
    assert_eq!(value["result"]["isError"], false);
}

#[tokio::test]
async fn mcp_tools_call_strips_reserved_session_id_before_dispatch() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("mcp strip".to_string()));
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(32)),
            json!({
                "name": "list_projects",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                }
            }),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(_) => {}
        other => panic!("expected Ok, got {:?}", other),
    }
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(10))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    let started = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started")
        .unwrap();
    assert_eq!(started.transport, "mcp");
    assert_eq!(started.tool_name, "list_projects");
    assert!(
        !serde_json::to_string(&started.input_summary)
            .unwrap()
            .contains(MCP_RESERVED_SESSION_ID_FIELD),
        "_session_id must be stripped before recording/dispatch"
    );
}

#[tokio::test]
async fn mcp_tools_call_records_event_with_session_id() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(None, None);
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(33)),
            json!({
                "name": "list_projects",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                }
            }),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(value["result"]["structuredContent"]["success"], true);
        }
        other => panic!("expected Ok, got {:?}", other),
    }
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(10))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.succeeded, 1);
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();
    assert_eq!(finished.transport, "mcp");
    assert_eq!(finished.status.as_deref(), Some("succeeded"));
    assert_eq!(finished.risk_class, "read_only");
}

#[tokio::test]
async fn mcp_tools_list_hides_testing_metadata_while_raw_call_records_it() {
    let runtime = test_runtime();
    let listed = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(330)), json!({})),
        None,
    )
    .await;
    let listed = match listed {
        McpOutcome::Ok(value) => value,
        other => panic!("expected tools/list Ok, got {other:?}"),
    };
    let job_status = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "job_status")
        .expect("job_status must be model-visible on local_coding");
    let properties = job_status["inputSchema"]["properties"].as_object().unwrap();
    for field in [
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
    ] {
        assert!(
            !properties.contains_key(field),
            "MCP tools/list must not publish recorder metadata field {field}"
        );
    }

    let session = runtime
        .sessions
        .start_session(None, Some("hidden metadata compatibility".to_string()));
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(331)),
            json!({
                "name": "job_status",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id,
                    "job_id": "missing-job",
                    "expected_failure": true,
                    "expected_failure_kind": "job_not_found",
                    "assertion_name": "mcp hidden metadata compatibility"
                }
            }),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(value) => value,
        other => panic!("expected tools/call result, got {other:?}"),
    };
    assert_eq!(value["result"]["isError"], true);

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(10))
        .unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .expect("raw MCP call must be recorded");
    assert_eq!(finished.tool_name, "job_status");
    assert_eq!(finished.expected_failure, Some(true));
    assert_eq!(
        finished.expected_failure_kind.as_deref(),
        Some("job_not_found")
    );
    assert_eq!(
        finished.assertion_name.as_deref(),
        Some("mcp hidden metadata compatibility")
    );
    assert_eq!(
        finished.actual_failure_kind.as_deref(),
        Some("job_not_found")
    );
    assert_eq!(
        finished.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );
}

#[tokio::test]
async fn mcp_show_changes_distinguishes_reserved_session_id_from_query_session_id() {
    use crate::shell_protocol::{
        ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
        ShellClientCapabilities, ShellClientRegisterRequest,
    };

    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "mcp-client".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                git: true,
                internal_posix_script: true,
                ..Default::default()
            }),
            projects: Some(vec![ShellAgentProjectSummary {
                id: "demo".to_string(),
                name: Some("Demo".to_string()),
                path: "/tmp/demo".to_string(),
                allow_patch: true,
                kind: None,
                description: None,
                hooks: vec![],
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 0,
                shell_profile: None,
            }]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let project = "agent:mcp-client:demo";
    let tracking_session = runtime
        .sessions
        .start_session(Some(project.to_string()), Some("track call".to_string()));
    let query_session = runtime
        .sessions
        .start_session(Some(project.to_string()), Some("query session".to_string()));
    let write_args = json!({"project": project, "path": "src/query.rs"});
    let start = runtime.sessions.record_tool_call_started(
        Some(&query_session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Mcp,
        "write_project_file",
        &write_args,
    );
    runtime
        .sessions
        .record_tool_call_finished(start, true, &json!({}), None, None);
    let auth = AuthContext {
        role: Some("admin".to_string()),
        scopes: vec!["admin".to_string()],
        is_bootstrap: true,
        ..AuthContext::new(crate::auth::AuthKind::Bootstrap)
    };

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(34)),
            json!({
                "name": "show_changes",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &tracking_session.session_id,
                    "project": project,
                    "session_id": &query_session.session_id,
                    "include_diff": false
                }
            }),
        ),
        Some(&auth),
    );
    let complete = async {
        let mut req = None;
        for _ in 0..50 {
            req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "mcp-client".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let req = req.expect("show_changes should enqueue an agent shell request");
        let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "mcp-client".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(stdout.to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    };
    let (outcome, _) = tokio::join!(outcome, complete);
    let value = match outcome {
        McpOutcome::Ok(value) => value,
        other => panic!("expected Ok, got {:?}", other),
    };
    let output = &value["result"]["structuredContent"]["output"];
    assert_eq!(output["session"]["found"], true);
    assert_eq!(output["session"]["session_id"], query_session.session_id);
    assert_eq!(output["session"]["changed_paths"], json!(["src/query.rs"]));

    let tracking_summary = runtime
        .sessions
        .summary(&tracking_session.session_id, Some(10))
        .unwrap();
    assert!(tracking_summary
        .events
        .iter()
        .any(|event| event.tool_name == "show_changes"));
}

#[tokio::test]
async fn mcp_tools_call_unknown_tool_is_bad_request() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(5)),
            json!({"name": "no_such_tool", "arguments": {}}),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert!(value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no_such_tool"));
        }
        other => panic!("expected BadRequest, got {:?}", other),
    }
}
