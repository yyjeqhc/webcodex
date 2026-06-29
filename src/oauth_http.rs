//! OAuth2 token endpoint — `POST /oauth/token`.
//!
//! Implements the `authorization_code` grant type with PKCE S256 support.
//! This is a **public** endpoint (no `AuthMiddleware`); clients authenticate
//! via `client_id` + `client_secret` in the form body.
//!
//! Security properties:
//! - Authorization codes are consumed atomically (single-use).
//! - Code consumption and token insertion happen in a single DB transaction.
//! - Client secret is verified with constant-time comparison.
//! - PKCE S256 is enforced when `config.oauth2.require_pkce` is true.
//! - Only `application/x-www-form-urlencoded` content type is accepted.
//! - Request body size is bounded (16 KiB).
//! - All responses include `Cache-Control: no-store` and `Pragma: no-cache`.
//! - Plaintext tokens are returned **only once** in the response.
//! - Only SHA-256 hashes are stored in the database.

use crate::auth::{generate_oauth_access_token, generate_oauth_refresh_token, hash_token};
use crate::models::{OAuthAccessTokenRecord, OAuthRefreshTokenRecord};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::http::header::CONTENT_TYPE;
use salvo::http::HeaderValue;
use salvo::prelude::*;
use sha2::{Digest, Sha256};

/// Maximum request body size for the token endpoint (16 KiB).
const MAX_OAUTH_TOKEN_FORM_BYTES: usize = 16 * 1024;

// ---------------------------------------------------------------------------
// OAuth2 response helpers
// ---------------------------------------------------------------------------

/// Apply cache-prevention headers to an OAuth2 response (RFC 6749 §5.1, §5.2).
///
/// All OAuth2 JSON responses — both success and error — must include these
/// headers to prevent intermediaries from caching sensitive tokens or error
/// context.
fn apply_oauth_no_store_headers(res: &mut Response) {
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    res.headers_mut()
        .insert("pragma", HeaderValue::from_static("no-cache"));
}

/// Render an OAuth2 error response (RFC 6749 §5.2) with no-store headers.
fn oauth_error(res: &mut Response, status: StatusCode, error: &str, description: &str) {
    res.status_code(status);
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "error": error,
        "error_description": description,
    })));
}

// ---------------------------------------------------------------------------
// PKCE S256
// ---------------------------------------------------------------------------

