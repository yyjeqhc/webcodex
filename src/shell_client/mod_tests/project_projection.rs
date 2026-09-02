use super::*;
use crate::shell_protocol::ShellProjectInventoryPage;

#[tokio::test]
async fn bounded_semantic_inventory_fails_closed_on_client_or_project_overflow() {
    let registry = ShellClientRegistry::default();
    for index in 0..2 {
        let client_id = format!("bounded-{index}");
        let instance_id = format!("bounded-instance-{index}");
        registry
            .register(runner_registration(&client_id, &instance_id, Vec::new()))
            .await
            .unwrap();
        crate::test_support::apply_project_inventory_snapshot(
            &registry,
            &client_id,
            &instance_id,
            vec![project_summary(
                &format!("project-{index}"),
                &format!("/tmp/project-{index}"),
            )],
        )
        .await;
    }
    let admin = auth_context(None, true);
    assert_eq!(
        registry
            .list_bounded_client_semantic_views_for_auth(Some(&admin), 2, 2)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(registry
        .list_bounded_client_semantic_views_for_auth(Some(&admin), 1, 2)
        .await
        .is_none());
    assert!(registry
        .list_bounded_client_semantic_views_for_auth(Some(&admin), 2, 1)
        .await
        .is_none());
    let locked_count = registry
        .with_bounded_client_semantic_views_for_auth_locked(Some(&admin), 2, 2, |views| {
            views.map(|views| views.len())
        })
        .await;
    assert_eq!(locked_count, Some(2));
    let locked_overflow = registry
        .with_bounded_client_semantic_views_for_auth_locked(Some(&admin), 1, 2, |views| views)
        .await;
    assert!(locked_overflow.is_none());
}

#[tokio::test]
async fn project_cardinality_does_not_reject_runner_liveness_or_dynamic_upsert() {
    let registry = ShellClientRegistry::default();
    let shared = crate::auth::shared_key::shared_key_context("project-scale-shared");
    let projects = (0..65)
        .map(|index| project_summary(&format!("project-{index}"), "/tmp/project"))
        .collect::<Vec<_>>();
    let view = registry
        .register_with_auth(
            runner_registration("project-scale", "project-scale-instance", Vec::new()),
            Some(&shared),
        )
        .await
        .expect("project cardinality must not reject Runner registration");
    assert!(view.connected);
    assert!(view.projects.is_empty());
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "project-scale",
        "project-scale-instance",
        projects,
    )
    .await;
    assert_eq!(
        registry
            .list_client_projects("project-scale")
            .await
            .unwrap()
            .len(),
        65
    );

    registry
        .upsert_client_project(
            "project-scale",
            project_summary("project-65", "/tmp/project-65"),
        )
        .await
        .expect("dynamic projection may cross the historical 64-project threshold");
    assert_eq!(
        registry
            .list_client_projects("project-scale")
            .await
            .unwrap()
            .len(),
        66
    );

    let duplicate_projects = vec![project_summary("duplicate", "/tmp/duplicate"); 64];
    let status = registry
        .apply_project_inventory_page(
            "project-scale",
            "project-scale-instance",
            ShellProjectInventoryPage {
                generation: "duplicate-refresh".to_string(),
                snapshot_sequence: u64::MAX,
                page_index: 0,
                total_reported: 65,
                complete: false,
                projects: duplicate_projects,
            },
        )
        .await
        .expect("malformed inventory refresh must not reject Runner liveness");
    assert_eq!(
        registry
            .list_client_projects("project-scale")
            .await
            .unwrap()
            .len(),
        66,
        "malformed inventory refresh must preserve the authoritative projection"
    );
    assert_eq!(status.sync_state, "degraded");
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("project_inventory_duplicate_project_id")
    );
}

#[tokio::test]
async fn registry_inventory_snapshot_saves_projects() {
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
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "oe",
        "inst",
        vec![project_summary("webcodex", "/root/git/webcodex")],
    )
    .await;
    let clients = registry.list_clients().await;
    assert_eq!(clients[0].projects.len(), 1);
    assert_eq!(clients[0].projects[0].id, "webcodex");

    let projects = registry.list_client_projects("oe").await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].path, "/root/git/webcodex");
}

#[tokio::test]
async fn registry_inventory_snapshot_updates_projects() {
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
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "oe",
        "inst",
        vec![project_summary("one", "/tmp/one")],
    )
    .await;
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "oe",
        "inst",
        vec![
            project_summary("one", "/tmp/one"),
            project_summary("two", "/tmp/two"),
        ],
    )
    .await;

    let projects = registry.list_client_projects("oe").await.unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].id, "one");
    assert_eq!(projects[1].id, "two");
}

#[tokio::test]
async fn registry_poll_without_projects_preserves_existing_projection() {
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
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "oe",
        "inst",
        vec![project_summary("one", "/tmp/one")],
    )
    .await;

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
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
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "alice-client".to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "bob-client".to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("bob".to_string()),
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                ShellClientCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "alice-client",
        "inst",
        vec![project_summary("alice-project", "/tmp/alice-project")],
    )
    .await;
    crate::test_support::apply_project_inventory_snapshot(
        &registry,
        "bob-client",
        "inst",
        vec![project_summary("bob-project", "/tmp/bob-project")],
    )
    .await;

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
    assert_eq!(mismatch.0, StatusCode::BAD_REQUEST);
    assert!(
        mismatch.1.contains("unknown shell client"),
        "{}",
        mismatch.1
    );
    assert!(!mismatch.1.contains("owned by"), "{}", mismatch.1);
}
