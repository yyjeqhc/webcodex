//! HTTP adapter identity and authority fixtures. Requests traverse the real
//! Salvo router and AuthMiddleware; no external service or Claude installation.
use super::*;
use crate::client_window::{ClientWindow, CLIENT_WINDOW_HEADER, MCP_SESSION_HEADER};

async fn window_request(
    service: &Service,
    token: Option<&str>,
    modern: bool,
    method: &str,
    mut params: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value, Option<String>) {
    let mut request = TestClient::post("http://localhost/mcp");
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if modern {
        let host_meta = params.get("_meta").and_then(Value::as_object).cloned();
        params = mcp_2026_params(params);
        if let Some(meta) = host_meta {
            params["_meta"].as_object_mut().unwrap().extend(meta);
        }
        request = request
            .add_header(
                MCP_PROTOCOL_VERSION_HEADER,
                MCP_STATELESS_PROTOCOL_VERSION,
                true,
            )
            .add_header(MCP_METHOD_HEADER, method, true);
        if method == "tools/call" {
            request = request.add_header(MCP_NAME_HEADER, params["name"].as_str().unwrap(), true);
        }
    }
    for (name, value) in headers {
        request = request.add_header(
            salvo::http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            salvo::http::HeaderValue::from_str(value).unwrap(),
            false,
        );
    }
    let mut response = request
        .json(&json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}))
        .send(service)
        .await;
    let status = effective_status(&response);
    let issued = response
        .headers()
        .get(MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response.take_json::<Value>().await.unwrap();
    (status, body, issued)
}

fn status_params(meta: Value) -> Value {
    json!({"name":"get_runtime_status", "arguments":{}, "_meta":meta})
}

fn last_window(db: &crate::Database) -> (Option<String>, Option<String>) {
    db.conn_for_tests()
        .query_row(
            "SELECT client_window_key, client_window_source FROM action_events ORDER BY rowid DESC LIMIT 1",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn assert_success(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["isError"], false, "{body}");
}

#[tokio::test]
async fn codex_window_survives_new_requests_and_legacy_transport_ids() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-pat");
    let token = seed_action_audit_pat(&db, &user);
    let runtime = Arc::new(test_runtime());
    let first = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        runtime.clone(),
    ));
    let second = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        runtime,
    ));
    let mut keys = Vec::new();
    for (service, modern, transport) in [
        (&first, true, "transport-one"),
        (&second, true, "transport-two"),
        (&second, false, "transport-three"),
    ] {
        let (status, body, issued) = window_request(
            service,
            Some(&token),
            modern,
            "tools/call",
            status_params(json!({"threadId":"codex-thread-fixture"})),
            &[(MCP_SESSION_HEADER, transport)],
        )
        .await;
        assert_success(status, &body);
        assert!(issued.is_none());
        let (key, source) = last_window(&db);
        assert_eq!(source.as_deref(), Some("codex-thread"));
        let key = key.unwrap();
        assert_eq!(key.len(), 64);
        assert!(!key.contains("codex-thread-fixture"));
        keys.push(key);
    }
    assert!(keys.windows(2).all(|pair| pair[0] == pair[1]));
}

#[tokio::test]
async fn thread_header_source_and_api_credential_scopes_stay_separate() {
    let (_temp, db) = test_db();
    let alice = seed_user(&db, "window-alice");
    let bob = seed_user(&db, "window-bob");
    let alice_one = seed_action_audit_pat(&db, &alice);
    let alice_two = seed_action_audit_pat(&db, &alice);
    let bob_one = seed_action_audit_pat(&db, &bob);
    let service = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let mut keys = std::collections::BTreeSet::new();
    for (token, thread) in [
        (alice_one.as_str(), "same"),
        (alice_two.as_str(), "same"),
        (bob_one.as_str(), "same"),
        (alice_one.as_str(), "different"),
    ] {
        let (status, body, _) = window_request(
            &service,
            Some(token),
            true,
            "tools/call",
            status_params(json!({"threadId":thread})),
            &[],
        )
        .await;
        assert_success(status, &body);
        assert!(keys.insert(last_window(&db).0.unwrap()));
    }
    for modern in [false, true] {
        let (status, body, _) = window_request(
            &service,
            Some(&alice_one),
            modern,
            "tools/call",
            status_params(json!({})),
            &[(CLIENT_WINDOW_HEADER, "same")],
        )
        .await;
        assert_success(status, &body);
        let (key, source) = last_window(&db);
        assert_eq!(source.as_deref(), Some("client-window-header"));
        if !modern {
            assert!(keys.insert(key.unwrap()));
        } else {
            assert!(keys.contains(&key.unwrap()));
        }
    }
    assert_eq!(keys.len(), 5);
}

