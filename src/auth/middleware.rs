//! HTTP request extraction, token surface gates, and Salvo auth middleware.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{Config, Database};
use salvo::prelude::*;

use super::context::{AuthContext, AuthError};
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

// v0.1.0 publicly documented `/api/agents/ws?token=...` as handshake
// compatibility even though the first-party Runner already used Authorization.
// Keep that exact legacy surface until the project explicitly retires the
// v0.1.0-era transport support window; do not add new callers or endpoints.
static WS_QUERY_TOKEN_DEPRECATION_WARNED: AtomicBool = AtomicBool::new(false);

pub(crate) fn allow_query_token_for_path(path: &str) -> bool {
    path == "/api/agents/ws"
}

const PROJECT_SHARE_MCP_QUERY_TOKEN_ENV: &str = "WEBCODEX_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED";

fn allow_project_share_mcp_query_token(path: &str, project_mode: bool, enabled: bool) -> bool {
    project_mode && enabled && path == "/mcp"
}

fn project_share_mcp_query_token(req: &Request, project_mode: bool) -> Option<String> {
    // The query-token convenience is intentionally narrower than generic auth:
    // only an explicitly opted-in project-share Server may accept it, and an
    // Authorization header always remains authoritative when present.
    if req.headers().contains_key("authorization")
        || !allow_project_share_mcp_query_token(
            req.uri().path(),
            project_mode,
            crate::config::env_flag(PROJECT_SHARE_MCP_QUERY_TOKEN_ENV).unwrap_or(false),
        )
    {
        return None;
    }
    req.query::<String>("token")
}

fn claim_ws_query_token_deprecation_warning(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::Relaxed)
}

fn warn_deprecated_ws_query_token_once() {
    if claim_ws_query_token_deprecation_warning(&WS_QUERY_TOKEN_DEPRECATION_WARNED) {
        tracing::warn!(
            transport = "websocket",
            reason_code = "deprecated_query_token_auth",
            "deprecated WebSocket query-token authentication used; use Authorization: Bearer"
        );
    }
}

#[cfg(test)]
mod query_token_deprecation_tests {
    use super::{
        allow_project_share_mcp_query_token, claim_ws_query_token_deprecation_warning, AtomicBool,
    };

    #[test]
    fn query_token_deprecation_warning_claim_is_process_bounded() {
        let flag = AtomicBool::new(false);
        assert!(claim_ws_query_token_deprecation_warning(&flag));
        assert!(!claim_ws_query_token_deprecation_warning(&flag));
    }

    #[test]
    fn project_share_query_token_is_exact_opt_in_mcp_only() {
        assert!(allow_project_share_mcp_query_token("/mcp", true, true));
        assert!(!allow_project_share_mcp_query_token("/mcp", false, true));
        assert!(!allow_project_share_mcp_query_token("/mcp", true, false));
        assert!(!allow_project_share_mcp_query_token(
            "/mcp/extra",
            true,
            true
        ));
        assert!(!allow_project_share_mcp_query_token(
            "/api/agents/ws",
            true,
            true
        ));
    }
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

pub(crate) fn scope_forbidden_body(
    auth: Option<&AuthContext>,
    description: impl Into<String>,
) -> serde_json::Value {
    let description = description.into();
    if auth.is_some_and(AuthContext::is_oauth_token) {
        oauth_insufficient_scope_body(description)
    } else {
        serde_json::json!({
            "status": StatusCode::FORBIDDEN.as_u16(),
            "error": description,
        })
    }
}

pub(crate) fn render_scope_forbidden(
    res: &mut Response,
    auth: Option<&AuthContext>,
    required_scope: Option<&str>,
    description: impl Into<String>,
) {
    let description = description.into();
    if auth.is_some_and(AuthContext::is_oauth_token) {
        render_oauth_insufficient_scope(res, required_scope, description);
        return;
    }
    res.status_code(StatusCode::FORBIDDEN);
    res.render(Json(scope_forbidden_body(auth, description)));
}

pub(crate) fn bearer_or_allowed_query_token(req: &Request) -> Option<String> {
    // Header authority always wins. A query credential must never rescue an
    // invalid or malformed Authorization value.
    if req.headers().contains_key("authorization") {
        return bearer_token(req);
    }
    if !allow_query_token_for_path(req.uri().path()) {
        return None;
    }
    let token = req.query::<String>("token");
    if token.is_some() {
        warn_deprecated_ws_query_token_once();
    }
    token
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
    "/api/shell/agent/persistent_shell_result",
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
/// Agent tokens are only allowed on agent transport endpoints. Direct
/// shared-key principals may also use those endpoints when shared-key auth
/// produced their context. Account credentials are only allowed on account
/// control endpoints. Other token kinds retain their normal surfaces.
///
/// Returns `Ok(())` when the token is permitted, `Err((status, message))`
/// when it should be rejected.
pub(crate) fn enforce_token_surface(
    ctx: &AuthContext,
    path: &str,
) -> Result<(), (StatusCode, &'static str)> {
    // Lightweight principals, project credentials, and project/lightweight
    // OAuth subjects must never reach account-control management surfaces.
    if (ctx.is_lightweight()
        || ctx.is_project_credential()
        || ctx.is_oauth_shared_key_subject()
        || ctx.is_oauth_project_subject())
        && is_account_control_path(path)
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
    // OAuth2 access tokens are not permitted on agent transport endpoints,
    // including the shared-key OAuth bridge. Only a direct bearer shared key
    // may pair a lightweight Runner.
    if ctx.is_oauth_token() && is_agent_transport_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "OAuth2 tokens are not allowed on agent transport endpoints",
        ));
    }
    if is_agent_transport_path(path)
        && !ctx.is_bootstrap()
        && !ctx.is_agent_token()
        && !ctx.is_shared_key()
    {
        return Err((
            StatusCode::FORBIDDEN,
            "agent transport endpoints require bootstrap, a bound Agent Token, or a direct shared key",
        ));
    }
    Ok(())
}

