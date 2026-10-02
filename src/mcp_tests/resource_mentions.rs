use super::work_result_app::handle_with_server_apps_enabled;
use super::*;
use std::sync::Arc;

#[tokio::test]
async fn workbench_native_metadata_and_mentions_match_extension_contract() {
    for app in [false, true] {
        let mut listed = super::super::tools::mcp_tools_list_payload_with_features_for_auth(
            false, app, true, None,
        );
        super::super::tools::add_stateless_workflow_recorder_metadata(&mut listed);
        let tools = listed["tools"].as_array().unwrap();
        let launcher = tools
            .iter()
            .find(|tool| tool["name"] == "open_webcodex_workbench")
            .unwrap();
        assert_eq!(
            launcher["title"],
            app.then_some(json!("Projects & Resources"))
                .unwrap_or(Value::Null)
        );
        assert_eq!(
            launcher.pointer("/_meta/openai~1ui/entrypoints"),
            app.then_some(&json!([{"type":"global"},{"type":"thread"}]))
        );
        let mentions = tools.iter().find(|tool| tool["name"] == "search_mentions");
        assert_eq!(mentions.is_some(), app);
        if let Some(mentions) = mentions {
            assert_eq!(
                mentions.pointer("/_meta/openai~1extensions/mentions~1search"),
                Some(&json!({}))
            );
            assert_eq!(
                mentions.pointer("/_meta/ui/visibility"),
                Some(&json!(["app"]))
            );
            assert_eq!(mentions["inputSchema"]["required"], json!(["query"]));
            assert_eq!(
                mentions["inputSchema"]["properties"],
                json!({"query":{"type":"string","maxLength":200}})
            );
            assert_eq!(mentions["outputSchema"]["required"], json!(["items"]));
            assert_eq!(mentions["annotations"]["readOnlyHint"], true);
            assert!(!mentions["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .contains_key("project"));
        }
    }
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "search_mentions",
            true
        )
    );
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "search_mentions"));
}

#[tokio::test]
async fn native_mentions_preserve_goal_owner_and_independent_domain_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests().with_communication_database(Arc::new(
        crate::Database::open(&temp.path().join("mentions.db")).unwrap(),
    ));
    let mut owner = mcp_export_api_auth("owner-key", "owner");
    owner.scopes = vec![crate::auth::SCOPE_COMMUNICATION_READ.into()];
    assert!(
        runtime
            .create_goal(
                Some(&owner),
                "Native reference".into(),
                "literal %_ goal".into(),
                "mentions".into()
            )
            .success
    );
    let call = |query: &str| {
        serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":mcp_2026_ui_params(json!({"name":"search_mentions","arguments":{"query":query}}))})).unwrap()
    };
    let McpOutcome::Ok(result) =
        handle_with_server_apps_enabled(&runtime, call("%_"), Some(&owner), true).await
    else {
        panic!("mentions failed")
    };
    assert_eq!(result["result"]["content"], json!([]));
    let items = result["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], "resource_link");
    assert_eq!(items[0]["_meta"]["kind"], "goal");
    assert!(result["result"]["structuredContent"]
        .get("output")
        .is_none());
    let mut other = mcp_export_api_auth("other-key", "other");
    other.scopes = owner.scopes.clone();
    let McpOutcome::Ok(foreign) =
        handle_with_server_apps_enabled(&runtime, call(""), Some(&other), true).await
    else {
        panic!("foreign mentions failed")
    };
    assert_eq!(foreign["result"]["structuredContent"]["items"], json!([]));
    assert!(!foreign.to_string().contains("Native reference"));
    assert!(matches!(
        handle_with_server_apps_enabled(&runtime, call(""), Some(&owner), false).await,
        McpOutcome::BadRequest(_)
    ));
    let legacy=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_mentions","arguments":{"query":""}}})).unwrap();
    assert!(matches!(
        handle_with_server_apps_enabled(&runtime, legacy, Some(&owner), true).await,
        McpOutcome::BadRequest(_)
    ));
    let bad=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":mcp_2026_ui_params(json!({"name":"search_mentions","arguments":{"query":"","project":"hidden-target"}}))})).unwrap();
    assert!(matches!(
        handle_with_server_apps_enabled(&runtime, bad, Some(&owner), true).await,
        McpOutcome::BadRequest(_)
    ));
}
