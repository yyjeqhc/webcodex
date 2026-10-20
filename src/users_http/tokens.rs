use crate::auth::{
    clean_token_name, generate_api_token, hash_token, is_unique_constraint_error,
    normalize_token_hash, scopes_to_string, token_prefix, validate_scopes, validate_token_prefix,
    validate_username, SCOPE_ADMIN,
};
use crate::management_http::{
    auth_context, database, parse_json, require_admin_or_self, require_user_by_username,
    JsonApiError, JsonApiResult,
};
use crate::models::ApiKeyRecord;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use super::reject_agent_token;

/// Maximum number of tokens returned by `listApiTokens`.
const MAX_TOKENS_LIST: usize = 200;

#[derive(Debug, Deserialize)]
pub(crate) struct CreateApiTokenRequest {
    pub username: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterApiTokenHashRequest {
    pub username: String,
    #[serde(default)]
    pub name: Option<String>,
    pub token_hash: String,
    pub token_prefix: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListApiTokensRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevokeApiTokenRequest {
    pub token_id: String,
    pub username: String,
}

/// Token metadata returned by list/revoke. Never includes `key_hash` or the
/// plaintext token.
fn token_summary(key: &ApiKeyRecord) -> Value {
    json!({
        "id": key.id,
        "user_id": key.user_id,
        "name": key.name,
        "token_prefix": key.key_prefix,
        "scopes": key.scopes_vec(),
        "created_at": key.created_at,
        "last_used_at": key.last_used_at,
        "expires_at": key.expires_at,
        "revoked_at": key.revoked_at,
    })
}

/// `POST /api/tokens/create` — operationId `createApiToken`.
///
/// Bootstrap/admin may create a token for any user; a normal user may create a
/// token only for themselves. Returns the plaintext token **exactly once**
/// alongside the token metadata (prefix, scopes, etc.).
#[handler]
pub(crate) async fn tokens_create(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: CreateApiTokenRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    reject_agent_token(auth)?;
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let token_name = clean_token_name(body.name, "default").map_err(JsonApiError::bad_request)?;

    let scopes =
        validate_scopes(&body.scopes.unwrap_or_default()).map_err(JsonApiError::bad_request)?;
    // A normal user may not grant themselves the `admin` scope; only
    // bootstrap/admin callers can mint admin-scoped tokens.
    if scopes.iter().any(|s| s == SCOPE_ADMIN) && !auth.is_admin_caller() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "only admin/bootstrap callers may grant the admin scope",
        ));
    }
    if let Some(exp) = body.expires_at {
        if exp <= chrono::Utc::now().timestamp() {
            return Err(JsonApiError::bad_request(
                "expires_at must be in the future",
            ));
        }
    }

    let db = database(depot)?;

    let user = require_user_by_username(&db, &username)?;
    if user.is_disabled() {
        return Err(JsonApiError::new(StatusCode::FORBIDDEN, "user is disabled"));
    }

    let plaintext = generate_api_token();
    let prefix = token_prefix(&plaintext);
    let key_hash = hash_token(&plaintext);
    let now = chrono::Utc::now().timestamp();
    let record = ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: token_name,
        key_prefix: prefix,
        created_at: now,
        last_used_at: None,
        revoked_at: None,
        scopes: scopes_to_string(&scopes),
        expires_at: body.expires_at,
        kind: crate::models::TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &key_hash)
        .map_err(JsonApiError::internal)?;

    // Return the plaintext token exactly once. It is never stored and never
    // logged. The response also carries the safe metadata (prefix, scopes).
    Ok(Json(json!({
        "success": true,
        "token": plaintext,
        "token_prefix": record.key_prefix,
        "token_id": record.id,
        "name": record.name,
        "username": user.username,
        "user_id": user.id,
        "scopes": scopes,
        "created_at": record.created_at,
        "expires_at": record.expires_at,
    })))
}

