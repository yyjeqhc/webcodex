use salvo::prelude::*;

use crate::auth::{hash_token, AuthContext};

use super::responses::{parse_management_json, OAuthManagementError as ManagementError};
use super::{apply_oauth_no_store_headers, is_authorize_identity_allowed, oauth_scopes_supported};

const MAX_CLIENT_NAME_LEN: usize = 128;
const MAX_CLIENT_REDIRECT_URIS: usize = 16;

#[derive(Debug, serde::Deserialize)]
struct CreateOAuthClientRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    redirect_uris: Option<Vec<String>>,
    #[serde(default)]
    allowed_scopes: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct RevokeOAuthClientRequest {
    client_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct UpdateOAuthClientScopesRequest {
    client_id: String,
    allowed_scopes: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct UpdateOAuthClientRedirectUriRequest {
    client_id: String,
    redirect_uri: String,
}

/// Exact OAuth permission default that existed before application launch.
/// Keep this list closed and explicit: adding a scope to the supported registry
/// must never widen clients that omit or empty `allowed_scopes`.
const LEGACY_DEFAULT_CLIENT_ALLOWED_SCOPES: &[&str] = &[
    "runtime:read",
    "project:read",
    "project:write",
    "job:run",
    "computer:read",
    "computer:control",
    "account:manage",
];

fn default_client_allowed_scopes() -> &'static [&'static str] {
    LEGACY_DEFAULT_CLIENT_ALLOWED_SCOPES
}

#[cfg(test)]
mod detached_scope_tests {
    use super::*;

    #[test]
    fn legacy_default_client_does_not_gain_detached_execution_scope() {
        assert!(default_client_allowed_scopes().contains(&"job:run"));
        assert!(!default_client_allowed_scopes().contains(&"job:detach"));
    }

    #[test]
    fn legacy_default_client_does_not_gain_browser_authority() {
        for scope in ["browser:read", "browser:control", "browser:launch"] {
            assert!(super::super::scope_registry::oauth_scopes_supported().contains(&scope));
            assert!(!default_client_allowed_scopes().contains(&scope));
        }
    }

    #[test]
    fn legacy_default_client_does_not_gain_memory_authority() {
        assert!(super::super::scope_registry::oauth_scopes_supported()
            .contains(&crate::auth::SCOPE_MEMORY_READ));
        assert!(super::super::scope_registry::oauth_scopes_supported()
            .contains(&crate::auth::SCOPE_MEMORY_MANAGE));
        assert!(!default_client_allowed_scopes().contains(&"memory:read"));
        assert!(!default_client_allowed_scopes().contains(&"memory:manage"));
    }
}

/// Validate a redirect URI for OAuth client registration.
///
/// Rules:
/// - Must parse as an absolute URL.
/// - Scheme must be `http` or `https`.
/// - `http` is only allowed for loopback hosts (`localhost`, `127.0.0.1`,
///   `[::1]`). All other hosts must use `https`.
pub(crate) fn validate_redirect_uri(uri: &str) -> Result<(), String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err("redirect_uri cannot be empty".to_string());
    }
    if trimmed.chars().any(|ch| ch.is_ascii_control()) {
        return Err("redirect_uri must not contain ASCII control characters".to_string());
    }
    let parsed =
        url::Url::parse(trimmed).map_err(|_| "redirect_uri is not a valid URL".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("redirect_uri must not contain userinfo".to_string());
    }
    if parsed.fragment().is_some() {
        return Err("redirect_uri must not contain a fragment".to_string());
    }
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("redirect_uri must use http or https".to_string());
    }
    let host = parsed.host_str().unwrap_or("");
    if scheme == "http" {
        let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
        if !is_loopback {
            return Err("http redirect_uri is only allowed for localhost/127.0.0.1; use https for other hosts".to_string());
        }
    }
    if host.is_empty() {
        return Err("redirect_uri must have a host".to_string());
    }
    Ok(())
}

