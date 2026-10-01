//! Runner enrollment-token management. These operational routes are not model
//! tools and retain their account/bootstrap authorization requirements.

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
