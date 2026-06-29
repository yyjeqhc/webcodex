//! OAuth2 token and revocation endpoints.
//!
//! - `POST /oauth/token` — token endpoint (authorization_code, refresh_token)
//! - `POST /oauth/revoke` — token revocation endpoint (RFC 7009)
//!
//! Both are **public** endpoints (no `AuthMiddleware`); clients authenticate
//! via `client_id` + `client_secret` in the form body.
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
}
