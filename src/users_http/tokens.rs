use crate::auth::{
    clean_token_name, generate_api_token, hash_token, is_unique_constraint_error,
    normalize_token_hash, scopes_to_string, token_prefix, validate_scopes, validate_token_prefix,
    validate_username, AuthContext, SCOPE_ADMIN,
};
use crate::json_error;
use crate::models::ApiKeyRecord;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{reject_agent_token, require_admin_or_self, require_user_by_username};

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
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateApiTokenScopesRequest {
    pub username: String,
    pub token_id: String,
    pub scopes: Vec<String>,
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
pub(crate) async fn tokens_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: CreateApiTokenRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {}", e),
            ));
            return;
        }
    };

    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }
    let username = match validate_username(&body.username) {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if let Err((code, msg)) = require_admin_or_self(auth, &username) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let token_name = match clean_token_name(body.name, "default") {
        Ok(n) => n,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };

    let scopes = match validate_scopes(&body.scopes.unwrap_or_default()) {
        Ok(s) => s,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    // A normal user may not grant themselves the `admin` scope; only
    // bootstrap/admin callers can mint admin-scoped tokens.
    if scopes.iter().any(|s| s == SCOPE_ADMIN) && !auth.is_admin_caller() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "only admin/bootstrap callers may grant the admin scope",
        ));
        return;
    }
    if let Some(exp) = body.expires_at {
        if exp <= chrono::Utc::now().timestamp() {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "expires_at must be in the future",
            ));
            return;
        }
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };

    let user = match require_user_by_username(&db, &username) {
        Ok(u) => u,
        Err((code, msg)) => {
            res.status_code(code);
            res.render(json_error(code, msg));
            return;
        }
    };
    if user.is_disabled() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(StatusCode::FORBIDDEN, "user is disabled"));
        return;
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
    if let Err(e) = db.insert_api_key(&record, &key_hash) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        return;
    }

    // Return the plaintext token exactly once. It is never stored and never
    // logged. The response also carries the safe metadata (prefix, scopes).
    res.render(Json(json!({
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
    })));
}

#[handler]
pub(crate) async fn tokens_register_hash(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: RegisterApiTokenHashRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {}", e),
            ));
            return;
        }
    };

    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }
    let username = match validate_username(&body.username) {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if let Err((code, msg)) = require_admin_or_self(auth, &username) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let token_name = match clean_token_name(body.name, "default") {
        Ok(n) => n,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let token_hash = match normalize_token_hash(&body.token_hash) {
        Ok(h) => h,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let token_prefix = match validate_token_prefix(&body.token_prefix, "wc_pat_") {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let scopes = match validate_scopes(&body.scopes) {
        Ok(s) => s,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if scopes.iter().any(|s| s == SCOPE_ADMIN) && !auth.is_admin_caller() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "only admin/bootstrap callers may grant the admin scope",
        ));
        return;
    }
    if let Some(exp) = body.expires_at {
        if exp <= chrono::Utc::now().timestamp() {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "expires_at must be in the future",
            ));
            return;
        }
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let user = match require_user_by_username(&db, &username) {
        Ok(u) => u,
        Err((code, msg)) => {
            res.status_code(code);
            res.render(json_error(code, msg));
            return;
        }
    };
    if user.is_disabled() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(StatusCode::FORBIDDEN, "user is disabled"));
        return;
    }
    match db.get_account_credential_by_hash(&token_hash) {
        Ok(Some(_)) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(json_error(StatusCode::CONFLICT, "credential hash conflict"));
            return;
        }
        Ok(None) => {}
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
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
    if let Err(e) = db.insert_api_key(&record, &token_hash) {
        if is_unique_constraint_error(&e) {
            res.status_code(StatusCode::CONFLICT);
            res.render(json_error(
                StatusCode::CONFLICT,
                "token hash already exists",
            ));
        } else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
        return;
    }

    res.render(Json(json!({
        "success": true,
        "token": token_summary(&record),
    })));
}

