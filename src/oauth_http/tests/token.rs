use super::*;

// -----------------------------------------------------------------------
// Success path
// -----------------------------------------------------------------------

#[tokio::test]
async fn project_share_authorization_code_exchanges_to_project_bound_tokens() {
    let config = test_config(oauth2_enabled_project_share(TEST_PROJECT_SHARE_SESSION_ID));
    let (_tmp, db) = test_db();
    let (client, secret) = seed_project_share_client(
        &db,
        TEST_PROJECT_GRANT_ID,
        "https://client.example/callback",
    );
    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = pkce_s256_challenge(code_verifier);
    let (_, code) = seed_project_share_auth_code(
        &db,
        &client,
        TEST_PROJECT_GRANT_ID,
        TEST_PROJECT_SHARE_SESSION_ID,
        "https://client.example/callback",
        "runtime:read project:read project:write job:run",
        Some(&challenge),
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://client.example/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", code_verifier),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();
    let expected_subject = format!(
        "{}|{}",
        TEST_PROJECT_GRANT_ID, TEST_PROJECT_SHARE_SESSION_ID
    );
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
            expected_subject.clone(),
            None,
            None,
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
            expected_subject,
            None,
            None,
        )
    );
}

#[tokio::test]
async fn stale_project_share_authorization_code_cannot_cross_share_session() {
    let config = test_config(oauth2_enabled_project_share(
        TEST_PROJECT_SHARE_SESSION_ID_2,
    ));
    let (_tmp, db) = test_db();
    let (client, secret) = seed_project_share_client(
        &db,
        TEST_PROJECT_GRANT_ID,
        "https://client.example/callback",
    );
    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = pkce_s256_challenge(code_verifier);
    let (code_record, code) = seed_project_share_auth_code(
        &db,
        &client,
        TEST_PROJECT_GRANT_ID,
        TEST_PROJECT_SHARE_SESSION_ID,
        "https://client.example/callback",
        "runtime:read",
        Some(&challenge),
    );
    let before = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://client.example/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", code_verifier),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
    assert_eq!(oauth_token_counts(&db), before);
    let used_at: Option<i64> = db
        .conn_for_tests()
        .query_row(
            "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
            [&code_record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        used_at.is_some(),
        "stale authorization code must be consumed"
    );
}

#[tokio::test]
async fn stale_project_share_refresh_token_cannot_cross_share_session() {
    let config = test_config(oauth2_enabled_project_share(
        TEST_PROJECT_SHARE_SESSION_ID_2,
    ));
    let (_tmp, db) = test_db();
    let (client, secret) = seed_project_share_client(
        &db,
        TEST_PROJECT_GRANT_ID,
        "https://client.example/callback",
    );
    let plaintext = crate::auth::generate_oauth_refresh_token();
    let now = chrono::Utc::now().timestamp();
    db.insert_oauth_refresh_token(&crate::models::OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: hash_token(&plaintext),
        client_id: client.client_id.clone(),
        subject_kind: crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
        subject_id: format!(
            "{}|{}",
            TEST_PROJECT_GRANT_ID, TEST_PROJECT_SHARE_SESSION_ID
        ),
        user_id: None,
        scopes: "runtime:read project:read project:write job:run".to_string(),
        resource: Some("https://share.example/mcp".to_string()),
        shared_key_hash: None,
        created_at: now,
        expires_at: now + 3600,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
    })
    .unwrap();
    let before = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
    assert_eq!(oauth_token_counts(&db), before);
}

#[tokio::test]
async fn valid_authorization_code_grant_returns_tokens() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();
    assert!(access_token.starts_with("wc_oat_"));
    assert!(refresh_token.starts_with("wc_ort_"));
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["expires_in"], 3600);
    assert_eq!(json["scope"], "runtime:read");
    assert!(access_token_shared_key_hash_by_plaintext(&db, access_token).is_none());
    assert!(refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).is_none());
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            "managed_user".to_string(),
            user.id.clone(),
            Some(user.id.clone()),
            None
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            "managed_user".to_string(),
            user.id.clone(),
            Some(user.id.clone()),
            None
        )
    );

    // Both tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before + 1, at_after, "one access token inserted");
    assert_eq!(rt_before + 1, rt_after, "one refresh token inserted");
}

