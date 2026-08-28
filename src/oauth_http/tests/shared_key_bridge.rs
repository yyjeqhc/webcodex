use super::*;

#[test]
fn bridge_shared_key_hash_matches_shared_key_visibility_hash() {
    assert_eq!(
        bridge_shared_key_hash("shared-secret").unwrap(),
        shared_key_hash_of("shared-secret")
    );
    assert_eq!(
        bridge_shared_key_hash("  shared-secret  ").unwrap(),
        shared_key_hash_of("shared-secret")
    );
    assert_eq!(
        bridge_shared_key_hash("shared-secret").unwrap(),
        shared_key_hash_of("  shared-secret  ")
    );
}
#[test]
fn normalize_bridge_oauth_scopes_rejects_account_scope_with_bridge_message() {
    let err = normalize_bridge_oauth_scopes(
        Some("account:manage"),
        "runtime:read project:read account:manage",
    )
    .unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidScope(OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE)
    );
}

#[test]
fn bridge_computer_scopes_use_explicit_closed_ceiling_without_changing_direct_authority() {
    assert_eq!(
        bridge_oauth_scopes(),
        crate::auth::DIRECT_SHARED_KEY_MODEL_SCOPES
    );
    assert_eq!(
        bridge_oauth_computer_enabled_scopes(),
        &[
            "runtime:read",
            "session:collaborate",
            "project:read",
            "project:write",
            "memory:read",
            "memory:manage",
            "job:run",
            "computer:read",
            "computer:control",
            "computer:launch",
            "computer:display_read",
            "computer:pointer_control",
            "computer:clipboard_read",
            "computer:clipboard_write",
        ]
    );
    assert_eq!(
        normalize_bridge_oauth_scopes(
            Some("computer:launch computer:display_read computer:pointer_control computer:clipboard_read computer:clipboard_write"),
            &bridge_oauth_computer_enabled_scopes().join(" "),
        )
        .unwrap(),
        "computer:launch computer:display_read computer:pointer_control computer:clipboard_read computer:clipboard_write"
    );
    for scope in [
        "account:manage",
        "job:detach",
        "admin",
        "agent:register",
        "agent:future",
        "computer:future_sensitive",
    ] {
        assert!(
            normalize_bridge_oauth_scopes(Some(scope), scope).is_err(),
            "future or privileged scope escaped bridge ceiling: {scope}"
        );
    }
    assert!(!bridge_oauth_scopes().contains(&"computer:launch"));
    assert!(!bridge_oauth_scopes().contains(&"computer:display_read"));
    assert!(!bridge_oauth_scopes().contains(&"computer:pointer_control"));
    assert!(!bridge_oauth_scopes().contains(&"computer:clipboard_read"));
    assert!(!bridge_oauth_scopes().contains(&"computer:clipboard_write"));
}

#[test]
fn bridge_local_mcp_scope_requires_explicit_client_ceiling_opt_in() {
    assert!(!bridge_oauth_scopes().contains(&crate::auth::SCOPE_MCP_LOCAL));

    let baseline = bridge_oauth_scopes()
        .iter()
        .map(|scope| (*scope).to_string())
        .collect::<Vec<_>>();
    let mut opted_in = baseline.clone();
    opted_in.push(crate::auth::SCOPE_MCP_LOCAL.to_string());
    assert!(
        normalize_bridge_oauth_scopes(Some(crate::auth::SCOPE_MCP_LOCAL), &opted_in.join(" "),)
            .is_ok()
    );
    assert!(
        normalize_bridge_oauth_scopes(Some(crate::auth::SCOPE_MCP_LOCAL), &baseline.join(" "),)
            .is_err()
    );
}

#[tokio::test]
async fn bridge_authorize_local_mcp_requires_shared_key_owned_opt_in() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "local-mcp-owned-shared-key";
    let allowed_scopes = format!(
        "{} {}",
        bridge_oauth_scopes().join(" "),
        crate::auth::SCOPE_MCP_LOCAL
    );
    let (owned, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://local-mcp.example/callback",
        &allowed_scopes,
    );
    let service = Service::new(build_router(config.clone(), db.clone()));
    let owned_url = valid_bridge_authorize_url(
        &owned,
        "https://local-mcp.example/callback",
        "runtime:read mcp:local",
    );
    let mut owned_response = TestClient::get(&owned_url).send(&service).await;
    assert_eq!(owned_response.status_code, Some(StatusCode::OK));
    let owned_html = owned_response.take_string().await.unwrap_or_default();
    assert!(owned_html.contains("mcp:local"));

    let user = seed_user(&db, "local-mcp-legacy-owner");
    let legacy = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://legacy-local-mcp.example/callback",
        &allowed_scopes,
    );
    let legacy_url = valid_bridge_authorize_url(
        &legacy,
        "https://legacy-local-mcp.example/callback",
        "runtime:read mcp:local",
    );
    let legacy_response = TestClient::get(&legacy_url).send(&service).await;
    assert_eq!(legacy_response.status_code, Some(StatusCode::FOUND));
    let location = url::Url::parse(&location_header(&legacy_response).unwrap()).unwrap();
    assert_eq!(
        location
            .query_pairs()
            .find(|(key, _)| key == "error")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("invalid_scope")
    );
}

#[test]
fn bridge_coding_agent_scope_requires_explicit_client_ceiling_opt_in() {
    assert!(!bridge_oauth_scopes().contains(&crate::auth::SCOPE_CODING_AGENT_RUN));

    let baseline = bridge_oauth_scopes()
        .iter()
        .map(|scope| (*scope).to_string())
        .collect::<Vec<_>>();
    let mut opted_in = baseline.clone();
    opted_in.push(crate::auth::SCOPE_CODING_AGENT_RUN.to_string());
    assert!(normalize_bridge_oauth_scopes(
        Some(crate::auth::SCOPE_CODING_AGENT_RUN),
        &opted_in.join(" "),
    )
    .is_ok());
    assert!(normalize_bridge_oauth_scopes(
        Some(crate::auth::SCOPE_CODING_AGENT_RUN),
        &baseline.join(" "),
    )
    .is_err());
}

