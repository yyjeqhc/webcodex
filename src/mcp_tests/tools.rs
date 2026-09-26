use super::*;

async fn wait_for_mcp_agent_request(
    registry: &crate::runner_http::RunnerRegistry,
    client_id: &str,
    runner_instance_id: &str,
    label: &str,
) -> crate::runner_protocol::RunnerRequest {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(request) = registry
            .poll(RunnerPollRequest {
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
            })
            .await
            .unwrap()
        {
            return request;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{label} did not dispatch within 10 seconds"
        );
        tokio::task::yield_now().await;
    }
}

// The compact switch is read per tools/list request, so `WEBCODEX_MCP_COMPACT_SCHEMAS`
// must stay stable (and serialized against other env-mutating tests) for the whole
// async body below. Adaptive Runtime is fixed; only schema projection varies.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_list_uses_adaptive_inventory_in_both_schema_modes() {
    let mut env = crate::test_support::TestEnvGuard::new();
    let runtime = test_runtime();
    for compact in [false, true] {
        env.set(
            "WEBCODEX_MCP_COMPACT_SCHEMAS",
            if compact { "true" } else { "false" },
        );
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(3)), json!({})),
            None,
        )
        .await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("expected legacy tools/list success (compact={compact})");
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(names.contains(&crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
        assert!(names.contains(&"run_script"));
        assert!(!names.contains(&"memory_search"));
        for direct in crate::model_surface::adaptive_runtime_direct_tool_specs() {
            if direct.name == crate::plugin_gateway::PLUGIN_TOOL_NAME {
                continue;
            }
            assert!(
                names.contains(&direct.name.as_str()),
                "missing {}",
                direct.name
            );
        }

        let stateless = handle_mcp_request(
            &runtime,
            rpc(
                "tools/list",
                Some(Value::from(3003)),
                mcp_2026_params(json!({})),
            ),
            None,
        )
        .await;
        let McpOutcome::Ok(stateless_value) = stateless else {
            panic!("expected stateless tools/list success (compact={compact})");
        };
        let stateless_tools = stateless_value["result"]["tools"].as_array().unwrap();
        let stateless_names = stateless_tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(stateless_names.contains(&crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
        assert!(!stateless_names.contains(&"skill_list"));
        assert!(!stateless_names.contains(&"skill_read_file"));
        assert!(!stateless_names.contains(&"memory_search"));
        assert!(!stateless_names.contains(&"read_tool_trace"));
        for tool in stateless_tools {
            let properties = tool["inputSchema"]["properties"].as_object().unwrap();
            assert!(properties
                .contains_key(crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD));
            if compact {
                assert!(tool.get("outputSchema").is_none(), "{}", tool["name"]);
            } else {
                assert!(tool["outputSchema"].is_object(), "{}", tool["name"]);
            }
        }
    }
}

#[tokio::test]
async fn stateless_mcp_gateway_advertises_peer_ack_without_session_wrappers() {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
    auth.scopes = vec![crate::auth::SCOPE_MCP_LOCAL.to_string()];

    let legacy =
        crate::mcp::tools::handle_list(Some(Value::from(3004)), Some(&auth), false, false, false)
            .await;
    let McpOutcome::Ok(legacy) = legacy else {
        panic!("expected legacy tools/list success");
    };
    let legacy_mcp_tool = legacy["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == crate::mcp_gateway::MCP_TOOL_NAME)
        .expect("legacy mcp_tool spec");
    assert!(!legacy_mcp_tool["inputSchema"]["properties"]
        .as_object()
        .unwrap()
        .contains_key(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD));

    let stateless =
        crate::mcp::tools::handle_list(Some(Value::from(3005)), Some(&auth), true, false, false)
            .await;
    let McpOutcome::Ok(stateless) = stateless else {
        panic!("expected stateless tools/list success");
    };
    let stateless_mcp_tool = stateless["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == crate::mcp_gateway::MCP_TOOL_NAME)
        .expect("stateless mcp_tool spec");
    let properties = stateless_mcp_tool["inputSchema"]["properties"]
        .as_object()
        .unwrap();
    assert!(properties
        .contains_key(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD));
    for field in [
        crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
        crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD,
        crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
    ] {
        assert!(
            !properties.contains_key(field),
            "mcp_tool must not advertise unsupported wrapper metadata: {field}"
        );
    }
}

#[test]
fn memory_tools_remain_canonical_extensions_without_top_level_advertising() {
    let specs = crate::tool_runtime::memory_runtime_tool_specs()
        .into_iter()
        .chain(crate::tool_runtime::memory_management_tool_specs())
        .collect::<Vec<_>>();
    assert_eq!(specs.len(), 6);
    let mut full_auth = crate::auth::shared_key_context("memory-tools-test");
    full_auth.scopes.push(crate::auth::SCOPE_ADMIN.to_string());
    for compact in [false, true] {
        for auth in [None, Some(&full_auth)] {
            let payload = mcp_tools_list_payload_with_features_for_auth(compact, false, true, auth);
            for spec in &specs {
                assert!(!payload["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == spec.name));
                assert!(
                    crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(
                        &spec.name, true
                    )
                );
                assert!(
                    !crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(
                        &spec.name, false
                    )
                );
            }
        }
    }
    let search = specs
        .iter()
        .find(|spec| spec.name == "memory_search")
        .unwrap();
    assert!(search.output_schema["properties"]["output"]["properties"]["memories"].is_object());
    let set = specs.iter().find(|spec| spec.name == "memory_set").unwrap();
    for required in [
        "project:write",
        "memory:manage",
        "permission",
        "credentials",
        "execution authority",
    ] {
        assert!(set.description.contains(required), "{}", set.description);
    }
}

#[tokio::test]
async fn hidden_extensions_keep_exact_manifest_and_gateway_execution() {
    let tmp = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(crate::Database::open(&tmp.path().join("memory.db")).unwrap());
    let runtime = test_runtime()
        .with_memory_database(db)
        .with_permission_evaluator(
            crate::tool_runtime::permissions::PermissionEvaluator::with_mode(
                crate::tool_runtime::permissions::AuthorityMode::TrustedAgent,
            ),
        );
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
    auth.username = Some("memory-owner".to_string());
    auth.user_id = Some("user-memory-owner".to_string());
    auth.token_kind = Some("oauth2".to_string());
    auth.scopes = [
        "runtime:read",
        "project:read",
        "project:write",
        "memory:read",
        "memory:manage",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    runtime
        .runner_registry
        .register_with_auth(
            crate::test_support::current_runner_registration(RunnerRegisterRequest {
                client_id: "hidden-extension-runner".to_string(),
                runner_instance_id: "inst".to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: auth.username.clone(),
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities::default(),
                policy: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
            }),
            Some(&crate::test_support::runner_access(&auth)),
        )
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "hidden-extension-runner",
        "inst",
        vec![RunnerProjectSummary {
            id: "demo".to_string(),
            name: None,
            path: tmp.path().display().to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("hidden-extension-runner", "demo");
    for name in ["memory_search", "skill_list", "skill_read_file"] {
        let McpOutcome::Ok(value) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(1)),
                mcp_2026_params(json!({
                    "name": "tool_manifest", "arguments": {"tool_name": name},
                })),
            ),
            Some(&auth),
        )
        .await
        else {
            panic!("manifest {name}");
        };
        let output = &value["result"]["structuredContent"]["output"];
        assert_eq!(output["route"]["primary"]["mode"], "gateway");
        assert_eq!(output["route"]["primary"]["tool"], "call_runtime_tool");
        assert_eq!(output["route"]["primary"]["target"], name);
        assert!(output["route"]["fallback"].is_null());
        let spec = crate::tool_runtime::stateless_operator_extension_tool_specs()
            .into_iter()
            .find(|spec| spec.name == name)
            .unwrap();
        assert_eq!(output["description"], spec.description);
        assert_eq!(output["input_schema"], spec.input_schema);
    }
    for (name, arguments) in [
        (
            "memory_set",
            json!({"project": project, "memory_key": "discovery", "summary": "Keep gateway reachability", "bootstrap": true}),
        ),
        (
            "memory_search",
            json!({"project": project, "context_request": ["memory.bootstrap"]}),
        ),
        (
            "memory_read",
            json!({"project": project, "memory_key": "discovery"}),
        ),
    ] {
        let McpOutcome::Ok(value) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(2)),
                mcp_2026_params(adaptive_runtime_gateway_params(name, arguments)),
            ),
            Some(&auth),
        )
        .await
        else {
            panic!("gateway {name}");
        };
        let result = &value["result"]["structuredContent"];
        assert_eq!(result["success"], true, "{name}: {result}");
        if name == "memory_search" {
            let material = &result["output"]["context_projection"]["materials"][0];
            assert_eq!(material["key"], "memory.bootstrap");
            assert_eq!(material["status"], "available");
            assert!(material["projection"]
                .to_string()
                .contains("Keep gateway reachability"));
        }
    }
    // Both compatibility paths reach the same Project authority boundary;
    // gateway admission never makes an unknown Project available.
    for (name, arguments) in [
        ("skill_list", json!({"project": "missing-project"})),
        (
            "skill_read_file",
            json!({"project": "missing-project", "skill_id": "wc_skill_AAAAAAAAAAAAAAAAAAAAAA", "path": "SKILL.md"}),
        ),
    ] {
        let mut results = Vec::new();
        for params in [
            json!({"name": name, "arguments": arguments}),
            adaptive_runtime_gateway_params(name, arguments),
        ] {
            let McpOutcome::Ok(value) = handle_mcp_request(
                &runtime,
                rpc("tools/call", Some(json!(3)), mcp_2026_params(params)),
                Some(&auth),
            )
            .await
            else {
                panic!("compatibility call {name}");
            };
            let result = value["result"]["structuredContent"].clone();
            assert_eq!(result["success"], false, "{name}: {result}");
            assert_eq!(
                result["output"]["error_kind"], "unknown_project",
                "{name}: {result}"
            );
            results.push(result);
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
fn trace_reader_is_stateless_protocol_extension_admin_scoped_and_schema_static() {
    let render = |stateless_2026: bool, auth: Option<&crate::auth::AuthContext>| {
        mcp_tools_list_payload_with_features_for_auth(false, false, stateless_2026, auth)
    };
    let names = |payload: &Value| {
        payload["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };
    let admin = crate::auth::AuthContext {
        role: Some("admin".to_string()),
        scopes: vec![crate::auth::SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap)
    };
    let ordinary = crate::auth::AuthContext {
        scopes: vec![crate::auth::SCOPE_PROJECT_READ.to_string()],
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token)
    };

    assert!(!names(&render(true, None)).contains(&"read_tool_trace".to_string()));
    assert!(!names(&render(true, Some(&ordinary))).contains(&"read_tool_trace".to_string()));
    assert!(!names(&render(false, Some(&admin))).contains(&"read_tool_trace".to_string()));
    assert!(
        names(&render(true, Some(&admin))).contains(&"read_tool_trace".to_string()),
        "Stateless MCP 2026 plus admin authority must admit the trace extension"
    );

    let full = render(true, Some(&admin));
    let tool = full["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "read_tool_trace")
        .expect("admin Stateless MCP 2026 trace reader");
    assert_eq!(tool["inputSchema"]["required"], json!(["trace_ref"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert!(tool["inputSchema"]["properties"]["payload_index"].is_object());
    assert!(tool["outputSchema"]["properties"]["output"]["properties"]["payload"].is_object());
}

#[test]
fn skill_runtime_tools_are_stateless_protocol_extensions_and_schema_static() {
    let generic_names = registered_tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert!(generic_names.iter().any(|name| name == "skill_load"));
    assert!(generic_names
        .iter()
        .any(|name| name == "run_skill_resource"));
    assert!(!generic_names.iter().any(|name| name == "skill_list"));
    assert!(!generic_names.iter().any(|name| name == "skill_read_file"));

    let render_full = || {
        let mut payload = mcp_tools_list_payload_with_features_for_auth(false, false, true, None);
        add_stateless_workflow_recorder_metadata(&mut payload);
        payload
    };
    let before = render_full();
    let skill_names = before["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .filter(|name| name.starts_with("skill_"))
        .collect::<Vec<_>>();
    assert_eq!(skill_names, vec!["skill_load"]);

    let run_skill_resource = before["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "run_skill_resource")
        .expect("run_skill_resource must be exposed");
    assert!(run_skill_resource["inputSchema"]["properties"]
        .get("stdin")
        .is_none());
    assert!(run_skill_resource["inputSchema"]["properties"]
        .get("executable")
        .is_none());
    assert_eq!(
        run_skill_resource["inputSchema"]["properties"]["path"]["pattern"],
        "^scripts/.+$"
    );
    assert!(run_skill_resource["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "expected_definition_revision"));
    assert_eq!(
        run_skill_resource["outputSchema"]["properties"]["output"]["properties"]["skill_trust"]
            ["enum"],
        json!([
            "operator_configured_guidance",
            "operator_installed_guidance"
        ])
    );

    let compatibility_specs = crate::tool_runtime::stateless_operator_extension_tool_specs();
    let skill_list = compatibility_specs
        .iter()
        .find(|spec| spec.name == "skill_list")
        .unwrap();
    assert_eq!(
        skill_list.input_schema["properties"]["limit"]["maximum"],
        64
    );
    assert_eq!(
        skill_list.output_schema["properties"]["output"]["properties"]["skills"]["type"],
        "array"
    );
    for name in ["skill_list", "skill_read_file"] {
        assert!(crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true));
        assert!(!crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(name, false));
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".agents/skills/foo")).unwrap();
    std::fs::write(
        tmp.path().join(".agents/skills/foo/SKILL.md"),
        "---\nname: foo\ndescription: first\n---\nbody\n",
    )
    .unwrap();
    let one_package = render_full();
    std::fs::create_dir_all(tmp.path().join(".agents/skills/bar")).unwrap();
    std::fs::write(
        tmp.path().join(".agents/skills/bar/SKILL.md"),
        "---\nname: bar\ndescription: second\n---\nbody\n",
    )
    .unwrap();
    let two_packages = render_full();
    assert_eq!(before, one_package);
    assert_eq!(
        one_package, two_packages,
        "Skill package count must not alter MCP tool schemas"
    );

    let legacy_full = mcp_tools_list_payload_with_compact(false);
    assert!(legacy_full["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| !matches!(
            tool["name"].as_str(),
            Some("skill_list" | "skill_read_file")
        )));
}

#[test]
fn skill_management_tools_require_admin_and_remain_fixed_schema() {
    let render = |auth: Option<&crate::auth::AuthContext>| {
        mcp_tools_list_payload_with_features_for_auth(false, false, true, auth)
    };
    let shared = crate::auth::shared_key_context("skill-management-test");
    let shared_payload = render(Some(&shared));
    let shared_names = shared_payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .filter(|name| name.starts_with("skill_"))
        .collect::<Vec<_>>();
    assert_eq!(shared_names, vec!["skill_load"]);

    let admin = crate::auth::AuthContext {
        role: Some("admin".to_string()),
        scopes: vec![crate::auth::SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap)
    };
    let first = render(Some(&admin));
    let names = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .filter(|name| name.starts_with("skill_"))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "skill_load",
            "skill_versions",
            "skill_install",
            "skill_activate",
            "skill_remove_revision",
        ]
    );
    assert_eq!(
        crate::tool_runtime::skill_management_tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        vec![
            "skill_versions",
            "skill_install",
            "skill_activate",
            "skill_remove_revision",
        ]
    );
    for name in ["skill_install", "skill_activate", "skill_remove_revision"] {
        let description = first["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["description"].as_str())
            .unwrap_or_else(|| panic!("missing {name} retention description"));
        for required in ["24 hours", "7 days", "skill_versions", "not proof"] {
            assert!(
                description.contains(required),
                "{name} must document replay retention: {description}"
            );
        }
    }
    assert_eq!(
        first,
        render(Some(&admin)),
        "management schemas are content-independent"
    );
}

#[test]
fn stateless_workflow_recorder_metadata_adds_protocol_projection() {
    let mut full = mcp_tools_list_payload_with_compact(false);
    add_stateless_workflow_recorder_metadata(&mut full);
    for tool in full["tools"].as_array().unwrap() {
        if tool["name"] != "work_on_project" {
            let serialized = serde_json::to_string(tool).unwrap();
            assert!(
                !serialized.contains("work_on_project"),
                "{} pollutes exact work_on_project discovery",
                tool["name"]
            );
        }
    }
    let read_files = full["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "read_files")
        .expect("Adaptive direct read_files schema");
    let read_files_output = serde_json::to_string(&read_files["outputSchema"]).unwrap();
    let read_files_input = read_files["inputSchema"]["properties"]
        .as_object()
        .expect("read_files input properties");
    assert!(
        read_files_input
            .get(crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD)
            .is_some(),
        "ordinary model-visible tools must advertise Window replies"
    );
    assert!(
        read_files_output.contains("\"window_reply\""),
        "ordinary model-visible output must admit the post-result reply receipt"
    );
    let mut gateway_payload = json!({
        "tools": [{
            "name": "call_runtime_tool",
            "inputSchema": {"type": "object", "properties": {}},
            "outputSchema": {"type": "object", "properties": {"output": {"type": "object", "properties": {}}}}
        }]
    });
    add_stateless_workflow_recorder_metadata(&mut gateway_payload);
    assert!(
        gateway_payload["tools"][0]["inputSchema"]["properties"]
            .get(crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD)
            .is_some(),
        "adaptive wrapper must advertise Window reply metadata"
    );
    assert!(!serde_json::to_string(&full)
        .unwrap()
        .contains("\"recovery_required\""));
    assert!(!serde_json::to_string(&full)
        .unwrap()
        .contains("\"session_context_continuation\""));
    assert!(read_files_output.contains("context_projection"));
    assert!(read_files_output.contains("post-tool context sidecar"));
    for tool in full["tools"].as_array().unwrap() {
        let input = &tool["inputSchema"]["properties"];
        assert!(input.get("ack_session_context_revision").is_none());
        let output = tool["outputSchema"].to_string();
        for retired in [
            "session_context_revision",
            "session_continuity",
            "session_recovery",
            "ignored_invocation_metadata",
        ] {
            assert!(
                !output.contains(retired),
                "{} exposes {retired}",
                tool["name"]
            );
        }
        assert!(input.get("ack_session_message_ids").is_some());
        assert!(input.get("ack_ref").is_some());
    }
    assert!(!read_files_output.contains("session_continuity"));
    assert!(!read_files_output.contains("session_recovery"));
    assert!(full["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["name"] != "list_tools"));
    assert_eq!(
        crate::model_surface::adaptive_runtime_tool_invocation_route("list_tools"),
        ("gateway", Some("call_runtime_tool"))
    );

    let generic = registered_tool_specs()
        .into_iter()
        .find(|tool| tool.name == "complete_session_message")
        .expect("generic complete_session_message spec");
    let generic_properties = generic.input_schema["properties"].as_object().unwrap();
    assert!(!generic_properties
        .contains_key(crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD));
    assert!(!generic_properties
        .contains_key(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD));
    assert!(
        !generic_properties.contains_key(crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD)
    );
    assert!(!generic_properties
        .contains_key(crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD));
    assert!(!generic_properties
        .contains_key(crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD));
    assert!(!generic_properties.contains_key("ack_session_context_revision"));
    assert!(!generic_properties
        .contains_key(crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD));
}

#[test]
fn stateless_ack_wrapper_normalizes_and_is_removed_before_concrete_tool_parsing() {
    let mut arguments = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD: [
            "wc_msg_abcd-efgh_ijklmn",
            "wc_msg_abcd-efgh_ijklmn",
            "wc_msg_0123456789abcdef"
        ]
    });
    let normalized = strip_stateless_ack_session_message_ids(&mut arguments).unwrap();
    assert_eq!(
        normalized,
        vec!["wc_msg_abcd-efgh_ijklmn", "wc_msg_0123456789abcdef"]
    );
    assert!(arguments
        .get(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD)
        .is_none());
    assert_eq!(
        normalized,
        vec!["wc_msg_abcd-efgh_ijklmn", "wc_msg_0123456789abcdef"]
    );
    crate::tool_runtime::ToolCall::from_tool_name("list_tools", arguments)
        .expect("wrapper ACK metadata must be gone before concrete parsing");

    let mut malformed = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD: ["not-a-message-id"]
    });
    assert!(strip_stateless_ack_session_message_ids(&mut malformed).is_err());
    let mut oversized = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD:
            (0..=crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS)
                .map(|index| format!("wc_msg_{index}"))
                .collect::<Vec<_>>()
    });
    assert!(strip_stateless_ack_session_message_ids(&mut oversized).is_err());
}

#[test]
fn stateless_ack_ref_wrapper_is_bounded_and_removed_before_concrete_parsing() {
    let mut arguments = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD: "  wc_ack1_example  "
    });
    let ack_ref = strip_stateless_ack_ref(&mut arguments).unwrap();
    assert_eq!(ack_ref.as_deref(), Some("wc_ack1_example"));
    assert!(arguments
        .get(crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD)
        .is_none());
    crate::tool_runtime::ToolCall::from_tool_name("list_tools", arguments)
        .expect("ACK ref wrapper metadata must be gone before concrete parsing");

    let mut wrong_type = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD: ["wc_ack1_example"]
    });
    assert!(strip_stateless_ack_ref(&mut wrong_type).is_err());

    let mut oversized = json!({
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD:
            "x".repeat(crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_REF_CHARS + 1)
    });
    assert!(strip_stateless_ack_ref(&mut oversized).is_err());
}

#[test]
fn stateless_message_resolution_wrapper_is_validated_and_removed_before_concrete_parsing() {
    let mut arguments = json!({
        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: {
            "message_id": "wc_msg_abcd-efgh_ijklmn",
            "resolution": "  handled in the current model turn  "
        }
    });
    let resolution = strip_stateless_session_message_resolution(&mut arguments)
        .unwrap()
        .expect("message resolution wrapper");
    assert_eq!(resolution.message_id, "wc_msg_abcd-efgh_ijklmn");
    assert_eq!(resolution.resolution, "handled in the current model turn");
    assert!(arguments
        .get(crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD)
        .is_none());
    crate::tool_runtime::ToolCall::from_tool_name("list_tools", arguments)
        .expect("message resolution wrapper metadata must be gone before concrete parsing");

    for malformed in [
        json!({
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: {
                "message_id": "not-a-message-id",
                "resolution": "handled"
            }
        }),
        json!({
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: {
                "message_id": "wc_msg_beta",
                "resolution": "   "
            }
        }),
        json!({
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: {
                "message_id": "wc_msg_beta",
                "resolution": "handled",
                "extra": true
            }
        }),
    ] {
        let mut malformed = malformed;
        assert!(strip_stateless_session_message_resolution(&mut malformed).is_err());
    }
}

#[test]
fn stateless_context_request_is_deduped_open_ended_and_removed_before_parsing() {
    let mut arguments = json!({
        crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD: [
            "project.instructions",
            "future.material",
            "project.instructions"
        ]
    });
    let normalized = strip_stateless_context_request(&mut arguments).unwrap();
    assert_eq!(
        normalized,
        vec![
            "project.instructions".to_string(),
            "future.material".to_string()
        ]
    );
    assert!(arguments
        .get(crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD)
        .is_none());
    crate::tool_runtime::ToolCall::from_tool_name("list_tools", arguments)
        .expect("context_request wrapper metadata must be gone before concrete parsing");

    for malformed in [
        json!({crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD: "project.instructions"}),
        json!({crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD: ["bad key"]}),
        json!({crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD: [""]}),
        json!({crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD:
            (0..=crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS)
                .map(|index| format!("future.material.{index}"))
                .collect::<Vec<_>>()
        }),
    ] {
        let mut malformed = malformed;
        assert!(strip_stateless_context_request(&mut malformed).is_err());
    }
}

#[test]
fn stateless_invocation_metadata_stays_typed_and_business_arguments_stay_clean() {
    let mut arguments = json!({
        "project": "proj",
        "items": [{"path": "src/lib.rs"}],
        crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_adapter",
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD: ["wc_msg_abcd-efgh_ijklmn"],
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD: "wc_ack1_fixture",
        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: {
            "message_id": "wc_msg_abcd-efgh_ijklmn",
            "resolution": "handled"
        },
        crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD: {
            "reply_to": "wc_msg_abcd-efgh_ijklmn",
            "message": "Tests are clean"
        },
        crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD: ["webcodex.workflow"],
    });
    let recording_session_id = strip_recording_session_id(&mut arguments).unwrap();
    let ack_session_message_ids = strip_stateless_ack_session_message_ids(&mut arguments).unwrap();
    let ack_ref = strip_stateless_ack_ref(&mut arguments).unwrap();
    let session_message_resolution =
        strip_stateless_session_message_resolution(&mut arguments).unwrap();
    let window_reply = strip_stateless_window_reply(&mut arguments).unwrap();
    let context_request = strip_stateless_context_request(&mut arguments).unwrap();
    let metadata = crate::tool_runtime::kernel::ToolInvocationMetadata {
        control: None,
        ack_session_message_ids,
        ack_ref,
        session_message_resolution,
        window_reply,
        context_request,
    };

    assert_eq!(recording_session_id.as_deref(), Some("wc_sess_adapter"));
    assert_eq!(
        metadata.ack_session_message_ids,
        vec!["wc_msg_abcd-efgh_ijklmn"]
    );
    assert_eq!(metadata.ack_ref.as_deref(), Some("wc_ack1_fixture"));
    assert_eq!(metadata.context_request, vec!["webcodex.workflow"]);
    assert!(metadata.session_message_resolution.is_some());
    assert_eq!(
        metadata
            .window_reply
            .as_ref()
            .map(|reply| reply.message.as_str()),
        Some("Tests are clean")
    );
    for field in [
        crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
        crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD,
        crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
    ] {
        assert!(
            arguments.get(field).is_none(),
            "wrapper leaked into business args: {field}"
        );
    }
    assert!(!arguments.to_string().contains("__webcodex_"));
    crate::tool_runtime::ToolCall::from_tool_name("read_files", arguments)
        .expect("typed invocation metadata must not be required for concrete ToolCall parsing");
}

#[test]
fn read_project_artifact_stays_gateway_only_without_changing_generic_schema() {
    let payload = mcp_tools_list_payload_with_compact(false);
    assert!(payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["name"] != "read_project_artifact"));
    assert_eq!(
        crate::model_surface::adaptive_runtime_tool_invocation_route("read_project_artifact"),
        ("gateway", Some("call_runtime_tool"))
    );

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

    assert!(generic_tool.description.to_lowercase().contains("bounded"));
}

#[test]
fn mcp_tools_list_exposes_host_file_params_for_conversation_import() {
    let payload = mcp_tools_list_payload_with_compact(false);
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
        host_file_import_provenance,
        ..
    } = call
    else {
        unreachable!()
    };
    assert_eq!(openai_file_id_refs.len(), 1);
    assert_eq!(
        openai_file_id_refs[0].file_id.as_deref(),
        Some("file_host_rewritten")
    );
    assert_eq!(
        host_file_import_provenance,
        HostFileImportTrust::Untrusted,
        "raw input cannot set provenance"
    );
}

#[test]
fn mcp_file_import_trust_distinguishes_exact_tier1_from_active_oauth_tier2() {
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
        owner_shared_key_hash: None,
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
        HostFileImportTrust::TrustedMcpHostFile,
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
        HostFileImportTrust::AuthenticatedMcpOpenAiHostFile,
        "sharing a redirect URI must not grant Tier 1 authority"
    );
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&trusted_auth)),
        HostFileImportTrust::TrustedMcpHostFile,
        "multiple active clients sharing the callback must not revoke explicit client-ID trust"
    );

    let same_name = make_client("ChatGPT WebCodex", "https://other.example/callback");
    db.insert_oauth_client(&same_name).unwrap();
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&auth_for(&same_name.client_id))),
        HostFileImportTrust::AuthenticatedMcpOpenAiHostFile,
        "sharing the display name must not grant Tier 1 authority"
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
        HostFileImportTrust::AuthenticatedMcpOpenAiHostFile,
        "empty Tier 1 config still permits only authenticated OpenAI-host import"
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
        HostFileImportTrust::AuthenticatedMcpOpenAiHostFile,
        "recreated active client cannot inherit Tier 1 but keeps OpenAI-host-only Tier 2"
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
async fn project_artifact_image_call_returns_native_image_for_remote_agent_project() {
    // Exercise the unified facade through the real MCP dispatch and native-image
    // framing path while the canonical registry retains the legacy specialist behavior.
    let runtime = test_runtime();
    let client_id = "mcp-vision-agent";
    let runner_instance_id = "inst-mcp-vision";
    let project_name = "remote-images";
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities {
                    file_read: true,
                    ..Default::default()
                },
                policy: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        runner_instance_id,
        vec![RunnerProjectSummary {
            id: project_name.to_string(),
            name: Some(project_name.to_string()),
            path: "/remote/session-atlas".to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id(client_id, project_name);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;

    // Larger than the legacy 1 MiB image cap and the ordinary 256 KiB runner
    // stdout cap: this proves the widened native-image path carries a real large
    // image without silently tail-truncating its JSON/base64.
    let mut image_bytes = vec![0u8; 2 * 1024 * 1024];
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
                        "name": "project_artifact",
                        "arguments": {
                            "project": project,
                            "path": path,
                            "action": "image"
                        }
                    }),
                ),
                Some(&auth),
            )
            .await
        }
    });

    let request = wait_for_mcp_agent_request(
        &runtime.runner_registry,
        client_id,
        runner_instance_id,
        "MCP image call",
    )
    .await;
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
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: runner_instance_id.to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
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
fn computer_observe_snapshot_frames_native_image_without_structured_base64() {
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

    let value = crate::mcp::mcp_runtime_tool_result("computer_observe", false, result);
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
fn browser_observe_screenshot_uses_shared_native_image_framing_without_structured_base64() {
    let image_bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4];
    let image_base64 = general_purpose::STANDARD.encode(&image_bytes);
    let result = ToolResult::ok(json!({
        "browser_id": "browser_abcdefghijklmnop",
        "page_id": "page_abcdefghijklmnop",
        "width": 1024,
        "height": 768,
        "mime_type": "image/png",
        "file_bytes": image_bytes.len(),
        "sha256": "a".repeat(64),
        "content_base64": image_base64
    }));

    let value = crate::mcp::mcp_runtime_tool_result("browser_observe", false, result);
    assert_eq!(value["isError"], false);
    let content = value["content"].as_array().expect("native content");
    assert_eq!(content.len(), 2);
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert_eq!(content[1]["data"], image_base64);
    assert_eq!(
        value["structuredContent"]["output"]["content_delivery"],
        "mcp_image"
    );
    assert_eq!(
        value["structuredContent"]["output"]["browser_id"],
        "browser_abcdefghijklmnop"
    );
    assert!(value["structuredContent"]["output"]
        .get("content_base64")
        .is_none());
}

