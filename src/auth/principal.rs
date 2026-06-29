//! Principal — the authenticated identity abstraction.
//!
//! A [`Principal`] represents *who* is making a request and *how* they
//! authenticated. It is the unified identity type that all authorization
//! decisions should go through, regardless of whether the caller used a PAT,
//! an agent token, an account credential, or (in a future phase) an OAuth2
//! bearer token.
//!
//! ## Relationship to `AuthContext`
//!
//! The existing [`AuthContext`] is the low-level Salvo depot-injected struct
//! that carries the raw database fields. `Principal` is a higher-level
//! abstraction derived from `AuthContext`. During this first refactoring phase
//! both types coexist — `AuthContext` remains the depot-injected type so
//! existing handlers are unaffected. A future phase can migrate handlers to
//! read `Principal` directly from the depot.

use crate::auth::scopes::SCOPE_ADMIN;

// ---------------------------------------------------------------------------
// AuthMethod — how did the caller authenticate?
// ---------------------------------------------------------------------------

/// The authentication method used by a caller.
///
/// This enum is intentionally non-exhaustive so that new methods (e.g.
/// `OAuth2`) can be added in a future phase without breaking existing match
/// arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(dead_code)] // OAuth2 variant reserved for future phase
pub enum AuthMethod {
    /// The server-wide `WEBCODEX_TOKEN` bootstrap token (or auth disabled in
    /// development mode).
    Bootstrap,
    /// A Phase 2 personal access token (`wc_pat_*`).
    Pat,
    /// A Phase 3 agent token (`wc_agent_*`), bound to an owner and an
    /// `allowed_client_id`.
    AgentToken,
    /// A high-entropy account credential (`wc_acct_*`).
    AccountCredential,
    /// An OAuth2 bearer token. **Reserved for future use** — no verifier is
    /// wired up yet.
    OAuth2,
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::Bootstrap => write!(f, "bootstrap"),
            AuthMethod::Pat => write!(f, "pat"),
            AuthMethod::AgentToken => write!(f, "agent_token"),
            AuthMethod::AccountCredential => write!(f, "account_credential"),
            AuthMethod::OAuth2 => write!(f, "oauth2"),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthError — what went wrong during authentication or authorization?
// ---------------------------------------------------------------------------

/// Errors that can occur during the authentication or authorization pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Several variants reserved for future OAuth2 phase
pub enum AuthError {
    /// No credentials were provided (missing or empty `Authorization` header).
    MissingToken,
    /// The provided token was syntactically or cryptographically invalid.
    InvalidToken,
    /// The token was valid but has expired.
    TokenExpired,
    /// The authenticated identity is disabled.
    IdentityDisabled,
    /// The caller lacks the required scope for this operation.
    InsufficientScope {
        /// The scope that was required.
        required: String,
    },
    /// The token kind is not permitted on this endpoint (e.g. agent token
    /// reaching a non-agent-transport endpoint).
    ForbiddenTokenKind,
    /// An internal error occurred during the authentication pipeline.
    Internal(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "missing authentication token"),
            AuthError::InvalidToken => write!(f, "invalid authentication token"),
            AuthError::TokenExpired => write!(f, "authentication token has expired"),
            AuthError::IdentityDisabled => write!(f, "authenticated identity is disabled"),
            AuthError::InsufficientScope { required } => {
                write!(f, "missing required scope: {}", required)
            }
            AuthError::ForbiddenTokenKind => {
                write!(f, "token kind not permitted on this endpoint")
            }
            AuthError::Internal(msg) => write!(f, "internal auth error: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

impl AuthError {
    /// HTTP status code to use when rendering this error as an HTTP response.
    #[allow(dead_code)] // Will be used when handlers migrate to Principal
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::MissingToken | AuthError::InvalidToken | AuthError::TokenExpired => 401,
            AuthError::IdentityDisabled => 401,
            AuthError::InsufficientScope { .. } => 403,
            AuthError::ForbiddenTokenKind => 403,
            AuthError::Internal(_) => 500,
        }
    }
}

// ---------------------------------------------------------------------------
// Principal
// ---------------------------------------------------------------------------

/// The authenticated identity of a request caller.
///
/// `Principal` is the **single source of truth** for authorization decisions.
/// It captures who is making the request, how they authenticated, and what
/// scopes they hold. All authorization checks (`require_scope`,
/// `authorize_tool_call`, etc.) should operate on `Principal` rather than
/// reaching into raw token or database fields.
///
/// ## Construction
///
/// `Principal` instances are created from [`AuthContext`](crate::auth::AuthContext)
/// via [`Principal::from_auth_context`] during the authentication pipeline.
/// Handlers and tool dispatch code should not construct `Principal` manually.
///
/// ## Extensibility
///
/// The `allowed_agents` and `allowed_projects` fields are reserved for future
/// fine-grained authorization without requiring a struct change.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields and methods reserved for future handler migration
pub struct Principal {
    /// The subject identifier — typically the user ID from the database.
    /// `None` only for bootstrap auth (which acts as a virtual admin).
    pub subject: Option<String>,

