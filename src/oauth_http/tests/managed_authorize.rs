use super::*;

#[tokio::test]
async fn oauth_authorize_without_bearer_or_session_returns_login_page() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read",
    );
    let url = valid_authorize_url(&client, "https://example.com/callback");
    let before = auth_code_count(&db);
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = TestClient::get(&url).send(&service).await;

    assert_eq!(resp.status_code, Some(StatusCode::OK));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
    let body = resp.take_string().await.unwrap_or_default();
    assert!(
        body.contains("/oauth/authorize/login"),
        "login form missing"
    );
    assert!(body.contains("name=\"token\""), "token input missing");
    // The login page must not reveal the original query token or echo
    // any secret; it only carries the return_to path.
    assert!(body.contains("return_to"));
}
// -----------------------------------------------------------------------
// Test helpers for authorize browser UX
// -----------------------------------------------------------------------

fn set_cookie_value(resp: &Response, name: &str) -> Option<String> {
    for v in resp.headers.get_all("set-cookie") {
        if let Ok(s) = v.to_str() {
            let prefix = format!("{}=", name);
            if let Some(rest) = s.strip_prefix(&prefix) {
                if let Some(val) = rest.split(';').next() {
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

fn post_form_with_cookie(url: &str, body: String, cookie: &str) -> salvo::test::RequestBuilder {
    TestClient::post(url)
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .add_header("cookie", cookie, true)
        .body(body)
}

fn return_to_for(client: &OAuthClientRecord, redirect_uri: &str) -> String {
    let params: &[(&str, &str)] = &[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("/oauth/authorize?{}", query)
}

fn consent_form_body(client: &OAuthClientRecord, redirect_uri: &str, decision: &str) -> String {
    form_body(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("decision", decision),
    ])
}

fn consent_form_body_with_resource(
    client: &OAuthClientRecord,
    redirect_uri: &str,
    decision: &str,
    resource: &str,
) -> String {
    form_body(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("resource", resource),
        ("decision", decision),
    ])
}

// -----------------------------------------------------------------------
// Authorize login / session / consent tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn oauth_authorize_login_rejects_invalid_token() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");
    let body = form_body(&[("return_to", &return_to), ("token", "wc_pat_bogus")]);

    let mut resp = post_form("http://localhost/oauth/authorize/login", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let text = resp.take_string().await.unwrap_or_default();
    assert!(text.contains("invalid token") || text.contains("required"));
    // No session cookie set on failure.
    assert!(set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).is_none());
}

#[tokio::test]
async fn oauth_authorize_login_accepts_pat_and_sets_httponly_cookie() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");
    let body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);

    let resp = post_form("http://localhost/oauth/authorize/login", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    let location = location_header(&resp).expect("redirect after login");
    assert!(location.starts_with("/oauth/authorize"));
    let cookie =
        set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie should be set");
    assert!(cookie.starts_with("wc_authsess_"));
    // Verify HttpOnly + SameSite=Lax attributes on the raw Set-Cookie.
    let raw = resp
        .headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(raw.contains("HttpOnly"), "cookie must be HttpOnly");
    assert!(raw.contains("SameSite=Lax"), "cookie must be SameSite=Lax");
}

#[tokio::test]
async fn oauth_authorize_login_rejects_oauth2_access_token() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let (_at, oauth_access_token) = seed_access_token(&db, &client, &user, "runtime:read");
    let service = Service::new(build_router(config, db.clone()));
    let return_to = return_to_for(&client, "https://example.com/callback");
    let body = form_body(&[
        ("return_to", return_to.as_str()),
        ("token", oauth_access_token.as_str()),
    ]);

    let resp = post_form("http://localhost/oauth/authorize/login", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
}

#[tokio::test]
async fn oauth_authorize_login_rejects_bootstrap_without_user_id() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");
    // Bootstrap is a valid first-party token but has no user_id, so the
    // authorize login must reject it — an authorization code must bind to
    // a concrete resource owner.
    let body = form_body(&[
        ("return_to", return_to.as_str()),
        ("token", "bootstrap-token"),
    ]);

    let mut resp = post_form("http://localhost/oauth/authorize/login", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    let text = resp.take_string().await.unwrap_or_default();
    assert!(
        text.contains("bootstrap login is not supported"),
        "expected bootstrap rejection message, got: {}",
        text
    );
    // No session cookie set on failure.
    assert!(set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).is_none());
}

#[tokio::test]
async fn oauth_authorize_with_valid_session_shows_consent_page() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    // Log in to obtain a session cookie.
    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    // GET /oauth/authorize with the session cookie -> consent page.
    let url = valid_authorize_url(&client, "https://example.com/callback");
    let mut resp = TestClient::get(&url)
        .add_header("cookie", &cookie, true)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let text = resp.take_string().await.unwrap_or_default();
    assert!(text.contains("Authorize WebCodex client"), "consent title");
    assert!(text.contains("Allow"), "Allow button");
    assert!(text.contains("Deny"), "Deny button");
    assert!(text.contains(&client.name), "client name shown");
    assert!(text.contains("runtime:read"), "requested scope shown");
    // No code is issued yet.
    assert!(!text.contains("wc_oac_"));
}

