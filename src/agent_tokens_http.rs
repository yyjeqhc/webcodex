//! Phase 3 agent token management endpoints.
//!
//! These are REST-only admin/self-management surfaces for agent tokens —
//! tokens bound to an owner username and an `allowed_client_id`, usable only
//! on agent transport endpoints (`/api/shell/agent/*`, `/api/agents/ws`). They
//! are intentionally **not** exposed in `/openapi.json` (GPT Actions) because
//! token creation is sensitive and should be driven by an admin CLI/HTTP
//! client, not a GPT. The paths are listed in `LEGACY_FORBIDDEN_PATHS` so
//! tests catch accidental OpenAPI inclusion. All endpoints sit behind the
//! shared `AuthMiddleware` (Bearer auth) and resolve the caller's
//! [`AuthContext`] to enforce the admin/bootstrap-or-self boundary.
//!
//! Security properties:
//! - Agent token plaintext is returned **only once** at creation time.
//! - Only the SHA-256 hash (`key_hash`) is persisted.
//! - `key_hash` and plaintext tokens never appear in list/revoke responses.
//! - Agent tokens may only carry `agent:*` scopes.
//! - Agent tokens are rejected from these management endpoints (only bootstrap
//!   and user tokens may manage agent tokens), so a leaked agent token cannot
//!   mint more agent tokens.
//! - Agent tokens may not call the Phase 2 personal API token management
//!   endpoints either.

use crate::auth::{
    generate_agent_token, hash_token, is_agent_scope, scopes_to_string, token_prefix,
    validate_agent_scopes, validate_allowed_client_id, validate_username, AuthContext,
    AGENT_SCOPES, SCOPE_ADMIN,
};
use crate::json_error;
use crate::models::{ApiKeyRecord, TOKEN_KIND_AGENT};
use crate::Database;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

/// Maximum number of agent tokens returned by `listAgentTokens`.
const MAX_AGENT_TOKENS_LIST: usize = 200;
/// Maximum length of a token `name`.
const MAX_TOKEN_NAME_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

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
pub(crate) struct ListAgentTokensRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevokeAgentTokenRequest {
    pub username: String,
    pub token_id: String,
}

// ---------------------------------------------------------------------------
// Auth helpers (mirror users_http.rs)
// ---------------------------------------------------------------------------

/// True when the caller may manage any user (bootstrap token or `admin` role).
fn is_admin_caller(auth: &AuthContext) -> bool {
    auth.is_bootstrap
        || auth.role.as_deref() == Some("admin")
        || auth.scopes.iter().any(|s| s == SCOPE_ADMIN)
}

/// Resolve the authenticated caller's username, if any. Bootstrap callers do
/// not have a username.
fn caller_username(auth: &AuthContext) -> Option<&str> {
    if auth.is_bootstrap {
        None
    } else {
        auth.username.as_deref()
    }
}

/// Enforce that the caller may act on `target_username`:
/// - bootstrap/admin may act on anyone;
/// - a normal user may only act on themselves.
fn require_admin_or_self(
    auth: &AuthContext,
    target_username: &str,
) -> Result<(), (StatusCode, String)> {
    if is_admin_caller(auth) {
        return Ok(());
    }
    match caller_username(auth) {
        Some(caller) if caller == target_username => Ok(()),
        _ => Err((
            StatusCode::FORBIDDEN,
            "caller may only manage their own resources".to_string(),
        )),
    }
}

/// Load a user by username, returning a JSON 404-style error when missing.
fn require_user_by_username(
    db: &Database,
    username: &str,
) -> Result<crate::models::UserRecord, (StatusCode, String)> {
    db.get_user_by_username(username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "user not found".to_string()))
}

// ---------------------------------------------------------------------------
// Serialization (no secrets)
// ---------------------------------------------------------------------------

