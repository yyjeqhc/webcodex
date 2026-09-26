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
}

#[tokio::test]
async fn mcp_2026_computer_app_is_minimal_handshake_and_snapshot_only() {
    const PUBLIC_URL: &str = "https://self-host.example";
    let runtime = test_runtime_with_public_url(PUBLIC_URL);
    // The URI is a host cache key. Bump it whenever the App delivery contract
    // changes so a previously failed/blank iframe cannot pin the old resource.
    assert_eq!(MCP_COMPUTER_UI_RESOURCE_URI, "ui://webcodex/computer/v12");
    assert!(MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/computer/v11"));
    assert_eq!(MCP_COMPUTER_UI_RESOURCE_TTL_MS, 0);
    assert!(mcp_computer_app_resource_meta(None)["ui"]
        .get("domain")
        .is_none());
    let expected_resource_meta = json!({
        "ui": {
            "prefersBorder": true,
            "domain": PUBLIC_URL,
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
    assert!(tools
        .iter()
        .any(|tool| { tool["name"] == crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME }));
    assert!(
        !tools.iter().any(|tool| tool["name"] == "computer_observe"),
        "Computer observation remains model-visible long tail and must use call_runtime_tool"
    );
    for retired in [
        "computer_snapshot",
        "computer_snapshot_display",
        "computer_list_windows",
    ] {
        assert!(!tools.iter().any(|tool| tool["name"] == retired));
    }

    let compact = mcp_tools_list_payload_with_compact_and_app(true, true);
    assert!(!compact["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "computer_observe"));

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
    let names = tools_without_ui_capability["result"]["tools"]
        .as_array()
        .unwrap();
    assert!(names
        .iter()
        .any(|tool| { tool["name"] == crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME }));
    assert!(!names.iter().any(|tool| tool["name"] == "computer_observe"));

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

#[tokio::test]
async fn mcp_computer_snapshot_resource_links_are_unique_caller_bound_and_scope_checked() {
    fn snapshot_auth(api_key_id: &str, include_display_scope: bool) -> crate::auth::AuthContext {
        let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
        auth.api_key_id = Some(api_key_id.to_string());
        auth.token_kind = Some("user".to_string());
        auth.scopes = vec![crate::auth::SCOPE_COMPUTER_READ.to_string()];
        if include_display_scope {
            auth.scopes
                .push(crate::auth::SCOPE_COMPUTER_DISPLAY_READ.to_string());
        }
        auth
    }

    let runtime = test_runtime();
    let auth = snapshot_auth("snapshot-link-owner", true);
    let caller = mcp_artifact_export_caller_binding(Some(&auth)).unwrap();
    let bytes = vec![0xff, 0xd8, 0xff, 0xd9];
    let encoded = general_purpose::STANDARD.encode(&bytes);
    let mut uris = Vec::new();

    for generation in 1..=4u64 {
        let framed = mcp_runtime_tool_result_with_snapshot_resource(
            "computer_observe",
            false,
            ToolResult::ok(json!({
                "client_id": "msi",
                "display_id": "display_iavN7wEjRWeJq83v",
                "snapshot_generation": generation,
                "source_width": 1,
                "source_height": 1,
                "width": 1,
                "height": 1,
                "mime_type": "image/jpeg",
                "file_bytes": bytes.len(),
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "captured_at_unix_ms": 1,
                "content_base64": encoded,
            })),
            Some(caller.clone()),
        );
        assert_eq!(framed["isError"], false);
        let content = framed["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "resource_link");
        assert_eq!(content[0]["mimeType"], "image/jpeg");
        assert_eq!(content[0]["name"], "msi-display-snapshot.jpg");
        assert_eq!(content[0]["size"], Value::from(bytes.len() as u64));
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[2]["type"], "image");
        let uri = content[0]["uri"].as_str().unwrap().to_string();
        assert!(uri.starts_with(MCP_SNAPSHOT_RESOURCE_URI_PREFIX));
        assert!(!serde_json::to_string(&framed["structuredContent"])
            .unwrap()
            .contains(MCP_SNAPSHOT_RESOURCE_URI_PREFIX));
        uris.push(uri);
    }

    let unique: std::collections::HashSet<_> = uris.iter().collect();
    assert_eq!(unique.len(), 4);
    let uri = uris.last().unwrap();

    let read = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2110)),
            mcp_2026_params(json!({ "uri": uri })),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(read) = read else {
        panic!("snapshot resource must be readable by its caller");
    };
    let contents = &read["result"]["contents"][0];
    assert_eq!(contents["uri"], *uri);
    assert_eq!(contents["mimeType"], "image/jpeg");
    assert_eq!(
        general_purpose::STANDARD
            .decode(contents["blob"].as_str().unwrap())
            .unwrap(),
        bytes
    );

    let narrowed = snapshot_auth("snapshot-link-owner", false);
    let denied = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2111)),
            mcp_2026_params(json!({ "uri": uri })),
        ),
        Some(&narrowed),
    )
    .await;
    assert!(matches!(denied, McpOutcome::Forbidden { .. }));

    let other = snapshot_auth("snapshot-link-other", true);
    let hidden = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2112)),
            mcp_2026_params(json!({ "uri": uri })),
        ),
        Some(&other),
    )
    .await;
    let McpOutcome::BadRequest(hidden) = hidden else {
        panic!("snapshot resource must be hidden from other callers");
    };
    assert_eq!(hidden["error"]["code"], -32602);
    assert_eq!(hidden["error"]["data"], json!({ "uri": uri }));

    let window_auth = snapshot_auth("snapshot-window-owner", false);
    let window_caller = mcp_artifact_export_caller_binding(Some(&window_auth)).unwrap();
    let window = mcp_runtime_tool_result_with_snapshot_resource(
        "computer_observe",
        false,
        ToolResult::ok(json!({
            "client_id": "mini",
            "surface": {
                "surface_id": "surface_iavN7wEjRWeJq83v",
                "application": "Microsoft Edge",
                "title": "Snapshot test"
            },
            "snapshot_generation": 1,
            "source_width": 1,
            "source_height": 1,
            "width": 1,
            "height": 1,
            "mime_type": "image/jpeg",
            "file_bytes": bytes.len(),
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "captured_at_unix_ms": 1,
            "content_base64": encoded,
        })),
        Some(window_caller),
    );
    assert_eq!(window["content"][0]["type"], "resource_link");
    assert_eq!(window["content"][0]["name"], "mini-window-snapshot.jpg");
    let window_uri = window["content"][0]["uri"].as_str().unwrap();
    let window_read = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2113)),
            mcp_2026_params(json!({ "uri": window_uri })),
        ),
        Some(&window_auth),
    )
    .await;
    assert!(matches!(window_read, McpOutcome::Ok(_)));
}

