use super::*;
use crate::mcp::request_policy::{resolve, BUDGET_HEADER, PROFILE_HEADER};
use crate::mcp_host::{McpHostConfig, McpHostProfile, McpHostRuntimePolicy};
use salvo::http::{HeaderMap, HeaderValue};
#[path = "request_policy/diagnostics.rs"]
mod diagnostics;

fn deployment() -> McpHostRuntimePolicy {
    McpHostConfig {
        profile: McpHostProfile::HostCodeMode,
        host_budget_secs: None,
    }
    .runtime_policy()
}

#[test]
fn request_policy_is_explicit_bounded_and_never_sticky() {
    let base = deployment();
    let mut headers = HeaderMap::new();
    assert_eq!(resolve(&headers, base).unwrap().effective, base);
    headers.insert(PROFILE_HEADER, HeaderValue::from_static("direct"));
    let direct = resolve(&headers, base).unwrap().effective;
    assert_eq!(direct.profile, McpHostProfile::Direct);
    assert_eq!(direct.initial_job_handoff_secs, 10);
    assert_eq!(direct.max_sync_wait_secs, 50);
    assert_eq!(direct.continuation_wait_secs, 50);
    assert_eq!(direct.host_budget_secs, 55);
    headers.insert(BUDGET_HEADER, HeaderValue::from_static("999999"));
    assert_eq!(resolve(&headers, base).unwrap().effective, direct);
    headers.insert(BUDGET_HEADER, HeaderValue::from_static("9"));
    let bounded = resolve(&headers, base).unwrap().effective;
    assert_eq!(bounded.host_budget_secs, 9);
    assert_eq!(bounded.max_sync_wait_secs, 4);
    headers.remove(PROFILE_HEADER);
    assert_eq!(
        resolve(&headers, base).unwrap().effective.profile,
        McpHostProfile::HostCodeMode
    );
    headers.clear();
    assert_eq!(resolve(&headers, base).unwrap().effective, base);
    assert_eq!(base, deployment());
    let mut selection_headers = HeaderMap::new();
    selection_headers.insert(PROFILE_HEADER, HeaderValue::from_static("host_code_mode"));
    selection_headers.insert(BUDGET_HEADER, HeaderValue::from_static("999999"));
    let selection = resolve(&selection_headers, base).unwrap();
    assert_eq!(selection.effective, base);
    assert_eq!(
        selection.profile_source,
        crate::mcp_host::McpHostPolicySource::RequestHeader
    );
    assert_eq!(
        selection.budget_source,
        crate::mcp_host::McpHostPolicySource::RequestHeader
    );
    assert_eq!(selection.requested_budget_secs, Some(999999));
    assert_eq!(selection.deployment_budget_secs, base.host_budget_secs);
}

#[test]
fn request_policy_rejects_ambiguous_or_malformed_headers_without_echoing_values() {
    for (name, value) in [
        (PROFILE_HEADER, "PRIVATE_BAD_PROFILE"),
        (PROFILE_HEADER, ""),
        (PROFILE_HEADER, "direct,host_code_mode"),
        (BUDGET_HEADER, "0"),
        (BUDGET_HEADER, "-1"),
        (BUDGET_HEADER, "1.5"),
        (BUDGET_HEADER, "999999999999999999999999999999"),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(name, HeaderValue::from_str(value).unwrap());
        let error = resolve(&headers, deployment()).unwrap_err();
        assert!(!error.contains("PRIVATE_"));
    }
    for name in [PROFILE_HEADER, BUDGET_HEADER] {
        let mut headers = HeaderMap::new();
        headers.append(name, HeaderValue::from_static("direct"));
        headers.append(name, HeaderValue::from_static("direct"));
        assert!(resolve(&headers, deployment()).is_err());
        headers.clear();
        headers.insert(name, HeaderValue::from_bytes(&[0xff]).unwrap());
        assert!(resolve(&headers, deployment()).is_err());
    }
}

