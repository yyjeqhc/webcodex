use super::*;
use std::sync::Arc;

fn tool<'a>(payload: &'a Value, name: &str) -> Option<&'a Value> {
    payload["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}

fn goal_auth(username: &str) -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    auth.user_id = Some(format!("user-{username}"));
    auth.username = Some(username.to_string());
    auth.api_key_id = Some(format!("key-{username}"));
    auth.role = Some("user".to_string());
    auth.scopes = vec![
        crate::auth::SCOPE_RUNTIME_READ.to_string(),
        crate::auth::SCOPE_PROJECT_READ.to_string(),
        crate::auth::SCOPE_COMMUNICATION_READ.to_string(),
        crate::auth::SCOPE_COMMUNICATION_MANAGE.to_string(),
        crate::auth::scopes::SCOPE_SESSION_COLLABORATE.to_string(),
    ];
    auth.token_kind = Some("user".to_string());
    auth
}

fn goal_runtime() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("goal-plan.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    (temp, db, runtime)
}

fn create_goal(runtime: &ToolRuntime, auth: &crate::auth::AuthContext, key: &str) -> String {
    let created = runtime.create_goal(
        Some(auth),
        "Ship Goal Plan presentation".to_string(),
        "Expose one bounded read-only durable Goal card with exact-state polling.".to_string(),
        key.to_string(),
    );
    assert!(created.success, "{:?}", created.output);
    created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string()
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
async fn goal_plan_app_descriptor_is_sparse_app_only_resource_backed_and_adaptive_direct() {
    assert_eq!(MCP_GOAL_PLAN_UI_RESOURCE_URI, "ui://webcodex/goal-plan/v6");
    assert!(
        MCP_GOAL_PLAN_APP_HTML.contains("version: \"6.0.0\""),
        "Goal Plan App self-version must advance with its cache-breaking resource identity"
    );
    let (_temp, _db, adaptive) = goal_runtime();
    let auth = goal_auth("goal-plan-descriptor");

    let ui = handle_with_server_apps_enabled(
        &adaptive,
        rpc(
            "tools/list",
            Some(json!(4101)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let ui = match ui {
        McpOutcome::Ok(value) => value,
        other => panic!("expected UI-capable adaptive tools/list: {other:?}"),
    };
    let present = tool(&ui["result"], "present_goal_plan").expect("adaptive present_goal_plan");
    assert_eq!(
        present.pointer("/_meta/ui/resourceUri"),
        Some(&json!(MCP_GOAL_PLAN_UI_RESOURCE_URI))
    );
    assert!(present.pointer("/_meta/ui/visibility").is_none());
    let state = tool(&ui["result"], "goal_plan_sync").expect("app-only goal_plan_sync");
    assert_eq!(state.pointer("/_meta/ui/visibility"), Some(&json!(["app"])));
    assert!(state.pointer("/_meta/ui/resourceUri").is_none());
    assert!(tool(&ui["result"], "goal_plan_recheck_attention").is_none());
    assert_eq!(state["inputSchema"]["required"], json!(["goal_id"]));
    assert_eq!(
        state["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["goal_id".to_string()]
    );

    let plain = handle_with_server_apps_enabled(
        &adaptive,
        rpc("tools/list", Some(json!(4102)), mcp_2026_params(json!({}))),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(plain) = plain else {
        panic!("expected ordinary adaptive tools/list");
    };
    assert!(tool(&plain["result"], "present_goal_plan").is_some());
    assert!(tool(&plain["result"], "present_goal_plan")
        .unwrap()
        .pointer("/_meta/ui/resourceUri")
        .is_none());
    assert!(tool(&plain["result"], "goal_plan_sync").is_none());
    assert!(tool(&plain["result"], "goal_plan_recheck_attention").is_none());

    let disabled_ui = handle_with_server_apps_enabled(
        &adaptive,
        rpc(
            "tools/list",
            Some(json!(4106)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        false,
    )
    .await;
    let McpOutcome::Ok(disabled_ui) = disabled_ui else {
        panic!("expected tools/list with Server Apps disabled");
    };
    let disabled_present = tool(&disabled_ui["result"], "present_goal_plan")
        .expect("present_goal_plan remains a normal read tool");
    assert!(disabled_present.pointer("/_meta/ui/resourceUri").is_none());
    assert!(tool(&disabled_ui["result"], "goal_plan_sync").is_none());

    for descriptor in ui["result"]["tools"].as_array().unwrap() {
        if descriptor["name"] == "present_goal_plan" {
            continue;
        }
        assert_ne!(
            descriptor
                .pointer("/_meta/ui/resourceUri")
                .and_then(Value::as_str),
            Some(MCP_GOAL_PLAN_UI_RESOURCE_URI),
            "only present_goal_plan may create the Goal Plan card"
        );
    }

    let resources = handle_with_server_apps_enabled(
        &adaptive,
        rpc(
            "resources/list",
            Some(json!(4104)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(resources) = resources else {
        panic!("expected Goal Plan resources/list");
    };
    let resource = resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["uri"] == MCP_GOAL_PLAN_UI_RESOURCE_URI)
        .expect("Goal Plan App resource");
    assert_eq!(resource["mimeType"], MCP_UI_RESOURCE_MIME_TYPE);
    assert_eq!(
        resource["_meta"]["ui"]["csp"],
        json!({"connectDomains": [], "resourceDomains": []})
    );

    assert!(!resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|resource| resource["uri"] == "ui://webcodex/goal-plan/v1"));
    for old_uri in [
        "ui://webcodex/goal-plan/v1",
        "ui://webcodex/goal-plan/v2",
        "ui://webcodex/goal-plan/v3",
        "ui://webcodex/goal-plan/v4",
    ] {
        assert!(super::super::resources::mcp_goal_plan_app_resource_read(old_uri, None).is_none());
    }
    for uri in [MCP_GOAL_PLAN_UI_RESOURCE_URI] {
        let read = handle_with_server_apps_enabled(
            &adaptive,
            rpc(
                "resources/read",
                Some(json!(4105)),
                mcp_2026_ui_params(json!({"uri": uri})),
            ),
            Some(&auth),
            true,
        )
        .await;
        let McpOutcome::Ok(read) = read else {
            panic!("expected Goal Plan resource read");
        };
        assert_eq!(read["result"]["contents"][0]["uri"], uri);
        assert_eq!(
            read["result"]["contents"][0]["text"],
            MCP_GOAL_PLAN_APP_HTML
        );
    }
    for required in [
        "goal_plan_sync",
        "completed_step_count",
        "current_step_id",
        "progress_summary",
        "continuity",
        "production_auto_resume_available",
        "observation_lease_ms",
        "attention_candidate_at_unix_ms",
        "host_dispatch_accepted_at_unix_ms",
        "first_post_resume_meaningful_at_unix_ms",
        "host-delivery",
        "fresh-turn",
        "last-resume",
        "controller_agent_id",
        "ui/notifications/tool-input",
        "visibilitychange",
        "ui/resource-teardown",
        "lastRevision",
        "setTimeout",
        "VISIBLE_NORMAL_POLL_MS",
        "HIDDEN_POLL_MS",
        "TRANSIENT_POLL_MS",
        "pagehide",
    ] {
        assert!(
            MCP_GOAL_PLAN_APP_HTML.contains(required),
            "missing App behavior {required}"
        );
    }
    for forbidden in [
        "localStorage",
        "ui/message",
        "consume_token",
        "wake_token",
        "attempt_fence",
        "authority_fingerprint",
        "endpoint_id",
        "controller_generation",
        "client_attachment_id",
    ] {
        assert!(
            !MCP_GOAL_PLAN_APP_HTML.contains(forbidden),
            "Goal Plan App contains forbidden G3/private marker {forbidden}"
        );
    }
}

#[tokio::test]
async fn goal_plan_poll_reads_authoritative_revision_without_ui_request_identity_or_mutation() {
    let (_temp, _db, runtime) = goal_runtime();
    let bob = goal_auth("goal-plan-bob");
    let alice = goal_auth("goal-plan-alice");
    let goal_id = create_goal(&runtime, &bob, "goal-plan-bob-create");

    let present = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4201)),
            mcp_2026_ui_params(json!({
                "name": "present_goal_plan",
                "arguments": {"goal_id": goal_id}
            })),
        ),
        Some(&bob),
        true,
    )
    .await;
    let McpOutcome::Ok(present) = present else {
        panic!("present_goal_plan failed");
    };
    assert_eq!(present["result"]["structuredContent"]["success"], true);
    assert_eq!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["goal_id"],
        goal_id
    );
    assert_eq!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["revision"],
        1
    );
    assert_eq!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["version"],
        3
    );
    assert_eq!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["activity"]["available"],
        false
    );
    assert_eq!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["activity"]["state"],
        "unobserved"
    );
    assert!(
        present["result"]["structuredContent"]["output"]["goal_plan"]["activity"]
            ["last_seen_at_ms"]
            .is_null()
    );

    // App polling must not rely on the initiating tools/list/call carrying UI capability
    // metadata. Exact Goal identity plus the caller's normal Goal authority is sufficient.
    let poll = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4202)),
            mcp_2026_params(json!({
                "name": "goal_plan_sync",
                "arguments": {"goal_id": goal_id}
            })),
        ),
        Some(&bob),
        true,
    )
    .await;
    let McpOutcome::Ok(poll) = poll else {
        panic!("app-only Goal polling failed");
    };
    assert_eq!(poll["result"]["structuredContent"]["success"], true);
    assert_eq!(
        poll["result"]["structuredContent"]["output"]["goal_plan"]["revision"],
        1
    );
    assert_eq!(
        poll["result"]["structuredContent"]["output"]["goal_plan"]["activity"]["available"],
        false
    );
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["summary"]["revision"],
        1
    );

    let update = runtime.update_goal(
        Some(&bob),
        goal_id.clone(),
        1,
        None,
        Some("Revision two objective".to_string()),
        None,
        None,
        "goal-plan-revision-two".to_string(),
    );
    assert!(update.success, "{:?}", update.output);
    let poll2 = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4203)),
            mcp_2026_params(json!({
                "name": "goal_plan_sync",
                "arguments": {"goal_id": goal_id}
            })),
        ),
        Some(&bob),
        true,
    )
    .await;
    let McpOutcome::Ok(poll2) = poll2 else {
        panic!("second app-only Goal polling failed");
    };
    assert_eq!(
        poll2["result"]["structuredContent"]["output"]["goal_plan"]["revision"],
        2
    );
    assert!(poll2["result"]["structuredContent"]["output"]["goal_plan"]
        .get("objective")
        .is_none());
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["objective"],
        "Revision two objective"
    );
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["summary"]["revision"],
        2
    );

    let foreign = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4204)),
            mcp_2026_params(json!({
                "name": "goal_plan_sync",
                "arguments": {"goal_id": goal_id}
            })),
        ),
        Some(&alice),
        true,
    )
    .await;
    let McpOutcome::Ok(foreign) = foreign else {
        panic!("foreign exact lookup must return an existence-hidden tool result");
    };
    assert_eq!(foreign["result"]["structuredContent"]["success"], false);
    assert_eq!(
        foreign["result"]["structuredContent"]["output"]["error_kind"],
        "goal_not_found"
    );

    let disabled = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4205)),
            mcp_2026_params(json!({
                "name": "goal_plan_sync",
                "arguments": {"goal_id": goal_id}
            })),
        ),
        Some(&bob),
        false,
    )
    .await;
    assert!(matches!(disabled, McpOutcome::BadRequest(_)));
}

