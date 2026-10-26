use super::*;
use crate::mcp::request_policy::{resolve_surface, McpSurface, SURFACE_HEADER};
use salvo::http::{HeaderMap, HeaderValue};

#[path = "cli/collaboration.rs"]
mod collaboration;
#[path = "cli/peer_clients.rs"]
mod peer_clients;

fn project(tool: &str, success: bool, output: Value) -> Value {
    let mut body = json!({"result": {
        "structuredContent": {"success":success, "output":output, "error": if success {Value::Null} else {json!("failed")}},
        "content":[{"type":"text","text":"WebCodex tool completed successfully."}],
        "isError":false
    }});
    crate::mcp::cli::project_response(Some(tool), &mut body);
    body
}

fn text(body: &Value) -> &str {
    body["result"]["content"][0]["text"].as_str().unwrap()
}

#[test]
fn cli_surface_is_request_local_and_rejects_ambiguous_headers() {
    let mut headers = HeaderMap::new();
    assert_eq!(resolve_surface(&headers).unwrap(), McpSurface::Adaptive);
    headers.insert(SURFACE_HEADER, HeaderValue::from_static("cli"));
    assert_eq!(resolve_surface(&headers).unwrap(), McpSurface::Cli);
    headers.append(SURFACE_HEADER, HeaderValue::from_static("cli"));
    assert!(resolve_surface(&headers).is_err());
    for value in ["", "CLI", "cli,adaptive", "private-value"] {
        headers.insert(SURFACE_HEADER, HeaderValue::from_str(value).unwrap());
        let error = resolve_surface(&headers).unwrap_err();
        assert!(!error.contains("private-value"));
    }
    headers.clear();
    assert_eq!(resolve_surface(&headers).unwrap(), McpSurface::Adaptive);
}

fn assert_collaboration_only_schema(schema: &Value) {
    let envelope = &schema["properties"]["_wc"];
    assert_eq!(envelope["additionalProperties"], false);
    let fields = envelope["properties"]
        .as_object()
        .expect("collaboration schema");
    assert!(fields.contains_key("ack"), "{envelope}");
    assert!(
        fields
            .keys()
            .all(|field| matches!(field.as_str(), "ack" | "reply")),
        "{envelope}"
    );
}

pub(super) fn business_schema_with_collaboration_checked(schema: &Value) -> Value {
    assert_collaboration_only_schema(schema);
    assert_eq!(
        schema["properties"]["_wc"]["properties"]["ack"]["maxItems"],
        8
    );
    let mut business = schema.clone();
    business["properties"]
        .as_object_mut()
        .unwrap()
        .remove("_wc");
    business
}

#[test]
fn cli_discovery_uses_canonical_inputs_and_only_collaboration_sidecars() {
    let listed = crate::mcp::cli::tools_list(None, false);
    let tools = listed["tools"].as_array().unwrap();
    for name in crate::mcp::cli::DIRECT_TOOLS {
        let tool = tools.iter().find(|tool| tool["name"] == *name).expect(name);
        assert!(crate::tool_runtime::tool_definition::is_model_visible_tool_name(name));
        assert!(tool.get("outputSchema").is_none());
        assert!(tool.get("_meta").is_none());
        assert_collaboration_only_schema(&tool["inputSchema"]);
        assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{name}");
    }
    assert!(tools
        .iter()
        .all(|t| !crate::mcp::cli::is_app_tool(t["name"].as_str().unwrap())));
    let cli_bytes = serde_json::to_vec(&listed).unwrap().len();
    let default = mcp_tools_list_payload_with_features_for_auth(true, true, true, None);
    let default_bytes = serde_json::to_vec(&default).unwrap().len();
    eprintln!(
        "CLI tools: {}, {} bytes; default App surface: {} tools, {} bytes",
        tools.len(),
        cli_bytes,
        default["tools"].as_array().unwrap().len(),
        default_bytes
    );
    let ordinary = mcp_tools_list_payload_with_features_for_auth(true, false, false, None);
    eprintln!(
        "Default without Apps: {} tools, {} bytes",
        ordinary["tools"].as_array().unwrap().len(),
        serde_json::to_vec(&ordinary).unwrap().len()
    );
    let mut sizes: Vec<_> = tools
        .iter()
        .map(|tool| {
            (
                serde_json::to_vec(tool).unwrap().len(),
                tool["name"].as_str().unwrap(),
            )
        })
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    eprintln!("Largest CLI descriptors: {:?}", &sizes[..5]);
    assert!(cli_bytes < default_bytes);
}