    /// The username associated with this identity. `None` for bootstrap.
    pub username: Option<String>,

    /// How this identity was authenticated.
    pub method: AuthMethod,

    /// The scopes granted to this identity. For bootstrap, this is `["admin"]`.
    pub scopes: Vec<String>,

    /// Token identifier — the database key ID for PATs and agent tokens, or
    /// `None` for bootstrap. Useful for audit logging.
    pub token_id: Option<String>,

    /// The display name of the token (the `name` column in `api_keys`).
    /// `None` for bootstrap.
    pub token_name: Option<String>,

    /// The role of the authenticated user (`"admin"` or `"user"`). `None` for
    /// bootstrap (which is always treated as admin).
    pub role: Option<String>,

    /// For agent tokens: the bound `allowed_client_id`. `None` for all other
    /// auth methods.
    pub allowed_client_id: Option<String>,

    /// Reserved for future fine-grained agent-level authorization.
    /// Currently unused; will be populated when per-principal agent ACLs are
    /// introduced.
    pub allowed_agents: Vec<String>,

    /// Reserved for future fine-grained project-level authorization.
    /// Currently unused; will be populated when per-principal project ACLs are
    /// introduced.
    pub allowed_projects: Vec<String>,
}

#[allow(dead_code)] // Methods reserved for future handler migration to Principal
impl Principal {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Create a bootstrap principal (full admin access). Used when auth is
    /// disabled or the server-wide `WEBCODEX_TOKEN` is presented.
    pub fn bootstrap() -> Self {
        Principal {
            subject: None,
            username: None,
            method: AuthMethod::Bootstrap,
            scopes: vec![SCOPE_ADMIN.to_string()],
            token_id: None,
            token_name: None,
            role: Some("admin".to_string()),
            allowed_client_id: None,
            allowed_agents: Vec::new(),
            allowed_projects: Vec::new(),
        }
    }

