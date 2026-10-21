use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::prelude::*;
use sha2::{Digest, Sha256};

use crate::auth::{generate_oauth_access_token, generate_oauth_refresh_token, hash_token};
use crate::models::{OAuthAccessTokenRecord, OAuthRefreshTokenRecord};

use super::responses::{
    authenticate_token_client, enabled_token_config, parse_token_form, required_token_parameter,
    OAuthTokenError,
};
use super::{apply_oauth_no_store_headers, validate_authorize_resource};

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
    resource: Option<String>,
}

/// Resolve the RFC 8707 resource indicator for a token grant while retaining
/// compatibility with pre-resource clients. When the authorization grant is
/// already resource-bound, a token request may only repeat the same target.
fn resolve_token_resource(
    config: &crate::Config,
    requested: Option<&str>,
    authorized: Option<&str>,
) -> Result<Option<String>, ()> {
    let Some(requested) = requested else {
        return Ok(authorized.map(str::to_string));
    };
    let normalized = validate_authorize_resource(Some(requested), config).map_err(|_| ())?;
    if let Some(authorized) = authorized {
        if normalized.as_deref() != Some(authorized) {
            return Err(());
        }
    }
    Ok(normalized)
}

#[handler]
pub(crate) async fn oauth_token(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), OAuthTokenError> {
    let config = enabled_token_config(depot)?;
    let form: TokenRequest = parse_token_form(req).await?;
    let grant_type = form
        .grant_type
        .as_deref()
        .ok_or_else(|| OAuthTokenError::invalid_request("missing grant_type"))?;

    // Grant validation precedes client parameters, authentication and DB access.
    if !matches!(grant_type, "authorization_code" | "refresh_token") {
        return Err(OAuthTokenError::new(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "only authorization_code and refresh_token grants are supported",
        ));
    }
    if grant_type == "refresh_token" && form.scope.is_some() {
        return Err(OAuthTokenError::invalid_request(
            "scope narrowing is not supported for refresh_token grant yet",
        ));
    }
    let (db, client) = authenticate_token_client(
        depot,
        form.client_id.as_deref(),
        form.client_secret.as_deref(),
    )?;
    let now = chrono::Utc::now().timestamp();
    let body = if grant_type == "authorization_code" {
        handle_authorization_code_grant(&config, &db, &client, &form, now)?
    } else {
        handle_refresh_token_grant(&config, &db, &client, &form, now)?
    };
    apply_oauth_no_store_headers(res);
    res.render(Json(body));
    Ok(())
}