#[tokio::test]
async fn bridge_authorize_coding_agent_requires_shared_key_owned_opt_in() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "coding-agent-owned-shared-key";
    let allowed_scopes = format!(
        "{} {}",
        bridge_oauth_scopes().join(" "),
        crate::auth::SCOPE_CODING_AGENT_RUN
    );
    let (owned, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://coding-agent.example/callback",
        &allowed_scopes,
    );
    let service = Service::new(build_router(config.clone(), db.clone()));
    let owned_url = valid_bridge_authorize_url(
        &owned,
        "https://coding-agent.example/callback",
        "runtime:read coding_agent:run",
    );
    let mut owned_response = TestClient::get(&owned_url).send(&service).await;
    assert_eq!(owned_response.status_code, Some(StatusCode::OK));
    let owned_html = owned_response.take_string().await.unwrap_or_default();
    assert!(owned_html.contains("coding_agent:run"));

    let user = seed_user(&db, "coding-agent-legacy-owner");
    let legacy = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://legacy-coding-agent.example/callback",
        &allowed_scopes,
    );
    let legacy_url = valid_bridge_authorize_url(
        &legacy,
        "https://legacy-coding-agent.example/callback",
        "runtime:read coding_agent:run",
    );
    let legacy_response = TestClient::get(&legacy_url).send(&service).await;
    assert_eq!(legacy_response.status_code, Some(StatusCode::FOUND));
    let location = url::Url::parse(&location_header(&legacy_response).unwrap()).unwrap();
    assert_eq!(
        location
            .query_pairs()
            .find(|(key, _)| key == "error")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("invalid_scope")
    );
}

#[test]
fn normalize_bridge_oauth_scopes_accepts_offline_access_as_protocol_scope() {
    let normalized = normalize_bridge_oauth_scopes(
        Some("runtime:read offline_access"),
        "runtime:read project:read",
    )
    .unwrap();

    assert_eq!(normalized, "runtime:read offline_access");
}

async fn register_shared_key_runner(registry: &crate::ShellClientRegistry, shared_key: &str) {
    register_shared_key_runner_with_capabilities(
        registry,
        shared_key,
        "bridge-runner",
        "bridge-instance",
        crate::shell_protocol::ShellClientCapabilities::default(),
    )
    .await;
}

async fn register_shared_key_runner_with_capabilities(
    registry: &crate::ShellClientRegistry,
    shared_key: &str,
    client_id: &str,
    instance_id: &str,
    capabilities: crate::shell_protocol::ShellClientCapabilities,
) {
    let auth = crate::auth::shared_key_context(shared_key);
    registry
        .register_with_auth(
            crate::shell_protocol::ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: instance_id.to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(capabilities),
                host_context: None,
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
            },
            Some(&auth),
        )
        .await
        .unwrap();
}