#[tokio::test]
async fn mcp_large_computer_snapshot_uses_bounded_inline_preview_and_preserves_full_resource() {
    use image::codecs::jpeg::JpegEncoder;
    use sha2::{Digest, Sha256};

    fn snapshot_auth(api_key_id: &str) -> crate::auth::AuthContext {
        let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
        auth.api_key_id = Some(api_key_id.to_string());
        auth.token_kind = Some("user".to_string());
        auth.scopes = vec![
            crate::auth::SCOPE_COMPUTER_READ.to_string(),
            crate::auth::SCOPE_COMPUTER_DISPLAY_READ.to_string(),
        ];
        auth
    }

    let (width, height, full) = [(1280u32, 720u32), (1024, 576), (960, 540)]
        .into_iter()
        .find_map(|(width, height)| {
            let mut rgba = image::RgbaImage::new(width, height);
            for (x, y, pixel) in rgba.enumerate_pixels_mut() {
                let v = ((x.wrapping_mul(37) ^ y.wrapping_mul(91) ^ (x + y).wrapping_mul(13))
                    & 0xff) as u8;
                *pixel = image::Rgba([v, v.rotate_left(2), v.rotate_left(5), 255]);
            }
            [82u8, 76, 70, 64, 58].into_iter().find_map(|quality| {
                let mut bytes = Vec::new();
                JpegEncoder::new_with_quality(&mut bytes, quality)
                    .encode_image(&rgba)
                    .expect("encode full JPEG fixture");
                (bytes.len() > 192 * 1024
                    && bytes.len() < crate::artifact_policy::MAX_MCP_IMAGE_BYTES)
                    .then_some((width, height, bytes))
            })
        })
        .expect("fixture must fit full-image budget while exceeding preview budget");

    let runtime = test_runtime();
    let auth = snapshot_auth("snapshot-large-owner");
    let caller = mcp_artifact_export_caller_binding(Some(&auth)).unwrap();
    let encoded = general_purpose::STANDARD.encode(&full);
    let sha256 = format!("{:x}", Sha256::digest(&full));
    let framed = mcp_runtime_tool_result_with_snapshot_resource(
        "computer_observe",
        false,
        ToolResult::ok(json!({
            "client_id": "large-preview-runner",
            "display_id": "display_large_preview",
            "snapshot_generation": 1,
            "source_width": width,
            "source_height": height,
            "width": width,
            "height": height,
            "mime_type": "image/jpeg",
            "file_bytes": full.len(),
            "sha256": sha256,
            "captured_at_unix_ms": 1,
            "content_base64": encoded,
        })),
        Some(caller),
    );

    assert_eq!(framed["isError"], false);
    let content = framed["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert_eq!(content[0]["type"], "resource_link");
    assert_eq!(content[0]["size"], Value::from(full.len() as u64));
    assert_eq!(content[2]["type"], "image");
    let preview = general_purpose::STANDARD
        .decode(content[2]["data"].as_str().unwrap())
        .unwrap();
    assert!(preview.len() <= 192 * 1024);
    assert!(preview.len() < full.len());
    assert_eq!(content[2]["mimeType"], "image/jpeg");
    assert_eq!(
        framed["structuredContent"]["output"]["content_delivery"],
        "mcp_image"
    );
    assert_eq!(
        framed["structuredContent"]["output"]["full_image_resource"],
        true
    );
    assert_eq!(
        framed["structuredContent"]["output"]["preview_file_bytes"],
        Value::from(preview.len() as u64)
    );
    let structured = serde_json::to_string(&framed["structuredContent"]).unwrap();
    assert!(!structured.contains("content_base64"));
    assert!(!structured.contains(&general_purpose::STANDARD.encode(&full)));

    let uri = content[0]["uri"].as_str().unwrap().to_string();
    let read = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(2116)),
            mcp_2026_params(json!({ "uri": uri })),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(read) = read else {
        panic!("full screenshot resource must remain readable");
    };
    let blob = read["result"]["contents"][0]["blob"].as_str().unwrap();
    assert_eq!(general_purpose::STANDARD.decode(blob).unwrap(), full);
}

#[tokio::test]
async fn mcp_resources_read_not_found_echoes_uri_without_changing_param_errors() {
    let runtime = test_runtime();
    let uri = "test://nonexistent-resource-for-conformance-testing";

    let unknown = super::super::resources::handle_read(
        &runtime,
        json!({ "uri": uri }),
        Some(json!(2114)),
        None,
        true,
    )
    .await;
    let McpOutcome::BadRequest(unknown) = unknown else {
        panic!("unknown resource must fail as Invalid params");
    };
    assert_eq!(unknown["error"]["code"], -32602);
    assert_eq!(unknown["error"]["data"], json!({ "uri": uri }));
    assert!(unknown.get("result").is_none());

    let missing =
        super::super::resources::handle_read(&runtime, json!({}), Some(json!(2115)), None, true)
            .await;
    let McpOutcome::BadRequest(missing) = missing else {
        panic!("missing uri must fail as Invalid params");
    };
    assert_eq!(missing["error"]["code"], -32602);
    assert_eq!(
        missing["error"]["message"],
        "Invalid params: uri is required"
    );
    assert!(missing["error"].get("data").is_none());
}
