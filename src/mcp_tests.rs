use super::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellClientCapabilities, ShellClientRegisterRequest,
};
use base64::engine::general_purpose;
use sha2::{Digest, Sha256};

fn test_runtime() -> ToolRuntime {
    test_runtime_with_surface(ModelSurface::LocalCoding)
}

fn test_runtime_with_surface(model_surface: ModelSurface) -> ToolRuntime {
    ToolRuntime::new_for_tests().with_model_surface(model_surface)
}

/// Run one synchronous operation with a temporary model-surface env value.
/// The previous value is restored while the shared env lock is still held,
/// including during unwinding. Async request tests receive an already-built
/// runtime so process-global env state never needs to span an await.
fn with_model_surface_env<T>(value: Option<&str>, operation: impl FnOnce() -> T) -> T {
    struct Restore {
        previous: Option<std::ffi::OsString>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for Restore {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(previous) => {
                    std::env::set_var(crate::model_surface::MCP_MODEL_SURFACE_ENV, previous)
                }
                None => std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV),
            }
        }
    }

    let guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let previous = std::env::var_os(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    match value {
        Some(value) => std::env::set_var(crate::model_surface::MCP_MODEL_SURFACE_ENV, value),
        None => std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV),
    }
    let _restore = Restore {
        previous,
        _guard: guard,
    };
    operation()
}

fn test_runtime_from_model_surface_env(value: Option<&str>) -> ToolRuntime {
    with_model_surface_env(value, || {
        let model_surface = crate::model_surface::resolve_model_surface(None)
            .expect("test model surface configuration");
        test_runtime_with_surface(model_surface)
    })
}

fn rpc(method: &str, id: Option<Value>, params: Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: Some("2.0".to_string()),
        method: method.to_string(),
        params,
        id,
    }
}

fn mcp_2026_params(mut params: Value) -> Value {
    params
        .as_object_mut()
        .expect("MCP params must be an object")
        .insert(
            "_meta".to_string(),
            json!({
                "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {}
            }),
        );
    params
}

fn mcp_2026_ui_params(mut params: Value) -> Value {
    params
        .as_object_mut()
        .expect("MCP params must be an object")
        .insert(
            "_meta".to_string(),
            json!({
                "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {
                    "extensions": {
                        MCP_UI_EXTENSION: {
                            "mimeTypes": [MCP_UI_RESOURCE_MIME_TYPE]
                        }
                    }
                }
            }),
        );
    params
}

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

// The compact switch is read per tools/list request, so `WEBCODEX_MCP_COMPACT_SCHEMAS`
// must stay stable (and serialized against other env-mutating tests) for the whole
// async body below. The full-operator surface is passed explicitly instead of via env.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_list_returns_same_names_as_runtime() {
    // Name parity with the runtime registry must hold for stateless-2026 under
    // both full and compact schema modes. Legacy MCP intentionally omits the
    // stateless-only export_project_artifact transport adapter. Schema shape is
    // covered by dedicated tests:
    // `mcp_tools_list_default_retains_output_schema` and
    // `mcp_tools_list_compact_omits_output_schema_only`.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let runtime_names: Vec<String> = registered_tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let legacy_runtime_names: Vec<String> = runtime_names
        .iter()
        .filter(|name| name.as_str() != "export_project_artifact")
        .cloned()
        .collect();

    for compact in [false, true] {
        if compact {
            std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        } else {
            std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        }
        let outcome = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(Value::from(3)), json!({})),
            None,
        )
        .await;
        let value = match outcome {
            McpOutcome::Ok(v) => v,
            other => panic!("expected Ok (compact={compact}), got {other:?}"),
        };
        let tools = value["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names, legacy_runtime_names,
            "legacy tools/list must equal runtime registry minus stateless-only tools (compact={compact})"
        );

        let stateless_outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/list",
                Some(Value::from(3003)),
                mcp_2026_params(json!({})),
            ),
            None,
        )
        .await;
        let stateless_value = match stateless_outcome {
            McpOutcome::Ok(value) => value,
            other => panic!("expected stateless Ok (compact={compact}), got {other:?}"),
        };
        let stateless_names: Vec<String> = stateless_value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            stateless_names, runtime_names,
            "stateless-2026 tools/list must match the full runtime registry (compact={compact})"
        );
        // Exercise the real env adapter, not just the pure renderer: compact
        // must change outputSchema shape while preserving the common fields.
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
            if compact {
                assert!(
                    tool.get("outputSchema").is_none(),
                    "compact env adapter must omit outputSchema for {}",
                    tool["name"]
                );
            } else {
                assert!(
                    tool["outputSchema"].is_object(),
                    "default env adapter must retain outputSchema for {}",
                    tool["name"]
                );
            }
        }
    }
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
}

#[test]
fn mcp_tools_list_adds_image_mode_without_changing_generic_artifact_schema() {
    // Explicit non-compact rendering: no env involvement, nothing to serialize.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let mcp_tool = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "read_project_artifact")
        .expect("MCP read_project_artifact");
    assert_eq!(
        mcp_tool["inputSchema"]["properties"]["as_image"]["type"],
        "boolean"
    );
    assert!(mcp_tool["description"]
        .as_str()
        .unwrap()
        .contains("as_image=true"));

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
}

#[test]
fn mcp_tools_list_exposes_host_file_params_for_conversation_import() {
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
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
        trusted_mcp_host_file_import,
        ..
    } = call
    else {
        unreachable!()
    };
    assert_eq!(openai_file_id_refs.len(), 1);
    assert_eq!(openai_file_id_refs[0].file_id, "file_host_rewritten");
    assert!(
        !trusted_mcp_host_file_import,
        "raw input cannot set provenance"
    );
}

#[test]
fn mcp_file_import_trust_requires_exact_configured_active_client_id() {
    const CALLBACK: &str = "https://chatgpt.example/connector/oauth/test";
    let mut config = (*test_config_oauth2(Some("secret"))).clone();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");

    let make_client = |name: &str, redirect_uris: &str| crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: crate::auth::hash_token("test-secret"),
        name: name.to_string(),
        owner_user_id: user.id.clone(),
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
        HostFileImportTrust::TrustedOAuthClient,
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
        HostFileImportTrust::Untrusted,
        "sharing a redirect URI must not grant authority"
    );
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&trusted_auth)),
        HostFileImportTrust::TrustedOAuthClient,
        "multiple active clients sharing the callback must not revoke explicit client-ID trust"
    );

    let same_name = make_client("ChatGPT WebCodex", "https://other.example/callback");
    db.insert_oauth_client(&same_name).unwrap();
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&auth_for(&same_name.client_id))),
        HostFileImportTrust::Untrusted,
        "sharing the display name must not grant authority"
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
        HostFileImportTrust::Untrusted,
        "empty operator trust config must fail closed"
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
        HostFileImportTrust::Untrusted,
        "recreating a client with the same callback cannot inherit the configured client-ID trust"
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
async fn mcp_image_call_returns_native_image_for_remote_agent_project() {
    // read_project_artifact is an artifact tool outside the local_coding
    // surface; select the full operator surface for this call.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let client_id = "mcp-vision-agent";
    let agent_instance_id = "inst-mcp-vision";
    let project_name = "remote-images";
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            }),
            projects: Some(vec![ShellAgentProjectSummary {
                id: project_name.to_string(),
                name: Some(project_name.to_string()),
                path: "/remote/session-atlas".to_string(),
                allow_patch: true,
                kind: Some("repo".to_string()),
                description: None,
                hooks: Vec::new(),
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 1,
                shell_profile: None,
            }]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
        })
        .await
        .unwrap();
    let project = crate::tool_runtime::agent_project_runtime_id(client_id, project_name);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;

    // Larger than the ordinary 256 KiB runner stdout cap: this proves the
    // narrowly widened MCP image result path carries a real screenshot-sized
    // response without silently tail-truncating its JSON/base64.
    let mut image_bytes = vec![0u8; 300 * 1024];
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
                        "name": "read_project_artifact",
                        "arguments": {
                            "project": project,
                            "path": path,
                            "as_image": true
                        }
                    }),
                ),
                Some(&auth),
            )
            .await
        }
    });

    let mut request = None;
    for _ in 0..200 {
        request = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if request.is_some() || call.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let request = match request {
        Some(request) => request,
        None => {
            let outcome = call.await.unwrap();
            panic!("MCP image call should enqueue a remote artifact read, got {outcome:?}");
        }
    };
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
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
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
fn computer_snapshot_frames_native_image_without_structured_base64() {
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

    let value = crate::mcp::mcp_runtime_tool_result("computer_snapshot", false, result);
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
fn project_connector_tools_list_is_exact_canonical_surface() {
    // Explicit non-compact rendering: no env involvement.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::CanonicalConnector, false);
    let tools = payload["tools"].as_array().expect("tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, crate::connector_runtime::surface::CAPABILITY_NAMES);
    assert_eq!(tools.len(), 14);
    assert!(tools.iter().all(|tool| tool["inputSchema"].is_object()));
    assert!(tools.iter().all(|tool| tool["outputSchema"].is_object()));
    assert!(!names.contains(&"runtime_status"));
    assert!(!names.contains(&"list_projects"));
    assert!(!names.contains(&"start_session"));
    assert!(names.contains(&"code_navigate"));
    assert!(names.contains(&"code_impact"));
    for raw_name in [
        "lsp_status",
        "document_symbols",
        "workspace_symbols",
        "goto_definition",
        "find_references",
        "document_diagnostics",
        "hover",
        "call_hierarchy",
        "prepare_call_hierarchy",
        "incoming_calls",
        "outgoing_calls",
    ] {
        assert!(!names.contains(&raw_name));
    }
}

#[test]
fn mcp_tools_list_default_retains_output_schema() {
    // Pure renderer with the explicit default compact=false switch; the
    // env-adapter path for the default is covered end-to-end by
    // `mcp_tools_list_returns_same_names_as_runtime`.
    let value = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let tools = value["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(
            tool["outputSchema"].is_object(),
            "default mode must keep outputSchema for {}",
            tool["name"]
        );
        assert!(tool["annotations"].is_object() || tool.get("annotations").is_some());
    }
}

#[test]
fn explicit_resume_mcp_schema_and_metadata_are_exposed() {
    // Explicit non-compact rendering: no env involvement.
    let payload = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    let tool = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "start_coding_task")
        .expect("start_coding_task MCP metadata");
    let property = &tool["inputSchema"]["properties"]["resume_session_id"];
    assert_eq!(property["type"], "string");
    assert_eq!(property["pattern"], "^wc_sess_[A-Za-z0-9_]+$");
    let description = property["description"].as_str().unwrap();
    assert!(description.contains("failure never falls back"));
    assert!(description.contains("no current binding"));
    assert!(description.contains("recording_session_id"));
    assert_eq!(
        tool["inputSchema"]["not"]["required"],
        json!(["resume_session_id", "new_session"])
    );
    assert!(tool["description"]
        .as_str()
        .unwrap()
        .contains("resume_session_id"));
}

#[test]
fn mcp_tools_list_compact_omits_output_schema_only() {
    // Pure renderer with the explicit compact=true switch; the env-adapter
    // path for compact mode is covered end-to-end by
    // `mcp_tools_list_returns_same_names_as_runtime`.
    let value = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, true);
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
        // First-version experiment keeps annotations to reduce variables.
        assert!(
            tool.get("annotations").is_some(),
            "compact mode keeps annotations for {}",
            tool["name"]
        );
    }
}

#[test]
fn mcp_tools_list_compact_is_smaller_than_full_serialized() {
    // Explicit compact switches on the pure renderer: no env involvement.
    let full = serde_json::to_vec(&mcp_tools_list_payload_with_compact(
        ModelSurface::FullOperatorRuntime,
        false,
    ))
    .expect("full serialize");
    let compact = serde_json::to_vec(&mcp_tools_list_payload_with_compact(
        ModelSurface::FullOperatorRuntime,
        true,
    ))
    .expect("compact serialize");
    assert!(
        compact.len() < full.len(),
        "compact={} full={}",
        compact.len(),
        full.len()
    );
    // Guard against accidental total collapse (must still list many tools).
    assert!(
        compact.len() > 10_000,
        "compact unexpectedly tiny: {}",
        compact.len()
    );
}

// The compact switch is the tested product behavior: `tools/call` must be
// unaffected while `WEBCODEX_MCP_COMPACT_SCHEMAS` is set, so the env must stay
// stable (and serialized against other env-mutating tests) for the whole call.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn mcp_tools_call_still_returns_structured_content_under_compact_flag() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(3)),
            json!({"name": "list_projects", "arguments": {}}),
        ),
        None,
    )
    .await;
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Ok, got {outcome:?}");
    };
    assert!(value["result"]["content"].is_array());
    assert!(value["result"]["structuredContent"].is_object());
    assert!(value["result"]["structuredContent"]["success"].is_boolean());
}

#[tokio::test]
async fn session_tools_exposed_in_registry_and_mcp() {
    // Session tools live on the full operator surface, not local_coding.
    // Assertions cover names/descriptions/inputSchema only, which compact
    // mode keeps, so no env or lock is needed.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let specs = registered_tool_specs();
    let registry_names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(registry_names.contains(&"session_summary"));
    assert!(registry_names.contains(&"update_session_context"));
    assert!(registry_names.contains(&"validation_summary"));
    assert!(registry_names.contains(&"current_session"));
    assert!(registry_names.contains(&"unbind_current_session"));
    // `start_session` and `bind_current_session` are ModelHidden: the
    // model coding line is covered by `start_coding_task` (resume_session_id
    // + bind_current), and `current_session` is the query-only view. They
    // keep no public ToolSpec, so they must be absent from both the
    // registry and the MCP tools/list surface.
    assert!(!registry_names.contains(&"start_session"));
    assert!(!registry_names.contains(&"bind_current_session"));

    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(31)), json!({})),
        None,
    )
    .await;
    let value = match outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let names: Vec<String> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.iter().any(|name| name == "session_summary"));
    assert!(names.iter().any(|name| name == "update_session_context"));
    assert!(names.iter().any(|name| name == "validation_summary"));
    assert!(names.iter().any(|name| name == "current_session"));
    assert!(names.iter().any(|name| name == "unbind_current_session"));
    assert!(!names.iter().any(|name| name == "start_session"));
    assert!(!names.iter().any(|name| name == "bind_current_session"));
    let tools = value["result"]["tools"].as_array().unwrap();
    let tool_description = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"))["description"]
            .as_str()
            .unwrap()
            .to_lowercase()
    };
    assert!(tool_description("session_summary").contains("session ledger"));
    assert!(tool_description("update_session_context").contains("authorized project"));
    assert!(tool_description("update_session_context").contains("background writer"));
    assert!(tool_description("update_session_context").contains("success does not mean"));
    assert!(tool_description("validation_summary").contains("does not run cargo"));
    assert!(tool_description("session_handoff_summary")
        .contains("does not depend on current-session binding"));
    for name in ["current_session", "unbind_current_session"] {
        let description = tool_description(name);
        assert!(
                description.contains("process-local")
                    && description.contains("hashed durable"),
                "MCP {name} description should distinguish the exact cache and hashed durable projection: {description}"
            );
    }
    let validation_summary = tools
        .iter()
        .find(|tool| tool["name"] == "validation_summary")
        .expect("missing MCP validation_summary tool");
    assert_eq!(
        validation_summary["inputSchema"]["required"],
        json!(["project", "session_id"])
    );
    assert_eq!(
        validation_summary["inputSchema"]["additionalProperties"],
        false
    );
    for name in ["read_file", "run_shell", "write_project_file"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"));
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
}

#[tokio::test]
async fn mcp_tools_call_list_projects_returns_content_blocks() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(4)),
            json!({"name": "list_projects", "arguments": {}}),
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
async fn mcp_tools_call_strips_reserved_session_id_before_dispatch() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("mcp strip".to_string()));
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(32)),
            json!({
                "name": "list_projects",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                }
            }),
        ),
        None,
    )
    .await;
    match outcome {
        McpOutcome::Ok(_) => {}
        other => panic!("expected Ok, got {:?}", other),
    }
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(10))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    let started = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started")
        .unwrap();
    assert_eq!(started.transport, "mcp");
    assert_eq!(started.tool_name, "list_projects");
    assert!(
        !serde_json::to_string(&started.input_summary)
            .unwrap()
            .contains(MCP_RESERVED_SESSION_ID_FIELD),
        "_session_id must be stripped before recording/dispatch"
    );
}

