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
    depot: &Depot,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Result<(Arc<crate::Database>, crate::models::OAuthClientRecord), OAuthTokenError> {
    let client_id = required_token_parameter(client_id, "client_id")?;
    let client_secret = client_secret
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| OAuthTokenError::invalid_client("missing client_secret"))?;
    let db = crate::auth::get_db(depot)
        .ok_or_else(|| OAuthTokenError::server_error("DB not available"))?;
    let secret_ok = crate::auth::verify_oauth_client_secret(&db, client_id, client_secret)
        .map_err(|_| OAuthTokenError::server_error("internal error"))?;
    if !secret_ok {
        return Err(OAuthTokenError::invalid_client(
            "client authentication failed",
        ));
    }
    // Recheck liveness after secret verification, retaining the existing
    // invalid_client response if this second lookup fails or sees revocation.
    let client = db
        .get_oauth_client_by_client_id(client_id)
        .ok()
        .flatten()
        .ok_or_else(|| OAuthTokenError::invalid_client("client authentication failed"))?;
    Ok((db, client))
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
