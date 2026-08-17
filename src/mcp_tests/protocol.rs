use super::*;

#[test]
fn rpc_result_envelope_is_valid() {
    let value = rpc_result(Some(Value::from(1)), json!({"ok": true}));
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert_eq!(value["result"]["ok"], true);
}

#[test]
fn rpc_error_envelope_carries_code_and_message() {
    let value = rpc_error(Some(Value::from("a")), -32601, "missing");
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], "a");
    assert_eq!(value["error"]["code"], -32601);
    assert_eq!(value["error"]["message"], "missing");
}

#[tokio::test]
async fn mcp_initialize_returns_protocol_and_server_info() {
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let outcome = handle_mcp_request(
        &runtime,
        rpc("initialize", Some(Value::from(1)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(value["jsonrpc"], "2.0");
            assert_eq!(value["id"], 1);
            assert_eq!(value["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
            assert_eq!(value["result"]["serverInfo"]["name"], "webcodex");
            assert!(value["result"]["serverInfo"]["version"].is_string());
            assert_eq!(
                value["result"]["serverInfo"]["modelSurface"],
                crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
            );
            assert_eq!(
                value["result"]["capabilities"]["tools"]["listChanged"],
                false
            );
        }
        other => panic!("expected Ok, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_ping_returns_empty_result() {
    let runtime = test_runtime();
    let outcome =
        handle_mcp_request(&runtime, rpc("ping", Some(Value::from(2)), json!({})), None).await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(value["id"], 2);
            assert!(value["result"].is_object());
            assert!(value["result"].as_object().unwrap().is_empty());
        }
        other => panic!("expected Ok, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_2026_ping_is_removed() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("ping", Some(Value::from(2002)), mcp_2026_params(json!({}))),
        None,
    )
    .await;
    match outcome {
        McpOutcome::NotFound(value) => assert_eq!(value["error"]["code"], -32601),
        other => panic!("modern ping must be method-not-found, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_info_advertised_methods_match_dispatch() {
    let runtime = test_runtime();
    for method in MCP_INFO_METHODS {
        let params = match *method {
            "server/discover" | "resources/list" => mcp_2026_ui_params(json!({})),
            "resources/read" => mcp_2026_ui_params(json!({ "uri": MCP_COMPUTER_UI_RESOURCE_URI })),
            _ => json!({}),
        };
        let outcome = handle_mcp_request(&runtime, rpc(method, Some(json!(1)), params), None).await;
        assert!(
            !matches!(&outcome, McpOutcome::BadRequest(value) if value["error"]["code"] == -32601),
            "advertised method {method} must be dispatchable"
        );
    }
    let outcome = handle_mcp_request(
        &runtime,
        rpc("not/a/method", Some(json!(1)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::BadRequest(value) => assert_eq!(value["error"]["code"], -32601),
        other => panic!("unknown method must return -32601, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_server_discover_advertises_modern_and_legacy_protocols() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "server/discover",
            Some(Value::from(6)),
            json!({
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(
                value["result"]["supportedVersions"],
                json!([MCP_STATELESS_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION])
            );
            assert_eq!(
                value["result"]["capabilities"]["tools"]["listChanged"],
                false
            );
            assert_eq!(value["result"]["resultType"], "complete");
            assert_eq!(value["result"]["ttlMs"], 0);
            assert_eq!(value["result"]["cacheScope"], "private");
            assert_eq!(
                value["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
                "webcodex"
            );
        }
        other => panic!("expected Ok for server/discover, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_legacy_server_discover_is_method_not_found() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("server/discover", Some(Value::from(61)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::BadRequest(value) => assert_eq!(value["error"]["code"], -32601),
        other => panic!("legacy server/discover must be method-not-found, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_stateless_tools_list_uses_2026_result_shape() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/list",
            Some(Value::from(7)),
            json!({
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert!(value["result"]["tools"].is_array());
            assert_eq!(value["result"]["resultType"], "complete");
            assert_eq!(value["result"]["ttlMs"], 0);
            assert_eq!(value["result"]["cacheScope"], "private");
            assert_eq!(
                value["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
                "webcodex"
            );
        }
        other => panic!("expected Ok for stateless tools/list, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_legacy_tools_list_omits_2026_only_result_fields() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(8)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert!(value["result"]["tools"].is_array());
            assert!(value["result"].get("resultType").is_none());
            assert!(value["result"].get("ttlMs").is_none());
            assert!(value["result"].get("cacheScope").is_none());
        }
        other => panic!("expected Ok for legacy tools/list, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_unknown_method_is_bad_request() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("resources/list", Some(Value::from(6)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32601);
            assert!(value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("resources/list"));
        }
        other => panic!("expected BadRequest, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_rejects_non_2_0_jsonrpc() {
    let runtime = test_runtime();
    let request = JsonRpcRequest {
        jsonrpc: Some("1.0".to_string()),
        method: "initialize".to_string(),
        params: json!({}),
        id: Some(Value::from(7)),
    };
    let outcome = handle_mcp_request(&runtime, request, None).await;
    match outcome {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32600);
            assert_eq!(value["id"], 7);
        }
        other => panic!("expected BadRequest, got {:?}", other),
    }
}

#[tokio::test]
async fn mcp_notification_without_id_yields_no_response_body() {
    // A notification (request without an `id` member) must not produce a
    // JSON-RPC response body. This covers `notifications/initialized`
    // which MCP clients send after `initialize` completes.
    let runtime = test_runtime();
    let request = JsonRpcRequest {
        jsonrpc: Some("2.0".to_string()),
        method: "notifications/initialized".to_string(),
        params: json!({}),
        id: None,
    };
    let outcome = handle_mcp_request(&runtime, request, None).await;
    assert!(
        matches!(outcome, McpOutcome::Notification),
        "expected Notification, got {:?}",
        outcome
    );
}

#[tokio::test]
async fn mcp_notification_unknown_method_also_silent() {
    // Any id-less request is a notification and is accepted silently,
    // even if the method is not recognized.
    let runtime = test_runtime();
    let request = JsonRpcRequest {
        jsonrpc: Some("2.0".to_string()),
        method: "notifications/cancelled".to_string(),
        params: json!({}),
        id: None,
    };
    let outcome = handle_mcp_request(&runtime, request, None).await;
    assert!(matches!(outcome, McpOutcome::Notification));
}

#[tokio::test]
async fn mcp_notifications_initialized_with_id_returns_result() {
    // If a client (incorrectly) sends notifications/initialized with an
    // id, we still treat it as a normal request and return a result.
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("notifications/initialized", Some(Value::from(9)), json!({})),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(value) => {
            assert_eq!(value["id"], 9);
            assert!(value["result"].is_object());
        }
        other => panic!("expected Ok, got {:?}", other),
    }
}
