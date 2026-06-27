use crate::{Config, Database};
use salvo::prelude::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// The kind of credential that produced an [`AuthContext`].
///
/// - `Bootstrap`: the server-wide `WEBCODEX_TOKEN` (admin/bootstrap auth).
/// - `ApiToken`: a Phase 2 personal API token (kind=`user`) backed by the
///   `api_keys` table.
/// - `AgentToken`: a Phase 3 agent token (kind=`agent`) bound to an owner
///   username and an `allowed_client_id`, usable only on agent transport
///   endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthKind {
    Bootstrap,
    ApiToken,
    AgentToken,
}

/// The set of scopes a Phase 2 personal API token may carry. Bootstrap auth is
/// treated as having the `admin` scope (full access). Stored space-separated in
/// the database; parsed into a list on read.
pub const SCOPE_RUNTIME_READ: &str = "runtime:read";
pub const SCOPE_PROJECT_READ: &str = "project:read";
pub const SCOPE_PROJECT_WRITE: &str = "project:write";
pub const SCOPE_JOB_RUN: &str = "job:run";
pub const SCOPE_AGENT_REGISTER: &str = "agent:register";
pub const SCOPE_ADMIN: &str = "admin";

/// Phase 3 agent transport scopes. Agent tokens may only carry `agent:*`
/// scopes and may only be used on agent transport endpoints. They are rejected
/// by all normal runtime/project/admin/user-token-management endpoints.
pub const SCOPE_AGENT_POLL: &str = "agent:poll";
pub const SCOPE_AGENT_RESULT: &str = "agent:result";
pub const SCOPE_AGENT_JOB_UPDATE: &str = "agent:job_update";

/// The complete set of agent transport scopes, in canonical order.
pub const AGENT_SCOPES: &[&str] = &[
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
];

/// All scopes recognized by this phase. Unknown scopes are rejected at token
/// creation time so the stored scope string stays clean.
pub const KNOWN_SCOPES: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
    SCOPE_ADMIN,
];

/// True when `scope` is one of the agent transport scopes.
pub(crate) fn is_agent_scope(scope: &str) -> bool {
    AGENT_SCOPES.contains(&scope)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthContext {
    pub kind: AuthKind,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub role: Option<String>,
    pub scopes: Vec<String>,
    pub is_bootstrap: bool,
    /// Phase 3: the token kind string (`"user"` or `"agent"`) from the
    /// underlying `api_keys` row. `None` for bootstrap auth.
    pub token_kind: Option<String>,
    /// Phase 3: the `allowed_client_id` bound to an agent token. `None` for
    /// bootstrap auth and user tokens.
    pub allowed_client_id: Option<String>,
}

impl AuthContext {
    /// True when the caller holds the `admin` scope or authenticated as the
    /// bootstrap token. Bootstrap is always treated as admin.
    #[allow(dead_code)]
    pub fn is_admin(&self) -> bool {
        self.is_bootstrap || self.scopes.iter().any(|s| s == SCOPE_ADMIN)
    }

    /// True when the caller holds the given scope (or is bootstrap/admin).
    #[allow(dead_code)]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.is_bootstrap || self.scopes.iter().any(|s| s == scope || s == SCOPE_ADMIN)
    }

    /// True when the caller authenticated as the bootstrap token (or auth is
    /// disabled in development).
    #[allow(dead_code)]
    pub fn is_bootstrap(&self) -> bool {
        self.is_bootstrap
    }

    /// True when the caller authenticated with a Phase 2 personal API token
    /// (kind=`user`).
    #[allow(dead_code)]
    pub fn is_user_token(&self) -> bool {
        matches!(self.kind, AuthKind::ApiToken)
    }

    /// True when the caller authenticated with a Phase 3 agent token
    /// (kind=`agent`).
    #[allow(dead_code)]
    pub fn is_agent_token(&self) -> bool {
        matches!(self.kind, AuthKind::AgentToken)
    }

    /// True when the caller may use an agent transport endpoint for the given
    /// `client_id`. Bootstrap may use any client_id. Agent tokens may only use
    /// the `allowed_client_id` they are bound to. User tokens may not use
    /// agent transport endpoints.
    #[allow(dead_code)]
    pub fn can_use_agent_endpoint(&self, client_id: &str) -> bool {
        if self.is_bootstrap {
            return true;
        }
        if matches!(self.kind, AuthKind::AgentToken) {
            return self
                .allowed_client_id
                .as_deref()
                .map(|allowed| allowed == client_id)
                .unwrap_or(false);
        }
        false
    }

    /// Require the caller to hold `scope`. Returns `Err(message)` when the
    /// scope is missing. Bootstrap is always treated as holding every scope.
    #[allow(dead_code)]
    pub fn require_scope(&self, scope: &str) -> Result<(), String> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(format!("missing required scope: {}", scope))
        }
    }
}