#[test]
fn mcp_tools_list_explicit_full_projection_retains_output_schema() {
    // Pure renderer with explicit compact=false. Exposure-specific defaults
    // are covered through the request adapter rather than inferred here.
    let value = mcp_tools_list_payload_with_compact(false);
    let tools = value["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(
            tool["outputSchema"].is_object(),
            "explicit full projection must keep outputSchema for {}",
            tool["name"]
        );
        assert!(tool["annotations"].is_object() || tool.get("annotations").is_some());
    }
}

#[test]
fn retired_start_coding_task_is_absent_from_mcp_discovery() {
    let payload = mcp_tools_list_payload_with_compact(false);
    assert!(payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["name"] != "start_coding_task"));
}

#[test]
fn mcp_work_on_project_schema_exposes_managed_worktree_without_internal_operation() {
    let payload = mcp_tools_list_payload_with_compact(false);
    let tools = payload["tools"].as_array().expect("tools array");
    let work = tools
        .iter()
        .find(|tool| tool["name"] == "work_on_project")
        .expect("work_on_project MCP ToolSpec");
    let properties = work["inputSchema"]["properties"]
        .as_object()
        .expect("work_on_project input properties");
    assert_eq!(properties["mode"]["enum"], json!(["checkout", "worktree"]));
    assert_eq!(properties["mode"]["default"], "checkout");
    assert!(properties.contains_key("base_ref"));
    assert!(work["outputSchema"]["properties"]["output"]["properties"]["worktree"].is_object());
    assert!(tools
        .iter()
        .all(|tool| tool["name"] != "prepare_managed_worktree"));
}

