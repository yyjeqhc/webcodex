use super::*;

#[test]
fn project_share_scope_ceiling_is_connector_only() {
    let allowed = "runtime:read project:read project:write job:run";
    let normalized = normalize_project_share_oauth_scopes(Some(allowed), allowed).unwrap();
    assert_eq!(normalized, allowed);

    for forbidden in ["account:manage", "computer:read", "agent:poll"] {
        let client_allowed = format!("{allowed} {forbidden}");
        assert!(matches!(
            normalize_project_share_oauth_scopes(Some(&client_allowed), &client_allowed),
            Err(OAuthAuthorizeError::InvalidScope(_))
        ));
    }

    let globally_supported_but_outside_connector =
        "runtime:read project:read project:write job:run computer:read";
    if normalize_oauth_scopes(
        Some(globally_supported_but_outside_connector),
        globally_supported_but_outside_connector,
    )
    .is_ok()
    {
        assert_eq!(
            normalize_project_share_oauth_scopes(
                Some(globally_supported_but_outside_connector),
                globally_supported_but_outside_connector,
            ),
            Err(OAuthAuthorizeError::InvalidScope(
                PROJECT_SHARE_OAUTH_INVALID_SCOPE_MESSAGE
            ))
        );
    }
}

#[tokio::test]
async fn standard_authorize_route_renders_project_share_credential_form() {
    let config = test_config(oauth2_enabled_project_share(TEST_PROJECT_SHARE_SESSION_ID));
    let (_tmp, db) = test_db();
    let (client, _secret) = seed_project_share_client(
        &db,
        TEST_PROJECT_GRANT_ID,
        "https://client.example/callback",
    );
    let service = Service::new(build_router(config, db));
    let query = form_body(&[
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", "https://client.example/callback"),
        ("scope", "runtime:read project:read project:write job:run"),
        ("state", "state-1"),
        ("code_challenge", "challenge-1"),
        ("code_challenge_method", "S256"),
        ("resource", "https://share.example/mcp"),
    ]);
    let mut resp = TestClient::get(format!("http://localhost/oauth/authorize?{query}"))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body = resp.take_string().await.unwrap();
    assert!(body.contains("Authorize WebCodex project share"));
    assert!(body.contains("project-share-client"));
    assert!(body.contains("project_credential"));
    assert!(!body.contains("WebCodex personal API token"));
}

#[tokio::test]
async fn project_share_authorize_rejects_managed_or_wrong_project_clients() {
    let config = test_config(oauth2_enabled_project_share(TEST_PROJECT_SHARE_SESSION_ID));
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let managed = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://client.example/callback",
        "runtime:read",
    );
    let (wrong_project, _secret) = seed_project_share_client(
        &db,
        "wc_pgrant_222222222222222222222222",
        "https://client.example/callback",
    );
    let service = Service::new(build_router(config, db));

    for client_id in [&managed.client_id, &wrong_project.client_id] {
        let query = form_body(&[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", "https://client.example/callback"),
            ("scope", "runtime:read"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);
        let mut resp = TestClient::get(format!("http://localhost/oauth/authorize?{query}"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["error"], "invalid_request");
    }
}
