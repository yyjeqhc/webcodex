//! WebCodex authentication and authorization.
//!
//! This module implements the bearer-token authentication pipeline used by all
//! protected API endpoints. It supports four credential types today (bootstrap,
//! personal API token, agent token, account credential) and reserves extension
//! points for OAuth2 in a future phase.
//!
//! ## Submodules
//!
//! - [`principal`] — the [`Principal`] identity abstraction and [`AuthMethod`]
//!   / [`AuthError`] types.
//! - [`scopes`] — scope constants, validation, and authorization helpers.
//! - [`pat`] — PAT / agent token / account credential generation, hashing, and
//!   validation utilities.
//!
//! ## Architecture
//!
//! The [`AuthMiddleware`] Salvo handler is the single entry point for HTTP
//! authentication. It extracts a bearer token, validates it, and injects an
//! [`AuthContext`] into the depot. Handlers extract `AuthContext` and pass it
//! to the tool runtime for scope-based authorization.
//!
//! [`Principal`] is a higher-level abstraction derived from `AuthContext` that
//! unifies the identity representation regardless of auth method. During this
//! first refactoring phase both types coexist — `AuthContext` remains the
//! depot-injected type so existing handlers are unaffected. See
//! [`principal::Principal::from_auth_context`].
//!
//! ## Future: OAuth2
//!
//! The [`TokenVerifier`] trait is the extension point for future OAuth2 bearer
//! token verification. Its only implementation today is [`PatVerifier`] which
//! delegates to the existing PAT / bootstrap validation logic. An
//! `OAuth2Verifier` will be added in a subsequent phase.

use crate::{Config, Database};
use salvo::prelude::*;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub mod principal;
pub mod scopes;

// `pat` is `pub(crate)` — its functions are internal utilities.
pub(crate) mod pat;

// ---------------------------------------------------------------------------
// Re-exports — backward compatibility
// ---------------------------------------------------------------------------
// All items that were previously exported from `auth.rs` are re-exported here
// so that existing `use crate::auth::*` imports continue to work.

pub use principal::{AuthError, AuthMethod, Principal};

pub use scopes::{
    AGENT_SCOPES, KNOWN_SCOPES, SCOPE_ACCOUNT_MANAGE, SCOPE_ADMIN, SCOPE_AGENT_JOB_UPDATE,
    SCOPE_AGENT_POLL, SCOPE_AGENT_REGISTER, SCOPE_AGENT_RESULT, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};

pub(crate) use scopes::{
    is_agent_scope, require_scope, scopes_include, scopes_to_string, validate_agent_scopes,
    validate_scopes,
};

pub(crate) use pat::{
    generate_account_credential, generate_agent_token, generate_api_token, hash_token,
    token_prefix, validate_allowed_client_id, validate_role, validate_username,
};

// ---------------------------------------------------------------------------
// AuthKind — the low-level credential kind (preserved from Phase 2/3)
// ---------------------------------------------------------------------------

/// The kind of credential that produced an [`AuthContext`].
///
/// - `Bootstrap`: the server-wide `WEBCODEX_TOKEN` (admin/bootstrap auth).
/// - `ApiToken`: a Phase 2 personal API token (kind=`user`) backed by the
///   `api_keys` table.
/// - `AgentToken`: a Phase 3 agent token (kind=`agent`) bound to an owner
///   username and an `allowed_client_id`, usable only on agent transport
///   endpoints.
/// - `AccountCredential`: a high-entropy account credential backed by the
///   `account_credentials` table, usable only on account control endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthKind {
    Bootstrap,
    ApiToken,
    AgentToken,
    AccountCredential,
}

// ---------------------------------------------------------------------------
// AuthContext — the depot-injected auth state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthContext {
    pub kind: AuthKind,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub role: Option<String>,
    pub scopes: Vec<String>,
    pub is_bootstrap: bool,
    /// Phase 3: the token kind string (`"user"` or `"agent"`) from the
    /// underlying `api_keys` row. `None` for bootstrap auth.
    pub token_kind: Option<String>,
    /// Phase 3: the `allowed_client_id` bound to an agent token. `None` for
    /// bootstrap auth and user tokens.
    pub allowed_client_id: Option<String>,
}

impl AuthContext {
    /// True when the caller holds the `admin` scope or authenticated as the
    /// bootstrap token. Bootstrap is always treated as admin.
    #[allow(dead_code)]
    pub fn is_admin(&self) -> bool {
        self.is_bootstrap || self.scopes.iter().any(|s| s == SCOPE_ADMIN)
    }