#[tokio::test]
async fn mcp_tools_call_records_event_with_session_id() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(None, None);
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(Value::from(33)),
            json!({
                "name": "list_projects",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id
                }
            }),
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
    let job_status = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "job_status")
        .expect("job_status must be model-visible on local_coding");
    let properties = job_status["inputSchema"]["properties"].as_object().unwrap();
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
            json!({
                "name": "job_status",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &session.session_id,
                    "job_id": "missing-job",
                    "expected_failure": true,
                    "expected_failure_kind": "job_not_found",
                    "assertion_name": "mcp hidden metadata compatibility"
                }
            }),
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
    assert_eq!(finished.tool_name, "job_status");
    assert_eq!(finished.expected_failure, Some(true));
    assert_eq!(
        finished.expected_failure_kind.as_deref(),
        Some("job_not_found")
    );
    assert_eq!(
        finished.assertion_name.as_deref(),
        Some("mcp hidden metadata compatibility")
    );
    assert_eq!(
        finished.actual_failure_kind.as_deref(),
        Some("job_not_found")
    );
    assert_eq!(
        finished.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );
}

#[tokio::test]
async fn mcp_show_changes_distinguishes_reserved_session_id_from_query_session_id() {
    use crate::shell_protocol::{
        ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
        ShellClientRegisterRequest,
    };

    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "mcp-client".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![ShellAgentProjectSummary {
                id: "demo".to_string(),
                name: Some("Demo".to_string()),
                path: "/tmp/demo".to_string(),
                allow_patch: true,
                kind: None,
                description: None,
                hooks: vec![],
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 0,
                shell_profile: None,
            }]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
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
            json!({
                "name": "show_changes",
                "arguments": {
                    MCP_RESERVED_SESSION_ID_FIELD: &tracking_session.session_id,
                    "project": project,
                    "session_id": &query_session.session_id,
                    "include_diff": false
                }
            }),
        ),
        Some(&auth),
    );
    let complete = async {
        let mut req = None;
        for _ in 0..50 {
            req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "mcp-client".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let req = req.expect("show_changes should enqueue an agent shell request");
        let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "mcp-client".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(stdout.to_string()),
                stderr: Some(String::new()),
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

#[tokio::test]
async fn mcp_tools_list_parity_with_rest_tools_list() {
    // MCP tools/list and REST /api/tools/list both expose the exact same
    // registry-backed tool names on the full operator surface.
    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let mcp_outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/list",
            Some(Value::from(8)),
            mcp_2026_params(json!({})),
        ),
        None,
    )
    .await;
    let mcp_value = match mcp_outcome {
        McpOutcome::Ok(v) => v,
        other => panic!("expected Ok, got {:?}", other),
    };
    let mcp_names: Vec<String> = mcp_value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let rest_names: Vec<String> = registered_tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(mcp_names, rest_names);
}

// =========================================================================
// HTTP integration tests — exercise the real Salvo router + AuthMiddleware.
// These do not start a real server; they build a Router, wrap it in a
// Service, and dispatch TestClient requests through it.
// =========================================================================

use crate::test_support::{seed_oauth_client, seed_user, test_config, test_config_oauth2, test_db};
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;

fn seed_oauth_access_token(
    db: &crate::Database,
    client: &crate::models::OAuthClientRecord,
    user: &crate::models::UserRecord,
    scopes: &str,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let plaintext = crate::auth::generate_oauth_access_token();
    let record = crate::models::OAuthAccessTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: crate::auth::hash_token(&plaintext),
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".to_string(),
        subject_id: user.id.clone(),
        user_id: Some(user.id.clone()),
        scopes: scopes.to_string(),
        resource: None,
        shared_key_hash: None,
        created_at: now,
        expires_at: now + 3600,
        revoked_at: None,
        last_used_at: None,
    };
    db.insert_oauth_access_token(&record).unwrap();
    plaintext
}

#[tokio::test]
async fn mcp_tools_call_writes_a_summary_action_audit_row() {
    // list_tools is a full-operator-only tool; select that surface so the
    // call dispatches and lands an action audit row.
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // End the connection guard structurally inside the block: the awaited
    // requests below must not overlap it.
    let (endpoint, action, operation, status, summary) = {
        let conn = db.conn_for_tests();
        let (endpoint, action, operation, status): (String, String, String, String) = conn
            .query_row(
                "SELECT endpoint, action_name, operation, status FROM action_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        // Summary-level discipline: no tool output is persisted for MCP rows.
        let summary: String = conn
            .query_row("SELECT summary_json FROM action_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        (endpoint, action, operation, status, summary)
    };
    assert_eq!(endpoint, "/mcp");
    assert_eq!(action, "toolsCall");
    assert_eq!(operation, "list_tools");
    assert_eq!(status, "success");
    assert!(
        !summary.contains("tools"),
        "summary must not embed output: {summary}"
    );

    // Non-tool methods stay out of the audit.
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // A no-id tools/call is a JSON-RPC notification. The core dispatcher
    // intentionally ignores every notification, so it must not create a
    // successful action row for work that never ran.
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {"name": "list_tools", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::ACCEPTED));

    let count: i64 = {
        let conn = db.conn_for_tests();
        conn.query_row("SELECT COUNT(*) FROM action_events", [], |row| row.get(0))
            .unwrap()
    };
    assert_eq!(count, 1);
}

fn oauth_mcp_service_with_surface(
    scopes: &str,
    model_surface: ModelSurface,
) -> (tempfile::TempDir, Service, String) {
    let config = test_config_oauth2(Some("secret"));
    let (tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, scopes);
    let runtime = Arc::new(test_runtime_with_surface(model_surface));
    let service = Service::new(build_test_router(config, db, runtime));
    (tmp, service, token)
}

fn oauth_mcp_service(scopes: &str) -> (tempfile::TempDir, Service, String) {
    oauth_mcp_service_with_surface(scopes, ModelSurface::LocalCoding)
}

const MCP_IMPORT_TRUSTED_REDIRECT: &str = "https://chatgpt.example/connector/oauth/webcodex-test";

struct McpImportStartupEnvGuard {
    _env_lock: std::sync::MutexGuard<'static, ()>,
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl McpImportStartupEnvGuard {
    fn new() -> Self {
        const NAMES: &[&str] = &[
            "WEBCODEX_ENV_FILE",
            "WEBCODEX_TOKEN",
            "WEBCODEX_OAUTH2_ENABLED",
            "WEBCODEX_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS",
        ];
        let env_lock = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        let previous = NAMES
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect();
        Self {
            _env_lock: env_lock,
            previous,
        }
    }

    fn set(&self, name: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        std::env::set_var(name, value);
    }
}

impl Drop for McpImportStartupEnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn mcp_import_config_from_startup_env(
    trusted_client_id: &str,
    env_file_client_id: &str,
) -> Arc<crate::Config> {
    let guard = McpImportStartupEnvGuard::new();
    let dir = tempfile::tempdir().unwrap();
    let env_file = dir.path().join("webcodex.env");
    std::fs::write(
        &env_file,
        format!(
            "WEBCODEX_OAUTH2_ENABLED=false\nWEBCODEX_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS={env_file_client_id}\n"
        ),
    )
    .unwrap();

    // Production startup loads env files first, but an already-present process
    // environment is authoritative and load_env_file deliberately does not
    // replace it. This matches the live deployment shape being debugged.
    guard.set("WEBCODEX_ENV_FILE", &env_file);
    guard.set("WEBCODEX_TOKEN", "startup-env-bootstrap-token");
    guard.set("WEBCODEX_OAUTH2_ENABLED", "true");
    guard.set(
        "WEBCODEX_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS",
        trusted_client_id,
    );
    let loads = crate::config::load_startup_env_files().unwrap();
    assert_eq!(loads.len(), 1);
    assert_eq!(loads[0].path, env_file);
    assert_eq!(
        loads[0].loaded_count, 0,
        "explicit env file must not override already-present OAuth trust settings"
    );

    let config = Arc::new(crate::Config::from_env());
    assert!(config.oauth2.enabled);
    assert_eq!(
        config.oauth2.trusted_mcp_file_client_ids,
        vec![trusted_client_id.to_string()]
    );
    config
}

async fn lock_mcp_import_test() -> tokio::sync::MutexGuard<'static, ()> {
    crate::tool_runtime::conversation_import::lock_import_test_network().await
}

struct McpImportNetworkOverride;

impl McpImportNetworkOverride {
    fn set(base_url: String) -> Self {
        crate::tool_runtime::conversation_import::set_import_test_download_base_url(Some(base_url));
        crate::tool_runtime::conversation_import::set_import_test_resolved_ips(Some(vec![
            "8.8.8.8".parse().unwrap(),
        ]));
        crate::tool_runtime::conversation_import::reset_import_test_dns_resolution_count();
        Self
    }

    fn without_download() -> Self {
        crate::tool_runtime::conversation_import::set_import_test_download_base_url(None);
        crate::tool_runtime::conversation_import::set_import_test_resolved_ips(Some(vec![
            "8.8.8.8".parse().unwrap(),
        ]));
        crate::tool_runtime::conversation_import::reset_import_test_dns_resolution_count();
        Self
    }
}

impl Drop for McpImportNetworkOverride {
    fn drop(&mut self) {
        crate::tool_runtime::conversation_import::set_import_test_download_base_url(None);
        crate::tool_runtime::conversation_import::set_import_test_resolved_ips(None);
        crate::tool_runtime::conversation_import::reset_import_test_dns_resolution_count();
    }
}

struct McpImportMockServer {
    base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for McpImportMockServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn mcp_import_http_response(status: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status}\r\n").into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

async fn start_mcp_import_mock_server(response: Vec<u8>) -> McpImportMockServer {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = [0_u8; 4096];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(&response).await;
        let _ = stream.shutdown().await;
    });
    McpImportMockServer {
        base_url: format!("http://{addr}"),
        handle,
    }
}

fn mcp_import_config(trusted_client_ids: &[&str]) -> Arc<crate::Config> {
    let mut config = (*test_config_oauth2(Some("secret"))).clone();
    config.oauth2.trusted_mcp_file_client_ids = trusted_client_ids
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    Arc::new(config)
}

fn seed_mcp_import_client(
    db: &crate::Database,
    user: &crate::models::UserRecord,
    name: &str,
    redirect_uris: &str,
) -> crate::models::OAuthClientRecord {
    let record = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: crate::auth::hash_token("test-client-secret"),
        name: name.to_string(),
        owner_user_id: user.id.clone(),
        redirect_uris: redirect_uris.to_string(),
        allowed_scopes: "project:write".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    };
    db.insert_oauth_client(&record).unwrap();
    record
}

fn seed_mcp_import_pat(db: &crate::Database, user: &crate::models::UserRecord) -> String {
    let plaintext = crate::auth::generate_api_token();
    let record = crate::models::ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: "mcp-import-client-manager".to_string(),
        key_prefix: crate::auth::token_prefix(&plaintext),
        created_at: chrono::Utc::now().timestamp(),
        last_used_at: None,
        revoked_at: None,
        scopes: "runtime:read project:read project:write job:run account:manage".to_string(),
        expires_at: None,
        kind: crate::models::TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &crate::auth::hash_token(&plaintext))
        .unwrap();
    plaintext
}

fn mcp_import_oauth_auth(client_id: &str) -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
    auth.username = Some("alice".to_string());
    auth.token_kind = Some("oauth2".to_string());
    auth.scopes = vec![crate::auth::SCOPE_PROJECT_WRITE.to_string()];
    auth.allowed_client_id = Some(client_id.to_string());
    auth
}

async fn mcp_import_runtime(
    root: &std::path::Path,
    owner: Option<&str>,
) -> (
    Arc<ToolRuntime>,
    Arc<crate::shell_client::ShellClientRegistry>,
) {
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
    };
    let registry = Arc::new(crate::shell_client::ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "importer".to_string(),
            agent_instance_id: "inst-import".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                file_write: true,
                ..Default::default()
            }),
            projects: Some(vec![ShellAgentProjectSummary {
                id: "demo".to_string(),
                name: Some("Demo".to_string()),
                path: root.to_string_lossy().to_string(),
                allow_patch: true,
                kind: None,
                description: None,
                hooks: vec![],
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 0,
                shell_profile: None,
            }]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let runtime = Arc::new(
        ToolRuntime::new_for_tests_with_shell_clients(registry.clone())
            .with_model_surface(ModelSurface::FullOperatorRuntime),
    );
    (runtime, registry)
}

