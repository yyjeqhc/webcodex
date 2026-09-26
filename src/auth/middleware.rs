//! HTTP request extraction, token surface gates, and Salvo auth middleware.

use std::sync::Arc;

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

const PROJECT_SHARE_MCP_QUERY_TOKEN_ENV: &str = "WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED";

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

#[cfg(test)]
mod query_token_tests {
    use super::allow_project_share_mcp_query_token;

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
/// True when `path` is one of the exact Runner transport endpoints an agent
/// token may call. Used by [`AuthMiddleware`] to gate agent tokens centrally.
pub(crate) fn is_runner_transport_path(path: &str) -> bool {
    crate::route_metadata::path_has_surface(
        path,
        crate::route_metadata::RouteSurface::RunnerTransport,
    )
}

pub(crate) fn is_account_control_path(path: &str) -> bool {
    crate::route_metadata::path_has_surface(
        path,
        crate::route_metadata::RouteSurface::AccountControl,
    )
}

/// Enforce that the token kind is permitted on the requested HTTP path.
///
/// Agent tokens are only allowed on Runner transport endpoints. Direct
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
    if ctx.is_agent_token() && !is_runner_transport_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "agent tokens are only allowed on Runner transport endpoints",
        ));
    }
    if ctx.is_account_credential() && !is_account_control_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "account credentials may only access account control endpoints",
        ));
    }
    // OAuth2 access tokens are not permitted on Runner transport endpoints,
    // including the shared-key OAuth bridge. Only a direct bearer shared key
    // may pair a lightweight Runner.
    if ctx.is_oauth_token() && is_runner_transport_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "OAuth2 tokens are not allowed on Runner transport endpoints",
        ));
    }
    if is_runner_transport_path(path)
        && !ctx.is_bootstrap()
        && !ctx.is_agent_token()
        && !ctx.is_shared_key()
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Runner transport endpoints require bootstrap, a bound Agent Token, or a direct shared key",
        ));
    }
    Ok(())
}

fn project_auth_state(depot: &Depot) -> Option<Arc<super::ProjectAuthState>> {
    depot.obtain::<Arc<super::ProjectAuthState>>().ok().cloned()
}

fn enforce_request_surface(
    ctx: &AuthContext,
    path: &str,
) -> Result<(), (StatusCode, &'static str)> {
    enforce_token_surface(ctx, path)
}

fn reject(res: &mut Response, ctrl: &mut FlowCtrl, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(serde_json::json!({"error": message})));
    ctrl.skip_rest();
}