fn handle_authorization_code_grant(
    config: &crate::Config,
    db: &crate::Database,
    client: &crate::models::OAuthClientRecord,
    form: &TokenRequest,
    now: i64,
) -> Result<serde_json::Value, OAuthTokenError> {
    let plaintext_code = required_token_parameter(form.code.as_deref(), "code")?;

    let redirect_uri = required_token_parameter(form.redirect_uri.as_deref(), "redirect_uri")?;

    // --- Read authorization code metadata (without consuming) ---
    let code_hash = hash_token(plaintext_code);

    let code_record = db
        .get_oauth_authorization_code_by_hash(&code_hash)
        .map_err(|_| OAuthTokenError::server_error("internal error"))?
        .ok_or_else(|| {
            OAuthTokenError::invalid_grant(
                "authorization code is invalid, expired, or already used",
            )
        })?;

    // --- Pre-exchange validation (using metadata, code is NOT yet consumed) ---

    if crate::auth::validate_project_share_grant_subject(
        config,
        &code_record.subject_kind,
        &code_record.subject_id,
    )
    .is_err()
    {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        return Err(OAuthTokenError::invalid_grant(
            "authorization grant is no longer active",
        ));
    }
    if code_record.subject_kind == crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND {
        let owner_matches = crate::auth::parse_project_share_subject_id(&code_record.subject_id)
            .ok()
            .is_some_and(|(grant_id, _)| {
                client.is_project_grant_owned()
                    && client.owner_project_grant_id.as_deref() == Some(grant_id)
            });
        if !owner_matches {
            let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
            return Err(OAuthTokenError::invalid_grant(
                "authorization grant does not belong to this client",
            ));
        }
    }

    if code_record.client_id != client.client_id {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        return Err(OAuthTokenError::invalid_grant(
            "authorization code was not issued to this client",
        ));
    }

    if code_record.redirect_uri != redirect_uri {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        return Err(OAuthTokenError::invalid_grant(
            "redirect_uri does not match",
        ));
    }

    // --- PKCE S256 (before code consumption) ---
    let require_pkce = config.oauth2.require_pkce;
    if let Some(ref challenge) = code_record.code_challenge {
        let verifier = match form.code_verifier.as_deref() {
            Some(v) if !v.is_empty() => v,
            _ => {
                let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
                return Err(OAuthTokenError::invalid_grant(
                    "missing code_verifier for PKCE",
                ));
            }
        };

        match code_record.code_challenge_method.as_deref() {
            Some("S256") => {}
            _ => {
                let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
                return Err(OAuthTokenError::invalid_grant(
                    "unsupported code_challenge_method; only S256 is supported",
                ));
            }
        }

        if !verify_pkce_s256(verifier, challenge) {
            let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
            return Err(OAuthTokenError::invalid_grant("PKCE verification failed"));
        }
    } else if require_pkce {
        let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
        return Err(OAuthTokenError::invalid_grant(
            "PKCE is required but no code_challenge was provided during authorization",
        ));
    }

    let token_resource = match resolve_token_resource(
        config,
        form.resource.as_deref(),
        code_record.resource.as_deref(),
    ) {
        Ok(resource) => resource,
        Err(()) => {
            let _ = db.consume_oauth_authorization_code_by_hash(&code_hash, now);
            return Err(OAuthTokenError::invalid_target(
                "resource does not match authorization grant",
            ));
        }
    };

    // --- All validations passed — transactional exchange ---
    let access_token = generate_oauth_access_token();
    let refresh_token = generate_oauth_refresh_token();
    let at_hash = hash_token(&access_token);
    let rt_hash = hash_token(&refresh_token);
    let at_expires_at = now + config.oauth2.access_token_ttl_secs;
    let rt_expires_at = now + config.oauth2.refresh_token_ttl_secs;

    let at_record = OAuthAccessTokenRecord {
        admin_authority: code_record.admin_authority,
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: at_hash,
        client_id: client.client_id.clone(),
        subject_kind: code_record.subject_kind.clone(),
        subject_id: code_record.subject_id.clone(),
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: token_resource.clone(),
        shared_key_hash: code_record.shared_key_hash.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let rt_record = OAuthRefreshTokenRecord {
        admin_authority: code_record.admin_authority,
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: rt_hash,
        client_id: client.client_id.clone(),
        subject_kind: code_record.subject_kind.clone(),
        subject_id: code_record.subject_id.clone(),
        user_id: code_record.user_id.clone(),
        scopes: code_record.scopes.clone(),
        resource: token_resource,
        shared_key_hash: code_record.shared_key_hash.clone(),
        created_at: now,
        expires_at: rt_expires_at,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
    };

    let code_record = db
        .exchange_oauth_authorization_code_for_tokens(&code_hash, now, &at_record, &rt_record)
        .map_err(|_| OAuthTokenError::server_error("internal error"))?
        .ok_or_else(|| {
            OAuthTokenError::invalid_grant(
                "authorization code is invalid, expired, or already used",
            )
        })?;

    Ok(token_response(
        config,
        access_token,
        refresh_token,
        &code_record.scopes,
    ))
}

fn handle_refresh_token_grant(
    config: &crate::Config,
    db: &crate::Database,
    client: &crate::models::OAuthClientRecord,
    form: &TokenRequest,
    now: i64,
) -> Result<serde_json::Value, OAuthTokenError> {
    let plaintext_rt = required_token_parameter(form.refresh_token.as_deref(), "refresh_token")?;

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
    let old_rt_metadata = match db
        .get_oauth_refresh_token_by_hash(&rt_hash)
        .map_err(|_| OAuthTokenError::server_error("internal error"))?
    {
        Some(rt) => rt,
        None => {
            // Preserve the existing diagnostic lookup without rotating or
            // changing the refresh token when its metadata is unavailable.
            let description = match db.get_oauth_refresh_token_by_hash_for_rotate(&rt_hash) {
                Ok(Some(rt)) if rt.revoked_at.is_some() => "refresh token has been revoked",
                Ok(Some(rt)) if rt.is_expired(now) => "refresh token has expired",
                Ok(Some(_)) => "refresh token client_id mismatch",
                _ => "refresh token is invalid",
            };
            return Err(OAuthTokenError::invalid_grant(description));
        }
    };

    if crate::auth::validate_project_share_grant_subject(
        config,
        &old_rt_metadata.subject_kind,
        &old_rt_metadata.subject_id,
    )
    .is_err()
    {
        return Err(OAuthTokenError::invalid_grant(
            "refresh token is no longer active",
        ));
    }
    if old_rt_metadata.subject_kind == crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND {
        let owner_matches =
            crate::auth::parse_project_share_subject_id(&old_rt_metadata.subject_id)
                .ok()
                .is_some_and(|(grant_id, _)| {
                    client.is_project_grant_owned()
                        && client.owner_project_grant_id.as_deref() == Some(grant_id)
                });
        if !owner_matches {
            return Err(OAuthTokenError::invalid_grant(
                "refresh token does not belong to this client",
            ));
        }
    }

    let token_resource = resolve_token_resource(
        config,
        form.resource.as_deref(),
        old_rt_metadata.resource.as_deref(),
    )
    .map_err(|()| OAuthTokenError::invalid_target("resource does not match refresh token"))?;

    let at_record = OAuthAccessTokenRecord {
        admin_authority: old_rt_metadata.admin_authority,
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: new_at_hash,
        client_id: client.client_id.clone(),
        subject_kind: old_rt_metadata.subject_kind.clone(),
        subject_id: old_rt_metadata.subject_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: token_resource.clone(),
        shared_key_hash: old_rt_metadata.shared_key_hash.clone(),
        created_at: now,
        expires_at: at_expires_at,
        revoked_at: None,
        last_used_at: None,
    };

    let new_rt_record = OAuthRefreshTokenRecord {
        admin_authority: old_rt_metadata.admin_authority,
        id: uuid::Uuid::new_v4().to_string(),
        token_hash: new_rt_hash,
        client_id: client.client_id.clone(),
        subject_kind: old_rt_metadata.subject_kind.clone(),
        subject_id: old_rt_metadata.subject_id.clone(),
        user_id: old_rt_metadata.user_id.clone(),
        scopes: old_rt_metadata.scopes.clone(),
        resource: token_resource,
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
        Ok(crate::RotateResult::Rotated(old_rt)) => Ok(token_response(
            config,
            new_access_token,
            new_refresh_token,
            &old_rt.scopes,
        )),
        Ok(crate::RotateResult::NotFound) => {
            return Err(OAuthTokenError::invalid_grant("refresh token is invalid"));
        }
        Ok(crate::RotateResult::Revoked) => {
            return Err(OAuthTokenError::invalid_grant(
                "refresh token has been revoked",
            ));
        }
        Ok(crate::RotateResult::Expired) => {
            return Err(OAuthTokenError::invalid_grant("refresh token has expired"));
        }
        Ok(crate::RotateResult::ClientMismatch) => {
            return Err(OAuthTokenError::invalid_grant(
                "refresh token does not belong to this client",
            ));
        }
        Err(_) => {
            return Err(OAuthTokenError::server_error("internal error"));
        }
    }
}

fn token_response(
    config: &crate::Config,
    access_token: String,
    refresh_token: String,
    scopes: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": config.oauth2.access_token_ttl_secs,
        "refresh_token": refresh_token,
    });
    if !scopes.is_empty() {
        body["scope"] = serde_json::Value::String(scopes.to_string());
    }
    body
}
