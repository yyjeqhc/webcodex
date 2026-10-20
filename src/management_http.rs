//! Shared response plumbing for authenticated user and token management routes.
//! Handlers keep their authorization and validation checks in execution order.

use crate::auth::AuthContext;
use crate::models::UserRecord;
use crate::Database;
use salvo::prelude::*;
use salvo::writing::Scribe;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt::Display;
use std::sync::Arc;

pub(crate) type JsonApiResult = Result<Json<Value>, JsonApiError>;

#[derive(Debug)]
pub(crate) struct JsonApiError {
    status: StatusCode,
    message: String,
}

impl JsonApiError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub(crate) fn internal(error: impl Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl Scribe for JsonApiError {
    fn render(self, res: &mut Response) {
        res.status_code(self.status);
        res.render(crate::json_error(self.status, self.message));
    }
}

pub(crate) async fn parse_json<T: DeserializeOwned>(req: &mut Request) -> Result<T, JsonApiError> {
    req.parse_json()
        .await
        .map_err(|e| JsonApiError::bad_request(format!("invalid request body: {}", e)))
}

pub(crate) fn auth_context(depot: &Depot) -> Result<&AuthContext, JsonApiError> {
    depot
        .obtain::<AuthContext>()
        .map_err(|_| JsonApiError::internal("no auth context"))
}

pub(crate) fn database(depot: &Depot) -> Result<Arc<Database>, JsonApiError> {
    crate::get_db(depot).ok_or_else(|| JsonApiError::internal("DB not available"))
}

/// Bootstrap/admin may act on anyone; normal callers may act only on themselves.
pub(crate) fn require_admin_or_self(
    auth: &AuthContext,
    target_username: &str,
) -> Result<(), JsonApiError> {
    if auth.is_admin_caller() {
        return Ok(());
    }
    match auth.caller_username() {
        Some(caller) if caller == target_username => Ok(()),
        _ => Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "caller may only manage their own resources",
        )),
    }
}

pub(crate) fn require_user_by_username(
    db: &Database,
    username: &str,
) -> Result<UserRecord, JsonApiError> {
    db.get_user_by_username(username)
        .map_err(JsonApiError::internal)?
        .ok_or_else(|| JsonApiError::new(StatusCode::NOT_FOUND, "user not found"))
}

#[cfg(test)]
mod tests;
