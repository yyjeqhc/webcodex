use super::*;

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
