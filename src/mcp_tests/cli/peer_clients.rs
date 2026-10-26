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
            "runtime:read project:read project:write job:run {}",
            crate::auth::SCOPE_SESSION_COLLABORATE
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
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

fn rendered(body: &Value, client: usize) -> String {
    if client == 0 {
        body.to_string()
    } else {
        text(body).to_string()
    }
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
fn chatgpt_codex_and_claude_discover_message_and_ack_through_real_mcp() {
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
            for client in 0..3 {
                // No-ACK discovery may legitimately be omitted by the production
                // 40ms optional budget. Observe later ordinary HTTP calls without
                // changing that budget or inserting discovery routes ourselves.
                let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
                let mut seen = [false; 3];
                seen[client] = true;
                let mut last = String::new();
                for _ in 0..12 {
                    let body = peer_call(&service, &token, client, "list_sessions", json!({"project":project}), None).await;
                    assert_eq!(body["result"]["isError"], false, "{body}");
                    last = rendered(&body, client);
                    if last.contains("new_peers") {
                        assert!(last.contains("recent_same_project_activity_not_liveness"), "{last}");
                        assert!(last.contains(project), "{last}");
                    }
                    for other in 0..3 {
                        if other != client && last.contains(&peers[other]) {
                            assert!(!seen[other], "duplicate discovery {client}/{other}: {last}");
                            assert!(last.contains(&identities[other].1), "{last}");
                            seen[other] = true;
                        }
                    }
                    if seen.iter().all(|found| *found) || std::time::Instant::now() >= until { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                assert!(seen.iter().all(|found| *found), "missing peers for {client}: {seen:?}; {last}");
                let again = peer_call(&service, &token, client, "list_sessions", json!({"project":project}), None).await;
                assert_eq!(again["result"]["isError"], false, "{again}");
                assert!(!rendered(&again, client).contains("new_peers"), "discovery must be deduplicated: {again}");
            }
            for sender in 0..3 {
                let recipient = (sender + 1) % 3;
                let message = format!("cross-client-message-{sender}");
                let posted = peer_call(&service, &token, sender, "post_peer_message", json!({
                    "peer_id":peers[recipient], "kind":"question", "message":message, "requires_ack":true
                }), None).await;
                assert_eq!(posted["result"]["isError"], false, "{posted}");
                let id: String = db.conn_for_tests().query_row(
                    "SELECT message_id FROM window_peer_messages WHERE message=?1", [&message], |r| r.get(0),
                ).unwrap();
                let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
                let mut received = false;
                let mut last = String::new();
                for _ in 0..12 {
                    let delivered = peer_call(&service, &token, recipient, "list_jobs", json!({}), None).await;
                    assert_eq!(delivered["result"]["isError"], false, "{delivered}");
                    last = rendered(&delivered, recipient);
                    let before: Option<i64> = db.conn_for_tests().query_row(
                        "SELECT first_ack_observed_at_ms FROM window_peer_messages WHERE message_id=?1", [&id], |r| r.get(0),
                    ).unwrap();
                    assert!(before.is_none(), "projection is not ACK");
                    received = last.contains(&message) && last.contains(&id);
                    if received || std::time::Instant::now() >= until { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                assert!(received, "missing retained message {id}: {last}");
                // Explicit ACK is not optional budget shedding: send it once and
                // require its durable timestamp immediately.
                let acked = peer_call(&service, &token, recipient, "list_jobs", json!({}), Some(&id)).await;
                assert_eq!(acked["result"]["isError"], false, "{acked}");
                let after: Option<i64> = db.conn_for_tests().query_row(
                    "SELECT first_ack_observed_at_ms FROM window_peer_messages WHERE message_id=?1", [&id], |r| r.get(0),
                ).unwrap();
                assert!(after.is_some(), "client {recipient} must ACK through its advertised envelope");
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