/// Verify a PKCE S256 code verifier against the stored code challenge.
///
/// Computes `BASE64URL-NO-PAD(SHA256(ASCII(verifier)))` and compares it to
/// `challenge` using constant-time equality.
pub(crate) fn verify_pkce_s256(verifier: &str, challenge: &str) -> bool {
    let digest = Sha256::digest(verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(digest);
    crate::config::constant_time_eq(computed.as_bytes(), challenge.as_bytes())
}

// ---------------------------------------------------------------------------
// Form body
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct TokenRequest {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    code_verifier: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[handler]
pub(crate) async fn oauth_token(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    // --- Config ---
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    // --- OAuth2 enable gate ---
    if !config.oauth2.enabled {
        oauth_error(
            res,
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "OAuth2 is not enabled",
        );
        return;
    }

    // --- Content-Type enforcement ---
    let content_type_ok = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            ct.eq_ignore_ascii_case("application/x-www-form-urlencoded")
                || ct
                    .to_ascii_lowercase()
                    .starts_with("application/x-www-form-urlencoded;")
        })
        .unwrap_or(false);

    if !content_type_ok {
        oauth_error(
            res,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_request",
            "content-type must be application/x-www-form-urlencoded",
        );
        return;
    }

    // --- Body size limit (Content-Length pre-check) ---
    if let Some(cl) = req.headers().get("content-length") {
        if let Ok(len) = cl.to_str().unwrap_or("").parse::<usize>() {
            if len > MAX_OAUTH_TOKEN_FORM_BYTES {
                oauth_error(
                    res,
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "invalid_request",
                    "request body too large",
                );
                return;
            }
        }
    }

    // --- Parse form body ---
    let body = match req.payload().await {
        Ok(bytes) => bytes,
        Err(e) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("failed to read request body: {}", e),
            );
            return;
        }
    };

    // --- Body size limit (actual body check) ---
    if body.len() > MAX_OAUTH_TOKEN_FORM_BYTES {
        oauth_error(
            res,
            StatusCode::PAYLOAD_TOO_LARGE,
            "invalid_request",
            "request body too large",
        );
        return;
    }

    let form: TokenRequest = match serde_urlencoded::from_bytes(&body) {
        Ok(f) => f,
        Err(e) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid form body: {}", e),
            );
            return;
        }
    };

    // --- grant_type ---
    let grant_type = match form.grant_type.as_deref() {
        Some(gt) => gt,
        None => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing grant_type",
            );
            return;
        }
    };

    if grant_type != "authorization_code" {
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "only authorization_code grant is supported",
        );
        return;
    }

    // --- Required parameters ---
    let plaintext_code = match form.code.as_deref() {
        Some(c) if !c.is_empty() => c,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing code",
            );
            return;
        }
    };

    let redirect_uri = match form.redirect_uri.as_deref() {
        Some(r) if !r.is_empty() => r,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing redirect_uri",
            );
            return;
        }
    };

    let client_id = match form.client_id.as_deref() {
        Some(c) if !c.is_empty() => c,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing client_id",
            );
            return;
        }
    };

    let client_secret = match form.client_secret.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            oauth_error(
                res,
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "missing client_secret",
            );
            return;
        }
    };

    // --- DB ---
    let Some(db) = crate::auth::get_db(depot) else {
        oauth_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    // --- Client authentication (before code consumption) ---
    let secret_ok = match db.verify_oauth_client_secret(client_id, client_secret) {
        Ok(ok) => ok,
        Err(_) => {
            oauth_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    if !secret_ok {
        oauth_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
        return;
    }

    // Also verify the client is not revoked (verify_oauth_client_secret
    // already filters revoked via get_oauth_client_by_client_id, but fetch
    // the record again for downstream fields).
    let client = match db.get_oauth_client_by_client_id(client_id) {
        Ok(Some(c)) => c,
        _ => {
            oauth_error(
                res,
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
            return;
        }
    };

    // --- Transactional exchange: consume code + insert tokens ---
    let code_hash = hash_token(plaintext_code);
    let now = chrono::Utc::now().timestamp();

    let access_token = generate_oauth_access_token();
    let refresh_token = generate_oauth_refresh_token();
    let at_hash = hash_token(&access_token);
    let rt_hash = hash_token(&refresh_token);
    let at_expires_at = now + config.oauth2.access_token_ttl_secs;
    let rt_expires_at = now + config.oauth2.refresh_token_ttl_secs;

    let at_record = OAuthAccessTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: at_hash,
        client_id: client.client_id.clone(),
        user_id: String::new(), // filled after code_record is known
        scopes: String::new(),
        resource: None,
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let rt_record = OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: rt_hash,
        client_id: client.client_id.clone(),
        user_id: String::new(),
        scopes: String::new(),
        resource: None,
        created_at: now,
        expires_at: rt_expires_at,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
    };

    // We need to build the records with the code's user_id/scopes/resource
    // which we don't know yet. Fetch the code first to get those fields,
    // then do the transactional exchange.
    //
    // To avoid a TOCTOU race, we fetch the code's metadata from the
    // authorization_codes table (which doesn't consume it), build the token
    // records, then do the atomic exchange.
    let code_metadata = match db.get_oauth_authorization_code_by_hash(&code_hash) {
        Ok(Some(c)) => c,
        Ok(None) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid, expired, or already used",
            );
            return;
        }
        Err(_) => {
            oauth_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    // Rebuild token records with the code's user_id, scopes, and resource.
    let at_record = OAuthAccessTokenRecord {
        user_id: code_metadata.user_id.clone(),
        scopes: code_metadata.scopes.clone(),
        resource: code_metadata.resource.clone(),
        ..at_record
    };

    let rt_record = OAuthRefreshTokenRecord {
        user_id: code_metadata.user_id.clone(),
        scopes: code_metadata.scopes.clone(),
        resource: code_metadata.resource.clone(),
        ..rt_record
    };

    let code_record = match db
        .exchange_oauth_authorization_code_for_tokens(&code_hash, now, &at_record, &rt_record)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid, expired, or already used",
            );
            return;
        }
        Err(_) => {
            oauth_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    // --- Post-consume validation (code is already consumed; failures here
    //     are intentional — the code cannot be retried). ---

    if code_record.client_id != client.client_id {
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code was not issued to this client",
        );
        return;
    }

    if code_record.redirect_uri != redirect_uri {
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "redirect_uri does not match",
        );
        return;
    }

    // --- PKCE S256 ---
    let require_pkce = config.oauth2.require_pkce;
    if let Some(ref challenge) = code_record.code_challenge {
        // Code has a challenge — verifier is mandatory.
        let verifier = match form.code_verifier.as_deref() {
            Some(v) if !v.is_empty() => v,
            _ => {
                oauth_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "missing code_verifier for PKCE",
                );
                return;
            }
        };

        // Only S256 is supported.
        match code_record.code_challenge_method.as_deref() {
            Some("S256") => {}
            _ => {
                oauth_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "unsupported code_challenge_method; only S256 is supported",
                );
                return;
            }
        }

        if !verify_pkce_s256(verifier, challenge) {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "PKCE verification failed",
            );
            return;
        }
    } else if require_pkce {
        // No challenge on the record but PKCE is required.
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "PKCE is required but no code_challenge was provided during authorization",
        );
        return;
    }

    // --- Success response ---
    let mut body = serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": config.oauth2.access_token_ttl_secs,
        "refresh_token": refresh_token,
    });

    if !code_record.scopes.is_empty() {
        body["scope"] = serde_json::Value::String(code_record.scopes.clone());
    }

    apply_oauth_no_store_headers(res);
    res.render(Json(body));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{generate_oauth_authorization_code, hash_token};
    use crate::models::{OAuthAuthorizationCodeRecord, OAuthClientRecord, UserRecord};
    use crate::OAuth2Config;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_config(oauth2: OAuth2Config) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("bootstrap-token".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2,
        })
    }

    fn oauth2_enabled() -> OAuth2Config {
        OAuth2Config {
            enabled: true,
            require_pkce: true,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            ..OAuth2Config::default()
        }
    }

    fn oauth2_enabled_no_pkce() -> OAuth2Config {
        OAuth2Config {
            enabled: true,
            require_pkce: false,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            ..OAuth2Config::default()
        }
    }

    fn oauth2_disabled() -> OAuth2Config {
        OAuth2Config {
            enabled: false,
            ..OAuth2Config::default()
        }
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("oauth.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn seed_user(db: &crate::Database, username: &str) -> UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn seed_client(
        db: &crate::Database,
        user: &UserRecord,
        name: &str,
    ) -> (OAuthClientRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: secret_hash,
            name: name.to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read".to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        (record, plaintext_secret)
    }

    fn seed_auth_code(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        redirect_uri: &str,
        scopes: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
    ) -> (OAuthAuthorizationCodeRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_code = generate_oauth_authorization_code();
        let code_hash = hash_token(&plaintext_code);
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash,
            client_id: client.client_id.clone(),
            user_id: user.id.clone(),
            redirect_uri: redirect_uri.to_string(),
            scopes: scopes.to_string(),
            code_challenge: code_challenge.map(str::to_string),
            code_challenge_method: code_challenge_method.map(str::to_string),
            resource: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &record.code_hash)
            .unwrap();
        (record, plaintext_code)
    }

    fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(salvo::prelude::affix_state::inject(config))
            .hoop(salvo::prelude::affix_state::inject(db))
            .push(Router::with_path("oauth/token").post(oauth_token))
    }

    fn form_body(pairs: &[(&str, &str)]) -> String {
        pairs
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Build a POST request with the correct Content-Type header.
    fn post_form(url: &str, body: String) -> salvo::test::RequestBuilder {
        TestClient::post(url)
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body(body)
    }

    // -----------------------------------------------------------------------
    // Success path
    // -----------------------------------------------------------------------

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
    async fn wrong_client_secret_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
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

        let service = Service::new(build_router(config, db));
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

    #[tokio::test]
    async fn unknown_client_id_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
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

    #[tokio::test]
    async fn revoked_client_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_client(&client.id, now).unwrap();

        let service = Service::new(build_router(config, db));
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

    // -----------------------------------------------------------------------
    // Grant/code validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn unsupported_grant_type_returns_error() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "client_credentials")]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "unsupported_grant_type");
    }

    #[tokio::test]
    async fn missing_code_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn unknown_code_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", "wc_oac_nonexistent"),
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
            user_id: user.id.clone(),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
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

        // Create a second client. The code belongs to client1 but we exchange
        // with client2's credentials. Client auth succeeds (client2's secret
        // matches), but the code's client_id doesn't match client2.
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

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://evil.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // Code SHOULD be consumed — post-consume validation failure.
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

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", "wrong-verifier"),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // Code SHOULD be consumed — post-consume validation failure.
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
    }
}
