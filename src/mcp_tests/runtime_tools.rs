use super::*;

// =========================================================================
// runtime_status via MCP tools/list and tools/call
// =========================================================================

#[tokio::test]
async fn mcp_tools_list_exposes_coding_task_and_runtime_status_ux_flags() {
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
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
    for name in ["start_coding_task", "work_on_project"] {
        let description = tool(name)["description"].as_str().unwrap();
        assert!(
            description.contains("built-in workflow guidance"),
            "{name}: {description}"
        );
        assert!(
            description.contains("project-local instructions"),
            "{name}: {description}"
        );
        assert!(
            description.contains("task instruction"),
            "tool-only consumers must learn that behavioral role is instruction-selected: {name}: {description}"
        );
        assert!(
            description.contains("grants no authority"),
            "tool-only consumers must not mistake guidance for authority: {name}: {description}"
        );
    }

    let start_schema = &tool("start_coding_task")["inputSchema"];
    assert!(
        start_schema["properties"].get("role").is_none(),
        "start_coding_task must not grow a role wire field"
    );
    let work_schema = &tool("work_on_project")["inputSchema"];
    for (name, schema) in [
        ("start_coding_task", start_schema),
        ("work_on_project", work_schema),
    ] {
        assert!(
            schema["properties"]["path"].get("pattern").is_none(),
            "{name} path schema must not encode Control-host POSIX path semantics"
        );
    }
    let work_props = work_schema["properties"]
        .as_object()
        .expect("work_on_project MCP properties");
    assert!(
        !work_props.contains_key("role"),
        "work_on_project must not grow a role wire field"
    );
    for field in ["project", "client_id", "path", "instruction", "session_id"] {
        assert!(work_props.contains_key(field), "MCP schema missing {field}");
    }
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

    let finish_props = tool("finish_coding_task")["inputSchema"]["properties"]
        .as_object()
        .expect("finish_coding_task inputSchema properties");
    assert!(
        finish_props.contains_key("include_workspace"),
        "MCP finish_coding_task schema should expose include_workspace"
    );
    let finish_required = tool("finish_coding_task")["inputSchema"]["required"]
        .as_array()
        .expect("finish_coding_task required fields");
    assert!(
        !finish_required
            .iter()
            .any(|field| field.as_str() == Some("include_workspace")),
        "include_workspace must not be required in MCP schema"
    );

    let start_props = tool("start_coding_task")["inputSchema"]["properties"]
        .as_object()
        .expect("start_coding_task inputSchema properties");
    assert_eq!(start_props["detail"]["type"], "string");
    assert_eq!(
        start_props["detail"]["enum"],
        json!(["minimal", "standard", "full"])
    );
    assert_eq!(
        start_props["execution_context"]["properties"]["default_shell"]["enum"],
        json!(["sh", "bash"])
    );
    assert_eq!(
        start_props["execution_context"]["additionalProperties"],
        false
    );
    assert!(!start_props.contains_key("tool_manifest_intent"));

    let update = tool("update_session_context");
    assert_eq!(
        update["inputSchema"]["required"],
        json!(["project", "session_id", "execution_context"])
    );
    assert_eq!(update["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        update["inputSchema"]["properties"]["execution_context"]["additionalProperties"],
        false
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

    let overview = tool("project_overview");
    let overview_props = overview["inputSchema"]["properties"]
        .as_object()
        .expect("project_overview inputSchema properties");
    for field in ["project", "path", "max_depth", "limit"] {
        assert!(
            overview_props.contains_key(field),
            "MCP project_overview schema should expose {field}"
        );
    }
    let overview_output = overview["outputSchema"]["properties"]["output"]["properties"]
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
async fn mcp_tools_list_includes_validate_patch() {
    // validate_patch is a patch preflight / dry-run tool exposed via MCP
    // tools/list (and a thin REST wrapper), but NOT via GPT Actions.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(12)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "validate_patch"),
        "MCP tools/list must include validate_patch: {:?}",
        names
    );
}

#[tokio::test]
async fn mcp_tools_list_includes_show_changes() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(13)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "show_changes"),
        "MCP tools/list must include show_changes: {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n == "git_log"),
        "MCP tools/list must include git_log: {:?}",
        names
    );
}

#[tokio::test]
async fn mcp_tools_call_runtime_status_returns_content() {
    // runtime_status is not part of the local_coding surface; select the full
    // operator surface so the call reaches dispatch.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
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
    assert!(value["result"]["content"][0]["text"].is_string());
    // structuredContent carries the ToolResult shape
    assert!(value["result"]["structuredContent"].is_object());
    assert_eq!(value["result"]["structuredContent"]["success"], true);
    let out = &value["result"]["structuredContent"]["output"];
    assert_eq!(out["service"], "webcodex");
    assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
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

#[tokio::test]
async fn mcp_tools_list_includes_project_management_tools() {
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(99)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let tools = value["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"register_project"),
        "MCP tools/list must include register_project: {:?}",
        names
    );
    assert!(
        names.contains(&"unregister_project"),
        "MCP tools/list must include unregister_project: {:?}",
        names
    );
    assert!(
        names.contains(&"create_project"),
        "MCP tools/list must include create_project: {:?}",
        names
    );
}
