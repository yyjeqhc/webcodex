use super::*;
use crate::shell_protocol::{
    ShellCommandExecutionState, AGENT_PROTOCOL_VERSION_QUIC_V1,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
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

fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("rust".to_string()),
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
    projects: Vec<ShellAgentProjectSummary>,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: Some(async_job_capabilities()),
        projects: Some(projects),
        agent_protocol_version: None,
        policy: None,
    }
}

fn async_job_capabilities() -> ShellClientCapabilities {
    ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        jobs: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn registration_retains_reported_job_concurrency_and_preserves_legacy_unknown() {
    let registry = ShellClientRegistry::default();
    for limit in [1, 4, 8, 64] {
        let client_id = format!("current-limit-{limit}");
        let instance_id = format!("inst-current-{limit}");
        let mut current = runner_registration(&client_id, &instance_id, Vec::new());
        current.job_concurrency_limit = Some(limit);
        let current_view = registry.register(current).await.unwrap();
        assert_eq!(current_view.job_concurrency_limit, Some(limit));
        assert_eq!(
            registry
                .get_client_view(&client_id)
                .await
                .unwrap()
                .job_concurrency_limit,
            Some(limit)
        );
    }

    let legacy = registry
        .register(runner_registration(
            "legacy-limit",
            "inst-legacy",
            Vec::new(),
        ))
        .await
        .unwrap();
    assert_eq!(legacy.job_concurrency_limit, None);

    for (client_id, limit) in [("invalid-limit-zero", 0), ("invalid-limit-high", 65)] {
        let mut invalid = runner_registration(client_id, "inst-invalid", Vec::new());
        invalid.job_concurrency_limit = Some(limit);
        assert_eq!(
            registry.register(invalid).await.unwrap_err(),
            "job_concurrency_limit must be between 1 and 64"
        );
    }
}

fn file_request(op: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: op.to_string(),
        client_id: "oe".to_string(),
        path: "src/auth/scopes.rs".to_string(),
        cwd: Some("/root/git/webcodex".to_string()),
        content: None,
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 0,
    }
}

#[path = "mod_tests/file_validation.rs"]
mod file_validation;

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

#[tokio::test]
async fn shared_key_runner_limit_is_per_group_and_reconnects_do_not_consume_capacity() {
    let registry = ShellClientRegistry::default();
    let shared_a = crate::auth::shared_key::shared_key_context("shared-limit-a");
    let shared_b = crate::auth::shared_key::shared_key_context("shared-limit-b");

    for index in 0..MAX_SHARED_KEY_RUNNERS_PER_GROUP {
        registry
            .register_with_auth(
                runner_registration(
                    &format!("shared-a-{index}"),
                    &format!("shared-a-instance-{index}"),
                    vec![project_summary(&format!("project-{index}"), "/tmp/project")],
                ),
                Some(&shared_a),
            )
            .await
            .unwrap();
    }

    let mut reconnect = runner_registration(
        "shared-a-0",
        "shared-a-instance-0",
        vec![project_summary("replacement-project", "/tmp/replacement")],
    );
    reconnect.hostname = Some("reconnected-host".to_string());
    registry
        .register_with_auth(reconnect, Some(&shared_a))
        .await
        .expect("an existing shared-key client reconnect must not consume capacity");

    let rejected = registry
        .register_with_auth(
            runner_registration(
                "shared-a-over-limit",
                "shared-a-over-limit-instance",
                vec![project_summary("over-limit", "/tmp/over-limit")],
            ),
            Some(&shared_a),
        )
        .await
        .unwrap_err();
    assert_eq!(
        rejected,
        format!(
            "shared-key runner group limit reached (maximum {} runners)",
            MAX_SHARED_KEY_RUNNERS_PER_GROUP
        )
    );
    assert!(registry
        .get_client_view("shared-a-over-limit")
        .await
        .is_none());
    assert_eq!(
        registry
            .get_client_view_for_auth("shared-a-0", Some(&shared_a))
            .await
            .unwrap()
            .hostname
            .as_deref(),
        Some("reconnected-host"),
        "a rejected new client must not modify an existing registration"
    );

    registry
        .register_with_auth(
            runner_registration(
                "shared-b-0",
                "shared-b-instance-0",
                vec![project_summary("project-b", "/tmp/project-b")],
            ),
            Some(&shared_b),
        )
        .await
        .expect("a different shared-key group has its own capacity");

    let managed = agent_auth_context(
        "managed",
        "managed-beyond-shared-group-limit",
        vec!["agent:register"],
    );
    let mut managed_registration = runner_registration(
        "managed-beyond-shared-group-limit",
        "managed-instance",
        vec![project_summary("managed-project", "/tmp/managed")],
    );
    managed_registration.owner = Some("managed".to_string());
    registry
        .register_with_auth(managed_registration, Some(&managed))
        .await
        .expect("managed Agent Tokens are not subject to shared-key group capacity");
}

#[tokio::test]
async fn shared_key_global_runner_limit_excludes_reconnects_and_managed_runners() {
    let registry = ShellClientRegistry::with_shared_key_limits_for_test(
        MAX_SHARED_KEY_RUNNERS_PER_GROUP,
        3,
        SHARED_KEY_OFFLINE_TTL_SECS,
    );
    let shared_a = crate::auth::shared_key::shared_key_context("shared-global-a");
    let shared_b = crate::auth::shared_key::shared_key_context("shared-global-b");
    let shared_c = crate::auth::shared_key::shared_key_context("shared-global-c");

    for (client_id, instance_id, auth) in [
        ("global-a-0", "global-a-instance-0", &shared_a),
        ("global-a-1", "global-a-instance-1", &shared_a),
        ("global-b-0", "global-b-instance-0", &shared_b),
    ] {
        registry
            .register_with_auth(
                runner_registration(client_id, instance_id, Vec::new()),
                Some(auth),
            )
            .await
            .unwrap();
    }

    let rejected = registry
        .register_with_auth(
            runner_registration("global-c-0", "global-c-instance-0", Vec::new()),
            Some(&shared_c),
        )
        .await
        .unwrap_err();
    assert_eq!(
        rejected,
        "shared-key runner global limit reached (maximum 3 runners)"
    );

    registry
        .register_with_auth(
            runner_registration("global-a-0", "global-a-instance-0", Vec::new()),
            Some(&shared_a),
        )
        .await
        .expect("an existing client reconnect remains available at global capacity");

    let managed = agent_auth_context(
        "managed",
        "managed-at-shared-global-limit",
        vec!["agent:register"],
    );
    let mut managed_registration = runner_registration(
        "managed-at-shared-global-limit",
        "managed-global-instance",
        Vec::new(),
    );
    managed_registration.owner = Some("managed".to_string());
    registry
        .register_with_auth(managed_registration, Some(&managed))
        .await
        .expect("managed Agent Tokens are excluded from shared-key global capacity");
}

