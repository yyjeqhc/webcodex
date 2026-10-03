use super::*;

#[test]
fn selector_uses_bounded_authoritative_client_id_without_parsing_project_id() {
    let projected = project_selector_row(&serde_json::json!({
        "id": "agent:not-the-device:project",
        "client_id": "device-real",
        "name": "Demo",
        "path": "C:\\Users\\demo\\worktree",
        "connected": true,
        "agent_status": "online"
    }))
    .unwrap();
    assert_eq!(projected.client_id, "device-real");
    assert_ne!(projected.client_id, "not-the-device");
    assert_eq!(projected.path.as_deref(), Some("C:\\Users\\demo\\worktree"));

    let invalid_path = project_selector_row(&serde_json::json!({
        "id": "agent:looks-valid:project",
        "client_id": "device-real",
        "path": "/private/bad\npath",
        "connected": true
    }))
    .unwrap();
    assert!(invalid_path.path.is_none());
    let overlong_path = format!("/{}", "x".repeat(MAX_PROJECT_PATH_BYTES));
    let invalid_path = project_selector_row(&serde_json::json!({
        "id": "agent:looks-valid:project",
        "client_id": "device-real",
        "path": overlong_path,
        "connected": true
    }))
    .unwrap();
    assert!(invalid_path.path.is_none());

    let overlong = "x".repeat(MAX_CLIENT_ID_CHARS + 1);
    assert!(project_selector_row(&serde_json::json!({
        "id": "agent:looks-valid:project",
        "client_id": overlong,
        "connected": true
    }))
    .is_none());
    assert!(project_selector_row(&serde_json::json!({
        "id": "agent:looks-valid:project",
        "client_id": "bad\nclient",
        "connected": true
    }))
    .is_none());
}

