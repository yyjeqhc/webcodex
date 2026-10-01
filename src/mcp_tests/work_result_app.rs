use super::*;

fn tool<'a>(payload: &'a Value, name: &str) -> Option<&'a Value> {
    payload["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}

async fn handle_with_server_apps_enabled(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&crate::auth::AuthContext>,
    enabled: bool,
) -> McpOutcome {
    let protocol_era = super::super::inferred_protocol_era(&request);
    super::super::handle_mcp_request_with_lifecycle(
        runtime,
        request,
        auth,
        protocol_era,
        super::super::HostFileImportTrust::Untrusted,
        None,
        None,
        None,
        crate::model_surface::effective_mcp_compact_schemas(
            crate::config::mcp_compact_schemas_override(),
        ),
        enabled,
        None,
    )
    .await
}

#[tokio::test]
async fn work_result_descriptor_is_explicit_sparse_app_only_and_resource_backed() {
    assert_eq!(
        MCP_WORK_RESULT_UI_RESOURCE_URI,
        "ui://webcodex/work-result/v14"
    );
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v9"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v10"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v11"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v4"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v5"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v6"));
    assert!(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS.contains(&"ui://webcodex/work-result/v7"));
    let runtime = test_runtime();

    let ui = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/list",
            Some(json!(5101)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(ui) = ui else {
        panic!("expected UI-capable adaptive tools/list");
    };
    assert!(tool(&ui["result"], "present_changes").is_none());
    let present = tool(&ui["result"], "present_work_result").expect("present_work_result");
    let diff = tool(&ui["result"], "changes_file_diff").expect("Work Result lazy diff");
    assert_eq!(diff.pointer("/_meta/ui/visibility"), Some(&json!(["app"])));
    assert!(diff.pointer("/_meta/ui/resourceUri").is_none());
    assert_eq!(
        diff["inputSchema"]["required"],
        json!(["project", "session_id", "snapshot_id", "path"])
    );
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "changes_file_diff"));
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "changes_file_diff",
            true
        )
    );
    assert_eq!(
        present.pointer("/_meta/ui/resourceUri"),
        Some(&json!(MCP_WORK_RESULT_UI_RESOURCE_URI))
    );
    assert!(present.pointer("/_meta/ui/visibility").is_none());
    assert_eq!(present["inputSchema"]["required"], json!(["project"]));
    let state = tool(&ui["result"], "work_result_state").expect("app-only work_result_state");
    assert_eq!(state.pointer("/_meta/ui/visibility"), Some(&json!(["app"])));
    assert!(state.pointer("/_meta/ui/resourceUri").is_none());
    assert_eq!(state["inputSchema"]["required"], json!(["project"]));
    let activity_detail = tool(&ui["result"], "work_result_activity_detail")
        .expect("app-only work_result_activity_detail");
    assert_eq!(
        activity_detail.pointer("/_meta/ui/visibility"),
        Some(&json!(["app"]))
    );
    assert!(activity_detail.pointer("/_meta/ui/resourceUri").is_none());
    assert_eq!(
        activity_detail["inputSchema"]["required"],
        json!(["project", "server_trace_id"])
    );
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "work_result_activity_detail"));
    let send =
        tool(&ui["result"], "work_result_send_message").expect("app-only work_result_send_message");
    assert_eq!(send.pointer("/_meta/ui/visibility"), Some(&json!(["app"])));
    assert!(send.pointer("/_meta/ui/resourceUri").is_none());
    assert_eq!(
        send["inputSchema"]["required"],
        json!(["project", "message", "delivery_key"])
    );
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "work_result_send_message"));
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "work_result_send_message",
            true
        )
    );

    let full = test_runtime();
    let full_ui = handle_with_server_apps_enabled(
        &full,
        rpc(
            "tools/list",
            Some(json!(5104)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(full_ui) = full_ui else {
        panic!("expected UI-capable full tools/list");
    };
    for descriptor in full_ui["result"]["tools"].as_array().unwrap() {
        if descriptor["name"] == "present_work_result" {
            continue;
        }
        assert_ne!(
            descriptor
                .pointer("/_meta/ui/resourceUri")
                .and_then(Value::as_str),
            Some(MCP_WORK_RESULT_UI_RESOURCE_URI),
            "only present_work_result may create a Work Result card"
        );
    }
    assert_eq!(
        tool(&full_ui["result"], "present_goal_plan")
            .unwrap()
            .pointer("/_meta/ui/resourceUri"),
        Some(&json!(MCP_GOAL_PLAN_UI_RESOURCE_URI))
    );
    assert!(tool(&full_ui["result"], "present_agent_continuation").is_none());

    let plain = handle_with_server_apps_enabled(
        &runtime,
        rpc("tools/list", Some(json!(5102)), mcp_2026_params(json!({}))),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(plain) = plain else {
        panic!("ordinary tools/list failed");
    };
    assert!(tool(&plain["result"], "present_work_result").is_some());
    assert!(tool(&plain["result"], "present_work_result")
        .unwrap()
        .pointer("/_meta/ui/resourceUri")
        .is_none());
    assert!(tool(&plain["result"], "work_result_state").is_none());
    assert!(tool(&plain["result"], "work_result_send_message").is_none());
    assert!(tool(&plain["result"], "changes_file_diff").is_none());
    assert!(tool(&plain["result"], "present_changes").is_none());

    let disabled = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/list",
            Some(json!(5103)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
        false,
    )
    .await;
    let McpOutcome::Ok(disabled) = disabled else {
        panic!("Apps-disabled tools/list failed");
    };
    assert!(tool(&disabled["result"], "work_result_state").is_none());
    assert!(tool(&disabled["result"], "work_result_activity_detail").is_none());
    assert!(tool(&disabled["result"], "work_result_send_message").is_none());
    assert!(tool(&disabled["result"], "changes_file_diff").is_none());
    assert!(tool(&disabled["result"], "present_work_result")
        .unwrap()
        .pointer("/_meta/ui/resourceUri")
        .is_none());

    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "work_result_state"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "work_result_activity_detail"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "work_result_send_message"));
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "work_result_state",
            true
        )
    );
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "work_result_activity_detail",
            true
        )
    );
}