fn all_optional_computer_capabilities() -> crate::shell_protocol::ShellClientCapabilities {
    crate::shell_protocol::ShellClientCapabilities {
        computer_application_discovery: true,
        computer_application_launch: true,
        computer_display_observe: true,
        computer_pointer_control: true,
        computer_clipboard_read: true,
        computer_clipboard_write: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn shared_key_client_provision_is_group_bound_and_preserves_narrow_scope_on_rotation() {
    let env = crate::auth::AuthEnvGuard::new();
    env.enable_direct_shared_key();
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let registry = Arc::new(crate::ShellClientRegistry::default());
    let shared_key = "ordinary-connect-shared-key";
    register_shared_key_runner(&registry, shared_key).await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://chatgpt.example/callback"
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let created: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(created["reused"], false);
    let client_id = created["client"]["client_id"].as_str().unwrap().to_string();
    assert!(created["client_secret"]
        .as_str()
        .unwrap()
        .starts_with("wc_csec_"));
    let expected_scopes = bridge_oauth_scopes()
        .iter()
        .map(|scope| serde_json::Value::String((*scope).to_string()))
        .collect::<Vec<_>>();
    assert_eq!(
        created["client"]["allowed_scopes"],
        serde_json::Value::Array(expected_scopes)
    );
    let stored = db
        .get_oauth_client_by_client_id(&client_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.owner_shared_key_hash.as_deref(),
        Some(shared_key_hash_of(shared_key).as_str())
    );
    assert!(!stored.allowed_scopes_vec().iter().any(|scope| {
        scope == "account:manage" || scope == "admin" || scope.starts_with("agent:")
    }));

    // Simulate an older/narrower persisted connect client. Reuse must not widen it.
    db.update_oauth_client_allowed_scopes_and_revoke_grants(
        &client_id,
        &stored.allowed_scopes,
        "runtime:read project:read",
        chrono::Utc::now().timestamp(),
    )
    .unwrap()
    .unwrap();
    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://chatgpt.example/callback",
            "client_id": client_id,
            "previous_allowed_scopes": ["runtime:read", "project:read"]
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let reused: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(reused["reused"], true);
    assert!(reused.get("client_secret").is_none());
    assert_eq!(
        reused["client"]["allowed_scopes"],
        serde_json::json!(["runtime:read", "project:read"])
    );

    // Revocation + explicit Computer opt-in rotates to the same narrow baseline
    // plus only the fixed optional Computer scopes.
    let old = db
        .get_oauth_client_by_client_id(&client_id)
        .unwrap()
        .unwrap();
    db.revoke_oauth_client(&old.id, chrono::Utc::now().timestamp())
        .unwrap();
    let narrow_computer_scopes = serde_json::json!([
        "runtime:read",
        "project:read",
        "computer:launch",
        "computer:display_read",
        "computer:pointer_control",
        "computer:clipboard_read",
        "computer:clipboard_write"
    ]);
    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://chatgpt.example/callback",
            "client_id": client_id,
            "previous_allowed_scopes": ["runtime:read", "project:read"],
            "computer_permissions": true
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let rotated: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(rotated["reused"], false);
    assert_ne!(rotated["client"]["client_id"], client_id);
    assert_eq!(rotated["client"]["allowed_scopes"], narrow_computer_scopes);
    for restored in [
        "project:write",
        "job:run",
        "computer:read",
        "computer:control",
    ] {
        assert!(!rotated["client"]["allowed_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|scope| scope == restored));
    }

    // Missing-client replacement of an already Computer-enabled protected profile
    // preserves the same narrow baseline + optional ceiling.
    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://chatgpt.example/callback",
            "client_id": "wc_client_missing_narrow_computer",
            "previous_allowed_scopes": [
                "runtime:read",
                "project:read",
                "computer:launch",
                "computer:display_read",
                "computer:pointer_control",
                "computer:clipboard_read",
                "computer:clipboard_write"
            ],
            "computer_permissions": true
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let missing_rotated: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(missing_rotated["reused"], false);
    assert_eq!(
        missing_rotated["client"]["allowed_scopes"],
        narrow_computer_scopes
    );

    // A genuinely fresh opt-in keeps the ordinary full baseline + optional behavior.
    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://fresh-computer.example/callback",
            "computer_permissions": true
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let fresh: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(fresh["reused"], false);
    assert_eq!(
        fresh["client"]["allowed_scopes"],
        serde_json::Value::Array(
            bridge_oauth_computer_enabled_scopes()
                .iter()
                .map(|scope| serde_json::Value::String((*scope).to_string()))
                .collect()
        )
    );
}

#[tokio::test]
async fn shared_key_client_provision_rejects_invalid_computer_enabled_previous_ceiling() {
    let env = crate::auth::AuthEnvGuard::new();
    env.enable_direct_shared_key();
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let registry = Arc::new(crate::ShellClientRegistry::default());
    let shared_key = "invalid-computer-ceiling-shared-key";
    register_shared_key_runner(&registry, shared_key).await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db,
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    for previous in vec![
        vec!["runtime:read", "computer:launch"],
        vec!["runtime:read", "runtime:read"],
        vec!["runtime:read", "computer:future"],
        vec!["runtime:read", "account:manage"],
        vec!["runtime:read", "admin"],
        vec!["runtime:read", "job:detach"],
        vec!["runtime:read", "agent:register"],
    ] {
        let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
            .add_header("authorization", format!("Bearer {shared_key}"), true)
            .json(&serde_json::json!({
                "redirect_uri": "https://invalid-computer.example/callback",
                "previous_allowed_scopes": previous,
                "computer_permissions": true
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::CONFLICT));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(
            body["error"],
            "persisted OAuth scope ceiling is not valid for Computer opt-in"
        );
        assert!(body.get("client_secret").is_none());
    }
}

#[tokio::test]
async fn shared_key_client_provision_fails_closed_on_client_lookup_error() {
    let env = crate::auth::AuthEnvGuard::new();
    env.enable_direct_shared_key();
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let registry = Arc::new(crate::ShellClientRegistry::default());
    let shared_key = "ordinary-connect-shared-key";
    register_shared_key_runner(&registry, shared_key).await;
    db.conn_for_tests()
        .execute_batch("DROP TABLE oauth_clients;")
        .unwrap();
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db,
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let mut resp = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://chatgpt.example/callback",
            "client_id": "wc_client_existing",
            "previous_allowed_scopes": ["runtime:read"]
        }))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"], "failed to read existing OAuth client");
    assert!(body.get("client_secret").is_none());
}

#[tokio::test]
async fn shared_key_owned_bridge_client_rejects_wrong_key_and_revoked_client() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let correct_key = "correct-connect-shared-key";
    let (client, _secret) = seed_shared_key_bridge_client(
        &db,
        correct_key,
        "https://chatgpt.example/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let wrong_body = bridge_form_body(
        &client,
        "https://chatgpt.example/callback",
        "runtime:read",
        "wrong-connect-shared-key",
    );
    let before = auth_code_count(&db);
    let resp = post_form("http://localhost/oauth/authorize/bridge", wrong_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    assert_eq!(auth_code_count(&db), before);

    let correct_body = bridge_form_body(
        &client,
        "https://chatgpt.example/callback",
        "runtime:read",
        correct_key,
    );
    let mut resp = post_form("http://localhost/oauth/authorize/bridge", correct_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
    let text = resp.take_string().await.unwrap_or_default();
    assert!(text.contains("shared-key Runner group is no longer connected"));

    db.revoke_oauth_client(&client.id, chrono::Utc::now().timestamp())
        .unwrap();
    let url =
        valid_bridge_authorize_url(&client, "https://chatgpt.example/callback", "runtime:read");
    let resp = TestClient::get(&url).send(&service).await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    assert_eq!(auth_code_count(&db), before);
}

// -----------------------------------------------------------------------
// Authorization endpoint
// -----------------------------------------------------------------------

#[tokio::test]
async fn bridge_authorize_get_disabled_creates_no_code() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_bridge_authorize_url(&client, "https://example.com/callback", "runtime:read");
    let before = auth_code_count(&db);

    let resp = TestClient::get(&url).send(&service).await;

    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
}

#[tokio::test]
async fn bridge_authorize_post_disabled_creates_no_code() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let body = bridge_form_body(
        &client,
        "https://example.com/callback",
        "runtime:read",
        "shared-secret",
    );
    let before = auth_code_count(&db);

    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
}

#[tokio::test]
async fn bridge_disabled_does_not_break_managed_user_authorize() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    let (_resp, _location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let record = auth_code_by_plaintext(&db, &code);

    assert_eq!(record.subject_kind, "managed_user");
    assert_eq!(record.user_id.as_deref(), Some(user.id.as_str()));
    assert_eq!(record.shared_key_hash, None);
}

#[tokio::test]
async fn bridge_authorize_get_invalid_client_or_redirect_creates_no_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let invalid_client_url = authorize_url(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", "wc_client_missing"),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);
    let mismatch_url =
        valid_bridge_authorize_url(&client, "https://attacker.example/callback", "runtime:read");

    for url in [invalid_client_url, mismatch_url] {
        let before = auth_code_count(&db);
        let resp = TestClient::get(&url).send(&service).await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }
}

#[tokio::test]
async fn bridge_authorize_rejects_missing_or_invalid_pkce_without_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let urls = [
        authorize_url(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("code_challenge_method", "S256"),
        ]),
        authorize_url(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "plain"),
        ]),
    ];

    for url in urls {
        let before = auth_code_count(&db);
        let resp = TestClient::get(&url).send(&service).await;
        assert_eq!(auth_code_count(&db), before);
        assert_ne!(resp.status_code, Some(StatusCode::OK));
    }
}

