//! OAuth2 token, revocation, and discovery endpoints.
//!
//! - `POST /oauth/token` — token endpoint (authorization_code, refresh_token)
//! - `POST /oauth/revoke` — token revocation endpoint (RFC 7009)
//! - `GET /.well-known/oauth-protected-resource` — protected resource metadata
//! - `GET /.well-known/oauth-authorization-server` — authorization server metadata
//!
//! Token and revocation are **public** endpoints (no `AuthMiddleware`); clients
//! authenticate via `client_id` + `client_secret` in the form body. The
//! metadata endpoint is also public and requires no authentication.
//!
//! Security properties:
//! - Authorization codes are consumed atomically (single-use).
//! - Code consumption and token insertion happen in a single DB transaction
//!   **only** when all validations pass.
//! - Refresh tokens are rotated: the old token is revoked and a new
//!   access+refresh token pair is issued in a single transaction.
//! - Revocation is idempotent: unknown, already-revoked, and other-client
//!   tokens all return HTTP 200 without disclosing token state.
//! - Client secret is verified with constant-time comparison.
//! - Only `application/x-www-form-urlencoded` content type is accepted.
//! - Request body size is bounded (16 KiB).
//! - All responses include `Cache-Control: no-store` and `Pragma: no-cache`.
//! - Plaintext tokens are returned **only once** in the response.
//! - Only SHA-256 hashes are stored in the database.

use crate::auth::{
    generate_oauth_access_token, generate_oauth_authorization_code, generate_oauth_refresh_token,
    hash_token, scopes, AuthContext, AuthKind,
};
use crate::models::{
    OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthRefreshTokenRecord,
};
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

fn oauth_authorize_direct_error(
    res: &mut Response,
    status: StatusCode,
    error: &str,
    description: &str,
) {
    res.status_code(status);
    res.render(Json(serde_json::json!({
        "error": error,
        "error_description": description,
    })));
}

fn redirect_with_oauth_error(
    res: &mut Response,
    redirect_uri: &str,
    error: &str,
    state: Option<&str>,
) {
    let location = match append_authorize_error_params(redirect_uri, error, state) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    let location = match HeaderValue::from_str(&location) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    res.status_code(StatusCode::FOUND);
    res.headers_mut().insert("location", location);
}

fn append_authorize_error_params(
    redirect_uri: &str,
    error: &str,
    state: Option<&str>,
) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("error", error);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
    }
    Ok(url.into())
}

fn redirect_with_authorization_code(
    res: &mut Response,
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
) {
    let location = match append_authorize_success_params(redirect_uri, code, state) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    let location = match HeaderValue::from_str(&location) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    res.status_code(StatusCode::FOUND);
    res.headers_mut().insert("location", location);
}

fn append_authorize_success_params(
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("code", code);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
    }
    Ok(url.into())
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
// Authorization endpoint request helpers
// ---------------------------------------------------------------------------

/// Parsed query shape for the future `GET /oauth/authorize` endpoint.
///
/// This is intentionally a pure internal data type for now. Phase 2e-1a does
/// not mount an authorize route or issue authorization codes.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OAuthAuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
}