#[tokio::test]
async fn cli_bootstrap_discovery_and_manifest_share_canonical_core_and_defaults() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    let (_, listed) = post_cli(&service, "tools/list", json!({}), true).await;
    let descriptor = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "work_on_project")
        .unwrap();
    let (_, manifest) = post_cli(
        &service,
        "tools/call",
        json!({"name":"read_tool_manifest","arguments":{"tool_name":"work_on_project"}}),
        true,
    )
    .await;
    let rendered = text(&manifest);
    let description = rendered
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .expect("exact Manifest description");
    assert_eq!(descriptor["description"], description);
    let canonical = webcodex_tool_contracts::lookup_tool_definition("work_on_project")
        .unwrap()
        .model_spec
        .unwrap()
        .description;
    assert!(description.starts_with(canonical));
    let schema: Value = serde_json::from_str(
        rendered
            .lines()
            .find_map(|line| line.strip_prefix("input_schema: "))
            .expect("exact Manifest schema"),
    )
    .unwrap();
    for schema in [&descriptor["inputSchema"], &schema] {
        assert_eq!(
            schema["properties"]["include_extension_catalog"]["default"],
            false
        );
        assert_collaboration_only_schema(schema);
    }
}

#[test]
fn cli_reads_keep_raw_source_revision_partial_failures_and_fenced_continuation() {
    let source = "1 | let text = \"你好\";\n2 | // \\n is literal";
    let output = json!({
        "project":"agent:r:p", "items":[
            {"index":0,"path":"a.rs","success":true,"output":{"text":source,"format":"numbered","start_line":1,"end_line":2,"read_revision":7,"has_more":true,"next_start_line":3},"error":null},
            {"index":1,"path":"missing.rs","success":false,"output":{"error_kind":"not_found"},"error":"File missing"}
        ],
        "peer_awareness":{"new_peers":["unused-peer"]},
        "output_truncated":true,
        "suggested_call":{"tool":"read_files","arguments":{"project":"agent:r:p","items":[{"path":"a.rs","start_line":3,"expected_read_revision":7},{"path":"unreturned.rs"}]}}
    });
    let body = project("read_files", true, output);
    assert!(body["result"].get("structuredContent").is_none());
    let rendered = text(&body);
    assert!(rendered.contains(source));
    assert!(rendered.contains("==> a.rs:1-2 (read_revision 7)"));
    for expected in [
        "File missing",
        "not_found",
        "output_truncated: true",
        "\"expected_read_revision\":7",
        "\"start_line\":3",
        "unreturned.rs",
    ] {
        assert!(rendered.contains(expected), "{rendered}");
    }
    assert!(rendered.contains("unused-peer"));
    assert_eq!(body["result"]["isError"], false);
}

#[test]
fn cli_collaboration_preserves_execution_facts_on_every_outcome() {
    for tool in [
        "run_job",
        "run_process",
        "run_shell",
        "run_script",
        "observe_jobs",
    ] {
        for outcome in [
            "completed",
            "pending",
            "failed",
            "timeout",
            "outcome_unknown",
        ] {
            let success = matches!(outcome, "completed" | "pending");
            let output = json!({
                "execution_state": outcome,
                "job_id": "wc_job_0123456789abcdef",
                "stdout_tail": "execution-output-marker",
                "error_kind": if success { Value::Null } else { json!(outcome) },
                "continuation": {"tool": "observe_jobs", "arguments": {"items": [{"job_id":"wc_job_0123456789abcdef"}]}},
                "operator_messages": {"messages":[{"message_id":"wc_msg_operator00000000","message":"operator-message-marker","requires_ack":true}]},
                "peer_messages": {"messages":[{"message_id":"wc_msg_peer000000000000","message":"peer-message-marker","requires_ack":true}]},
                "session_attention": {"messages":[{"message_id":"wc_msg_session000000000","message":"session-message-marker","requires_ack":true}]},
                "peer_awareness": {"new_peers":[{"peer_id":"wc_peer_0123456789abcdef0123456789abcdef","source":"codex-thread"}],"semantics":"recent_same_project_activity_not_liveness"}
            });
            let body = project(tool, success, output);
            let rendered = text(&body);
            for marker in [
                "execution-output-marker",
                "wc_job_0123456789abcdef",
                "observe_jobs",
                "operator-message-marker",
                "peer-message-marker",
                "session-message-marker",
                "wc_peer_0123456789abcdef0123456789abcdef",
                "recent_same_project_activity_not_liveness",
            ] {
                assert!(
                    rendered.contains(marker),
                    "{tool}/{outcome}: missing {marker}: {rendered}"
                );
            }
            assert!(rendered.contains(outcome), "{tool}/{outcome}: {rendered}");
            assert_eq!(body["result"]["isError"], !success);
            assert!(body["result"].get("structuredContent").is_none());
        }
    }
}

