//! Phase 3 agent token management endpoints.
//!
//! These are REST-only admin/self-management surfaces for agent tokens —
//! tokens bound to an owner username and an `allowed_client_id`, usable only
//! on Runner transport endpoints (`/api/shell/agent/*`, `/api/agents/ws`). They
//! are intentionally **not** exposed in `/openapi.json` (GPT Actions) because
//! token creation is sensitive and should be driven by an admin CLI/HTTP
//! client, not a GPT. Their canonical `RouteSpec` entries are `Hidden`, and
//! OpenAPI tests derive the exclusion invariant from that metadata. All endpoints
//! sit behind the shared `AuthMiddleware` (Bearer auth) and resolve the caller's
//! [`AuthContext`] to enforce the admin/bootstrap-or-self boundary. Personal
//! API tokens must also carry explicit `account:manage` authority.
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

use crate::auth::AuthContext;
#[cfg(test)]
use crate::auth::AGENT_SCOPES;
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
    runner_tokens_create, runner_tokens_list, runner_tokens_register_hash, runner_tokens_revoke,
};

// ---------------------------------------------------------------------------
// Auth helpers (mirror users_http.rs)
// ---------------------------------------------------------------------------

/// Enforce that the caller may act on `target_username`:
/// - bootstrap/admin may act on anyone;
/// - a normal user may only act on themselves.
fn require_admin_or_self(
    auth: &AuthContext,
    target_username: &str,
) -> Result<(), (StatusCode, String)> {
    if auth.is_admin_caller() {
        return Ok(());
    }
    match auth.caller_username() {
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