/// Agent token metadata returned by list/revoke. Never includes `key_hash` or
/// the plaintext token. Includes the Phase 3 `kind` and `allowed_client_id`.
fn agent_token_summary(key: &ApiKeyRecord) -> Value {
    json!({
        "id": key.id,
        "user_id": key.user_id,
        "name": key.name,
        "token_prefix": key.key_prefix,
        "kind": key.kind(),
        "allowed_client_id": key.allowed_client_id,
        "scopes": key.scopes_vec(),
        "created_at": key.created_at,
        "last_used_at": key.last_used_at,
        "expires_at": key.expires_at,
        "revoked_at": key.revoked_at,
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
    let tokens: Vec<Value> = keys.iter().map(agent_token_summary).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::hash_token;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Minimal `Config` for tests (token sets whether auth is enabled).
    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        })
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    /// Build a router mirroring the production agent-token management wiring.
    fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                    .push(Router::with_path("agent-tokens/list").post(agent_tokens_list))
                    .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
            )
    }

    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    /// Bootstrap helper: create a user directly via the DB so tests can mint
    /// tokens for them.
    fn seed_user(db: &crate::Database, username: &str, role: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: role.to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    /// Create a Phase 2 user token for `username` by calling the existing
    /// tokens/create endpoint via the bootstrap path, returning the plaintext.
    async fn mint_user_token(
        service: &salvo::Service,
        username: &str,
        scopes: Vec<String>,
    ) -> String {
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": username, "scopes": scopes}))
            .send(service)
            .await;
        resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // =========================================================================
    // createAgentToken
    // =========================================================================

    #[tokio::test]
    async fn http_agent_tokens_create_requires_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_agent_tokens_create_bootstrap_creates_for_anyone() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db.clone()));
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({
                "username": "alice",
                "client_id": "alice-laptop",
                "name": "alice laptop agent",
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let token = body["token"].as_str().unwrap().to_string();
        assert!(
            token.starts_with("wc_agent_"),
            "agent token must use wc_agent_ prefix"
        );
        assert!(token.len() > "wc_agent_".len() + 32);
        let prefix = body["token_prefix"].as_str().unwrap();
        assert!(prefix.starts_with("wc_agent_"));
        assert_ne!(prefix, token);
        assert_eq!(body["kind"], "agent");
        assert_eq!(body["username"], "alice");
        assert_eq!(body["allowed_client_id"], "alice-laptop");
        // Default scopes are all agent transport scopes.
        let scopes = body["scopes"].as_array().unwrap();
        let scope_strs: Vec<&str> = scopes.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(
            scope_strs,
            vec![
                "agent:register",
                "agent:poll",
                "agent:result",
                "agent:job_update"
            ]
        );

        // The DB must store the hash, not the plaintext token.
        let hash = hash_token(&token);
        let stored = db.get_api_key_by_hash(&hash).unwrap().unwrap();
        assert_eq!(stored.name, "alice laptop agent");
        assert!(stored.is_agent_token());
        assert_eq!(stored.allowed_client_id(), Some("alice-laptop"));
        {
            let conn = db.conn_for_tests();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM api_keys WHERE key_hash = ?1",
                    rusqlite::params![token],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "plaintext token must never be stored as key_hash");
        }
    }

    #[tokio::test]
    async fn http_agent_tokens_create_user_creates_for_self() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        // Need the tokens/create endpoint to mint a user token for alice.
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create)),
            );
        let service = Service::new(router);
        let alice_token =
            mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["kind"], "agent");
        assert_eq!(body["allowed_client_id"], "alice-laptop");
    }

    #[tokio::test]
    async fn http_agent_tokens_create_user_cannot_create_for_other() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        seed_user(&db, "bob", "user");
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create)),
            );
        let service = Service::new(router);
        let alice_token =
            mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "bob", "client_id": "bob-laptop"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_agent_tokens_create_requires_client_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "client_id": ""}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn http_agent_tokens_create_validates_client_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        for bad in ["bad/client", "bad client", "bad\x00client"] {
            let resp = TestClient::post("http://localhost/api/agent-tokens/create")
                .bearer_auth("secret")
                .json(&json!({"username": "alice", "client_id": bad}))
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST,
                "client_id {:?} should be rejected",
                bad
            );
        }
    }

    #[tokio::test]
    async fn http_agent_tokens_create_rejects_non_agent_scopes() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({
                "username": "alice",
                "client_id": "alice-laptop",
                "scopes": ["runtime:read", "agent:register"],
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn http_agent_tokens_create_rejects_admin_scope_on_agent_token() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({
                "username": "alice",
                "client_id": "alice-laptop",
                "scopes": ["admin"],
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // listAgentTokens
    // =========================================================================

    #[tokio::test]
    async fn http_agent_tokens_list_never_returns_hash_or_plaintext() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        // Create two agent tokens, capturing their plaintext values.
        let mut plaintext_tokens = Vec::new();
        for name in ["a", "b"] {
            let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
                .bearer_auth("secret")
                .json(&json!({
                    "username": "alice",
                    "client_id": "alice-laptop",
                    "name": name,
                }))
                .send(&service)
                .await;
            assert_eq!(effective_status(&resp), StatusCode::OK);
            let body: Value = resp.take_json().await.unwrap();
            plaintext_tokens.push(body["token"].as_str().unwrap().to_string());
        }
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/list")
            .bearer_auth("secret")
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let tokens = body["tokens"].as_array().unwrap();
        assert_eq!(tokens.len(), 2);
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(!serialized.contains("token_hash"));
        assert!(!serialized.contains("key_hash"));
        for plaintext in &plaintext_tokens {
            assert!(
                !serialized.contains(plaintext),
                "list response must never include the full plaintext token"
            );
        }
        for t in tokens {
            assert_eq!(t["kind"], "agent");
            assert!(t["allowed_client_id"].is_string());
            assert!(t.get("key_hash").is_none());
            assert!(t.get("token").is_none());
        }
    }

    #[tokio::test]
    async fn http_agent_tokens_list_does_not_include_user_tokens() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                    .push(Router::with_path("agent-tokens/list").post(agent_tokens_list)),
            );
        let service = Service::new(router);
        // Create a user token and an agent token for alice.
        let _user_token =
            mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
        let _ = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
            .send(&service)
            .await;
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/list")
            .bearer_auth("secret")
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let tokens = body["tokens"].as_array().unwrap();
        assert_eq!(tokens.len(), 1, "agent token list must exclude user tokens");
        assert_eq!(tokens[0]["kind"], "agent");
    }

    // =========================================================================
    // revokeAgentToken
    // =========================================================================

    #[tokio::test]
    async fn http_agent_tokens_revoke_works() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db.clone()));
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let token_id = body["token_id"].as_str().unwrap().to_string();
        // Revoke as bootstrap.
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "token_id": token_id}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["token"]["revoked_at"].is_number());
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(!serialized.contains(&token));
        // Revoked agent token no longer authenticates.
        let hash = hash_token(&token);
        assert!(db.get_api_key_by_hash(&hash).unwrap().is_none());
    }

    #[tokio::test]
    async fn http_agent_tokens_revoke_user_cannot_revoke_others() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        seed_user(&db, "bob", "user");
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                    .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
            );
        let service = Service::new(router);
        // Create an agent token for bob.
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "bob", "client_id": "bob-laptop"}))
            .send(&service)
            .await;
        let bob_token_id = resp.take_json::<Value>().await.unwrap()["token_id"]
            .as_str()
            .unwrap()
            .to_string();
        // alice mints a user token and tries to revoke bob's agent token.
        let alice_token =
            mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
        let resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "alice", "token_id": bob_token_id}))
            .send(&service)
            .await;
        // alice targets her own username but passes bob's token_id: the
        // ownership check must reject this.
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_agent_tokens_revoke_rejects_user_token_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                    .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
            );
        let service = Service::new(router);
        // Create a user token for alice, capture its id via tokens/list.
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let user_token_id = resp.take_json::<Value>().await.unwrap()["token_id"]
            .as_str()
            .unwrap()
            .to_string();
        // Attempt to revoke it via the agent-tokens endpoint: must be rejected
        // because it is not an agent token.
        let resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "token_id": user_token_id}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // Agent token cannot call management endpoints
    // =========================================================================

    /// Phase 3 no-secret guard: the agent_token_summary serialization must
    /// never include `key_hash` or a `token` field. This is a unit-level
    /// guard complementing the HTTP-level list/revoke tests.
    #[test]
    fn agent_token_summary_never_includes_hash_or_plaintext() {
        let key = ApiKeyRecord {
            id: "k-1".to_string(),
            user_id: "u-1".to_string(),
            name: "agent".to_string(),
            key_prefix: "wc_agent_pre".to_string(),
            created_at: 1,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register".to_string(),
            expires_at: None,
            kind: "agent".to_string(),
            allowed_client_id: Some("laptop".to_string()),
        };
        let summary = agent_token_summary(&key);
        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("key_hash"));
        assert!(!serialized.contains("token_hash"));
        assert!(
            summary.get("token").is_none(),
            "summary must not include a plaintext token field"
        );
        assert!(
            summary.get("key_hash").is_none(),
            "summary must not include key_hash"
        );
        assert_eq!(summary["kind"], "agent");
        assert_eq!(summary["allowed_client_id"], "laptop");
    }

    #[tokio::test]
    async fn http_agent_token_cannot_call_management_endpoints() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let router = Router::new()
            .hoop(affix_state::inject(config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                    .push(Router::with_path("agent-tokens/list").post(agent_tokens_list))
                    .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
            );
        let service = Service::new(router);
        // Bootstrap creates an agent token for alice.
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
            .send(&service)
            .await;
        let agent_token = resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        // The agent token must not be able to call create/list/revoke.
        for path in [
            "/api/agent-tokens/create",
            "/api/agent-tokens/list",
            "/api/agent-tokens/revoke",
        ] {
            let body = match path {
                "/api/agent-tokens/create" => json!({
                    "username": "alice",
                    "client_id": "alice-laptop",
                }),
                "/api/agent-tokens/list" => json!({"username": "alice"}),
                "/api/agent-tokens/revoke" => json!({
                    "username": "alice",
                    "token_id": "00000000-0000-0000-0000-000000000000",
                }),
                _ => unreachable!(),
            };
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .bearer_auth(&agent_token)
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::FORBIDDEN,
                "agent token must not be able to call {}",
                path
            );
        }
    }

    // =========================================================================
    // Unauthorized responses are JSON
    // =========================================================================

    #[tokio::test]
    async fn http_agent_tokens_unauthorized_responses_are_json() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        for path in [
            "/api/agent-tokens/create",
            "/api/agent-tokens/list",
            "/api/agent-tokens/revoke",
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&json!({}))
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{}",
                path
            );
            let ct = resp.headers().get("content-type").unwrap();
            assert!(
                ct.to_str().unwrap().contains("application/json"),
                "{}",
                path
            );
        }
    }
}
