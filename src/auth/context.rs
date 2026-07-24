//! Depot-injected authentication identity and credential classification.

use crate::auth::scopes::SCOPE_ADMIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidToken,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "invalid authentication token"),
        }
    }
}

impl std::error::Error for AuthError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    Bootstrap,
    ApiToken,
    AgentToken,
    AccountCredential,
    OAuth2Token,
    SharedKey,
    ProjectCredential,
    OpenAnonymous,
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub kind: AuthKind,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub api_key_id: Option<String>,
    #[allow(dead_code)]
    pub api_key_name: Option<String>,
    pub role: Option<String>,
    pub scopes: Vec<String>,
    pub is_bootstrap: bool,
    pub token_kind: Option<String>,
    pub allowed_client_id: Option<String>,
    /// SHA-256 of a quick-start shared key or OAuth bridge subject key.
    /// Plaintext shared keys are never retained.
    pub shared_key_hash: Option<String>,
    /// Stable, non-secret identity of the configured project credential.
    pub project_grant_id: Option<String>,
}

impl AuthContext {
    pub(crate) fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: None,
            scopes: Vec::new(),
            is_bootstrap: false,
            token_kind: None,
            allowed_client_id: None,
            shared_key_hash: None,
            project_grant_id: None,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.is_bootstrap || self.scopes.iter().any(|scope| scope == SCOPE_ADMIN)
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.is_bootstrap
            || self
                .scopes
                .iter()
                .any(|granted| granted == scope || granted == SCOPE_ADMIN)
    }

    pub fn is_bootstrap(&self) -> bool {
        self.is_bootstrap
    }

    pub fn is_agent_token(&self) -> bool {
        matches!(self.kind, AuthKind::AgentToken)
    }

    pub fn is_account_credential(&self) -> bool {
        matches!(self.kind, AuthKind::AccountCredential)
    }

    pub fn is_oauth_token(&self) -> bool {
        matches!(self.kind, AuthKind::OAuth2Token)
    }

    pub fn is_oauth_shared_key_subject(&self) -> bool {
        self.is_oauth_token() && self.token_kind.as_deref() == Some("oauth2_shared_key")
    }

    pub fn is_shared_key(&self) -> bool {
        matches!(self.kind, AuthKind::SharedKey)
    }

    pub fn is_project_credential(&self) -> bool {
        matches!(self.kind, AuthKind::ProjectCredential)
    }

    pub fn is_open_anonymous(&self) -> bool {
        matches!(self.kind, AuthKind::OpenAnonymous)
    }

    pub fn is_lightweight(&self) -> bool {
        self.is_shared_key() || self.is_project_credential() || self.is_open_anonymous()
    }
}