async fn post(
    service: &Service,
    method: &str,
    params: Value,
    profile: Option<&str>,
    budget: Option<&str>,
) -> (StatusCode, Value) {
    let name = params.get("name").and_then(Value::as_str);
    let mut request = TestClient::post("http://localhost/mcp")
        .bearer_auth("secret")
        .add_header(
            MCP_PROTOCOL_VERSION_HEADER,
            MCP_STATELESS_PROTOCOL_VERSION,
            true,
        )
        .add_header(MCP_METHOD_HEADER, method, true);
    if let Some(name) = name {
        request = request.add_header(MCP_NAME_HEADER, name, true);
    }
    if let Some(profile) = profile {
        request = request.add_header(PROFILE_HEADER, profile, true);
    }
    if let Some(budget) = budget {
        request = request.add_header(BUDGET_HEADER, budget, true);
    }
    let params = if request_protocol_version(&params) == Some(MCP_STATELESS_PROTOCOL_VERSION) {
        params
    } else {
        mcp_2026_params(params)
    };
    let mut response = request
        .json(&json!({"jsonrpc":"2.0", "id":810,
        "method":method, "params":params}))
        .send(service)
        .await;
    (
        effective_status(&response),
        response.take_json().await.unwrap(),
    )
}

fn context_call(gateway: bool) -> Value {
    let mut params = if gateway {
        adaptive_runtime_gateway_params("tool_manifest", json!({"tool_name":"cargo_test"}))
    } else {
        json!({"name":"tool_manifest", "arguments":{"tool_name":"cargo_test"}})
    };
    params["arguments"]["_wc"] = json!({"context":["webcodex.workflow"]});
    params
}

fn profile_from_result(body: &Value) -> &str {
    assert_eq!(
        body["result"]["structuredContent"]["success"], true,
        "{body}"
    );
    body["result"]["structuredContent"]["output"]["context_projection"]["materials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "webcodex.workflow")
        .unwrap()["projection"]["tool_strategy"]["profile"]
        .as_str()
        .unwrap()
}

#[tokio::test]
async fn request_policy_isolated_across_concurrent_http_direct_and_gateway_calls() {
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime().with_mcp_host_policy(deployment()));
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        runtime.clone(),
    ));
    let (direct, host, omitted) = tokio::join!(
        post(
            &service,
            "tools/call",
            context_call(false),
            Some("direct"),
            Some("9")
        ),
        post(
            &service,
            "tools/call",
            context_call(true),
            Some("host_code_mode"),
            None
        ),
        post(&service, "tools/call", context_call(false), None, None),
    );
    for (result, expected) in [
        (direct, "direct"),
        (host, "host_code_mode"),
        (omitted, "host_code_mode"),
    ] {
        assert_eq!(result.0, StatusCode::OK, "{}", result.1);
        assert_eq!(profile_from_result(&result.1), expected);
    }
    let later = post(&service, "tools/call", context_call(true), None, None).await;
    assert_eq!(profile_from_result(&later.1), "host_code_mode");
    assert_eq!(runtime.mcp_host_policy, deployment());
    let compact_status = post(
        &service,
        "tools/call",
        json!({"name":"runtime_status", "arguments":{"compact":true}}),
        Some("direct"),
        Some("9"),
    )
    .await;
    assert_eq!(compact_status.0, StatusCode::OK, "{}", compact_status.1);
    assert_eq!(
        compact_status.1["result"]["structuredContent"]["output"]["mcp_host"]["profile"],
        "host_code_mode"
    );
    let status = post(
        &service,
        "tools/call",
        json!({"name":"runtime_status", "arguments":{"compact":false}}),
        Some("direct"),
        Some("9"),
    )
    .await;
    assert_eq!(status.0, StatusCode::OK, "{}", status.1);
    let deployment_status =
        &status.1["result"]["structuredContent"]["output"]["effective_config"]["mcp_host"];
    assert_eq!(deployment_status["profile"], "host_code_mode");
    assert_eq!(deployment_status["host_budget_secs"], 55);
    assert_eq!(deployment_status["initial_job_handoff_secs"], 5);
    assert_eq!(deployment_status["max_sync_wait_secs"], 5);
    assert_eq!(deployment_status["continuation_wait_secs"], 5);
    let bad = post(
        &service,
        "tools/call",
        context_call(false),
        Some("PRIVATE_BAD_PROFILE"),
        None,
    )
    .await;
    assert_eq!(bad.0, StatusCode::BAD_REQUEST);
    assert_eq!(bad.1["error"]["code"], -32600);
    assert!(bad.1.get("result").is_none());
    assert!(!bad.1.to_string().contains("PRIVATE_"));
}