async fn complete_mcp_import_save(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
    expected_bytes: Vec<u8>,
) {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    async fn next_request(
        registry: &crate::shell_client::ShellClientRegistry,
    ) -> crate::shell_protocol::ShellAgentShellRequest {
        loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                return request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    let request = next_request(&registry).await;
    assert_eq!(request.kind, "file_artifact_upload_begin");
    let begin: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert!(begin.get("download_url").is_none());
    assert!(begin.get("download_link").is_none());
    assert!(begin.get("openaiFileIdRefs").is_none());
    assert_eq!(
        begin["max_bytes"],
        crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_BYTES
    );
    assert_eq!(begin["expected_bytes"], expected_bytes.len());
    let path = begin["path"].as_str().unwrap().to_string();
    let mime_type = begin["mime_type"].as_str().unwrap().to_string();
    let upload_id = "wc_upload_mcp_import_fixture";
    registry
        .complete(ShellAgentResultRequest {
            client_id: "importer".to_string(),
            agent_instance_id: "inst-import".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                json!({
                    "path": path,
                    "upload_id": upload_id,
                    "received_bytes": 0,
                    "next_offset": 0,
                    "expected_bytes": expected_bytes.len(),
                    "expected_sha256": null,
                    "max_bytes": crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_BYTES,
                    "mime_type": mime_type,
                    "committed": false
                })
                .to_string(),
            ),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let mut bytes = Vec::new();
    loop {
        let request = next_request(&registry).await;
        let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        assert_eq!(payload["path"], path);
        assert_eq!(payload["upload_id"], upload_id);
        match request.kind.as_str() {
            "file_artifact_upload_chunk" => {
                assert_eq!(
                    payload["max_chunk_bytes"],
                    crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES
                );
                assert_eq!(payload["offset"], bytes.len());
                let chunk = base64::engine::general_purpose::STANDARD
                    .decode(payload["content_base64"].as_str().unwrap())
                    .unwrap();
                assert!(!chunk.is_empty());
                assert!(
                    chunk.len()
                        <= crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES
                );
                bytes.extend_from_slice(&chunk);
                registry
                    .complete(ShellAgentResultRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(
                            json!({
                                "path": path,
                                "upload_id": upload_id,
                                "received_bytes": bytes.len(),
                                "next_offset": bytes.len(),
                                "expected_bytes": expected_bytes.len(),
                                "expected_sha256": null,
                                "max_bytes": crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_BYTES,
                                "mime_type": mime_type,
                                "committed": false
                            })
                            .to_string(),
                        ),
                        stderr: None,
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
            }
            "file_artifact_upload_finish" => {
                assert_eq!(bytes, expected_bytes);
                let sha256 = format!("{:x}", Sha256::digest(&bytes));
                registry
                    .complete(ShellAgentResultRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(
                            json!({
                                "path": path,
                                "upload_id": upload_id,
                                "bytes": bytes.len(),
                                "received_bytes": bytes.len(),
                                "expected_bytes": expected_bytes.len(),
                                "expected_sha256": null,
                                "sha256": sha256,
                                "mime_type": mime_type,
                                "committed": true
                            })
                            .to_string(),
                        ),
                        stderr: None,
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
                break;
            }
            other => panic!("unexpected MCP import artifact request: {other}"),
        }
    }
}

async fn complete_mcp_import_until_abort(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
) -> usize {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    use base64::Engine as _;

    async fn next_request(
        registry: &crate::shell_client::ShellClientRegistry,
    ) -> crate::shell_protocol::ShellAgentShellRequest {
        loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                return request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    let request = next_request(&registry).await;
    assert_eq!(request.kind, "file_artifact_upload_begin");
    let begin: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert!(begin["expected_bytes"].is_null());
    let path = begin["path"].as_str().unwrap().to_string();
    let mime_type = begin["mime_type"].as_str().unwrap().to_string();
    let upload_id = "wc_upload_mcp_abort_fixture";
    registry
        .complete(ShellAgentResultRequest {
            client_id: "importer".to_string(),
            agent_instance_id: "inst-import".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                json!({
                    "path": path,
                    "upload_id": upload_id,
                    "received_bytes": 0,
                    "next_offset": 0,
                    "expected_bytes": null,
                    "expected_sha256": null,
                    "max_bytes": crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_BYTES,
                    "mime_type": mime_type,
                    "committed": false
                })
                .to_string(),
            ),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let mut received_bytes = 0usize;
    let mut chunk_count = 0usize;
    loop {
        let request = next_request(&registry).await;
        let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        assert_eq!(payload["path"], path);
        assert_eq!(payload["upload_id"], upload_id);
        match request.kind.as_str() {
            "file_artifact_upload_chunk" => {
                assert_eq!(payload["offset"], received_bytes);
                let chunk = base64::engine::general_purpose::STANDARD
                    .decode(payload["content_base64"].as_str().unwrap())
                    .unwrap();
                assert!(!chunk.is_empty());
                assert!(
                    chunk.len()
                        <= crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES
                );
                received_bytes += chunk.len();
                chunk_count += 1;
                registry
                    .complete(ShellAgentResultRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(
                            json!({
                                "path": path,
                                "upload_id": upload_id,
                                "received_bytes": received_bytes,
                                "next_offset": received_bytes,
                                "expected_bytes": null,
                                "expected_sha256": null,
                                "max_bytes": crate::tool_runtime::files::MAX_PROJECT_ARTIFACT_UPLOAD_BYTES,
                                "mime_type": mime_type,
                                "committed": false
                            })
                            .to_string(),
                        ),
                        stderr: None,
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
            }
            "file_artifact_upload_abort" => {
                registry
                    .complete(ShellAgentResultRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some(
                            json!({
                                "path": path,
                                "upload_id": upload_id,
                                "received_bytes": received_bytes,
                                "temp_file_removed": true,
                                "sidecar_removed": true,
                                "final_file_exists": false,
                                "committed": false
                            })
                            .to_string(),
                        ),
                        stderr: None,
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
                return chunk_count;
            }
            other => panic!("unexpected over-limit MCP import request: {other}"),
        }
    }
}

async fn mcp_export_runtime_with_optimized_chunk(
    root: &std::path::Path,
    owner: Option<&str>,
    optimized_chunk: bool,
) -> (
    Arc<ToolRuntime>,
    Arc<crate::shell_client::ShellClientRegistry>,
) {
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
    };
    let registry = Arc::new(crate::shell_client::ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "exporter".to_string(),
            agent_instance_id: "inst-export".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                file_read: true,
                artifact_export_chunk_read: optimized_chunk,
                ..Default::default()
            }),
            projects: Some(vec![ShellAgentProjectSummary {
                id: "demo".to_string(),
                name: Some("Demo".to_string()),
                path: root.to_string_lossy().to_string(),
                allow_patch: true,
                kind: None,
                description: None,
                hooks: vec![],
                disabled: false,
                revision: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 0,
                shell_profile: None,
            }]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let runtime = Arc::new(
        ToolRuntime::new_for_tests_with_shell_clients(registry.clone())
            .with_model_surface(ModelSurface::FullOperatorRuntime),
    );
    (runtime, registry)
}

async fn mcp_export_runtime(
    root: &std::path::Path,
    owner: Option<&str>,
) -> (
    Arc<ToolRuntime>,
    Arc<crate::shell_client::ShellClientRegistry>,
) {
    mcp_export_runtime_with_optimized_chunk(root, owner, true).await
}

fn mcp_export_api_auth(api_key_id: &str, username: &str) -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    auth.username = Some(username.to_string());
    auth.api_key_id = Some(api_key_id.to_string());
    auth.token_kind = Some("user".to_string());
    auth.scopes = vec![
        crate::auth::SCOPE_RUNTIME_READ.to_string(),
        crate::auth::SCOPE_PROJECT_READ.to_string(),
    ];
    auth
}

async fn poll_mcp_export_request(
    registry: &Arc<crate::shell_client::ShellClientRegistry>,
) -> crate::shell_protocol::ShellAgentShellRequest {
    use crate::shell_protocol::ShellAgentPollRequest;
    loop {
        if let Some(request) = registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            return request;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

async fn complete_mcp_export_request(
    registry: &Arc<crate::shell_client::ShellClientRegistry>,
    request: crate::shell_protocol::ShellAgentShellRequest,
    stdout: Value,
) {
    use crate::shell_protocol::ShellAgentResultRequest;
    registry
        .complete(ShellAgentResultRequest {
            client_id: "exporter".to_string(),
            agent_instance_id: "inst-export".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout.to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

fn mcp_export_optimized_chunk_range(
    request: &crate::shell_protocol::ShellAgentShellRequest,
    path: &str,
    file_bytes: usize,
) -> (usize, usize) {
    assert_eq!(request.kind, "file_read_project_artifact_export_chunk");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["expected_file_bytes"], file_bytes);
    let offset = payload["offset"].as_u64().unwrap() as usize;
    let length = payload["length"].as_u64().unwrap() as usize;
    assert!(length <= MAX_READ_PROJECT_ARTIFACT_LENGTH);
    let end = offset.saturating_add(length).min(file_bytes);
    (offset, end)
}

async fn complete_mcp_export_optimized_chunk(
    registry: &Arc<crate::shell_client::ShellClientRegistry>,
    request: crate::shell_protocol::ShellAgentShellRequest,
    path: &str,
    bytes: &[u8],
) -> usize {
    let (offset, end) = mcp_export_optimized_chunk_range(&request, path, bytes.len());
    complete_mcp_export_request(
        registry,
        request,
        json!({
            "path": path,
            "file_bytes": bytes.len(),
            "offset": offset,
            "bytes_returned": end - offset,
            "content_base64": general_purpose::STANDARD.encode(&bytes[offset..end]),
            "next_offset": end,
            "truncated": end < bytes.len(),
            "eof": end == bytes.len(),
        }),
    )
    .await;
    offset
}

async fn complete_mcp_export_metadata(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
    path: &str,
    bytes: usize,
    sha256: &str,
    mime_type: &str,
) {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    let request = loop {
        if let Some(request) = registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            break request;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["max_bytes"], MAX_PROJECT_ARTIFACT_BYTES);
    assert_eq!(payload["allow_missing"], false);
    registry
        .complete(ShellAgentResultRequest {
            client_id: "exporter".to_string(),
            agent_instance_id: "inst-export".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                json!({
                    "path": path,
                    "bytes": bytes,
                    "sha256": sha256,
                    "mime_type": mime_type,
                })
                .to_string(),
            ),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpExportChunkFault {
    None,
    InvalidBase64,
    Offset,
    Eof,
    MutateFirstChunk,
    MutateLaterChunk,
}

async fn complete_mcp_export_resource_read(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
    path: &str,
    bytes: Vec<u8>,
    mime_type: &str,
    sha256: &str,
    fault: McpExportChunkFault,
) {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    complete_mcp_export_metadata(registry.clone(), path, bytes.len(), sha256, mime_type).await;
    let mut expected_offset = 0usize;
    while expected_offset < bytes.len() {
        let request = loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "exporter".to_string(),
                    agent_instance_id: "inst-export".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                break request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        };
        assert_eq!(
            request.kind, "file_read_project_artifact_export_chunk",
            "optimized-capable Runner must receive the internal export chunk request"
        );
        let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        assert_eq!(payload["path"], path);
        assert_eq!(payload["expected_file_bytes"], bytes.len());
        assert!(payload.get("max_file_bytes").is_none());
        let offset = payload["offset"].as_u64().unwrap() as usize;
        let length = payload["length"].as_u64().unwrap() as usize;
        assert_eq!(offset, expected_offset);
        assert!(length <= MAX_READ_PROJECT_ARTIFACT_LENGTH);
        let end = offset.saturating_add(length).min(bytes.len());
        let mut chunk = bytes[offset..end].to_vec();
        if (fault == McpExportChunkFault::MutateFirstChunk && offset == 0)
            || (fault == McpExportChunkFault::MutateLaterChunk && offset > 0)
        {
            if let Some(first) = chunk.first_mut() {
                *first ^= 0xff;
            }
        }
        let eof = end == bytes.len();
        let reported_offset = if fault == McpExportChunkFault::Offset && offset == 0 {
            1
        } else {
            offset
        };
        let reported_eof = if fault == McpExportChunkFault::Eof && offset == 0 {
            !eof
        } else {
            eof
        };
        let content_base64 = if fault == McpExportChunkFault::InvalidBase64 && offset == 0 {
            "***not-base64***".to_string()
        } else {
            general_purpose::STANDARD.encode(&chunk)
        };
        let stdout = json!({
            "path": path,
            "file_bytes": bytes.len(),
            "offset": reported_offset,
            "bytes_returned": chunk.len(),
            "content_base64": content_base64,
            "next_offset": end,
            "truncated": !reported_eof,
            "eof": reported_eof,
        })
        .to_string();
        registry
            .complete(ShellAgentResultRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(stdout),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        if matches!(
            fault,
            McpExportChunkFault::InvalidBase64
                | McpExportChunkFault::Offset
                | McpExportChunkFault::Eof
        ) && offset == 0
        {
            return;
        }
        expected_offset = end;
    }
}

async fn complete_mcp_export_resource_read_legacy(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
    path: &str,
    bytes: Vec<u8>,
    mime_type: &str,
    sha256: &str,
) {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    complete_mcp_export_metadata(registry.clone(), path, bytes.len(), sha256, mime_type).await;
    let mut expected_offset = 0usize;
    while expected_offset < bytes.len() {
        let request = loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "exporter".to_string(),
                    agent_instance_id: "inst-export".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                break request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        };
        assert_eq!(request.kind, "file_read_project_artifact");
        assert!(
            registry
                .poll(ShellAgentPollRequest {
                    client_id: "exporter".to_string(),
                    agent_instance_id: "inst-export".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
                .is_none(),
            "legacy fallback must keep at most one public artifact read in flight"
        );
        let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        assert_eq!(payload["path"], path);
        assert_eq!(payload["max_file_bytes"], MAX_PROJECT_ARTIFACT_BYTES);
        let offset = payload["offset"].as_u64().unwrap() as usize;
        let length = payload["length"].as_u64().unwrap() as usize;
        assert_eq!(offset, expected_offset);
        let end = offset.saturating_add(length).min(bytes.len());
        let chunk = &bytes[offset..end];
        let eof = end == bytes.len();
        registry
            .complete(ShellAgentResultRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(
                    json!({
                        "path": path,
                        "mime_type": mime_type,
                        "file_bytes": bytes.len(),
                        "sha256": sha256,
                        "offset": offset,
                        "bytes_returned": chunk.len(),
                        "content_base64": general_purpose::STANDARD.encode(chunk),
                        "next_offset": end,
                        "truncated": !eof,
                        "eof": eof,
                    })
                    .to_string(),
                ),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        expected_offset = end;
    }
}

async fn issue_mcp_artifact_export(
    runtime: Arc<ToolRuntime>,
    registry: Arc<crate::shell_client::ShellClientRegistry>,
    auth: crate::auth::AuthContext,
    path: &str,
    bytes: &[u8],
    mime_type: &str,
) -> Value {
    use sha2::{Digest, Sha256};
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let call = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let path = path.to_string();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "tools/call",
                    Some(json!(3101)),
                    mcp_2026_params(json!({
                        "name": "export_project_artifact",
                        "arguments": {
                            "project": "agent:exporter:demo",
                            "path": path,
                        }
                    })),
                ),
                Some(&auth),
            )
            .await
        }
    });
    complete_mcp_export_metadata(registry, path, bytes.len(), &sha256, mime_type).await;
    let outcome = call.await.unwrap();
    let McpOutcome::Ok(value) = outcome else {
        panic!("artifact export must succeed, got {outcome:?}");
    };
    value
}

#[tokio::test]
async fn mcp_artifact_export_surface_is_stateless_full_operator_only() {
    let legacy = mcp_tools_list_payload_with_compact(ModelSurface::FullOperatorRuntime, false);
    assert!(!legacy["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "export_project_artifact"));

    let stateless =
        mcp_tools_list_payload_with_compact_and_app(ModelSurface::FullOperatorRuntime, false, true);
    let spec = stateless["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "export_project_artifact")
        .expect("stateless full-operator tools/list must expose artifact export");
    assert_eq!(spec["inputSchema"]["required"], json!(["project", "path"]));
    assert!(spec["inputSchema"]["properties"]
        .get("session_id")
        .is_some());
    assert!(spec["inputSchema"]["properties"]
        .get(crate::tool_runtime::ALLOW_CROSS_PROJECT_SESSION_FIELD)
        .is_none());

    let runtime = test_runtime_with_surface(ModelSurface::FullOperatorRuntime);
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    let legacy_call = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(3100)),
            json!({
                "name": "export_project_artifact",
                "arguments": {"project": "agent:any:any", "path": "report.pdf"}
            }),
        ),
        Some(&auth),
    )
    .await;
    match legacy_call {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert!(value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("stateless-2026"));
        }
        other => panic!("legacy artifact export must fail closed, got {other:?}"),
    }
}

#[test]
fn mcp_artifact_export_oauth_binding_survives_access_token_refresh() {
    let oauth = |access_token_id: &str, client_id: &str| {
        let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.user_id = Some("user-alice".to_string());
        auth.username = Some("alice".to_string());
        auth.api_key_id = Some(access_token_id.to_string());
        auth.token_kind = Some("oauth2".to_string());
        auth.allowed_client_id = Some(client_id.to_string());
        auth.scopes = vec![crate::auth::SCOPE_PROJECT_READ.to_string()];
        auth
    };
    let first =
        mcp_artifact_export_caller_binding(Some(&oauth("wc_oat_record_1", "client-a"))).unwrap();
    let refreshed =
        mcp_artifact_export_caller_binding(Some(&oauth("wc_oat_record_2", "client-a"))).unwrap();
    let other_client =
        mcp_artifact_export_caller_binding(Some(&oauth("wc_oat_record_3", "client-b"))).unwrap();
    assert_eq!(
        first, refreshed,
        "access-token refresh must retain export identity"
    );
    assert_ne!(
        first, other_client,
        "OAuth client identity remains part of the binding"
    );
}

#[tokio::test]
async fn mcp_artifact_export_oauth_resource_read_uses_project_read_and_stable_identity() {
    use crate::shell_protocol::ShellAgentPollRequest;
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let oauth = |access_token_id: &str, scopes: Vec<String>| {
        let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.user_id = Some("user-alice".to_string());
        auth.username = Some("alice".to_string());
        auth.api_key_id = Some(access_token_id.to_string());
        auth.token_kind = Some("oauth2".to_string());
        auth.allowed_client_id = Some("client-chatgpt".to_string());
        auth.scopes = scopes;
        auth
    };
    let creator = oauth(
        "wc_oat_record_1",
        vec![crate::auth::SCOPE_PROJECT_READ.to_string()],
    );
    let bytes = b"%PDF-1.7\noauth export\n%%EOF\n".to_vec();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        creator,
        "paper/oauth.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();

    let refreshed = oauth(
        "wc_oat_record_2",
        vec![crate::auth::SCOPE_PROJECT_READ.to_string()],
    );
    let read = tokio::spawn({
        let runtime = runtime.clone();
        let uri = uri.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "resources/read",
                    Some(json!(3110)),
                    mcp_2026_params(json!({"uri": uri})),
                ),
                Some(&refreshed),
            )
            .await
        }
    });
    complete_mcp_export_resource_read(
        registry.clone(),
        "paper/oauth.pdf",
        bytes.clone(),
        "application/pdf",
        &sha256,
        McpExportChunkFault::None,
    )
    .await;
    let outcome = read.await.unwrap();
    let McpOutcome::Ok(value) = outcome else {
        panic!("refreshed OAuth caller should retain export identity, got {outcome:?}");
    };
    let decoded = general_purpose::STANDARD
        .decode(value["result"]["contents"][0]["blob"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, bytes);

    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        oauth(
            "wc_oat_record_3",
            vec![crate::auth::SCOPE_PROJECT_READ.to_string()],
        ),
        "paper/oauth.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let missing_scope = oauth("wc_oat_record_4", vec![]);
    let denied = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(3111)),
            mcp_2026_params(json!({"uri": uri})),
        ),
        Some(&missing_scope),
    )
    .await;
    match denied {
        McpOutcome::Forbidden {
            body,
            required_scope,
        } => {
            assert_eq!(required_scope, Some(crate::auth::SCOPE_PROJECT_READ));
            assert_eq!(body["error"], "insufficient_scope");
            assert!(body["error_description"]
                .as_str()
                .unwrap_or("")
                .contains(crate::auth::SCOPE_PROJECT_READ));
        }
        other => panic!("OAuth export read without project:read must fail, got {other:?}"),
    }
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "exporter".to_string(),
            agent_instance_id: "inst-export".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn export_project_artifact_non_mcp_path_fails_before_runner_read() {
    use crate::shell_protocol::ShellAgentPollRequest;
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-export", "alice");
    let result = runtime
        .dispatch_with_auth(
            crate::tool_runtime::ToolCall::ExportProjectArtifact {
                project: "agent:exporter:demo".to_string(),
                path: "paper/report.pdf".to_string(),
                session_id: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("MCP-only")));
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "exporter".to_string(),
            agent_instance_id: "inst-export".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn mcp_artifact_export_resource_link_and_binary_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-roundtrip", "alice");
    // Keep the PDF above one public read_project_artifact chunk so the export
    // resource path must exercise the bounded multi-read loop.
    let mut pdf = Vec::with_capacity(96 * 1024);
    pdf.extend_from_slice(b"%PDF-1.7\nWebCodex export fixture\n");
    while pdf.len() < 96 * 1024 - 6 {
        pdf.extend_from_slice(b"artifact export bounded chunk fixture\n");
    }
    pdf.truncate(96 * 1024 - 6);
    pdf.extend_from_slice(b"%%EOF\n");

    // Minimal real OOXML ZIP: [Content_Types].xml, package relationship, and
    // ppt/presentation.xml. The export path does not semantically parse Office
    // content, but this keeps the PPTX round-trip fixture structurally genuine.
    let pptx = general_purpose::STANDARD
        .decode("UEsDBBQAAAAAAPtWD10vICcR9gAAAPYAAAATAAAAW0NvbnRlbnRfVHlwZXNdLnhtbDw/eG1sIHZlcnNpb249IjEuMCI/PjxUeXBlcyB4bWxucz0iaHR0cDovL3NjaGVtYXMub3BlbnhtbGZvcm1hdHMub3JnL3BhY2thZ2UvMjAwNi9jb250ZW50LXR5cGVzIj48T3ZlcnJpZGUgUGFydE5hbWU9Ii9wcHQvcHJlc2VudGF0aW9uLnhtbCIgQ29udGVudFR5cGU9ImFwcGxpY2F0aW9uL3ZuZC5vcGVueG1sZm9ybWF0cy1vZmZpY2Vkb2N1bWVudC5wcmVzZW50YXRpb25tbC5wcmVzZW50YXRpb24ubWFpbit4bWwiLz48L1R5cGVzPlBLAwQUAAAAAAD7Vg9dO8y/FQoBAAAKAQAACwAAAF9yZWxzLy5yZWxzPD94bWwgdmVyc2lvbj0iMS4wIj8+PFJlbGF0aW9uc2hpcHMgeG1sbnM9Imh0dHA6Ly9zY2hlbWFzLm9wZW54bWxmb3JtYXRzLm9yZy9wYWNrYWdlLzIwMDYvcmVsYXRpb25zaGlwcyI+PFJlbGF0aW9uc2hpcCBJZD0icklkMSIgVHlwZT0iaHR0cDovL3NjaGVtYXMub3BlbnhtbGZvcm1hdHMub3JnL29mZmljZURvY3VtZW50LzIwMDYvcmVsYXRpb25zaGlwcy9vZmZpY2VEb2N1bWVudCIgVGFyZ2V0PSJwcHQvcHJlc2VudGF0aW9uLnhtbCIvPjwvUmVsYXRpb25zaGlwcz5QSwMEFAAAAAAA+1YPXZD24kRrAAAAawAAABQAAABwcHQvcHJlc2VudGF0aW9uLnhtbDw/eG1sIHZlcnNpb249IjEuMCI/PjxwOnByZXNlbnRhdGlvbiB4bWxuczpwPSJodHRwOi8vc2NoZW1hcy5vcGVueG1sZm9ybWF0cy5vcmcvcHJlc2VudGF0aW9ubWwvMjAwNi9tYWluIi8+UEsBAhQDFAAAAAAA+1YPXS8gJxH2AAAA9gAAABMAAAAAAAAAAAAAAIABAAAAAFtDb250ZW50X1R5cGVzXS54bWxQSwECFAMUAAAAAAD7Vg9dO8y/FQoBAAAKAQAACwAAAAAAAAAAAAAAgAEnAQAAX3JlbHMvLnJlbHNQSwECFAMUAAAAAAD7Vg9dkPbiRGsAAABrAAAAFAAAAAAAAAAAAAAAgAFaAgAAcHB0L3ByZXNlbnRhdGlvbi54bWxQSwUGAAAAAAMAAwC8AAAA9wIAAAAA")
        .unwrap();
    let cases = vec![
        ("paper/report.pdf", "application/pdf", pdf),
        ("paper/deck.pptx", crate::artifact_policy::PPTX_MIME, pptx),
    ];

    for (path, mime_type, bytes) in cases {
        let export = issue_mcp_artifact_export(
            runtime.clone(),
            registry.clone(),
            auth.clone(),
            path,
            &bytes,
            mime_type,
        )
        .await;
        let result = &export["result"];
        assert_eq!(result["isError"], false, "export: {export:?}");
        assert_eq!(result["content"].as_array().unwrap().len(), 1);
        let link = &result["content"][0];
        assert_eq!(link["type"], "resource_link");
        assert_eq!(link["mimeType"], mime_type);
        assert_eq!(
            link["name"],
            std::path::Path::new(path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
        let uri = link["uri"].as_str().unwrap().to_string();
        assert!(uri.starts_with(MCP_ARTIFACT_EXPORT_URI_PREFIX));
        let structured = serde_json::to_string(&result["structuredContent"]).unwrap();
        assert!(!structured.contains(MCP_ARTIFACT_EXPORT_URI_PREFIX));
        assert!(!structured.contains("content_base64"));
        assert!(!structured.contains("\"blob\""));

        let listed = handle_mcp_request(
            &runtime,
            rpc(
                "resources/list",
                Some(json!(3102)),
                mcp_2026_params(json!({})),
            ),
            Some(&auth),
        )
        .await;
        let McpOutcome::Ok(listed) = listed else {
            panic!("resources/list must succeed");
        };
        assert!(!serde_json::to_string(&listed).unwrap().contains(&uri));

        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let read = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let uri = uri.clone();
            async move {
                handle_mcp_request(
                    &runtime,
                    rpc(
                        "resources/read",
                        Some(json!(3103)),
                        mcp_2026_params(json!({"uri": uri})),
                    ),
                    Some(&auth),
                )
                .await
            }
        });
        complete_mcp_export_resource_read(
            registry.clone(),
            path,
            bytes.clone(),
            mime_type,
            &sha256,
            McpExportChunkFault::None,
        )
        .await;
        let outcome = read.await.unwrap();
        let McpOutcome::Ok(value) = outcome else {
            panic!("resources/read must return embedded binary, got {outcome:?}");
        };
        let contents = &value["result"]["contents"][0];
        assert_eq!(contents["uri"], uri);
        assert_eq!(contents["mimeType"], mime_type);
        let decoded = general_purpose::STANDARD
            .decode(contents["blob"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded, bytes);
        assert!(value["result"].get("structuredContent").is_none());
    }
}

#[tokio::test]
async fn mcp_artifact_export_old_runner_uses_safe_legacy_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) =
        mcp_export_runtime_with_optimized_chunk(tmp.path(), Some("alice"), false).await;
    let auth = mcp_export_api_auth("key-legacy", "alice");
    let bytes = vec![0x41; 70 * 1024];
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        "paper/legacy.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let read = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let uri = uri.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "resources/read",
                    Some(json!(3120)),
                    mcp_2026_params(json!({"uri": uri})),
                ),
                Some(&auth),
            )
            .await
        }
    });
    complete_mcp_export_resource_read_legacy(
        registry,
        "paper/legacy.pdf",
        bytes.clone(),
        "application/pdf",
        &sha256,
    )
    .await;
    let McpOutcome::Ok(value) = read.await.unwrap() else {
        panic!("old Runner must use the explicit legacy fallback");
    };
    let decoded = general_purpose::STANDARD
        .decode(value["result"]["contents"][0]["blob"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, bytes);
}