#[test]
fn cli_complete_file_read_header_omits_absent_range() {
    // Canonical complete-file reads drop start_line/end_line
    // (sparsify_complete_file_read_output); the header must not invent a range.
    let body = project(
        "read_files",
        true,
        json!({"items":[{"index":0,"path":"a.rs","success":true,"error":null,
            "output":{"text":"fn main() {}\n","read_revision":7,"total_lines":1}}]}),
    );
    let rendered = text(&body);
    assert!(
        rendered.starts_with("==> a.rs (read_revision 7)\nfn main() {}\ntotal_lines: 1\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("null"), "{rendered}");
}

#[test]
fn cli_search_keeps_context_counts_failure_and_truncation_without_read_hints() {
    let body = project(
        "search_project_texts",
        true,
        json!({"items":[
            {"index":0,"success":true,"output":{"matches":[{"path":"a.rs","line":3,"preview":"fn \"source\"()","context_before":["before"],"read_hint":{"path":"duplicate"}}],"truncated":true,"total_matches":null}},
            {"index":1,"success":false,"error":"search timeout","output":{"error_kind":"timeout"}}
        ]}),
    );
    let rendered = text(&body);
    for expected in [
        "a.rs:3:fn \"source\"()",
        "before",
        "truncated: true",
        "total_matches: null",
        "search timeout",
    ] {
        assert!(rendered.contains(expected), "{rendered}");
    }
    assert!(!rendered.contains("read_hint"));
    assert!(!rendered.contains("duplicate"));
}

#[test]
fn cli_preserves_unknown_effects_and_native_blocks_and_provider_results() {
    let output = json!({"state_changed":true,"failure_kind":"outcome_unknown","job_id":"wc_job_exact","continuation":{"tool":"observe_jobs","arguments":{"items":[{"job_id":"wc_job_exact"}]}},"stdout_tail":"line one\nline two"});
    let body = project("run_process", false, output.clone());
    assert_eq!(body["result"]["isError"], true);
    for expected in [
        "ERROR: failed",
        "outcome_unknown",
        "state_changed: true",
        "wc_job_exact",
        "observe_jobs",
        "line one\nline two",
    ] {
        assert!(text(&body).contains(expected));
    }
    let mut native = json!({"result":{"structuredContent":{"success":true,"output":{"content_delivery":"mcp_image"}},"content":[{"type":"image","data":"fixture","mimeType":"image/png"},{"type":"resource","resource":{"uri":"fixture://text","text":"native resource"}},{"type":"resource_link","uri":"fixture://download","name":"download"}]}});
    crate::mcp::cli::project_response(Some("observe_computer"), &mut native);
    assert_eq!(native["result"]["content"][1]["type"], "image");
    assert_eq!(native["result"]["content"][2]["type"], "resource");
    assert_eq!(native["result"]["content"][3]["type"], "resource_link");
    for name in ["mcp_tool", "plugin_tool", "ssh_resource"] {
        let mut provider = json!({"result":{"structuredContent":{"success":true,"output":output},"content":[{"type":"text","text":"provider-owned"}]}});
        let original = provider.clone();
        crate::mcp::cli::project_response(Some(name), &mut provider);
        assert_eq!(provider, original);
    }
}

#[test]
fn cli_text_overhead_is_bounded_without_losing_unknown_effects() {
    let output = json!({
        "failure_kind":"outcome_unknown", "job_id":"wc_job_exact",
        "unknown_nested_evidence": (0..200).map(|_| json!([[[["证据"]]]])).collect::<Vec<_>>()
    });
    let body = project("run_process", false, output.clone());
    let canonical = json!({"success":false,"output":output,"error":"failed"});
    assert!(text(&body).len() <= serde_json::to_vec(&canonical).unwrap().len() + 1024);
    // Dense nested output uses the lossless compact fallback, not truncation.
    assert_eq!(
        serde_json::from_str::<Value>(text(&body)).unwrap(),
        canonical
    );
    assert_eq!(body["result"]["isError"], true);
    assert!(body["result"].get("structuredContent").is_none());
}

async fn post_cli(
    service: &Service,
    method: &str,
    params: Value,
    cli: bool,
) -> (StatusCode, Value) {
    let mut request = TestClient::post("http://localhost/mcp").bearer_auth("secret");
    if cli {
        request = request.add_header(SURFACE_HEADER, "cli", true);
    }
    if request_protocol_version(&params) == Some(MCP_STATELESS_PROTOCOL_VERSION) {
        request = request
            .add_header("Mcp-Protocol-Version", MCP_STATELESS_PROTOCOL_VERSION, true)
            .add_header("Mcp-Method", method, true);
        if let Some(name) = params["name"].as_str() {
            request = request.add_header("Mcp-Name", name, true);
        }
    }
    let mut response = request
        .json(&json!({"jsonrpc":"2.0","id":9,"method":method,"params":params}))
        .send(service)
        .await;
    (
        effective_status(&response),
        response.take_json().await.unwrap(),
    )
}

#[tokio::test]
async fn cli_http_discovery_dispatch_and_default_are_isolated() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    let (_, init) = post_cli(
        &service,
        "initialize",
        json!({"protocolVersion":"2025-11-25"}),
        true,
    )
    .await;
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("owning Runner"));
    let (_, normal_init) = post_cli(&service, "initialize", json!({}), false).await;
    assert!(normal_init["result"].get("instructions").is_none());
    let (_, listed) = post_cli(&service, "tools/list", json!({}), true).await;
    assert!(listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "find_definition"));
    let params = json!({"name":"list_project_files","arguments":{"project":"agent:missing:p"}});
    let (status, result) = post_cli(&service, "tools/call", params.clone(), true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["result"]["isError"], true);
    assert!(result["result"].get("structuredContent").is_none());
    let (status, _) = post_cli(&service, "tools/call", params, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, normal) = post_cli(
        &service,
        "tools/call",
        json!({"name":"read_tool_manifest","arguments":{"tool_name":"read_files"}}),
        false,
    )
    .await;
    assert!(normal["result"].get("structuredContent").is_some());
}

