/// Authenticated, non-secret access projection understood by the Runner
/// registry. Authentication mechanisms, credential verification, token scopes,
/// and transport admission remain root concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerAccess {
    pub admin: bool,
    pub username: Option<String>,
    pub group: Option<RunnerAccessGroup>,
}

/// Non-secret isolation partition captured when a Runner or Job is admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerAccessGroup {
    /// Existing SHA-256 shared-key/OAuth-bridge group; never plaintext.
    SharedKey(String),
    /// Stable non-secret project grant identity.
    ProjectGrant(String),
    /// Explicit open-anonymous partition.
    OpenAnonymous,
}

/// Opaque, stable, non-secret identity used only to partition detached Job
/// idempotency. Root authentication policy decides how credentials map to this
/// value; the registry never interprets credential kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedInitiatorIdentity(String);

impl DetachedInitiatorIdentity {
    pub fn from_stable_principal(principal: String) -> Self {
        Self(principal)
    }

    pub fn internal() -> Self {
        Self("internal".to_string())
    }

    pub fn as_stable_principal(&self) -> &str {
        &self.0
    }
}
