use super::super::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
    ToolProtocolCapabilities, ToolTransport,
};
use super::super::{ToolResult, ToolRuntime};
use super::support::*;
use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use serde_json::{json, Value};
use std::sync::Arc;

fn runtime_with_peer_db() -> (tempfile::TempDir, Arc<crate::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("peer-collaboration.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests()
        .with_communication_database(db.clone())
        .with_window_activity_database(db.clone());
    (temp, db, runtime)
}

fn record_meaningful_window_activity(
    db: &Arc<crate::Database>,
    auth: &AuthContext,
    window: &ClientWindow,
    project: &str,
    operation: &str,
    at_ms: i64,
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
            project: Some(project.to_string()),
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
            client_window_key: Some(window.key().to_string()),
            client_window_source: Some(window.source().to_string()),
            server_trace_id: Some(format!("peer-fixture-{at_ms}-{operation}")),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(at_ms),
            window_ended_at_ms: Some(at_ms + 1),
            request_observed_at_ms: None,
            response_handed_at_ms: None,
            window_transition_kind: None,
            response_streaming: None,
            window_continuity_eligible: None,
            window_meaningful: true,
            recorder_gap_session_id: None,
            workflow_links: Vec::new(),
        },
    );
}

fn establish_peer_route(
    db: &Arc<crate::Database>,
    runtime: &ToolRuntime,
    auth: &AuthContext,
    observer: &ClientWindow,
    peer: &ClientWindow,
    project: &str,
    operation: &str,
    at_ms: i64,
) {
    record_meaningful_window_activity(db, auth, peer, project, operation, at_ms);
    let mut discovery = ToolResult::ok(json!({"success": true}));
    runtime.add_peer_collaboration_projection(
        &mut discovery,
        Some(auth),
        Some(observer),
        Some(project),
        &[],
    );
    assert_eq!(
        discovery.output["peer_awareness"]["new_peers"][0]["peer_id"],
        peer.peer_id(),
        "fixture must establish the same retained route production discovery exposes"
    );
}

async fn call_in_window(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    window: &ClientWindow,
    tool_name: &str,
    arguments: Value,
    invocation_metadata: ToolInvocationMetadata,
) -> ToolResult {
    let outcome = runtime
        .call_tool_with_invocation_metadata(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(auth),
                window: Some(window),
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
            invocation_metadata,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(
        outcome.error_status.is_none(),
        "unexpected transport error: {:?}",
        outcome.error_status
    );
    outcome.result.expect("tool result")
}

async fn call_in_window_with_control(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    window: &ClientWindow,
    tool_name: &str,
    arguments: Value,
    control: Value,
) -> ToolResult {
    let outcome = runtime
        .call_tool_with_invocation_metadata(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(auth),
                window: Some(window),
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
            ToolInvocationMetadata {
                control: Some(serde_json::from_value(control).unwrap()),
                ..Default::default()
            },
            ToolProtocolCapabilities {
                control_sidecars: true,
                ..Default::default()
            },
        )
        .await;
    assert!(outcome.error_status.is_none(), "{:?}", outcome.error_status);
    outcome.result.unwrap()
}

#[tokio::test]
async fn control_communication_peer_message_uses_canonical_replay_path() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-control-owner");
    let sender = ClientWindow::for_test("peer-control-sender");
    let recipient = ClientWindow::for_test("peer-control-recipient");
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:source-project",
        "read_files",
        chrono::Utc::now().timestamp_millis() - 1_000,
    );
    let control = json!({"communication": {"before": [{"peer_message": {
        "peer_id": recipient.peer_id(),
        "kind": "progress",
        "message": "parser review complete",
        "tags": ["parser"],
        "delivery_key": "parser-review-complete"
    }}]}});
    let first = call_in_window_with_control(
        &runtime,
        &auth,
        &sender,
        "runtime_status",
        json!({"compact": true}),
        control.clone(),
    )
    .await;
    assert!(first.success, "{:?}", first.output);
    let projection = &first.output["control"]["communication"]["before"][0];
    assert_eq!(projection["success"], true);
    assert_eq!(projection["state_changed"], true);
    let message_id = projection["message_id"].as_str().unwrap().to_string();

    let replay = call_in_window_with_control(
        &runtime,
        &auth,
        &sender,
        "runtime_status",
        json!({"compact": true}),
        control,
    )
    .await;
    let replay = &replay.output["control"]["communication"]["before"][0];
    assert_eq!(replay["message_id"], message_id);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["state_changed"], false);

    let standalone = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "progress",
            "message": "parser review complete",
            "tags": ["parser"],
            "delivery_key": "parser-review-complete"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(standalone.success);
    assert_eq!(standalone.output["message_id"], message_id);
    assert_eq!(standalone.output["replayed"], true);
    let count: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM window_peer_messages", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn ordinary_peer_message_is_projected_once_on_the_next_tool_result() {
    let (_temp, _db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner");
    let sender = ClientWindow::for_test("peer-sender-once");
    let recipient = ClientWindow::for_test("peer-recipient-once");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &_db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:source-project",
        "read_files",
        now_ms - 1_000,
    );

    let posted = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "question",
            "message": "which part are you changing?",
            "priority": "normal"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);
    let message_id = posted.output["message_id"].as_str().unwrap().to_string();

    let first = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(
        first.output["peer_messages"]["messages"][0]["message_id"],
        message_id
    );
    assert_eq!(
        first.output["peer_messages"]["messages"][0]["from_peer_id"],
        sender.peer_id()
    );
    assert_eq!(
        first.output["peer_messages"]["messages"][0]["message"],
        "which part are you changing?"
    );
    assert_eq!(
        first.output["peer_messages"]["messages"][0]["projection_count"],
        1
    );

    let second = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert!(second.output.get("peer_messages").is_none());
}