#[tokio::test]
async fn shared_key_offline_ttl_prunes_only_expired_clients_and_all_associated_state() {
    let ttl_secs = 10;
    let registry = ShellClientRegistry::with_shared_key_limits_for_test(1, 4, ttl_secs);
    let connected_auth = crate::auth::shared_key::shared_key_context("ttl-connected");
    let fresh_auth = crate::auth::shared_key::shared_key_context("ttl-fresh");
    let expired_auth = crate::auth::shared_key::shared_key_context("ttl-expired");

    registry
        .register_with_auth_connection(
            runner_registration("ttl-connected", "ttl-connected-instance", Vec::new()),
            Some(&connected_auth),
            Some("ttl-connected-connection"),
        )
        .await
        .unwrap();
    registry
        .register_notifier_for_connection(
            "ttl-connected",
            "ttl-connected-instance",
            "ttl-connected-connection",
            Arc::new(Notify::new()),
        )
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("ttl-connected", now_ts() - ttl_secs - 100)
        .await;

    registry
        .register_with_auth_connection(
            runner_registration("ttl-fresh", "ttl-fresh-instance", Vec::new()),
            Some(&fresh_auth),
            Some("ttl-fresh-connection"),
        )
        .await
        .unwrap();
    registry
        .register_notifier_for_connection(
            "ttl-fresh",
            "ttl-fresh-instance",
            "ttl-fresh-connection",
            Arc::new(Notify::new()),
        )
        .await
        .unwrap();
    registry
        .reconcile_disconnect_for_connection(
            "ttl-fresh",
            "ttl-fresh-instance",
            "ttl-fresh-connection",
        )
        .await;

    registry
        .register_with_auth(
            runner_registration(
                "ttl-expired",
                "ttl-expired-instance",
                vec![project_summary("expired-project", "/tmp/expired")],
            ),
            Some(&expired_auth),
        )
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("ttl-expired".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let (sync_request_id, sync_rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "ttl-expired".to_string(),
                cwd: None,
                command: "echo pending".to_string(),
                stdin: None,
                timeout_secs: 30,
                wait_timeout_secs: 30,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    registry.record_hidden_cleanup_intent(job.job_id.clone(), None);
    let job_revision = {
        let mut inner = registry.inner.lock().await;
        let expired_at = now_ts() - CLIENT_ONLINE_WINDOW_SECS - ttl_secs - 1;
        let client = inner.clients.get_mut("ttl-expired").unwrap();
        client.last_seen = expired_at;
        client.disconnected_at = Some(expired_at);
        inner.retired_instances.insert(
            "ttl-expired".to_string(),
            std::collections::VecDeque::from(["old-expired-instance".to_string()]),
        );
        inner
            .unregistering_projects
            .insert("agent:ttl-expired:expired-project".to_string(), 1);
        inner
            .jobs_by_id
            .get(&job.job_id)
            .unwrap()
            .public_revision
            .clone()
    };
    let revision_before = job_revision.load(std::sync::atomic::Ordering::Relaxed);
    let observation_token = job.observation_token.clone().expect("observation token");
    let waiter_registry = registry.clone();
    let waiter_auth = expired_auth.clone();
    let waiter_job_id = job.job_id.clone();
    let waiter = tokio::spawn(async move {
        waiter_registry
            .job_log_for_auth(
                Some(&waiter_auth),
                &waiter_job_id,
                None,
                None,
                None,
                Some(&observation_token),
                Some(5),
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let visible_expired = registry.list_clients_for_auth(Some(&expired_auth)).await;
    assert!(visible_expired.is_empty());
    assert!(
        job_revision.load(std::sync::atomic::Ordering::Relaxed) > revision_before,
        "a non-terminal job must transition through the existing lost notifier"
    );
    let (waited_job, _, _, _, _, wait) =
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("lost transition should wake the observation waiter")
            .expect("waiter task should complete")
            .expect("same shared key should still observe the retained Job");
    assert!(wait.changed);
    assert!(wait.terminal);
    assert_eq!(waited_job.status, "lost");
    assert_eq!(
        waited_job.recovery_reason_code.as_deref(),
        Some("shared_key_runner_expired")
    );
    assert!(waited_job
        .error
        .as_deref()
        .is_some_and(|error| error.contains("registration expired")));
    let sync_response = tokio::time::timeout(std::time::Duration::from_secs(1), sync_rx)
        .await
        .expect("expired request waiter should be resolved")
        .expect("expired request waiter should receive a response");
    assert_eq!(sync_response.request_id, sync_request_id);
    assert!(sync_response
        .error
        .as_deref()
        .is_some_and(|error| error.contains("registration expired")));
    let retained = registry
        .get_job_for_auth(Some(&expired_auth), &job.job_id)
        .await
        .expect("same shared key should query the retained lost Job");
    assert_eq!(retained.status, "lost");
    let (first_terminal_observed_at, first_ended_at, first_error, first_reason, first_revision) = {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        (
            record
                .terminal_observed_at
                .expect("TTL prune records Server terminal observation time"),
            record.ended_at,
            record.error.clone(),
            record.recovery_reason_code.clone(),
            record
                .public_revision
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    };
    registry.prune_expired_shared_key_clients().await;
    {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        assert_eq!(record.status, "lost");
        assert_eq!(record.ended_at, first_ended_at);
        assert_eq!(record.error, first_error);
        assert_eq!(record.recovery_reason_code, first_reason);
        assert_eq!(
            record.terminal_observed_at,
            Some(first_terminal_observed_at),
            "repeated TTL prune must not extend retention"
        );
        assert_eq!(
            record
                .public_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            first_revision,
            "repeated TTL prune must not publish a duplicate terminal update"
        );
    }
    let other_key = crate::auth::shared_key::shared_key_context("ttl-other-key");
    assert!(registry
        .get_job_for_auth(Some(&other_key), &job.job_id)
        .await
        .unwrap_err()
        .contains("unknown shell job"));
    let managed_reader = auth_context(Some("managed-reader"), false);
    assert!(registry
        .get_job_for_auth(Some(&managed_reader), &job.job_id)
        .await
        .unwrap_err()
        .contains("unknown shell job"));
    assert!(!registry.has_hidden_cleanup_intent_for_test(&job.job_id));
    assert!(registry.list_client_projects("ttl-expired").await.is_err());
    {
        let inner = registry.inner.lock().await;
        assert!(!inner.clients.contains_key("ttl-expired"));
        assert!(!inner.queues_by_client.contains_key("ttl-expired"));
        assert!(!inner.notifiers.contains_key("ttl-expired"));
        assert!(!inner.retired_instances.contains_key("ttl-expired"));
        assert!(inner
            .pending_by_id
            .values()
            .all(|pending| pending.request.client_id != "ttl-expired"));
        let retained = inner
            .jobs_by_id
            .get(&job.job_id)
            .expect("lost Job retained");
        assert_eq!(retained.status, "lost");
        assert_eq!(retained.client_id, "ttl-expired");
        assert!(inner
            .unregistering_projects
            .keys()
            .all(|project_id| !project_id.starts_with("agent:ttl-expired:")));
    }

    assert!(registry
        .get_client_view_for_auth("ttl-connected", Some(&connected_auth))
        .await
        .is_some());
    assert!(registry
        .get_client_view_for_auth("ttl-fresh", Some(&fresh_auth))
        .await
        .is_some());

    let managed = agent_auth_context("managed", "ttl-managed", vec!["agent:register"]);
    let mut managed_registration =
        runner_registration("ttl-managed", "ttl-managed-instance", Vec::new());
    managed_registration.owner = Some("managed".to_string());
    registry
        .register_with_auth(managed_registration, Some(&managed))
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        let managed_record = inner.clients.get_mut("ttl-managed").unwrap();
        managed_record.last_seen = now_ts() - ttl_secs - 100;
        managed_record.disconnected_at = Some(now_ts() - ttl_secs - 100);
    }
    assert!(registry
        .get_client_view_for_auth("ttl-managed", Some(&managed))
        .await
        .is_some());

    {
        let mut inner = registry.inner.lock().await;
        inner
            .jobs_by_id
            .get_mut(&job.job_id)
            .unwrap()
            .terminal_observed_at =
            Some(now_ts() - crate::shell_protocol::JOB_TERMINAL_RETENTION_SECS);
    }
    super::reconciliation::recovery_timeout_sweep(&registry).await;
    assert!(registry
        .get_job_for_auth(Some(&expired_auth), &job.job_id)
        .await
        .is_err());
    {
        let inner = registry.inner.lock().await;
        assert!(!inner.request_to_job.values().any(|id| id == &job.job_id));
        assert!(inner
            .pending_by_id
            .values()
            .all(|pending| { pending.job_id.as_deref() != Some(job.job_id.as_str()) }));
        assert!(inner.persistent_waiters.is_empty());
        assert!(inner
            .queues_by_client
            .values()
            .all(|queue| queue.iter().all(|request_id| {
                inner
                    .pending_by_id
                    .get(request_id)
                    .is_none_or(|pending| pending.job_id.as_deref() != Some(job.job_id.as_str()))
            })));
    }

    registry
        .register_with_auth(
            runner_registration(
                "ttl-expired-replacement",
                "ttl-expired-replacement-instance",
                Vec::new(),
            ),
            Some(&expired_auth),
        )
        .await
        .expect("pruning an expired runner must release its group capacity");
}

#[tokio::test]
async fn shared_key_project_summary_limit_uses_raw_input_and_preserves_existing_projects() {
    let registry = ShellClientRegistry::default();
    let shared = crate::auth::shared_key::shared_key_context("project-limit-shared");
    let projects = (0..MAX_RUNNER_PROJECT_SUMMARIES)
        .map(|index| project_summary(&format!("project-{index}"), "/tmp/project"))
        .collect::<Vec<_>>();
    registry
        .register_with_auth(
            runner_registration("project-limit", "project-limit-instance", projects.clone()),
            Some(&shared),
        )
        .await
        .expect("the documented project limit is accepted");

    let too_many = (0..=MAX_RUNNER_PROJECT_SUMMARIES)
        .map(|index| project_summary(&format!("project-{index}"), "/tmp/project"))
        .collect::<Vec<_>>();
    let error = registry
        .register_with_auth(
            runner_registration("project-limit", "project-limit-instance", too_many),
            Some(&shared),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error,
        format!(
            "runner project summary limit exceeded (maximum {} projects)",
            MAX_RUNNER_PROJECT_SUMMARIES
        )
    );
    assert_eq!(
        registry
            .list_client_projects("project-limit")
            .await
            .unwrap()
            .len(),
        MAX_RUNNER_PROJECT_SUMMARIES
    );

    let duplicate_projects =
        vec![project_summary("duplicate", "/tmp/duplicate"); MAX_RUNNER_PROJECT_SUMMARIES + 1];
    let poll_error = registry
        .poll(ShellAgentPollRequest {
            client_id: "project-limit".to_string(),
            agent_instance_id: "project-limit-instance".to_string(),
            projects: Some(duplicate_projects),
        })
        .await
        .unwrap_err();
    assert!(poll_error.contains("project summary limit exceeded"));
    assert_eq!(
        registry
            .list_client_projects("project-limit")
            .await
            .unwrap()
            .len(),
        MAX_RUNNER_PROJECT_SUMMARIES,
        "an oversized polling refresh must not overwrite the existing project list"
    );
    let upsert_error = registry
        .upsert_client_project(
            "project-limit",
            project_summary("project-over-limit", "/tmp/project-over-limit"),
        )
        .await
        .unwrap_err();
    assert!(upsert_error.contains("project summary limit reached"));
    registry
        .upsert_client_project(
            "project-limit",
            project_summary("project-0", "/tmp/project-replaced"),
        )
        .await
        .expect("replacing an existing project remains allowed at the limit");
    assert_eq!(
        registry
            .list_client_projects("project-limit")
            .await
            .unwrap()
            .len(),
        MAX_RUNNER_PROJECT_SUMMARIES
    );

    let managed = agent_auth_context("managed", "managed-project-limit", vec!["agent:register"]);
    let mut managed_registration = runner_registration(
        "managed-project-limit",
        "managed-project-limit-instance",
        vec![project_summary("duplicate", "/tmp/duplicate"); MAX_RUNNER_PROJECT_SUMMARIES + 1],
    );
    managed_registration.owner = Some("managed".to_string());
    let managed_error = registry
        .register_with_auth(managed_registration, Some(&managed))
        .await
        .unwrap_err();
    assert!(managed_error.contains("project summary limit exceeded"));
}

#[test]
fn requested_by_from_auth_uses_bootstrap_username_or_anonymous() {
    let bootstrap = auth_context(None, true);
    assert_eq!(requested_by_from_auth(Some(&bootstrap)), "bootstrap");

    let alice = auth_context(Some("alice"), false);
    assert_eq!(requested_by_from_auth(Some(&alice)), "alice");

    assert_eq!(requested_by_from_auth(None), "anonymous");
}

#[test]
fn assert_shell_client_owner_enforces_owner_boundary() {
    let bootstrap = auth_context(None, true);
    assert!(assert_shell_client_owner(Some(&bootstrap), "client-1", None).is_ok());

    let alice = auth_context(Some("alice"), false);
    assert!(assert_shell_client_owner(Some(&alice), "client-1", Some("alice")).is_ok());

    let mismatch = assert_shell_client_owner(Some(&alice), "client-1", Some("bob")).unwrap_err();
    assert!(mismatch.contains("owned by bob"));
    assert!(mismatch.contains("belongs to alice"));

    let missing = assert_shell_client_owner(Some(&alice), "client-1", None).unwrap_err();
    assert_eq!(missing, "agent client client-1 has no owner");

    let anonymous = assert_shell_client_owner(None, "client-1", Some("anonymous")).unwrap_err();
    assert!(anonymous.contains("belongs to anonymous"));
}

#[tokio::test]
async fn registry_registers_and_lists_client() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: Some("XRH".to_string()),
            owner: Some("yyjeqhc".to_string()),
            hostname: Some("fineserver".to_string()),
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].client_id, "xrh");
    assert!(clients[0].connected);
    assert_eq!(clients[0].pending_requests, 0);
}

#[tokio::test]
async fn registry_register_saves_projects() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let clients = registry.list_clients().await;
    assert_eq!(clients[0].projects.len(), 1);
    assert_eq!(clients[0].projects[0].id, "webcodex");

    let projects = registry.list_client_projects("oe").await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].path, "/root/git/webcodex");
}

#[tokio::test]
async fn registry_poll_updates_projects() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![project_summary("one", "/tmp/one")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: Some(vec![
                project_summary("one", "/tmp/one"),
                project_summary("two", "/tmp/two"),
            ]),
        })
        .await
        .unwrap();
    assert!(polled.is_none());

    let projects = registry.list_client_projects("oe").await.unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].id, "one");
    assert_eq!(projects[1].id, "two");
}

#[tokio::test]
async fn registry_poll_without_projects_preserves_existing_projection() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![project_summary("one", "/tmp/one")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(polled.is_none());

    let projects = registry.list_client_projects("oe").await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "one");
    assert_eq!(projects[0].path, "/tmp/one");
}

#[tokio::test]
async fn registry_project_owner_check_enforces_boundary() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "alice-client".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "bob-client".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("bob".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: Some(vec![project_summary("secret", "/tmp/secret")]),
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    let alice = auth_context(Some("alice"), false);
    assert!(
        assert_registry_client_owner(&registry, Some(&alice), "alice-client")
            .await
            .is_ok()
    );
    let projects = registry.list_client_projects("alice-client").await.unwrap();
    assert_eq!(projects.len(), 1);

    let mismatch = assert_registry_client_owner(&registry, Some(&alice), "bob-client")
        .await
        .unwrap_err();
    assert_eq!(mismatch.0, StatusCode::FORBIDDEN);
    assert!(mismatch.1.contains("owned by bob"));
}

#[path = "mod_tests/protocol.rs"]
mod protocol;

#[tokio::test]
async fn registry_enqueues_polls_and_completes_shell_request() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (request_id, rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: Some("/tmp".to_string()),
                command: "echo hello".to_string(),
                stdin: Some("hello stdin".to_string()),
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.command, "echo hello");
    assert_eq!(polled.stdin.as_deref(), Some("hello stdin"));
    registry
        .complete(ShellAgentResultRequest {
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(12),
            error: None,
        })
        .await
        .unwrap();
    let response = rx.await.unwrap();
    assert!(response.success);
    assert_eq!(response.stdout.as_deref(), Some("hello\n"));
}

