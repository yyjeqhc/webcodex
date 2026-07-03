use salvo::prelude::*;

use super::oauth_scopes_supported;

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

    let resource = config
        .oauth2
        .issuer
        .as_deref()
        .unwrap_or("http://localhost");

    let metadata = serde_json::json!({
        "resource": resource,
        "authorization_servers": [resource],
        "bearer_methods_supported": ["header"],
        "scopes_supported": oauth_scopes_supported(),
        "resource_name": "WebCodex",
    });

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

    let metadata = serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/oauth/authorize", endpoint_base),
        "token_endpoint": format!("{}/oauth/token", endpoint_base),
        "revocation_endpoint": format!("{}/oauth/revoke", endpoint_base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post"],
        "scopes_supported": oauth_scopes_supported(),
    });

    res.render(Json(metadata));
}