#[tokio::test]
async fn bridge_authorize_get_renders_form_and_creates_no_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_bridge_authorize_url(&client, "https://example.com/callback", "runtime:read");
    let before = auth_code_count(&db);

    let mut resp = TestClient::get(&url).send(&service).await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
    let text = resp.take_string().await.unwrap_or_default();
    assert!(text.contains("/oauth/authorize/bridge"));
    assert!(text.contains("name=\"shared_key\""));
    assert!(!text.contains("wc_oac_"));
}

#[tokio::test]
async fn bridge_authorize_picker_is_only_for_explicit_computer_enabled_owned_clients() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "picker-shared-key";
    let (baseline_client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://baseline.example/callback",
        &bridge_oauth_scopes().join(" "),
    );
    let (enabled_client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://enabled.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let narrow_computer_scopes = "runtime:read project:read computer:launch computer:display_read computer:pointer_control computer:clipboard_read computer:clipboard_write";
    let (narrow_enabled_client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://narrow-enabled.example/callback",
        narrow_computer_scopes,
    );
    let (partial_optional_client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://partial-optional.example/callback",
        "runtime:read project:read computer:launch",
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "picker-runner",
        "picker-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let baseline_url = valid_bridge_authorize_url(
        &baseline_client,
        "https://baseline.example/callback",
        &bridge_oauth_scopes().join(" "),
    );
    let mut baseline = TestClient::get(&baseline_url).send(&service).await;
    assert_eq!(baseline.status_code, Some(StatusCode::OK));
    let baseline_html = baseline.take_string().await.unwrap_or_default();
    assert!(baseline_html.contains("Standard access"));
    assert!(!baseline_html.contains("Additional Computer permissions"));
    assert!(!baseline_html.contains("name=\"computer_permission\""));

    let enabled_url = valid_bridge_authorize_url(
        &enabled_client,
        "https://enabled.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let mut enabled = TestClient::get(&enabled_url).send(&service).await;
    assert_eq!(enabled.status_code, Some(StatusCode::OK));
    assert_eq!(auth_code_count(&db), 0);
    let enabled_html = enabled.take_string().await.unwrap_or_default();
    assert!(enabled_html.contains("Additional Computer permissions"));
    for (id, label) in [
        ("launch", "Launch applications"),
        ("display", "Full-display observation"),
        ("pointer", "Pointer control"),
        ("clipboard_read", "Read clipboard"),
        ("clipboard_write", "Write clipboard"),
    ] {
        assert!(enabled_html.contains(&format!("value=\"{id}\">")), "{id}");
        assert!(enabled_html.contains(label), "{id}");
        assert!(
            !enabled_html.contains(&format!("value=\"{id}\" checked")),
            "{id}"
        );
    }
    let narrow_url = valid_bridge_authorize_url(
        &narrow_enabled_client,
        "https://narrow-enabled.example/callback",
        narrow_computer_scopes,
    );
    let mut narrow = TestClient::get(&narrow_url).send(&service).await;
    assert_eq!(narrow.status_code, Some(StatusCode::OK));
    let narrow_html = narrow.take_string().await.unwrap_or_default();
    assert!(narrow_html.contains("Additional Computer permissions"));
    assert!(narrow_html.contains("value=\"launch\" disabled>"));

    let partial_url = valid_bridge_authorize_url(
        &partial_optional_client,
        "https://partial-optional.example/callback",
        "runtime:read project:read computer:launch",
    );
    let partial = TestClient::get(&partial_url).send(&service).await;
    assert_eq!(partial.status_code, Some(StatusCode::FOUND));
    let location = url::Url::parse(&location_header(&partial).unwrap()).unwrap();
    assert_eq!(
        location
            .query_pairs()
            .find(|(key, _)| key == "error")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("invalid_scope")
    );

    assert!(!enabled_html.contains("value=\"computer:launch\""));
}

#[tokio::test]
async fn bridge_authorize_picker_respects_explicit_requested_scope_and_launch_dependency() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "request-scope-shared-key";
    let (client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://scope.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "scope-runner",
        "scope-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let launch_without_read = valid_bridge_authorize_url(
        &client,
        "https://scope.example/callback",
        "runtime:read computer:launch computer:pointer_control",
    );
    let mut resp = TestClient::get(&launch_without_read).send(&service).await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let html = resp.take_string().await.unwrap_or_default();
    assert!(html.contains("value=\"launch\" disabled>"));
    assert!(html.contains("value=\"pointer\" disabled>"));
    assert!(html.contains("value=\"display\" disabled>"));
    assert!(html.contains("Not requested by this OAuth authorization request"));

    let launch_with_read = valid_bridge_authorize_url(
        &client,
        "https://scope.example/callback",
        "runtime:read computer:read computer:launch",
    );
    let mut resp = TestClient::get(&launch_with_read).send(&service).await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let html = resp.take_string().await.unwrap_or_default();
    assert!(html.contains("value=\"launch\">"));
    assert!(!html.contains("value=\"launch\" disabled>"));

    let before = auth_code_count(&db);
    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://scope.example/callback"),
        ("scope", "computer:launch"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("computer_permission", "launch"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    assert_eq!(auth_code_count(&db), before);

    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://scope.example/callback"),
        ("scope", "computer:read computer:launch"),
        ("state", "state-2"),
        ("code_challenge", "challenge-2"),
        ("code_challenge_method", "S256"),
        ("computer_permission", "launch"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let code = url::Url::parse(&location_header(&resp).unwrap())
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        auth_code_by_plaintext(&db, &code).scopes,
        "computer:read computer:launch"
    );
}

#[tokio::test]
async fn bridge_authorize_omitted_scope_grants_baseline_but_not_optional_permissions() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "omitted-scope-shared-key";
    let (client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://omitted.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "omitted-runner",
        "omitted-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));
    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://omitted.example/callback"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let location = location_header(&resp).unwrap();
    let parsed = url::Url::parse(&location).unwrap();
    let code = parsed
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    let record = auth_code_by_plaintext(&db, &code);
    assert_eq!(record.scopes, bridge_oauth_scopes().join(" "));
    for optional in [
        "computer:launch",
        "computer:display_read",
        "computer:pointer_control",
        "computer:clipboard_read",
        "computer:clipboard_write",
    ] {
        assert!(!record
            .scopes
            .split_whitespace()
            .any(|scope| scope == optional));
    }
}

#[tokio::test]
async fn bridge_authorize_permission_ids_are_closed_and_server_side_bundles_are_exact() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "permission-bundle-shared-key";
    let (client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://bundle.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "bundle-runner",
        "bundle-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let before = auth_code_count(&db);
    for tampered in [
        "computer:launch",
        "admin",
        "account:manage",
        "future_permission",
    ] {
        let body = form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://bundle.example/callback"),
            ("scope", &bridge_oauth_computer_enabled_scopes().join(" ")),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("computer_permission", tampered),
            ("shared_key", shared_key),
        ]);
        let mut resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(
            resp.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{tampered}"
        );
        assert_no_location(&resp);
        let html = resp.take_string().await.unwrap_or_default();
        assert!(
            !html.contains(shared_key),
            "shared key echoed for {tampered}"
        );
        assert_eq!(auth_code_count(&db), before, "{tampered}");
    }

    for (permission, requested, expected) in [
        (
            "launch",
            "runtime:read computer:read computer:launch offline_access",
            "runtime:read computer:read computer:launch offline_access",
        ),
        (
            "display",
            "runtime:read computer:read computer:display_read",
            "runtime:read computer:read computer:display_read",
        ),
        (
            "pointer",
            "computer:read computer:control computer:display_read computer:pointer_control",
            "computer:read computer:control computer:display_read computer:pointer_control",
        ),
        (
            "clipboard_read",
            "computer:read computer:clipboard_read computer:clipboard_write",
            "computer:read computer:clipboard_read",
        ),
        (
            "clipboard_write",
            "computer:control computer:clipboard_read computer:clipboard_write",
            "computer:control computer:clipboard_write",
        ),
    ] {
        let body = form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://bundle.example/callback"),
            ("scope", requested),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("computer_permission", permission),
            ("shared_key", shared_key),
        ]);
        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND), "{permission}");
        let location = location_header(&resp).unwrap();
        let parsed = url::Url::parse(&location).unwrap();
        let code = parsed
            .query_pairs()
            .find(|(key, _)| key == "code")
            .unwrap()
            .1
            .into_owned();
        assert_eq!(
            auth_code_by_plaintext(&db, &code).scopes,
            expected,
            "{permission}"
        );
    }
}