#[tokio::test]
async fn polling_out_of_order_results_resolve_only_their_original_waiters() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (request_a, waiter_a) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "ordered".to_string(),
                cwd: None,
                command: "slow-a".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let (request_b, waiter_b) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "ordered".to_string(),
                cwd: None,
                command: "fast-b".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let polled_a = registry
        .poll(ShellAgentPollRequest {
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    let polled_b = registry
        .poll(ShellAgentPollRequest {
            client_id: "ordered".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled_a.request_id, request_a);
    assert_eq!(polled_b.request_id, request_b);

    for (request_id, stdout) in [(&request_b, "result-b\n"), (&request_a, "result-a\n")] {
        registry
            .complete(ShellAgentResultRequest {
                client_id: "ordered".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some(stdout.to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    }

    let response_b = waiter_b.await.unwrap();
    let response_a = waiter_a.await.unwrap();
    assert_eq!(response_b.request_id, request_b);
    assert_eq!(response_b.stdout.as_deref(), Some("result-b\n"));
    assert_eq!(response_a.request_id, request_a);
    assert_eq!(response_a.stdout.as_deref(), Some("result-a\n"));
}

#[tokio::test]
async fn registry_allows_session_scoped_run_without_ssh_resource() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "xrh".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    let (request_id, _rx) = registry
        .enqueue_run_with_sandbox_and_ssh(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo local".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            None,
            None,
            Some("wc_sess_local".to_string()),
        )
        .await
        .unwrap();
    assert!(!registry.cancel_request(&request_id).await);

    let error = registry
        .enqueue_run_with_sandbox_and_ssh(
            ShellRunRequest {
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo remote".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            None,
            Some("tmp".to_string()),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("ssh_session_required"), "{error}");
}

#[tokio::test]
async fn registry_rejects_unknown_client_run() {
    let registry = ShellClientRegistry::default();
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "missing".to_string(),
                cwd: None,
                command: "pwd".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("unknown shell client"));
}

async fn register_computer_test_client(
    registry: &ShellClientRegistry,
    client_id: &str,
    owner: &str,
    observe_capable: bool,
    accessibility_capable: bool,
    control_capable: bool,
    text_input_capable: bool,
) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some(owner.to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: observe_capable,
                computer_accessibility_observe: accessibility_capable,
                computer_control: control_capable,
                computer_window_activate: false,
                computer_text_input: text_input_capable,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn computer_enqueue_requires_exact_owner_and_distinct_capability() {
    let registry = ShellClientRegistry::default();
    register_computer_test_client(
        &registry,
        "computer-owned",
        "alice",
        false,
        false,
        false,
        false,
    )
    .await;
    let alice = auth_context(Some("alice"), false);
    let error = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_list_windows",
            r#"{"limit":1}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_observe"));

    register_computer_test_client(
        &registry,
        "computer-owned",
        "alice",
        true,
        false,
        false,
        false,
    )
    .await;
    let bob = auth_context(Some("bob"), false);
    let error = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_list_windows",
            r#"{"limit":1}"#.to_string(),
            "bob".to_string(),
            Some(&bob),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("owned by alice"));

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-owned".to_string(),
            "computer_snapshot",
            r#"{"surface_id":"surface_test"}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-owned".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer request");
    assert_eq!(request.kind, "computer_snapshot");
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_snapshot_artifact_requires_current_target_project_and_file_write_under_registry_lock(
) {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    let request = |client_id: &str| {
        ShellFileOpRequest {
        op: "save_project_artifact".to_string(),
        client_id: client_id.to_string(),
        path: "artifacts/ui.jpg".to_string(),
        cwd: Some("/tmp/project".to_string()),
        content: Some(
            r#"{"path":"artifacts/ui.jpg","content_base64":"/9j/4A==","mime_type":"image/jpeg","overwrite":false,"max_bytes":1048576}"#
                .to_string(),
        ),
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 60,
    }
    };
    let register = |client_id: &str, instance_id: &str, file_write: bool, project_path: &str| {
        ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance_id.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write,
                ..Default::default()
            }),
            projects: Some(vec![project_summary("demo", project_path)]),
            agent_protocol_version: None,
            policy: None,
        }
    };

    registry
        .register(register(
            "computer-artifact-no-write",
            "artifact-no-write-inst",
            false,
            "/tmp/project",
        ))
        .await
        .unwrap();
    let error = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-no-write"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap_err();
    assert!(error.contains("file_write"), "{error}");

    registry
        .register(register(
            "computer-artifact-write",
            "artifact-inst",
            true,
            "/tmp/project",
        ))
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-write"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    let queued = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-write".to_string(),
            agent_instance_id: "artifact-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("snapshot artifact request should be queued");
    assert_eq!(queued.request_id, request_id);
    assert_eq!(queued.kind, "file_save_project_artifact");
    assert_eq!(queued.path.as_deref(), Some("artifacts/ui.jpg"));
    assert_eq!(queued.cwd.as_deref(), Some("/tmp/project"));
    assert!(queued.command.is_empty());

    registry
        .register(register(
            "computer-artifact-poll-change",
            "artifact-poll-inst",
            true,
            "/tmp/project",
        ))
        .await
        .unwrap();
    let (_request_id, response_rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-poll-change"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    // Poll may refresh project projection for the same process. The pending
    // placement fence is checked after that refresh but before dispatched=true.
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-poll-change".to_string(),
            agent_instance_id: "artifact-poll-inst".to_string(),
            projects: Some(vec![project_summary("demo", "/tmp/replaced")]),
        })
        .await
        .unwrap()
        .is_none());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), response_rx)
        .await
        .expect("stale placement response timed out")
        .expect("stale placement response channel closed");
    assert!(!response.success);
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale_project"),
        "{response:?}"
    );

    registry
        .register(register(
            "computer-artifact-cap-change",
            "artifact-cap-inst",
            true,
            "/tmp/project",
        ))
        .await
        .unwrap();
    let (_request_id, response_rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-cap-change"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    registry
        .register(register(
            "computer-artifact-cap-change",
            "artifact-cap-inst",
            false,
            "/tmp/project",
        ))
        .await
        .unwrap();
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-cap-change".to_string(),
            agent_instance_id: "artifact-cap-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), response_rx)
        .await
        .expect("capability downgrade response timed out")
        .expect("capability downgrade response channel closed");
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale_authority"),
        "{response:?}"
    );

    registry
        .register(register(
            "computer-artifact-owner-change",
            "artifact-owner-inst",
            true,
            "/tmp/project",
        ))
        .await
        .unwrap();
    let (_request_id, response_rx) = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-owner-change"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap();
    let mut changed_owner = register(
        "computer-artifact-owner-change",
        "artifact-owner-inst",
        true,
        "/tmp/project",
    );
    changed_owner.owner = Some("bob".to_string());
    registry.register(changed_owner).await.unwrap();
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-owner-change".to_string(),
            agent_instance_id: "artifact-owner-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), response_rx)
        .await
        .expect("owner change response timed out")
        .expect("owner change response channel closed");
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(
        response.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale_authority"),
        "{response:?}"
    );

    registry
        .register(register(
            "computer-artifact-replaced",
            "artifact-replaced-inst",
            true,
            "/tmp/project",
        ))
        .await
        .unwrap();
    // Model a placement captured before a reconnect: by admission time the same
    // Runner identity reports the project at a different path.
    registry
        .register(register(
            "computer-artifact-replaced",
            "artifact-replaced-inst",
            true,
            "/tmp/replaced",
        ))
        .await
        .unwrap();
    let error = registry
        .enqueue_computer_snapshot_artifact(
            request("computer-artifact-replaced"),
            "demo",
            "/tmp/project",
            "alice".to_string(),
            Some(&alice),
        )
        .await
        .unwrap_err();
    assert!(error.contains("stale_project"), "{error}");
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-artifact-replaced".to_string(),
            agent_instance_id: "artifact-replaced-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn computer_snapshot_region_requires_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-region-old",
        "alice",
        true,
        false,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","region":{"x":0,"y":0,"width":10,"height":10},"max_width":null,"max_height":null}"#;
    let error = registry
        .enqueue_computer(
            "computer-region-old".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_snapshot_region"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-region-only".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: false,
                computer_snapshot_region: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let error = registry
        .enqueue_computer(
            "computer-region-only".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_observe"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-region-new".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_snapshot_region: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-region-new".to_string(),
            "computer_snapshot_region",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-region-new".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued region snapshot request");
    assert_eq!(request.kind, "computer_snapshot_region");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
}

#[tokio::test]
async fn computer_accessibility_enqueue_requires_distinct_capability() {
    let registry = ShellClientRegistry::default();
    register_computer_test_client(&registry, "computer-ax", "alice", true, false, false, false)
        .await;
    let alice = auth_context(Some("alice"), false);
    let error = registry
        .enqueue_computer(
            "computer-ax".to_string(),
            "computer_accessibility_status",
            "{}".to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_accessibility_observe"));

    register_computer_test_client(&registry, "computer-ax", "alice", true, true, false, false)
        .await;
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-ax".to_string(),
            "computer_accessibility_tree",
            r#"{"surface_id":"surface_test","max_depth":2,"max_nodes":8}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-ax".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued accessibility request");
    assert_eq!(request.kind, "computer_accessibility_tree");
    assert!(request.command.is_empty());
}

#[tokio::test]
async fn computer_element_state_requires_its_own_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-state",
        "alice",
        true,
        true,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-state".to_string(),
            "computer_element_state",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_element_state"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-state-capable".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_accessibility_observe: true,
                computer_element_state: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-state-capable".to_string(),
            "computer_element_state",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-state-capable".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer element-state request");
    assert_eq!(request.kind, "computer_element_state");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
}

#[tokio::test]
async fn computer_control_enqueue_requires_independent_capability() {
    // CU-AX2 control remains independently fenced from CU-AX3 text input.
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test","action":"focus"}"#;
    let error = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_control"));

    register_computer_test_client(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-control".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer control request");
    assert_eq!(request.kind, "computer_control");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_scroll_to_element_requires_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-scroll-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-scroll-control-only".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_scroll_to_element"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-scroll-capable".to_string(),
            agent_instance_id: "computer-scroll-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                computer_control: true,
                computer_scroll_to_element: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-scroll-capable".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-scroll-capable".to_string(),
            agent_instance_id: "computer-scroll-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer scroll request");
    assert_eq!(request.kind, "computer_scroll_to_element");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_key_input_requires_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-key-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","key":"tab","modifiers":["shift"]}"#;
    let error = registry
        .enqueue_computer(
            "computer-key-control-only".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_key_input"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-key-capable".to_string(),
            agent_instance_id: "computer-key-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                computer_control: true,
                computer_key_input: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-key-capable".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-key-capable".to_string(),
            agent_instance_id: "computer-key-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer key-input request");
    assert_eq!(request.kind, "computer_key_input");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_window_activation_requires_its_own_additive_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-activate",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_window_activate"));

    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "computer-activate".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_accessibility_observe: true,
                computer_control: true,
                computer_window_activate: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-activate".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer window activation request");
    assert_eq!(request.kind, "computer_activate_window");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_text_input_enqueue_requires_exact_owner_and_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    let bob = auth_context(Some("bob"), false);
    let text = "隐私输入🙂";
    let payload = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": text,
    })
    .to_string();

    register_computer_test_client(
        &registry,
        "computer-input",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let error = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_text_input"));

    register_computer_test_client(&registry, "computer-input", "alice", true, true, true, true)
        .await;
    let error = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "bob".to_string(),
            Some(&bob),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("owned by alice"));

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-input".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer text input request");
    assert_eq!(request.kind, "computer_input_text");
    assert_eq!(request.stdin.as_deref(), Some(payload.as_str()));
    assert!(request.command.is_empty());
    let preview = super::jobs::request_preview(&request);
    assert!(preview.is_empty());
    assert!(!preview.contains(text));
}

#[tokio::test]
async fn computer_text_input_enqueue_preserves_max_utf8_text_after_json_escaping() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-input-escaped",
        "alice",
        true,
        true,
        true,
        true,
    )
    .await;

    let text = "\u{1}".repeat(2048);
    let payload = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": text,
    })
    .to_string();
    assert!(payload.len() > crate::shell_protocol::SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES);
    assert!(payload.len() <= crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES);

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-input-escaped".to_string(),
            "computer_input_text",
            payload,
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-input-escaped".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued escaped computer text input request");
    let decoded: serde_json::Value =
        serde_json::from_str(request.stdin.as_deref().expect("computer input payload")).unwrap();
    assert_eq!(decoded["text"].as_str(), Some(text.as_str()));

    let oversized =
        "x".repeat(crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES + 1);
    let error = registry
        .enqueue_computer(
            "computer-input-escaped".to_string(),
            "computer_input_text",
            oversized,
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("payload is invalid or too large"));
}

#[path = "mod_tests/lsp.rs"]
mod lsp;

async fn register_quic_v1_client(registry: &ShellClientRegistry, client_id: &str) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: Some(vec![project_summary("webcodex", "/tmp/webcodex")]),
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_transport(client_id, TRANSPORT_QUIC)
        .await
        .unwrap();
}

#[tokio::test]
async fn raw_shell_run_wait_timeout_preserves_known_dispatch_evidence() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let client_id = "raw-shell-timeout";
    let registry = Arc::new(ShellClientRegistry::default());
    let mut registration = runner_registration(
        client_id,
        "inst",
        vec![project_summary("webcodex", "/tmp/webcodex")],
    );
    registration.capabilities = Some(ShellClientCapabilities {
        shell: true,
        ..Default::default()
    });
    registry.register(registration).await.unwrap();

    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/run").post(shell_run)),
    );
    let response = TestClient::post("http://localhost/api/shell/run")
        .json(&json!({
            "client_id": client_id,
            "cwd": null,
            "command": "echo hi",
            "stdin": null,
            "timeout_secs": 5,
            "wait_timeout_secs": 1
        }))
        .send(&service);
    let poll = async {
        for _ in 0..200 {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: client_id.to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                return request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("raw shell request was not dispatched");
    };
    let (mut response, request) = tokio::join!(response, poll);
    assert_eq!(request.kind, "run_shell");
    assert_eq!(response.status_code, Some(StatusCode::REQUEST_TIMEOUT));
    let body = response
        .take_json::<serde_json::Value>()
        .await
        .expect("raw shell timeout JSON");
    assert_eq!(body["request_dispatched"], true);
    assert!(
        body.get("command_execution_state").is_none(),
        "the server must not fabricate Runner lifecycle evidence: {body}"
    );
}

