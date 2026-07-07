use salvo::http::HeaderValue;
use salvo::prelude::*;

use crate::auth::{generate_oauth_authorization_code, hash_token, AuthContext, AuthKind};
use crate::models::OAuthAuthorizationCodeRecord;

use super::html::{authorize_consent_html, authorize_login_html};
use super::normalize_oauth_scopes;
use super::shared_key_bridge::{
    is_shared_key_bridge_query, render_bridge_authorize_form, validate_bridge_authorize_request,
};

pub(super) fn oauth_authorize_direct_error(
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

pub(super) fn redirect_with_oauth_error(
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

pub(super) fn redirect_with_authorization_code(
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

/// Parse an `/oauth/authorize` query string without performing runtime
/// validation, DB lookups, redirects, or authorization code issuance.
///
/// Duplicate known parameters are rejected because they make the OAuth request
/// ambiguous. Unknown parameters are ignored for forward compatibility.
#[allow(dead_code)]
pub(super) fn parse_authorize_query(
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

pub(super) fn decoded_authorize_param(
    query: &str,
    name: &str,
) -> Result<Option<String>, OAuthAuthorizeError> {
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

pub(super) fn validate_authorize_resource(
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

/// Cookie name carrying the opaque authorize session id.
pub(super) const AUTHORIZE_SESSION_COOKIE: &str = "webcodex_authorize_session";

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
pub(super) async fn parse_form_body(req: &mut Request) -> Option<Vec<(String, String)>> {
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

pub(super) fn form_field<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

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

pub(super) fn is_authorize_identity_allowed(ctx: &AuthContext) -> bool {
    matches!(ctx.kind, AuthKind::Bootstrap | AuthKind::ApiToken)
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
