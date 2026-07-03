//! Phase 3 agent token management endpoints.
//!
//! These are REST-only admin/self-management surfaces for agent tokens —
//! tokens bound to an owner username and an `allowed_client_id`, usable only
//! on agent transport endpoints (`/api/shell/agent/*`, `/api/agents/ws`). They
//! are intentionally **not** exposed in `/openapi.json` (GPT Actions) because
//! token creation is sensitive and should be driven by an admin CLI/HTTP
//! client, not a GPT. The paths are listed in `LEGACY_FORBIDDEN_PATHS` so
//! tests catch accidental OpenAPI inclusion. All endpoints sit behind the
//! shared `AuthMiddleware` (Bearer auth) and resolve the caller's
//! [`AuthContext`] to enforce the admin/bootstrap-or-self boundary.
//!
//! Security properties:
//! - Agent token plaintext is returned **only once** at creation time.
//! - Only the SHA-256 hash (`key_hash`) is persisted.
//! - `key_hash` and plaintext tokens never appear in list/revoke responses.
//! - Agent tokens may only carry `agent:*` scopes.
//! - Agent tokens are rejected from these management endpoints (only bootstrap
//!   and user tokens may manage agent tokens), so a leaked agent token cannot
//!   mint more agent tokens.
//! - Agent tokens may not call the Phase 2 personal API token management
//!   endpoints either.

#[cfg(test)]
use crate::auth::AGENT_SCOPES;
use crate::auth::{AuthContext, SCOPE_ADMIN};
#[cfg(test)]
use crate::models::ApiKeyRecord;
use crate::Database;
use salvo::prelude::*;
#[cfg(test)]
use serde_json::{json, Value};

mod responses;
mod routes;

#[cfg(test)]
use responses::agent_token_summary;
pub(crate) use routes::{
    agent_tokens_create, agent_tokens_list, agent_tokens_register_hash, agent_tokens_revoke,
};

// ---------------------------------------------------------------------------
// Auth helpers (mirror users_http.rs)
// ---------------------------------------------------------------------------

/// True when the caller may manage any user (bootstrap token or `admin` role).
fn is_admin_caller(auth: &AuthContext) -> bool {
    auth.is_bootstrap
        || auth.role.as_deref() == Some("admin")
        || auth.scopes.iter().any(|s| s == SCOPE_ADMIN)
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
) -> Result<crate::models::UserRecord, (StatusCode, String)> {
    db.get_user_by_username(username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "user not found".to_string()))
}

#[cfg(test)]
mod tests;
