use crate::auth::{
    clean_token_name, generate_agent_token, hash_token, is_agent_scope, is_unique_constraint_error,
    normalize_token_hash, scopes_to_string, token_prefix, validate_agent_scopes,
    validate_allowed_client_id, validate_token_prefix, validate_username, AGENT_SCOPES,
};
use crate::management_http::{
    auth_context, database, parse_json, require_admin_or_self, require_user_by_username,
    JsonApiError, JsonApiResult,
};
use crate::models::{ApiKeyRecord, TOKEN_KIND_AGENT};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;

use super::responses::agent_token_summary;

/// Maximum number of agent tokens returned by `listAgentTokens`.
const MAX_AGENT_TOKENS_LIST: usize = 200;

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
/// transport scopes; when omitted, all Runner transport scopes are granted.
/// Returns the plaintext token **exactly once**.
#[handler]
pub(crate) async fn runner_tokens_create(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: CreateAgentTokenRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    // Agent tokens may not mint tokens.
    if auth.is_agent_token() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
    }
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let allowed_client_id =
        validate_allowed_client_id(&body.client_id).map_err(JsonApiError::bad_request)?;

    let token_name = clean_token_name(body.name, "default").map_err(JsonApiError::bad_request)?;

    // Default to all Runner transport scopes when omitted.
    let scopes = match body.scopes {
        Some(raw) => validate_agent_scopes(&raw).map_err(JsonApiError::bad_request)?,
        None => AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
    };
    // Defensive: ensure every scope is an agent scope (validate_agent_scopes
    // already enforces this, but double-check so a future refactor cannot
    // accidentally widen agent tokens).
    if scopes.iter().any(|s| !is_agent_scope(s)) {
        return Err(JsonApiError::bad_request(
            "agent tokens may only carry agent:* scopes",
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
    db.insert_api_key(&record, &key_hash)
        .map_err(JsonApiError::internal)?;

    // Return the plaintext token exactly once. It is never stored and never
    // logged. The response also carries the safe metadata (prefix, scopes,
    // allowed_client_id).
    Ok(Json(json!({
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
    })))
}

/// `POST /api/agent-tokens/register_hash`
///
/// Registers an agent token hash generated locally by the CLI. The server
/// receives only hash/prefix/metadata, stores `kind='agent'` and binds the row
/// to `allowed_client_id = client_id`. It never accepts or returns the
/// plaintext `wc_agent_*` token.
#[handler]
pub(crate) async fn runner_tokens_register_hash(
    req: &mut Request,
    depot: &mut Depot,
) -> JsonApiResult {
    let body: RegisterAgentTokenHashRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    if auth.is_agent_token() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
    }

    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let allowed_client_id =
        validate_allowed_client_id(&body.client_id).map_err(JsonApiError::bad_request)?;
    let token_name =
        clean_token_name(body.name, &allowed_client_id).map_err(JsonApiError::bad_request)?;
    let token_hash = normalize_token_hash(&body.token_hash).map_err(JsonApiError::bad_request)?;
    let token_prefix = validate_token_prefix(&body.token_prefix, "wc_agent_")
        .map_err(JsonApiError::bad_request)?;
    let raw_scopes = if body.scopes.is_empty() {
        AGENT_SCOPES.iter().map(|s| s.to_string()).collect()
    } else {
        body.scopes
    };
    let scopes = validate_agent_scopes(&raw_scopes).map_err(JsonApiError::bad_request)?;
    if scopes.iter().any(|s| !is_agent_scope(s)) {
        return Err(JsonApiError::bad_request(
            "agent tokens may only carry agent:* scopes",
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
        kind: TOKEN_KIND_AGENT.to_string(),
        allowed_client_id: Some(allowed_client_id),
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
        "token": {
            "id": record.id,
            "name": record.name,
            "token_prefix": record.key_prefix,
            "allowed_client_id": record.allowed_client_id,
            "scopes": scopes,
            "created_at": record.created_at,
            "expires_at": record.expires_at,
        },
    })))
}

/// `POST /api/agent-tokens/list` — operationId `listAgentTokens`.
///
/// Bootstrap/admin may list anyone's agent tokens; a user may list only their
/// own. Returns metadata only — never `key_hash` or the plaintext token.
/// Only `kind='agent'` rows are returned.
#[handler]
pub(crate) async fn runner_tokens_list(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: ListAgentTokensRequest = parse_json(req).await?;
    let auth = auth_context(depot)?;
    if auth.is_agent_token() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
    }
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let db = database(depot)?;
    let user = require_user_by_username(&db, &username)?;
    let mut keys = db
        .list_agent_api_keys_by_user(&user.id)
        .map_err(JsonApiError::internal)?;
    keys.truncate(MAX_AGENT_TOKENS_LIST);
    let tokens: Vec<serde_json::Value> = keys.iter().map(agent_token_summary).collect();
    Ok(Json(json!({
        "success": true,
        "username": user.username,
        "user_id": user.id,
        "tokens": tokens,
        "count": tokens.len(),
    })))
}

/// `POST /api/agent-tokens/revoke` — operationId `revokeAgentToken`.
///
/// Bootstrap/admin may revoke anyone's agent token; a user may revoke only
/// their own. Verifies the token belongs to the user and `kind == "agent"`.
/// Idempotent. Never returns the plaintext token.
#[handler]
pub(crate) async fn runner_tokens_revoke(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: RevokeAgentTokenRequest = parse_json(req).await?;
    let auth = auth_context(depot)?;
    if auth.is_agent_token() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage agent tokens",
        ));
    }
    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    require_admin_or_self(auth, &username)?;

    let token_id = body.token_id.trim().to_string();
    if token_id.is_empty() {
        return Err(JsonApiError::bad_request("token_id cannot be empty"));
    }

    let db = database(depot)?;
    let user = require_user_by_username(&db, &username)?;

    // Verify the token belongs to the target user and is an agent token.
    let existing = db
        .get_api_key_by_id(&token_id)
        .map_err(JsonApiError::internal)?;
    let existing = match existing {
        Some(k) if k.user_id == user.id && k.is_agent_token() => k,
        Some(k) if k.user_id == user.id => {
            return Err(JsonApiError::bad_request("token is not an agent token"));
        }
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
        "token": agent_token_summary(&revoked),
    })))
}
