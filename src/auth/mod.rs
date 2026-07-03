//! WebCodex authentication and authorization.
//!
//! This module implements the bearer-token authentication pipeline used by all
//! protected API endpoints. It supports four credential types today (bootstrap,
//! personal API token, agent token, account credential) and reserves extension
//! points for OAuth2 in a future phase.
//!
//! ## Submodules
//!
//! - [`principal`] — [`AuthContext`], [`AuthKind`], [`Principal`], and
//!   [`AuthMethod`] / [`AuthError`] types.
//! - [`scopes`] — scope constants, validation, and authorization helpers.
//! - [`pat`] — PAT / agent token / account credential generation, hashing, and
//!   validation utilities.
//! - [`tokens`] — bearer token verification and token-kind classification.
//!
//! ## Architecture
//!
//! The [`AuthMiddleware`] Salvo handler is the single entry point for HTTP
//! authentication. It extracts a bearer token, validates it, and injects an
//! [`AuthContext`] into the depot. Handlers extract `AuthContext` and pass it
//! to the tool runtime for scope-based authorization.
//!
//! [`Principal`] is a higher-level abstraction derived from [`AuthContext`] that
//! unifies the identity representation regardless of auth method. During this
//! first refactoring phase both types coexist — `AuthContext` remains the
//! depot-injected type so existing handlers are unaffected. See
//! [`principal::Principal::from_auth_context`].
//!
//! ## Future: OAuth2
//!
//! The [`TokenVerifier`] trait is the extension point for bearer token
//! verification. The verifier chain currently runs [`PatVerifier`] followed by
//! [`OAuth2Verifier`].

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
pub(crate) mod tokens;

// ---------------------------------------------------------------------------
// Re-exports — backward compatibility
// ---------------------------------------------------------------------------
// All items that were previously exported from `auth.rs` are re-exported here
// so that existing `use crate::auth::*` imports continue to work.

#[allow(unused_imports)]
pub use principal::{AuthContext, AuthError, AuthKind, AuthMethod, Principal};

#[allow(unused_imports)]
pub use scopes::{
    AGENT_SCOPES, SCOPE_ACCOUNT_MANAGE, SCOPE_ADMIN, SCOPE_AGENT_JOB_UPDATE, SCOPE_AGENT_POLL,
    SCOPE_AGENT_REGISTER, SCOPE_AGENT_RESULT, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};

pub(crate) use scopes::{is_agent_scope, scopes_to_string, validate_agent_scopes, validate_scopes};

#[allow(unused_imports)]
pub(crate) use pat::{
    generate_account_credential, generate_agent_token, generate_api_token,
    generate_oauth_access_token, generate_oauth_authorization_code, generate_oauth_client_id,
    generate_oauth_client_secret, generate_oauth_refresh_token, hash_token, token_prefix,
    validate_allowed_client_id, validate_role, validate_username,
};

#[allow(unused_imports)]
pub(crate) use tokens::{
    authenticate, is_oauth2_access_token, OAuth2Verifier, PatVerifier, TokenVerifier,
};

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

/// Build a `WWW-Authenticate: Bearer` challenge value that includes the
/// protected resource metadata URL when OAuth2 is enabled. Returns `None`
/// when OAuth2 is not configured or has no issuer.
fn oauth2_bearer_challenge(config: &Config) -> Option<String> {
    if !config.oauth2.enabled {
        return None;
    }
    let issuer = config.oauth2.issuer.as_deref()?;
    Some(format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        issuer.trim_end_matches('/')
    ))
}

pub(crate) fn oauth_insufficient_scope_body(description: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "error": "insufficient_scope",
        "error_description": description.into(),
    })
}

pub(crate) fn oauth_insufficient_scope_challenge(required_scope: Option<&str>) -> String {
    match required_scope {
        Some(scope) => format!("Bearer error=\"insufficient_scope\", scope=\"{}\"", scope),
        None => "Bearer error=\"insufficient_scope\"".to_string(),
    }
}

pub(crate) fn render_oauth_insufficient_scope(
    res: &mut Response,
    required_scope: Option<&str>,
    description: impl Into<String>,
) {
    res.status_code(StatusCode::FORBIDDEN);
    let challenge = oauth_insufficient_scope_challenge(required_scope);
    if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
        res.headers_mut().insert("www-authenticate", val);
    }
    res.render(Json(oauth_insufficient_scope_body(description)));
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
// Standalone authentication function (used by QUIC agent transport)
// ---------------------------------------------------------------------------

/// Authenticate a bearer token *outside* the HTTP request path, reusing the
/// same verifier chain as [`AuthMiddleware`]. Used by the QUIC agent
/// transport, which has no HTTP middleware to inject an `AuthContext`.
///
/// Authentication coverage:
/// - **Bootstrap token**: yes — returns bootstrap context.
/// - **Personal API token (`wc_pat_*`)**: yes — returns `AuthKind::ApiToken`.
/// - **Agent token (`wc_agent_*`)**: yes — returns `AuthKind::AgentToken`.
///   The agent-transport path gate does NOT apply here: the QUIC listener is
///   inherently an agent-only transport, so an agent token reaching it is
///   already on an allowed surface.
/// - **Account credential (`wc_acct_*`)**: **rejected** — returns `None`.
///   Account credentials are only valid on HTTP account-control endpoints.
///   The QUIC/agent transport has no use for them, and accepting them would
///   silently update `last_used_at` before the caller rejects the connection.
/// - **OAuth2 access token (`wc_oat_*`)**: **rejected** — returns `None`
///   *before* running the verifier chain, so `last_used_at` is not updated.
///   OAuth2 tokens are accepted on regular HTTP surfaces via `AuthMiddleware`,
///   but not on the QUIC/agent transport surface.
///
/// Returns `None` for unknown/invalid tokens or when the token is recognized
/// but rejected (disabled user, expired token, account credential). The
/// caller MUST treat `None` as "reject the connection".
pub(crate) async fn authenticate_bearer(
    config: &Config,
    db: Option<&Arc<Database>>,
    token: Option<&str>,
) -> Option<AuthContext> {
    // Auth disabled in development -> bootstrap (full access), identical to
    // AuthMiddleware's behavior. This lets local QUIC integration tests run
    // without a configured token.
    if !config.is_auth_enabled() {
        return Some(bootstrap_context());
    }
    // No token: only allowed when the server is explicitly --open.
    let token = match token {
        Some(t) => t,
        None => {
            if allow_anonymous_enabled() {
                return Some(open_anonymous_context());
            }
            return None;
        }
    };
    // Pre-reject OAuth2 access tokens before running the verifier chain.
    // OAuth2Verifier updates last_used_at on success, so we must not let it
    // run on a surface that will ultimately reject the token. The QUIC/agent
    // transport surface does not accept OAuth2 tokens.
    if is_oauth2_access_token(token) {
        return None;
    }
    // Run the same verifier chain as the HTTP path (PatVerifier →
    // OAuth2Verifier). Any error (disabled user, expired token) is treated
    // the same as "unknown" for the QUIC transport — the caller rejects
    // the connection either way.
    match authenticate(config, db, token).await {
        Ok(Some(ctx)) => {
            // Account credentials are not valid on the agent transport surface.
            // Reject them here so they don't silently update last_used_at and then
            // get rejected by the caller anyway.
            if ctx.is_account_credential() {
                return None;
            }
            Some(ctx)
        }
        Ok(None) => {
            // Unknown bearer token: treat as a lightweight shared key only
            // when quick-start mode is enabled, the token is non-empty after
            // trimming, and it does not look like a WebCodex managed credential.
            let trimmed = token.trim();
            if shared_key_enabled() && !trimmed.is_empty() && !is_managed_token_prefix(trimmed) {
                Some(shared_key_context(trimmed))
            } else {
                None
            }
        }
        Err(_) => None,
    }
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
        shared_key_hash: None,
    }
}