#[tokio::test]
async fn mcp_artifact_export_optimized_pipeline_is_four_way_bounded_and_offset_ordered() {
    use crate::shell_protocol::ShellAgentPollRequest;
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-pipeline", "alice");
    let path = "paper/pipeline.pdf";
    let size = MAX_READ_PROJECT_ARTIFACT_LENGTH * 9 + 123;
    let bytes: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        path,
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let read = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let uri = uri.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "resources/read",
                    Some(json!(3130)),
                    mcp_2026_params(json!({"uri": uri})),
                ),
                Some(&auth),
            )
            .await
        }
    });

    complete_mcp_export_metadata(
        registry.clone(),
        path,
        bytes.len(),
        &sha256,
        "application/pdf",
    )
    .await;
    let first = poll_mcp_export_request(&registry).await;
    assert_eq!(
        mcp_export_optimized_chunk_range(&first, path, bytes.len()).0,
        0
    );
    complete_mcp_export_optimized_chunk(&registry, first, path, &bytes).await;

    let mut first_batch = Vec::new();
    for _ in 0..MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS {
        first_batch.push(poll_mcp_export_request(&registry).await);
    }
    first_batch
        .sort_by_key(|request| mcp_export_optimized_chunk_range(request, path, bytes.len()).0);
    let first_offsets: Vec<usize> = first_batch
        .iter()
        .map(|request| mcp_export_optimized_chunk_range(request, path, bytes.len()).0)
        .collect();
    assert_eq!(
        first_offsets,
        (1..=4)
            .map(|index| index * MAX_READ_PROJECT_ARTIFACT_LENGTH)
            .collect::<Vec<_>>()
    );
    assert!(
        registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .is_none(),
        "the optimized batch must not dispatch a fifth chunk"
    );

    let mut batch = first_batch.into_iter();
    let b0 = batch.next().unwrap();
    let b1 = batch.next().unwrap();
    let b2 = batch.next().unwrap();
    let b3 = batch.next().unwrap();
    for request in [b3, b1, b2] {
        complete_mcp_export_optimized_chunk(&registry, request, path, &bytes).await;
    }
    assert!(
        registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .is_none(),
        "the next batch must wait until every request in the current batch is drained"
    );
    complete_mcp_export_optimized_chunk(&registry, b0, path, &bytes).await;

    let mut second_batch = Vec::new();
    for _ in 0..MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS {
        second_batch.push(poll_mcp_export_request(&registry).await);
    }
    second_batch
        .sort_by_key(|request| mcp_export_optimized_chunk_range(request, path, bytes.len()).0);
    assert_eq!(
        second_batch
            .iter()
            .map(|request| mcp_export_optimized_chunk_range(request, path, bytes.len()).0)
            .collect::<Vec<_>>(),
        (5..=8)
            .map(|index| index * MAX_READ_PROJECT_ARTIFACT_LENGTH)
            .collect::<Vec<_>>()
    );
    assert!(
        registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .is_none(),
        "a later optimized batch must keep the same four-request bound"
    );
    while let Some(request) = second_batch.pop() {
        complete_mcp_export_optimized_chunk(&registry, request, path, &bytes).await;
    }

    let final_chunk = poll_mcp_export_request(&registry).await;
    assert_eq!(
        mcp_export_optimized_chunk_range(&final_chunk, path, bytes.len()).0,
        9 * MAX_READ_PROJECT_ARTIFACT_LENGTH
    );
    complete_mcp_export_optimized_chunk(&registry, final_chunk, path, &bytes).await;

    let McpOutcome::Ok(value) = read.await.unwrap() else {
        panic!("optimized pipelined resource read must succeed");
    };
    let decoded = general_purpose::STANDARD
        .decode(value["result"]["contents"][0]["blob"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, bytes);
    assert_eq!(format!("{:x}", Sha256::digest(&decoded)), sha256);
    assert_eq!(
        registry
            .get_client_view("exporter")
            .await
            .unwrap()
            .pending_requests,
        0
    );
}

#[tokio::test]
async fn mcp_artifact_export_total_timeout_cleans_abandoned_pending_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-pipeline-timeout", "alice");
    let path = "paper/pipeline-timeout.pdf";
    let bytes: Vec<u8> = (0..MAX_READ_PROJECT_ARTIFACT_LENGTH * 5)
        .map(|index| (index % 233) as u8)
        .collect();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        path,
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let gate = Arc::new(Semaphore::new(1));
    let read = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let gate = gate.clone();
        async move {
            mcp_artifact_export_resource_read_with_gate_timeout(
                &runtime,
                &uri,
                Some(&auth),
                gate.as_ref(),
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .await
        }
    });

    complete_mcp_export_metadata(
        registry.clone(),
        path,
        bytes.len(),
        &sha256,
        "application/pdf",
    )
    .await;
    let first = poll_mcp_export_request(&registry).await;
    complete_mcp_export_optimized_chunk(&registry, first, path, &bytes).await;

    let mut inflight = Vec::new();
    for _ in 0..MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS {
        inflight.push(poll_mcp_export_request(&registry).await);
    }
    assert!(matches!(
        read.await.unwrap(),
        Err(McpArtifactExportReadError::Timeout)
    ));
    assert_eq!(gate.available_permits(), 1);
    for request in inflight {
        assert!(
            !registry.cancel_request(&request.request_id).await,
            "resource timeout must remove every abandoned optimized chunk request"
        );
    }

    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        "paper/metadata-timeout.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let read = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let gate = gate.clone();
        async move {
            mcp_artifact_export_resource_read_with_gate_timeout(
                &runtime,
                &uri,
                Some(&auth),
                gate.as_ref(),
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .await
        }
    });
    let metadata = poll_mcp_export_request(&registry).await;
    assert_eq!(metadata.kind, "file_read_project_artifact_metadata");
    assert!(matches!(
        read.await.unwrap(),
        Err(McpArtifactExportReadError::Timeout)
    ));
    assert!(
        !registry.cancel_request(&metadata.request_id).await,
        "resource timeout must also remove an abandoned metadata recheck"
    );
}

#[tokio::test]
async fn mcp_artifact_export_optimized_batch_drains_before_offset_ordered_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-pipeline-error", "alice");
    let path = "paper/pipeline-error.pdf";
    let bytes: Vec<u8> = (0..MAX_READ_PROJECT_ARTIFACT_LENGTH * 5)
        .map(|index| (index % 239) as u8)
        .collect();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        path,
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let mut read = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            handle_mcp_request(
                &runtime,
                rpc(
                    "resources/read",
                    Some(json!(3131)),
                    mcp_2026_params(json!({"uri": uri})),
                ),
                Some(&auth),
            )
            .await
        }
    });

    complete_mcp_export_metadata(
        registry.clone(),
        path,
        bytes.len(),
        &sha256,
        "application/pdf",
    )
    .await;
    let first = poll_mcp_export_request(&registry).await;
    complete_mcp_export_optimized_chunk(&registry, first, path, &bytes).await;

    let mut batch = Vec::new();
    for _ in 0..MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS {
        batch.push(poll_mcp_export_request(&registry).await);
    }
    batch.sort_by_key(|request| mcp_export_optimized_chunk_range(request, path, bytes.len()).0);
    let mut batch = batch.into_iter();
    let earliest = batch.next().unwrap();
    let later_unsafe = batch.next().unwrap();
    let good_two = batch.next().unwrap();
    let good_three = batch.next().unwrap();

    let (unsafe_offset, unsafe_end) =
        mcp_export_optimized_chunk_range(&later_unsafe, path, bytes.len());
    complete_mcp_export_request(
        &registry,
        later_unsafe,
        json!({
            "path": path,
            "file_bytes": bytes.len(),
            "offset": unsafe_offset + 1,
            "bytes_returned": unsafe_end - unsafe_offset,
            "content_base64": general_purpose::STANDARD.encode(&bytes[unsafe_offset..unsafe_end]),
            "next_offset": unsafe_end,
            "truncated": unsafe_end < bytes.len(),
            "eof": unsafe_end == bytes.len(),
        }),
    )
    .await;
    complete_mcp_export_optimized_chunk(&registry, good_three, path, &bytes).await;
    complete_mcp_export_optimized_chunk(&registry, good_two, path, &bytes).await;

    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut read)
            .await
            .is_err(),
        "one completed batch error must not short-circuit and drop another dispatched request"
    );

    complete_mcp_export_request(
        &registry,
        earliest,
        json!({
            "error_kind": "snapshot_changed",
            "error": Value::Null,
        }),
    )
    .await;

    match read.await.unwrap() {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert_eq!(
                value["error"]["message"],
                "Exported artifact no longer matches its snapshot"
            );
        }
        other => panic!("earliest requested-offset batch error must win, got {other:?}"),
    }
    assert_eq!(
        registry
            .get_client_view("exporter")
            .await
            .unwrap()
            .pending_requests,
        0
    );
}

