use super::*;

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn code_mode_composition_projects_on_one_outer_window_activity() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    let client_window = crate::client_window::ClientWindow::for_test("code-mode-composition");
    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    crate::action_audit_sessions::record_action_event(
        &db,
        crate::action_audit_sessions::ActionAuditEventInput {
            explicit_session_id: Some("code-mode-window-audit".to_string()),
            session_title: None,
            endpoint: "/mcp".to_string(),
            action_name: "toolsCall".to_string(),
            operation: Some("execute_code_mode".to_string()),
            project: None,
            principal_kind: None,
            principal_user_id: None,
            oauth_client_id: None,
            status: "success".to_string(),
            http_status: Some(200),
            started_at: 1,
            ended_at: 1,
            duration_ms: 13,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({
                "transport": "mcp",
                "code_mode_composition": {
                    "nested_calls": 3,
                    "nested_successes": 2,
                    "nested_failures": 1,
                    "max_in_flight": 2,
                    "duration_ms": 11,
                    "slot_wait_ms": 3,
                    "returned_bytes": 19,
                    "nested_raw_result_bytes_total": 31,
                    "nested_tool_counts": {
                        "get_git_status": 1,
                        "read_files": 1,
                        "search_project_texts": 1
                    },
                    "consequential_calls": 1,
                    "known_results": 1,
                    "job_handoffs": 0,
                    "outcome_unknown": 0
                }
            }),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(client_window.key().to_string()),
            client_window_source: Some("openai-session".to_string()),
            server_trace_id: Some("trace-code-mode-composition".to_string()),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(1_000),
            window_ended_at_ms: Some(1_013),
            request_observed_at_ms: Some(1_000),
            response_handed_at_ms: Some(1_013),
            window_transition_kind: Some("unavailable".to_string()),
            response_streaming: Some(false),
            window_continuity_eligible: Some(true),
            window_meaningful: true,
            recorder_gap_session_id: None,
            workflow_links: Vec::new(),
        },
    );

    let detail = window_for_auth(
        &runtime,
        &auth,
        WindowInput {
            client_window_key: client_window.key().to_string(),
            activity_limit: None,
            session_limit: None,
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        detail.activity.len(),
        1,
        "nested canonical calls must not fabricate Window activity rows"
    );
    let activity = &detail.activity[0];
    assert_eq!(activity.tool_name.as_deref(), Some("execute_code_mode"));
    assert!(activity.meaningful);
    let composition = activity
        .code_mode_composition
        .as_ref()
        .expect("bounded Code Mode composition projection");
    assert_eq!(composition.nested_calls, 3);
    assert_eq!(composition.nested_successes, 2);
    assert_eq!(composition.nested_failures, 1);
    assert_eq!(composition.max_in_flight, 2);
    assert_eq!(composition.duration_ms, 11);
    assert_eq!(composition.slot_wait_ms, 3);
    assert_eq!(composition.returned_bytes, 19);
    assert_eq!(composition.nested_raw_result_bytes_total, 31);
    assert_eq!(composition.nested_tool_counts.len(), 3);
    assert_eq!(composition.consequential_calls, 1);
    assert_eq!(composition.known_results, 1);
    assert_eq!(composition.job_handoffs, 0);
    assert_eq!(composition.outcome_unknown, 0);

    let invalid = json!({
        "nested_calls": 1,
        "nested_successes": 1,
        "nested_failures": 0,
        "max_in_flight": 1,
        "duration_ms": 1,
        "slot_wait_ms": 0,
        "returned_bytes": 1,
        "nested_raw_result_bytes_total": 1,
        "nested_tool_counts": {"run_shell": 1},
        "consequential_calls": 0,
        "known_results": 0,
        "job_handoffs": 0,
        "outcome_unknown": 0
    });
    assert!(project_code_mode_composition(&invalid).is_none());
    let events = db.list_action_events("code-mode-window-audit", 10).unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn window_activity_counts_all_visible_requests_before_bounding_details() {
    let (_tmp, _db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    let client_window = crate::client_window::ClientWindow::for_test("concurrent-window");
    let guards = (0..12)
        .map(|index| {
            runtime.window_activity.start(
                &client_window,
                &format!("trace-{index}"),
                "tools/list",
                None,
            )
        })
        .collect::<Vec<_>>();
    let detail = window_for_auth(
        &runtime,
        &auth,
        WindowInput {
            client_window_key: client_window.key().to_string(),
            activity_limit: None,
            session_limit: None,
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(detail.active_count, guards.len());
    assert_eq!(
        detail.active_requests.len(),
        crate::tool_runtime::MAX_ACTIVE_REQUESTS_PER_WINDOW
    );
    let list = windows_for_auth(&runtime, &auth, None, None).await.unwrap();
    assert_eq!(list.windows[0].active_count, detail.active_count);
    assert_eq!(
        list.windows[0].last_activity_name.as_deref(),
        Some("tools/list")
    );
    assert_eq!(
        list.windows[0].last_activity_status.as_deref(),
        Some("running")
    );
    assert_eq!(
        active_window_count_for_auth(&runtime, &auth).await.unwrap(),
        1
    );
}

#[tokio::test]
async fn window_activity_live_history_outside_durable_page_is_not_counted_twice() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    let old = crate::client_window::ClientWindow::for_test("old-active-window");
    record_window_event(&db, &auth, old.key(), None, None, 1_000);
    for index in 0..MAX_WINDOW_LIMIT {
        record_window_event(
            &db,
            &auth,
            &format!("{index:064x}"),
            None,
            None,
            2_000 + index as i64,
        );
    }
    let _active = runtime
        .window_activity
        .start(&old, "old-active", "tools/call", None);
    let list = windows_for_auth(&runtime, &auth, None, None).await.unwrap();
    assert_eq!(list.total, MAX_WINDOW_LIMIT + 1);
    assert!(list.truncated);
    let row = list
        .windows
        .iter()
        .find(|row| row.client_window_key == old.key())
        .unwrap();
    assert_eq!(row.active_count, 1);
    assert_eq!(row.last_tool_call_at_ms, Some(1_001));
    assert_eq!(row.last_meaningful_activity_at_ms, Some(1_001));
}

#[tokio::test]
async fn window_summary_ignores_legacy_work_result_app_polling() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("window-summary-noise");
    let project = "agent:window-summary-noise:project";
    register_project(
        &runtime,
        "window-summary-noise",
        "project",
        "/private/window-summary-noise",
        Some(&auth),
    )
    .await;
    let window_key = "9".repeat(64);
    record_window_event_with_activity(
        &db,
        &auth,
        &window_key,
        Some(project),
        None,
        1_000,
        "read_files",
        true,
    );
    record_window_event_with_activity(
        &db,
        &auth,
        &window_key,
        Some(project),
        None,
        2_000,
        "get_work_result_state",
        false,
    );

    let list = windows_for_auth(&runtime, &auth, Some(20), None)
        .await
        .unwrap();
    let row = list
        .windows
        .iter()
        .find(|row| row.client_window_key == window_key)
        .unwrap();
    assert_eq!(row.last_activity_name.as_deref(), Some("read_files"));
    assert_eq!(row.last_seen_at_ms, 1_001);
    assert_eq!(row.first_seen_at_ms, Some(1_001));
    assert_eq!(row.last_tool_call_at_ms, Some(1_001));
    assert_eq!(row.last_project.as_deref(), Some(project));
}

#[tokio::test]
async fn window_primary_detail_is_bounded_and_defers_secondary_hydration() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("window-primary-detail");
    let project = "agent:window-primary-detail:webcodex";
    register_project(
        &runtime,
        "window-primary-detail",
        "webcodex",
        "/private/window-primary-detail",
        Some(&auth),
    )
    .await;
    let session = runtime.sessions.start_session(
        Some(project.to_string()),
        Some("Primary detail hydration".to_string()),
    );
    let window_key = "8".repeat(64);

    record_window_event_with_activity(
        &db,
        &auth,
        &window_key,
        Some(project),
        Some((&session.session_id, project)),
        1_000,
        "work_on_project",
        true,
    );
    for (index, tool) in [
        "read_files",
        "run_shell",
        "search_project_texts",
        "cargo_test",
    ]
    .into_iter()
    .enumerate()
    {
        record_window_event_with_activity(
            &db,
            &auth,
            &window_key,
            Some(project),
            None,
            2_000 + index as i64 * 1_000,
            tool,
            true,
        );
    }

    let primary = window_for_auth(
        &runtime,
        &auth,
        WindowInput {
            client_window_key: window_key.clone(),
            activity_limit: Some(2),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Primary,
        },
    )
    .await
    .unwrap();
    assert_eq!(primary.detail_level, WindowDetailLevel::Primary);
    assert_eq!(primary.activity.len(), 2);
    assert!(primary.activity_truncated);
    assert!(primary.linked_sessions.is_empty());
    assert_eq!(primary.sessions_returned, 0);
    assert!(primary.jobs.is_empty());

    let full = window_for_auth(
        &runtime,
        &auth,
        WindowInput {
            client_window_key: window_key,
            activity_limit: Some(20),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    assert_eq!(full.detail_level, WindowDetailLevel::Full);
    assert_eq!(full.first_seen_at_ms, Some(1_001));
    assert_eq!(full.activity.len(), 5);
    assert!(!full.activity_truncated);
    assert_eq!(full.linked_sessions.len(), 1);
    assert_eq!(
        full.linked_sessions[0].workflow_session_id,
        session.session_id
    );
    assert_eq!(full.linked_sessions[0].project.as_deref(), Some(project));
}

#[tokio::test]
async fn window_activity_project_filter_returns_only_exact_project_evidence() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("window-filter");
    let project_a = "agent:window-filter-a:proj-a";
    let project_b = "agent:window-filter-b:proj-b";
    register_project(
        &runtime,
        "window-filter-a",
        "proj-a",
        "/private/window-filter-a",
        Some(&auth),
    )
    .await;
    register_project(
        &runtime,
        "window-filter-b",
        "proj-b",
        "/private/window-filter-b",
        Some(&auth),
    )
    .await;
    let window_a = "a".repeat(64);
    let window_b = "b".repeat(64);
    record_window_event(&db, &auth, &window_a, Some(project_a), None, 1_000);
    record_window_event(&db, &auth, &window_b, Some(project_b), None, 2_000);

    let all = windows_for_auth(&runtime, &auth, Some(20), None)
        .await
        .unwrap();
    assert_eq!(all.total, 2);

    let filtered = windows_for_auth(&runtime, &auth, Some(20), Some(project_a))
        .await
        .unwrap();
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.returned, 1);
    assert_eq!(filtered.windows[0].client_window_key, window_a);
    assert_eq!(filtered.windows[0].last_project.as_deref(), Some(project_a));
    assert!(all
        .windows
        .iter()
        .any(|row| row.last_project.as_deref() == Some(project_b)));
    assert_eq!(
        filtered.windows[0].last_meaningful_activity_at_ms,
        Some(1_001)
    );
}

#[tokio::test]
async fn window_activity_projection_keeps_persisted_meaningful_and_projects_current_activity_semantics(
) {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("window-activity-semantics");
    let project = "agent:window-activity-semantics:project";
    register_project(
        &runtime,
        "window-activity-semantics",
        "project",
        "/private/window-activity-semantics",
        Some(&auth),
    )
    .await;
    let activity_window_key = "e".repeat(64);
    // Deliberately model historical event-time truth that disagrees with the
    // current definition. The read projection must not rewrite it.
    record_window_event_with_activity(
        &db,
        &auth,
        &activity_window_key,
        Some(project),
        None,
        1_000,
        "sync_goal_plan",
        true,
    );

    let detail = window_for_auth(
        &runtime,
        &auth,
        WindowInput {
            client_window_key: activity_window_key,
            activity_limit: Some(20),
            session_limit: Some(20),
            detail_level: WindowDetailLevel::Full,
        },
    )
    .await
    .unwrap();
    let activity = detail.activity.first().expect("projected activity");
    assert_eq!(activity.tool_name.as_deref(), Some("sync_goal_plan"));
    assert!(activity.meaningful, "persisted event-time bit must win");
    assert_eq!(activity.activity_presentation.as_deref(), Some("transport"));
    assert_eq!(activity.activity_kind, None);
}

#[tokio::test]
async fn window_timing_uses_response_handoff_not_legacy_audit_end() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("window-timing");
    let project = "agent:window-timing:visible";
    register_project(
        &runtime,
        "window-timing",
        "visible",
        "/private/window-timing",
        Some(&auth),
    )
    .await;
    let window_key = "e".repeat(64);
    record_timed_window_event(
        &db,
        &auth,
        &window_key,
        Some(project),
        1_000,
        1_100,
        1_900,
        "unavailable",
    );
    record_timed_window_event(
        &db,
        &auth,
        &window_key,
        Some(project),
        1_500,
        1_550,
        1_600,
        "serial",
    );

    let detail = window_for_auth(
        &runtime,
        &auth,
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
    let first = detail
        .activity
        .iter()
        .find(|event| event.service_ms == Some(100))
        .expect("first canonical-timing event");
    assert_eq!(
        first.ended_at_ms, 1_900,
        "legacy audit boundary remains distinct"
    );
    assert_eq!(first.next_call_gap_ms, Some(400));
    assert_eq!(first.cycle_ms, Some(500));
    let second = detail
        .activity
        .iter()
        .find(|event| event.service_ms == Some(50))
        .expect("second canonical-timing event");
    assert_eq!(second.window_transition_kind.as_deref(), Some("serial"));
}

#[test]
fn window_timing_does_not_bridge_an_ineligible_meaningful_event() {
    let (_tmp, db, _runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("interrupted-timing");
    let window_key = "a".repeat(64);
    for (start, transition) in [(1_000, "unavailable"), (1_500, "serial"), (2_000, "serial")] {
        record_timed_window_event(
            &db,
            &auth,
            &window_key,
            None,
            start,
            start + 100,
            start + 100,
            transition,
        );
    }
    let mut events = db
        .list_window_activity_events(&window_key, None, 20)
        .unwrap();
    assert_eq!(events.len(), 3);
    // A retained pre-fix sequence can still label the third request serial.
    // Neither a visible nor a revoked stream may be skipped to pair it with A.
    events[1].window_continuity_eligible = Some(false);
    events[1].response_streaming = Some(true);
    for visible in [[true, true, true], [true, false, true]] {
        let timings = project_window_loop_timings(&events, &visible);
        assert_eq!(timings[2].next_call_gap_ms, visible[1].then_some(400));
        assert_eq!(timings[2].cycle_ms, visible[1].then_some(500));
        assert!(timings[1].next_call_gap_ms.is_none());
        assert!(timings[1].service_ms.is_none());
    }
}

#[tokio::test]
async fn session_window_liveness_survives_sparse_workflow_relations() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("sparse-session-window");
    let project = "agent:sparse-runner:webcodex";
    register_project(
        &runtime,
        "sparse-runner",
        "webcodex",
        "/private/sparse",
        Some(&auth),
    )
    .await;
    let other_project = "agent:other-runner:other";
    register_project(
        &runtime,
        "other-runner",
        "other",
        "/private/other",
        Some(&auth),
    )
    .await;
    let session = runtime.sessions.start_session(
        Some(project.to_string()),
        Some("sparse relation".to_string()),
    );
    let window_key = "0cae4d71e62fe073f130a0e5da2c83425866e4aba9ac5746c043e2ea66000471";

    record_window_event_with_activity(
        &db,
        &auth,
        window_key,
        Some(project),
        Some((&session.session_id, project)),
        1_000,
        "work_on_project",
        true,
    );
    for (at_ms, tool) in [
        (2_000, "observe_jobs"),
        (3_000, "run_shell"),
        (4_000, "observe_jobs"),
    ] {
        record_window_event_with_activity(
            &db,
            &auth,
            window_key,
            Some(project),
            None,
            at_ms,
            tool,
            true,
        );
    }
    // Activity in another currently-visible Project must not advance this
    // Session's Window/Model liveness projection.
    record_window_event_with_activity(
        &db,
        &auth,
        window_key,
        Some(other_project),
        None,
        5_000,
        "run_shell",
        true,
    );
    db.insert_workspace_activity(
        3,
        &webcodex_core::activity_contract::ActivityRecord {
            tool: "run_shell",
            project: Some(project),
            surface: "mcp",
            client: Some("sparse-runner"),
            success: true,
            session_id: Some(&session.session_id),
            command: None,
            paths: Vec::new(),
            error_summary: None,
            scope: webcodex_core::activity_contract::ActivityScope::Unscoped,
        },
        None,
        2_000,
    )
    .unwrap();

    let detail = workflow_session_detail_with_windows(
        &runtime,
        &auth,
        project,
        &session.session_id,
        Some(20),
    )
    .await
    .unwrap();
    assert!(detail.window_activity_available);
    assert_eq!(detail.linked_windows.len(), 1);
    assert_eq!(detail.linked_windows[0].last_linked_at_ms, 1_001);
    assert_eq!(detail.linked_windows[0].last_seen_at_ms, 4_001);
    assert_eq!(
        detail.linked_windows[0].last_meaningful_activity_at_ms,
        Some(4_001)
    );
    assert_eq!(detail.linked_windows[0].active_count, 0);
    assert_eq!(detail.window_activity_after_last_session_record.len(), 3);
    assert_eq!(
        detail
            .window_activity_after_last_session_record
            .iter()
            .filter_map(|activity| activity.tool_name.as_deref())
            .collect::<Vec<_>>(),
        vec!["observe_jobs", "run_shell", "observe_jobs"]
    );
    assert!(!detail.window_activity_after_last_session_record_truncated);
    assert!(detail.workspace_activity_available);
    let workspace = detail.workspace_last_activity.expect("workspace action");
    assert_eq!(workspace.created_at, 3);
    assert_eq!(workspace.tool, "run_shell");
    assert!(workspace.success);
    assert!(detail.job_activity_available);
    assert!(detail.jobs.is_empty());
    assert!(!detail.jobs_truncated);
}

#[tokio::test]
async fn session_window_activity_reports_bounded_source_truncation() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = crate::auth::shared_key_context("session-window-truncation");
    let project = "agent:truncation-runner:webcodex";
    register_project(
        &runtime,
        "truncation-runner",
        "webcodex",
        "/private/truncation",
        Some(&auth),
    )
    .await;
    let other_project = "agent:truncation-other:other";
    register_project(
        &runtime,
        "truncation-other",
        "other",
        "/private/truncation-other",
        Some(&auth),
    )
    .await;
    let session = runtime.sessions.start_session(
        Some(project.to_string()),
        Some("bounded window activity".to_string()),
    );
    let window_key = "1cae4d71e62fe073f130a0e5da2c83425866e4aba9ac5746c043e2ea66000471";
    record_window_event_with_activity(
        &db,
        &auth,
        window_key,
        Some(project),
        Some((&session.session_id, project)),
        1_000,
        "work_on_project",
        true,
    );
    for index in 0..MAX_WINDOW_ACTIVITY_LIMIT {
        record_window_event_with_activity(
            &db,
            &auth,
            window_key,
            Some(other_project),
            None,
            2_000 + index as i64,
            "observe_jobs",
            true,
        );
    }

    let detail = workflow_session_detail_with_windows(
        &runtime,
        &auth,
        project,
        &session.session_id,
        Some(20),
    )
    .await
    .unwrap();
    assert!(detail.window_activity_after_last_session_record.is_empty());
    assert!(detail.window_activity_after_last_session_record_truncated);
}