#[test]
fn mcp_tools_list_compact_omits_output_schema_and_preserves_annotations() {
    // Pure renderer with the explicit compact=true switch; the env-adapter
    // path for compact mode is covered end-to-end by
    // `mcp_tools_list_returns_same_names_as_runtime`.
    let value = mcp_tools_list_payload_with_compact(true);
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
        // Description compaction must preserve effect/approval annotations.
        assert!(
            tool.get("annotations").is_some(),
            "compact mode keeps annotations for {}",
            tool["name"]
        );
    }
}

fn strip_description_text(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("description").is_some_and(Value::is_string) {
                object.remove("description");
            }
            // Normalize schema copy only, never descriptions inside literal
            // const/default/enum/examples or unrelated adapter metadata.
            for (keyword, child) in object {
                match keyword.as_str() {
                    "properties" | "patternProperties" | "$defs" | "definitions"
                    | "dependentSchemas" | "dependencies" => {
                        if let Some(children) = child.as_object_mut() {
                            for schema in children.values_mut() {
                                strip_description_text(schema);
                            }
                        }
                    }
                    "items"
                    | "prefixItems"
                    | "allOf"
                    | "anyOf"
                    | "oneOf"
                    | "additionalItems"
                    | "additionalProperties"
                    | "unevaluatedItems"
                    | "unevaluatedProperties"
                    | "propertyNames"
                    | "contains"
                    | "not"
                    | "if"
                    | "then"
                    | "else" => strip_description_text(child),
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_description_text(child);
            }
        }
        _ => {}
    }
}

