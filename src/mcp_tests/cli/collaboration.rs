//! Exercise collaboration through the real HTTP adapter and both response surfaces.
use super::*;
use crate::tool_runtime::startup_brief::validate_schema_instance_for_test;

async fn call_in_window(
    service: &Service,
    cli: bool,
    stateless: bool,
    raw_window: &str,
    envelope: Option<Value>,
) -> (StatusCode, Value) {
    let mut params = json!({
        "name":"read_tool_manifest",
        "arguments":{"tool_name":"read_files"}
    });
    if let Some(envelope) = envelope {
        params["arguments"]["_wc"] = envelope;
    }
    let mut request = TestClient::post("http://localhost/mcp").bearer_auth("secret");
    if cli {
        request = request.add_header(SURFACE_HEADER, "cli", true);
    }
    if stateless {
        params = mcp_2026_params(params);
        params["_meta"]["openai/session"] = json!(raw_window);
        request = request
            .add_header("Mcp-Protocol-Version", MCP_STATELESS_PROTOCOL_VERSION, true)
            .add_header("Mcp-Method", "tools/call", true)
            .add_header("Mcp-Name", "read_tool_manifest", true);
    } else {
        request = request.add_header("Mcp-Session-Id", raw_window, true);
    }
    let mut response = request
        .json(&json!({"jsonrpc":"2.0","id":19,"method":"tools/call","params":params}))
        .send(service)
        .await;
    (
        effective_status(&response),
        response.take_json().await.unwrap(),
    )
}

#[test]
fn operator_messages_ack_and_reply_work_on_cli_and_adaptive_protocol_surfaces() {
    std::thread::Builder::new()
        .name("collaboration-mcp-surfaces".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for cli in [false, true] {
                for stateless in [false, true] {
                    let (_temp, db) = test_db();
                    let runtime = Arc::new(test_runtime_with_mcp_settings(false, false)
                        .with_window_activity_database(db.clone())
                        .with_communication_database(db.clone()));
                    let service = Service::new(build_test_router(
                        test_config(Some("secret")), db, runtime.clone(),
                    ));
                    let raw_window = "collaboration-surface-window";
                    let (_, discovery) = post_cli(
                        &service, "tools/list",
                        if stateless { mcp_2026_params(json!({})) } else { json!({}) }, cli,
                    ).await;
                    let output_schema = discovery["result"]["tools"].as_array().unwrap()
                        .iter().find(|tool| tool["name"] == "read_tool_manifest").unwrap()["outputSchema"].clone();
                    if !cli {
                        assert!(output_schema.is_object(), "Adaptive output schema must be present: {discovery}");
                        assert_eq!(output_schema["type"], "object", "{output_schema}");
                    }

                    let window = crate::client_window::ClientWindow::from_opaque(
                        if stateless { "openai-session" } else { "mcp" }, raw_window,
                    ).unwrap();
                    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap);
                    auth.is_bootstrap = true;
                    let (status, initial) = call_in_window(&service, cli, stateless, raw_window, None).await;
                    assert_eq!(status, StatusCode::OK, "{cli}/{stateless}: {initial}");
                    let posted = runtime.post_window_operator_message_with_options(
                        window.key(), None, None, "note", "normal", true,
                        "Review the focused checks".into(), "surface-operator-delivery".into(), Some(&auth),
                    ).await;
                    assert!(posted.success, "{:?}", posted.error);
                    let message_id = posted.output["message_id"].as_str().unwrap().to_string();
                    let (status, delivered) = call_in_window(&service, cli, stateless, raw_window, None).await;
                    assert_eq!(status, StatusCode::OK, "{delivered}");
                    let rendered = if cli { text(&delivered).to_string() } else { delivered.to_string() };
                    assert!(rendered.contains("Review the focused checks"), "{cli}/{stateless}: {rendered}");
                    assert!(rendered.contains(&message_id), "{rendered}");
                    let before = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
                    assert!(before["messages"][0]["first_ack_observed_at_ms"].is_null(), "{before}");

                    // Test ACK independently of reply rejection: neither operation in a
                    // different Window may advance this retained message before its real ACK.
                    let (status, foreign_ack) = call_in_window(
                        &service, cli, stateless, "different-collaboration-window",
                        Some(json!({"ack":[message_id]})),
                    ).await;
                    assert_eq!(status, StatusCode::OK, "{foreign_ack}");
                    let after_foreign_ack = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
                    assert!(after_foreign_ack["messages"][0]["first_ack_observed_at_ms"].is_null(), "{after_foreign_ack}");
                    assert_eq!(after_foreign_ack["messages"].as_array().unwrap().len(), 1);
                    let (status, foreign_reply) = call_in_window(
                        &service, cli, stateless, "different-collaboration-window",
                        Some(json!({"reply":{"reply_to":message_id,"message":"Foreign reply"}})),
                    ).await;
                    assert_eq!(status, StatusCode::OK, "{foreign_reply}");
                    let rendered = if cli { text(&foreign_reply).to_string() } else { foreign_reply.to_string() };
                    assert!(rendered.contains("operator_message_unavailable"), "{rendered}");
                    let after_foreign_reply = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
                    assert!(after_foreign_reply["messages"][0]["first_ack_observed_at_ms"].is_null(), "{after_foreign_reply}");
                    assert_eq!(after_foreign_reply["messages"].as_array().unwrap().len(), 1);

                    // Projection alone is not ACK. Only the client's explicit echo advances it.
                    let (status, acknowledged) = call_in_window(
                        &service, cli, stateless, raw_window,
                        Some(json!({"ack":[message_id], "reply":{"reply_to":message_id,"message":"Checks reviewed"}})),
                    ).await;
                    assert_eq!(status, StatusCode::OK, "{acknowledged}");
                    if !cli {
                        assert!(acknowledged["result"]["structuredContent"].is_object(), "{acknowledged}");
                        validate_schema_instance_for_test(
                            &acknowledged["result"]["structuredContent"], &output_schema,
                        ).expect("Adaptive discovery must describe the actual collaboration reply");
                    }

                    let rendered = if cli { text(&acknowledged).to_string() } else { acknowledged.to_string() };
                    assert!(rendered.contains("accepted_ids"), "{rendered}");
                    assert!(rendered.contains("window_reply"), "{rendered}");
                    let transcript = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
                    let messages = transcript["messages"].as_array().unwrap();
                    assert_eq!(messages.len(), 2, "{transcript}");
                    assert!(messages[0]["first_ack_observed_at_ms"].is_number(), "{transcript}");
                    assert_eq!(messages[1]["source"], "window");
                    assert_eq!(messages[1]["message"], "Checks reviewed");
                    assert_eq!(messages[1]["reply_to_message_id"], message_id);

                    // The same replay is idempotent.
                    let (_, replayed) = call_in_window(
                        &service, cli, stateless, raw_window,
                        Some(json!({"ack":[message_id], "reply":{"reply_to":message_id,"message":"Checks reviewed"}})),
                    ).await;
                    assert!(replayed.get("error").is_none(), "{replayed}");
                    assert_eq!(runtime.window_collaboration(Some(window.key()), Some(&auth), 10)["messages"].as_array().unwrap().len(), 2);
                }
            }
        })).unwrap().join().unwrap();
}