/// A project-bound runtime is a capability grant for one configured project,
/// not a general runtime admin endpoint. Non-bootstrap user-facing credentials may
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
    if (ctx.is_project_credential() || ctx.is_oauth_project_subject())
        && (path == "/mcp" || is_project_connector_path(path) || is_project_console_path(path))
    {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "project connector credentials may only access canonical connector capabilities",
    ))
}

fn is_project_console_path(path: &str) -> bool {
    crate::host_console_http::CONSOLE_ROUTES.contains(&path)
}

fn is_project_connector_path(path: &str) -> bool {
    path == "/api/connector/readiness"
        || crate::connector_runtime::surface::CAPABILITY_NAMES
            .iter()
            .any(|name| crate::connector_runtime::surface::route_for(name) == Some(path))
}

fn project_connector_runtime(
    depot: &Depot,
) -> Option<Arc<crate::connector_runtime::ConnectorRuntime>> {
    depot
        .obtain::<crate::connector_runtime::ConnectorRuntimeSlot>()
        .ok()
        .and_then(|slot| slot.0.clone())
}

fn project_connector_enabled(depot: &Depot) -> bool {
    project_connector_runtime(depot).is_some()
}

fn enforce_request_surface(
    project_mode: bool,
    ctx: &AuthContext,
    path: &str,
) -> Result<(), (StatusCode, &'static str)> {
    enforce_token_surface(ctx, path)?;
    enforce_project_connector_surface(project_mode, ctx, path)
}

