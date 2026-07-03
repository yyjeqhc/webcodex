//! Bearer token verification and token-kind classification.

use std::sync::Arc;

use crate::{Config, Database};
use salvo::prelude::async_trait;

use super::pat::hash_token;
use super::scopes::SCOPE_ACCOUNT_MANAGE;
use super::{bootstrap_context, AuthContext, AuthError, AuthKind};

// ---------------------------------------------------------------------------
// TokenVerifier — the trait for bearer token verification
// ---------------------------------------------------------------------------

/// A `TokenVerifier` validates a bearer token and returns an [`AuthContext`] on
/// success.
///
/// This trait is the extension point for plugging in alternative verification
/// strategies. The current implementations are [`PatVerifier`] and
/// [`OAuth2Verifier`].
///
/// ## Design notes
///
/// The trait is `Send + Sync` so it can be stored in shared state (e.g. behind
/// an `Arc`). Implementations receive `&Config` and `Option<&Database>` so they
/// can perform the full validation chain (bootstrap token check, database
/// lookup, etc.) without owning those resources.
#[async_trait]
pub(crate) trait TokenVerifier: Send + Sync {
    /// Attempt to verify the given bearer token.
    ///
    /// Returns `Ok(Some(AuthContext))` on success, `Ok(None)` when this verifier
    /// does not recognize the token format (allowing a chained verifier to try),
    /// and `Err` for hard failures.
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String>;
}

/// PAT / bootstrap verifier — the existing validation logic wrapped in the
/// [`TokenVerifier`] trait.
///
/// This verifier handles:
/// 1. Auth-disabled mode (returns bootstrap)
/// 2. Bootstrap token match
/// 3. Database lookup by SHA-256 hash for API keys and account credentials
pub(crate) struct PatVerifier;

#[async_trait]
impl TokenVerifier for PatVerifier {
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String> {
        // Auth disabled in development -> bootstrap (full access), identical to
        // AuthMiddleware's `!config.is_auth_enabled()` branch.
        if !config.is_auth_enabled() {
            return Ok(Some(bootstrap_context()));
        }

        // Bootstrap token check (constant-time comparison).
        if config.validate_token(token) {
            return Ok(Some(bootstrap_context()));
        }

        // Database lookup.
        let Some(db) = db else {
            return Ok(None);
        };
        let token_hash = hash_token(token);

        // API key lookup (personal API tokens and agent tokens).
        if let Ok(Some(api_key)) = db.get_api_key_by_hash(&token_hash) {
            let user = db
                .get_user_by_id(&api_key.user_id)
                .ok()
                .flatten()
                .ok_or_else(|| "user not found".to_string())?;

            if user.is_disabled() {
                return Err("user is disabled".to_string());
            }

            let now = chrono::Utc::now().timestamp();
            if api_key.is_expired(now) {
                return Err("token expired".to_string());
            }

            if let Err(e) = db.update_api_key_last_used(&api_key.id, now) {
                tracing::warn!("failed to update api key last_used_at: {}", e);
            }

            let auth_kind = if api_key.is_agent_token() {
                AuthKind::AgentToken
            } else {
                AuthKind::ApiToken
            };

            return Ok(Some(AuthContext {
                kind: auth_kind,
                user_id: Some(user.id.clone()),
                username: Some(user.username.clone()),
                api_key_id: Some(api_key.id.clone()),
                api_key_name: Some(api_key.name.clone()),
                role: Some(user.role.clone()),
                scopes: api_key.scopes_vec(),
                is_bootstrap: false,
                token_kind: Some(api_key.kind().to_string()),
                allowed_client_id: api_key.allowed_client_id.clone(),
                shared_key_hash: None,
            }));
        }

        // Account credential lookup.
        if let Ok(Some(account_credential)) = db.get_account_credential_by_hash(&token_hash) {
            let user = db
                .get_user_by_id(&account_credential.user_id)
                .ok()
                .flatten()
                .ok_or_else(|| "user not found".to_string())?;

            if user.is_disabled() {
                return Err("user is disabled".to_string());
            }

            let now = chrono::Utc::now().timestamp();
            if let Err(e) = db.update_account_credential_last_used(&account_credential.id, now) {
                tracing::warn!("failed to update account credential last_used_at: {}", e);
            }

            return Ok(Some(AuthContext {
                kind: AuthKind::AccountCredential,
                user_id: Some(user.id.clone()),
                username: Some(user.username.clone()),
                api_key_id: None,
                api_key_name: None,
                role: Some(user.role.clone()),
                scopes: vec![SCOPE_ACCOUNT_MANAGE.to_string()],
                is_bootstrap: false,
                token_kind: Some("account".to_string()),
                allowed_client_id: None,
                shared_key_hash: None,
            }));
        }

        // Token not recognized by any verifier.
        Ok(None)
    }
}