#[tokio::test]
async fn goal_plan_sync_discards_unadvertised_recording_session_wrapper() {
    let (_temp, _db, runtime) = goal_runtime();
    let owner = goal_auth("goal-plan-wrapper");
    let goal_id = create_goal(&runtime, &owner, "goal-plan-wrapper-goal");
    let session = runtime.sessions.start_session(
        Some("agent:missing:goal-plan".to_string()),
        Some("Goal Plan wrapper suppression".to_string()),
    );
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();

    let outcome = handle_with_server_apps_enabled(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(4389)),
            mcp_2026_ui_params(json!({
                "name": "goal_plan_sync",
                "arguments": {
                    "goal_id": goal_id,
                    "recording_session_id": session.session_id
                }
            })),
        ),
        Some(&owner),
        true,
    )
    .await;
    let McpOutcome::Ok(result) = outcome else {
        panic!("Goal Plan sync with discarded wrapper must reach the canonical tool");
    };
    assert_eq!(result["result"]["structuredContent"]["success"], true);

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.updated_at, before.updated_at);
}

#[test]
fn goal_plan_sync_accepts_only_exact_selector_and_never_client_timing_or_authority() {
    let goal_id = "wc_goal_G4G4G4G4G4G4G4G4";
    let base = json!({"goal_id": goal_id});
    assert!(crate::tool_runtime::ToolCall::from_tool_name("goal_plan_sync", base.clone()).is_ok());
    assert!(crate::tool_runtime::ToolCall::from_tool_name(
        "goal_plan_recheck_attention",
        base.clone()
    )
    .is_err());
    for (field, value) in [
        ("last_seen_at_ms", json!(900000)),
        ("last_meaningful_activity_at_ms", json!(1)),
        ("quiet_for_ms", json!(899999)),
        ("active_meaningful_request_count", json!(0)),
        ("workflow_session_id", json!("wc_sess_SsSsSsSsSsSsSsSs")),
        ("controller_agent_id", json!("wc_dagent_AgAgAgAgAgAgAgAg")),
        ("window_id", json!("client-window")),
        ("coverage_partial", json!(false)),
    ] {
        let mut forged = base.clone();
        forged[field] = value;
        assert!(
            crate::tool_runtime::ToolCall::from_tool_name("goal_plan_sync", forged).is_err(),
            "{field}"
        );
    }
    assert_eq!(
        super::super::goal_plan_observation_id(Some("goal_plan_sync"), &json!({"arguments": base})),
        Some(goal_id.into())
    );
    assert!(super::super::goal_plan_observation_id(
        Some("goal_plan_recheck_attention"),
        &json!({"arguments": base})
    )
    .is_none());
    assert!(super::super::goal_plan_observation_id(
        Some("goal_plan_sync"),
        &json!({"arguments": {"goal_id": "not-a-goal"}})
    )
    .is_none());
}