pub(crate) fn get_config(depot: &Depot) -> Option<Arc<Config>> {
    depot.obtain::<Arc<Config>>().ok().cloned()
}

pub(crate) fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

pub(crate) fn bearer_or_query_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
        .or_else(|| req.query::<String>("token"))
}

/// Hash a plaintext token with SHA-256 and return a lowercase hex digest.
///
/// This is the **single** place token hashing is performed; all token lookups
/// go through [`Database::get_api_key_by_hash`] using this digest. The digest
/// is compared by exact SQLite equality on the indexed `key_hash` column rather
/// than a byte-wise comparison in Rust, so a timing leak of the hash does not
/// reveal the secret. We do not currently use a keyed/HMAC hash (no server
/// secret is configured for hashing); this is acceptable for self-hosted use
/// and keeps the dependency surface small. Upgrading to a keyed hash is a
/// drop-in change here if a `WEBCODEX_TOKEN_HASH_KEY` is added later.
pub(crate) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Random component length (hex characters) for generated personal API tokens.
/// Two uuid v4 simple values concatenated = 64 hex chars = 256 bits of entropy.
const TOKEN_RANDOM_HEX_LEN: usize = 64;

/// Generate a fresh personal API token. Format: `wc_pat_<random>` where
/// `<random>` is 256 bits of hex-encoded randomness. The plaintext token is
/// returned **only** here (at creation time) and is never persisted; only its
/// SHA-256 hash is stored.
pub(crate) fn generate_api_token() -> String {
    let mut random = String::with_capacity(TOKEN_RANDOM_HEX_LEN);
    while random.len() < TOKEN_RANDOM_HEX_LEN {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(TOKEN_RANDOM_HEX_LEN);
    format!("wc_pat_{}", random)
}

/// Generate a fresh Phase 3 agent token. Format: `wc_agent_<random>` where
/// `<random>` is 256 bits of hex-encoded randomness. The distinct `wc_agent_`
/// prefix makes an agent token immediately recognizable in `token_prefix`
/// displays and logs so operators can tell it apart from a personal API token.
/// The plaintext token is returned **only** here (at creation time) and is
/// never persisted; only its SHA-256 hash is stored.
pub(crate) fn generate_agent_token() -> String {
    let mut random = String::with_capacity(TOKEN_RANDOM_HEX_LEN);
    while random.len() < TOKEN_RANDOM_HEX_LEN {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(TOKEN_RANDOM_HEX_LEN);
    format!("wc_agent_{}", random)
}

/// Return a short, display-safe prefix of a token (the first 16 characters,
/// including the `wc_pat_` / `wc_agent_` kind marker). Used for listing tokens
/// without revealing the secret.
pub(crate) fn token_prefix(token: &str) -> String {
    let end = token.len().min(16);
    token[..end].to_string()
}

/// Validate an `allowed_client_id` for an agent token. Applies the same safe-id
/// rules as `client_id`: non-empty, bounded length, ASCII alphanumeric, `-`,
/// `_`, and `.` only. Returns the trimmed value on success.
pub(crate) fn validate_allowed_client_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("allowed_client_id cannot be empty".to_string());
    }
    if value.chars().count() > 80 {
        return Err("allowed_client_id is too long; maximum is 80 characters".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(
            "allowed_client_id may only contain ASCII letters, digits, '-', '_', and '.'"
                .to_string(),
        );
    }
    Ok(value.to_string())
}

/// Validate and normalize a list of agent transport scopes. Returns an error
/// if any scope is not an `agent:*` scope. Rejects duplicates and unknown
/// scopes.
pub(crate) fn validate_agent_scopes(scopes: &[String]) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(scopes.len());
    for raw in scopes {
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if !is_agent_scope(s) {
            return Err(format!(
                "agent tokens may only carry agent:* scopes; got '{}'",
                s
            ));
        }
        if !seen.insert(s.to_string()) {
            continue;
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// Validate a username against the Phase 2 rules: non-empty, bounded length,
/// only lowercase ASCII letters, digits, `_`, and `-`. No slash, no `..`, no
/// whitespace, no uppercase. Returns the trimmed username on success.
pub(crate) fn validate_username(username: &str) -> Result<String, String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("username cannot be empty".to_string());
    }
    if username.chars().count() > 64 {
        return Err("username is too long; maximum is 64 characters".to_string());
    }
    if username.contains("..") {
        return Err("username cannot contain '..'".to_string());
    }
    for ch in username.chars() {
        let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if !ok {
            return Err(format!(
                "username contains an invalid character '{}'; only lowercase letters, digits, '_' and '-' are allowed",
                ch
            ));
        }
    }
    Ok(username.to_string())
}

/// Validate a role string. Must be `"admin"` or `"user"`.
pub(crate) fn validate_role(role: &str) -> Result<String, String> {
    let role = role.trim();
    match role {
        "admin" | "user" => Ok(role.to_string()),
        _ => Err(format!("role must be 'admin' or 'user', got '{}'", role)),
    }
}

/// Validate and normalize a list of scopes. Returns the cleaned scope list.
/// Rejects duplicates and unknown scopes.
pub(crate) fn validate_scopes(scopes: &[String]) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(scopes.len());
    for raw in scopes {
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if !KNOWN_SCOPES.contains(&s) {
            return Err(format!("unknown scope '{}'", s));
        }
        if !seen.insert(s.to_string()) {
            continue;
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// Serialize a scope list into the space-separated storage form.
pub(crate) fn scopes_to_string(scopes: &[String]) -> String {
    scopes.join(" ")
}

pub(crate) struct AuthMiddleware;

/// The exact set of authenticated paths an agent token (kind="agent") may use.
/// Any other authenticated path must reject agent tokens with a 403. This is
/// the central Phase 3 security gate enforced in [`AuthMiddleware`] before the
/// request reaches any handler, so per-handler owner-boundary checks cannot be
/// bypassed by a leaked agent token whose username matches an agent owner.
///
/// The paths are compared exactly (no prefix match) so a path like
/// `/api/agent-tokens/create` is correctly rejected for agent tokens even
/// though it starts with `/api/agent`.
pub(crate) const AGENT_TRANSPORT_PATHS: &[&str] = &[
    "/api/shell/agent/register",
    "/api/shell/agent/poll",
    "/api/shell/agent/result",
    "/api/shell/agent/job_update",
    "/api/agents/ws",
];

/// True when `path` is one of the exact agent transport endpoints an agent
/// token may call. Used by [`AuthMiddleware`] to gate agent tokens centrally.
pub(crate) fn is_agent_transport_path(path: &str) -> bool {
    AGENT_TRANSPORT_PATHS.contains(&path)
}

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(config) = get_config(depot) else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "No config"})));
            ctrl.skip_rest();
            return;
        };

        let db = get_db(depot);
        let token = bearer_or_query_token(req);

        if !config.is_auth_enabled() {
            let ctx = AuthContext {
                kind: AuthKind::Bootstrap,
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                role: Some("admin".to_string()),
                scopes: vec![SCOPE_ADMIN.to_string()],
                is_bootstrap: true,
                token_kind: None,
                allowed_client_id: None,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(token) = token else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        if config.validate_token(&token) {
            let ctx = AuthContext {
                kind: AuthKind::Bootstrap,
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                role: Some("admin".to_string()),
                scopes: vec![SCOPE_ADMIN.to_string()],
                is_bootstrap: true,
                token_kind: None,
                allowed_client_id: None,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(db) = db else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "DB not available"})));
            ctrl.skip_rest();
            return;
        };

        let key_hash = hash_token(&token);

        // Lookup is centralized in the DB layer; the same "Unauthorized"
        // message is used whether the token prefix exists or not, to avoid
        // leaking which prefixes are present.
        let Ok(Some(api_key)) = db.get_api_key_by_hash(&key_hash) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        let Ok(Some(user)) = db.get_user_by_id(&api_key.user_id) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        if user.is_disabled() {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        }

        let now = chrono::Utc::now().timestamp();
        if api_key.is_expired(now) {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        }

        if let Err(e) = db.update_api_key_last_used(&api_key.id, now) {
            tracing::warn!("failed to update api key last_used_at: {}", e);
        }

        // Phase 3: distinguish agent tokens (kind="agent") from personal API
        // tokens (kind="user"). Agent tokens authenticate but carry a
        // different AuthKind so handlers can reject them from normal runtime
        // endpoints and accept them only on agent transport endpoints.
        let auth_kind = if api_key.is_agent_token() {
            AuthKind::AgentToken
        } else {
            AuthKind::ApiToken
        };

        let ctx = AuthContext {
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
        };

        // Phase 3 central security gate: an agent token (kind="agent") may
        // only reach the exact agent transport endpoints. Everything else —
        // runtime, tools, projects, jobs, mcp, audit, users, tokens,
        // agent-tokens management — is forbidden. This prevents a leaked agent
        // token from accessing normal project/runtime APIs even when its
        // username matches an agent owner (per-handler owner-boundary checks
        // are not sufficient on their own). Bootstrap and user tokens are
        // unaffected.
        if ctx.is_agent_token() {
            let path = req.uri().path();
            if !is_agent_transport_path(path) {
                res.status_code(StatusCode::FORBIDDEN);
                res.render(Json(serde_json::json!({
                    "error": "agent tokens are only allowed on agent transport endpoints",
                })));
                ctrl.skip_rest();
                return;
            }
        }

        depot.inject(ctx);
        ctrl.call_next(req, depot, res).await;
    }
}

