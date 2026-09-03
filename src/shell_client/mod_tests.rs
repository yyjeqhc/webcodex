use super::*;
use crate::shell_protocol::{
    AGENT_PROTOCOL_GENERATION_V2, AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES,
};

fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
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
        user_id: username.map(|username| format!("user-{}", username)),
        username: username.map(str::to_string),
        api_key_id: username.map(|username| format!("key-{}", username)),
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

/// Phase 3 test helper: build an agent-token AuthContext bound to
/// `username` and `allowed_client_id`, carrying the given agent scopes.
fn agent_auth_context(
    username: &str,
    allowed_client_id: &str,
    scopes: Vec<&str>,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::AgentToken,
        user_id: Some(format!("user-{}", username)),
        username: Some(username.to_string()),
        api_key_id: Some("key-agent".to_string()),
        role: Some("user".to_string()),
        scopes: scopes.into_iter().map(str::to_string).collect(),
        is_bootstrap: false,
        token_kind: Some("agent".to_string()),
        allowed_client_id: Some(allowed_client_id.to_string()),
        shared_key_hash: None,
        project_grant_id: None,
    }
}

fn open_auth_context() -> crate::auth::AuthContext {
    crate::auth::shared_key::open_anonymous_context()
}

fn oauth_bridge_auth_context(hash: &str, scopes: Vec<&str>) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: None,
        username: None,
        api_key_id: Some("oauth-access-token".to_string()),
        role: Some("shared-key".to_string()),
        scopes: scopes.into_iter().map(str::to_string).collect(),
        is_bootstrap: false,
        token_kind: Some("oauth2_shared_key".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: Some(hash.to_string()),
        project_grant_id: None,
    }
}

fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("rust".to_string()),
        registration_source: None,
        description: Some("test project".to_string()),
        hooks: vec!["doctor".to_string(), "precommit".to_string()],
        disabled: false,
        revision: None,
        git_branch: Some("codex".to_string()),
        git_head: Some("9a7d3ce".to_string()),
        git_dirty: Some(false),
        updated_at: 123456,
        shell_profile: None,
    }
}

fn runner_registration(
    client_id: &str,
    agent_instance_id: &str,
    _projects: Vec<ShellAgentProjectSummary>,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: async_job_capabilities(),
        policy: None,
    }
}

fn v2_baseline_capabilities() -> ShellClientCapabilities {
    let mut value = serde_json::Map::new();
    // Shell remains RegistrationRequired in generation 2, so pin it false in
    // the baseline-only fixture.
    value.insert("shell".to_string(), serde_json::Value::Bool(false));
    for capability in AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES {
        value.insert((*capability).to_string(), serde_json::Value::Bool(true));
    }
    serde_json::from_value(serde_json::Value::Object(value)).unwrap()
}

fn async_job_capabilities() -> ShellClientCapabilities {
    let mut capabilities = v2_baseline_capabilities();
    capabilities.shell = true;
    capabilities
}

#[path = "mod_tests/agent_transport_auth.rs"]
mod agent_transport_auth;

#[path = "mod_tests/auth_owner.rs"]
mod auth_owner;

#[path = "mod_tests/shared_key_isolation.rs"]
mod shared_key_isolation;

#[path = "mod_tests/protocol_http.rs"]
mod protocol_http;

#[path = "mod_tests/raw_shell_http.rs"]
mod raw_shell_http;
