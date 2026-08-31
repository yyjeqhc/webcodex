use super::*;

// =========================================================================
// local_coding model surface
// =========================================================================

#[tokio::test]
async fn local_coding_tools_list_returns_exact_ordered_surface() {
    // Explicit local_coding surface; names are compact-invariant, so no env
    // or lock is needed.
    let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(60)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES,
        "local_coding tools/list must be the exact ordered surface"
    );
    // Focused, not the full runtime surface.
    assert!(
        names.len() < registered_tool_specs().len(),
        "local_coding must expose fewer tools than the full runtime"
    );
    for required in [
        "work_on_project",
        "read_file",
        "read_files",
        "search_project_texts",
        "get_session_assignment",
        "complete_session_message",
        "apply_text_edits",
        "go_test",
        "finish_coding_task",
    ] {
        assert!(names.contains(&required), "missing {required}: {names:?}");
    }
    for forbidden in [
        "start_coding_task",
        "register_project",
        "unregister_project",
        "create_project",
        "start_session",
        "current_session",
        "open_session_shell",
        "session_shell_exec",
        "close_session_shell",
        "runtime_status",
        "tool_manifest",
        "workspace_checkpoint_create",
        "delete_project_files",
        "git_restore_paths",
        "discard_untracked",
    ] {
        assert!(
            !names.contains(&forbidden),
            "local_coding must not expose {forbidden}: {names:?}"
        );
    }

    let work = tools
        .iter()
        .find(|tool| tool["name"] == "work_on_project")
        .expect("local_coding work_on_project");
    let schema = &work["inputSchema"];
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "project",
        "client_id",
        "path",
        "instruction",
        "include_project_instructions",
        "include_workflow_guidance",
        "session_id",
    ] {
        assert!(props.contains_key(field), "local_coding missing {field}");
    }
    assert_eq!(props["include_project_instructions"]["default"], true);
    assert_eq!(props["include_workflow_guidance"]["default"], true);
    assert_eq!(schema["required"], json!(["instruction"]));
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema.get("oneOf").is_none());
    assert!(schema.get("not").is_none());
}

#[tokio::test]
async fn apply_text_edits_discriminated_schema_reaches_full_and_local_coding_mcp_surfaces() {
    let expected = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "apply_text_edits")
        .expect("apply_text_edits ToolSpec")
        .input_schema;
    assert_eq!(
        expected["properties"]["changes"]["items"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    for surface in [
        ModelSurface::FullOperatorRuntime,
        ModelSurface::LocalCoding,
        ModelSurface::AdaptiveRuntime,
    ] {
        let runtime = test_runtime_with_surface(surface);
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(601)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(value) => value,
            other => panic!("expected tools/list success for {surface:?}, got {other:?}"),
        };
        let schema = &value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "apply_text_edits")
            .unwrap_or_else(|| panic!("missing apply_text_edits on {surface:?}"))["inputSchema"];
        assert_eq!(schema, &expected, "schema drift on {surface:?}");
    }
}

#[tokio::test]
async fn local_coding_default_initialize_and_discovery_report_local_coding() {
    // Preserve the unset-env integration path, but confine process env state
    // to synchronous runtime construction.
    let runtime = test_runtime_from_model_surface_env(None);
    let outcome = handle_mcp_request(
        &runtime,
        rpc("initialize", Some(Value::from(61)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    assert_eq!(
        value["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_LOCAL_CODING
    );
}

#[tokio::test]
async fn local_coding_rejects_non_surface_tools_at_mcp_boundary() {
    // Explicit local_coding surface: no env or lock needed.
    let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
    for denied in [
        "start_coding_task",
        "register_project",
        "unregister_project",
        "create_project",
        "start_session",
        "open_session_shell",
        "runtime_status",
        "tool_manifest",
        "workspace_checkpoint_create",
    ] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(70)),
                json!({"name": denied, "arguments": {}}),
            ),
            None,
        )
        .await;
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32602);
                assert!(
                    value["error"]["message"].as_str().unwrap().contains(denied),
                    "denial message must name the tool: {:?}",
                    value
                );
                assert!(
                    value["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("local_coding"),
                    "denial message must name the surface: {:?}",
                    value
                );
            }
            other => panic!("{denied} must be rejected, got {:?}", other),
        }
    }
}

