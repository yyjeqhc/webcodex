use super::*;

// =========================================================================
// runtime_status via MCP tools/list and tools/call
// =========================================================================

// An explicit compact-schema=false override keeps full outputSchema projection.
// Serialize it with process-global env tests so a concurrent override cannot
// change the observed contract.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_list_exposes_canonical_coding_bootstrap_and_runtime_status_ux_flags() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_MCP_COMPACT_SCHEMAS", "false");
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(10)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let tool = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"))
    };
    assert!(
        tools.iter().all(|tool| tool["name"] != "start_coding_task"),
        "retired start_coding_task must stay out of MCP tools/list"
    );
    let description = tool("work_on_project")["description"].as_str().unwrap();
    let normalized_description = description.to_ascii_lowercase();
    for phrase in [
        "canonical bootstrap",
        "ordinary coding/review",
        "mode=worktree",
        "exact git base",
        "fresh workflow session",
        "exact resume",
        "primary result stays compact",
        "context_request",
        "project.instructions",
        "webcodex.workflow",
        "skills",
        "plugin",
        "without bypassing project authority",
    ] {
        assert!(
            normalized_description.contains(phrase),
            "work_on_project description should mention {phrase}: {description}"
        );
    }

    let work_schema = &tool("work_on_project")["inputSchema"];
    assert!(
        work_schema["properties"]["path"].get("pattern").is_none(),
        "work_on_project path schema must not encode Control-host POSIX path semantics"
    );
    let work_props = work_schema["properties"]
        .as_object()
        .expect("work_on_project MCP properties");
    assert!(!work_props.contains_key("include_project_instructions"));
    assert!(!work_props.contains_key("include_workflow_guidance"));
    assert!(
        !work_props.contains_key("role"),
        "work_on_project must not grow a role wire field"
    );
    for field in [
        "project",
        "client_id",
        "path",
        "mode",
        "base_ref",
        "instruction",
        "guidance_profile",
        "include_extension_catalog",
        "session_id",
    ] {
        assert!(work_props.contains_key(field), "MCP schema missing {field}");
    }
    assert!(work_props["guidance_profile"].get("default").is_none());
    assert_eq!(
        work_props["guidance_profile"]["enum"],
        if cfg!(feature = "experimental-code-mode") {
            json!(["direct", "host_code_mode", "code_mode"])
        } else {
            json!(["direct", "host_code_mode"])
        }
    );
    assert_eq!(work_props["include_extension_catalog"]["default"], true);
    assert_eq!(work_props["mode"]["enum"], json!(["checkout", "worktree"]));
    assert_eq!(work_props["mode"]["default"], "checkout");
    assert!(
        !work_schema["required"]
            .as_array()
            .expect("work_on_project required fields")
            .contains(&json!("guidance_profile")),
        "guidance_profile must remain optional in the MCP schema"
    );
    assert_eq!(work_schema["required"], json!(["instruction"]));
    assert_eq!(work_schema["additionalProperties"], false);
    for keyword in [
        "oneOf",
        "anyOf",
        "allOf",
        "not",
        "dependentRequired",
        "if",
        "then",
        "else",
    ] {
        assert!(
            work_schema.get(keyword).is_none(),
            "work_on_project MCP schema must not expose top-level {keyword}"
        );
    }

    let finish_schema = webcodex_tool_contracts::input_schema_for_tool("finish_coding_task");
    let finish_props = finish_schema["properties"]
        .as_object()
        .expect("finish_coding_task inputSchema properties");
    assert!(
        finish_props.contains_key("include_workspace"),
        "MCP finish_coding_task schema should expose include_workspace"
    );
    let finish_required = finish_schema["required"]
        .as_array()
        .expect("finish_coding_task required fields");
    assert!(
        !finish_required
            .iter()
            .any(|field| field.as_str() == Some("include_workspace")),
        "include_workspace must not be required in MCP schema"
    );

    let registered = crate::tool_runtime::registered_tool_specs();
    let registered_tool = |name: &str| {
        registered
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("missing registered ToolSpec {name}"))
    };
    assert!(tools
        .iter()
        .all(|tool| tool["name"] != "update_session_context"));
    let update = registered_tool("update_session_context");
    assert_eq!(
        update.input_schema["required"],
        json!(["project", "session_id", "execution_context"])
    );
    assert_eq!(update.input_schema["additionalProperties"], false);
    assert_eq!(
        update.input_schema["properties"]["execution_context"]["additionalProperties"],
        false
    );
    assert_eq!(
        crate::model_surface::adaptive_runtime_tool_invocation_route("update_session_context"),
        ("gateway", Some("call_runtime_tool"))
    );

    let runtime_props = tool("runtime_status")["inputSchema"]["properties"]
        .as_object()
        .expect("runtime_status inputSchema properties");
    for field in ["compact", "summary_only"] {
        assert!(
            runtime_props.contains_key(field),
            "MCP runtime_status schema should expose {field}"
        );
        assert_eq!(runtime_props[field]["type"], "boolean");
    }

    assert!(tools.iter().all(|tool| tool["name"] != "project_overview"));
    let overview = registered_tool("project_overview");
    assert_eq!(
        crate::model_surface::adaptive_runtime_tool_invocation_route("project_overview"),
        ("gateway", Some("call_runtime_tool"))
    );
    let overview_props = overview.input_schema["properties"]
        .as_object()
        .expect("project_overview inputSchema properties");
    for field in ["project", "path", "max_depth", "limit"] {
        assert!(
            overview_props.contains_key(field),
            "MCP project_overview schema should expose {field}"
        );
    }
    let overview_output = overview.output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("project_overview outputSchema properties");
    for field in ["project_types", "key_files", "top_level", "scan"] {
        assert!(
            overview_output.contains_key(field),
            "MCP project_overview output schema should expose {field}"
        );
    }
}