#[tokio::test]
async fn mcp_artifact_export_same_size_mutations_fail_final_sha() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-mutation", "alice");
    let bytes = vec![0x5a; 70 * 1024];
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    for fault in [
        McpExportChunkFault::MutateFirstChunk,
        McpExportChunkFault::MutateLaterChunk,
    ] {
        let export = issue_mcp_artifact_export(
            runtime.clone(),
            registry.clone(),
            auth.clone(),
            "paper/mutation.pdf",
            &bytes,
            "application/pdf",
        )
        .await;
        let uri = export["result"]["content"][0]["uri"]
            .as_str()
            .unwrap()
            .to_string();
        let read = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move {
                handle_mcp_request(
                    &runtime,
                    rpc(
                        "resources/read",
                        Some(json!(3121)),
                        mcp_2026_params(json!({"uri": uri})),
                    ),
                    Some(&auth),
                )
                .await
            }
        });
        complete_mcp_export_resource_read(
            registry.clone(),
            "paper/mutation.pdf",
            bytes.clone(),
            "application/pdf",
            &sha256,
            fault,
        )
        .await;
        match read.await.unwrap() {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32602, "fault: {fault:?}");
                assert_eq!(
                    value["error"]["message"], "Exported artifact no longer matches its snapshot",
                    "fault: {fault:?}"
                );
            }
            other => panic!("same-size mutation {fault:?} must fail closed, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn mcp_artifact_export_backpressure_is_two_way_bounded_and_retryable() {
    use crate::shell_protocol::ShellAgentPollRequest;
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-gate", "alice");
    let bytes = b"%PDF-1.7\nconcurrent export\n%%EOF\n".to_vec();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let stable = ToolResult::ok(json!({
        "project": "agent:exporter:demo",
        "path": "paper/gate.pdf",
        "bytes": bytes.len(),
        "sha256": sha256,
        "mime_type": "application/pdf",
        "name": "gate.pdf",
    }));
    let caller = mcp_artifact_export_caller_binding(Some(&auth)).unwrap();
    let (uri, _) = mcp_issue_artifact_export(caller, &stable).unwrap();
    let gate = Arc::new(Semaphore::new(2));

    let spawn_read = |id: i64| {
        let runtime = runtime.clone();
        let auth = auth.clone();
        let uri = uri.clone();
        let gate = gate.clone();
        tokio::spawn(async move {
            let _ = id;
            mcp_artifact_export_resource_read_with_gate(
                &runtime,
                &uri,
                Some(&auth),
                gate.as_ref(),
                Duration::from_secs(1),
            )
            .await
        })
    };
    let first = spawn_read(1);
    let second = spawn_read(2);
    let first_metadata = poll_mcp_export_request(&registry).await;
    let second_metadata = poll_mcp_export_request(&registry).await;
    assert_eq!(first_metadata.kind, "file_read_project_artifact_metadata");
    assert_eq!(second_metadata.kind, "file_read_project_artifact_metadata");
    assert_eq!(gate.available_permits(), 0);

    let busy = mcp_artifact_export_resource_read_with_gate(
        &runtime,
        &uri,
        Some(&auth),
        gate.as_ref(),
        Duration::from_millis(25),
    )
    .await;
    assert!(matches!(busy, Err(McpArtifactExportReadError::Busy)));
    assert!(
        registry
            .poll(ShellAgentPollRequest {
                client_id: "exporter".to_string(),
                agent_instance_id: "inst-export".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .is_none(),
        "busy admission must not start a Runner read"
    );

    for request in [first_metadata, second_metadata] {
        complete_mcp_export_request(
            &registry,
            request,
            json!({
                "path": "paper/gate.pdf",
                "bytes": bytes.len(),
                "sha256": sha256,
                "mime_type": "application/pdf",
            }),
        )
        .await;
    }
    for _ in 0..2 {
        let request = poll_mcp_export_request(&registry).await;
        assert_eq!(request.kind, "file_read_project_artifact_export_chunk");
        let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        let offset = payload["offset"].as_u64().unwrap() as usize;
        let length = payload["length"].as_u64().unwrap() as usize;
        let end = offset.saturating_add(length).min(bytes.len());
        complete_mcp_export_request(
            &registry,
            request,
            json!({
                "path": "paper/gate.pdf",
                "file_bytes": bytes.len(),
                "offset": offset,
                "bytes_returned": end - offset,
                "content_base64": general_purpose::STANDARD.encode(&bytes[offset..end]),
                "next_offset": end,
                "truncated": end < bytes.len(),
                "eof": end == bytes.len(),
            }),
        )
        .await;
    }
    assert!(first.await.unwrap().is_ok());
    assert!(second.await.unwrap().is_ok());
    assert_eq!(gate.available_permits(), 2);

    // The busy attempt did not consume the handle: the same authenticated
    // caller can retry it after capacity returns.
    let retry = spawn_read(3);
    let metadata = poll_mcp_export_request(&registry).await;
    complete_mcp_export_request(
        &registry,
        metadata,
        json!({
            "path": "paper/gate.pdf",
            "bytes": bytes.len(),
            "sha256": sha256,
            "mime_type": "application/pdf",
        }),
    )
    .await;
    let chunk = poll_mcp_export_request(&registry).await;
    let payload: Value = serde_json::from_str(chunk.content.as_deref().unwrap()).unwrap();
    let offset = payload["offset"].as_u64().unwrap() as usize;
    let length = payload["length"].as_u64().unwrap() as usize;
    let end = offset.saturating_add(length).min(bytes.len());
    complete_mcp_export_request(
        &registry,
        chunk,
        json!({
            "path": "paper/gate.pdf",
            "file_bytes": bytes.len(),
            "offset": offset,
            "bytes_returned": end - offset,
            "content_base64": general_purpose::STANDARD.encode(&bytes[offset..end]),
            "next_offset": end,
            "truncated": false,
            "eof": true,
        }),
    )
    .await;
    assert!(retry.await.unwrap().is_ok());
    assert_eq!(gate.available_permits(), 2);

    // A terminal snapshot failure also releases its RAII permit.
    let failed = spawn_read(4);
    let metadata = poll_mcp_export_request(&registry).await;
    complete_mcp_export_request(
        &registry,
        metadata,
        json!({
            "path": "paper/gate.pdf",
            "bytes": bytes.len(),
            "sha256": "b".repeat(64),
            "mime_type": "application/pdf",
        }),
    )
    .await;
    assert!(matches!(
        failed.await.unwrap(),
        Err(McpArtifactExportReadError::SnapshotChanged)
    ));
    assert_eq!(gate.available_permits(), 2);
}

#[tokio::test]
async fn mcp_artifact_export_resource_is_caller_bound_and_rechecks_project_authorization() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let creator = mcp_export_api_auth("key-owner", "alice");
    let bytes = b"%PDF-1.7\nowner bound\n%%EOF\n".to_vec();
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        creator.clone(),
        "private/report.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();

    let other_caller = mcp_export_api_auth("key-other", "alice");
    let stolen = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(3104)),
            mcp_2026_params(json!({"uri": uri.clone()})),
        ),
        Some(&other_caller),
    )
    .await;
    match stolen {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert_eq!(
                value["error"]["message"],
                "Artifact export resource is unavailable"
            );
        }
        other => panic!("stolen URI must not transfer authority, got {other:?}"),
    }

    let same_stable_identity_wrong_project_owner = mcp_export_api_auth("key-owner", "bob");
    let denied = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(3105)),
            mcp_2026_params(json!({"uri": uri})),
        ),
        Some(&same_stable_identity_wrong_project_owner),
    )
    .await;
    match denied {
        McpOutcome::BadRequest(value) => {
            assert_eq!(value["error"]["code"], -32602);
            assert_eq!(
                value["error"]["message"],
                "Artifact export resource is unavailable"
            );
        }
        other => panic!("project authorization must be rechecked, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_artifact_export_unknown_expired_and_changed_snapshots_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-snapshot", "alice");
    let unknown = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(3106)),
            mcp_2026_params(json!({
                "uri": "webcodex-artifact://export/wc_export_0123456789abcdef0123456789abcdef"
            })),
        ),
        Some(&auth),
    )
    .await;
    assert!(matches!(unknown, McpOutcome::BadRequest(_)));

    let bytes = b"%PDF-1.7\nsnapshot\n%%EOF\n".to_vec();
    let original_sha = format!("{:x}", Sha256::digest(&bytes));
    let expired_export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        "paper/snapshot.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let expired_uri = expired_export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    mcp_expire_artifact_export_for_test(&expired_uri);
    let expired = handle_mcp_request(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(3107)),
            mcp_2026_params(json!({"uri": expired_uri})),
        ),
        Some(&auth),
    )
    .await;
    assert!(matches!(expired, McpOutcome::BadRequest(_)));

    for (changed_bytes, changed_sha, changed_mime) in [
        (bytes.len() + 1, original_sha.clone(), "application/pdf"),
        (bytes.len(), "b".repeat(64), "application/pdf"),
        (bytes.len(), original_sha.clone(), "application/zip"),
    ] {
        let export = issue_mcp_artifact_export(
            runtime.clone(),
            registry.clone(),
            auth.clone(),
            "paper/snapshot.pdf",
            &bytes,
            "application/pdf",
        )
        .await;
        let uri = export["result"]["content"][0]["uri"]
            .as_str()
            .unwrap()
            .to_string();
        let read = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let uri = uri.clone();
            async move {
                handle_mcp_request(
                    &runtime,
                    rpc(
                        "resources/read",
                        Some(json!(3108)),
                        mcp_2026_params(json!({"uri": uri})),
                    ),
                    Some(&auth),
                )
                .await
            }
        });
        complete_mcp_export_metadata(
            registry.clone(),
            "paper/snapshot.pdf",
            changed_bytes,
            &changed_sha,
            changed_mime,
        )
        .await;
        let outcome = read.await.unwrap();
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32602);
                assert_eq!(
                    value["error"]["message"],
                    "Exported artifact no longer matches its snapshot"
                );
            }
            other => panic!("changed export snapshot must fail closed, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn mcp_artifact_export_malformed_chunk_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), Some("alice")).await;
    let auth = mcp_export_api_auth("key-malformed", "alice");
    let bytes = vec![0x5a; 70 * 1024];
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let export = issue_mcp_artifact_export(
        runtime.clone(),
        registry.clone(),
        auth.clone(),
        "paper/malformed.pdf",
        &bytes,
        "application/pdf",
    )
    .await;
    let uri = export["result"]["content"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    for fault in [
        McpExportChunkFault::InvalidBase64,
        McpExportChunkFault::Offset,
        McpExportChunkFault::Eof,
    ] {
        let read = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let uri = uri.clone();
            async move {
                handle_mcp_request(
                    &runtime,
                    rpc(
                        "resources/read",
                        Some(json!(3109)),
                        mcp_2026_params(json!({"uri": uri})),
                    ),
                    Some(&auth),
                )
                .await
            }
        });
        complete_mcp_export_resource_read(
            registry.clone(),
            "paper/malformed.pdf",
            bytes.clone(),
            "application/pdf",
            &sha256,
            fault,
        )
        .await;
        let outcome = read.await.unwrap();
        match outcome {
            McpOutcome::BadRequest(value) => {
                assert_eq!(value["error"]["code"], -32603, "fault: {fault:?}");
                assert_eq!(
                    value["error"]["message"],
                    "Artifact export resource failed bounded safety validation",
                    "fault: {fault:?}"
                );
            }
            other => panic!("malformed chunk {fault:?} must fail closed, got {other:?}"),
        }
    }
}

#[test]
fn mcp_artifact_export_preserves_ten_mib_bound_and_durable_projection_has_no_handle_or_blob() {
    let output = json!({
        "project": "agent:exporter:demo",
        "path": "paper/too-large.pdf",
        "bytes": MAX_PROJECT_ARTIFACT_BYTES + 1,
        "sha256": "a".repeat(64),
        "mime_type": "application/pdf",
        "name": "too-large.pdf",
    });
    let error =
        validate_project_artifact_export_snapshot("paper/too-large.pdf", &output).unwrap_err();
    assert!(error.contains("maximum"));

    let stable = ToolResult::ok(json!({
        "project": "agent:exporter:demo",
        "path": "paper/report.pdf",
        "bytes": 11,
        "sha256": "a".repeat(64),
        "mime_type": "application/pdf",
        "name": "report.pdf",
    }));
    let durable =
        crate::tool_runtime::audit_safe_result_for_tool("export_project_artifact", &stable.output);
    let serialized = serde_json::to_string(&durable).unwrap();
    assert!(!serialized.contains(MCP_ARTIFACT_EXPORT_URI_PREFIX));
    assert!(!serialized.contains("content_base64"));
    assert!(!serialized.contains("\"blob\""));
}

#[test]
fn mcp_artifact_export_registry_is_bounded_fair_and_cleans_expired_entries() {
    let snapshot = ProjectArtifactExportSnapshot {
        path: "paper/report.pdf".to_string(),
        bytes: 1,
        sha256: "a".repeat(64),
        mime_type: "application/pdf".to_string(),
        name: "report.pdf".to_string(),
    };
    let caller_a = McpArtifactExportCallerBinding::ApiToken {
        api_key_id: "key-registry-a".to_string(),
    };
    let caller_b = McpArtifactExportCallerBinding::ApiToken {
        api_key_id: "key-registry-b".to_string(),
    };
    let mut registry = McpArtifactExportRegistry::default();
    let expired_uri = registry.insert(McpArtifactExportRecord {
        caller: caller_a.clone(),
        project: "agent:exporter:demo".to_string(),
        snapshot: snapshot.clone(),
        expires_at: Instant::now(),
    });
    assert!(registry.get_for_caller(&expired_uri, &caller_a).is_none());
    assert!(registry.entries.is_empty());

    let b_uri = registry.insert(McpArtifactExportRecord {
        caller: caller_b.clone(),
        project: "agent:exporter:demo".to_string(),
        snapshot: snapshot.clone(),
        expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
    });
    let mut a_uris = Vec::new();
    for _ in 0..MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER {
        a_uris.push(registry.insert(McpArtifactExportRecord {
            caller: caller_a.clone(),
            project: "agent:exporter:demo".to_string(),
            snapshot: snapshot.clone(),
            expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
        }));
    }
    let a_oldest = a_uris[0].clone();
    let a_17th = registry.insert(McpArtifactExportRecord {
        caller: caller_a.clone(),
        project: "agent:exporter:demo".to_string(),
        snapshot: snapshot.clone(),
        expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
    });
    assert!(registry.get_for_caller(&a_oldest, &caller_a).is_none());
    assert!(registry.get_for_caller(&a_17th, &caller_a).is_some());
    assert!(
        registry.get_for_caller(&b_uri, &caller_b).is_some(),
        "caller A churn must not evict caller B while A is constrained by its own quota"
    );
    assert_eq!(
        registry
            .entries
            .values()
            .filter(|record| record.caller == caller_a)
            .count(),
        MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER
    );

    let mut global = McpArtifactExportRegistry::default();
    for caller_index in 0..(MAX_MCP_ARTIFACT_EXPORTS / MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER) {
        let caller = McpArtifactExportCallerBinding::ApiToken {
            api_key_id: format!("key-global-{caller_index}"),
        };
        for _ in 0..MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER {
            global.insert(McpArtifactExportRecord {
                caller: caller.clone(),
                project: "agent:exporter:demo".to_string(),
                snapshot: snapshot.clone(),
                expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
            });
        }
    }
    assert_eq!(global.entries.len(), MAX_MCP_ARTIFACT_EXPORTS);
    assert_eq!(global.order.len(), MAX_MCP_ARTIFACT_EXPORTS);
    let extra_caller = McpArtifactExportCallerBinding::ApiToken {
        api_key_id: "key-global-extra".to_string(),
    };
    global.insert(McpArtifactExportRecord {
        caller: extra_caller,
        project: "agent:exporter:demo".to_string(),
        snapshot,
        expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
    });
    assert_eq!(global.entries.len(), MAX_MCP_ARTIFACT_EXPORTS);
    assert_eq!(global.order.len(), MAX_MCP_ARTIFACT_EXPORTS);
}

#[tokio::test]
async fn mcp_artifact_export_action_audit_does_not_persist_handle_or_blob() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_export_runtime(tmp.path(), None).await;
    let (_db_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db.clone(),
        runtime,
    ));
    let bytes = b"%PDF-1.7\naudit export\n%%EOF\n".to_vec();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let agent = tokio::spawn({
        let sha256 = sha256.clone();
        let bytes_len = bytes.len();
        async move {
            complete_mcp_export_metadata(
                registry,
                "paper/audit.pdf",
                bytes_len,
                &sha256,
                "application/pdf",
            )
            .await;
        }
    });
    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "export_project_artifact", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3112,
            "method": "tools/call",
            "params": mcp_2026_params(json!({
                "name": "export_project_artifact",
                "arguments": {
                    "project": "agent:exporter:demo",
                    "path": "paper/audit.pdf"
                }
            }))
        }))
        .send(&service)
        .await;
    agent.await.unwrap();
    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    let uri = body["result"]["content"][0]["uri"]
        .as_str()
        .expect("successful export must return resource link");
    assert!(uri.starts_with(MCP_ARTIFACT_EXPORT_URI_PREFIX));

    let (operation, summary, error): (String, String, String) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT operation, summary_json, COALESCE(error_summary, '') FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    };
    assert_eq!(operation, "export_project_artifact");
    for durable in [&summary, &error] {
        assert!(!durable.contains(MCP_ARTIFACT_EXPORT_URI_PREFIX));
        assert!(!durable.contains("wc_export_"));
        assert!(!durable.contains("content_base64"));
        assert!(!durable.contains("\"blob\""));
    }
}

