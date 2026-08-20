//! WebCodex authentication and authorization.
//!
//! This module implements the bearer-token authentication pipeline used by all
//! protected API endpoints. It supports bootstrap, personal API tokens, agent
//! tokens, account credentials, OAuth2 access tokens, quick-start shared keys,
//! and explicit open-anonymous mode.
//!
//! ## Submodules
//!
//! - [`context`] — [`AuthContext`], [`AuthKind`], and [`AuthError`].
//! - [`scopes`] — scope constants, validation, and authorization helpers.
//! - [`pat`] — PAT / agent token / account credential generation, hashing, and
//!   validation utilities.
//! - [`shared_key`] — shared-key and open-anonymous lightweight auth helpers.
//! - [`tokens`] — bearer token verification and token-kind classification.
//! - [`middleware`] — HTTP request extraction, token surface gates, and Salvo
//!   auth middleware.
//!
//! ## Architecture
//!
//! The [`AuthMiddleware`] Salvo handler is the single entry point for HTTP
//! authentication. It extracts a bearer token, validates it, and injects an
//! [`AuthContext`] into the depot. Handlers extract `AuthContext` and pass it
//! to the tool runtime for scope-based authorization.
//!
//! ## Token Verifier Chain
//!
//! The [`TokenVerifier`] trait is the extension point for bearer token
//! verification. The verifier chain currently runs [`PatVerifier`] followed by
//! [`OAuth2Verifier`].

use crate::{Config, Database};
use std::sync::Arc;

#[cfg(test)]
use salvo::prelude::*;

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

mod context;
mod project_credential;
pub mod scopes;

// `pat` is `pub(crate)` — its functions are internal utilities.
pub(crate) mod middleware;
pub(crate) mod pat;
pub(crate) mod project_share;
pub(crate) mod shared_key;
pub(crate) mod tokens;

// ---------------------------------------------------------------------------
// Re-exports — backward compatibility
// ---------------------------------------------------------------------------
// All items that were previously exported from `auth.rs` are re-exported here
// so that existing `use crate::auth::*` imports continue to work.

pub use context::{AuthContext, AuthError, AuthKind};
pub(crate) use project_credential::{
    read_protected_secret, validate_agent_token as validate_project_agent_token,
    validate_credential as validate_project_credential, ProjectAgentTokenVerifier,
    ProjectCredentialVerifier,
};
pub(crate) use project_share::{
    configured_project_share_subject, generate_project_share_session_id,
    parse_project_share_subject_id, project_share_scopes_are_bounded, validate_project_grant_id,
    validate_project_share_grant_subject, PROJECT_SHARE_OAUTH_SCOPES,
    PROJECT_SHARE_OAUTH_SUBJECT_KIND, PROJECT_SHARE_OAUTH_TOKEN_KIND,
};

pub use scopes::{
    AGENT_SCOPES, SCOPE_ADMIN, SCOPE_AGENT_JOB_UPDATE, SCOPE_AGENT_POLL, SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_RESULT, SCOPE_COMPUTER_CLIPBOARD_READ, SCOPE_COMPUTER_CLIPBOARD_WRITE,
    SCOPE_COMPUTER_CONTROL, SCOPE_COMPUTER_DISPLAY_READ, SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_POINTER_CONTROL, SCOPE_COMPUTER_READ, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};
#[cfg(test)]
pub use scopes::{SCOPE_ACCOUNT_MANAGE, SCOPE_JOB_DETACH};

pub(crate) use scopes::{is_agent_scope, scopes_to_string, validate_agent_scopes, validate_scopes};

#[cfg(test)]
pub(crate) use middleware::{
    allow_query_token_for_path, enforce_token_surface, is_account_control_path,
    is_agent_transport_path, ACCOUNT_CONTROL_PATHS, AGENT_TRANSPORT_PATHS,
};
pub(crate) use middleware::{
    bearer_token, get_config, get_db, json_error, oauth_insufficient_scope_challenge,
    render_scope_forbidden, require_json_same_origin, require_same_origin, scope_forbidden_body,
    AuthMiddleware,
};

pub(crate) use pat::{
    clean_token_name, generate_account_credential, generate_agent_token, generate_api_token,
    generate_oauth_access_token, generate_oauth_authorization_code, generate_oauth_client_id,
    generate_oauth_client_secret, generate_oauth_refresh_token, hash_token,
    is_unique_constraint_error, normalize_token_hash, token_prefix, validate_allowed_client_id,
    validate_role, validate_token_prefix, validate_username,
};
pub(crate) use shared_key::{
    allow_anonymous_enabled, is_managed_token_prefix, open_anonymous_context, shared_key_context,
    shared_key_enabled, shared_key_hash_of,
};

pub(crate) use tokens::{authenticate, is_oauth2_access_token};
#[cfg(test)]
pub(crate) use tokens::{OAuth2Verifier, PatVerifier, TokenVerifier};

