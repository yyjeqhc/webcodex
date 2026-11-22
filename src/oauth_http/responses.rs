use crate::models::DynamicClientAuth;
use base64::Engine;
use salvo::http::{header::CONTENT_TYPE, HeaderValue};
use salvo::prelude::*;
use salvo::writing::Scribe;
use serde::de::DeserializeOwned;
use std::fmt::Display;
use std::sync::Arc;

use super::MAX_OAUTH_TOKEN_FORM_BYTES;

/// Apply cache-prevention headers to an OAuth2 response (RFC 6749 §5.1, §5.2).
///
/// All OAuth2 JSON responses — both success and error — must include these
/// headers to prevent intermediaries from caching sensitive tokens or error
/// context.
pub(super) fn apply_oauth_no_store_headers(res: &mut Response) {
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    res.headers_mut()
        .insert("pragma", HeaderValue::from_static("no-cache"));
}

/// Render an OAuth2 error response (RFC 6749 §5.2) with no-store headers.
pub(super) fn oauth_error(res: &mut Response, status: StatusCode, error: &str, description: &str) {
    res.status_code(status);
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "error": error,
        "error_description": description,
    })));
}

/// Token/revocation protocol errors always carry the OAuth error shape and
/// cache-prevention headers. Authorize and management responses have separate
/// contracts and do not use this renderer.
#[derive(Debug)]
pub(crate) struct OAuthTokenError {
    status: StatusCode,
    error: &'static str,
    description: String,
    challenge_basic: bool,
}

impl OAuthTokenError {
    pub(super) fn new(
        status: StatusCode,
        error: &'static str,
        description: impl Into<String>,
    ) -> Self {
        Self {
            status,
            error,
            description: description.into(),
            challenge_basic: false,
        }
    }

    pub(super) fn invalid_request(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", description)
    }

    pub(super) fn invalid_grant(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_grant", description)
    }

    pub(super) fn invalid_target(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_target", description)
    }

    pub(super) fn invalid_client(description: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "invalid_client", description)
    }

    pub(super) fn server_error(description: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            description,
        )
    }
}

impl Scribe for OAuthTokenError {
    fn render(self, res: &mut Response) {
        if self.challenge_basic {
            res.headers_mut().insert(
                salvo::http::header::WWW_AUTHENTICATE,
                HeaderValue::from_static(r#"Basic realm="WebCodex""#),
            );
        }
        oauth_error(res, self.status, self.error, &self.description);
    }
}

pub(super) fn enabled_token_config(depot: &Depot) -> Result<Arc<crate::Config>, OAuthTokenError> {
    let config =
        crate::auth::get_config(depot).ok_or_else(|| OAuthTokenError::server_error("no config"))?;
    if !config.oauth2.enabled {
        return Err(OAuthTokenError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "OAuth2 is not enabled",
        ));
    }
    Ok(config)
}

pub(super) async fn parse_token_form<T: DeserializeOwned>(
    req: &mut Request,
) -> Result<T, OAuthTokenError> {
    let content_type_ok = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| {
            ct.eq_ignore_ascii_case("application/x-www-form-urlencoded")
                || ct
                    .to_ascii_lowercase()
                    .starts_with("application/x-www-form-urlencoded;")
        });
    if !content_type_ok {
        return Err(OAuthTokenError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_request",
            "content-type must be application/x-www-form-urlencoded",
        ));
    }
    let too_large = || {
        OAuthTokenError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "invalid_request",
            "request body too large",
        )
    };
    if req
        .headers()
        .get("content-length")
        .and_then(|cl| cl.to_str().unwrap_or("").parse::<usize>().ok())
        .is_some_and(|len| len > MAX_OAUTH_TOKEN_FORM_BYTES)
    {
        return Err(too_large());
    }
    let body = req.payload().await.map_err(|error| {
        OAuthTokenError::invalid_request(format!("failed to read request body: {error}"))
    })?;
    if body.len() > MAX_OAUTH_TOKEN_FORM_BYTES {
        return Err(too_large());
    }
    serde_urlencoded::from_bytes(body)
        .map_err(|error| OAuthTokenError::invalid_request(format!("invalid form body: {error}")))
}

pub(super) fn required_token_parameter<'a>(
    value: Option<&'a str>,
    name: &str,
) -> Result<&'a str, OAuthTokenError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OAuthTokenError::invalid_request(format!("missing {name}")))
}

