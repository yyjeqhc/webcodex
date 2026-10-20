//! User and personal-token management. These authenticated operational routes
//! are not model tools; account and bootstrap authority remain mandatory.

use crate::auth::AuthContext;
use crate::management_http::JsonApiError;
#[cfg(test)]
use crate::models::ApiKeyRecord;
use salvo::prelude::*;
#[cfg(test)]
use serde_json::{json, Value};

mod tokens;
mod users;

pub(crate) use tokens::{tokens_create, tokens_list, tokens_register_hash, tokens_revoke};
pub(crate) use users::{users_create, users_list, users_me};

/// Agent tokens cannot call user/token management endpoints.
fn reject_agent_token(auth: &AuthContext) -> Result<(), JsonApiError> {
    if auth.is_agent_token() {
        Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "agent tokens may not manage users or tokens",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
