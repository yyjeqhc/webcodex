//! Project-first OAuth subject helpers used by `webcodex share`.
//!
//! The OAuth transport credential is session-fenced, while Connector durable
//! identity remains the stable project grant. A share restart therefore makes
//! old authorization codes/access tokens/refresh tokens unusable without
//! changing ownership of existing project tasks.

use super::{
    SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
    SCOPE_SESSION_COLLABORATE,
};

pub(crate) const PROJECT_SHARE_OAUTH_SUBJECT_KIND: &str = "project_share";
pub(crate) const PROJECT_SHARE_OAUTH_TOKEN_KIND: &str = "oauth2_project";
pub(crate) const PROJECT_SHARE_SESSION_PREFIX: &str = "wc_share_";
pub(crate) const PROJECT_SHARE_OAUTH_SCOPES: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
];

pub(crate) fn validate_project_grant_id(value: &str) -> Result<(), String> {
    let suffix = value.strip_prefix("wc_pgrant_").unwrap_or_default();
    if suffix.len() != 24
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("configured project grant identity is invalid".to_string());
    }
    Ok(())
}

pub(crate) fn generate_project_share_session_id() -> String {
    let mut random = String::with_capacity(64);
    while random.len() < 64 {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(64);
    format!("{PROJECT_SHARE_SESSION_PREFIX}{random}")
}

pub(crate) fn validate_project_share_session_id(value: &str) -> Result<(), String> {
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

pub(crate) fn project_share_subject_id(
    project_grant_id: &str,
    session_id: &str,
) -> Result<String, String> {
    validate_project_grant_id(project_grant_id)?;
    validate_project_share_session_id(session_id)?;
    Ok(format!("{project_grant_id}|{session_id}"))
}

pub(crate) fn parse_project_share_subject_id(value: &str) -> Result<(&str, &str), String> {
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

pub(crate) fn configured_project_share_subject(
    config: &crate::Config,
) -> Result<Option<String>, String> {
    match (
        config.oauth2.project_share_grant_id.as_deref(),
        config.oauth2.project_share_session_id.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(grant_id), Some(session_id)) => {
            project_share_subject_id(grant_id, session_id).map(Some)
        }
        _ => Err("project-share OAuth configuration is incomplete".to_string()),
    }
}

pub(crate) fn validate_project_share_grant_subject(
    config: &crate::Config,
    subject_kind: &str,
    subject_id: &str,
) -> Result<(), String> {
    if subject_kind != PROJECT_SHARE_OAUTH_SUBJECT_KIND {
        return Ok(());
    }
    let expected = configured_project_share_subject(config)?
        .ok_or_else(|| "project-share OAuth session is not active".to_string())?;
    if expected != subject_id {
        return Err("project-share OAuth session is no longer active".to_string());
    }
    Ok(())
}

pub(crate) fn project_share_scopes_are_bounded(scopes: &str) -> bool {
    scopes
        .split_ascii_whitespace()
        .all(|scope| PROJECT_SHARE_OAUTH_SCOPES.contains(&scope) || scope == "offline_access")
}
