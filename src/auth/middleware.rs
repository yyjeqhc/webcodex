//! HTTP request extraction, token surface gates, and Salvo auth middleware.

use std::sync::Arc;

use crate::{Config, Database};
use salvo::prelude::*;

use super::principal::{AuthContext, AuthError};
use super::shared_key::{
    allow_anonymous_enabled, is_managed_token_prefix, open_anonymous_context, shared_key_context,
    shared_key_enabled,
};
use super::tokens::{authenticate, is_oauth2_access_token};
use super::{bootstrap_context, scopes};

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

/// A `webcodex connect` process is a capability grant for one project, not a
/// general runtime admin endpoint. Non-bootstrap user-facing credentials may
/// therefore reach only the canonical connector API and MCP. Bootstrap stays
/// available for local setup; agent tokens stay available for their already
/// exact transport routes.
pub(crate) fn enforce_project_connector_surface(
    enabled: bool,
    ctx: &AuthContext,
    path: &str,
) -> Result<(), (StatusCode, &'static str)> {
    if !enabled || ctx.is_bootstrap() || ctx.is_agent_token() {
        return Ok(());
    }
    if path == "/mcp" || path.starts_with("/api/connector/") {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "project connector credentials may only access canonical connector capabilities",
    ))
}

fn project_connector_enabled(depot: &Depot) -> bool {
    depot
        .obtain::<crate::connector_runtime::ConnectorRuntimeSlot>()
        .ok()
        .is_some_and(|slot| slot.0.is_some())
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
                    if let Err((status, msg)) = enforce_project_connector_surface(
                        project_connector_enabled(depot),
                        &ctx,
                        req.uri().path(),
                    ) {
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
                if let Err((status, msg)) = enforce_project_connector_surface(
                    project_connector_enabled(depot),
                    &ctx,
                    req.uri().path(),
                ) {
                    res.status_code(status);
                    res.render(Json(serde_json::json!({"error": msg})));
                    ctrl.skip_rest();
                    return;
                }
                if let Err((scope, description)) =
                    scopes::enforce_oauth_route_scope(&ctx, req.method().as_str(), req.uri().path())
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
                    if let Err((status, msg)) = enforce_project_connector_surface(
                        project_connector_enabled(depot),
                        &ctx,
                        req.uri().path(),
                    ) {
                        res.status_code(status);
                        res.render(Json(serde_json::json!({"error": msg})));
                        ctrl.skip_rest();
                        return;
                    }
                    if let Err((scope, description)) = scopes::enforce_oauth_route_scope(
                        &ctx,
                        req.method().as_str(),
                        req.uri().path(),
                    ) {
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

#[cfg(test)]
mod connector_surface_tests {
    use super::*;
    use crate::auth::{AuthKind, SCOPE_PROJECT_READ};

    fn user_context() -> AuthContext {
        AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some("u1".to_string()),
            username: Some("owner".to_string()),
            api_key_id: Some("key".to_string()),
            api_key_name: Some("connector".to_string()),
            role: Some("user".to_string()),
            scopes: vec![SCOPE_PROJECT_READ.to_string()],
            is_bootstrap: false,
            token_kind: Some("user".to_string()),
            allowed_client_id: None,
            shared_key_hash: None,
        }
    }

    #[test]
    fn project_connector_hard_gates_legacy_user_routes() {
        let user = user_context();
        assert!(
            enforce_project_connector_surface(true, &user, "/api/connector/files/read").is_ok()
        );
        assert!(enforce_project_connector_surface(true, &user, "/mcp").is_ok());
        assert!(enforce_project_connector_surface(true, &user, "/api/tools/call").is_err());
        assert!(enforce_project_connector_surface(true, &user, "/api/projects/list").is_err());
        assert!(enforce_project_connector_surface(false, &user, "/api/tools/call").is_ok());

        let bootstrap = bootstrap_context();
        assert!(enforce_project_connector_surface(true, &bootstrap, "/api/projects/list").is_ok());
    }
}