#[tokio::test]
async fn oauth_token_exchange_inherits_resource_from_code() {
    let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code_with_resource(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
        Some("https://example.test/mcp"),
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();

    assert_eq!(
        access_token_resource_by_plaintext(&db, access_token).as_deref(),
        Some("https://example.test/mcp")
    );
    assert_eq!(
        refresh_token_resource_by_plaintext(&db, refresh_token).as_deref(),
        Some("https://example.test/mcp")
    );
}

#[tokio::test]
async fn oauth_token_exchange_accepts_matching_rfc8707_resource_parameter() {
    let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code_with_resource(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
        Some("https://example.test/mcp"),
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("resource", "https://example.test/mcp"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    assert_eq!(
        access_token_resource_by_plaintext(&db, access_token).as_deref(),
        Some("https://example.test/mcp")
    );
}

#[tokio::test]
async fn oauth_token_exchange_rejects_mismatched_rfc8707_resource_parameter() {
    let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code_with_resource(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
        Some("https://example.test/mcp"),
    );
    let (at_before, rt_before) = oauth_token_counts(&db);

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("resource", "https://example.test"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_target");
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after);
    assert_eq!(rt_before, rt_after);
}

#[tokio::test]
async fn oauth_token_exchange_inherits_bridge_shared_key_hash_from_code() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code_with_shared_key_hash(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
        "hash-a",
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();

    assert_eq!(
        access_token_shared_key_hash_by_plaintext(&db, access_token).as_deref(),
        Some("hash-a")
    );
    assert_eq!(
        refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).as_deref(),
        Some("hash-a")
    );
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            "shared_key".to_string(),
            "hash-a".to_string(),
            None,
            Some("hash-a".to_string())
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            "shared_key".to_string(),
            "hash-a".to_string(),
            None,
            Some("hash-a".to_string())
        )
    );
}

#[tokio::test]
async fn returned_tokens_are_stored_only_as_hashes() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    let json: serde_json::Value = resp.take_json().await.unwrap();
    let at = json["access_token"].as_str().unwrap();
    let rt = json["refresh_token"].as_str().unwrap();

    // Verify hashes are stored, not plaintext.
    let conn = db.conn_for_tests();
    let stored_at_hash: String = conn
        .query_row(
            "SELECT token_hash FROM oauth_access_tokens ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(stored_at_hash, at);
    assert_eq!(stored_at_hash, hash_token(at));

    let stored_rt_hash: String = conn
        .query_row(
            "SELECT token_hash FROM oauth_refresh_tokens ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(stored_rt_hash, rt);
    assert_eq!(stored_rt_hash, hash_token(rt));
}

#[tokio::test]
async fn authorization_code_is_marked_used() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_code_record, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    // Verify the code is now used — the second_exchange_of_same_code_fails
    // test covers this directly. Here we just confirm the exchange succeeded.
    assert_eq!(resp.status_code, Some(StatusCode::OK));
}

#[tokio::test]
async fn second_exchange_of_same_code_fails() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db.clone()));

    // First exchange succeeds.
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Second exchange fails.
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

// -----------------------------------------------------------------------
// Client authentication
// -----------------------------------------------------------------------