#[tokio::test]
async fn ack_required_peer_message_repeats_on_omission_and_current_ack_suppresses_it() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-ack");
    let sender = ClientWindow::for_test("peer-sender-ack");
    let recipient = ClientWindow::for_test("peer-recipient-ack");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:ack-source",
        "read_files",
        now_ms - 1_000,
    );

    let posted = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "risk",
            "message": "please retain this risk until acknowledged",
            "priority": "high",
            "requires_ack": true
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);
    let message_id = posted.output["message_id"].as_str().unwrap().to_string();

    for expected_projection_count in [1, 2] {
        let result = call_in_window(
            &runtime,
            &auth,
            &recipient,
            "runtime_status",
            json!({"compact": true}),
            ToolInvocationMetadata::default(),
        )
        .await;
        assert_eq!(
            result.output["peer_messages"]["messages"][0]["message_id"],
            message_id
        );
        assert_eq!(
            result.output["peer_messages"]["messages"][0]["projection_count"],
            expected_projection_count
        );
    }

    let acknowledged = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata {
            ack_session_message_ids: vec![message_id.clone()],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        acknowledged.output["peer_messages"]["ack"]["accepted_ids"],
        json!([message_id.clone()])
    );
    assert_eq!(acknowledged.output["peer_messages"]["messages"], json!([]));

    let omitted_again = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        omitted_again.output["peer_messages"]["messages"][0]["message_id"],
        message_id
    );
    assert_eq!(
        omitted_again.output["peer_messages"]["messages"][0]["projection_count"],
        3
    );

    let conn = db.conn_for_tests();
    let (projection_count, first_ack): (i64, Option<i64>) = conn
        .query_row(
            "SELECT projection_count, first_ack_observed_at_ms
             FROM window_peer_messages WHERE message_id = ?1",
            [&message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(projection_count, 3);
    assert!(first_ack.is_some());
}

#[tokio::test]
async fn peer_route_requires_retained_discovery_or_message_state() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-route-retention");
    let sender = ClientWindow::for_test("peer-route-sender");
    let peer = ClientWindow::for_test("peer-route-target");
    let project = "agent:special:peer-route";
    let now_ms = chrono::Utc::now().timestamp_millis();

    // ActionAudit supplies recent meaningful activity for discovery, but it is
    // not itself a retained communication route. A caller that somehow knows
    // the peer id cannot bypass the awareness projection step.
    record_meaningful_window_activity(&db, &auth, &peer, project, "read_files", now_ms - 1_000);
    let undiscovered = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": peer.peer_id(),
            "kind": "note",
            "message": "must not route from ActionAudit alone"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(!undiscovered.success);
    assert_eq!(undiscovered.output["failure_kind"], "peer_not_found");

    let mut discovery = ToolResult::ok(json!({"success": true}));
    runtime.add_peer_collaboration_projection(
        &mut discovery,
        Some(&auth),
        Some(&sender),
        Some(project),
        &[],
    );
    assert_eq!(
        discovery.output["peer_awareness"]["new_peers"][0]["peer_id"],
        peer.peer_id()
    );

    let discovered = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": peer.peer_id(),
            "kind": "note",
            "message": "retained awareness establishes the route"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(discovered.success, "{:?}", discovered.error);
}