#[tokio::test]
async fn mcp_tools_call_runtime_status_returns_content() {
    // runtime_status is part of the canonical Adaptive direct set.
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(11)),
            json!({"name": "runtime_status", "arguments": {}}),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    assert_eq!(value["id"], 11);
    // content blocks
    assert!(value["result"]["content"].is_array());
    assert_eq!(value["result"]["content"][0]["type"], "text");
    assert_eq!(
        value["result"]["content"][0]["text"],
        "WebCodex tool completed successfully."
    );
    // structuredContent carries the ToolResult shape exactly once; content.text
    // must not serialize the structured payload again.
    assert!(value["result"]["structuredContent"].is_object());
    assert_eq!(value["result"]["structuredContent"]["success"], true);
    let out = &value["result"]["structuredContent"]["output"];
    assert_eq!(out["service"], "webcodex");
    assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
    assert!(!value["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains(env!("CARGO_PKG_VERSION")));
    // runtime_status never errors on a failed-projects runtime — it
    // reports configured=false instead.
    assert_eq!(value["result"]["isError"], false);
}

#[tokio::test]
async fn mcp_tools_call_show_changes_returns_structured_tool_error() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(14)),
            json!({
                "name": "show_changes",
                "arguments": {"project": "agent:nope:nope"}
            }),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    assert_eq!(value["id"], 14);
    assert_eq!(value["result"]["isError"], true);
    assert_eq!(value["result"]["structuredContent"]["success"], false);
    assert_eq!(
        value["result"]["structuredContent"]["output"]["error_kind"],
        "unknown_project"
    );
}

#[test]
fn mcp_suggested_call_output_schema_tracks_adaptive_route() {
    let adaptive = mcp_tools_list_payload_with_compact(false);
    let adaptive_work = adaptive["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "work_on_project")
        .expect("Adaptive work_on_project");
    let adaptive_call =
        &adaptive_work["outputSchema"]["properties"]["output"]["properties"]["suggested_call"];
    assert_eq!(
        adaptive_call["properties"]["tool"]["const"],
        crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert_eq!(
        adaptive_call["properties"]["arguments"]["properties"]["tool"]["const"],
        "list_runners"
    );
    assert!(
        crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "skill_versions",
            true
        )
    );
    assert!(
        !crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "skill_versions",
            false
        ),
        "ModelHidden Skill management recovery must require the stateless operator-extension admission context"
    );
}