#[handler]
pub(crate) async fn tokens_register_hash(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: RegisterApiTokenHashRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    reject_agent_token(auth)?;
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let token_name = clean_token_name(body.name, "default").map_err(JsonApiError::bad_request)?;
    let token_hash = normalize_token_hash(&body.token_hash).map_err(JsonApiError::bad_request)?;
    let token_prefix =
        validate_token_prefix(&body.token_prefix, "wc_pat_").map_err(JsonApiError::bad_request)?;
    let scopes = validate_scopes(&body.scopes).map_err(JsonApiError::bad_request)?;
    if scopes.iter().any(|s| s == SCOPE_ADMIN) && !auth.is_admin_caller() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "only admin/bootstrap callers may grant the admin scope",
        ));
    }
    if let Some(exp) = body.expires_at {
        if exp <= chrono::Utc::now().timestamp() {
            return Err(JsonApiError::bad_request(
                "expires_at must be in the future",
            ));
        }
    }

    let db = database(depot)?;
    let user = require_user_by_username(&db, &username)?;
    if user.is_disabled() {
        return Err(JsonApiError::new(StatusCode::FORBIDDEN, "user is disabled"));
    }
    if db
        .get_account_credential_by_hash(&token_hash)
        .map_err(JsonApiError::internal)?
        .is_some()
    {
        return Err(JsonApiError::new(
            StatusCode::CONFLICT,
            "credential hash conflict",
        ));
    }

    let now = chrono::Utc::now().timestamp();
    let record = ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: token_name,
        key_prefix: token_prefix,
        created_at: now,
        last_used_at: None,
        revoked_at: None,
        scopes: scopes_to_string(&scopes),
        expires_at: body.expires_at,
        kind: crate::models::TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &token_hash).map_err(|e| {
        if is_unique_constraint_error(&e) {
            JsonApiError::new(StatusCode::CONFLICT, "token hash already exists")
        } else {
            JsonApiError::internal(e)
        }
    })?;

    Ok(Json(json!({
        "success": true,
        "token": token_summary(&record),
    })))
}

/// `POST /api/tokens/list` — operationId `listApiTokens`.
///
/// Bootstrap/admin may list anyone; a user may list only their own tokens.
/// Returns token metadata only — never `key_hash` or the plaintext token.
#[handler]
pub(crate) async fn tokens_list(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: ListApiTokensRequest = parse_json(req).await?;
    let auth = auth_context(depot)?;
    reject_agent_token(auth)?;
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let db = database(depot)?;
    let user = require_user_by_username(&db, &username)?;
    let mut keys = db
        .list_api_keys_by_user(&user.id)
        .map_err(JsonApiError::internal)?;
    keys.truncate(MAX_TOKENS_LIST);
    let tokens: Vec<Value> = keys.iter().map(token_summary).collect();
    Ok(Json(json!({
        "success": true,
        "username": user.username,
        "user_id": user.id,
        "tokens": tokens,
        "count": tokens.len(),
    })))
}

/// `POST /api/tokens/revoke` — operationId `revokeApiToken`.
///
/// Bootstrap/admin may revoke anyone's token; a user may revoke only their own.
/// Idempotent: revoking an already-revoked token succeeds and returns the
/// metadata with the original `revoked_at`. Never returns the plaintext token.
#[handler]
pub(crate) async fn tokens_revoke(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: RevokeApiTokenRequest = parse_json(req).await?;
    let auth = auth_context(depot)?;
    reject_agent_token(auth)?;
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let token_id = body.token_id.trim().to_string();
    if token_id.is_empty() {
        return Err(JsonApiError::bad_request("token_id cannot be empty"));
    }

    let db = database(depot)?;
    let user = require_user_by_username(&db, &username)?;

    // Verify the token actually belongs to the target user before revoking,
    // so a user cannot revoke another user's token by guessing an id.
    let existing = db
        .get_api_key_by_id(&token_id)
        .map_err(JsonApiError::internal)?;
    let existing = match existing {
        Some(k) if k.user_id == user.id => k,
        Some(_) => {
            return Err(JsonApiError::new(
                StatusCode::FORBIDDEN,
                "token does not belong to the specified user",
            ));
        }
        None => {
            return Err(JsonApiError::new(StatusCode::NOT_FOUND, "token not found"));
        }
    };

    let now = chrono::Utc::now().timestamp();
    let revoked = db
        .revoke_api_key(&token_id, now)
        .map_err(JsonApiError::internal)?
        .unwrap_or(existing);
    Ok(Json(json!({
        "success": true,
        "token": token_summary(&revoked),
    })))
}