fn description_chars(value: &Value) -> usize {
    match value {
        Value::Object(object) => object
            .iter()
            .map(|(key, child)| {
                if key == "description" && child.is_string() {
                    child.as_str().unwrap().chars().count()
                } else {
                    description_chars(child)
                }
            })
            .sum(),
        Value::Array(items) => items.iter().map(description_chars).sum(),
        _ => 0,
    }
}

#[test]
fn mcp_tools_list_inputs_equal_canonical_except_descriptions_and_host_file_overlay() {
    let mut auth = crate::auth::shared_key_context("canonical-mcp-test");
    auth.scopes.push(crate::auth::SCOPE_ADMIN.to_string());
    let specs = registered_tool_specs()
        .into_iter()
        .chain(crate::tool_runtime::stateless_operator_extension_tool_specs())
        .map(|spec| (spec.name.clone(), spec))
        .collect::<std::collections::HashMap<_, _>>();
    for compact in [false, true] {
        let payload =
            mcp_tools_list_payload_with_features_for_auth(compact, false, true, Some(&auth));
        for tool in payload["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let canonical = &specs[name];
            let mut expected = canonical.input_schema.clone();
            // MCP Host rewrites these two required references; this is the only
            // direct-input transport overlay, independent of description mode.
            if name == "import_conversation_files_to_project" {
                expected["properties"]["openaiFileIdRefs"]["items"]["required"] =
                    json!(["download_url", "file_id"]);
            }
            let mut actual = tool["inputSchema"].clone();
            if compact {
                strip_description_text(&mut expected);
                strip_description_text(&mut actual);
            } else {
                assert_eq!(tool["description"], canonical.description, "{name}");
                // Output schemas retain their existing MCP suggested-call
                // routing overlays; compact discovery must not remove them here.
                assert!(tool["outputSchema"].is_object(), "{name}");
            }
            assert_eq!(actual, expected, "{name} compact={compact}");
            assert_eq!(tool["annotations"], canonical.annotations, "{name}");
        }
    }
}

// The exact manifest must stay canonical even when the request adapter reads
// compact=true, so hold the existing environment guard through the calls.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_compact_preserves_stateless_wrappers_app_metadata_and_exact_manifest() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let mut auth = crate::auth::shared_key_context("compact-overlays-test");
    auth.scopes.push(crate::auth::SCOPE_ADMIN.to_string());
    for stateless in [false, true] {
        for app_enabled in [false, true] {
            let mut payloads = Vec::new();
            for compact in [false, true] {
                let McpOutcome::Ok(value) = crate::mcp::tools::handle_list(
                    Some(json!(1)),
                    Some(&auth),
                    stateless,
                    compact,
                    app_enabled,
                )
                .await
                else {
                    panic!("tools/list");
                };
                let result = value["result"].clone();
                for tool in result["tools"].as_array().unwrap() {
                    if compact {
                        assert!(tool.get("outputSchema").is_none());
                        let properties = &tool["inputSchema"]["properties"];
                        for (field, hint) in [
                            ("recording_session_id", "wc_sess_*"),
                            ("ack_session_message_ids", "wc_msg_*"),
                            ("session_message_resolution", "wc_msg_*"),
                            ("context_request", "jobs.attention"),
                        ] {
                            if let Some(property) = properties.get(field) {
                                assert!(property["description"].as_str().unwrap().contains(hint));
                            }
                        }
                        for pointer in [
                            "/ack_session_message_ids/items",
                            "/session_message_resolution/properties/message_id",
                            "/session_message_resolution/properties/resolution",
                        ] {
                            if let Some(property) = properties.pointer(pointer) {
                                assert!(property.get("description").is_none());
                            }
                        }
                    }
                }
                payloads.push(result);
            }
            let full_tools = payloads[0]["tools"].as_array().unwrap();
            let compact_tools = payloads[1]["tools"].as_array().unwrap();
            assert_eq!(full_tools.len(), compact_tools.len());
            for (full, compact) in full_tools.iter().zip(compact_tools) {
                assert_compact_tool_diff(full, compact);
            }
            // Include the Stateless result envelope in the equality check.
            payloads[0].as_object_mut().unwrap().remove("tools");
            payloads[1].as_object_mut().unwrap().remove("tools");
            assert_eq!(
                payloads[0], payloads[1],
                "stateless={stateless} app={app_enabled}"
            );
        }
    }
    let runtime = test_runtime();
    let specs = registered_tool_specs();
    for name in [
        "run_process",
        "work_on_project",
        "run_skill_resource",
        "project_artifact",
        "git_diff_hunks",
        "import_conversation_files_to_project",
    ] {
        let McpOutcome::Ok(value) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(2)),
                mcp_2026_params(json!({
                    "name": "tool_manifest", "arguments": {"tool_name": name},
                })),
            ),
            Some(&auth),
        )
        .await
        else {
            panic!("exact manifest: {name}");
        };
        let output = &value["result"]["structuredContent"]["output"];
        let canonical = specs.iter().find(|spec| spec.name == name).unwrap();
        assert_eq!(output["description"], canonical.description, "{name}");
        assert_eq!(output["input_schema"], canonical.input_schema, "{name}");
        assert!(
            output.get("output_schema").is_none(),
            "exact manifest is input-only: {name}"
        );
    }
}