#[tokio::test]
async fn bridge_picker_requires_capabilities_on_one_same_runner_and_rechecks_post() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "same-runner-shared-key";
    let (client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://same-runner.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "display-only-runner",
        "display-only-instance",
        crate::shell_protocol::ShellClientCapabilities {
            computer_display_observe: true,
            ..Default::default()
        },
    )
    .await;
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "pointer-only-runner",
        "pointer-only-instance",
        crate::shell_protocol::ShellClientCapabilities {
            computer_pointer_control: true,
            ..Default::default()
        },
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry.clone(),
    ));
    let requested = "runtime:read computer:read computer:control computer:display_read computer:pointer_control";
    let url =
        valid_bridge_authorize_url(&client, "https://same-runner.example/callback", requested);
    let mut get = TestClient::get(&url).send(&service).await;
    assert_eq!(get.status_code, Some(StatusCode::OK));
    let html = get.take_string().await.unwrap_or_default();
    assert!(html.contains("value=\"display\">"));
    assert!(html.contains("value=\"pointer\" disabled>"));

    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "display-only-runner",
        "display-only-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let mut get = TestClient::get(&url).send(&service).await;
    assert_eq!(get.status_code, Some(StatusCode::OK));
    let html = get.take_string().await.unwrap_or_default();
    assert!(html.contains("value=\"pointer\">"));

    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "display-only-runner",
        "display-only-instance",
        crate::shell_protocol::ShellClientCapabilities::default(),
    )
    .await;
    let before = auth_code_count(&db);
    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://same-runner.example/callback"),
        ("scope", requested),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("computer_permission", "pointer"),
        ("shared_key", shared_key),
    ]);
    let mut post = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(post.status_code, Some(StatusCode::BAD_REQUEST));
    assert_eq!(auth_code_count(&db), before);
    let html = post.take_string().await.unwrap_or_default();
    assert!(html.contains("capability changed"));
    assert!(!html.contains(shared_key));
}

#[tokio::test]
async fn legacy_non_shared_key_owned_client_cannot_elevate_through_picker() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "legacy-owner");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://legacy.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_bridge_authorize_url(
        &client,
        "https://legacy.example/callback",
        "runtime:read computer:launch",
    );
    let before = auth_code_count(&db);
    let resp = TestClient::get(&url).send(&service).await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before);
    let location = location_header(&resp).unwrap();
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(
        params.get("error").map(String::as_str),
        Some("invalid_scope")
    );
}

#[tokio::test]
async fn bridge_authorize_post_rejects_empty_or_managed_key_without_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));

    for submitted in ["   ", "wc_pat_not_a_shared_key"] {
        let body = bridge_form_body(
            &client,
            "https://example.com/callback",
            "runtime:read",
            submitted,
        );
        let before = auth_code_count(&db);
        let mut resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
        let text = resp.take_string().await.unwrap_or_default();
        let trimmed = submitted.trim();
        if !trimmed.is_empty() {
            assert!(!text.contains(trimmed));
        }
    }
}

