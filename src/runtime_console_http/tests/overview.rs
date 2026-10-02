use super::*;

#[test]
fn runtime_home_recent_ranking_is_working_then_updated_at_then_identity() {
    let rows = vec![
        recent_test_row("z", "agent:z:newest", "newest", 400, false, false, false),
        recent_test_row(
            "a",
            "agent:a:attention",
            "attention",
            200,
            false,
            true,
            true,
        ),
        recent_test_row("b", "agent:b:active", "active", 300, false, false, true),
        recent_test_row("c", "agent:c:working", "working", 100, true, false, false),
        recent_test_row("a", "agent:a:tie", "tie-b", 50, false, false, false),
        recent_test_row("a", "agent:a:tie", "tie-a", 50, false, false, false),
    ];
    let ranked = finalize_recent_sessions(rows, false);
    assert_eq!(
        ranked
            .sessions
            .iter()
            .map(|row| row.session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["working", "newest", "active", "attention", "tie-a", "tie-b"]
    );
    assert!(!ranked.truncated);
    assert!(!ranked.scan_truncated);
}

#[test]
fn runtime_home_recent_and_project_scans_are_explicitly_bounded() {
    let recent = finalize_recent_sessions(
        (0..HOME_RECENT_SESSION_LIMIT + 3)
            .map(|index| {
                recent_test_row(
                    "runner",
                    "agent:runner:project",
                    &format!("session-{index:02}"),
                    index as i64,
                    false,
                    false,
                    false,
                )
            })
            .collect(),
        true,
    );
    assert_eq!(recent.returned, HOME_RECENT_SESSION_LIMIT);
    assert_eq!(recent.candidate_count, HOME_RECENT_SESSION_LIMIT + 3);
    assert!(recent.truncated);
    assert!(recent.scan_truncated);

    let runtime = test_runtime();
    let visible = RuntimeConsoleProjects {
        projects: (0..HOME_PROJECT_SCAN_LIMIT)
            .map(|index| RuntimeConsoleProject {
                id: format!("agent:runner:project-{index}"),
                client_id: "runner".to_string(),
                project_ref: None,
                name: Some(format!("Project {index}")),
                path: None,
                registration_source: None,
                lineage: None,
                connected: true,
                runner_status: Some("online".to_string()),
                sessions: None,
            })
            .collect(),
        total: HOME_PROJECT_SCAN_LIMIT + 1,
        truncated: true,
    };
    let scan = scan_runtime_home(&runtime, &visible, &RunningJobSnapshot::default());
    assert_eq!(scan.projects.len(), HOME_PROJECT_SCAN_LIMIT);
    assert_eq!(scan.workflow.projects_scanned, HOME_PROJECT_SCAN_LIMIT);
    assert_eq!(scan.workflow.projects_total, HOME_PROJECT_SCAN_LIMIT + 1);
    assert!(scan.project_scan_truncated);
    assert!(scan.workflow.truncated);
    assert!(scan.recent_sessions.scan_truncated);

    let rows = runner_fleet_rows(
        &serde_json::json!({"runners": [{"client_id": "runner", "connected": true}]}),
        &[],
        &scan,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].projects_scanned, HOME_PROJECT_SCAN_LIMIT);
    assert!(rows[0].projects_scan_partial);
    assert!(rows[0].sessions.sessions_truncated);
    let serialized = serde_json::to_string(&rows[0]).unwrap();
    assert!(!serialized.contains("visible_project_count"));
}

#[test]
fn runtime_home_projects_all_retained_sessions_without_extra_presentation_truncation() {
    let runtime = test_runtime();
    let project_id = "agent:runner:busy";
    for index in 0..HOME_SESSIONS_PER_PROJECT_LIMIT {
        runtime.sessions.start_session(
            Some(project_id.to_string()),
            Some(format!("Session {index}")),
        );
    }
    let visible = RuntimeConsoleProjects {
        projects: vec![RuntimeConsoleProject {
            id: project_id.to_string(),
            client_id: "runner".to_string(),
            project_ref: None,
            name: Some("Busy".to_string()),
            path: Some("/root/git/busy".to_string()),
            registration_source: None,
            lineage: None,
            connected: true,
            runner_status: Some("online".to_string()),
            sessions: None,
        }],
        total: 1,
        truncated: false,
    };
    let scan = scan_runtime_home(&runtime, &visible, &RunningJobSnapshot::default());
    let project_sessions = scan.projects[0].sessions.as_ref().unwrap();
    assert_eq!(
        project_sessions.retained_sessions,
        HOME_SESSIONS_PER_PROJECT_LIMIT
    );
    assert_eq!(
        project_sessions.returned_sessions,
        HOME_SESSIONS_PER_PROJECT_LIMIT
    );
    assert!(!project_sessions.sessions_truncated);

    let rows = runner_fleet_rows(
        &serde_json::json!({"runners": [{"client_id": "runner", "connected": true}]}),
        &[],
        &scan,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].projects_scanned, 1);
    assert!(!rows[0].projects_scan_partial);
    assert!(!rows[0].sessions.sessions_truncated);
}