#[tokio::test]
async fn hosted_runtime_console_uses_ordinary_runtime_and_projects_are_safe() {
    let runtime = test_runtime();
    register_project(
        &runtime,
        "special",
        "webcodex",
        "/root/private/webcodex",
        None,
    )
    .await;
    let (_tmp, service) = hosted_service(runtime);
    let mut response = TestClient::post("http://localhost/api/runtime-console/projects")
        .json(&serde_json::json!({}))
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["projects"][0]["id"], "agent:special:webcodex");
    assert_eq!(body["projects"][0]["client_id"], "special");
    assert_eq!(body["projects"][0]["path"], "/root/private/webcodex");
    let selector = body["projects"][0].as_object().unwrap();
    assert!(selector.keys().all(|key| matches!(
        key.as_str(),
        "id" | "client_id" | "name" | "path" | "connected" | "agent_status"
    )));
    let serialized = serde_json::to_string(&body).unwrap();
    for private in [
        "private-host-special",
        "private-shell-profile",
        "private-hook",
        &format!("sha256:{}", "1".repeat(64)),
        "private description",
    ] {
        assert!(
            !serialized.contains(private),
            "leaked {private}: {serialized}"
        );
    }

    let mut filtered = TestClient::post("http://localhost/api/runtime-console/projects")
        .json(&serde_json::json!({
            "client_id": "special",
            "query": "webcodex",
            "limit": 100
        }))
        .send(&service)
        .await;
    assert_eq!(filtered.status_code, Some(StatusCode::OK));
    let filtered_body: Value = filtered.take_json().await.unwrap();
    assert_eq!(filtered_body["total"], 1);
    assert_eq!(filtered_body["truncated"], false);
    assert_eq!(filtered_body["projects"][0]["id"], "agent:special:webcodex");

    let invalid_query = TestClient::post("http://localhost/api/runtime-console/projects")
        .json(&serde_json::json!({"query": "   "}))
        .send(&service)
        .await;
    assert_eq!(invalid_query.status_code, Some(StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn product_routes_reject_unknown_effect_selectors_and_invisible_projects() {
    let (_tmp, service) = hosted_service(test_runtime());
    for route in ["extensions", "project-git"] {
        let invalid = TestClient::post(format!("http://localhost/api/runtime-console/{route}"))
            .json(&serde_json::json!({"project":"agent:missing:project","tool":"run_shell"}))
            .send(&service)
            .await;
        assert_eq!(invalid.status_code, Some(StatusCode::BAD_REQUEST));
        let hidden = TestClient::post(format!("http://localhost/api/runtime-console/{route}"))
            .json(&serde_json::json!({"project":"agent:missing:project"}))
            .send(&service)
            .await;
        assert_eq!(hidden.status_code, Some(StatusCode::NOT_FOUND));
    }
    let instruction = TestClient::post("http://localhost/api/runtime-console/instruction")
            .json(&serde_json::json!({"project":"agent:missing:project","source_scope":"runner","path":"/private/secret","fingerprint":"old"}))
            .send(&service).await;
    assert_eq!(instruction.status_code, Some(StatusCode::NOT_FOUND));
    let retarget = TestClient::post("http://localhost/api/runtime-console/plugin-reload")
            .json(&serde_json::json!({"project":"agent:missing:project","plugin":"provider","runner":"other-runner"}))
            .send(&service).await;
    assert_eq!(retarget.status_code, Some(StatusCode::BAD_REQUEST));
}

#[test]
fn runtime_console_project_projection_preserves_short_project_ref() {
    let row = project_selector_row(&serde_json::json!({
        "id": "agent:special:webcodex",
        "client_id": "special",
        "project_ref": "~p118",
        "name": "WebCodex",
        "path": "/root/git/webcodex",
        "registration_source": "auto_registered",
        "lineage": {
            "kind": "managed_worktree_source",
            "source_project_id": "webcodex-source",
            "base_sha": "0123456789abcdef0123456789abcdef01234567"
        },
        "connected": true,
        "agent_status": "online"
    }))
    .unwrap();
    assert_eq!(row.project_ref.as_deref(), Some("~p118"));
    assert_eq!(row.registration_source.as_deref(), Some("auto_registered"));
    assert!(matches!(
        row.lineage,
        Some(RuntimeConsoleProjectLineage::ManagedWorktreeSource {
            ref source_project_id,
            ref base_sha,
        }) if source_project_id == "webcodex-source"
            && base_sha == "0123456789abcdef0123456789abcdef01234567"
    ));
    assert!(serde_json::to_string(&row)
        .unwrap()
        .contains("\"project_ref\":\"~p118\""));
}

#[tokio::test]
async fn project_filters_apply_before_bounded_runtime_console_limit() {
    let runtime = test_runtime();
    let auth = test_bootstrap_auth();
    for index in 0..100 {
        register_project(
            &runtime,
            &format!("a-{index:03}"),
            "project",
            &format!("/private/a-{index:03}"),
            None,
        )
        .await;
    }
    register_project(
        &runtime,
        "special",
        "webcodex",
        "/root/private/webcodex",
        None,
    )
    .await;

    let global = projects_for_auth(&runtime, &auth, Some(100)).await.unwrap();
    assert_eq!(global.total, 101);
    assert_eq!(global.projects.len(), 100);
    assert!(global.truncated);
    assert!(!global
        .projects
        .iter()
        .any(|project| project.id == "agent:special:webcodex"));

    let full = projects_for_auth(&runtime, &auth, Some(MAX_PROJECT_LIMIT))
        .await
        .unwrap();
    assert_eq!(full.total, 101);
    assert_eq!(full.projects.len(), 101);
    assert!(!full.truncated);
    assert!(full
        .projects
        .iter()
        .any(|project| project.id == "agent:special:webcodex"));

    let by_runner = projects_for_filters_auth(&runtime, &auth, Some("special"), None, Some(100))
        .await
        .unwrap();
    assert_eq!(by_runner.total, 1);
    assert!(!by_runner.truncated);
    assert_eq!(by_runner.projects[0].id, "agent:special:webcodex");

    let by_query = projects_for_filters_auth(&runtime, &auth, None, Some("webcodex"), Some(100))
        .await
        .unwrap();
    assert_eq!(by_query.total, 1);
    assert!(!by_query.truncated);
    assert_eq!(by_query.projects[0].id, "agent:special:webcodex");

    let combined = projects_for_filters_auth(
        &runtime,
        &auth,
        Some("special"),
        Some("webcodex"),
        Some(100),
    )
    .await
    .unwrap();
    assert_eq!(combined.total, 1);
    assert_eq!(combined.projects[0].id, "agent:special:webcodex");
}

#[tokio::test]
async fn selector_and_session_access_follow_authoritative_project_visibility() {
    let runtime = test_runtime();
    let auth_a = crate::auth::shared_key_context("group-a");
    let auth_b = crate::auth::shared_key_context("group-b");
    register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
    register_project(&runtime, "client-b", "proj-b", "/private/b", Some(&auth_b)).await;

    let direct = runtime
        .dispatch_with_auth(
            ToolCall::ListProjects {
                include_git_summary: false,
                client_id: None,
                project: None,
                query: None,
                limit: None,
                summary_only: false,
            },
            Some(&auth_a),
        )
        .await;
    let projected = projects_for_auth(&runtime, &auth_a, Some(100))
        .await
        .unwrap();
    let direct_ids = direct.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value["id"].as_str())
        .collect::<Vec<_>>();
    let projected_ids = projected
        .projects
        .iter()
        .map(|project| project.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(projected_ids, direct_ids);
    assert_eq!(projected_ids, vec!["agent:client-a:proj-a"]);
    assert_eq!(projected.projects[0].client_id, "client-a");
    assert_eq!(projected.projects[0].path.as_deref(), Some("/private/a"));
    assert_eq!(
        projected.projects[0].client_id,
        direct.output["projects"][0]["client_id"].as_str().unwrap()
    );

    let foreign = runtime.sessions.start_session(
        Some("agent:client-b:proj-b".to_string()),
        Some("foreign".to_string()),
    );
    assert_eq!(
        workflow_session_for_auth(
            &runtime,
            &auth_a,
            "agent:client-b:proj-b",
            &foreign.session_id,
            Some(20),
        )
        .await
        .unwrap_err(),
        RuntimeConsoleError::NotFound
    );
    assert_eq!(
        workflow_session_for_auth(
            &runtime,
            &auth_a,
            "agent:client-a:proj-a",
            &foreign.session_id,
            Some(20),
        )
        .await
        .unwrap_err(),
        RuntimeConsoleError::NotFound
    );

    let local = runtime.sessions.start_session(
        Some("agent:client-a:proj-a".to_string()),
        Some("locatable".to_string()),
    );
    let located = workflow_session_locate_for_auth(&runtime, &auth_a, &local.session_id)
        .await
        .unwrap();
    assert_eq!(located.project_id, "agent:client-a:proj-a");
    assert_eq!(located.client_id, "client-a");
    assert_eq!(located.session.session_id, local.session_id);
    assert_eq!(located.session.title, "locatable");
    assert_eq!(
        workflow_session_locate_for_auth(&runtime, &auth_b, &local.session_id)
            .await
            .unwrap_err(),
        RuntimeConsoleError::NotFound
    );
    assert_eq!(
        workflow_session_locate_for_auth(&runtime, &auth_a, "wc_sess_invalid")
            .await
            .unwrap_err(),
        RuntimeConsoleError::Invalid
    );
}

#[tokio::test]
async fn project_read_routes_survive_without_runtime_read_but_runtime_views_fail_closed() {
    let runtime = test_runtime();
    let auth = scoped_oauth(&[SCOPE_PROJECT_READ]);
    register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;

    let project_view = projects_for_auth(&runtime, &auth, Some(20)).await.unwrap();
    assert_eq!(project_view.projects.len(), 1);
    assert_eq!(project_view.projects[0].id, "agent:client-a:proj-a");
    assert_eq!(project_view.projects[0].path.as_deref(), Some("/private/a"));
    let runtime_only = scoped_oauth(&[SCOPE_RUNTIME_READ]);
    let runtime_only_view = overview_for_auth(&runtime, &runtime_only).await.unwrap();
    assert!(!runtime_only_view.projects_available);
    assert!(runtime_only_view.projects.is_empty());
    assert!(!serde_json::to_string(&runtime_only_view)
        .unwrap()
        .contains("/private/a"));

    assert_eq!(
        overview_for_auth(&runtime, &auth).await.unwrap_err(),
        RuntimeConsoleError::Request {
            status: 403,
            message: "Runtime read access required",
        }
    );
    assert_eq!(
        runner_for_auth(&runtime, &auth, "client-a", Some(20))
            .await
            .unwrap_err(),
        RuntimeConsoleError::Request {
            status: 403,
            message: "Runtime read access required",
        }
    );
}

#[tokio::test]
async fn server_and_runner_overviews_stay_within_caller_authorization_and_safe_projection() {
    let runtime = test_runtime();
    let auth_a = crate::auth::shared_key_context("runtime-console-overview-a");
    let auth_b = crate::auth::shared_key_context("runtime-console-overview-b");
    register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
    register_project(&runtime, "client-b", "proj-b", "/private/b", Some(&auth_b)).await;

    let overview_view = overview_for_auth(&runtime, &auth_a).await.unwrap();
    assert_eq!(overview_view.runner_count, 1);
    assert_eq!(overview_view.visible_projects, 1);
    assert!(!overview_view.projects_truncated);
    assert_eq!(
        overview_view.effective_config,
        runtime.effective_config_status()
    );
    assert_eq!(
        overview_view
            .effective_config
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["auth", "mcp_host", "tool_request_trace_mode"]
    );
    assert!(overview_view.effective_config["auth"]
        .as_object()
        .unwrap()
        .values()
        .all(Value::is_boolean));

    let runner_view = runner_for_auth(&runtime, &auth_a, "client-a", Some(20))
        .await
        .unwrap();
    assert_eq!(runner_view.client_id, "client-a");
    assert_eq!(runner_view.visible_project_count, 1);
    assert_eq!(runner_view.projects.len(), 1);
    assert_eq!(runner_view.projects[0].id, "agent:client-a:proj-a");
    assert_eq!(runner_view.projects[0].path.as_deref(), Some("/private/a"));
    assert_eq!(
        runner_for_auth(&runtime, &auth_a, "client-b", Some(20))
            .await
            .unwrap_err(),
        RuntimeConsoleError::NotFound
    );

    let serialized = format!(
        "{}{}",
        serde_json::to_string(&overview_view).unwrap(),
        serde_json::to_string(&runner_view).unwrap()
    );
    assert!(serialized.contains("/private/a"));
    for private in [
        "/private/b",
        "private-host-client-a",
        "private-host-client-b",
        "private-shell-profile",
        "private-hook",
        "private description",
    ] {
        assert!(
            !serialized.contains(private),
            "leaked {private}: {serialized}"
        );
    }
    assert!(!serialized.contains("agent:client-b:proj-b"));
}

#[tokio::test]
async fn computer_session_availability_reaches_only_authorized_overview_and_runner_detail() {
    let runtime = test_runtime();
    let auth_a = crate::auth::shared_key_context("computer-availability-a");
    let auth_b = crate::auth::shared_key_context("computer-availability-b");
    for (client, availability, auth) in [
        ("available", Some(true), &auth_a),
        ("unavailable", Some(true), &auth_a),
        ("legacy", None, &auth_a),
        ("private", Some(true), &auth_b),
    ] {
        register_project_with_computer_availability(
            &runtime,
            client,
            "project",
            "/private/project",
            Some(auth),
            availability,
        )
        .await;
    }
    runtime
        .runner_registry
        .update_computer_session_availability("unavailable", "inst-unavailable", None, Some(false))
        .await
        .unwrap();

    let overview_view = overview_for_auth(&runtime, &auth_a).await.unwrap();
    let rows = serde_json::to_value(&overview_view).unwrap()["runners"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 3);
    let full = runtime
        .dispatch_with_auth(
            ToolCall::ListRunners {
                query: None,
                status: None,
                limit: None,
                client_id: None,
                client_ids: None,
                include_projects: Some(false),
                summary_only: false,
            },
            Some(&auth_a),
        )
        .await;
    assert!(full.success);
    let full_rows = full.output["runners"].as_array().unwrap();
    assert_eq!(full_rows.len(), 3);
    for (client, expected) in [
        ("available", Some(true)),
        ("unavailable", Some(false)),
        ("legacy", None),
    ] {
        let row = rows.iter().find(|row| row["client_id"] == client).unwrap();
        let full_row = full_rows
            .iter()
            .find(|row| row["client_id"] == client)
            .unwrap();
        assert_eq!(
            row.get("computer_session_availability")
                .and_then(Value::as_bool),
            expected,
        );
        assert_eq!(
            row.get("computer_session_availability").is_some(),
            expected.is_some()
        );
        assert_eq!(
            full_row
                .get("computer_session_availability")
                .and_then(Value::as_bool),
            expected,
        );
        assert_eq!(
            full_row.get("computer_session_availability").is_some(),
            expected.is_some(),
        );
        let detail = runner_for_auth(&runtime, &auth_a, client, Some(20))
            .await
            .unwrap();
        let detail = serde_json::to_value(detail).unwrap();
        assert_eq!(
            detail
                .get("computer_session_availability")
                .and_then(Value::as_bool),
            expected,
        );
        assert_eq!(
            detail.get("computer_session_availability").is_some(),
            expected.is_some(),
        );
    }
    assert_eq!(
        runner_for_auth(&runtime, &auth_a, "private", Some(20))
            .await
            .unwrap_err(),
        RuntimeConsoleError::NotFound,
    );
    assert!(rows.iter().all(|row| row["client_id"] != "private"));
    assert!(full_rows.iter().all(|row| row["client_id"] != "private"));
}