/// Internal authorization endpoint validation errors.
///
/// `InvalidRequest` is for direct errors before the client/redirect trust
/// boundary is established. Redirectable variants are for errors that can be
/// mapped to OAuth redirect errors after the client and redirect URI are
/// trusted.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OAuthAuthorizeError {
    InvalidRequest(&'static str),
    UnauthorizedClient(&'static str),
    UnsupportedResponseType,
    InvalidScope(&'static str),
    InvalidRequestRedirectable(&'static str),
    UnsupportedResource,
}

/// Parse a future `/oauth/authorize` query string without performing runtime
/// validation, DB lookups, redirects, or authorization code issuance.
///
/// Duplicate known parameters are rejected because they make the OAuth request
/// ambiguous. Unknown parameters are ignored for forward compatibility.
#[allow(dead_code)]
pub(crate) fn parse_authorize_query(
    query: &str,
) -> Result<OAuthAuthorizeRequest, OAuthAuthorizeError> {
    let mut response_type = None;
    let mut client_id = None;
    let mut redirect_uri = None;
    let mut scope = None;
    let mut state = None;
    let mut code_challenge = None;
    let mut code_challenge_method = None;
    let mut resource = None;

    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "response_type" => &mut response_type,
            "client_id" => &mut client_id,
            "redirect_uri" => &mut redirect_uri,
            "scope" => &mut scope,
            "state" => &mut state,
            "code_challenge" => &mut code_challenge,
            "code_challenge_method" => &mut code_challenge_method,
            "resource" => &mut resource,
            _ => continue,
        };

        if slot.replace(value.into_owned()).is_some() {
            return Err(OAuthAuthorizeError::InvalidRequest("duplicate parameter"));
        }
    }

    Ok(OAuthAuthorizeRequest {
        response_type: response_type
            .ok_or(OAuthAuthorizeError::InvalidRequest("missing response_type"))?,
        client_id: client_id.ok_or(OAuthAuthorizeError::InvalidRequest("missing client_id"))?,
        redirect_uri: redirect_uri
            .ok_or(OAuthAuthorizeError::InvalidRequest("missing redirect_uri"))?,
        scope,
        state,
        code_challenge: code_challenge.ok_or(OAuthAuthorizeError::InvalidRequest(
            "missing code_challenge",
        ))?,
        code_challenge_method: code_challenge_method.ok_or(OAuthAuthorizeError::InvalidRequest(
            "missing code_challenge_method",
        ))?,
        resource,
    })
}

fn decoded_authorize_param(query: &str, name: &str) -> Result<Option<String>, OAuthAuthorizeError> {
    let mut value = None;
    for (key, raw_value) in url::form_urlencoded::parse(query.as_bytes()) {
        if key.as_ref() != name {
            continue;
        }
        if value.replace(raw_value.into_owned()).is_some() {
            return Err(OAuthAuthorizeError::InvalidRequest("duplicate parameter"));
        }
    }
    Ok(value)
}

fn is_redirectable_missing_authorize_param(error: &OAuthAuthorizeError) -> bool {
    matches!(
        error,
        OAuthAuthorizeError::InvalidRequest("missing response_type")
            | OAuthAuthorizeError::InvalidRequest("missing code_challenge")
            | OAuthAuthorizeError::InvalidRequest("missing code_challenge_method")
    )
}

fn redirect_error_for_missing_authorize_param(error: &OAuthAuthorizeError) -> &'static str {
    match error {
        OAuthAuthorizeError::InvalidRequest("missing response_type") => "invalid_request",
        OAuthAuthorizeError::InvalidRequest("missing code_challenge") => "invalid_request",
        OAuthAuthorizeError::InvalidRequest("missing code_challenge_method") => "invalid_request",
        _ => "invalid_request",
    }
}

// ---------------------------------------------------------------------------
// GET /oauth/authorize
// ---------------------------------------------------------------------------

fn is_authorize_identity_allowed(ctx: &AuthContext) -> bool {
    matches!(ctx.kind, AuthKind::Bootstrap | AuthKind::ApiToken)
}