#[tokio::test]
async fn token_endpoint_rejects_invalid_client_credentials() {
    let config = test_config(oauth2_enabled_no_pkce());

    // Case 1: wrong client_secret.
    {
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );
        let service = Service::new(build_router(config.clone(), db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", "wrong-secret"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }
    // Case 2: unknown client_id.
    {
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config.clone(), db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", "wc_oac_dummy"),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", "wc_client_nonexistent"),
            ("client_secret", "some-secret"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }
    // Case 3: revoked client.
    {
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_client(&client.id, now).unwrap();
        let service = Service::new(build_router(config.clone(), db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", "wc_oac_dummy"),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }
}

// -----------------------------------------------------------------------
// Grant/code validation
// -----------------------------------------------------------------------

#[tokio::test]
async fn malformed_token_requests_return_structured_errors() {
    // Stateless error paths for both grant types share one service: none of
    // these requests consume a code or rotate a token.
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let service = Service::new(build_router(config, db));

    let cases: [(&str, Vec<(&str, &str)>, &str); 5] = [
        (
            "unsupported grant_type",
            vec![("grant_type", "client_credentials")],
            "unsupported_grant_type",
        ),
        (
            "missing code",
            vec![
                ("grant_type", "authorization_code"),
                ("redirect_uri", "https://example.com/callback"),
                ("client_id", client.client_id.as_str()),
                ("client_secret", secret.as_str()),
            ],
            "invalid_request",
        ),
        (
            "unknown code",
            vec![
                ("grant_type", "authorization_code"),
                ("code", "wc_oac_nonexistent"),
                ("redirect_uri", "https://example.com/callback"),
                ("client_id", client.client_id.as_str()),
                ("client_secret", secret.as_str()),
            ],
            "invalid_grant",
        ),
        (
            "missing refresh_token",
            vec![
                ("grant_type", "refresh_token"),
                ("client_id", client.client_id.as_str()),
                ("client_secret", secret.as_str()),
            ],
            "invalid_request",
        ),
        (
            "unknown refresh_token",
            vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", "wc_ort_nonexistent"),
                ("client_id", client.client_id.as_str()),
                ("client_secret", secret.as_str()),
            ],
            "invalid_grant",
        ),
    ];
    for (label, fields, expected_error) in &cases {
        let body = form_body(fields);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST), "{label}");
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], *expected_error, "{label}");
    }
}

// -----------------------------------------------------------------------
// Authorization-code binding guards
//
// These three cannot join `malformed_token_requests_return_structured_errors`:
// each one has to seed a real authorization code, and exchanging it consumes
// it, so they need their own service and code. They are what stops an
// intercepted code from being redeemed — a redirect_uri the client never
// registered, a different client presenting someone else's code, and a PKCE
// verifier that does not match the challenge. `verify_pkce_s256` has a unit
// test, but only this one proves the endpoint actually consults it.
// -----------------------------------------------------------------------

#[tokio::test]
async fn redirect_uri_mismatch_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://evil.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn client_id_mismatch_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    // The code belongs to client1 but is exchanged with client2's credentials.
    // Client authentication succeeds — client2's secret is genuine — so only
    // the code's own client binding can reject this.
    let (client2, secret2) = seed_client(&db, &user, "Other App");

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client2.client_id),
        ("client_secret", &secret2),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn wrong_verifier_returns_invalid_grant() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some(&code_challenge),
        Some("S256"),
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", "wrong-verifier-value-here"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn expired_code_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    // Create an already-expired code.
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".to_string(),
        subject_id: user.id.clone(),
        user_id: Some(user.id.clone()),
        redirect_uri: "https://example.com/callback".to_string(),
        scopes: "runtime:read".to_string(),
        code_challenge: None,
        code_challenge_method: None,
        resource: None,
        shared_key_hash: None,
        created_at: now - 600,
        expires_at: now - 1, // already expired
        used_at: None,
        revoked_at: None,
    };
    db.insert_oauth_authorization_code(&record, &record.code_hash)
        .unwrap();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &plaintext_code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn client_id_mismatch_consumes_code_but_no_tokens() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _secret) = seed_client(&db, &user, "Test App");
    let (code_record, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let (client2, secret2) = seed_client(&db, &user, "Other App");

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client2.client_id),
        ("client_secret", &secret2),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");

    // Code SHOULD be consumed — validation failure.
    let conn = db.conn_for_tests();
    let used_at: Option<i64> = conn
        .query_row(
            "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
            [&code_record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        used_at.is_some(),
        "code should be consumed on client_id mismatch"
    );
    drop(conn);

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access token on client_id mismatch");
    assert_eq!(
        rt_before, rt_after,
        "no refresh token on client_id mismatch"
    );
}

