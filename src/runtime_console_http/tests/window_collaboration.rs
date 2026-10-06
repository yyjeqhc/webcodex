use super::*;
use crate::runtime_console_http::window_collaboration;

#[tokio::test]
async fn window_collaboration_app_pages_are_window_scoped_read_only_and_independent_of_session() {
    use crate::client_window::ClientWindow;
    use crate::tool_runtime::tool_call::WorkResultCollaborationRequest;
    let (_temp, db, runtime) = test_runtime_with_goal_db();
    let auth = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    let project = "agent:history-api:demo";
    register_project(&runtime, "history-api", "demo", "/history-api", Some(&auth)).await;
    let client_window = ClientWindow::for_test("history-api");
    record_window_event(&db, &auth, client_window.key(), Some(project), None, 5000);
    for i in 0..9 {
        let result = runtime
            .work_result_send_message_with_kind(
                project.into(),
                None,
                format!("saved-{i}"),
                format!("history-key-{i}"),
                "question",
                Some(&auth),
                Some(&client_window),
            )
            .await;
        assert!(result.success, "{:?}", result.error);
    }
    let page = runtime
        .work_result_collaboration_page(
            project.into(),
            None,
            WorkResultCollaborationRequest {
                limit: Some(3),
                before_message_id: None,
            },
            Some(&auth),
            Some(&client_window),
        )
        .await;
    assert!(page.success, "{:?}", page.error);
    let body = &page.output["work_result_collaboration"];
    assert_eq!(body["messages"].as_array().unwrap().len(), 3);
    assert_eq!(body["truncated"], true);
    assert!(body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["kind"] == "question" && row["first_projected_at_ms"].is_null()));
    let cursor = body["next_before"].as_str().unwrap().to_string();
    let older = runtime
        .work_result_collaboration_page(
            project.into(),
            None,
            WorkResultCollaborationRequest {
                limit: Some(3),
                before_message_id: Some(cursor.clone()),
            },
            Some(&auth),
            Some(&client_window),
        )
        .await;
    assert!(older.success);
    assert_eq!(
        older.output["work_result_collaboration"]["history_scope"],
        body["history_scope"]
    );
    for (limit, target) in [
        (Some(0), Some(&client_window)),
        (Some(101), Some(&client_window)),
        (Some(3), None),
    ] {
        let denied = runtime
            .work_result_collaboration_page(
                project.into(),
                None,
                WorkResultCollaborationRequest {
                    limit,
                    before_message_id: None,
                },
                Some(&auth),
                target,
            )
            .await;
        assert!(!denied.success);
    }
    let other_window = ClientWindow::for_test("other-history");
    let foreign = runtime
        .work_result_collaboration_page(
            project.into(),
            None,
            WorkResultCollaborationRequest {
                limit: Some(3),
                before_message_id: Some(cursor),
            },
            Some(&auth),
            Some(&other_window),
        )
        .await;
    assert!(!foreign.success, "a cursor cannot retarget another Window");
    let denied = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
    let result = runtime
        .work_result_collaboration_page(
            project.into(),
            None,
            WorkResultCollaborationRequest::default(),
            Some(&denied),
            Some(&client_window),
        )
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn window_collaboration_survives_operator_token_rotation_for_visible_project() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut window_auth = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    window_auth.api_key_id = Some("oauth-window-collaboration-token-a".to_string());
    let mut operator_auth = window_auth.clone();
    operator_auth.api_key_id = Some("oauth-window-collaboration-token-b".to_string());
    let recipient = crate::tool_runtime::runtime_observation_principal(Some(&window_auth)).unwrap();
    assert_ne!(
        recipient,
        crate::tool_runtime::runtime_observation_principal(Some(&operator_auth)).unwrap()
    );

    let project = "agent:window-collaboration:shared-project";
    register_project(
        &runtime,
        "window-collaboration",
        "shared-project",
        "/private/window-collaboration",
        Some(&window_auth),
    )
    .await;
    let key = "c".repeat(64);
    record_window_event(&db, &window_auth, &key, Some(project), None, 5_000);

    let routed = window_collaboration::authorize(&runtime, &operator_auth, &key)
        .await
        .unwrap();
    assert_eq!(routed, recipient);
    let sent = runtime
        .post_window_operator_message_with_options_for_principal(
            &key,
            None,
            None,
            "question",
            "high",
            true,
            "Can you confirm the current state?".to_string(),
            "rotated-operator-delivery".to_string(),
            &routed.0,
            &routed.1,
            Some(&operator_auth),
        )
        .await;
    assert!(sent.success, "{:?}", sent.error);

    let model_view = runtime.window_collaboration(Some(&key), Some(&window_auth), 10);
    assert_eq!(model_view["messages"].as_array().unwrap().len(), 1);
    assert_eq!(
        model_view["messages"][0]["message"],
        "Can you confirm the current state?"
    );
    assert_eq!(model_view["messages"][0]["kind"], "question");
    assert_eq!(model_view["messages"][0]["priority"], "high");
    let console_view = runtime.window_collaboration_for_principal(&key, &routed.0, &routed.1, 10);
    assert_eq!(console_view["messages"], model_view["messages"]);
}

