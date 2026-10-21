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
    let _config = enabled_token_config(depot)?;
    let form: RevokeRequest = parse_token_form(req).await?;
    let plaintext_token = required_token_parameter(form.token.as_deref(), "token")?;
    let (db, _) = authenticate_token_client(
        depot,
        form.client_id.as_deref(),
        form.client_secret.as_deref(),
    )?;
    let client_id = form.client_id.as_deref().expect("authenticated client_id");

    // --- Revoke ---
    let token_hash = hash_token(plaintext_token);
    let now = chrono::Utc::now().timestamp();

    let hint = form.token_type_hint.as_deref().unwrap_or("");

    // Per RFC 7009: if hint is provided and recognized, try only that type.
    // If hint is missing or unrecognized, try both.
    match hint {
        "access_token" => {
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
        }
        "refresh_token" => {
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
        _ => {
            // No hint or unrecognized hint — try both.
            let _ = db.revoke_oauth_access_token_by_hash_for_client(&token_hash, client_id, now);
            let _ = db.revoke_oauth_refresh_token_by_hash_for_client(&token_hash, client_id, now);
        }
    }

    // Always return 200 — idempotent, no token state disclosure.
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({})));
    Ok(())
}
