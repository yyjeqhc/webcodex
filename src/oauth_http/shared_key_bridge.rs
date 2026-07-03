use salvo::prelude::*;

use crate::auth::{generate_oauth_authorization_code, hash_token, scopes, shared_key_hash_of};
use crate::models::OAuthAuthorizationCodeRecord;

use super::{
    authorize_bridge_html, decoded_authorize_param, form_field, normalize_oauth_scopes,
    oauth_authorize_direct_error, parse_authorize_query, parse_form_body,
    redirect_with_authorization_code, redirect_with_oauth_error, validate_authorize_resource,
    OAuthAuthorizeError, OAuthAuthorizeRequest,
};

const OAUTH_BRIDGE_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_JOB_RUN,
];

pub(crate) const OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE: &str =
    "bridge tokens are limited to runtime:read, project:read, project:write, job:run";

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
        .any(|scope| !OAUTH_BRIDGE_SCOPES_SUPPORTED.contains(&scope))
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
        &validated.parsed.redirect_uri,
        &plaintext_code,
        validated.parsed.state.as_deref(),
    );
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

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs.iter().filter(|(key, _)| key != "shared_key") {
        serializer.append_pair(key, value);
    }
    let query = serializer.finish();

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

    issue_bridge_authorization_code(res, &config, &db, &validated, shared_key_hash);
}