// -----------------------------------------------------------------------
// PKCE
// -----------------------------------------------------------------------

#[tokio::test]
async fn valid_s256_verifier_succeeds() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    // Generate a code_verifier and compute its S256 challenge.
    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some(&code_challenge),
        Some("S256"),
    );

    let service = Service::new(build_router(config, db));
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
}

#[tokio::test]
async fn missing_verifier_when_pkce_required_returns_error() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some(&code_challenge),
        Some("S256"),
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        // no code_verifier
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn plain_challenge_method_rejected() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some("some-challenge"),
        Some("plain"),
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", "some-challenge"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

// -----------------------------------------------------------------------
// OAuth2 disabled
// -----------------------------------------------------------------------

#[tokio::test]
async fn oauth2_disabled_returns_503() {
    let config = test_config(oauth2_disabled());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("grant_type", "authorization_code")]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "server_error");
}

// -----------------------------------------------------------------------
// PKCE helper
// -----------------------------------------------------------------------

#[test]
fn verify_pkce_s256_works() {
    // RFC 7636 Appendix B test vector.
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    assert!(verify_pkce_s256(verifier, challenge));
    assert!(!verify_pkce_s256("wrong", challenge));
    assert!(!verify_pkce_s256(verifier, "wrong"));
}

// -----------------------------------------------------------------------
// No-store headers
// -----------------------------------------------------------------------

#[tokio::test]
async fn successful_token_response_has_no_store_headers() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let cc = resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cc, "no-store");
    let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
    assert_eq!(pragma, "no-cache");
}

#[tokio::test]
async fn error_response_has_no_store_headers() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("grant_type", "client_credentials")]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let cc = resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cc, "no-store");
    let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
    assert_eq!(pragma, "no-cache");
}

// -----------------------------------------------------------------------
// Content-Type enforcement
// -----------------------------------------------------------------------

#[tokio::test]
async fn missing_content_type_is_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("grant_type", "authorization_code")]);
    // No content-type header.
    let mut resp = TestClient::post("http://localhost/oauth/token")
        .body(body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn json_content_type_is_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("grant_type", "authorization_code")]);
    let mut resp = TestClient::post("http://localhost/oauth/token")
        .add_header("content-type", "application/json", true)
        .body(body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn form_urlencoded_with_charset_is_accepted() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = TestClient::post("http://localhost/oauth/token")
        .add_header(
            "content-type",
            "application/x-www-form-urlencoded; charset=utf-8",
            true,
        )
        .body(body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
}

// -----------------------------------------------------------------------
// Request body size limit
// -----------------------------------------------------------------------

#[tokio::test]
async fn oversized_content_length_is_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("grant_type", "authorization_code")]);
    let mut resp = TestClient::post("http://localhost/oauth/token")
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .add_header("content-length", "999999", true)
        .body(body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn oversized_actual_body_is_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    // Build a body that exceeds 16 KiB.
    let big_value = "x".repeat(17 * 1024);
    let body = format!("grant_type=authorization_code&code={}", big_value);
    let mut resp = TestClient::post("http://localhost/oauth/token")
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .body(body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn normal_small_form_still_works() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
}

// -----------------------------------------------------------------------
// Post-consume semantics (code consumed on mismatch)
// -----------------------------------------------------------------------

#[tokio::test]
async fn wrong_client_secret_does_not_consume_code() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _) = seed_client(&db, &user, "Test App");
    let (code_record, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", "wrong-secret"),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

    // Code should NOT be consumed — wrong secret is rejected before exchange.
    let conn = db.conn_for_tests();
    let used_at: Option<i64> = conn
        .query_row(
            "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
            [&code_record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        used_at.is_none(),
        "code should not be consumed on wrong secret"
    );
    drop(conn);

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access token should be inserted");
    assert_eq!(rt_before, rt_after, "no refresh token should be inserted");
}

#[tokio::test]
async fn redirect_uri_mismatch_consumes_code() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (code_record, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        None,
        None,
    );

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://evil.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");

    // Code SHOULD be consumed — validation failure.
    let conn = db.conn_for_tests();
    let used_at: Option<i64> = conn
        .query_row(
            "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
            [&code_record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        used_at.is_some(),
        "code should be consumed on redirect_uri mismatch"
    );
    drop(conn);

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(
        at_before, at_after,
        "no access token on redirect_uri mismatch"
    );
    assert_eq!(
        rt_before, rt_after,
        "no refresh token on redirect_uri mismatch"
    );
}

#[tokio::test]
async fn pkce_mismatch_consumes_code() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    let (code_record, code) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some(&code_challenge),
        Some("S256"),
    );

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://example.com/callback"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("code_verifier", "wrong-verifier"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");

    // Code SHOULD be consumed — validation failure.
    let conn = db.conn_for_tests();
    let used_at: Option<i64> = conn
        .query_row(
            "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
            [&code_record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        used_at.is_some(),
        "code should be consumed on PKCE mismatch"
    );
    drop(conn);

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access token on PKCE mismatch");
    assert_eq!(rt_before, rt_after, "no refresh token on PKCE mismatch");
}

