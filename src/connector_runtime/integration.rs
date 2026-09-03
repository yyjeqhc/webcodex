//! Root-owned integration projections for the transport-neutral Connector
//! runtime. Authentication and ToolRuntime policy terminate here.

use crate::auth::{
    AuthContext, AuthKind, SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE,
    SCOPE_RUNTIME_READ,
};
use crate::client_window::ClientWindow;
use webcodex_connector_runtime::{
    ConnectorAccess, ConnectorExecutionAuthority, ConnectorPermission, ConnectorPermissions,
    ConnectorPrincipalId, ConnectorWindowId,
};

pub(super) fn stable_subject_id(auth: &AuthContext) -> Result<String, String> {
    if let Some(user_id) = auth.user_id.as_deref() {
        return Ok(format!("user:{user_id}"));
    }
    if let Some(hash) = auth.shared_key_hash.as_deref() {
        return Ok(format!("shared:{hash}"));
    }
    if let Some(grant_id) = auth.project_grant_id.as_deref() {
        return Ok(format!("project:{grant_id}"));
    }
    match auth.kind {
        AuthKind::Bootstrap => Ok("bootstrap".to_string()),
        AuthKind::OpenAnonymous => Ok("open:anonymous".to_string()),
        AuthKind::ApiToken
        | AuthKind::OAuth2Token
        | AuthKind::SharedKey
        | AuthKind::ProjectCredential
        | AuthKind::AgentToken
        | AuthKind::AccountCredential => {
            Err("authenticated identity has no stable connector subject".to_string())
        }
    }
}

pub(super) fn connector_access(auth: &AuthContext) -> Result<ConnectorAccess, String> {
    let principal = ConnectorPrincipalId::new(stable_subject_id(auth)?)?;
    let runner_access = crate::runner_http::runner_access_from_auth(Some(auth))
        .expect("authenticated Connector access has Runner access projection");
    Ok(ConnectorAccess {
        principal,
        project_grant_id: auth.project_grant_id.clone(),
        bootstrap: auth.is_bootstrap(),
        global_admin: auth.is_admin(),
        permissions: ConnectorPermissions {
            runtime_read: auth.has_scope(SCOPE_RUNTIME_READ),
            project_read: auth.has_scope(SCOPE_PROJECT_READ),
            project_write: auth.has_scope(SCOPE_PROJECT_WRITE),
            job_run: auth.has_scope(SCOPE_JOB_RUN),
        },
        runner_access,
    })
}

pub(super) fn connector_window(window: &ClientWindow) -> ConnectorWindowId {
    ConnectorWindowId::new(window.key().to_string(), window.source().to_string())
        .expect("validated ClientWindow projection")
}

pub(super) fn connector_execution_authority(
    tools: &crate::tool_runtime::ToolRuntime,
) -> ConnectorExecutionAuthority {
    let authority = tools.permission_evaluator.config();
    ConnectorExecutionAuthority {
        auto_authorize: authority.auto_authorize(),
        mode: authority.mode_name().to_string(),
        source: authority.source().as_str().to_string(),
        resolved_rule: crate::tool_runtime::permissions::TRUSTED_AGENT_AUTO_REASON.to_string(),
    }
}

pub(super) fn permission_scope(permission: ConnectorPermission) -> &'static str {
    match permission {
        ConnectorPermission::RuntimeRead => SCOPE_RUNTIME_READ,
        ConnectorPermission::ProjectRead => SCOPE_PROJECT_READ,
        ConnectorPermission::ProjectWrite => SCOPE_PROJECT_WRITE,
        ConnectorPermission::JobRun => SCOPE_JOB_RUN,
    }
}

pub(super) fn permission_from_scope(scope: Option<&str>) -> Option<ConnectorPermission> {
    match scope {
        Some(SCOPE_RUNTIME_READ) => Some(ConnectorPermission::RuntimeRead),
        Some(SCOPE_PROJECT_READ) => Some(ConnectorPermission::ProjectRead),
        Some(SCOPE_PROJECT_WRITE) => Some(ConnectorPermission::ProjectWrite),
        Some(SCOPE_JOB_RUN) => Some(ConnectorPermission::JobRun),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(kind: AuthKind) -> AuthContext {
        AuthContext::new(kind)
    }

    #[test]
    fn stable_subject_precedence_matches_durable_connector_contract() {
        let mut value = auth(AuthKind::OAuth2Token);
        value.user_id = Some("u".to_string());
        value.shared_key_hash = Some("hash".to_string());
        value.project_grant_id = Some("wc_pgrant_x".to_string());
        assert_eq!(stable_subject_id(&value).unwrap(), "user:u");

        value.user_id = None;
        assert_eq!(stable_subject_id(&value).unwrap(), "shared:hash");
        value.shared_key_hash = None;
        assert_eq!(stable_subject_id(&value).unwrap(), "project:wc_pgrant_x");

        assert_eq!(
            stable_subject_id(&auth(AuthKind::Bootstrap)).unwrap(),
            "bootstrap"
        );
        assert_eq!(
            stable_subject_id(&auth(AuthKind::OpenAnonymous)).unwrap(),
            "open:anonymous"
        );
        assert!(stable_subject_id(&auth(AuthKind::OAuth2Token)).is_err());
        assert!(stable_subject_id(&auth(AuthKind::ApiToken)).is_err());
    }
}