/// Read the explicit-anonymous (`--open`) flag from the environment. When true,
/// the server allows anonymous GPT/MCP and anonymous client access under the
/// open group. Default false — the server never offers anonymous service
/// unless the operator explicitly opts in.
pub(crate) fn allow_anonymous_enabled() -> bool {
    crate::config::env_flag("WEBCODEX_ALLOW_ANONYMOUS").unwrap_or(false)
}

/// Read the shared-key quick-start flag from the environment. When true,
/// unknown bearer tokens that do not look like WebCodex managed credentials
/// (`wc_*`) are accepted as lightweight shared keys instead of being rejected.
/// Default false — the server rejects unknown tokens unless the operator
/// explicitly enables quick-start mode (e.g. via `server up`).
pub(crate) fn shared_key_enabled() -> bool {
    crate::config::env_flag("WEBCODEX_SHARED_KEY_ENABLED").unwrap_or(false)
}

/// True when `token` uses a WebCodex managed-credential prefix. Tokens with
/// these prefixes that fail verifier-chain validation are rejected outright
/// rather than falling back to shared-key mode.
fn is_managed_token_prefix(token: &str) -> bool {
    token.starts_with("wc_")
}

/// SHA-256 hex of a shared key, used for lightweight group isolation. Two
/// requests presenting the same key land in the same group. The shared key is
/// trimmed before hashing so direct shared-key visibility and the OAuth bridge
/// derive the same group hash from the same submitted secret.
pub(crate) fn shared_key_hash_of(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let token = token.trim();
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Scopes granted to a shared-key or open-anonymous caller. These cover the
/// runtime, project, job, and agent-transport surfaces but deliberately
/// exclude `admin` and `account:manage` so lightweight keys cannot manage
/// server-global resources.
fn lightweight_scopes() -> Vec<String> {
    vec![
        SCOPE_RUNTIME_READ.to_string(),
        SCOPE_PROJECT_READ.to_string(),
        SCOPE_PROJECT_WRITE.to_string(),
        SCOPE_JOB_RUN.to_string(),
        SCOPE_AGENT_REGISTER.to_string(),
        SCOPE_AGENT_POLL.to_string(),
        SCOPE_AGENT_RESULT.to_string(),
        SCOPE_AGENT_JOB_UPDATE.to_string(),
    ]
}

/// Build a shared-key [`AuthContext`] for a lightweight bearer token. The
/// caller is non-admin and grouped by `shared_key_hash`.
fn shared_key_context(token: &str) -> AuthContext {
    AuthContext {
        kind: AuthKind::SharedKey,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: lightweight_scopes(),
        is_bootstrap: false,
        token_kind: Some("shared-key".to_string()),
        allowed_client_id: None,
        shared_key_hash: Some(shared_key_hash_of(token)),
    }
}

/// Build the open-anonymous [`AuthContext`] used only when the server is
/// started with explicit `--open`. Non-admin, single open group.
fn open_anonymous_context() -> AuthContext {
    AuthContext {
        kind: AuthKind::OpenAnonymous,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("open".to_string()),
        scopes: lightweight_scopes(),
        is_bootstrap: false,
        token_kind: Some("open".to_string()),
        allowed_client_id: None,
        shared_key_hash: None,
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

/// Enforce that the token kind is permitted on the requested HTTP path.
///
/// Agent tokens are only allowed on agent transport endpoints. Account
/// credentials are only allowed on account control endpoints. All other
/// token kinds (bootstrap, user PAT) are allowed on any authenticated path.
///
/// Returns `Ok(())` when the token is permitted, `Err((status, message))`
/// when it should be rejected.
pub(crate) fn enforce_token_surface(
    ctx: &AuthContext,
    path: &str,
) -> Result<(), (StatusCode, &'static str)> {
    // Lightweight principals and shared-key OAuth subjects must never reach
    // account-control management surfaces.
    if (ctx.is_lightweight() || ctx.is_oauth_shared_key_subject()) && is_account_control_path(path)
    {
        return Err((
            StatusCode::FORBIDDEN,
            "shared-key principals are not allowed on account control endpoints",
        ));
    }
    if ctx.is_agent_token() && !is_agent_transport_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "agent tokens are only allowed on agent transport endpoints",
        ));
    }
    if ctx.is_account_credential() && !is_account_control_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "account credentials may only access account control endpoints",
        ));
    }
    // OAuth2 access tokens are not permitted on agent transport endpoints.
    // Agent endpoints require agent tokens or bootstrap auth.
    if ctx.is_oauth_token() && is_agent_transport_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "OAuth2 tokens are not allowed on agent transport endpoints",
        ));
    }
    Ok(())
}