    /// Derive a `Principal` from an existing [`AuthContext`](crate::auth::AuthContext).
    ///
    /// This is the primary construction path during Phase 1. It maps the
    /// existing `AuthKind` values to `AuthMethod` and carries over all
    /// relevant fields.
    pub fn from_auth_context(ctx: &crate::auth::AuthContext) -> Self {
        use crate::auth::AuthKind;

        let method = match ctx.kind {
            AuthKind::Bootstrap => AuthMethod::Bootstrap,
            AuthKind::ApiToken => AuthMethod::Pat,
            AuthKind::AgentToken => AuthMethod::AgentToken,
            AuthKind::AccountCredential => AuthMethod::AccountCredential,
            AuthKind::OAuth2Token => AuthMethod::OAuth2,
        };

        Principal {
            subject: ctx.user_id.clone(),
            username: ctx.username.clone(),
            method,
            scopes: ctx.scopes.clone(),
            token_id: ctx.api_key_id.clone(),
            token_name: ctx.api_key_name.clone(),
            role: ctx.role.clone(),
            allowed_client_id: ctx.allowed_client_id.clone(),
            allowed_agents: Vec::new(),
            allowed_projects: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Identity queries
    // ------------------------------------------------------------------

    /// True when this principal authenticated as the bootstrap token (or auth
    /// is disabled in development).
    pub fn is_bootstrap(&self) -> bool {
        self.method == AuthMethod::Bootstrap
    }

    /// True when this principal authenticated with a personal access token.
    pub fn is_pat(&self) -> bool {
        self.method == AuthMethod::Pat
    }

    /// True when this principal authenticated with an agent token.
    pub fn is_agent_token(&self) -> bool {
        self.method == AuthMethod::AgentToken
    }

    /// True when this principal authenticated with an account credential.
    pub fn is_account_credential(&self) -> bool {
        self.method == AuthMethod::AccountCredential
    }

    /// True when this principal authenticated via OAuth2.
    pub fn is_oauth2(&self) -> bool {
        self.method == AuthMethod::OAuth2
    }

    /// Display-friendly identity string. Returns the username if available,
    /// falling back to `"bootstrap"`.
    pub fn display_name(&self) -> &str {
        self.username.as_deref().unwrap_or("bootstrap")
    }

    // ------------------------------------------------------------------
    // Scope / authorization queries
    // ------------------------------------------------------------------

    /// True when the caller holds the given scope (or is bootstrap/admin).
    pub fn has_scope(&self, scope: &str) -> bool {
        self.is_bootstrap() || self.scopes.iter().any(|s| s == scope || s == SCOPE_ADMIN)
    }

    /// Require the caller to hold `scope`. Returns `Ok(())` when present,
    /// `Err(AuthError)` when missing.
    pub fn require_scope(&self, scope: &str) -> Result<(), AuthError> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(AuthError::InsufficientScope {
                required: scope.to_string(),
            })
        }
    }

    /// True when the caller is admin (bootstrap or holds `admin` scope).
    pub fn is_admin(&self) -> bool {
        self.is_bootstrap() || self.scopes.iter().any(|s| s == SCOPE_ADMIN)
    }

    /// True when the caller may use an agent transport endpoint for the given
    /// `client_id`. Bootstrap may use any client_id. Agent tokens may only use
    /// the `allowed_client_id` they are bound to. All other auth methods are
    /// rejected.
    pub fn can_use_agent_endpoint(&self, client_id: &str) -> bool {
        if self.is_bootstrap() {
            return true;
        }
        if self.is_agent_token() {
            return self
                .allowed_client_id
                .as_deref()
                .map(|allowed| allowed == client_id)
                .unwrap_or(false);
        }
        false
    }

    // ------------------------------------------------------------------
    // Future OAuth2 extension point
    // ------------------------------------------------------------------