#[tokio::test]
async fn cli_stateless_ignores_app_capabilities_and_host_failure_compatibility() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    let (_, discovery) = post_cli(
        &service,
        "server/discover",
        mcp_2026_ui_params(json!({})),
        true,
    )
    .await;
    assert!(
        discovery["result"]["instructions"].is_string(),
        "{discovery}"
    );
    let (_, listed) = post_cli(&service, "tools/list", mcp_2026_ui_params(json!({})), true).await;
    assert_eq!(listed["result"]["resultType"], "complete");
    assert!(listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| {
            assert_collaboration_only_schema(&tool["inputSchema"]);
            !crate::mcp::cli::is_app_tool(tool["name"].as_str().unwrap())
        }));
    let mut params = mcp_2026_ui_params(
        json!({"name":"list_project_files","arguments":{"project":"agent:missing:p"}}),
    );
    params["_meta"]["io.modelcontextprotocol/clientInfo"] =
        json!({"name":"openai-mcp","version":"test"});
    let (status, failed) = post_cli(&service, "tools/call", params, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(failed["result"]["isError"], true);
    assert!(failed["result"].get("structuredContent").is_none());
    for params in [
        json!({"name":"present_work_result","arguments":{}}),
        adaptive_runtime_gateway_params("present_work_result", json!({})),
        json!({"name":"read_tool_manifest","arguments":{"_wc":{"context":["webcodex.workflow"]}}}),
        adaptive_runtime_gateway_params(
            "read_tool_manifest",
            json!({"_wc":{"context":["webcodex.workflow"]}}),
        ),
    ] {
        let (status, body) =
            post_cli(&service, "tools/call", mcp_2026_ui_params(params), true).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn cli_rejects_apps_hidden_tools_and_sidecars_through_both_routes() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for name in [
        "present_work_result",
        "present_agent_continuation",
        "open_webcodex_workbench",
        "get_work_result_state",
        "sync_goal_plan",
    ] {
        for params in [
            json!({"name":name,"arguments":{}}),
            adaptive_runtime_gateway_params(name, json!({})),
        ] {
            let (_, body) = post_cli(&service, "tools/call", params, true).await;
            assert!(
                body.get("error").is_some() || body["result"]["isError"] == true,
                "{name}: {body}"
            );
        }
    }
    let (status, _) = post_cli(&service, "tools/call", json!({"name":"read_tool_manifest","arguments":{"tool_name":"read_files","_wc":{"context":["webcodex.workflow"]}}}), true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn cli_default_project_is_explicit_bounded_and_preserves_other_selectors() {
    use crate::mcp::request_policy::{resolve_default_project, PROJECT_HEADER};
    let mut headers = HeaderMap::new();
    for value in ["agent:mini:webcodex", "~p17"] {
        headers.insert(PROJECT_HEADER, HeaderValue::from_str(value).unwrap());
        assert_eq!(
            resolve_default_project(&headers, McpSurface::Cli)
                .unwrap()
                .as_deref(),
            Some(value)
        );
        assert!(resolve_default_project(&headers, McpSurface::Adaptive).is_err());
    }
    for value in [
        "",
        "webcodex",
        "agent::p",
        "agent:r:",
        "agent:r:p:q",
        // A bare `p17` is a short name to the resolver, not a server-issued ref.
        "p17",
        "~p",
        "~p0",
        "~p01",
        "~p-1",
        "~p17,other",
        "private secret",
    ] {
        headers.insert(PROJECT_HEADER, HeaderValue::from_str(value).unwrap());
        let error = resolve_default_project(&headers, McpSurface::Cli).unwrap_err();
        assert!(!error.contains("private secret"));
    }
    headers.insert(
        PROJECT_HEADER,
        HeaderValue::from_str(&format!("agent:r:{}", "p".repeat(512))).unwrap(),
    );
    assert!(resolve_default_project(&headers, McpSurface::Cli).is_err());
    let mut args = json!({"items":[{"path":"a.rs"}]});
    crate::mcp::cli::apply_default_project("read_files", &mut args, Some("~p17"));
    assert_eq!(args["project"], "~p17");
    for mut args in [json!({"project":"~p99"}), json!({"project":null})] {
        let before = args.clone();
        crate::mcp::cli::apply_default_project("read_files", &mut args, Some("~p17"));
        assert_eq!(args, before);
    }
    for mut args in [
        json!({"session_id":"wc_sess_exact"}),
        json!({"client_id":"r","path":"/p"}),
        json!({"path":"/invalid"}),
    ] {
        let before = args.clone();
        crate::mcp::cli::apply_default_project("work_on_project", &mut args, Some("~p17"));
        assert_eq!(args, before);
    }
    for name in ["list_projects", "list_jobs", "unknown_tool"] {
        let mut args = json!({});
        crate::mcp::cli::apply_default_project(name, &mut args, Some("~p17"));
        assert_eq!(args, json!({}), "{name}");
    }
}

#[tokio::test]
async fn cli_default_project_discovery_and_direct_gateway_calls_agree() {
    use crate::mcp::request_policy::PROJECT_HEADER;
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for (method, params) in [
        ("tools/list", json!({})),
        (
            "tools/call",
            json!({"name":"list_project_files","arguments":{}}),
        ),
        (
            "tools/call",
            adaptive_runtime_gateway_params("list_project_files", json!({})),
        ),
        (
            "tools/call",
            json!({"name":"read_tool_manifest","arguments":{"tool_name":"list_project_files"}}),
        ),
    ] {
        let mut response = TestClient::post("http://localhost/mcp")
            .bearer_auth("secret")
            .add_header(SURFACE_HEADER, "cli", true)
            .add_header(PROJECT_HEADER, "agent:missing:p", true)
            .json(&json!({"jsonrpc":"2.0","id":10,"method":method,"params":params}))
            .send(&service)
            .await;
        let status = effective_status(&response);
        let body: Value = response.take_json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        if method == "tools/list" {
            let tool = body["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == "list_project_files")
                .unwrap();
            assert!(!tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("project")));
        } else if params["name"] == "read_tool_manifest" {
            assert!(text(&body).contains(r#""mode":"direct""#));
        } else {
            assert_eq!(body["result"]["isError"], true);
            assert!(!text(&body).contains("missing field"), "{}", text(&body));
        }
    }
}

#[tokio::test]
async fn cli_scope_and_protocol_capability_checks_still_apply() {
    let runtime = test_runtime();
    let mut auth = mcp_export_api_auth("cli-scope", "alice");
    auth.kind = crate::auth::AuthKind::OAuth2Token;
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
    let listed = crate::mcp::cli::tools_list(Some(&auth), false);
    assert!(!listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "read_files"));
    for stateless in [false, true] {
        for gateway in [false, true] {
            let arguments = json!({"project":"agent:r:p","items":[{"path":"a.rs"}]});
            let params = if gateway {
                adaptive_runtime_gateway_params("read_files", arguments)
            } else {
                json!({"name":"read_files","arguments":arguments})
            };
            let outcome = super::super::handle_mcp_request_for_surface(
                &runtime,
                rpc("tools/call", Some(json!(1)), params),
                Some(&auth),
                if stateless {
                    McpProtocolEra::Stateless2026
                } else {
                    McpProtocolEra::Legacy
                },
                HostFileImportTrust::Untrusted,
                None,
                None,
                None,
                true,
                true,
                None,
                McpSurface::Cli,
                None,
            )
            .await;
            assert!(
                matches!(outcome, McpOutcome::Forbidden { .. }),
                "{outcome:?}"
            );
        }
    }
}

#[test]
fn cli_manifest_routes_match_the_advertised_surface() {
    for (name, mode) in [
        ("find_definition", "direct"),
        ("list_project_tracked_files", "direct"),
        ("get_runtime_status", "gateway"),
        ("present_work_result", "unavailable"),
    ] {
        let mut result = ToolResult::ok(
            json!({"name":name,"route":{"mode":"wrong"},"input_schema":webcodex_tool_contracts::input_schema_for_tool(name)}),
        );
        crate::mcp::cli::project_manifest(&mut result, true);
        crate::mcp::tool_exposure::ToolExposure::default().project_manifest(
            &mut result,
            McpSurface::Cli,
            true,
        );
        assert_eq!(result.output["route"]["mode"], mode);
    }
}

#[tokio::test]
async fn cli_manifest_text_is_bounded_and_exact_schema_remains_complete() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    let (_, catalog) = post_cli(
        &service,
        "tools/call",
        json!({"name":"read_tool_manifest","arguments":{}}),
        true,
    )
    .await;
    let rendered = text(&catalog);
    assert!(
        rendered.contains("[gateway via call_runtime_tool]"),
        "{rendered}"
    );
    assert!(rendered.contains("truncated: true"), "{rendered}");
    assert!(!rendered.contains("present_work_result"));
    assert!(!rendered.lines().any(|line| line.trim() == "[0]"));
    assert!(rendered.len() < 12000, "{}", rendered.len());
    let (_, exact) = post_cli(
        &service,
        "tools/call",
        json!({"name":"read_tool_manifest","arguments":{"tool_name":"search_file_context"}}),
        true,
    )
    .await;
    let rendered = text(&exact);
    let line = rendered
        .lines()
        .find_map(|line| line.strip_prefix("input_schema: "))
        .unwrap();
    let schema: Value = serde_json::from_str(line).unwrap();
    assert!(schema["properties"].get("query").is_none());
    assert!(schema["properties"]["queries"]["items"]["properties"]["pattern"].is_object());
    assert!(rendered.contains("authority:"));
    assert!(rendered.contains("route:"));
    assert!(rendered.contains("preserving per-query batch failures"));
}

#[tokio::test]
async fn published_edit_selector_schemas_reject_runtime_conflicts() {
    use webcodex_tool_contracts::test_support::validate_schema_instance;

    let choices = [None, Some(Value::Null), Some(json!(1))];
    let mut cases = Vec::new();
    for occurrence in &choices {
        for count in &choices {
            let mut edit = json!({"kind":"replace_exact","old_text":"old","new_text":"new"});
            for (key, value) in [("occurrence", occurrence), ("expected_match_count", count)] {
                if let Some(value) = value {
                    edit[key] = value.clone();
                }
            }
            let conflict = occurrence.as_ref().is_some_and(Value::is_number)
                && count.as_ref().is_some_and(Value::is_number);
            let has_null = occurrence.as_ref().is_some_and(Value::is_null)
                || count.as_ref().is_some_and(Value::is_null);
            cases.push((
                json!({"project":"demo","changes":[{
                    "kind":"edit","path":"a.rs","expected_read_revision":123,"edits":[edit]
                }]}),
                !conflict && !has_null,
            ));
        }
    }
    let mut mixed = json!({"project":"demo","changes":[{
        "kind":"edit","path":"a.rs","expected_read_revision":123,"edits":[
            {"kind":"insert_before","anchor_text":"anchor","new_text":"prefix","occurrence":1},
            {"kind":"replace_exact","old_text":"old","new_text":"new","expected_match_count":2}
        ]
    }]});
    cases.push((mixed.clone(), true));
    mixed["changes"][0]["edits"][1]["occurrence"] = json!(1);
    cases.push((mixed, false));

    for compact in [false, true] {
        let (_tmp, db) = test_db();
        let service = Service::new(build_test_router(
            test_config(Some("secret")),
            db,
            Arc::new(test_runtime_with_mcp_settings(compact, false)),
        ));
        for cli in [false, true] {
            for stateless in [false, true] {
                let params = |value| {
                    if stateless {
                        mcp_2026_params(value)
                    } else {
                        value
                    }
                };
                let (status, listed) =
                    post_cli(&service, "tools/list", params(json!({})), cli).await;
                assert_eq!(status, StatusCode::OK, "{listed}");
                let descriptor = listed["result"]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|tool| tool["name"] == "edit_project_files")
                    .expect("published editor");
                let (status, manifest) = post_cli(
                    &service, "tools/call",
                    params(json!({"name":"read_tool_manifest","arguments":{"tool_name":"edit_project_files"}})),
                    cli,
                ).await;
                assert_eq!(status, StatusCode::OK, "{manifest}");
                let exact = if cli {
                    let line = text(&manifest)
                        .lines()
                        .find_map(|line| line.strip_prefix("input_schema: "))
                        .expect("exact CLI manifest schema");
                    serde_json::from_str::<Value>(line).unwrap()
                } else {
                    manifest["result"]["structuredContent"]["output"]["input_schema"].clone()
                };
                for (source, schema) in [
                    ("tools/list", &descriptor["inputSchema"]),
                    ("manifest", &exact),
                ] {
                    assert!(schema.is_object(), "{source}: {manifest}");
                    for (request, expected) in &cases {
                        assert_eq!(
                            validate_schema_instance(request, schema).is_ok(),
                            *expected,
                            "compact={compact} cli={cli} stateless={stateless} {source}: {request}"
                        );
                    }
                    eprintln!("edit_selector_schema compact={compact} cli={cli} stateless={stateless} source={source} bytes={}",
                        serde_json::to_vec(schema).unwrap().len());
                }
            }
        }
    }
}

#[tokio::test]
async fn cli_manifest_omitted_and_null_options_keep_the_catalog_bound() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for params in [
        json!({"name":"read_tool_manifest"}),
        json!({"name":"read_tool_manifest","arguments":{"tool_name":null,"limit":null}}),
    ] {
        let (_, body) = post_cli(&service, "tools/call", params, true).await;
        assert_eq!(body["result"]["isError"], false, "{body}");
        assert!(
            text(&body).contains("returned_count: 20"),
            "{}",
            text(&body)
        );
        assert!(text(&body).contains("truncated: true"));
    }
}
#[path = "cli/text_projection.rs"]
mod text_projection;