#[tokio::test]
async fn present_work_result_keeps_model_text_compact_and_private_view_envelope() {
    let runtime = test_runtime();
    let outcome = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5105)),
            mcp_2026_ui_params(json!({
                "name": "present_work_result",
                "arguments": {
                    "project": "agent:missing:project",
                    "session_id": format!("wc_sess_{}", "1".repeat(32))
                }
            })),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(outcome) = outcome else {
        panic!("present_work_result should return a canonical ToolResult");
    };
    let result = &outcome["result"];
    assert_eq!(
        result["_meta"][super::super::tools::WORK_RESULT_APP_RESULT_META_KEY],
        result["structuredContent"]
    );
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(
        !text.trim_start().starts_with('{'),
        "model-visible presentation text must stay compact"
    );
}

#[tokio::test]
async fn work_result_resource_is_canonical_while_changes_resources_are_hidden_compatibility() {
    const PUBLIC_URL: &str = "https://self-host.example";
    let runtime = test_runtime_with_public_url(PUBLIC_URL);
    let resources = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "resources/list",
            Some(json!(5110)),
            mcp_2026_ui_params(json!({})),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(resources) = resources else {
        panic!("resources/list failed");
    };
    let resources = resources["result"]["resources"].as_array().unwrap();
    assert!(resources
        .iter()
        .any(|resource| resource["uri"] == MCP_WORK_RESULT_UI_RESOURCE_URI));
    let work_resource = resources
        .iter()
        .find(|resource| resource["uri"] == MCP_WORK_RESULT_UI_RESOURCE_URI)
        .expect("canonical Work Result resource");
    let work_description = work_resource["description"].as_str().unwrap();
    assert!(work_description.contains("client Window"));
    assert!(work_description.contains("Window ActionAudit activity"));
    assert!(work_description.contains("observe/diagnostic"));
    assert!(work_description.contains("optional linked evidence"));
    assert!(!resources
        .iter()
        .any(|resource| resource["uri"] == MCP_RESULT_UI_RESOURCE_URI));
    for legacy in MCP_RESULT_UI_RESOURCE_LEGACY_URIS
        .iter()
        .chain(MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS)
    {
        assert!(!resources.iter().any(|resource| resource["uri"] == *legacy));
    }
    let read = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(5111)),
            mcp_2026_ui_params(json!({"uri": MCP_WORK_RESULT_UI_RESOURCE_URI})),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(read) = read else {
        panic!("Work resource read failed");
    };
    assert_eq!(
        read["result"]["contents"][0]["text"],
        MCP_WORK_RESULT_APP_HTML
    );
    assert_eq!(
        read["result"]["contents"][0]["_meta"]["ui"]["domain"],
        PUBLIC_URL
    );
    for legacy in MCP_WORK_RESULT_UI_RESOURCE_LEGACY_URIS {
        let alias = handle_with_server_apps_enabled(
            &runtime,
            rpc(
                "resources/read",
                Some(json!(5112)),
                mcp_2026_ui_params(json!({"uri": legacy})),
            ),
            None,
            true,
        )
        .await;
        let McpOutcome::Ok(alias) = alias else {
            panic!("cached descriptor read failed");
        };
        assert_eq!(alias["result"]["contents"][0]["uri"], *legacy);
        assert_eq!(
            alias["result"]["contents"][0]["text"],
            MCP_WORK_RESULT_APP_HTML
        );
    }
}

