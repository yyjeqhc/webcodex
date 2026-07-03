use crate::auth::{
    generate_agent_token, hash_token, is_agent_scope, scopes_to_string, token_prefix,
    validate_agent_scopes, validate_allowed_client_id, validate_username, AuthContext,
    AGENT_SCOPES,
};
use crate::json_error;
use crate::models::{ApiKeyRecord, TOKEN_KIND_AGENT};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;

use super::{
    require_admin_or_self, require_user_by_username,
    responses::{
        agent_token_summary, is_unique_constraint_error, normalize_token_hash,
        validate_agent_prefix,
    },
};

/// Maximum number of agent tokens returned by `listAgentTokens`.
const MAX_AGENT_TOKENS_LIST: usize = 200;
/// Maximum length of a token `name`.
const MAX_TOKEN_NAME_LEN: usize = 128;

#[derive(Debug, Deserialize)]
pub(crate) struct CreateAgentTokenRequest {
    pub username: String,
    pub client_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterAgentTokenHashRequest {
    pub username: String,
    pub name: Option<String>,
    pub client_id: String,
    pub token_hash: String,
    pub token_prefix: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListAgentTokensRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevokeAgentTokenRequest {
    pub username: String,
    pub token_id: String,
}

/// `POST /api/agent-tokens/create` — operationId `createAgentToken`.
///
/// Bootstrap/admin may create an agent token for any user; a normal user may
/// create one only for themselves. An agent token may **not** call this
/// endpoint (agent tokens cannot mint more tokens). `allowed_client_id` is
/// required and validated. Agent token scopes must be a subset of the agent
/// transport scopes; when omitted, all agent transport scopes are granted.
/// Returns the plaintext token **exactly once**.
#[handler]
pub(crate) async fn agent_tokens_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: CreateAgentTokenRequest = match req.parse_json().await {
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
    // Agent tokens may not mint tokens.
    if auth.is_agent_token() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
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

    let allowed_client_id = match validate_allowed_client_id(&body.client_id) {
        Ok(c) => c,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };

    let token_name = body
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    if token_name.chars().count() > MAX_TOKEN_NAME_LEN {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "token name is too long",
        ));
        return;
    }

    // Default to all agent transport scopes when omitted.
    let scopes = match body.scopes {
        Some(raw) => match validate_agent_scopes(&raw) {
            Ok(s) => s,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        },
        None => AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
    };
    // Defensive: ensure every scope is an agent scope (validate_agent_scopes
    // already enforces this, but double-check so a future refactor cannot
    // accidentally widen agent tokens).
    if scopes.iter().any(|s| !is_agent_scope(s)) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "agent tokens may only carry agent:* scopes",
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

    let plaintext = generate_agent_token();
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
        kind: TOKEN_KIND_AGENT.to_string(),
        allowed_client_id: Some(allowed_client_id.clone()),
    };
    if let Err(e) = db.insert_api_key(&record, &key_hash) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        return;
    }

    // Return the plaintext token exactly once. It is never stored and never
    // logged. The response also carries the safe metadata (prefix, scopes,
    // allowed_client_id).
    res.render(Json(json!({
        "success": true,
        "token": plaintext,
        "token_prefix": record.key_prefix,
        "token_id": record.id,
        "name": record.name,
        "kind": "agent",
        "username": user.username,
        "user_id": user.id,
        "allowed_client_id": allowed_client_id,
        "scopes": scopes,
        "created_at": record.created_at,
        "expires_at": record.expires_at,
    })));
}

/// `POST /api/agent-tokens/register_hash`
///
/// Registers an agent token hash generated locally by the CLI. The server
/// receives only hash/prefix/metadata, stores `kind='agent'` and binds the row
/// to `allowed_client_id = client_id`. It never accepts or returns the
/// plaintext `wc_agent_*` token.
#[handler]
pub(crate) async fn agent_tokens_register_hash(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let body: RegisterAgentTokenHashRequest = match req.parse_json().await {
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
    if auth.is_agent_token() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
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

    let allowed_client_id = match validate_allowed_client_id(&body.client_id) {
        Ok(c) => c,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let token_name = body
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| allowed_client_id.clone());
    if token_name.chars().count() > MAX_TOKEN_NAME_LEN {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "token name is too long",
        ));
        return;
    }
    let token_hash = match normalize_token_hash(&body.token_hash) {
        Ok(h) => h,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let token_prefix = match validate_agent_prefix(&body.token_prefix) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let raw_scopes = if body.scopes.is_empty() {
        AGENT_SCOPES.iter().map(|s| s.to_string()).collect()
    } else {
        body.scopes
    };
    let scopes = match validate_agent_scopes(&raw_scopes) {
        Ok(s) => s,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    if scopes.iter().any(|s| !is_agent_scope(s)) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "agent tokens may only carry agent:* scopes",
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
        kind: TOKEN_KIND_AGENT.to_string(),
        allowed_client_id: Some(allowed_client_id),
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
        "token": {
            "id": record.id,
            "name": record.name,
            "token_prefix": record.key_prefix,
            "allowed_client_id": record.allowed_client_id,
            "scopes": scopes,
            "created_at": record.created_at,
            "expires_at": record.expires_at,
        },
    })));
}

/// `POST /api/agent-tokens/list` — operationId `listAgentTokens`.
///
/// Bootstrap/admin may list anyone's agent tokens; a user may list only their
/// own. Returns metadata only — never `key_hash` or the plaintext token.
/// Only `kind='agent'` rows are returned.
#[handler]
pub(crate) async fn agent_tokens_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: ListAgentTokensRequest = match req.parse_json().await {
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
    if auth.is_agent_token() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
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
    let mut keys = match db.list_agent_api_keys_by_user(&user.id) {
        Ok(k) => k,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    keys.truncate(MAX_AGENT_TOKENS_LIST);
    let tokens: Vec<serde_json::Value> = keys.iter().map(agent_token_summary).collect();
    res.render(Json(json!({
        "success": true,
        "username": user.username,
        "user_id": user.id,
        "tokens": tokens,
        "count": tokens.len(),
    })));
}

/// `POST /api/agent-tokens/revoke` — operationId `revokeAgentToken`.
///
/// Bootstrap/admin may revoke anyone's agent token; a user may revoke only
/// their own. Verifies the token belongs to the user and `kind == "agent"`.
/// Idempotent. Never returns the plaintext token.
#[handler]
pub(crate) async fn agent_tokens_revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: RevokeAgentTokenRequest = match req.parse_json().await {
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
    if auth.is_agent_token() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
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

    // Verify the token belongs to the target user and is an agent token.
    let existing = match db.get_api_key_by_id(&token_id) {
        Ok(o) => o,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    let existing = match existing {
        Some(k) if k.user_id == user.id && k.is_agent_token() => k,
        Some(k) if k.user_id == user.id => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "token is not an agent token",
            ));
            return;
        }
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
        "token": agent_token_summary(&revoked),
    })));
}
