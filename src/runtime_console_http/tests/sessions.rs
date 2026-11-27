use super::*;

#[tokio::test]
async fn runtime_console_preserves_browser_same_origin_and_json_errors() {
    let (_tmp, service) = hosted_service(test_runtime());
    let cross_origin = TestClient::post("http://localhost/api/runtime-console/projects")
        .bearer_auth("console-bootstrap-test")
        .add_header("host", "localhost", true)
        .add_header("origin", "http://attacker.example", true)
        .json(&serde_json::json!({}))
        .send(&service)
        .await;
    assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));

    let unsupported = TestClient::post("http://localhost/api/runtime-console/projects")
        .bearer_auth("console-bootstrap-test")
        .add_header("host", "localhost", true)
        .body("{}")
        .send(&service)
        .await;
    assert_eq!(
        unsupported.status_code,
        Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
    );
}

#[tokio::test]
async fn runtime_console_reuses_workflow_session_projection_and_sanitizer() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("group-a");
    let project_id = "agent:client-a:proj-a";
    register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
    let session = runtime
        .sessions
        .start_session(Some(project_id.to_string()), Some("observe".to_string()));
    runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: session.session_id.clone(),
            kind: SessionMessageKind::Progress,
            message: "working in /root/private/source.rs".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();

    let hosted_list = workflow_sessions_for_auth(&runtime, &auth, project_id, Some(20))
        .await
        .unwrap();
    let direct_list = runtime.workflow_sessions_console_list(project_id, Some(20));
    let mut hosted_list_value = serde_json::to_value(&hosted_list).unwrap();
    let mut direct_list_value = serde_json::to_value(&direct_list).unwrap();
    for value in [&mut hosted_list_value, &mut direct_list_value] {
        for session in value["sessions"].as_array_mut().unwrap() {
            let session = session.as_object_mut().unwrap();
            session.remove("running_jobs");
            session.remove("running_jobs_complete");
        }
    }
    assert_eq!(hosted_list_value, direct_list_value);
    assert_eq!(hosted_list.sessions[0].running_jobs, 0);
    assert!(hosted_list.sessions[0].running_jobs_complete);

    let hosted_detail =
        workflow_session_for_auth(&runtime, &auth, project_id, &session.session_id, Some(20))
            .await
            .unwrap();
    let direct_detail = runtime
        .workflow_session_console_detail(project_id, &session.session_id, Some(20))
        .unwrap();
    let mut hosted_detail_value = serde_json::to_value(&hosted_detail).unwrap();
    let mut direct_detail_value = serde_json::to_value(&direct_detail).unwrap();
    for value in [&mut hosted_detail_value, &mut direct_detail_value] {
        let detail = value.as_object_mut().unwrap();
        detail.remove("running_jobs");
        detail.remove("running_jobs_complete");
    }
    assert_eq!(hosted_detail_value, direct_detail_value);
    assert_eq!(hosted_detail.running_jobs, 0);
    assert!(hosted_detail.running_jobs_complete);
    let home = overview_for_auth(&runtime, &auth).await.unwrap();
    assert_eq!(home.recent_sessions.sessions.len(), 1);
    let serialized = format!(
        "{}{}",
        serde_json::to_string(&hosted_detail).unwrap(),
        serde_json::to_string(&home).unwrap()
    );
    assert!(!serialized.contains("/root/private/source.rs"));
    assert!(serialized.contains("/private/a"));
    assert!(serialized.contains("[private path]"));
}

#[tokio::test]
async fn legacy_registration_confirmation_requires_complete_inventory_and_durable_commit() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Arc::new(
        ToolRuntime::new(
            Arc::new(crate::RunnerRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        )
        .with_session_database(
            dir.path().join("sessions.sqlite3"),
            &dir.path().join("sessions.json"),
        )
        .unwrap(),
    );
    let auth = test_bootstrap_auth();
    let registered = project("repo", "/fixture");
    let project_id = "agent:fixture:repo";
    let session = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project_id.into()),
                None,
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
            )),
        )
        .unwrap();
    runtime.sessions.flush().await.unwrap();
    let identity = webcodex_core::runner_protocol::workflow_project_identity(
        registered.root_fingerprint.as_deref(),
        &registered.path,
        registered.registration_incarnation.as_deref(),
    )
    .unwrap();
    runtime
        .runner_registry
        .register(crate::test_support::runner_registration(
            "fixture",
            "process",
            RunnerCapabilities::default(),
        ))
        .await
        .unwrap();
    let (_tmp, service) = hosted_service(runtime.clone());
    let body = json!({"project":project_id,"session_id":session.session_id,"expected_project_identity":identity});
    let confirm = || {
        TestClient::post("http://localhost/api/runtime-console/workflow-session-reauthorize")
            .bearer_auth("console-bootstrap-test")
            .json(&body)
            .send(&service)
    };
    assert_eq!(confirm().await.status_code, Some(StatusCode::CONFLICT));
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "fixture",
        "process",
        vec![registered],
    )
    .await;
    runtime.sessions.set_persistence_failure_for_test(true);
    assert_eq!(
        confirm().await.status_code,
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(
        runtime
            .sessions
            .session_project_identity(&session.session_id),
        Some(None)
    );
    runtime.sessions.set_persistence_failure_for_test(false);
    // Observe the original record through the normal recovery barrier first.
    runtime.sessions.prepare_request().await.unwrap();
    assert_eq!(
        runtime
            .sessions
            .session_project_identity(&session.session_id),
        Some(Some(identity))
    );
    let result: Value = confirm().await.take_json().await.unwrap();
    assert_eq!(result, json!({"changed":false}));
}

#[tokio::test]
async fn legacy_registration_preview_excludes_delegated_owner_tokens() {
    let runtime = test_runtime();
    let delegated = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    let mut direct = delegated.clone();
    direct.kind = AuthKind::ApiToken;
    register_project(&runtime, "fixture", "repo", "/fixture", Some(&direct)).await;
    let project = "agent:fixture:repo";
    let session = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.into()),
                None,
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                crate::tool_runtime::workflow_session_authority_fingerprint(Some(&delegated))
                    .unwrap(),
            )),
        )
        .unwrap();
    assert!(
        session_reauthorization::preview(&runtime, &direct, project, &session.session_id)
            .await
            .is_some()
    );
    assert!(
        session_reauthorization::preview(&runtime, &delegated, project, &session.session_id)
            .await
            .is_none()
    );
}
