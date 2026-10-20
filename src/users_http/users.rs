use crate::auth::{
    generate_account_credential, hash_token, token_prefix, validate_role, validate_username,
    AuthKind,
};
use crate::management_http::{auth_context, database, parse_json, JsonApiError, JsonApiResult};
use crate::models::{AccountCredentialRecord, UserRecord};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use super::reject_agent_token;

#[derive(Debug, Deserialize)]
pub(crate) struct CreateUserRequest {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub issue_credential: bool,
}

fn user_summary(user: &UserRecord) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "display_name": user.display_name,
        "role": user.role,
        "disabled": user.is_disabled(),
        "disabled_at": user.disabled_at,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}

fn auth_kind_name(kind: AuthKind) -> &'static str {
    match kind {
        AuthKind::Bootstrap => "bootstrap",
        AuthKind::ApiToken => "api",
        AuthKind::AgentToken => "agent",
        AuthKind::AccountCredential => "account",
        AuthKind::OAuth2Token => "oauth2",
        AuthKind::SharedKey => "shared-key",
        AuthKind::ProjectCredential => "project",
        AuthKind::OpenAnonymous => "open",
    }
}

/// `POST /api/users/create` — operationId `createUser`.
///
/// Requires bootstrap/admin auth. Creates a new user with a validated username
/// and role. Duplicate usernames are rejected. Returns a user summary (no
/// secrets).
#[handler]
pub(crate) async fn users_create(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: CreateUserRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    if !auth.is_admin_caller() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
    }
    reject_agent_token(auth)?;

    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    let role = match body.role {
        Some(r) => validate_role(&r).map_err(JsonApiError::bad_request)?,
        None => "user".to_string(),
    };
    let display_name = body
        .display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(d) = display_name.as_ref() {
        if d.chars().count() > 128 {
            return Err(JsonApiError::bad_request("display_name is too long"));
        }
    }

    let db = database(depot)?;

    if db
        .get_user_by_username(&username)
        .map(|o| o.is_some())
        .unwrap_or(false)
    {
        return Err(JsonApiError::new(
            StatusCode::CONFLICT,
            "username already exists",
        ));
    }

    let now = chrono::Utc::now().timestamp();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.clone(),
        created_at: now,
        disabled: 0,
        display_name,
        role,
        disabled_at: None,
        updated_at: Some(now),
    };
    db.create_user(&user).map_err(JsonApiError::internal)?;

    let mut response = json!({
        "success": true,
        "user": user_summary(&user),
    });
    if body.issue_credential {
        let plaintext = generate_account_credential();
        let credential_hash = hash_token(&plaintext);
        let record = AccountCredentialRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            credential_prefix: token_prefix(&plaintext),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        db.insert_account_credential(&record, &credential_hash)
            .map_err(JsonApiError::internal)?;
        response["account_credential"] = json!(plaintext);
        response["account_credential_prefix"] = json!(record.credential_prefix);
    }

    Ok(Json(response))
}

#[handler]
pub(crate) async fn users_me(depot: &mut Depot) -> JsonApiResult {
    let auth = auth_context(depot)?;
    reject_agent_token(auth)?;

    let db = database(depot)?;
    let user = match auth.user_id.as_deref() {
        Some(id) => match db.get_user_by_id(id) {
            Ok(Some(user)) => Some(user),
            Ok(None) => {
                return Err(JsonApiError::new(StatusCode::UNAUTHORIZED, "Unauthorized"));
            }
            Err(e) => {
                return Err(JsonApiError::internal(e));
            }
        },
        None => None,
    };

    Ok(Json(json!({
        "success": true,
        "auth": {
            "kind": auth_kind_name(auth.kind),
            "username": auth.username,
            "role": auth.role,
            "scopes": auth.scopes,
            "is_bootstrap": auth.is_bootstrap,
            "token_kind": auth.token_kind,
        },
        "user": user.as_ref().map(user_summary),
    })))
}

/// `POST /api/users/list` — operationId `listUsers`.
///
/// Bootstrap/admin only. Returns all user summaries.
#[handler]
pub(crate) async fn users_list(depot: &mut Depot) -> JsonApiResult {
    let auth = auth_context(depot)?;
    if !auth.is_admin_caller() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
    }
    reject_agent_token(auth)?;
    let db = database(depot)?;
    let users = db.list_users().map_err(JsonApiError::internal)?;
    let summaries: Vec<Value> = users.iter().map(user_summary).collect();
    Ok(Json(json!({
        "success": true,
        "users": summaries,
        "count": summaries.len(),
    })))
}
