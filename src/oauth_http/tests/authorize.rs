use super::*;

#[test]
fn parse_authorize_query_requires_response_type() {
    let err = parse_authorize_query(&authorize_query_without("response_type")).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("missing response_type")
    );
}

#[test]
fn parse_authorize_query_requires_client_id() {
    let err = parse_authorize_query(&authorize_query_without("client_id")).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("missing client_id")
    );
}

#[test]
fn parse_authorize_query_requires_redirect_uri() {
    let err = parse_authorize_query(&authorize_query_without("redirect_uri")).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("missing redirect_uri")
    );
}

#[test]
fn parse_authorize_query_requires_code_challenge() {
    let err = parse_authorize_query(&authorize_query_without("code_challenge")).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("missing code_challenge")
    );
}

#[test]
fn parse_authorize_query_requires_code_challenge_method() {
    let err = parse_authorize_query(&authorize_query_without("code_challenge_method")).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("missing code_challenge_method")
    );
}

#[test]
fn parse_authorize_query_preserves_state() {
    let parsed = parse_authorize_query(&valid_authorize_query()).unwrap();

    assert_eq!(parsed.state.as_deref(), Some("keep+this value"));
}

#[test]
fn parse_authorize_query_keeps_resource_for_later_rejection() {
    let query = format!(
        "{}&resource={}",
        valid_authorize_query(),
        urlencoding::encode("https://api.example/resource")
    );

    let parsed = parse_authorize_query(&query).unwrap();

    assert_eq!(
        parsed.resource.as_deref(),
        Some("https://api.example/resource")
    );
}

#[test]
fn validate_authorize_resource_rejects_unsafe_values() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
    for resource in [
        "",
        "   ",
        "/mcp",
        "ftp://example.test/mcp",
        "https://example.test/mcp?x=1",
        "https://example.test/mcp#frag",
        "https://evil.example",
    ] {
        assert_eq!(
            validate_authorize_resource(Some(resource), &config).unwrap_err(),
            OAuthAuthorizeError::UnsupportedResource,
            "resource should be rejected: {resource:?}"
        );
    }
}

#[test]
fn parse_authorize_query_rejects_duplicate_parameters() {
    let query = format!("{}&client_id=client-2", valid_authorize_query());
    let err = parse_authorize_query(&query).unwrap_err();

    assert_eq!(
        err,
        OAuthAuthorizeError::InvalidRequest("duplicate parameter")
    );
}
#[tokio::test]
async fn authorize_accepts_user_pat_for_code_issuance() {
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

    assert!(code.starts_with("wc_oac_"));
    let record = auth_code_by_plaintext(&db, &code);
    assert_eq!(record.shared_key_hash, None);
}

#[tokio::test]
async fn authorize_rejects_non_user_tokens_without_issuing_code() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    // Each token type must be rejected with 403, no redirect, no code issued.
    let tokens: Vec<String> = vec![
        {
            let (_record, token) = seed_access_token(&db, &client, &user, "runtime:read");
            token
        },
        seed_agent_token(&db, &user),
        seed_account_credential(&db, &user),
    ];
    for token in &tokens {
        let before = auth_code_count(&db);
        let resp = authorized_get(&url, token).send(&service).await;
        assert_eq!(
            resp.status_code,
            Some(StatusCode::FORBIDDEN),
            "token should be rejected"
        );
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before, "no code should be issued");
    }
}

