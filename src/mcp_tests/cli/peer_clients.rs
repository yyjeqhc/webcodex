//! Client-source composition fixture: real HTTP identity, gateway and text/JSON projection.
use super::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerProjectSummary};

fn peer_token(db: &crate::Database, username: &str) -> (String, String) {
    let user = crate::test_support::seed_user_with_role(db, username, "admin");
    let plaintext = crate::auth::generate_api_token();
    let record = crate::models::ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: "peer-client-fixture".into(),
        key_prefix: crate::auth::token_prefix(&plaintext),
        created_at: chrono::Utc::now().timestamp(),
        last_used_at: None,
        revoked_at: None,
        scopes: format!(
            "runtime:read project:read project:write job:run {} {}",
            crate::auth::SCOPE_SESSION_COLLABORATE,
            crate::auth::SCOPE_PLUGIN_INSPECT
        ),
        expires_at: None,
        kind: crate::models::TOKEN_KIND_USER.into(),
        allowed_client_id: None,
    };
    db.insert_api_key(&record, &crate::auth::hash_token(&plaintext))
        .unwrap();
    (plaintext, user.username)
}

async fn peer_call(
    service: &Service,
    token: &str,
    client: usize,
    tool: &str,
    arguments: Value,
    ack: Option<&str>,
) -> Value {
    let mut params =
        json!({"name":"call_runtime_tool", "arguments":{"tool":tool,"arguments":arguments}});
    if let Some(id) = ack {
        params["arguments"]["_wc"] = json!({"ack":[id]});
    }
    let mut request = TestClient::post("http://localhost/mcp").bearer_auth(token);
    if client != 0 {
        request = request.add_header(SURFACE_HEADER, "cli", true);
    }
    if client != 2 {
        params = mcp_2026_params(params);
        params["_meta"][if client == 0 {
            "openai/session"
        } else {
            "threadId"
        }] = json!(if client == 0 {
            "chatgpt-peer-fixture"
        } else {
            "codex-peer-fixture"
        });
        request = request
            .add_header("Mcp-Protocol-Version", MCP_STATELESS_PROTOCOL_VERSION, true)
            .add_header("Mcp-Method", "tools/call", true)
            .add_header("Mcp-Name", "call_runtime_tool", true);
    } else {
        request = request.add_header("X-WebCodex-Client-Window", "claude-peer-fixture", true);
    }
    let mut response = request
        .json(&json!({"jsonrpc":"2.0","id":71,"method":"tools/call","params":params}))
        .send(service)
        .await;
    let status = effective_status(&response);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(
        status,
        if ack.is_some() {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::OK
        },
        "{body}"
    );
    body
}

fn rendered(body: &Value, _client: usize) -> String {
    body.to_string()
}

fn seed_meaningful_activity(
    db: &Arc<crate::Database>,
    key: &str,
    source: &str,
    principal: (&str, &str),
    project: &str,
    at: i64,
) {
    assert!(crate::action_audit_sessions::record_action_event(
        db,
        crate::action_audit_sessions::ActionAuditEventInput {
            explicit_session_id: None,
            session_title: None,
            endpoint: "/mcp".into(),
            action_name: "toolsCall".into(),
            operation: Some("read_files".into()),
            project: Some(project.into()),
            principal_kind: None,
            principal_user_id: None,
            oauth_client_id: None,
            status: "success".into(),
            http_status: Some(200),
            started_at: at / 1000,
            ended_at: at / 1000,
            duration_ms: 1,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({}),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(key.into()),
            client_window_source: Some(source.into()),
            server_trace_id: Some(format!("peer-client-fixture-{at}")),
            principal_correlation_kind: Some(principal.0.into()),
            principal_correlation_id: Some(principal.1.into()),
            window_started_at_ms: Some(at),
            window_ended_at_ms: Some(at + 1),
            request_observed_at_ms: None,
            response_handed_at_ms: None,
            window_transition_kind: None,
            response_streaming: None,
            window_continuity_eligible: None,
            window_meaningful: true,
            recorder_gap_session_id: None,
            workflow_links: Vec::new(),
        }
    ));
}