#[test]
fn runtime_home_runner_fleet_joins_health_build_jobs_projects_and_sessions() {
    let mut sessions = empty_console_aggregate();
    sessions.active_sessions = 2;
    sessions.running_sessions = 1;
    sessions.attention.open_todos = 3;
    let scan = RuntimeConsoleHomeScan {
        workflow: RuntimeConsoleWorkflowAggregate::default(),
        recent_sessions: finalize_recent_sessions(Vec::new(), false),
        projects: Vec::new(),
        runner_sessions: HashMap::from([("runner-a".to_string(), sessions)]),
        runner_projects_scanned: HashMap::from([("runner-a".to_string(), 4)]),
        project_scan_truncated: false,
    };
    let runners = serde_json::json!({
        "runners": [{
            "client_id": "runner-a",
            "connected": true,
            "status": "online",
            "transport": "websocket",
            "runner_protocol_generation": 2,
            "last_seen_age_secs": 2,
            "active_jobs": 3,
            "job_concurrency": {"limit": 8, "running": 2, "queued": 1},
            "build": {"version": "0.3.8", "git_commit": "agent-commit", "git_dirty": false}
        }]
    });
    let status = vec![serde_json::json!({
        "client_id": "runner-a",
        "build_git_commit": "status-commit",
        "build_git_dirty": true,
        "version_matches_server": false,
        "source_alignment": {"status": "different"}
    })];
    let rows = runner_fleet_rows(&runners, &status, &scan);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let serialized = serde_json::to_value(row).unwrap();
    assert_eq!(serialized["runner_protocol_generation"], 2);
    assert!(serialized.get("agent_protocol_generation").is_none());
    assert_eq!(row.client_id, "runner-a");
    assert_eq!(row.active_jobs, 3);
    assert_eq!(row.job_concurrency_limit, Some(8));
    assert_eq!(row.jobs_running, 2);
    assert_eq!(row.jobs_queued, 1);
    assert_eq!(row.projects_scanned, 4);
    assert!(!row.projects_scan_partial);
    assert!(!row.sessions.sessions_truncated);
    assert_eq!(row.sessions.active_sessions, 2);
    assert_eq!(row.sessions.running_sessions, 1);
    assert_eq!(row.sessions.attention.open_todos, 3);
    assert_eq!(row.build_git_commit.as_deref(), Some("status-commit"));
    assert_eq!(row.build_git_dirty, Some(true));
    assert_eq!(row.source_alignment.as_deref(), Some("different"));
    assert_eq!(row.version_matches_server, Some(false));
    assert_eq!(row.transport.as_deref(), Some("websocket"));
    assert_eq!(row.runner_protocol_generation, Some(2));
}

#[tokio::test]
async fn navigation_overview_does_not_project_projects_or_scan_job_history() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("aggregate-home");
    let foreign = crate::auth::shared_key_context("foreign-aggregate");
    register_project(&runtime, "own", "repo", "/private/own", Some(&auth)).await;
    register_project(
        &runtime,
        "foreign",
        "repo",
        "/private/foreign",
        Some(&foreign),
    )
    .await;
    for _ in 0..20 {
        start_authorized_session(&runtime, "agent:own:repo", &auth);
    }
    let before_jobs = runtime
        .runner_registry
        .full_job_history_scan_count_for_test();
    let before_projects = runtime.runner_registry.project_job_scan_count_for_test();
    let result = super::super::overview_primary::primary_for_auth(&runtime, &auth)
        .await
        .unwrap();
    assert_eq!(result.visible_projects, 1);
    assert_eq!(result.visible_project_families, 1);
    assert!(!result.projects_included);
    assert!(result.projects.is_empty());
    assert_eq!(result.runners.len(), 1);
    assert_eq!(result.runners[0].client_id, "own");
    assert_eq!(result.workflow_sessions.projects_scanned, 0);
    assert!(result.workflow_sessions.truncated);
    assert!(result.recent_sessions.sessions.is_empty());
    assert_eq!(
        runtime
            .runner_registry
            .full_job_history_scan_count_for_test(),
        before_jobs
    );
    assert_eq!(
        runtime.runner_registry.project_job_scan_count_for_test(),
        before_projects
    );
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains("/private/"));
    assert!(!encoded.contains("foreign"));
    eprintln!("PRIMARY_OVERVIEW project_rows=0 project_job_scans=0 full_job_scans=0 workflow_projects_scanned=0");
    // Legacy include_sessions=false still includes its explicitly requested rows.
    let legacy = overview_for_auth_detail(&runtime, &auth, false)
        .await
        .unwrap();
    assert!(legacy.projects_included);
    assert_eq!(legacy.projects.len(), 1);
    assert!(
        runtime
            .runner_registry
            .full_job_history_scan_count_for_test()
            > before_jobs
    );
}

