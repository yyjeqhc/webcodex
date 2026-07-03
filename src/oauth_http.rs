//! OAuth2 token, revocation, and discovery endpoints.
//!
//! - `POST /oauth/token` — token endpoint (authorization_code, refresh_token)
//! - `POST /oauth/revoke` — token revocation endpoint (RFC 7009)
//! - `GET /.well-known/oauth-protected-resource` — protected resource metadata
//! - `GET /.well-known/oauth-authorization-server` — authorization server metadata
//!
//! Token and revocation are **public** endpoints (no `AuthMiddleware`); clients
//! authenticate via `client_id` + `client_secret` in the form body. The
//! metadata endpoint is also public and requires no authentication.
//!
//! Security properties:
//! - Authorization codes are consumed atomically (single-use).
//! - Code consumption and token insertion happen in a single DB transaction
//!   **only** when all validations pass.
//! - Refresh tokens are rotated: the old token is revoked and a new
//!   access+refresh token pair is issued in a single transaction.
//! - Revocation is idempotent: unknown, already-revoked, and other-client
//!   tokens all return HTTP 200 without disclosing token state.
//! - Client secret is verified with constant-time comparison.
//! - Only `application/x-www-form-urlencoded` content type is accepted.
//! - Request body size is bounded (16 KiB).
//! - All responses include `Cache-Control: no-store` and `Pragma: no-cache`.
//! - Plaintext tokens are returned **only once** in the response.
//! - Only SHA-256 hashes are stored in the database.

use crate::auth::{
    generate_oauth_authorization_code, hash_token, scopes, shared_key_hash_of, AuthContext,
    AuthKind,
};
use crate::models::OAuthAuthorizationCodeRecord;
use salvo::http::header::CONTENT_TYPE;
use salvo::http::HeaderValue;
use salvo::prelude::*;

mod clients;
mod html;
mod metadata;
mod token;

pub(crate) use clients::{oauth_clients_create, oauth_clients_list, oauth_clients_revoke};
use html::{authorize_bridge_html, authorize_consent_html, authorize_login_html};
pub(crate) use metadata::{oauth_authorization_server_metadata, oauth_metadata};
pub(crate) use token::oauth_token;
#[cfg(test)]
pub(crate) use token::verify_pkce_s256;

/// Maximum request body size for the token endpoint (16 KiB).
const MAX_OAUTH_TOKEN_FORM_BYTES: usize = 16 * 1024;

// ---------------------------------------------------------------------------
// OAuth2 response helpers
// ---------------------------------------------------------------------------

/// Apply cache-prevention headers to an OAuth2 response (RFC 6749 §5.1, §5.2).
///
/// All OAuth2 JSON responses — both success and error — must include these
/// headers to prevent intermediaries from caching sensitive tokens or error
/// context.
fn apply_oauth_no_store_headers(res: &mut Response) {
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    res.headers_mut()
        .insert("pragma", HeaderValue::from_static("no-cache"));
}

/// Render an OAuth2 error response (RFC 6749 §5.2) with no-store headers.
fn oauth_error(res: &mut Response, status: StatusCode, error: &str, description: &str) {
    res.status_code(status);
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "error": error,
        "error_description": description,
    })));
}

fn oauth_authorize_direct_error(
    res: &mut Response,
    status: StatusCode,
    error: &str,
    description: &str,
) {
    res.status_code(status);
    res.render(Json(serde_json::json!({
        "error": error,
        "error_description": description,
    })));
}

fn redirect_with_oauth_error(
    res: &mut Response,
    redirect_uri: &str,
    error: &str,
    state: Option<&str>,
) {
    let location = match append_authorize_error_params(redirect_uri, error, state) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    let location = match HeaderValue::from_str(&location) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    res.status_code(StatusCode::FOUND);
    res.headers_mut().insert("location", location);
}

fn append_authorize_error_params(
    redirect_uri: &str,
    error: &str,
    state: Option<&str>,
) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("error", error);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
    }
    Ok(url.into())
}

fn redirect_with_authorization_code(
    res: &mut Response,
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
) {
    let location = match append_authorize_success_params(redirect_uri, code, state) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    let location = match HeaderValue::from_str(&location) {
        Ok(location) => location,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid redirect_uri",
            );
            return;
        }
    };

    res.status_code(StatusCode::FOUND);
    res.headers_mut().insert("location", location);
}

fn append_authorize_success_params(
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("code", code);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
    }
    Ok(url.into())
}

// ---------------------------------------------------------------------------
// Authorization endpoint request helpers
// ---------------------------------------------------------------------------

/// Parsed query shape for the future `GET /oauth/authorize` endpoint.
///
/// This is intentionally a pure internal data type for now. Phase 2e-1a does
/// not mount an authorize route or issue authorization codes.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OAuthAuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
}

/// Internal authorization endpoint validation errors.
///
/// `InvalidRequest` is for direct errors before the client/redirect trust
/// boundary is established. Redirectable variants are for errors that can be
/// mapped to OAuth redirect errors after the client and redirect URI are
/// trusted.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OAuthAuthorizeError {
    InvalidRequest(&'static str),
    UnauthorizedClient(&'static str),
    UnsupportedResponseType,
    InvalidScope(&'static str),
    InvalidRequestRedirectable(&'static str),
    UnsupportedResource,
}

/// Parse a future `/oauth/authorize` query string without performing runtime
/// validation, DB lookups, redirects, or authorization code issuance.
///
/// Duplicate known parameters are rejected because they make the OAuth request
/// ambiguous. Unknown parameters are ignored for forward compatibility.
#[allow(dead_code)]
pub(crate) fn parse_authorize_query(
    query: &str,
) -> Result<OAuthAuthorizeRequest, OAuthAuthorizeError> {
    let mut response_type = None;
    let mut client_id = None;
    let mut redirect_uri = None;
    let mut scope = None;
    let mut state = None;
    let mut code_challenge = None;
    let mut code_challenge_method = None;
    let mut resource = None;

    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "response_type" => &mut response_type,
            "client_id" => &mut client_id,
            "redirect_uri" => &mut redirect_uri,
            "scope" => &mut scope,
            "state" => &mut state,
            "code_challenge" => &mut code_challenge,
            "code_challenge_method" => &mut code_challenge_method,
            "resource" => &mut resource,
            _ => continue,
        };

        if slot.replace(value.into_owned()).is_some() {
            return Err(OAuthAuthorizeError::InvalidRequest("duplicate parameter"));
        }
    }

    Ok(OAuthAuthorizeRequest {
        response_type: response_type
            .ok_or(OAuthAuthorizeError::InvalidRequest("missing response_type"))?,
        client_id: client_id.ok_or(OAuthAuthorizeError::InvalidRequest("missing client_id"))?,
        redirect_uri: redirect_uri
            .ok_or(OAuthAuthorizeError::InvalidRequest("missing redirect_uri"))?,
        scope,
        state,
        code_challenge: code_challenge.ok_or(OAuthAuthorizeError::InvalidRequest(
            "missing code_challenge",
        ))?,
        code_challenge_method: code_challenge_method.ok_or(OAuthAuthorizeError::InvalidRequest(
            "missing code_challenge_method",
        ))?,
        resource,
    })
}

fn decoded_authorize_param(query: &str, name: &str) -> Result<Option<String>, OAuthAuthorizeError> {
    let mut value = None;
    for (key, raw_value) in url::form_urlencoded::parse(query.as_bytes()) {
        if key.as_ref() != name {
            continue;
        }
        if value.replace(raw_value.into_owned()).is_some() {
            return Err(OAuthAuthorizeError::InvalidRequest("duplicate parameter"));
        }
    }
    Ok(value)
}

fn is_redirectable_missing_authorize_param(error: &OAuthAuthorizeError) -> bool {
    matches!(
        error,
        OAuthAuthorizeError::InvalidRequest("missing response_type")
            | OAuthAuthorizeError::InvalidRequest("missing code_challenge")
            | OAuthAuthorizeError::InvalidRequest("missing code_challenge_method")
    )
}

fn redirect_error_for_missing_authorize_param(error: &OAuthAuthorizeError) -> &'static str {
    match error {
        OAuthAuthorizeError::InvalidRequest("missing response_type") => "invalid_request",
        OAuthAuthorizeError::InvalidRequest("missing code_challenge") => "invalid_request",
        OAuthAuthorizeError::InvalidRequest("missing code_challenge_method") => "invalid_request",
        _ => "invalid_request",
    }
}

fn normalize_oauth_resource_indicator(resource: &str) -> Result<String, OAuthAuthorizeError> {
    let resource = resource.trim();
    if resource.is_empty() {
        return Err(OAuthAuthorizeError::UnsupportedResource);
    }

    let parsed = url::Url::parse(resource).map_err(|_| OAuthAuthorizeError::UnsupportedResource)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(OAuthAuthorizeError::UnsupportedResource);
    }
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(OAuthAuthorizeError::UnsupportedResource);
    }

    let mut normalized = format!(
        "{}://{}",
        parsed.scheme(),
        parsed
            .host_str()
            .ok_or(OAuthAuthorizeError::UnsupportedResource)?
    );
    if let Some(port) = parsed.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }

    let mut path = parsed.path().to_string();
    if path == "/" {
        path.clear();
    } else {
        while path.ends_with('/') {
            path.pop();
        }
    }
    normalized.push_str(&path);
    Ok(normalized)
}

fn allowed_oauth_resource_indicators(config: &crate::Config) -> Vec<String> {
    let Some(base) = config.oauth2.issuer.as_deref() else {
        return Vec::new();
    };
    let Ok(base) = normalize_oauth_resource_indicator(base) else {
        return Vec::new();
    };

    let mcp = format!("{}/mcp", base);
    vec![base, mcp]
}

fn validate_authorize_resource(
    resource: Option<&str>,
    config: &crate::Config,
) -> Result<Option<String>, OAuthAuthorizeError> {
    let Some(resource) = resource else {
        return Ok(None);
    };
    let normalized = normalize_oauth_resource_indicator(resource)?;
    if allowed_oauth_resource_indicators(config)
        .iter()
        .any(|allowed| allowed == &normalized)
    {
        Ok(Some(normalized))
    } else {
        Err(OAuthAuthorizeError::UnsupportedResource)
    }
}

// ---------------------------------------------------------------------------
// First-party authorize browser session (Phase 2e-3)
// ---------------------------------------------------------------------------

/// Cookie name carrying the opaque authorize session id.
const AUTHORIZE_SESSION_COOKIE: &str = "webcodex_authorize_session";

/// Authorize session lifetime in seconds (10 minutes). Short on purpose:
/// the session only bridges the login form to the consent decision.
const AUTHORIZE_SESSION_TTL_SECS: i64 = 600;

/// In-memory first-party authorize session store. Holds short-lived sessions
/// created when a user submits a PAT at the authorize login page. The session
/// cookie carries an opaque random id; only its SHA-256 hash is used as the
/// map key so the plaintext id is never stored.
#[derive(Default)]
pub(crate) struct AuthorizeSessionStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, AuthorizeSession>>,
}

#[derive(Clone)]
#[allow(dead_code)] // fields retained for session audit/future consent display
struct AuthorizeSession {
    user_id: String,
    username: Option<String>,
    /// `AuthKind` of the credential used to log in (Bootstrap or ApiToken).
    auth_kind: AuthKind,
    created_at: i64,
    expires_at: i64,
}

impl AuthorizeSessionStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Create a new session and return the opaque plaintext session id. Only
    /// the SHA-256 hash of the id is stored in the map. PAT/bootstrap
    /// plaintext is never stored — only the resolved user identity.
    fn create_session(
        &self,
        user_id: String,
        username: Option<String>,
        auth_kind: AuthKind,
    ) -> String {
        let now = chrono::Utc::now().timestamp();
        let session = AuthorizeSession {
            user_id,
            username,
            auth_kind,
            created_at: now,
            expires_at: now + AUTHORIZE_SESSION_TTL_SECS,
        };
        let id = generate_authorize_session_id();
        let hash = hash_token(&id);
        let mut guard = self.inner.lock().unwrap();
        // Opportunistic cleanup of expired sessions to bound growth.
        guard.retain(|_, s| s.expires_at > now);
        guard.insert(hash, session);
        id
    }

    /// Look up a session by its opaque plaintext id. Returns `None` when the
    /// session does not exist or has expired. Expired sessions are removed.
    fn get_session(&self, id: &str) -> Option<AuthorizeSession> {
        if id.is_empty() {
            return None;
        }
        let hash = hash_token(id);
        let now = chrono::Utc::now().timestamp();
        let mut guard = self.inner.lock().unwrap();
        let session = guard.get(&hash).cloned();
        match session {
            Some(s) if s.expires_at > now => Some(s),
            Some(_) => {
                guard.remove(&hash);
                None
            }
            None => None,
        }
    }
}

fn generate_authorize_session_id() -> String {
    let mut random = String::with_capacity(64);
    while random.len() < 64 {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(64);
    format!("wc_authsess_{}", random)
}

/// Build a `Set-Cookie` header value for the authorize session id.
fn authorize_session_cookie_header(id: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{}={}; Max-Age={}; Path=/; HttpOnly; SameSite=Lax",
        AUTHORIZE_SESSION_COOKIE, id, AUTHORIZE_SESSION_TTL_SECS
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Build a `Set-Cookie` header that clears the authorize session cookie.
fn authorize_session_clear_cookie_header(secure: bool) -> String {
    let mut cookie = format!(
        "{}=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax",
        AUTHORIZE_SESSION_COOKIE
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Return the opaque session id from the request's Cookie header, if present.
fn authorize_session_id_from_request(req: &Request) -> Option<String> {
    let header = req.headers().get("cookie")?.to_str().ok()?;
    let prefix = format!("{}=", AUTHORIZE_SESSION_COOKIE);
    for pair in header.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(&prefix) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// True when the configured issuer/public URL is an https URL, so the session
/// cookie should carry the `Secure` attribute.
fn is_secure_authorize(config: &crate::Config) -> bool {
    config
        .oauth2
        .issuer
        .as_deref()
        .map(|s| s.trim_start().to_ascii_lowercase().starts_with("https://"))
        .unwrap_or(false)
}

fn is_managed_credential_like(value: &str) -> bool {
    value.starts_with("wc_")
}

fn bridge_shared_key_hash(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("a shared key is required");
    }
    if is_managed_credential_like(value) {
        return Err("managed credentials cannot be used as shared keys");
    }

    Ok(shared_key_hash_of(value))
}

/// Validate a `return_to` value submitted from the login form. It must be a
/// same-origin relative path to prevent open redirect. Accepted form:
/// starts with a single `/`, is not `//...` or `/\...`, and must point at the
/// authorize endpoint.
fn validate_authorize_return_to(return_to: &str) -> Result<(), ()> {
    if !return_to.starts_with('/') {
        return Err(());
    }
    // Reject scheme-relative (`//host`) and backslash tricks.
    if return_to.starts_with("//") || return_to.starts_with("/\\") {
        return Err(());
    }
    let path = return_to.split('?').next().unwrap_or("");
    // Normalize trailing slash for the comparison.
    let path = path.trim_end_matches('/');
    if path != "/oauth/authorize" {
        return Err(());
    }
    Ok(())
}

/// Parse an `application/x-www-form-urlencoded` body into owned pairs.
async fn parse_form_body(req: &mut Request) -> Option<Vec<(String, String)>> {
    let body = req.payload().await.ok()?;
    if body.len() > 16 * 1024 {
        return None;
    }
    Some(
        url::form_urlencoded::parse(&body)
            .into_owned()
            .collect::<Vec<_>>(),
    )
}

fn form_field<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// POST /oauth/authorize/login
// ---------------------------------------------------------------------------

#[handler]
pub(crate) async fn oauth_authorize_login(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };
    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }
    let db = crate::auth::get_db(depot);
    let Some(session_store) = depot
        .obtain::<std::sync::Arc<AuthorizeSessionStore>>()
        .ok()
        .cloned()
    else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no session store"})));
        return;
    };

    let pairs = match parse_form_body(req).await {
        Some(p) => p,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Text::Html(authorize_login_html(
                "/oauth/authorize",
                Some("invalid request body"),
            )));
            return;
        }
    };

    let return_to = form_field(&pairs, "return_to").unwrap_or("/oauth/authorize");
    let return_to_owned = return_to.to_string();

    // Revalidate return_to before doing anything with the submitted token.
    if validate_authorize_return_to(&return_to_owned).is_err() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Text::Html(authorize_login_html(
            "/oauth/authorize",
            Some("invalid return destination"),
        )));
        return;
    }

    let token = match form_field(&pairs, "token") {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Text::Html(authorize_login_html(
                &return_to_owned,
                Some("a WebCodex token is required"),
            )));
            return;
        }
    };

    // Reuse the shared verifier chain (PatVerifier -> OAuth2Verifier). This
    // accepts PAT (wc_pat_*), bootstrap, agent, account credentials, and
    // OAuth2 access tokens. We then narrow to Bootstrap / ApiToken only;
    // bootstrap is further rejected below because it has no user_id, so only
    // a PAT can complete the authorize login.
    let ctx = match crate::auth::authenticate(&config, db.as_ref(), &token).await {
        Ok(Some(ctx)) => ctx,
        _ => {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Text::Html(authorize_login_html(
                &return_to_owned,
                Some("invalid token"),
            )));
            return;
        }
    };

    if !is_authorize_identity_allowed(&ctx) {
        // Reject OAuth2 access tokens, agent tokens, and account credentials.
        // Do not reveal which kind was rejected; generic message.
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Text::Html(authorize_login_html(
            &return_to_owned,
            Some("this token kind cannot authorize OAuth clients"),
        )));
        return;
    }

    let Some(user_id) = ctx.user_id.clone() else {
        // Bootstrap has no user_id. Authorization codes must bind to a
        // concrete resource owner, so bootstrap cannot drive the browser
        // authorize login flow. Bootstrap/PAT may still *create* OAuth
        // clients via the management API, but the authorize login requires a
        // PAT that carries a real user_id.
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Text::Html(authorize_login_html(
            &return_to_owned,
            Some("bootstrap login is not supported for OAuth authorize; use a PAT"),
        )));
        return;
    };

    let session_id = session_store.create_session(user_id, ctx.username.clone(), ctx.kind);
    let secure = is_secure_authorize(&config);
    res.headers_mut().append(
        salvo::http::header::SET_COOKIE,
        HeaderValue::from_str(&authorize_session_cookie_header(&session_id, secure)).unwrap(),
    );
    res.status_code(StatusCode::FOUND);
    res.headers_mut().insert(
        salvo::http::header::LOCATION,
        HeaderValue::from_str(&return_to_owned).unwrap(),
    );
}

