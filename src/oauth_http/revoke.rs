use salvo::prelude::*;

use crate::auth::hash_token;

use super::apply_oauth_no_store_headers;
use super::responses::{
    authenticate_token_client, enabled_token_config, parse_token_form, required_token_parameter,
    OAuthTokenError,
};

#[derive(Debug, serde::Deserialize)]
struct RevokeRequest {
    token: Option<String>,
    token_type_hint: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[handler]
pub(crate) async fn oauth_revoke(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), OAuthTokenError> {
    let config = enabled_token_config(depot)?;
    let form: RevokeRequest = parse_token_form(req).await?;
    let plaintext_token = required_token_parameter(form.token.as_deref(), "token")?;
    let (db, client) = authenticate_token_client(
        req,
        depot,
        &config,
        form.client_id.as_deref(),
        form.client_secret.as_deref(),
    )?;
    let client_id = client.client_id.as_str();

    // --- Revoke ---
    let token_hash = hash_token(plaintext_token);
    let now = chrono::Utc::now().timestamp();

    let fence = crate::public_ingress_auth::entry(depot).map(|entry| entry.fence());
    db.revoke_oauth_token_for_client(
        &token_hash,
        client_id,
        form.token_type_hint.as_deref(),
        now,
        fence.as_ref(),
    )
    .map_err(|_| OAuthTokenError::server_error("revocation unavailable"))?;

    // Always return 200 — idempotent, no token state disclosure.
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({})));
    Ok(())
}