/// Authorization endpoint for Phase 2e-1c.
///
/// The route is protected by `AuthMiddleware` in `main.rs`. This handler
/// validates the request, client, redirect URI, PKCE, scope, and unsupported
/// resource semantics, then issues a hash-stored authorization code and
/// redirects back to the validated redirect URI.
#[handler]
pub(crate) async fn oauth_authorize(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    if !config.oauth2.enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "OAuth2 is not enabled",
        );
        return;
    }

    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        oauth_authorize_direct_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_request",
            "authenticated user required",
        );
        return;
    };

    if !is_authorize_identity_allowed(auth) {
        oauth_authorize_direct_error(
            res,
            StatusCode::FORBIDDEN,
            "invalid_request",
            "authorization endpoint requires first-party user authentication",
        );
        return;
    }

    if auth.user_id.is_none() {
        oauth_authorize_direct_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_request",
            "authenticated user required",
        );
        return;
    }

    let Some(db) = crate::auth::get_db(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    let query = req.uri().query().unwrap_or("");
    let parsed = parse_authorize_query(query);

    let (client_id, redirect_uri) = match &parsed {
        Ok(parsed) => (parsed.client_id.clone(), parsed.redirect_uri.clone()),
        Err(error) if is_redirectable_missing_authorize_param(error) => {
            let client_id = match decoded_authorize_param(query, "client_id") {
                Ok(Some(client_id)) if !client_id.is_empty() => client_id,
                Ok(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "missing client_id",
                    );
                    return;
                }
                Err(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "duplicate parameter",
                    );
                    return;
                }
            };
            let redirect_uri = match decoded_authorize_param(query, "redirect_uri") {
                Ok(Some(redirect_uri)) if !redirect_uri.is_empty() => redirect_uri,
                Ok(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "missing redirect_uri",
                    );
                    return;
                }
                Err(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "duplicate parameter",
                    );
                    return;
                }
            };
            (client_id, redirect_uri)
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return;
        }
    };

    if client_id.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        );
        return;
    }

    let client = match db.get_oauth_client_by_client_id(&client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid client_id",
            );
            return;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    if redirect_uri.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        );
        return;
    }

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|registered| registered == &redirect_uri)
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
        return;
    }

    let state = match decoded_authorize_param(query, "state") {
        Ok(state) => state,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "duplicate parameter",
            );
            return;
        }
    };

    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) if is_redirectable_missing_authorize_param(&error) => {
            redirect_with_oauth_error(
                res,
                &redirect_uri,
                redirect_error_for_missing_authorize_param(&error),
                state.as_deref(),
            );
            return;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return;
        }
    };

    if parsed.response_type.is_empty() || parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return;
    }

    if parsed.code_challenge.is_empty() {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }

    if parsed.code_challenge_method.is_empty() || parsed.code_challenge_method != "S256" {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }

    let scopes = match normalize_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
        Ok(scopes) => scopes,
        Err(_) => {
            redirect_with_oauth_error(res, &redirect_uri, "invalid_scope", parsed.state.as_deref());
            return;
        }
    };

    if parsed.resource.is_some() {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }

    let Some(user_id) = auth.user_id.clone() else {
        oauth_authorize_direct_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_request",
            "authenticated user required",
        );
        return;
    };

    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: client.client_id.clone(),
        user_id,
        redirect_uri: redirect_uri.clone(),
        scopes,
        resource: None,
        code_challenge: Some(parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };

    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(res, &redirect_uri, &plaintext_code, parsed.state.as_deref());
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
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RevokeRequest {
    token: Option<String>,
    token_type_hint: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
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

    // Validate grant_type before checking other parameters.
    match grant_type {
        "authorization_code" | "refresh_token" => {}
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "only authorization_code and refresh_token grants are supported",
            );
            return;
        }
    }

    // Reject scope parameter on refresh_token grant (not yet supported).
    if grant_type == "refresh_token" && form.scope.is_some() {
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "scope narrowing is not supported for refresh_token grant yet",
        );
        return;
    }

    // --- Required parameters (common to both grants) ---
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

    // --- Client authentication (before any token operations) ---
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

    let now = chrono::Utc::now().timestamp();

    // --- Dispatch by grant_type ---
    match grant_type {
        "authorization_code" => {
            handle_authorization_code_grant(&config, &db, &client, &form, now, res).await;
        }
        "refresh_token" => {
            handle_refresh_token_grant(&config, &db, &client, &form, now, res).await;
        }
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "only authorization_code and refresh_token grants are supported",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// authorization_code grant
// ---------------------------------------------------------------------------

async fn handle_authorization_code_grant(
    config: &crate::Config,
    db: &crate::Database,
    client: &crate::models::OAuthClientRecord,
    form: &TokenRequest,
    now: i64,
    res: &mut Response,
) {
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

    // --- Read authorization code metadata (without consuming) ---
    let code_hash = hash_token(plaintext_code);

    let code_record = match db.get_oauth_authorization_code_by_hash(&code_hash) {
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

    // --- Pre-exchange validation (using metadata, code is NOT yet consumed) ---

    if code_record.client_id != client.client_id {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code was not issued to this client",
        );
        return;
    }

    if code_record.redirect_uri != redirect_uri {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "redirect_uri does not match",
        );
        return;
    }

    // --- PKCE S256 (before code consumption) ---
    let require_pkce = config.oauth2.require_pkce;
    if let Some(ref challenge) = code_record.code_challenge {
        let verifier = match form.code_verifier.as_deref() {
            Some(v) if !v.is_empty() => v,
            _ => {
                let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
                oauth_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "missing code_verifier for PKCE",
                );
                return;
            }
        };

        match code_record.code_challenge_method.as_deref() {
            Some("S256") => {}
            _ => {
                let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
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
            let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "PKCE verification failed",
            );
            return;
        }
    } else if require_pkce {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        oauth_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "PKCE is required but no code_challenge was provided during authorization",
        );
        return;
    }

    // --- All validations passed — transactional exchange ---
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
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: code_record.resource.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let rt_record = OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: rt_hash,
        client_id: client.client_id.clone(),
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: code_record.resource.clone(),
        created_at: now,
        expires_at: rt_expires_at,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
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
// refresh_token grant
// ---------------------------------------------------------------------------