#[tokio::test]
async fn registry_allows_quic_v1_run_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-run").await;

    let (_request_id, _rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "quic-run".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let view = registry.get_client_view("quic-run").await.unwrap();
    assert_eq!(view.transport, TRANSPORT_QUIC);
    assert_eq!(view.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
    assert_eq!(view.pending_requests, 1);
    assert!(view.capabilities.shell);
    assert!(view.capabilities.async_shell_jobs);
}

#[tokio::test]
async fn enqueue_file_op_allows_read_with_line_range() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;

    let mut req = file_request("read");
    req.start_line = Some(7);
    req.end_line = Some(12);
    let (request_id, _rx) = registry
        .enqueue_file_op(req, "tester".to_string())
        .await
        .unwrap();

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_read");
    assert_eq!(polled.path.as_deref(), Some("src/auth/scopes.rs"));
    assert_eq!(polled.start_line, Some(7));
    assert_eq!(polled.end_line, Some(12));
}

#[tokio::test]
async fn registry_allows_quic_v1_file_and_project_ops_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-ops").await;

    let (_file_request_id, _file_rx) = registry
        .enqueue_file_op(
            ShellFileOpRequest {
                op: "read".to_string(),
                client_id: "quic-ops".to_string(),
                path: "README.md".to_string(),
                cwd: None,
                content: None,
                max_bytes: None,
                old_text: None,
                pattern: None,
                expected_sha256: None,
                expected_prefix: None,
                start_line: None,
                end_line: None,
                line: None,
                create_dirs: false,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let (_project_request_id, _project_rx) = registry
        .enqueue_project_op(
            "quic-ops".to_string(),
            "register_project",
            "{}".to_string(),
            "tester".to_string(),
        )
        .await
        .unwrap();

    let view = registry.get_client_view("quic-ops").await.unwrap();
    assert_eq!(view.pending_requests, 2);
}

// ---------------------------------------------------------------------------
// Structured delete enqueue: the authoritative `structured_file_delete`
// capability fence. The capability check and pending-request admission must
// happen under the same registry lock, so a client that re-registered without
// the capability never receives an unknown `file_delete_project_files` op and
// a failed admission leaves no request or waiter behind.
// ---------------------------------------------------------------------------

async fn register_structured_delete_client(
    registry: &ShellClientRegistry,
    client_id: &str,
    structured_file_delete: bool,
) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                structured_file_delete,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

async fn register_instance_with_capabilities(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    capabilities: ShellClientCapabilities,
) -> Result<ShellClientView, String> {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(capabilities),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
}

async fn register_structured_delete_instance(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    structured_file_delete: bool,
) -> Result<ShellClientView, String> {
    register_instance_with_capabilities(
        registry,
        client_id,
        instance,
        ShellClientCapabilities {
            structured_file_delete,
            ..Default::default()
        },
    )
    .await
}

fn structured_delete_request(client_id: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: "delete_project_files".to_string(),
        client_id: client_id.to_string(),
        path: ".".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(r#"{"paths":["tmp.txt"]}"#.to_string()),
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 30,
    }
}

async fn assert_structured_delete_client_idle(registry: &ShellClientRegistry, client_id: &str) {
    let inner = registry.inner.lock().await;
    assert!(inner
        .queues_by_client
        .get(client_id)
        .is_none_or(|queue| queue.is_empty()));
    assert!(inner
        .pending_by_id
        .values()
        .all(|pending| pending.request.client_id != client_id));
}

#[tokio::test]
async fn enqueue_internal_posix_script_is_typed_and_capability_fenced() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-on",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let script = "while [ 0 -lt 1 ]; do break; done\n";
    let (request_id, _rx) = registry
        .enqueue_internal_posix_script(
            "internal-posix-on".to_string(),
            Some("/tmp/proj".to_string()),
            script.to_string(),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .expect("capable client should accept internal POSIX work");

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "internal-posix-on".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "run_internal_posix_script");
    assert!(polled.command.is_empty());
    assert!(polled.stdin.is_none());
    let payload = polled.script.expect("typed internal script payload");
    assert_eq!(
        payload.language,
        crate::shell_protocol::ShellScriptLanguage::Sh
    );
    assert_eq!(payload.script, script);
    assert!(payload.args.is_empty());
}

#[tokio::test]
async fn enqueue_internal_posix_script_missing_capability_fails_closed() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-off",
        "inst",
        ShellClientCapabilities {
            shell: true,
            structured_script_payload: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = registry
        .enqueue_internal_posix_script(
            "internal-posix-off".to_string(),
            Some("/tmp/proj".to_string()),
            "printf ok\n".to_string(),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.starts_with("capability_unavailable:"), "{error}");
    assert!(error.contains("internal_posix_script"), "{error}");
    assert_structured_delete_client_idle(&registry, "internal-posix-off").await;
}

#[tokio::test]
async fn enqueue_internal_posix_script_preserves_generated_command_wire_bound() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-bound",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = registry
        .enqueue_internal_posix_script(
            "internal-posix-bound".to_string(),
            Some("/tmp/proj".to_string()),
            "x".repeat(crate::shell_protocol::RAW_SHELL_WIRE_MAX_BYTES + 1),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("Runner wire envelope"), "{error}");
    assert_structured_delete_client_idle(&registry, "internal-posix-bound").await;
}

#[tokio::test]
async fn same_instance_internal_posix_capability_downgrade_is_rejected() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-monotonic",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = register_instance_with_capabilities(
        &registry,
        "internal-posix-monotonic",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: false,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(
        error.contains("cannot downgrade internal_posix_script"),
        "{error}"
    );
    let view = registry
        .get_client_view("internal-posix-monotonic")
        .await
        .unwrap();
    assert!(view.capabilities.internal_posix_script);
}

#[tokio::test]
async fn enqueue_structured_file_delete_queues_when_capability_advertised() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_client(&registry, "structured-delete-on", true).await;
    let (request_id, _rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-on"),
            "tester".to_string(),
        )
        .await
        .expect("capable client should accept the structured delete request");

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "structured-delete-on".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_delete_project_files");
    assert!(polled.command.is_empty());
    assert_eq!(polled.path.as_deref(), Some("."));
    assert_eq!(polled.content.as_deref(), Some(r#"{"paths":["tmp.txt"]}"#));
}

#[tokio::test]
async fn enqueue_structured_file_delete_capability_false_queues_nothing() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_client(&registry, "structured-delete-off", false).await;
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-off"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        error.starts_with("capability_unavailable:"),
        "error must be distinguishable for the legacy fallback: {error}"
    );
    assert!(
        error.contains("structured_file_delete"),
        "error was: {error}"
    );
    assert_structured_delete_client_idle(&registry, "structured-delete-off").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_missing_capability_defaults_false() {
    let registry = ShellClientRegistry::default();
    // The client advertises related capabilities (file_write, shell) but not
    // structured_file_delete; the capability must default to false and must
    // never be inferred from anything else.
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "structured-delete-missing".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                file_write: true,
                shell: true,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-missing"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        error.starts_with("capability_unavailable:"),
        "missing capability must fail closed: {error}"
    );
    assert_structured_delete_client_idle(&registry, "structured-delete-missing").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_rechecks_capability_atomically_after_revoke() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "structured-delete-flip", "inst-a", true)
        .await
        .unwrap();
    assert!(registry
        .client_supports(
            "structured-delete-flip",
            crate::shell_protocol::SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
        )
        .await
        .unwrap());

    // A same-instance downgrade is rejected by the monotonic capability rule,
    // so the only way the current registration loses the capability is a
    // replacement: the capable instance goes stale and a different instance
    // without structured_file_delete takes over the lease.
    registry
        .set_last_seen_for_test(
            "structured-delete-flip",
            chrono::Utc::now().timestamp() - 120,
        )
        .await;
    register_structured_delete_instance(&registry, "structured-delete-flip", "inst-b", false)
        .await
        .unwrap();
    assert!(!registry
        .client_supports(
            "structured-delete-flip",
            crate::shell_protocol::SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
        )
        .await
        .unwrap());

    // The authoritative enqueue must re-check under the registry lock and
    // queue nothing for the replacement Runner.
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-flip"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        error.starts_with("capability_unavailable:"),
        "revoked capability must fail closed: {error}"
    );
    assert_structured_delete_client_idle(&registry, "structured-delete-flip").await;
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "structured-delete-flip".to_string(),
            agent_instance_id: "inst-b".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        polled.is_none(),
        "the replacement Runner must receive no file_delete_project_files request: {polled:?}"
    );
}

#[tokio::test]
async fn enqueue_structured_file_delete_unknown_client_fails_closed() {
    let registry = ShellClientRegistry::default();
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-ghost"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert_eq!(error, "unknown shell client: structured-delete-ghost");
    assert_structured_delete_client_idle(&registry, "structured-delete-ghost").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_offline_client_fails_closed() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_client(&registry, "structured-delete-offline", true).await;
    registry
        .set_last_seen_for_test(
            "structured-delete-offline",
            chrono::Utc::now().timestamp() - 120,
        )
        .await;
    let error = registry
        .enqueue_structured_file_delete(
            structured_delete_request("structured-delete-offline"),
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("offline"), "error was: {error}");
    assert_structured_delete_client_idle(&registry, "structured-delete-offline").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_rejects_other_ops_before_registry() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_client(&registry, "structured-delete-op", true).await;
    let mut req = structured_delete_request("structured-delete-op");
    req.op = "write".to_string();
    let error = registry
        .enqueue_structured_file_delete(req, "tester".to_string())
        .await
        .unwrap_err();
    assert!(
        error.contains("op=delete_project_files"),
        "error was: {error}"
    );
    assert_structured_delete_client_idle(&registry, "structured-delete-op").await;
}

#[tokio::test]
async fn enqueue_structured_file_delete_validates_request_before_locking() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_client(&registry, "structured-delete-invalid", true).await;
    let mut req = structured_delete_request("structured-delete-invalid");
    req.path = "".to_string();
    let error = registry
        .enqueue_structured_file_delete(req, "tester".to_string())
        .await
        .unwrap_err();
    assert_eq!(error, "path cannot be empty");
    assert_structured_delete_client_idle(&registry, "structured-delete-invalid").await;
}

// ---------------------------------------------------------------------------
// Structured delete across runner replacement: same-instance capability is
// process-lifetime (monotonic) and a different-instance replacement never
// inherits synchronous requests admitted for the replaced process.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn generic_file_enqueue_rejects_internal_artifact_export_chunk() {
    let registry = ShellClientRegistry::default();
    let error = registry
        .enqueue_file_op(
            ShellFileOpRequest {
                op: "read_project_artifact_export_chunk".to_string(),
                client_id: "internal-only-probe".to_string(),
                path: "paper/report.pdf".to_string(),
                cwd: Some("/tmp/proj".to_string()),
                content: Some(
                    r#"{"path":"paper/report.pdf","expected_file_bytes":1,"offset":0,"length":1}"#
                        .to_string(),
                ),
                max_bytes: None,
                old_text: None,
                pattern: None,
                expected_sha256: None,
                expected_prefix: None,
                start_line: None,
                end_line: None,
                line: None,
                create_dirs: false,
                wait_timeout_secs: 30,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("internal-only"), "error was: {error}");
}

