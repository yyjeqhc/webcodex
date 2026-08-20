use salvo::prelude::*;

use crate::auth::{
    generate_oauth_authorization_code, hash_token, shared_key_hash_of, AuthContext,
    DIRECT_SHARED_KEY_MODEL_SCOPES,
};
use crate::models::OAuthAuthorizationCodeRecord;

use super::{
    apply_oauth_no_store_headers, authorize_bridge_html, decoded_authorize_param, form_field,
    normalize_oauth_scopes, oauth_authorize_direct_error, parse_authorize_query, parse_form_body,
    redirect_with_authorization_code, redirect_with_oauth_error, validate_authorize_resource,
    validate_redirect_uri, OAuthAuthorizeError, OAuthAuthorizeRequest, OAUTH_OFFLINE_ACCESS_SCOPE,
};

pub(crate) const OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE: &str =
    "bridge tokens are limited to the direct shared-key model-facing scope ceiling";

pub(crate) fn bridge_oauth_scopes() -> &'static [&'static str] {
    DIRECT_SHARED_KEY_MODEL_SCOPES
}

fn is_managed_credential_like(value: &str) -> bool {
    value.starts_with("wc_")
}

pub(crate) fn bridge_shared_key_hash(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("a shared key is required");
    }
    if is_managed_credential_like(value) {
        return Err("managed credentials cannot be used as shared keys");
    }

    Ok(shared_key_hash_of(value))
}

pub(crate) fn normalize_bridge_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let normalized = normalize_oauth_scopes(requested, client_allowed)?;
    if normalized
        .split_whitespace()
        .any(|scope| scope != OAUTH_OFFLINE_ACCESS_SCOPE && !bridge_oauth_scopes().contains(&scope))
    {
        return Err(OAuthAuthorizeError::InvalidScope(
            OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE,
        ));
    }
    Ok(normalized)
}

#[derive(Clone)]
pub(super) struct BridgeAuthorizeValidated {
    parsed: OAuthAuthorizeRequest,
    client: crate::models::OAuthClientRecord,
    scopes: String,
    resource: Option<String>,
}

pub(super) fn is_shared_key_bridge_query(query: &str) -> Result<bool, OAuthAuthorizeError> {
    match decoded_authorize_param(query, "bridge")? {
        Some(value) if value == "shared_key" => Ok(true),
        Some(_) => Err(OAuthAuthorizeError::InvalidRequest("unsupported bridge")),
        None => Ok(false),
    }
}

pub(super) fn validate_bridge_authorize_request(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    query: &str,
) -> Option<BridgeAuthorizeValidated> {
    let parsed = match parse_authorize_query(query) {
        Ok(parsed) => parsed,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return None;
        }
    };

    if parsed.client_id.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        );
        return None;
    }

    let client = match db.get_oauth_client_by_client_id(&parsed.client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid client_id",
            );
            return None;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return None;
        }
    };

    if parsed.redirect_uri.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        );
        return None;
    }

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|registered| registered == &parsed.redirect_uri)
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
        return None;
    }

    if parsed.response_type.is_empty() || parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return None;
    }

    if parsed.code_challenge.is_empty()
        || parsed.code_challenge_method.is_empty()
        || parsed.code_challenge_method != "S256"
    {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return None;
    }

    let scopes =
        match normalize_bridge_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
            Ok(scopes) => scopes,
            Err(_) => {
                redirect_with_oauth_error(
                    res,
                    config,
                    &parsed.redirect_uri,
                    "invalid_scope",
                    parsed.state.as_deref(),
                );
                return None;
            }
        };

    let resource = match validate_authorize_resource(parsed.resource.as_deref(), config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                config,
                &parsed.redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return None;
        }
    };

    Some(BridgeAuthorizeValidated {
        parsed,
        client,
        scopes,
        resource,
    })
}

pub(super) fn render_bridge_authorize_form(
    res: &mut Response,
    validated: &BridgeAuthorizeValidated,
    query: &str,
    error: Option<&str>,
) {
    let scopes = validated
        .scopes
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let html = authorize_bridge_html(
        &validated.client.name,
        &validated.client.client_id,
        &validated.parsed.redirect_uri,
        &scopes,
        validated.resource.as_deref(),
        query,
        error,
    );
    res.status_code(if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    });
    res.render(Text::Html(html));
}

fn issue_bridge_authorization_code(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    validated: &BridgeAuthorizeValidated,
    shared_key_hash: String,
) {
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: validated.client.client_id.clone(),
        subject_kind: "shared_key".to_string(),
        subject_id: shared_key_hash.clone(),
        user_id: None,
        redirect_uri: validated.parsed.redirect_uri.clone(),
        scopes: validated.scopes.clone(),
        resource: validated.resource.clone(),
        code_challenge: Some(validated.parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: Some(shared_key_hash),
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };

    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(
        res,
        config,
        &validated.parsed.redirect_uri,
        &plaintext_code,
        validated.parsed.state.as_deref(),
    );
}

#[derive(Debug, serde::Deserialize)]
struct ProvisionSharedKeyOAuthClientRequest {
    redirect_uri: String,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    previous_allowed_scopes: Option<Vec<String>>,
}

fn bridge_client_scopes_are_current(client: &crate::models::OAuthClientRecord) -> bool {
    let ceiling = bridge_oauth_scopes();
    !client.allowed_scopes.trim().is_empty()
        && client
            .allowed_scopes_vec()
            .iter()
            .all(|scope| ceiling.contains(&scope.as_str()))
}