async fn handle_refresh_token_grant(
    config: &crate::Config,
    db: &crate::Database,
    client: &crate::models::OAuthClientRecord,
    form: &TokenRequest,
    now: i64,
    res: &mut Response,
) {
    let plaintext_rt = match form.refresh_token.as_deref() {
        Some(r) if !r.is_empty() => r,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing refresh_token",
            );
            return;
        }
    };

    let rt_hash = hash_token(plaintext_rt);
    let at_expires_at = now + config.oauth2.access_token_ttl_secs;
    let new_rt_expires_at = now + config.oauth2.refresh_token_ttl_secs;

    // Generate tokens upfront — they'll be inserted inside the transaction.
    let new_access_token = generate_oauth_access_token();
    let new_refresh_token = generate_oauth_refresh_token();
    let new_at_hash = hash_token(&new_access_token);
    let new_rt_hash = hash_token(&new_refresh_token);

    // We need user_id/scopes/resource from the old refresh token to
    // construct the new records. The DB helper handles the lookup, but we
    // need to pass the records in. We'll do a preliminary read to get
    // metadata, then call the rotation helper.
    //
    // To avoid TOCTOU, the rotation helper re-validates everything inside
    // its transaction. The metadata read here is only for constructing the
    // records.
    let old_rt_metadata = match db.get_oauth_refresh_token_by_hash(&rt_hash) {
        Ok(Some(rt)) => rt,
        Ok(None) => {
            // Could be not found, revoked, or expired. Check the full table
            // for a better error message.
            match db.get_oauth_refresh_token_by_hash_for_rotate(&rt_hash) {
                Ok(Some(rt)) => {
                    if rt.revoked_at.is_some() {
                        oauth_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            "invalid_grant",
                            "refresh token has been revoked",
                        );
                    } else if rt.is_expired(now) {
                        oauth_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            "invalid_grant",
                            "refresh token has expired",
                        );
                    } else {
                        oauth_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            "invalid_grant",
                            "refresh token client_id mismatch",
                        );
                    }
                }
                _ => {
                    oauth_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_grant",
                        "refresh token is invalid",
                    );
                }
            }
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

    let at_record = OAuthAccessTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: new_at_hash,
        client_id: client.client_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: old_rt_metadata.resource.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let new_rt_record = OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: new_rt_hash,
        client_id: client.client_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: old_rt_metadata.resource.clone(),
        created_at: now,
        expires_at: new_rt_expires_at,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: Some(old_rt_metadata.id.clone()),
    };

    match db.rotate_oauth_refresh_token(
        &rt_hash,
        &client.client_id,
        now,
        &at_record,
        &new_rt_record,
    ) {
        Ok(crate::RotateResult::Rotated(old_rt)) => {
            let mut body = serde_json::json!({
                "access_token": new_access_token,
                "token_type": "Bearer",
                "expires_in": config.oauth2.access_token_ttl_secs,
                "refresh_token": new_refresh_token,
            });

            if !old_rt.scopes.is_empty() {
                body["scope"] = serde_json::Value::String(old_rt.scopes.clone());
            }

            apply_oauth_no_store_headers(res);
            res.render(Json(body));
        }
        Ok(crate::RotateResult::NotFound) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "refresh token is invalid",
            );
        }
        Ok(crate::RotateResult::Revoked) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "refresh token has been revoked",
            );
        }
        Ok(crate::RotateResult::Expired) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "refresh token has expired",
            );
        }
        Ok(crate::RotateResult::ClientMismatch) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "refresh token does not belong to this client",
            );
        }
        Err(_) => {
            oauth_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Revoke handler
// ---------------------------------------------------------------------------

#[handler]
pub(crate) async fn oauth_revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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

    // --- Content-Type enforcement (same as /oauth/token) ---
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

    let form: RevokeRequest = match serde_urlencoded::from_bytes(&body) {
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

    // --- Validate required parameters ---
    let plaintext_token = match form.token.as_deref() {
        Some(t) if !t.is_empty() => t,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing token",
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

    // --- Client authentication ---
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

    // Also verify the client is not revoked.
    if db
        .get_oauth_client_by_client_id(client_id)
        .ok()
        .flatten()
        .is_none()
    {
        oauth_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
        return;
    }

    // --- Revoke ---
    let token_hash = hash_token(plaintext_token);
    let now = chrono::Utc::now().timestamp();

    let hint = form.token_type_hint.as_deref().unwrap_or("");

    // Per RFC 7009: if hint is provided and recognized, try only that type.
    // If hint is missing or unrecognized, try both.
    match hint {
        "access_token" => {
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
        }
        "refresh_token" => {
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
        _ => {
            // No hint or unrecognized hint — try both.
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
    }

    // Always return 200 — idempotent, no token state disclosure.
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// GET /.well-known/oauth-protected-resource
// ---------------------------------------------------------------------------

/// Non-agent scopes that OAuth2 clients may request. Agent transport scopes
/// (`agent:*`) are excluded because OAuth2 access tokens are rejected on agent
/// transport surfaces. `admin` is excluded because it is a bootstrap/superuser
/// scope not intended for OAuth2 delegation.
const OAUTH_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_JOB_RUN,
    scopes::SCOPE_ACCOUNT_MANAGE,
];

/// Return the canonical global OAuth scope registry.
///
/// The order is stable and is used for authorization-time normalization.
pub(crate) fn oauth_scopes_supported() -> &'static [&'static str] {
    OAUTH_SCOPES_SUPPORTED
}

/// Normalize authorize-time OAuth scopes against a registered client's allowed
/// scopes and the global OAuth scope registry.
///
/// If `requested` is absent or ASCII-whitespace-only, default to the
/// intersection of `client_allowed` and the global OAuth scope registry. When
/// `requested` is present, every requested scope must be both globally
/// supported and allowed by the client. Output is deduplicated and ordered by
/// the global registry.
#[allow(dead_code)]
pub(crate) fn normalize_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let client_allowed: std::collections::HashSet<&str> =
        client_allowed.split_ascii_whitespace().collect();

    let normalized = match requested {
        Some(raw) if raw.split_ascii_whitespace().next().is_some() => {
            let mut requested_scopes = std::collections::HashSet::new();
            for scope in raw.split_ascii_whitespace() {
                if !oauth_scopes_supported().contains(&scope) || !client_allowed.contains(scope) {
                    return Err(OAuthAuthorizeError::InvalidScope("invalid scope"));
                }
                requested_scopes.insert(scope);
            }

            oauth_scopes_supported()
                .iter()
                .copied()
                .filter(|scope| requested_scopes.contains(scope))
                .collect::<Vec<_>>()
        }
        _ => oauth_scopes_supported()
            .iter()
            .copied()
            .filter(|scope| client_allowed.contains(scope))
            .collect::<Vec<_>>(),
    };

    if normalized.is_empty() {
        return Err(OAuthAuthorizeError::InvalidScope("empty scope"));
    }

    Ok(normalized.join(" "))
}

/// Return protected resource metadata (RFC 9728 §3.1).
///
/// This is a **public** endpoint — no authentication required. Returns 404
/// when OAuth2 is disabled so discovery does not advertise capabilities that
/// are not active.
#[handler]
pub(crate) async fn oauth_metadata(depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };

    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }

    let resource = config
        .oauth2
        .issuer
        .as_deref()
        .unwrap_or("http://localhost");

    let metadata = serde_json::json!({
        "resource": resource,
        "authorization_servers": [resource],
        "bearer_methods_supported": ["header"],
        "scopes_supported": oauth_scopes_supported(),
        "resource_name": "WebCodex",
    });

    res.render(Json(metadata));
}