// ---------------------------------------------------------------------------
// POST /oauth/authorize/consent
// ---------------------------------------------------------------------------

#[handler]
pub(crate) async fn oauth_authorize_consent(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };
    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };
    let Some(session_store) = depot
        .obtain::<std::sync::Arc<AuthorizeSessionStore>>()
        .ok()
        .cloned()
    else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no session store"})));
        return;
    };

    // A valid first-party authorize session is mandatory. Hidden form fields
    // are NOT trusted for identity.
    let Some(session_cookie) = authorize_session_id_from_request(req) else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Text::Html(authorize_login_html(
            "/oauth/authorize",
            Some("session expired; please sign in again"),
        )));
        return;
    };
    let Some(session) = session_store.get_session(&session_cookie) else {
        res.status_code(StatusCode::UNAUTHORIZED);
        // Clear the stale cookie.
        let secure = is_secure_authorize(&config);
        res.headers_mut().append(
            salvo::http::header::SET_COOKIE,
            HeaderValue::from_str(&authorize_session_clear_cookie_header(secure)).unwrap(),
        );
        res.render(Text::Html(authorize_login_html(
            "/oauth/authorize",
            Some("session expired; please sign in again"),
        )));
        return;
    };

    let pairs = match parse_form_body(req).await {
        Some(p) => p,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": "invalid request body"})));
            return;
        }
    };

    let decision = form_field(&pairs, "decision").unwrap_or("").to_string();
    let is_allow = decision == "allow";

    // Reconstruct the authorize query from the submitted hidden fields and
    // revalidate client / redirect_uri / scope / PKCE from scratch.
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs.iter().filter(|(k, _)| k != "decision") {
        serializer.append_pair(k, v);
    }
    let query = serializer.finish();

    // Always need client + redirect to issue a safe redirect (even for deny).
    let parsed = match parse_authorize_query(&query) {
        Ok(p) => p,
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(
                serde_json::json!({"error": "invalid authorization request"}),
            ));
            return;
        }
    };

    let client = match db.get_oauth_client_by_client_id(&parsed.client_id) {
        Ok(Some(c)) => c,
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": "invalid client_id"})));
            return;
        }
    };

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|r| r == &parsed.redirect_uri)
    {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": "redirect_uri mismatch"})));
        return;
    }

    if !is_allow {
        // Deny: redirect with error=access_denied.
        redirect_with_oauth_error(
            res,
            &parsed.redirect_uri,
            "access_denied",
            parsed.state.as_deref(),
        );
        return;
    }

    // Allow: revalidate response_type / PKCE / scope / resource.
    if parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            &parsed.redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return;
    }
    if parsed.code_challenge.is_empty() || parsed.code_challenge_method != "S256" {
        redirect_with_oauth_error(
            res,
            &parsed.redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }
    let scopes = match normalize_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
        Ok(s) => s,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                &parsed.redirect_uri,
                "invalid_scope",
                parsed.state.as_deref(),
            );
            return;
        }
    };
    let resource = match validate_authorize_resource(parsed.resource.as_deref(), &config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                &parsed.redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return;
        }
    };

    // Issue the authorization code bound to the session's user.
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".to_string(),
        subject_id: session.user_id.clone(),
        user_id: Some(session.user_id.clone()),
        redirect_uri: parsed.redirect_uri.clone(),
        scopes,
        resource,
        code_challenge: Some(parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: None,
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };
    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(
        res,
        &parsed.redirect_uri,
        &plaintext_code,
        parsed.state.as_deref(),
    );
}

// ---------------------------------------------------------------------------
// GET /oauth/authorize
// ---------------------------------------------------------------------------

fn is_authorize_identity_allowed(ctx: &AuthContext) -> bool {
    matches!(ctx.kind, AuthKind::Bootstrap | AuthKind::ApiToken)
}

#[derive(Clone)]
struct BridgeAuthorizeValidated {
    parsed: OAuthAuthorizeRequest,
    client: crate::models::OAuthClientRecord,
    scopes: String,
    resource: Option<String>,
}

fn is_shared_key_bridge_query(query: &str) -> Result<bool, OAuthAuthorizeError> {
    match decoded_authorize_param(query, "bridge")? {
        Some(value) if value == "shared_key" => Ok(true),
        Some(_) => Err(OAuthAuthorizeError::InvalidRequest("unsupported bridge")),
        None => Ok(false),
    }
}

fn validate_bridge_authorize_request(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    query: &str,
) -> Option<BridgeAuthorizeValidated> {
    let parsed = match parse_authorize_query(query) {
        Ok(parsed) => parsed,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return None;
        }
    };

    if parsed.client_id.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        );
        return None;
    }

    let client = match db.get_oauth_client_by_client_id(&parsed.client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid client_id",
            );
            return None;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return None;
        }
    };

    if parsed.redirect_uri.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        );
        return None;
    }

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|registered| registered == &parsed.redirect_uri)
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
        return None;
    }

    if parsed.response_type.is_empty() || parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            &parsed.redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return None;
    }

    if parsed.code_challenge.is_empty()
        || parsed.code_challenge_method.is_empty()
        || parsed.code_challenge_method != "S256"
    {
        redirect_with_oauth_error(
            res,
            &parsed.redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return None;
    }

    let scopes =
        match normalize_bridge_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
            Ok(scopes) => scopes,
            Err(_) => {
                redirect_with_oauth_error(
                    res,
                    &parsed.redirect_uri,
                    "invalid_scope",
                    parsed.state.as_deref(),
                );
                return None;
            }
        };

    let resource = match validate_authorize_resource(parsed.resource.as_deref(), config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                &parsed.redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return None;
        }
    };

    Some(BridgeAuthorizeValidated {
        parsed,
        client,
        scopes,
        resource,
    })
}

fn render_bridge_authorize_form(
    res: &mut Response,
    validated: &BridgeAuthorizeValidated,
    query: &str,
    error: Option<&str>,
) {
    let scopes = validated
        .scopes
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let html = authorize_bridge_html(
        &validated.client.name,
        &validated.client.client_id,
        &validated.parsed.redirect_uri,
        &scopes,
        validated.resource.as_deref(),
        query,
        error,
    );
    res.status_code(if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    });
    res.render(Text::Html(html));
}

fn issue_bridge_authorization_code(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    validated: &BridgeAuthorizeValidated,
    shared_key_hash: String,
) {
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: validated.client.client_id.clone(),
        subject_kind: "shared_key".to_string(),
        subject_id: shared_key_hash.clone(),
        user_id: None,
        redirect_uri: validated.parsed.redirect_uri.clone(),
        scopes: validated.scopes.clone(),
        resource: validated.resource.clone(),
        code_challenge: Some(validated.parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: Some(shared_key_hash),
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };

    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(
        res,
        &validated.parsed.redirect_uri,
        &plaintext_code,
        validated.parsed.state.as_deref(),
    );
}

/// Authorization endpoint.
///
/// `/oauth/authorize` is mounted **without** `AuthMiddleware`. The handler
/// accepts either:
/// 1. a first-party Bearer PAT (with a concrete `user_id`) → direct
///    authorization-code issuance, or
/// 2. a short-lived `webcodex_authorize_session` cookie → consent page, or
/// 3. neither → minimal HTML login page.
///
/// The handler itself validates token/session/client/redirect/scope/PKCE.
/// OAuth2 access tokens, agent tokens, account credentials, and bootstrap
/// (which has no `user_id`) are rejected; no code is issued for them.
#[handler]
pub(crate) async fn oauth_authorize(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    if !config.oauth2.enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "OAuth2 is not enabled",
        );
        return;
    }

    let Some(db) = crate::auth::get_db(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    let Some(session_store) = depot
        .obtain::<std::sync::Arc<AuthorizeSessionStore>>()
        .ok()
        .cloned()
    else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no session store",
        );
        return;
    };

    let query = req.uri().query().unwrap_or("").to_string();

    match is_shared_key_bridge_query(&query) {
        Ok(true) => {
            if !config.oauth2.shared_key_bridge_enabled {
                oauth_authorize_direct_error(
                    res,
                    StatusCode::NOT_FOUND,
                    "invalid_request",
                    "shared-key OAuth bridge is not enabled",
                );
                return;
            }
            let Some(validated) = validate_bridge_authorize_request(res, &config, &db, &query)
            else {
                return;
            };
            render_bridge_authorize_form(res, &validated, &query, None);
            return;
        }
        Ok(false) => {}
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "unsupported bridge",
            );
            return;
        }
    }

    // Path 1: Bearer token (first-party direct issuance). A PAT (ApiToken)
    // with a concrete user_id goes straight to code issuance. Bootstrap is
    // rejected because it has no user_id to bind the authorization code to.
    if let Some(token) = crate::auth::bearer_token(req) {
        match crate::auth::authenticate(&config, Some(&db), &token).await {
            Ok(Some(ctx)) if is_authorize_identity_allowed(&ctx) && ctx.user_id.is_some() => {
                let user_id = ctx.user_id.clone().unwrap();
                authorize_issue_with_context(res, &config, &db, &user_id, &query).await;
                return;
            }
            Ok(Some(_)) => {
                // OAuth2Token / AgentToken / AccountCredential, or bootstrap
                // (no user_id): not a valid authorize identity.
                oauth_authorize_direct_error(
                    res,
                    StatusCode::FORBIDDEN,
                    "invalid_request",
                    "authorization endpoint requires first-party user authentication",
                );
                return;
            }
            _ => {
                oauth_authorize_direct_error(
                    res,
                    StatusCode::UNAUTHORIZED,
                    "invalid_request",
                    "invalid token",
                );
                return;
            }
        }
    }

    // Path 2: browser first-party session cookie.
    if let Some(session_cookie) = authorize_session_id_from_request(req) {
        if session_store.get_session(&session_cookie).is_some() {
            authorize_render_consent(res, &config, &db, &query);
            return;
        }
    }

    // Path 3: no Bearer, no session -> minimal login page.
    let return_to = if query.is_empty() {
        "/oauth/authorize".to_string()
    } else {
        format!("/oauth/authorize?{}", query)
    };
    res.status_code(StatusCode::OK);
    res.render(Text::Html(authorize_login_html(&return_to, None)));
}

#[handler]
pub(crate) async fn oauth_authorize_bridge(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    if !config.oauth2.enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "OAuth2 is not enabled",
        );
        return;
    }

    if !config.oauth2.shared_key_bridge_enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "shared-key OAuth bridge is not enabled",
        );
        return;
    }

    let Some(db) = crate::auth::get_db(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    let pairs = match parse_form_body(req).await {
        Some(pairs) => pairs,
        None => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid request body",
            );
            return;
        }
    };

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs.iter().filter(|(key, _)| key != "shared_key") {
        serializer.append_pair(key, value);
    }
    let query = serializer.finish();

    match is_shared_key_bridge_query(&query) {
        Ok(true) => {}
        _ => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "unsupported bridge",
            );
            return;
        }
    }

    let Some(validated) = validate_bridge_authorize_request(res, &config, &db, &query) else {
        return;
    };

    let submitted = form_field(&pairs, "shared_key").unwrap_or("");
    let shared_key_hash = match bridge_shared_key_hash(submitted) {
        Ok(hash) => hash,
        Err(message) => {
            render_bridge_authorize_form(res, &validated, &query, Some(message));
            return;
        }
    };

    issue_bridge_authorization_code(res, &config, &db, &validated, shared_key_hash);
}

/// First-party direct issuance path: validate the authorize query against the
/// registered client and, on success, issue a hashed authorization code bound
/// to `user_id` and redirect with the plaintext code. Used by the Bearer PAT
/// path of [`oauth_authorize`] (bootstrap is rejected upstream because it has
/// no `user_id`).
async fn authorize_issue_with_context(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    user_id: &str,
    query: &str,
) {
    let parsed = parse_authorize_query(query);

    let (client_id, redirect_uri) = match &parsed {
        Ok(parsed) => (parsed.client_id.clone(), parsed.redirect_uri.clone()),
        Err(error) if is_redirectable_missing_authorize_param(error) => {
            let client_id = match decoded_authorize_param(query, "client_id") {
                Ok(Some(client_id)) if !client_id.is_empty() => client_id,
                Ok(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "missing client_id",
                    );
                    return;
                }
                Err(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "duplicate parameter",
                    );
                    return;
                }
            };
            let redirect_uri = match decoded_authorize_param(query, "redirect_uri") {
                Ok(Some(redirect_uri)) if !redirect_uri.is_empty() => redirect_uri,
                Ok(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "missing redirect_uri",
                    );
                    return;
                }
                Err(_) => {
                    oauth_authorize_direct_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "duplicate parameter",
                    );
                    return;
                }
            };
            (client_id, redirect_uri)
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return;
        }
    };

    if client_id.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        );
        return;
    }

    let client = match db.get_oauth_client_by_client_id(&client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid client_id",
            );
            return;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    if redirect_uri.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        );
        return;
    }

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|registered| registered == &redirect_uri)
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
        return;
    }

    let state = match decoded_authorize_param(query, "state") {
        Ok(state) => state,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "duplicate parameter",
            );
            return;
        }
    };

    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) if is_redirectable_missing_authorize_param(&error) => {
            redirect_with_oauth_error(
                res,
                &redirect_uri,
                redirect_error_for_missing_authorize_param(&error),
                state.as_deref(),
            );
            return;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return;
        }
    };

    if parsed.response_type.is_empty() || parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return;
    }

    if parsed.code_challenge.is_empty() {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }

    if parsed.code_challenge_method.is_empty() || parsed.code_challenge_method != "S256" {
        redirect_with_oauth_error(
            res,
            &redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return;
    }

    let scopes = match normalize_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
        Ok(scopes) => scopes,
        Err(_) => {
            redirect_with_oauth_error(res, &redirect_uri, "invalid_scope", parsed.state.as_deref());
            return;
        }
    };

    let resource = match validate_authorize_resource(parsed.resource.as_deref(), config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                &redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return;
        }
    };

    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".to_string(),
        subject_id: user_id.to_string(),
        user_id: Some(user_id.to_string()),
        redirect_uri: redirect_uri.clone(),
        scopes,
        resource,
        code_challenge: Some(parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: None,
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };

    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(res, &redirect_uri, &plaintext_code, parsed.state.as_deref());
}