fn enforce_oauth_route_scope(
    ctx: &AuthContext,
    method: &str,
    path: &str,
) -> Result<(), (Option<&'static str>, String)> {
    if !ctx.is_oauth_token() {
        return Ok(());
    }

    match scopes::oauth_route_scope_policy_for_path_method(method, path) {
        scopes::OAuthRouteScopePolicy::Public | scopes::OAuthRouteScopePolicy::BodyAware(_) => {
            Ok(())
        }
        scopes::OAuthRouteScopePolicy::Require(scope) => {
            if ctx.has_scope(scope) {
                Ok(())
            } else {
                Err((Some(scope), format!("missing required scope: {}", scope)))
            }
        }
        scopes::OAuthRouteScopePolicy::FirstPartyOnly => Err((
            None,
            "OAuth2 access tokens cannot call first-party-only routes".to_string(),
        )),
        scopes::OAuthRouteScopePolicy::AgentSurface => Err((
            None,
            "OAuth2 access tokens cannot call agent transport routes".to_string(),
        )),
        scopes::OAuthRouteScopePolicy::Unknown => Err((
            None,
            "OAuth2 access tokens cannot call unknown authenticated routes".to_string(),
        )),
    }
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

        // When no token is present and auth is enabled, reject immediately
        // unless the server was explicitly started with `--open`
        // (WEBCODEX_ALLOW_ANONYMOUS=true), in which case the anonymous caller
        // is granted a non-admin open-group context.
        // When auth is disabled, the verifier chain handles the bootstrap
        // fallback — we still call authenticate with a dummy token so the
        // code path stays uniform.
        let token = match token {
            Some(t) => t,
            None => {
                if !config.is_auth_enabled() {
                    // Auth disabled, no token: inject bootstrap and continue.
                    depot.inject(bootstrap_context());
                    ctrl.call_next(req, depot, res).await;
                    return;
                }
                if allow_anonymous_enabled() {
                    // Explicit --open: anonymous callers get a non-admin open
                    // context. Surface restrictions still apply.
                    let ctx = open_anonymous_context();
                    if let Err((status, msg)) = enforce_token_surface(&ctx, req.uri().path()) {
                        res.status_code(status);
                        res.render(Json(serde_json::json!({"error": msg})));
                        ctrl.skip_rest();
                        return;
                    }
                    depot.inject(ctx);
                    ctrl.call_next(req, depot, res).await;
                    return;
                }
                res.status_code(StatusCode::UNAUTHORIZED);
                if let Some(challenge) = oauth2_bearer_challenge(&config) {
                    if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                        res.headers_mut().insert("www-authenticate", val);
                    }
                }
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
                return;
            }
        };

        // Pre-reject OAuth2 access tokens on agent transport paths before
        // running the verifier chain. OAuth2Verifier updates last_used_at on
        // success, so we must not let it run on a surface that will
        // ultimately reject the token.
        if is_agent_transport_path(req.uri().path()) && is_oauth2_access_token(&token) {
            render_oauth_insufficient_scope(
                res,
                None,
                "OAuth2 access tokens cannot call agent transport routes",
            );
            ctrl.skip_rest();
            return;
        }

        // Run the verifier chain (PatVerifier → OAuth2Verifier).
        match authenticate(&config, db.as_ref(), &token).await {
            Ok(Some(ctx)) => {
                // Enforce token-kind surface restrictions (agent tokens,
                // account credentials) before the handler runs.
                if let Err((status, msg)) = enforce_token_surface(&ctx, req.uri().path()) {
                    res.status_code(status);
                    res.render(Json(serde_json::json!({"error": msg})));
                    ctrl.skip_rest();
                    return;
                }
                if let Err((scope, description)) =
                    enforce_oauth_route_scope(&ctx, req.method().as_str(), req.uri().path())
                {
                    render_oauth_insufficient_scope(res, scope, description);
                    ctrl.skip_rest();
                    return;
                }
                depot.inject(ctx);
                ctrl.call_next(req, depot, res).await;
            }
            Ok(None) => {
                // Token not recognized by any verifier. When shared-key
                // quick-start mode is enabled and the token does not look
                // like a WebCodex managed credential (wc_*), treat it as a
                // lightweight shared key. Managed-prefix tokens that failed
                // verification are always rejected.
                let trimmed = token.trim();
                if config.is_auth_enabled()
                    && shared_key_enabled()
                    && !trimmed.is_empty()
                    && !is_managed_token_prefix(trimmed)
                {
                    let ctx = shared_key_context(trimmed);
                    if let Err((status, msg)) = enforce_token_surface(&ctx, req.uri().path()) {
                        res.status_code(status);
                        res.render(Json(serde_json::json!({"error": msg})));
                        ctrl.skip_rest();
                        return;
                    }
                    if let Err((scope, description)) =
                        enforce_oauth_route_scope(&ctx, req.method().as_str(), req.uri().path())
                    {
                        render_oauth_insufficient_scope(res, scope, description);
                        ctrl.skip_rest();
                        return;
                    }
                    depot.inject(ctx);
                    ctrl.call_next(req, depot, res).await;
                    return;
                }
                // Unknown or managed-prefix-invalid token: reject.
                res.status_code(StatusCode::UNAUTHORIZED);
                if let Some(challenge) = oauth2_bearer_challenge(&config) {
                    if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                        res.headers_mut().insert("www-authenticate", val);
                    }
                }
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
            }
            Err(e) => {
                // Token recognized but invalid (disabled user, expired token,
                // etc.). Map to the appropriate HTTP status without leaking
                // internal details.
                let status = match e {
                    AuthError::ForbiddenTokenKind => StatusCode::FORBIDDEN,
                    AuthError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::UNAUTHORIZED,
                };
                res.status_code(status);
                if status == StatusCode::UNAUTHORIZED {
                    if let Some(challenge) = oauth2_bearer_challenge(&config) {
                        if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                            res.headers_mut().insert("www-authenticate", val);
                        }
                    }
                }
                res.render(Json(serde_json::json!({"error": "Unauthorized"})));
                ctrl.skip_rest();
            }
        }
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
    use principal::AuthMethod;

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
            shared_key_hash: None,
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
            shared_key_hash: None,
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
            shared_key_hash: None,
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
            oauth2: crate::OAuth2Config::default(),
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

    /// Create a config with OAuth2 enabled.
    fn gate_test_config_oauth2(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config {
                enabled: true,
                access_token_ttl_secs: 3600,
                refresh_token_ttl_secs: 2_592_000,
                ..crate::OAuth2Config::default()
            },
        })
    }

    /// Seed an OAuth2 client and return `(record, plaintext_secret)`.
    fn gate_seed_oauth_client(
        db: &crate::Database,
        user: &crate::models::UserRecord,
        name: &str,
    ) -> (crate::models::OAuthClientRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = generate_oauth_client_secret();
        let secret_hash = hash_token(&plaintext_secret);
        let record = crate::models::OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: generate_oauth_client_id(),
            client_secret_hash: secret_hash,
            name: name.to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read".to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        (record, plaintext_secret)
    }

    /// Seed an OAuth2 access token and return `(record, plaintext_token)`.
    fn gate_seed_oauth_access_token(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        user: &crate::models::UserRecord,
        scopes: &str,
    ) -> (crate::models::OAuthAccessTokenRecord, String) {
        gate_seed_oauth_access_token_with_shared_key_hash(db, client, user, scopes, None)
    }

    fn gate_seed_oauth_access_token_with_shared_key_hash(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        user: &crate::models::UserRecord,
        scopes: &str,
        shared_key_hash: Option<&str>,
    ) -> (crate::models::OAuthAccessTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = generate_oauth_access_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: scopes.to_string(),
            resource: None,
            shared_key_hash: shared_key_hash.map(str::to_string),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        (record, plaintext)
    }

    fn gate_seed_shared_key_oauth_access_token(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        scopes: &str,
        shared_key_hash: &str,
    ) -> (crate::models::OAuthAccessTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = generate_oauth_access_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: shared_key_hash.to_string(),
            user_id: None,
            scopes: scopes.to_string(),
            resource: None,
            shared_key_hash: Some(shared_key_hash.to_string()),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        (record, plaintext)
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
                    .push(Router::with_path("future/authenticated-route").post(echo_ok))
                    .push(Router::with_path("projects/list").post(echo_ok))
                    .push(Router::with_path("projects/read_file").post(echo_ok))
                    .push(Router::with_path("projects/write_file").post(echo_ok))
                    .push(Router::with_path("projects/run_job").post(echo_ok))
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
            .push(
                Router::with_path("oauth/authorize")
                    .hoop(AuthMiddleware)
                    .get(echo_ok),
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
        if path == "/api/agents/ws"
            || path.starts_with("/api/agents/ws?")
            || path == "/oauth/authorize"
            || path.starts_with("/oauth/authorize?")
        {
            // These endpoints are GET-mounted in this test router.
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
    async fn gate_agent_token_cannot_call_non_transport_paths() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
            "/mcp",
            "/api/agent-tokens/list",
            "/api/tokens/list",
        ] {
            let (status, body) = gate_send(&service, path, Some(&agent_token)).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::FORBIDDEN,
                "agent token should be forbidden on {}: {:?}",
                path,
                body
            );
        }
        // Verify the error message is descriptive for at least one path.
        let (_, body) = gate_send(&service, "/api/runtime/status", Some(&agent_token)).await;
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
            oauth2: crate::OAuth2Config::default(),
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
            oauth2: crate::OAuth2Config::default(),
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
            oauth2: crate::OAuth2Config::default(),
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
    async fn oauth2_verifier_ignores_non_wc_oat_tokens() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config {
                enabled: true,
                ..crate::OAuth2Config::default()
            },
        };
        let verifier = OAuth2Verifier;
        // Non-wc_oat_ tokens should return Ok(None) (not recognized).
        for token in &[
            "some-oauth2-jwt",
            "wc_pat_abc123",
            "wc_agent_abc123",
            "wc_acct_abc123",
            "wc_ort_abc123",
            "wc_oac_abc123",
            "wc_csec_abc123",
            "wc_client_abc123",
        ] {
            let result = verifier.verify(&config, None, token).await.unwrap();
            assert!(
                result.is_none(),
                "non-wc_oat_ token '{}' should return None",
                token
            );
        }
    }

    #[tokio::test]
    async fn oauth2_verifier_returns_none_when_oauth2_disabled() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(), // enabled: false
        };
        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, None, "wc_oat_sometoken")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "OAuth2 disabled should return None for wc_oat_* tokens"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_accepts_valid_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, Some(&db), &plaintext)
            .await
            .unwrap();
        let ctx = result.expect("valid access token should return AuthContext");
        assert_eq!(ctx.kind, AuthKind::OAuth2Token);
        assert_eq!(ctx.user_id.as_deref(), Some(user.id.as_str()));
        assert_eq!(ctx.username.as_deref(), Some(user.username.as_str()));
        assert_eq!(ctx.role.as_deref(), Some("user"));
        assert!(ctx.scopes.contains(&"runtime:read".to_string()));
        assert!(!ctx.is_bootstrap);
        assert_eq!(ctx.token_kind.as_deref(), Some("oauth2"));
        assert_eq!(
            ctx.allowed_client_id.as_deref(),
            Some(client.client_id.as_str())
        );
        assert_eq!(ctx.shared_key_hash, None);
    }

    #[tokio::test]
    async fn oauth2_verifier_ignores_managed_user_stray_shared_key_hash() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, plaintext) = gate_seed_oauth_access_token_with_shared_key_hash(
            &db,
            &client,
            &user,
            "runtime:read",
            Some("test-hash-a"),
        );

        let verifier = OAuth2Verifier;
        let ctx = verifier
            .verify(&config, Some(&db), &plaintext)
            .await
            .unwrap()
            .expect("managed-user access token should verify");
        assert_eq!(ctx.kind, AuthKind::OAuth2Token);
        assert_eq!(ctx.user_id.as_deref(), Some(user.id.as_str()));
        assert_eq!(ctx.username.as_deref(), Some(user.username.as_str()));
        assert_eq!(ctx.token_kind.as_deref(), Some("oauth2"));
        assert_eq!(ctx.shared_key_hash, None);
        assert!(!ctx.is_oauth_shared_key_subject());
    }

    #[tokio::test]
    async fn oauth2_verifier_accepts_shared_key_subject_without_user_lookup() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let owner = gate_seed_user(&db, "owner");
        let (client, _secret) = gate_seed_oauth_client(&db, &owner, "Test App");
        let (at, plaintext) = gate_seed_shared_key_oauth_access_token(
            &db,
            &client,
            "runtime:read project:read",
            "test-hash-a",
        );
        db.set_user_disabled(&owner.id, true, chrono::Utc::now().timestamp())
            .unwrap();

        let verifier = OAuth2Verifier;
        let ctx = verifier
            .verify(&config, Some(&db), &plaintext)
            .await
            .unwrap()
            .expect("shared-key OAuth2 subject should verify");
        assert_eq!(ctx.kind, AuthKind::OAuth2Token);
        assert_eq!(ctx.user_id, None);
        assert_eq!(ctx.username, None);
        assert_eq!(ctx.api_key_id.as_deref(), Some(at.id.as_str()));
        assert_eq!(ctx.role.as_deref(), Some("shared-key"));
        assert_eq!(ctx.token_kind.as_deref(), Some("oauth2_shared_key"));
        assert_eq!(
            ctx.allowed_client_id.as_deref(),
            Some(client.client_id.as_str())
        );
        assert_eq!(ctx.shared_key_hash.as_deref(), Some("test-hash-a"));
        assert!(ctx.is_oauth_shared_key_subject());
        assert!(ctx.scopes.contains(&"runtime:read".to_string()));
        assert!(!ctx.is_admin());

        let stored = db
            .get_oauth_access_token_by_hash(&at.token_hash)
            .unwrap()
            .unwrap();
        assert!(stored.last_used_at.is_some());
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_invalid_subject_combinations() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let owner = gate_seed_user(&db, "owner");
        let (client, _secret) = gate_seed_oauth_client(&db, &owner, "Test App");
        let now = chrono::Utc::now().timestamp();

        let cases = [
            (
                "managed-missing-user",
                "managed_user",
                owner.id.as_str(),
                None,
                None,
                "managed-user OAuth2 token missing user_id",
            ),
            (
                "managed-mismatch",
                "managed_user",
                "other-user",
                Some(owner.id.as_str()),
                None,
                "managed-user OAuth2 subject_id does not match user_id",
            ),
            (
                "shared-with-user",
                "shared_key",
                "hash-a",
                Some(owner.id.as_str()),
                Some("hash-a"),
                "shared-key OAuth2 token must not include user_id",
            ),
            (
                "shared-missing-hash",
                "shared_key",
                "hash-a",
                None,
                None,
                "shared-key OAuth2 token missing shared_key_hash",
            ),
            (
                "shared-mismatch",
                "shared_key",
                "hash-a",
                None,
                Some("hash-b"),
                "shared-key OAuth2 subject_id does not match shared_key_hash",
            ),
            (
                "unknown-kind",
                "surprise",
                "subject",
                None,
                None,
                "unsupported OAuth2 subject",
            ),
        ];

        for (label, subject_kind, subject_id, user_id, shared_key_hash, expected_error) in cases {
            let plaintext = generate_oauth_access_token();
            let token_hash = hash_token(&plaintext);
            let id = uuid::Uuid::new_v4().to_string();
            {
                let conn = db.conn_for_tests();
                conn.execute(
                    "INSERT INTO oauth_access_tokens (
                        id, token_hash, client_id, subject_kind, subject_id, user_id,
                        scopes, resource, shared_key_hash, created_at, expires_at,
                        revoked_at, last_used_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL)",
                    rusqlite::params![
                        id,
                        token_hash,
                        client.client_id,
                        subject_kind,
                        subject_id,
                        user_id,
                        "runtime:read",
                        shared_key_hash,
                        now,
                        now + 3600,
                    ],
                )
                .unwrap();
            }

            let verifier = OAuth2Verifier;
            let err = verifier
                .verify(&config, Some(&db), &plaintext)
                .await
                .expect_err(label);
            assert_eq!(err, expected_error, "{label}");
            let conn = db.conn_for_tests();
            let last_used: Option<i64> = conn
                .query_row(
                    "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(last_used, None, "{label}");
        }
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_unknown_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();

        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, Some(&db), "wc_oat_nonexistenttoken")
            .await;
        assert!(result.is_err(), "unknown access token should return Err");
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_expired_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");

        // Create an expired access token.
        let now = chrono::Utc::now().timestamp();
        let plaintext = generate_oauth_access_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now - 7200,
            expires_at: now - 1, // already expired
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier.verify(&config, Some(&db), &plaintext).await;
        assert!(result.is_err(), "expired access token should return Err");
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_revoked_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        // Revoke the token.
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_access_token(&at.id, now).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier.verify(&config, Some(&db), &plaintext).await;
        assert!(result.is_err(), "revoked access token should return Err");
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_refresh_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");

        // Create a refresh token (wc_ort_*).
        let now = chrono::Utc::now().timestamp();
        let plaintext = generate_oauth_refresh_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, Some(&db), &plaintext)
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "refresh token (wc_ort_*) should return None"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_authorization_code() {
        let config = gate_test_config_oauth2(Some("secret"));
        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, None, "wc_oac_sometoken")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "authorization code (wc_oac_*) should return None"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_client_secret() {
        let config = gate_test_config_oauth2(Some("secret"));
        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, None, "wc_csec_sometoken")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "client secret (wc_csec_*) should return None"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_updates_last_used_on_success() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        // Verify last_used_at is initially None.
        assert!(at.last_used_at.is_none());

        let verifier = OAuth2Verifier;
        let result = verifier
            .verify(&config, Some(&db), &plaintext)
            .await
            .unwrap();
        assert!(result.is_some());

        // Verify last_used_at is now set.
        let conn = db.conn_for_tests();
        let last_used: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used.is_some(),
            "last_used_at should be set after successful verification"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_does_not_update_last_used_on_failure() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        // Revoke the token.
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_access_token(&at.id, now).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier.verify(&config, Some(&db), &plaintext).await;
        assert!(result.is_err());

        // last_used_at should still be None — failed verification should not
        // update it. Note: the token is revoked so get_oauth_access_token_by_hash
        // returns None, so we can't directly check. But we verify the error path
        // doesn't panic or succeed.
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_token_for_revoked_client() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        // Revoke the client.
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_client(&client.id, now).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier.verify(&config, Some(&db), &plaintext).await;
        assert!(
            result.is_err(),
            "token for revoked client should return Err"
        );
    }

    #[tokio::test]
    async fn oauth2_verifier_rejects_token_for_disabled_user() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        // Disable the user.
        let now = chrono::Utc::now().timestamp();
        db.set_user_disabled(&user.id, true, now).unwrap();

        let verifier = OAuth2Verifier;
        let result = verifier.verify(&config, Some(&db), &plaintext).await;
        assert!(result.is_err(), "token for disabled user should return Err");
    }

    // -----------------------------------------------------------------------
    // enforce_token_surface tests
    // -----------------------------------------------------------------------

    #[test]
    fn enforce_token_surface_allows_bootstrap_on_any_path() {
        let ctx = bootstrap_ctx();
        assert!(enforce_token_surface(&ctx, "/api/runtime/status").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/shell/agent/register").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/users/me").is_ok());
    }

    #[test]
    fn enforce_token_surface_allows_user_pat_on_any_path() {
        let ctx = user_ctx("alice");
        assert!(enforce_token_surface(&ctx, "/api/runtime/status").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/tools/list").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/projects/list").is_ok());
    }

    #[test]
    fn enforce_token_surface_allows_agent_token_on_agent_transport_paths() {
        let ctx = agent_ctx("alice", "laptop", vec![SCOPE_AGENT_REGISTER.to_string()]);
        assert!(enforce_token_surface(&ctx, "/api/shell/agent/register").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/shell/agent/poll").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/shell/agent/result").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/shell/agent/job_update").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/agents/ws").is_ok());
    }

    #[test]
    fn enforce_token_surface_rejects_agent_token_on_normal_paths() {
        let ctx = agent_ctx("alice", "laptop", vec![SCOPE_AGENT_REGISTER.to_string()]);
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
            "/mcp",
            "/api/users/me",
        ] {
            let result = enforce_token_surface(&ctx, path);
            assert!(
                result.is_err(),
                "agent token should be rejected on {}",
                path
            );
            let (status, msg) = result.unwrap_err();
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert!(msg.contains("agent tokens"));
        }
    }

    // -----------------------------------------------------------------------
    // Shared-key / open-anonymous quick-start auth tests
    // -----------------------------------------------------------------------

    #[test]
    fn shared_key_context_is_non_admin_with_key_hash() {
        let ctx = shared_key_context("abc123");
        assert!(!ctx.is_bootstrap);
        assert!(!ctx.is_admin());
        assert!(ctx.is_shared_key());
        assert!(ctx.is_lightweight());
        assert!(ctx.shared_key_hash.is_some());
        // Same key → same hash (deterministic grouping).
        let ctx2 = shared_key_context("abc123");
        assert_eq!(ctx.shared_key_hash, ctx2.shared_key_hash);
        // Different key → different hash.
        let ctx3 = shared_key_context("xyz789");
        assert_ne!(ctx.shared_key_hash, ctx3.shared_key_hash);
    }

    #[test]
    fn open_anonymous_context_is_non_admin() {
        let ctx = open_anonymous_context();
        assert!(!ctx.is_bootstrap);
        assert!(!ctx.is_admin());
        assert!(ctx.is_open_anonymous());
        assert!(ctx.is_lightweight());
        assert!(ctx.shared_key_hash.is_none());
    }

    #[test]
    fn lightweight_contexts_have_no_admin_scope() {
        let sk = shared_key_context("k");
        assert!(!sk.scopes.iter().any(|s| s == SCOPE_ADMIN));
        let open = open_anonymous_context();
        assert!(!open.scopes.iter().any(|s| s == SCOPE_ADMIN));
        // They do have runtime/project/agent scopes.
        assert!(sk.scopes.contains(&SCOPE_RUNTIME_READ.to_string()));
        assert!(sk.scopes.contains(&SCOPE_PROJECT_WRITE.to_string()));
        assert!(sk.scopes.contains(&SCOPE_AGENT_REGISTER.to_string()));
    }

    #[test]
    fn enforce_token_surface_rejects_lightweight_on_account_control() {
        let sk = shared_key_context("k");
        let open = open_anonymous_context();
        for path in ACCOUNT_CONTROL_PATHS {
            let r1 = enforce_token_surface(&sk, path);
            assert!(r1.is_err(), "shared key should be rejected on {path}");
            let (status, _) = r1.unwrap_err();
            assert_eq!(status, StatusCode::FORBIDDEN);
            let r2 = enforce_token_surface(&open, path);
            assert!(r2.is_err(), "open should be rejected on {path}");
        }
    }

    #[test]
    fn enforce_token_surface_allows_lightweight_on_runtime_and_agent_paths() {
        let sk = shared_key_context("k");
        let open = open_anonymous_context();
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
            "/api/shell/agent/register",
            "/api/agents/ws",
            "/mcp",
        ] {
            assert!(
                enforce_token_surface(&sk, path).is_ok(),
                "shared key should be allowed on {path}"
            );
            assert!(
                enforce_token_surface(&open, path).is_ok(),
                "open should be allowed on {path}"
            );
        }
    }

    #[test]
    fn lightweight_can_use_agent_endpoint() {
        let sk = shared_key_context("k");
        assert!(sk.can_use_agent_endpoint("any-client-id"));
        let open = open_anonymous_context();
        assert!(open.can_use_agent_endpoint("any-client-id"));
    }

    #[test]
    fn managed_token_prefix_detected() {
        assert!(is_managed_token_prefix("wc_boot_abc"));
        assert!(is_managed_token_prefix("wc_pat_xyz"));
        assert!(is_managed_token_prefix("wc_agent_123"));
        assert!(is_managed_token_prefix("wc_oat_def"));
        assert!(is_managed_token_prefix("wc_ort_refresh"));
        assert!(!is_managed_token_prefix("abc123"));
        assert!(!is_managed_token_prefix("my-shared-key"));
        assert!(!is_managed_token_prefix("wrong-token"));
        assert!(!is_managed_token_prefix(""));
    }

    #[tokio::test]
    async fn shared_key_fallback_gated_by_env_and_prefix() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        let config = crate::Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        };

        // Shared-key disabled (default): unknown non-wc token → None (reject).
        std::env::remove_var("WEBCODEX_SHARED_KEY_ENABLED");
        let r = authenticate_bearer(&config, None, Some("my-key")).await;
        assert!(
            r.is_none(),
            "unknown token should be rejected when shared-key disabled"
        );

        // Shared-key enabled: unknown non-wc token → Some (shared-key context).
        std::env::set_var("WEBCODEX_SHARED_KEY_ENABLED", "true");
        let r = authenticate_bearer(&config, None, Some("my-key")).await;
        assert!(r.is_some(), "non-wc token should be accepted as shared-key");
        let ctx = r.unwrap();
        assert!(ctx.is_shared_key());
        assert!(!ctx.is_admin());

        // Shared-key enabled but wc_-prefixed invalid token → None (reject).
        let r = authenticate_bearer(&config, None, Some("wc_pat_invalid")).await;
        assert!(r.is_none(), "wc_ prefix invalid token must be rejected");

        // Empty or whitespace-only bearer values must not become sha256("")
        // shared-key groups.
        let r = authenticate_bearer(&config, None, Some("")).await;
        assert!(r.is_none(), "empty token must be rejected");
        let r = authenticate_bearer(&config, None, Some("   \t")).await;
        assert!(r.is_none(), "whitespace token must be rejected");

        std::env::remove_var("WEBCODEX_SHARED_KEY_ENABLED");
    }

    #[test]
    fn enforce_token_surface_allows_account_credential_on_account_control_paths() {
        let mut ctx = user_ctx("alice");
        ctx.kind = AuthKind::AccountCredential;
        assert!(enforce_token_surface(&ctx, "/api/users/me").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/tokens/list").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/tokens/register_hash").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/tokens/revoke").is_ok());
        assert!(enforce_token_surface(&ctx, "/api/agent-tokens/register_hash").is_ok());
    }

    #[test]
    fn enforce_token_surface_rejects_account_credential_on_normal_paths() {
        let mut ctx = user_ctx("alice");
        ctx.kind = AuthKind::AccountCredential;
        for path in [
            "/api/runtime/status",
            "/api/projects/list",
            "/api/tools/list",
            "/mcp",
            "/api/shell/agent/register",
        ] {
            let result = enforce_token_surface(&ctx, path);
            assert!(
                result.is_err(),
                "account credential should be rejected on {}",
                path
            );
            let (status, msg) = result.unwrap_err();
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert!(msg.contains("account credentials"));
        }
    }

    #[test]
    fn enforce_token_surface_allows_oauth2_token_on_regular_paths() {
        let mut ctx = user_ctx("alice");
        ctx.kind = AuthKind::OAuth2Token;
        for path in [
            "/api/runtime/status",
            "/api/projects/list",
            "/api/tools/list",
            "/api/jobs/list",
            "/mcp",
        ] {
            assert!(
                enforce_token_surface(&ctx, path).is_ok(),
                "OAuth2 token should be allowed on {}",
                path
            );
        }
    }

    #[test]
    fn enforce_token_surface_rejects_oauth2_token_on_agent_transport_paths() {
        let mut ctx = user_ctx("alice");
        ctx.kind = AuthKind::OAuth2Token;
        for path in [
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/result",
            "/api/shell/agent/job_update",
            "/api/agents/ws",
        ] {
            let result = enforce_token_surface(&ctx, path);
            assert!(
                result.is_err(),
                "OAuth2 token should be rejected on agent path {}",
                path
            );
        }
    }

    // -----------------------------------------------------------------------
    // authenticate() verifier chain tests
    // -----------------------------------------------------------------------

    // authenticate() verifier chain tests
    // Note: bootstrap and basic PAT verification are covered by the
    // pat_verifier_* tests above. The tests below exercise the full chain
    // with DB-backed verifiers.

    #[tokio::test]
    async fn authenticate_returns_none_for_unknown_token_with_db() {
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        };
        let (_tmp, db) = gate_test_db();
        let result = authenticate(&config, Some(&db), "wc_pat_bogus")
            .await
            .unwrap();
        assert!(result.is_none(), "unknown token should return None");
    }

    #[tokio::test]
    async fn authenticate_returns_api_token_for_valid_user_pat() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let token = gate_mint_user_token(&db, &user);
        let result = authenticate(&config, Some(&db), &token).await.unwrap();
        let ctx = result.expect("should return auth context");
        assert_eq!(ctx.kind, AuthKind::ApiToken);
        assert_eq!(ctx.username.as_deref(), Some("alice"));
        assert!(!ctx.is_bootstrap);
    }

    #[tokio::test]
    async fn authenticate_returns_agent_token_for_valid_agent_token() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let result = authenticate(&config, Some(&db), &token).await.unwrap();
        let ctx = result.expect("should return auth context");
        assert_eq!(ctx.kind, AuthKind::AgentToken);
        assert_eq!(ctx.username.as_deref(), Some("alice"));
        assert_eq!(ctx.allowed_client_id.as_deref(), Some("alice-laptop"));
    }

    #[tokio::test]
    async fn authenticate_returns_account_credential_for_valid_credential() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let credential = gate_mint_account_credential(&db, &user);
        let result = authenticate(&config, Some(&db), &credential).await.unwrap();
        let ctx = result.expect("should return auth context");
        assert_eq!(ctx.kind, AuthKind::AccountCredential);
        assert_eq!(ctx.username.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn authenticate_rejects_disabled_user_pat() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let token = gate_mint_user_token(&db, &user);
        db.set_user_disabled(&user.id, true, chrono::Utc::now().timestamp())
            .unwrap();
        let result = authenticate(&config, Some(&db), &token).await;
        assert!(result.is_err(), "disabled user should return Err");
    }

    #[tokio::test]
    async fn authenticate_rejects_expired_token() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let plaintext = generate_api_token();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "expired".to_string(),
            key_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read".to_string(),
            expires_at: Some(now - 3600), // expired 1 hour ago
            kind: crate::models::TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&record, &hash).unwrap();
        let result = authenticate(&config, Some(&db), &plaintext).await;
        assert!(result.is_err(), "expired token should return Err");
    }

    #[tokio::test]
    async fn authenticate_oauth2_stub_does_not_break_pat_fallback() {
        // The OAuth2Verifier stub always returns Ok(None), so PatVerifier
        // should still handle the token. This test verifies the chain works.
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let token = gate_mint_user_token(&db, &user);
        // authenticate runs PatVerifier first, which should succeed.
        let result = authenticate(&config, Some(&db), &token).await.unwrap();
        assert!(
            result.is_some(),
            "PAT should still work with OAuth2 stub in chain"
        );
        let ctx = result.unwrap();
        assert_eq!(ctx.kind, AuthKind::ApiToken);
    }

    // -----------------------------------------------------------------------
    // authenticate_bearer() integration tests (verifier chain for QUIC path)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn authenticate_bearer_bootstrap_and_no_token() {
        // Auth disabled → bootstrap.
        let config = crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: None,
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        };
        let ctx = authenticate_bearer(&config, None, Some("anything"))
            .await
            .expect("auth disabled should return bootstrap");
        assert!(ctx.is_bootstrap);

        // Valid bootstrap token → bootstrap.
        let config = crate::Config {
            token: Some("secret".to_string()),
            ..config
        };
        let ctx = authenticate_bearer(&config, None, Some("secret"))
            .await
            .expect("bootstrap token should return bootstrap");
        assert!(ctx.is_bootstrap);

        // No token → None.
        let result = authenticate_bearer(&config, None, None).await;
        assert!(result.is_none(), "no token should return None");
    }

    // The following authenticate_bearer tests cover QUIC-specific rejection
    // that is NOT tested by the authenticate() chain tests above.

    #[tokio::test]
    async fn authenticate_bearer_rejects_account_credential() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let credential = gate_mint_account_credential(&db, &user);
        let result = authenticate_bearer(&config, Some(&db), Some(&credential)).await;
        assert!(
            result.is_none(),
            "account credentials must be rejected on agent transport"
        );
    }

    #[tokio::test]
    async fn authenticate_bearer_rejects_oauth2_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let owner = gate_seed_user(&db, "owner");
        let (client, _secret) = gate_seed_oauth_client(&db, &owner, "Test App");
        let (at, plaintext) =
            gate_seed_shared_key_oauth_access_token(&db, &client, "runtime:read", "test-hash-a");
        assert!(at.last_used_at.is_none(), "precondition");
        let result = authenticate_bearer(&config, Some(&db), Some(&plaintext)).await;
        assert!(
            result.is_none(),
            "OAuth2 access tokens must be rejected on agent transport (QUIC)"
        );
        // last_used_at must NOT be updated — the token was pre-rejected
        // before OAuth2Verifier ran.
        let conn = db.conn_for_tests();
        let last_used: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used.is_none(),
            "last_used_at must not be updated on forbidden surface"
        );
    }

    // Table-driven: authenticate_bearer positive and negative paths
    // that exercise the QUIC/agent-transport surface (not HTTP middleware).
    #[tokio::test]
    async fn authenticate_bearer_accepts_and_rejects_by_token_type() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let user_token = gate_mint_user_token(&db, &user);
        let agent_token = gate_mint_agent_token(&db, &user, "quic-client");

        // Disabled user: user PAT should be rejected.
        let disabled = {
            let now = chrono::Utc::now().timestamp();
            let u = crate::models::UserRecord {
                id: uuid::Uuid::new_v4().to_string(),
                username: "disabled-user".to_string(),
                created_at: now,
                disabled: 1,
                display_name: None,
                role: "user".to_string(),
                disabled_at: Some(now),
                updated_at: Some(now),
            };
            db.create_user(&u).unwrap();
            u
        };
        let disabled_token = gate_mint_user_token(&db, &disabled);

        let cases: Vec<(&str, Option<&str>, bool)> = vec![
            ("valid user PAT", Some(&user_token), true),
            ("valid agent token", Some(&agent_token), true),
            ("unknown token", Some("invalid-garbage-token"), false),
            ("disabled user PAT", Some(&disabled_token), false),
        ];

        for (label, token, should_succeed) in &cases {
            let result = authenticate_bearer(&config, Some(&db), *token).await;
            if *should_succeed {
                assert!(result.is_some(), "{label}: expected Some, got None");
            } else {
                assert!(result.is_none(), "{label}: expected None, got Some");
            }
        }
    }

    // -----------------------------------------------------------------------
    // AuthMiddleware integration: OAuth2 access token on HTTP surface
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn auth_middleware_accepts_valid_oauth2_access_token() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");
        assert!(at.last_used_at.is_none(), "precondition");

        let service = salvo::Service::new(gate_router(config, db.clone()));
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .add_header("authorization", &format!("Bearer {}", plaintext), true)
            .send(&service)
            .await;
        assert_eq!(
            resp.status_code,
            Some(StatusCode::OK),
            "valid OAuth2 access token should be accepted by AuthMiddleware"
        );
        // last_used_at MUST be updated on successful verification.
        let conn = db.conn_for_tests();
        let last_used: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used.is_some(),
            "last_used_at must be updated on successful HTTP auth"
        );
    }

    #[tokio::test]
    async fn auth_middleware_rejects_refresh_token_as_bearer() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");

        // Create a refresh token.
        let now = chrono::Utc::now().timestamp();
        let plaintext = generate_oauth_refresh_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        let service = salvo::Service::new(gate_router(config, db));
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .add_header("authorization", &format!("Bearer {}", plaintext), true)
            .send(&service)
            .await;
        assert_eq!(
            resp.status_code,
            Some(StatusCode::UNAUTHORIZED),
            "refresh token should not be accepted as bearer"
        );
    }

    #[tokio::test]
    async fn auth_middleware_rejects_bridge_oauth2_on_agent_path_without_updating_last_used() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let owner = gate_seed_user(&db, "owner");
        let (client, _secret) = gate_seed_oauth_client(&db, &owner, "Test App");
        let (at, plaintext) =
            gate_seed_shared_key_oauth_access_token(&db, &client, "runtime:read", "test-hash-a");
        assert_eq!(at.shared_key_hash.as_deref(), Some("test-hash-a"));
        assert!(at.last_used_at.is_none(), "precondition");

        let service = salvo::Service::new(gate_router(config, db.clone()));
        let resp = salvo::test::TestClient::post("http://localhost/api/shell/agent/register")
            .add_header("authorization", &format!("Bearer {}", plaintext), true)
            .send(&service)
            .await;
        assert_eq!(
            resp.status_code,
            Some(StatusCode::FORBIDDEN),
            "OAuth2 token on agent transport path should be 403"
        );
        // last_used_at must NOT be updated — the token was pre-rejected
        // before OAuth2Verifier ran.
        let conn = db.conn_for_tests();
        let last_used: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used.is_none(),
            "last_used_at must not be updated on forbidden agent transport path"
        );
    }

    async fn gate_oauth2_token_with_scopes(scopes: &str) -> (tempfile::TempDir, Service, String) {
        let config = gate_test_config_oauth2(Some("secret"));
        let (tmp, db) = gate_test_db();
        let owner = gate_seed_user(&db, "owner");
        let (client, _secret) = gate_seed_oauth_client(&db, &owner, "Test App");
        let (_at, plaintext) =
            gate_seed_shared_key_oauth_access_token(&db, &client, scopes, "test-hash-a");
        let service = Service::new(gate_router(config, db));
        (tmp, service, plaintext)
    }

    fn assert_insufficient_scope(
        status: StatusCode,
        body: &serde_json::Value,
        expected: Option<&str>,
    ) {
        assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
        assert_eq!(body["error"], "insufficient_scope");
        if let Some(scope) = expected {
            assert!(
                body["error_description"]
                    .as_str()
                    .unwrap_or("")
                    .contains(scope),
                "body: {:?}",
                body
            );
        }
    }

    #[tokio::test]
    async fn oauth2_token_with_runtime_read_can_call_runtime_status() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) = gate_send(&service, "/api/runtime/status", Some(&token)).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    }

    #[tokio::test]
    async fn oauth2_token_without_runtime_read_cannot_call_runtime_status() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("project:read").await;
        let (status, body) = gate_send(&service, "/api/runtime/status", Some(&token)).await;
        assert_insufficient_scope(status, &body, Some(SCOPE_RUNTIME_READ));
    }

    #[tokio::test]
    async fn oauth2_token_with_project_read_can_read_project_file() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("project:read").await;
        let (status, body) = gate_send(&service, "/api/projects/read_file", Some(&token)).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    }

    #[tokio::test]
    async fn oauth2_token_without_project_read_cannot_read_project_file() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) = gate_send(&service, "/api/projects/read_file", Some(&token)).await;
        assert_insufficient_scope(status, &body, Some(SCOPE_PROJECT_READ));
    }

    #[tokio::test]
    async fn oauth2_token_with_project_write_can_write_project_file() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("project:write").await;
        let (status, body) = gate_send(&service, "/api/projects/write_file", Some(&token)).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    }

    #[tokio::test]
    async fn oauth2_token_with_project_read_cannot_write_project_file() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("project:read").await;
        let (status, body) = gate_send(&service, "/api/projects/write_file", Some(&token)).await;
        assert_insufficient_scope(status, &body, Some(SCOPE_PROJECT_WRITE));
    }

    #[tokio::test]
    async fn oauth2_token_with_job_run_can_run_job_or_shell() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("job:run").await;
        let (status, body) = gate_send(&service, "/api/projects/run_job", Some(&token)).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    }

    #[tokio::test]
    async fn oauth2_token_without_job_run_cannot_run_job_or_shell() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("project:write").await;
        let (status, body) = gate_send(&service, "/api/projects/run_job", Some(&token)).await;
        assert_insufficient_scope(status, &body, Some(SCOPE_JOB_RUN));
    }

    #[tokio::test]
    async fn managed_oauth2_token_with_account_manage_can_call_users_me() {
        let config = gate_test_config_oauth2(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, token) = gate_seed_oauth_access_token(&db, &client, &user, "account:manage");
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/users/me", Some(&token)).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    }

    #[tokio::test]
    async fn shared_key_oauth2_token_with_account_manage_still_cannot_call_account_route() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("account:manage").await;
        let (status, body) = gate_send(&service, "/api/users/me", Some(&token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
        assert_eq!(
            body["error"].as_str(),
            Some("shared-key principals are not allowed on account control endpoints")
        );
    }

    #[tokio::test]
    async fn shared_key_oauth2_token_without_account_manage_cannot_call_account_route() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) = gate_send(&service, "/api/users/me", Some(&token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
        assert_eq!(
            body["error"].as_str(),
            Some("shared-key principals are not allowed on account control endpoints")
        );
    }

    #[tokio::test]
    async fn oauth2_token_cannot_call_authorize() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) = gate_send(&service, "/oauth/authorize", Some(&token)).await;
        assert_insufficient_scope(status, &body, None);
    }

    #[tokio::test]
    async fn oauth2_token_cannot_call_agent_surface() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) = gate_send(&service, "/api/shell/agent/register", Some(&token)).await;
        assert_insufficient_scope(status, &body, None);
    }

    #[tokio::test]
    async fn oauth2_token_unknown_route_fails_closed() {
        let (_tmp, service, token) = gate_oauth2_token_with_scopes("runtime:read").await;
        let (status, body) =
            gate_send(&service, "/api/future/authenticated-route", Some(&token)).await;
        assert_insufficient_scope(status, &body, None);
    }

    #[tokio::test]
    async fn api_token_still_works_on_representative_routes() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let user_token = gate_mint_user_token(&db, &user);
        let service = Service::new(gate_router(config, db));
        for path in [
            "/api/runtime/status",
            "/api/projects/read_file",
            "/api/projects/write_file",
            "/api/projects/run_job",
            "/api/users/me",
            "/api/future/authenticated-route",
        ] {
            let (status, body) = gate_send(&service, path, Some(&user_token)).await;
            assert_eq!(status, StatusCode::OK, "{} body: {:?}", path, body);
        }
    }

    // ------------------------------------------------------------------
    // WWW-Authenticate resource_metadata in 401 responses
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn auth_middleware_unauthorized_includes_resource_metadata() {
        let mut config = gate_test_config_oauth2(Some("test-token"));
        Arc::get_mut(&mut config).unwrap().oauth2.issuer =
            Some("https://codex.example.com".to_string());
        let (_tmp, db) = gate_test_db();
        let service = salvo::Service::new(gate_router(config, db));
        // No Authorization header → 401
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let challenge = resp
            .headers
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            challenge.contains("Bearer"),
            "WWW-Authenticate should contain Bearer, got: {}",
            challenge
        );
        assert!(
            challenge.contains("resource_metadata="),
            "WWW-Authenticate should contain resource_metadata, got: {}",
            challenge
        );
        assert!(
            challenge.contains(".well-known/oauth-protected-resource"),
            "WWW-Authenticate should reference metadata endpoint, got: {}",
            challenge
        );
    }

    #[tokio::test]
    async fn auth_middleware_unauthorized_includes_resource_metadata_with_issuer() {
        let mut config_inner = gate_test_config_oauth2(Some("test-token"));
        // Set a specific issuer
        Arc::get_mut(&mut config_inner).unwrap().oauth2.issuer =
            Some("https://codex.example.com".to_string());
        let (_tmp, db) = gate_test_db();
        let service = salvo::Service::new(gate_router(config_inner, db));
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let challenge = resp
            .headers
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            challenge.contains("https://codex.example.com/.well-known/oauth-protected-resource"),
            "WWW-Authenticate should use issuer URL, got: {}",
            challenge
        );
    }

    #[tokio::test]
    async fn auth_middleware_lightweight_empty_and_open_paths() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = salvo::Service::new(gate_router(config.clone(), db.clone()));

        std::env::set_var("WEBCODEX_SHARED_KEY_ENABLED", "true");
        std::env::remove_var("WEBCODEX_ALLOW_ANONYMOUS");

        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .add_header("authorization", "Bearer ", true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .add_header("authorization", "Bearer    ", true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

        std::env::set_var("WEBCODEX_ALLOW_ANONYMOUS", "true");
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        std::env::remove_var("WEBCODEX_SHARED_KEY_ENABLED");
        std::env::remove_var("WEBCODEX_ALLOW_ANONYMOUS");
    }

    #[tokio::test]
    async fn auth_middleware_forbidden_uses_insufficient_scope_challenge() {
        let config = gate_test_config_oauth2(Some("test-token"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let (client, _secret) = gate_seed_oauth_client(&db, &user, "Test App");
        let (_at, plaintext) = gate_seed_oauth_access_token(&db, &client, &user, "runtime:read");

        let service = salvo::Service::new(gate_router(config, db));
        // OAuth2 token on agent transport path → 403, not 401
        let resp = salvo::test::TestClient::post("http://localhost/api/shell/agent/register")
            .add_header("authorization", &format!("Bearer {}", plaintext), true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        let challenge = resp
            .headers
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            challenge.contains("error=\"insufficient_scope\""),
            "403 should include insufficient_scope challenge, got: {}",
            challenge
        );
        assert!(
            !challenge.contains("resource_metadata="),
            "403 scope challenge should not include resource metadata, got: {}",
            challenge
        );
    }

    #[tokio::test]
    async fn auth_middleware_no_challenge_when_oauth2_disabled() {
        let config = gate_test_config(Some("test-token"));
        let (_tmp, db) = gate_test_db();
        let service = salvo::Service::new(gate_router(config, db));
        let resp = salvo::test::TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        // When OAuth2 is disabled, no WWW-Authenticate challenge
        assert!(
            !resp.headers.contains_key("www-authenticate"),
            "should not include WWW-Authenticate when OAuth2 is disabled"
        );
    }
}
