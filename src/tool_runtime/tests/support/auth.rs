pub(in crate::tool_runtime::tests) fn auth_context(
    username: Option<&str>,
    is_bootstrap: bool,
) -> crate::auth::AuthContext {
    let (role, scopes) = if is_bootstrap {
        ("admin".to_string(), vec!["admin".to_string()])
    } else {
        ("user".to_string(), Vec::new())
    };
    crate::auth::AuthContext {
        kind: if is_bootstrap {
            crate::auth::AuthKind::Bootstrap
        } else {
            crate::auth::AuthKind::ApiToken
        },
        user_id: username.map(|u| format!("user-{}", u)),
        username: username.map(str::to_string),
        api_key_id: username.map(|u| format!("key-{}", u)),
        api_key_name: username.map(|u| format!("{} key", u)),
        role: Some(role),
        scopes,
        is_bootstrap,
        token_kind: if is_bootstrap {
            None
        } else {
            Some("user".to_string())
        },
        allowed_client_id: None,
        shared_key_hash: None,
        project_grant_id: None,
    }
}

pub(in crate::tool_runtime::tests) fn shared_key_auth_context(
    hash: &str,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::SharedKey,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("shared-key".to_string()),
        allowed_client_id: None,
        shared_key_hash: Some(hash.to_string()),
        project_grant_id: None,
    }
}

pub(in crate::tool_runtime::tests) fn oauth_bridge_auth_context(
    hash: &str,
    scopes: &[&str],
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: None,
        username: None,
        api_key_id: Some("oauth-access-token".to_string()),
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
        is_bootstrap: false,
        token_kind: Some("oauth2_shared_key".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: Some(hash.to_string()),
        project_grant_id: None,
    }
}

pub(in crate::tool_runtime::tests) fn managed_oauth_auth_context(
    username: &str,
    shared_key_hash: Option<&str>,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: Some(format!("user-{}", username)),
        username: Some(username.to_string()),
        api_key_id: Some("oauth-access-token".to_string()),
        api_key_name: None,
        role: Some("user".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("oauth2".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: shared_key_hash.map(str::to_string),
        project_grant_id: None,
    }
}

pub(in crate::tool_runtime::tests) fn open_auth_context() -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OpenAnonymous,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("open".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("open".to_string()),
        allowed_client_id: None,
        shared_key_hash: None,
        project_grant_id: None,
    }
}

pub(in crate::tool_runtime::tests) fn bootstrap_auth_context() -> crate::auth::AuthContext {
    auth_context(None, true)
}