    /// True when the caller holds the given scope (or is bootstrap/admin).
    #[allow(dead_code)]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.is_bootstrap || self.scopes.iter().any(|s| s == scope || s == SCOPE_ADMIN)
    }

    /// True when the caller authenticated as the bootstrap token (or auth is
    /// disabled in development).
    #[allow(dead_code)]
    pub fn is_bootstrap(&self) -> bool {
        self.is_bootstrap
    }

    /// True when the caller authenticated with a Phase 2 personal API token
    /// (kind=`user`).
    #[allow(dead_code)]
    pub fn is_user_token(&self) -> bool {
        matches!(self.kind, AuthKind::ApiToken)
    }

    /// True when the caller authenticated with a Phase 3 agent token
    /// (kind=`agent`).
    #[allow(dead_code)]
    pub fn is_agent_token(&self) -> bool {
        matches!(self.kind, AuthKind::AgentToken)
    }

    #[allow(dead_code)]
    pub fn is_account_credential(&self) -> bool {
        matches!(self.kind, AuthKind::AccountCredential)
    }

    /// True when the caller may use an agent transport endpoint for the given
    /// `client_id`. Bootstrap may use any client_id. Agent tokens may only use
    /// the `allowed_client_id` they are bound to. User tokens may not use
    /// agent transport endpoints.
    #[allow(dead_code)]
    pub fn can_use_agent_endpoint(&self, client_id: &str) -> bool {
        if self.is_bootstrap {
            return true;
        }
        if matches!(self.kind, AuthKind::AgentToken) {
            return self
                .allowed_client_id
                .as_deref()
                .map(|allowed| allowed == client_id)
                .unwrap_or(false);
        }
        false
    }

    /// Require the caller to hold `scope`. Returns `Err(message)` when the
    /// scope is missing. Bootstrap is always treated as holding every scope.
    #[allow(dead_code)]
    pub fn require_scope(&self, scope: &str) -> Result<(), String> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(format!("missing required scope: {}", scope))
        }
    }

    /// Derive a [`Principal`] from this `AuthContext`.
    ///
    /// This is the bridge between the low-level depot-injected type and the
    /// higher-level identity abstraction. Handlers that want to use
    /// `Principal`-based authorization can call this once.
    #[allow(dead_code)]
    pub fn to_principal(&self) -> Principal {
        Principal::from_auth_context(self)
    }
}

// ---------------------------------------------------------------------------
// Token extraction helpers
// ---------------------------------------------------------------------------

pub(crate) fn get_config(depot: &Depot) -> Option<Arc<Config>> {
    depot.obtain::<Arc<Config>>().ok().cloned()
}

pub(crate) fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

pub(crate) fn bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
}

pub(crate) fn allow_query_token_for_path(path: &str) -> bool {
    path == "/api/agents/ws"
}