fn reject(res: &mut Response, ctrl: &mut FlowCtrl, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(serde_json::json!({"error": message})));
    ctrl.skip_rest();
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
        let project_mode = project_connector_enabled(depot);
        let project_share_query_token = project_share_mcp_query_token(req, project_mode);
        let project_share_query_token_used = project_share_query_token.is_some();
        let token = project_share_query_token.or_else(|| bearer_or_allowed_query_token(req));

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
                    // context. Surface restrictions and declared scopes still apply.
                    let ctx = open_anonymous_context();
                    if let Err((status, msg)) = enforce_request_surface(
                        project_connector_enabled(depot),
                        &ctx,
                        req.uri().path(),
                    ) {
                        reject(res, ctrl, status, msg);
                        return;
                    }
                    if let Err((scope, description)) =
                        scopes::enforce_route_scope(&ctx, req.method().as_str(), req.uri().path())
                    {
                        render_scope_forbidden(res, Some(&ctx), scope, description);
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

        // Project mode has one exact credential verifier loaded from its
        // protected setup state. This path is separate from the ordinary
        // shared-key quick-start fallback below.
        if let Some(runtime) = project_connector_runtime(depot) {
            if let Some(ctx) = runtime.authenticate_project_credential(&token) {
                if let Err((status, msg)) = enforce_request_surface(true, &ctx, req.uri().path()) {
                    reject(res, ctrl, status, msg);
                    return;
                }
                // Project credentials are a specialized Connector capability.
                // Their exact surface and operation authorization stay owned by
                // the project Connector instead of the ordinary route registry.
                depot.inject(ctx);
                ctrl.call_next(req, depot, res).await;
                return;
            }
            if project_share_query_token_used {
                // Query auth is a share-only transport convenience for the
                // exact temporary Connector credential. It must never fall
                // through to project Agent tokens, PATs, OAuth, or shared keys.
                reject(
                    res,
                    ctrl,
                    StatusCode::UNAUTHORIZED,
                    "invalid project share query credential",
                );
                return;
            }
            if let Some(ctx) = runtime.authenticate_project_agent_token(&token) {
                if let Err((status, msg)) = enforce_request_surface(true, &ctx, req.uri().path()) {
                    reject(res, ctrl, status, msg);
                    return;
                }
                // Project Agent Tokens remain governed by the exact Agent
                // transport surface and its agent:* scope checks.
                depot.inject(ctx);
                ctrl.call_next(req, depot, res).await;
                return;
            }
        }

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
                if let Err((status, msg)) = enforce_request_surface(
                    project_connector_enabled(depot),
                    &ctx,
                    req.uri().path(),
                ) {
                    reject(res, ctrl, status, msg);
                    return;
                }
                if let Err((scope, description)) =
                    scopes::enforce_route_scope(&ctx, req.method().as_str(), req.uri().path())
                {
                    render_scope_forbidden(res, Some(&ctx), scope, description);
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
                    && !project_connector_enabled(depot)
                    && !trimmed.is_empty()
                    && !is_managed_token_prefix(trimmed)
                {
                    let ctx = shared_key_context(trimmed);
                    if let Err((status, msg)) = enforce_request_surface(
                        project_connector_enabled(depot),
                        &ctx,
                        req.uri().path(),
                    ) {
                        reject(res, ctrl, status, msg);
                        return;
                    }
                    if let Err((scope, description)) =
                        scopes::enforce_route_scope(&ctx, req.method().as_str(), req.uri().path())
                    {
                        render_scope_forbidden(res, Some(&ctx), scope, description);
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
                    AuthError::InvalidToken => StatusCode::UNAUTHORIZED,
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

pub(crate) fn require_same_origin(req: &Request) -> Result<(), (u16, &'static str, &'static str)> {
    if let Some(origin) = req
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok())
    {
        let host = req
            .headers()
            .get("host")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if origin.rsplit_once("://").map(|(_, value)| value) != Some(host) {
            return Err((
                403,
                "cross_origin_denied",
                "cross-origin requests are not allowed",
            ));
        }
    }
    Ok(())
}

pub(crate) fn require_json_same_origin(
    req: &Request,
) -> Result<(), (u16, &'static str, &'static str)> {
    require_same_origin(req)?;
    if req
        .content_type()
        .is_none_or(|content_type| content_type.essence_str() != "application/json")
    {
        return Err((
            415,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod connector_surface_tests {
    use super::*;
    use crate::auth::{AuthKind, SCOPE_PROJECT_READ};

    fn project_context() -> AuthContext {
        AuthContext {
            role: Some("project".to_string()),
            scopes: vec![SCOPE_PROJECT_READ.to_string()],
            token_kind: Some("project".to_string()),
            project_grant_id: Some("wc_pgrant_1111111111111111".to_string()),
            ..AuthContext::new(AuthKind::ProjectCredential)
        }
    }

    #[test]
    fn project_connector_hard_gates_legacy_user_routes() {
        let user = project_context();
        assert!(
            enforce_project_connector_surface(true, &user, "/api/connector/files/read").is_ok()
        );
        assert!(enforce_project_connector_surface(true, &user, "/mcp").is_ok());
        assert!(
            enforce_project_connector_surface(true, &user, "/api/connector/not-a-capability")
                .is_err()
        );
        assert!(enforce_project_connector_surface(true, &user, "/api/tools/call").is_err());
        assert!(enforce_project_connector_surface(true, &user, "/api/projects/list").is_err());
        assert!(
            enforce_project_connector_surface(true, &user, "/api/runtime-console/projects")
                .is_err()
        );
        let agent = AuthContext::new(AuthKind::AgentToken);
        assert!(enforce_token_surface(&agent, "/api/runtime-console/projects").is_err());
        assert!(enforce_project_connector_surface(false, &user, "/api/tools/call").is_ok());

        let bootstrap = bootstrap_context();
        assert!(enforce_project_connector_surface(true, &bootstrap, "/api/projects/list").is_ok());
    }
}