/// OAuth2 bearer token verifier.
///
/// Validates WebCodex-issued opaque OAuth2 access tokens (`wc_oat_*`). The
/// database stores only SHA-256 hashes; the plaintext token is never persisted.
///
/// Validation steps:
/// 1. Token must start with `wc_oat_` — non-matching tokens return `Ok(None)`
///    (not recognized), allowing `PatVerifier` to handle them.
/// 2. OAuth2 must be enabled in config; otherwise returns `Ok(None)`.
/// 3. Hash the plaintext token and look up `oauth_access_tokens`.
/// 4. Token must not be revoked (`revoked_at IS NULL` — enforced by the query).
/// 5. Token must not be expired (`expires_at > now`).
/// 6. The owning client must not be revoked.
/// 7. The owning user must not be disabled.
/// 8. On success, `last_used_at` is updated and an `AuthContext` with
///    `AuthKind::OAuth2Token` is returned.
///
/// Refresh tokens (`wc_ort_*`), authorization codes (`wc_oac_*`), client
/// secrets (`wc_csec_*`), and client IDs (`wc_client_*`) are never accepted.
pub(crate) struct OAuth2Verifier;

/// Prefix for OAuth2 access tokens. Only tokens starting with this prefix are
/// handled by [`OAuth2Verifier`]; all others return `Ok(None)`.
const OAUTH2_ACCESS_TOKEN_PREFIX: &str = "wc_oat_";

/// Returns `true` when `token` looks like an OAuth2 access token by prefix.
///
/// This is a cheap text check — no DB access, no secret logging. Used by
/// [`crate::auth::authenticate_bearer`] and [`crate::auth::AuthMiddleware`] to
/// pre-reject OAuth2 tokens on forbidden surfaces **before** [`OAuth2Verifier`]
/// runs, so that `last_used_at` is not updated for rejected attempts.
pub(crate) fn is_oauth2_access_token(token: &str) -> bool {
    token.starts_with(OAUTH2_ACCESS_TOKEN_PREFIX)
}