#[cfg(test)]
pub(crate) struct AuthEnvGuard {
    _env_lock: std::sync::MutexGuard<'static, ()>,
    shared_key_enabled: Option<std::ffi::OsString>,
    allow_anonymous: Option<std::ffi::OsString>,
    oauth2_shared_key_bridge: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl AuthEnvGuard {
    pub(crate) fn new() -> Self {
        let env_lock = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        Self {
            _env_lock: env_lock,
            shared_key_enabled: std::env::var_os("WEBCODEX_SHARED_KEY_ENABLED"),
            allow_anonymous: std::env::var_os("WEBCODEX_ALLOW_ANONYMOUS"),
            oauth2_shared_key_bridge: std::env::var_os("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE"),
        }
    }

    pub(crate) fn auth_required() -> Self {
        let guard = Self::new();
        guard.disable_direct_shared_key();
        guard.disable_open_anonymous();
        guard.disable_oauth2_shared_key_bridge();
        guard
    }

    pub(crate) fn enable_direct_shared_key(&self) {
        std::env::set_var("WEBCODEX_SHARED_KEY_ENABLED", "true");
    }

    pub(crate) fn disable_direct_shared_key(&self) {
        std::env::remove_var("WEBCODEX_SHARED_KEY_ENABLED");
    }

    pub(crate) fn enable_open_anonymous(&self) {
        std::env::set_var("WEBCODEX_ALLOW_ANONYMOUS", "true");
    }

    pub(crate) fn disable_open_anonymous(&self) {
        std::env::remove_var("WEBCODEX_ALLOW_ANONYMOUS");
    }

    pub(crate) fn enable_oauth2_shared_key_bridge(&self) {
        std::env::set_var("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE", "true");
    }

    pub(crate) fn disable_oauth2_shared_key_bridge(&self) {
        std::env::remove_var("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE");
    }
}

#[cfg(test)]
impl Drop for AuthEnvGuard {
    fn drop(&mut self) {
        restore_test_env("WEBCODEX_SHARED_KEY_ENABLED", &self.shared_key_enabled);
        restore_test_env("WEBCODEX_ALLOW_ANONYMOUS", &self.allow_anonymous);
        restore_test_env(
            "WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE",
            &self.oauth2_shared_key_bridge,
        );
    }
}

#[cfg(test)]
fn restore_test_env(name: &str, value: &Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

// ---------------------------------------------------------------------------
// Standalone authentication function (used by QUIC agent transport)
// ---------------------------------------------------------------------------

/// Authenticate a bearer token *outside* the HTTP request path, reusing the
/// same verifier chain as [`AuthMiddleware`]. Used by the QUIC agent
/// transport, which has no HTTP middleware to inject an `AuthContext`.
///
/// Authentication coverage:
/// - **Bootstrap token**: yes — returns bootstrap context.
/// - **Personal API token (`wc_pat_*`)**: yes — returns `AuthKind::ApiToken`.
/// - **Agent token (`wc_agent_*`)**: yes — returns `AuthKind::AgentToken`.
///   The agent-transport path gate does NOT apply here: the QUIC listener is
///   inherently an agent-only transport, so an agent token reaching it is
///   already on an allowed surface.
/// - **Account credential (`wc_acct_*`)**: **rejected** — returns `None`.
///   Account credentials are only valid on HTTP account-control endpoints.
///   The QUIC/agent transport has no use for them, and accepting them would
///   silently update `last_used_at` before the caller rejects the connection.
/// - **OAuth2 access token (`wc_oat_*`)**: **rejected** — returns `None`
///   *before* running the verifier chain, so `last_used_at` is not updated.
///   OAuth2 tokens are accepted on regular HTTP surfaces via `AuthMiddleware`,
///   but not on the QUIC/agent transport surface.
///
/// Returns `None` for unknown/invalid tokens or when the token is recognized
/// but rejected (disabled user, expired token, account credential). The
/// caller MUST treat `None` as "reject the connection".
pub(crate) async fn authenticate_bearer(
    config: &Config,
    db: Option<&Arc<Database>>,
    token: Option<&str>,
) -> Option<AuthContext> {
    // Auth disabled in development -> bootstrap (full access), identical to
    // AuthMiddleware's behavior. This lets local QUIC integration tests run
    // without a configured token.
    if !config.is_auth_enabled() {
        return Some(bootstrap_context());
    }
    // No token: only allowed when the server is explicitly --open.
    let token = match token {
        Some(t) => t,
        None => {
            if allow_anonymous_enabled() {
                return Some(open_anonymous_context());
            }
            return None;
        }
    };
    // Pre-reject OAuth2 access tokens before running the verifier chain.
    // OAuth2Verifier updates last_used_at on success, so we must not let it
    // run on a surface that will ultimately reject the token. The QUIC/agent
    // transport surface does not accept OAuth2 tokens.
    if is_oauth2_access_token(token) {
        return None;
    }
    // Run the same verifier chain as the HTTP path (PatVerifier →
    // OAuth2Verifier). Any error (disabled user, expired token) is treated
    // the same as "unknown" for the QUIC transport — the caller rejects
    // the connection either way.
    match authenticate(config, db, token).await {
        Ok(Some(ctx)) => {
            // Account credentials are not valid on the agent transport surface.
            // Reject them here so they don't silently update last_used_at and then
            // get rejected by the caller anyway.
            if ctx.is_account_credential() {
                return None;
            }
            Some(ctx)
        }
        Ok(None) => {
            // Unknown bearer token: treat as a lightweight shared key only
            // when quick-start mode is enabled, the token is non-empty after
            // trimming, and it does not look like a WebCodex managed credential.
            let trimmed = token.trim();
            if shared_key_enabled() && !trimmed.is_empty() && !is_managed_token_prefix(trimmed) {
                Some(shared_key_context(trimmed))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Build the bootstrap `AuthContext` used when auth is disabled or the
/// server-wide `WEBCODEX_TOKEN` is presented. Kept private to `auth`; the only
/// callers are `AuthMiddleware` (inline) and `authenticate_bearer`.
fn bootstrap_context() -> AuthContext {
    AuthContext {
        role: Some("admin".to_string()),
        scopes: vec![SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        ..AuthContext::new(AuthKind::Bootstrap)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
