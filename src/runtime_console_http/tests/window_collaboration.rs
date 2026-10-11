use super::*;
use crate::runtime_console_http::window_collaboration;

#[tokio::test]
async fn operator_continuity_rotated_projectless_window_remains_discoverable() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut old = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE]);
    old.api_key_id = Some("review-old-token".into());
    let mut renewed = old.clone();
    renewed.api_key_id = Some("review-renewed-token".into());
    let target_window_key = "a".repeat(64);
    record_window_event(&db, &old, &target_window_key, None, None, 5_000);
    assert!(
        window_collaboration::authorize(&runtime, &renewed, &target_window_key)
            .await
            .is_ok()
    );
    let inventory = windows_for_auth(&runtime, &renewed, Some(10), None)
        .await
        .unwrap();
    assert_eq!(
        inventory.total, 1,
        "an authorized renewed Window must remain discoverable"
    );
    let mut foreign = old.clone();
    foreign.api_key_id = Some("foreign-token".into());
    foreign.allowed_client_id = Some("foreign-client".into());
    record_window_event(&db, &foreign, &"b".repeat(64), None, None, 6_000);
    let mut foreign_user = old.clone();
    foreign_user.api_key_id = Some("foreign-user-token".into());
    foreign_user.user_id = Some("foreign-user".into());
    record_window_event(&db, &foreign_user, &"c".repeat(64), None, None, 7_000);
    record_window_event(
        &db,
        &old,
        &"d".repeat(64),
        Some("agent:hidden:project"),
        None,
        8_000,
    );
    let inventory = windows_for_auth(&runtime, &renewed, Some(10), None)
        .await
        .unwrap();
    assert_eq!(
        inventory.total, 1,
        "stable identity must not bypass user/client/Project visibility"
    );
    assert_eq!(inventory.windows[0].client_window_key, target_window_key);
    for denied in [foreign, foreign_user] {
        assert!(
            window_collaboration::authorize(&runtime, &denied, &target_window_key)
                .await
                .is_err()
        );
    }
    let detail = window_for_auth(
        &runtime,
        &renewed,
        WindowInput {
            client_window_key: target_window_key,
            activity_limit: Some(20),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(detail.activity.len(), 1);
    let serialized = serde_json::to_string(&detail).unwrap();
    assert!(!serialized.contains(old.user_id.as_deref().unwrap()));
    assert!(!serialized.contains(old.allowed_client_id.as_deref().unwrap()));
}

#[tokio::test]
async fn operator_continuity_live_visibility_uses_admission_identity_without_sqlite() {
    use rusqlite::hooks::{AuthContext as SqlAuthContext, Authorization};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut old = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE]);
    old.api_key_id = Some("live-old-token".into());
    let mut renewed = old.clone();
    renewed.api_key_id = Some("live-renewed-token".into());
    let target = crate::client_window::ClientWindow::for_test("live-renewal");
    let guard = runtime.window_activity.start_authenticated(
        &target,
        "live-renewal-trace",
        "tools/list",
        None,
        Some(&old),
        5_000,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    db.conn_for_tests()
        .authorizer(Some(move |_: SqlAuthContext<'_>| {
            observed.fetch_add(1, Ordering::SeqCst);
            Authorization::Deny
        }))
        .unwrap();
    for (auth, expected) in [
        (renewed.clone(), 1),
        (
            {
                let mut other = renewed.clone();
                other.allowed_client_id = Some("different-client".into());
                other
            },
            0,
        ),
        (
            {
                let mut other = renewed.clone();
                other.user_id = Some("different-user".into());
                other
            },
            0,
        ),
        (
            {
                let mut other = renewed.clone();
                other.token_kind = Some("oauth2_shared_key".into());
                other
            },
            0,
        ),
        (
            {
                let mut other = renewed.clone();
                other.allowed_client_id = None;
                other
            },
            0,
        ),
    ] {
        let live = super::super::window_inventory::query_for_auth(
            &runtime,
            &auth,
            WindowsInput {
                projection: WindowInventoryProjection::Liveness,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(live.total, expected);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "live visibility must not query the writer"
    );
    assert_eq!(
        db.window_read_counts_for_test(),
        (0, 0),
        "live visibility must not query the history reader"
    );
    db.conn_for_tests()
        .authorizer(None::<fn(SqlAuthContext<'_>) -> Authorization>)
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn operator_continuity_missing_attribution_does_not_create_an_unreadable_mailbox() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut recipient = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE]);
    recipient.api_key_id = Some("missing-attribution-token".into());
    let mut historical = recipient.clone();
    historical.user_id = None;
    historical.allowed_client_id = None;
    let key = "e".repeat(64);
    record_window_event(&db, &historical, &key, None, None, 5_000);
    let operator = test_bootstrap_auth();
    let target = window_collaboration::authorize(&runtime, &operator, &key)
        .await
        .unwrap();
    let result = runtime
        .post_window_operator_message_with_options_for_principal(
            &key,
            None,
            None,
            "guidance",
            "normal",
            true,
            "must not be orphaned".into(),
            "missing-attribution".into(),
            &target.0,
            &target.1,
            Some(&operator),
        )
        .await;
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "operator_recipient_unavailable"
    );
    assert_eq!(result.output["state_changed"], false);
    assert!(db
        .list_window_operator_messages(&target.0, &target.1, &key, 10)
        .unwrap()
        .is_empty());
    let own = runtime
        .post_window_operator_message(
            &key,
            None,
            None,
            "current identity is sufficient".into(),
            "authenticated-send".into(),
            Some(&recipient),
        )
        .await;
    assert!(own.success);
    let page = runtime.window_collaboration_page(Some(&key), Some(&recipient), 10, None);
    assert_eq!(page["messages"][0]["message_id"], own.output["message_id"]);
}