#[tokio::test]
async fn same_instance_artifact_export_chunk_downgrade_registration_rejected() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "monotonic-export-chunk",
        "inst-a",
        ShellClientCapabilities {
            file_read: true,
            artifact_export_chunk_read: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = register_instance_with_capabilities(
        &registry,
        "monotonic-export-chunk",
        "inst-a",
        ShellClientCapabilities {
            file_read: true,
            artifact_export_chunk_read: false,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(
        error.contains("cannot downgrade artifact_export_chunk_read"),
        "error was: {error}"
    );
    let view = registry
        .get_client_view("monotonic-export-chunk")
        .await
        .unwrap();
    assert!(view.capabilities.artifact_export_chunk_read);
}

#[tokio::test]
async fn same_instance_structured_file_delete_downgrade_registration_rejected() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "monotonic-delete", "inst-a", true)
        .await
        .unwrap();
    let error = register_structured_delete_instance(&registry, "monotonic-delete", "inst-a", false)
        .await
        .unwrap_err();
    assert!(
        error.contains("cannot downgrade structured_file_delete"),
        "error was: {error}"
    );

    // The rejected downgrade leaves the original capable registration
    // authoritative and intact.
    let view = registry.get_client_view("monotonic-delete").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
    assert!(view.capabilities.structured_file_delete);
    assert!(registry
        .client_supports(
            "monotonic-delete",
            crate::shell_protocol::SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
        )
        .await
        .unwrap());

    // A queued structured delete is still dispatchable to the capable lease.
    let (request_id, _rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("monotonic-delete"),
            "tester".to_string(),
        )
        .await
        .expect("capable lease must remain authoritative after rejected downgrade");
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "monotonic-delete".to_string(),
            agent_instance_id: "inst-a".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_delete_project_files");
}

#[tokio::test]
async fn same_instance_structured_file_delete_same_capability_reconnect_allowed() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "monotonic-reconnect", "inst-a", true)
        .await
        .unwrap();
    register_structured_delete_instance(&registry, "monotonic-reconnect", "inst-a", true)
        .await
        .unwrap();
    let view = registry
        .get_client_view("monotonic-reconnect")
        .await
        .unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
    assert!(view.capabilities.structured_file_delete);
}

#[tokio::test]
async fn same_instance_structured_file_delete_upgrade_allowed() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "monotonic-upgrade", "inst-a", false)
        .await
        .unwrap();
    register_structured_delete_instance(&registry, "monotonic-upgrade", "inst-a", true)
        .await
        .unwrap();
    let view = registry.get_client_view("monotonic-upgrade").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
    assert!(view.capabilities.structured_file_delete);
}

#[tokio::test]
async fn same_instance_reconnect_keeps_queued_structured_delete_dispatchable() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "reconnect-keeps", "inst-a", true)
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("reconnect-keeps"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    // Same-instance transport reconnect with the capability still true: the
    // queued structured request remains valid and dispatchable to that
    // instance (never failed or re-enqueued).
    register_structured_delete_instance(&registry, "reconnect-keeps", "inst-a", true)
        .await
        .unwrap();
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "reconnect-keeps".to_string(),
            agent_instance_id: "inst-a".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "file_delete_project_files");
}

#[tokio::test]
async fn instance_replacement_drains_sync_requests_before_installing_new_lease() {
    let registry = ShellClientRegistry::default();
    register_structured_delete_instance(&registry, "replace-drain", "inst-a", true)
        .await
        .unwrap();
    let (request_id, mut rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("replace-drain"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    // Age out instance A so a different instance may take over the lease.
    registry
        .set_last_seen_for_test("replace-drain", chrono::Utc::now().timestamp() - 120)
        .await;
    // Replacement instance B does not support structured delete.
    register_structured_delete_instance(&registry, "replace-drain", "inst-b", false)
        .await
        .unwrap();

    // The old synchronous waiter resolves safely with request_dispatched=false
    // (the request was never polled by the replaced instance).
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut rx)
        .await
        .expect("replaced instance waiter must resolve promptly")
        .expect("replaced instance waiter must not be dropped");
    assert!(!response.success);
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.request_dispatched, Some(false));
    assert!(
        response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("replaced"),
        "error was: {:?}",
        response.error
    );

    // The replacement Runner polls no inherited file_delete_project_files.
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "replace-drain".to_string(),
            agent_instance_id: "inst-b".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        polled.is_none(),
        "replacement must not inherit the structured delete request: {polled:?}"
    );

    // No pending request or queue leak remains for the client.
    assert_structured_delete_client_idle(&registry, "replace-drain").await;
}

#[tokio::test]
async fn instance_replacement_keeps_job_reconciliation_contract_unchanged() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "replace-job-sync",
        "inst-a",
        ShellClientCapabilities {
            jobs: true,
            async_jobs: true,
            async_shell_jobs: true,
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("replace-job-sync".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let (request_id, mut rx) = registry
        .enqueue_structured_file_delete(
            structured_delete_request("replace-job-sync"),
            "tester".to_string(),
        )
        .await
        .expect("capable instance should accept the structured delete request");

    registry
        .set_last_seen_for_test("replace-job-sync", chrono::Utc::now().timestamp() - 120)
        .await;
    register_instance_with_capabilities(
        &registry,
        "replace-job-sync",
        "inst-b",
        ShellClientCapabilities::default(),
    )
    .await
    .unwrap();

    // The synchronous structured delete is drained with its waiter resolved...
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut rx)
        .await
        .expect("waiter must resolve promptly")
        .expect("waiter must not be dropped");
    assert!(!response.success);
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.request_dispatched, Some(false));
    // ...while the Job keeps its existing replacement reconciliation contract:
    // terminated to `lost` with `runner_instance_replaced`, never drained as a
    // synchronous request.
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert_eq!(
        lost.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );
    // The replacement polls no inherited structured delete and nothing leaks.
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "replace-job-sync".to_string(),
            agent_instance_id: "inst-b".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(polled.is_none());
    assert_structured_delete_client_idle(&registry, "replace-job-sync").await;
}

#[tokio::test]
async fn registry_allows_quic_v1_start_job_queueing() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "quic-job").await;

    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("quic-job".to_string()),
                cwd: None,
                command: Some("sleep 1".to_string()),
                timeout_secs: Some(5),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    let view = registry.get_client_view("quic-job").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(job.status, "queued");
    assert_eq!(registry.list_jobs(Some(10)).await.len(), 1);
}

#[tokio::test]
async fn registry_allows_quic_v1_stop_job_delivery_queueing() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "quic-stop".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("quic-stop".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let _ = registry
        .poll(ShellAgentPollRequest {
            client_id: "quic-stop".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    registry
        .set_transport("quic-stop", TRANSPORT_QUIC)
        .await
        .unwrap();

    let stopped = registry
        .stop_job(&job.job_id, "tester".to_string())
        .await
        .unwrap();
    let view = registry.get_client_view("quic-stop").await.unwrap();
    assert_eq!(view.pending_requests, 1);
    assert_eq!(stopped.status, "stop_requested");
}

#[test]
fn validate_run_request_uses_the_internal_raw_shell_wire_bound() {
    let exact = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "x".repeat(crate::shell_protocol::RAW_SHELL_WIRE_MAX_BYTES),
        stdin: None,
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    validate_run_request(&exact).expect("wire-bound command accepted");

    let mut oversized = exact;
    oversized.command.push('x');
    let error = validate_run_request(&oversized).unwrap_err();
    assert!(error.contains("Runner wire envelope"), "{error}");
}

#[test]
fn validate_run_request_allows_bounded_stdin_beyond_command_limit() {
    let body = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "cat >/dev/null".to_string(),
        stdin: Some("x".repeat(crate::shell_protocol::RAW_SHELL_COMMAND_MAX_BYTES + 1024)),
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    validate_run_request(&body).expect("stdin has its own larger bound");
}

#[test]
fn validate_run_request_rejects_oversized_stdin() {
    let body = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "cat >/dev/null".to_string(),
        stdin: Some("x".repeat(MAX_RUN_STDIN_BYTES + 1)),
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    let err = validate_run_request(&body).unwrap_err();
    assert!(err.contains("stdin is too large"), "got: {}", err);
}

#[tokio::test]
async fn terminal_observed_legacy_poll_complete_and_log() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: Some("/tmp".to_string()),
                command: Some("printf hello".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: Some(ShellJobCodexMetadata {
                    project: Some("demo".to_string()),
                    goal_id: Some("goal-1".to_string()),
                    client_request_id: Some("crid-1".to_string()),
                    command: Some("printf hello".to_string()),
                    kind: Some("command".to_string()),
                    suite: None,
                    script_path: None,
                    reason: Some("test job".to_string()),
                    max_runtime_secs: Some(10),
                }),
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(job.status, "queued");
    assert_eq!(
        job.codex
            .as_ref()
            .and_then(|codex| codex.client_request_id.as_deref()),
        Some("crid-1")
    );
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.command, "printf hello");
    let running = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(running.status, "agent_queued");
    registry
        .complete(ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: polled.request_id,
            exit_code: Some(0),
            stdout: Some("hello\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(20),
            error: None,
        })
        .await
        .unwrap();
    let done = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.exit_code, Some(0));
    {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        assert!(record.terminal_observed_at.is_some());
        assert_eq!(record.terminal_observed_at, record.ended_at);
    }
    assert_eq!(
        done.codex
            .as_ref()
            .and_then(|codex| codex.project.as_deref()),
        Some("demo")
    );
    let listed = registry.list_jobs(Some(10)).await;
    assert_eq!(
        listed
            .iter()
            .find(|listed| listed.job_id == job.job_id)
            .and_then(|listed| listed.codex.as_ref())
            .and_then(|codex| codex.goal_id.as_deref()),
        Some("goal-1")
    );
    let (_info, stdout, stderr, next_stdout, next_stderr) = registry
        .job_log(&job.job_id, Some(1), Some(1), None)
        .await
        .unwrap();
    assert_eq!(stdout.as_deref(), Some("hello\n"));
    assert_eq!(stderr.as_deref(), Some(""));
    assert_eq!(next_stdout, 2);
    assert_eq!(next_stderr, 1);
}

#[tokio::test]
async fn terminal_observed_queued_stop_records_server_time() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let stopped = registry
        .stop_job(&job.job_id, "test".to_string())
        .await
        .unwrap();
    assert_eq!(stopped.status, "stopped");
    {
        let inner = registry.inner.lock().await;
        let record = inner.jobs_by_id.get(&job.job_id).unwrap();
        assert!(record.terminal_observed_at.is_some());
        assert_eq!(record.terminal_observed_at, record.ended_at);
    }
    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(polled.is_none());
}

#[tokio::test]
async fn registry_shell_job_stop_running_delivers_stop_to_client() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let started = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.kind, "start_job");

    let stop_requested = registry
        .stop_job(&job.job_id, "test".to_string())
        .await
        .unwrap();
    assert_eq!(stop_requested.status, "stop_requested");
    let stop = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stop.kind, "stop_job");
    assert_eq!(stop.job_id.as_deref(), Some(job.job_id.as_str()));
}

#[tokio::test]
async fn registry_marks_running_job_lost_when_client_stale() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let _ = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        let client = inner.clients.get_mut("oe").unwrap();
        client.last_seen = now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1;
    }
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert!(lost.error.unwrap().contains("stale"));
}

#[tokio::test]
async fn touch_client_refreshes_stale_client_back_to_online() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();

    // Age the client past the online window so it reads as stale.
    registry
        .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    let stale = registry.get_client_view("oe").await.unwrap();
    assert!(!stale.connected);
    assert_eq!(stale.status, "stale");

    // A keepalive touch must bring it back online.
    registry.touch_client("oe", "inst").await.unwrap();
    let fresh = registry.get_client_view("oe").await.unwrap();
    assert!(fresh.connected);
    assert_eq!(fresh.status, "online");

    // Unknown client_id is a clear error and does not mutate state.
    assert!(registry.touch_client("nope", "inst").await.is_err());
}

#[tokio::test]
async fn touch_client_refreshes_websocket_transport_client() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "ws-1".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_transport("ws-1", TRANSPORT_WEBSOCKET)
        .await
        .unwrap();

    registry
        .set_last_seen_for_test("ws-1", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    let stale = registry.get_client_view("ws-1").await.unwrap();
    assert_eq!(stale.transport, "websocket");
    assert!(!stale.connected);

    registry.touch_client("ws-1", "inst").await.unwrap();
    let fresh = registry.get_client_view("ws-1").await.unwrap();
    assert_eq!(fresh.transport, "websocket");
    assert!(fresh.connected);
    assert_eq!(fresh.status, "online");
}