#[tokio::test]
async fn pat_created_replacement_client_with_same_redirect_remains_untrusted() {
    let (_db_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let trusted =
        seed_mcp_import_client(&db, &user, "ChatGPT WebCodex", MCP_IMPORT_TRUSTED_REDIRECT);
    let config = mcp_import_config(&[trusted.client_id.as_str()]);
    let pat = seed_mcp_import_pat(&db, &user);
    let service = Service::new(build_mcp_import_oauth_management_router(
        config.clone(),
        db.clone(),
    ));

    let mut revoked = TestClient::post("http://localhost/api/oauth/clients/revoke")
        .bearer_auth(&pat)
        .json(&json!({"client_id": trusted.client_id}))
        .send(&service)
        .await;
    assert_eq!(revoked.status_code, Some(StatusCode::OK));
    let revoked_body: Value = revoked.take_json().await.unwrap();
    assert_eq!(revoked_body["success"], true);

    let mut created = TestClient::post("http://localhost/api/oauth/clients/create")
        .bearer_auth(&pat)
        .json(&json!({
            "name": "ChatGPT WebCodex",
            "redirect_uris": [MCP_IMPORT_TRUSTED_REDIRECT],
            "allowed_scopes": ["project:write"]
        }))
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::OK));
    let created_body: Value = created.take_json().await.unwrap();
    let replacement_client_id = created_body["client"]["client_id"]
        .as_str()
        .expect("replacement client id");
    assert!(replacement_client_id.starts_with("wc_client_"));
    assert_ne!(replacement_client_id, trusted.client_id);
    assert_eq!(
        created_body["client"]["redirect_uris"][0],
        MCP_IMPORT_TRUSTED_REDIRECT
    );

    // Model the strongest downstream case: even if the PAT holder completes
    // OAuth for the replacement and obtains a valid access token, its
    // allowed_client_id is the new server-generated ID and cannot match the
    // operator-controlled trust configuration for the revoked client.
    let replacement_auth = mcp_import_oauth_auth(replacement_client_id);
    assert_eq!(
        mcp_host_file_import_trust_from_state(&config, &db, Some(&replacement_auth)),
        HostFileImportTrust::Untrusted
    );
}

#[test]
fn mcp_file_import_trust_decision_reports_exact_failure_stage() {
    let mut config = (*test_config_oauth2(Some("secret"))).clone();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client =
        seed_mcp_import_client(&db, &user, "ChatGPT WebCodex", MCP_IMPORT_TRUSTED_REDIRECT);
    config.oauth2.trusted_mcp_file_client_ids = vec![client.client_id.clone()];

    let missing_auth = mcp_host_file_import_trust_decision_from_state(&config, &db, None);
    assert_eq!(missing_auth.reason, HostFileImportTrustReason::MissingAuth);

    let api_auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    let not_oauth = mcp_host_file_import_trust_decision_from_state(&config, &db, Some(&api_auth));
    assert_eq!(not_oauth.reason, HostFileImportTrustReason::NotOAuthToken);

    let mut missing_id = mcp_import_oauth_auth(&client.client_id);
    missing_id.allowed_client_id = None;
    assert_eq!(
        mcp_host_file_import_trust_decision_from_state(&config, &db, Some(&missing_id)).reason,
        HostFileImportTrustReason::MissingAllowedClientId
    );

    let mut disabled = config.clone();
    disabled.oauth2.enabled = false;
    assert_eq!(
        mcp_host_file_import_trust_decision_from_state(
            &disabled,
            &db,
            Some(&mcp_import_oauth_auth(&client.client_id))
        )
        .reason,
        HostFileImportTrustReason::OAuthDisabled
    );

    let other_client_id = crate::auth::generate_oauth_client_id();
    assert_eq!(
        mcp_host_file_import_trust_decision_from_state(
            &config,
            &db,
            Some(&mcp_import_oauth_auth(&other_client_id))
        )
        .reason,
        HostFileImportTrustReason::ClientIdNotConfigured
    );

    let unknown_configured_id = crate::auth::generate_oauth_client_id();
    let mut unknown_config = config.clone();
    unknown_config.oauth2.trusted_mcp_file_client_ids = vec![unknown_configured_id.clone()];
    assert_eq!(
        mcp_host_file_import_trust_decision_from_state(
            &unknown_config,
            &db,
            Some(&mcp_import_oauth_auth(&unknown_configured_id))
        )
        .reason,
        HostFileImportTrustReason::ClientRegistrationMissingOrRevoked
    );

    let trusted = mcp_host_file_import_trust_decision_from_state(
        &config,
        &db,
        Some(&mcp_import_oauth_auth(&client.client_id)),
    );
    assert_eq!(trusted.reason, HostFileImportTrustReason::Trusted);
    assert_eq!(trusted.trust, HostFileImportTrust::TrustedOAuthClient);
    assert_eq!(trusted.client_id_configured, Some(true));
    assert_eq!(trusted.active_client_registration_found, Some(true));
}

#[tokio::test]
async fn oauth_mcp_file_import_startup_env_stateless_2026_crosses_provenance_gate() {
    use crate::auth::{OAuth2Verifier, TokenVerifier};
    use sha2::{Digest, Sha256};

    let _lock = lock_mcp_import_test().await;
    let pptx = b"startup-env-stateless-trusted-pptx".to_vec();
    let expected_sha256 = format!("{:x}", Sha256::digest(&pptx));
    let server = start_mcp_import_mock_server(mcp_import_http_response(
        "200 OK",
        &[("Content-Length", pptx.len().to_string())],
        &pptx,
    ))
    .await;
    let _network = McpImportNetworkOverride::set(server.base_url.clone());

    let (_db_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client =
        seed_mcp_import_client(&db, &user, "ChatGPT WebCodex", MCP_IMPORT_TRUSTED_REDIRECT);
    let token = seed_oauth_access_token(&db, &client, &user, "project:write");
    let env_file_other_client_id = crate::auth::generate_oauth_client_id();
    let config = mcp_import_config_from_startup_env(&client.client_id, &env_file_other_client_id);

    // Stage A: the same verifier used by AuthMiddleware preserves the exact
    // OAuth client id on the authenticated context.
    let verifier = OAuth2Verifier;
    let verified = verifier
        .verify(config.as_ref(), Some(&db), &token)
        .await
        .unwrap()
        .expect("seeded OAuth access token must authenticate");
    assert_eq!(verified.kind, crate::auth::AuthKind::OAuth2Token);
    assert_eq!(verified.token_kind.as_deref(), Some("oauth2"));
    assert_eq!(
        verified.allowed_client_id.as_deref(),
        Some(client.client_id.as_str())
    );

    // Stage B: the parsed startup Config + active DB registration grants only
    // the exact configured OAuth client.
    let decision = mcp_host_file_import_trust_decision_from_state(
        config.as_ref(),
        db.as_ref(),
        Some(&verified),
    );
    assert_eq!(decision.reason, HostFileImportTrustReason::Trusted);
    assert_eq!(decision.trust, HostFileImportTrust::TrustedOAuthClient);

    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_import_runtime(project_tmp.path(), Some("alice")).await;
    let service = Service::new(build_test_router(config, db, runtime));
    let agent = tokio::spawn(complete_mcp_import_save(registry, pptx.clone()));
    let temporary_url = "https://download.example/temporary-secret-token/stateless-import.pptx";
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        mcp_2026_params(json!({
            "name": "import_conversation_files_to_project",
            "arguments": {
                "project": "agent:importer:demo",
                "openaiFileIdRefs": [{
                    "download_url": temporary_url,
                    "file_id": "file_stateless_host_rewritten",
                    "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                    "file_name": "source.pptx"
                }],
                "output_dir": "paper/export",
                "targets": ["stateless-import.pptx"],
                "overwrite": false,
                "trusted_mcp_host_file_import": false
            }
        })),
    )
    .await;
    // Stage C: observe the exact mcp_post trust decision after AuthMiddleware.
    let mcp_decision = take_last_mcp_host_file_import_trust_decision()
        .expect("mcp_post must evaluate host-file trust for the import tool");
    assert_eq!(mcp_decision.reason, HostFileImportTrustReason::Trusted);
    assert_eq!(mcp_decision.trust, HostFileImportTrust::TrustedOAuthClient);

    // Stages D-E: the kernel injects the internal provenance bit after JSON
    // deserialization and dispatch crosses the pre-network provenance gate to
    // SaveProjectArtifact.
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["resultType"], "complete", "body: {body:?}");
    assert_eq!(body["result"]["isError"], false, "body: {body:?}");
    tokio::time::timeout(std::time::Duration::from_secs(5), agent)
        .await
        .unwrap_or_else(|_| panic!("save_project_artifact fixture timed out; body: {body:?}"))
        .unwrap();
    let imported = &body["result"]["structuredContent"]["output"]["imported"][0];
    assert_eq!(imported["path"], "paper/export/stateless-import.pptx");
    assert_eq!(imported["bytes_written"], pptx.len());
    assert_eq!(imported["sha256"], expected_sha256);
    assert_eq!(
        crate::tool_runtime::conversation_import::import_test_dns_resolution_count(),
        1
    );
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains(temporary_url));
    assert!(!serialized.contains("file_stateless_host_rewritten"));
}

#[tokio::test]
async fn oauth_mcp_file_import_trusted_client_saves_pptx() {
    use sha2::{Digest, Sha256};

    let _lock = lock_mcp_import_test().await;
    let pptx = b"trusted-chatgpt-pptx-attachment".to_vec();
    let expected_sha256 = format!("{:x}", Sha256::digest(&pptx));
    let server = start_mcp_import_mock_server(mcp_import_http_response(
        "200 OK",
        &[("Content-Length", pptx.len().to_string())],
        &pptx,
    ))
    .await;
    let _network = McpImportNetworkOverride::set(server.base_url.clone());

    let (_db_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client =
        seed_mcp_import_client(&db, &user, "ChatGPT WebCodex", MCP_IMPORT_TRUSTED_REDIRECT);
    let token = seed_oauth_access_token(&db, &client, &user, "project:write");
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_import_runtime(project_tmp.path(), Some("alice")).await;
    let service = Service::new(build_test_router(
        mcp_import_config(&[client.client_id.as_str()]),
        db.clone(),
        runtime,
    ));
    let agent = tokio::spawn(complete_mcp_import_save(registry, pptx.clone()));
    let temporary_url = "https://download.example/temporary-secret-token/import-test.pptx";
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": "import_conversation_files_to_project",
            "arguments": {
                "project": "agent:importer:demo",
                "openaiFileIdRefs": [{
                    "download_url": temporary_url,
                    "file_id": "file_host_rewritten",
                    "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                    "file_name": "source.pptx"
                }],
                "output_dir": "paper/export",
                "targets": ["import-test.pptx"],
                "overwrite": false
            }
        }),
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), agent)
        .await
        .expect("save_project_artifact fixture timed out")
        .unwrap();

    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], false, "body: {body:?}");
    let imported = &body["result"]["structuredContent"]["output"]["imported"][0];
    assert_eq!(imported["path"], "paper/export/import-test.pptx");
    assert_eq!(imported["bytes_written"], pptx.len());
    assert_eq!(imported["sha256"], expected_sha256);
    assert_eq!(
        imported["mime_type"],
        "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    );
    assert_eq!(
        crate::tool_runtime::conversation_import::import_test_dns_resolution_count(),
        1,
        "trusted hostname must be resolved exactly once before the request"
    );

    let (summary, error): (String, String) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT summary_json, COALESCE(error_summary, '') FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    };
    assert!(!summary.contains(temporary_url));
    assert!(!error.contains(temporary_url));
}

#[tokio::test]
async fn oauth_mcp_file_import_trusted_download_guards_remain_bounded() {
    let _lock = lock_mcp_import_test().await;
    let (_db_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client =
        seed_mcp_import_client(&db, &user, "ChatGPT WebCodex", MCP_IMPORT_TRUSTED_REDIRECT);
    let token = seed_oauth_access_token(&db, &client, &user, "project:write");
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = mcp_import_runtime(project_tmp.path(), Some("alice")).await;
    let service = Service::new(build_test_router(
        mcp_import_config(&[client.client_id.as_str()]),
        db,
        runtime,
    ));
    let temporary_url = "https://download.example/DO-NOT-LOG/trusted-guard.pptx";
    let params = json!({
        "name": "import_conversation_files_to_project",
        "arguments": {
            "project": "agent:importer:demo",
            "openaiFileIdRefs": [{
                "download_url": temporary_url,
                "file_id": "file_host_guard",
                "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "file_name": "guard.pptx"
            }],
            "targets": ["guard.pptx"]
        }
    });

    let cases = vec![
        (
            mcp_import_http_response(
                "302 Found",
                &[("Location", "https://public.example/other".to_string())],
                b"",
            ),
            "HTTP 302",
            false,
        ),
        (
            mcp_import_http_response(
                "200 OK",
                &[(
                    "Content-Length",
                    (crate::tool_runtime::conversation_import::MAX_IMPORT_FILE_BYTES + 1)
                        .to_string(),
                )],
                b"",
            ),
            "exceeds",
            false,
        ),
        (
            mcp_import_http_response(
                "200 OK",
                &[],
                &vec![b'x'; crate::tool_runtime::conversation_import::MAX_IMPORT_FILE_BYTES + 1],
            ),
            "exceeds",
            true,
        ),
    ];

    for (response, expected_error, requires_upload_cleanup) in cases {
        let server = start_mcp_import_mock_server(response).await;
        let network = McpImportNetworkOverride::set(server.base_url.clone());
        let upload_fixture = requires_upload_cleanup
            .then(|| tokio::spawn(complete_mcp_import_until_abort(registry.clone())));
        let (status, body, _) =
            oauth_mcp_request(&service, &token, "tools/call", params.clone()).await;
        if let Some(upload_fixture) = upload_fixture {
            tokio::time::timeout(std::time::Duration::from_secs(10), upload_fixture)
                .await
                .expect("over-limit import abort fixture timed out")
                .unwrap();
            assert!(!project_tmp.path().join("guard.pptx").exists());
        }
        assert_eq!(status, StatusCode::OK, "body: {body:?}");
        assert_eq!(body["result"]["isError"], true, "body: {body:?}");
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(serialized.contains(expected_error), "body: {body:?}");
        assert!(!serialized.contains(temporary_url));
        assert_eq!(
            crate::tool_runtime::conversation_import::import_test_dns_resolution_count(),
            1
        );
        drop(network);
        drop(server);
    }
}

#[tokio::test]
async fn mcp_file_import_untrusted_callers_fail_before_dns() {
    let _lock = lock_mcp_import_test().await;
    let _network = McpImportNetworkOverride::without_download();
    let (_db_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let ordinary = seed_mcp_import_client(
        &db,
        &user,
        "Ordinary MCP Client",
        "https://ordinary.example/oauth/callback",
    );
    let token = seed_oauth_access_token(&db, &ordinary, &user, "project:write");
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, _registry) = mcp_import_runtime(project_tmp.path(), Some("alice")).await;
    let configured_but_unrelated_client_id = crate::auth::generate_oauth_client_id();
    let service = Service::new(build_test_router(
        mcp_import_config(&[configured_but_unrelated_client_id.as_str()]),
        db,
        runtime,
    ));
    let temporary_url = "https://download.example/SHOULD-NOT-BE-RESOLVED/file.pptx";
    let params = json!({
        "name": "import_conversation_files_to_project",
        "arguments": {
            "project": "agent:importer:demo",
            "openaiFileIdRefs": [{
                "download_url": temporary_url,
                "file_id": "caller-controlled-id",
                "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "file_name": "safe.pptx"
            }],
            "targets": ["safe.pptx"]
        }
    });

    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/call", params.clone()).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], true);
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(serialized.contains("explicitly trusted OAuth MCP client"));
    assert!(!serialized.contains(temporary_url));
    assert_eq!(
        crate::tool_runtime::conversation_import::import_test_dns_resolution_count(),
        0,
        "ordinary OAuth client must be rejected before DNS/network"
    );

    crate::tool_runtime::conversation_import::reset_import_test_dns_resolution_count();
    let (status, body, _) = oauth_mcp_request(&service, "secret", "tools/call", params).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["result"]["isError"], true);
    assert!(serde_json::to_string(&body)
        .unwrap()
        .contains("explicitly trusted OAuth MCP client"));
    assert_eq!(
        crate::tool_runtime::conversation_import::import_test_dns_resolution_count(),
        0,
        "raw/API-token MCP client must be rejected before DNS/network"
    );
}

/// Build a minimal Router matching the production /mcp wiring: Config,
/// Database, and ToolRuntime are injected so AuthMiddleware and mcp_post
/// resolve state exactly as in `main.rs`.
fn build_test_router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
    runtime: Arc<ToolRuntime>,
) -> Router {
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(runtime))
        .hoop(affix_state::inject(
            crate::connector_runtime::ConnectorRuntimeSlot::default(),
        ))
        .push(
            Router::with_path("mcp")
                .hoop(crate::AuthMiddleware)
                .get(mcp_info)
                .post(mcp_post),
        )
}

fn build_mcp_import_oauth_management_router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
) -> Router {
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .push(
            Router::with_path("api/oauth/clients")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("create").post(crate::oauth_http::oauth_clients_create))
                .push(Router::with_path("revoke").post(crate::oauth_http::oauth_clients_revoke)),
        )
}