pub(crate) fn bearer_or_allowed_query_token(req: &Request) -> Option<String> {
    bearer_token(req).or_else(|| {
        if allow_query_token_for_path(req.uri().path()) {
            req.query::<String>("token")
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// TokenVerifier — the trait for bearer token verification
// ---------------------------------------------------------------------------

/// A `TokenVerifier` validates a bearer token and returns an [`AuthContext`] on
/// success.
///
/// This trait is the extension point for plugging in alternative verification
/// strategies (e.g. OAuth2 JWT validation). The current implementation is
/// [`PatVerifier`], which mirrors the existing bootstrap + database lookup
/// logic.
///
/// ## Design notes
///
/// The trait is `Send + Sync` so it can be stored in shared state (e.g. behind
/// an `Arc`). Implementations receive `&Config` and `Option<&Database>` so
/// they can perform the full validation chain (bootstrap token check, database
/// lookup, etc.) without owning those resources.
///
/// ## Future: OAuth2
///
/// A future `OAuth2Verifier` will implement this trait by:
/// 1. Decoding the JWT bearer token
/// 2. Validating the signature against the OIDC provider's JWKS
/// 3. Extracting claims (sub, scope, exp, etc.)
/// 4. Mapping claims to an `AuthContext` with `AuthKind::ApiToken` or a new
///    kind
///
/// The verifier will be composed with `PatVerifier` in a chain: try PAT first,
/// fall back to OAuth2.
#[async_trait]
pub(crate) trait TokenVerifier: Send + Sync {
    /// Attempt to verify the given bearer token.
    ///
    /// Returns `Ok(Some(AuthContext))` on success, `Ok(None)` when this
    /// verifier does not recognize the token format (allowing a chained
    /// verifier to try), and `Err` for hard failures.
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String>;
}

/// PAT / bootstrap verifier — the existing validation logic wrapped in the
/// [`TokenVerifier`] trait.
///
/// This verifier handles:
/// 1. Auth-disabled mode (returns bootstrap)
/// 2. Bootstrap token match
/// 3. Database lookup by SHA-256 hash for API keys and account credentials
pub(crate) struct PatVerifier;

#[async_trait]
impl TokenVerifier for PatVerifier {
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String> {
        // Auth disabled in development -> bootstrap (full access), identical to
        // AuthMiddleware's `!config.is_auth_enabled()` branch.
        if !config.is_auth_enabled() {
            return Ok(Some(bootstrap_context()));
        }

        // Bootstrap token check (constant-time comparison).
        if config.validate_token(token) {
            return Ok(Some(bootstrap_context()));
        }

        // Database lookup.
        let Some(db) = db else {
            return Ok(None);
        };
        let token_hash = hash_token(token);

        // API key lookup (personal API tokens and agent tokens).
        if let Ok(Some(api_key)) = db.get_api_key_by_hash(&token_hash) {
            let user = db
                .get_user_by_id(&api_key.user_id)
                .ok()
                .flatten()
                .ok_or_else(|| "user not found".to_string())?;

            if user.is_disabled() {
                return Err("user is disabled".to_string());
            }

            let now = chrono::Utc::now().timestamp();
            if api_key.is_expired(now) {
                return Err("token expired".to_string());
            }

            if let Err(e) = db.update_api_key_last_used(&api_key.id, now) {
                tracing::warn!("failed to update api key last_used_at: {}", e);
            }

            let auth_kind = if api_key.is_agent_token() {
                AuthKind::AgentToken
            } else {
                AuthKind::ApiToken
            };

            return Ok(Some(AuthContext {
                kind: auth_kind,
                user_id: Some(user.id.clone()),
                username: Some(user.username.clone()),
                api_key_id: Some(api_key.id.clone()),
                api_key_name: Some(api_key.name.clone()),
                role: Some(user.role.clone()),
                scopes: api_key.scopes_vec(),
                is_bootstrap: false,
                token_kind: Some(api_key.kind().to_string()),
                allowed_client_id: api_key.allowed_client_id.clone(),
            }));
        }

        // Account credential lookup.
        if let Ok(Some(account_credential)) = db.get_account_credential_by_hash(&token_hash) {
            let user = db
                .get_user_by_id(&account_credential.user_id)
                .ok()
                .flatten()
                .ok_or_else(|| "user not found".to_string())?;

            if user.is_disabled() {
                return Err("user is disabled".to_string());
            }

            let now = chrono::Utc::now().timestamp();
            if let Err(e) = db.update_account_credential_last_used(&account_credential.id, now) {
                tracing::warn!("failed to update account credential last_used_at: {}", e);
            }

            return Ok(Some(AuthContext {
                kind: AuthKind::AccountCredential,
                user_id: Some(user.id.clone()),
                username: Some(user.username.clone()),
                api_key_id: None,
                api_key_name: None,
                role: Some(user.role.clone()),
                scopes: vec![SCOPE_ACCOUNT_MANAGE.to_string()],
                is_bootstrap: false,
                token_kind: Some("account".to_string()),
                allowed_client_id: None,
            }));
        }

        // Token not recognized by any verifier.
        Ok(None)
    }
}

/// OAuth2 bearer token verifier — **stub / placeholder** for future use.
///
/// This verifier will validate OAuth2 JWT tokens against an OIDC provider.
/// It is not wired up yet; the `verify` implementation always returns
/// `Ok(None)` (token not recognized), allowing `PatVerifier` to handle all
/// tokens in the current phase.
///
/// When implemented, this verifier will:
/// 1. Decode the JWT header to determine the signing algorithm
/// 2. Fetch the JWKS from the configured OIDC provider
/// 3. Validate the signature, expiry, audience, and issuer claims
/// 4. Map the `sub` and `scope` claims to an `AuthContext`
pub(crate) struct OAuth2Verifier;

#[async_trait]
impl TokenVerifier for OAuth2Verifier {
    async fn verify(
        &self,
        _config: &Config,
        _db: Option<&Arc<Database>>,
        _token: &str,
    ) -> Result<Option<AuthContext>, String> {
        // Stub: always returns "not recognized" so PatVerifier handles everything.
        // A real implementation will validate JWT, check JWKS, etc.
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Standalone authentication function (used by QUIC agent transport)
// ---------------------------------------------------------------------------

/// Authenticate a bearer token *outside* the HTTP request path, reusing the
/// exact same validation as [`AuthMiddleware`]. Used by the QUIC agent
/// transport, which has no HTTP middleware to inject an `AuthContext`.
///
/// Mirrors `AuthMiddleware::handle`:
/// - When auth is disabled (`!config.is_auth_enabled()`), returns a bootstrap
///   context (full access). This matches the HTTP behavior where unauthenticated
///   dev mode is treated as bootstrap.
/// - When auth is enabled, the bootstrap token is checked first
///   (`config.validate_token`); otherwise the token is hashed and looked up in
///   the `api_keys` table (personal API tokens and Phase 3 agent tokens).
///   Disabled users and expired tokens are rejected. Returns `None` for an
///   unknown/invalid token; the caller MUST treat `None` as "reject the
///   connection".
///
/// The `is_agent_transport_path` gate from `AuthMiddleware` does not apply
/// here: the QUIC listener is inherently an agent-only transport, so an agent
/// token reaching it is already on an allowed surface.
pub(crate) fn authenticate_bearer(
    config: &Config,
    db: Option<&Arc<Database>>,
    token: Option<&str>,
) -> Option<AuthContext> {
    // Auth disabled in development -> bootstrap (full access), identical to
    // AuthMiddleware's `!config.is_auth_enabled()` branch. This lets local
    // QUIC integration tests run without a configured token.
    if !config.is_auth_enabled() {
        return Some(bootstrap_context());
    }
    let token = token?;
    if config.validate_token(token) {
        return Some(bootstrap_context());
    }
    let db = db?;
    let key_hash = hash_token(token);
    let api_key = db.get_api_key_by_hash(&key_hash).ok()??;
    let user = db.get_user_by_id(&api_key.user_id).ok()??;
    if user.is_disabled() {
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    if api_key.is_expired(now) {
        return None;
    }
    if let Err(e) = db.update_api_key_last_used(&api_key.id, now) {
        tracing::warn!("failed to update api key last_used_at: {}", e);
    }
    let auth_kind = if api_key.is_agent_token() {
        AuthKind::AgentToken
    } else {
        AuthKind::ApiToken
    };
    Some(AuthContext {
        kind: auth_kind,
        user_id: Some(user.id.clone()),
        username: Some(user.username.clone()),
        api_key_id: Some(api_key.id.clone()),
        api_key_name: Some(api_key.name.clone()),
        role: Some(user.role.clone()),
        scopes: api_key.scopes_vec(),
        is_bootstrap: false,
        token_kind: Some(api_key.kind().to_string()),
        allowed_client_id: api_key.allowed_client_id.clone(),
    })
}

/// Build the bootstrap `AuthContext` used when auth is disabled or the
/// server-wide `WEBCODEX_TOKEN` is presented. Kept private to `auth`; the only
/// callers are `AuthMiddleware` (inline) and `authenticate_bearer`.
fn bootstrap_context() -> AuthContext {
    AuthContext {
        kind: AuthKind::Bootstrap,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("admin".to_string()),
        scopes: vec![SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        token_kind: None,
        allowed_client_id: None,
    }
}

// ---------------------------------------------------------------------------
// Path gating helpers
// ---------------------------------------------------------------------------

/// The exact set of authenticated paths an agent token (kind="agent") may use.
/// Any other authenticated path must reject agent tokens with a 403. This is
/// the central Phase 3 security gate enforced in [`AuthMiddleware`] before the
/// request reaches any handler, so per-handler owner-boundary checks cannot be
/// bypassed by a leaked agent token whose username matches an agent owner.
///
/// The paths are compared exactly (no prefix match) so a path like
/// `/api/agent-tokens/create` is correctly rejected for agent tokens even
/// though it starts with `/api/agent`.
pub(crate) const AGENT_TRANSPORT_PATHS: &[&str] = &[
    "/api/shell/agent/register",
    "/api/shell/agent/poll",
    "/api/shell/agent/result",
    "/api/shell/agent/job_update",
    "/api/agents/ws",
];

/// True when `path` is one of the exact agent transport endpoints an agent
/// token may call. Used by [`AuthMiddleware`] to gate agent tokens centrally.
pub(crate) fn is_agent_transport_path(path: &str) -> bool {
    AGENT_TRANSPORT_PATHS.contains(&path)
}

pub(crate) const ACCOUNT_CONTROL_PATHS: &[&str] = &[
    "/api/users/me",
    "/api/tokens/list",
    "/api/tokens/register_hash",
    "/api/tokens/revoke",
    "/api/agent-tokens/register_hash",
];

pub(crate) fn is_account_control_path(path: &str) -> bool {
    ACCOUNT_CONTROL_PATHS.contains(&path)
}

// ---------------------------------------------------------------------------
// AuthMiddleware — the Salvo handler
// ---------------------------------------------------------------------------

pub(crate) struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(config) = get_config(depot) else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "No config"})));
            ctrl.skip_rest();
            return;
        };

        let db = get_db(depot);
        let token = bearer_or_allowed_query_token(req);

        if !config.is_auth_enabled() {
            let ctx = AuthContext {
                kind: AuthKind::Bootstrap,
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                role: Some("admin".to_string()),
                scopes: vec![SCOPE_ADMIN.to_string()],
                is_bootstrap: true,
                token_kind: None,
                allowed_client_id: None,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(token) = token else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        if config.validate_token(&token) {
            let ctx = AuthContext {
                kind: AuthKind::Bootstrap,
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                role: Some("admin".to_string()),
                scopes: vec![SCOPE_ADMIN.to_string()],
                is_bootstrap: true,
                token_kind: None,
                allowed_client_id: None,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(db) = db else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "DB not available"})));
            ctrl.skip_rest();
            return;
        };

        let token_hash = hash_token(&token);

        // Lookup is centralized in the DB layer; the same "Unauthorized"
        // message is used whether the token prefix exists or not, to avoid
        // leaking which prefixes are present.
        if let Ok(Some(api_key)) = db.get_api_key_by_hash(&token_hash) {
            let Ok(Some(user)) = db.get_user_by_id(&api_key.user_id) else {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
                return;
            };

            if user.is_disabled() {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
                return;
            }

            let now = chrono::Utc::now().timestamp();
            if api_key.is_expired(now) {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
                return;
            }

            if let Err(e) = db.update_api_key_last_used(&api_key.id, now) {
                tracing::warn!("failed to update api key last_used_at: {}", e);
            }

            // Phase 3: distinguish agent tokens (kind="agent") from personal API
            // tokens (kind="user"). Agent tokens authenticate but carry a
            // different AuthKind so handlers can reject them from normal runtime
            // endpoints and accept them only on agent transport endpoints.
            let auth_kind = if api_key.is_agent_token() {
                AuthKind::AgentToken
            } else {
                AuthKind::ApiToken
            };

            let ctx = AuthContext {
                kind: auth_kind,
                user_id: Some(user.id.clone()),
                username: Some(user.username.clone()),
                api_key_id: Some(api_key.id.clone()),
                api_key_name: Some(api_key.name.clone()),
                role: Some(user.role.clone()),
                scopes: api_key.scopes_vec(),
                is_bootstrap: false,
                token_kind: Some(api_key.kind().to_string()),
                allowed_client_id: api_key.allowed_client_id.clone(),
            };

            if ctx.is_agent_token() {
                let path = req.uri().path();
                if !is_agent_transport_path(path) {
                    res.status_code(StatusCode::FORBIDDEN);
                    res.render(Json(serde_json::json!({
                        "error": "agent tokens are only allowed on agent transport endpoints",
                    })));
                    ctrl.skip_rest();
                    return;
                }
            }

            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Ok(Some(account_credential)) = db.get_account_credential_by_hash(&token_hash) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        let Ok(Some(user)) = db.get_user_by_id(&account_credential.user_id) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };
        if user.is_disabled() {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        }
        let now = chrono::Utc::now().timestamp();
        if let Err(e) = db.update_account_credential_last_used(&account_credential.id, now) {
            tracing::warn!("failed to update account credential last_used_at: {}", e);
        }
        let ctx = AuthContext {
            kind: AuthKind::AccountCredential,
            user_id: Some(user.id.clone()),
            username: Some(user.username.clone()),
            api_key_id: None,
            api_key_name: None,
            role: Some(user.role.clone()),
            scopes: vec![SCOPE_ACCOUNT_MANAGE.to_string()],
            is_bootstrap: false,
            token_kind: Some("account".to_string()),
            allowed_client_id: None,
        };
        if !is_account_control_path(req.uri().path()) {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(Json(serde_json::json!({
                "error": "account credentials may only access account control endpoints",
            })));
            ctrl.skip_rest();
            return;
        }

        depot.inject(ctx);
        ctrl.call_next(req, depot, res).await;
    }
}

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