#[tokio::test]
async fn touch_client_rejects_stale_instance_and_accepts_active() {
    // Regression: a stale/replaced instance must not refresh the active
    // lease's `last_seen` via Ping/Pong keepalive.
    let registry = ShellClientRegistry::default();
    // Instance A registers and is online.
    let view_a = register_with_instance(&registry, "oe", "inst-a").await;
    assert!(view_a.connected);

    // Age A out so a newer instance may take over the lease.
    registry
        .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    // Instance B replaces A.
    let view_b = register_with_instance(&registry, "oe", "inst-b").await;
    assert_eq!(view_b.agent_instance_id, "inst-b");
    assert!(view_b.connected);

    // Capture B's last_seen right after registration.
    let before = registry.get_client_view("oe").await.unwrap().last_seen;
    // Sleep a moment so a successful touch would observably advance
    // last_seen.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Stale instance A's keepalive must be rejected and must NOT advance
    // last_seen for B.
    let err = registry.touch_client("oe", "inst-a").await.unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "error was: {err}"
    );
    let after_a = registry.get_client_view("oe").await.unwrap().last_seen;
    assert_eq!(
        after_a, before,
        "stale instance touch must not refresh active last_seen"
    );
    // A stale instance must not resurrect the client to online either.
    let view_after_a = registry.get_client_view("oe").await.unwrap();
    assert!(view_after_a.connected);

    // Active instance B's keepalive succeeds and refreshes last_seen.
    registry.touch_client("oe", "inst-b").await.unwrap();
    let after_b = registry.get_client_view("oe").await.unwrap().last_seen;
    assert!(
        after_b > before,
        "active instance touch must refresh last_seen"
    );
    assert!(registry.get_client_view("oe").await.unwrap().connected);

    // An empty agent_instance_id is rejected by validation.
    assert!(registry.touch_client("oe", "").await.is_err());
}

#[test]
fn enforce_register_owner_cases() {
    let bootstrap = auth_context(None, true);
    let user_alice = auth_context(Some("alice"), false);
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    let agent_alice = agent_auth_context(
        "alice",
        "alice-laptop",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    let agent_alice_register_only =
        agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);

    // (case, auth, client_id, owner, Ok or Err(required error fragments)).
    let cases = vec![
        // No AuthMiddleware (unit tests): defer to the middleware, which in
        // production rejects anonymous requests before the handler runs.
        (
            "no auth skips with owner",
            None,
            "client-1",
            Some("anyone"),
            Ok(()),
        ),
        (
            "no auth skips without owner",
            None,
            "client-1",
            None,
            Ok(()),
        ),
        // Bootstrap may register any owner.
        (
            "bootstrap allows missing owner",
            Some(&bootstrap),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "bootstrap allows any owner",
            Some(&bootstrap),
            "client-1",
            Some("bob"),
            Ok(()),
        ),
        (
            "shared key ignores missing owner",
            Some(&shared),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "shared key ignores untrusted owner",
            Some(&shared),
            "client-1",
            Some("forged-owner"),
            Ok(()),
        ),
        // Phase 3: user tokens (Phase 2 personal API tokens) are no longer
        // allowed on agent transport endpoints. Only bootstrap or agent
        // tokens may register.
        (
            "user token is rejected",
            Some(&user_alice),
            "client-1",
            Some("alice"),
            Err(vec!["user tokens are not allowed"]),
        ),
        // Matching client_id + matching owner -> Ok.
        (
            "agent token matching client_id and owner",
            Some(&agent_alice),
            "alice-laptop",
            Some("alice"),
            Ok(()),
        ),
        // Matching client_id + missing owner -> Ok (owner filled in by the
        // caller via effective_register_owner).
        (
            "agent token matching client_id, missing owner",
            Some(&agent_alice),
            "alice-laptop",
            None,
            Ok(()),
        ),
        (
            "agent token wrong client_id rejected",
            Some(&agent_alice_register_only),
            "other-laptop",
            None,
            Err(vec!["not bound to client_id"]),
        ),
        (
            "agent token owner mismatch rejected",
            Some(&agent_alice_register_only),
            "alice-laptop",
            Some("bob"),
            Err(vec!["agent token owner is 'alice'", "bob"]),
        ),
    ];

    for (case, auth, client_id, owner, expected) in cases {
        let result = enforce_register_owner(auth, client_id, owner);
        match expected {
            Ok(()) => assert!(result.is_ok(), "case '{case}': got: {result:?}"),
            Err(fragments) => {
                let err = result.expect_err(&format!("case '{case}': expected an error"));
                for fragment in fragments {
                    assert!(
                        err.contains(fragment),
                        "case '{case}': missing '{fragment}' in error: {err}"
                    );
                }
            }
        }
    }
}

#[test]
fn effective_register_owner_agent_token_fills_username() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
    // Missing owner -> filled with the token's username.
    assert_eq!(
        effective_register_owner(Some(&alice), None),
        Some("alice".to_string())
    );
    // Matching owner preserved.
    assert_eq!(
        effective_register_owner(Some(&alice), Some("alice")),
        Some("alice".to_string())
    );
    // Bootstrap keeps the request owner.
    let bootstrap = auth_context(None, true);
    assert_eq!(
        effective_register_owner(Some(&bootstrap), Some("bob")),
        Some("bob".to_string())
    );
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert_eq!(
        effective_register_owner(Some(&shared), Some("forged-owner")),
        None,
        "shared-key owner must not become an authorization input"
    );
}

#[test]
fn enforce_agent_transport_rejects_user_token() {
    let alice = auth_context(Some("alice"), false);
    let err = enforce_agent_transport(Some(&alice), "client-1").unwrap_err();
    assert!(err.contains("user tokens are not allowed"), "got: {}", err);
}

#[test]
fn enforce_agent_transport_agent_token_matching_client_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(enforce_agent_transport(Some(&alice), "alice-laptop").is_ok());
    let err = enforce_agent_transport(Some(&alice), "other").unwrap_err();
    assert!(err.contains("not bound"), "got: {}", err);
}

#[test]
fn enforce_agent_transport_bootstrap_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(enforce_agent_transport(Some(&bootstrap), "any-client").is_ok());
}

#[test]
fn enforce_agent_transport_direct_shared_key_succeeds() {
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert!(enforce_agent_transport(Some(&shared), "any-client").is_ok());
    for scope in crate::auth::AGENT_SCOPES {
        assert!(require_agent_transport_scope(Some(&shared), scope).is_ok());
    }
}

#[test]
fn enforce_agent_transport_open_anonymous_is_rejected() {
    let open = open_auth_context();
    assert!(enforce_agent_transport(Some(&open), "client-a").is_err());
    assert!(require_agent_transport_scope(Some(&open), "agent:register").is_err());
}

#[test]
fn require_agent_transport_scope_agent_token_with_scope_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(require_agent_transport_scope(Some(&alice), "agent:poll").is_ok());
    assert!(require_agent_transport_scope(Some(&alice), "agent:register").is_err());
}

#[test]
fn require_agent_transport_scope_bootstrap_always_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(require_agent_transport_scope(Some(&bootstrap), "agent:register").is_ok());
}

#[test]
fn require_agent_transport_scope_user_token_rejected() {
    let alice = auth_context(Some("alice"), false);
    let err = require_agent_transport_scope(Some(&alice), "agent:register").unwrap_err();
    assert!(err.contains("missing required scope"), "got: {}", err);
}

#[test]
fn oauth_bridge_token_remains_blocked_from_agent_transport() {
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
    assert!(enforce_agent_transport(Some(&bridge), "client-a")
        .unwrap_err()
        .contains("user tokens are not allowed"));
    assert!(
        require_agent_transport_scope(Some(&bridge), "agent:register")
            .unwrap_err()
            .contains("missing required scope")
    );
}

#[tokio::test]
async fn registry_rejects_enqueue_when_queue_full() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "full".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    // Fill the queue to the limit without any consumer draining it.
    for _ in 0..MAX_QUEUED_REQUESTS_PER_CLIENT {
        registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "full".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
    }
    // The next enqueue must be rejected with a structured error instead
    // of growing the queue unboundedly.
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "full".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("too many pending requests"));
    assert!(err.contains("full"));
    // The queue is exactly at the cap; memory is bounded.
    let view = registry.get_client_view("full").await.unwrap();
    assert_eq!(view.pending_requests, MAX_QUEUED_REQUESTS_PER_CLIENT);
}

#[tokio::test]
async fn registry_rejects_enqueue_when_client_offline() {
    // Registered-but-stale agents must fail fast at enqueue rather than
    // accepting work that can only time out (or fill the 256-deep queue).
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "stale".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("stale", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;

    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "stale".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("offline"),
        "enqueue against a stale agent must fail fast as offline: {err}"
    );
    let view = registry.get_client_view("stale").await.unwrap();
    assert_eq!(view.pending_requests, 0);
    assert!(!view.connected);
}

#[tokio::test]
async fn reconcile_disconnect_marks_running_jobs_lost() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    // Job is "queued" with its request sitting in the client's queue.
    let before = registry.get_client_view("oe").await.unwrap();
    assert_eq!(before.pending_requests, 1);
    // Transport disconnects (e.g. WebSocket dropped).
    registry.reconcile_disconnect("oe", "inst").await;
    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert!(lost.error.unwrap().contains("disconnected"));
    // Pending request was dropped: no dangling waiter / queue entry.
    let after = registry.get_client_view("oe").await.unwrap();
    assert_eq!(after.pending_requests, 0);
}

#[tokio::test]
async fn reconcile_disconnect_fails_pending_sync_requests_fast() {
    // Regression guard for the MCP "no reply" hang: a synchronous tool
    // request (run_shell/read_file/... with job_id: None) whose agent drops
    // mid-flight must be resolved immediately, not parked until the caller's
    // wait timeout.
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let (_request_id, rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "oe".to_string(),
                cwd: Some("/tmp".to_string()),
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 30,
                wait_timeout_secs: 30,
            },
            "test".to_string(),
        )
        .await
        .unwrap();
    let before = registry.get_client_view("oe").await.unwrap();
    assert_eq!(before.pending_requests, 1);

    // Agent transport drops before returning a result.
    registry.reconcile_disconnect("oe", "inst").await;

    // Waiter resolves promptly with a disconnect error rather than parking
    // for the full 30s wait timeout. The short timeout turns a regression
    // (unbounded park) into a fast test failure instead of a hang.
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .expect("waiter must resolve promptly, not park until the caller timeout")
        .expect("waiter must be resolved, not dropped");
    assert!(!response.success);
    let error = response.error.expect("disconnect must set an error");
    assert!(
        error.contains("offline"),
        "error should classify as agent_offline: {error}"
    );
    assert!(
        !error.to_ascii_lowercase().contains("command"),
        "generic sync disconnect errors must remain request-neutral: {error}"
    );
    assert_eq!(response.request_dispatched, Some(false));
    assert_eq!(response.command_execution_state, None);
    // No dangling waiter or queue entry remains.
    let after = registry.get_client_view("oe").await.unwrap();
    assert_eq!(after.pending_requests, 0);
}

#[tokio::test]
async fn abandoned_sync_cleanup_removes_only_closed_waiters() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;
    let (abandoned_id, abandoned_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    let (live_id, live_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    drop(abandoned_rx);

    assert_eq!(registry.cancel_abandoned_sync_requests().await, 1);
    assert_eq!(
        registry
            .get_client_view("oe")
            .await
            .unwrap()
            .pending_requests,
        1
    );
    assert!(
        !registry.cancel_request(&abandoned_id).await,
        "closed-waiter request should already be removed"
    );
    assert_eq!(
        registry.cancel_request_dispatch_state(&live_id).await,
        Some(false),
        "cleanup must preserve an undispatched synchronous request with a live receiver"
    );
    drop(live_rx);
}

#[tokio::test]
async fn dispatched_file_request_disconnect_remains_request_neutral() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;
    let (_request_id, rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("file request should be dispatched");

    registry.reconcile_disconnect("oe", "inst").await;

    let response = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .expect("waiter must resolve promptly")
        .expect("waiter must receive a response");
    let error = response.error.as_deref().unwrap_or_default();
    assert!(
        !error.to_ascii_lowercase().contains("command"),
        "generic sync disconnect errors must not invent command lifecycle prose: {error}"
    );
    assert_eq!(response.request_dispatched, Some(true));
    assert_eq!(response.command_execution_state, None);
}

#[tokio::test]
async fn reconcile_disconnect_releases_active_lease_immediately() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;

    registry.reconcile_disconnect("oe", "inst-a").await;

    let offline = registry.get_client_view("oe").await.unwrap();
    assert!(
        !offline.connected,
        "active disconnect must immediately leave online window"
    );
    assert!(now_ts().saturating_sub(offline.last_seen) > CLIENT_ONLINE_WINDOW_SECS);

    let new_view = register_with_instance(&registry, "oe", "inst-b").await;
    assert_eq!(new_view.agent_instance_id, "inst-b");
    assert!(
        new_view.connected,
        "new instance should register without waiting 60 seconds"
    );
}

// ------------------------------------------------------------------------
// Agent instance identity / lease model (Phase 1)
// ------------------------------------------------------------------------

