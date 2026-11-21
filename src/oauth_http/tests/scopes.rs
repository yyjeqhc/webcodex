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
fn normalize_oauth_scopes_accepts_offline_access_without_expanding_client_permissions() {
    let normalized =
        normalize_oauth_scopes(Some("offline_access runtime:read"), "runtime:read").unwrap();

    assert_eq!(normalized, "runtime:read offline_access");
}

#[test]
fn oauth_discovery_scopes_include_offline_access_but_permission_registry_does_not() {
    assert!(oauth_discovery_scopes_supported().contains(&OAUTH_OFFLINE_ACCESS_SCOPE));
    assert!(!oauth_scopes_supported().contains(&OAUTH_OFFLINE_ACCESS_SCOPE));
}

#[test]
fn direct_shared_key_model_scopes_are_all_oauth_delegable() {
    for scope in crate::auth::DIRECT_SHARED_KEY_MODEL_SCOPES {
        assert!(
            oauth_scopes_supported().contains(scope),
            "direct shared-key model scope {scope} must be present in the OAuth permission registry"
        );
    }
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

#[test]
fn dynamically_registered_clients_narrow_global_discovery_scopes_without_widening_authority() {
    let (_tmp, db) = test_db();
    let owner = seed_user(&db, "scope-owner");
    let (mut client, _) = seed_client(&db, &owner, "manual");
    let global = oauth_discovery_scopes_supported().join(" ");

    // Existing manually registered clients keep strict allow-list semantics.
    assert_eq!(
        scope_registry::normalize_authorize_scopes(Some(&global), &client).unwrap_err(),
        OAuthAuthorizeError::InvalidScope("invalid scope")
    );

    client.dynamic_registration = Some(crate::models::DynamicClientRegistration {
        issuer: "https://example.test".to_string(),
        auth_method: crate::models::DynamicClientAuth::Post,
        refresh: true,
    });
    client.allowed_scopes = webcodex_core::authority::profiles::LOCAL_USER.join(" ");
    let narrowed = scope_registry::normalize_authorize_scopes(Some(&global), &client).unwrap();
    assert_eq!(
        narrowed,
        format!("{} offline_access", client.allowed_scopes)
    );
    assert!(!narrowed.contains("account:manage"));
    assert!(!narrowed.contains("computer:control"));

    // Unknown scopes and a request with no permitted WebCodex permission fail closed.
    assert_eq!(
        scope_registry::normalize_authorize_scopes(Some("runtime:read unknown"), &client)
            .unwrap_err(),
        OAuthAuthorizeError::InvalidScope("invalid scope")
    );
    assert_eq!(
        scope_registry::normalize_authorize_scopes(Some("account:manage offline_access"), &client)
            .unwrap_err(),
        OAuthAuthorizeError::InvalidScope("empty scope")
    );
}
