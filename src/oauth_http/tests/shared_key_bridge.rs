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
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Bridge App");
    let verifier = "bridge-code-verifier";
    let challenge = pkce_s256_challenge(verifier);
    let shared_key = "bridge-shared-secret";
    let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
    let service = Service::new(build_router(config.clone(), db.clone()));
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
        .add_header("authorization", &format!("Bearer {}", access_token), true)
        .body("{}")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"], "insufficient_scope");
}

#[tokio::test]
async fn bridge_issued_access_token_is_rejected_on_agent_path_without_updating_last_used() {
    let config = test_config(oauth2_enabled_bridge());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Bridge App");
    let verifier = "bridge-code-verifier";
    let challenge = pkce_s256_challenge(verifier);
    let shared_key = "bridge-shared-secret";
    let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
    let service = Service::new(build_router(config, db.clone()));

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
        .add_header("authorization", &format!("Bearer {}", access_token), true)
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
