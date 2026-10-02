use super::*;
fn stateless_ui_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion":MCP_STATELESS_PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities":{"extensions":{"io.modelcontextprotocol/ui":{"mimeTypes":["text/html;profile=mcp-app"]}}}
    })
}

#[tokio::test]
async fn workbench_app_descriptor_accepts_empty_input_and_preserves_existing_card() {
    let runtime = ToolRuntime::new_for_tests();
    for enabled in [false, true] {
        let request=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":stateless_ui_meta()}})).unwrap();
        let outcome = handle_with_app_policy(&runtime, request, None, enabled).await;
        let McpOutcome::Ok(value) = outcome else {
            panic!("tools list failed")
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let launcher = tools
            .iter()
            .find(|tool| tool["name"] == "open_webcodex_workbench")
            .unwrap();
        assert!(!launcher["inputSchema"]["required"]
            .as_array()
            .is_some_and(|fields| fields.contains(&json!("project"))));
        assert!(webcodex_tool_contracts::ToolCall::from_tool_name(
            "open_webcodex_workbench",
            json!({})
        )
        .is_ok());
        assert_eq!(
            launcher.pointer("/_meta/ui/resourceUri"),
            enabled.then_some(&json!(
                super::super::resources::MCP_WORKBENCH_UI_RESOURCE_URI
            ))
        );
        let card = tools
            .iter()
            .find(|tool| tool["name"] == "present_work_result")
            .unwrap();
        assert_eq!(card["inputSchema"]["required"], json!(["project"]));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "search_webcodex_resources"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "read_webcodex_resource"));
    }
    let outcome=handle_with_app_policy(&runtime,serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":super::super::resources::MCP_WORKBENCH_UI_RESOURCE_URI,"_meta":stateless_ui_meta()}})).unwrap(),None,true).await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("workbench resource failed")
    };
    assert_eq!(
        value["result"]["contents"][0]["_meta"]["openai/ui"],
        json!({"availableDisplayModes":["inline","fullscreen"],"preferredDisplayMode":"inline"})
    );
    let html = value["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(html.contains("ui/update-model-context"));
    assert!(!html.contains("ui/message"));
    assert!(!html.contains("finish_coding_task"));
}

#[tokio::test]
async fn empty_workbench_keeps_goal_access_when_project_domain_is_unavailable() {
    let runtime = ToolRuntime::new_for_tests();
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    auth.username = Some("resource-owner".into());
    auth.scopes = vec![crate::auth::SCOPE_COMMUNICATION_READ.into()];
    let result = runtime
        .open_webcodex_workbench(None, None, Some(&auth))
        .await;
    assert!(result.success);
    assert!(result.output["project"].is_null());
    assert_eq!(
        result.output["projects"]["incomplete"],
        "project_discovery_unavailable"
    );
    assert_eq!(result.output["projects"]["items"], json!([]));
    assert!(
        !runtime
            .open_webcodex_workbench(None, Some("wc_sess_unselected".into()), Some(&auth))
            .await
            .success
    );
}

#[tokio::test]
async fn workbench_view_reads_receive_canonical_text_fallback() {
    let runtime = ToolRuntime::new_for_tests();
    let request=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"open_webcodex_workbench","arguments":{},"_meta":stateless_ui_meta()}})).unwrap();
    let McpOutcome::Ok(value) = handle_with_app_policy(&runtime, request, None, true).await else {
        panic!("launcher failed")
    };
    let fallback: Value =
        serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(fallback, value["result"]["structuredContent"]);
    assert_eq!(fallback["success"], true);
    assert!(fallback["output"]["project"].is_null());
}

#[tokio::test]
async fn workbench_refresh_cannot_record_in_an_unadvertised_session() {
    let runtime = ToolRuntime::new_for_tests();
    let session = runtime
        .sessions
        .start_session(None, Some("Workbench observer".into()));
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();
    let params = mcp_2026_ui_params(
        json!({"name":"open_webcodex_workbench","arguments":{"_wc":{"record":session.session_id}}}),
    );
    let request = serde_json::from_value(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":params}),
    )
    .unwrap();
    let outcome = handle_with_app_policy(&runtime, request, None, true).await;
    assert!(matches!(
        outcome,
        McpOutcome::Ok(_) | McpOutcome::BadRequest(_)
    ));
    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(before.events.len(), after.events.len());
}
