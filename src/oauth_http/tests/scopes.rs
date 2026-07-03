use super::*;

#[test]
fn normalize_oauth_scopes_defaults_to_client_global_intersection() {
    let normalized =
        normalize_oauth_scopes(None, "project:write runtime:read agent:poll admin").unwrap();

    assert_eq!(normalized, "runtime:read project:write");
}

#[test]
fn normalize_oauth_scopes_default_rejects_empty_intersection() {
    let err = normalize_oauth_scopes(None, "agent:poll admin unknown").unwrap_err();

    assert_eq!(err, OAuthAuthorizeError::InvalidScope("empty scope"));
}

#[test]
fn normalize_oauth_scopes_requested_subset_success() {
    let normalized = normalize_oauth_scopes(
        Some("project:write runtime:read"),
        "runtime:read project:read project:write",
    )
    .unwrap();

    assert_eq!(normalized, "runtime:read project:write");
}

#[test]
fn normalize_oauth_scopes_deduplicates_and_orders() {
    let normalized = normalize_oauth_scopes(
        Some("project:write runtime:read runtime:read"),
        "runtime:read project:read project:write",
    )
    .unwrap();

    assert_eq!(normalized, "runtime:read project:write");
}

#[test]
fn normalize_oauth_scopes_rejects_unknown_scope() {
    let err = normalize_oauth_scopes(Some("unknown"), "runtime:read unknown").unwrap_err();

    assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
}

#[test]
fn normalize_oauth_scopes_rejects_scope_not_allowed_by_client() {
    let err = normalize_oauth_scopes(Some("runtime:read"), "project:read").unwrap_err();

    assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
}

#[test]
fn normalize_oauth_scopes_rejects_agent_scope() {
    let err = normalize_oauth_scopes(Some("agent:poll"), "agent:poll").unwrap_err();

    assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
}

#[test]
fn normalize_oauth_scopes_rejects_admin_scope() {
    let err = normalize_oauth_scopes(Some("admin"), "admin").unwrap_err();

    assert_eq!(err, OAuthAuthorizeError::InvalidScope("invalid scope"));
}

#[test]
fn normalize_oauth_scopes_treats_empty_requested_as_default() {
    let normalized =
        normalize_oauth_scopes(Some(" \t\n"), "project:write runtime:read admin").unwrap();

    assert_eq!(normalized, "runtime:read project:write");
}
