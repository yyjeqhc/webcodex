use crate::{Config, Database};
use salvo::prelude::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// The kind of credential that produced an [`AuthContext`].
///
/// - `Bootstrap`: the server-wide `WEBCODEX_TOKEN` (admin/bootstrap auth).
/// - `ApiToken`: a Phase 2 personal API token backed by the `api_keys` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthKind {
    Bootstrap,
    ApiToken,
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

/// All scopes recognized by this phase. Unknown scopes are rejected at token
/// creation time so the stored scope string stays clean.
pub const KNOWN_SCOPES: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
    SCOPE_AGENT_REGISTER,
    SCOPE_ADMIN,
];

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

/// Return a short, display-safe prefix of a token (the first 16 characters,
/// including the `wc_pat_` kind marker). Used for listing tokens without
/// revealing the secret.
pub(crate) fn token_prefix(token: &str) -> String {
    let end = token.len().min(16);
    token[..end].to_string()
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

        let ctx = AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some(user.id.clone()),
            username: Some(user.username.clone()),
            api_key_id: Some(api_key.id.clone()),
            api_key_name: Some(api_key.name.clone()),
            role: Some(user.role.clone()),
            scopes: api_key.scopes_vec(),
            is_bootstrap: false,
        };

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