fn reject_unauthorized(
    req: &Request,
    config: &Config,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
    message: &str,
) {
    if crate::public_http_security::render_invalid_auth_rate_limit_if_needed(req, res, ctrl) {
        return;
    }
    res.status_code(StatusCode::UNAUTHORIZED);
    if let Some(challenge) = oauth2_bearer_challenge(config) {
        if let Ok(value) = salvo::http::HeaderValue::from_str(&challenge) {
            res.headers_mut().insert("www-authenticate", value);
        }
    }
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
        let project_auth = project_auth_state(depot);
        let project_mode = project_auth
            .as_deref()
            .is_some_and(super::ProjectAuthState::is_configured);
        let project_share_query_token = project_share_mcp_query_token(req, project_mode);
        let project_share_query_token_used = project_share_query_token.is_some();
        let token = project_share_query_token.or_else(|| bearer_token(req));

        // When no token is present and auth is enabled, reject immediately
        // unless the server was explicitly started with `--open`
        // (WEBPI_ALLOW_ANONYMOUS=true), in which case the anonymous caller
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
                    if let Err((status, msg)) = enforce_request_surface(&ctx, req.uri().path()) {
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
                reject_unauthorized(req, &config, res, ctrl, "Unauthorized");
                return;
            }
        };

        // A project-scoped Server has exact protected credential verifiers in
        // ordinary auth state. Successful model credentials continue through the
        // same route-scope checks as every other ordinary Runtime principal.
        if let Some(project_auth) = project_auth.as_deref() {
            if let Some(ctx) = project_auth.authenticate_project_credential(&token) {
                if let Err((status, msg)) = enforce_request_surface(&ctx, req.uri().path()) {
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
            if project_share_query_token_used {
                // Query auth is a share-only transport convenience for the exact
                // temporary project credential. It must never fall through to an
                // Agent Token, PAT, OAuth token, or shared key.
                reject_unauthorized(
                    req,
                    &config,
                    res,
                    ctrl,
                    "invalid project share query credential",
                );
                return;
            }
            if let Some(ctx) = project_auth.authenticate_project_agent_token(&token) {
                if let Err((status, msg)) = enforce_request_surface(&ctx, req.uri().path()) {
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
        }

        // Pre-reject OAuth2 access tokens on Runner transport paths before
        // running the verifier chain. OAuth2Verifier updates last_used_at on
        // success, so we must not let it run on a surface that will
        // ultimately reject the token.
        if is_runner_transport_path(req.uri().path()) && is_oauth2_access_token(&token) {
            render_oauth_insufficient_scope(
                res,
                None,
                "OAuth2 access tokens cannot call Runner transport routes",
            );
            ctrl.skip_rest();
            return;
        }

        // Run the verifier chain (PatVerifier → OAuth2Verifier).
        match authenticate(&config, db.as_ref(), &token).await {
            Ok(Some(ctx)) => {
                // Enforce token-kind surface restrictions (agent tokens,
                // account credentials) before the handler runs.
                if let Err((status, msg)) = enforce_request_surface(&ctx, req.uri().path()) {
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
                    && !trimmed.is_empty()
                    && !is_managed_token_prefix(trimmed)
                {
                    let ctx = shared_key_context(trimmed);
                    if let Err((status, msg)) = enforce_request_surface(&ctx, req.uri().path()) {
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
                reject_unauthorized(req, &config, res, ctrl, "Unauthorized");
            }
            Err(e) => {
                // Token recognized but invalid (disabled user, expired token,
                // etc.). Map to the appropriate HTTP status without leaking
                // internal details.
                match e {
                    AuthError::InvalidToken => {
                        reject_unauthorized(req, &config, res, ctrl, "Unauthorized");
                    }
                }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpAuthority {
    host: String,
    port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpOrigin {
    scheme: String,
    authority: HttpAuthority,
}

fn parse_http_authority(value: &str) -> Option<HttpAuthority> {
    if value.is_empty() || value.trim() != value {
        return None;
    }
    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        let close = rest.find(']')?;
        let host = &rest[..close];
        let suffix = &rest[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':')?.parse::<u16>().ok()?)
        };
        (host, port)
    } else {
        if value.matches(':').count() > 1 {
            return None;
        }
        match value.rsplit_once(':') {
            Some((host, port)) => {
                if host.is_empty() || port.is_empty() {
                    return None;
                }
                (host, Some(port.parse::<u16>().ok()?))
            }
            None => (value, None),
        }
    };
    if host.is_empty() {
        return None;
    }
    let host = if let Ok(address) = host.parse::<std::net::IpAddr>() {
        address.to_string()
    } else {
        match url::Host::parse(host).ok()? {
            url::Host::Domain(domain) => {
                let domain = domain.trim_end_matches('.').to_ascii_lowercase();
                if domain.is_empty() {
                    return None;
                }
                domain
            }
            url::Host::Ipv4(address) => address.to_string(),
            url::Host::Ipv6(address) => address.to_string(),
        }
    };
    Some(HttpAuthority { host, port })
}

fn parse_http_origin(value: &str) -> Option<HttpOrigin> {
    let parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let host = parsed.host_str()?;
    let authority = parse_http_authority(&match parsed.port() {
        Some(port) if host.contains(':') => format!("[{host}]:{port}"),
        Some(port) => format!("{host}:{port}"),
        None if host.contains(':') => format!("[{host}]"),
        None => host.to_string(),
    })?;
    Some(HttpOrigin {
        scheme: parsed.scheme().to_string(),
        authority,
    })
}

fn default_http_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

fn origin_effective_port(origin: &HttpOrigin) -> Option<u16> {
    origin
        .authority
        .port
        .or_else(|| default_http_port(&origin.scheme))
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_unspecified_host(host: &str) -> bool {
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_unspecified())
}

fn configured_public_origin() -> Option<HttpOrigin> {
    let value = std::env::var("WEBPI_PUBLIC_URL").ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    parse_http_origin(value)
}

fn authority_matches_configured_origin(authority: &HttpAuthority, origin: &HttpOrigin) -> bool {
    if authority.host != origin.authority.host {
        return false;
    }
    match origin.authority.port {
        Some(expected_port) => authority.port == Some(expected_port),
        None => authority
            .port
            .is_none_or(|port| default_http_port(&origin.scheme) == Some(port)),
    }
}

fn request_authority_allowed(
    authority: &HttpAuthority,
    config: &Config,
    public_origin: Option<&HttpOrigin>,
) -> bool {
    if is_loopback_host(&authority.host) {
        return true;
    }
    if public_origin.is_some_and(|origin| authority_matches_configured_origin(authority, origin)) {
        return true;
    }
    parse_http_authority(&config.addr).is_some_and(|bound| {
        !is_unspecified_host(&bound.host)
            && bound.host == authority.host
            && authority
                .port
                .is_none_or(|port| bound.port.is_some_and(|bound_port| bound_port == port))
    })
}

fn origin_matches_request_authority(origin: &HttpOrigin, authority: &HttpAuthority) -> bool {
    if origin.authority.host != authority.host {
        return false;
    }
    match authority.port {
        Some(port) => origin_effective_port(origin) == Some(port),
        None => origin.authority.port.is_none(),
    }
}

pub(crate) fn require_mcp_request_authority(
    req: &Request,
    config: &Config,
) -> Result<(), (u16, &'static str, &'static str)> {
    let host = match req.headers().get("host") {
        Some(value) => value.to_str().ok(),
        None => req.uri().authority().map(|authority| authority.as_str()),
    }
    .and_then(parse_http_authority)
    .ok_or((400, "invalid_request_authority", "invalid Host header"))?;
    let public_origin = configured_public_origin();
    if !request_authority_allowed(&host, config, public_origin.as_ref()) {
        return Err((
            403,
            "untrusted_request_authority",
            "request Host is not an allowed WebPi authority",
        ));
    }

    let Some(raw_origin) = req.headers().get("origin") else {
        return Ok(());
    };
    let raw_origin = raw_origin
        .to_str()
        .map_err(|_| (400, "invalid_origin", "invalid Origin header"))?;
    let origin =
        parse_http_origin(raw_origin).ok_or((400, "invalid_origin", "invalid Origin header"))?;
    if !origin_matches_request_authority(&origin, &host) {
        return Err((
            403,
            "cross_origin_denied",
            "cross-origin requests are not allowed",
        ));
    }
    if public_origin
        .as_ref()
        .is_some_and(|public| authority_matches_configured_origin(&host, public))
        && public_origin.as_ref().is_some_and(|public| {
            origin.scheme != public.scheme
                || origin_effective_port(&origin) != origin_effective_port(public)
        })
    {
        return Err((
            403,
            "cross_origin_denied",
            "cross-origin requests are not allowed",
        ));
    }
    Ok(())
}

pub(crate) fn require_mcp_json_request(
    req: &Request,
    config: &Config,
) -> Result<(), (u16, &'static str, &'static str)> {
    require_mcp_request_authority(req, config)?;
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