#[test]
fn navigation_family_count_uses_explicit_lineage_like_project_view() {
    let row = |id: &str, source: Option<&str>| RuntimeConsoleProject {
        id: format!("agent:r:{id}"),
        client_id: "r".into(),
        project_ref: None,
        name: None,
        path: None,
        registration_source: None,
        lineage: source.map(
            |source| RuntimeConsoleProjectLineage::ManagedWorktreeSource {
                source_project_id: source.into(),
                base_sha: "a".repeat(40),
            },
        ),
        connected: true,
        runner_status: None,
        sessions: None,
    };
    assert_eq!(
        super::super::overview_primary::family_count(&[
            row("repo", None),
            row("wt", Some("repo")),
            row("other", None)
        ]),
        2
    );
}

#[tokio::test]
async fn runtime_home_primary_skips_session_hydration_but_keeps_visibility_and_partial_truth() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("primary-home");
    let foreign = crate::auth::shared_key_context("foreign-home");
    register_project(&runtime, "own", "one", "/private/own", Some(&auth)).await;
    register_project(
        &runtime,
        "foreign",
        "one",
        "/private/foreign",
        Some(&foreign),
    )
    .await;
    start_authorized_session(&runtime, "agent:own:one", &auth);
    let primary = overview_for_auth_detail(&runtime, &auth, false)
        .await
        .unwrap();
    assert_eq!(primary.detail_level, "primary");
    assert_eq!(primary.projects.len(), 1);
    assert_eq!(primary.workflow_sessions.projects_scanned, 0);
    assert!(primary.workflow_sessions.truncated);
    assert!(primary.recent_sessions.sessions.is_empty());
    assert!(primary
        .projects
        .iter()
        .all(|project| project.sessions.is_none()));
    assert!(!serde_json::to_string(&primary)
        .unwrap()
        .contains("/private/foreign"));
    let full = overview_for_auth_detail(&runtime, &auth, true)
        .await
        .unwrap();
    assert_eq!(full.detail_level, "full");
    assert_eq!(full.recent_sessions.sessions.len(), 1);
    assert_eq!(full.workflow_sessions.projects_scanned, 1);
}

#[tokio::test]
async fn runtime_home_projects_and_recent_sessions_span_visible_runners() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("runtime-home-fleet");
    register_project(&runtime, "runner-a", "proj-a", "/private/a", Some(&auth)).await;
    register_project(&runtime, "runner-b", "proj-b", "/private/b", Some(&auth)).await;
    start_authorized_session(&runtime, "agent:runner-a:proj-a", &auth);
    start_authorized_session(&runtime, "agent:runner-b:proj-b", &auth);

    let home = overview_for_auth(&runtime, &auth).await.unwrap();
    let recent_clients = home
        .recent_sessions
        .sessions
        .iter()
        .map(|row| row.client_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        recent_clients,
        std::collections::BTreeSet::from(["runner-a", "runner-b"])
    );
    assert_eq!(home.projects.len(), 2);
    assert!(home
        .projects
        .iter()
        .all(|project| project.sessions.is_some()));
    assert_eq!(home.runners.len(), 2);
    assert_eq!(home.workflow_sessions.projects_scanned, 2);
    assert!(!home.projects_truncated);
    assert!(!home.recent_sessions.scan_truncated);
    let runner_view = runner_for_auth(&runtime, &auth, "runner-a", Some(20))
        .await
        .unwrap();
    assert_eq!(runner_view.recent_sessions.sessions.len(), 1);
    assert_eq!(
        runner_view.recent_sessions.sessions[0].project_id,
        "agent:runner-a:proj-a"
    );
    assert!(!runner_view.recent_sessions.scan_truncated);
}

#[tokio::test]
async fn runtime_home_recent_sessions_follow_project_authority() {
    let runtime = test_runtime();
    let auth_a = crate::auth::shared_key_context("runtime-home-a");
    let auth_b = crate::auth::shared_key_context("runtime-home-b");
    register_project(&runtime, "runner-a", "proj-a", "/private/a", Some(&auth_a)).await;
    register_project(&runtime, "runner-b", "proj-b", "/private/b", Some(&auth_b)).await;
    start_authorized_session(&runtime, "agent:runner-a:proj-a", &auth_a);
    start_authorized_session(&runtime, "agent:runner-b:proj-b", &auth_b);

    let home = overview_for_auth(&runtime, &auth_a).await.unwrap();
    assert_eq!(home.projects.len(), 1);
    assert_eq!(home.projects[0].id, "agent:runner-a:proj-a");
    assert_eq!(home.recent_sessions.sessions.len(), 1);
    assert_eq!(home.recent_sessions.sessions[0].client_id, "runner-a");
    let serialized = serde_json::to_string(&home).unwrap();
    assert!(!serialized.contains("runner-b"));
    assert!(!serialized.contains("agent:runner-b:proj-b"));
    assert!(serialized.contains("/private/a"));
    assert!(!serialized.contains("/private/b"));
}
