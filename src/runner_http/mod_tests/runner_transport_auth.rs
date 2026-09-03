use super::*;

#[test]
fn enforce_runner_transport_rejects_user_token() {
    let alice = auth_context(Some("alice"), false);
    let err = enforce_runner_transport(Some(&alice), "client-1").unwrap_err();
    assert!(err.contains("user tokens are not allowed"), "got: {}", err);
}

#[test]
fn enforce_runner_transport_agent_token_matching_client_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(enforce_runner_transport(Some(&alice), "alice-laptop").is_ok());
    let err = enforce_runner_transport(Some(&alice), "other").unwrap_err();
    assert!(err.contains("not bound"), "got: {}", err);
}

#[test]
fn enforce_runner_transport_bootstrap_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(enforce_runner_transport(Some(&bootstrap), "any-client").is_ok());
}

#[test]
fn enforce_runner_transport_direct_shared_key_succeeds() {
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert!(enforce_runner_transport(Some(&shared), "any-client").is_ok());
    for scope in crate::auth::AGENT_SCOPES {
        assert!(require_runner_transport_scope(Some(&shared), scope).is_ok());
    }
}

#[test]
fn enforce_runner_transport_open_anonymous_is_rejected() {
    let open = open_auth_context();
    assert!(enforce_runner_transport(Some(&open), "client-a").is_err());
    assert!(require_runner_transport_scope(Some(&open), "agent:register").is_err());
}

#[test]
fn require_runner_transport_scope_agent_token_with_scope_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(require_runner_transport_scope(Some(&alice), "agent:poll").is_ok());
    assert!(require_runner_transport_scope(Some(&alice), "agent:register").is_err());
}

#[test]
fn require_runner_transport_scope_bootstrap_always_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(require_runner_transport_scope(Some(&bootstrap), "agent:register").is_ok());
}

#[test]
fn require_runner_transport_scope_user_token_rejected() {
    let alice = auth_context(Some("alice"), false);
    let err = require_runner_transport_scope(Some(&alice), "agent:register").unwrap_err();
    assert!(err.contains("missing required scope"), "got: {}", err);
}

#[test]
fn oauth_bridge_token_remains_blocked_from_runner_transport() {
    let bridge = oauth_bridge_auth_context(
        "hash-a",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    assert!(!bridge.is_lightweight());
    assert!(enforce_runner_transport(Some(&bridge), "client-a")
        .unwrap_err()
        .contains("user tokens are not allowed"));
    assert!(
        require_runner_transport_scope(Some(&bridge), "agent:register")
            .unwrap_err()
            .contains("missing required scope")
    );
}