#[tokio::test]
async fn managed_oauth_window_survives_rotation_but_separates_user_and_client() {
    let (_temp, db) = test_db();
    let alice = seed_user(&db, "window-oauth-alice");
    let bob = seed_user(&db, "window-oauth-bob");
    let client = seed_oauth_client(&db, &alice);
    let other_client = seed_oauth_client(&db, &alice);
    let original = seed_oauth_access_token(&db, &client, &alice, "runtime:read");
    let rotated = seed_oauth_access_token(&db, &client, &alice, "runtime:read");
    let different_client = seed_oauth_access_token(&db, &other_client, &alice, "runtime:read");
    let different_user = seed_oauth_access_token(&db, &client, &bob, "runtime:read");
    let service = Service::new(build_test_router(
        test_config_oauth2(Some("unused-bootstrap")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let mut keys = Vec::new();
    for token in [&original, &rotated, &different_client, &different_user] {
        let (status, body, _) = window_request(
            &service,
            Some(token),
            true,
            "tools/call",
            status_params(json!({"threadId":"oauth-stable-thread"})),
            &[],
        )
        .await;
        assert_success(status, &body);
        keys.push(last_window(&db).0.unwrap());
    }
    assert_eq!(keys[0], keys[1]);
    assert_ne!(keys[0], keys[2]);
    assert_ne!(keys[0], keys[3]);
}

#[tokio::test]
async fn explicit_precedence_applies_to_both_protocol_eras() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-priority");
    let token = seed_action_audit_pat(&db, &user);
    let service = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let expected_openai = ClientWindow::from_opaque("openai-session", "compatible-openai").unwrap();
    for modern in [false, true] {
        for (meta, expected_source) in [
            (
                json!({"openai/session":"  compatible-openai  ", "threadId":"codex"}),
                "openai-session",
            ),
            (json!({"threadId":"codex"}), "codex-thread"),
            (json!({}), "client-window-header"),
        ] {
            let (status, body, _) = window_request(
                &service,
                Some(&token),
                modern,
                "tools/call",
                status_params(meta),
                &[
                    (CLIENT_WINDOW_HEADER, "explicit"),
                    (MCP_SESSION_HEADER, "transport"),
                ],
            )
            .await;
            assert_success(status, &body);
            let (key, source) = last_window(&db);
            assert_eq!(source.as_deref(), Some(expected_source));
            if expected_source == "openai-session" {
                assert_eq!(key.as_deref(), Some(expected_openai.key()));
            }
        }
    }
}

#[tokio::test]
async fn invalid_identity_rejects_before_audit_or_tool_effect_without_echo() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-invalid");
    let token = seed_action_audit_pat(&db, &user);
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        runtime.clone(),
    ));
    for modern in [false, true] {
        for invalid in [
            json!(""),
            json!(" private-invalid-value "),
            json!("a".repeat(257)),
            json!(42),
            json!(null),
            json!("界"),
            json!("private\nvalue"),
        ] {
            let (status, body, issued) = window_request(
                &service,
                Some(&token),
                modern,
                "tools/call",
                status_params(json!({"threadId":invalid})),
                &[
                    (CLIENT_WINDOW_HEADER, "valid-fallback"),
                    (MCP_SESSION_HEADER, "transport"),
                ],
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(body["error"]["code"], -32600);
            assert_eq!(
                body["error"]["message"],
                "invalid_client_window: malformed or ambiguous window identity"
            );
            assert!(!body.to_string().contains("private"));
            assert!(issued.is_none());
        }
        for headers in [
            vec![(CLIENT_WINDOW_HEADER, "invalid/value")],
            vec![
                (CLIENT_WINDOW_HEADER, "same"),
                (CLIENT_WINDOW_HEADER, "same"),
            ],
            vec![
                (CLIENT_WINDOW_HEADER, "first"),
                (CLIENT_WINDOW_HEADER, "second"),
            ],
        ] {
            let (status, body, _) = window_request(
                &service,
                Some(&token),
                modern,
                "tools/call",
                status_params(json!({"openai/session":"higher-priority"})),
                &headers,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        }
    }
    let count: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM action_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert!(runtime
        .window_activity_registry()
        .active_windows(None)
        .is_empty());
}

#[tokio::test]
async fn legacy_initialize_and_tools_call_keep_sessions_without_tools_list_ghosts() {
    let (_temp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("bootstrap-fixture")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let (status, initialized, issued) = window_request(
        &service,
        Some("bootstrap-fixture"),
        false,
        "initialize",
        json!({"_meta":{"openai/session":"host-window"}}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initialized}");
    let issued =
        issued.expect("legacy initialize still issues transport session with higher Window source");
    assert!(issued.starts_with("wc_mcp_"));
    let (status, listed, _) = window_request(
        &service,
        Some("bootstrap-fixture"),
        false,
        "tools/list",
        json!({}),
        &[(MCP_SESSION_HEADER, &issued)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(listed["result"]["tools"].is_array());
    assert_eq!(last_window(&db), (None, None));
    let (status, called, _) = window_request(
        &service,
        Some("bootstrap-fixture"),
        false,
        "tools/call",
        status_params(json!({})),
        &[(MCP_SESSION_HEADER, &issued)],
    )
    .await;
    assert_success(status, &called);
    assert_eq!(
        last_window(&db).0.as_deref(),
        Some(ClientWindow::from_opaque("mcp", &issued).unwrap().key())
    );
    assert_eq!(last_window(&db).1.as_deref(), Some("mcp"));

    // The existing format-only legacy contract accepts a valid opaque value;
    // there is intentionally no invented issued-session registry.
    let (status, body, _) = window_request(
        &service,
        Some("bootstrap-fixture"),
        false,
        "tools/call",
        status_params(json!({})),
        &[(MCP_SESSION_HEADER, "legacy-opaque-not-issued-here")],
    )
    .await;
    assert_success(status, &body);
    assert_eq!(last_window(&db).1.as_deref(), Some("mcp"));
}

#[tokio::test]
async fn explicit_tools_list_remains_window_liveness_and_unknown_hints_do_not_bind() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-list");
    let token = seed_action_audit_pat(&db, &user);
    let service = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    for (meta, headers, source) in [
        (
            json!({"openai/session":"host-list"}),
            vec![],
            "openai-session",
        ),
        (json!({"threadId":"codex-list"}), vec![], "codex-thread"),
        (
            json!({}),
            vec![(CLIENT_WINDOW_HEADER, "header-list")],
            "client-window-header",
        ),
    ] {
        let (status, body, _) = window_request(
            &service,
            Some(&token),
            true,
            "tools/list",
            json!({"_meta":meta}),
            &headers,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(last_window(&db).1.as_deref(), Some(source));
    }
    let (status, body, issued) = window_request(
        &service,
        Some(&token),
        true,
        "tools/call",
        status_params(json!({"windowId":"do-not-guess", "sessionId":"do-not-guess"})),
        &[(MCP_SESSION_HEADER, "ignored-stateless-transport")],
    )
    .await;
    assert_success(status, &body);
    assert!(issued.is_none());
    assert_eq!(last_window(&db), (None, None));
}

#[tokio::test]
async fn valid_new_sources_without_verified_scope_allow_unidentified_tool_execution() {
    let env = crate::auth::AuthEnvGuard::auth_required();
    env.enable_open_anonymous();
    for bootstrap in [false, true] {
        let (_temp, db) = test_db();
        let token = bootstrap.then_some("bootstrap-fixture");
        let service = Service::new(build_test_router(
            test_config(Some("bootstrap-fixture")),
            db.clone(),
            Arc::new(test_runtime()),
        ));
        for meta in [json!({"threadId":"valid-thread"}), json!({})] {
            let (status, body, _) = window_request(
                &service,
                token,
                true,
                "tools/call",
                status_params(meta),
                &[(CLIENT_WINDOW_HEADER, "valid-header")],
            )
            .await;
            assert_success(status, &body);
            assert_eq!(last_window(&db), (None, None));
        }
    }
}

fn seed_subject_oauth(
    db: &crate::Database,
    subject_kind: &str,
    subject_id: &str,
    grant: Option<&str>,
    shared_hash: Option<&str>,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let client = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: crate::auth::hash_token(&crate::auth::generate_oauth_client_secret()),
        name: "window-subject-fixture".into(),
        owner_user_id: None,
        owner_project_grant_id: grant.map(str::to_owned),
        owner_shared_key_hash: shared_hash.map(str::to_owned),
        redirect_uris: "https://client.example/callback".into(),
        allowed_scopes: "runtime:read project:read project:write job:run".into(),
        created_at: now,
        revoked_at: None,
    };
    db.insert_oauth_client(&client).unwrap();
    let token = crate::auth::generate_oauth_access_token();
    db.insert_oauth_access_token(&crate::models::OAuthAccessTokenRecord {
        admin_authority: false,
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: crate::auth::hash_token(&token),
        client_id: client.client_id,
        subject_kind: subject_kind.into(),
        subject_id: subject_id.into(),
        user_id: None,
        scopes: "runtime:read project:read project:write job:run".into(),
        resource: None,
        shared_key_hash: shared_hash.map(str::to_owned),
        created_at: now,
        expires_at: now + 3600,
        revoked_at: None,
        last_used_at: None,
    })
    .unwrap();
    token
}

#[tokio::test]
async fn shared_key_and_oauth_bridge_share_only_the_credential_window_scope() {
    let env = crate::auth::AuthEnvGuard::auth_required();
    env.enable_direct_shared_key();
    env.enable_oauth2_shared_key_bridge();
    let (_temp, db) = test_db();
    let config = test_config_oauth2(Some("unused-bootstrap"));
    let shared = crate::auth::shared_key_context("window-shared-key-a");
    let hash = shared.shared_key_hash.as_deref().unwrap();
    let bridge = seed_subject_oauth(&db, "shared_key", hash, None, Some(hash));
    let rotated = seed_subject_oauth(&db, "shared_key", hash, None, Some(hash));
    let service = Service::new(build_test_router(
        config.clone(),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let mut keys = Vec::new();
    for token in [
        "window-shared-key-a",
        bridge.as_str(),
        rotated.as_str(),
        "window-shared-key-b",
    ] {
        let (status, body, _) = window_request(
            &service,
            Some(token),
            true,
            "tools/call",
            status_params(json!({"threadId":"automatically-supplied-codex-thread"})),
            &[],
        )
        .await;
        assert_success(status, &body);
        keys.push(last_window(&db).0.unwrap());
    }
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[0], keys[2]);
    assert_ne!(keys[0], keys[3]);
    let bridge_auth = crate::auth::authenticate(&config, Some(&db), &bridge)
        .await
        .unwrap()
        .unwrap();
    let rotated_auth = crate::auth::authenticate(&config, Some(&db), &rotated)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        crate::tool_runtime::runtime_observation_principal(Some(&bridge_auth)).unwrap(),
        crate::tool_runtime::runtime_observation_principal(Some(&rotated_auth)).unwrap(),
        "Window scope must not broaden the token-scoped peer/observation principal",
    );
}

#[tokio::test]
async fn project_oauth_and_direct_credentials_bind_to_exact_grant_not_access_token() {
    let (_temp, db) = test_db();
    let share_session = format!("wc_share_{}", "a".repeat(64));
    let mut keys = Vec::new();
    for grant in [
        "wc_pgrant_aaaaaaaaaaaaaaaaaaaaaaaa",
        "wc_pgrant_bbbbbbbbbbbbbbbbbbbbbbbb",
    ] {
        let mut config = (*test_config_oauth2(Some("unused-bootstrap"))).clone();
        config.oauth2.project_share_grant_id = Some(grant.into());
        config.oauth2.project_share_session_id = Some(share_session.clone());
        let service = Service::new(build_test_router(
            Arc::new(config),
            db.clone(),
            Arc::new(test_runtime()),
        ));
        let subject = format!("{grant}|{share_session}");
        let params = status_params(json!({"threadId":"same-project-thread"}));
        let direct_auth = crate::auth::shared_key::project_credential_context(grant);
        let expected = crate::client_window::mcp_window(
            &Request::new(),
            &params,
            Some(&direct_auth),
            false,
            false,
        )
        .unwrap()
        .identity
        .unwrap();
        for _ in 0..2 {
            let token = seed_subject_oauth(
                &db,
                crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND,
                &subject,
                Some(grant),
                None,
            );
            let (status, body, _) = window_request(
                &service,
                Some(&token),
                true,
                "tools/call",
                params.clone(),
                &[],
            )
            .await;
            assert_success(status, &body);
            let key = last_window(&db).0.unwrap();
            assert_eq!(key, expected.key());
            keys.push(key);
        }
    }
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[2], keys[3]);
    assert_ne!(keys[0], keys[2]);
}

#[tokio::test]
async fn stable_managed_oauth_window_does_not_broaden_peer_principal() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-peer-scope");
    let client = seed_oauth_client(&db, &user);
    let config = test_config_oauth2(Some("unused-bootstrap"));
    let first = seed_oauth_access_token(&db, &client, &user, "runtime:read");
    let next = seed_oauth_access_token(&db, &client, &user, "runtime:read");
    let first = crate::auth::authenticate(&config, Some(&db), &first)
        .await
        .unwrap()
        .unwrap();
    let next = crate::auth::authenticate(&config, Some(&db), &next)
        .await
        .unwrap()
        .unwrap();
    let params = status_params(json!({"threadId":"same-thread"}));
    let resolve = |auth: &crate::auth::AuthContext| {
        crate::client_window::mcp_window(&Request::new(), &params, Some(auth), false, false)
            .unwrap()
            .identity
            .unwrap()
    };
    assert_eq!(resolve(&first).key(), resolve(&next).key());
    assert_ne!(
        crate::tool_runtime::runtime_observation_principal(Some(&first)).unwrap(),
        crate::tool_runtime::runtime_observation_principal(Some(&next)).unwrap(),
    );
}

fn runtime_tool_params(name: &str, arguments: Value, meta: Value) -> Value {
    let mut params = if matches!(
        crate::model_surface::adaptive_runtime_gateway_target_route(name),
        crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Gateway
    ) {
        adaptive_runtime_gateway_params(name, arguments)
    } else {
        json!({"name": name, "arguments": arguments})
    };
    params["_meta"] = meta;
    params
}

#[tokio::test]
async fn supplied_window_identity_cannot_grant_foreign_project_or_session_authority() {
    let (_temp, db) = test_db();
    let user = seed_user(&db, "identity-owner");
    let foreign = seed_user(&db, "identity-foreign");
    let owner_token = seed_action_audit_pat(&db, &user);
    let foreign_token = seed_action_audit_pat(&db, &foreign);
    let config = test_config(Some("unused-bootstrap"));
    let owner_auth = crate::auth::authenticate(&config, Some(&db), &owner_token)
        .await
        .unwrap()
        .unwrap();
    let runtime = Arc::new(test_runtime());
    let registration: RunnerRegisterRequest = serde_json::from_value(json!({
        "client_id":"identity-owner-runner", "agent_instance_id":"identity-owner-instance",
        "agent_protocol_generation":crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        "owner":"identity-owner", "capabilities":RunnerCapabilities::default()
    }))
    .unwrap();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            registration,
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "identity-owner-runner",
        "identity-owner-instance",
        vec![RunnerProjectSummary {
            id: "repo".into(),
            name: None,
            path: "/identity-fixture/repo".into(),
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
    let project = "agent:identity-owner-runner:repo";
    let session = runtime
        .sessions
        .start_session_with_options(
            crate::tool_runtime::sessions::SessionCreateOptions::new(
                Some(project.into()),
                Some("identity authority fixture".into()),
                crate::tool_runtime::SessionMode::Normal,
                crate::tool_runtime::sessions::SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                crate::tool_runtime::workflow_session_authority_fingerprint(Some(&owner_auth))
                    .unwrap(),
            )),
        )
        .unwrap()
        .session_id;
    let before = runtime
        .sessions
        .summary(&session, None)
        .unwrap()
        .events_total;
    let service = Service::new(build_test_router(config, db, runtime.clone()));
    for meta in [
        json!({"threadId":"same-caller-controlled-thread"}),
        json!({"openai/session":"same-global-host-window"}),
    ] {
        // Neither client metadata nor a custom header supplies Project authority.
        let (status, denied, _) = window_request(
            &service,
            Some(&foreign_token),
            true,
            "tools/call",
            runtime_tool_params(
                "read_files",
                json!({"project":project, "items":[{"path":"README.md"}]}),
                meta.clone(),
            ),
            &[(CLIENT_WINDOW_HEADER, "same-header")],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{denied}");
        assert_eq!(denied["result"]["isError"], true, "{denied}");
        assert!(!denied.to_string().contains("/identity-fixture/repo"));
        let (status, denied, _) = window_request(
            &service,
            Some(&foreign_token),
            true,
            "tools/call",
            runtime_tool_params("read_session_handoff", json!({"session_id":session}), meta),
            &[(CLIENT_WINDOW_HEADER, "same-header")],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{denied}");
        assert_eq!(denied["result"]["isError"], true, "{denied}");
        assert!(!denied.to_string().contains("identity authority fixture"));
    }
    assert_eq!(
        runtime
            .sessions
            .summary(&session, None)
            .unwrap()
            .events_total,
        before
    );
}

#[test]
fn new_mcp_window_values_are_absent_from_full_trace_and_durable_audit() {
    std::thread::Builder::new()
        .name("mcp-window-privacy".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(window_privacy_fixture())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn window_privacy_fixture() {
    let trace_root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        trace_root.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    let (_temp, db) = test_db();
    let user = seed_user(&db, "window-privacy");
    let token = seed_action_audit_pat(&db, &user);
    let service = Service::new(build_test_router(
        test_config(Some("unused-bootstrap")),
        db.clone(),
        Arc::new(test_runtime()),
    ));
    let raw_thread = "raw-thread-value-must-not-be-retained";
    let raw_header = "raw-header-value-must-not-be-retained";
    for meta in [json!({"threadId":raw_thread}), json!({})] {
        let (status, body, _) = window_request(
            &service,
            Some(&token),
            true,
            "tools/call",
            status_params(meta),
            &[(CLIENT_WINDOW_HEADER, raw_header)],
        )
        .await;
        assert_success(status, &body);
    }
    crate::tool_request_trace::flush_full_trace_writer();
    let mut parsed_sources = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(trace_root.path()).unwrap().flatten() {
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let path = entry.path();
        let Ok(events) = std::fs::read_to_string(path.join("events.jsonl")) else {
            continue;
        };
        assert!(!events.contains(raw_thread));
        assert!(!events.contains(raw_header));
        for line in events.lines() {
            let event: Value = serde_json::from_str(line).unwrap();
            if event["event"] == "mcp_tool_request_parsed" {
                if let Some(source) = event["client_window_source"].as_str() {
                    parsed_sources.insert(source.to_owned());
                }
            }
            if let Some(relative) = event["payload_path"].as_str() {
                let compressed = std::fs::read(path.join(relative)).unwrap();
                let raw = zstd::stream::decode_all(compressed.as_slice()).unwrap();
                let text = String::from_utf8_lossy(&raw);
                assert!(!text.contains(raw_thread));
                assert!(!text.contains(raw_header));
            }
        }
    }
    assert_eq!(parsed_sources.len(), 2);
    assert!(parsed_sources.contains("codex-thread"));
    assert!(parsed_sources.contains("client-window-header"));
    let rows =
        {
            let conn = db.conn_for_tests();
            let mut statement = conn.prepare(
            "SELECT client_window_key, client_window_source, summary_json FROM action_events"
        ).unwrap();
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
    assert_eq!(rows.len(), 2);
    let persisted = format!("{rows:?}");
    assert!(!persisted.contains(raw_thread));
    assert!(!persisted.contains(raw_header));
}
