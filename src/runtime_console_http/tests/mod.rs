//! Shared Console fixtures; behavior tests are grouped by domain.
    mod job_queries;

    use super::*;
    use crate::auth::AuthKind;
    use crate::db::{NewGoal, NewGoalStep};
    use crate::runner_protocol::{RunnerCapabilities, RunnerProjectSummary, RunnerRegisterRequest};
    use crate::tool_runtime::sessions::{
        CompleteSessionMessageInput, PostSessionMessageInput, SessionCreateOptions, SessionGuards,
        SessionMessageKind, SessionMessagePriority,
    };
    use crate::tool_runtime::{
        AgentWaitEventSelectorCall, AgentWaitModeCall, RecoveryKind, RuntimeInfo, SessionMode,
        ToolResult,
    };
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use serde_json::json;


mod communication;
mod goals;
mod overview;
mod projects;
mod session_collaboration;
mod sessions;
mod window_activity;
mod window_collaboration;
mod window_visibility;

    fn project(id: &str, private_path: &str) -> RunnerProjectSummary {
        RunnerProjectSummary {
            id: id.to_string(),
            name: Some(format!("Project {id}")),
            path: private_path.to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: Some("private description".to_string()),
            hooks: vec!["private-hook".to_string()],
            disabled: false,
            revision: Some(format!("sha256:{}", "1".repeat(64))),
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: Some("private-shell-profile".to_string()),
        }
    }

    async fn register_project(
        runtime: &ToolRuntime,
        client_id: &str,
        project_id: &str,
        private_path: &str,
        auth: Option<&AuthContext>,
    ) {
        register_project_with_computer_availability(
            runtime,
            client_id,
            project_id,
            private_path,
            auth,
            None,
        )
        .await;
    }

    async fn register_project_with_computer_availability(
        runtime: &ToolRuntime,
        client_id: &str,
        project_id: &str,
        private_path: &str,
        auth: Option<&AuthContext>,
        computer_session_availability: Option<bool>,
    ) {
        let runner_instance_id = format!("inst-{client_id}");
        let access = auth.map(crate::test_support::runner_access);
        runtime
            .runner_registry
            .register_with_auth(
                RunnerRegisterRequest {
                    computer_session_availability,
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    coding_agent_providers: None,
                    coding_agent_inventory: None,
                    client_id: client_id.to_string(),
                    runner_instance_id: runner_instance_id.clone(),
                    runner_protocol_generation:
                        crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                    display_name: Some(format!("Device {client_id}")),
                    owner: auth.and_then(|auth| auth.username.clone()),
                    hostname: Some(format!("private-host-{client_id}")),
                    host_context: None,
                    capabilities: crate::test_support::current_runner_capabilities(
                        RunnerCapabilities::default(),
                    ),
                    policy: None,
                },
                access.as_ref(),
            )
            .await
            .unwrap();
        crate::test_support::apply_project_inventory_snapshot(
            &runtime.runner_registry,
            client_id,
            &runner_instance_id,
            vec![project(project_id, private_path)],
        )
        .await;
    }

    fn test_runtime() -> Arc<ToolRuntime> {
        Arc::new(ToolRuntime::new(
            Arc::new(crate::RunnerRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        ))
    }

    fn test_runtime_with_window_db() -> (tempfile::TempDir, Arc<crate::Database>, Arc<ToolRuntime>)
    {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::Database::open(&tmp.path().join("window-console.db")).unwrap());
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_window_activity_database(db.clone()),
        );
        (tmp, db, runtime)
    }

    fn test_runtime_with_goal_db() -> (tempfile::TempDir, Arc<crate::Database>, Arc<ToolRuntime>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::Database::open(&tmp.path().join("goal-console.db")).unwrap());
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_window_activity_database(db.clone())
            .with_communication_database(db.clone()),
        );
        (tmp, db, runtime)
    }

    fn record_window_event(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        workflow_link: Option<(&str, &str)>,
        at_ms: i64,
    ) {
        record_window_event_with_activity(
            db,
            auth,
            window_key,
            project,
            workflow_link,
            at_ms,
            "workspace_hygiene_check",
            true,
        );
    }

    fn record_window_event_with_activity(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        workflow_link: Option<(&str, &str)>,
        at_ms: i64,
        operation: &str,
        window_meaningful: bool,
    ) {
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
        crate::action_audit_sessions::record_action_event(
            db,
            crate::action_audit_sessions::ActionAuditEventInput {
                explicit_session_id: None,
                session_title: None,
                endpoint: "/mcp".to_string(),
                action_name: "toolsCall".to_string(),
                operation: Some(operation.to_string()),
                project: project.map(str::to_string),
                principal_kind: None,
                principal_user_id: None,
                oauth_client_id: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: at_ms / 1000,
                ended_at: at_ms / 1000,
                duration_ms: 1,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({}),
                request_bytes: None,
                response_bytes: None,
                client_window_key: Some(window_key.to_string()),
                client_window_source: Some("openai-session".to_string()),
                server_trace_id: Some(format!("trace-{at_ms}")),
                principal_correlation_kind: Some(principal_kind),
                principal_correlation_id: Some(principal_id),
                window_started_at_ms: Some(at_ms),
                window_ended_at_ms: Some(at_ms + 1),
                request_observed_at_ms: None,
                response_handed_at_ms: None,
                window_transition_kind: None,
                response_streaming: None,
                window_continuity_eligible: None,
                window_meaningful,
                recorder_gap_session_id: None,
                workflow_links: workflow_link
                    .map(|(session_id, project)| {
                        vec![crate::action_audit_sessions::ActionAuditWorkflowLinkInput {
                            workflow_session_id: session_id.to_string(),
                            relation:
                                crate::action_audit_sessions::WorkflowSessionRelation::Recording,
                            project: Some(project.to_string()),
                        }]
                    })
                    .unwrap_or_default(),
            },
        );
    }

    fn record_timed_window_event(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        request_observed_at_ms: i64,
        response_handed_at_ms: i64,
        legacy_window_ended_at_ms: i64,
        transition: &str,
    ) {
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
        crate::action_audit_sessions::record_action_event(
            db,
            crate::action_audit_sessions::ActionAuditEventInput {
                explicit_session_id: None,
                session_title: None,
                endpoint: "/mcp".to_string(),
                action_name: "toolsCall".to_string(),
                operation: Some("read_files".to_string()),
                project: project.map(str::to_string),
                principal_kind: None,
                principal_user_id: None,
                oauth_client_id: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: request_observed_at_ms / 1000,
                ended_at: legacy_window_ended_at_ms / 1000,
                duration_ms: response_handed_at_ms - request_observed_at_ms,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({}),
                request_bytes: None,
                response_bytes: None,
                client_window_key: Some(window_key.to_string()),
                client_window_source: Some("openai-session".to_string()),
                server_trace_id: Some(format!("timed-trace-{request_observed_at_ms}")),
                principal_correlation_kind: Some(principal_kind),
                principal_correlation_id: Some(principal_id),
                window_started_at_ms: Some(request_observed_at_ms),
                window_ended_at_ms: Some(legacy_window_ended_at_ms),
                request_observed_at_ms: Some(request_observed_at_ms),
                response_handed_at_ms: Some(response_handed_at_ms),
                window_transition_kind: Some(transition.to_string()),
                response_streaming: Some(false),
                window_continuity_eligible: Some(true),
                window_meaningful: true,
                recorder_gap_session_id: None,
                workflow_links: Vec::new(),
            },
        );
    }

    fn scoped_oauth(scopes: &[&str]) -> AuthContext {
        let mut auth = AuthContext::new(AuthKind::OAuth2Token);
        auth.user_id = Some("runtime-console-test-user".to_string());
        auth.username = Some("runtime-console-test-user".to_string());
        auth.scopes = scopes.iter().map(|scope| (*scope).to_string()).collect();
        auth
    }

    fn test_bootstrap_auth() -> AuthContext {
        let mut auth = AuthContext::new(AuthKind::Bootstrap);
        auth.role = Some("admin".to_string());
        auth.is_bootstrap = true;
        auth
    }

    fn start_authorized_session(
        runtime: &ToolRuntime,
        project: &str,
        auth: &AuthContext,
    ) -> crate::tool_runtime::sessions::SessionSummary {
        let fingerprint = crate::tool_runtime::workflow_session_authority_fingerprint(Some(auth))
            .expect("stable test authority");
        runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    Some(project.to_string()),
                    Some("runtime console collaboration".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(fingerprint)),
            )
            .unwrap()
    }

    fn hosted_service(runtime: Arc<ToolRuntime>) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(None);
        let (tmp, db) = crate::test_support::test_db();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    fn hosted_service_with_shared_key(
        runtime: Arc<ToolRuntime>,
        shared_key: &str,
    ) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(Some(shared_key));
        let (tmp, db) = crate::test_support::test_db();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    fn hosted_communication_service(shared_key: &str) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(Some(shared_key));
        let (tmp, db) = crate::test_support::test_db();
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_communication_database(db.clone()),
        );
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    async fn post_communication(
        service: &Service,
        shared_key: &str,
        route: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut response = TestClient::post(format!(
            "http://localhost/api/runtime-console/communication/{route}"
        ))
        .bearer_auth(shared_key)
        .json(&body)
        .send(service)
        .await;
        let status = response.status_code.unwrap_or(StatusCode::OK);
        let body = response.take_json::<Value>().await.unwrap_or_default();
        (status, body)
    }

    fn recent_test_row(
        client_id: &str,
        project_id: &str,
        session_id: &str,
        updated_at: i64,
        running: bool,
        attention: bool,
        active: bool,
    ) -> RuntimeConsoleRecentSession {
        let runtime = test_runtime();
        runtime.sessions.start_session(
            Some(project_id.to_string()),
            Some(format!("Session {session_id}")),
        );
        let mut session = runtime
            .workflow_sessions_console_list(project_id, Some(1))
            .sessions
            .remove(0);
        session.session_id = session_id.to_string();
        session.updated_at = updated_at;
        session.running_call = running;
        session.lifecycle = if active { "active" } else { "closed" }.to_string();
        session.overview.attention.open_todos = usize::from(attention);
        RuntimeConsoleRecentSession {
            client_id: client_id.to_string(),
            project_id: project_id.to_string(),
            project_name: Some(format!("Project {project_id}")),
            session,
        }
    }

    async fn window_activity_lookup_is_runtime_management_and_current_project_authority_bounded_body(
    ) {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth_a = crate::auth::shared_key_context("window-group-a");
        let auth_b = crate::auth::shared_key_context("window-group-b");
        let project_a = "agent:window-a:proj-a";
        let project_b = "agent:window-b:proj-b";
        register_project(
            &runtime,
            "window-a",
            "proj-a",
            "/private/window-a",
            Some(&auth_a),
        )
        .await;
        register_project(
            &runtime,
            "window-b",
            "proj-b",
            "/private/window-b",
            Some(&auth_b),
        )
        .await;

        let window_a = "a".repeat(64);
        let window_b = "b".repeat(64);
        let revoked_project_window = "c".repeat(64);
        let revoked_session_window = "d".repeat(64);
        let foreign_unscoped_window = "e".repeat(64);
        record_window_event(&db, &auth_a, &window_a, Some(project_a), None, 1_000);
        record_window_event(&db, &auth_b, &window_b, Some(project_b), None, 2_000);
        // Model a historical event that was legitimate for this principal before
        // a Project grant was revoked. The current registry intentionally exposes
        // project_b only to auth_b, so auth_a must not retain Window metadata for it.
        record_window_event(
            &db,
            &auth_a,
            &revoked_project_window,
            Some(project_b),
            None,
            3_000,
        );
        // Session/collaboration events can be projectless at the business-call
        // layer while their authoritative Workflow link carries the Project.
        // That link must still enforce current Project visibility.
        record_window_event(
            &db,
            &auth_a,
            &revoked_session_window,
            None,
            Some(("wc_sess_hidden", project_b)),
            4_000,
        );
        // Cross-credential management discovery must not turn a projectless
        // historical event from another principal into shared runtime evidence.
        record_window_event(&db, &auth_b, &foreign_unscoped_window, None, None, 4_500);

        // A meaningful tools/call is not exposed to a non-admin during the
        // tiny pre-resolution interval where its exact Project is not known yet.
        let pre_resolution_window =
            crate::client_window::ClientWindow::for_test("runtime-console-pre-resolution-hidden");
        let pre_resolution_key = pre_resolution_window.key().to_string();
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(&auth_a)).unwrap();
        let _pre_resolution = runtime.window_activity.start(
            &pre_resolution_window,
            "trace-pre-resolution",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution",
            Some("workspace_hygiene_check"),
            None,
        );

        // Presentation is not visibility authority. observe_jobs is Transport
        // presentation but still Meaningful interaction, so it must fail closed
        // during the same unresolved-Project interval.
        let transport_window = crate::client_window::ClientWindow::for_test(
            "runtime-console-pre-resolution-transport",
        );
        let transport_key = transport_window.key().to_string();
        let _transport = runtime.window_activity.start(
            &transport_window,
            "trace-pre-resolution-transport",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution-transport",
            Some("observe_jobs"),
            None,
        );

        // NonMeaningful controller/status traffic keeps the existing bounded
        // diagnostic visibility before exact Project resolution.
        let diagnostic_window = crate::client_window::ClientWindow::for_test(
            "runtime-console-pre-resolution-diagnostic",
        );
        let diagnostic_key = diagnostic_window.key().to_string();
        let _diagnostic = runtime.window_activity.start(
            &diagnostic_window,
            "trace-pre-resolution-diagnostic",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution-diagnostic",
            Some("goal_plan_sync"),
            None,
        );

        let visible = windows_for_auth(&runtime, &auth_a, Some(20), None)
            .await
            .unwrap();
        assert_eq!(visible.total, 2);
        assert_eq!(visible.returned, 2);
        let visible_keys = visible
            .windows
            .iter()
            .map(|row| row.client_window_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            visible_keys,
            std::collections::BTreeSet::from([window_a.as_str(), diagnostic_key.as_str()])
        );
        assert_eq!(
            visible.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        let serialized = serde_json::to_string(&visible).unwrap();
        assert!(!serialized.contains(&window_b));
        assert!(!serialized.contains(&revoked_project_window));
        assert!(!serialized.contains(&revoked_session_window));
        assert!(!serialized.contains(&foreign_unscoped_window));
        assert!(!serialized.contains(&pre_resolution_key));
        assert!(!serialized.contains(&transport_key));
        assert!(serialized.contains(&diagnostic_key));
        assert!(!serialized.contains(project_b));
        assert!(serialized.contains("\"scope\":\"global\""));

        let own = window_for_auth(
            &runtime,
            &auth_a,
            WindowInput {
                client_window_key: window_a.clone(),
                activity_limit: Some(20),
                session_limit: Some(20),
                detail_level: WindowDetailLevel::Full,
            },
        )
        .await
        .unwrap();
        assert_eq!(own.client_window_key, window_a);
        assert_eq!(own.last_seen_at_ms, 1_001);
        assert_eq!(
            own.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );

        for hidden_key in [
            &window_b,
            &revoked_project_window,
            &revoked_session_window,
            &foreign_unscoped_window,
            &pre_resolution_key,
            &transport_key,
        ] {
            assert_eq!(
                window_for_auth(
                    &runtime,
                    &auth_a,
                    WindowInput {
                        client_window_key: hidden_key.clone(),
                        activity_limit: Some(20),
                        session_limit: Some(20),
                        detail_level: WindowDetailLevel::Full,
                    },
                )
                .await
                .unwrap_err(),
                RuntimeConsoleError::NotFound
            );
        }

        let admin = test_bootstrap_auth();
        let global = windows_for_auth(&runtime, &admin, Some(20), None)
            .await
            .unwrap();
        assert_eq!(
            global.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        let global_serialized = serde_json::to_string(&global).unwrap();
        assert!(global_serialized.contains("\"scope\":\"global\""));
        let keys = global
            .windows
            .iter()
            .map(|row| row.client_window_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            keys,
            std::collections::BTreeSet::from([
                window_a.as_str(),
                window_b.as_str(),
                revoked_project_window.as_str(),
                revoked_session_window.as_str(),
                foreign_unscoped_window.as_str(),
                pre_resolution_key.as_str(),
                transport_key.as_str(),
                diagnostic_key.as_str(),
            ])
        );
    }

