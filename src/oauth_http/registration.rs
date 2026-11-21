use salvo::prelude::*;
use serde::Deserialize;
use webcodex_store::DynamicRegistrationOutcome;

use super::clients::{registration_redirect_uris, MAX_CLIENT_NAME_LEN};
use super::responses::{apply_oauth_no_store_headers, OAuthTokenError};
use super::{normalize_oauth_scopes, OAUTH_OFFLINE_ACCESS_SCOPE};
use crate::models::{DynamicClientAuth, DynamicClientRegistration, OAuthClientRecord};

#[derive(Deserialize)]
struct RegistrationRequest {
    #[serde(default)]
    redirect_uris: Vec<String>,
    client_name: Option<String>,
    token_endpoint_auth_method: Option<DynamicClientAuth>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
    scope: Option<String>,
}

fn invalid_metadata(description: &'static str) -> OAuthTokenError {
    OAuthTokenError::new(
        StatusCode::BAD_REQUEST,
        "invalid_client_metadata",
        description,
    )
}

#[handler]
pub(crate) async fn oauth_register(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), OAuthTokenError> {
    let config =
        crate::auth::get_config(depot).ok_or_else(|| OAuthTokenError::server_error("no config"))?;
    let (owner, issuer) = config
        .oauth2
        .registration_target(crate::public_ingress_auth::entry(depot).is_some())
        .ok_or_else(|| {
            OAuthTokenError::new(
                StatusCode::NOT_FOUND,
                "invalid_request",
                "dynamic registration is disabled",
            )
        })?;
    crate::public_ingress_auth::require_json_authority(req, depot, &config).map_err(
        |(status, error, description)| {
            OAuthTokenError::new(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
                error,
                description,
            )
        },
    )?;
    let bytes = req
        .payload_with_max_size(super::MAX_OAUTH_TOKEN_FORM_BYTES)
        .await
        .map_err(|err| match err {
            salvo::http::ParseError::PayloadTooLarge => OAuthTokenError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "invalid_client_metadata",
                "registration body too large",
            ),
            _ => invalid_metadata("invalid registration body"),
        })?;
    let input: RegistrationRequest = serde_json::from_slice(bytes)
        .map_err(|_| invalid_metadata("invalid registration metadata"))?;
    let redirects = registration_redirect_uris(&input.redirect_uris).map_err(|reason| {
        OAuthTokenError::new(StatusCode::BAD_REQUEST, "invalid_redirect_uri", reason)
    })?;
    let name = input.client_name.as_deref().unwrap_or("MCP client").trim();
    if name.is_empty()
        || name.chars().count() > MAX_CLIENT_NAME_LEN
        || name.chars().any(char::is_control)
    {
        return Err(invalid_metadata("invalid client_name"));
    }
    if input.response_types.is_some_and(|types| types != ["code"]) {
        return Err(invalid_metadata("only the code response type is supported"));
    }
    let grants = input
        .grant_types
        .unwrap_or_else(|| vec!["authorization_code".into()]);
    let refresh = grants.iter().any(|g| g == "refresh_token");
    if !grants.iter().any(|g| g == "authorization_code")
        || grants.len() != if refresh { 2 } else { 1 }
    {
        return Err(invalid_metadata("unsupported grant_types"));
    }
    let auth_method = input
        .token_endpoint_auth_method
        .unwrap_or(DynamicClientAuth::Basic);
    let ceiling = webcodex_core::authority::profiles::LOCAL_USER.join(" ");
    let scopes = normalize_oauth_scopes(input.scope.as_deref(), &ceiling)
        .map_err(|_| invalid_metadata("unsupported scope"))?;
    let allowed_scopes = scopes
        .split_ascii_whitespace()
        .filter(|s| *s != OAUTH_OFFLINE_ACCESS_SCOPE)
        .collect::<Vec<_>>()
        .join(" ");
    if allowed_scopes.is_empty() {
        return Err(invalid_metadata("a WebCodex permission scope is required"));
    }
    let db = crate::auth::get_db(depot)
        .ok_or_else(|| OAuthTokenError::server_error("DB not available"))?;
    let secret =
        (auth_method != DynamicClientAuth::Public).then(crate::auth::generate_oauth_client_secret);
    let record = OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: secret
            .as_deref()
            .map(crate::auth::hash_token)
            .unwrap_or_default(),
        dynamic_registration: Some(DynamicClientRegistration {
            issuer: issuer.to_owned(),
            auth_method,
            refresh,
        }),
        name: name.to_owned(),
        owner_user_id: Some(owner.to_owned()),
        owner_project_grant_id: None,
        owner_shared_key_hash: None,
        redirect_uris: redirects.join("\n"),
        allowed_scopes,
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    };
    match db
        .register_dynamic_oauth_client(&record)
        .map_err(|_| OAuthTokenError::server_error("registration failed"))?
    {
        DynamicRegistrationOutcome::Created => {}
        DynamicRegistrationOutcome::OwnerUnavailable => {
            return Err(OAuthTokenError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "temporarily_unavailable",
                "registration owner is unavailable",
            ))
        }
        DynamicRegistrationOutcome::AtCapacity => {
            return Err(OAuthTokenError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "temporarily_unavailable",
                "registration capacity reached",
            ))
        }
    }
    let mut body = serde_json::json!({
        "client_id": record.client_id,
        "client_id_issued_at": record.created_at,
        "client_name": name,
        "redirect_uris": redirects,
        "response_types": ["code"],
        "grant_types": grants,
        "scope": scopes,
        "token_endpoint_auth_method": auth_method,
    });
    if let Some(secret) = secret {
        body["client_secret"] = secret.into();
        body["client_secret_expires_at"] = 0.into();
    }
    apply_oauth_no_store_headers(res);
    res.status_code(StatusCode::CREATED);
    res.render(Json(body));
    Ok(())
}
