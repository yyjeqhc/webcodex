//! Protocol-neutral workspace activity persistence contracts.
//!
//! Runtime authentication decides the scope before constructing a record;
//! durable storage persists that immutable attribution and applies visibility
//! filters without consulting live runtime ownership state.

/// Who an activity row belonged to when it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityScope {
    /// Stable, non-secret project grant identity.
    ProjectGrant(String),
    /// Bootstrap/admin or another host-global principal.
    HostGlobal,
    /// Attribution could not be proven. Such rows remain host-global only.
    Unscoped,
}

impl ActivityScope {
    /// Stored `scope_kind`. Each value has one security meaning.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectGrant(_) => "project_grant",
            Self::HostGlobal => "host_global",
            Self::Unscoped => "unscoped",
        }
    }

    /// Stored `scope_id`. Only project grants carry an id.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::ProjectGrant(grant) => Some(grant.as_str()),
            Self::HostGlobal | Self::Unscoped => None,
        }
    }
}

/// Which persisted activity rows a reader may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityVisibility<'a> {
    /// Bootstrap/admin, or a host-local operator with the state directory.
    Global,
    /// Sees only rows written by this project grant.
    ProjectGrant(&'a str),
}

/// One persistence-neutral activity record emitted by runtime orchestration.
pub struct ActivityRecord<'a> {
    pub tool: &'a str,
    pub project: Option<&'a str>,
    /// Client surface that issued the call (for example `mcp` or `api`).
    pub surface: &'a str,
    /// Executing Runner for Runner-backed Projects.
    pub client: Option<&'a str>,
    pub success: bool,
    pub session_id: Option<&'a str>,
    /// Raw command text for shell-like tools. Durable adapters must apply
    /// their configured bounded-preview policy before persistence.
    pub command: Option<&'a str>,
    /// Distinct project-relative paths named by the request (bounded upstream).
    pub paths: Vec<String>,
    pub error_summary: Option<&'a str>,
    /// Attribution fixed at write time.
    pub scope: ActivityScope,
}
