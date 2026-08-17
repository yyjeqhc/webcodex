use super::*;

fn managed_oauth_auth_context(
    username: &str,
    shared_key_hash: Option<&str>,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: Some(format!("user-{}", username)),
        username: Some(username.to_string()),
        api_key_id: Some("oauth-access-token".to_string()),
        role: Some("user".to_string()),
        scopes: Vec::new(),
        is_bootstrap: false,
        token_kind: Some("oauth2".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: shared_key_hash.map(str::to_string),
        project_grant_id: None,
    }
}

#[tokio::test]
async fn registry_filters_lightweight_clients_by_auth_group() {
    let registry = ShellClientRegistry::default();
    let shared_a = crate::auth::shared_key::shared_key_context("token-a");
    let shared_b = crate::auth::shared_key::shared_key_context("token-b");
    let shared_hash = crate::auth::shared_key::shared_key_hash_of("token-a");
    let bridge_a = oauth_bridge_auth_context(&shared_hash, vec![]);
    let managed_oauth = managed_oauth_auth_context("alice", Some("hash-a"));
    let open = open_auth_context();
    let bootstrap = auth_context(None, true);

    for (client_id, auth) in [
        ("shared-a", &shared_a),
        ("shared-b", &shared_b),
        ("open", &open),
    ] {
        registry
            .register_with_auth(
                ShellClientRegisterRequest {
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    client_id: client_id.to_string(),
                    agent_instance_id: format!("inst-{}", client_id),
                    display_name: None,
                    owner: None,
                    hostname: None,
                    host_context: None,
                    capabilities: Some(async_job_capabilities()),
                    projects: Some(vec![project_summary(client_id, "/tmp/project")]),
                    agent_protocol_version: None,
                    policy: None,
                },
                Some(auth),
            )
            .await
            .unwrap();
    }
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "managed".to_string(),
            agent_instance_id: "inst-managed".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: Some(vec![project_summary("managed", "/tmp/managed")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    let visible_to_a: Vec<String> = registry
        .list_clients_for_auth(Some(&shared_a))
        .await
        .into_iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(visible_to_a, vec!["shared-a"]);
    let visible_to_bridge_a: Vec<String> = registry
        .list_clients_for_auth(Some(&bridge_a))
        .await
        .into_iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(visible_to_bridge_a, vec!["shared-a"]);
    assert!(registry
        .assert_client_access(Some(&shared_a), "shared-a")
        .await
        .is_ok());
    assert!(registry
        .assert_client_access(Some(&bridge_a), "shared-a")
        .await
        .is_ok());
    assert!(registry
        .assert_client_access(Some(&shared_a), "shared-b")
        .await
        .unwrap_err()
        .contains("unknown shell client"));
    assert!(registry
        .assert_client_access(Some(&shared_a), "open")
        .await
        .unwrap_err()
        .contains("unknown shell client"));
    assert!(registry
        .assert_client_access(Some(&bridge_a), "shared-b")
        .await
        .unwrap_err()
        .contains("unknown shell client"));
    assert!(registry
        .assert_client_access(Some(&bridge_a), "open")
        .await
        .unwrap_err()
        .contains("unknown shell client"));

    let visible_to_open: Vec<String> = registry
        .list_clients_for_auth(Some(&open))
        .await
        .into_iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(visible_to_open, vec!["open"]);
    assert_eq!(
        ShellClientAuthGroup::from_auth(&open),
        Some(ShellClientAuthGroup::OpenAnonymous)
    );
    assert_eq!(
        ShellClientAuthGroup::from_auth(&bridge_a),
        Some(ShellClientAuthGroup::SharedKey(shared_hash))
    );
    assert!(bridge_a.is_oauth_shared_key_subject());
    assert_eq!(ShellClientAuthGroup::from_auth(&managed_oauth), None);
    assert!(!managed_oauth.is_oauth_shared_key_subject());
    let visible_to_managed_oauth: Vec<String> = registry
        .list_clients_for_auth(Some(&managed_oauth))
        .await
        .into_iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(visible_to_managed_oauth, vec!["managed"]);
    assert!(registry
        .assert_client_access(Some(&managed_oauth), "managed")
        .await
        .is_ok());
    assert!(registry
        .assert_client_access(Some(&managed_oauth), "shared-a")
        .await
        .unwrap_err()
        .contains("unknown shell client"));

    let visible_to_bootstrap: Vec<String> = registry
        .list_clients_for_auth(Some(&bootstrap))
        .await
        .into_iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(
        visible_to_bootstrap,
        vec!["managed", "open", "shared-a", "shared-b"]
    );
}

#[tokio::test]
async fn same_client_id_in_different_project_grants_is_isolated() {
    // Expected pre-fix failure: reusing the same instance id currently
    // lets a second auth group replace the first group's global lease.
    let registry = ShellClientRegistry::default();
    let grant_a = crate::auth::shared_key::project_credential_context("wc_pgrant_aaaaaaaaaaaaaaaa");
    let grant_b = crate::auth::shared_key::project_credential_context("wc_pgrant_bbbbbbbbbbbbbbbb");
    let registration = |hostname: &str, project: &str| ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        client_id: "same-project-agent".to_string(),
        agent_instance_id: "same-instance-id".to_string(),
        display_name: None,
        owner: None,
        hostname: Some(hostname.to_string()),
        host_context: None,
        capabilities: Some(async_job_capabilities()),
        projects: Some(vec![project_summary(project, "/tmp/project")]),
        agent_protocol_version: None,
        policy: None,
    };
    registry
        .register_with_auth(
            registration("grant-a-host", "grant-a-project"),
            Some(&grant_a),
        )
        .await
        .unwrap();

    let error = registry
        .register_with_auth(
            registration("grant-b-host", "grant-b-project"),
            Some(&grant_b),
        )
        .await
        .unwrap_err();
    assert!(!error.contains("grant-a-host"));
    assert!(!error.contains("grant-a-project"));
    let original = registry
        .get_client_view_for_auth("same-project-agent", Some(&grant_a))
        .await
        .expect("the original grant must retain its lease");
    assert_eq!(original.hostname.as_deref(), Some("grant-a-host"));
    assert!(registry
        .get_client_view_for_auth("same-project-agent", Some(&grant_b))
        .await
        .is_none());
}

#[tokio::test]
async fn shared_key_client_id_collision_cannot_cross_group_or_revive_old_connection() {
    let registry = ShellClientRegistry::default();
    let shared_a = crate::auth::shared_key::shared_key_context("shared-a");
    let shared_b = crate::auth::shared_key::shared_key_context("shared-b");
    let managed = agent_auth_context(
        "managed",
        "managed-client",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    let bootstrap = auth_context(None, true);
    let registration = |client_id: &str, instance: &str, hostname: &str, owner: Option<&str>| {
        ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: Some(hostname.to_string()),
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: Some(vec![project_summary("project", "/tmp/project")]),
            agent_protocol_version: None,
            policy: None,
        }
    };

    registry
        .register_with_auth_connection(
            registration("shared-client", "shared-instance", "host-a", None),
            Some(&shared_a),
            Some("connection-a"),
        )
        .await
        .unwrap();
    registry
        .register_with_auth_connection(
            registration(
                "managed-client",
                "managed-instance",
                "managed-host",
                Some("managed"),
            ),
            Some(&managed),
            Some("managed-connection"),
        )
        .await
        .unwrap();

    let collision = registry
        .register_with_auth_connection(
            registration("shared-client", "shared-instance", "host-b", None),
            Some(&shared_b),
            Some("connection-b"),
        )
        .await
        .unwrap_err();
    assert_eq!(collision, "agent client identity is unavailable");
    assert!(!collision.contains("host-a"));
    assert_eq!(
        registry
            .get_client_view_for_auth("shared-client", Some(&shared_a))
            .await
            .unwrap()
            .hostname
            .as_deref(),
        Some("host-a"),
        "cross-group collision must not refresh or replace the original record"
    );
    assert!(registry
        .get_client_view_for_auth("shared-client", Some(&shared_b))
        .await
        .is_none());
    assert!(registry
        .get_client_view_for_auth("shared-client", Some(&managed))
        .await
        .is_none());
    assert!(registry
        .get_client_view_for_auth("managed-client", Some(&shared_a))
        .await
        .is_none());
    assert!(registry
        .get_client_view_for_auth("shared-client", Some(&bootstrap))
        .await
        .is_some());

    // Same key + same process identity is a legitimate reconnect and replaces
    // only the concrete connection lease.
    registry
        .register_with_auth_connection(
            registration("shared-client", "shared-instance", "host-a-new", None),
            Some(&shared_a),
            Some("connection-new"),
        )
        .await
        .unwrap();
    let old_connection = registry
        .touch_client_for_connection("shared-client", "shared-instance", "connection-a")
        .await
        .unwrap_err();
    assert!(old_connection.contains("transport connection is no longer active"));
    registry
        .touch_client_for_connection("shared-client", "shared-instance", "connection-new")
        .await
        .unwrap();
}
