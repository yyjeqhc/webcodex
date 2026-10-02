use super::*;

#[test]
fn window_activity_lookup_is_runtime_management_and_current_project_authority_bounded() {
    // This multi-principal integration fixture overflows the default libtest
    // stack in workspace builds, even when selected alone with one test thread.
    // Match the bounded stack isolation used by the large MCP fixtures without
    // changing production runtime stacks or weakening any authority assertions.
    std::thread::Builder::new()
            .name("runtime-console-window-authority".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build window authority test runtime")
                    .block_on(
                        window_activity_lookup_is_runtime_management_and_current_project_authority_bounded_body(),
                    );
            })
            .expect("spawn window authority test thread")
            .join()
            .expect("window authority test thread panicked");
}

#[tokio::test]
async fn window_visibility_scope_distinguishes_management_and_project_scoped_credentials() {
    let (_tmp, _db, runtime) = test_runtime_with_window_db();
    let admin = test_bootstrap_auth();
    let ordinary = crate::auth::shared_key_context("window-vis-test");
    let mut project_scoped = AuthContext::new(AuthKind::ProjectCredential);
    project_scoped.project_grant_id = Some("window-project-grant".to_string());
    project_scoped.scopes = vec![
        SCOPE_RUNTIME_READ.to_string(),
        SCOPE_PROJECT_READ.to_string(),
    ];

    for management in [&admin, &ordinary] {
        let list = windows_for_auth(&runtime, management, Some(10), None)
            .await
            .unwrap();
        assert_eq!(
            list.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        let encoded = serde_json::to_string(&list).unwrap();
        assert!(encoded.contains("\"visibility\":{\"scope\":\"global\"}"));
        assert!(!encoded.contains("window-vis-test"));
    }

    let project_list = windows_for_auth(&runtime, &project_scoped, Some(10), None)
        .await
        .unwrap();
    assert_eq!(
        project_list.visibility.scope,
        RuntimeConsoleWindowVisibilityScope::Principal
    );
    assert!(serde_json::to_string(&project_list)
        .unwrap()
        .contains("\"visibility\":{\"scope\":\"principal\"}"));
}

#[tokio::test]
async fn window_management_view_survives_oauth_access_token_rotation() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let mut writer = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
    writer.api_key_id = Some("oauth-window-token-a".to_string());
    let mut reader = writer.clone();
    reader.api_key_id = Some("oauth-window-token-b".to_string());
    assert_ne!(
        crate::tool_runtime::runtime_observation_principal(Some(&writer)).unwrap(),
        crate::tool_runtime::runtime_observation_principal(Some(&reader)).unwrap(),
        "fixture must model the historical token-specific observation principal"
    );

    let project = "agent:window-user:shared-project";
    register_project(
        &runtime,
        "window-user",
        "shared-project",
        "/private/window-user",
        Some(&writer),
    )
    .await;
    let window_key = "9".repeat(64);
    record_window_event(&db, &writer, &window_key, Some(project), None, 5_000);

    let list = windows_for_auth(&runtime, &reader, Some(10), None)
        .await
        .unwrap();
    assert_eq!(
        list.visibility.scope,
        RuntimeConsoleWindowVisibilityScope::Global
    );
    assert_eq!(list.total, 1);
    assert_eq!(list.windows[0].client_window_key, window_key);
    assert_eq!(list.windows[0].last_project.as_deref(), Some(project));

    let detail = window_for_auth(
        &runtime,
        &reader,
        WindowInput {
            client_window_key: window_key,
            activity_limit: Some(20),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        detail.visibility.scope,
        RuntimeConsoleWindowVisibilityScope::Global
    );
    assert_eq!(detail.activity.len(), 1);
    assert_eq!(detail.activity[0].project.as_deref(), Some(project));
}

#[tokio::test]
async fn window_liveness_is_history_free_and_project_scope_precedes_pagination() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    register_project(
        &runtime,
        "live-inventory",
        "repo",
        "/tmp/live-inventory",
        Some(&auth),
    )
    .await;
    let project_id = "agent:live-inventory:repo";
    let target = crate::client_window::ClientWindow::for_test("inventory-live");
    let principal = crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    let guard = runtime.window_activity.start(
        &target,
        "inventory-trace",
        "tools/call",
        Some((&principal.0, &principal.1)),
    );
    runtime
        .window_activity
        .update("inventory-trace", Some("read_files"), Some(project_id));
    let before = db.window_read_counts_for_test();
    let live = super::super::window_inventory::query_for_auth(
        &runtime,
        &auth,
        WindowsInput {
            projection: WindowInventoryProjection::Liveness,
            projects: Some(vec![project_id.into()]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(live.total, 1);
    assert_eq!(live.windows[0].active_count, 1);
    assert_eq!(
        db.window_read_counts_for_test(),
        before,
        "liveness must not read SQLite inventory or history"
    );
    drop(guard);
    let idle = super::super::window_inventory::query_for_auth(
        &runtime,
        &auth,
        WindowsInput {
            projection: WindowInventoryProjection::Liveness,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(idle.total, 0);
    for index in 0..3 {
        record_window_event(
            &db,
            &auth,
            &format!("{index:064x}"),
            Some(project_id),
            None,
            index,
        );
    }
    let page = super::super::window_inventory::query_for_auth(
        &runtime,
        &auth,
        WindowsInput {
            projects: Some(vec![project_id.into()]),
            limit: Some(1),
            offset: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.next_offset, Some(2));
    assert_eq!(page.windows[0].client_window_key, format!("{:064x}", 1));
    let exact = super::super::window_inventory::query_for_auth(
        &runtime,
        &auth,
        WindowsInput {
            client_window_key: Some(format!("{:064x}", 0)),
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(exact.total, 1);
    assert!(super::super::window_inventory::query_for_auth(
        &runtime,
        &auth,
        WindowsInput {
            projects: Some(vec![]),
            ..Default::default()
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn window_inventory_reads_two_projections_and_zero_per_window_histories() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    register_project(&runtime, "inventory", "repo", "/tmp/inventory", Some(&auth)).await;
    for index in 0..25 {
        record_window_event(
            &db,
            &auth,
            &format!("{index:064x}"),
            Some("agent:inventory:repo"),
            None,
            index,
        );
    }
    let before = db.window_read_counts_for_test();
    let page = windows_for_auth(&runtime, &auth, Some(10), None)
        .await
        .unwrap();
    let after = db.window_read_counts_for_test();
    assert_eq!(page.returned, 10);
    assert_eq!(page.total, 25);
    assert!(page.truncated);
    assert_eq!(
        after.0 - before.0,
        0,
        "inventory must not hydrate per-Window history"
    );
    assert_eq!(
        after.1 - before.1,
        2,
        "one anchor projection plus one inventory statement"
    );
}

#[tokio::test]
async fn revoked_project_event_cannot_be_bridged_by_window_gap_projection() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth_a = crate::auth::shared_key_context("timing-visible-a");
    let auth_b = crate::auth::shared_key_context("timing-hidden-b");
    let project_a = "agent:timing-a:visible";
    let project_b = "agent:timing-b:hidden";
    register_project(
        &runtime,
        "timing-a",
        "visible",
        "/private/timing-a",
        Some(&auth_a),
    )
    .await;
    register_project(
        &runtime,
        "timing-b",
        "hidden",
        "/private/timing-b",
        Some(&auth_b),
    )
    .await;
    let window_key = "f".repeat(64);
    record_timed_window_event(
        &db,
        &auth_a,
        &window_key,
        Some(project_a),
        1_000,
        1_100,
        1_101,
        "unavailable",
    );
    // Historical same-principal event whose Project is no longer visible.
    record_timed_window_event(
        &db,
        &auth_a,
        &window_key,
        Some(project_b),
        1_500,
        1_550,
        1_551,
        "serial",
    );
    record_timed_window_event(
        &db,
        &auth_a,
        &window_key,
        Some(project_a),
        2_000,
        2_050,
        2_051,
        "serial",
    );

    let detail = window_for_auth(
        &runtime,
        &auth_a,
        WindowInput {
            client_window_key: window_key,
            activity_limit: Some(20),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(detail.activity.len(), 2);
    assert!(detail
        .activity
        .iter()
        .all(|event| event.project.as_deref() == Some(project_a)));
    assert!(detail
        .activity
        .iter()
        .all(|event| event.next_call_gap_ms.is_none() && event.cycle_ms.is_none()));
    let serialized = serde_json::to_string(&detail).unwrap();
    assert!(!serialized.contains(project_b));
}