fn assert_compact_tool_diff(full: &Value, compact: &Value) {
    let name = full["name"].as_str().unwrap();
    let mut expected = full.clone();
    let mut actual = compact.clone();
    if name == "mcp_tool" {
        assert!(
            full.get("outputSchema").is_none(),
            "gateway has no output schema"
        );
    } else {
        assert!(full["outputSchema"].is_object(), "{name}");
    }
    assert!(compact.get("outputSchema").is_none(), "{name}");
    expected.as_object_mut().unwrap().remove("outputSchema");
    // Independent, closed allowlist of actual removed annotations. A changed
    // regex, any other location, or any removed bound must fail equality.
    for (pointer, pattern) in [
        (
            "/properties/recording_session_id",
            "^(~s[1-9][0-9]{0,19}|wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32}))$",
        ),
        (
            "/properties/ack_session_message_ids/items",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
        (
            "/properties/session_message_resolution/properties/message_id",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
    ] {
        if let Some(property) = expected["inputSchema"].pointer_mut(pointer) {
            assert_eq!(property["type"], "string", "{name} {pointer}");
            assert_eq!(property["pattern"], pattern, "{name} {pointer}");
            assert!(
                compact["inputSchema"]
                    .pointer(pointer)
                    .unwrap()
                    .get("pattern")
                    .is_none(),
                "opaque wrapper pattern survived: {name} {pointer}"
            );
            property.as_object_mut().unwrap().remove("pattern");
        }
    }
    // `_control` is the one intentional structural compacting exception. The
    // full Stateless MCP schema remains the exact closed operational contract;
    // compact tools/list keeps only an object selection entry so the same large
    // canonical sidecar payload schemas are not repeated on every ordinary tool.
    if let (Some(full_control), Some(compact_control)) = (
        expected["inputSchema"]
            .pointer("/properties/_control")
            .cloned(),
        actual["inputSchema"]
            .pointer("/properties/_control")
            .cloned(),
    ) {
        assert_eq!(full_control["type"], "object", "{name}");
        assert_eq!(full_control["additionalProperties"], false, "{name}");
        assert_eq!(
            full_control["properties"]["before"]["maxProperties"], 1,
            "{name}"
        );
        assert_eq!(
            full_control["properties"]["after_success"]["maxProperties"], 1,
            "{name}"
        );
        assert_eq!(compact_control["type"], "object", "{name}");
        assert!(compact_control.get("properties").is_none(), "{name}");
        expected["inputSchema"]["properties"]["_control"] = compact_control;
    }
    // window_reply is the other intentional structural compacting exception.
    // Full discovery owns the exact closed reply contract; compact discovery
    // keeps only the object entry because startup guidance carries the shape.
    if let (Some(full_reply), Some(compact_reply)) = (
        expected["inputSchema"]
            .pointer("/properties/window_reply")
            .cloned(),
        actual["inputSchema"]
            .pointer("/properties/window_reply")
            .cloned(),
    ) {
        assert_eq!(full_reply["type"], "object", "{name}");
        assert_eq!(full_reply["additionalProperties"], false, "{name}");
        assert_eq!(
            full_reply["properties"]["reply_to"]["pattern"],
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
            "{name}"
        );
        assert_eq!(
            full_reply["properties"]["message"]["minLength"], 1,
            "{name}"
        );
        assert_eq!(
            full_reply["properties"]["message"]["maxLength"], 8_000,
            "{name}"
        );
        assert_eq!(
            full_reply["required"],
            json!(["reply_to", "message"]),
            "{name}"
        );
        assert_eq!(compact_reply, json!({"type": "object"}), "{name}");
        expected["inputSchema"]["properties"]["window_reply"] = compact_reply;
    }
    for tool in [&mut expected, &mut actual] {
        tool.as_object_mut().unwrap().remove("description");
        strip_description_text(&mut tool["inputSchema"]);
    }
    // Includes annotations, App _meta/ui, Host-file overlays, requiredness,
    // additionalProperties, every non-wrapper pattern and every bound.
    assert_eq!(actual, expected, "unexpected compact difference: {name}");
}

#[tokio::test]
async fn mcp_compact_preserves_safety_patterns_and_wrapper_bounds() {
    let mut auth = crate::auth::shared_key_context("compact-bounds-test");
    auth.scopes.push(crate::auth::SCOPE_ADMIN.to_string());
    let McpOutcome::Ok(value) =
        crate::mcp::tools::handle_list(Some(json!(1)), Some(&auth), true, true, true).await
    else {
        panic!("tools/list");
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let schema =
        |name: &str| &tools.iter().find(|tool| tool["name"] == name).unwrap()["inputSchema"];
    for (name, field, pattern, min, max) in [
        (
            "project_artifact",
            "expected_sha256",
            "^[0-9a-f]{64}$",
            64,
            64,
        ),
        ("run_skill_resource", "path", "^scripts/.+$", 9, 512),
        (
            "git_review_summary",
            "base_commit",
            "^[0-9A-Fa-f]{40}$",
            40,
            40,
        ),
        (
            "git_review_summary",
            "head_commit",
            "^[0-9A-Fa-f]{40}$",
            40,
            40,
        ),
        ("git_diff_hunks", "base_commit", "^[0-9A-Fa-f]{40}$", 40, 40),
        ("git_diff_hunks", "head_commit", "^[0-9A-Fa-f]{40}$", 40, 40),
    ] {
        let property = &schema(name)["properties"][field];
        assert_eq!(property["pattern"], pattern, "{name}.{field}");
        assert_eq!(property["minLength"], min, "{name}.{field}");
        assert_eq!(property["maxLength"], max, "{name}.{field}");
    }
    assert_eq!(
        schema("run_skill_resource")["properties"]["expected_definition_revision"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    // The identical opaque Session pattern remains on business Session inputs.
    assert_eq!(
        schema("work_on_project")["properties"]["session_id"]["pattern"],
        "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
    );
    let properties = &schema("run_process")["properties"];
    assert_eq!(
        properties["ack_session_message_ids"]["maxItems"],
        crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS
    );
    assert_eq!(
        properties["session_message_resolution"]["properties"]["resolution"]["minLength"],
        1
    );
    assert_eq!(
        properties["session_message_resolution"]["properties"]["resolution"]["maxLength"],
        crate::tool_runtime::sessions::MAX_MESSAGE_RESOLUTION_CHARS
    );
    assert_eq!(
        properties["context_request"]["maxItems"],
        crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS
    );
    assert_eq!(properties["context_request"]["items"]["minLength"], 1);
    assert_eq!(
        properties["context_request"]["items"]["maxLength"],
        crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS
    );
    assert_eq!(properties["timeout_secs"]["minimum"], 1);
    assert!(
        properties.get("sync_wait_secs").is_none(),
        "legacy sync_wait_secs must stay hidden from MCP discovery"
    );
}

#[test]
fn mcp_compact_opaque_patterns_require_exact_wrapper_location_and_format() {
    use crate::mcp::discovery::compact_tool;
    let session_pattern = "^(~s[1-9][0-9]{0,19}|wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32}))$";
    // Even at a known path, a future/different format must not silently vanish.
    for pattern in [
        "^wc_sess_[A-Za-z0-9_-]{16}$",
        "^[0-9a-f]{64}$",
        "^scripts/.+$",
    ] {
        let literal =
            json!({"type": "string", "pattern": session_pattern, "minLength": 24, "maxLength": 40});
        let mut tool = json!({"name": "fixture", "inputSchema": {
            "properties": {
                "recording_session_id": {"type": "string", "pattern": pattern},
                "session_id": literal,
                "nested": {"properties": {"recording_session_id": literal}}
            },
            "const": literal, "default": literal, "enum": [literal], "examples": [literal]
        }});
        let original = tool.clone();
        compact_tool(&mut tool);
        assert_eq!(tool, original, "pattern={pattern}");
    }
    let mut tool = json!({"name": "fixture", "inputSchema": {"properties": {
        "recording_session_id": {"type": "string", "pattern": session_pattern, "minLength": 24, "maxLength": 40}
    }}});
    compact_tool(&mut tool);
    assert_eq!(
        tool["inputSchema"]["properties"]["recording_session_id"],
        json!({"type": "string", "minLength": 24, "maxLength": 40})
    );
    let once = tool.clone();
    compact_tool(&mut tool);
    assert_eq!(
        tool, once,
        "wrapper annotation projection must be idempotent"
    );
}

#[tokio::test]
async fn mcp_recording_session_ref_fails_closed_when_malformed() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(1)),
            mcp_2026_params(adaptive_runtime_gateway_params(
                "list_projects",
                json!({
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: "~s01"
                }),
            )),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::BadRequest(value) => value,
        other => panic!("expected malformed recorder ref BadRequest, got {other:?}"),
    };
    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("unknown_session_ref")));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_compact_stateless_wrapper_ids_still_reject_malformed_invocations() {
    let mut env = crate::test_support::TestEnvGuard::new();
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("compact wrapper validation".to_string()));
    for compact in [false, true] {
        env.set(
            "WEBCODEX_MCP_COMPACT_SCHEMAS",
            if compact { "true" } else { "false" },
        );
        for malformed in ["not-an-id", "wc_msg_short", "wc_msg_0123456789abcde!"] {
            for field in [
                "recording_session_id",
                "ack_session_message_ids",
                "session_message_resolution",
            ] {
                let mut arguments =
                    json!({"tool_name": "run_process", "recording_session_id": session.session_id});
                arguments[field] = match field {
                    "recording_session_id" => json!(malformed.replace("wc_msg_", "wc_sess_")),
                    "ack_session_message_ids" => json!([malformed]),
                    _ => json!({"message_id": malformed, "resolution": "handled"}),
                };
                let outcome = handle_mcp_request(
                    &runtime,
                    rpc(
                        "tools/call",
                        Some(json!(1)),
                        mcp_2026_params(json!({"name": "tool_manifest", "arguments": arguments})),
                    ),
                    None,
                )
                .await;
                match (field, outcome) {
                    // Recorder identity is rejected by the existing exact
                    // Session lookup/authority gate, before business dispatch.
                    ("recording_session_id", McpOutcome::Ok(value)) => {
                        assert_eq!(value["result"]["isError"], true);
                        assert_eq!(value["result"]["structuredContent"]["success"], false);
                        assert_eq!(
                            value["result"]["structuredContent"]["output"]["error_kind"],
                            "unknown_session_id"
                        );
                    }
                    (_, McpOutcome::BadRequest(value)) => {
                        assert_eq!(value["error"]["code"], -32602);
                        let message = value["error"]["message"].as_str().unwrap();
                        assert!(
                            message.contains(field) && message.contains("valid wc_msg_*"),
                            "{message}"
                        );
                    }
                    (_, other) => panic!("{field}={malformed}, compact={compact}: {other:?}"),
                }
            }
        }
    }
    assert!(
        runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap()
            .events
            .is_empty(),
        "rejected invocations must not reach the recorder ledger"
    );
    // A real server-generated recorder remains usable with compact=true.
    let McpOutcome::Ok(value) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(2)),
            mcp_2026_params(json!({"name": "tool_manifest", "arguments": {
                "tool_name": "run_process", "recording_session_id": session.session_id
            }})),
        ),
        None,
    )
    .await
    else {
        panic!("valid recorder");
    };
    assert_eq!(value["result"]["structuredContent"]["success"], true);
}