#[tokio::test]
async fn goal_plan_sync_is_app_only_reauthorized_and_does_not_create_attention_without_evidence() {
    let (_temp, db, runtime) = goal_runtime();
    let owner = goal_auth("detector-owner");
    let goal_id = create_goal(&runtime, &owner, "detector-goal");
    let request = || {
        rpc(
            "tools/call",
            Some(json!(4390)),
            mcp_2026_params(json!({
                "name": "goal_plan_sync", "arguments": {"goal_id": goal_id}
            })),
        )
    };
    let allowed = handle_with_server_apps_enabled(&runtime, request(), Some(&owner), true).await;
    let McpOutcome::Ok(result) = allowed else {
        panic!("authorized App sync must return a tool result");
    };
    assert_eq!(result["result"]["structuredContent"]["success"], true);
    assert_eq!(
        result["result"]["structuredContent"]["output"]["goal_plan"]["goal_id"],
        goal_id
    );
    let disabled = handle_with_server_apps_enabled(&runtime, request(), Some(&owner), false).await;
    assert!(matches!(disabled, McpOutcome::BadRequest(_)));
    let foreign = goal_auth("detector-foreign");
    let denied = handle_with_server_apps_enabled(&runtime, request(), Some(&foreign), true).await;
    let McpOutcome::Ok(result) = denied else {
        panic!("foreign Goal must return existence-hidden failure");
    };
    assert_eq!(
        result["result"]["structuredContent"]["output"]["error_kind"],
        "goal_not_found"
    );
    assert!(!result.to_string().contains(&goal_id));
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_attention_events",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