#[tokio::test]
async fn adaptive_mcp_work_on_project_recovery_is_immediately_gateway_callable() {
    let root = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root.path())
        .status()
        .expect("git init");
    assert!(status.success());
    let path = root
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let runtime = test_runtime();

    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(1493)),
            mcp_2026_params(json!({
                "name": "work_on_project",
                "arguments": {
                    "client_id": "missing-493-runner",
                    "path": path,
                    "instruction": "recover the unknown Runner"
                }
            })),
        ),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(value) => value,
        other => panic!("expected MCP tool result, got {other:?}"),
    };
    let suggested = &value["result"]["structuredContent"]["output"]["suggested_call"];
    assert_eq!(
        suggested["tool"],
        crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert_eq!(suggested["arguments"]["tool"], "list_runners");
    assert_eq!(
        suggested["arguments"]["arguments"],
        json!({"include_projects": false, "summary_only": true})
    );

    let projected_tool = suggested["tool"].as_str().unwrap().to_string();
    let projected_arguments = suggested["arguments"].clone();
    let recovery = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(1494)),
            mcp_2026_params(json!({
                "name": projected_tool,
                "arguments": projected_arguments
            })),
        ),
        None,
    )
    .await;
    let recovery = match recovery {
        McpOutcome::Ok(value) => value,
        other => panic!("projected recovery must pass MCP gateway admission: {other:?}"),
    };
    assert_eq!(recovery["result"]["structuredContent"]["success"], true);
}

#[tokio::test]
async fn mcp_runtime_status_defaults_sparse_preserves_explicit_full_and_gateway_parity() {
    let runtime = test_runtime();
    for gateway in [false, true] {
        for (arguments, sparse) in [
            (json!({}), true),
            (Value::Null, true),
            (json!({"compact": true}), true),
            (json!({"compact": false}), false),
            (json!({"compact": false, "summary_only": true}), true),
        ] {
            let params = if gateway {
                adaptive_runtime_gateway_params("runtime_status", arguments)
            } else {
                json!({"name": "runtime_status", "arguments": arguments})
            };
            let McpOutcome::Ok(value) =
                handle_mcp_request(&runtime, rpc("tools/call", Some(json!(1)), params), None).await
            else {
                panic!("status call")
            };
            let output = &value["result"]["structuredContent"]["output"];
            assert_eq!(value["result"]["structuredContent"]["success"], true);
            assert_eq!(output.get("authority").is_none(), sparse);
            assert_eq!(output.get("mcp_host").is_some(), sparse);
            if !sparse {
                for field in [
                    "effective_config",
                    "session_store",
                    "quic",
                    "version_compatibility",
                    "connection_layers",
                    "tools",
                ] {
                    assert!(output.get(field).is_some(), "{field}");
                }
            }
        }
    }
    let McpOutcome::Ok(manifest) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(2)),
            mcp_2026_params(json!({
                "name": "tool_manifest",
                "arguments": {"tool_name": "runtime_status"}
            })),
        ),
        None,
    )
    .await
    else {
        panic!("runtime_status manifest")
    };
    let manifest_output = &manifest["result"]["structuredContent"]["output"];
    assert_eq!(manifest_output["name"], "runtime_status");
    assert_eq!(
        manifest_output["input_schema"]["properties"]["compact"]["default"],
        true
    );
    assert!(
        manifest_output["input_schema"]["properties"]["compact"]["description"]
            .as_str()
            .unwrap()
            .contains("MCP defaults to sparse status")
    );

    let canonical = runtime
        .dispatch(
            crate::tool_runtime::ToolCall::from_tool_name("runtime_status", json!({})).unwrap(),
        )
        .await;
    assert!(canonical.output.get("authority").is_some());
    assert_eq!(
        webcodex_tool_contracts::input_schema_for_tool("runtime_status")["properties"]["compact"]
            ["default"],
        false,
        "canonical/API runtime_status default must remain full"
    );
}