/// Normalize an `allowed_scopes` input for client registration. When `input`
/// is `None` or empty, returns the stable legacy default scope set.
/// Otherwise every scope must be a member of the global OAuth scope registry.
/// Output is deduplicated and ordered by the global registry.
fn normalize_client_allowed_scopes(input: Option<&[String]>) -> Result<Vec<String>, String> {
    let provided: Vec<&String> = input
        .map(|v| v.iter().filter(|s| !s.trim().is_empty()).collect())
        .unwrap_or_default();
    if provided.is_empty() {
        return Ok(default_client_allowed_scopes()
            .iter()
            .map(|s| s.to_string())
            .collect());
    }
    let supported: std::collections::HashSet<&str> =
        oauth_scopes_supported().iter().copied().collect();
    for scope in &provided {
        if !supported.contains(scope.as_str()) {
            return Err(format!("unknown scope '{}'", scope));
        }
    }
    // Order by the global registry and dedup.
    let mut out = Vec::new();
    for scope in oauth_scopes_supported() {
        if provided.iter().any(|s| s == scope) {
            out.push((*scope).to_string());
        }
    }
    Ok(out)
}

fn auth_can_manage_managed_oauth_client(
    auth: &AuthContext,
    client: &crate::models::OAuthClientRecord,
) -> bool {
    client.is_managed_user_owned()
        && (auth.is_admin() || auth.user_id.as_deref() == client.owner_user_id.as_deref())
}

fn management_context(
    depot: &Depot,
) -> Result<(&AuthContext, std::sync::Arc<crate::Database>), ManagementError> {
    let auth = depot.obtain::<AuthContext>().map_err(|_| {
        ManagementError::new(StatusCode::UNAUTHORIZED, "authenticated user required")
    })?;
    // Keep the route policy's first-party/account authority check here too.
    if !is_authorize_identity_allowed(auth)
        || !auth.has_scope(webcodex_core::authority::SCOPE_ACCOUNT_MANAGE)
    {
        return Err(ManagementError::new(
            StatusCode::FORBIDDEN,
            "first-party account:manage authority required",
        ));
    }
    let db =
        crate::auth::get_db(depot).ok_or_else(|| ManagementError::internal("DB not available"))?;
    Ok((auth, db))
}

fn managed_client(
    db: &crate::Database,
    auth: &AuthContext,
    client_id: &str,
) -> Result<Option<crate::models::OAuthClientRecord>, ManagementError> {
    let client = db
        .get_oauth_client_by_client_id(client_id)
        .map_err(|error| ManagementError::internal("failed to read client").with_detail(error))?;
    if client
        .as_ref()
        .is_some_and(|client| !auth_can_manage_managed_oauth_client(auth, client))
    {
        return Err(ManagementError::new(
            StatusCode::NOT_FOUND,
            "OAuth client not found",
        ));
    }
    Ok(client)
}

#[handler]
pub(crate) async fn oauth_clients_create(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    let (auth, db) = management_context(depot)?;

    let body: CreateOAuthClientRequest = parse_management_json(req).await?;

    let name = body.name.as_deref().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err(ManagementError::bad_request("name is required"));
    }
    if name.chars().count() > MAX_CLIENT_NAME_LEN {
        return Err(ManagementError::bad_request(format!(
            "name is too long; maximum is {} characters",
            MAX_CLIENT_NAME_LEN
        )));
    }

    let redirect_uris: Vec<String> = body.redirect_uris.unwrap_or_default();
    if redirect_uris.is_empty() {
        return Err(ManagementError::bad_request(
            "redirect_uris must be a non-empty array",
        ));
    }
    if redirect_uris.len() > MAX_CLIENT_REDIRECT_URIS {
        return Err(ManagementError::bad_request(format!(
            "too many redirect_uris; maximum is {}",
            MAX_CLIENT_REDIRECT_URIS
        )));
    }
    // Trim each redirect URI, validate the trimmed value, and dedup the
    // trimmed values so whitespace-padded duplicates collapse and the stored
    // record contains no leading/trailing whitespace.
    let mut seen = std::collections::HashSet::new();
    let mut trimmed_uris: Vec<String> = Vec::new();
    for uri in &redirect_uris {
        let trimmed = uri.trim().to_string();
        validate_redirect_uri(&trimmed).map_err(ManagementError::bad_request)?;
        if seen.insert(trimmed.clone()) {
            trimmed_uris.push(trimmed);
        }
    }
    let redirect_uris_str = trimmed_uris.join("\n");

    let allowed_scopes = normalize_client_allowed_scopes(body.allowed_scopes.as_deref())
        .map_err(ManagementError::bad_request)?;
    let allowed_scopes_str = allowed_scopes.join(" ");

    // Bootstrap has no real user_id; attribute the client to the first
    // registered user (bootstrap is the server-wide admin and has no identity
    // row of its own). PATs carry their real user_id. If no users exist,
    // refuse rather than violating the `owner_user_id` foreign key.
    let owner_user_id = match auth.user_id.clone() {
        Some(id) => id,
        None => match db.list_users().map(|mut u| u.pop()) {
            Ok(Some(u)) => u.id,
            _ => {
                return Err(ManagementError::bad_request(
                    "no registered users; create a user or use a PAT to create OAuth clients",
                ));
            }
        },
    };

    let plaintext_secret = crate::auth::generate_oauth_client_secret();
    let secret_hash = hash_token(&plaintext_secret);
    let now = chrono::Utc::now().timestamp();
    let record = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: secret_hash,
        name: name.clone(),
        owner_user_id: Some(owner_user_id),
        owner_project_grant_id: None,
        owner_shared_key_hash: None,
        redirect_uris: redirect_uris_str,
        allowed_scopes: allowed_scopes_str,
        created_at: now,
        revoked_at: None,
    };
    db.insert_oauth_client(&record)
        .map_err(|error| ManagementError::internal("failed to create client").with_detail(error))?;

    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "success": true,
        "client": {
            "client_id": record.client_id,
            "name": record.name,
            "redirect_uris": record.redirect_uris_vec(),
            "allowed_scopes": record.allowed_scopes_vec(),
            "created_at": record.created_at,
            "revoked_at": record.revoked_at,
        },
        "client_secret": plaintext_secret,
    })));
    Ok(())
}

