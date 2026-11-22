use salvo::prelude::*;

use super::{oauth_discovery_scopes_supported, OAUTH_OFFLINE_ACCESS_SCOPE};

fn advertised_scopes(config: &crate::Config) -> Vec<&'static str> {
    if config.oauth2.project_share_grant_id.is_some() {
        let mut scopes = crate::auth::PROJECT_SHARE_OAUTH_SCOPES.to_vec();
        scopes.push(OAUTH_OFFLINE_ACCESS_SCOPE);
        scopes
    } else {
        oauth_discovery_scopes_supported()
    }
}

/// Return protected resource metadata (RFC 9728 §3.1).
///
/// This is a **public** endpoint — no authentication required. Returns 404
/// when OAuth2 is disabled so discovery does not advertise capabilities that
/// are not active.
#[handler]
pub(crate) async fn oauth_metadata(depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };

    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }

    let issuer = config
        .oauth2
        .issuer
        .as_deref()
        .unwrap_or("http://localhost");
    let resource = format!("{}/mcp", issuer.trim_end_matches('/'));

    let mut metadata = serde_json::json!({
        "resource": resource,
        "authorization_servers": [issuer],
        "bearer_methods_supported": ["header"],
        "resource_name": "WebCodex",
    });
    // A global OAuth client may have a narrower registered scope ceiling.
    // An unqualified resource-level All-scopes request would then fail
    // invalid_scope. Only a Project Share has a fixed, authorizable ceiling.
    if config.oauth2.project_share_grant_id.is_some() {
        metadata["scopes_supported"] = serde_json::json!(advertised_scopes(&config));
    }

    res.render(Json(metadata));
}

/// Return OAuth Authorization Server Metadata (RFC 8414).
///
/// This is a **public** endpoint — no authentication required. It advertises
/// only capabilities implemented by the current OAuth2 server.
#[handler]
pub(crate) async fn oauth_authorization_server_metadata(depot: &mut Depot, res: &mut Response) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };

    if !config.oauth2.enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "OAuth2 is not enabled"})));
        return;
    }

    let issuer = config
        .oauth2
        .issuer
        .as_deref()
        .unwrap_or("http://localhost");
    let endpoint_base = issuer.trim_end_matches('/');

    let mut metadata = serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/oauth/authorize", endpoint_base),
        "token_endpoint": format!("{}/oauth/token", endpoint_base),
        "revocation_endpoint": format!("{}/oauth/revoke", endpoint_base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post"],
        "authorization_response_iss_parameter_supported": config.oauth2.https_issuer().is_some(),
        "scopes_supported": advertised_scopes(&config),
    });

    if config
        .oauth2
        .registration_target(crate::public_ingress_auth::entry(depot))
        .is_some_and(|target| {
            target.fence.as_ref().is_none_or(|fence| {
                crate::auth::get_db(depot).is_some_and(|db| {
                    db.public_ingress_registration_available(fence)
                        .unwrap_or(false)
                })
            })
        })
    {
        metadata["registration_endpoint"] = format!("{endpoint_base}/oauth/register").into();
        metadata["token_endpoint_auth_methods_supported"] =
            serde_json::json!(["client_secret_post", "client_secret_basic", "none"]);
    }
    res.render(Json(metadata));
}