#[tokio::test]
async fn oauth_authorize_consent_shows_resource_when_present() {
    let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    let url = valid_authorize_url_with_resource(
        &client,
        "https://example.com/callback",
        "https://example.test/mcp",
    );
    let mut resp = TestClient::get(&url)
        .add_header("cookie", &cookie, true)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let text = resp.take_string().await.unwrap_or_default();
    assert!(text.contains("Resource:"), "resource label shown");
    assert!(
        text.contains("https://example.test/mcp"),
        "resource value shown"
    );
    assert!(text.contains("Allow"), "Allow button");
    assert!(!text.contains("wc_oac_"));
}

#[tokio::test]
async fn oauth_authorize_consent_allow_redirects_with_code() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    let before = auth_code_count(&db);
    let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
    let resp = post_form_with_cookie(
        "http://localhost/oauth/authorize/consent",
        consent_body,
        &cookie,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before + 1);
    let location = location_header(&resp).expect("redirect with code");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    let code = params.get("code").expect("code in redirect");
    assert!(code.starts_with("wc_oac_"));
    let record = auth_code_by_plaintext(&db, code);
    assert_eq!(record.shared_key_hash, None);
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
}

#[tokio::test]
async fn oauth_authorize_consent_allow_stores_resource_on_code() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    let before = auth_code_count(&db);
    let consent_body = consent_form_body_with_resource(
        &client,
        "https://example.com/callback",
        "allow",
        "https://example.test/mcp",
    );
    let resp = post_form_with_cookie(
        "http://localhost/oauth/authorize/consent",
        consent_body,
        &cookie,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before + 1);
    let location = location_header(&resp).expect("redirect with code");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    let code = params.get("code").expect("code in redirect");
    let record = auth_code_by_plaintext(&db, code);

    assert!(code.starts_with("wc_oac_"));
    assert_eq!(record.resource.as_deref(), Some("https://example.test/mcp"));
    assert_eq!(record.shared_key_hash, None);
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
}

#[tokio::test]
async fn oauth_authorize_consent_deny_redirects_with_access_denied() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    let before = auth_code_count(&db);
    let consent_body = consent_form_body(&client, "https://example.com/callback", "deny");
    let resp = post_form_with_cookie(
        "http://localhost/oauth/authorize/consent",
        consent_body,
        &cookie,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FOUND));
    assert_eq!(auth_code_count(&db), before);
    let location = location_header(&resp).expect("redirect on deny");
    let parsed = url::Url::parse(&location).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(
        params.get("error").map(String::as_str),
        Some("access_denied")
    );
    assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
}

#[tokio::test]
async fn oauth_authorize_consent_requires_valid_session() {
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
    let before = auth_code_count(&db);

    // No cookie at all.
    let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
    let resp = post_form("http://localhost/oauth/authorize/consent", consent_body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    assert_eq!(auth_code_count(&db), before);

    // Bogus cookie.
    let cookie = format!("{}=wc_authsess_bogus", AUTHORIZE_SESSION_COOKIE);
    let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
    let resp = post_form_with_cookie(
        "http://localhost/oauth/authorize/consent",
        consent_body,
        &cookie,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
    assert_eq!(auth_code_count(&db), before);
}

#[tokio::test]
async fn oauth_authorize_consent_revalidates_redirect_uri() {
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
    let return_to = return_to_for(&client, "https://example.com/callback");

    let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
    let resp = post_form("http://localhost/oauth/authorize/login", login_body)
        .send(&service)
        .await;
    let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

    let before = auth_code_count(&db);
    // Tampered redirect_uri in the consent hidden fields.
    let consent_body = form_body(&[
        ("response_type", "code"),
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", "https://evil.com/callback"),
        ("scope", "runtime:read"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("decision", "allow"),
    ]);
    let resp = post_form_with_cookie(
        "http://localhost/oauth/authorize/consent",
        consent_body,
        &cookie,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    assert_no_location(&resp);
    assert_eq!(auth_code_count(&db), before);
}

#[tokio::test]
async fn oauth_authorize_return_to_rejects_absolute_url() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    // Absolute URL return_to -> rejected (open-redirect guard).
    let body = form_body(&[
        ("return_to", "https://evil.com/oauth/authorize"),
        ("token", token.as_str()),
    ]);
    let resp = post_form("http://localhost/oauth/authorize/login", body)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
}
