use super::*;

    #[tokio::test]
    async fn runtime_console_preserves_browser_same_origin_and_json_errors() {
        let (_tmp, service) = hosted_service(test_runtime());
        let cross_origin = TestClient::post("http://localhost/api/runtime-console/projects")
            .add_header("host", "localhost", true)
            .add_header("origin", "http://attacker.example", true)
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));

        let unsupported = TestClient::post("http://localhost/api/runtime-console/projects")
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

