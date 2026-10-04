// Reconnect metadata policy and auto transport promotion use the actual
// supervisors over bounded loopback fixtures, without production services.

#[test]
fn reconnect_inventory_metadata_retries_but_authority_errors_remain_fatal() {
    use super::registration::{RegisterError, RegisterRecoveryAction};
    for message in [
        "job inventory shell is invalid",
        "job inventory timestamps are inconsistent",
    ] {
        let body = serde_json::json!({"success": false, "error": message}).to_string();
        let http = RunnerHttpError::status(
            "/api/shell/agent/register",
            reqwest::StatusCode::BAD_REQUEST,
            &body,
        );
        assert_eq!(
            RegisterError::from_http(http, "oe").recovery_action(),
            RegisterRecoveryAction::Retry
        );
        assert_eq!(
            RegisterError::from_response_error("oe", Some(message.into())).recovery_action(),
            RegisterRecoveryAction::Retry
        );
        for prefix in [
            "register rejected by server: ",
            "server error during register register_failed: ",
        ] {
            for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
                let error = classify_session_error(format!("{prefix}{message}"));
                assert!(matches!(error, RunnerTransportError::InventoryRejected(_)));
                assert!(matches!(
                    decide_stream_session(StreamSupervisorMode::Auto, transport, false, Err(error),),
                    StreamSessionDecision::Reconnect(Some(
                        RunnerTransportError::InventoryRejected(_)
                    ))
                ));
            }
        }
    }
    for message in [
        "unauthorized",
        "runner identity is unavailable",
        "job inventory runtime_project_id does not belong to client_id",
        "job inventory shell is invalid or oversized",
        "job inventory shell is invalid: unauthorized",
        "job_state_reconciliation capability requires job_inventory",
    ] {
        let body = serde_json::json!({"success": false, "error": message}).to_string();
        let http = RunnerHttpError::status(
            "/api/shell/agent/register",
            reqwest::StatusCode::BAD_REQUEST,
            &body,
        );
        assert_eq!(
            RegisterError::from_http(http, "oe").recovery_action(),
            RegisterRecoveryAction::Fatal
        );
        assert!(classify_session_error(format!(
            "server error during register register_failed: {message}"
        ))
        .is_fatal());
    }
}

#[test]
fn reconnect_polling_inventory_rejection_preserves_registration_identity() {
    for body in [
        r#"{"success":false,"error":"job inventory shell is invalid"}"#,
        r#"{"success":false,"error":"job inventory timestamps are inconsistent"}"#,
    ] {
        let server = start_scripted_runner_server(vec![
            ScriptStep::RegisterResponse {
                status: "400 Bad Request",
                body,
            },
            ScriptStep::Register,
            ScriptStep::PollEmpty,
        ]);
        run_polling_runner_against_scripted_server(&server, false).unwrap();
        server.handle.join().unwrap();
        let requests = server.requests.lock().unwrap();
        let registrations = requests
            .iter()
            .filter(|(path, _)| path == "/api/shell/agent/register")
            .map(|(_, body)| {
                serde_json::from_str::<webcodex_core::runner_protocol::RunnerRegisterRequest>(body)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(registrations.len(), 2);
        assert!(!registrations[0].runner_instance_id.is_empty());
        assert_eq!(
            registrations[0].runner_instance_id,
            registrations[1].runner_instance_id
        );
        assert_eq!(
            registrations[0].job_inventory,
            registrations[1].job_inventory
        );
        assert!(
            registrations[1]
                .job_inventory
                .as_ref()
                .unwrap()
                .active_complete
        );
    }
}

#[test]
fn reconnect_auto_polling_promotes_back_to_websocket_with_same_instance() {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    cfg.transport = Some(TRANSPORT_AUTO.to_string());
    cfg.project_registry_dir = Some(temp.path().join("projects"));
    cfg.websocket_connect_timeout_secs = 1;
    let runtime = test_runtime(&cfg);
    let server_runtime = runtime.clone();
    let shutdown_runtime = runtime.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let failsafe = thread::spawn(move || {
        if done_rx.recv_timeout(Duration::from_secs(15)).is_err() {
            shutdown_runtime.request_shutdown_signal();
        }
    });
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut websocket_attempts = 0;
        let mut registered_inventory = None;
        let mut polls = 0;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "auto did not leave polling");
            let mut stream = accept_with_deadline(&listener, remaining);
            stream.set_read_timeout(Some(remaining)).unwrap();
            let mut peek = [0u8; 128];
            let n = loop {
                let n = stream.peek(&mut peek).unwrap();
                assert!(n > 0 && Instant::now() < deadline, "incomplete HTTP header");
                if n >= 4 {
                    break n;
                }
                thread::sleep(Duration::from_millis(1));
            };
            if peek[..n].starts_with(b"GET ") {
                websocket_attempts += 1;
                if websocket_attempts == 1 {
                    let request = read_http_request(&mut stream);
                    assert_eq!(request_path(&request), "/api/agents/ws");
                    write_http_response(
                        &mut stream,
                        "503 Service Unavailable",
                        "text/plain",
                        "restart",
                    );
                    continue;
                }
                assert!(polls > 0, "fixture must first establish polling fallback");
                stream.set_nonblocking(true).unwrap();
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    tokio::time::timeout(Duration::from_secs(5), async {
                        let stream = tokio::net::TcpStream::from_std(stream).unwrap();
                        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                        let msg = ws.next().await.unwrap().unwrap().into_text().unwrap();
                        let RunnerEnvelope::Register { payload } =
                            RunnerEnvelope::from_slice(msg.as_bytes()).unwrap()
                        else {
                            panic!("expected registration on promoted WebSocket");
                        };
                        assert_eq!(payload.runner_instance_id, "same-auto-instance");
                        assert_eq!(payload.job_inventory, registered_inventory);
                        send_registered_ack(&mut ws).await;
                        server_runtime.request_shutdown_signal();
                        while let Some(Ok(message)) = ws.next().await {
                            if message.is_close() {
                                break;
                            }
                        }
                    })
                    .await
                    .expect("promoted WebSocket handshake/shutdown deadline");
                });
                return;
            }
            let request = read_http_request(&mut stream);
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let response = match request_path(&request) {
                "/api/shell/agent/register" => {
                    let request: webcodex_core::runner_protocol::RunnerRegisterRequest =
                        serde_json::from_str(body).unwrap();
                    assert_eq!(request.runner_instance_id, "same-auto-instance");
                    registered_inventory = request.job_inventory;
                    register_inventory_support_response()
                }
                "/api/shell/agent/poll" => {
                    polls += 1;
                    project_inventory_poll_response(body).unwrap_or_else(|| {
                        ConcurrentHttpResponse::json(
                            r#"{"success":true,"request":null,"error":null}"#,
                        )
                    })
                }
                path => panic!("unexpected request during transport handoff: {path}"),
            };
            write_http_response(
                &mut stream,
                response.status,
                response.content_type,
                &response.body,
            );
        }
    });
    let result = supervisor::run_auto_runner_with_retry_interval(
        cfg,
        false,
        "same-auto-instance",
        &runtime,
        Duration::from_millis(500),
    );
    let _ = done_tx.send(());
    failsafe.join().unwrap();
    server.join().unwrap();
    result.unwrap();
}