#[test]
fn mcp_compact_common_copy_respects_tool_and_argument_boundaries() {
    use crate::mcp::discovery::{bound_description, compact_tool, INPUT_DESCRIPTION_MAX_CHARS};
    let full = mcp_tools_list_payload_with_compact(false);
    let compact = mcp_tools_list_payload_with_compact(true);
    for (name, field, hints) in [
        (
            "run_process",
            "cwd",
            vec!["Project-relative", "root", "No named Session SSH"],
        ),
        (
            "run_skill_resource",
            "cwd",
            vec!["Project-relative", "Skill resolution"],
        ),
        (
            "run_detached_process",
            "timeout_secs",
            vec!["Total runtime", "604800"],
        ),
        (
            "run_shell",
            "assertion_name",
            vec!["reuse after a fix", "validation-like"],
        ),
    ] {
        let property = |payload: &Value| {
            payload["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap()["inputSchema"]["properties"][field]["description"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let before = bound_description(&property(&full), INPUT_DESCRIPTION_MAX_CHARS);
        let after = property(&compact);
        assert!(after.len() < before.len(), "{name}.{field}");
        for hint in hints {
            assert!(after.contains(hint), "{name}.{field}: {after}");
        }
    }
    // These share names, but carry different selection, authority or retry
    // semantics. They must stay on the existing generic bounding path.
    for tool in full["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        let projected = compact["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|candidate| candidate["name"] == name)
            .unwrap();
        for field in [
            "project",
            "client_id",
            "idempotency_key",
            "purpose",
            "result_expectation",
        ] {
            if let Some(copy) = tool["inputSchema"]["properties"][field]["description"].as_str() {
                assert_eq!(
                    projected["inputSchema"]["properties"][field]["description"],
                    bound_description(copy, INPUT_DESCRIPTION_MAX_CHARS),
                    "{name}.{field}"
                );
            }
        }
        if name == "run_shell" {
            for field in ["cwd", "timeout_secs"] {
                let copy = tool["inputSchema"]["properties"][field]["description"]
                    .as_str()
                    .unwrap();
                assert_eq!(
                    projected["inputSchema"]["properties"][field]["description"],
                    bound_description(copy, INPUT_DESCRIPTION_MAX_CHARS),
                    "{name}.{field}"
                );
            }
        }
    }
    let mut fixture = json!({"name": "run_process", "inputSchema": {"properties": {
        "cwd": {"type": "string"},
        "nested": {"properties": {"cwd": {"description": "A different cwd contract."}}}
    }}});
    let original = fixture.clone();
    compact_tool(&mut fixture);
    assert_eq!(
        fixture, original,
        "do not add copy or rewrite nested business fields"
    );
}

#[test]
fn mcp_compact_descriptions_preserve_selection_and_schema_literals() {
    use crate::mcp::discovery::{
        bound_description, compact_tool, INPUT_DESCRIPTION_MAX_CHARS, TOOL_DESCRIPTION_MAX_CHARS,
    };
    let full = mcp_tools_list_payload_with_compact(false);
    let compact = mcp_tools_list_payload_with_compact(true);
    let tools = compact["tools"].as_array().unwrap();
    for (name, phrases) in [
        (
            "run_process",
            vec!["native executable", "literal argv", "observe_jobs"],
        ),
        (
            "run_shell",
            vec!["shell grammar", "related command chain", "observe_jobs"],
        ),
        (
            "run_detached_process",
            vec!["survives Runner", "idempotency_key", "same Job"],
        ),
        (
            "observe_jobs",
            vec![
                "observation_token",
                "after_observation_token",
                "never redispatches",
            ],
        ),
        (
            "list_jobs",
            vec!["Recover or inventory", "observe_jobs directly"],
        ),
        (
            "wait_for_job_terminal",
            vec![
                "exact existing Job",
                "returned continuation",
                "Host continuation",
            ],
        ),
        ("stop_job", vec!["confirm=true", "without stopping"]),
        (
            "present_agent_continuation",
            vec![
                "create_agent_identity",
                "yield/end promptly",
                "not wake readiness",
                "production_auto_resume_available",
            ],
        ),
    ] {
        let description = tools.iter().find(|tool| tool["name"] == name).unwrap()["description"]
            .as_str()
            .unwrap();
        for phrase in phrases {
            assert!(description.contains(phrase), "{name}: {description}");
        }
    }
    for tool in tools {
        assert!(
            tool["description"].as_str().unwrap().chars().count() <= TOOL_DESCRIPTION_MAX_CHARS
        );
    }
    assert!(description_chars(&compact) < description_chars(&full));
    let long = "Read foo.rs with v0.4.0. ".to_string() + &"Additional detail. ".repeat(100);
    assert_eq!(bound_description(&long, 28), "Read foo.rs with v0.4.0.");
    let unicode = bound_description(&"界".repeat(300), INPUT_DESCRIPTION_MAX_CHARS);
    assert_eq!(unicode.chars().count(), INPUT_DESCRIPTION_MAX_CHARS);
    assert!(unicode.ends_with('…'));
    let mut tool = json!({
        "name": "schema-fixture", "description": long,
        "inputSchema": {
            "type": "object", "additionalProperties": false,
            "required": ["description"],
            "properties": {
                "description": {"type": "string", "description": long, "minLength": 1, "maxLength": 400, "pattern": "^x"},
                "context_request": {"type": "array", "description": long}
            },
            "$defs": {"nested": {"description": long}},
            "anyOf": [{"description": long, "properties": {"x": {"enum": ["a", "b"]}}}],
            "oneOf": [{"description": long, "items": {"description": long, "maximum": 3}}],
            "if": {"description": long}, "then": {"description": long}, "else": {"description": long},
            "const": {"description": long}, "default": {"description": long},
            "enum": [{"description": long}], "examples": [{"description": long}]
        }
    });
    let original = tool.clone();
    compact_tool(&mut tool);
    let context_request_description = tool["inputSchema"]["properties"]["context_request"]
        ["description"]
        .as_str()
        .unwrap();
    assert!(
        context_request_description.contains("jobs.attention"),
        "{context_request_description}"
    );
    assert!(context_request_description.chars().count() <= INPUT_DESCRIPTION_MAX_CHARS);
    for keyword in ["const", "default", "enum", "examples"] {
        assert_eq!(
            tool["inputSchema"][keyword],
            original["inputSchema"][keyword]
        );
    }
    for pointer in [
        "/properties/description/description",
        "/$defs/nested/description",
        "/anyOf/0/description",
        "/oneOf/0/items/description",
        "/if/description",
        "/then/description",
        "/else/description",
    ] {
        assert!(
            tool["inputSchema"]
                .pointer(pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .chars()
                .count()
                <= INPUT_DESCRIPTION_MAX_CHARS
        );
    }
    let once = tool.clone();
    compact_tool(&mut tool);
    assert_eq!(
        tool, once,
        "projection must be idempotent across adapter overlays"
    );
    let mut original_schema = original["inputSchema"].clone();
    strip_description_text(&mut original_schema);
    strip_description_text(&mut tool["inputSchema"]);
    assert_eq!(tool["inputSchema"], original_schema);
}

#[tokio::test]
async fn mcp_tools_list_stateless_serialized_size_budget() {
    let mut scoped = crate::auth::shared_key_context("surface-size-test");
    scoped.scopes.extend([
        crate::auth::SCOPE_PLUGIN_INSPECT.to_string(),
        crate::auth::SCOPE_PLUGIN_INVOKE.to_string(),
        crate::auth::SCOPE_PLUGIN_MANAGE.to_string(),
    ]);
    let mut admin = scoped.clone();
    admin.scopes.push(crate::auth::SCOPE_ADMIN.to_string());
    // Final Stateless result bytes (including wrappers/gateways, excluding the
    // JSON-RPC envelope). With compact `_control`: 99,640 / 102,324 / 113,348
    // bytes, plus 16,987 with Apps. Keep roughly 10% byte headroom rather than
    // silently absorbing future advertised surface growth.
    for (label, auth, max_tools, max_bytes) in [
        ("anonymous", None, 34, 110_000),
        ("scoped", Some(&scoped), 35, 113_000),
        ("admin", Some(&admin), 41, 125_000),
    ] {
        for app_enabled in [false, true] {
            let mut sizes = Vec::new();
            for compact in [true, false] {
                let McpOutcome::Ok(value) = crate::mcp::tools::handle_list(
                    Some(json!(1)),
                    auth,
                    true,
                    compact,
                    app_enabled,
                )
                .await
                else {
                    panic!("tools/list");
                };
                let result = &value["result"];
                let count = result["tools"].as_array().unwrap().len();
                let bytes = serde_json::to_vec(result).unwrap().len();
                let tools = result["tools"].as_array().unwrap();
                let top_chars: usize = tools
                    .iter()
                    .map(|tool| tool["description"].as_str().unwrap().chars().count())
                    .sum();
                let input_chars: usize = tools
                    .iter()
                    .map(|tool| description_chars(&tool["inputSchema"]))
                    .sum();
                eprintln!("MCP_SIZE {label} app={app_enabled} compact={compact} count={count} bytes={bytes} top_description_chars={top_chars} input_description_chars={input_chars}");
                let feature_tools = if cfg!(feature = "experimental-code-mode") {
                    3
                } else {
                    0
                };
                // Work Result v3 adds one bounded App-only collaboration adapter
                // alongside the existing Goal Plan/continuation/read helpers.
                let count_budget = max_tools + if app_enabled { 17 } else { 0 } + feature_tools;
                let byte_budget =
                    max_bytes + if app_enabled { 18_000 } else { 0 } + feature_tools * 4096;
                if feature_tools == 0 {
                    assert_eq!(
                        count, count_budget,
                        "{label} app={app_enabled}: tool inventory changed"
                    );
                } else {
                    // Preserve the existing experimental Code Mode allowance.
                    assert!(
                        count <= count_budget,
                        "{label} app={app_enabled}: {count} tools exceeds {count_budget}"
                    );
                }
                if compact {
                    assert!(
                        bytes <= byte_budget,
                        "{label} app={app_enabled}: compact {bytes} bytes exceeds {byte_budget}"
                    );
                }
                sizes.push(bytes);
            }
            let ratio = sizes[0] as f64 / sizes[1] as f64;
            eprintln!("MCP_RATIO {label} app={app_enabled} {ratio:.4}");
            assert!(
                ratio <= 0.18,
                "{label} app={app_enabled}: compact/full={ratio:.4}"
            );
        }
    }
}

// The compact switch is the tested product behavior: `tools/call` must be
// unaffected while `WEBCODEX_MCP_COMPACT_SCHEMAS` is set, so the env must stay
// stable (and serialized against other env-mutating tests) for the whole call.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_call_still_returns_structured_content_under_compact_flag() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(3)),
            adaptive_runtime_gateway_params("list_projects", json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Ok, got {outcome:?}");
    };
    assert!(value["result"]["content"].is_array());
    assert!(value["result"]["structuredContent"].is_object());
    assert!(value["result"]["structuredContent"]["success"].is_boolean());
}

#[tokio::test]
async fn session_tools_stay_registered_and_follow_adaptive_routes() {
    let runtime = test_runtime();
    let specs = registered_tool_specs();
    let registry_names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    for name in [
        "session_summary",
        "update_session_context",
        "validation_summary",
        "session_handoff_summary",
    ] {
        assert!(
            registry_names.contains(&name),
            "missing registered Session tool {name}"
        );
    }
    for removed in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
        "start_session",
    ] {
        assert!(
            !registry_names.contains(&removed),
            "retired/model-hidden Session tool leaked into model registry: {removed}"
        );
    }

    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(31)), json!({})),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Adaptive tools/list success, got {outcome:?}");
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"session_handoff_summary"));
    for long_tail in [
        "session_summary",
        "update_session_context",
        "validation_summary",
    ] {
        assert!(
            !names.contains(&long_tail),
            "long-tail Session tool leaked into Adaptive direct inventory: {long_tail}"
        );
        assert_eq!(
            crate::model_surface::adaptive_runtime_tool_invocation_route(long_tail),
            ("gateway", Some("call_runtime_tool")),
            "{long_tail}"
        );
    }

    let registered = |name: &str| {
        specs
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("missing registered tool {name}"))
    };
    assert!(registered("session_summary")
        .description
        .to_lowercase()
        .contains("session ledger"));
    assert!(registered("update_session_context")
        .description
        .contains("authorized project"));
    assert!(registered("update_session_context")
        .description
        .contains("background writer"));
    assert!(registered("update_session_context")
        .description
        .contains("success does not mean"));
    assert!(registered("validation_summary")
        .description
        .to_lowercase()
        .contains("does not run cargo"));

    let handoff = tools
        .iter()
        .find(|tool| tool["name"] == "session_handoff_summary")
        .expect("Adaptive direct session_handoff_summary");
    assert!(handoff["description"]
        .as_str()
        .unwrap()
        .contains("exact session_id"));

    let validation_summary = registered("validation_summary");
    assert_eq!(
        validation_summary.input_schema["required"],
        json!(["project", "session_id"])
    );
    assert_eq!(
        validation_summary.input_schema["additionalProperties"],
        false
    );

    for name in ["read_files", "run_shell"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing Adaptive direct tool {name}"));
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
    assert!(!names.contains(&"write_project_file"));
    assert_eq!(
        crate::model_surface::adaptive_runtime_tool_invocation_route("write_project_file"),
        ("gateway", Some("call_runtime_tool"))
    );
}

#[tokio::test]
async fn mcp_tools_call_list_projects_returns_content_blocks() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(4)),
            adaptive_runtime_gateway_params("list_projects", json!({})),
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
async fn mcp_current_window_activity_requires_adapter_window_identity() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("window-diagnostic-mcp");
    let McpOutcome::Ok(value) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(41)),
            mcp_2026_params(adaptive_runtime_gateway_params(
                "current_window_activity",
                json!({"limit": 20}),
            )),
        ),
        Some(&auth),
    )
    .await
    else {
        panic!("current_window_activity must be callable")
    };
    assert_eq!(
        value["result"]["structuredContent"]["output"]["reason_code"],
        "window_identity_unavailable"
    );
    assert_eq!(value["result"]["isError"], false);
}

#[tokio::test]
async fn mcp_tools_call_rejects_legacy_reserved_session_id_before_dispatch() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        Some("demo".to_string()),
        Some("legacy recorder".to_string()),
    );
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(32)),
            mcp_2026_params(adaptive_runtime_gateway_params(
                "list_projects",
                json!({"_session_id": &session.session_id}),
            )),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::BadRequest(value) => value,
        other => panic!("expected invalid-params BadRequest, got {other:?}"),
    };
    assert_eq!(value["error"]["code"], -32602);
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("_session_id"));
    assert!(message.contains("no longer supported"));
    assert!(message.contains("recording_session_id"));
    assert_eq!(
        runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap()
            .counts
            .tool_calls,
        0
    );
}