/// Call only after endpoint-specific parameters have been checked: a missing
/// grant/token must retain priority over client authentication and DB access.
pub(super) fn authenticate_token_client(
    req: &Request,
    depot: &Depot,
    config: &crate::Config,
    form_id: Option<&str>,
    form_secret: Option<&str>,
) -> Result<(Arc<crate::Database>, crate::models::OAuthClientRecord), OAuthTokenError> {
    let unauthorized = || {
        let mut error = OAuthTokenError::invalid_client("client authentication failed");
        error.challenge_basic = req
            .headers()
            .contains_key(salvo::http::header::AUTHORIZATION);
        error
    };
    let credentials = basic_client(req, form_id, form_secret).map_err(|_| unauthorized())?;
    let (id, secret, method) = if let Some((id, secret)) = &credentials {
        (id.as_str(), Some(secret.as_str()), DynamicClientAuth::Basic)
    } else {
        let id = required_token_parameter(form_id, "client_id")?;
        let method = if form_secret.is_some() {
            DynamicClientAuth::Post
        } else {
            DynamicClientAuth::Public
        };
        (id, form_secret, method)
    };
    let missing_secret = || OAuthTokenError::invalid_client("missing client_secret");
    let ingress = crate::public_ingress_auth::entry(depot);
    if secret == Some("")
        || (method == DynamicClientAuth::Public
            && config.oauth2.registration_target(ingress).is_none())
    {
        return Err(missing_secret());
    }
    let db = crate::auth::get_db(depot)
        .ok_or_else(|| OAuthTokenError::server_error("DB not available"))?;
    let client = db
        .get_oauth_client_by_client_id(id)
        .map_err(|_| OAuthTokenError::server_error("internal error"))?
        .ok_or_else(unauthorized)?;
    if !config.oauth2.accepts_client_registration(&client, ingress) {
        return Err(unauthorized());
    }
    let registered_method = client
        .dynamic_registration
        .as_ref()
        .map(|d| d.auth_method)
        .unwrap_or(DynamicClientAuth::Post);
    if method != registered_method {
        return Err(if method == DynamicClientAuth::Public {
            missing_secret()
        } else {
            unauthorized()
        });
    }
    if !crate::public_ingress_auth::client_allowed(&db, depot, id) {
        return Err(OAuthTokenError::new(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "client is not registered for this listener",
        ));
    }
    if secret.is_some_and(|value| !crate::auth::oauth_client_secret_matches(&client, value)) {
        return Err(unauthorized());
    }
    Ok((db, client))
}

/// A form client_id may repeat the Basic identity. Two credential methods or
/// different identities in the same request are ambiguous.
fn basic_client(
    req: &Request,
    form_id: Option<&str>,
    form_secret: Option<&str>,
) -> Result<Option<(String, String)>, ()> {
    let mut headers = req.headers().get_all("authorization").iter();
    let Some(header) = headers.next() else {
        return Ok(None);
    };
    if headers.next().is_some() || form_secret.is_some() {
        return Err(());
    }
    let (scheme, encoded) = header
        .to_str()
        .ok()
        .and_then(|v| v.split_once(' '))
        .ok_or(())?;
    if !scheme.eq_ignore_ascii_case("Basic") || encoded.len() > 2048 {
        return Err(());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| ())?;
    let (id, secret) = std::str::from_utf8(&decoded)
        .ok()
        .and_then(|v| v.split_once(':'))
        .ok_or(())?;
    let id = urlencoding::decode(id).map_err(|_| ())?.into_owned();
    let secret = urlencoding::decode(secret).map_err(|_| ())?.into_owned();
    if id.is_empty() || secret.is_empty() || form_id.is_some_and(|form| form != id) {
        return Err(());
    }
    Ok(Some((id, secret)))
}

/// Management routes retain their error/detail JSON, including which failures
/// expose a detail. Their errors do not inherit token-endpoint headers.
#[derive(Debug)]
pub(crate) struct OAuthManagementError {
    status: StatusCode,
    message: String,
    detail: Option<String>,
}

impl OAuthManagementError {
    pub(super) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            detail: None,
        }
    }

    pub(super) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub(super) fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub(super) fn with_detail(mut self, detail: impl Display) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

impl Scribe for OAuthManagementError {
    fn render(self, res: &mut Response) {
        let mut body = serde_json::json!({ "error": self.message });
        if let Some(detail) = self.detail {
            body["detail"] = serde_json::Value::String(detail);
        }
        res.status_code(self.status);
        res.render(Json(body));
    }
}

pub(super) async fn parse_management_json<T: DeserializeOwned>(
    req: &mut Request,
) -> Result<T, OAuthManagementError> {
    req.parse_json().await.map_err(|error| {
        OAuthManagementError::bad_request("invalid request body").with_detail(error)
    })
}