/// Browser session path: validate the client + redirect_uri + scope against
/// the authorize query, then render the minimal consent page. Does NOT issue
/// a code; the actual issuance happens in [`oauth_authorize_consent`] after
/// the user picks Allow/Deny. Unknown client / redirect mismatch produce a
/// direct 400. Invalid scope produces a direct error page so the user is not
/// shown a misleading consent prompt.
fn authorize_render_consent(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    query: &str,
) {
    let parsed = match parse_authorize_query(query) {
        Ok(p) => p,
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Text::Html(authorize_login_html(
                "/oauth/authorize",
                Some("invalid authorization request"),
            )));
            return;
        }
    };

    let client = match db.get_oauth_client_by_client_id(&parsed.client_id) {
        Ok(Some(c)) => c,
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Text::Html(authorize_login_html(
                "/oauth/authorize",
                Some("invalid client_id"),
            )));
            return;
        }
    };

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|r| r == &parsed.redirect_uri)
    {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Text::Html(authorize_login_html(
            "/oauth/authorize",
            Some("redirect_uri mismatch"),
        )));
        return;
    }

    let scopes = match normalize_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
        Ok(s) => s.split_whitespace().map(str::to_string).collect::<Vec<_>>(),
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Text::Html(authorize_login_html(
                "/oauth/authorize",
                Some("invalid scope"),
            )));
            return;
        }
    };

    let resource = match validate_authorize_resource(parsed.resource.as_deref(), config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                &parsed.redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return;
        }
    };

    let html = authorize_consent_html(
        &client.name,
        &client.client_id,
        &parsed.redirect_uri,
        &scopes,
        resource.as_deref(),
        query,
    );
    res.status_code(StatusCode::OK);
    res.render(Text::Html(html));
}

// ---------------------------------------------------------------------------
// Form body
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct RevokeRequest {
    token: Option<String>,
    token_type_hint: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

// ---------------------------------------------------------------------------
// Revoke handler
// ---------------------------------------------------------------------------

#[handler]
pub(crate) async fn oauth_revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    // --- Config ---
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    // --- OAuth2 enable gate ---
    if !config.oauth2.enabled {
        oauth_error(
            res,
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "OAuth2 is not enabled",
        );
        return;
    }

    // --- Content-Type enforcement (same as /oauth/token) ---
    let content_type_ok = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            ct.eq_ignore_ascii_case("application/x-www-form-urlencoded")
                || ct
                    .to_ascii_lowercase()
                    .starts_with("application/x-www-form-urlencoded;")
        })
        .unwrap_or(false);

    if !content_type_ok {
        oauth_error(
            res,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_request",
            "content-type must be application/x-www-form-urlencoded",
        );
        return;
    }

    // --- Body size limit (Content-Length pre-check) ---
    if let Some(cl) = req.headers().get("content-length") {
        if let Ok(len) = cl.to_str().unwrap_or("").parse::<usize>() {
            if len > MAX_OAUTH_TOKEN_FORM_BYTES {
                oauth_error(
                    res,
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "invalid_request",
                    "request body too large",
                );
                return;
            }
        }
    }

    // --- Parse form body ---
    let body = match req.payload().await {
        Ok(bytes) => bytes,
        Err(e) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("failed to read request body: {}", e),
            );
            return;
        }
    };

    // --- Body size limit (actual body check) ---
    if body.len() > MAX_OAUTH_TOKEN_FORM_BYTES {
        oauth_error(
            res,
            StatusCode::PAYLOAD_TOO_LARGE,
            "invalid_request",
            "request body too large",
        );
        return;
    }

    let form: RevokeRequest = match serde_urlencoded::from_bytes(&body) {
        Ok(f) => f,
        Err(e) => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid form body: {}", e),
            );
            return;
        }
    };

    // --- Validate required parameters ---
    let plaintext_token = match form.token.as_deref() {
        Some(t) if !t.is_empty() => t,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing token",
            );
            return;
        }
    };

    let client_id = match form.client_id.as_deref() {
        Some(c) if !c.is_empty() => c,
        _ => {
            oauth_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing client_id",
            );
            return;
        }
    };

    let client_secret = match form.client_secret.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            oauth_error(
                res,
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "missing client_secret",
            );
            return;
        }
    };

    // --- DB ---
    let Some(db) = crate::auth::get_db(depot) else {
        oauth_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    // --- Client authentication ---
    let secret_ok = match db.verify_oauth_client_secret(client_id, client_secret) {
        Ok(ok) => ok,
        Err(_) => {
            oauth_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return;
        }
    };

    if !secret_ok {
        oauth_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
        return;
    }

    // Also verify the client is not revoked.
    if db
        .get_oauth_client_by_client_id(client_id)
        .ok()
        .flatten()
        .is_none()
    {
        oauth_error(
            res,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
        return;
    }

    // --- Revoke ---
    let token_hash = hash_token(plaintext_token);
    let now = chrono::Utc::now().timestamp();

    let hint = form.token_type_hint.as_deref().unwrap_or("");

    // Per RFC 7009: if hint is provided and recognized, try only that type.
    // If hint is missing or unrecognized, try both.
    match hint {
        "access_token" => {
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
        }
        "refresh_token" => {
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
        _ => {
            // No hint or unrecognized hint — try both.
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
    }

    // Always return 200 — idempotent, no token state disclosure.
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// GET /.well-known/oauth-protected-resource
// ---------------------------------------------------------------------------

/// Non-agent scopes that OAuth2 clients may request. Agent transport scopes
/// (`agent:*`) are excluded because OAuth2 access tokens are rejected on agent
/// transport surfaces. `admin` is excluded because it is a bootstrap/superuser
/// scope not intended for OAuth2 delegation.
const OAUTH_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_JOB_RUN,
    scopes::SCOPE_ACCOUNT_MANAGE,
];

const OAUTH_BRIDGE_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_JOB_RUN,
];

const OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE: &str =
    "bridge tokens are limited to runtime:read, project:read, project:write, job:run";

/// Return the canonical global OAuth scope registry.
///
/// The order is stable and is used for authorization-time normalization.
pub(crate) fn oauth_scopes_supported() -> &'static [&'static str] {
    OAUTH_SCOPES_SUPPORTED
}

/// Normalize authorize-time OAuth scopes against a registered client's allowed
/// scopes and the global OAuth scope registry.
///
/// If `requested` is absent or ASCII-whitespace-only, default to the
/// intersection of `client_allowed` and the global OAuth scope registry. When
/// `requested` is present, every requested scope must be both globally
/// supported and allowed by the client. Output is deduplicated and ordered by
/// the global registry.
#[allow(dead_code)]
pub(crate) fn normalize_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let client_allowed: std::collections::HashSet<&str> =
        client_allowed.split_ascii_whitespace().collect();

    let normalized = match requested {
        Some(raw) if raw.split_ascii_whitespace().next().is_some() => {
            let mut requested_scopes = std::collections::HashSet::new();
            for scope in raw.split_ascii_whitespace() {
                if !oauth_scopes_supported().contains(&scope) || !client_allowed.contains(scope) {
                    return Err(OAuthAuthorizeError::InvalidScope("invalid scope"));
                }
                requested_scopes.insert(scope);
            }

            oauth_scopes_supported()
                .iter()
                .copied()
                .filter(|scope| requested_scopes.contains(scope))
                .collect::<Vec<_>>()
        }
        _ => oauth_scopes_supported()
            .iter()
            .copied()
            .filter(|scope| client_allowed.contains(scope))
            .collect::<Vec<_>>(),
    };

    if normalized.is_empty() {
        return Err(OAuthAuthorizeError::InvalidScope("empty scope"));
    }

    Ok(normalized.join(" "))
}