#[tokio::test]
async fn mcp_read_files_ignores_inapplicable_context_ack_without_consuming_it() {
    use crate::runner_protocol::{
        RunnerCapabilities, RunnerProjectSummary, RunnerRegisterRequest, RunnerResultRequest,
    };
    use webcodex_workspace::file_read_range::{self, EffectiveRange};

    let runtime = test_runtime();
    let client_id = "mcp-read-files-wrapper-metadata";
    let runner_instance_id = "inst-mcp-read-files-wrapper-metadata";
    let project_name = "repo";
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities {
                    file_read: true,
                    ..Default::default()
                },
                policy: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        runner_instance_id,
        vec![RunnerProjectSummary {
            id: project_name.to_string(),
            name: Some(project_name.to_string()),
            path: "/remote/repo".to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id(client_id, project_name);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    let call = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "tools/call",
                    Some(json!(320)),
                    mcp_2026_params(json!({
                        "name": "read_files",
                        "arguments": {
                            "project": project,
                            "items": [{"path": "src/lib.rs"}],
                        }
                    })),
                ),
                Some(&auth),
            )
            .await
        }
    });

    let request = wait_for_mcp_agent_request(
        &runtime.runner_registry,
        client_id,
        runner_instance_id,
        "read_files without retired context metadata",
    )
    .await;
    assert_eq!(request.kind, "file_read");
    assert_eq!(request.path.as_deref(), Some("src/lib.rs"));
    let start = request.start_line.unwrap();
    let end = request.end_line.unwrap();
    let range = EffectiveRange::new(Some(start), Some(end - start + 1));
    let read = file_read_range::read_range_from(&b"small\n"[..], range).unwrap();
    let stdout = json!({
        "format": "webcodex.file_read_range.v1",
        "content": read.content,
        "sha256": read.sha256,
        "total_lines": read.total_lines,
        "start_line": read.start_line,
        "limit": read.limit,
    })
    .to_string();
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: runner_instance_id.to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let outcome = call.await.unwrap();
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected read_files success, got {outcome:?}");
    };
    assert_eq!(value["result"]["isError"], false);
    let output = &value["result"]["structuredContent"]["output"];
    assert_eq!(output["items"][0]["output"]["text"], "small");
    assert!(output.get("ignored_invocation_metadata").is_none());
    for field in [
        "session_context_revision",
        "session_continuity",
        "session_recovery",
    ] {
        assert!(output.get(field).is_none(), "unexpected {field}: {output}");
    }
}

#[tokio::test]
async fn stateless_mcp_ack_wrapper_is_removed_before_concrete_dispatch_and_is_request_scoped() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("stateless ack wrapper".to_string()));
    let guidance = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "Remember this guidance for the current context.".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();

    let call = |ack: Option<&str>, id: i64| {
        let mut arguments = json!({
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: &session.session_id
        });
        if let Some(message_id) = ack {
            arguments[crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD] =
                json!([message_id, message_id]);
        }
        rpc(
            "tools/call",
            Some(Value::from(id)),
            mcp_2026_params(adaptive_runtime_gateway_params("list_projects", arguments)),
        )
    };

    let acknowledged =
        handle_mcp_request(&runtime, call(Some(&guidance.message_id), 321), None).await;
    let acknowledged = match acknowledged {
        McpOutcome::Ok(value) => value,
        other => panic!("expected ACK call success, got {other:?}"),
    };
    assert_eq!(acknowledged["result"]["structuredContent"]["success"], true);
    assert_eq!(
        acknowledged["result"]["structuredContent"]["output"]["session_attention"]["ack"]
            ["accepted_count"],
        1
    );
    assert!(
        acknowledged["result"]["structuredContent"]["output"]["session_attention"]["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let forgotten = handle_mcp_request(&runtime, call(None, 322), None).await;
    let forgotten = match forgotten {
        McpOutcome::Ok(value) => value,
        other => panic!("expected forgotten-ACK call success, got {other:?}"),
    };
    assert_eq!(forgotten["result"]["structuredContent"]["success"], true);
    assert_eq!(
        forgotten["result"]["structuredContent"]["output"]["session_attention"]["messages"][0]
            ["message_id"],
        guidance.message_id
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let started = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started")
        .unwrap();
    let input = serde_json::to_string(&started.input_summary).unwrap();
    assert!(!input.contains("ack_session_message_ids"));
    assert!(!input.contains("__webcodex_stateless_ack_session_message_ids"));
}

#[tokio::test]
async fn mcp_tools_call_rejects_legacy_session_alias_even_with_canonical_recorder() {
    let runtime = test_runtime();
    let canonical = runtime
        .sessions
        .start_session(None, Some("canonical recorder".to_string()));

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(320)),
            mcp_2026_params(adaptive_runtime_gateway_params(
                "list_projects",
                json!({
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: &canonical.session_id,
                    "_session_id": &canonical.session_id
                }),
            )),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::BadRequest(value) => value,
        other => panic!("expected invalid-params BadRequest, got {other:?}"),
    };
    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .is_some_and(
            |message| message.contains("_session_id") && message.contains("no longer supported")
        ));
    assert_eq!(
        runtime
            .sessions
            .summary(&canonical.session_id, Some(10))
            .unwrap()
            .counts
            .tool_calls,
        0
    );
}

#[tokio::test]
async fn mcp_tools_call_records_event_with_recording_session_id() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_project_reference_database(std::sync::Arc::new(
        crate::Database::open(&tmp.path().join("recorder-refs.db")).unwrap(),
    ));
    let authority = crate::tool_runtime::workflow_session_authority_fingerprint(None)
        .expect("local test principal should have stable authority");
    let session = runtime
        .sessions
        .start_session_with_options(
            crate::tool_runtime::SessionCreateOptions::new(
                None,
                Some("short recorder".to_string()),
                crate::tool_runtime::SessionMode::Normal,
                crate::tool_runtime::SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(authority)),
        )
        .unwrap();
    let session_ref = runtime
        .session_reference_for_id(&session.session_id, None)
        .expect("test runtime should issue Session refs");
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(33)),
            mcp_2026_params(adaptive_runtime_gateway_params(
                "list_projects",
                json!({
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: session_ref
                }),
            )),
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
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert!(
        tools.iter().all(|tool| tool["name"] != "job_status"),
        "retired job_status must stay absent from MCP tools/list"
    );
    let observe_jobs = tools
        .iter()
        .find(|tool| tool["name"] == "observe_jobs")
        .expect("observe_jobs must remain Adaptive-direct");
    let properties = observe_jobs["inputSchema"]["properties"]
        .as_object()
        .unwrap();
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
            mcp_2026_params(json!({
                "name": "call_runtime_tool",
                "arguments": {
                    "tool": "stop_job",
                    "arguments": {
                        crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: &session.session_id,
                        "project": "agent:nope:nope",
                        "job_id": "missing-job",
                        "confirm": false,
                        "expected_failure": true,
                        "expected_failure_kind": "confirmation_required",
                        "assertion_name": "mcp hidden metadata compatibility"
                    }
                }
            })),
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
    assert_eq!(finished.tool_name, "stop_job");
    assert_eq!(finished.expected_failure, Some(true));
    assert_eq!(
        finished.expected_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        finished.assertion_name.as_deref(),
        Some("mcp hidden metadata compatibility")
    );
    assert_eq!(
        finished.actual_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        finished.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );
}

