//! User and personal-token management. These authenticated operational routes
//! are not model tools; account and bootstrap authority remain mandatory.

use crate::auth::AuthContext;
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
) -> Result<UserRecord, (StatusCode, String)> {
    db.get_user_by_username(username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "user not found".to_string()))
}

#[cfg(test)]
mod tests;