#[tokio::test]
async fn work_result_state_call_requires_app_protocol_capability() {
    let runtime = test_runtime();
    let args = json!({
        "name": "work_result_state",
        "arguments": {
            "project": "agent:missing:project",
            "session_id": format!("wc_sess_{}", "1".repeat(32))
        }
    });
    let app = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5120)),
            mcp_2026_ui_params(args.clone()),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(app) = app else {
        panic!("App-only state call should reach runtime under App capability");
    };
    assert_eq!(app["result"]["structuredContent"]["success"], false);
    let fallback: Value = serde_json::from_str(
        app["result"]["content"][0]["text"]
            .as_str()
            .expect("Work Result state content fallback"),
    )
    .unwrap();
    assert_eq!(fallback, app["result"]["structuredContent"]);

    for params in [mcp_2026_params(args.clone()), mcp_2026_ui_params(args)] {
        let outcome = handle_with_server_apps_enabled(
            &runtime,
            rpc("tools/call", Some(json!(5121)), params),
            None,
            false,
        )
        .await;
        assert!(matches!(outcome, McpOutcome::BadRequest(_)));
    }
}

#[tokio::test]
async fn work_result_send_message_requires_app_protocol_capability() {
    let runtime = test_runtime();
    let args = json!({
        "name": "work_result_send_message",
        "arguments": {
            "project": "agent:missing:project",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "message": "hello from the card",
            "delivery_key": "work-result-card-test"
        }
    });
    let app = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5123)),
            mcp_2026_ui_params(args.clone()),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(app) = app else {
        panic!("App-only collaboration call should reach runtime under App capability");
    };
    assert_eq!(app["result"]["structuredContent"]["success"], false);
    let fallback: Value = serde_json::from_str(
        app["result"]["content"][0]["text"]
            .as_str()
            .expect("Work Result message content fallback"),
    )
    .unwrap();
    assert_eq!(fallback, app["result"]["structuredContent"]);

    for params in [mcp_2026_params(args.clone()), mcp_2026_ui_params(args)] {
        let outcome = handle_with_server_apps_enabled(
            &runtime,
            rpc("tools/call", Some(json!(5124)), params),
            None,
            false,
        )
        .await;
        assert!(matches!(outcome, McpOutcome::BadRequest(_)));
    }
}

#[tokio::test]
async fn work_result_state_discards_unadvertised_recording_session_wrapper() {
    let runtime = test_runtime();
    let project = "agent:missing:work-result".to_string();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Work Result wrapper suppression".to_string()),
    );
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();

    let outcome = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5122)),
            mcp_2026_ui_params(json!({
                "name": "work_result_state",
                "arguments": {
                    "project": project,
                    "session_id": session.session_id,
                    "recording_session_id": session.session_id
                }
            })),
        ),
        None,
        true,
    )
    .await;
    assert!(matches!(
        outcome,
        McpOutcome::Ok(_) | McpOutcome::BadRequest(_)
    ));

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.updated_at, before.updated_at);
}

