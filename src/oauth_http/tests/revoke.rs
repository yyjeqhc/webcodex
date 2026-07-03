use super::*;

// -----------------------------------------------------------------------
// POST /oauth/revoke — success path
// -----------------------------------------------------------------------

fn post_revoke(url: &str, body: String) -> salvo::test::RequestBuilder {
    TestClient::post(url)
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .body(body)
}

#[tokio::test]
async fn revoke_access_token_success() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
    let (rt, _rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    // Response must not disclose token state.
    assert_eq!(json, serde_json::json!({}));

    // Access token should be revoked.
    let conn = db.conn_for_tests();
    let at_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(at_revoked.is_some(), "access token should be revoked");

    // Refresh token should NOT be affected.
    let rt_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rt_revoked.is_none(), "refresh token should not be affected");
}

#[tokio::test]
async fn revoke_refresh_token_success() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, _at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
    let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &rt_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Refresh token should be revoked.
    let conn = db.conn_for_tests();
    let rt_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rt_revoked.is_some(), "refresh token should be revoked");

    // Access token should NOT be affected.
    let at_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(at_revoked.is_none(), "access token should not be affected");
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — idempotent
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_token_is_idempotent() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));

    // First revoke.
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Second revoke — same token.
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // revoked_at should still be Some.
    let conn = db.conn_for_tests();
    let revoked_at: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(revoked_at.is_some(), "token should still be revoked");
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — unknown token
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_unknown_token_returns_200() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let (at_before, rt_before) = oauth_token_counts(&db);

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", "wc_oat_nonexistent"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // No tokens should be inserted or modified.
    let (at_after, rt_after) = oauth_token_counts(&db);
    assert_eq!(at_before, at_after, "no access tokens added");
    assert_eq!(rt_before, rt_after, "no refresh tokens added");
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — wrong client
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_token_belonging_to_other_client_returns_200_but_does_not_revoke() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client_a, _secret_a) = seed_client(&db, &user, "App A");
    let (client_b, secret_b) = seed_client(&db, &user, "App B");
    let (at, at_plaintext) = seed_access_token(&db, &client_a, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    // Client B tries to revoke client A's token.
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client_b.client_id),
        ("client_secret", &secret_b),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Client A's token should NOT be revoked.
    let conn = db.conn_for_tests();
    let revoked_at: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        revoked_at.is_none(),
        "token belonging to other client should not be revoked"
    );
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — client authentication errors
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_wrong_client_secret_returns_invalid_client_and_does_not_revoke() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _secret) = seed_client(&db, &user, "Test App");
    let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", "wrong-secret"),
    ]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");

    // Token should NOT be revoked.
    let conn = db.conn_for_tests();
    let revoked_at: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        revoked_at.is_none(),
        "token should not be revoked on wrong secret"
    );
}

#[tokio::test]
async fn revoke_unknown_client_returns_invalid_client() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("token", "wc_oat_dummy"),
        ("client_id", "wc_client_nonexistent"),
        ("client_secret", "some-secret"),
    ]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");
}

#[tokio::test]
async fn revoke_revoked_client_returns_invalid_client() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let now = chrono::Utc::now().timestamp();
    db.revoke_oauth_client(&client.id, now).unwrap();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("token", "wc_oat_dummy"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — request validation
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_missing_token_returns_invalid_request() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("client_id", &client.client_id), ("client_secret", &secret)]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn revoke_missing_client_id_returns_invalid_request() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("token", "wc_oat_dummy"), ("client_secret", "some-secret")]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn revoke_missing_client_secret_returns_invalid_client() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, _secret) = seed_client(&db, &user, "Test App");

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("token", "wc_oat_dummy"), ("client_id", &client.client_id)]);
    let mut resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_client");
}

#[tokio::test]
async fn revoke_json_content_type_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("token", "wc_oat_dummy")]);
    let mut resp = TestClient::post("http://localhost/oauth/revoke")
        .add_header("content-type", "application/json", true)
        .body(body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn revoke_missing_content_type_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[("token", "wc_oat_dummy")]);
    let mut resp = TestClient::post("http://localhost/oauth/revoke")
        .body(body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn revoke_oversized_body_rejected() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let big_value = "x".repeat(17 * 1024);
    let body = format!("token={}", big_value);
    let mut resp = TestClient::post("http://localhost/oauth/revoke")
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .body(body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    let json: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(json["error"], "invalid_request");
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — token_type_hint
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_access_token_hint_only_revokes_access_token() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
    let (rt, _rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &at_plaintext),
        ("token_type_hint", "access_token"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let conn = db.conn_for_tests();
    let at_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(at_revoked.is_some(), "access token should be revoked");

    let rt_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rt_revoked.is_none(), "refresh token should not be affected");
}

#[tokio::test]
async fn revoke_refresh_token_hint_only_revokes_refresh_token() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, _at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
    let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &rt_plaintext),
        ("token_type_hint", "refresh_token"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let conn = db.conn_for_tests();
    let rt_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rt_revoked.is_some(), "refresh token should be revoked");

    let at_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(at_revoked.is_none(), "access token should not be affected");
}

#[tokio::test]
async fn revoke_unknown_hint_attempts_both() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    // Seed a refresh token and try to revoke it with an unknown hint.
    let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &rt_plaintext),
        ("token_type_hint", "unknown_type"),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Refresh token should be revoked (both types are tried).
    let conn = db.conn_for_tests();
    let rt_revoked: Option<i64> = conn
        .query_row(
            "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
            [&rt.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        rt_revoked.is_some(),
        "refresh token should be revoked with unknown hint"
    );
}

// -----------------------------------------------------------------------
// POST /oauth/revoke — no-store headers
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_success_has_no_store_headers() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (_, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
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
async fn revoke_error_has_no_store_headers() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();

    let service = Service::new(build_router(config, db));
    let body = form_body(&[
        ("token", "wc_oat_dummy"),
        ("client_id", "wc_client_nonexistent"),
        ("client_secret", "some-secret"),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
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
// POST /oauth/revoke — last_used_at not updated
// -----------------------------------------------------------------------

#[tokio::test]
async fn revoke_does_not_update_last_used_at() {
    let config = test_config(oauth2_enabled_no_pkce());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let (client, secret) = seed_client(&db, &user, "Test App");
    let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

    let service = Service::new(build_router(config, db.clone()));
    let body = form_body(&[
        ("token", &at_plaintext),
        ("client_id", &client.client_id),
        ("client_secret", &secret),
    ]);
    let resp = post_revoke("http://localhost/oauth/revoke", body)
        .send(&service)
        .await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let conn = db.conn_for_tests();
    let last_used_at: Option<i64> = conn
        .query_row(
            "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
            [&at.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        last_used_at.is_none(),
        "revoke should not update last_used_at"
    );
}