pub(crate) fn json_error(status: StatusCode, msg: impl Into<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": status.as_u16(),
        "error": msg.into(),
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap_ctx() -> AuthContext {
        AuthContext {
            kind: AuthKind::Bootstrap,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: Some("admin".to_string()),
            scopes: vec![SCOPE_ADMIN.to_string()],
            is_bootstrap: true,
            token_kind: None,
            allowed_client_id: None,
        }
    }

    fn user_ctx(username: &str) -> AuthContext {
        AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-1".to_string()),
            api_key_name: Some("user key".to_string()),
            role: Some("user".to_string()),
            scopes: vec![SCOPE_RUNTIME_READ.to_string()],
            is_bootstrap: false,
            token_kind: Some("user".to_string()),
            allowed_client_id: None,
        }
    }

    fn agent_ctx(username: &str, client_id: &str, scopes: Vec<String>) -> AuthContext {
        AuthContext {
            kind: AuthKind::AgentToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-agent".to_string()),
            api_key_name: Some("agent key".to_string()),
            role: Some("user".to_string()),
            scopes,
            is_bootstrap: false,
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(client_id.to_string()),
        }
    }

    #[test]
    fn agent_token_auth_context_identifies_as_agent_token() {
        let ctx = agent_ctx(
            "alice",
            "alice-laptop",
            vec![
                SCOPE_AGENT_REGISTER.to_string(),
                SCOPE_AGENT_POLL.to_string(),
            ],
        );
        assert!(ctx.is_agent_token());
        assert!(!ctx.is_user_token());
        assert!(!ctx.is_bootstrap());
        assert_eq!(ctx.token_kind.as_deref(), Some("agent"));
        assert_eq!(ctx.allowed_client_id.as_deref(), Some("alice-laptop"));
        assert_eq!(ctx.username.as_deref(), Some("alice"));
    }

    #[test]
    fn user_token_auth_context_does_not_get_agent_kind() {
        let ctx = user_ctx("alice");
        assert!(ctx.is_user_token());
        assert!(!ctx.is_agent_token());
        assert_eq!(ctx.token_kind.as_deref(), Some("user"));
        assert!(ctx.allowed_client_id.is_none());
    }

    #[test]
    fn bootstrap_can_use_any_agent_endpoint() {
        let ctx = bootstrap_ctx();
        assert!(ctx.can_use_agent_endpoint("any-client"));
    }

    #[test]
    fn agent_token_can_use_matching_client_id_only() {
        let ctx = agent_ctx(
            "alice",
            "alice-laptop",
            vec![SCOPE_AGENT_REGISTER.to_string()],
        );
        assert!(ctx.can_use_agent_endpoint("alice-laptop"));
        assert!(!ctx.can_use_agent_endpoint("other-laptop"));
    }

    #[test]
    fn user_token_cannot_use_agent_endpoint() {
        let ctx = user_ctx("alice");
        assert!(!ctx.can_use_agent_endpoint("alice-laptop"));
    }

    #[test]
    fn require_scope_works_for_agent_tokens() {
        let ctx = agent_ctx("alice", "alice-laptop", vec![SCOPE_AGENT_POLL.to_string()]);
        assert!(ctx.require_scope(SCOPE_AGENT_POLL).is_ok());
        assert!(ctx.require_scope(SCOPE_AGENT_REGISTER).is_err());
    }

    #[test]
    fn bootstrap_require_scope_always_ok() {
        let ctx = bootstrap_ctx();
        assert!(ctx.require_scope(SCOPE_AGENT_REGISTER).is_ok());
        assert!(ctx.require_scope(SCOPE_RUNTIME_READ).is_ok());
    }

    #[test]
    fn is_agent_transport_path_allows_only_the_five_exact_paths() {
        // The five agent transport endpoints an agent token may call.
        assert!(is_agent_transport_path("/api/shell/agent/register"));
        assert!(is_agent_transport_path("/api/shell/agent/poll"));
        assert!(is_agent_transport_path("/api/shell/agent/result"));
        assert!(is_agent_transport_path("/api/shell/agent/job_update"));
        assert!(is_agent_transport_path("/api/agents/ws"));

        // Everything else is rejected — including paths that look similar.
        assert!(!is_agent_transport_path("/api/agent-tokens/create"));
        assert!(!is_agent_transport_path("/api/agent-tokens/register_hash"));
        assert!(!is_agent_transport_path("/api/agent-tokens/list"));
        assert!(!is_agent_transport_path("/api/agent-tokens/revoke"));
        assert!(!is_agent_transport_path("/api/pairing/create"));
        assert!(!is_agent_transport_path("/api/pairing/enroll"));
        assert!(!is_agent_transport_path("/api/runtime/status"));
        assert!(!is_agent_transport_path("/api/tools/list"));
        assert!(!is_agent_transport_path("/api/tools/call"));
        assert!(!is_agent_transport_path("/api/projects/list"));
        assert!(!is_agent_transport_path("/api/jobs/list"));
        assert!(!is_agent_transport_path("/mcp"));
        assert!(!is_agent_transport_path("/api/audit/sessions"));
        assert!(!is_agent_transport_path("/api/users/list"));
        assert!(!is_agent_transport_path("/api/tokens/list"));
        // Prefix-only matches must not pass (exact match required).
        assert!(!is_agent_transport_path("/api/shell/agent/register/extra"));
        assert!(!is_agent_transport_path("/api/agents/ws/extra"));
        assert!(!is_agent_transport_path(""));
    }

    #[test]
    fn query_token_is_allowed_only_for_agent_websocket_path() {
        assert!(allow_query_token_for_path("/api/agents/ws"));
        assert!(!allow_query_token_for_path("/api/runtime/status"));
        assert!(!allow_query_token_for_path("/api/shell/agent/register"));
        assert!(!allow_query_token_for_path("/api/agents/ws/extra"));
    }

    // -----------------------------------------------------------------------
    // HTTP-level central gate tests
    // -----------------------------------------------------------------------
    //
    // These build a minimal router that mounts a generic echo handler behind
    // the shared AuthMiddleware on a representative set of paths. The handler
    // is intentionally trivial — the point is to verify the Phase 3 central
    // gate in AuthMiddleware rejects agent tokens before any handler runs, and
    // that bootstrap/user tokens still reach the handler.

    use salvo::prelude::{affix_state, Json};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Router;
    use salvo::Service;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn gate_test_config(token: Option<&str>) -> Arc<crate::Config> {
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

    fn gate_test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("gate.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn gate_seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
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

    /// Create an agent token for `username` bound to `client_id` via the DB
    /// layer directly, returning the plaintext token. Used by the gate tests
    /// so we exercise the real AuthMiddleware path.
    fn gate_mint_agent_token(
        db: &crate::Database,
        user: &crate::models::UserRecord,
        client_id: &str,
    ) -> String {
        let plaintext = generate_agent_token();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "agent".to_string(),
            key_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register agent:poll agent:result agent:job_update".to_string(),
            expires_at: None,
            kind: crate::models::TOKEN_KIND_AGENT.to_string(),
            allowed_client_id: Some(client_id.to_string()),
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    /// Create a user token for `username` via the DB layer directly.
    fn gate_mint_user_token(db: &crate::Database, user: &crate::models::UserRecord) -> String {
        let plaintext = generate_api_token();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "user".to_string(),
            key_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:read project:write job:run".to_string(),
            expires_at: None,
            kind: crate::models::TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    fn gate_mint_account_credential(
        db: &crate::Database,
        user: &crate::models::UserRecord,
    ) -> String {
        let plaintext = generate_account_credential();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::AccountCredentialRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            credential_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        db.insert_account_credential(&record, &hash).unwrap();
        plaintext
    }

    /// Trivial handler that returns 200 OK JSON. Reaching it proves the central
    /// gate did not reject the request.
    #[salvo::handler]
    async fn echo_ok(
        _req: &mut salvo::Request,
        _depot: &mut salvo::Depot,
        res: &mut salvo::Response,
    ) {
        res.render(Json(serde_json::json!({"ok": true})));
    }

    /// Build a router mirroring the production path shapes the central gate
    /// must protect, each backed by the same trivial echo handler.
    fn gate_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            // /mcp at the root (not under /api).
            .push(
                Router::with_path("mcp")
                    .hoop(AuthMiddleware)
                    .get(echo_ok)
                    .post(echo_ok),
            )
            .push(
                Router::with_path("api")
                    .hoop(AuthMiddleware)
                    .push(Router::with_path("runtime/status").post(echo_ok))
                    .push(Router::with_path("tools/list").post(echo_ok))
                    .push(Router::with_path("tools/call").post(echo_ok))
                    .push(Router::with_path("projects/list").post(echo_ok))
                    .push(Router::with_path("jobs/list").post(echo_ok))
                    .push(Router::with_path("audit/sessions").post(echo_ok))
                    .push(Router::with_path("users/me").post(echo_ok))
                    .push(Router::with_path("users/list").post(echo_ok))
                    .push(Router::with_path("tokens/list").post(echo_ok))
                    .push(Router::with_path("tokens/register_hash").post(echo_ok))
                    .push(Router::with_path("tokens/revoke").post(echo_ok))
                    .push(Router::with_path("agent-tokens/register_hash").post(echo_ok))
                    .push(Router::with_path("agent-tokens/list").post(echo_ok))
                    .push(Router::with_path("shell/agent/register").post(echo_ok))
                    .push(Router::with_path("shell/agent/poll").post(echo_ok))
                    .push(Router::with_path("shell/agent/result").post(echo_ok))
                    .push(Router::with_path("shell/agent/job_update").post(echo_ok))
                    .push(Router::with_path("agents/ws").get(echo_ok)),
            )
    }

    fn gate_status(resp: &salvo::Response) -> salvo::http::StatusCode {
        resp.status_code.unwrap_or(salvo::http::StatusCode::OK)
    }

    async fn gate_send(
        service: &salvo::Service,
        path: &str,
        auth: Option<&str>,
    ) -> (salvo::http::StatusCode, serde_json::Value) {
        let mut req = TestClient::post(&format!("http://localhost{}", path));
        if path == "/api/agents/ws" || path.starts_with("/api/agents/ws?") {
            // The WS endpoint is GET-mounted in this test router.
            req = TestClient::get(&format!("http://localhost{}", path));
        }
        if let Some(token) = auth {
            req = req.bearer_auth(token);
        }
        let mut resp = req.send(service).await;
        let status = gate_status(&resp);
        let body = resp
            .take_json::<serde_json::Value>()
            .await
            .ok()
            .unwrap_or(serde_json::json!({"_raw": "<no json body>"}));
        (status, body)
    }

    #[tokio::test]
    async fn gate_agent_token_can_call_agent_transport_register() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        // /api/shell/agent/register is an allowed transport path.
        let (status, body) =
            gate_send(&service, "/api/shell/agent/register", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::OK, "body: {:?}", body);
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_runtime_status() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/runtime/status", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or("")
                .contains("agent tokens are only allowed"),
            "body: {:?}",
            body
        );
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_tools_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/tools/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_projects_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/projects/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_mcp() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/mcp", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_agent_tokens_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) =
            gate_send(&service, "/api/agent-tokens/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_tokens_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/tokens/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_user_token_can_call_normal_apis() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let user_token = gate_mint_user_token(&db, &user);
        let service = Service::new(gate_router(config, db));
        // User tokens must still reach normal runtime/project APIs.
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
        ] {
            let (status, body) = gate_send(&service, path, Some(&user_token)).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::OK,
                "{} body: {:?}",
                path,
                body
            );
        }
        // And must NOT reach agent transport endpoints (enforced per-handler in
        // Phase 3, but here the central gate lets them through; the per-handler
        // agent transport check rejects them). For this gate test we only
        // assert the central gate does not block user tokens on normal APIs.
    }

    #[test]
    fn account_control_path_allowlist_is_exact() {
        assert!(is_account_control_path("/api/users/me"));
        assert!(is_account_control_path("/api/tokens/list"));
        assert!(is_account_control_path("/api/tokens/register_hash"));
        assert!(is_account_control_path("/api/tokens/revoke"));
        assert!(is_account_control_path("/api/agent-tokens/register_hash"));
        assert!(!is_account_control_path("/api/runtime/status"));
        assert!(!is_account_control_path("/api/projects/list"));
        assert!(!is_account_control_path("/api/tools/list"));
        assert!(!is_account_control_path("/mcp"));
        assert!(!is_account_control_path("/api/users/me/extra"));
    }

    #[tokio::test]
    async fn gate_account_credential_can_call_account_control_endpoints_and_updates_last_used() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let credential = gate_mint_account_credential(&db, &user);
        let credential_hash = hash_token(&credential);
        let before = db
            .get_account_credential_by_hash(&credential_hash)
            .unwrap()
            .unwrap();
        assert!(before.last_used_at.is_none());
        let service = Service::new(gate_router(config, db.clone()));
        for path in [
            "/api/users/me",
            "/api/tokens/list",
            "/api/tokens/register_hash",
            "/api/tokens/revoke",
            "/api/agent-tokens/register_hash",
        ] {
            let (status, body) = gate_send(&service, path, Some(&credential)).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::OK,
                "{} body: {:?}",
                path,
                body
            );
        }
        let after = db
            .get_account_credential_by_hash(&credential_hash)
            .unwrap()
            .unwrap();
        assert!(after.last_used_at.is_some());
    }

    #[tokio::test]
    async fn gate_account_credential_cannot_call_runtime_project_tool_or_mcp_paths() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let credential = gate_mint_account_credential(&db, &user);
        let service = Service::new(gate_router(config, db));
        for path in [
            "/api/runtime/status",
            "/api/projects/list",
            "/api/tools/list",
            "/api/shell/agent/register",
            "/mcp",
        ] {
            let (status, body) = gate_send(&service, path, Some(&credential)).await;
            assert_eq!(status, salvo::http::StatusCode::FORBIDDEN, "{}", path);
            assert_eq!(
                body["error"],
                "account credentials may only access account control endpoints"
            );
        }
    }

    #[tokio::test]
    async fn gate_disabled_user_account_credential_is_rejected() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let credential = gate_mint_account_credential(&db, &user);
        db.set_user_disabled(&user.id, true, chrono::Utc::now().timestamp())
            .unwrap();
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/users/me", Some(&credential)).await;
        assert_eq!(status, salvo::http::StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"], "Unauthorized");
    }

    #[tokio::test]
    async fn query_token_is_rejected_on_runtime_status() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        let mut resp = TestClient::post("http://localhost/api/runtime/status?token=secret")
            .send(&service)
            .await;
        assert_eq!(gate_status(&resp), salvo::http::StatusCode::UNAUTHORIZED);
        let body = resp.take_json::<serde_json::Value>().await.unwrap();
        assert_eq!(body["error"], "Unauthorized");
    }

    #[tokio::test]
    async fn query_token_still_works_for_agent_websocket_path() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/agents/ws?token=secret", None).await;
        assert_eq!(status, salvo::http::StatusCode::OK, "body: {:?}", body);
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn authorization_header_still_works_on_runtime_status() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/runtime/status", Some("secret")).await;
        assert_eq!(status, salvo::http::StatusCode::OK, "body: {:?}", body);
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn gate_bootstrap_can_call_all_apis() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        // Bootstrap reaches normal APIs and agent transport paths alike.
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
            "/api/shell/agent/register",
            "/api/agent-tokens/list",
        ] {
            let (status, body) = gate_send(&service, path, Some("secret")).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::OK,
                "{} body: {:?}",
                path,
                body
            );
        }
    }

    #[tokio::test]
    async fn gate_forbidden_response_is_json_not_html() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth(&agent_token)
            .send(&service)
            .await;
        assert_eq!(gate_status(&resp), salvo::http::StatusCode::FORBIDDEN);
        let ct = resp.headers().get("content-type").unwrap();
        assert!(
            ct.to_str().unwrap().contains("application/json"),
            "forbidden response must be JSON, got: {:?}",
            ct
        );
    }

    #[tokio::test]
    async fn gate_unauthorized_response_is_json_not_html() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        let resp = TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(gate_status(&resp), salvo::http::StatusCode::UNAUTHORIZED);
        let ct = resp.headers().get("content-type").unwrap();
        assert!(
            ct.to_str().unwrap().contains("application/json"),
            "unauthorized response must be JSON, got: {:?}",
            ct
        );
    }

    // -----------------------------------------------------------------------
    // Principal integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn auth_context_to_principal_preserves_identity() {
        let ctx = user_ctx("alice");
        let p = ctx.to_principal();
        assert_eq!(p.username.as_deref(), Some("alice"));
        assert_eq!(p.method, AuthMethod::Pat);
        assert!(p.has_scope(SCOPE_RUNTIME_READ));
    }

    #[test]
    fn auth_context_to_principal_bootstrap() {
        let ctx = bootstrap_ctx();
        let p = ctx.to_principal();
        assert!(p.is_bootstrap());
        assert!(p.is_admin());
    }

    #[test]
    fn auth_context_to_principal_agent() {
        let ctx = agent_ctx(
            "bob",
            "bob-phone",
            vec![
                SCOPE_AGENT_REGISTER.to_string(),
                SCOPE_AGENT_POLL.to_string(),
            ],
        );
        let p = ctx.to_principal();
        assert!(p.is_agent_token());
        assert_eq!(p.method, AuthMethod::AgentToken);
        assert!(p.can_use_agent_endpoint("bob-phone"));
        assert!(!p.can_use_agent_endpoint("other"));
    }

    // -----------------------------------------------------------------------
    // TokenVerifier trait tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn pat_verifier_bootstrap_when_auth_disabled() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: None, // auth disabled
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        };
        let verifier = PatVerifier;
        let result = verifier.verify(&config, None, "anything").await.unwrap();
        let ctx = result.expect("should return bootstrap context");
        assert!(ctx.is_bootstrap);
        assert_eq!(ctx.kind, AuthKind::Bootstrap);
    }

    #[tokio::test]
    async fn pat_verifier_bootstrap_token() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        };
        let verifier = PatVerifier;
        let result = verifier.verify(&config, None, "secret").await.unwrap();
        let ctx = result.expect("should return bootstrap context");
        assert!(ctx.is_bootstrap);
    }

    #[tokio::test]
    async fn pat_verifier_rejects_unknown_token_without_db() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        };
        let verifier = PatVerifier;
        let result = verifier
            .verify(&config, None, "wc_pat_bogus")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "unknown token without DB should return None"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_always_returns_none() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        };
        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, None, "some-oauth2-jwt")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "stub OAuth2 verifier should always return None"
        );
    }
}