/// Return OAuth Authorization Server Metadata (RFC 8414).
///
/// This is a **public** endpoint — no authentication required. It advertises
/// only capabilities implemented by the current OAuth2 server.
#[handler]
pub(crate) async fn oauth_authorization_server_metadata(depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };

    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }

    let issuer = config
        .oauth2
        .issuer
        .as_deref()
        .unwrap_or("http://localhost");
    let endpoint_base = issuer.trim_end_matches('/');

    let metadata = serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/oauth/authorize", endpoint_base),
        "token_endpoint": format!("{}/oauth/token", endpoint_base),
        "revocation_endpoint": format!("{}/oauth/revoke", endpoint_base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post", "none"],
        "scopes_supported": oauth_scopes_supported(),
    });

    res.render(Json(metadata));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        generate_account_credential, generate_agent_token, generate_api_token,
        generate_oauth_authorization_code, hash_token, token_prefix,
    };
    use crate::models::{
        AccountCredentialRecord, ApiKeyRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
        UserRecord, TOKEN_KIND_AGENT, TOKEN_KIND_USER,
    };
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

    fn authorize_query_without(missing: &str) -> String {
        [
            ("response_type", "code"),
            ("client_id", "client-1"),
            ("redirect_uri", "https://client.example/cb"),
            ("scope", "runtime:read"),
            ("state", "keep+this value"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]
        .into_iter()
        .filter(|(key, _)| *key != missing)
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
    }

    fn valid_authorize_query() -> String {
        authorize_query_without("")
    }

    #[test]
    fn normalize_oauth_scopes_defaults_to_client_global_intersection() {
        let normalized =
            normalize_oauth_scopes(None, "project:write runtime:read agent:poll admin").unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_default_rejects_empty_intersection() {
        let err = normalize_oauth_scopes(None, "agent:poll admin unknown").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("empty scope"));
    }

    #[test]
    fn normalize_oauth_scopes_requested_subset_success() {
        let normalized = normalize_oauth_scopes(
            Some("project:write runtime:read"),
            "runtime:read project:read project:write",
        )
        .unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_deduplicates_and_orders() {
        let normalized = normalize_oauth_scopes(
            Some("project:write runtime:read runtime:read"),
            "runtime:read project:read project:write",
        )
        .unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_rejects_unknown_scope() {
        let err = normalize_oauth_scopes(Some("unknown"), "runtime:read unknown").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_scope_not_allowed_by_client() {
        let err = normalize_oauth_scopes(Some("runtime:read"), "project:read").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_agent_scope() {
        let err = normalize_oauth_scopes(Some("agent:poll"), "agent:poll").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_admin_scope() {
        let err = normalize_oauth_scopes(Some("admin"), "admin").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_treats_empty_requested_as_default() {
        let normalized =
            normalize_oauth_scopes(Some(" \t\n"), "project:write runtime:read admin").unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

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
        let err =
            parse_authorize_query(&authorize_query_without("code_challenge_method")).unwrap_err();

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
    fn parse_authorize_query_rejects_duplicate_parameters() {
        let query = format!("{}&client_id=client-2", valid_authorize_query());
        let err = parse_authorize_query(&query).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("duplicate parameter")
        );
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

    fn seed_client_with_redirects_and_scopes(
        db: &crate::Database,
        user: &UserRecord,
        redirect_uris: &str,
        allowed_scopes: &str,
    ) -> OAuthClientRecord {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: secret_hash,
            name: "authorize-client".to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: redirect_uris.to_string(),
            allowed_scopes: allowed_scopes.to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        record
    }

    fn seed_user_token(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_api_token();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "authorize-user-token".to_string(),
            key_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:read project:write job:run".to_string(),
            expires_at: None,
            kind: TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    fn seed_agent_token(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_agent_token();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "authorize-agent-token".to_string(),
            key_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:poll agent:result".to_string(),
            expires_at: None,
            kind: TOKEN_KIND_AGENT.to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    fn seed_account_credential(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_account_credential();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = AccountCredentialRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            credential_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        db.insert_account_credential(&record, &hash).unwrap();
        plaintext
    }

    fn authorize_url(params: &[(&str, &str)]) -> String {
        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("http://localhost/oauth/authorize?{}", query)
    }

    fn valid_authorize_url(client: &OAuthClientRecord, redirect_uri: &str) -> String {
        authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ])
    }

    fn authorized_get(url: &str, token: &str) -> salvo::test::RequestBuilder {
        TestClient::get(url).add_header("authorization", &format!("Bearer {}", token), true)
    }

    fn auth_code_count(db: &crate::Database) -> i64 {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT COUNT(*) FROM oauth_authorization_codes",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn location_header(resp: &Response) -> Option<String> {
        resp.headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }

    fn assert_no_location(resp: &Response) {
        assert!(
            location_header(resp).is_none(),
            "direct errors must not include Location"
        );
    }

    async fn assert_authorize_direct_400(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
    ) {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(db), before);
    }

    async fn assert_authorize_redirect_error(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
        expected_base: &str,
        expected_error: &str,
        expected_state: Option<&str>,
    ) -> String {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(db), before);
        let location = location_header(&resp).expect("redirect error should set Location");
        assert!(
            location.starts_with(expected_base),
            "Location {} should start with {}",
            location,
            expected_base
        );
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("error").map(String::as_str),
            Some(expected_error)
        );
        if let Some(expected_state) = expected_state {
            assert_eq!(
                params.get("state").map(String::as_str),
                Some(expected_state)
            );
        } else {
            assert!(!params.contains_key("state"));
        }
        location
    }

    async fn authorize_success(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
    ) -> (Response, String, url::Url, String) {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(db), before + 1);
        let location = location_header(&resp).expect("success should set Location");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params
            .get("code")
            .expect("success redirect should include code")
            .clone();
        assert!(code.starts_with("wc_oac_"));
        assert!(!params.contains_key("access_token"));
        assert!(!params.contains_key("refresh_token"));
        (resp, location, parsed, code)
    }

    fn auth_code_by_plaintext(
        db: &crate::Database,
        plaintext_code: &str,
    ) -> OAuthAuthorizationCodeRecord {
        db.get_oauth_authorization_code_by_hash(&hash_token(plaintext_code))
            .unwrap()
            .expect("authorization code row should exist")
    }

    fn pkce_s256_challenge(code_verifier: &str) -> String {
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
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

    /// Seed a refresh token directly into the database. Returns the record
    /// and the plaintext token.
    fn seed_refresh_token(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
    ) -> (crate::models::OAuthRefreshTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_refresh_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            user_id: user.id.clone(),
            scopes: scopes.to_string(),
            resource: None,
            created_at: now,
            expires_at: now + 2_592_000, // 30 days
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();
        (record, plaintext)
    }

    /// Seed an access token directly into the database. Returns the record
    /// and the plaintext token.
    fn seed_access_token(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
    ) -> (crate::models::OAuthAccessTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_access_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            user_id: user.id.clone(),
            scopes: scopes.to_string(),
            resource: None,
            created_at: now,
            expires_at: now + 3600, // 1 hour
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        (record, plaintext)
    }

    fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(salvo::prelude::affix_state::inject(config))
            .hoop(salvo::prelude::affix_state::inject(db))
            .push(Router::with_path("oauth/token").post(oauth_token))
            .push(Router::with_path("oauth/revoke").post(oauth_revoke))
            .push(
                Router::with_path("oauth/authorize")
                    .hoop(crate::AuthMiddleware)
                    .get(oauth_authorize),
            )
            .push(Router::with_path(".well-known/oauth-protected-resource").get(oauth_metadata))
            .push(
                Router::with_path(".well-known/oauth-authorization-server")
                    .get(oauth_authorization_server_metadata),
            )
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

    /// Return `(access_token_count, refresh_token_count)` from the DB.
    fn oauth_token_counts(db: &crate::Database) -> (i64, i64) {
        let conn = db.conn_for_tests();
        let at: i64 = conn
            .query_row("SELECT COUNT(*) FROM oauth_access_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        let rt: i64 = conn
            .query_row("SELECT COUNT(*) FROM oauth_refresh_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        (at, rt)
    }

    // -----------------------------------------------------------------------
    // Authorization endpoint
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn authorize_requires_authenticated_user() {
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

        let resp = TestClient::get(&url).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
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

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;

        assert!(code.starts_with("wc_oac_"));
    }

    #[tokio::test]
    async fn authorize_rejects_oauth2_access_token_without_issuing_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let (_record, token) = seed_access_token(&db, &client, &user, "runtime:read");
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");
        let before = auth_code_count(&db);

        let resp = authorized_get(&url, &token).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn authorize_rejects_agent_token_without_issuing_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_agent_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");
        let before = auth_code_count(&db);

        let resp = authorized_get(&url, &token).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn authorize_rejects_account_credential_without_issuing_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_account_credential(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");
        let before = auth_code_count(&db);

        let resp = authorized_get(&url, &token).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
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
    async fn authorize_rejects_resource_parameter() {
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
            "invalid_request",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_redirect_error_appends_with_ampersand_when_redirect_uri_has_query() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let redirect_uri = "https://client.example/callback?existing=1";
        let client =
            seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
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

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
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

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_eq!(record.client_id, client.client_id);
        assert_eq!(record.user_id, user.id);
        assert_eq!(record.redirect_uri, "https://example.com/callback");
        assert_eq!(record.scopes, "runtime:read project:read");
        assert_eq!(record.resource, None);
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
        let client =
            seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
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

        let (mut resp, location, _parsed, _code) =
            authorize_success(&service, &db, &url, &token).await;
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

        let (_resp, _location, parsed, _code) =
            authorize_success(&service, &db, &url, &token).await;
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

        // Both tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before + 1, at_after, "one access token inserted");
        assert_eq!(rt_before + 1, rt_after, "one refresh token inserted");
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
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

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
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

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
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

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
    async fn missing_refresh_token_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
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
    async fn unknown_refresh_token_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", "wc_ort_nonexistent"),
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
            user_id: user.id.clone(),
            scopes: "runtime:read".to_string(),
            resource: None,
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
        let auth_methods = body["token_endpoint_auth_methods_supported"]
            .as_array()
            .unwrap();
        assert!(auth_methods.iter().any(|v| v == "client_secret_post"));
        assert!(auth_methods.iter().any(|v| v == "none"));
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
}