#[tokio::test]
async fn bridge_authorize_post_revalidates_hidden_fields_without_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));

    let direct_error_cases = [
        (
            "missing bridge hidden field",
            form_body(&[
                ("response_type", "code"),
                ("client_id", &client.client_id),
                ("redirect_uri", "https://example.com/callback"),
                ("scope", "runtime:read"),
                ("state", "state-1"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "S256"),
                ("shared_key", "shared-secret"),
            ]),
        ),
        (
            "missing response_type",
            form_body(&[
                ("bridge", "shared_key"),
                ("client_id", &client.client_id),
                ("redirect_uri", "https://example.com/callback"),
                ("scope", "runtime:read"),
                ("state", "state-1"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "S256"),
                ("shared_key", "shared-secret"),
            ]),
        ),
        (
            "tampered redirect_uri",
            form_body(&[
                ("bridge", "shared_key"),
                ("response_type", "code"),
                ("client_id", &client.client_id),
                ("redirect_uri", "https://attacker.example/callback"),
                ("scope", "runtime:read"),
                ("state", "state-1"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "S256"),
                ("shared_key", "shared-secret"),
            ]),
        ),
        (
            "tampered client_id",
            form_body(&[
                ("bridge", "shared_key"),
                ("response_type", "code"),
                ("client_id", "wc_client_missing"),
                ("redirect_uri", "https://example.com/callback"),
                ("scope", "runtime:read"),
                ("state", "state-1"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "S256"),
                ("shared_key", "shared-secret"),
            ]),
        ),
    ];

    for (name, body) in direct_error_cases {
        let before = auth_code_count(&db);
        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST), "{name}");
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before, "{name}");
    }

    let unsupported_response_type = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "token"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("shared_key", "shared-secret"),
    ]);
    let before = auth_code_count(&db);
    let resp = post_form(
        "http://localhost/oauth/authorize/bridge",
        unsupported_response_type,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before);
    let location = location_header(&resp).expect("unsupported response_type redirect");
    let parsed = url::Url::parse(&location).unwrap();
    assert_eq!(
        parsed.as_str().split('?').next(),
        Some("https://example.com/callback")
    );
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(
        params.get("error").map(String::as_str),
        Some("unsupported_response_type")
    );
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    assert!(!params.contains_key("code"));
}

#[tokio::test]
async fn bridge_authorize_valid_shared_key_creates_shared_key_code() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let shared_key = "shared-secret-value";
    let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
    let body = bridge_form_body(
        &client,
        "https://example.com/callback",
        "runtime:read project:read",
        shared_key,
    );
    let before = auth_code_count(&db);

    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before + 1);
    let location = location_header(&resp).expect("success redirect");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    let code = params.get("code").expect("code");
    let record = auth_code_by_plaintext(&db, code);
    assert_eq!(record.subject_kind, "shared_key");
    assert_eq!(record.subject_id, expected_hash);
    assert_eq!(record.user_id, None);
    assert_eq!(
        record.shared_key_hash.as_deref(),
        Some(record.subject_id.as_str())
    );
    assert_eq!(record.scopes, "runtime:read project:read");
    assert_ne!(record.code_hash, *code);

    let leaked: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM oauth_authorization_codes
                 WHERE code_hash LIKE ?1 OR client_id LIKE ?1 OR subject_id LIKE ?1
                    OR COALESCE(user_id, '') LIKE ?1 OR redirect_uri LIKE ?1
                    OR scopes LIKE ?1 OR COALESCE(code_challenge, '') LIKE ?1
                    OR COALESCE(code_challenge_method, '') LIKE ?1
                    OR COALESCE(resource, '') LIKE ?1 OR COALESCE(shared_key_hash, '') LIKE ?1",
            [format!("%{}%", shared_key)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0, "plaintext shared key must not be stored");
}

#[tokio::test]
async fn bridge_authorize_code_exchanges_to_shared_key_tokens_and_verifies() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let verifier = "bridge-code-verifier";
    let challenge = pkce_s256_challenge(verifier);
    let shared_key = "bridge-shared-secret";
    let (client, secret) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://example.com/callback",
        "runtime:read",
    );
    let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner(&registry, shared_key).await;
    let service = Service::new(build_router_with_session_and_registry(
        config.clone(),
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));
    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let location = location_header(&resp).expect("success redirect");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    let code = params.get("code").expect("code").clone();

    let exchange_body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", verifier),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", exchange_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();
    assert_eq!(json["scope"], "runtime:read");
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            "shared_key".to_string(),
            expected_hash.clone(),
            None,
            Some(expected_hash.clone())
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            "shared_key".to_string(),
            expected_hash.clone(),
            None,
            Some(expected_hash.clone())
        )
    );

    let refresh_body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut refreshed = post_form("http://localhost/oauth/token", refresh_body)
        .send(&service)
        .await;
    assert_eq!(refreshed.status_code, Some(StatusCode::OK));
    let refreshed_json: serde_json::Value = refreshed.take_json().await.unwrap();
    let refreshed_access = refreshed_json["access_token"].as_str().unwrap();
    let refreshed_refresh = refreshed_json["refresh_token"].as_str().unwrap();
    assert_eq!(
        access_token_subject_by_plaintext(&db, refreshed_access),
        (
            "shared_key".to_string(),
            expected_hash.clone(),
            None,
            Some(expected_hash.clone())
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refreshed_refresh),
        (
            "shared_key".to_string(),
            expected_hash.clone(),
            None,
            Some(expected_hash.clone())
        )
    );

    let ctx = OAuth2Verifier
        .verify(config.as_ref(), Some(&db), access_token)
        .await
        .unwrap()
        .expect("bridge access token should verify");
    assert_eq!(ctx.kind, AuthKind::OAuth2Token);
    assert_eq!(ctx.user_id, None);
    assert_eq!(ctx.token_kind.as_deref(), Some("oauth2_shared_key"));
    assert_eq!(ctx.shared_key_hash.as_deref(), Some(expected_hash.as_str()));
    assert!(ctx.has_scope(crate::auth::SCOPE_RUNTIME_READ));
    assert!(!ctx.has_scope(crate::auth::SCOPE_PROJECT_WRITE));
    assert!(!ctx.has_scope(crate::auth::SCOPE_ACCOUNT_MANAGE));

    let mut resp = TestClient::post("http://localhost/api/oauth/clients/list")
        .add_header("authorization", format!("Bearer {}", access_token), true)
        .body("{}")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"], "insufficient_scope");
}

