use crate::auth::{
    generate_account_credential, hash_token, token_prefix, validate_role, validate_username,
    AuthContext, AuthKind,
};
use crate::json_error;
use crate::models::{AccountCredentialRecord, UserRecord};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{is_admin_caller, reject_agent_token};

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
        AuthKind::OpenAnonymous => "open",
    }
}

/// `POST /api/users/create` — operationId `createUser`.
///
/// Requires bootstrap/admin auth. Creates a new user with a validated username
/// and role. Duplicate usernames are rejected. Returns a user summary (no
/// secrets).
#[handler]
pub(crate) async fn users_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body: CreateUserRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {}", e),
            ));
            return;
        }
    };

    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if !is_admin_caller(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
        return;
    }
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let username = match validate_username(&body.username) {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let role = match body.role {
        Some(r) => match validate_role(&r) {
            Ok(r) => r,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        },
        None => "user".to_string(),
    };
    let display_name = body
        .display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(d) = display_name.as_ref() {
        if d.chars().count() > 128 {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "display_name is too long",
            ));
            return;
        }
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };

    if db
        .get_user_by_username(&username)
        .map(|o| o.is_some())
        .unwrap_or(false)
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(json_error(StatusCode::CONFLICT, "username already exists"));
        return;
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
    if let Err(e) = db.create_user(&user) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        return;
    }

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
        if let Err(e) = db.insert_account_credential(&record, &credential_hash) {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
        response["account_credential"] = json!(plaintext);
        response["account_credential_prefix"] = json!(record.credential_prefix);
    }

    res.render(Json(response));
}

#[handler]
pub(crate) async fn users_me(depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }

    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let user = match auth.user_id.as_deref() {
        Some(id) => match db.get_user_by_id(id) {
            Ok(Some(user)) => Some(user),
            Ok(None) => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
                return;
            }
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
                return;
            }
        },
        None => None,
    };

    res.render(Json(json!({
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
    })));
}

/// `POST /api/users/list` — operationId `listUsers`.
///
/// Bootstrap/admin only. Returns all user summaries.
#[handler]
pub(crate) async fn users_list(depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ));
        return;
    };
    if !is_admin_caller(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(json_error(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
        return;
    }
    if let Err((code, msg)) = reject_agent_token(auth) {
        res.status_code(code);
        res.render(json_error(code, msg));
        return;
    }
    let Some(db) = crate::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ));
        return;
    };
    let users = match db.list_users() {
        Ok(u) => u,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            return;
        }
    };
    let summaries: Vec<Value> = users.iter().map(user_summary).collect();
    res.render(Json(json!({
        "success": true,
        "users": summaries,
        "count": summaries.len(),
    })));
}
