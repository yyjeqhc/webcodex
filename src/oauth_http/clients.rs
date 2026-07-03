use salvo::prelude::*;

use crate::auth::{hash_token, AuthContext};

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

/// The full default delegable OAuth scope set, used when `allowed_scopes` is
/// omitted or empty on client creation.
fn default_client_allowed_scopes() -> Vec<&'static str> {
    oauth_scopes_supported().to_vec()
}

/// Validate a redirect URI for OAuth client registration.
///
/// Rules:
/// - Must parse as an absolute URL.
/// - Scheme must be `http` or `https`.
/// - `http` is only allowed for loopback hosts (`localhost`, `127.0.0.1`,
///   `[::1]`). All other hosts must use `https`.
fn validate_redirect_uri(uri: &str) -> Result<(), String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err("redirect_uri cannot be empty".to_string());
    }
    let parsed =
        url::Url::parse(trimmed).map_err(|_| "redirect_uri is not a valid URL".to_string())?;
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
/// is `None` or empty, returns the full default delegable OAuth scope set.
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

#[handler]
pub(crate) async fn oauth_clients_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "authenticated user required"}),
        ));
        return;
    };
    // Double-check first-party identity. The route policy + AuthMiddleware
    // already block OAuth2Token/AgentToken/AccountCredential, but we defend
    // in depth here too.
    if !is_authorize_identity_allowed(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(serde_json::json!({
            "error": "OAuth2 access tokens cannot manage OAuth clients"
        })));
        return;
    }
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };

    let body: CreateOAuthClientRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({
                "error": "invalid request body",
                "detail": e.to_string()
            })));
            return;
        }
    };

    let name = body.name.as_deref().unwrap_or("").trim().to_string();
    if name.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": "name is required"})));
        return;
    }
    if name.chars().count() > MAX_CLIENT_NAME_LEN {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({
            "error": format!("name is too long; maximum is {} characters", MAX_CLIENT_NAME_LEN)
        })));
        return;
    }

    let redirect_uris: Vec<String> = body.redirect_uris.unwrap_or_default();
    if redirect_uris.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            serde_json::json!({"error": "redirect_uris must be a non-empty array"}),
        ));
        return;
    }
    if redirect_uris.len() > MAX_CLIENT_REDIRECT_URIS {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({
            "error": format!("too many redirect_uris; maximum is {}", MAX_CLIENT_REDIRECT_URIS)
        })));
        return;
    }
    // Trim each redirect URI, validate the trimmed value, and dedup the
    // trimmed values so whitespace-padded duplicates collapse and the stored
    // record contains no leading/trailing whitespace.
    let mut seen = std::collections::HashSet::new();
    let mut trimmed_uris: Vec<String> = Vec::new();
    for uri in &redirect_uris {
        let trimmed = uri.trim().to_string();
        if let Err(msg) = validate_redirect_uri(&trimmed) {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": msg})));
            return;
        }
        if seen.insert(trimmed.clone()) {
            trimmed_uris.push(trimmed);
        }
    }
    let redirect_uris_str = trimmed_uris.join("\n");

    let allowed_scopes = match normalize_client_allowed_scopes(body.allowed_scopes.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": msg})));
            return;
        }
    };
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
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(serde_json::json!({
                    "error": "no registered users; create a user or use a PAT to create OAuth clients"
                })));
                return;
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
        owner_user_id,
        redirect_uris: redirect_uris_str,
        allowed_scopes: allowed_scopes_str,
        created_at: now,
        revoked_at: None,
    };
    if let Err(e) = db.insert_oauth_client(&record) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            serde_json::json!({"error": "failed to create client", "detail": e.to_string()}),
        ));
        return;
    }

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
}

#[handler]
pub(crate) async fn oauth_clients_list(depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "authenticated user required"}),
        ));
        return;
    };
    if !is_authorize_identity_allowed(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(serde_json::json!({
            "error": "OAuth2 access tokens cannot manage OAuth clients"
        })));
        return;
    }
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };

    let clients = match db.list_oauth_clients() {
        Ok(c) => c,
        Err(_) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "failed to list clients"})));
            return;
        }
    };

    let clients_json: Vec<serde_json::Value> = clients
        .into_iter()
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
}

#[handler]
pub(crate) async fn oauth_clients_revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "authenticated user required"}),
        ));
        return;
    };
    if !is_authorize_identity_allowed(auth) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(serde_json::json!({
            "error": "OAuth2 access tokens cannot manage OAuth clients"
        })));
        return;
    }
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };

    let body: RevokeOAuthClientRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({
                "error": "invalid request body",
                "detail": e.to_string()
            })));
            return;
        }
    };
    let client_id = body.client_id.trim().to_string();
    if client_id.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": "client_id is required"})));
        return;
    }

    let now = chrono::Utc::now().timestamp();
    // Revoke the client (idempotent — a missing client_id is a no-op that
    // still returns success), then cascade to all tokens/codes. Each step
    // must succeed; a DB failure returns 500 rather than silently leaving
    // partial revocation state.
    if let Err(e) = db.revoke_oauth_client_by_client_id(&client_id, now) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": "failed to revoke client",
            "detail": e.to_string()
        })));
        return;
    }
    if let Err(e) = db.revoke_oauth_access_tokens_for_client(&client_id, now) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": "failed to revoke client",
            "detail": e.to_string()
        })));
        return;
    }
    if let Err(e) = db.revoke_oauth_refresh_tokens_for_client(&client_id, now) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": "failed to revoke client",
            "detail": e.to_string()
        })));
        return;
    }
    if let Err(e) = db.revoke_oauth_authorization_codes_for_client(&client_id, now) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": "failed to revoke client",
            "detail": e.to_string()
        })));
        return;
    }

    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({"success": true})));
}