#[tokio::test]
async fn peer_discovery_is_same_project_but_contact_survives_project_change() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-discovery");
    let observer = ClientWindow::for_test("peer-observer");
    let peer = ClientWindow::for_test("peer-moving-window");
    let stale_peer = ClientWindow::for_test("peer-stale-window");
    let now_ms = chrono::Utc::now().timestamp_millis();
    let project_a = "agent:special:main-checkout";
    let project_b = "agent:special:managed-worktree";

    record_meaningful_window_activity(
        &db,
        &auth,
        &peer,
        project_a,
        "search_project_texts",
        now_ms - 2_000,
    );
    record_meaningful_window_activity(
        &db,
        &auth,
        &stale_peer,
        project_a,
        "search_project_texts",
        now_ms - super::super::peer_collaboration::peer_recent_window_ms_for_tests() - 2_000,
    );

    let mut first_discovery = ToolResult::ok(json!({"success": true}));
    runtime.add_peer_collaboration_projection(
        &mut first_discovery,
        Some(&auth),
        Some(&observer),
        Some(project_a),
        &[],
    );
    assert_eq!(
        first_discovery.output["peer_awareness"]["recent_window_secs"],
        600
    );
    assert_eq!(
        first_discovery.output["peer_awareness"]["new_peers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        first_discovery.output["peer_awareness"]["new_peers"][0]["peer_id"],
        peer.peer_id()
    );

    let mut repeated_discovery = ToolResult::ok(json!({"success": true}));
    runtime.add_peer_collaboration_projection(
        &mut repeated_discovery,
        Some(&auth),
        Some(&observer),
        Some(project_a),
        &[],
    );
    assert!(repeated_discovery.output.get("peer_awareness").is_none());

    {
        let conn = db.conn_for_tests();
        conn.execute(
            "DELETE FROM action_events WHERE client_window_key = ?1",
            [peer.key()],
        )
        .unwrap();
    }
    let durable_contact = call_in_window(
        &runtime,
        &auth,
        &observer,
        "post_peer_message",
        json!({
            "peer_id": peer.peer_id(),
            "kind": "note",
            "message": "contact survives ActionAudit retention after discovery"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(durable_contact.success, "{:?}", durable_contact.error);
    let delivered_after_prune = call_in_window(
        &runtime,
        &auth,
        &peer,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        delivered_after_prune.output["peer_messages"]["messages"][0]["message"],
        "contact survives ActionAudit retention after discovery"
    );

    record_meaningful_window_activity(
        &db,
        &auth,
        &peer,
        project_b,
        "work_on_project",
        now_ms + 1_000,
    );
    let posted_after_move = call_in_window(
        &runtime,
        &auth,
        &observer,
        "post_peer_message",
        json!({
            "peer_id": peer.peer_id(),
            "kind": "question",
            "message": "did the worktree task change your current scope?"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(posted_after_move.success, "{:?}", posted_after_move.error);

    let received_after_move = call_in_window(
        &runtime,
        &auth,
        &peer,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        received_after_move.output["peer_messages"]["messages"][0]["message"],
        "did the worktree task change your current scope?"
    );
}

#[tokio::test]
async fn specialized_mcp_structured_content_receives_peer_projection_without_rewrapping() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-specialized");
    let sender = ClientWindow::for_test("peer-specialized-sender");
    let recipient = ClientWindow::for_test("peer-specialized-recipient");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:specialized-source",
        "plugin_tool",
        now_ms - 1_000,
    );

    let posted = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "question",
            "message": "specialized fast path message",
            "requires_ack": true
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    let message_id = posted.output["message_id"].as_str().unwrap().to_string();

    let mut native_result = json!({
        "content": [{"type": "text", "text": "provider response"}],
        "structuredContent": {"providerField": "preserved"},
        "isError": false
    });
    runtime.add_peer_collaboration_to_mcp_call_result(
        &mut native_result,
        Some(&auth),
        Some(&recipient),
        None,
        &[],
    );
    assert_eq!(
        native_result["structuredContent"]["providerField"],
        "preserved"
    );
    assert_eq!(
        native_result["structuredContent"]["peer_messages"]["messages"][0]["message_id"],
        message_id
    );
    assert_eq!(
        native_result["content"][0]["text"], "provider response",
        "native provider content must remain untouched"
    );

    let mut acknowledged_result = json!({
        "content": [{"type": "text", "text": "second provider response"}],
        "structuredContent": {"providerField": "still-preserved"},
        "isError": false
    });
    runtime.add_peer_collaboration_to_mcp_call_result(
        &mut acknowledged_result,
        Some(&auth),
        Some(&recipient),
        None,
        std::slice::from_ref(&message_id),
    );
    assert_eq!(
        acknowledged_result["structuredContent"]["peer_messages"]["ack"]["accepted_ids"],
        json!([message_id])
    );
    assert_eq!(
        acknowledged_result["structuredContent"]["peer_messages"]["messages"],
        json!([])
    );
}

#[tokio::test]
async fn peer_message_input_is_trimmed_deduplicated_and_empty_rejected() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-input-bounds");
    let sender = ClientWindow::for_test("peer-input-sender");
    let recipient = ClientWindow::for_test("peer-input-recipient");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:peer-input",
        "read_files",
        now_ms - 1_000,
    );

    let posted = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "note",
            "message": "  bounded hello  ",
            "tags": [" alpha ", "", "alpha", " beta "]
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);

    let received = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        received.output["peer_messages"]["messages"][0]["message"],
        "bounded hello"
    );
    assert_eq!(
        received.output["peer_messages"]["messages"][0]["tags"],
        json!(["alpha", "beta"])
    );

    let empty = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "note",
            "message": "   "
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(!empty.success);
    assert_eq!(empty.output["failure_kind"], "invalid_peer_message");

    let oversized = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "note",
            "message": "x".repeat(super::super::sessions::MAX_MESSAGE_CHARS + 1)
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(!oversized.success);
    assert_eq!(oversized.output["failure_kind"], "invalid_peer_message");
}