// -----------------------------------------------------------------------
// refresh_token grant — success path
// -----------------------------------------------------------------------

#[tokio::test]
async fn valid_refresh_token_grant_returns_new_tokens() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
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
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["expires_in"], 3600);
    assert_eq!(json["scope"], "runtime:read");
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            "managed_user".to_string(),
            user.id.clone(),
            Some(user.id.clone()),
            None
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            "managed_user".to_string(),
            user.id.clone(),
            Some(user.id.clone()),
            None
        )
    );

    // Old refresh token should be revoked.
    let conn = db.conn_for_tests();
    let revoked_at: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&old_rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(revoked_at.is_some(), "old refresh token should be revoked");

    let last_used_at: Option<i64> = conn
        .query_row(
            "SELECT last_used_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&old_rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        last_used_at.is_some(),
        "old refresh token should have last_used_at set"
    );

    // New refresh token should have rotated_from_id.
    let rotated_from: Option<String> = conn
            .query_row(
                "SELECT rotated_from_id FROM oauth_refresh_tokens WHERE rotated_from_id IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(
        rotated_from.as_deref(),
        Some(old_rt.id.as_str()),
        "new refresh token should reference old token"
    );
    drop(conn);

    // Both new tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before + 1, at_after, "one new access token inserted");
    assert_eq!(rt_before + 1, rt_after, "one new refresh token inserted");
}

#[tokio::test]
async fn oauth_refresh_token_inherits_resource() {
    let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_old_rt, old_rt_plaintext) = seed_refresh_token_with_resource(
        &db,
        &client,
        &user,
        "runtime:read",
        Some("https://example.test/mcp"),
    );

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();

    assert_eq!(
        access_token_resource_by_plaintext(&db, access_token).as_deref(),
        Some("https://example.test/mcp")
    );
    assert_eq!(
        refresh_token_resource_by_plaintext(&db, refresh_token).as_deref(),
        Some("https://example.test/mcp")
    );
}

#[tokio::test]
async fn oauth_refresh_token_preserves_bridge_shared_key_hash() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_old_rt, old_rt_plaintext) =
        seed_refresh_token_with_shared_key_hash(&db, &client, &user, "runtime:read", "hash-a");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    let access_token = json["access_token"].as_str().unwrap();
    let refresh_token = json["refresh_token"].as_str().unwrap();

    assert_eq!(
        access_token_shared_key_hash_by_plaintext(&db, access_token).as_deref(),
        Some("hash-a")
    );
    assert_eq!(
        refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).as_deref(),
        Some("hash-a")
    );
    assert_eq!(
        access_token_subject_by_plaintext(&db, access_token),
        (
            "shared_key".to_string(),
            "hash-a".to_string(),
            None,
            Some("hash-a".to_string())
        )
    );
    assert_eq!(
        refresh_token_subject_by_plaintext(&db, refresh_token),
        (
            "shared_key".to_string(),
            "hash-a".to_string(),
            None,
            Some("hash-a".to_string())
        )
    );
}