/// `POST /api/tokens/list` — operationId `listApiTokens`.
///
/// Bootstrap/admin may list anyone; a user may list only their own tokens.
/// Returns token metadata only — never `key_hash` or the plaintext token.
#[handler]
pub(crate) async fn tokens_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: ListApiTokensRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {}", e),
            ));
            return;
        }
    };
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }
    let username = match validate_username(&body.username) {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if let Err((code, msg)) = require_admin_or_self(auth, &username) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let user = match require_user_by_username(&db, &username) {
        Ok(u) => u,
        Err((code, msg)) => {
            res.status_code(code);
            res.render(json_error(code, msg));
            return;
        }
    };
    let mut keys = match db.list_api_keys_by_user(&user.id) {
        Ok(k) => k,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    keys.truncate(MAX_TOKENS_LIST);
    let tokens: Vec<Value> = keys.iter().map(token_summary).collect();
    res.render(Json(json!({
        "success": true,
        "username": user.username,
        "user_id": user.id,
        "tokens": tokens,
        "count": tokens.len(),
    })));
}

/// `POST /api/tokens/update_scopes` — replace the scopes on one existing user PAT.
///
/// This preserves the token id/hash/prefix, so an existing client credential does
/// not need to be rotated. Bootstrap/admin may manage any user; normal users may
/// update only their own tokens. Revoked, expired, and agent tokens are rejected.
#[handler]
pub(crate) async fn tokens_update_scopes(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: UpdateApiTokenScopesRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {error}"),
            ));
            return;
        }
    };
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, message)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, message));
        return;
    }
    let username = match validate_username(&body.username) {
        Ok(username) => username,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, error));
            return;
        }
    };
    if let Err((code, message)) = require_admin_or_self(auth, &username) {
        res.status_code(code);
        res.render(json_error(code, message));
        return;
    }
    let token_id = body.token_id.trim();
    if token_id.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "token_id cannot be empty",
        ));
        return;
    }
    let scopes = match validate_scopes(&body.scopes) {
        Ok(scopes) => scopes,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, error));
            return;
        }
    };
    if scopes.iter().any(|scope| scope == SCOPE_ADMIN) && !auth.is_admin_caller() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "only admin/bootstrap callers may grant the admin scope",
        ));
        return;
    }
    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let user = match require_user_by_username(&db, &username) {
        Ok(user) => user,
        Err((code, message)) => {
            res.status_code(code);
            res.render(json_error(code, message));
            return;
        }
    };
    if user.is_disabled() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(StatusCode::FORBIDDEN, "user is disabled"));
        return;
    }
    let existing = match db.get_api_key_by_id(token_id) {
        Ok(Some(key)) if key.user_id == user.id => key,
        Ok(Some(_)) => {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(json_error(
                StatusCode::FORBIDDEN,
                "token does not belong to the specified user",
            ));
            return;
        }
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "token not found"));
            return;
        }
        Err(error) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
            return;
        }
    };
    if !existing.is_user_token() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "only user PAT scopes can be updated by this endpoint",
        ));
        return;
    }
    if existing.is_revoked() {
        res.status_code(StatusCode::CONFLICT);
        res.render(json_error(
            StatusCode::CONFLICT,
            "revoked token scopes cannot be updated",
        ));
        return;
    }
    if existing.is_expired(chrono::Utc::now().timestamp()) {
        res.status_code(StatusCode::CONFLICT);
        res.render(json_error(
            StatusCode::CONFLICT,
            "expired token scopes cannot be updated",
        ));
        return;
    }
    let changed = existing.scopes_vec() != scopes;
    if !changed {
        res.render(Json(json!({
            "success": true,
            "changed": false,
            "token": token_summary(&existing),
        })));
        return;
    }
    let updated = match db.update_api_key_scopes(token_id, &scopes_to_string(&scopes)) {
        Ok(Some(key)) => key,
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "token not found"));
            return;
        }
        Err(error) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
            return;
        }
    };
    res.render(Json(json!({
        "success": true,
        "changed": true,
        "token": token_summary(&updated),
    })));
}

/// `POST /api/tokens/revoke` — operationId `revokeApiToken`.
///
/// Bootstrap/admin may revoke anyone's token; a user may revoke only their own.
/// Idempotent: revoking an already-revoked token succeeds and returns the
/// metadata with the original `revoked_at`. Never returns the plaintext token.
#[handler]
pub(crate) async fn tokens_revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: RevokeApiTokenRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {}", e),
            ));
            return;
        }
    };
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }
    let username = match validate_username(&body.username) {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if let Err((code, msg)) = require_admin_or_self(auth, &username) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let token_id = body.token_id.trim().to_string();
    if token_id.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "token_id cannot be empty",
        ));
        return;
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let user = match require_user_by_username(&db, &username) {
        Ok(u) => u,
        Err((code, msg)) => {
            res.status_code(code);
            res.render(json_error(code, msg));
            return;
        }
    };

    // Verify the token actually belongs to the target user before revoking,
    // so a user cannot revoke another user's token by guessing an id.
    let existing = match db.get_api_key_by_id(&token_id) {
        Ok(o) => o,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    let existing = match existing {
        Some(k) if k.user_id == user.id => k,
        Some(_) => {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(json_error(
                StatusCode::FORBIDDEN,
                "token does not belong to the specified user",
            ));
            return;
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "token not found"));
            return;
        }
    };

    let now = chrono::Utc::now().timestamp();
    let revoked = match db.revoke_api_key(&token_id, now) {
        Ok(Some(k)) => k,
        Ok(None) => existing,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    res.render(Json(json!({
        "success": true,
        "token": token_summary(&revoked),
    })));
}