#[tokio::test]
async fn request_policy_pending_wait_and_terminal_observe_keep_one_execution_without_apps() {
    use crate::runner_protocol::{RunnerJobUpdateRequest, ShellCommandExecutionState};
    let (_tmp, db) = test_db();
    let registry = Arc::new(crate::runner_http::RunnerRegistry::default());
    registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                client_id: "request-policy".into(),
                runner_instance_id: "inst-policy".into(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                capabilities: RunnerCapabilities {
                    shell: true,
                    structured_process_argv: true,
                    ..Default::default()
                },
                computer_session_availability: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                policy: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "request-policy",
        "inst-policy",
        vec![RunnerProjectSummary {
            id: "demo".into(),
            name: None,
            path: "/tmp/request-policy".into(),
            allow_patch: true,
            kind: Some("repo".into()),
            registration_source: None,
            description: None,
            hooks: vec![],
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: None,
        }],
    )
    .await;
    let info = crate::tool_runtime::RuntimeInfo {
        mcp_apps_enabled: false,
        ..Default::default()
    };
    let runtime = Arc::new(
        ToolRuntime::new(registry.clone(), Arc::new(info)).with_mcp_host_policy(deployment()),
    );
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        runtime.clone(),
    ));
    let started = post(&service, "tools/call", json!({"name":"run_process", "arguments":{
        "project":"agent:request-policy:demo", "executable":"probe", "args":["hello"], "timeout_secs":60
    }}), Some("host_code_mode"), Some("6")).await;
    assert_eq!(started.0, StatusCode::OK, "{}", started.1);
    let output = &started.1["result"]["structuredContent"]["output"];
    assert_eq!(output["execution_state"], "pending");
    assert_eq!(output["continuation"]["tool"], "observe_jobs");
    assert_eq!(output["continuation"]["arguments"]["wait_secs"], 1);
    let job = output["continuation"]["arguments"]["items"][0]["job_id"]
        .as_str()
        .unwrap();
    let request = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(request) = registry
                .poll(RunnerPollRequest {
                    client_id: "request-policy".into(),
                    runner_instance_id: "inst-policy".into(),
                })
                .await
                .unwrap()
            {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(request.kind, "start_process_job");
    assert_eq!(request.job_id.as_deref(), Some(job));
    assert_eq!(serde_json::to_value(&request).unwrap()["timeout_secs"], 60);
    let joined = post(
        &service,
        "tools/call",
        json!({"name":"wait_for_job_readiness", "arguments":{
            "job_ids":[job], "mode":"all", "wait_secs":45
        }}),
        Some("direct"),
        Some("6"),
    )
    .await;
    assert_eq!(joined.0, StatusCode::OK, "{}", joined.1);
    let joined = &joined.1["result"]["structuredContent"]["output"];
    assert_eq!(joined["wait_state"], "deadline");
    assert_eq!(joined["pending_job_ids"], json!([job]));
    registry
        .update_job(RunnerJobUpdateRequest {
            client_id: "request-policy".into(),
            runner_instance_id: "inst-policy".into(),
            job_id: job.into(),
            request_id: Some(request.request_id),
            update_seq: None,
            status: "completed".into(),
            stdout_chunk: Some("done\n".into()),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(1),
            error: None,
            command_execution_state: Some(ShellCommandExecutionState::Completed),
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
    let observed = post(
        &service,
        "tools/call",
        json!({"name":"observe_jobs", "arguments":{
            "items":[{"job_id":job}]
        }}),
        Some("direct"),
        None,
    )
    .await;
    assert_eq!(observed.0, StatusCode::OK, "{}", observed.1);
    let item = &observed.1["result"]["structuredContent"]["output"]["items"][0];
    assert_eq!(item["job_id"], job);
    assert_eq!(item["terminal"], true);
    assert_eq!(item["exit_code"], 0);
    assert_eq!(registry.list_jobs(None).await.len(), 1);
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "request-policy".into(),
            runner_instance_id: "inst-policy".into()
        })
        .await
        .unwrap()
        .is_none());
    assert_eq!(runtime.mcp_host_policy, deployment());
}