#[test]
fn work_result_html_is_bounded_live_progress_ui() {
    for required in [
        "work_result_state",
        "changes_file_diff",
        "work_result_send_message",
        "wc_changes_snapshot_",
        "Window activity",
        "id=\"windowIdentity\"",
        "Project · ",
        "Session · ",
        "Calls in this chat, earliest first.",
        "Activity",
        "Collaboration",
        "Final changes",
        "id=\"tabResults\"",
        "id=\"workspaceFiles\"",
        "Workspace changes",
        "Checks and review",
        "Message this Window",
        "No messages yet",
        "Acknowledged",
        "Included in tool result",
        "ui/notifications/tool-input",
        "ui/notifications/tool-result",
        "id=\"refresh\"",
        "Refreshing…",
        "ui/resource-teardown",
        "state_version",
        "pagehide",
        "beforeunload",
        "Current activity",
        "visibilitychange",
        "VISIBLE_REFRESH_MS",
        "VISIBLE_IDLE_REFRESH_MS",
        "Observe · ",
        "async_job_id",
        "observed_job_ids",
    ] {
        assert!(
            MCP_WORK_RESULT_APP_HTML.contains(required),
            "missing {required}"
        );
    }
    assert!(MCP_WORK_RESULT_APP_HTML.contains("const VISIBLE_REFRESH_MS = 10000;"));
    assert!(MCP_WORK_RESULT_APP_HTML.contains("const VISIBLE_IDLE_REFRESH_MS = 30000;"));
    assert!(MCP_WORK_RESULT_APP_HTML.contains("lastStateVersion === null || document.hidden"));
    assert!(MCP_WORK_RESULT_APP_HTML.contains("if (document.hidden) {"));
    assert!(!MCP_WORK_RESULT_APP_HTML.contains("HIDDEN_REFRESH_MS"));
    assert!(!MCP_WORK_RESULT_APP_HTML.contains("HIDDEN_IDLE_REFRESH_MS"));
    assert!(
        MCP_WORK_RESULT_APP_HTML
            .contains("scrollbar-gutter: stable; box-sizing: border-box; padding-right: 10px"),
        "scrolling Work Result regions must reserve content space beside the scrollbar"
    );
    assert!(
        MCP_WORK_RESULT_APP_HTML.contains("Changed content"),
        "live workspace file contents should be rendered in Results"
    );
    // Output export is an explicit user action, covered by executable App tests;
    // refresh and mounting still never send an automatic chat message.
    assert!(MCP_WORK_RESULT_APP_HTML.contains("Get file in chat"));
    assert!(MCP_WORK_RESULT_APP_HTML.contains("First check that its current SHA-256"));
    for forbidden in [
        "Linked work conversation",
        "No linked work conversation",
        "A linked Workflow Session has not appeared",
        "Task workflow",
        "Result · Ready",
        "setInterval",
        "clearInterval",
        "POLL_MS",
        "pollTimer",
        "localStorage",
        "indexedDB",
        "fetch(",
        "WebSocket",
        "continuationToken",
        "authority_fingerprint",
        "baseline_tree",
        "final_tree",
    ] {
        assert!(
            !MCP_WORK_RESULT_APP_HTML.contains(forbidden),
            "Work Result App contains forbidden marker {forbidden}"
        );
    }
}

#[tokio::test]
async fn work_result_activity_detail_call_requires_app_protocol_capability() {
    let runtime = test_runtime();
    let args = json!({
        "name": "work_result_activity_detail",
        "arguments": {
            "project": "agent:missing:project",
            "server_trace_id": "trace-window-detail"
        }
    });
    let app = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5219)),
            mcp_2026_ui_params(args.clone()),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(app) = app else {
        panic!("App-only activity detail should reach runtime under App capability");
    };
    assert_eq!(app["result"]["structuredContent"]["success"], false);
    let fallback: Value = serde_json::from_str(
        app["result"]["content"][0]["text"]
            .as_str()
            .expect("Work Result activity detail content fallback"),
    )
    .unwrap();
    assert_eq!(fallback, app["result"]["structuredContent"]);

    for params in [mcp_2026_params(args.clone()), mcp_2026_ui_params(args)] {
        let outcome = handle_with_server_apps_enabled(
            &runtime,
            rpc("tools/call", Some(json!(5220)), params),
            None,
            false,
        )
        .await;
        assert!(matches!(outcome, McpOutcome::BadRequest(_)));
    }
}

