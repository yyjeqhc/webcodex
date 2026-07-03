use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::http::header::CONTENT_TYPE;
use salvo::prelude::*;
use sha2::{Digest, Sha256};

use crate::auth::{generate_oauth_access_token, generate_oauth_refresh_token, hash_token};
use crate::models::{OAuthAccessTokenRecord, OAuthRefreshTokenRecord};

use super::{apply_oauth_no_store_headers, oauth_error, MAX_OAUTH_TOKEN_FORM_BYTES};

/// Verify a PKCE S256 code verifier against the stored code challenge.
///
/// Computes `BASE64URL-NO-PAD(SHA256(ASCII(verifier)))` and compares it to
/// `challenge` using constant-time equality.
pub(crate) fn verify_pkce_s256(verifier: &str, challenge: &str) -> bool {
    let digest = Sha256::digest(verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(digest);
    crate::config::constant_time_eq(computed.as_bytes(), challenge.as_bytes())
}

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
        subject_kind: code_record.subject_kind.clone(),
        subject_id: code_record.subject_id.clone(),
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: code_record.resource.clone(),
        shared_key_hash: code_record.shared_key_hash.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let rt_record = OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: rt_hash,
        client_id: client.client_id.clone(),
        subject_kind: code_record.subject_kind.clone(),
        subject_id: code_record.subject_id.clone(),
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: code_record.resource.clone(),
        shared_key_hash: code_record.shared_key_hash.clone(),
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

    // We need user_id/scopes/resource/shared_key_hash from the old refresh token
    // to construct the new records. The DB helper handles the lookup, but we
    // need to pass the records in. We'll do a preliminary read to get metadata,
    // then call the rotation helper.
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
        subject_kind: old_rt_metadata.subject_kind.clone(),
        subject_id: old_rt_metadata.subject_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: old_rt_metadata.resource.clone(),
        shared_key_hash: old_rt_metadata.shared_key_hash.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let new_rt_record = OAuthRefreshTokenRecord {
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: new_rt_hash,
        client_id: client.client_id.clone(),
        subject_kind: old_rt_metadata.subject_kind.clone(),
        subject_id: old_rt_metadata.subject_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: old_rt_metadata.resource.clone(),
        shared_key_hash: old_rt_metadata.shared_key_hash.clone(),
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