#[handler]
pub(crate) async fn oauth_clients_list(
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    let (auth, db) = management_context(depot)?;

    let clients = db
        .list_oauth_clients()
        .map_err(|_| ManagementError::internal("failed to list clients"))?;

    let clients_json: Vec<serde_json::Value> = clients
        .into_iter()
        .filter(|client| auth_can_manage_managed_oauth_client(auth, client))
        .map(|c| {
            serde_json::json!({
                "client_id": c.client_id,
                "name": c.name,
                "redirect_uris": c.redirect_uris_vec(),
                "allowed_scopes": c.allowed_scopes_vec(),
                "created_at": c.created_at,
                "revoked_at": c.revoked_at,
            })
        })
        .collect();

    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "success": true,
        "clients": clients_json,
    })));
    Ok(())
}

#[handler]
pub(crate) async fn oauth_clients_add_redirect_uri(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    oauth_clients_update_redirect_uri(req, depot, res, true).await
}

#[handler]
pub(crate) async fn oauth_clients_remove_redirect_uri(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    oauth_clients_update_redirect_uri(req, depot, res, false).await
}

async fn oauth_clients_update_redirect_uri(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    add: bool,
) -> Result<(), ManagementError> {
    let (auth, db) = management_context(depot)?;
    let body: UpdateOAuthClientRedirectUriRequest = parse_management_json(req).await?;
    let redirect_uri = body.redirect_uri.trim().to_string();
    validate_redirect_uri(&redirect_uri).map_err(ManagementError::bad_request)?;
    let current = managed_client(&db, auth, &body.client_id)?
        .ok_or_else(|| ManagementError::new(StatusCode::NOT_FOUND, "OAuth client not found"))?;
    let mut redirects = current.redirect_uris_vec();
    let contains = redirects.iter().any(|uri| uri == &redirect_uri);
    if add {
        if contains {
            apply_oauth_no_store_headers(res);
            res.render(Json(serde_json::json!({"success": true, "changed": false, "client": {"client_id": current.client_id, "redirect_uris": redirects}})));
            return Ok(());
        }
        if redirects.len() >= MAX_CLIENT_REDIRECT_URIS {
            return Err(ManagementError::bad_request(format!(
                "too many redirect_uris; maximum is {}",
                MAX_CLIENT_REDIRECT_URIS
            )));
        }
        redirects.push(redirect_uri);
    } else {
        if !contains {
            return Err(ManagementError::new(
                StatusCode::NOT_FOUND,
                "redirect_uri is not registered",
            ));
        }
        if redirects.len() <= 1 {
            return Err(ManagementError::bad_request(
                "OAuth client must retain at least one redirect_uri",
            ));
        }
        redirects.retain(|uri| uri != &redirect_uri);
    }
    let updated = redirects.join("\n");
    match db.update_oauth_client_redirect_uris(&current.client_id, &current.redirect_uris, &updated)
    {
        Ok(true) => {
            apply_oauth_no_store_headers(res);
            res.render(Json(serde_json::json!({
                "success": true, "changed": true,
                "client": {"client_id": current.client_id, "redirect_uris": redirects},
                "tokens_revoked": false,
            })));
        }
        Ok(false) => {
            return Err(ManagementError::new(
                StatusCode::CONFLICT,
                "OAuth client changed or is no longer active",
            ));
        }
        Err(e) => {
            return Err(ManagementError::internal("failed to update redirect_uris").with_detail(e));
        }
    }
    Ok(())
}

