use super::*;

    #[tokio::test]
    async fn collaboration_routes_require_runtime_read_before_session_lookup() {
        let runtime = test_runtime();
        let auth = scoped_oauth(&[SCOPE_PROJECT_READ]);
        let error = session_messages_for_auth(
            &runtime,
            &auth,
            WorkflowSessionMessagesInput {
                project: "agent:missing:project".to_string(),
                session_id: "wc_sess_missing000000000".to_string(),
                limit: Some(20),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            RuntimeConsoleError::Request {
                status: 403,
                message: "Runtime read access required",
            }
        );
    }

    #[tokio::test]
    async fn collaboration_mutations_require_session_collaborate_before_session_lookup() {
        let runtime = test_runtime();
        let runtime_read_only = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
        let error = session_post_message_for_auth(
            &runtime,
            &runtime_read_only,
            WorkflowSessionPostMessageInput {
                project: "agent:missing:project".to_string(),
                session_id: "wc_sess_missing000000000".to_string(),
                kind: SessionMessageKind::Guidance,
                priority: SessionMessagePriority::High,
                message: "must not be injected by runtime:read".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            RuntimeConsoleError::Request {
                status: 403,
                message: "Session collaboration access required",
            }
        );
    }

    #[tokio::test]
    async fn collaboration_message_projection_reuses_authority_fence_and_hides_completion_identity()
    {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-group-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-group-b");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);
        let todo = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Todo,
                message: "safe todo body".to_string(),
                tags: vec!["private-tag".to_string()],
                reply_to: None,
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        let assignment_fence = runtime
            .sessions
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap()
            .assignment_fence;
        runtime
            .sessions
            .complete_message(CompleteSessionMessageInput {
                session_id: session.session_id.clone(),
                message_id: todo.message_id,
                answer: "done".to_string(),
                tags: vec!["answer-tag".to_string()],
                priority: SessionMessagePriority::Normal,
                completion_id: "a".repeat(64),
                author_session_id: Some("wc_sess_worker0000000000".to_string()),
                expected_assignment_fence: assignment_fence,
            })
            .unwrap();

        let board = session_messages_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionMessagesInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert_eq!(board.messages.len(), 2);
        let serialized = serde_json::to_string(&board).unwrap();
        assert!(serialized.contains("safe todo body"));
        assert!(serialized.contains("done"));
        assert!(!serialized.contains(&"a".repeat(64)));
        assert!(!serialized.contains("private-tag"));
        assert!(!serialized.contains("answer-tag"));
        assert!(!serialized.contains("completion_id"));
        assert!(!serialized.contains("observation_revision"));

        assert_eq!(
            session_messages_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionMessagesInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    limit: Some(20),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_messages_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionMessagesInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    limit: Some(20),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
    }

    #[tokio::test]
    async fn human_join_reuses_formal_session_authority_and_ack_validation() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-human-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-human-b");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);

        let posted = session_post_message_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionPostMessageInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Guidance,
                priority: SessionMessagePriority::High,
                message: "Please preserve the exact authority fence.".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(posted.kind, "guidance");
        assert_eq!(posted.priority, "high");
        assert!(posted.requires_ack);
        assert!(posted.first_ack_observed_at.is_none());

        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    priority: SessionMessagePriority::Normal,
                    message: "wrong project".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    priority: SessionMessagePriority::Normal,
                    message: "foreign authority".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        let ack_required_note = session_post_message_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionPostMessageInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                priority: SessionMessagePriority::High,
                message: "ack-required note".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(ack_required_note.kind, "note");
        assert_eq!(ack_required_note.priority, "high");
        assert!(ack_required_note.requires_ack);
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Progress,
                    priority: SessionMessagePriority::Normal,
                    message: "progress is not a Human Join kind".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Guidance,
                    priority: SessionMessagePriority::High,
                    message: "x".repeat(8001),
                    reply_to: None,
                    requires_ack: true,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
        #[cfg(feature = "legacy-gpt-actions")]
        {
            let openapi = crate::openapi::build_openapi_spec();
            assert!(openapi["paths"]
                .get("/api/runtime-console/workflow-session-post-message")
                .is_none());
        }
    }

    #[tokio::test]
    async fn browser_message_mutation_routes_succeed_and_retain_history() {
        let runtime = test_runtime();
        let shared_key = "runtime-console-browser-mutate";
        let auth = test_bootstrap_auth();
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let withdraw_target = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "mistyped retained note".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let replace_target = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Question,
                message: "wrong retained question".to_string(),
                tags: Vec::new(),
                reply_to: Some(withdraw_target.message_id.clone()),
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        let (_tmp, service) = hosted_service_with_shared_key(runtime.clone(), shared_key);

        let mut withdrawn = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .bearer_auth(shared_key)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": withdraw_target.message_id,
        }))
        .send(&service)
        .await;
        assert_eq!(withdrawn.status_code, Some(StatusCode::OK));
        let withdrawn_body: Value = withdrawn.take_json().await.unwrap();
        assert_eq!(
            withdrawn_body["message"]["message_id"],
            withdraw_target.message_id
        );
        assert_eq!(withdrawn_body["message"]["status"], "resolved");
        assert_eq!(withdrawn_body["message"]["closure_kind"], "withdrawn");
        assert_eq!(
            withdrawn_body["message"]["message"],
            "mistyped retained note"
        );
        assert_eq!(withdrawn_body["replayed"], false);

        let mut replaced = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-replace-message",
        )
        .bearer_auth(shared_key)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": replace_target.message_id,
            "message": "correct retained question",
        }))
        .send(&service)
        .await;
        assert_eq!(replaced.status_code, Some(StatusCode::OK));
        let replaced_body: Value = replaced.take_json().await.unwrap();
        assert_eq!(
            replaced_body["original"]["message_id"],
            replace_target.message_id
        );
        assert_eq!(
            replaced_body["original"]["message"],
            "wrong retained question"
        );
        assert_eq!(replaced_body["original"]["status"], "resolved");
        assert_eq!(replaced_body["original"]["closure_kind"], "superseded");
        assert_eq!(
            replaced_body["replacement"]["message"],
            "correct retained question"
        );
        assert_eq!(replaced_body["replacement"]["status"], "open");
        assert_eq!(replaced_body["replacement"]["kind"], "question");
        assert_eq!(replaced_body["replacement"]["priority"], "high");
        assert_eq!(
            replaced_body["replacement"]["reply_to"],
            withdraw_target.message_id
        );
        assert_eq!(
            replaced_body["original"]["superseded_by_message_id"],
            replaced_body["replacement"]["message_id"]
        );
        assert_eq!(
            replaced_body["replacement"]["supersedes_message_id"],
            replace_target.message_id
        );
        assert_eq!(replaced_body["replayed"], false);
    }

    #[tokio::test]
    async fn browser_message_mutations_fail_closed_on_authority_and_state_conflicts() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-mutate-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-mutate-b");
        let runtime_read_only = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);
        let note = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "authority target".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();

        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &runtime_read_only,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Request {
                status: 403,
                message: "Session collaboration access required",
            }
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                    message: "foreign edit".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: "wc_sess_missing000000000".to_string(),
                    message_id: "wc_msg_missing000000000".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: "wc_msg_missing000000000".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );

        let risk = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Risk,
                message: "unsupported operator mutation".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: risk.message_id,
                    message: "must stay unsupported".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );

        let todo = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Todo,
                message: "completion wins".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let assignment_fence = runtime
            .sessions
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap()
            .assignment_fence;
        runtime
            .sessions
            .complete_message(CompleteSessionMessageInput {
                session_id: session.session_id.clone(),
                message_id: todo.message_id.clone(),
                answer: "done".to_string(),
                tags: Vec::new(),
                priority: SessionMessagePriority::Normal,
                completion_id: "b".repeat(64),
                author_session_id: None,
                expected_assignment_fence: assignment_fence,
            })
            .unwrap();
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: todo.message_id,
                    message: "too late".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Conflict
        );

        let closed_note = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "closed target".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        runtime.sessions.close_session(&session.session_id).unwrap();
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id,
                    message_id: closed_note.message_id,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Conflict
        );
    }

    #[tokio::test]
    async fn browser_message_mutation_json_and_uncertain_errors_are_distinct() {
        let (_tmp, service) = hosted_service(test_runtime());
        let unknown_field = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .json(&serde_json::json!({
            "project": "agent:missing:project",
            "session_id": "wc_sess_missing000000000",
            "message_id": "wc_msg_missing000000000",
            "unexpected": true,
        }))
        .send(&service)
        .await;
        assert_eq!(unknown_field.status_code, Some(StatusCode::BAD_REQUEST));

        assert_eq!(
            session_message_mutation_error(
                crate::tool_runtime::sessions::SessionMessageError::PersistenceUncertain,
            ),
            RuntimeConsoleError::PersistenceUncertain
        );
        let mut response = Response::new();
        render_error(&mut response, RuntimeConsoleError::PersistenceUncertain);
        assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        let body = response.take_string().await.unwrap();
        assert!(body.contains("Outcome may have happened"));
        assert!(body.contains("refresh retained messages before retrying"));

        #[cfg(feature = "legacy-gpt-actions")]
        {
            let openapi = crate::openapi::build_openapi_spec();
            for id in [
                crate::route_metadata::RouteId::RuntimeConsoleWorkflowSessionWithdrawMessage,
                crate::route_metadata::RouteId::RuntimeConsoleWorkflowSessionReplaceMessage,
            ] {
                let path = crate::route_metadata::path(id);
                assert!(
                    openapi["paths"].get(path).is_none(),
                    "{path} leaked into OpenAPI"
                );
            }
        }
    }

    #[tokio::test]
    async fn browser_message_mutation_persistence_uncertain_is_503_and_retains_live_state() {
        let root = tempfile::tempdir().unwrap();
        let ledger_dir = root.path().join("session-ledger");
        std::fs::create_dir_all(&ledger_dir).unwrap();
        let ledger = ledger_dir.join("sessions.json");
        let mut runtime = ToolRuntime::new(
            Arc::new(crate::RunnerRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        );
        runtime.sessions =
            crate::tool_runtime::sessions::SessionStore::with_persistence(&ledger, 10, 50);
        let runtime = Arc::new(runtime);
        let token = "runtime-console-persistence-uncertain";
        let auth = test_bootstrap_auth();
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let message = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "uncertain withdraw".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        runtime.sessions.flush_persistence();
        std::fs::remove_dir_all(&ledger_dir).unwrap();
        std::fs::write(&ledger_dir, b"block durable ledger recreation").unwrap();
        let (_tmp, service) = hosted_service_with_shared_key(runtime.clone(), token);

        let mut response = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .bearer_auth(token)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": message.message_id,
        }))
        .send(&service)
        .await;
        assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        let body = response.take_string().await.unwrap();
        assert!(body.contains("Outcome may have happened"));
        assert!(body.contains("refresh retained messages before retrying"));

        let retained = runtime
            .sessions
            .list_messages(
                &session.session_id,
                crate::tool_runtime::sessions::ListSessionMessagesFilter {
                    message_id: Some(message.message_id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(
            retained[0].status,
            crate::tool_runtime::sessions::SessionMessageStatus::Resolved
        );
        assert_eq!(
            serde_json::to_value(&retained[0]).unwrap()["closure_kind"],
            "withdrawn"
        );
    }

    #[tokio::test]
    async fn collaboration_observation_route_preserves_baseline_update_timeout_and_paging_semantics(
    ) {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("runtime-console-observe");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);

        let baseline = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: None,
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(!baseline.changed);
        assert!(baseline.messages.is_empty());
        assert!(!baseline.history_lost);
        assert!(!baseline.has_more);

        runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Question,
                message: "first update".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let updated = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(baseline.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(updated.changed);
        assert_eq!(updated.messages.len(), 1);
        assert_eq!(updated.messages[0].message, "first update");

        let timed_out = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(updated.observation_token.clone()),
                wait_secs: Some(1),
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert_eq!(timed_out.wait_outcome, "timeout");
        assert!(!timed_out.changed);

        for body in ["page one", "page two"] {
            runtime
                .sessions
                .post_message(PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Guidance,
                    message: body.to_string(),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: SessionMessagePriority::Normal,
                })
                .unwrap();
        }
        let page_one = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(updated.observation_token),
                wait_secs: None,
                limit: Some(1),
            },
        )
        .await
        .unwrap();
        assert!(page_one.has_more);
        assert_eq!(page_one.messages.len(), 1);
        let page_two = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id,
                after_observation_token: Some(page_one.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(!page_two.has_more);
        assert_eq!(page_two.messages.len(), 1);
    }

    #[tokio::test]
    async fn collaboration_observation_route_surfaces_history_loss_from_authoritative_retention() {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("runtime-console-history-loss");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let baseline = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: None,
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();

        let retention_limit = runtime.sessions.status().max_messages_per_session;
        for index in 0..=retention_limit {
            runtime
                .sessions
                .post_message(PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    message: format!("retention filler {index}"),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: SessionMessagePriority::Normal,
                })
                .unwrap();
        }

        let observed = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id,
                after_observation_token: Some(baseline.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(observed.changed);
        assert!(observed.history_lost);
        assert!(observed.has_more);
        assert_eq!(observed.messages.len(), 100);
    }