#[tokio::test]
async fn local_coding_allows_surface_tools_to_dispatch() {
    // Explicit local_coding surface: no env or lock needed.
    let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
    // list_projects and work_on_project resolve to the runtime registry; they
    // must reach dispatch (not be rejected at the MCP boundary).
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(71)),
            json!({"name": "list_projects", "arguments": {}}),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(value["result"]["structuredContent"]["success"], true);
        }
        other => panic!("list_projects must dispatch, got {:?}", other),
    }
}

// =========================================================================
// adaptive_runtime model surface
// =========================================================================

#[tokio::test]
async fn adaptive_runtime_tools_list_is_small_core_plus_gateway() {
    let runtime = test_runtime_with_surface(ModelSurface::AdaptiveRuntime);
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/list",
            Some(Value::from(720)),
            mcp_2026_params(json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("adaptive tools/list must succeed");
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names.len(),
        crate::model_surface::ADAPTIVE_RUNTIME_CORE_TOOL_NAMES.len() + 1,
        "adaptive surface should expose only the typed core plus one gateway"
    );
    assert_eq!(
        &names[..crate::model_surface::ADAPTIVE_RUNTIME_CORE_TOOL_NAMES.len()],
        crate::model_surface::ADAPTIVE_RUNTIME_CORE_TOOL_NAMES
    );
    assert_eq!(
        names.last().copied(),
        Some(crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
    );
    for long_tail in [
        "list_tools",
        "read_file",
        "run_shell",
        "goto_definition",
        "computer_list_windows",
        "post_session_message",
        "coding_agent_start",
        "artifact_upload_begin",
    ] {
        assert!(
            !names.contains(&long_tail),
            "{long_tail} must stay behind the adaptive gateway"
        );
    }

    let gateway = tools
        .iter()
        .find(|tool| tool["name"] == crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
        .expect("adaptive gateway");
    let properties = gateway["inputSchema"]["properties"].as_object().unwrap();
    for field in [
        "tool",
        "arguments",
        "recording_session_id",
        "ack_session_message_ids",
        "session_message_resolution",
        "context_request",
        "ack_session_context_revision",
    ] {
        assert!(
            properties.contains_key(field),
            "adaptive gateway missing stateless wrapper field {field}"
        );
    }
}

#[tokio::test]
async fn adaptive_runtime_requires_gateway_for_long_tail_and_preserves_dispatch() {
    let runtime = test_runtime_with_surface(ModelSurface::AdaptiveRuntime);
    let direct = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(721)),
            mcp_2026_params(json!({
                "name": "list_tools",
                "arguments": {"summary_only": true, "limit": 1}
            })),
        ),
        None,
    )
    .await;
    match direct {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert!(value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("adaptive_runtime"));
        }
        other => panic!("direct adaptive long-tail call must be rejected, got {other:?}"),
    }

    let gateway = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(722)),
            mcp_2026_params(json!({
                "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                "arguments": {
                    "tool": "list_tools",
                    "arguments": {"summary_only": true, "limit": 1}
                }
            })),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = gateway else {
        panic!("adaptive gateway must dispatch an allowed long-tail tool");
    };
    assert_eq!(value["result"]["structuredContent"]["success"], true);
    assert_eq!(
        value["result"]["structuredContent"]["output"]["returned_count"],
        1
    );
}

#[tokio::test]
async fn adaptive_runtime_gateway_rejects_recursive_and_unknown_targets() {
    let runtime = test_runtime_with_surface(ModelSurface::AdaptiveRuntime);
    for target in [
        crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
        "work_on_project",
        "start_coding_task",
        "not_a_real_webcodex_tool",
    ] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(723)),
                mcp_2026_params(json!({
                    "name": crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                    "arguments": {"tool": target, "arguments": {}}
                })),
            ),
            None,
        )
        .await;
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32602);
                assert!(value["error"]["message"].as_str().unwrap().contains(target));
            }
            other => panic!("target {target} must fail closed, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn full_operator_explicit_surface_lists_full_runtime_and_dispatches() {
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let listed = handle_mcp_request(
        &runtime,
        rpc(
            "tools/list",
            Some(Value::from(72)),
            mcp_2026_params(json!({})),
        ),
        None,
    )
    .await;
    let value = match listed {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let names: Vec<String> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let expected = mcp_tools_list_payload_with_features_for_auth(
        ModelSurface::FullOperatorRuntime,
        false,
        false,
        true,
        true,
        None,
    );
    let registry_names: Vec<String> = expected["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names, registry_names,
        "full operator lists the full runtime"
    );
    assert!(names.iter().any(|name| name == "read_files"));
    assert!(names.iter().any(|name| name == "search_project_texts"));

    let called = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(73)),
            json!({"name": "start_coding_task", "arguments": {}}),
        ),
        None,
    )
    .await;
    let McpOutcome::BadRequest(value) = called else {
        panic!("retired start_coding_task must fail closed");
    };
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("no longer supported"), "{message}");
    assert!(message.contains("work_on_project"), "{message}");
}