#[tokio::test]
async fn window_collaboration_does_not_fall_back_to_older_visible_history() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut operator = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    operator.api_key_id = Some("operator-token".to_string());
    let visible_project = "agent:collab-visible:demo";
    register_project(
        &runtime,
        "collab-visible",
        "demo",
        "/private/collab-visible",
        Some(&operator),
    )
    .await;

    let mut foreign = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    foreign.user_id = Some("foreign-collaboration-user".to_string());
    foreign.username = Some("foreign-collaboration-user".to_string());
    foreign.api_key_id = Some("foreign-window-token".to_string());
    let foreign_project = "agent:collab-foreign:demo";
    register_project(
        &runtime,
        "collab-foreign",
        "demo",
        "/private/collab-foreign",
        Some(&foreign),
    )
    .await;

    let key = "d".repeat(64);
    record_window_event(&db, &operator, &key, Some(visible_project), None, 5_000);
    record_window_event(&db, &foreign, &key, Some(foreign_project), None, 6_000);

    let inventory = windows_for_auth(&runtime, &operator, Some(10), None)
        .await
        .unwrap();
    assert_eq!(
        inventory.total, 1,
        "older visible history still keeps the Window discoverable"
    );
    assert!(
        window_collaboration::authorize(&runtime, &operator, &key)
            .await
            .is_err(),
        "collaboration must fail closed when the latest recipient is not visible"
    );
}

#[tokio::test]
async fn window_collaboration_unscoped_target_remains_exact_principal_bound() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let writer = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    let mut other = writer.clone();
    other.api_key_id = Some("another-token".into());
    let key = "a".repeat(64);
    record_window_event(&db, &writer, &key, None, None, 5000);
    assert!(window_collaboration::authorize(&runtime, &writer, &key)
        .await
        .is_ok());
    assert!(window_collaboration::authorize(&runtime, &other, &key)
        .await
        .is_err());
    assert!(
        window_collaboration::authorize(&runtime, &writer, &"b".repeat(64))
            .await
            .is_err()
    );
    assert!(
        window_collaboration::authorize(&runtime, &writer, "bad-window")
            .await
            .is_err()
    );
    let sent = runtime
        .post_window_operator_message(
            &key,
            None,
            None,
            "hello".into(),
            "console-1".into(),
            Some(&writer),
        )
        .await;
    assert!(sent.success, "{:?}", sent.error);
    let transcript = runtime.window_collaboration(Some(&key), Some(&writer), 10);
    assert_eq!(
        transcript["messages"][0]["message_id"],
        sent.output["message_id"]
    );
    assert!(transcript["messages"][0]["first_projected_at_ms"].is_null());
    assert!(
        runtime.window_collaboration(Some(&key), Some(&other), 10)["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let invalid = runtime
        .post_window_operator_message(
            &key,
            Some("wc_sess_invalid"),
            None,
            "hello".into(),
            "console-2".into(),
            Some(&writer),
        )
        .await;
    assert!(!invalid.success);
    assert_eq!(
        runtime.window_collaboration(Some(&key), Some(&writer), 10)["messages"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let project = "agent:context-owner:demo";
    register_project(
        &runtime,
        "context-owner",
        "demo",
        "/context-project",
        Some(&writer),
    )
    .await;
    let session = runtime
        .sessions
        .start_session(Some(project.into()), Some("context".into()));
    let unlinked = runtime
        .post_window_operator_message(
            &key,
            Some(&session.session_id),
            None,
            "context".into(),
            "context-key".into(),
            Some(&writer),
        )
        .await;
    assert!(!unlinked.success);
    record_window_event(
        &db,
        &writer,
        &key,
        Some(project),
        Some((&session.session_id, project)),
        6000,
    );
    let webui_style = runtime
        .post_window_operator_message(
            &key,
            Some(&session.session_id),
            None,
            "context from WebUI".into(),
            "context-key-webui".into(),
            Some(&writer),
        )
        .await;
    assert!(webui_style.success, "{:?}", webui_style.error);
    let mcp_style = runtime
        .post_window_operator_message(
            &key,
            Some(&session.session_id),
            Some(project),
            "context from MCP".into(),
            "context-key-mcp".into(),
            Some(&writer),
        )
        .await;
    assert!(mcp_style.success, "{:?}", mcp_style.error);
    let mismatch = runtime
        .post_window_operator_message(
            &key,
            Some(&session.session_id),
            Some("agent:other:project"),
            "mismatched context".into(),
            "context-key-mismatch".into(),
            Some(&writer),
        )
        .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["failure_kind"], "invalid_context");
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");

    let rows = runtime.window_collaboration(Some(&key), Some(&writer), 10);
    let context_rows = rows["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["context_session_id"] == session.session_id)
        .collect::<Vec<_>>();
    assert_eq!(context_rows.len(), 2);
    assert!(context_rows
        .iter()
        .all(|row| row["context_project"] == project));
    assert!(context_rows
        .iter()
        .all(|row| row["first_projected_at_ms"].is_null()));
    let rows_again = runtime.window_collaboration(Some(&key), Some(&writer), 10);
    assert_eq!(rows["messages"], rows_again["messages"]);
    assert!(runtime
        .sessions
        .list_messages(&session.session_id, Default::default())
        .unwrap()
        .is_empty());
}