pub(crate) fn json_error(status: StatusCode, msg: impl Into<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": status.as_u16(),
        "error": msg.into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap_ctx() -> AuthContext {
        AuthContext {
            kind: AuthKind::Bootstrap,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: Some("admin".to_string()),
            scopes: vec![SCOPE_ADMIN.to_string()],
            is_bootstrap: true,
            token_kind: None,
            allowed_client_id: None,
        }
    }

    fn user_ctx(username: &str) -> AuthContext {
        AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-1".to_string()),
            api_key_name: Some("user key".to_string()),
            role: Some("user".to_string()),
            scopes: vec![SCOPE_RUNTIME_READ.to_string()],
            is_bootstrap: false,
            token_kind: Some("user".to_string()),
            allowed_client_id: None,
        }
    }

    fn agent_ctx(username: &str, client_id: &str, scopes: Vec<String>) -> AuthContext {
        AuthContext {
            kind: AuthKind::AgentToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-agent".to_string()),
            api_key_name: Some("agent key".to_string()),
            role: Some("user".to_string()),
            scopes,
            is_bootstrap: false,
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(client_id.to_string()),
        }
    }

    #[test]
    fn agent_token_auth_context_identifies_as_agent_token() {
        let ctx = agent_ctx(
            "alice",
            "alice-laptop",
            vec![
                SCOPE_AGENT_REGISTER.to_string(),
                SCOPE_AGENT_POLL.to_string(),
            ],
        );
        assert!(ctx.is_agent_token());
        assert!(!ctx.is_user_token());
        assert!(!ctx.is_bootstrap());
        assert_eq!(ctx.token_kind.as_deref(), Some("agent"));
        assert_eq!(ctx.allowed_client_id.as_deref(), Some("alice-laptop"));
        assert_eq!(ctx.username.as_deref(), Some("alice"));
    }

    #[test]
    fn user_token_auth_context_does_not_get_agent_kind() {
        let ctx = user_ctx("alice");
        assert!(ctx.is_user_token());
        assert!(!ctx.is_agent_token());
        assert_eq!(ctx.token_kind.as_deref(), Some("user"));
        assert!(ctx.allowed_client_id.is_none());
    }

    #[test]
    fn bootstrap_can_use_any_agent_endpoint() {
        let ctx = bootstrap_ctx();
        assert!(ctx.can_use_agent_endpoint("any-client"));
    }

    #[test]
    fn agent_token_can_use_matching_client_id_only() {
        let ctx = agent_ctx(
            "alice",
            "alice-laptop",
            vec![SCOPE_AGENT_REGISTER.to_string()],
        );
        assert!(ctx.can_use_agent_endpoint("alice-laptop"));
        assert!(!ctx.can_use_agent_endpoint("other-laptop"));
    }

    #[test]
    fn user_token_cannot_use_agent_endpoint() {
        let ctx = user_ctx("alice");
        assert!(!ctx.can_use_agent_endpoint("alice-laptop"));
    }

    #[test]
    fn require_scope_works_for_agent_tokens() {
        let ctx = agent_ctx("alice", "alice-laptop", vec![SCOPE_AGENT_POLL.to_string()]);
        assert!(ctx.require_scope(SCOPE_AGENT_POLL).is_ok());
        assert!(ctx.require_scope(SCOPE_AGENT_REGISTER).is_err());
    }

    #[test]
    fn bootstrap_require_scope_always_ok() {
        let ctx = bootstrap_ctx();
        assert!(ctx.require_scope(SCOPE_AGENT_REGISTER).is_ok());
        assert!(ctx.require_scope(SCOPE_RUNTIME_READ).is_ok());
    }

    #[test]
    fn validate_allowed_client_id_accepts_valid_ids() {
        assert_eq!(
            validate_allowed_client_id("alice-laptop").unwrap(),
            "alice-laptop"
        );
        assert_eq!(
            validate_allowed_client_id("my_agent.1").unwrap(),
            "my_agent.1"
        );
    }

    #[test]
    fn validate_allowed_client_id_rejects_invalid_ids() {
        assert!(validate_allowed_client_id("").is_err());
        assert!(validate_allowed_client_id("bad/client").is_err());
        assert!(validate_allowed_client_id("bad client").is_err());
        assert!(validate_allowed_client_id("bad\x00client").is_err());
        assert!(validate_allowed_client_id(&"x".repeat(81)).is_err());
        // Uppercase ASCII letters are allowed, matching the existing client_id
        // validation rules (validate_id uses is_ascii_alphanumeric).
        assert!(validate_allowed_client_id("UPPER").is_ok());
    }

    #[test]
    fn validate_agent_scopes_rejects_non_agent_scopes() {
        assert!(validate_agent_scopes(&["agent:register".to_string()]).is_ok());
        assert!(validate_agent_scopes(
            &["agent:register".to_string(), "runtime:read".to_string(),]
        )
        .is_err());
        assert!(validate_agent_scopes(&["admin".to_string()]).is_err());
    }

    #[test]
    fn generate_agent_token_uses_wc_agent_prefix() {
        let token = generate_agent_token();
        assert!(token.starts_with("wc_agent_"));
        assert!(token.len() > "wc_agent_".len() + 32);
    }

    #[test]
    fn token_prefix_for_agent_token_shows_prefix() {
        let token = generate_agent_token();
        let prefix = token_prefix(&token);
        assert!(prefix.starts_with("wc_agent_"));
        assert_ne!(prefix, token);
        assert_eq!(prefix.len(), 16);
    }

    #[test]
    fn is_agent_transport_path_allows_only_the_five_exact_paths() {
        // The five agent transport endpoints an agent token may call.
        assert!(is_agent_transport_path("/api/shell/agent/register"));
        assert!(is_agent_transport_path("/api/shell/agent/poll"));
        assert!(is_agent_transport_path("/api/shell/agent/result"));
        assert!(is_agent_transport_path("/api/shell/agent/job_update"));
        assert!(is_agent_transport_path("/api/agents/ws"));

        // Everything else is rejected — including paths that look similar.
        assert!(!is_agent_transport_path("/api/agent-tokens/create"));
        assert!(!is_agent_transport_path("/api/agent-tokens/list"));
        assert!(!is_agent_transport_path("/api/agent-tokens/revoke"));
        assert!(!is_agent_transport_path("/api/runtime/status"));
        assert!(!is_agent_transport_path("/api/tools/list"));
        assert!(!is_agent_transport_path("/api/tools/call"));
        assert!(!is_agent_transport_path("/api/projects/list"));
        assert!(!is_agent_transport_path("/api/jobs/list"));
        assert!(!is_agent_transport_path("/mcp"));
        assert!(!is_agent_transport_path("/api/audit/sessions"));
        assert!(!is_agent_transport_path("/api/users/list"));
        assert!(!is_agent_transport_path("/api/tokens/list"));
        // Prefix-only matches must not pass (exact match required).
        assert!(!is_agent_transport_path("/api/shell/agent/register/extra"));
        assert!(!is_agent_transport_path("/api/agents/ws/extra"));
        assert!(!is_agent_transport_path(""));
    }

    // -----------------------------------------------------------------------
    // HTTP-level central gate tests
    // -----------------------------------------------------------------------
    //
    // These build a minimal router that mounts a generic echo handler behind
    // the shared AuthMiddleware on a representative set of paths. The handler
    // is intentionally trivial — the point is to verify the Phase 3 central
    // gate in AuthMiddleware rejects agent tokens before any handler runs, and
    // that bootstrap/user tokens still reach the handler.

    use salvo::prelude::{affix_state, Json};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Router;
    use salvo::Service;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn gate_test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        })
    }

    fn gate_test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("gate.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn gate_seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    /// Create an agent token for `username` bound to `client_id` via the DB
    /// layer directly, returning the plaintext token. Used by the gate tests
    /// so we exercise the real AuthMiddleware path.
    fn gate_mint_agent_token(
        db: &crate::Database,
        user: &crate::models::UserRecord,
        client_id: &str,
    ) -> String {
        let plaintext = generate_agent_token();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "agent".to_string(),
            key_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register agent:poll agent:result agent:job_update".to_string(),
            expires_at: None,
            kind: crate::models::TOKEN_KIND_AGENT.to_string(),
            allowed_client_id: Some(client_id.to_string()),
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    /// Create a user token for `username` via the DB layer directly.
    fn gate_mint_user_token(db: &crate::Database, user: &crate::models::UserRecord) -> String {
        let plaintext = generate_api_token();
        let prefix = token_prefix(&plaintext);
        let hash = hash_token(&plaintext);
        let now = chrono::Utc::now().timestamp();
        let record = crate::models::ApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            name: "user".to_string(),
            key_prefix: prefix,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:read project:write job:run".to_string(),
            expires_at: None,
            kind: crate::models::TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&record, &hash).unwrap();
        plaintext
    }

    /// Trivial handler that returns 200 OK JSON. Reaching it proves the central
    /// gate did not reject the request.
    #[salvo::handler]
    async fn echo_ok(
        _req: &mut salvo::Request,
        _depot: &mut salvo::Depot,
        res: &mut salvo::Response,
    ) {
        res.render(Json(serde_json::json!({"ok": true})));
    }

    /// Build a router mirroring the production path shapes the central gate
    /// must protect, each backed by the same trivial echo handler.
    fn gate_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            // /mcp at the root (not under /api).
            .push(
                Router::with_path("mcp")
                    .hoop(AuthMiddleware)
                    .get(echo_ok)
                    .post(echo_ok),
            )
            .push(
                Router::with_path("api")
                    .hoop(AuthMiddleware)
                    .push(Router::with_path("runtime/status").post(echo_ok))
                    .push(Router::with_path("tools/list").post(echo_ok))
                    .push(Router::with_path("tools/call").post(echo_ok))
                    .push(Router::with_path("projects/list").post(echo_ok))
                    .push(Router::with_path("jobs/list").post(echo_ok))
                    .push(Router::with_path("audit/sessions").post(echo_ok))
                    .push(Router::with_path("users/list").post(echo_ok))
                    .push(Router::with_path("tokens/list").post(echo_ok))
                    .push(Router::with_path("agent-tokens/list").post(echo_ok))
                    .push(Router::with_path("shell/agent/register").post(echo_ok))
                    .push(Router::with_path("shell/agent/poll").post(echo_ok))
                    .push(Router::with_path("shell/agent/result").post(echo_ok))
                    .push(Router::with_path("shell/agent/job_update").post(echo_ok))
                    .push(Router::with_path("agents/ws").get(echo_ok)),
            )
    }

    fn gate_status(resp: &salvo::Response) -> salvo::http::StatusCode {
        resp.status_code.unwrap_or(salvo::http::StatusCode::OK)
    }

    async fn gate_send(
        service: &salvo::Service,
        path: &str,
        auth: Option<&str>,
    ) -> (salvo::http::StatusCode, serde_json::Value) {
        let mut req = TestClient::post(&format!("http://localhost{}", path));
        if path == "/api/agents/ws" {
            // The WS endpoint is GET-mounted in this test router.
            req = TestClient::get(&format!("http://localhost{}", path));
        }
        if let Some(token) = auth {
            req = req.bearer_auth(token);
        }
        let mut resp = req.send(service).await;
        let status = gate_status(&resp);
        let body = resp
            .take_json::<serde_json::Value>()
            .await
            .ok()
            .unwrap_or(serde_json::json!({"_raw": "<no json body>"}));
        (status, body)
    }

    #[tokio::test]
    async fn gate_agent_token_can_call_agent_transport_register() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        // /api/shell/agent/register is an allowed transport path.
        let (status, body) =
            gate_send(&service, "/api/shell/agent/register", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::OK, "body: {:?}", body);
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_runtime_status() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, body) = gate_send(&service, "/api/runtime/status", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or("")
                .contains("agent tokens are only allowed"),
            "body: {:?}",
            body
        );
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_tools_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/tools/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_projects_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/projects/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_mcp() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/mcp", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_agent_tokens_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) =
            gate_send(&service, "/api/agent-tokens/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_agent_token_cannot_call_tokens_list() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let (status, _body) = gate_send(&service, "/api/tokens/list", Some(&agent_token)).await;
        assert_eq!(status, salvo::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn gate_user_token_can_call_normal_apis() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let user_token = gate_mint_user_token(&db, &user);
        let service = Service::new(gate_router(config, db));
        // User tokens must still reach normal runtime/project APIs.
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
        ] {
            let (status, body) = gate_send(&service, path, Some(&user_token)).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::OK,
                "{} body: {:?}",
                path,
                body
            );
        }
        // And must NOT reach agent transport endpoints (enforced per-handler in
        // Phase 3, but here the central gate lets them through; the per-handler
        // agent transport check rejects them). For this gate test we only
        // assert the central gate does not block user tokens on normal APIs.
    }

    #[tokio::test]
    async fn gate_bootstrap_can_call_all_apis() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        // Bootstrap reaches normal APIs and agent transport paths alike.
        for path in [
            "/api/runtime/status",
            "/api/tools/list",
            "/api/projects/list",
            "/api/shell/agent/register",
            "/api/agent-tokens/list",
        ] {
            let (status, body) = gate_send(&service, path, Some("secret")).await;
            assert_eq!(
                status,
                salvo::http::StatusCode::OK,
                "{} body: {:?}",
                path,
                body
            );
        }
    }

    #[tokio::test]
    async fn gate_forbidden_response_is_json_not_html() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let user = gate_seed_user(&db, "alice");
        let agent_token = gate_mint_agent_token(&db, &user, "alice-laptop");
        let service = Service::new(gate_router(config, db));
        let resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth(&agent_token)
            .send(&service)
            .await;
        assert_eq!(gate_status(&resp), salvo::http::StatusCode::FORBIDDEN);
        let ct = resp.headers().get("content-type").unwrap();
        assert!(
            ct.to_str().unwrap().contains("application/json"),
            "forbidden response must be JSON, got: {:?}",
            ct
        );
    }

    #[tokio::test]
    async fn gate_unauthorized_response_is_json_not_html() {
        let config = gate_test_config(Some("secret"));
        let (_tmp, db) = gate_test_db();
        let service = Service::new(gate_router(config, db));
        let resp = TestClient::post("http://localhost/api/runtime/status")
            .send(&service)
            .await;
        assert_eq!(gate_status(&resp), salvo::http::StatusCode::UNAUTHORIZED);
        let ct = resp.headers().get("content-type").unwrap();
        assert!(
            ct.to_str().unwrap().contains("application/json"),
            "unauthorized response must be JSON, got: {:?}",
            ct
        );
    }
}
