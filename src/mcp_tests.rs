use super::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellClientCapabilities, ShellClientRegisterRequest,
};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

fn test_runtime() -> ToolRuntime {
    let model_surface = crate::model_surface::resolve_model_surface(None)
        .expect("test model surface configuration");
    test_runtime_with_surface(model_surface)
}

fn test_runtime_with_surface(model_surface: ModelSurface) -> ToolRuntime {
    ToolRuntime::new_for_tests().with_model_surface(model_surface)
}

/// Lock the shared env lock and select the full operator runtime MCP surface
/// for the duration of a test. Restores the env on drop. Used by tests whose
/// assertions target the full operator surface, which is no longer the
/// default (the default is `local_coding`).
///
/// The env var is set only AFTER the lock is acquired so a concurrently
/// running test that holds the lock cannot clear it between the set and the
/// lock; on drop the var is removed before the guard is released.
fn full_operator_mcp_env() -> impl Drop {
    struct Cleanup {
        _guard: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
        }
    }
    let guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var(
        crate::model_surface::MCP_MODEL_SURFACE_ENV,
        crate::model_surface::MCP_MODEL_SURFACE_FULL_OPERATOR_V1,
    );
    Cleanup { _guard: guard }
}

/// Variant for callers that already hold `TEST_ENV_LOCK`: only sets/removes
/// the surface env var.
fn full_operator_mcp_env_locked() -> impl Drop {
    struct SurfaceEnvCleanup;
    impl Drop for SurfaceEnvCleanup {
        fn drop(&mut self) {
            std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
        }
    }
    std::env::set_var(
        crate::model_surface::MCP_MODEL_SURFACE_ENV,
        crate::model_surface::MCP_MODEL_SURFACE_FULL_OPERATOR_V1,
    );
    SurfaceEnvCleanup
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
    let _guard = full_operator_mcp_env();
    let runtime = test_runtime();
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
        let params = if *method == "server/discover" {
            mcp_2026_params(json!({}))
        } else {
            json!({})
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
async fn mcp_tools_list_returns_same_names_as_runtime() {
    // Name parity with the runtime registry must hold under both full and
    // compact schema modes. Schema shape is covered by dedicated tests:
    // `mcp_tools_list_default_retains_output_schema` and
    // `mcp_tools_list_compact_omits_output_schema_only`.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let _full = full_operator_mcp_env_locked();
    let runtime = test_runtime();
    let runtime_names: Vec<String> = registered_tool_specs()
        .iter()
        .map(|s| s.name.clone())
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
            names, runtime_names,
            "tools/list names must match runtime registry (compact={compact})"
        );
        // Fields retained in both modes (compact only drops outputSchema).
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
        }
    }
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
}

#[test]
fn mcp_tools_list_adds_image_mode_without_changing_generic_artifact_schema() {
    let _guard = full_operator_mcp_env();
    let payload = mcp_tools_list_payload(ModelSurface::FullOperatorRuntime);
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
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
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
fn project_connector_tools_list_is_exact_canonical_surface() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let payload = mcp_tools_list_payload(ModelSurface::CanonicalConnector);
    let tools = payload["tools"].as_array().expect("tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, crate::connector_runtime::surface::CAPABILITY_NAMES);
    assert_eq!(tools.len(), 12);
    assert!(tools.iter().all(|tool| tool["inputSchema"].is_object()));
    assert!(tools.iter().all(|tool| tool["outputSchema"].is_object()));
    assert!(!names.contains(&"runtime_status"));
    assert!(!names.contains(&"list_projects"));
    assert!(!names.contains(&"start_session"));
}

#[tokio::test]
async fn mcp_tools_list_default_retains_output_schema() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let runtime = test_runtime();
    let outcome =
        handle_mcp_request(&runtime, rpc("tools/list", Some(json!(1)), json!({})), None).await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Ok");
    };
    let tools = value["result"]["tools"].as_array().expect("tools array");
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let _full = full_operator_mcp_env_locked();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let payload = mcp_tools_list_payload(ModelSurface::FullOperatorRuntime);
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

#[tokio::test]
async fn mcp_tools_list_compact_omits_output_schema_only() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let runtime = test_runtime();
    let outcome =
        handle_mcp_request(&runtime, rpc("tools/list", Some(json!(2)), json!({})), None).await;
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let McpOutcome::Ok(value) = outcome else {
        panic!("expected Ok");
    };
    let tools = value["result"]["tools"].as_array().expect("tools array");
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