#[tokio::test]
async fn newly_unprojected_message_is_not_starved_by_old_ack_reminders() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-priority");
    let sender = ClientWindow::for_test("peer-priority-sender");
    let recipient = ClientWindow::for_test("peer-priority-recipient");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:peer-priority",
        "read_files",
        now_ms - 1_000,
    );

    for index in 0..4 {
        let posted = call_in_window(
            &runtime,
            &auth,
            &sender,
            "post_peer_message",
            json!({
                "peer_id": recipient.peer_id(),
                "kind": "risk",
                "message": format!("old ack reminder {index}"),
                "requires_ack": true
            }),
            ToolInvocationMetadata::default(),
        )
        .await;
        assert!(posted.success, "{:?}", posted.error);
    }
    let first = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        first.output["peer_messages"]["messages"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    let fresh = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "question",
            "message": "fresh one-shot question"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(fresh.success, "{:?}", fresh.error);

    let second = call_in_window(
        &runtime,
        &auth,
        &recipient,
        "runtime_status",
        json!({"compact": true}),
        ToolInvocationMetadata::default(),
    )
    .await;
    let messages = second.output["peer_messages"]["messages"]
        .as_array()
        .unwrap();
    assert!(messages
        .iter()
        .any(|message| message["message"] == "fresh one-shot question"));
}