fn build_connector_test_router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
    runtime: Arc<ToolRuntime>,
    project_root: &std::path::Path,
) -> Router {
    const PROJECT_GRANT_ID: &str = "wc_pgrant_3333333333333333";
    const PROJECT_CREDENTIAL: &str =
        "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let state_root = project_root
        .parent()
        .expect("connector test project parent")
        .join("connector-state");
    let connector = crate::connector_runtime::ConnectorRuntime::new(
        runtime.clone(),
        db.clone(),
        crate::connector_runtime::ConnectorContext {
            project_id: "wc_proj_1234567890".to_string(),
            project_name: "demo".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:demo".to_string(),
            executor_root: project_root.to_string_lossy().to_string(),
            runs_root: state_root.join("runs").to_string_lossy().to_string(),
            results_root: state_root.join("results").to_string_lossy().to_string(),
            projects_dir: state_root
                .join("agent/projects.d")
                .to_string_lossy()
                .to_string(),
            profile: "personal".to_string(),
            project_grant_id: PROJECT_GRANT_ID.to_string(),
        },
        crate::auth::ProjectCredentialVerifier::new(
            PROJECT_GRANT_ID.to_string(),
            PROJECT_CREDENTIAL,
        )
        .unwrap(),
    )
    .unwrap();
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(runtime))
        .hoop(affix_state::inject(
            crate::connector_runtime::ConnectorRuntimeSlot(Some(Arc::new(connector))),
        ))
        .push(
            Router::with_path("mcp")
                .hoop(crate::AuthMiddleware)
                .get(mcp_info)
                .post(mcp_post),
        )
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(crate::connector_runtime::http::routes())
                .push(Router::with_path("tools/call").post(crate::runtime_http::tools_call)),
        )
        .push(Router::with_path("openapi.json").get(crate::openapi::openapi_json))
}

/// Effective HTTP status: the explicitly set status_code, or OK when the
/// handler only rendered a body (Salvo defaults Json bodies to 200).
fn effective_status(resp: &Response) -> StatusCode {
    resp.status_code.unwrap_or(StatusCode::OK)
}

#[tokio::test]
async fn http_mcp_initialize_success() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let session_id = resp
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("initialize must mint an MCP session id");
    assert!(session_id.starts_with("wc_mcp_"));
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["result"]["serverInfo"]["name"], "webcodex");
    assert_eq!(
        body["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
    assert!(body["result"]["protocolVersion"].is_string());
    assert_eq!(
        body["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
}

// The asserted outputSchema presence is the default (non-compact) product
// behavior: `WEBCODEX_MCP_COMPACT_SCHEMAS` must stay unset (and serialized
// against other env-mutating tests) for the whole HTTP request.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_mcp_tools_list_success() {
    // Default (non-compact) HTTP tools/list: full schema fields present.
    // Compact-mode shape is covered by mcp_tools_list_compact_*.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 2);
    assert!(body["result"]["tools"].is_array());
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(
            tool["outputSchema"].is_object(),
            "default HTTP tools/list must include outputSchema for {}",
            tool["name"]
        );
    }
}

#[tokio::test]
async fn http_mcp_2026_validates_headers_and_ignores_legacy_session_id() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({}));

    let mut missing_headers = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 200,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_headers), StatusCode::BAD_REQUEST);
    let missing_body: Value = missing_headers.take_json().await.unwrap();
    assert_eq!(missing_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut ok = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-be-ignored",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 201,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&ok), StatusCode::OK);
    assert!(ok
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .is_none());
    let ok_body: Value = ok.take_json().await.unwrap();
    assert_eq!(ok_body["result"]["resultType"], "complete");
    assert_eq!(ok_body["result"]["cacheScope"], "private");

    let mut method_mismatch = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "ping", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 202,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&method_mismatch), StatusCode::BAD_REQUEST);
    let mismatch_body: Value = method_mismatch.take_json().await.unwrap();
    assert_eq!(mismatch_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut missing_capabilities = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2021,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&missing_capabilities),
        StatusCode::BAD_REQUEST
    );
    let missing_capabilities_body: Value = missing_capabilities.take_json().await.unwrap();
    assert_eq!(missing_capabilities_body["error"]["code"], -32602);

    let mut version_mismatch = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2022,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&version_mismatch), StatusCode::BAD_REQUEST);
    let version_mismatch_body: Value = version_mismatch.take_json().await.unwrap();
    assert_eq!(version_mismatch_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut malformed_client_info = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2023,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {"name": "missing-version"}
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&malformed_client_info),
        StatusCode::BAD_REQUEST
    );
    let malformed_client_info_body: Value = malformed_client_info.take_json().await.unwrap();
    assert_eq!(malformed_client_info_body["error"]["code"], -32602);

    let mut missing_jsonrpc = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "id": 2024,
            "method": "tools/list",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_jsonrpc), StatusCode::BAD_REQUEST);
    let missing_jsonrpc_body: Value = missing_jsonrpc.take_json().await.unwrap();
    assert_eq!(missing_jsonrpc_body["error"]["code"], -32600);

    let unsupported_params = json!({
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2099-01-01",
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    });
    let mut unsupported = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 203,
            "method": "tools/list",
            "params": unsupported_params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&unsupported), StatusCode::BAD_REQUEST);
    let unsupported_body: Value = unsupported.take_json().await.unwrap();
    assert_eq!(
        unsupported_body["error"]["code"],
        MCP_UNSUPPORTED_PROTOCOL_VERSION
    );
    assert_eq!(unsupported_body["error"]["data"]["requested"], "2099-01-01");
    assert_eq!(
        unsupported_body["error"]["data"]["supported"],
        json!(MCP_SUPPORTED_PROTOCOL_VERSIONS)
    );
}

#[tokio::test]
async fn http_mcp_2026_tools_call_requires_matching_name_and_accepts_base64_sentinel() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({"name": "list_projects", "arguments": {}}));

    let mut missing_name = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 204,
            "method": "tools/call",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_name), StatusCode::BAD_REQUEST);
    let missing_name_body: Value = missing_name.take_json().await.unwrap();
    assert_eq!(missing_name_body["error"]["code"], MCP_HEADER_MISMATCH);

    let encoded = general_purpose::STANDARD.encode("list_projects");
    let encoded = format!("=?base64?{encoded}?=");
    let mut ok = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, &encoded, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 205,
            "method": "tools/call",
            "params": params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&ok), StatusCode::OK);
    let ok_body: Value = ok.take_json().await.unwrap();
    assert_eq!(ok_body["result"]["resultType"], "complete");
}

#[tokio::test]
async fn http_mcp_2026_unknown_method_is_404_jsonrpc_method_not_found() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "prompts/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 206,
            "method": "prompts/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::NOT_FOUND);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn http_mcp_2026_reads_computer_app_template_with_cache_contract() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let params = mcp_2026_ui_params(json!({"uri": MCP_COMPUTER_UI_RESOURCE_URI}));

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, MCP_COMPUTER_UI_RESOURCE_URI, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2060,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["result"]["resultType"], "complete");
    assert_eq!(
        body["result"]["ttlMs"],
        Value::from(MCP_COMPUTER_UI_RESOURCE_TTL_MS)
    );
    assert_eq!(body["result"]["cacheScope"], "private");
    assert_eq!(
        body["result"]["contents"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );
    assert_eq!(
        body["result"]["contents"][0]["mimeType"],
        MCP_UI_RESOURCE_MIME_TYPE
    );
    assert!(body["result"]["contents"][0]["_meta"]
        .get("openai/widgetDomain")
        .is_none());
    let html = body["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(html.starts_with("<div id=\"app\""));
    assert!(html.contains("HTML loaded"));
    assert!(html.contains("ui/initialize"));
    assert!(html.contains("ui/notifications/initialized"));
    assert!(html.contains("ui/notifications/tool-result"));
    assert!(!html.contains("tools/call"));
    assert!(!html.contains("ui/request-display-mode"));

    let (endpoint, action, operation, status, http_status, summary): (
        String,
        String,
        String,
        String,
        i64,
        String,
    ) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT endpoint, action_name, operation, status, http_status, summary_json FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap()
    };
    assert_eq!(endpoint, "/mcp");
    assert_eq!(action, "resourcesRead");
    assert_eq!(operation, "computer_app_resource_read");
    assert_eq!(status, "success");
    assert_eq!(http_status, 200);
    let summary: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(
        summary,
        json!({
            "transport": "mcp",
            "resource_uri": MCP_COMPUTER_UI_RESOURCE_URI,
            "resource_version": "v11",
            "protocol_era": "stateless_2026",
            "ui_capability_present": true,
            "mcp_error_code": Value::Null,
        })
    );
    let durable = summary.to_string();
    for forbidden in ["HTML loaded", "content_base64", "surface_", "Waiting for"] {
        assert!(!durable.contains(forbidden));
    }
}

#[tokio::test]
async fn http_mcp_computer_app_resource_protocol_failure_is_audited_without_content() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db.clone(), runtime));
    let mut params = mcp_2026_ui_params(json!({"uri": MCP_COMPUTER_UI_RESOURCE_URI}));
    params["_meta"]["io.modelcontextprotocol/protocolVersion"] = Value::from("2099-01-01");

    let mut response = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(MCP_PROTOCOL_VERSION_HEADER, "2099-01-01", true)
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, MCP_COMPUTER_UI_RESOURCE_URI, true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 20601,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&response), StatusCode::BAD_REQUEST);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], MCP_UNSUPPORTED_PROTOCOL_VERSION);

    let (action, operation, status, http_status, summary): (String, String, String, i64, String) = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT action_name, operation, status, http_status, summary_json FROM action_events ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap()
    };
    assert_eq!(action, "resourcesRead");
    assert_eq!(operation, "computer_app_resource_read");
    assert_eq!(status, "failed");
    assert_eq!(http_status, 400);
    let summary: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(summary["resource_uri"], MCP_COMPUTER_UI_RESOURCE_URI);
    assert_eq!(summary["resource_version"], "v11");
    assert_eq!(summary["protocol_era"], "validation_failed");
    assert_eq!(summary["ui_capability_present"], true);
    assert_eq!(
        summary["mcp_error_code"],
        Value::from(MCP_UNSUPPORTED_PROTOCOL_VERSION)
    );
    let durable = summary.to_string();
    for forbidden in ["HTML loaded", "content_base64", "surface_", "Waiting for"] {
        assert!(!durable.contains(forbidden));
    }
}

#[tokio::test]
async fn http_mcp_2026_validates_resource_name_header_before_resource_contract() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let params = mcp_2026_params(json!({"uri": "file:///demo.txt"}));

    let mut missing_name = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2061,
            "method": "resources/read",
            "params": params.clone()
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_name), StatusCode::BAD_REQUEST);
    let missing_body: Value = missing_name.take_json().await.unwrap();
    assert_eq!(missing_body["error"]["code"], MCP_HEADER_MISMATCH);

    let mut unsupported = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "resources/read", true)
        .add_header(MCP_NAME_HEADER, "file:///demo.txt", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2062,
            "method": "resources/read",
            "params": params
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&unsupported), StatusCode::BAD_REQUEST);
    let unsupported_body: Value = unsupported.take_json().await.unwrap();
    assert_eq!(unsupported_body["error"]["code"], -32602);
}

#[tokio::test]
async fn http_mcp_2026_rejects_legacy_lifecycle_and_cross_origin_transport() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));

    let mut initialize = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "initialize", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 207,
            "method": "initialize",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&initialize), StatusCode::NOT_FOUND);
    let initialize_body: Value = initialize.take_json().await.unwrap();
    assert_eq!(initialize_body["error"]["code"], -32601);

    let cross_origin = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/list", true)
        .add_header("origin", "https://attacker.example", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 208,
            "method": "tools/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&cross_origin), StatusCode::FORBIDDEN);

    let legacy_cross_origin = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("origin", "https://attacker.example", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2081,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&legacy_cross_origin),
        StatusCode::FORBIDDEN
    );

    let legacy_cross_origin_get = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header("origin", "https://attacker.example", true)
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&legacy_cross_origin_get),
        StatusCode::FORBIDDEN
    );

    let modern_get = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&modern_get),
        StatusCode::METHOD_NOT_ALLOWED
    );

    let modern_delete = TestClient::delete("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&modern_delete),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn http_project_connector_lists_and_dispatches_only_canonical_capabilities() {
    // The runtime surface is explicit below and the request path never
    // re-reads WEBCODEX_MCP_MODEL_SURFACE, so no env lock is needed.
    let config = test_config(Some("secret"));
    let (tmp, db) = test_db();
    let project = tmp.path().join("connector-project");
    crate::connector_runtime::tests::init_repo(&project);
    let user_token = "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::CanonicalConnector));
    let service = Service::new(build_connector_test_router(config, db, runtime, &project));

    let mut discovery = TestClient::get("http://localhost/mcp")
        .bearer_auth(user_token)
        .send(&service)
        .await;
    assert_eq!(effective_status(&discovery), StatusCode::OK);
    let discovery_body: Value = discovery.take_json().await.unwrap();
    assert_eq!(
        discovery_body["modelSurface"],
        crate::model_surface::MODEL_SURFACE_CANONICAL_CONNECTOR
    );

    let mut schema = TestClient::get("http://localhost/openapi.json")
        .send(&service)
        .await;
    assert_eq!(effective_status(&schema), StatusCode::OK);
    let schema_body: Value = schema.take_json().await.unwrap();
    assert_eq!(schema_body["paths"].as_object().unwrap().len(), 14);
    assert!(schema_body["paths"]
        .get("/api/connector/task/start")
        .is_some());
    assert!(schema_body["paths"]
        .get("/api/connector/code/navigate")
        .is_some());
    assert!(schema_body["paths"]
        .get("/api/connector/code/impact")
        .is_some());
    assert!(schema_body["paths"].get("/api/tools/call").is_none());
    let action_checks_schema = schema_body["paths"]["/api/connector/checks/run"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();
    let action_navigation_schema = schema_body["paths"]["/api/connector/code/navigate"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();
    let action_impact_schema = schema_body["paths"]["/api/connector/code/impact"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]
        .clone();

    let mut listed = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&listed), StatusCode::OK);
    let listed_body: Value = listed.take_json().await.unwrap();
    let names = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, crate::connector_runtime::surface::CAPABILITY_NAMES);
    let mcp_checks_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "checks_run")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_checks_schema, action_checks_schema);
    let mcp_navigation_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "code_navigate")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_navigation_schema, action_navigation_schema);
    let mcp_impact_schema = listed_body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "code_impact")
        .unwrap()["inputSchema"]
        .clone();
    assert_eq!(mcp_impact_schema, action_impact_schema);

    let mut missing_window = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": { "goal": "must not create an anonymous context" }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&missing_window), StatusCode::BAD_REQUEST);
    let missing_window_body: Value = missing_window.take_json().await.unwrap();
    assert_eq!(missing_window_body["error"]["code"], -32600);
    assert!(missing_window_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("initialize"));

    let mut initialized = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&initialized), StatusCode::OK);
    let mcp_session_id = initialized
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("connector initialize session id")
        .to_string();
    let initialized_body: Value = initialized.take_json().await.unwrap();
    assert_eq!(
        initialized_body["result"]["serverInfo"]["modelSurface"],
        crate::model_surface::MODEL_SURFACE_CANONICAL_CONNECTOR
    );

    let mut action_started = TestClient::post("http://localhost/api/connector/task/start")
        .bearer_auth(user_token)
        .json(&json!({
            "goal": "exercise the Actions adapter",
            "mode": "read_only"
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&action_started), StatusCode::OK);
    let window_cookie = action_started
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("webcodex_window="))
        .and_then(|value| value.split(';').next())
        .expect("first connector call must mint a window cookie")
        .to_string();
    let action_body: Value = action_started.take_json().await.unwrap();
    assert_eq!(action_body["ok"], true);
    assert!(action_body["task_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_task_"));
    assert!(action_body.get("success").is_none());

    let mut action_continued = TestClient::post("http://localhost/api/connector/task/start")
        .bearer_auth(user_token)
        .add_header("cookie", &window_cookie, true)
        .json(&json!({
            "goal": "continue the Actions inspection",
            "mode": "read_only"
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&action_continued), StatusCode::OK);
    let continued_body: Value = action_continued.take_json().await.unwrap();
    assert_eq!(continued_body["task_id"], action_body["task_id"]);
    assert_eq!(continued_body["data"]["continuation"], "continued");

    let action_conversation_request = |conversation_id: &'static str, goal: &'static str| {
        TestClient::post("http://localhost/api/connector/task/start")
            .bearer_auth(user_token)
            .add_header("openai-conversation-id", conversation_id, true)
            .json(&json!({"goal": goal, "mode": "read_only"}))
    };
    let mut conversation_a = action_conversation_request("conversation-a", "conversation A work")
        .send(&service)
        .await;
    let conversation_a_body: Value = conversation_a.take_json().await.unwrap();
    let mut conversation_b = action_conversation_request("conversation-b", "conversation B work")
        .send(&service)
        .await;
    let conversation_b_body: Value = conversation_b.take_json().await.unwrap();
    assert_ne!(
        conversation_a_body["task_id"], conversation_b_body["task_id"],
        "one credential must not merge two hosted conversations"
    );
    let mut conversation_a_again =
        action_conversation_request("conversation-a", "conversation A follow-up")
            .send(&service)
            .await;
    let conversation_a_again_body: Value = conversation_a_again.take_json().await.unwrap();
    assert_eq!(
        conversation_a_again_body["task_id"],
        conversation_a_body["task_id"]
    );
    assert_eq!(
        conversation_a_again_body["data"]["continuation"],
        "continued"
    );

    let mut legacy = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth(user_token)
        .json(&json!({ "name": "runtime_status", "arguments": {} }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&legacy), StatusCode::FORBIDDEN);
    let legacy_body: Value = legacy.take_json().await.unwrap();
    assert!(legacy_body["error"]
        .as_str()
        .unwrap()
        .contains("canonical connector capabilities"));

    let mut started = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            &mcp_session_id,
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": { "goal": "inspect the project", "mode": "read_only" }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&started), StatusCode::OK);
    let started_body: Value = started.take_json().await.unwrap();
    assert_eq!(started_body["result"]["structuredContent"]["ok"], true);
    assert!(started_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_task_"));
    assert!(started_body["result"]["structuredContent"]
        .get("success")
        .is_none());

    let mut continued = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            &mcp_session_id,
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 211,
            "method": "tools/call",
            "params": {
                "name": "task_start",
                "arguments": {
                    "goal": "continue inspecting the project",
                    "mode": "read_only"
                }
            }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&continued), StatusCode::OK);
    let continued_body: Value = continued.take_json().await.unwrap();
    assert_eq!(
        continued_body["result"]["structuredContent"]["task_id"],
        started_body["result"]["structuredContent"]["task_id"]
    );
    assert_eq!(
        continued_body["result"]["structuredContent"]["data"]["continuation"],
        "continued"
    );

    let mut hidden = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 22,
            "method": "tools/call",
            "params": { "name": "runtime_status", "arguments": {} }
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&hidden), StatusCode::BAD_REQUEST);
    let hidden_body: Value = hidden.take_json().await.unwrap();
    assert_eq!(hidden_body["error"]["code"], -32602);
    assert!(hidden_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not available"));
}