#[test]
fn cli_preserves_peer_audit_identity_without_consuming_or_acknowledging_delivery() {
    std::thread::Builder::new().name("peer-client-mcp-matrix".into())
        .stack_size(8 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let (temp, db) = test_db();
            let (token, owner) = peer_token(&db, "peer-client-owner");
            let (foreign_token, _) = peer_token(&db, "peer-client-foreign");
            let runtime = Arc::new(test_runtime()
                .with_communication_database(db.clone()).with_window_activity_database(db.clone()));
            runtime.runner_registry.register(crate::runner_protocol::RunnerRegisterRequest {
                owner: Some(owner),
                ..crate::test_support::runner_registration("peer-client-runner", "instance", RunnerCapabilities::default())
            }).await.unwrap();
            crate::test_support::apply_project_inventory_snapshot(&runtime.runner_registry,
                "peer-client-runner", "instance", vec![RunnerProjectSummary {
        registration_incarnation: None,
                    id: "repo".into(), name: Some("Peer fixture".into()), path: temp.path().to_string_lossy().to_string(),
                    allow_patch: true, kind: Some("repo".into()), registration_source: None, description: None,
                    hooks: Vec::new(), disabled: false, revision: None, root_fingerprint: None, lineage: None,
                    git_branch: None, git_head: None, git_dirty: None, updated_at: 1, shell_profile: None,
                }]).await;
            let project = "agent:peer-client-runner:repo";
            let service = Service::new(build_test_router(test_config(Some("unused-bootstrap")), db.clone(), runtime));
            let mut identities = Vec::new();
            for client in 0..3 {
                // Bind actual identities before Project discovery can consume its one-shot hints.
                let body = peer_call(&service, &token, client, "list_jobs", json!({}), None).await;
                assert_eq!(body["result"]["isError"], false, "{body}");
                let row: (String, String, String, String) = db.conn_for_tests().query_row(
                    "SELECT client_window_key,client_window_source,principal_correlation_kind,principal_correlation_id FROM action_events ORDER BY rowid DESC LIMIT 1",
                    [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
                ).unwrap();
                identities.push(row);
            }
            assert_eq!(identities.iter().map(|row| row.1.as_str()).collect::<Vec<_>>(),
                ["openai-session", "codex-thread", "client-window-header"]);
            let now = chrono::Utc::now().timestamp_millis() - 1_000;
            for (index, (key, source, kind, principal)) in identities.iter().enumerate() {
                // Discovery needs meaningful Project evidence. Seed it through the
                // canonical audit writer; the subsequent discovery is real HTTP.
                seed_meaningful_activity(&db, key, source, (kind, principal), project, now + index as i64);
            }
            let peers: Vec<_> = identities.iter().map(|row| crate::client_window::peer_id_from_window_key(&row.0).unwrap()).collect();
            // Seed the Adaptive sender's retained discoveries through the canonical
            // store API. Optional HTTP discovery has a 40ms budget and may be shed;
            // this fixture tests CLI suppression, not scheduler-dependent delivery.
            let sender = &identities[0];
            let known = db.take_new_recent_project_peers(&sender.2, &sender.3, &sender.0, project, now - 1, now + 10, 8).unwrap();
            assert_eq!(known.len(), 2);
            for client in 1..3 {
                let response = peer_call(&service, &token, client, "list_jobs", json!({"project":project}), None).await;
                assert_eq!(response["result"]["isError"], false, "{response}");
                assert!(!text(&response).contains("peer_awareness"), "{response}");
                let discovered: i64 = db.conn_for_tests().query_row(
                    "SELECT count(*) FROM window_peer_discoveries WHERE observer_window_key=?1", [&identities[client].0], |r| r.get(0),
                ).unwrap();
                assert_eq!(discovered, 0, "CLI must not consume optional discovery");
                let message = format!("retained-for-client-{client}");
                let posted = peer_call(&service, &token, 0, "post_peer_message", json!({
                    "peer_id":peers[client], "kind":"question", "message":message, "requires_ack":true
                }), None).await;
                assert_eq!(posted["result"]["isError"], false, "{posted}");
                let id: String = db.conn_for_tests().query_row(
                    "SELECT message_id FROM window_peer_messages WHERE message=?1", [&message], |r| r.get(0),
                ).unwrap();
                // Exercise the ordinary kernel and the specialized Plugin adapter.
                for (tool, args) in [("list_jobs", json!({})), ("plugin_tool", json!({"action":"list"}))] {
                    let response = peer_call(&service, &token, client, tool, args, None).await;
                    assert!(!response.to_string().contains(&message));
                    assert!(!response.to_string().contains(&id));
                }
                let rejected = peer_call(&service, &token, client, "list_jobs", json!({}), Some(&id)).await;
                assert_eq!(rejected["error"]["code"], -32602);
                let (ack, projections): (Option<i64>, i64) = db.conn_for_tests().query_row(
                    "SELECT first_ack_observed_at_ms,projection_count FROM window_peer_messages WHERE message_id=?1", [&id], |r| Ok((r.get(0)?,r.get(1)?)),
                ).unwrap();
                assert!(ack.is_none());
                assert_eq!(projections, 0, "CLI must skip collection, not strip an already consumed delivery");
            }
            let foreign = peer_call(&service, &foreign_token, 0, "post_peer_message", json!({
                "peer_id":peers[1], "kind":"question", "message":"foreign principal must not send"
            }), None).await;
            assert!(rendered(&foreign, 0).contains("peer_not_found"), "{foreign}");
            let leaked: i64 = db.conn_for_tests().query_row(
                "SELECT count(*) FROM window_peer_messages WHERE message='foreign principal must not send'", [], |r| r.get(0),
            ).unwrap();
            assert_eq!(leaked, 0);
        });
    }).unwrap().join().unwrap();
}
