//! Phase 2 multi-user auth: user + personal API token management endpoints.
//!
//! These are REST-only admin/self-management surfaces. They are intentionally
//! **not** exposed in `/openapi.json` (GPT Actions) because token creation is
//! sensitive and should be driven by an admin CLI/HTTP client, not a GPT. The
//! paths are listed in `LEGACY_FORBIDDEN_PATHS` so tests catch accidental
//! OpenAPI inclusion. All endpoints sit behind the shared `AuthMiddleware`
//! (Bearer auth) and resolve the caller's [`AuthContext`] to enforce the
//! admin/bootstrap-or-self boundary.
//!
//! Security properties:
//! - Plaintext tokens are returned **only once** at creation time.
//! - Only the SHA-256 hash (`token_hash`) is persisted.
//! - `token_hash` and plaintext tokens never appear in list/status responses.
//! - `token_prefix` is returned for display so users can identify tokens.
//! - Unauthorized responses are JSON with a generic `error` message that does
//!   not leak whether a token prefix or username exists.

use crate::auth::{
    generate_api_token, hash_token, scopes_to_string, token_prefix, validate_role, validate_scopes,
    validate_username, AuthContext, SCOPE_ADMIN,
};
use crate::json_error;
use crate::models::{ApiKeyRecord, UserRecord};
use crate::Database;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