#[tokio::test]
async fn mcp_show_changes_distinguishes_recording_session_id_from_query_session_id() {
    use crate::runner_protocol::{
        RunnerCapabilities, RunnerProjectSummary, RunnerRegisterRequest, RunnerResultRequest,
    };

    let runtime = test_runtime();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: "mcp-client".to_string(),
                runner_instance_id: "inst".to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities {
                    shell: true,
                    git: true,
                    internal_posix_script: true,
                    ..Default::default()
                },
                policy: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "mcp-client",
        "inst",
        vec![RunnerProjectSummary {
            id: "demo".to_string(),
            name: Some("demo".to_string()),
            path: "/tmp/mcp-demo".to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: chrono::Utc::now().timestamp(),
            shell_profile: None,
        }],
    )
    .await;
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
        crate::tool_runtime::sessions::session_tool_contract("write_project_file"),
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
            mcp_2026_params(json!({
                "name": "show_changes",
                "arguments": {
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD: &tracking_session.session_id,
                    "project": project,
                    "session_id": &query_session.session_id,
                    "include_diff": false
                }
            })),
        ),
        Some(&auth),
    );
    let complete = async {
        let req = wait_for_mcp_agent_request(
            &runtime.runner_registry,
            "mcp-client",
            "inst",
            "show_changes",
        )
        .await;
        let stdout = crate::tool_runtime::framed_clean_show_changes_test_stdout("test head", false);
        runtime
            .runner_registry
            .complete(RunnerResultRequest {
                client_id: "mcp-client".to_string(),
                runner_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(stdout),
                stderr: Some(String::new()),
                stdout_truncated: false,
                stderr_truncated: false,
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
async fn project_grant_authority_is_identical_for_project_credential_and_share_oauth_across_direct_and_gateway(
) {
    const GRANT_A: &str = "wc_pgrant_aaaaaaaaaaaaaaaa";
    const GRANT_B: &str = "wc_pgrant_bbbbbbbbbbbbbbbb";
    const CLIENT_A: &str = "project-grant-a-runner";
    const CLIENT_B: &str = "project-grant-b-runner";
    const INSTANCE_A: &str = "inst-project-grant-a";
    const INSTANCE_B: &str = "inst-project-grant-b";
    const PROJECT_A: &str = "project-a";
    const PROJECT_B: &str = "project-b";

    let runtime = test_runtime();
    let runner_auth = |grant: &str, client_id: &str| crate::auth::AuthContext {
        kind: crate::auth::AuthKind::AgentToken,
        username: Some("local-owner".to_string()),
        role: Some("agent".to_string()),
        token_kind: Some("agent".to_string()),
        allowed_client_id: Some(client_id.to_string()),
        project_grant_id: Some(grant.to_string()),
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::AgentToken)
    };
    let registration = |client_id: &str, instance_id: &str| {
        crate::test_support::current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            runner_instance_id: instance_id.to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: Some(client_id.to_string()),
            owner: Some("local-owner".to_string()),
            hostname: None,
            host_context: None,
            capabilities: RunnerCapabilities::default(),
            policy: None,
        })
    };
    let project = |id: &str, path: &str| RunnerProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("repo".to_string()),
        registration_source: None,
        description: None,
        hooks: Vec::new(),
        disabled: false,
        revision: None,
        root_fingerprint: None,
        lineage: None,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: 1,
        shell_profile: None,
    };

    let runner_a = runner_auth(GRANT_A, CLIENT_A);
    let runner_b = runner_auth(GRANT_B, CLIENT_B);
    let access_a = crate::runner_http::runner_access_from_auth(Some(&runner_a)).unwrap();
    let access_b = crate::runner_http::runner_access_from_auth(Some(&runner_b)).unwrap();
    runtime
        .runner_registry
        .register_with_auth(registration(CLIENT_A, INSTANCE_A), Some(&access_a))
        .await
        .unwrap();
    runtime
        .runner_registry
        .register_with_auth(registration(CLIENT_B, INSTANCE_B), Some(&access_b))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        CLIENT_A,
        INSTANCE_A,
        vec![project(PROJECT_A, "/tmp/project-grant-a")],
    )
    .await;
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        CLIENT_B,
        INSTANCE_B,
        vec![project(PROJECT_B, "/tmp/project-grant-b")],
    )
    .await;

    let project_credential = crate::auth::shared_key::project_credential_context(GRANT_A);
    let project_share_oauth = crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        api_key_id: Some("oauth-project-share".to_string()),
        role: Some("project".to_string()),
        scopes: crate::auth::PROJECT_SHARE_OAUTH_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect(),
        token_kind: Some(crate::auth::PROJECT_SHARE_OAUTH_TOKEN_KIND.to_string()),
        allowed_client_id: Some("project-share-oauth-client".to_string()),
        project_grant_id: Some(GRANT_A.to_string()),
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token)
    };
    assert!(project_share_oauth.is_oauth_project_subject());

    for (label, auth) in [
        ("project credential", &project_credential),
        ("project-share oauth", &project_share_oauth),
    ] {
        let runners = runtime
            .dispatch_with_auth(
                crate::tool_runtime::ToolCall::from_tool_name(
                    "list_runners",
                    json!({"summary_only": true}),
                )
                .unwrap(),
                Some(auth),
            )
            .await;
        assert!(runners.success, "{label}: {:?}", runners.error);
        let runner_ids = runners.output["runners"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|runner| runner["client_id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(runner_ids, vec![CLIENT_A], "{label}");

        let projects = runtime
            .dispatch_with_auth(
                crate::tool_runtime::ToolCall::from_tool_name(
                    "list_projects",
                    json!({"summary_only": true}),
                )
                .unwrap(),
                Some(auth),
            )
            .await;
        assert!(projects.success, "{label}: {:?}", projects.error);
        let project_ids = projects.output["projects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|project| project["id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            project_ids,
            vec![format!("agent:{CLIENT_A}:{PROJECT_A}")],
            "{label}"
        );

        for guessed in [
            format!("agent:{CLIENT_B}:{PROJECT_B}"),
            PROJECT_B.to_string(),
            "definitely-missing-project".to_string(),
        ] {
            let denied = runtime
                .dispatch_with_auth(
                    crate::tool_runtime::ToolCall::ReadFiles {
                        project: guessed.clone(),
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "README.md".to_string(),
                            start_line: None,
                            limit: None,
                            expected_read_revision: None,
                        }],
                        session_id: None,
                        with_line_numbers: None,
                        max_result_bytes: None,
                    },
                    Some(auth),
                )
                .await;
            assert!(
                !denied.success,
                "{label}: guessed {guessed} unexpectedly routed"
            );
            assert_eq!(denied.output["error_kind"], "unknown_project", "{label}");
            let error = denied.error.unwrap_or_default();
            assert!(!error.contains("owned by"), "{label}: {error}");
            assert!(!error.contains("belongs to"), "{label}: {error}");
            assert!(!error.contains("/tmp/project-grant-b"), "{label}: {error}");
        }

        let exact_path = runtime
            .resolve_or_register_project(
                CLIENT_A.to_string(),
                "/tmp/project-grant-a".to_string(),
                Some(auth),
            )
            .await;
        assert!(exact_path.success, "{label}: {:?}", exact_path.error);
        assert_eq!(
            exact_path.output["outcome"], "reused_existing_registration",
            "{label}"
        );
        assert_eq!(exact_path.output["registered"], false, "{label}");

        for (operation, result) in [
            (
                "resolve sibling path",
                runtime
                    .resolve_or_register_project(
                        CLIENT_A.to_string(),
                        "/tmp/project-grant-sibling".to_string(),
                        Some(auth),
                    )
                    .await,
            ),
            (
                "prepare worktree from sibling path",
                runtime
                    .prepare_managed_worktree(
                        CLIENT_A.to_string(),
                        "/tmp/project-grant-sibling".to_string(),
                        None,
                        "grant-scope-test-op".to_string(),
                        None,
                        Some(auth),
                    )
                    .await,
            ),
            (
                "work_on_project sibling path",
                runtime
                    .dispatch_with_auth(
                        crate::tool_runtime::ToolCall::from_tool_name(
                            "work_on_project",
                            json!({
                                "client_id": CLIENT_A,
                                "path": "/tmp/project-grant-sibling",
                                "mode": "checkout",
                                "instruction": "must not expand the ProjectGrant registry"
                            }),
                        )
                        .unwrap(),
                        Some(auth),
                    )
                    .await,
            ),
        ] {
            assert!(
                !result.success,
                "{label}: {operation} unexpectedly succeeded"
            );
            assert_eq!(
                result.output["error_kind"], "project_registry_scope_denied",
                "{label}: {operation}: {:?}",
                result.error
            );
        }

        for (operation, call) in [
            (
                "register_project",
                crate::tool_runtime::ToolCall::from_tool_name(
                    "register_project",
                    json!({
                        "client_id": CLIENT_A,
                        "id": "sibling",
                        "name": "sibling",
                        "path": "/tmp/project-grant-sibling"
                    }),
                )
                .unwrap(),
            ),
            (
                "create_project",
                crate::tool_runtime::ToolCall::from_tool_name(
                    "create_project",
                    json!({
                        "client_id": CLIENT_A,
                        "id": "new-project",
                        "name": "new-project",
                        "path": "/tmp/project-grant-new"
                    }),
                )
                .unwrap(),
            ),
            (
                "unregister_project",
                crate::tool_runtime::ToolCall::from_tool_name(
                    "unregister_project",
                    json!({
                        "project": format!("agent:{CLIENT_A}:{PROJECT_A}"),
                        "expected_revision": "revision-does-not-matter"
                    }),
                )
                .unwrap(),
            ),
        ] {
            let denied = runtime.dispatch_with_auth(call, Some(auth)).await;
            assert!(
                !denied.success,
                "{label}: {operation} unexpectedly succeeded"
            );
            assert_eq!(
                denied.output["error_kind"], "project_registry_scope_denied",
                "{label}: {operation}: {:?}",
                denied.error
            );
        }

        let gateway_list = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(4100)),
                mcp_2026_params(json!({
                    "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                    "arguments": {
                        "tool": "list_projects",
                        "arguments": {"summary_only": true}
                    }
                })),
            ),
            Some(auth),
        )
        .await;
        let McpOutcome::Ok(gateway_list) = gateway_list else {
            panic!("{label}: expected gateway list_projects result");
        };
        assert_eq!(gateway_list["result"]["isError"], false, "{label}");
        let gateway_ids = gateway_list["result"]["structuredContent"]["output"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|project| project["id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            gateway_ids,
            vec![format!("agent:{CLIENT_A}:{PROJECT_A}")],
            "{label}: gateway widened ProjectGrant visibility"
        );

        let gateway_denied = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(Value::from(4101)),
                mcp_2026_params(json!({
                    "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                    "arguments": {
                        "tool": "read_files",
                        "arguments": {
                            "project": format!("agent:{CLIENT_B}:{PROJECT_B}"),
                            "items": [{"path": "README.md"}]
                        }
                    }
                })),
            ),
            Some(auth),
        )
        .await;
        let McpOutcome::Ok(gateway_denied) = gateway_denied else {
            panic!("{label}: expected canonical gateway tool result");
        };
        assert_eq!(gateway_denied["result"]["isError"], true, "{label}");
        assert_eq!(
            gateway_denied["result"]["structuredContent"]["output"]["error_kind"],
            "unknown_project",
            "{label}"
        );
        let rendered = serde_json::to_string(&gateway_denied).unwrap();
        assert!(!rendered.contains("owned by"), "{label}: {rendered}");
        assert!(!rendered.contains("belongs to"), "{label}: {rendered}");
        assert!(
            !rendered.contains("/tmp/project-grant-b"),
            "{label}: {rendered}"
        );
    }
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

#[tokio::test]
async fn mcp_2026_control_sidecars_gateway_strip_and_closed_schema() {
    let temp = tempfile::tempdir().unwrap();
    let db =
        std::sync::Arc::new(crate::db::Database::open(&temp.path().join("sidecars.db")).unwrap());
    let runtime = test_runtime().with_communication_database(db);
    let goal = runtime.create_goal_with_plan(
        None,
        crate::db::NewGoal {
            title: "MCP sidecar".into(),
            objective: "Prove wrapper stripping".into(),
            controller_agent_id: None,
            completion_conditions: vec!["contract verified".into()],
            steps: vec![crate::db::NewGoalStep {
                id: "inspect".into(),
                title: "inspect".into(),
            }],
            idempotency_key: "create".into(),
        },
    );
    assert!(goal.success);
    let goal_id = &goal.output["goal"]["summary"]["goal_id"];
    let control = json!({"before": {"goal_progress": {
        "goal_id": goal_id, "expected_revision": 1, "completed_step_ids": ["inspect"],
        "summary": "already inspected", "idempotency_key": "checkpoint"
    }}});
    for outer in [true, false] {
        let mut arguments = json!({"tool": "get_goal", "arguments": {"goal_id": goal_id}});
        if outer {
            arguments["_control"] = control.clone();
        } else {
            arguments["arguments"]["_control"] = control.clone();
        }
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(601)),
                mcp_2026_params(json!({"name": "call_runtime_tool", "arguments": arguments})),
            ),
            None,
        )
        .await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("expected structured success")
        };
        let result = &value["result"]["structuredContent"];
        assert_eq!(result["success"], true, "{value}");
        assert_eq!(result["output"]["goal"]["summary"]["revision"], 2);
        assert_eq!(result["output"]["control"]["before"]["replayed"], !outer);
        assert_eq!(result["output"]["control"]["before"]["revision"], 2);
    }
    let rejected = handle_mcp_request(&runtime, rpc("tools/call", Some(json!(602)), mcp_2026_params(json!({"name": "call_runtime_tool", "arguments": {
        "tool": "get_goal", "arguments": {"goal_id": goal_id}, "_control": {"before": {"unknown_private_value": {}}}
    }}))), None).await;
    let McpOutcome::BadRequest(value) = rejected else {
        panic!("closed wrapper must reject")
    };
    assert!(!value.to_string().contains("unknown_private_value"));
    let mut payload = json!({"tools": [
        {"name": "get_goal", "inputSchema": webcodex_tool_contracts::input_schema_for_tool("get_goal"), "outputSchema": crate::tool_runtime::registry::output_schema_for_tool("get_goal")},
        {"name": "goal_plan_sync", "inputSchema": webcodex_tool_contracts::input_schema_for_tool("goal_plan_sync")}
    ]});
    add_stateless_workflow_recorder_metadata(&mut payload);
    assert_eq!(
        payload["tools"][0]["inputSchema"]["properties"]["_control"]["properties"]["before"]
            ["maxProperties"],
        1
    );
    assert!(payload["tools"][1]["inputSchema"]["properties"]
        .get("_control")
        .is_none());
    assert!(
        webcodex_tool_contracts::input_schema_for_tool("get_goal")["properties"]
            .get("_control")
            .is_none()
    );
}
