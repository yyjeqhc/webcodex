use salvo::prelude::*;

use crate::auth::{
    configured_project_share_subject, project_share_scopes_are_bounded, PROJECT_SHARE_OAUTH_SCOPES,
    PROJECT_SHARE_OAUTH_SUBJECT_KIND,
};
use crate::models::OAuthAuthorizationCodeRecord;

use super::html::authorize_project_share_html;
use super::{
    form_field, normalize_oauth_scopes, oauth_authorize_direct_error, parse_authorize_query,
    parse_form_body, redirect_with_authorization_code, redirect_with_oauth_error,
    validate_authorize_resource, OAuthAuthorizeError, OAuthAuthorizeRequest,
    OAUTH_OFFLINE_ACCESS_SCOPE,
};

pub(crate) const PROJECT_SHARE_OAUTH_INVALID_SCOPE_MESSAGE: &str =
    "project share tokens are limited to runtime:read, project:read, project:write, job:run";

pub(crate) fn normalize_project_share_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let normalized = normalize_oauth_scopes(requested, client_allowed)?;
    if normalized.split_ascii_whitespace().any(|scope| {
        scope != OAUTH_OFFLINE_ACCESS_SCOPE && !PROJECT_SHARE_OAUTH_SCOPES.contains(&scope)
    }) {
        return Err(OAuthAuthorizeError::InvalidScope(
            PROJECT_SHARE_OAUTH_INVALID_SCOPE_MESSAGE,
        ));
    }
    Ok(normalized)
}

#[derive(Clone)]
pub(super) struct ProjectShareAuthorizeValidated {
    parsed: OAuthAuthorizeRequest,
    client: crate::models::OAuthClientRecord,
    scopes: String,
    resource: Option<String>,
}

pub(super) fn validate_project_share_authorize_request(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    query: &str,
) -> Option<ProjectShareAuthorizeValidated> {
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

    let Some(configured_grant_id) = config.oauth2.project_share_grant_id.as_deref() else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "project share OAuth configuration is incomplete",
        );
        return None;
    };
    if client.owner_project_grant_id.as_deref() != Some(configured_grant_id)
        || !client.is_project_grant_owned()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "invalid client_id",
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

    if parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return None;
    }
    if parsed.code_challenge.is_empty() || parsed.code_challenge_method != "S256" {
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
        match normalize_project_share_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes)
        {
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
    if !project_share_scopes_are_bounded(&scopes) {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "project share scope ceiling was violated",
        );
        return None;
    }

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

    Some(ProjectShareAuthorizeValidated {
        parsed,
        client,
        scopes,
        resource,
    })
}

pub(super) fn render_project_share_authorize_form(
    res: &mut Response,
    validated: &ProjectShareAuthorizeValidated,
    query: &str,
    error: Option<&str>,
) {
    let scopes = validated
        .scopes
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let html = authorize_project_share_html(
        &validated.client.name,
        &validated.client.client_id,
        &validated.parsed.redirect_uri,
        &scopes,
        validated.resource.as_deref(),
        query,
        error,
    );
    res.status_code(if error.is_some() {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::OK
    });
    res.render(Text::Html(html));
}

fn issue_project_share_authorization_code(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    validated: &ProjectShareAuthorizeValidated,
) {
    let subject_id = match configured_project_share_subject(config) {
        Ok(Some(subject_id)) => subject_id,
        _ => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "project share OAuth session is not active",
            );
            return;
        }
    };
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = crate::auth::generate_oauth_authorization_code();
    let code_hash = crate::auth::hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: validated.client.client_id.clone(),
        subject_kind: PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
        subject_id,
        user_id: None,
        redirect_uri: validated.parsed.redirect_uri.clone(),
        scopes: validated.scopes.clone(),
        resource: validated.resource.clone(),
        code_challenge: Some(validated.parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: None,
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

#[handler]
pub(crate) async fn oauth_authorize_project(
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
    if !config.oauth2.enabled
        || configured_project_share_subject(&config)
            .ok()
            .flatten()
            .is_none()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "project share OAuth is not enabled",
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
    for (key, value) in pairs.iter().filter(|(key, _)| key != "project_credential") {
        serializer.append_pair(key, value);
    }
    let query = serializer.finish();
    let Some(validated) = validate_project_share_authorize_request(res, &config, &db, &query)
    else {
        return;
    };

    let submitted = form_field(&pairs, "project_credential").unwrap_or("");
    let runtime = depot
        .obtain::<crate::connector_runtime::ConnectorRuntimeSlot>()
        .ok()
        .and_then(|slot| slot.0.clone());
    let Some(runtime) = runtime else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "project Connector runtime is unavailable",
        );
        return;
    };
    let authenticated = runtime
        .authenticate_project_credential(submitted)
        .and_then(|ctx| ctx.project_grant_id)
        .is_some_and(|grant_id| {
            config.oauth2.project_share_grant_id.as_deref() == Some(grant_id.as_str())
        });
    if !authenticated {
        render_project_share_authorize_form(
            res,
            &validated,
            &query,
            Some("invalid project share credential"),
        );
        return;
    }

    issue_project_share_authorization_code(res, &config, &db, &validated);
}