    /// Placeholder for future OAuth2-specific principal construction.
    ///
    /// When an OAuth2 token verifier is wired up (Phase 2 of the auth
    /// refactoring), it will call this method with the decoded token claims.
    /// For now this is a compile-time reminder that the extension point exists.
    #[allow(dead_code)]
    pub(crate) fn from_oauth2_claims_stub(
        _subject: String,
        _scopes: Vec<String>,
    ) -> Result<Self, AuthError> {
        // Stub: will be implemented when the OAuth2 verifier is added.
        Err(AuthError::Internal(
            "OAuth2 verification is not yet implemented".to_string(),
        ))
    }
}

impl std::fmt::Display for Principal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Principal({}, method={}, scopes=[{}])",
            self.display_name(),
            self.method,
            self.scopes.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthContext, AuthKind, SCOPE_AGENT_POLL, SCOPE_RUNTIME_READ};

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

    fn agent_ctx(username: &str, client_id: &str) -> AuthContext {
        AuthContext {
            kind: AuthKind::AgentToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-agent".to_string()),
            api_key_name: Some("agent key".to_string()),
            role: Some("user".to_string()),
            scopes: vec![SCOPE_AGENT_POLL.to_string()],
            is_bootstrap: false,
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(client_id.to_string()),
        }
    }

    #[test]
    fn principal_from_bootstrap_context() {
        let p = Principal::from_auth_context(&bootstrap_ctx());
        assert!(p.is_bootstrap());
        assert!(p.is_admin());
        assert_eq!(p.method, AuthMethod::Bootstrap);
        assert_eq!(p.display_name(), "bootstrap");
        assert!(p.has_scope(SCOPE_RUNTIME_READ));
    }

    #[test]
    fn principal_from_user_context() {
        let p = Principal::from_auth_context(&user_ctx("alice"));
        assert!(p.is_pat());
        assert!(!p.is_bootstrap());
        assert!(!p.is_admin());
        assert_eq!(p.username.as_deref(), Some("alice"));
        assert_eq!(p.method, AuthMethod::Pat);
        assert!(p.has_scope(SCOPE_RUNTIME_READ));
        assert!(!p.has_scope(SCOPE_ADMIN));
    }

    #[test]
    fn principal_from_agent_context() {
        let p = Principal::from_auth_context(&agent_ctx("alice", "laptop"));
        assert!(p.is_agent_token());
        assert_eq!(p.method, AuthMethod::AgentToken);
        assert_eq!(p.allowed_client_id.as_deref(), Some("laptop"));
        assert!(p.can_use_agent_endpoint("laptop"));
        assert!(!p.can_use_agent_endpoint("other"));
    }

    #[test]
    fn principal_require_scope_success_and_failure() {
        let p = Principal::from_auth_context(&user_ctx("bob"));
        assert!(p.require_scope(SCOPE_RUNTIME_READ).is_ok());
        match p.require_scope(SCOPE_ADMIN) {
            Err(AuthError::InsufficientScope { required }) => {
                assert_eq!(required, SCOPE_ADMIN);
            }
            other => panic!("expected InsufficientScope, got {:?}", other),
        }
    }

    #[test]
    fn principal_display_format() {
        let p = Principal::from_auth_context(&user_ctx("carol"));
        let display = format!("{}", p);
        assert!(display.contains("carol"));
        assert!(display.contains("pat"));
    }

    #[test]
    fn bootstrap_principal_satisfies_any_scope() {
        let p = Principal::bootstrap();
        assert!(p.has_scope("any:scope"));
        assert!(p.require_scope("any:scope").is_ok());
    }

    #[test]
    fn agent_token_cannot_use_agent_endpoint_with_wrong_client_id() {
        let p = Principal::from_auth_context(&agent_ctx("alice", "laptop"));
        assert!(!p.can_use_agent_endpoint("desktop"));
    }

    #[test]
    fn pat_cannot_use_agent_endpoint() {
        let p = Principal::from_auth_context(&user_ctx("alice"));
        assert!(!p.can_use_agent_endpoint("anything"));
    }

    #[test]
    fn auth_method_display() {
        assert_eq!(format!("{}", AuthMethod::Bootstrap), "bootstrap");
        assert_eq!(format!("{}", AuthMethod::Pat), "pat");
        assert_eq!(format!("{}", AuthMethod::AgentToken), "agent_token");
        assert_eq!(format!("{}", AuthMethod::OAuth2), "oauth2");
    }

    #[test]
    fn auth_error_display_and_status() {
        let e = AuthError::MissingToken;
        assert_eq!(e.status_code(), 401);
        assert!(format!("{}", e).contains("missing"));

        let e = AuthError::InsufficientScope {
            required: "admin".to_string(),
        };
        assert_eq!(e.status_code(), 403);
        assert!(format!("{}", e).contains("admin"));

        let e = AuthError::ForbiddenTokenKind;
        assert_eq!(e.status_code(), 403);
    }
}
