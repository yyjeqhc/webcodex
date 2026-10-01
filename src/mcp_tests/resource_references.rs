use super::*;
use std::sync::Arc;

#[tokio::test]
async fn resource_reads_match_canonical_tool_and_work_without_apps_in_both_eras() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests().with_communication_database(Arc::new(
        crate::Database::open(&tmp.path().join("resource.db")).unwrap(),
    ));
    let goal = runtime.create_goal(
        None,
        "Resource MCP".into(),
        "latest objective".into(),
        "resource-mcp".into(),
    );
    assert!(goal.success);
    let found = runtime
        .search_webcodex_resources(
            webcodex_tool_contracts::tool_call::WebcodexResourceKind::Goal,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(found.success);
    let uri = found.output["items"][0]["uri"].as_str().unwrap();
    let canonical = runtime.read_webcodex_resource(uri, None).await;
    for stateless in [false, true] {
        let meta = if stateless {
            json!({"io.modelcontextprotocol/protocolVersion":MCP_STATELESS_PROTOCOL_VERSION})
        } else {
            json!({})
        };
        let read=super::work_result_app::handle_with_server_apps_enabled(&runtime,serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":uri,"_meta":meta}})).unwrap(),None,false).await;
        let read = match read {
            McpOutcome::Ok(v) => v,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            serde_json::from_str::<Value>(read["result"]["contents"][0]["text"].as_str().unwrap())
                .unwrap(),
            canonical.output
        );
        let tool=super::work_result_app::handle_with_server_apps_enabled(&runtime,serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_webcodex_resource","arguments":{"uri":uri},"_meta":meta}})).unwrap(),None,false).await;
        let tool = match tool {
            McpOutcome::Ok(v) => v,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            tool["result"]["structuredContent"]["output"],
            canonical.output
        );
    }
}