/// Helper: register a client with an explicit `agent_instance_id`.
async fn register_with_instance(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
) -> ShellClientView {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap()
}

/// Helper: register a long-lived-transport (WebSocket/QUIC) client bound to
/// a server-internal `connection_id`. Mirrors what `agent_ws`/`agent_quic`
/// do at register time. Returns the view along with the connection_id so a
/// test can drive the connection-scoped poll/touch/result/update APIs.
async fn register_with_connection(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    connection_id: &str,
) -> ShellClientView {
    registry
        .register_with_auth_connection(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                client_id: client_id.to_string(),
                agent_instance_id: instance.to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                host_context: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            None,
            Some(connection_id),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn lease_first_register_accepts_instance() {
    let registry = ShellClientRegistry::default();
    let view = register_with_instance(&registry, "oe", "inst-a").await;
    assert_eq!(view.agent_instance_id, "inst-a");
    assert!(view.connected);
    // The view/list path exposes the instance id.
    let clients = registry.list_clients().await;
    assert_eq!(clients[0].agent_instance_id, "inst-a");
}

#[tokio::test]
async fn lease_same_instance_reregister_accepts() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Same client_id + same instance id is a reconnect/refresh: accepted.
    let _ = register_with_instance(&registry, "oe", "inst-a").await;
    let view = registry.get_client_view("oe").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
    assert!(view.connected);
}

#[tokio::test]
async fn lease_different_online_instance_rejected() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // A second process with the same client_id but a different instance
    // must be rejected while the first is online.
    let err = registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst-b".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap_err();
    assert!(err.contains("already online"), "error was: {err}");
    assert!(err.contains("different instance"), "error was: {err}");
    // The active instance is unchanged.
    let view = registry.get_client_view("oe").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
}

#[tokio::test]
async fn lease_stale_replaced_by_different_instance_accepts() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Age the first instance past the online window so it reads as stale.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    // A different instance may now take over the lease.
    let _ = register_with_instance(&registry, "oe", "inst-b").await;
    let view = registry.get_client_view("oe").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-b");
    assert!(view.connected);
}

#[tokio::test]
async fn lease_stale_instance_poll_rejected() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Replace with a newer instance after aging out.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-b").await;

    // The stale instance A can no longer poll.
    let err = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-a".to_string(),
            projects: None,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "error was: {err}"
    );

    // The active instance B can still poll.
    registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-b".to_string(),
            projects: None,
        })
        .await
        .expect("active instance must poll");
}

#[tokio::test]
async fn lease_stale_instance_result_rejected() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Enqueue a synchronous request and let instance A poll it (dispatched).
    let (request_id, mut rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "oe".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let _ = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-a".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();

    // Replace instance A with B after aging out. The dispatched synchronous
    // request is owned by the replaced Runner process, so it is failed and
    // drained at replacement with `request_dispatched` preserved.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-b").await;

    // The stale instance A cannot submit the result.
    let err = registry
        .complete(ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-a".to_string(),
            request_id: request_id.clone(),
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "error was: {err}"
    );

    // The waiter resolves with the replacement error and preserves the truth
    // that the request had already been dispatched.
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut rx)
        .await
        .expect("replaced instance waiter must resolve promptly")
        .expect("replaced instance waiter must not be dropped");
    assert!(!response.success);
    assert_eq!(response.request_dispatched, Some(true));

    // The active instance B cannot submit the old instance's result: the
    // request record was drained with the replaced process, so it is no
    // longer present for the new lease.
    let err = registry
        .complete(ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-b".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("unknown or expired shell request"),
        "replacement must not inherit the replaced request: {err}"
    );
}

#[tokio::test]
async fn lease_stale_instance_job_update_rejected() {
    // A new `agent_instance_id` replacing the old instance terminates the
    // old instance's active/recovering jobs to `lost` with
    // `runner_instance_replaced` immediately at registration. The old
    // instance's late update is rejected, the new instance cannot inherit
    // or update the old instance's job, and the terminal state never
    // revives.
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    // Replace instance A with B after aging out. The replacement must
    // terminate A's job to `lost` at registration time.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-b").await;

    let lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(lost.status, "lost");
    assert_eq!(
        lost.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );
    assert!(lost.ended_at.is_some(), "replaced job must record ended_at");
    assert_eq!(
        lost.recovery_state.as_deref(),
        Some("lost_after_reconcile"),
        "replaced job must record lost_after_reconcile"
    );

    // The stale instance A cannot update the job (lease check).
    let err = registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-a".to_string(),
            update_seq: None,
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "error was: {err}"
    );

    // The active instance B cannot inherit or update A's job: it belongs
    // to the replaced runner instance.
    let err = registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-b".to_string(),
            update_seq: None,
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("replaced runner instance"),
        "active instance must not inherit replaced job: {err}"
    );

    // The terminal state is stable: a second late update from A does not
    // revive the job or change the first `ended_at` / reason.
    let first_ended_at = lost.ended_at.unwrap();
    let _ = registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-a".to_string(),
            update_seq: None,
            job_id: job.job_id.clone(),
            request_id: None,
            status: "completed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(1),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: true,
        })
        .await;
    let still_lost = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(still_lost.status, "lost");
    assert_eq!(still_lost.ended_at, Some(first_ended_at));
    assert_eq!(
        still_lost.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );
}

#[tokio::test]
async fn lease_list_clients_exposes_instance_id() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    let clients = registry.list_clients().await;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].agent_instance_id, "inst-a");
    let view = registry.get_client_view("oe").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-a");
}

#[tokio::test]
async fn lease_reconcile_disconnect_stale_instance_is_noop() {
    // A delayed disconnect from a stale, replaced instance must not affect
    // the current active instance: it must not clear B's notifier, not mark
    // B's freshly-created job lost/recovering, and not change A's old job
    // which was already terminated to `lost` (`runner_instance_replaced`)
    // at replacement time. Only B's own disconnect reconciles B's job.
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Install a notifier for instance A.
    let notify_a = Arc::new(Notify::new());
    registry
        .register_notifier("oe", "inst-a", notify_a.clone())
        .await
        .unwrap();
    // Start a job under instance A. It is terminated to `lost` when B
    // replaces A, before any disconnect runs.
    let old_job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    // Age out A and let B take over. The replacement terminates A's job.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-b").await;
    // B installs its own notifier.
    let notify_b = Arc::new(Notify::new());
    registry
        .register_notifier("oe", "inst-b", notify_b.clone())
        .await
        .unwrap();

    // B starts a fresh job of its own after the replacement.
    let b_job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    // Snapshot A's old job terminal state before the stale disconnect.
    let old_lost = registry.get_job(&old_job.job_id).await.unwrap();
    assert_eq!(old_lost.status, "lost");
    assert_eq!(
        old_lost.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );
    let old_ended_at = old_lost.ended_at.unwrap();

    // A's transport finally disconnects. This must be a no-op: B stays the
    // current instance, B's notifier stays installed, B's job stays
    // active, and A's old job keeps its first `ended_at`/reason.
    registry.reconcile_disconnect("oe", "inst-a").await;

    let view = registry.get_client_view("oe").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-b");
    assert!(view.connected, "stale disconnect must not drop B's lease");

    // B's notifier remains installed (still addressable) and B's job is
    // untouched.
    let b_view = registry.get_job(&b_job.job_id).await.unwrap();
    assert_ne!(
        b_view.status, "lost",
        "stale disconnect must not mark B's active job lost"
    );
    assert_ne!(
        b_view.status, "recovering",
        "stale disconnect must not drive B's job into recovering"
    );

    let old_after = registry.get_job(&old_job.job_id).await.unwrap();
    assert_eq!(old_after.status, "lost");
    assert_eq!(old_after.ended_at, Some(old_ended_at));
    assert_eq!(
        old_after.recovery_reason_code.as_deref(),
        Some("runner_instance_replaced")
    );

    // B can still poll/update/complete its own job after A's stale
    // disconnect.
    let updated = registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-b".to_string(),
            update_seq: None,
            job_id: b_job.job_id.clone(),
            request_id: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .expect("B must still update its own job after A's stale disconnect");
    assert_eq!(updated.status, "running");

    // Only B's own disconnect reconciles B's job. A non-reconciliation
    // client's active job becomes `lost` (legacy_runner_disconnected).
    registry.reconcile_disconnect("oe", "inst-b").await;
    let b_final = registry.get_job(&b_job.job_id).await.unwrap();
    assert_eq!(b_final.status, "lost");
    assert_eq!(
        b_final.recovery_reason_code.as_deref(),
        Some("legacy_runner_disconnected")
    );
    // A's old job is unaffected by B's disconnect.
    let old_final = registry.get_job(&old_job.job_id).await.unwrap();
    assert_eq!(old_final.status, "lost");
    assert_eq!(old_final.ended_at, Some(old_ended_at));
}

#[tokio::test]
async fn lease_register_notifier_rejects_stale_instance() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-a").await;
    // Replace A with B.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-b").await;
    // A's late notifier registration must be rejected so it cannot
    // overwrite B's notifier.
    let err = registry
        .register_notifier("oe", "inst-a", Arc::new(Notify::new()))
        .await
        .unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "error was: {err}"
    );
    // B can still install its notifier.
    registry
        .register_notifier("oe", "inst-b", Arc::new(Notify::new()))
        .await
        .expect("active instance must install notifier");
}

