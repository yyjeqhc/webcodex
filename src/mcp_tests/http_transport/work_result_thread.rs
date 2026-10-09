use super::*;

#[test]
fn work_result_thread_panel_reuses_the_http_presentation_binding() {
    std::thread::Builder::new()
        .name("mcp-work-result-thread".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(exercise())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn call(
    service: &Service,
    name: &str,
    arguments: Value,
    window: &str,
) -> (StatusCode, Value) {
    call_with_ui_capabilities(service, name, arguments, window, true).await
}

async fn call_with_ui_capabilities(
    service: &Service,
    name: &str,
    arguments: Value,
    window: &str,
    ui_capabilities: bool,
) -> (StatusCode, Value) {
    let mut params = mcp_2026_ui_params(json!({"name": name, "arguments": arguments}));
    if !ui_capabilities {
        params["_meta"]["io.modelcontextprotocol/clientCapabilities"] = json!({});
    }
    params["_meta"]["openai/session"] = json!(window);
    stateless_2026_jsonrpc(
        service,
        "thread-panel-secret",
        Some(MCP_STATELESS_PROTOCOL_VERSION),
        Some("tools/call"),
        Some(name),
        None,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params}),
    )
    .await
}

async fn exercise() {
    let config = test_config(Some("thread-panel-secret"));
    let (_temp, db) = test_db();
    let registry = stateless_observation_runner_registry().await;
    let executor = spawn_stateless_observation_agent_executor(registry.clone());
    let runtime = Arc::new(
        ToolRuntime::new(
            registry,
            Arc::new(crate::tool_runtime::RuntimeInfo {
                mcp_apps_enabled: true,
                ..Default::default()
            }),
        )
        .with_window_activity_database(db.clone()),
    );
    let project = "agent:mcp-observation-agent:shared";
    let window = "thread-panel-host-window";
    let session = start_mcp_fixture_session(&runtime, Some(project), "Thread panel binding");
    let session_events = runtime
        .sessions
        .summary(&session, None)
        .unwrap()
        .events_total;
    let service = Service::new(build_test_router(config, db.clone(), runtime.clone()));

    // A newly opened conversation has no presentation binding. Opening its
    // panel must not borrow a Project/Session or create any work evidence.
    for ui_capabilities in [true, false] {
        let (status, empty) = call_with_ui_capabilities(
            &service,
            "work_result_thread_panel",
            json!({}),
            window,
            ui_capabilities,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{empty}");
        assert_eq!(
            empty["result"]["structuredContent"],
            json!({"success": true, "output": {"work_result": null}, "error": null})
        );
        assert_eq!(
            empty["result"]["_meta"][super::super::super::tools::WORK_RESULT_APP_RESULT_META_KEY],
            empty["result"]["structuredContent"]
        );
        assert_eq!(
            empty["result"]["_meta"]
                [super::super::super::tools::WORK_RESULT_THREAD_CONTEXT_META_KEY],
            json!({"empty": true, "session_id": null})
        );
    }
    let identity =
        crate::client_window::ClientWindow::from_opaque("openai-session", window).unwrap();
    assert!(db
        .list_window_activity_events(identity.key(), None, 20)
        .unwrap()
        .is_empty());

    for session_id in [None, Some(session.as_str())] {
        let mut arguments = json!({"project": project});
        if let Some(session_id) = session_id {
            arguments["session_id"] = json!(session_id);
        }
        let (status, presented) = call(&service, "present_work_result", arguments, window).await;
        assert_eq!(status, StatusCode::OK, "{presented}");
        assert_eq!(presented["result"]["structuredContent"]["success"], true);

        // ChatGPT can omit the capabilities advertised during discovery when
        // invoking the already admitted native thread entrypoint.
        for ui_capabilities in [true, false] {
            let (status, opened) = call_with_ui_capabilities(
                &service,
                "work_result_thread_panel",
                json!({}),
                window,
                ui_capabilities,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{opened}");
            assert_eq!(opened["result"]["structuredContent"]["success"], true);
            assert!(
                !opened["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .trim_start()
                    .starts_with('{'),
                "Public thread rendering must keep model text compact"
            );
            let output = &stateless_tool_output(&opened)["work_result"];
            assert_eq!(output["project"], project);
            assert_eq!(output["session_id"].as_str(), session_id);
            assert_eq!(
                opened["result"]["_meta"]
                    [super::super::super::tools::WORK_RESULT_APP_RESULT_META_KEY],
                opened["result"]["structuredContent"]
            );
            assert_eq!(
                opened["result"]["_meta"]
                    [super::super::super::tools::WORK_RESULT_THREAD_CONTEXT_META_KEY],
                json!({"session_id": session_id})
            );
        }
    }

    let (status, failed) = call(
        &service,
        "present_work_result",
        json!({"project": "agent:missing:project"}),
        window,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["result"]["structuredContent"]["success"], false);
    let (status, opened) = call(&service, "work_result_thread_panel", json!({}), window).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(
        stateless_tool_output(&opened)["work_result"]["project"],
        project
    );
    assert_eq!(
        stateless_tool_output(&opened)["work_result"]["session_id"],
        session
    );

    let (status, other_window) = call(
        &service,
        "work_result_thread_panel",
        json!({}),
        "another-host-window",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{other_window}");
    assert_eq!(
        other_window["result"]["structuredContent"]["output"],
        json!({"work_result": null}),
        "A different Window must not inherit the presented Project or Session"
    );
    assert_eq!(
        other_window["result"]["_meta"]
            [super::super::super::tools::WORK_RESULT_THREAD_CONTEXT_META_KEY],
        json!({"empty": true, "session_id": null})
    );
    let (status, overridden) = call(
        &service,
        "work_result_thread_panel",
        json!({"project": project}),
        window,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{overridden}");

    let identity =
        crate::client_window::ClientWindow::from_opaque("openai-session", window).unwrap();
    let events = db
        .list_window_activity_events(identity.key(), None, 20)
        .unwrap();
    assert_eq!(events.len(), 3, "App reads must not create Window activity");
    let bound = events
        .iter()
        .find(|event| event.status == "success")
        .unwrap();
    assert_eq!(bound.operation.as_deref(), Some("present_work_result"));
    assert_eq!(bound.business_session_id.as_deref(), Some(session.as_str()));
    assert_eq!(
        runtime
            .sessions
            .summary(&session, None)
            .unwrap()
            .events_total,
        session_events,
        "Presentation binding must not become Session work evidence"
    );

    executor.abort();
}
