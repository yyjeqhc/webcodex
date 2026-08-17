use super::*;

#[test]
fn mcp_2026_ui_capability_detection_is_explicit_and_mime_aware() {
    assert!(!request_supports_mcp_apps(&mcp_2026_params(json!({}))));
    assert!(request_supports_mcp_apps(&mcp_2026_ui_params(json!({}))));
    let extension_without_mime = json!({
        "_meta": {
            "io.modelcontextprotocol/clientCapabilities": {
                "extensions": { MCP_UI_EXTENSION: {} }
            }
        }
    });
    assert!(!request_supports_mcp_apps(&extension_without_mime));
    let incompatible = json!({
        "_meta": {
            "io.modelcontextprotocol/clientCapabilities": {
                "extensions": {
                    MCP_UI_EXTENSION: { "mimeTypes": ["text/plain"] }
                }
            }
        }
    });
    assert!(!request_supports_mcp_apps(&incompatible));
    assert!(model_surface_supports_computer_app(
        ModelSurface::FullOperatorRuntime
    ));
    assert!(!model_surface_supports_computer_app(
        ModelSurface::LocalCoding
    ));
    assert!(!model_surface_supports_computer_app(
        ModelSurface::CanonicalConnector
    ));
}

#[tokio::test]
async fn mcp_2026_computer_app_is_minimal_handshake_and_snapshot_only() {
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    // The URI is a host cache key. Bump it whenever the App delivery contract
    // changes so a previously failed/blank iframe cannot pin the old resource.
    assert_eq!(MCP_COMPUTER_UI_RESOURCE_URI, "ui://webcodex/computer/v11");
    assert!(MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/computer/v10"));
    assert_eq!(MCP_COMPUTER_UI_RESOURCE_TTL_MS, 0);
    let expected_resource_meta = json!({
        "ui": {
            "prefersBorder": true,
            "domain": MCP_COMPUTER_UI_DOMAIN,
            "csp": {
                "connectDomains": [],
                "resourceDomains": []
            }
        }
    });

    let discover = handle_mcp_request(
        &runtime,
        rpc(
            "server/discover",
            Some(json!(2101)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(discover) = discover else {
        panic!("expected modern discovery");
    };
    assert_eq!(
        discover["result"]["capabilities"]["resources"]["listChanged"],
        false
    );
    assert_eq!(
        discover["result"]["capabilities"]["extensions"][MCP_UI_EXTENSION]["mimeTypes"][0],
        MCP_UI_RESOURCE_MIME_TYPE
    );

    let tools = handle_mcp_request(
        &runtime,
        rpc(
            "tools/list",
            Some(json!(2102)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(tools) = tools else {
        panic!("expected UI-enabled tools/list");
    };
    let tools = tools["result"]["tools"].as_array().unwrap();
    let snapshot = tools
        .iter()
        .find(|tool| tool["name"] == "computer_snapshot")
        .unwrap();
    assert_eq!(
        snapshot["_meta"],
        json!({
            "ui": {
                "resourceUri": MCP_COMPUTER_UI_RESOURCE_URI,
                "visibility": ["model", "app"]
            },
            "openai/outputTemplate": MCP_COMPUTER_UI_RESOURCE_URI
        })
    );
    let list_windows = tools
        .iter()
        .find(|tool| tool["name"] == "computer_list_windows")
        .unwrap();
    assert!(list_windows.get("_meta").is_none());

    let compact =
        mcp_tools_list_payload_with_compact_and_app(ModelSurface::FullOperatorRuntime, true, true);
    let compact_snapshot = compact["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "computer_snapshot")
        .unwrap();
    assert_eq!(
        compact_snapshot["_meta"],
        json!({
            "ui": {
                "resourceUri": MCP_COMPUTER_UI_RESOURCE_URI,
                "visibility": ["model", "app"]
            },
            "openai/outputTemplate": MCP_COMPUTER_UI_RESOURCE_URI
        })
    );

    let resources = handle_mcp_request(
        &runtime,
        rpc(
            "resources/list",
            Some(json!(2103)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(resources) = resources else {
        panic!("expected UI resources/list");
    };
    assert_eq!(
        resources["result"]["resources"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );
    assert_eq!(
        resources["result"]["resources"][0]["mimeType"],
        MCP_UI_RESOURCE_MIME_TYPE
    );
    assert_eq!(
        resources["result"]["resources"][0]["_meta"],
        expected_resource_meta
    );

    let resource = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2104)),
            mcp_2026_ui_params(json!({ "uri": MCP_COMPUTER_UI_RESOURCE_URI })),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(resource) = resource else {
        panic!("expected UI resources/read");
    };
    let resource_meta = &resource["result"]["contents"][0]["_meta"];
    assert_eq!(
        resource["result"]["ttlMs"],
        Value::from(MCP_COMPUTER_UI_RESOURCE_TTL_MS)
    );
    assert_eq!(resource["result"]["cacheScope"], "private");
    assert_eq!(resource_meta, &expected_resource_meta);
    assert!(resource_meta.get("openai/widgetDomain").is_none());
    let html = resource["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(html.starts_with("<div id=\"app\""));
    for expected in [
        "HTML loaded",
        "Minimal MCP Apps handshake",
        "ui/initialize",
        "2026-01-26",
        "appCapabilities: {}",
        "ui/notifications/initialized",
        "ui/notifications/tool-result",
        "pending.set(id",
        "window.parent.postMessage",
        "content_delivery",
        "mcp_image",
        ";base64,",
        "output.client_id",
    ] {
        assert!(
            html.contains(expected),
            "missing {expected} in computer app HTML"
        );
    }
    for forbidden in [
        "<!DOCTYPE html>",
        "<script type=\"module\">",
        "ui/notifications/tool-input",
        "hostCapabilities",
        "availableDisplayModes",
        "tools/call",
        "ui/request-display-mode",
        "ui/update-model-context",
        "ui/message",
        "ui/resource-teardown",
        "atob(",
        "computer_list_windows",
        "content_base64",
        "innerHTML",
        "localStorage",
        "indexedDB",
        "console.log",
        "URL.createObjectURL",
        "URL.revokeObjectURL",
        "new Blob",
        "<button",
    ] {
        assert!(
            !html.contains(forbidden),
            "minimal computer app HTML must not contain {forbidden}"
        );
    }

    for legacy_uri in MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS {
        let legacy = handle_mcp_request(
            &runtime,
            rpc(
                "resources/read",
                Some(json!(21041)),
                mcp_2026_params(json!({ "uri": legacy_uri })),
            ),
            None,
        )
        .await;
        let McpOutcome::Ok(legacy) = legacy else {
            panic!("legacy advertised UI resource must remain readable: {legacy_uri}");
        };
        assert_eq!(legacy["result"]["contents"][0]["uri"], *legacy_uri);
        assert_eq!(legacy["result"]["ttlMs"], 0);
        assert_eq!(legacy["result"]["cacheScope"], "private");
        assert_eq!(
            legacy["result"]["contents"][0]["mimeType"],
            MCP_UI_RESOURCE_MIME_TYPE
        );
        assert_eq!(
            legacy["result"]["contents"][0]["_meta"],
            expected_resource_meta
        );
        assert_eq!(
            legacy["result"]["contents"][0]["text"].as_str(),
            Some(MCP_COMPUTER_APP_HTML)
        );
    }

    let unknown = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2105)),
            mcp_2026_ui_params(json!({ "uri": "ui://webcodex/computer/unknown" })),
        ),
        None,
    )
    .await;
    match unknown {
        McpOutcome::BadRequest(value) => assert_eq!(value["error"]["code"], -32602),
        other => panic!("unknown UI resource must fail closed, got {other:?}"),
    }

    let tools_without_ui_capability = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(json!(2106)), mcp_2026_params(json!({}))),
        None,
    )
    .await;
    let McpOutcome::Ok(tools_without_ui_capability) = tools_without_ui_capability else {
        panic!("expected tools/list without UI capability metadata");
    };
    let snapshot = tools_without_ui_capability["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "computer_snapshot")
        .unwrap();
    assert_eq!(
        snapshot["_meta"],
        json!({
            "ui": {
                "resourceUri": MCP_COMPUTER_UI_RESOURCE_URI,
                "visibility": ["model", "app"]
            },
            "openai/outputTemplate": MCP_COMPUTER_UI_RESOURCE_URI
        })
    );

    let no_ui_resources = handle_mcp_request(
        &runtime,
        rpc(
            "resources/list",
            Some(json!(2107)),
            mcp_2026_params(json!({})),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(no_ui_resources) = no_ui_resources else {
        panic!("expected non-UI resources/list");
    };
    assert!(no_ui_resources["result"]["resources"]
        .as_array()
        .unwrap()
        .is_empty());

    let resource_without_repeated_ui_capability = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2108)),
            mcp_2026_params(json!({ "uri": MCP_COMPUTER_UI_RESOURCE_URI })),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(resource_without_repeated_ui_capability) =
        resource_without_repeated_ui_capability
    else {
        panic!(
            "advertised UI resource must remain readable without repeated UI capability metadata"
        );
    };
    assert_eq!(
        resource_without_repeated_ui_capability["result"]["contents"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );
    assert_eq!(
        resource_without_repeated_ui_capability["result"]["contents"][0]["mimeType"],
        MCP_UI_RESOURCE_MIME_TYPE
    );
}

#[test]
fn mcp_computer_snapshot_output_schema_matches_native_image_framing() {
    let runtime_spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "computer_snapshot")
        .expect("computer_snapshot runtime spec");
    let runtime_properties = runtime_spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("runtime computer_snapshot output properties");
    assert!(runtime_properties.contains_key("content_base64"));
    assert!(!runtime_properties.contains_key("content_delivery"));

    for app_enabled in [false, true] {
        let payload = mcp_tools_list_payload_with_compact_and_app(
            ModelSurface::FullOperatorRuntime,
            false,
            app_enabled,
        );
        let tool = payload["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "computer_snapshot")
            .expect("MCP computer_snapshot descriptor");
        let properties = tool["outputSchema"]["properties"]["output"]["properties"]
            .as_object()
            .expect("MCP computer_snapshot output properties");
        assert!(!properties.contains_key("content_base64"));
        assert_eq!(properties["content_delivery"]["type"], "string");
        assert_eq!(properties["content_delivery"]["const"], "mcp_image");
    }
}