#[async_trait]
impl TokenVerifier for OAuth2Verifier {
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String> {
        // Only handle wc_oat_* tokens. Non-matching tokens are not recognized by
        // this verifier — let PatVerifier try.
        if !token.starts_with(OAUTH2_ACCESS_TOKEN_PREFIX) {
            return Ok(None);
        }

        // If OAuth2 is not enabled, treat as not recognized. This avoids
        // rejecting tokens when the subsystem is simply disabled.
        if !config.oauth2.enabled {
            return Ok(None);
        }

        let Some(db) = db else {
            return Ok(None);
        };

        let token_hash = hash_token(token);
        let now = chrono::Utc::now().timestamp();

        // Look up the access token (revoked_at IS NULL is enforced by the
        // query).
        let at_record = match db.get_oauth_access_token_by_hash(&token_hash) {
            Ok(Some(record)) => record,
            Ok(None) => {
                // Token not found or already revoked — reject.
                return Err("invalid or revoked OAuth2 access token".to_string());
            }
            Err(_) => {
                return Err("internal error".to_string());
            }
        };

        // Check expiry.
        if at_record.is_expired(now) {
            return Err("expired OAuth2 access token".to_string());
        }

        // Verify the owning client is not revoked.
        match db.get_oauth_client_by_client_id(&at_record.client_id) {
            Ok(Some(_)) => {} // client is active
            Ok(None) => {
                return Err("OAuth2 client is revoked".to_string());
            }
            Err(_) => {
                return Err("internal error".to_string());
            }
        }

        let ctx = match at_record.subject_kind.as_str() {
            "managed_user" => {
                let user_id = at_record
                    .user_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "managed-user OAuth2 token missing user_id".to_string())?;
                if at_record.subject_id != user_id {
                    return Err("managed-user OAuth2 subject_id does not match user_id".to_string());
                }

                // Verify the owning user is not disabled (consistent with
                // PatVerifier behavior).
                let user = db
                    .get_user_by_id(user_id)
                    .ok()
                    .flatten()
                    .ok_or_else(|| "user not found".to_string())?;

                if user.is_disabled() {
                    return Err("user is disabled".to_string());
                }

                AuthContext {
                    kind: AuthKind::OAuth2Token,
                    user_id: Some(user.id.clone()),
                    username: Some(user.username.clone()),
                    // OAuth2 tokens don't map to an api_keys row. Use the
                    // access token ID as the credential identifier.
                    api_key_id: Some(at_record.id.clone()),
                    api_key_name: None,
                    role: Some(user.role.clone()),
                    scopes: at_record.scopes_vec(),
                    is_bootstrap: false,
                    token_kind: Some("oauth2".to_string()),
                    allowed_client_id: Some(at_record.client_id.clone()),
                    shared_key_hash: None,
                }
            }
            "shared_key" => {
                if at_record.user_id.is_some() {
                    return Err("shared-key OAuth2 token must not include user_id".to_string());
                }
                let shared_key_hash = at_record
                    .shared_key_hash
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "shared-key OAuth2 token missing shared_key_hash".to_string())?;
                if at_record.subject_id != shared_key_hash {
                    return Err(
                        "shared-key OAuth2 subject_id does not match shared_key_hash".to_string(),
                    );
                }

                AuthContext {
                    kind: AuthKind::OAuth2Token,
                    user_id: None,
                    username: None,
                    api_key_id: Some(at_record.id.clone()),
                    api_key_name: None,
                    role: Some("shared-key".to_string()),
                    scopes: at_record.scopes_vec(),
                    is_bootstrap: false,
                    token_kind: Some("oauth2_shared_key".to_string()),
                    allowed_client_id: Some(at_record.client_id.clone()),
                    shared_key_hash: Some(shared_key_hash.to_string()),
                }
            }
            _ => return Err("unsupported OAuth2 subject".to_string()),
        };

        // All checks passed — update last_used_at.
        if let Err(e) = db.update_oauth_access_token_last_used(&at_record.id, now) {
            tracing::warn!("failed to update oauth access token last_used_at: {}", e);
        }

        Ok(Some(ctx))
    }
}

// ---------------------------------------------------------------------------
// Verifier chain — shared authentication logic
// ---------------------------------------------------------------------------

/// Run the token through the verifier chain and return an [`AuthContext`].
///
/// Verifiers are tried in order: [`PatVerifier`], then [`OAuth2Verifier`]. The
/// first verifier that returns `Ok(Some(ctx))` wins. If a verifier returns
/// `Err`, authentication fails immediately (the token was recognized but
/// invalid — e.g. disabled user or expired token). If all verifiers return
/// `Ok(None)`, the token is unknown and the caller should return 401.
///
/// This is the **single** token verification path used by both the HTTP
/// [`crate::auth::AuthMiddleware`] and the non-HTTP
/// [`crate::auth::authenticate_bearer`].
pub(crate) async fn authenticate(
    config: &Config,
    db: Option<&Arc<Database>>,
    token: &str,
) -> Result<Option<AuthContext>, AuthError> {
    let verifiers: &[&dyn TokenVerifier] = &[&PatVerifier, &OAuth2Verifier];

    for verifier in verifiers {
        match verifier.verify(config, db, token).await {
            Ok(Some(ctx)) => return Ok(Some(ctx)),
            Ok(None) => continue,
            Err(_) => return Err(AuthError::InvalidToken),
        }
    }

    Ok(None)
}
