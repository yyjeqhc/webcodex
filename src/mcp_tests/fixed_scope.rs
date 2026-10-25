//! Real AuthMiddleware and MCP/Runtime HTTP handler fixtures; no listening Server.
//! These validate protocol framing, authorization and audit facts, not Host token cost.
use super::*;
use crate::mcp::request_policy::{McpSurface, SURFACE_HEADER};

struct ScopeFixture {
    _temp: tempfile::TempDir,
    db: Arc<crate::Database>,
    runtime: Arc<ToolRuntime>,
    service: Service,
    pat: String,
    oauth: String,
    runtime_only_pat: String,
    runtime_only_oauth: String,
    project_only_pat: String,
}

fn mint_fixed_scope_pat(
    db: &crate::Database,
    user: &crate::models::UserRecord,
    scopes: &str,
) -> String {
    let token = crate::auth::generate_api_token();
    let record = crate::models::ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: "fixed-scope-fixture".to_string(),
        key_prefix: crate::auth::token_prefix(&token),
        created_at: chrono::Utc::now().timestamp(),
        last_used_at: None,
        revoked_at: None,
        scopes: scopes.to_string(),
        expires_at: None,
        kind: crate::models::TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &crate::auth::hash_token(&token))
        .unwrap();
    token
}

async fn scope_fixture_runner(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    shared_key: Option<&str>,
) {
    let registration: RunnerRegisterRequest = serde_json::from_value(json!({
        "client_id":client_id, "agent_instance_id":format!("inst-{client_id}"),
        "agent_protocol_generation":crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        "owner":owner, "display_name":format!("private {client_id}"),
        "capabilities":{
            "shell":true, "jobs":true, "async_jobs":true, "async_shell_jobs":true,
            "native_tool_plugins":true
        }
    }))
    .unwrap();
    let access = shared_key
        .map(|key| crate::test_support::runner_access(&crate::auth::shared_key_context(key)));
    runtime
        .runner_registry
        .register_with_auth(
            crate::test_support::current_runner_registration(registration),
            access.as_ref(),
        )
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        &format!("inst-{client_id}"),
        vec![RunnerProjectSummary {
            id: "repo".to_string(),
            name: None,
            path: format!("/scope-fixture/{client_id}"),
            allow_patch: true,
            kind: Some("repo".to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
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
}

async fn fixed_scope_fixture() -> ScopeFixture {
    let (temp, db) = test_db();
    let user = seed_user(&db, "scope-alice-owner");
    let client = seed_oauth_client(&db, &user);
    let pat = mint_fixed_scope_pat(&db, &user, "runtime:read project:read");
    let oauth = seed_oauth_access_token(&db, &client, &user, "runtime:read project:read");
    let runtime_only_pat = mint_fixed_scope_pat(&db, &user, "runtime:read");
    let runtime_only_oauth = seed_oauth_access_token(&db, &client, &user, "runtime:read");
    let project_only_pat = mint_fixed_scope_pat(&db, &user, "project:read");
    let runtime = Arc::new(
        test_runtime_with_mcp_snapshot(true, true, false).with_window_activity_database(db.clone()),
    );
    scope_fixture_runner(&runtime, "scope-alice", Some("scope-alice-owner"), None).await;
    scope_fixture_runner(&runtime, "scope-bob", Some("scope-bob-owner"), None).await;
    scope_fixture_runner(
        &runtime,
        "scope-shared-a",
        None,
        Some("fixed-scope-shared-a"),
    )
    .await;
    scope_fixture_runner(
        &runtime,
        "scope-shared-b",
        None,
        Some("fixed-scope-shared-b"),
    )
    .await;
    let router = build_test_router(
        test_config_oauth2(Some("unused-fixed-scope-bootstrap")),
        db.clone(),
        runtime.clone(),
    )
    .push(
        Router::with_path("api/tools/call")
            .hoop(crate::AuthMiddleware)
            .post(crate::runtime_http::tools_call),
    );
    ScopeFixture {
        _temp: temp,
        db,
        runtime,
        service: Service::new(router),
        pat,
        oauth,
        runtime_only_pat,
        runtime_only_oauth,
        project_only_pat,
    }
}

async fn fixed_scope_request(
    service: &Service,
    token: &str,
    route: &str,
    modern: bool,
    client_name: &str,
    audit_id: &str,
    mut params: Value,
) -> (StatusCode, Value, Option<String>) {
    if modern {
        params = mcp_2026_params(params);
    }
    params["_meta"]["io.modelcontextprotocol/clientInfo"] =
        json!({"name":client_name,"version":"1.0"});
    params["_meta"]["openai/session"] = json!("fixed-scope-fixture-window");
    let mut request = TestClient::post("http://localhost/mcp")
        .bearer_auth(token)
        .add_header("x-action-session-id", audit_id, true);
    if route == "cli" {
        request = request.add_header(SURFACE_HEADER, "cli", true);
    }
    if modern {
        request = request
            .add_header(
                MCP_PROTOCOL_VERSION_HEADER,
                MCP_STATELESS_PROTOCOL_VERSION,
                true,
            )
            .add_header(MCP_METHOD_HEADER, "tools/call", true)
            .add_header(MCP_NAME_HEADER, params["name"].as_str().unwrap(), true);
    }
    let mut response = request
        .json(&json!({"jsonrpc":"2.0","id":42,"method":"tools/call","params":params}))
        .send(service)
        .await;
    let status = effective_status(&response);
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    (status, response.take_json().await.unwrap(), challenge)
}

fn fixed_scope_params(route: &str, tool: &str, arguments: Value) -> Value {
    if route == "gateway" {
        adaptive_runtime_gateway_params(tool, arguments)
    } else {
        json!({"name":tool,"arguments":arguments})
    }
}

pub(super) fn assert_fixed_scope_failure(body: &Value, scope: &str, cli: bool, recovery: &str) {
    assert!(body.get("error").is_none(), "{body}");
    let result = &body["result"];
    assert_eq!(result["isError"], true, "{body}");
    assert!(result.pointer("/_meta/mcp~1www_authenticate").is_none());
    let text = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for fact in [
        "insufficient_scope",
        scope,
        "execution_started=false",
        recovery,
    ] {
        assert!(text.contains(fact), "missing {fact}: {text}");
    }
    if cli {
        assert!(result.get("structuredContent").is_none());
    } else {
        let canonical = &result["structuredContent"];
        assert_eq!(canonical["success"], false);
        assert_eq!(canonical["output"]["error_kind"], "insufficient_scope");
        assert_eq!(canonical["output"]["missing_scope"], scope);
        assert_eq!(canonical["output"]["execution_started"], false);
        assert!(canonical["error"].as_str().unwrap().contains(scope));
    }
}

fn assert_fixed_scope_failed_audit(
    fixture: &ScopeFixture,
    audit_id: &str,
    body: &Value,
    cli: bool,
    canonical: bool,
) {
    let events = fixture.db.list_action_events(audit_id, 10).unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.status, "failed");
    assert_eq!(event.http_status, Some(200));
    let summary: Value = serde_json::from_str(&event.summary_json).unwrap();
    let expected = mcp_tool_response_audit_summary(
        &body["result"],
        if cli {
            McpSurface::Cli
        } else {
            McpSurface::Adaptive
        },
    )
    .unwrap();
    assert_eq!(summary["response_projection"], expected);
    assert_eq!(
        expected["result_json_bytes"],
        serde_json::to_vec(&body["result"]).unwrap().len()
    );
    if canonical {
        assert_eq!(summary["model_ergonomics"]["success"], false);
        assert_eq!(
            summary["model_ergonomics"]["error_kind"],
            "insufficient_scope"
        );
        // Canonical admission produced no ToolResult; final adapter bytes are measured above.
        assert!(summary["model_ergonomics"]["serialized_result_bytes"].is_null());
    }
}

async fn assert_scope_fixture_did_not_dispatch(fixture: &ScopeFixture) {
    assert!(fixture
        .runtime
        .runner_registry
        .list_jobs(None)
        .await
        .is_empty());
    for runner in fixture.runtime.runner_registry.list_runners().await {
        assert_eq!(runner.pending_requests, 0, "{}", runner.client_id);
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn fixed_credential_scope_canonical_http_matrix_preserves_failure_and_oauth() {
    let env = crate::auth::AuthEnvGuard::auth_required();
    env.enable_direct_shared_key();
    let fixture = fixed_scope_fixture().await;
    for (credential, token, oauth) in [
        ("pat", fixture.pat.as_str(), false),
        ("oauth", fixture.oauth.as_str(), true),
    ] {
        for modern in [false, true] {
            for route in ["direct", "gateway", "cli"] {
                for client in ["generic-test-client", "openai-mcp"] {
                    let mut baseline = None;
                    for (index, project) in [
                        "agent:scope-alice:repo",
                        "agent:scope-bob:repo",
                        "agent:scope-missing:repo",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let audit_id =
                            format!("fixed-scope-{credential}-{modern}-{route}-{client}-{index}");
                        let params = fixed_scope_params(
                            route,
                            "run_shell",
                            json!({"project":project,"command":"printf scope-fixture","purpose":"diagnostic"}),
                        );
                        let (status, body, challenge) = fixed_scope_request(
                            &fixture.service,
                            token,
                            route,
                            modern,
                            client,
                            &audit_id,
                            params,
                        )
                        .await;
                        if oauth {
                            super::oauth_scope::assert_mcp_oauth_scope_rejected(
                                status,
                                &body,
                                challenge.as_deref(),
                                Some(crate::auth::SCOPE_JOB_RUN),
                            );
                        } else {
                            assert_eq!(status, StatusCode::OK, "{body}");
                            assert!(challenge.is_none(), "{challenge:?}");
                            assert_fixed_scope_failure(
                                &body,
                                crate::auth::SCOPE_JOB_RUN,
                                route == "cli",
                                "fixed token",
                            );
                            assert_fixed_scope_failed_audit(
                                &fixture,
                                &audit_id,
                                &body,
                                route == "cli",
                                true,
                            );
                        }
                        let text = body.to_string();
                        for private in [
                            "scope-alice",
                            "scope-bob",
                            "scope-missing",
                            "/scope-fixture",
                            "printf scope-fixture",
                            token,
                        ] {
                            assert!(!text.contains(private), "{text}");
                        }
                        if let Some(baseline) = &baseline {
                            assert_eq!(
                                &body, baseline,
                                "scope errors must not disclose target existence"
                            );
                        } else {
                            baseline = Some(body);
                        }
                    }
                }
            }
        }
    }

    // tools/list stays HTTP 403 for a valid PAT lacking runtime:read.
    let (status, body, challenge) = oauth_mcp_request(
        &fixture.service,
        &fixture.project_only_pat,
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(challenge.is_none());
    assert_ne!(body["error"], "insufficient_scope");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains(crate::auth::SCOPE_RUNTIME_READ));

    // The canonical Runtime HTTP adapter retains its HTTP 403 contract.
    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth(&fixture.pat)
        .json(&json!({
            "tool":"run_shell",
            "params":{"project":"agent:scope-alice:repo","command":"printf scope-fixture"}
        }))
        .send(&fixture.service)
        .await;
    assert_eq!(effective_status(&response), StatusCode::FORBIDDEN);
    assert!(response.headers().get("www-authenticate").is_none());
    let body: Value = response.take_json().await.unwrap();
    assert!(body.get("result").is_none());
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains(crate::auth::SCOPE_JOB_RUN));
    assert_scope_fixture_did_not_dispatch(&fixture).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn fixed_credential_scope_specialized_and_shared_key_paths_fail_before_effects() {
    let env = crate::auth::AuthEnvGuard::auth_required();
    env.enable_direct_shared_key();
    let fixture = fixed_scope_fixture().await;
    let requests = [
        (
            json!({"name":crate::mcp_gateway::MCP_TOOL_NAME,"arguments":{"action":"list"}}),
            crate::auth::SCOPE_MCP_LOCAL,
        ),
        (
            json!({"name":crate::plugin_gateway::PLUGIN_TOOL_NAME,"arguments":{"action":"list"}}),
            crate::auth::SCOPE_PLUGIN_INSPECT,
        ),
        (
            json!({"name":crate::plugin_gateway::PLUGIN_TOOL_NAME,"arguments":{
                "action":"call","binding":"wc_pbind_AAAAAAAAAAAAAAAAAAAAAA","arguments":{}
            }}),
            crate::auth::SCOPE_PLUGIN_INVOKE,
        ),
        (
            json!({"name":crate::plugin_gateway::PLUGIN_TOOL_NAME,"arguments":{
                "action":"check","runner":"scope-bob","plugin":"private-plugin"
            }}),
            crate::auth::SCOPE_PLUGIN_MANAGE,
        ),
        (
            adaptive_runtime_gateway_params(
                crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME,
                json!({"action":"list","runner":"scope-bob"}),
            ),
            crate::auth::SCOPE_SSH_LOCAL,
        ),
    ];
    for (credential, token, oauth) in [
        ("pat", fixture.pat.as_str(), false),
        ("oauth", fixture.oauth.as_str(), true),
        ("shared", "fixed-scope-shared-a", false),
    ] {
        // Use the real shared-key profile, not a hand-edited AuthContext.
        for (index, (params, scope)) in requests.iter().enumerate() {
            for modern in [false, true] {
                let audit_id = format!("fixed-scope-special-{credential}-{index}-{modern}");
                let (status, body, challenge) = fixed_scope_request(
                    &fixture.service,
                    token,
                    "direct",
                    modern,
                    "openai-mcp",
                    &audit_id,
                    params.clone(),
                )
                .await;
                if oauth {
                    super::oauth_scope::assert_mcp_oauth_scope_rejected(
                        status,
                        &body,
                        challenge.as_deref(),
                        Some(scope),
                    );
                } else {
                    assert_eq!(status, StatusCode::OK, "{body}");
                    assert!(challenge.is_none());
                    assert_fixed_scope_failure(
                        &body,
                        scope,
                        false,
                        if credential == "shared" {
                            "fixed permission profile"
                        } else {
                            "fixed token"
                        },
                    );
                    assert_fixed_scope_failed_audit(&fixture, &audit_id, &body, false, false);
                }
                let text = body.to_string();
                for private in ["scope-bob", "private-plugin", "wc_pbind_", token] {
                    assert!(!text.contains(private), "{text}");
                }
            }
        }
    }

    // Shared Key includes job:run but does not acquire the independent job:detach scope.
    for route in ["gateway", "cli"] {
        let mut baseline = None;
        for (index, project) in [
            "agent:scope-shared-a:repo",
            "agent:scope-shared-b:repo",
            "agent:scope-missing:repo",
        ]
        .into_iter()
        .enumerate()
        {
            let arguments = json!({
                "project":project,"executable":"argv-helper","args":[],
                "idempotency_key":"fixed-scope-detach"
            });
            let params = adaptive_runtime_gateway_params("run_detached_process", arguments);
            let audit_id = format!("fixed-scope-detach-{route}-{index}");
            let (status, body, challenge) = fixed_scope_request(
                &fixture.service,
                "fixed-scope-shared-a",
                route,
                true,
                "openai-mcp",
                &audit_id,
                params,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert!(challenge.is_none());
            assert_fixed_scope_failure(
                &body,
                crate::auth::SCOPE_JOB_DETACH,
                route == "cli",
                "fixed permission profile",
            );
            assert_fixed_scope_failed_audit(&fixture, &audit_id, &body, route == "cli", true);
            if let Some(baseline) = &baseline {
                assert_eq!(&body, baseline);
            } else {
                baseline = Some(body);
            }
        }
    }

    // An empty Window still needs project:read, exactly as get_work_result_state does.
    // Fixed Shared Key grants already include it, so use genuinely limited PAT/OAuth tokens.
    for (token, oauth) in [
        (fixture.runtime_only_pat.as_str(), false),
        (fixture.runtime_only_oauth.as_str(), true),
    ] {
        let (status, body, challenge) = fixed_scope_request(
            &fixture.service,
            token,
            "direct",
            true,
            "openai-mcp",
            if oauth {
                "fixed-scope-empty-oauth"
            } else {
                "fixed-scope-empty-pat"
            },
            json!({"name":"work_result_thread_panel","arguments":{}}),
        )
        .await;
        if oauth {
            super::oauth_scope::assert_mcp_oauth_scope_rejected(
                status,
                &body,
                challenge.as_deref(),
                Some(crate::auth::SCOPE_PROJECT_READ),
            );
        } else {
            assert_eq!(status, StatusCode::OK, "{body}");
            assert!(challenge.is_none());
            assert_fixed_scope_failure(
                &body,
                crate::auth::SCOPE_PROJECT_READ,
                false,
                "fixed token",
            );
            // Keep standard stateless metadata; denial must not attach App state.
            let meta = &body["result"]["_meta"];
            assert_eq!(
                meta["io.modelcontextprotocol/serverInfo"]["name"],
                "webcodex"
            );
            for forbidden in [
                "ui",
                "ui/resourceUri",
                "mcp/www_authenticate",
                crate::mcp::tools::WORK_RESULT_APP_RESULT_META_KEY,
                crate::mcp::tools::WORK_RESULT_THREAD_CONTEXT_META_KEY,
            ] {
                assert!(
                    meta.get(forbidden).is_none(),
                    "unexpected {forbidden}: {meta}"
                );
            }
        }
    }
    assert_scope_fixture_did_not_dispatch(&fixture).await;
}