#[tokio::test]
async fn client_contract_published_coding_primitives_and_manifest_routes_agree() {
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for params in [mcp_2026_params(json!({})), mcp_2026_ui_params(json!({}))] {
        let listed = post(&service, "tools/list", params, None, None).await;
        assert_eq!(listed.0, StatusCode::OK);
        let tools = listed.1["result"]["tools"].as_array().unwrap();
        assert!(!tools.iter().any(|tool| tool["name"] == "apply_text_edits"));
        for (name, read_only) in [
            ("read_files", true),
            ("search_and_read", true),
            ("wait_for_job_readiness", true),
            ("edit_project_files", false),
            ("run_process", false),
        ] {
            let spec = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .expect("frequent coding callable must be published");
            assert!(spec["inputSchema"]["properties"]
                .as_object()
                .is_some_and(|p| !p.is_empty()));
            assert_eq!(spec["annotations"]["readOnlyHint"], read_only);
            let manifest = post(
                &service,
                "tools/call",
                json!({"name":"tool_manifest","arguments":{"tool_name":name}}),
                None,
                None,
            )
            .await;
            let canonical = &manifest.1["result"]["structuredContent"];
            assert_eq!(canonical["success"], true, "{canonical}");
            let route = &canonical["output"]["route"];
            assert_eq!(route["primary"]["mode"], "direct");
            assert_eq!(route["primary"]["tool"], name);
            assert_eq!(route["fallback"]["tool"], "call_runtime_tool");
            assert_eq!(route["tool_manifest_registers_host_tool"], false);
        }
        let gateway = tools
            .iter()
            .find(|tool| tool["name"] == "call_runtime_tool")
            .unwrap();
        assert_eq!(
            gateway["annotations"]["readOnlyHint"], false,
            "generic writes must not become parallel read hints"
        );
    }
}

#[tokio::test]
async fn request_policy_does_not_change_tools_apps_or_error_semantics() {
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime().with_mcp_host_policy(deployment()));
    let service = Service::new(build_test_router(test_config(Some("secret")), db, runtime));
    for params in [mcp_2026_params(json!({})), mcp_2026_ui_params(json!({}))] {
        let baseline = post(&service, "tools/list", params.clone(), None, None).await;
        for profile in ["direct", "host_code_mode"] {
            let listed = post(
                &service,
                "tools/list",
                params.clone(),
                Some(profile),
                Some("9"),
            )
            .await;
            assert_eq!(listed, baseline);
        }
    }
    for profile in ["direct", "host_code_mode"] {
        let failed = post(
            &service,
            "tools/call",
            json!({
                "name":"tool_manifest", "arguments":{"tool_name":"unknown_private_tool"}
            }),
            Some(profile),
            None,
        )
        .await;
        assert_eq!(failed.1["result"]["structuredContent"]["success"], false);
        assert_eq!(failed.1["result"]["isError"], true);
    }
}
