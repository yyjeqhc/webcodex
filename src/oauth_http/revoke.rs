use salvo::http::header::CONTENT_TYPE;
use salvo::prelude::*;

use crate::auth::hash_token;

use super::{apply_oauth_no_store_headers, oauth_error, MAX_OAUTH_TOKEN_FORM_BYTES};

#[derive(Debug, serde::Deserialize)]
struct RevokeRequest {
    token: Option<String>,
    token_type_hint: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

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