#[tokio::test]
async fn lease_register_rejects_empty_instance_id() {
    let registry = ShellClientRegistry::default();
    let err = registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap_err();
    assert!(err.contains("agent_instance_id"), "error was: {err}");
}
#[tokio::test]
async fn project_active_job_query_is_not_truncated_and_unregister_fences_starts() {
    let registry = ShellClientRegistry::default();
    register_with_instance(&registry, "oe", "inst-jobs").await;
    let request = |command: &str| ShellJobOpRequest {
        op: "start".to_string(),
        client_id: Some("oe".to_string()),
        cwd: None,
        command: Some(command.to_string()),
        timeout_secs: Some(60),
        job_id: None,
        since_stdout_line: None,
        since_stderr_line: None,
        tail_lines: None,
        limit: None,
        codex: None,
    };
    let target = "agent:oe:target";
    let target_job = registry
        .start_job_with_metadata(
            request("sleep 60"),
            "tester".to_string(),
            ShellJobStartMetadata {
                project_id: Some(target.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        inner
            .jobs_by_id
            .get_mut(&target_job.job_id)
            .unwrap()
            .created_at = 0;
    }
    for index in 0..101 {
        registry
            .start_job_with_metadata(
                request(&format!("echo {index}")),
                "tester".to_string(),
                ShellJobStartMetadata {
                    project_id: Some(format!("agent:oe:other-{index}")),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    assert_eq!(registry.list_jobs(Some(100)).await.len(), 100);
    assert_eq!(
        registry.count_active_jobs_for_project(None, target).await,
        1
    );
    assert_eq!(
        registry
            .begin_project_unregister(None, target)
            .await
            .unwrap(),
        1
    );

    {
        let mut inner = registry.inner.lock().await;
        let job = inner.jobs_by_id.get_mut(&target_job.job_id).unwrap();
        job.status = "completed".to_string();
        job.ended_at = Some(now_ts());
    }
    assert_eq!(
        registry
            .begin_project_unregister(None, target)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        registry
            .begin_project_unregister(None, target)
            .await
            .unwrap(),
        0
    );
    registry.end_project_unregister(target).await;
    let blocked = registry
        .start_job_with_metadata(
            request("echo blocked"),
            "tester".to_string(),
            ShellJobStartMetadata {
                project_id: Some(target.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(blocked, "project_unregister_in_progress");
    registry.end_project_unregister(target).await;
    registry
        .start_job_with_metadata(
            request("echo allowed"),
            "tester".to_string(),
            ShellJobStartMetadata {
                project_id: Some(target.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

// ------------------------------------------------------------------------
// Connection-scoped lease: same-instance transport reconnect races.
// A replaced connection (same client_id + same agent_instance_id but a
// newer connection_id) must not let the older socket dequeue new
// requests, refresh liveness, or clobber the new connection's metadata.
// ------------------------------------------------------------------------

#[tokio::test]
async fn stale_connection_poll_cannot_steal_new_request() {
    // Same runner instance registers over connection A, a request is
    // queued, then the instance reconnects over connection B (new lease).
    // Connection A's connection-scoped poll must be rejected with a stale
    // connection error AND leave the request in the queue / undispatched /
    // job un-transitioned (atomic: not just a stale error string). B then
    // polls and is the only one to receive the request.
    let registry = ShellClientRegistry::default();
    register_with_connection(&registry, "oe", "inst-x", "conn-a").await;

    // Start an async job (queued -> agent_queued only on dispatch).
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 1".to_string()),
                timeout_secs: Some(1),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    // The job starts queued with one pending request in the queue.
    assert_eq!(
        registry.get_job(&job.job_id).await.unwrap().status,
        "queued"
    );

    // Same instance reconnects over connection B; B takes the lease.
    register_with_connection(&registry, "oe", "inst-x", "conn-b").await;

    // A's connection-scoped poll is rejected with the stable stale error.
    let err = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-a",
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("transport connection is no longer active"),
        "error was: {err}"
    );

    // Atomicity: the request must still be queued, undispatched, and the
    // job must still be queued (no queued -> agent_queued transition).
    let pending_depth = registry
        .get_client_view("oe")
        .await
        .unwrap()
        .pending_requests;
    assert_eq!(pending_depth, 1, "stale poll must not dequeue the request");
    {
        let inner = registry.inner.lock().await;
        let request_id = inner
            .jobs_by_id
            .get(&job.job_id)
            .and_then(|j| j.request_id.clone());
        let request_id = request_id.expect("job has a request_id");
        let pending = inner
            .pending_by_id
            .get(&request_id)
            .expect("request still pending");
        assert!(
            !pending.dispatched,
            "stale poll must not mark request dispatched"
        );
        assert_eq!(
            inner.jobs_by_id.get(&job.job_id).unwrap().status,
            "queued",
            "stale poll must not transition the job"
        );
    }

    // B's connection-scoped poll receives the request (exactly once).
    let polled_b = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-b",
        )
        .await
        .unwrap()
        .expect("current connection must receive the request");
    assert_eq!(polled_b.kind, "start_job");
    assert_eq!(
        registry.get_job(&job.job_id).await.unwrap().status,
        "agent_queued"
    );
    // The queue is now drained: a second poll by either connection gets None.
    let again_a = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-a",
        )
        .await;
    // A is still stale, so this is an error (not a None success).
    assert!(again_a.is_err());
}

#[tokio::test]
async fn stale_connection_keepalive_does_not_refresh_new_lease() {
    // After a same-instance reconnect, a delayed Ping/Pong from the old
    // connection must not refresh the new connection's last_seen or revive
    // a disconnected client. The current connection's keepalive does
    // refresh.
    let registry = ShellClientRegistry::default();
    register_with_connection(&registry, "oe", "inst-x", "conn-a").await;
    register_with_connection(&registry, "oe", "inst-x", "conn-b").await;

    // Pin the current client's last_seen to a known stale value so a
    // successful touch would observably advance it.
    let pinned = chrono::Utc::now().timestamp() - 90;
    registry.set_last_seen_for_test("oe", pinned).await;

    // A's connection-scoped touch fails and leaves last_seen unchanged.
    let err = registry
        .touch_client_for_connection("oe", "inst-x", "conn-a")
        .await
        .unwrap_err();
    assert!(
        err.contains("transport connection is no longer active"),
        "error was: {err}"
    );
    assert_eq!(
        registry.get_client_view("oe").await.unwrap().last_seen,
        pinned,
        "stale connection touch must not refresh last_seen"
    );

    // B's connection-scoped touch succeeds and advances last_seen.
    registry
        .touch_client_for_connection("oe", "inst-x", "conn-b")
        .await
        .unwrap();
    assert!(
        registry.get_client_view("oe").await.unwrap().last_seen > pinned,
        "current connection touch must refresh last_seen"
    );

    // An even newer connection C supersedes B; B's touch now fails too.
    register_with_connection(&registry, "oe", "inst-x", "conn-c").await;
    let err = registry
        .touch_client_for_connection("oe", "inst-x", "conn-b")
        .await
        .unwrap_err();
    assert!(
        err.contains("transport connection is no longer active"),
        "superseded connection touch must be rejected, error was: {err}"
    );
}

#[tokio::test]
async fn stale_connection_runtime_metadata_does_not_overwrite_current() {
    // A stale same-instance connection must not overwrite the current
    // connection's provider metadata. The current connection can.
    let registry = ShellClientRegistry::default();
    let register_with_policy = async |connection_id: &str| {
        registry
            .register_with_auth_connection(
                ShellClientRegisterRequest {
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    client_id: "oe".to_string(),
                    agent_instance_id: "inst-x".to_string(),
                    display_name: None,
                    owner: Some("alice".to_string()),
                    hostname: None,
                    host_context: None,
                    capabilities: Some(async_job_capabilities()),
                    projects: None,
                    agent_protocol_version: Some("polling-v1".to_string()),
                    policy: Some(AgentPolicySummary::default()),
                },
                None,
                Some(connection_id),
            )
            .await
            .unwrap()
    };
    register_with_policy("conn-a").await;
    register_with_policy("conn-b").await;

    let provider_status = |strategy: &str| ToolProvidersStatus {
        strategy: strategy.to_string(),
        claude_code: ClaudeCodeProviderStatus {
            enabled: true,
            version: None,
            available: true,
            process_state: "running".to_string(),
            discovered_tool_names: Vec::new(),
            capabilities: std::collections::BTreeMap::new(),
            last_error_code: None,
            last_call: None,
        },
        config_reload: Default::default(),
    };

    // Current connection B reports a provider status.
    registry
        .update_tool_providers_for_connection(
            "oe",
            "inst-x",
            "conn-b",
            Some(provider_status("claude_code")),
        )
        .await
        .unwrap();
    {
        let inner = registry.inner.lock().await;
        let client = inner.clients.get("oe").unwrap();
        assert_eq!(
            client
                .policy
                .as_ref()
                .unwrap()
                .tool_providers
                .as_ref()
                .unwrap()
                .strategy,
            "claude_code"
        );
    }

    // Stale connection A tries to overwrite with a different valid
    // strategy; it must be rejected and must not change the recorded
    // strategy.
    let err = registry
        .update_tool_providers_for_connection(
            "oe",
            "inst-x",
            "conn-a",
            Some(provider_status("native")),
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("transport connection is no longer active"),
        "{err}"
    );
    {
        let inner = registry.inner.lock().await;
        let client = inner.clients.get("oe").unwrap();
        assert_eq!(
            client
                .policy
                .as_ref()
                .unwrap()
                .tool_providers
                .as_ref()
                .unwrap()
                .strategy,
            "claude_code",
            "stale connection must not overwrite current metadata"
        );
    }
}

#[tokio::test]
async fn stale_connection_disconnect_cleanup_is_noop_for_current_lease() {
    // Same-instance reconnect: A's delayed disconnect cleanup must not
    // touch B's notifier/queue/liveness. Extends the existing same-instance
    // reconnect coverage to the connection lease.
    let registry = ShellClientRegistry::default();
    register_with_connection(&registry, "oe", "inst-x", "conn-a").await;
    let notify_a = Arc::new(Notify::new());
    registry
        .register_notifier_for_connection("oe", "inst-x", "conn-a", notify_a)
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    // B reconnects (same instance) and installs its own notifier.
    register_with_connection(&registry, "oe", "inst-x", "conn-b").await;
    let notify_b = Arc::new(Notify::new());
    registry
        .register_notifier_for_connection("oe", "inst-x", "conn-b", notify_b)
        .await
        .unwrap();

    // A's delayed disconnect cleanup is a no-op: B's job is not lost.
    registry
        .reconcile_disconnect_for_connection("oe", "inst-x", "conn-a")
        .await;
    assert_ne!(
        registry.get_job(&job.job_id).await.unwrap().status,
        "lost",
        "stale connection cleanup must not mark current job lost"
    );
    // B's notifier survives A's cleanup and B's own dispatch still works.
    let polled = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-b",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.kind, "start_job");

    // B's own disconnect does reconcile the job to lost.
    registry
        .reconcile_disconnect_for_connection("oe", "inst-x", "conn-b")
        .await;
    assert_eq!(registry.get_job(&job.job_id).await.unwrap().status, "lost");
}

#[tokio::test]
async fn late_result_on_stale_connection_is_accepted_without_refreshing_liveness() {
    // A request dispatched to A (same instance) before the reconnect must
    // still complete on a late result arriving over the stale connection
    // A — it belongs to the same instance — but must NOT refresh B's
    // liveness. A cannot then poll a new request that arrived after B's
    // register.
    let registry = ShellClientRegistry::default();
    register_with_connection(&registry, "oe", "inst-x", "conn-a").await;

    // Enqueue a sync request and let A poll it (still current lease).
    let (request_id, rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "oe".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let polled_a = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-a",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled_a.request_id, request_id);

    // Same instance reconnects over B; B is now the current lease.
    register_with_connection(&registry, "oe", "inst-x", "conn-b").await;
    // Pin B's last_seen to an online-but-observable value. A refresh by a
    // successful connection-scoped operation would advance it to `now`; the
    // stale connection must leave it at the pinned value. Staying inside the
    // 60s online window keeps the later enqueue path valid.
    let pinned = chrono::Utc::now().timestamp() - 30;
    registry.set_last_seen_for_test("oe", pinned).await;

    // The late result arrives over stale connection A. It is accepted
    // (same instance) and resolves the waiter.
    registry
        .complete_for_connection(
            ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            }
            .into(),
            "conn-a",
        )
        .await
        .unwrap();
    let response = rx.await.unwrap();
    assert!(response.success);
    // But it did NOT refresh B's liveness.
    assert_eq!(
        registry.get_client_view("oe").await.unwrap().last_seen,
        pinned,
        "late result on stale connection must not refresh new lease liveness"
    );

    // A cannot now poll a request enqueued after B's register. Enqueue a
    // new request under B's lease and verify A's poll is rejected.
    let (_new_request_id, _new_rx) = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "oe".to_string(),
                cwd: None,
                command: "echo two".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let err = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-a",
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("transport connection is no longer active"),
        "{err}"
    );

    // B receives the new request.
    let polled_b = registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-b",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled_b.command, "echo two");
}

#[tokio::test]
async fn late_job_update_on_stale_connection_is_accepted_without_refreshing_liveness() {
    // A job dispatched to A before the reconnect: its high-sequence job
    // update arriving over stale connection A is still applied (ownership
    // + update_seq), but does not refresh B's liveness. A replaced runner
    // instance is still rejected.
    let registry = ShellClientRegistry::default();
    register_with_connection(&registry, "oe", "inst-x", "conn-a").await;
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: None,
                command: Some("sleep 10".to_string()),
                timeout_secs: Some(10),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    // A polls/dispatches the job (still current lease).
    registry
        .poll_for_connection(
            ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                projects: None,
            },
            "conn-a",
        )
        .await
        .unwrap()
        .unwrap();

    // Same instance reconnects over B.
    register_with_connection(&registry, "oe", "inst-x", "conn-b").await;
    // Pin to an online-but-observable value: a refresh would advance it to
    // `now`, but the stale connection must leave it pinned. Staying online
    // also prevents `get_job`'s status refresh from marking the active job
    // lost while we inspect it.
    let pinned = chrono::Utc::now().timestamp() - 30;
    registry.set_last_seen_for_test("oe", pinned).await;

    // Late job update over stale connection A is accepted and applied.
    registry
        .update_job_for_connection(
            ShellAgentJobUpdateRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-x".to_string(),
                update_seq: None,
                job_id: job.job_id.clone(),
                request_id: None,
                status: "running".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                command_execution_state: None,
                validation_progress: None,
                finished: false,
            },
            "conn-a",
        )
        .await
        .unwrap();
    assert_eq!(
        registry.get_job(&job.job_id).await.unwrap().status,
        "running"
    );
    // But B's liveness was not refreshed.
    assert_eq!(
        registry.get_client_view("oe").await.unwrap().last_seen,
        pinned,
        "late job update on stale connection must not refresh new lease liveness"
    );

    // A replaced runner instance is still rejected outright (a brand new
    // instance cannot submit updates for the old instance's job). Age the
    // old instance out so the replacement can take the lease.
    registry
        .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
        .await;
    register_with_instance(&registry, "oe", "inst-y").await;
    let err = registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst-x".to_string(),
            update_seq: None,
            job_id: job.job_id.clone(),
            request_id: None,
            status: "completed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(1),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: true,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("no longer the active instance"),
        "replaced runner instance must be rejected, error was: {err}"
    );
}

#[path = "mod_tests/job_log_wait.rs"]
mod job_log_wait;