#[tokio::test]
async fn selected_launch_scope_survives_code_access_and_refresh_rotation_without_expansion() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let verifier = "computer-launch-code-verifier";
    let challenge = pkce_s256_challenge(verifier);
    let shared_key = "computer-launch-shared-key";
    let (client, secret) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://launch.example/callback",
        &bridge_oauth_computer_enabled_scopes().join(" "),
    );
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner_with_capabilities(
        &registry,
        shared_key,
        "launch-runner",
        "launch-instance",
        all_optional_computer_capabilities(),
    )
    .await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));
    let granted = "runtime:read computer:read computer:control computer:launch offline_access";
    let body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://launch.example/callback"),
        ("scope", granted),
        ("state", "launch-state"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("computer_permission", "launch"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let location = location_header(&resp).unwrap();
    let parsed = url::Url::parse(&location).unwrap();
    let code = parsed
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(auth_code_by_plaintext(&db, &code).scopes, granted);

    let exchange_body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://launch.example/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", verifier),
    ]);
    let mut exchange = post_form("http://localhost/oauth/token", exchange_body)
        .send(&service)
        .await;
    assert_eq!(exchange.status_code, Some(StatusCode::OK));
    let exchanged: serde_json::Value = exchange.take_json().await.unwrap();
    assert_eq!(exchanged["scope"], granted);
    let access = exchanged["access_token"].as_str().unwrap();
    let refresh = exchanged["refresh_token"].as_str().unwrap();
    let access_scopes: String = db
        .conn_for_tests()
        .query_row(
            "SELECT scopes FROM oauth_access_tokens WHERE token_hash = ?1",
            [&hash_token(access)],
            |row| row.get(0),
        )
        .unwrap();
    let refresh_scopes: String = db
        .conn_for_tests()
        .query_row(
            "SELECT scopes FROM oauth_refresh_tokens WHERE token_hash = ?1",
            [&hash_token(refresh)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(access_scopes, granted);
    assert_eq!(refresh_scopes, granted);

    let refresh_body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut refreshed = post_form("http://localhost/oauth/token", refresh_body)
        .send(&service)
        .await;
    assert_eq!(refreshed.status_code, Some(StatusCode::OK));
    let refreshed_json: serde_json::Value = refreshed.take_json().await.unwrap();
    assert_eq!(refreshed_json["scope"], granted);
    let rotated_access = refreshed_json["access_token"].as_str().unwrap();
    let rotated_refresh = refreshed_json["refresh_token"].as_str().unwrap();
    for (table, token) in [
        ("oauth_access_tokens", rotated_access),
        ("oauth_refresh_tokens", rotated_refresh),
    ] {
        let scopes: String = db
            .conn_for_tests()
            .query_row(
                &format!("SELECT scopes FROM {table} WHERE token_hash = ?1"),
                [&hash_token(token)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(scopes, granted, "{table}");
        assert!(!scopes.contains("computer:display_read"));
        assert!(!scopes.contains("computer:pointer_control"));
        assert!(!scopes.contains("computer:clipboard_read"));
        assert!(!scopes.contains("computer:clipboard_write"));
    }
}

#[tokio::test]
async fn explicit_computer_opt_in_expands_existing_client_and_revokes_existing_grants_only_on_change(
) {
    let env = crate::auth::AuthEnvGuard::new();
    env.enable_direct_shared_key();
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let shared_key = "scope-expansion-shared-key";
    let (client, _) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://expand.example/callback",
        "runtime:read project:read",
    );
    let user = seed_user(&db, "grant-holder");
    let (access_record, _) = seed_access_token(&db, &client, &user, "runtime:read");
    let (refresh_record, _) = seed_refresh_token(&db, &client, &user, "runtime:read");
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner(&registry, shared_key).await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));
    let code_body = bridge_form_body(
        &client,
        "https://expand.example/callback",
        "runtime:read",
        shared_key,
    );
    let code_resp = post_form("http://localhost/oauth/authorize/bridge", code_body)
        .send(&service)
        .await;
    assert_eq!(code_resp.status_code, Some(StatusCode::FOUND));
    let code_location = location_header(&code_resp).unwrap();
    let code = url::Url::parse(&code_location)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    let code_record = auth_code_by_plaintext(&db, &code);

    let baseline_body = serde_json::json!({
        "redirect_uri": "https://expand.example/callback",
        "client_id": client.client_id,
        "previous_allowed_scopes": ["runtime:read", "project:read"],
        "computer_permissions": false
    });
    let mut baseline = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&baseline_body)
        .send(&service)
        .await;
    assert_eq!(baseline.status_code, Some(StatusCode::OK));
    let baseline_json: serde_json::Value = baseline.take_json().await.unwrap();
    assert_eq!(baseline_json["scope_ceiling_changed"], false);
    for (table, id) in [
        ("oauth_access_tokens", access_record.id.as_str()),
        ("oauth_refresh_tokens", refresh_record.id.as_str()),
        ("oauth_authorization_codes", code_record.id.as_str()),
    ] {
        let revoked_at: Option<i64> = db
            .conn_for_tests()
            .query_row(
                &format!("SELECT revoked_at FROM {table} WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(revoked_at.is_none(), "no-op reconnect revoked {table}");
    }

    let mut elevated = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://expand.example/callback",
            "client_id": client.client_id,
            "previous_allowed_scopes": ["runtime:read", "project:read"],
            "computer_permissions": true
        }))
        .send(&service)
        .await;
    assert_eq!(elevated.status_code, Some(StatusCode::OK));
    let elevated_json: serde_json::Value = elevated.take_json().await.unwrap();
    assert_eq!(elevated_json["scope_ceiling_changed"], true);
    assert_eq!(
        elevated_json["client"]["allowed_scopes"],
        serde_json::json!([
            "runtime:read",
            "project:read",
            "computer:launch",
            "computer:display_read",
            "computer:pointer_control",
            "computer:clipboard_read",
            "computer:clipboard_write"
        ])
    );
    for restored in [
        "project:write",
        "job:run",
        "computer:read",
        "computer:control",
    ] {
        assert!(!elevated_json["client"]["allowed_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|scope| scope == restored));
    }
    for (table, id) in [
        ("oauth_access_tokens", access_record.id.as_str()),
        ("oauth_refresh_tokens", refresh_record.id.as_str()),
        ("oauth_authorization_codes", code_record.id.as_str()),
    ] {
        let revoked_at: Option<i64> = db
            .conn_for_tests()
            .query_row(
                &format!("SELECT revoked_at FROM {table} WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(revoked_at.is_some(), "elevation failed to revoke {table}");
    }
    // A subsequent explicit Computer-enabled reconnect is a semantic no-op and
    // must preserve newly issued grants.
    let elevated_client = db
        .get_oauth_client_by_client_id(&client.client_id)
        .unwrap()
        .unwrap();
    let (noop_access, _) = seed_access_token(&db, &elevated_client, &user, "runtime:read");
    let (noop_refresh, _) = seed_refresh_token(&db, &elevated_client, &user, "runtime:read");
    let noop_code_body = bridge_form_body(
        &elevated_client,
        "https://expand.example/callback",
        "runtime:read",
        shared_key,
    );
    let noop_code_resp = post_form("http://localhost/oauth/authorize/bridge", noop_code_body)
        .send(&service)
        .await;
    assert_eq!(noop_code_resp.status_code, Some(StatusCode::FOUND));
    let noop_code = url::Url::parse(&location_header(&noop_code_resp).unwrap())
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    let noop_code_record = auth_code_by_plaintext(&db, &noop_code);

    let mut noop = TestClient::post("http://localhost/api/oauth/shared-key-client/provision")
        .add_header("authorization", format!("Bearer {shared_key}"), true)
        .json(&serde_json::json!({
            "redirect_uri": "https://expand.example/callback",
            "client_id": client.client_id,
            "previous_allowed_scopes": [
                "runtime:read",
                "project:read",
                "computer:launch",
                "computer:display_read",
                "computer:pointer_control",
                "computer:clipboard_read",
                "computer:clipboard_write"
            ],
            "computer_permissions": true
        }))
        .send(&service)
        .await;
    assert_eq!(noop.status_code, Some(StatusCode::OK));
    let noop_json: serde_json::Value = noop.take_json().await.unwrap();
    assert_eq!(noop_json["scope_ceiling_changed"], false);
    for (table, id) in [
        ("oauth_access_tokens", noop_access.id.as_str()),
        ("oauth_refresh_tokens", noop_refresh.id.as_str()),
        ("oauth_authorization_codes", noop_code_record.id.as_str()),
    ] {
        let revoked_at: Option<i64> = db
            .conn_for_tests()
            .query_row(
                &format!("SELECT revoked_at FROM {table} WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            revoked_at.is_none(),
            "Computer-enabled no-op revoked {table}"
        );
    }
}

#[tokio::test]
async fn bridge_issued_access_token_is_rejected_on_agent_path_without_updating_last_used() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let verifier = "bridge-code-verifier";
    let challenge = pkce_s256_challenge(verifier);
    let shared_key = "bridge-shared-secret";
    let (client, secret) = seed_shared_key_bridge_client(
        &db,
        shared_key,
        "https://example.com/callback",
        "runtime:read",
    );
    let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
    let registry = Arc::new(crate::ShellClientRegistry::default());
    register_shared_key_runner(&registry, shared_key).await;
    let service = Service::new(build_router_with_session_and_registry(
        config,
        db.clone(),
        Arc::new(AuthorizeSessionStore::new()),
        registry,
    ));

    let authorize_body = form_body(&[
        ("bridge", "shared_key"),
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("shared_key", shared_key),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/bridge", authorize_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let location = location_header(&resp).expect("success redirect");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    let code = params.get("code").expect("code").clone();

    let exchange_body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", verifier),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", exchange_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let (access_token_id, shared_key_hash, before_last_used): (
            String,
            Option<String>,
            Option<i64>,
        ) = db
            .conn_for_tests()
            .query_row(
                "SELECT id, shared_key_hash, last_used_at FROM oauth_access_tokens WHERE token_hash = ?1",
                [&hash_token(access_token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!(shared_key_hash.as_deref(), Some(expected_hash.as_str()));
    assert!(before_last_used.is_none(), "precondition");

    let resp = TestClient::post("http://localhost/api/shell/agent/register")
        .add_header("authorization", format!("Bearer {}", access_token), true)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));

    let after_last_used: Option<i64> = db
        .conn_for_tests()
        .query_row(
            "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
            [&access_token_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after_last_used, before_last_used);
}

#[tokio::test]
async fn bridge_authorize_rejects_denied_scopes_and_allows_project_write_job_run() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let denied_client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read project:write job:run account:manage",
    );
    let allowed_client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://allowed.example/callback",
        "runtime:read project:write job:run",
    );
    let service = Service::new(build_router(config, db.clone()));

    for scope in ["account:manage", "agent:register", "admin"] {
        let url = valid_bridge_authorize_url(&denied_client, "https://example.com/callback", scope);
        let before = auth_code_count(&db);
        let resp = TestClient::get(&url).send(&service).await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before);
        let location = location_header(&resp).expect("invalid_scope redirect");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("error").map(String::as_str),
            Some("invalid_scope")
        );
    }

    let body = bridge_form_body(
        &allowed_client,
        "https://allowed.example/callback",
        "project:write job:run",
        "shared-key-with-write-run",
    );
    let before = auth_code_count(&db);
    let resp = post_form("http://localhost/oauth/authorize/bridge", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before + 1);
    let location = location_header(&resp).expect("success redirect");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    let code = params.get("code").expect("code");
    let record = auth_code_by_plaintext(&db, code);
    assert_eq!(record.scopes, "project:write job:run");
    assert_eq!(record.subject_kind, "shared_key");
}
