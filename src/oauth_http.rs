//! OAuth2 authorization, token, revocation, and discovery endpoints.
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

use salvo::http::HeaderValue;
use salvo::prelude::*;

mod clients;
mod html;
mod managed_authorize;
mod metadata;
mod revoke;
mod scope_registry;
mod shared_key_bridge;
mod token;

pub(crate) use clients::{oauth_clients_create, oauth_clients_list, oauth_clients_revoke};
use html::authorize_bridge_html;
#[cfg(test)]
use managed_authorize::AUTHORIZE_SESSION_COOKIE;
use managed_authorize::{
    decoded_authorize_param, form_field, is_authorize_identity_allowed,
    oauth_authorize_direct_error, parse_authorize_query, parse_form_body,
    redirect_with_authorization_code, redirect_with_oauth_error, validate_authorize_resource,
    OAuthAuthorizeError, OAuthAuthorizeRequest,
};
pub(crate) use managed_authorize::{
    oauth_authorize, oauth_authorize_consent, oauth_authorize_login, AuthorizeSessionStore,
};
pub(crate) use metadata::{oauth_authorization_server_metadata, oauth_metadata};
pub(crate) use revoke::oauth_revoke;
pub(crate) use scope_registry::{normalize_oauth_scopes, oauth_scopes_supported};
pub(crate) use shared_key_bridge::oauth_authorize_bridge;
#[cfg(test)]
pub(crate) use shared_key_bridge::{
    bridge_shared_key_hash, normalize_bridge_oauth_scopes, OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE,
};
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