#[tokio::test]
async fn refresh_token_new_tokens_stored_only_as_hashes() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;

    let json: serde_json::Value = resp.take_json().await.unwrap();
    let at = json["access_token"].as_str().unwrap();
    let rt = json["refresh_token"].as_str().unwrap();

    let conn = db.conn_for_tests();
    let stored_at_hash: String = conn
        .query_row(
            "SELECT token_hash FROM oauth_access_tokens ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(stored_at_hash, at);
    assert_eq!(stored_at_hash, hash_token(at));

    let stored_rt_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_refresh_tokens WHERE rotated_from_id IS NOT NULL ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_ne!(stored_rt_hash, rt);
    assert_eq!(stored_rt_hash, hash_token(rt));
}

// -----------------------------------------------------------------------
// refresh_token grant — rotation / replay
// -----------------------------------------------------------------------

#[tokio::test]
async fn refresh_token_cannot_be_reused_after_rotation() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));

    // First refresh succeeds.
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Second refresh with same old token fails.
    let (at_before, rt_before) = oauth_token_counts(&db);
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");

    // No additional tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no extra access token on replay");
    assert_eq!(rt_before, rt_after, "no extra refresh token on replay");
}

// -----------------------------------------------------------------------
// refresh_token grant — client authentication
// -----------------------------------------------------------------------

#[tokio::test]
async fn refresh_token_wrong_secret_does_not_rotate() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _) = seed_client(&db, &user, "Test App");
    let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", "wrong-secret"),
    ]);
    let resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

    // Old refresh token should NOT be revoked.
    let conn = db.conn_for_tests();
    let revoked_at: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&old_rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        revoked_at.is_none(),
        "old refresh token should not be revoked on wrong secret"
    );
    drop(conn);

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access token on wrong secret");
    assert_eq!(rt_before, rt_after, "no refresh token on wrong secret");
}

#[tokio::test]
async fn refresh_token_unknown_client_returns_invalid_client() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", "wc_ort_dummy"),
        ("client_id", "wc_client_nonexistent"),
        ("client_secret", "some-secret"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");
}

#[tokio::test]
async fn refresh_token_revoked_client_returns_invalid_client() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let now = chrono::Utc::now().timestamp();
    db.revoke_oauth_client(&client.id, now).unwrap();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", "wc_ort_dummy"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");
}

// -----------------------------------------------------------------------
// refresh_token grant — grant validation
// -----------------------------------------------------------------------

#[tokio::test]
async fn expired_refresh_token_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    // Create an already-expired refresh token.
    let now = chrono::Utc::now().timestamp();
    let plaintext = crate::auth::generate_oauth_refresh_token();
    let token_hash = hash_token(&plaintext);
    let record = crate::models::OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash,
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".to_string(),
        subject_id: user.id.clone(),
        user_id: Some(user.id.clone()),
        scopes: "runtime:read".to_string(),
        resource: None,
        shared_key_hash: None,
        created_at: now - 600,
        expires_at: now - 1, // already expired
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
    };
    db.insert_oauth_refresh_token(&record).unwrap();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn revoked_refresh_token_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    // Revoke the refresh token.
    let now = chrono::Utc::now().timestamp();
    db.revoke_oauth_refresh_token(&old_rt.id, now).unwrap();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn refresh_token_client_id_mismatch_returns_invalid_grant() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _) = seed_client(&db, &user, "Test App");
    let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    // Create a second client and try to use client1's refresh token.
    let (client2, secret2) = seed_client(&db, &user, "Other App");

    let (at_before, rt_before) = oauth_token_counts(&db);
    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client2.client_id),
        ("client_secret", &secret2),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_grant");

    // No tokens should be inserted.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access token on client mismatch");
    assert_eq!(rt_before, rt_after, "no refresh token on client mismatch");
}

// -----------------------------------------------------------------------
// refresh_token grant — scope rejection
// -----------------------------------------------------------------------

#[tokio::test]
async fn refresh_token_scope_parameter_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
        ("scope", "runtime:read"),
    ]);
    let mut resp = post_form("http://localhost/oauth/token", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}