#[tokio::test]
async fn changes_file_diff_call_requires_app_protocol_capability() {
    let runtime = test_runtime();
    let args = json!({
        "name": "changes_file_diff",
        "arguments": {
            "project": "agent:missing:project",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "snapshot_id": format!("wc_changes_snapshot_{}", "2".repeat(32)),
            "path": "src/lib.rs"
        }
    });
    let app = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5220)),
            mcp_2026_ui_params(args.clone()),
        ),
        None,
        true,
    )
    .await;
    let McpOutcome::Ok(app) = app else {
        panic!("App-only diff call should reach runtime under App capability");
    };
    assert_eq!(app["result"]["structuredContent"]["success"], false);
    let fallback: Value = serde_json::from_str(
        app["result"]["content"][0]["text"]
            .as_str()
            .expect("Work Result diff content fallback"),
    )
    .unwrap();
    assert_eq!(fallback, app["result"]["structuredContent"]);

    for params in [mcp_2026_params(args.clone()), mcp_2026_ui_params(args)] {
        let outcome = handle_with_server_apps_enabled(
            &runtime,
            rpc("tools/call", Some(json!(5221)), params),
            None,
            false,
        )
        .await;
        assert!(matches!(outcome, McpOutcome::BadRequest(_)));
    }
}

#[tokio::test]
async fn changes_file_diff_discards_unadvertised_recording_session_wrapper() {
    let runtime = test_runtime();
    let project = "agent:missing:changes".to_string();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Changes wrapper suppression".to_string()),
    );
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();

    let outcome = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(5222)),
            mcp_2026_ui_params(json!({
                "name": "changes_file_diff",
                "arguments": {
                    "project": project,
                    "session_id": session.session_id,
                    "snapshot_id": format!("wc_changes_snapshot_{}", "3".repeat(32)),
                    "path": "src/lib.rs",
                    "recording_session_id": session.session_id
                }
            })),
        ),
        None,
        true,
    )
    .await;
    assert!(matches!(
        outcome,
        McpOutcome::Ok(_) | McpOutcome::BadRequest(_)
    ));

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.updated_at, before.updated_at);
}

/// Proves the `RuntimeInfo.mcp_apps_enabled` startup snapshot — not ambient env —
/// drives the real MCP request adapter. Both runtimes are built via
/// `test_runtime_with_mcp_settings` with explicit snapshots; the identical
/// UI-capable `tools/list` request is served through the production
/// `handle_mcp_request` path, which reads the snapshot rather than the env.
#[tokio::test]
async fn mcp_apps_enabled_snapshot_drives_real_request_adapter() {
    let runtime_off = test_runtime_with_mcp_settings(true, false);
    let runtime_on = test_runtime_with_mcp_settings(true, true);

    let request = || {
        rpc(
            "tools/list",
            Some(json!(5110)),
            mcp_2026_ui_params(json!({})),
        )
    };

    let off = handle_mcp_request(&runtime_off, request(), None).await;
    let McpOutcome::Ok(off) = off else {
        panic!("Apps-OFF tools/list failed");
    };
    let off_present =
        tool(&off["result"], "present_work_result").expect("present_work_result (OFF)");
    // The snapshot is OFF: no UI resource backing on the app tool.
    assert!(off_present.pointer("/_meta/ui/resourceUri").is_none());
    // App-only tools are absent when the snapshot disables MCP Apps.
    assert!(tool(&off["result"], "work_result_state").is_none());

    let on = handle_mcp_request(&runtime_on, request(), None).await;
    let McpOutcome::Ok(on) = on else {
        panic!("Apps-ON tools/list failed");
    };
    let on_present = tool(&on["result"], "present_work_result").expect("present_work_result (ON)");
    // The snapshot is ON: the work-result app presents its UI resource.
    assert_eq!(
        on_present.pointer("/_meta/ui/resourceUri"),
        Some(&json!(MCP_WORK_RESULT_UI_RESOURCE_URI))
    );
    assert!(
        tool(&on["result"], "work_result_state").is_some(),
        "app-only tool present when the snapshot enables MCP Apps"
    );
}
