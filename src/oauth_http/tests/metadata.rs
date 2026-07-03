use super::*;

// -----------------------------------------------------------------------
// GET /.well-known/oauth-protected-resource
// -----------------------------------------------------------------------

#[tokio::test]
async fn oauth_protected_resource_metadata_is_public() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let ct = resp
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("application/json"),
        "expected application/json, got {}",
        ct
    );
}

#[tokio::test]
async fn oauth_protected_resource_metadata_fields() {
    let mut oauth2 = oauth2_enabled();
    oauth2.issuer = Some("https://codex.example.com".to_string());
    let config = test_config(oauth2);
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();

    // resource is an absolute URL
    let resource = body["resource"].as_str().unwrap();
    assert!(
        resource.starts_with("https://"),
        "resource should be absolute URL, got {}",
        resource
    );
    assert_eq!(resource, "https://codex.example.com");

    // authorization_servers is an array whose first element matches issuer
    let auth_servers = body["authorization_servers"].as_array().unwrap();
    assert_eq!(auth_servers.len(), 1);
    assert_eq!(auth_servers[0], "https://codex.example.com");

    // bearer_methods_supported == ["header"]
    let methods = body["bearer_methods_supported"].as_array().unwrap();
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0], "header");

    // scopes_supported is a non-empty array
    let scopes = body["scopes_supported"].as_array().unwrap();
    assert!(!scopes.is_empty(), "scopes_supported should be non-empty");
    // Must contain at least runtime:read
    assert!(
        scopes.iter().any(|s| s == "runtime:read"),
        "scopes_supported should contain runtime:read"
    );

    // resource_name
    assert_eq!(body["resource_name"], "WebCodex");
}

#[tokio::test]
async fn oauth_protected_resource_metadata_disabled_returns_404() {
    let config = test_config(oauth2_disabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
}

#[tokio::test]
async fn oauth_protected_resource_metadata_no_issuer_fallback() {
    // When issuer is None, resource falls back to "http://localhost"
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["resource"], "http://localhost");
    let auth_servers = body["authorization_servers"].as_array().unwrap();
    assert_eq!(auth_servers[0], "http://localhost");
}

#[tokio::test]
async fn oauth_authorization_server_metadata_is_public() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let ct = resp
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("application/json"),
        "expected application/json, got {}",
        ct
    );
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["issuer"], "http://localhost");
    assert_eq!(
        body["authorization_endpoint"],
        "http://localhost/oauth/authorize"
    );
}

#[tokio::test]
async fn oauth_authorization_server_metadata_fields() {
    let mut oauth2 = oauth2_enabled();
    oauth2.issuer = Some("https://codex.example.com".to_string());
    let config = test_config(oauth2);
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();

    assert_eq!(body["issuer"], "https://codex.example.com");
    assert_eq!(
        body["authorization_endpoint"],
        "https://codex.example.com/oauth/authorize"
    );
    assert_eq!(
        body["token_endpoint"],
        "https://codex.example.com/oauth/token"
    );
    assert_eq!(
        body["revocation_endpoint"],
        "https://codex.example.com/oauth/revoke"
    );
    assert_eq!(
        body["response_types_supported"],
        serde_json::json!(["code"])
    );
    assert!(body["grant_types_supported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "authorization_code"));
    assert!(body["grant_types_supported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "refresh_token"));
    assert_eq!(
        body["code_challenge_methods_supported"],
        serde_json::json!(["S256"])
    );
    assert_eq!(
        body["token_endpoint_auth_methods_supported"],
        serde_json::json!(["client_secret_post"])
    );
    let auth_methods = body["token_endpoint_auth_methods_supported"]
        .as_array()
        .unwrap();
    assert!(
        !auth_methods.iter().any(|v| v == "none"),
        "metadata must not advertise unsupported public-client auth"
    );
    assert_eq!(
        body["scopes_supported"],
        serde_json::json!(oauth_scopes_supported())
    );

    assert!(
        body.get("jwks_uri").is_none(),
        "metadata must not advertise JWKS"
    );
    assert!(
        body.get("userinfo_endpoint").is_none(),
        "metadata must not advertise OIDC userinfo"
    );
}

#[tokio::test]
async fn oauth_authorization_server_metadata_trims_trailing_issuer_slash() {
    let mut oauth2 = oauth2_enabled();
    oauth2.issuer = Some("https://codex.example.com/".to_string());
    let config = test_config(oauth2);
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();

    assert_eq!(body["issuer"], "https://codex.example.com/");
    assert_eq!(
        body["authorization_endpoint"],
        "https://codex.example.com/oauth/authorize"
    );
    assert_eq!(
        body["token_endpoint"],
        "https://codex.example.com/oauth/token"
    );
    assert_eq!(
        body["revocation_endpoint"],
        "https://codex.example.com/oauth/revoke"
    );
}

#[tokio::test]
async fn oauth_authorization_server_metadata_disabled_returns_404() {
    let config = test_config(oauth2_disabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["error"], "OAuth2 is not enabled");
}

#[tokio::test]
async fn openid_configuration_not_exposed() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let resp = TestClient::get("http://localhost/.well-known/openid-configuration")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
}

#[tokio::test]
async fn oauth_protected_resource_metadata_scopes_exclude_agent() {
    // Agent scopes must not appear in scopes_supported because OAuth2
    // tokens are rejected on agent transport surfaces.
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    let scopes = body["scopes_supported"].as_array().unwrap();
    for scope in scopes {
        let s = scope.as_str().unwrap();
        assert!(
            !s.starts_with("agent:"),
            "agent scope '{}' must not appear in scopes_supported",
            s
        );
    }
    // admin is a bootstrap scope, not for OAuth2 delegation
    assert!(
        !scopes.iter().any(|s| s == "admin"),
        "admin scope must not appear in scopes_supported"
    );
}
