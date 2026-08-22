use super::*;

#[tokio::test]
async fn project_cardinality_does_not_reject_runner_liveness_or_dynamic_upsert() {
    let registry = ShellClientRegistry::default();
    let shared = crate::auth::shared_key::shared_key_context("project-scale-shared");
    let projects = (0..65)
        .map(|index| project_summary(&format!("project-{index}"), "/tmp/project"))
        .collect::<Vec<_>>();
    let view = registry
        .register_with_auth(
            runner_registration("project-scale", "project-scale-instance", projects),
            Some(&shared),
        )
        .await
        .expect("65 projects must not reject Runner registration");
    assert!(view.connected);
    assert_eq!(view.projects.len(), 65);

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

    let duplicate_projects = vec![project_summary("duplicate", "/tmp/duplicate"); 65];
    registry
        .poll(ShellAgentPollRequest {
            client_id: "project-scale".to_string(),
            agent_instance_id: "project-scale-instance".to_string(),
            projects: Some(duplicate_projects),
        })
        .await
        .expect("malformed inventory refresh must not reject Runner heartbeat");
    assert_eq!(
        registry
            .list_client_projects("project-scale")
            .await
            .unwrap()
            .len(),
        66,
        "malformed inventory refresh must preserve the authoritative projection"
    );
    let status = registry
        .project_inventory_status_for_test("project-scale")
        .await
        .unwrap();
    assert_eq!(status.sync_state, "degraded");
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("project_inventory_duplicate_project_id")
    );
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
