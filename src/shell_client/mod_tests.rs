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

#[path = "mod_tests/shared_key_isolation.rs"]
mod shared_key_isolation;

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

#[path = "mod_tests/polling.rs"]
mod polling;

#[path = "mod_tests/run_enqueue.rs"]
mod run_enqueue;

#[path = "mod_tests/internal_posix.rs"]
mod internal_posix;

#[path = "mod_tests/artifact_export.rs"]
mod artifact_export;

#[path = "mod_tests/instance_lease.rs"]
mod instance_lease;

#[path = "mod_tests/connection_lease.rs"]
mod connection_lease;

#[path = "mod_tests/structured_file_delete.rs"]
mod structured_file_delete;

#[path = "mod_tests/computer_observe.rs"]
mod computer_observe;

#[path = "mod_tests/computer_snapshot_artifact.rs"]
mod computer_snapshot_artifact;

#[path = "mod_tests/computer_accessibility.rs"]
mod computer_accessibility;

#[path = "mod_tests/computer_control.rs"]
mod computer_control;

#[path = "mod_tests/computer_text_input.rs"]
mod computer_text_input;

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

#[path = "mod_tests/job_lifecycle.rs"]
mod job_lifecycle;

#[path = "mod_tests/client_liveness.rs"]
mod client_liveness;

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

#[path = "mod_tests/disconnect_reconciliation.rs"]
mod disconnect_reconciliation;

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

#[path = "mod_tests/project_unregister.rs"]
mod project_unregister;

#[path = "mod_tests/job_log_wait.rs"]
mod job_log_wait;
