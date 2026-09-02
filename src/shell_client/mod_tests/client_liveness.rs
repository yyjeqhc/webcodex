use super::*;

#[tokio::test]
async fn touch_client_refreshes_stale_client_back_to_online() {
    let registry = ShellClientRegistry::default();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: async_job_capabilities(),
            policy: None,
        }))
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