#[tokio::test]
async fn operator_continuity_ambiguous_operator_recipient_does_not_report_delivery() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut recipient = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE]);
    recipient.api_key_id = Some("review-ambiguous-token".into());
    let target_window_key = "b".repeat(64);
    record_window_event(&db, &recipient, &target_window_key, None, None, 5_000);
    let mut contradictory = recipient.clone();
    contradictory.allowed_client_id = Some("review-different-client".into());
    record_window_event(&db, &contradictory, &target_window_key, None, None, 6_000);
    let operator = test_bootstrap_auth();
    let target = window_collaboration::authorize(&runtime, &operator, &target_window_key)
        .await
        .unwrap();
    assert!(db
        .oauth_window_operator_principal(&target_window_key, &target.1)
        .unwrap()
        .is_none());
    let result = runtime
        .post_window_operator_message_with_options_for_principal(
            &target_window_key,
            None,
            None,
            "guidance",
            "normal",
            true,
            "review fixture".into(),
            "review-delivery".into(),
            &target.0,
            &target.1,
            Some(&operator),
        )
        .await;
    let view = runtime.window_collaboration(Some(&target_window_key), Some(&recipient), 10);
    assert_eq!(view["messages"].as_array().unwrap().len(), 0);
    assert!(!result.success, "ambiguous routing must fail closed instead of reporting delivery to an unreadable legacy mailbox");
}

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
    window_auth.token_kind = Some("oauth2".to_string());
    window_auth.allowed_client_id = Some("managed-oauth-client-1".to_string());
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
    let client_window = crate::client_window::ClientWindow::for_test("operator-oauth-rotation");
    let key = client_window.key().to_string();
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
    let renewed_view = runtime.window_collaboration(Some(&key), Some(&operator_auth), 10);
    assert_eq!(
        renewed_view["messages"], model_view["messages"],
        "a refreshed access token must not orphan an Operator message in the same Window/client"
    );
    let message_id = sent.output["message_id"].as_str().unwrap().to_string();

    // The exact same persisted recipient survives an independent access token.
    let mut projected = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut projected,
        Some(&operator_auth),
        Some(&client_window),
        &[],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert_eq!(
        projected.output["operator_messages"]["messages"][0]["message_id"],
        message_id
    );
    let mut reduced_scope = operator_auth.clone();
    reduced_scope
        .scopes
        .retain(|scope| scope != SCOPE_SESSION_COLLABORATE);
    let mut scope_denied = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut scope_denied,
        Some(&reduced_scope),
        Some(&client_window),
        &[],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert!(
        scope_denied.output.get("operator_messages").is_none(),
        "a renewed grant without collaboration scope must not consume Operator messages"
    );
    let reply = crate::tool_runtime::window_collaboration::ToolCallWindowReply {
        reply_to_message_id: message_id.clone(),
        message: "rotated token can reply".to_string(),
    };
    let mut reply_result = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_model_reply_sidecar(
        &mut reply_result,
        Some(&operator_auth),
        Some(&client_window),
        Some(&reply),
    );
    assert_eq!(reply_result.output["window_reply"]["success"], true);
    assert_eq!(reply_result.output["window_reply"]["reply_to"], message_id);

    let mut acked = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut acked,
        Some(&operator_auth),
        Some(&client_window),
        &[message_id.clone()],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert_eq!(
        acked.output["operator_messages"]["ack"]["accepted_ids"][0],
        message_id
    );
    let mut after = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut after,
        Some(&window_auth),
        Some(&client_window),
        &[],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert!(
        after.output.get("operator_messages").is_none(),
        "persistent Operator ACK must suppress delivery even to the original token"
    );

    // Same key/intent under the same Window recipient principal is replay-safe.
    let replay = runtime
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
    assert!(replay.success);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(replay.output["message_id"], message_id);

    let mut other_user = operator_auth.clone();
    other_user.user_id = Some("different-user".into());
    let mut other_client = operator_auth.clone();
    other_client.allowed_client_id = Some("different-client".into());
    for denied in [&other_user, &other_client] {
        assert!(
            runtime.window_collaboration(Some(&key), Some(denied), 10)["messages"]
                .as_array()
                .unwrap()
                .is_empty(),
            "a matching Window key never overrides the authenticated user/client boundary"
        );
        let mut denied_reply = crate::tool_runtime::ToolResult::ok(json!({}));
        runtime.add_window_model_reply_sidecar(
            &mut denied_reply,
            Some(denied),
            Some(&client_window),
            Some(&reply),
        );
        assert_eq!(denied_reply.output["window_reply"]["success"], false);
    }
    // A stable Operator namespace must not hide or reauthorize Peer history.
    let other_peer = crate::client_window::ClientWindow::for_test("operator-peer");
    let legacy_peer =
        crate::tool_runtime::runtime_observation_principal(Some(&window_auth)).unwrap();
    db.post_peer_message(webcodex_store::NewPeerMessage {
        principal_kind: legacy_peer.0.clone(),
        principal_id: legacy_peer.1.clone(),
        sender_window_key: other_peer.key().to_string(),
        recipient_window_key: key.clone(),
        sender_peer_id: other_peer.peer_id(),
        recipient_peer_id: client_window.peer_id(),
        kind: "note".into(),
        priority: "normal".into(),
        message: "legacy peer conversation".into(),
        tags: vec![],
        requires_ack: false,
        sender_session_id: None,
        sender_project: None,
        created_at_ms: 9_000,
    })
    .unwrap();
    let original_history = runtime.window_collaboration(Some(&key), Some(&window_auth), 10);
    assert!(original_history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["source"] == "peer"));
    let renewed_history = runtime.window_collaboration(Some(&key), Some(&operator_auth), 10);
    assert!(
        renewed_history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["source"] != "peer"),
        "another access-token principal must not inherit legacy Peer history"
    );
    assert_eq!(
        original_history["messages"],
        runtime.window_collaboration_for_principal(&key, &routed.0, &routed.1, 10)["messages"],
        "Runtime Console and Work Result must merge exactly the recipient's history"
    );
    let different_window = crate::client_window::ClientWindow::for_test("another-window");
    assert!(
        runtime.window_collaboration(Some(different_window.key()), Some(&operator_auth), 10)
            ["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn managed_oauth_legacy_operator_history_is_migrated_without_cross_window_access() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut old = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    old.api_key_id = Some("oauth-legacy-token-a".into());
    let mut refreshed = old.clone();
    refreshed.api_key_id = Some("oauth-legacy-token-b".into());
    let target_window = crate::client_window::ClientWindow::for_test("legacy-operator-window");
    let principal = crate::tool_runtime::runtime_observation_principal(Some(&old)).unwrap();
    record_window_event(&db, &old, target_window.key(), None, None, 5_000);
    let legacy = webcodex_store::NewWindowOperatorMessage {
        principal_kind: principal.0.clone(),
        principal_id: principal.1.clone(),
        recipient_window_key: target_window.key().into(),
        context_session_id: None,
        context_project: None,
        kind: "guidance".into(),
        priority: "normal".into(),
        message: "legacy message must survive rollout".into(),
        tags: vec![],
        requires_ack: true,
        created_at_ms: 5_100,
    };
    let sent = db
        .post_window_operator_message(legacy, "legacy-send")
        .unwrap();
    let old_message = match sent {
        webcodex_store::WindowOperatorDeliveryOutcome::Delivered { message_id, .. } => message_id,
        _ => panic!("expected a legacy message"),
    };
    let projected = db
        .take_window_operator_attention(
            &principal.0,
            &principal.1,
            target_window.key(),
            &[],
            5_200,
            4,
        )
        .unwrap();
    assert_eq!(projected.messages.len(), 1);
    let reply = webcodex_store::NewWindowModelReply {
        principal_kind: principal.0.clone(),
        principal_id: principal.1.clone(),
        window_key: target_window.key().into(),
        reply_to_message_id: old_message.clone(),
        message: "legacy reply must survive rollout".into(),
        created_at_ms: 5_300,
    };
    db.post_window_model_reply(reply, "legacy-reply").unwrap();
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        1
    );
    assert_eq!(
        db.migrate_legacy_managed_oauth_operator_messages().unwrap(),
        0
    );
    let history = runtime.window_collaboration(Some(target_window.key()), Some(&refreshed), 10);
    assert_eq!(history["messages"].as_array().unwrap().len(), 2);
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["message_id"] == old_message && m["source"] == "operator"));
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["reply_to_message_id"] == old_message && m["source"] == "window"));

    let mut output = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut output,
        Some(&refreshed),
        Some(&target_window),
        &[],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert_eq!(
        output.output["operator_messages"]["messages"][0]["message_id"],
        old_message
    );

    let mut ack = crate::tool_runtime::ToolResult::ok(json!({}));
    runtime.add_window_operator_projection_until(
        &mut ack,
        Some(&refreshed),
        Some(&target_window),
        &[old_message.clone()],
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    );
    assert_eq!(
        ack.output["operator_messages"]["ack"]["accepted_ids"][0],
        old_message
    );

    let mut different_client = refreshed.clone();
    different_client.allowed_client_id = Some("not-original-client".into());
    assert!(
        runtime.window_collaboration(Some(target_window.key()), Some(&different_client), 10)
            ["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let other_window = crate::client_window::ClientWindow::for_test("not-original-window");
    assert!(
        runtime.window_collaboration(Some(other_window.key()), Some(&refreshed), 10)["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Existing databases migrate provably linked old-token rows on reopen,
    // retaining the original ACK and delivery-key replay identity.
    let another_legacy = webcodex_store::NewWindowOperatorMessage {
        principal_kind: principal.0.clone(),
        principal_id: principal.1.clone(),
        recipient_window_key: target_window.key().into(),
        context_session_id: None,
        context_project: None,
        kind: "guidance".into(),
        priority: "normal".into(),
        message: "migrated during reopen".into(),
        tags: vec![],
        requires_ack: true,
        created_at_ms: 5_600,
    };
    let extra_id = match db
        .post_window_operator_message(another_legacy, "legacy-on-restart")
        .unwrap()
    {
        webcodex_store::WindowOperatorDeliveryOutcome::Delivered { message_id, .. } => message_id,
        _ => panic!("expected another legacy message"),
    };
    drop(runtime);
    drop(db);
    let reopened = webcodex_store::Database::open(&_tmp.path().join("goal-console.db")).unwrap();
    let (stable_kind, stable_principal) = webcodex_store::managed_oauth_operator_principal(
        old.user_id.as_deref().unwrap(),
        old.allowed_client_id.as_deref().unwrap(),
    )
    .unwrap();
    let rows = reopened
        .list_window_operator_messages(&stable_kind, &stable_principal, target_window.key(), 10)
        .unwrap();
    assert!(rows.iter().any(|msg| msg.message_id == extra_id));
    assert!(rows
        .iter()
        .any(|msg| { msg.message_id == old_message && msg.first_ack_observed_at_ms.is_some() }));
    assert_eq!(
        reopened
            .migrate_legacy_managed_oauth_operator_messages()
            .unwrap(),
        0
    );
    let retried = webcodex_store::NewWindowOperatorMessage {
        principal_kind: stable_kind,
        principal_id: stable_principal,
        recipient_window_key: target_window.key().into(),
        context_session_id: None,
        context_project: None,
        kind: "guidance".into(),
        priority: "normal".into(),
        message: "migrated during reopen".into(),
        tags: vec![],
        requires_ack: true,
        created_at_ms: 5_700,
    };
    assert!(matches!(
        reopened.post_window_operator_message(retried, "legacy-on-restart").unwrap(),
        webcodex_store::WindowOperatorDeliveryOutcome::Delivered { message_id, replayed: true }
            if message_id == extra_id
    ));
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
    other.allowed_client_id = Some("another-oauth-client".into());
    let mut rotated = writer.clone();
    rotated.api_key_id = Some("new-access-token-same-oauth-client".into());
    let key = "a".repeat(64);
    record_window_event(&db, &writer, &key, None, None, 5000);
    assert!(window_collaboration::authorize(&runtime, &writer, &key)
        .await
        .is_ok());
    assert!(
        window_collaboration::authorize(&runtime, &rotated, &key)
            .await
            .is_ok(),
        "unscoped Window authorization must survive managed OAuth access-token renewal"
    );
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
    let rotated_transcript = runtime.window_collaboration(Some(&key), Some(&rotated), 10);
    assert_eq!(rotated_transcript["messages"], transcript["messages"]);
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