fn normalize_bridge_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let normalized = normalize_oauth_scopes(requested, client_allowed)?;
    if normalized
        .split_whitespace()
        .any(|scope| !OAUTH_BRIDGE_SCOPES_SUPPORTED.contains(&scope))
    {
        return Err(OAuthAuthorizeError::InvalidScope(
            OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE,
        ));
    }
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        generate_account_credential, generate_agent_token, generate_api_token,
        generate_oauth_authorization_code, hash_token, token_prefix, AuthKind, OAuth2Verifier,
        TokenVerifier,
    };
    use crate::models::{
        AccountCredentialRecord, ApiKeyRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
        UserRecord, TOKEN_KIND_AGENT, TOKEN_KIND_USER,
    };
    use crate::OAuth2Config;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_config(oauth2: OAuth2Config) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("bootstrap-token".to_string()),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2,
        })
    }

    fn oauth2_enabled() -> OAuth2Config {
        OAuth2Config {
            enabled: true,
            require_pkce: true,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            ..OAuth2Config::default()
        }
    }

    fn oauth2_enabled_bridge() -> OAuth2Config {
        OAuth2Config {
            shared_key_bridge_enabled: true,
            ..oauth2_enabled()
        }
    }

    fn oauth2_enabled_no_pkce() -> OAuth2Config {
        OAuth2Config {
            enabled: true,
            require_pkce: false,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            ..OAuth2Config::default()
        }
    }

    fn oauth2_enabled_with_issuer(issuer: &str) -> OAuth2Config {
        OAuth2Config {
            issuer: Some(issuer.to_string()),
            ..oauth2_enabled()
        }
    }

    fn oauth2_enabled_no_pkce_with_issuer(issuer: &str) -> OAuth2Config {
        OAuth2Config {
            issuer: Some(issuer.to_string()),
            ..oauth2_enabled_no_pkce()
        }
    }

    fn oauth2_disabled() -> OAuth2Config {
        OAuth2Config {
            enabled: false,
            ..OAuth2Config::default()
        }
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("oauth.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn authorize_query_without(missing: &str) -> String {
        [
            ("response_type", "code"),
            ("client_id", "client-1"),
            ("redirect_uri", "https://client.example/cb"),
            ("scope", "runtime:read"),
            ("state", "keep+this value"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]
        .into_iter()
        .filter(|(key, _)| *key != missing)
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
    }

    fn valid_authorize_query() -> String {
        authorize_query_without("")
    }

    #[test]
    fn bridge_shared_key_hash_matches_shared_key_visibility_hash() {
        assert_eq!(
            bridge_shared_key_hash("shared-secret").unwrap(),
            shared_key_hash_of("shared-secret")
        );
        assert_eq!(
            bridge_shared_key_hash("  shared-secret  ").unwrap(),
            shared_key_hash_of("shared-secret")
        );
        assert_eq!(
            bridge_shared_key_hash("shared-secret").unwrap(),
            shared_key_hash_of("  shared-secret  ")
        );
    }

    #[test]
    fn normalize_oauth_scopes_defaults_to_client_global_intersection() {
        let normalized =
            normalize_oauth_scopes(None, "project:write runtime:read agent:poll admin").unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_default_rejects_empty_intersection() {
        let err = normalize_oauth_scopes(None, "agent:poll admin unknown").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("empty scope"));
    }

    #[test]
    fn normalize_oauth_scopes_requested_subset_success() {
        let normalized = normalize_oauth_scopes(
            Some("project:write runtime:read"),
            "runtime:read project:read project:write",
        )
        .unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_deduplicates_and_orders() {
        let normalized = normalize_oauth_scopes(
            Some("project:write runtime:read runtime:read"),
            "runtime:read project:read project:write",
        )
        .unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_oauth_scopes_rejects_unknown_scope() {
        let err = normalize_oauth_scopes(Some("unknown"), "runtime:read unknown").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_scope_not_allowed_by_client() {
        let err = normalize_oauth_scopes(Some("runtime:read"), "project:read").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_agent_scope() {
        let err = normalize_oauth_scopes(Some("agent:poll"), "agent:poll").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_rejects_admin_scope() {
        let err = normalize_oauth_scopes(Some("admin"), "admin").unwrap_err();

        assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
    }

    #[test]
    fn normalize_oauth_scopes_treats_empty_requested_as_default() {
        let normalized =
            normalize_oauth_scopes(Some(" \t\n"), "project:write runtime:read admin").unwrap();

        assert_eq!(normalized, "runtime:read project:write");
    }

    #[test]
    fn normalize_bridge_oauth_scopes_rejects_account_scope_with_bridge_message() {
        let err = normalize_bridge_oauth_scopes(
            Some("account:manage"),
            "runtime:read project:read account:manage",
        )
        .unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidScope(OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE)
        );
    }

    #[test]
    fn parse_authorize_query_requires_response_type() {
        let err = parse_authorize_query(&authorize_query_without("response_type")).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("missing response_type")
        );
    }

    #[test]
    fn parse_authorize_query_requires_client_id() {
        let err = parse_authorize_query(&authorize_query_without("client_id")).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("missing client_id")
        );
    }

    #[test]
    fn parse_authorize_query_requires_redirect_uri() {
        let err = parse_authorize_query(&authorize_query_without("redirect_uri")).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("missing redirect_uri")
        );
    }

    #[test]
    fn parse_authorize_query_requires_code_challenge() {
        let err = parse_authorize_query(&authorize_query_without("code_challenge")).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("missing code_challenge")
        );
    }

    #[test]
    fn parse_authorize_query_requires_code_challenge_method() {
        let err =
            parse_authorize_query(&authorize_query_without("code_challenge_method")).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("missing code_challenge_method")
        );
    }

    #[test]
    fn parse_authorize_query_preserves_state() {
        let parsed = parse_authorize_query(&valid_authorize_query()).unwrap();

        assert_eq!(parsed.state.as_deref(), Some("keep+this value"));
    }

    #[test]
    fn parse_authorize_query_keeps_resource_for_later_rejection() {
        let query = format!(
            "{}&resource={}",
            valid_authorize_query(),
            urlencoding::encode("https://api.example/resource")
        );

        let parsed = parse_authorize_query(&query).unwrap();

        assert_eq!(
            parsed.resource.as_deref(),
            Some("https://api.example/resource")
        );
    }

    #[test]
    fn validate_authorize_resource_rejects_unsafe_values() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        for resource in [
            "",
            "   ",
            "/mcp",
            "ftp://example.test/mcp",
            "https://example.test/mcp?x=1",
            "https://example.test/mcp#frag",
            "https://evil.example",
        ] {
            assert_eq!(
                validate_authorize_resource(Some(resource), &config).unwrap_err(),
                OAuthAuthorizeError::UnsupportedResource,
                "resource should be rejected: {resource:?}"
            );
        }
    }

    #[test]
    fn parse_authorize_query_rejects_duplicate_parameters() {
        let query = format!("{}&client_id=client-2", valid_authorize_query());
        let err = parse_authorize_query(&query).unwrap_err();

        assert_eq!(
            err,
            OAuthAuthorizeError::InvalidRequest("duplicate parameter")
        );
    }

    fn seed_user(db: &crate::Database, username: &str) -> UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
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

    fn seed_client(
        db: &crate::Database,
        user: &UserRecord,
        name: &str,
    ) -> (OAuthClientRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
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

    fn seed_client_with_redirects_and_scopes(
        db: &crate::Database,
        user: &UserRecord,
        redirect_uris: &str,
        allowed_scopes: &str,
    ) -> OAuthClientRecord {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: secret_hash,
            name: "authorize-client".to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: redirect_uris.to_string(),
            allowed_scopes: allowed_scopes.to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        record
    }

    fn seed_user_token(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_api_token();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "authorize-user-token".to_string(),
            key_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:read project:write job:run".to_string(),
            expires_at: None,
            kind: TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    fn seed_agent_token(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_agent_token();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "authorize-agent-token".to_string(),
            key_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:poll agent:result".to_string(),
            expires_at: None,
            kind: TOKEN_KIND_AGENT.to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    fn seed_account_credential(db: &crate::Database, user: &UserRecord) -> String {
        let plaintext = generate_account_credential();
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = AccountCredentialRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            credential_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        db.insert_account_credential(&record, &hash).unwrap();
        plaintext
    }

    fn authorize_url(params: &[(&str, &str)]) -> String {
        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("http://localhost/oauth/authorize?{}", query)
    }

    fn valid_authorize_url(client: &OAuthClientRecord, redirect_uri: &str) -> String {
        authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ])
    }

    fn valid_bridge_authorize_url(
        client: &OAuthClientRecord,
        redirect_uri: &str,
        scope: &str,
    ) -> String {
        authorize_url(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", scope),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ])
    }

    fn bridge_form_body(
        client: &OAuthClientRecord,
        redirect_uri: &str,
        scope: &str,
        shared_key: &str,
    ) -> String {
        form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", scope),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("shared_key", shared_key),
        ])
    }

    fn valid_authorize_url_with_resource(
        client: &OAuthClientRecord,
        redirect_uri: &str,
        resource: &str,
    ) -> String {
        authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("resource", resource),
        ])
    }

    fn authorized_get(url: &str, token: &str) -> salvo::test::RequestBuilder {
        TestClient::get(url).add_header("authorization", &format!("Bearer {}", token), true)
    }

    fn auth_code_count(db: &crate::Database) -> i64 {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT COUNT(*) FROM oauth_authorization_codes",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn location_header(resp: &Response) -> Option<String> {
        resp.headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }

    fn assert_no_location(resp: &Response) {
        assert!(
            location_header(resp).is_none(),
            "direct errors must not include Location"
        );
    }

    async fn assert_authorize_direct_400(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
    ) {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(db), before);
    }

    async fn assert_authorize_redirect_error(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
        expected_base: &str,
        expected_error: &str,
        expected_state: Option<&str>,
    ) -> String {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(db), before);
        let location = location_header(&resp).expect("redirect error should set Location");
        assert!(
            location.starts_with(expected_base),
            "Location {} should start with {}",
            location,
            expected_base
        );
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("error").map(String::as_str),
            Some(expected_error)
        );
        if let Some(expected_state) = expected_state {
            assert_eq!(
                params.get("state").map(String::as_str),
                Some(expected_state)
            );
        } else {
            assert!(!params.contains_key("state"));
        }
        location
    }

    async fn authorize_success(
        service: &Service,
        db: &crate::Database,
        url: &str,
        token: &str,
    ) -> (Response, String, url::Url, String) {
        let before = auth_code_count(db);
        let resp = authorized_get(url, token).send(service).await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(db), before + 1);
        let location = location_header(&resp).expect("success should set Location");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params
            .get("code")
            .expect("success redirect should include code")
            .clone();
        assert!(code.starts_with("wc_oac_"));
        assert!(!params.contains_key("access_token"));
        assert!(!params.contains_key("refresh_token"));
        (resp, location, parsed, code)
    }

    fn auth_code_by_plaintext(
        db: &crate::Database,
        plaintext_code: &str,
    ) -> OAuthAuthorizationCodeRecord {
        db.get_oauth_authorization_code_by_hash(&hash_token(plaintext_code))
            .unwrap()
            .expect("authorization code row should exist")
    }

    fn pkce_s256_challenge(code_verifier: &str) -> String {
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    fn seed_auth_code(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        redirect_uri: &str,
        scopes: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
    ) -> (OAuthAuthorizationCodeRecord, String) {
        seed_auth_code_with_resource(
            db,
            client,
            user,
            redirect_uri,
            scopes,
            code_challenge,
            code_challenge_method,
            None,
        )
    }

    fn seed_auth_code_with_resource(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        redirect_uri: &str,
        scopes: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
        resource: Option<&str>,
    ) -> (OAuthAuthorizationCodeRecord, String) {
        seed_auth_code_with_resource_and_shared_key_hash(
            db,
            client,
            user,
            redirect_uri,
            scopes,
            code_challenge,
            code_challenge_method,
            resource,
            None,
        )
    }

    fn seed_auth_code_with_shared_key_hash(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        redirect_uri: &str,
        scopes: &str,
        shared_key_hash: &str,
    ) -> (OAuthAuthorizationCodeRecord, String) {
        seed_auth_code_with_resource_and_shared_key_hash(
            db,
            client,
            user,
            redirect_uri,
            scopes,
            None,
            None,
            None,
            Some(shared_key_hash),
        )
    }

    fn seed_auth_code_with_resource_and_shared_key_hash(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        redirect_uri: &str,
        scopes: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
        resource: Option<&str>,
        shared_key_hash: Option<&str>,
    ) -> (OAuthAuthorizationCodeRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_code = generate_oauth_authorization_code();
        let code_hash = hash_token(&plaintext_code);
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash,
            client_id: client.client_id.clone(),
            subject_kind: shared_key_hash
                .map(|_| "shared_key")
                .unwrap_or("managed_user")
                .to_string(),
            subject_id: shared_key_hash.unwrap_or(&user.id).to_string(),
            user_id: shared_key_hash
                .map(|_| ())
                .map_or(Some(user.id.clone()), |_| None),
            redirect_uri: redirect_uri.to_string(),
            scopes: scopes.to_string(),
            code_challenge: code_challenge.map(str::to_string),
            code_challenge_method: code_challenge_method.map(str::to_string),
            resource: resource.map(str::to_string),
            shared_key_hash: shared_key_hash.map(str::to_string),
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &record.code_hash)
            .unwrap();
        (record, plaintext_code)
    }

    /// Seed a refresh token directly into the database. Returns the record
    /// and the plaintext token.
    fn seed_refresh_token(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
    ) -> (crate::models::OAuthRefreshTokenRecord, String) {
        seed_refresh_token_with_resource(db, client, user, scopes, None)
    }

    fn seed_refresh_token_with_resource(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
        resource: Option<&str>,
    ) -> (crate::models::OAuthRefreshTokenRecord, String) {
        seed_refresh_token_with_resource_and_shared_key_hash(
            db, client, user, scopes, resource, None,
        )
    }

    fn seed_refresh_token_with_shared_key_hash(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
        shared_key_hash: &str,
    ) -> (crate::models::OAuthRefreshTokenRecord, String) {
        seed_refresh_token_with_resource_and_shared_key_hash(
            db,
            client,
            user,
            scopes,
            None,
            Some(shared_key_hash),
        )
    }

    fn seed_refresh_token_with_resource_and_shared_key_hash(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
        resource: Option<&str>,
        shared_key_hash: Option<&str>,
    ) -> (crate::models::OAuthRefreshTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_refresh_token();
        let token_hash = hash_token(&plaintext);
        let record = crate::models::OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            client_id: client.client_id.clone(),
            subject_kind: shared_key_hash
                .map(|_| "shared_key")
                .unwrap_or("managed_user")
                .to_string(),
            subject_id: shared_key_hash.unwrap_or(&user.id).to_string(),
            user_id: shared_key_hash
                .map(|_| ())
                .map_or(Some(user.id.clone()), |_| None),
            scopes: scopes.to_string(),
            resource: resource.map(str::to_string),
            shared_key_hash: shared_key_hash.map(str::to_string),
            created_at: now,
            expires_at: now + 2_592_000, // 30 days
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();
        (record, plaintext)
    }

    /// Seed an access token directly into the database. Returns the record
    /// and the plaintext token.
    fn seed_access_token(
        db: &crate::Database,
        client: &OAuthClientRecord,
        user: &UserRecord,
        scopes: &str,
    ) -> (crate::models::OAuthAccessTokenRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_access_token();
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
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600, // 1 hour
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        (record, plaintext)
    }

    #[handler]
    async fn test_agent_register_handler(res: &mut Response) {
        res.render(Json(serde_json::json!({"ok": true})));
    }

    fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        let session_store = Arc::new(AuthorizeSessionStore::new());
        build_router_with_session(config, db, session_store)
    }

    fn build_router_with_session(
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        session_store: Arc<AuthorizeSessionStore>,
    ) -> Router {
        Router::new()
            .hoop(salvo::prelude::affix_state::inject(config))
            .hoop(salvo::prelude::affix_state::inject(db))
            .hoop(salvo::prelude::affix_state::inject(session_store))
            .push(Router::with_path("oauth/token").post(oauth_token))
            .push(Router::with_path("oauth/revoke").post(oauth_revoke))
            .push(
                Router::with_path("oauth/authorize")
                    .get(oauth_authorize)
                    .push(Router::with_path("login").post(oauth_authorize_login))
                    .push(Router::with_path("consent").post(oauth_authorize_consent))
                    .push(Router::with_path("bridge").post(oauth_authorize_bridge)),
            )
            .push(
                Router::with_path("api/oauth/clients")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("create").post(oauth_clients_create))
                    .push(Router::with_path("list").post(oauth_clients_list))
                    .push(Router::with_path("revoke").post(oauth_clients_revoke)),
            )
            .push(
                Router::with_path("api/shell/agent/register")
                    .hoop(crate::AuthMiddleware)
                    .post(test_agent_register_handler),
            )
            .push(Router::with_path(".well-known/oauth-protected-resource").get(oauth_metadata))
            .push(
                Router::with_path(".well-known/oauth-authorization-server")
                    .get(oauth_authorization_server_metadata),
            )
    }

    fn form_body(pairs: &[(&str, &str)]) -> String {
        pairs
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Build a POST request with the correct Content-Type header.
    fn post_form(url: &str, body: String) -> salvo::test::RequestBuilder {
        TestClient::post(url)
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body(body)
    }

    /// Return `(access_token_count, refresh_token_count)` from the DB.
    fn oauth_token_counts(db: &crate::Database) -> (i64, i64) {
        let conn = db.conn_for_tests();
        let at: i64 = conn
            .query_row("SELECT COUNT(*) FROM oauth_access_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        let rt: i64 = conn
            .query_row("SELECT COUNT(*) FROM oauth_refresh_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        (at, rt)
    }

    fn access_token_resource_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> Option<String> {
        db.conn_for_tests()
            .query_row(
                "SELECT resource FROM oauth_access_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn access_token_shared_key_hash_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> Option<String> {
        db.conn_for_tests()
            .query_row(
                "SELECT shared_key_hash FROM oauth_access_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn access_token_subject_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> (String, String, Option<String>, Option<String>) {
        db.conn_for_tests()
            .query_row(
                "SELECT subject_kind, subject_id, user_id, shared_key_hash FROM oauth_access_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    fn refresh_token_resource_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> Option<String> {
        db.conn_for_tests()
            .query_row(
                "SELECT resource FROM oauth_refresh_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn refresh_token_shared_key_hash_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> Option<String> {
        db.conn_for_tests()
            .query_row(
                "SELECT shared_key_hash FROM oauth_refresh_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn refresh_token_subject_by_plaintext(
        db: &crate::Database,
        plaintext_token: &str,
    ) -> (String, String, Option<String>, Option<String>) {
        db.conn_for_tests()
            .query_row(
                "SELECT subject_kind, subject_id, user_id, shared_key_hash FROM oauth_refresh_tokens WHERE token_hash = ?1",
                [&hash_token(plaintext_token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // Authorization endpoint
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth_authorize_without_bearer_or_session_returns_login_page() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let url = valid_authorize_url(&client, "https://example.com/callback");
        let before = auth_code_count(&db);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = TestClient::get(&url).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
        let body = resp.take_string().await.unwrap_or_default();
        assert!(
            body.contains("/oauth/authorize/login"),
            "login form missing"
        );
        assert!(body.contains("name=\"token\""), "token input missing");
        // The login page must not reveal the original query token or echo
        // any secret; it only carries the return_to path.
        assert!(body.contains("return_to"));
    }

    #[tokio::test]
    async fn bridge_authorize_get_disabled_creates_no_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url =
            valid_bridge_authorize_url(&client, "https://example.com/callback", "runtime:read");
        let before = auth_code_count(&db);

        let resp = TestClient::get(&url).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn bridge_authorize_post_disabled_creates_no_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let body = bridge_form_body(
            &client,
            "https://example.com/callback",
            "runtime:read",
            "shared-secret",
        );
        let before = auth_code_count(&db);

        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn bridge_disabled_does_not_break_managed_user_authorize() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_eq!(record.subject_kind, "managed_user");
        assert_eq!(record.user_id.as_deref(), Some(user.id.as_str()));
        assert_eq!(record.shared_key_hash, None);
    }

    #[tokio::test]
    async fn bridge_authorize_get_invalid_client_or_redirect_creates_no_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let invalid_client_url = authorize_url(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", "wc_client_missing"),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);
        let mismatch_url = valid_bridge_authorize_url(
            &client,
            "https://attacker.example/callback",
            "runtime:read",
        );

        for url in [invalid_client_url, mismatch_url] {
            let before = auth_code_count(&db);
            let resp = TestClient::get(&url).send(&service).await;
            assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
            assert_no_location(&resp);
            assert_eq!(auth_code_count(&db), before);
        }
    }

    #[tokio::test]
    async fn bridge_authorize_rejects_missing_or_invalid_pkce_without_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let urls = [
            authorize_url(&[
                ("bridge", "shared_key"),
                ("response_type", "code"),
                ("client_id", &client.client_id),
                ("redirect_uri", "https://example.com/callback"),
                ("scope", "runtime:read"),
                ("code_challenge_method", "S256"),
            ]),
            authorize_url(&[
                ("bridge", "shared_key"),
                ("response_type", "code"),
                ("client_id", &client.client_id),
                ("redirect_uri", "https://example.com/callback"),
                ("scope", "runtime:read"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "plain"),
            ]),
        ];

        for url in urls {
            let before = auth_code_count(&db);
            let resp = TestClient::get(&url).send(&service).await;
            assert_eq!(auth_code_count(&db), before);
            assert_ne!(resp.status_code, Some(StatusCode::OK));
        }
    }

    #[tokio::test]
    async fn bridge_authorize_get_renders_form_and_creates_no_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url =
            valid_bridge_authorize_url(&client, "https://example.com/callback", "runtime:read");
        let before = auth_code_count(&db);

        let mut resp = TestClient::get(&url).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
        let text = resp.take_string().await.unwrap_or_default();
        assert!(text.contains("/oauth/authorize/bridge"));
        assert!(text.contains("name=\"shared_key\""));
        assert!(!text.contains("wc_oac_"));
    }

    #[tokio::test]
    async fn bridge_authorize_post_rejects_empty_or_managed_key_without_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));

        for submitted in ["   ", "wc_pat_not_a_shared_key"] {
            let body = bridge_form_body(
                &client,
                "https://example.com/callback",
                "runtime:read",
                submitted,
            );
            let before = auth_code_count(&db);
            let mut resp = post_form("http://localhost/oauth/authorize/bridge", body)
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
            assert_no_location(&resp);
            assert_eq!(auth_code_count(&db), before);
            let text = resp.take_string().await.unwrap_or_default();
            let trimmed = submitted.trim();
            if !trimmed.is_empty() {
                assert!(!text.contains(trimmed));
            }
        }
    }

    #[tokio::test]
    async fn bridge_authorize_post_revalidates_hidden_fields_without_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));

        let direct_error_cases = [
            (
                "missing bridge hidden field",
                form_body(&[
                    ("response_type", "code"),
                    ("client_id", &client.client_id),
                    ("redirect_uri", "https://example.com/callback"),
                    ("scope", "runtime:read"),
                    ("state", "state-1"),
                    ("code_challenge", "challenge-1"),
                    ("code_challenge_method", "S256"),
                    ("shared_key", "shared-secret"),
                ]),
            ),
            (
                "missing response_type",
                form_body(&[
                    ("bridge", "shared_key"),
                    ("client_id", &client.client_id),
                    ("redirect_uri", "https://example.com/callback"),
                    ("scope", "runtime:read"),
                    ("state", "state-1"),
                    ("code_challenge", "challenge-1"),
                    ("code_challenge_method", "S256"),
                    ("shared_key", "shared-secret"),
                ]),
            ),
            (
                "tampered redirect_uri",
                form_body(&[
                    ("bridge", "shared_key"),
                    ("response_type", "code"),
                    ("client_id", &client.client_id),
                    ("redirect_uri", "https://attacker.example/callback"),
                    ("scope", "runtime:read"),
                    ("state", "state-1"),
                    ("code_challenge", "challenge-1"),
                    ("code_challenge_method", "S256"),
                    ("shared_key", "shared-secret"),
                ]),
            ),
            (
                "tampered client_id",
                form_body(&[
                    ("bridge", "shared_key"),
                    ("response_type", "code"),
                    ("client_id", "wc_client_missing"),
                    ("redirect_uri", "https://example.com/callback"),
                    ("scope", "runtime:read"),
                    ("state", "state-1"),
                    ("code_challenge", "challenge-1"),
                    ("code_challenge_method", "S256"),
                    ("shared_key", "shared-secret"),
                ]),
            ),
        ];

        for (name, body) in direct_error_cases {
            let before = auth_code_count(&db);
            let resp = post_form("http://localhost/oauth/authorize/bridge", body)
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST), "{name}");
            assert_no_location(&resp);
            assert_eq!(auth_code_count(&db), before, "{name}");
        }

        let unsupported_response_type = form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "token"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("shared_key", "shared-secret"),
        ]);
        let before = auth_code_count(&db);
        let resp = post_form(
            "http://localhost/oauth/authorize/bridge",
            unsupported_response_type,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before);
        let location = location_header(&resp).expect("unsupported response_type redirect");
        let parsed = url::Url::parse(&location).unwrap();
        assert_eq!(
            parsed.as_str().split('?').next(),
            Some("https://example.com/callback")
        );
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("error").map(String::as_str),
            Some("unsupported_response_type")
        );
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
        assert!(!params.contains_key("code"));
    }

    #[tokio::test]
    async fn bridge_authorize_valid_shared_key_creates_shared_key_code() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let shared_key = "shared-secret-value";
        let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
        let body = bridge_form_body(
            &client,
            "https://example.com/callback",
            "runtime:read project:read",
            shared_key,
        );
        let before = auth_code_count(&db);

        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before + 1);
        let location = location_header(&resp).expect("success redirect");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
        let code = params.get("code").expect("code");
        let record = auth_code_by_plaintext(&db, code);
        assert_eq!(record.subject_kind, "shared_key");
        assert_eq!(record.subject_id, expected_hash);
        assert_eq!(record.user_id, None);
        assert_eq!(
            record.shared_key_hash.as_deref(),
            Some(record.subject_id.as_str())
        );
        assert_eq!(record.scopes, "runtime:read project:read");
        assert_ne!(record.code_hash, *code);

        let leaked: i64 = db
            .conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM oauth_authorization_codes
                 WHERE code_hash LIKE ?1 OR client_id LIKE ?1 OR subject_id LIKE ?1
                    OR COALESCE(user_id, '') LIKE ?1 OR redirect_uri LIKE ?1
                    OR scopes LIKE ?1 OR COALESCE(code_challenge, '') LIKE ?1
                    OR COALESCE(code_challenge_method, '') LIKE ?1
                    OR COALESCE(resource, '') LIKE ?1 OR COALESCE(shared_key_hash, '') LIKE ?1",
                [format!("%{}%", shared_key)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leaked, 0, "plaintext shared key must not be stored");
    }

    #[tokio::test]
    async fn bridge_authorize_code_exchanges_to_shared_key_tokens_and_verifies() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Bridge App");
        let verifier = "bridge-code-verifier";
        let challenge = pkce_s256_challenge(verifier);
        let shared_key = "bridge-shared-secret";
        let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
        let service = Service::new(build_router(config.clone(), db.clone()));
        let body = form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("shared_key", shared_key),
        ]);
        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        let location = location_header(&resp).expect("success redirect");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params.get("code").expect("code").clone();

        let exchange_body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", verifier),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", exchange_body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();
        assert_eq!(json["scope"], "runtime:read");
        assert_eq!(
            access_token_subject_by_plaintext(&db, access_token),
            (
                "shared_key".to_string(),
                expected_hash.clone(),
                None,
                Some(expected_hash.clone())
            )
        );
        assert_eq!(
            refresh_token_subject_by_plaintext(&db, refresh_token),
            (
                "shared_key".to_string(),
                expected_hash.clone(),
                None,
                Some(expected_hash.clone())
            )
        );

        let ctx = OAuth2Verifier
            .verify(config.as_ref(), Some(&db), access_token)
            .await
            .unwrap()
            .expect("bridge access token should verify");
        assert_eq!(ctx.kind, AuthKind::OAuth2Token);
        assert_eq!(ctx.user_id, None);
        assert_eq!(ctx.token_kind.as_deref(), Some("oauth2_shared_key"));
        assert_eq!(ctx.shared_key_hash.as_deref(), Some(expected_hash.as_str()));
        assert!(ctx.has_scope(crate::auth::SCOPE_RUNTIME_READ));
        assert!(!ctx.has_scope(crate::auth::SCOPE_PROJECT_WRITE));
        assert!(!ctx.has_scope(crate::auth::SCOPE_ACCOUNT_MANAGE));

        let mut resp = TestClient::post("http://localhost/api/oauth/clients/list")
            .add_header("authorization", &format!("Bearer {}", access_token), true)
            .body("{}")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["error"], "insufficient_scope");
    }

    #[tokio::test]
    async fn bridge_issued_access_token_is_rejected_on_agent_path_without_updating_last_used() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Bridge App");
        let verifier = "bridge-code-verifier";
        let challenge = pkce_s256_challenge(verifier);
        let shared_key = "bridge-shared-secret";
        let expected_hash = bridge_shared_key_hash(shared_key).unwrap();
        let service = Service::new(build_router(config, db.clone()));

        let authorize_body = form_body(&[
            ("bridge", "shared_key"),
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("shared_key", shared_key),
        ]);
        let resp = post_form("http://localhost/oauth/authorize/bridge", authorize_body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        let location = location_header(&resp).expect("success redirect");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params.get("code").expect("code").clone();

        let exchange_body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", verifier),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", exchange_body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let (access_token_id, shared_key_hash, before_last_used): (
            String,
            Option<String>,
            Option<i64>,
        ) = db
            .conn_for_tests()
            .query_row(
                "SELECT id, shared_key_hash, last_used_at FROM oauth_access_tokens WHERE token_hash = ?1",
                [&hash_token(access_token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(shared_key_hash.as_deref(), Some(expected_hash.as_str()));
        assert!(before_last_used.is_none(), "precondition");

        let resp = TestClient::post("http://localhost/api/shell/agent/register")
            .add_header("authorization", &format!("Bearer {}", access_token), true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));

        let after_last_used: Option<i64> = db
            .conn_for_tests()
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&access_token_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_last_used, before_last_used);
    }

    #[tokio::test]
    async fn bridge_authorize_rejects_denied_scopes_and_allows_project_write_job_run() {
        let config = test_config(oauth2_enabled_bridge());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let denied_client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read project:write job:run account:manage",
        );
        let allowed_client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://allowed.example/callback",
            "runtime:read project:write job:run",
        );
        let service = Service::new(build_router(config, db.clone()));

        for scope in ["account:manage", "agent:register", "admin"] {
            let url =
                valid_bridge_authorize_url(&denied_client, "https://example.com/callback", scope);
            let before = auth_code_count(&db);
            let resp = TestClient::get(&url).send(&service).await;
            assert_eq!(resp.status_code, Some(StatusCode::FOUND));
            assert_eq!(auth_code_count(&db), before);
            let location = location_header(&resp).expect("invalid_scope redirect");
            let parsed = url::Url::parse(&location).unwrap();
            let params: std::collections::HashMap<String, String> =
                parsed.query_pairs().into_owned().collect();
            assert_eq!(
                params.get("error").map(String::as_str),
                Some("invalid_scope")
            );
        }

        let body = bridge_form_body(
            &allowed_client,
            "https://allowed.example/callback",
            "project:write job:run",
            "shared-key-with-write-run",
        );
        let before = auth_code_count(&db);
        let resp = post_form("http://localhost/oauth/authorize/bridge", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before + 1);
        let location = location_header(&resp).expect("success redirect");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params.get("code").expect("code");
        let record = auth_code_by_plaintext(&db, code);
        assert_eq!(record.scopes, "project:write job:run");
        assert_eq!(record.subject_kind, "shared_key");
    }

    #[tokio::test]
    async fn authorize_accepts_user_pat_for_code_issuance() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;

        assert!(code.starts_with("wc_oac_"));
        let record = auth_code_by_plaintext(&db, &code);
        assert_eq!(record.shared_key_hash, None);
    }

    #[tokio::test]
    async fn authorize_rejects_non_user_tokens_without_issuing_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        // Each token type must be rejected with 403, no redirect, no code issued.
        let tokens: Vec<String> = vec![
            {
                let (_record, token) = seed_access_token(&db, &client, &user, "runtime:read");
                token
            },
            seed_agent_token(&db, &user),
            seed_account_credential(&db, &user),
        ];
        for token in &tokens {
            let before = auth_code_count(&db);
            let resp = authorized_get(&url, token).send(&service).await;
            assert_eq!(
                resp.status_code,
                Some(StatusCode::FORBIDDEN),
                "token should be rejected"
            );
            assert_no_location(&resp);
            assert_eq!(auth_code_count(&db), before, "no code should be issued");
        }
    }

    #[tokio::test]
    async fn authorize_oauth2_disabled_returns_404_invalid_request_without_code() {
        let config = test_config(oauth2_disabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let before = auth_code_count(&db);
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let mut resp = authorized_get(&url, &token).send(&service).await;

        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["error"], "invalid_request");
        assert_eq!(body["error_description"], "OAuth2 is not enabled");
    }

    #[tokio::test]
    async fn authorize_rejects_unknown_client_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", "wc_client_missing"),
            ("redirect_uri", "https://example.com/callback"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_revoked_client_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        db.revoke_oauth_client(&client.id, chrono::Utc::now().timestamp())
            .unwrap();
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_missing_redirect_uri_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_empty_redirect_uri_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", ""),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_redirect_uri_mismatch_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://attacker.example/callback");

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_empty_client_id_without_redirect() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", ""),
            ("redirect_uri", "https://example.com/callback"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_direct_400(&service, &db, &url, &token).await;
    }

    #[tokio::test]
    async fn authorize_rejects_unsupported_response_type_with_redirect_after_client_validation() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "token"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "unsupported_response_type",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_empty_response_type_with_redirect_after_client_validation() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", ""),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "unsupported_response_type",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_requires_pkce_s256() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("code_challenge", "challenge-1"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_request",
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_plain_pkce_method() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "plain"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_request",
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_missing_code_challenge() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", "state-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_request",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_empty_code_challenge() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", "state-1"),
            ("code_challenge", ""),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_request",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_empty_code_challenge_method() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", ""),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_request",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_rejects_invalid_scope() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "project:write"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_scope",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn oauth_authorize_accepts_self_resource_base() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test/"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url_with_resource(
            &client,
            "https://example.com/callback",
            " https://example.test/ ",
        );

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_eq!(record.resource.as_deref(), Some("https://example.test"));
    }

    #[tokio::test]
    async fn oauth_authorize_accepts_self_resource_mcp_endpoint() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url_with_resource(
            &client,
            "https://example.com/callback",
            "https://example.test/mcp",
        );

        let (_resp, location, _parsed, code) = authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert!(
            location.contains("code=wc_oac_"),
            "ChatGPT MCP resource flow should return a code: {}",
            location
        );
        assert_eq!(record.resource.as_deref(), Some("https://example.test/mcp"));
    }

    #[tokio::test]
    async fn oauth_authorize_rejects_external_resource() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("resource", "https://api.example/resource"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "invalid_target",
            Some("state-1"),
        )
        .await;
    }

    #[tokio::test]
    async fn oauth_authorize_without_resource_still_works() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_eq!(record.resource, None);
    }

    #[tokio::test]
    async fn authorize_redirect_error_appends_with_ampersand_when_redirect_uri_has_query() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let redirect_uri = "https://client.example/callback?existing=1";
        let client =
            seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "token"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        let location = assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            redirect_uri,
            "unsupported_response_type",
            None,
        )
        .await;

        assert!(
            location.contains("?existing=1&error=unsupported_response_type"),
            "Location should append with &: {}",
            location
        );
        assert!(
            !location.contains("?existing=1?error="),
            "Location must not append a second ?: {}",
            location
        );
    }

    #[tokio::test]
    async fn authorize_redirect_error_preserves_decoded_state_semantics() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let state = "a b+c&d=1/中文";
        let url = authorize_url(&[
            ("response_type", "token"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("state", state),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        assert_authorize_redirect_error(
            &service,
            &db,
            &url,
            &token,
            "https://example.com/callback",
            "unsupported_response_type",
            Some(state),
        )
        .await;
    }

    #[tokio::test]
    async fn authorize_issues_code_and_redirects_with_state() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let before = auth_code_count(&db);
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (_resp, location, parsed, code) = authorize_success(&service, &db, &url, &token).await;

        assert_eq!(auth_code_count(&db), before + 1);
        assert!(location.starts_with("https://example.com/callback?code=wc_oac_"));
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("example.com"));
        assert_eq!(parsed.path(), "/callback");
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("code").map(String::as_str), Some(code.as_str()));
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    }

    #[tokio::test]
    async fn authorize_stores_only_code_hash() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_ne!(record.code_hash, code);
        assert_eq!(record.code_hash, hash_token(&code));
        assert!(
            !record.code_hash.contains(&code),
            "hash field must not contain plaintext code"
        );
    }

    #[tokio::test]
    async fn authorize_code_contains_user_client_redirect_scope_pkce_metadata() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "project:read runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "project:read runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        let (_resp, _location, _parsed, code) =
            authorize_success(&service, &db, &url, &token).await;
        let record = auth_code_by_plaintext(&db, &code);

        assert_eq!(record.client_id, client.client_id);
        assert_eq!(record.subject_kind, "managed_user");
        assert_eq!(record.subject_id, user.id);
        assert_eq!(record.user_id, Some(user.id.clone()));
        assert_eq!(record.redirect_uri, "https://example.com/callback");
        assert_eq!(record.scopes, "runtime:read project:read");
        assert_eq!(record.resource, None);
        assert_eq!(record.shared_key_hash, None);
        assert_eq!(record.code_challenge.as_deref(), Some("challenge-1"));
        assert_eq!(record.code_challenge_method.as_deref(), Some("S256"));
        assert_eq!(record.used_at, None);
        assert_eq!(record.revoked_at, None);
        assert!(record.expires_at > record.created_at);
    }

    #[tokio::test]
    async fn authorize_success_redirect_appends_with_ampersand_when_redirect_uri_has_query() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let redirect_uri = "https://client.example/callback?existing=1";
        let client =
            seed_client_with_redirects_and_scopes(&db, &user, redirect_uri, "runtime:read");
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, redirect_uri);

        let (_resp, location, parsed, _code) = authorize_success(&service, &db, &url, &token).await;
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();

        assert_eq!(params.get("existing").map(String::as_str), Some("1"));
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
        assert!(
            location.contains("?existing=1&code=wc_oac_"),
            "Location should append with &: {}",
            location
        );
        assert!(
            !location.contains("?existing=1?code="),
            "Location must not append a second ?: {}",
            location
        );
    }

    #[tokio::test]
    async fn authorize_success_does_not_return_access_or_refresh_token() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let url = valid_authorize_url(&client, "https://example.com/callback");

        let (mut resp, location, _parsed, _code) =
            authorize_success(&service, &db, &url, &token).await;
        let body = resp.take_string().await.unwrap_or_default();

        assert!(!location.contains("access_token"));
        assert!(!location.contains("refresh_token"));
        assert!(!body.contains("access_token"));
        assert!(!body.contains("refresh_token"));
    }

    #[tokio::test]
    async fn authorize_success_preserves_decoded_state_semantics() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let state = "a b+c&d=1/中文";
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", state),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ]);

        let (_resp, _location, parsed, _code) =
            authorize_success(&service, &db, &url, &token).await;
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("state").map(String::as_str), Some(state));
    }

    #[tokio::test]
    async fn authorize_success_code_can_be_exchanged_for_tokens() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let (client, secret) = seed_client(&db, &user, "Test App");
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let code_challenge = pkce_s256_challenge(code_verifier);
        let service = Service::new(build_router(config, db.clone()));
        let url = authorize_url(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", &code_challenge),
            ("code_challenge_method", "S256"),
        ]);

        let (_resp, _location, parsed, code) = authorize_success(&service, &db, &url, &token).await;
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));

        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", code_verifier),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert!(json["access_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_oat_"));
        assert!(json["refresh_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_ort_"));

        let record = auth_code_by_plaintext(&db, &code);
        assert!(record.used_at.is_some(), "authorization code consumed");
    }

    // -----------------------------------------------------------------------
    // Success path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn valid_authorization_code_grant_returns_tokens() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();
        assert!(access_token.starts_with("wc_oat_"));
        assert!(refresh_token.starts_with("wc_ort_"));
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
        assert_eq!(json["scope"], "runtime:read");
        assert!(access_token_shared_key_hash_by_plaintext(&db, access_token).is_none());
        assert!(refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).is_none());
        assert_eq!(
            access_token_subject_by_plaintext(&db, access_token),
            (
                "managed_user".to_string(),
                user.id.clone(),
                Some(user.id.clone()),
                None
            )
        );
        assert_eq!(
            refresh_token_subject_by_plaintext(&db, refresh_token),
            (
                "managed_user".to_string(),
                user.id.clone(),
                Some(user.id.clone()),
                None
            )
        );

        // Both tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before + 1, at_after, "one access token inserted");
        assert_eq!(rt_before + 1, rt_after, "one refresh token inserted");
    }

    #[tokio::test]
    async fn oauth_token_exchange_inherits_resource_from_code() {
        let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code_with_resource(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
            Some("https://example.test/mcp"),
        );

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();

        assert_eq!(
            access_token_resource_by_plaintext(&db, access_token).as_deref(),
            Some("https://example.test/mcp")
        );
        assert_eq!(
            refresh_token_resource_by_plaintext(&db, refresh_token).as_deref(),
            Some("https://example.test/mcp")
        );
    }

    #[tokio::test]
    async fn oauth_token_exchange_inherits_bridge_shared_key_hash_from_code() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code_with_shared_key_hash(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
            "hash-a",
        );

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();

        assert_eq!(
            access_token_shared_key_hash_by_plaintext(&db, access_token).as_deref(),
            Some("hash-a")
        );
        assert_eq!(
            refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).as_deref(),
            Some("hash-a")
        );
        assert_eq!(
            access_token_subject_by_plaintext(&db, access_token),
            (
                "shared_key".to_string(),
                "hash-a".to_string(),
                None,
                Some("hash-a".to_string())
            )
        );
        assert_eq!(
            refresh_token_subject_by_plaintext(&db, refresh_token),
            (
                "shared_key".to_string(),
                "hash-a".to_string(),
                None,
                Some("hash-a".to_string())
            )
        );
    }

    #[tokio::test]
    async fn returned_tokens_are_stored_only_as_hashes() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        let json: serde_json::Value = resp.take_json().await.unwrap();
        let at = json["access_token"].as_str().unwrap();
        let rt = json["refresh_token"].as_str().unwrap();

        // Verify hashes are stored, not plaintext.
        let conn = db.conn_for_tests();
        let stored_at_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_access_tokens ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_at_hash, at);
        assert_eq!(stored_at_hash, hash_token(at));

        let stored_rt_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_refresh_tokens ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_rt_hash, rt);
        assert_eq!(stored_rt_hash, hash_token(rt));
    }

    #[tokio::test]
    async fn authorization_code_is_marked_used() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_code_record, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        // Verify the code is now used — the second_exchange_of_same_code_fails
        // test covers this directly. Here we just confirm the exchange succeeded.
        assert_eq!(resp.status_code, Some(StatusCode::OK));
    }

    #[tokio::test]
    async fn second_exchange_of_same_code_fails() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db.clone()));

        // First exchange succeeds.
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Second exchange fails.
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    // -----------------------------------------------------------------------
    // Client authentication
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn token_endpoint_rejects_invalid_client_credentials() {
        let config = test_config(oauth2_enabled_no_pkce());

        // Case 1: wrong client_secret.
        {
            let (_tmp, db) = test_db();
            let user = seed_user(&db, "alice");
            let (client, _) = seed_client(&db, &user, "Test App");
            let (_, code) = seed_auth_code(
                &db,
                &client,
                &user,
                "https://example.com/callback",
                "runtime:read",
                None,
                None,
            );
            let service = Service::new(build_router(config.clone(), db));
            let body = form_body(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "https://example.com/callback"),
                ("client_id", &client.client_id),
                ("client_secret", "wrong-secret"),
            ]);
            let mut resp = post_form("http://localhost/oauth/token", body)
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
            let json: serde_json::Value = resp.take_json().await.unwrap();
            assert_eq!(json["error"], "invalid_client");
        }
        // Case 2: unknown client_id.
        {
            let (_tmp, db) = test_db();
            let service = Service::new(build_router(config.clone(), db));
            let body = form_body(&[
                ("grant_type", "authorization_code"),
                ("code", "wc_oac_dummy"),
                ("redirect_uri", "https://example.com/callback"),
                ("client_id", "wc_client_nonexistent"),
                ("client_secret", "some-secret"),
            ]);
            let mut resp = post_form("http://localhost/oauth/token", body)
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
            let json: serde_json::Value = resp.take_json().await.unwrap();
            assert_eq!(json["error"], "invalid_client");
        }
        // Case 3: revoked client.
        {
            let (_tmp, db) = test_db();
            let user = seed_user(&db, "alice");
            let (client, secret) = seed_client(&db, &user, "Test App");
            let now = chrono::Utc::now().timestamp();
            db.revoke_oauth_client(&client.id, now).unwrap();
            let service = Service::new(build_router(config.clone(), db));
            let body = form_body(&[
                ("grant_type", "authorization_code"),
                ("code", "wc_oac_dummy"),
                ("redirect_uri", "https://example.com/callback"),
                ("client_id", &client.client_id),
                ("client_secret", &secret),
            ]);
            let mut resp = post_form("http://localhost/oauth/token", body)
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
            let json: serde_json::Value = resp.take_json().await.unwrap();
            assert_eq!(json["error"], "invalid_client");
        }
    }

    // -----------------------------------------------------------------------
    // Grant/code validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn unsupported_grant_type_returns_error() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "client_credentials")]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "unsupported_grant_type");
    }

    #[tokio::test]
    async fn missing_code_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn unknown_code_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", "wc_oac_nonexistent"),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn expired_code_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        // Create an already-expired code.
        let now = chrono::Utc::now().timestamp();
        let plaintext_code = generate_oauth_authorization_code();
        let code_hash = hash_token(&plaintext_code);
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash,
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now - 600,
            expires_at: now - 1, // already expired
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &record.code_hash)
            .unwrap();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &plaintext_code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn redirect_uri_mismatch_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://evil.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn client_id_mismatch_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        // Create a second client. The code belongs to client1 but we exchange
        // with client2's credentials. Client auth succeeds (client2's secret
        // matches), but the code's client_id doesn't match client2.
        let (client2, secret2) = seed_client(&db, &user, "Other App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client2.client_id),
            ("client_secret", &secret2),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn client_id_mismatch_consumes_code_but_no_tokens() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _secret) = seed_client(&db, &user, "Test App");
        let (code_record, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let (client2, secret2) = seed_client(&db, &user, "Other App");

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client2.client_id),
            ("client_secret", &secret2),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // Code SHOULD be consumed — validation failure.
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            used_at.is_some(),
            "code should be consumed on client_id mismatch"
        );
        drop(conn);

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access token on client_id mismatch");
        assert_eq!(
            rt_before, rt_after,
            "no refresh token on client_id mismatch"
        );
    }

    // -----------------------------------------------------------------------
    // PKCE
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn valid_s256_verifier_succeeds() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        // Generate a code_verifier and compute its S256 challenge.
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(code_verifier.as_bytes());
        let code_challenge = URL_SAFE_NO_PAD.encode(digest);

        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some(&code_challenge),
            Some("S256"),
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", code_verifier),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert!(json["access_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_oat_"));
    }

    #[tokio::test]
    async fn wrong_verifier_returns_invalid_grant() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(code_verifier.as_bytes());
        let code_challenge = URL_SAFE_NO_PAD.encode(digest);

        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some(&code_challenge),
            Some("S256"),
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", "wrong-verifier-value-here"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn missing_verifier_when_pkce_required_returns_error() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(code_verifier.as_bytes());
        let code_challenge = URL_SAFE_NO_PAD.encode(digest);

        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some(&code_challenge),
            Some("S256"),
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            // no code_verifier
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn plain_challenge_method_rejected() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some("some-challenge"),
            Some("plain"),
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", "some-challenge"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    // -----------------------------------------------------------------------
    // OAuth2 disabled
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth2_disabled_returns_503() {
        let config = test_config(oauth2_disabled());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "authorization_code")]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "server_error");
    }

    // -----------------------------------------------------------------------
    // PKCE helper
    // -----------------------------------------------------------------------

    #[test]
    fn verify_pkce_s256_works() {
        // RFC 7636 Appendix B test vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(verify_pkce_s256(verifier, challenge));
        assert!(!verify_pkce_s256("wrong", challenge));
        assert!(!verify_pkce_s256(verifier, "wrong"));
    }

    // -----------------------------------------------------------------------
    // No-store headers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn successful_token_response_has_no_store_headers() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let cc = resp
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-store");
        let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
        assert_eq!(pragma, "no-cache");
    }

    #[tokio::test]
    async fn error_response_has_no_store_headers() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "client_credentials")]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let cc = resp
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-store");
        let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
        assert_eq!(pragma, "no-cache");
    }

    // -----------------------------------------------------------------------
    // Content-Type enforcement
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn missing_content_type_is_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "authorization_code")]);
        // No content-type header.
        let mut resp = TestClient::post("http://localhost/oauth/token")
            .body(body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn json_content_type_is_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "authorization_code")]);
        let mut resp = TestClient::post("http://localhost/oauth/token")
            .add_header("content-type", "application/json", true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn form_urlencoded_with_charset_is_accepted() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = TestClient::post("http://localhost/oauth/token")
            .add_header(
                "content-type",
                "application/x-www-form-urlencoded; charset=utf-8",
                true,
            )
            .body(body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
    }

    // -----------------------------------------------------------------------
    // Request body size limit
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oversized_content_length_is_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("grant_type", "authorization_code")]);
        let mut resp = TestClient::post("http://localhost/oauth/token")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("content-length", "999999", true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn oversized_actual_body_is_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        // Build a body that exceeds 16 KiB.
        let big_value = "x".repeat(17 * 1024);
        let body = format!("grant_type=authorization_code&code={}", big_value);
        let mut resp = TestClient::post("http://localhost/oauth/token")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn normal_small_form_still_works() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
    }

    // -----------------------------------------------------------------------
    // Post-consume semantics (code consumed on mismatch)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn wrong_client_secret_does_not_consume_code() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _) = seed_client(&db, &user, "Test App");
        let (code_record, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", "wrong-secret"),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

        // Code should NOT be consumed — wrong secret is rejected before exchange.
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            used_at.is_none(),
            "code should not be consumed on wrong secret"
        );
        drop(conn);

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access token should be inserted");
        assert_eq!(rt_before, rt_after, "no refresh token should be inserted");
    }

    #[tokio::test]
    async fn redirect_uri_mismatch_consumes_code() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (code_record, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            None,
            None,
        );

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://evil.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // Code SHOULD be consumed — validation failure.
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            used_at.is_some(),
            "code should be consumed on redirect_uri mismatch"
        );
        drop(conn);

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(
            at_before, at_after,
            "no access token on redirect_uri mismatch"
        );
        assert_eq!(
            rt_before, rt_after,
            "no refresh token on redirect_uri mismatch"
        );
    }

    #[tokio::test]
    async fn pkce_mismatch_consumes_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(code_verifier.as_bytes());
        let code_challenge = URL_SAFE_NO_PAD.encode(digest);

        let (code_record, code) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some(&code_challenge),
            Some("S256"),
        );

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "https://example.com/callback"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("code_verifier", "wrong-verifier"),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // Code SHOULD be consumed — validation failure.
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            used_at.is_some(),
            "code should be consumed on PKCE mismatch"
        );
        drop(conn);

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access token on PKCE mismatch");
        assert_eq!(rt_before, rt_after, "no refresh token on PKCE mismatch");
    }

    // -----------------------------------------------------------------------
    // refresh_token grant — success path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn valid_refresh_token_grant_returns_new_tokens() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert!(json["access_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_oat_"));
        assert!(json["refresh_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_ort_"));
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
        assert_eq!(json["scope"], "runtime:read");
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();
        assert_eq!(
            access_token_subject_by_plaintext(&db, access_token),
            (
                "managed_user".to_string(),
                user.id.clone(),
                Some(user.id.clone()),
                None
            )
        );
        assert_eq!(
            refresh_token_subject_by_plaintext(&db, refresh_token),
            (
                "managed_user".to_string(),
                user.id.clone(),
                Some(user.id.clone()),
                None
            )
        );

        // Old refresh token should be revoked.
        let conn = db.conn_for_tests();
        let revoked_at: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&old_rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(revoked_at.is_some(), "old refresh token should be revoked");

        let last_used_at: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&old_rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used_at.is_some(),
            "old refresh token should have last_used_at set"
        );

        // New refresh token should have rotated_from_id.
        let rotated_from: Option<String> = conn
            .query_row(
                "SELECT rotated_from_id FROM oauth_refresh_tokens WHERE rotated_from_id IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            rotated_from.as_deref(),
            Some(old_rt.id.as_str()),
            "new refresh token should reference old token"
        );
        drop(conn);

        // Both new tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before + 1, at_after, "one new access token inserted");
        assert_eq!(rt_before + 1, rt_after, "one new refresh token inserted");
    }

    #[tokio::test]
    async fn oauth_refresh_token_inherits_resource() {
        let config = test_config(oauth2_enabled_no_pkce_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_old_rt, old_rt_plaintext) = seed_refresh_token_with_resource(
            &db,
            &client,
            &user,
            "runtime:read",
            Some("https://example.test/mcp"),
        );

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();

        assert_eq!(
            access_token_resource_by_plaintext(&db, access_token).as_deref(),
            Some("https://example.test/mcp")
        );
        assert_eq!(
            refresh_token_resource_by_plaintext(&db, refresh_token).as_deref(),
            Some("https://example.test/mcp")
        );
    }

    #[tokio::test]
    async fn oauth_refresh_token_preserves_bridge_shared_key_hash() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_old_rt, old_rt_plaintext) =
            seed_refresh_token_with_shared_key_hash(&db, &client, &user, "runtime:read", "hash-a");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        let access_token = json["access_token"].as_str().unwrap();
        let refresh_token = json["refresh_token"].as_str().unwrap();

        assert_eq!(
            access_token_shared_key_hash_by_plaintext(&db, access_token).as_deref(),
            Some("hash-a")
        );
        assert_eq!(
            refresh_token_shared_key_hash_by_plaintext(&db, refresh_token).as_deref(),
            Some("hash-a")
        );
        assert_eq!(
            access_token_subject_by_plaintext(&db, access_token),
            (
                "shared_key".to_string(),
                "hash-a".to_string(),
                None,
                Some("hash-a".to_string())
            )
        );
        assert_eq!(
            refresh_token_subject_by_plaintext(&db, refresh_token),
            (
                "shared_key".to_string(),
                "hash-a".to_string(),
                None,
                Some("hash-a".to_string())
            )
        );
    }

    #[tokio::test]
    async fn refresh_token_new_tokens_stored_only_as_hashes() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;

        let json: serde_json::Value = resp.take_json().await.unwrap();
        let at = json["access_token"].as_str().unwrap();
        let rt = json["refresh_token"].as_str().unwrap();

        let conn = db.conn_for_tests();
        let stored_at_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_access_tokens ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_at_hash, at);
        assert_eq!(stored_at_hash, hash_token(at));

        let stored_rt_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_refresh_tokens WHERE rotated_from_id IS NOT NULL ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_rt_hash, rt);
        assert_eq!(stored_rt_hash, hash_token(rt));
    }

    // -----------------------------------------------------------------------
    // refresh_token grant — rotation / replay
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_token_cannot_be_reused_after_rotation() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));

        // First refresh succeeds.
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Second refresh with same old token fails.
        let (at_before, rt_before) = oauth_token_counts(&db);
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");

        // No additional tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no extra access token on replay");
        assert_eq!(rt_before, rt_after, "no extra refresh token on replay");
    }

    // -----------------------------------------------------------------------
    // refresh_token grant — client authentication
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_token_wrong_secret_does_not_rotate() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _) = seed_client(&db, &user, "Test App");
        let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", "wrong-secret"),
        ]);
        let resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));

        // Old refresh token should NOT be revoked.
        let conn = db.conn_for_tests();
        let revoked_at: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&old_rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            revoked_at.is_none(),
            "old refresh token should not be revoked on wrong secret"
        );
        drop(conn);

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access token on wrong secret");
        assert_eq!(rt_before, rt_after, "no refresh token on wrong secret");
    }

    #[tokio::test]
    async fn refresh_token_unknown_client_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", "wc_ort_dummy"),
            ("client_id", "wc_client_nonexistent"),
            ("client_secret", "some-secret"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn refresh_token_revoked_client_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_client(&client.id, now).unwrap();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", "wc_ort_dummy"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }

    // -----------------------------------------------------------------------
    // refresh_token grant — grant validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn missing_refresh_token_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn unknown_refresh_token_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", "wc_ort_nonexistent"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn expired_refresh_token_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        // Create an already-expired refresh token.
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_refresh_token();
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
            created_at: now - 600,
            expires_at: now - 1, // already expired
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn revoked_refresh_token_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (old_rt, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        // Revoke the refresh token.
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_refresh_token(&old_rt.id, now).unwrap();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn refresh_token_client_id_mismatch_returns_invalid_grant() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _) = seed_client(&db, &user, "Test App");
        let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        // Create a second client and try to use client1's refresh token.
        let (client2, secret2) = seed_client(&db, &user, "Other App");

        let (at_before, rt_before) = oauth_token_counts(&db);
        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client2.client_id),
            ("client_secret", &secret2),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_grant");

        // No tokens should be inserted.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access token on client mismatch");
        assert_eq!(rt_before, rt_after, "no refresh token on client mismatch");
    }

    // -----------------------------------------------------------------------
    // refresh_token grant — scope rejection
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_token_scope_parameter_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, old_rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
            ("scope", "runtime:read"),
        ]);
        let mut resp = post_form("http://localhost/oauth/token", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — success path
    // -----------------------------------------------------------------------

    fn post_revoke(url: &str, body: String) -> salvo::test::RequestBuilder {
        TestClient::post(url)
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body(body)
    }

    #[tokio::test]
    async fn revoke_access_token_success() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
        let (rt, _rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        // Response must not disclose token state.
        assert_eq!(json, serde_json::json!({}));

        // Access token should be revoked.
        let conn = db.conn_for_tests();
        let at_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(at_revoked.is_some(), "access token should be revoked");

        // Refresh token should NOT be affected.
        let rt_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rt_revoked.is_none(), "refresh token should not be affected");
    }

    #[tokio::test]
    async fn revoke_refresh_token_success() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, _at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
        let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &rt_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Refresh token should be revoked.
        let conn = db.conn_for_tests();
        let rt_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rt_revoked.is_some(), "refresh token should be revoked");

        // Access token should NOT be affected.
        let at_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(at_revoked.is_none(), "access token should not be affected");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — idempotent
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_token_is_idempotent() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));

        // First revoke.
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Second revoke — same token.
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // revoked_at should still be Some.
        let conn = db.conn_for_tests();
        let revoked_at: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(revoked_at.is_some(), "token should still be revoked");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — unknown token
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_unknown_token_returns_200() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let (at_before, rt_before) = oauth_token_counts(&db);

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", "wc_oat_nonexistent"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // No tokens should be inserted or modified.
        let (at_after, rt_after) = oauth_token_counts(&db);
        assert_eq!(at_before, at_after, "no access tokens added");
        assert_eq!(rt_before, rt_after, "no refresh tokens added");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — wrong client
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_token_belonging_to_other_client_returns_200_but_does_not_revoke() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client_a, _secret_a) = seed_client(&db, &user, "App A");
        let (client_b, secret_b) = seed_client(&db, &user, "App B");
        let (at, at_plaintext) = seed_access_token(&db, &client_a, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        // Client B tries to revoke client A's token.
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client_b.client_id),
            ("client_secret", &secret_b),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Client A's token should NOT be revoked.
        let conn = db.conn_for_tests();
        let revoked_at: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            revoked_at.is_none(),
            "token belonging to other client should not be revoked"
        );
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — client authentication errors
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_wrong_client_secret_returns_invalid_client_and_does_not_revoke() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _secret) = seed_client(&db, &user, "Test App");
        let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", "wrong-secret"),
        ]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");

        // Token should NOT be revoked.
        let conn = db.conn_for_tests();
        let revoked_at: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            revoked_at.is_none(),
            "token should not be revoked on wrong secret"
        );
    }

    #[tokio::test]
    async fn revoke_unknown_client_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("token", "wc_oat_dummy"),
            ("client_id", "wc_client_nonexistent"),
            ("client_secret", "some-secret"),
        ]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn revoke_revoked_client_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        db.revoke_oauth_client(&client.id, now).unwrap();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("token", "wc_oat_dummy"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — request validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_missing_token_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("client_id", &client.client_id), ("client_secret", &secret)]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn revoke_missing_client_id_returns_invalid_request() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("token", "wc_oat_dummy"), ("client_secret", "some-secret")]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn revoke_missing_client_secret_returns_invalid_client() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, _secret) = seed_client(&db, &user, "Test App");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("token", "wc_oat_dummy"), ("client_id", &client.client_id)]);
        let mut resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn revoke_json_content_type_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("token", "wc_oat_dummy")]);
        let mut resp = TestClient::post("http://localhost/oauth/revoke")
            .add_header("content-type", "application/json", true)
            .body(body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn revoke_missing_content_type_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[("token", "wc_oat_dummy")]);
        let mut resp = TestClient::post("http://localhost/oauth/revoke")
            .body(body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    #[tokio::test]
    async fn revoke_oversized_body_rejected() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let big_value = "x".repeat(17 * 1024);
        let body = format!("token={}", big_value);
        let mut resp = TestClient::post("http://localhost/oauth/revoke")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body(body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
        let json: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — token_type_hint
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_access_token_hint_only_revokes_access_token() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
        let (rt, _rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &at_plaintext),
            ("token_type_hint", "access_token"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        let conn = db.conn_for_tests();
        let at_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(at_revoked.is_some(), "access token should be revoked");

        let rt_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rt_revoked.is_none(), "refresh token should not be affected");
    }

    #[tokio::test]
    async fn revoke_refresh_token_hint_only_revokes_refresh_token() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, _at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");
        let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &rt_plaintext),
            ("token_type_hint", "refresh_token"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        let conn = db.conn_for_tests();
        let rt_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rt_revoked.is_some(), "refresh token should be revoked");

        let at_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(at_revoked.is_none(), "access token should not be affected");
    }

    #[tokio::test]
    async fn revoke_unknown_hint_attempts_both() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        // Seed a refresh token and try to revoke it with an unknown hint.
        let (rt, rt_plaintext) = seed_refresh_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &rt_plaintext),
            ("token_type_hint", "unknown_type"),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Refresh token should be revoked (both types are tried).
        let conn = db.conn_for_tests();
        let rt_revoked: Option<i64> = conn
            .query_row(
                "SELECT revoked_at FROM oauth_refresh_tokens WHERE id = ?1",
                [&rt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            rt_revoked.is_some(),
            "refresh token should be revoked with unknown hint"
        );
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — no-store headers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_success_has_no_store_headers() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (_, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let cc = resp
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-store");
        let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
        assert_eq!(pragma, "no-cache");
    }

    #[tokio::test]
    async fn revoke_error_has_no_store_headers() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();

        let service = Service::new(build_router(config, db));
        let body = form_body(&[
            ("token", "wc_oat_dummy"),
            ("client_id", "wc_client_nonexistent"),
            ("client_secret", "some-secret"),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let cc = resp
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-store");
        let pragma = resp.headers().get("pragma").unwrap().to_str().unwrap();
        assert_eq!(pragma, "no-cache");
    }

    // -----------------------------------------------------------------------
    // POST /oauth/revoke — last_used_at not updated
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn revoke_does_not_update_last_used_at() {
        let config = test_config(oauth2_enabled_no_pkce());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "Test App");
        let (at, at_plaintext) = seed_access_token(&db, &client, &user, "runtime:read");

        let service = Service::new(build_router(config, db.clone()));
        let body = form_body(&[
            ("token", &at_plaintext),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]);
        let resp = post_revoke("http://localhost/oauth/revoke", body)
            .send(&service)
            .await;

        assert_eq!(resp.status_code, Some(StatusCode::OK));

        let conn = db.conn_for_tests();
        let last_used_at: Option<i64> = conn
            .query_row(
                "SELECT last_used_at FROM oauth_access_tokens WHERE id = ?1",
                [&at.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            last_used_at.is_none(),
            "revoke should not update last_used_at"
        );
    }

    // -----------------------------------------------------------------------
    // GET /.well-known/oauth-protected-resource
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth_protected_resource_metadata_is_public() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let ct = resp
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("application/json"),
            "expected application/json, got {}",
            ct
        );
    }

    #[tokio::test]
    async fn oauth_protected_resource_metadata_fields() {
        let mut oauth2 = oauth2_enabled();
        oauth2.issuer = Some("https://codex.example.com".to_string());
        let config = test_config(oauth2);
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();

        // resource is an absolute URL
        let resource = body["resource"].as_str().unwrap();
        assert!(
            resource.starts_with("https://"),
            "resource should be absolute URL, got {}",
            resource
        );
        assert_eq!(resource, "https://codex.example.com");

        // authorization_servers is an array whose first element matches issuer
        let auth_servers = body["authorization_servers"].as_array().unwrap();
        assert_eq!(auth_servers.len(), 1);
        assert_eq!(auth_servers[0], "https://codex.example.com");

        // bearer_methods_supported == ["header"]
        let methods = body["bearer_methods_supported"].as_array().unwrap();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0], "header");

        // scopes_supported is a non-empty array
        let scopes = body["scopes_supported"].as_array().unwrap();
        assert!(!scopes.is_empty(), "scopes_supported should be non-empty");
        // Must contain at least runtime:read
        assert!(
            scopes.iter().any(|s| s == "runtime:read"),
            "scopes_supported should contain runtime:read"
        );

        // resource_name
        assert_eq!(body["resource_name"], "WebCodex");
    }

    #[tokio::test]
    async fn oauth_protected_resource_metadata_disabled_returns_404() {
        let config = test_config(oauth2_disabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn oauth_protected_resource_metadata_no_issuer_fallback() {
        // When issuer is None, resource falls back to "http://localhost"
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["resource"], "http://localhost");
        let auth_servers = body["authorization_servers"].as_array().unwrap();
        assert_eq!(auth_servers[0], "http://localhost");
    }

    #[tokio::test]
    async fn oauth_authorization_server_metadata_is_public() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let ct = resp
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("application/json"),
            "expected application/json, got {}",
            ct
        );
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["issuer"], "http://localhost");
        assert_eq!(
            body["authorization_endpoint"],
            "http://localhost/oauth/authorize"
        );
    }

    #[tokio::test]
    async fn oauth_authorization_server_metadata_fields() {
        let mut oauth2 = oauth2_enabled();
        oauth2.issuer = Some("https://codex.example.com".to_string());
        let config = test_config(oauth2);
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();

        assert_eq!(body["issuer"], "https://codex.example.com");
        assert_eq!(
            body["authorization_endpoint"],
            "https://codex.example.com/oauth/authorize"
        );
        assert_eq!(
            body["token_endpoint"],
            "https://codex.example.com/oauth/token"
        );
        assert_eq!(
            body["revocation_endpoint"],
            "https://codex.example.com/oauth/revoke"
        );
        assert_eq!(
            body["response_types_supported"],
            serde_json::json!(["code"])
        );
        assert!(body["grant_types_supported"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "authorization_code"));
        assert!(body["grant_types_supported"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "refresh_token"));
        assert_eq!(
            body["code_challenge_methods_supported"],
            serde_json::json!(["S256"])
        );
        assert_eq!(
            body["token_endpoint_auth_methods_supported"],
            serde_json::json!(["client_secret_post"])
        );
        let auth_methods = body["token_endpoint_auth_methods_supported"]
            .as_array()
            .unwrap();
        assert!(
            !auth_methods.iter().any(|v| v == "none"),
            "metadata must not advertise unsupported public-client auth"
        );
        assert_eq!(
            body["scopes_supported"],
            serde_json::json!(oauth_scopes_supported())
        );

        assert!(
            body.get("jwks_uri").is_none(),
            "metadata must not advertise JWKS"
        );
        assert!(
            body.get("userinfo_endpoint").is_none(),
            "metadata must not advertise OIDC userinfo"
        );
    }

    #[tokio::test]
    async fn oauth_authorization_server_metadata_trims_trailing_issuer_slash() {
        let mut oauth2 = oauth2_enabled();
        oauth2.issuer = Some("https://codex.example.com/".to_string());
        let config = test_config(oauth2);
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();

        assert_eq!(body["issuer"], "https://codex.example.com/");
        assert_eq!(
            body["authorization_endpoint"],
            "https://codex.example.com/oauth/authorize"
        );
        assert_eq!(
            body["token_endpoint"],
            "https://codex.example.com/oauth/token"
        );
        assert_eq!(
            body["revocation_endpoint"],
            "https://codex.example.com/oauth/revoke"
        );
    }

    #[tokio::test]
    async fn oauth_authorization_server_metadata_disabled_returns_404() {
        let config = test_config(oauth2_disabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["error"], "OAuth2 is not enabled");
    }

    #[tokio::test]
    async fn openid_configuration_not_exposed() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let resp = TestClient::get("http://localhost/.well-known/openid-configuration")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn oauth_protected_resource_metadata_scopes_exclude_agent() {
        // Agent scopes must not appear in scopes_supported because OAuth2
        // tokens are rejected on agent transport surfaces.
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let service = Service::new(build_router(config, db));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-protected-resource")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let scopes = body["scopes_supported"].as_array().unwrap();
        for scope in scopes {
            let s = scope.as_str().unwrap();
            assert!(
                !s.starts_with("agent:"),
                "agent scope '{}' must not appear in scopes_supported",
                s
            );
        }
        // admin is a bootstrap scope, not for OAuth2 delegation
        assert!(
            !scopes.iter().any(|s| s == "admin"),
            "admin scope must not appear in scopes_supported"
        );
    }

    // -----------------------------------------------------------------------
    // Test helpers for client management + authorize browser UX
    // -----------------------------------------------------------------------

    fn authorized_post_json(url: &str, body: String, token: &str) -> salvo::test::RequestBuilder {
        TestClient::post(url)
            .add_header("authorization", format!("Bearer {}", token), true)
            .add_header("content-type", "application/json", true)
            .body(body)
    }

    fn set_cookie_value(resp: &Response, name: &str) -> Option<String> {
        for v in resp.headers.get_all("set-cookie") {
            if let Ok(s) = v.to_str() {
                let prefix = format!("{}=", name);
                if let Some(rest) = s.strip_prefix(&prefix) {
                    if let Some(val) = rest.split(';').next() {
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn post_form_with_cookie(url: &str, body: String, cookie: &str) -> salvo::test::RequestBuilder {
        TestClient::post(url)
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("cookie", cookie, true)
            .body(body)
    }

    fn create_client_json(name: &str, redirect_uris: &[&str], scopes: Option<&[&str]>) -> String {
        let mut obj = serde_json::json!({
            "name": name,
            "redirect_uris": redirect_uris,
        });
        if let Some(s) = scopes {
            obj["allowed_scopes"] = serde_json::json!(s);
        }
        obj.to_string()
    }

    fn return_to_for(client: &OAuthClientRecord, redirect_uri: &str) -> String {
        let params: &[(&str, &str)] = &[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
        ];
        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("/oauth/authorize?{}", query)
    }

    fn consent_form_body(client: &OAuthClientRecord, redirect_uri: &str, decision: &str) -> String {
        form_body(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("decision", decision),
        ])
    }

    fn consent_form_body_with_resource(
        client: &OAuthClientRecord,
        redirect_uri: &str,
        decision: &str,
        resource: &str,
    ) -> String {
        form_body(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", redirect_uri),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("resource", resource),
            ("decision", decision),
        ])
    }

    // -----------------------------------------------------------------------
    // OAuth client management API tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth_client_create_returns_client_secret_once() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("ChatGPT Action", &["https://example.com/callback"], None),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let secret = body["client_secret"]
            .as_str()
            .expect("client_secret returned");
        assert!(secret.starts_with("wc_csec_"));
        assert!(body["client"]["client_id"]
            .as_str()
            .unwrap()
            .starts_with("wc_client_"));

        // list must NOT return the secret.
        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/list",
            "{}".to_string(),
            &token,
        )
        .send(&service)
        .await;
        let list_body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(list_body["clients"].as_array().unwrap().len() >= 1);
        let clients = list_body["clients"].as_array().unwrap();
        assert!(clients.iter().all(|c| c.get("client_secret").is_none()));
        assert!(clients
            .iter()
            .all(|c| c.get("client_secret_hash").is_none()));
    }

    #[tokio::test]
    async fn oauth_client_create_hashes_secret_only() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Hashed", &["https://example.com/callback"], None),
            &token,
        )
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let secret = body["client_secret"].as_str().unwrap();
        let client_id = body["client"]["client_id"].as_str().unwrap();

        let stored_hash: String = {
            let conn = db.conn_for_tests();
            conn.query_row(
                "SELECT client_secret_hash FROM oauth_clients WHERE client_id = ?1",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_ne!(stored_hash, secret);
        assert_eq!(stored_hash, hash_token(secret));
    }

    #[tokio::test]
    async fn oauth_client_create_validates_redirect_uris() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        // not a URL
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Bad", &["not-a-url"], None),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // http non-loopback rejected
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Bad", &["http://example.com/cb"], None),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // http loopback accepted
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json(
                "Local",
                &["http://127.0.0.1:3000/cb", "http://localhost:3000/cb"],
                None,
            ),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // empty array rejected
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Empty", &[], None),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn oauth_client_create_trims_redirect_uris_before_storing() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        // redirect_uris with leading/trailing whitespace, plus a trim-duplicate.
        let body = serde_json::json!({
            "name": "Trimmed",
            "redirect_uris": [
                "  https://example.com/callback  ",
                "https://example.com/callback",
                "\thttp://127.0.0.1:3000/cb\t",
            ],
        })
        .to_string();
        let mut resp =
            authorized_post_json("http://localhost/api/oauth/clients/create", body, &token)
                .send(&service)
                .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let client_id = body["client"]["client_id"].as_str().unwrap();
        let returned: Vec<String> = body["client"]["redirect_uris"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        // The trim-duplicate must collapse; whitespace must be stripped.
        assert_eq!(
            returned,
            vec![
                "https://example.com/callback".to_string(),
                "http://127.0.0.1:3000/cb".to_string(),
            ]
        );

        // Verify the stored record has trimmed, deduped values.
        let stored: String = {
            let conn = db.conn_for_tests();
            conn.query_row(
                "SELECT redirect_uris FROM oauth_clients WHERE client_id = ?1",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stored,
            "https://example.com/callback\nhttp://127.0.0.1:3000/cb"
        );
    }

    #[tokio::test]
    async fn oauth_client_create_defaults_to_full_delegable_scopes() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Default Scopes", &["https://example.com/callback"], None),
            &token,
        )
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let scopes: Vec<String> = body["client"]["allowed_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            scopes,
            oauth_scopes_supported()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn oauth_client_create_rejects_unknown_scopes() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json(
                "Bad Scopes",
                &["https://example.com/callback"],
                Some(&["runtime:read", "bogus:scope"]),
            ),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

        // agent:poll and admin are rejected for OAuth delegation
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json(
                "Agent Scope",
                &["https://example.com/callback"],
                Some(&["agent:poll"]),
            ),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn oauth_client_list_does_not_return_secret_hash() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));
        seed_client(&db, &user, "Seeded");

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/list",
            "{}".to_string(),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let clients = body["clients"].as_array().unwrap();
        assert!(!clients.is_empty());
        for c in clients {
            assert!(c.get("client_secret_hash").is_none(), "secret hash leaked");
            assert!(c.get("client_secret").is_none(), "secret leaked");
        }
    }

    #[tokio::test]
    async fn oauth_client_revoke_revokes_client() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("To Revoke", &["https://example.com/callback"], None),
            &token,
        )
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let client_id = body["client"]["client_id"].as_str().unwrap().to_string();

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/revoke",
            serde_json::json!({ "client_id": client_id }).to_string(),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);

        // Idempotent: revoke again still success.
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/revoke",
            serde_json::json!({ "client_id": client_id }).to_string(),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // The revoked client is no longer returned by the active lookup.
        assert!(db
            .get_oauth_client_by_client_id(&client_id)
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn oauth_client_revoke_revokes_related_tokens() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Tokens", &["https://example.com/callback"], None),
            &token,
        )
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let client_id = body["client"]["client_id"].as_str().unwrap().to_string();
        // Build a temporary client record handle for seeding helpers.
        let client = db
            .list_oauth_clients()
            .unwrap()
            .into_iter()
            .find(|c| c.client_id == client_id)
            .unwrap();

        let (_at_rec, _at_plain) = seed_access_token(&db, &client, &user, "runtime:read");
        let (_rt_rec, _rt_plain) = seed_refresh_token(&db, &client, &user, "runtime:read");
        let (_ac_rec, _ac_plain) = seed_auth_code(
            &db,
            &client,
            &user,
            "https://example.com/callback",
            "runtime:read",
            Some("challenge-1"),
            Some("S256"),
        );

        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/revoke",
            serde_json::json!({ "client_id": client_id }).to_string(),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        let conn = db.conn_for_tests();
        let at_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
        let rt_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_refresh_tokens WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
        let ac_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_authorization_codes WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(at_revoked >= 1, "access token should be revoked");
        assert!(rt_revoked >= 1, "refresh token should be revoked");
        assert!(ac_revoked >= 1, "authorization code should be revoked");
    }

    #[tokio::test]
    async fn oauth_client_management_rejects_oauth2_token() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let service = Service::new(build_router(config, db.clone()));
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let (_at, oauth_access_token) = seed_access_token(&db, &client, &user, "runtime:read");

        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Should Fail", &["https://example.com/callback"], None),
            &oauth_access_token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));

        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/list",
            "{}".to_string(),
            &oauth_access_token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    }

    #[tokio::test]
    async fn oauth_client_management_allows_api_token_or_bootstrap() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let pat = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        // PAT
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Via PAT", &["https://example.com/callback"], None),
            &pat,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));

        // Bootstrap token
        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json("Via Bootstrap", &["https://example.com/callback"], None),
            "bootstrap-token",
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
    }

    // -----------------------------------------------------------------------
    // Authorize login / session / consent tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth_authorize_login_rejects_invalid_token() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");
        let body = form_body(&[("return_to", &return_to), ("token", "wc_pat_bogus")]);

        let mut resp = post_form("http://localhost/oauth/authorize/login", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let text = resp.take_string().await.unwrap_or_default();
        assert!(text.contains("invalid token") || text.contains("required"));
        // No session cookie set on failure.
        assert!(set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).is_none());
    }

    #[tokio::test]
    async fn oauth_authorize_login_accepts_pat_and_sets_httponly_cookie() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");
        let body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);

        let resp = post_form("http://localhost/oauth/authorize/login", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        let location = location_header(&resp).expect("redirect after login");
        assert!(location.starts_with("/oauth/authorize"));
        let cookie = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE)
            .expect("session cookie should be set");
        assert!(cookie.starts_with("wc_authsess_"));
        // Verify HttpOnly + SameSite=Lax attributes on the raw Set-Cookie.
        let raw = resp
            .headers
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap_or("").to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(raw.contains("HttpOnly"), "cookie must be HttpOnly");
        assert!(raw.contains("SameSite=Lax"), "cookie must be SameSite=Lax");
    }

    #[tokio::test]
    async fn oauth_authorize_login_rejects_oauth2_access_token() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let (_at, oauth_access_token) = seed_access_token(&db, &client, &user, "runtime:read");
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");
        let body = form_body(&[
            ("return_to", return_to.as_str()),
            ("token", oauth_access_token.as_str()),
        ]);

        let resp = post_form("http://localhost/oauth/authorize/login", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    }

    #[tokio::test]
    async fn oauth_authorize_login_rejects_bootstrap_without_user_id() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");
        // Bootstrap is a valid first-party token but has no user_id, so the
        // authorize login must reject it — an authorization code must bind to
        // a concrete resource owner.
        let body = form_body(&[
            ("return_to", return_to.as_str()),
            ("token", "bootstrap-token"),
        ]);

        let mut resp = post_form("http://localhost/oauth/authorize/login", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        let text = resp.take_string().await.unwrap_or_default();
        assert!(
            text.contains("bootstrap login is not supported"),
            "expected bootstrap rejection message, got: {}",
            text
        );
        // No session cookie set on failure.
        assert!(set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).is_none());
    }

    #[tokio::test]
    async fn oauth_authorize_with_valid_session_shows_consent_page() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        // Log in to obtain a session cookie.
        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        // GET /oauth/authorize with the session cookie -> consent page.
        let url = valid_authorize_url(&client, "https://example.com/callback");
        let mut resp = TestClient::get(&url)
            .add_header("cookie", &cookie, true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let text = resp.take_string().await.unwrap_or_default();
        assert!(text.contains("Authorize WebCodex client"), "consent title");
        assert!(text.contains("Allow"), "Allow button");
        assert!(text.contains("Deny"), "Deny button");
        assert!(text.contains(&client.name), "client name shown");
        assert!(text.contains("runtime:read"), "requested scope shown");
        // No code is issued yet.
        assert!(!text.contains("wc_oac_"));
    }

    #[tokio::test]
    async fn oauth_authorize_consent_shows_resource_when_present() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read project:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        let url = valid_authorize_url_with_resource(
            &client,
            "https://example.com/callback",
            "https://example.test/mcp",
        );
        let mut resp = TestClient::get(&url)
            .add_header("cookie", &cookie, true)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let text = resp.take_string().await.unwrap_or_default();
        assert!(text.contains("Resource:"), "resource label shown");
        assert!(
            text.contains("https://example.test/mcp"),
            "resource value shown"
        );
        assert!(text.contains("Allow"), "Allow button");
        assert!(!text.contains("wc_oac_"));
    }

    #[tokio::test]
    async fn oauth_authorize_consent_allow_redirects_with_code() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        let before = auth_code_count(&db);
        let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
        let resp = post_form_with_cookie(
            "http://localhost/oauth/authorize/consent",
            consent_body,
            &cookie,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before + 1);
        let location = location_header(&resp).expect("redirect with code");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params.get("code").expect("code in redirect");
        assert!(code.starts_with("wc_oac_"));
        let record = auth_code_by_plaintext(&db, code);
        assert_eq!(record.shared_key_hash, None);
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    }

    #[tokio::test]
    async fn oauth_authorize_consent_allow_stores_resource_on_code() {
        let config = test_config(oauth2_enabled_with_issuer("https://example.test"));
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        let before = auth_code_count(&db);
        let consent_body = consent_form_body_with_resource(
            &client,
            "https://example.com/callback",
            "allow",
            "https://example.test/mcp",
        );
        let resp = post_form_with_cookie(
            "http://localhost/oauth/authorize/consent",
            consent_body,
            &cookie,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before + 1);
        let location = location_header(&resp).expect("redirect with code");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        let code = params.get("code").expect("code in redirect");
        let record = auth_code_by_plaintext(&db, code);

        assert!(code.starts_with("wc_oac_"));
        assert_eq!(record.resource.as_deref(), Some("https://example.test/mcp"));
        assert_eq!(record.shared_key_hash, None);
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    }

    #[tokio::test]
    async fn oauth_authorize_consent_deny_redirects_with_access_denied() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        let before = auth_code_count(&db);
        let consent_body = consent_form_body(&client, "https://example.com/callback", "deny");
        let resp = post_form_with_cookie(
            "http://localhost/oauth/authorize/consent",
            consent_body,
            &cookie,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        assert_eq!(auth_code_count(&db), before);
        let location = location_header(&resp).expect("redirect on deny");
        let parsed = url::Url::parse(&location).unwrap();
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("error").map(String::as_str),
            Some("access_denied")
        );
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
    }

    #[tokio::test]
    async fn oauth_authorize_consent_requires_valid_session() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let before = auth_code_count(&db);

        // No cookie at all.
        let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
        let resp = post_form("http://localhost/oauth/authorize/consent", consent_body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        assert_eq!(auth_code_count(&db), before);

        // Bogus cookie.
        let cookie = format!("{}=wc_authsess_bogus", AUTHORIZE_SESSION_COOKIE);
        let consent_body = consent_form_body(&client, "https://example.com/callback", "allow");
        let resp = post_form_with_cookie(
            "http://localhost/oauth/authorize/consent",
            consent_body,
            &cookie,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::UNAUTHORIZED));
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn oauth_authorize_consent_revalidates_redirect_uri() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let client = seed_client_with_redirects_and_scopes(
            &db,
            &user,
            "https://example.com/callback",
            "runtime:read",
        );
        let service = Service::new(build_router(config, db.clone()));
        let return_to = return_to_for(&client, "https://example.com/callback");

        let login_body = form_body(&[("return_to", return_to.as_str()), ("token", token.as_str())]);
        let resp = post_form("http://localhost/oauth/authorize/login", login_body)
            .send(&service)
            .await;
        let cookie_val = set_cookie_value(&resp, AUTHORIZE_SESSION_COOKIE).expect("session cookie");
        let cookie = format!("{}={}", AUTHORIZE_SESSION_COOKIE, cookie_val);

        let before = auth_code_count(&db);
        // Tampered redirect_uri in the consent hidden fields.
        let consent_body = form_body(&[
            ("response_type", "code"),
            ("client_id", client.client_id.as_str()),
            ("redirect_uri", "https://evil.com/callback"),
            ("scope", "runtime:read"),
            ("state", "state-1"),
            ("code_challenge", "challenge-1"),
            ("code_challenge_method", "S256"),
            ("decision", "allow"),
        ]);
        let resp = post_form_with_cookie(
            "http://localhost/oauth/authorize/consent",
            consent_body,
            &cookie,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_no_location(&resp);
        assert_eq!(auth_code_count(&db), before);
    }

    #[tokio::test]
    async fn oauth_authorize_return_to_rejects_absolute_url() {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let token = seed_user_token(&db, &user);
        let service = Service::new(build_router(config, db.clone()));

        // Absolute URL return_to -> rejected (open-redirect guard).
        let body = form_body(&[
            ("return_to", "https://evil.com/oauth/authorize"),
            ("token", token.as_str()),
        ]);
        let resp = post_form("http://localhost/oauth/authorize/login", body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    }
}