#[handler]
pub(crate) async fn oauth_clients_update_scopes(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    let (auth, db) = management_context(depot)?;

    let body: UpdateOAuthClientScopesRequest = parse_management_json(req).await?;
    let client_id = body.client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(ManagementError::bad_request("client_id is required"));
    }
    if !body
        .allowed_scopes
        .iter()
        .any(|scope| !scope.trim().is_empty())
    {
        return Err(ManagementError::bad_request(
            "allowed_scopes must be a non-empty array",
        ));
    }
    let allowed_scopes = normalize_client_allowed_scopes(Some(&body.allowed_scopes))
        .map_err(ManagementError::bad_request)?;
    let allowed_scopes_str = allowed_scopes.join(" ");

    let current = managed_client(&db, auth, &client_id)?
        .ok_or_else(|| ManagementError::new(StatusCode::NOT_FOUND, "OAuth client not found"))?;

    let now = chrono::Utc::now().timestamp();
    let (changed, access_tokens, refresh_tokens, authorization_codes) = match db
        .update_oauth_client_allowed_scopes_and_revoke_grants(
            &client_id,
            &current.allowed_scopes,
            &allowed_scopes_str,
            now,
        ) {
        Ok(Some(result)) => result,
        Ok(None) => {
            return Err(ManagementError::new(
                StatusCode::CONFLICT,
                "OAuth client changed or is no longer active",
            ));
        }
        Err(e) => {
            return Err(ManagementError::internal("failed to update client scopes").with_detail(e));
        }
    };

    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "success": true,
        "changed": changed,
        "reauthorization_required": changed,
        "client": {
            "client_id": current.client_id,
            "name": current.name,
            "redirect_uris": current.redirect_uris_vec(),
            "allowed_scopes": allowed_scopes,
            "created_at": current.created_at,
            "revoked_at": current.revoked_at,
        },
        "revoked_grants": {
            "access_tokens": access_tokens,
            "refresh_tokens": refresh_tokens,
            "authorization_codes": authorization_codes,
        }
    })));
    Ok(())
}

#[handler]
pub(crate) async fn oauth_clients_revoke(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ManagementError> {
    let (auth, db) = management_context(depot)?;

    let body: RevokeOAuthClientRequest = parse_management_json(req).await?;
    let client_id = body.client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(ManagementError::bad_request("client_id is required"));
    }

    // Missing clients remain an idempotent success; a different owner is hidden.
    let _ = managed_client(&db, auth, &client_id)?;

    let now = chrono::Utc::now().timestamp();
    // Revoke the client (idempotent — a missing client_id is a no-op that
    // still returns success), then cascade to all tokens/codes. Each step
    // must succeed; a DB failure returns 500 rather than silently leaving
    // partial revocation state.
    db.revoke_oauth_client_by_client_id(&client_id, now)
        .map_err(|error| ManagementError::internal("failed to revoke client").with_detail(error))?;
    db.revoke_oauth_access_tokens_for_client(&client_id, now)
        .map_err(|error| ManagementError::internal("failed to revoke client").with_detail(error))?;
    db.revoke_oauth_refresh_tokens_for_client(&client_id, now)
        .map_err(|error| ManagementError::internal("failed to revoke client").with_detail(error))?;
    db.revoke_oauth_authorization_codes_for_client(&client_id, now)
        .map_err(|error| ManagementError::internal("failed to revoke client").with_detail(error))?;

    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({"success": true})));
    Ok(())
}
