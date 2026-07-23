//! Shared-key and open-anonymous lightweight auth helpers.

use sha2::{Digest, Sha256};

use super::context::{AuthContext, AuthKind};
use super::scopes::{
    SCOPE_AGENT_JOB_UPDATE, SCOPE_AGENT_POLL, SCOPE_AGENT_REGISTER, SCOPE_AGENT_RESULT,
    SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};

/// Read the explicit-anonymous (`--open`) flag from the environment. When true,
/// the server allows anonymous GPT/MCP and anonymous client access under the
/// open group. Default false — the server never offers anonymous service unless
/// the operator explicitly opts in.
pub(crate) fn allow_anonymous_enabled() -> bool {
    crate::config::env_flag("WEBCODEX_ALLOW_ANONYMOUS").unwrap_or(false)
}

/// Read the shared-key quick-start flag from the environment. When true,
/// unknown bearer tokens that do not look like WebCodex managed credentials
/// (`wc_*`) are accepted as lightweight shared keys instead of being rejected.
/// Default false — the server rejects unknown tokens unless the operator
/// explicitly enables quick-start mode (e.g. via `server up`).
pub(crate) fn shared_key_enabled() -> bool {
    crate::config::env_flag("WEBCODEX_SHARED_KEY_ENABLED").unwrap_or(false)
}

/// True when `token` uses a WebCodex managed-credential prefix. Tokens with
/// these prefixes that fail verifier-chain validation are rejected outright
/// rather than falling back to shared-key mode.
pub(crate) fn is_managed_token_prefix(token: &str) -> bool {
    token.starts_with("wc_")
}

/// SHA-256 hex of a shared key, used for lightweight group isolation. Two
/// requests presenting the same key land in the same group. The shared key is
/// trimmed before hashing so direct shared-key visibility and the OAuth bridge
/// derive the same group hash from the same submitted secret.
pub(crate) fn shared_key_hash_of(token: &str) -> String {
    let token = token.trim();
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Scopes granted to a shared-key or open-anonymous caller. These cover the
/// runtime, project, job, and agent-transport surfaces but deliberately exclude
/// `admin` and `account:manage` so lightweight keys cannot manage server-global
/// resources.
fn lightweight_scopes() -> Vec<String> {
    vec![
        SCOPE_RUNTIME_READ.to_string(),
        SCOPE_PROJECT_READ.to_string(),
        SCOPE_PROJECT_WRITE.to_string(),
        SCOPE_JOB_RUN.to_string(),
        SCOPE_AGENT_REGISTER.to_string(),
        SCOPE_AGENT_POLL.to_string(),
        SCOPE_AGENT_RESULT.to_string(),
        SCOPE_AGENT_JOB_UPDATE.to_string(),
    ]
}

/// Build a shared-key [`AuthContext`] for a lightweight bearer token. The caller
/// is non-admin and grouped by `shared_key_hash`.
pub(crate) fn shared_key_context(token: &str) -> AuthContext {
    AuthContext {
        kind: AuthKind::SharedKey,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: lightweight_scopes(),
        is_bootstrap: false,
        token_kind: Some("shared-key".to_string()),
        allowed_client_id: None,
        shared_key_hash: Some(shared_key_hash_of(token)),
    }
}

/// Build the open-anonymous [`AuthContext`] used only when the server is
/// started with explicit `--open`. Non-admin, single open group.
pub(crate) fn open_anonymous_context() -> AuthContext {
    AuthContext {
        kind: AuthKind::OpenAnonymous,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("open".to_string()),
        scopes: lightweight_scopes(),
        is_bootstrap: false,
        token_kind: Some("open".to_string()),
        allowed_client_id: None,
        shared_key_hash: None,
    }
}