#[tokio::test]
async fn authorize_oauth2_disabled_returns_404_invalid_request_without_code() {
    let config = test_config(oauth2_disabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let before = auth_code_count(&db);
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    let mut resp = authorized_get(&url, &token).send(&service).await;

    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");
    assert_eq!(body["error_description"], "OAuth2 is not enabled");
}

#[tokio::test]
async fn authorize_rejects_unknown_client_without_redirect() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", "wc_client_missing"),
        ("redirect_uri", "https://example.com/callback"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_revoked_client_without_redirect() {
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
    db.revoke_oauth_client(&client.id, chrono::Utc::now().timestamp())
        .unwrap();
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_missing_redirect_uri_without_redirect() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_empty_redirect_uri_without_redirect() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", ""),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_redirect_uri_mismatch_without_redirect() {
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
    let url = valid_authorize_url(&client, "https://attacker.example/callback");

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_empty_client_id_without_redirect() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", ""),
        ("redirect_uri", "https://example.com/callback"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_direct_400(&service, &db, &url, &token).await;
}

#[tokio::test]
async fn authorize_rejects_unsupported_response_type_with_redirect_after_client_validation() {
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
    let url = authorize_url(&[
        ("response_type", "token"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "unsupported_response_type",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_empty_response_type_with_redirect_after_client_validation() {
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
    let url = authorize_url(&[
        ("response_type", ""),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "unsupported_response_type",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn authorize_requires_pkce_s256() {
    let config = test_config(oauth2_enabled_no_pkce());
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("code_challenge", "challenge-1"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_request",
        None,
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_plain_pkce_method() {
    let config = test_config(oauth2_enabled_no_pkce());
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "plain"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_request",
        None,
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_missing_code_challenge() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", "state-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_request",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_empty_code_challenge() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", "state-1"),
        ("code_challenge", ""),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_request",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_empty_code_challenge_method() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", ""),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_request",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn authorize_rejects_invalid_scope() {
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "project:write"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_scope",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn oauth_authorize_accepts_self_resource_base() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test/"));
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
    let url = valid_authorize_url_with_resource(
        &client,
        "https://example.com/callback",
        " https://example.test/ ",
    );

    let (_resp, _location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let record = auth_code_by_plaintext(&db, &code);

    assert_eq!(record.resource.as_deref(), Some("https://example.test"));
}

#[tokio::test]
async fn oauth_authorize_accepts_self_resource_mcp_endpoint() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
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
    let url = valid_authorize_url_with_resource(
        &client,
        "https://example.com/callback",
        "https://example.test/mcp",
    );

    let (_resp, location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let record = auth_code_by_plaintext(&db, &code);

    assert!(
        location.contains("code=wc_oac_"),
        "ChatGPT MCP resource flow should return a code: {}",
        location
    );
    assert_eq!(record.resource.as_deref(), Some("https://example.test/mcp"));
}

#[tokio::test]
async fn oauth_authorize_rejects_external_resource() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
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
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("resource", "https://api.example/resource"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "invalid_target",
        Some("state-1"),
    )
    .await;
}

#[tokio::test]
async fn oauth_authorize_without_resource_still_works() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
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

    assert_eq!(record.resource, None);
}

#[tokio::test]
async fn authorize_redirect_error_appends_with_ampersand_when_redirect_uri_has_query() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let redirect_uri = "https://client.example/callback?existing=1";
    let client = seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
    let service = Service::new(build_router(config, db.clone()));
    let url = authorize_url(&[
        ("response_type", "token"),
        ("client_id", &client.client_id),
        ("redirect_uri", redirect_uri),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    let location = assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        redirect_uri,
        "unsupported_response_type",
        None,
    )
    .await;

    assert!(
        location.contains("?existing=1&error=unsupported_response_type"),
        "Location should append with &: {}",
        location
    );
    assert!(
        !location.contains("?existing=1?error="),
        "Location must not append a second ?: {}",
        location
    );
}

#[tokio::test]
async fn authorize_redirect_error_preserves_decoded_state_semantics() {
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
    let state = "a b+c&d=1/中文";
    let url = authorize_url(&[
        ("response_type", "token"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("state", state),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    assert_authorize_redirect_error(
        &service,
        &db,
        &url,
        &token,
        "https://example.com/callback",
        "unsupported_response_type",
        Some(state),
    )
    .await;
}

#[tokio::test]
async fn authorize_issues_code_and_redirects_with_state() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let before = auth_code_count(&db);
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    let (_resp, location, parsed, code) = authorize_success(&service, &db, &url, &token).await;

    assert_eq!(auth_code_count(&db), before + 1);
    assert!(location.starts_with("https://example.com/callback?code=wc_oac_"));
    assert_eq!(parsed.scheme(), "https");
    assert_eq!(parsed.host_str(), Some("example.com"));
    assert_eq!(parsed.path(), "/callback");
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(params.get("code").map(String::as_str), Some(code.as_str()));
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
}

#[tokio::test]
async fn authorize_stores_only_code_hash() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, "https://example.com/callback");

    let (_resp, _location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let record = auth_code_by_plaintext(&db, &code);

    assert_ne!(record.code_hash, code);
    assert_eq!(record.code_hash, hash_token(&code));
    assert!(
        !record.code_hash.contains(&code),
        "hash field must not contain plaintext code"
    );
}

#[tokio::test]
async fn authorize_code_contains_user_client_redirect_scope_pkce_metadata() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "project:read runtime:read",
    );
    let service = Service::new(build_router(config, db.clone()));
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "project:read runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    let (_resp, _location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let record = auth_code_by_plaintext(&db, &code);

    assert_eq!(record.client_id, client.client_id);
    assert_eq!(record.subject_kind, "managed_user");
    assert_eq!(record.subject_id, user.id);
    assert_eq!(record.user_id, Some(user.id.clone()));
    assert_eq!(record.redirect_uri, "https://example.com/callback");
    assert_eq!(record.scopes, "runtime:read project:read");
    assert_eq!(record.resource, None);
    assert_eq!(record.shared_key_hash, None);
    assert_eq!(record.code_challenge.as_deref(), Some("challenge-1"));
    assert_eq!(record.code_challenge_method.as_deref(), Some("S256"));
    assert_eq!(record.used_at, None);
    assert_eq!(record.revoked_at, None);
    assert!(record.expires_at > record.created_at);
}

#[tokio::test]
async fn authorize_success_redirect_appends_with_ampersand_when_redirect_uri_has_query() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let redirect_uri = "https://client.example/callback?existing=1";
    let client = seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
    let service = Service::new(build_router(config, db.clone()));
    let url = valid_authorize_url(&client, redirect_uri);

    let (_resp, location, parsed, _code) = authorize_success(&service, &db, &url, &token).await;
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();

    assert_eq!(params.get("existing").map(String::as_str), Some("1"));
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    assert!(
        location.contains("?existing=1&code=wc_oac_"),
        "Location should append with &: {}",
        location
    );
    assert!(
        !location.contains("?existing=1?code="),
        "Location must not append a second ?: {}",
        location
    );
}

#[tokio::test]
async fn authorize_success_does_not_return_access_or_refresh_token() {
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

    let (mut resp, location, _parsed, _code) = authorize_success(&service, &db, &url, &token).await;
    let body = resp.take_string().await.unwrap_or_default();

    assert!(!location.contains("access_token"));
    assert!(!location.contains("refresh_token"));
    assert!(!body.contains("access_token"));
    assert!(!body.contains("refresh_token"));
}

#[tokio::test]
async fn authorize_success_preserves_decoded_state_semantics() {
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
    let state = "a b+c&d=1/中文";
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", state),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ]);

    let (_resp, _location, parsed, _code) = authorize_success(&service, &db, &url, &token).await;
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(params.get("state").map(String::as_str), Some(state));
}

#[tokio::test]
async fn authorize_success_code_can_be_exchanged_for_tokens() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let (client, secret) = seed_client(&db, &user, "Test App");
    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let code_challenge = pkce_s256_challenge(code_verifier);
    let service = Service::new(build_router(config, db.clone()));
    let url = authorize_url(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://example.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", &code_challenge),
        ("code_challenge_method", "S256"),
    ]);

    let (_resp, _location, parsed, code) = authorize_success(&service, &db, &url, &token).await;
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));

    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", code_verifier),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert!(json["access_token"]
        .as_str()
        .unwrap()
        .starts_with("wc_oat_"));
    assert!(json["refresh_token"]
        .as_str()
        .unwrap()
        .starts_with("wc_ort_"));

    let record = auth_code_by_plaintext(&db, &code);
    assert!(record.used_at.is_some(), "authorization code consumed");
}
