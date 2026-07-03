//! Phase 2 multi-user auth: user + personal API token management endpoints.
//!
//! These are REST-only admin/self-management surfaces. They are intentionally
//! **not** exposed in `/openapi.json` (GPT Actions) because token creation is
//! sensitive and should be driven by an admin CLI/HTTP client, not a GPT. The
//! paths are listed in `LEGACY_FORBIDDEN_PATHS` so tests catch accidental
//! OpenAPI inclusion. All endpoints sit behind the shared `AuthMiddleware`
//! (Bearer auth) and resolve the caller's [`AuthContext`] to enforce the
//! admin/bootstrap-or-self boundary.
//!
//! Security properties:
//! - Plaintext tokens are returned **only once** at creation time.
//! - Only the SHA-256 hash (`token_hash`) is persisted.
//! - `token_hash` and plaintext tokens never appear in list/status responses.
//! - `token_prefix` is returned for display so users can identify tokens.
//! - Unauthorized responses are JSON with a generic `error` message that does
//!   not leak whether a token prefix or username exists.

use crate::auth::{AuthContext, SCOPE_ADMIN};
#[cfg(test)]
use crate::models::ApiKeyRecord;
use crate::models::UserRecord;
use crate::Database;
use salvo::prelude::*;
#[cfg(test)]
use serde_json::{json, Value};

mod tokens;
mod users;

pub(crate) use tokens::{tokens_create, tokens_list, tokens_register_hash, tokens_revoke};
pub(crate) use users::{users_create, users_list, users_me};

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// True when the caller may manage any user (bootstrap token or `admin` role).
fn is_admin_caller(auth: &AuthContext) -> bool {
    auth.is_bootstrap
        || auth.role.as_deref() == Some("admin")
        || auth.scopes.iter().any(|s| s == SCOPE_ADMIN)
}

/// Phase 3: agent tokens must not be able to call user/token management
/// endpoints. Returns an error response tuple when the caller is an agent
/// token.
fn reject_agent_token(auth: &AuthContext) -> Result<(), (StatusCode, String)> {
    if auth.is_agent_token() {
        Err((
            StatusCode::FORBIDDEN,
            "agent tokens may not manage users or tokens".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Resolve the authenticated caller's username, if any. Bootstrap callers do
/// not have a username.
fn caller_username(auth: &AuthContext) -> Option<&str> {
    if auth.is_bootstrap {
        None
    } else {
        auth.username.as_deref()
    }
}

/// Enforce that the caller may act on `target_username`:
/// - bootstrap/admin may act on anyone;
/// - a normal user may only act on themselves.
fn require_admin_or_self(
    auth: &AuthContext,
    target_username: &str,
) -> Result<(), (StatusCode, String)> {
    if is_admin_caller(auth) {
        return Ok(());
    }
    match caller_username(auth) {
        Some(caller) if caller == target_username => Ok(()),
        _ => Err((
            StatusCode::FORBIDDEN,
            "caller may only manage their own resources".to_string(),
        )),
    }
}

/// Load a user by username, returning a JSON 404-style error when missing.
fn require_user_by_username(
    db: &Database,
    username: &str,
) -> Result<UserRecord, (StatusCode, String)> {
    db.get_user_by_username(username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "user not found".to_string()))
}

#[cfg(test)]
mod tests;
