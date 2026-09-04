//! Shared declarative authority vocabulary.
//!
//! Authentication execution, principal context, middleware, and credential
//! verification remain application concerns. This module owns only stable scope
//! names and policy values consumed by auth, route metadata, and tool contracts.

pub const SCOPE_RUNTIME_READ: &str = "runtime:read";
pub const SCOPE_SESSION_COLLABORATE: &str = "session:collaborate";
pub const SCOPE_PROJECT_READ: &str = "project:read";
pub const SCOPE_PROJECT_WRITE: &str = "project:write";
pub const SCOPE_MEMORY_READ: &str = "memory:read";
pub const SCOPE_MEMORY_MANAGE: &str = "memory:manage";
pub const SCOPE_COMMUNICATION_READ: &str = "communication:read";
pub const SCOPE_COMMUNICATION_MANAGE: &str = "communication:manage";
pub const SCOPE_JOB_RUN: &str = "job:run";
pub const SCOPE_JOB_DETACH: &str = "job:detach";
pub const SCOPE_COMPUTER_READ: &str = "computer:read";
pub const SCOPE_COMPUTER_CONTROL: &str = "computer:control";
pub const SCOPE_COMPUTER_LAUNCH: &str = "computer:launch";
pub const SCOPE_COMPUTER_DISPLAY_READ: &str = "computer:display_read";
pub const SCOPE_COMPUTER_POINTER_CONTROL: &str = "computer:pointer_control";
pub const SCOPE_COMPUTER_CLIPBOARD_READ: &str = "computer:clipboard_read";
pub const SCOPE_COMPUTER_CLIPBOARD_WRITE: &str = "computer:clipboard_write";
pub const SCOPE_MCP_LOCAL: &str = "mcp:local";
pub const SCOPE_PLUGIN_LOCAL: &str = "plugin:local";
pub const SCOPE_CODING_AGENT_RUN: &str = "coding_agent:run";
pub const SCOPE_AGENT_REGISTER: &str = "agent:register";
pub const SCOPE_ADMIN: &str = "admin";
pub const SCOPE_AGENT_POLL: &str = "agent:poll";
pub const SCOPE_AGENT_RESULT: &str = "agent:result";
pub const SCOPE_AGENT_JOB_UPDATE: &str = "agent:job_update";
pub const SCOPE_ACCOUNT_MANAGE: &str = "account:manage";

/// Stable project-grant identity grammar shared by auth and persistence.
pub const PROJECT_GRANT_ID_PREFIX: &str = "wc_pgrant_";
/// Stable OAuth subject vocabulary for project-share credentials.
pub const PROJECT_SHARE_OAUTH_SUBJECT_KIND: &str = "project_share";
/// Stable session component prefix inside a project-share OAuth subject id.
pub const PROJECT_SHARE_SESSION_PREFIX: &str = "wc_share_";

pub fn validate_project_grant_id(value: &str) -> Result<(), String> {
    let suffix = value
        .strip_prefix(PROJECT_GRANT_ID_PREFIX)
        .unwrap_or_default();
    if suffix.len() != 24
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("configured project grant identity is invalid".to_string());
    }
    Ok(())
}

pub fn validate_project_share_session_id(value: &str) -> Result<(), String> {
    let suffix = value
        .strip_prefix(PROJECT_SHARE_SESSION_PREFIX)
        .unwrap_or_default();
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("configured project share session identity is invalid".to_string());
    }
    Ok(())
}

pub fn project_share_subject_id(
    project_grant_id: &str,
    session_id: &str,
) -> Result<String, String> {
    validate_project_grant_id(project_grant_id)?;
    validate_project_share_session_id(session_id)?;
    Ok(format!("{project_grant_id}|{session_id}"))
}

pub fn parse_project_share_subject_id(value: &str) -> Result<(&str, &str), String> {
    let (grant_id, session_id) = value
        .split_once('|')
        .ok_or_else(|| "project-share OAuth subject is malformed".to_string())?;
    if session_id.contains('|') {
        return Err("project-share OAuth subject is malformed".to_string());
    }
    validate_project_grant_id(grant_id)?;
    validate_project_share_session_id(session_id)?;
    Ok((grant_id, session_id))
}

pub const COMMUNICATION_READ_SCOPES: &[&str] = &[SCOPE_COMMUNICATION_READ];
pub const COMMUNICATION_MANAGE_SCOPES: &[&str] =
    &[SCOPE_COMMUNICATION_READ, SCOPE_COMMUNICATION_MANAGE];
pub const MEMORY_READ_SCOPES: &[&str] = &[SCOPE_PROJECT_READ, SCOPE_MEMORY_READ];
pub const MEMORY_MANAGE_SCOPES: &[&str] = &[SCOPE_PROJECT_WRITE, SCOPE_MEMORY_MANAGE];

pub const AGENT_SCOPES: &[&str] = &[
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
];

pub const KNOWN_SCOPES: &[&str] = &[
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
    SCOPE_RUNTIME_READ,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_MEMORY_READ,
    SCOPE_MEMORY_MANAGE,
    SCOPE_COMMUNICATION_READ,
    SCOPE_COMMUNICATION_MANAGE,
    SCOPE_JOB_RUN,
    SCOPE_JOB_DETACH,
    SCOPE_COMPUTER_READ,
    SCOPE_COMPUTER_CONTROL,
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
    SCOPE_MCP_LOCAL,
    SCOPE_PLUGIN_LOCAL,
    SCOPE_CODING_AGENT_RUN,
    SCOPE_ACCOUNT_MANAGE,
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
    SCOPE_ADMIN,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthRouteScopePolicy {
    Public,
    FirstPartyOnly,
    BootstrapOnly,
    AgentSurface,
    Require(&'static str),
    BodyAware(OAuthBodyAwarePolicy),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthBodyAwarePolicy {
    RuntimeToolCall,
    McpToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuthorityPolicy {
    Require(&'static str),
    RequireAll(&'static [&'static str]),
    Unknown,
}