/// Maximum number of tokens returned by `listApiTokens`.
const MAX_TOKENS_LIST: usize = 200;
/// Maximum length of a token `name`.
const MAX_TOKEN_NAME_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct CreateUserRequest {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

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
pub(crate) struct ListApiTokensRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevokeApiTokenRequest {
    pub token_id: String,
    pub username: String,
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// True when the caller may manage any user (bootstrap token or `admin` role).
fn is_admin_caller(auth: &AuthContext) -> bool {
    auth.is_bootstrap
        || auth.role.as_deref() == Some("admin")
        || auth.scopes.iter().any(|s| s == SCOPE_ADMIN)
}

/// Phase 3: agent tokens must not be able to call user/token management
/// endpoints. Returns an error response tuple when the caller is an agent
/// token.
fn reject_agent_token(auth: &AuthContext) -> Result<(), (StatusCode, String)> {
    if auth.is_agent_token() {
        Err((
            StatusCode::FORBIDDEN,
            "agent tokens may not manage users or tokens".to_string(),
        ))
    } else {
        Ok(())
    }
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
) -> Result<UserRecord, (StatusCode, String)> {
    db.get_user_by_username(username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "user not found".to_string()))
}

// ---------------------------------------------------------------------------
// Serialization (no secrets)
// ---------------------------------------------------------------------------

fn user_summary(user: &UserRecord) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "display_name": user.display_name,
        "role": user.role,
        "disabled": user.is_disabled(),
        "disabled_at": user.disabled_at,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
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

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /api/users/create` — operationId `createUser`.
///
/// Requires bootstrap/admin auth. Creates a new user with a validated username
/// and role. Duplicate usernames are rejected. Returns a user summary (no
/// secrets).
#[handler]
pub(crate) async fn users_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: CreateUserRequest = match req.parse_json().await {
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
    if !is_admin_caller(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
        return;
    }
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
    let role = match body.role {
        Some(r) => match validate_role(&r) {
            Ok(r) => r,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        },
        None => "user".to_string(),
    };
    let display_name = body
        .display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(d) = display_name.as_ref() {
        if d.chars().count() > 128 {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "display_name is too long",
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

    if db
        .get_user_by_username(&username)
        .map(|o| o.is_some())
        .unwrap_or(false)
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(json_error(StatusCode::CONFLICT, "username already exists"));
        return;
    }

    let now = chrono::Utc::now().timestamp();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.clone(),
        created_at: now,
        disabled: 0,
        display_name,
        role,
        disabled_at: None,
        updated_at: Some(now),
    };
    if let Err(e) = db.create_user(&user) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        return;
    }

    res.render(Json(json!({
        "success": true,
        "user": user_summary(&user),
    })));
}

/// `POST /api/users/list` — operationId `listUsers`.
///
/// Bootstrap/admin only. Returns all user summaries.
#[handler]
pub(crate) async fn users_list(depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if !is_admin_caller(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
        return;
    }
    if let Err((code, msg)) = reject_agent_token(auth) {
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
    let users = match db.list_users() {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    let summaries: Vec<Value> = users.iter().map(user_summary).collect();
    res.render(Json(json!({
        "success": true,
        "users": summaries,
        "count": summaries.len(),
    })));
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
    if scopes.iter().any(|s| s == SCOPE_ADMIN) && !is_admin_caller(auth) {
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

    /// Build a router mirroring the production Phase 2 user/token management
    /// wiring: Config + Database injected, AuthMiddleware enforced.
    fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("users/create").post(users_create))
                    .push(Router::with_path("users/list").post(users_list))
                    .push(Router::with_path("tokens/create").post(tokens_create))
                    .push(Router::with_path("tokens/list").post(tokens_list))
                    .push(Router::with_path("tokens/revoke").post(tokens_revoke)),
            )
    }

    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    /// Bootstrap helper: create a user directly via the DB so tests can mint
    /// tokens for them.
    fn seed_user(db: &crate::Database, username: &str, role: &str) -> UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
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

    // =========================================================================
    // createUser
    // =========================================================================

    #[tokio::test]
    async fn http_users_create_requires_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));

        let resp = TestClient::post("http://localhost/api/users/create")
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
        // Unauthorized responses must be JSON, not HTML.
        assert!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json"),
            "unauthorized response must be JSON"
        );
    }

    #[tokio::test]
    async fn http_users_create_rejects_non_admin_token() {
        // A normal user's personal token must not be able to create users.
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        // Mint a token for alice via the bootstrap path.
        let service = Service::new(build_router(config.clone(), db.clone()));
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let alice_token = body["token"].as_str().unwrap().to_string();

        // alice (non-admin) tries to create a user -> forbidden.
        let resp = TestClient::post("http://localhost/api/users/create")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "bob"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_users_create_with_bootstrap_creates_user() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/users/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "display_name": "Alice", "role": "user"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["user"]["username"], "alice");
        assert_eq!(body["user"]["display_name"], "Alice");
        assert_eq!(body["user"]["role"], "user");
        assert_eq!(body["user"]["disabled"], false);
    }

    #[tokio::test]
    async fn http_users_create_rejects_duplicate_username() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let req = json!({"username": "alice"});
        let first = TestClient::post("http://localhost/api/users/create")
            .bearer_auth("secret")
            .json(&req)
            .send(&service)
            .await;
        assert_eq!(effective_status(&first), StatusCode::OK);
        let second = TestClient::post("http://localhost/api/users/create")
            .bearer_auth("secret")
            .json(&req)
            .send(&service)
            .await;
        assert_eq!(effective_status(&second), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn http_users_create_rejects_invalid_username() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        for bad in ["", "Alice", "ali/ce", "ali..ce", "ali ce", &"a".repeat(65)] {
            let resp = TestClient::post("http://localhost/api/users/create")
                .bearer_auth("secret")
                .json(&json!({"username": bad}))
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST,
                "username {:?} should be rejected",
                bad
            );
        }
    }

    #[tokio::test]
    async fn http_users_create_rejects_invalid_role() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/users/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "role": "superuser"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn http_users_list_requires_admin() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config.clone(), db.clone()));

        // Bootstrap can list.
        let mut resp = TestClient::post("http://localhost/api/users/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["users"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "alice"));

        // Normal user cannot list.
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        let resp = TestClient::post("http://localhost/api/users/list")
            .bearer_auth(&alice_token)
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    // =========================================================================
    // createApiToken
    // =========================================================================

    #[tokio::test]
    async fn http_tokens_create_returns_plaintext_once_and_authenticates() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({
                "username": "alice",
                "name": "laptop",
                "scopes": ["runtime:read", "project:read", "project:write", "job:run"],
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        // Token format check.
        assert!(
            token.starts_with("wc_pat_"),
            "token must use wc_pat_ prefix"
        );
        assert!(token.len() > "wc_pat_".len() + 32);
        // Prefix returned for display must not equal the full token.
        let prefix = body["token_prefix"].as_str().unwrap();
        assert!(prefix.starts_with("wc_pat_"));
        assert_ne!(prefix, token);
        assert_eq!(body["name"], "laptop");
        assert_eq!(body["username"], "alice");
        let scopes = body["scopes"].as_array().unwrap();
        assert_eq!(
            scopes
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["runtime:read", "project:read", "project:write", "job:run"]
        );

        // The DB must store the hash, not the plaintext token.
        let hash = hash_token(&token);
        let stored = db.get_api_key_by_hash(&hash).unwrap().unwrap();
        assert_eq!(stored.name, "laptop");
        // Confirm no row stores the plaintext token verbatim. Scope the
        // connection guard so the Mutex is released before the next HTTP
        // request (which acquires the same lock via AuthMiddleware).
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

        // The personal token authenticates and resolves to alice.
        let mut resp = TestClient::post("http://localhost/api/tokens/list")
            .bearer_auth(&token)
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::OK,
            "personal token must authenticate"
        );
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["username"], "alice");
        assert_eq!(body["count"], 1);
    }

    #[tokio::test]
    async fn http_tokens_create_rejects_wrong_token() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/tokens/list")
            .bearer_auth("wc_pat_deadbeef")
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_tokens_create_non_admin_cannot_target_other_user() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        seed_user(&db, "bob", "user");
        let service = Service::new(build_router(config.clone(), db.clone()));
        // Mint a token for alice.
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        // alice tries to create a token for bob -> forbidden.
        let resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "bob"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_tokens_create_non_admin_cannot_grant_admin_scope() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config.clone(), db));
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        let resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth(&alice_token)
            .json(&json!({"username": "alice", "scopes": ["admin"]}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_tokens_create_rejects_unknown_scope() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        let resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read", "bogus:scope"]}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // listApiTokens
    // =========================================================================

    #[tokio::test]
    async fn http_tokens_list_never_returns_hash_or_plaintext() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));
        // Create two tokens, capturing their plaintext values so we can prove
        // the list response never echoes them. The short `token_prefix`
        // (which legitimately starts with `wc_pat_`) is allowed to appear.
        let mut plaintext_tokens = Vec::new();
        for name in ["a", "b"] {
            let mut resp = TestClient::post("http://localhost/api/tokens/create")
                .bearer_auth("secret")
                .json(&json!({"username": "alice", "name": name, "scopes": ["runtime:read"]}))
                .send(&service)
                .await;
            assert_eq!(effective_status(&resp), StatusCode::OK);
            let body: Value = resp.take_json().await.unwrap();
            plaintext_tokens.push(body["token"].as_str().unwrap().to_string());
        }
        let mut resp = TestClient::post("http://localhost/api/tokens/list")
            .bearer_auth("secret")
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let tokens = body["tokens"].as_array().unwrap();
        assert_eq!(tokens.len(), 2);
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(
            !serialized.contains("token_hash"),
            "list response must never include token_hash"
        );
        assert!(
            !serialized.contains("key_hash"),
            "list response must never include key_hash"
        );
        // No full plaintext token may appear in the response. The
        // `token_prefix` (first 16 chars) is allowed.
        for plaintext in &plaintext_tokens {
            assert!(
                !serialized.contains(plaintext),
                "list response must never include the full plaintext token"
            );
        }
        // Each token entry exposes metadata only.
        for t in tokens {
            assert!(t["token_prefix"].is_string());
            assert!(t["scopes"].is_array());
            assert!(t["created_at"].is_number());
            assert!(t.get("key_hash").is_none());
            assert!(t.get("token").is_none());
        }
    }

    // =========================================================================
    // revokeApiToken
    // =========================================================================

    #[tokio::test]
    async fn http_tokens_revoke_works_and_token_no_longer_authenticates() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        let service = Service::new(build_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let token_id = body["token_id"].as_str().unwrap().to_string();

        // Token works before revoke.
        let resp = TestClient::post("http://localhost/api/tokens/list")
            .bearer_auth(&token)
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);

        // Revoke as bootstrap.
        let mut resp = TestClient::post("http://localhost/api/tokens/revoke")
            .bearer_auth("secret")
            .json(&json!({"token_id": token_id, "username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["token"]["revoked_at"].is_number());
        // Revoke response must not leak the plaintext token. The
        // `token_prefix` (first 16 chars, starts with `wc_pat_`) is allowed;
        // only the full secret must never appear.
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(!serialized.contains(&token));

        // Idempotent revoke.
        let resp = TestClient::post("http://localhost/api/tokens/revoke")
            .bearer_auth("secret")
            .json(&json!({"token_id": token_id, "username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);

        // Revoked token no longer authenticates.
        let resp = TestClient::post("http://localhost/api/tokens/list")
            .bearer_auth(&token)
            .json(&json!({"username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_tokens_revoke_user_cannot_revoke_others_token() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_user(&db, "alice", "user");
        seed_user(&db, "bob", "user");
        let service = Service::new(build_router(config.clone(), db));
        // Create a token for bob.
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "bob", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let bob_body: Value = resp.take_json().await.unwrap();
        let bob_token_id = bob_body["token_id"].as_str().unwrap().to_string();
        // Mint a token for alice.
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        // alice targets her own username but passes bob's token_id. The
        // ownership check must reject this.
        let resp = TestClient::post("http://localhost/api/tokens/revoke")
            .bearer_auth(&alice_token)
            .json(&json!({"token_id": bob_token_id, "username": "alice"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
    }

    // =========================================================================
    // Unauthorized responses are JSON
    // =========================================================================

    #[tokio::test]
    async fn http_unauthorized_responses_are_json() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        for path in [
            "/api/users/create",
            "/api/users/list",
            "/api/tokens/create",
            "/api/tokens/list",
            "/api/tokens/revoke",
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
                "{} unauthorized response must be JSON, got {:?}",
                path,
                ct
            );
        }
    }
}
