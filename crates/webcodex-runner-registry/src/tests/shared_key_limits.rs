use super::*;

#[tokio::test]
async fn shared_key_runner_limit_is_per_group_and_reconnects_do_not_consume_capacity() {
    let registry = RunnerRegistry::default();
    let shared_a = shared_key_access("shared-limit-a");
    let shared_b = shared_key_access("shared-limit-b");

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
        .get_runner_view("shared-a-over-limit")
        .await
        .is_none());
    assert_eq!(
        registry
            .get_runner_view_for_auth("shared-a-0", Some(&shared_a))
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
    let registry = RunnerRegistry::with_shared_key_limits_for_test(
        MAX_SHARED_KEY_RUNNERS_PER_GROUP,
        3,
        SHARED_KEY_OFFLINE_TTL_SECS,
    );
    let shared_a = shared_key_access("shared-global-a");
    let shared_b = shared_key_access("shared-global-b");
    let shared_c = shared_key_access("shared-global-c");

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