#[handler]
pub(crate) async fn oauth_shared_key_client_provision(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };
    if !config.oauth2.enabled || !config.oauth2.shared_key_bridge_enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(
            serde_json::json!({"error": "shared-key OAuth bridge is not enabled"}),
        ));
        return;
    }
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "direct shared key required"}),
        ));
        return;
    };
    if !auth.is_shared_key() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(
            serde_json::json!({"error": "direct shared key required"}),
        ));
        return;
    }
    let Some(shared_key_hash) = auth.shared_key_hash.as_deref() else {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(
            serde_json::json!({"error": "direct shared key identity is unavailable"}),
        ));
        return;
    };
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };
    let Some(registry) = depot
        .obtain::<std::sync::Arc<crate::ShellClientRegistry>>()
        .ok()
    else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            serde_json::json!({"error": "Runner registry unavailable"}),
        ));
        return;
    };
    if !registry
        .has_connected_shared_key_group(shared_key_hash)
        .await
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": "shared-key Runner group is not connected"
        })));
        return;
    }
    let body: ProvisionSharedKeyOAuthClientRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": "invalid request body"})));
            return;
        }
    };
    let redirect_uri = body.redirect_uri.trim().to_string();
    if let Err(error) = validate_redirect_uri(&redirect_uri) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": error})));
        return;
    }

    if let Some(client_id) = body
        .client_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        match db.get_oauth_client_by_client_id(client_id) {
            Ok(Some(client)) => {
                if !client.is_shared_key_owned()
                    || client.owner_shared_key_hash.as_deref() != Some(shared_key_hash)
                {
                    res.status_code(StatusCode::FORBIDDEN);
                    res.render(Json(serde_json::json!({"error": "OAuth client belongs to a different shared-key group"})));
                    return;
                }
                if client.redirect_uris_vec() != vec![redirect_uri.clone()] {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(
                        serde_json::json!({"error": "OAuth client redirect URI does not match"}),
                    ));
                    return;
                }
                if !bridge_client_scopes_are_current(&client) {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({"error": "persisted OAuth client exceeds the current shared-key bridge scope ceiling"})));
                    return;
                }
                apply_oauth_no_store_headers(res);
                res.render(Json(serde_json::json!({
                    "success": true,
                    "reused": true,
                    "client": {
                        "client_id": client.client_id,
                        "redirect_uri": redirect_uri,
                        "allowed_scopes": client.allowed_scopes_vec(),
                    }
                })));
                return;
            }
            Ok(None) => {}
            Err(_) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({
                    "error": "failed to read existing OAuth client"
                })));
                return;
            }
        }
    }

    let create_scopes = if let Some(previous) = body.previous_allowed_scopes.as_ref() {
        let ceiling = bridge_oauth_scopes();
        if previous.is_empty()
            || previous
                .iter()
                .any(|scope| !ceiling.contains(&scope.as_str()))
            || previous
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != previous.len()
        {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(serde_json::json!({
                "error": "persisted OAuth scope ceiling is not valid for the current shared-key bridge"
            })));
            return;
        }
        ceiling
            .iter()
            .copied()
            .filter(|scope| previous.iter().any(|item| item == scope))
            .collect::<Vec<_>>()
    } else {
        bridge_oauth_scopes().to_vec()
    };

    let plaintext_secret = crate::auth::generate_oauth_client_secret();
    let record = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: hash_token(&plaintext_secret),
        name: "WebCodex hosted shared-key bridge".to_string(),
        owner_user_id: None,
        owner_project_grant_id: None,
        owner_shared_key_hash: Some(shared_key_hash.to_string()),
        redirect_uris: redirect_uri.clone(),
        allowed_scopes: create_scopes.join(" "),
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    };
    if db.insert_oauth_client(&record).is_err() {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            serde_json::json!({"error": "failed to create OAuth client"}),
        ));
        return;
    }
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "success": true,
        "reused": false,
        "client": {
            "client_id": record.client_id,
            "redirect_uri": redirect_uri,
            "allowed_scopes": record.allowed_scopes_vec(),
        },
        "client_secret": plaintext_secret,
    })));
}

#[handler]
pub(crate) async fn oauth_authorize_bridge(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    if !config.oauth2.enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "OAuth2 is not enabled",
        );
        return;
    }

    if !config.oauth2.shared_key_bridge_enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "shared-key OAuth bridge is not enabled",
        );
        return;
    }

    let Some(db) = crate::auth::get_db(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    let pairs = match parse_form_body(req).await {
        Some(pairs) => pairs,
        None => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid request body",
            );
            return;
        }
    };

    let query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs.iter().filter(|(key, _)| key != "shared_key") {
            serializer.append_pair(key, value);
        }
        serializer.finish()
    };

    match is_shared_key_bridge_query(&query) {
        Ok(true) => {}
        _ => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "unsupported bridge",
            );
            return;
        }
    }

    let Some(validated) = validate_bridge_authorize_request(res, &config, &db, &query) else {
        return;
    };

    let submitted = form_field(&pairs, "shared_key").unwrap_or("");
    let shared_key_hash = match bridge_shared_key_hash(submitted) {
        Ok(hash) => hash,
        Err(message) => {
            render_bridge_authorize_form(res, &validated, &query, Some(message));
            return;
        }
    };
    if let Some(owner_hash) = validated.client.owner_shared_key_hash.clone() {
        if !crate::config::constant_time_eq(shared_key_hash.as_bytes(), owner_hash.as_bytes()) {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some("shared key is not valid for this OAuth client"),
            );
            return;
        }
        let Some(registry) = depot
            .obtain::<std::sync::Arc<crate::ShellClientRegistry>>()
            .ok()
        else {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Runner registry unavailable",
            );
            return;
        };
        if !registry.has_connected_shared_key_group(&owner_hash).await {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some("shared-key Runner group is no longer connected"),
            );
            return;
        }
    }
    issue_bridge_authorization_code(res, &config, &db, &validated, shared_key_hash);
}