#[tokio::test]
async fn http_project_connector_2026_uses_explicit_task_ids_without_transport_window_state() {
    // The runtime surface is explicit below and the request path never
    // re-reads WEBCODEX_MCP_MODEL_SURFACE, so no env lock is needed.
    let config = test_config(Some("secret"));
    let (tmp, db) = test_db();
    let project = tmp.path().join("connector-2026-project");
    crate::connector_runtime::tests::init_repo(&project);
    let user_token = "webcodex_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::CanonicalConnector));
    let service = Service::new(build_connector_test_router(config, db, runtime, &project));

    let start_params = |goal: &str| {
        mcp_2026_params(json!({
            "name": "task_start",
            "arguments": { "goal": goal, "mode": "read_only" }
        }))
    };

    let mut first = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_start", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-not-bind-2026",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 220,
            "method": "tools/call",
            "params": start_params("inspect the stateless connector path")
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&first), StatusCode::OK);
    assert!(first
        .headers
        .get(crate::client_window::MCP_SESSION_HEADER)
        .is_none());
    let first_body: Value = first.take_json().await.unwrap();
    let first_task_id = first_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .expect("2026 task_start must return task_id")
        .to_string();
    assert!(first_task_id.starts_with("wc_task_"));
    assert_eq!(first_body["result"]["resultType"], "complete");

    let mut second = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_start", true)
        .add_header(
            crate::client_window::MCP_SESSION_HEADER,
            "legacy-session-must-not-bind-2026",
            true,
        )
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 221,
            "method": "tools/call",
            "params": start_params("start independent stateless work")
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&second), StatusCode::OK);
    let second_body: Value = second.take_json().await.unwrap();
    let second_task_id = second_body["result"]["structuredContent"]["task_id"]
        .as_str()
        .expect("second 2026 task_start must return task_id");
    assert_ne!(
        second_task_id, first_task_id,
        "2026 must not derive hidden continuity from Mcp-Session-Id"
    );

    let mut resumed = TestClient::post("http://localhost/mcp")
        .bearer_auth(user_token)
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, "tools/call", true)
        .add_header(MCP_NAME_HEADER, "task_resume", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 222,
            "method": "tools/call",
            "params": mcp_2026_params(json!({
                "name": "task_resume",
                "arguments": { "task_id": first_task_id }
            }))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resumed), StatusCode::OK);
    let resumed_body: Value = resumed.take_json().await.unwrap();
    assert_eq!(
        resumed_body["result"]["structuredContent"]["task_id"],
        first_task_id
    );
    assert_eq!(
        resumed_body["result"]["structuredContent"]["data"]["continuity"]["window_rebound"],
        false
    );
}

#[tokio::test]
async fn http_mcp_tools_call_list_projects_returns_mcp_content() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "list_projects", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 3);
    assert_eq!(body["result"]["content"][0]["type"], "text");
    assert!(body["result"]["content"][0]["text"].is_string());
    assert!(body["result"]["structuredContent"].is_object());
    assert!(
        body["result"]["structuredContent"]["success"].is_boolean(),
        "structuredContent.success must be a bool"
    );
    assert!(
        body["result"]["isError"].is_boolean(),
        "isError must be a bool"
    );
    // A business failure (no projects configured) is an MCP tool error,
    // not a JSON-RPC protocol error: the envelope is still a result.
    assert!(body["result"].get("error").is_none());
    assert!(body.get("error").is_none(), "no top-level JSON-RPC error");
}

#[tokio::test]
async fn http_mcp_tools_call_unknown_tool_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "no_such_tool", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no_such_tool"));
}

#[tokio::test]
async fn http_mcp_unknown_method_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/list",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("resources/list"));
}

#[tokio::test]
async fn http_mcp_invalid_jsonrpc_returns_jsonrpc_error() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "1.0",
            "id": 6,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32600);
    assert_eq!(body["id"], 6);
}

#[tokio::test]
async fn http_mcp_without_bearer_is_unauthorized() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_mcp_with_wrong_bearer_is_unauthorized() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("wrong-token")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "initialize",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_mcp_with_correct_bearer_succeeds() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "ping",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["id"], 9);
    assert!(body["result"].is_object());
}

async fn oauth_mcp_request(
    service: &Service,
    token: &str,
    method: &str,
    params: Value,
) -> (StatusCode, Value, Option<String>) {
    let stateless_2026 = request_protocol_version(&params) == Some(MCP_STATELESS_PROTOCOL_VERSION);
    let tool_name = (method == "tools/call")
        .then(|| {
            params
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .flatten();
    let mut request = TestClient::post("http://localhost/mcp").bearer_auth(token);
    if stateless_2026 {
        request = request
            .add_header(
                MCP_PROTOCOL_VERSION_HEADER,
                MCP_STATELESS_PROTOCOL_VERSION,
                true,
            )
            .add_header(MCP_METHOD_HEADER, method, true);
        if let Some(tool_name) = tool_name.as_deref() {
            request = request.add_header(MCP_NAME_HEADER, tool_name, true);
        }
    }
    let mut resp = request
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": method,
            "params": params,
        }))
        .send(service)
        .await;
    let status = effective_status(&resp);
    let challenge = resp
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp.take_json::<Value>().await.unwrap();
    (status, body, challenge)
}

fn assert_mcp_oauth_scope_rejected(
    status: StatusCode,
    body: &Value,
    challenge: Option<&str>,
    scope: Option<&str>,
) {
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
    assert_eq!(body["error"], "insufficient_scope");
    let challenge = challenge.unwrap_or("");
    assert!(
        challenge.contains("error=\"insufficient_scope\""),
        "challenge: {}",
        challenge
    );
    if let Some(scope) = scope {
        assert!(
            body["error_description"]
                .as_str()
                .unwrap_or("")
                .contains(scope),
            "body: {:?}",
            body
        );
        assert!(challenge.contains(scope), "challenge: {}", challenge);
    }
}

#[tokio::test]
async fn pat_mcp_tools_list_requires_runtime_read_without_oauth_framing() {
    let runtime = test_runtime();
    let mut auth = mcp_export_api_auth("pat-project-read-only", "alice");
    auth.scopes = vec![crate::auth::SCOPE_PROJECT_READ.to_string()];
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(json!(41)), json!({})),
        Some(&auth),
    )
    .await;

    match outcome {
        McpOutcome::Forbidden {
            body,
            required_scope,
        } => {
            assert_eq!(required_scope, Some(crate::auth::SCOPE_RUNTIME_READ));
            assert_eq!(body["status"], StatusCode::FORBIDDEN.as_u16());
            assert_ne!(body["error"], "insufficient_scope");
            assert!(body["error"]
                .as_str()
                .unwrap_or("")
                .contains(crate::auth::SCOPE_RUNTIME_READ));
        }
        other => panic!("PAT without runtime:read must fail closed, got {other:?}"),
    }
}

#[tokio::test]
async fn oauth2_mcp_tools_list_requires_runtime_read() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) =
        oauth_mcp_request(&service, &token, "tools/list", json!({})).await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_computer_app_resources_require_runtime_read() {
    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("runtime:read", ModelSurface::FullOperatorRuntime);
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "resources/list",
        mcp_2026_ui_params(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(
        body["result"]["resources"][0]["uri"],
        MCP_COMPUTER_UI_RESOURCE_URI
    );

    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("project:read", ModelSurface::FullOperatorRuntime);
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "resources/list",
        mcp_2026_ui_params(json!({})),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_computer_snapshot_keeps_computer_read_scope() {
    let arguments = json!({
        "client_id": "missing-runner",
        "surface_id": "surface_test"
    });
    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("runtime:read", ModelSurface::FullOperatorRuntime);
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({ "name": "computer_snapshot", "arguments": arguments.clone() }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_COMPUTER_READ),
    );

    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("computer:read", ModelSurface::FullOperatorRuntime);
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({ "name": "computer_snapshot", "arguments": arguments }),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {body:?}");
}

#[tokio::test]
async fn oauth2_mcp_server_discover_requires_runtime_read_and_advertises_both_versions() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "server/discover",
        json!({
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MCP_STATELESS_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(
        body["result"]["supportedVersions"],
        json!([MCP_STATELESS_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION])
    );
    assert_eq!(body["result"]["resultType"], "complete");

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "server/discover",
        mcp_2026_params(json!({})),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_RUNTIME_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_unknown_method_keeps_legacy_fail_closed_but_modern_returns_404() {
    let (_tmp, service, token) = oauth_mcp_service("runtime:read");

    let (legacy_status, legacy_body, legacy_challenge) =
        oauth_mcp_request(&service, &token, "prompts/list", json!({})).await;
    assert_mcp_oauth_scope_rejected(
        legacy_status,
        &legacy_body,
        legacy_challenge.as_deref(),
        None,
    );

    let (modern_status, modern_body, _) =
        oauth_mcp_request(&service, &token, "prompts/list", mcp_2026_params(json!({}))).await;
    assert_eq!(
        modern_status,
        StatusCode::NOT_FOUND,
        "body: {modern_body:?}"
    );
    assert_eq!(modern_body["error"]["code"], -32601);
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_project_read_for_read_file() {
    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "read_file", "arguments": {"project": "demo", "path": "README.md"}}),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("runtime:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "read_file", "arguments": {"project": "demo", "path": "README.md"}}),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_READ),
    );
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_project_write_for_edit_tools() {
    // Edit tools require the project:write scope. Select the explicit full
    // operator surface so the scope gate (not the local_coding boundary)
    // decides this call.
    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("project:write", ModelSurface::FullOperatorRuntime);
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": "write_project_file",
            "arguments": {
                "project": "demo",
                "path": "README.md",
                "content": "new"
            }
        }),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) =
        oauth_mcp_service_with_surface("project:read", ModelSurface::FullOperatorRuntime);
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({
            "name": "write_project_file",
            "arguments": {
                "project": "demo",
                "path": "README.md",
                "content": "new"
            }
        }),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_PROJECT_WRITE),
    );
}

#[tokio::test]
async fn oauth2_mcp_tool_call_requires_job_run_for_run_shell() {
    let (_tmp, service, token) = oauth_mcp_service("job:run");
    let (status, body, _) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "run_shell", "arguments": {"project": "demo", "command": "echo hi"}}),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = oauth_mcp_service("project:read");
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "run_shell", "arguments": {"project": "demo", "command": "echo hi"}}),
    )
    .await;
    assert_mcp_oauth_scope_rejected(
        status,
        &body,
        challenge.as_deref(),
        Some(crate::auth::SCOPE_JOB_RUN),
    );
}

#[tokio::test]
async fn oauth2_mcp_unknown_tool_fails_closed() {
    let (_tmp, service, token) = oauth_mcp_service_with_surface(
        "runtime:read project:read",
        ModelSurface::FullOperatorRuntime,
    );
    let (status, body, challenge) = oauth_mcp_request(
        &service,
        &token,
        "tools/call",
        json!({"name": "no_such_tool", "arguments": {}}),
    )
    .await;
    assert_mcp_oauth_scope_rejected(status, &body, challenge.as_deref(), None);
}

#[tokio::test]
async fn api_token_mcp_behavior_unchanged() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 43,
            "method": "tools/call",
            "params": {"name": "no_such_tool", "arguments": {}}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("no_such_tool"));
}

#[tokio::test]
async fn http_mcp_notification_returns_accepted_with_empty_body() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::ACCEPTED);
    let text = resp.take_string().await.unwrap();
    assert!(text.is_empty(), "notification response body must be empty");
}

#[tokio::test]
async fn http_mcp_get_discovery_returns_metadata() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime_with_surface(ModelSurface::FullOperatorRuntime));
    let service = Service::new(build_test_router(config, db, runtime));
    let mut resp = TestClient::get("http://localhost/mcp")
        .bearer_auth("secret")
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["name"], "webcodex");
    assert!(body["version"].is_string());
    assert_eq!(body["protocol"], "mcp");
    assert_eq!(
        body["modelSurface"],
        crate::model_surface::MODEL_SURFACE_FULL_OPERATOR_RUNTIME
    );
    assert!(body["protocolVersion"].is_string());
    assert_eq!(body["endpoint"], "/mcp");
    let methods = body["methods"].as_array().unwrap();
    let method_names: Vec<String> = methods
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    assert!(method_names.contains(&"initialize".to_string()));
    assert!(method_names.contains(&"tools/list".to_string()));
    assert!(method_names.contains(&"tools/call".to_string()));
    assert!(method_names.contains(&"notifications/initialized".to_string()));
    assert_eq!(body["auth"]["type"], "bearer");
    assert_eq!(body["auth"]["required"], true);
    assert_eq!(
        body["auth"]["header"],
        "Authorization: Bearer <shared_key_or_wc_pat>"
    );
    let auth_json = body["auth"].to_string();
    assert!(
        auth_json.contains("shared_key_or_wc_pat"),
        "MCP auth metadata must advertise shared key or wc_pat bearer use: {auth_json}"
    );
    assert!(
        !auth_json.contains("wc_pat_user_api_token"),
        "MCP auth metadata must not regress to PAT-only placeholder: {auth_json}"
    );
}

// =========================================================================
// runtime_status via MCP tools/list and tools/call
// =========================================================================

#[tokio::test]
async fn mcp_tools_list_includes_runtime_status() {
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
    let names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "runtime_status"),
        "MCP tools/list must include runtime_status: {:?}",
        names
    );
}

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
        names.contains(&"create_project"),
        "MCP tools/list must include create_project: {:?}",
        names
    );
}

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
        "apply_text_edits",
        "go_test",
        "finish_coding_task",
    ] {
        assert!(names.contains(&required), "missing {required}: {names:?}");
    }
    for forbidden in [
        "start_coding_task",
        "register_project",
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
    for field in ["project", "client_id", "path", "instruction", "session_id"] {
        assert!(props.contains_key(field), "local_coding missing {field}");
    }
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

    for surface in [ModelSurface::FullOperatorRuntime, ModelSurface::LocalCoding] {
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
    let registry_names: Vec<String> = registered_tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(
        names, registry_names,
        "full operator lists the full runtime"
    );
    assert!(names.iter().any(|name| name == "read_files"));
    assert!(names.iter().any(|name| name == "search_project_texts"));

    // start_coding_task (a non-local_coding tool) dispatches on full operator.
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
    assert!(
        !matches!(&called, McpOutcome::BadRequest(value) if value["error"]["message"].as_str().unwrap().contains("local_coding")),
        "start_coding_task must not be rejected by the local_coding boundary"
    );
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
    assert_eq!(
        value["result"]["tools"].as_array().unwrap().len(),
        registered_tool_specs().len()
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
