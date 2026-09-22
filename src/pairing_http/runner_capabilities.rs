//! Explicit operator grant for Desktop connections created before ACP/SSH UX.
//! This is account authorization, not an alternate SSH gateway. It requires
//! administrator/bootstrap authority and never accepts arbitrary scope names.
use super::*;
use crate::runtime_http::require_runtime;
use serde_json::Value;
use webcodex_store::Database;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantRequest {
    client_id: String,
    /// A native-only lookup; neither plaintext credential nor hash is returned.
    user_token_hash: String,
}

#[handler]
pub(crate) async fn grant_runner_capabilities(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(auth) = depot.obtain::<AuthContext>().ok().cloned() else {
        grant_error(res, StatusCode::UNAUTHORIZED);
        return;
    };
    if !auth.is_admin_caller()
        || !matches!(
            auth.kind,
            crate::auth::AuthKind::Bootstrap
                | crate::auth::AuthKind::ApiToken
                | crate::auth::AuthKind::OAuth2Token
        )
    {
        grant_error(res, StatusCode::FORBIDDEN);
        return;
    }
    let bytes = match req.payload_with_max_size(4096).await {
        Ok(bytes) => bytes,
        Err(_) => {
            grant_error(res, StatusCode::BAD_REQUEST);
            return;
        }
    };
    let body = match serde_json::from_slice::<GrantRequest>(bytes) {
        Ok(body)
            if validate_allowed_client_id(&body.client_id).is_ok_and(|id| id == body.client_id)
                && body.user_token_hash.len() == 64
                && body
                    .user_token_hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) =>
        {
            body
        }
        _ => {
            grant_error(res, StatusCode::BAD_REQUEST);
            return;
        }
    };
    let Some(db) = crate::get_db(depot) else {
        grant_error(res, StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    let observed = runtime.list_runners(Some(&auth)).await;
    let owner = observed
        .output
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|runners| {
            runners.iter().find(|runner| {
                runner.get("client_id").and_then(Value::as_str) == Some(body.client_id.as_str())
                    && runner.get("connected").and_then(Value::as_bool) == Some(true)
            })
        })
        .and_then(|runner| runner.get("owner"))
        .and_then(Value::as_str);
    let Some(owner) = owner else {
        grant_error(res, StatusCode::CONFLICT);
        return;
    };
    match grant_for_owner(
        &db,
        &body.user_token_hash,
        owner,
        chrono::Utc::now().timestamp(),
    ) {
        Ok(changed) => res.render(Json(json!({
            "success":true, "changed":changed,
            "applied_scopes":[SCOPE_SSH_LOCAL, SCOPE_CODING_AGENT_RUN],
        }))),
        Err(status) => grant_error(res, status),
    }
}

fn grant_for_owner(
    db: &Database,
    token_hash: &str,
    runner_owner: &str,
    now: i64,
) -> Result<bool, StatusCode> {
    let key = db
        .get_api_key_by_hash(token_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter(|key| {
            key.kind == TOKEN_KIND_USER
                && key.allowed_client_id.is_none()
                && key.revoked_at.is_none()
                && key.expires_at.is_none_or(|expires| expires > now)
        })
        .ok_or(StatusCode::FORBIDDEN)?;
    let user = db
        .get_user_by_id(&key.user_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter(|user| !user.is_disabled() && user.username == runner_owner)
        .ok_or(StatusCode::FORBIDDEN)?;
    let mut scopes: Vec<String> = key
        .scopes
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect();
    let before = scopes.len();
    for scope in [SCOPE_SSH_LOCAL, SCOPE_CODING_AGENT_RUN] {
        if !scopes.iter().any(|existing| existing == scope) {
            scopes.push(scope.to_owned());
        }
    }
    if scopes.len() == before {
        return Ok(false);
    }
    if db
        .compare_and_swap_user_key_scopes(
            &key.id,
            &user.id,
            &key.scopes,
            &scopes_to_string(&scopes),
            now,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Ok(true)
    } else {
        Err(StatusCode::CONFLICT)
    }
}

fn grant_error(res: &mut Response, status: StatusCode) {
    res.status_code(status);
    // Never echo a credential, hash, path, owner or raw parser/database error.
    res.render(json_error(status, "Runner capability grant was not applied; verify local operator authority and refresh this connection"));
}

#[cfg(test)]
mod tests;
