use super::*;

async fn assert_protocol_error(
    mut response: Response,
    status: StatusCode,
    error: &str,
    description: &str,
) {
    assert_eq!(response.status_code, Some(status), "{description}");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["pragma"], "no-cache");
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({"error": error, "error_description": description}),
    );
}

#[tokio::test]
async fn protocol_errors_keep_parameter_precedence_before_database_lookup() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    // No database is injected: malformed requests must still report their
    // first protocol error before reaching client authentication or storage.
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(test_config(oauth2_enabled_no_pkce())))
            .push(Router::with_path("oauth/token").post(oauth_token))
            .push(Router::with_path("oauth/revoke").post(oauth_revoke)),
    );
    for (endpoint, form, status, error, description) in [
        (
            "token",
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing grant_type",
        ),
        (
            "token",
            "grant_type=unsupported",
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "only authorization_code and refresh_token grants are supported",
        ),
        (
            "token",
            "grant_type=refresh_token&scope=runtime%3Aread",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "scope narrowing is not supported for refresh_token grant yet",
        ),
        (
            "token",
            "grant_type=authorization_code",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        ),
        (
            "token",
            "grant_type=authorization_code&client_id=client",
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "missing client_secret",
        ),
        (
            "revoke",
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing token",
        ),
        (
            "revoke",
            "token=token",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        ),
        (
            "revoke",
            "token=token&client_id=client",
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "missing client_secret",
        ),
        (
            "token",
            "grant_type=authorization_code&client_id=client&client_secret=secret",
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        ),
        (
            "revoke",
            "token=token&client_id=client&client_secret=secret",
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        ),
    ] {
        let response = post_form(
            &format!("http://localhost/oauth/{endpoint}"),
            form.to_string(),
        )
        .send(&service)
        .await;
        assert_protocol_error(response, status, error, description).await;
    }
}

#[tokio::test]
async fn protocol_errors_keep_configuration_gate_before_form_validation() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    for (config, status, description) in [
        (None, StatusCode::INTERNAL_SERVER_ERROR, "no config"),
        (
            Some(test_config(oauth2_disabled())),
            StatusCode::SERVICE_UNAVAILABLE,
            "OAuth2 is not enabled",
        ),
    ] {
        let mut router = Router::new()
            .push(Router::with_path("oauth/token").post(oauth_token))
            .push(Router::with_path("oauth/revoke").post(oauth_revoke));
        if let Some(config) = config {
            router = router.hoop(affix_state::inject(config));
        }
        let service = Service::new(router);
        for endpoint in ["token", "revoke"] {
            let response = TestClient::post(&format!("http://localhost/oauth/{endpoint}"))
                .add_header("content-type", "application/json", true)
                .body("{}")
                .send(&service)
                .await;
            assert_protocol_error(response, status, "server_error", description).await;
        }
    }
}