#[tokio::test]
async fn explicit_local_coding_v1_selects_local_coding() {
    let runtime = test_runtime_from_model_surface_env(Some(
        crate::model_surface::MCP_MODEL_SURFACE_LOCAL_CODING_V1,
    ));
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(74)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let names: Vec<&str> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
    );
}

#[tokio::test]
async fn explicit_adaptive_runtime_v1_reports_adaptive_surface() {
    let runtime = test_runtime_from_model_surface_env(Some(
        crate::model_surface::MCP_MODEL_SURFACE_ADAPTIVE_RUNTIME_V1,
    ));
    let outcome = handle_mcp_request(
        &runtime,
        rpc("initialize", Some(Value::from(745)), json!({})),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("adaptive initialize must succeed");
    };
    assert_eq!(
        value["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_ADAPTIVE_RUNTIME
    );
}

#[tokio::test]
async fn explicit_full_operator_v1_reports_full_operator_surface() {
    let runtime = test_runtime_from_model_surface_env(Some(
        crate::model_surface::MCP_MODEL_SURFACE_FULL_OPERATOR_V1,
    ));
    let outcome = handle_mcp_request(
        &runtime,
        rpc("initialize", Some(Value::from(75)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    assert_eq!(
        value["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
}

#[tokio::test]
async fn selected_surface_is_immutable_after_environment_changes() {
    let local = test_runtime_from_model_surface_env(None);
    // Prove the already-built runtime stays local_coding while the process env
    // actively requests the opposite surface; restore it before any await.
    with_model_surface_env(
        Some(crate::model_surface::MCP_MODEL_SURFACE_FULL_OPERATOR_V1),
        || assert_eq!(local.model_surface(), ModelSurface::LocalCoding),
    );
    for method in ["initialize", "tools/list"] {
        let outcome =
            handle_mcp_request(&local, rpc(method, Some(json!(80)), json!({})), None).await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("{method} must succeed");
        };
        if method == "initialize" {
            assert_eq!(
                value["result"]["serverInfo"]["modelSurface"],
                crate::model_surface::MODEL_SURFACE_LOCAL_CODING
            );
        } else {
            let names: Vec<&str> = value["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect();
            assert_eq!(
                names,
                crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
            );
        }
    }
    let denied = handle_mcp_request(
        &local,
        rpc(
            "tools/call",
            Some(json!(81)),
            json!({"name": "start_coding_task", "arguments": {}}),
        ),
        None,
    )
    .await;
    assert!(matches!(denied, McpOutcome::BadRequest(_)));
    let status = local.runtime_status(None).await;
    assert_eq!(
        status.output["model_surface"],
        crate::model_surface::MODEL_SURFACE_LOCAL_CODING
    );

    let full = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    with_model_surface_env(Some("broken-after-startup"), || {
        assert_eq!(full.model_surface(), ModelSurface::FullOperatorRuntime);
    });
    let listed = handle_mcp_request(
        &full,
        rpc("tools/list", Some(json!(82)), mcp_2026_params(json!({}))),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = listed else {
        panic!("full operator tools/list must remain available");
    };
    let expected = mcp_tools_list_payload_with_features_for_auth(
        ModelSurface::FullOperatorRuntime,
        false,
        false,
        true,
        true,
        None,
    );
    assert_eq!(
        value["result"]["tools"].as_array().unwrap().len(),
        expected["tools"].as_array().unwrap().len()
    );
    assert_eq!(
        full.runtime_status(None).await.output["model_surface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
}

#[tokio::test]
async fn local_coding_list_manifest_and_catalog_are_identical() {
    let runtime = test_runtime_with_surface(ModelSurface::LocalCoding);
    let listed = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(json!(83)), json!({})),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = listed else {
        panic!("tools/list must succeed");
    };
    let listed_names: Vec<&str> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    let manifest = runtime
        .dispatch(crate::tool_runtime::ToolCall::ToolManifest {
            category: None,
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    let manifest_names: Vec<&str> = manifest.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        listed_names,
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
    );
    assert_eq!(
        manifest_names,
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
    );
}