#[tokio::test]
async fn oversized_result_rolls_back_one_shot_peer_projection() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("peer-owner-oversized");
    let sender = ClientWindow::for_test("peer-oversized-sender");
    let recipient = ClientWindow::for_test("peer-oversized-recipient");
    let now_ms = chrono::Utc::now().timestamp_millis();
    establish_peer_route(
        &db,
        &runtime,
        &auth,
        &sender,
        &recipient,
        "agent:special:peer-oversized",
        "read_files",
        now_ms - 1_000,
    );

    let posted = call_in_window(
        &runtime,
        &auth,
        &sender,
        "post_peer_message",
        json!({
            "peer_id": recipient.peer_id(),
            "kind": "note",
            "message": "must survive projection rollback"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(posted.success, "{:?}", posted.error);

    let mut oversized = ToolResult::ok(json!({
        "blob": "x".repeat(webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES)
    }));
    runtime.add_peer_collaboration_projection(
        &mut oversized,
        Some(&auth),
        Some(&recipient),
        None,
        &[],
    );
    assert!(oversized.output.get("peer_messages").is_none());

    let mut next = ToolResult::ok(json!({"ok": true}));
    runtime.add_peer_collaboration_projection(&mut next, Some(&auth), Some(&recipient), None, &[]);
    assert_eq!(
        next.output["peer_messages"]["messages"][0]["message"],
        "must survive projection rollback"
    );
    assert_eq!(
        next.output["peer_messages"]["messages"][0]["projection_count"],
        1
    );
}

#[tokio::test]
async fn peer_identity_is_principal_scoped_and_does_not_cross_subjects() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let alice = shared_key_auth_context("peer-alice");
    let bob = shared_key_auth_context("peer-bob");
    let alice_peer = ClientWindow::for_test("alice-peer-window");
    let bob_sender = ClientWindow::for_test("bob-sender-window");
    let now_ms = chrono::Utc::now().timestamp_millis();
    record_meaningful_window_activity(
        &db,
        &alice,
        &alice_peer,
        "agent:special:shared-looking-project",
        "read_files",
        now_ms - 1_000,
    );

    let denied = call_in_window(
        &runtime,
        &bob,
        &bob_sender,
        "post_peer_message",
        json!({
            "peer_id": alice_peer.peer_id(),
            "kind": "question",
            "message": "this principal must not reach Alice"
        }),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert!(!denied.success);
    assert_eq!(denied.output["failure_kind"], "peer_not_found");
}

#[tokio::test]
async fn operator_attention_only_model_activity_consumes_and_acknowledges() {
    let (_temp, db, runtime) = runtime_with_peer_db();
    let auth = shared_key_auth_context("operator-attention-owner");
    let window = ClientWindow::for_test("operator-target");
    let posted = runtime
        .post_window_operator_message(
            window.key(),
            None,
            None,
            "Check the tests".into(),
            "operator-key".into(),
            Some(&auth),
        )
        .await;
    assert!(posted.success, "{:?}", posted.error);
    let id = posted.output["message_id"].as_str().unwrap().to_string();
    for tool in [
        "present_work_result",
        "work_result_state",
        "work_result_send_message",
        "changes_file_diff",
    ] {
        let arguments = match tool {
            "work_result_send_message" => {
                json!({"project":"agent:missing:project","message":"another","delivery_key":"another-key"})
            }
            "changes_file_diff" => {
                json!({"project":"agent:missing:project","session_id":format!("wc_sess_{}","a".repeat(32)),"snapshot_id":"invalid","path":"file"})
            }
            _ => json!({"project":"agent:missing:project"}),
        };
        let outcome = runtime
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: tool.into(),
                    arguments,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: None,
                    auth: Some(&auth),
                    window: Some(&window),
                    record_oauth_scope_denials: false,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                ToolInvocationMetadata::default(),
                ToolProtocolCapabilities {
                    work_result_app: true,
                    ..Default::default()
                },
            )
            .await;
        if let Some(result) = outcome.result {
            assert!(result.output.get("operator_messages").is_none(), "{tool}");
        }
        let count: i64 = db
            .conn_for_tests()
            .query_row(
                "SELECT projection_count FROM window_operator_messages",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{tool} must not consume attention");
    }
    let visible = call_in_window(
        &runtime,
        &auth,
        &window,
        "runtime_status",
        json!({}),
        ToolInvocationMetadata::default(),
    )
    .await;
    assert_eq!(
        visible.output["operator_messages"]["messages"][0]["message_id"],
        id
    );
    assert_eq!(
        visible.output["operator_messages"]["messages"][0]["source"],
        "operator"
    );
    let ack = call_in_window(
        &runtime,
        &auth,
        &window,
        "runtime_status",
        json!({}),
        ToolInvocationMetadata {
            ack_session_message_ids: vec![id.clone()],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        ack.output["operator_messages"]["ack"]["accepted_ids"][0],
        id
    );
    assert!(
        runtime.window_collaboration(Some(window.key()), Some(&auth), 10)["messages"][0]
            ["first_ack_observed_at_ms"]
            .is_number()
    );
}