#[tokio::test]
async fn mcp_tools_list_compact_is_smaller_than_full_serialized() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let full = serde_json::to_vec(&mcp_tools_list_payload(ModelSurface::FullOperatorRuntime))
        .expect("full serialize");
    std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
    let compact = serde_json::to_vec(&mcp_tools_list_payload(ModelSurface::FullOperatorRuntime))
        .expect("compact serialize");
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
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
    // tools/list outputSchema depends on WEBCODEX_MCP_COMPACT_SCHEMAS; take
    // the shared env lock so parallel compact-schema tests cannot strip it.
    // The session tools live on the full operator surface, not local_coding.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let _full = full_operator_mcp_env_locked();
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let runtime = test_runtime();
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
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
    let mcp_outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(8)), json!({})),
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
    let _full = full_operator_mcp_env();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
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

    let conn = db.conn_for_tests();
    let (endpoint, action, operation, status): (String, String, String, String) = conn
        .query_row(
            "SELECT endpoint, action_name, operation, status FROM action_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(endpoint, "/mcp");
    assert_eq!(action, "toolsCall");
    assert_eq!(operation, "list_tools");
    assert_eq!(status, "success");
    // Summary-level discipline: no tool output is persisted for MCP rows.
    let summary: String = conn
        .query_row("SELECT summary_json FROM action_events", [], |row| {
            row.get(0)
        })
        .unwrap();
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

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM action_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

fn oauth_mcp_service(scopes: &str) -> (tempfile::TempDir, Service, String) {
    let config = test_config_oauth2(Some("secret"));
    let (tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, scopes);
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    (tmp, service, token)
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
    let _full = full_operator_mcp_env();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
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
    let _full = full_operator_mcp_env();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
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
    let _full = full_operator_mcp_env();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
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
        .add_header(MCP_METHOD_HEADER, "resources/list", true)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 206,
            "method": "resources/list",
            "params": mcp_2026_params(json!({}))
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::NOT_FOUND);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn http_mcp_2026_validates_name_header_before_rejecting_unimplemented_named_method() {
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
    assert_eq!(effective_status(&unsupported), StatusCode::NOT_FOUND);
    let unsupported_body: Value = unsupported.take_json().await.unwrap();
    assert_eq!(unsupported_body["error"]["code"], -32601);
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
    // A Connector test must not observe a concurrent local_coding/full-operator
    // test's WEBCODEX_MCP_MODEL_SURFACE value; hold the env lock and clear it.
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
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
    assert_eq!(schema_body["paths"].as_object().unwrap().len(), 12);
    assert!(schema_body["paths"]
        .get("/api/connector/task/start")
        .is_some());
    assert!(schema_body["paths"].get("/api/tools/call").is_none());
    let action_checks_schema = schema_body["paths"]["/api/connector/checks/run"]["post"]
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
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
        oauth_mcp_request(&service, &token, "resources/list", json!({})).await;
    assert_mcp_oauth_scope_rejected(
        legacy_status,
        &legacy_body,
        legacy_challenge.as_deref(),
        None,
    );

    let (modern_status, modern_body, _) = oauth_mcp_request(
        &service,
        &token,
        "resources/list",
        mcp_2026_params(json!({})),
    )
    .await;
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
    let _full = full_operator_mcp_env();
    let (_tmp, service, token) = oauth_mcp_service("project:write");
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

    let (_tmp, service, token) = oauth_mcp_service("project:read");
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
    let _full = full_operator_mcp_env();
    let (_tmp, service, token) = oauth_mcp_service("runtime:read project:read");
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
    let _full = full_operator_mcp_env();
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
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
    let _full = full_operator_mcp_env();
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
    let _full = full_operator_mcp_env();
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
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
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
    let _full = full_operator_mcp_env();
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
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    let runtime = test_runtime();
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
}

#[tokio::test]
async fn local_coding_default_initialize_and_discovery_report_local_coding() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    let runtime = test_runtime();
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    let runtime = test_runtime();
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    let runtime = test_runtime();
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
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
    let listed = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(Value::from(72)), json!({})),
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var(
        crate::model_surface::MCP_MODEL_SURFACE_ENV,
        crate::model_surface::MCP_MODEL_SURFACE_LOCAL_CODING_V1,
    );
    let runtime = test_runtime();
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
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
}

#[tokio::test]
async fn explicit_full_operator_v1_reports_full_operator_surface() {
    let _full = full_operator_mcp_env();
    let runtime = test_runtime();
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
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
    let local = test_runtime();
    std::env::set_var(
        crate::model_surface::MCP_MODEL_SURFACE_ENV,
        crate::model_surface::MCP_MODEL_SURFACE_FULL_OPERATOR_V1,
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
    std::env::set_var(
        crate::model_surface::MCP_MODEL_SURFACE_ENV,
        "broken-after-startup",
    );
    let listed =
        handle_mcp_request(&full, rpc("tools/list", Some(json!(82)), json!({})), None).await;
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
    std::env::remove_var(crate::model_surface::MCP_MODEL_SURFACE_ENV);
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
