
#[test]
fn reconnect_backoff_is_bounded_exponential() {
    let mut backoff = RetryBackoff::new(&RECONNECT_BACKOFF_STEPS);
    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    assert_eq!(backoff.next_delay(), Duration::from_secs(2));
    assert_eq!(backoff.next_delay(), Duration::from_secs(5));
    assert_eq!(backoff.next_delay(), Duration::from_secs(10));
    assert_eq!(backoff.next_delay(), Duration::from_secs(30));
    assert_eq!(backoff.next_delay(), Duration::from_secs(30));
    backoff.reset();
    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
}

#[test]
fn polling_idle_backoff_progression_cap_and_request_reset() {
    let mut backoff = PollingIdleBackoff::new(Duration::from_secs(1));
    assert_eq!(
        polling_idle_delay(&mut backoff, false),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        polling_idle_delay(&mut backoff, false),
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        polling_idle_delay(&mut backoff, false),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        polling_idle_delay(&mut backoff, false),
        Some(Duration::from_secs(5))
    );

    assert_eq!(polling_idle_delay(&mut backoff, true), None);
    assert_eq!(
        polling_idle_delay(&mut backoff, false),
        Some(Duration::from_secs(1))
    );

    let mut custom = PollingIdleBackoff::new(Duration::from_secs(3));
    assert_eq!(custom.next_delay(), Duration::from_secs(3));
    assert_eq!(custom.next_delay(), Duration::from_secs(5));
    assert_eq!(custom.next_delay(), Duration::from_secs(5));

    let mut above_default_cap = PollingIdleBackoff::new(Duration::from_secs(60));
    assert_eq!(above_default_cap.next_delay(), Duration::from_secs(60));
    assert_eq!(above_default_cap.next_delay(), Duration::from_secs(60));
}

#[test]
fn polling_recovery_backoff_is_bounded_and_resets() {
    let mut backoff = RetryBackoff::new(&POLLING_RECOVERY_BACKOFF_STEPS);
    assert_eq!(backoff.next_delay(), Duration::from_millis(500));
    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    assert_eq!(backoff.next_delay(), Duration::from_secs(2));
    assert_eq!(backoff.next_delay(), Duration::from_secs(5));
    assert_eq!(backoff.next_delay(), Duration::from_secs(10));
    assert_eq!(backoff.next_delay(), Duration::from_secs(10));
    backoff.reset();
    assert_eq!(backoff.next_delay(), Duration::from_millis(500));
}

#[test]
fn polling_lease_conflict_retry_has_a_finite_total_wait() {
    let mut backoff = RetryBackoff::new(&POLLING_RECOVERY_BACKOFF_STEPS);
    assert_eq!(
        next_lease_conflict_delay(
            &mut backoff,
            POLLING_LEASE_CONFLICT_MAX_WAIT - Duration::from_millis(250)
        ),
        Some(Duration::from_millis(250))
    );
    assert_eq!(
        next_lease_conflict_delay(&mut backoff, POLLING_LEASE_CONFLICT_MAX_WAIT),
        None
    );
}

#[test]
fn transport_error_classification_separates_transient_and_fatal() {
    let transient = classify_session_error("websocket connect failed: connection refused");
    assert!(!transient.is_fatal(), "{transient}");

    let proxy_network =
        RunnerTransportError::transient("websocket connect failed: proxy TCP connect failed");
    assert!(matches!(proxy_network, RunnerTransportError::Transient(_)));

    let fatal = classify_session_error("register rejected by server: unauthorized");
    assert!(fatal.is_fatal(), "{fatal}");

    let fatal =
        classify_session_error("quic connect failed: certificate verify failed; check server_name");
    assert!(fatal.is_fatal(), "{fatal}");
}

#[test]
fn stream_supervisor_once_semantics_are_explicit_and_shared() {
    for (mode, transport) in [
        (
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
            StreamTransport::WebSocket,
        ),
        (
            StreamSupervisorMode::Strict(StreamTransport::Quic),
            StreamTransport::Quic,
        ),
        (StreamSupervisorMode::Auto, StreamTransport::WebSocket),
        (StreamSupervisorMode::Auto, StreamTransport::Quic),
    ] {
        assert_eq!(
            decide_stream_session(
                mode,
                transport,
                true,
                Ok(RunnerSessionExit::TransportDisconnected)
            ),
            StreamSessionDecision::Complete { shutdown: false },
            "{mode:?} {transport:?} must stop after a completed once session"
        );
    }

    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Strict(transport),
                transport,
                true,
                Err(classify_session_error("connection refused")),
            ),
            StreamSessionDecision::Fatal(error) if error == "connection refused"
        ));
    }

    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Auto,
            StreamTransport::Quic,
            true,
            Err(classify_session_error("connection refused")),
        ),
        StreamSessionDecision::TryNext(RunnerTransportError::Transient(error))
            if error == "connection refused"
    ));
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Auto,
            StreamTransport::WebSocket,
            true,
            Err(classify_session_error("connection refused")),
        ),
        StreamSessionDecision::Fatal(error) if error == "connection refused"
    ));
}

#[test]
fn stream_supervisor_reconnect_and_auto_fallback_semantics_are_shared() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Strict(transport),
                transport,
                false,
                Ok(RunnerSessionExit::TransportDisconnected),
            ),
            StreamSessionDecision::Reconnect(None)
        ));
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Strict(transport),
                transport,
                false,
                Err(classify_session_error("connection refused")),
            ),
            StreamSessionDecision::Reconnect(Some(RunnerTransportError::Transient(error)))
                if error == "connection refused"
        ));
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Auto,
                transport,
                false,
                Ok(RunnerSessionExit::TransportDisconnected),
            ),
            StreamSessionDecision::Reconnect(None)
        ));
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Auto,
                transport,
                false,
                Err(classify_session_error("connection refused")),
            ),
            StreamSessionDecision::TryNext(RunnerTransportError::Transient(error))
                if error == "connection refused"
        ));
        assert!(matches!(
            decide_stream_session(
                StreamSupervisorMode::Auto,
                transport,
                false,
                Err(classify_session_error(
                    "register rejected by server: unauthorized",
                )),
            ),
            StreamSessionDecision::Fatal(error) if error.contains("register rejected")
        ));
    }
}

#[test]
fn websocket_proxy_configuration_errors_are_mode_sensitive() {
    let unsupported = parse_http_proxy_endpoint("socks5://proxy.test:1080")
        .expect_err("unsupported proxy scheme must fail configuration");
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
            StreamTransport::WebSocket,
            false,
            Err(unsupported),
        ),
        StreamSessionDecision::Fatal(error) if error.contains("proxy scheme is unsupported")
    ));

    let auth = parse_http_proxy_endpoint("http://proxy-user:proxy-pass@proxy.test:8080")
        .expect_err("proxy auth URL must fail configuration");
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
            StreamTransport::WebSocket,
            false,
            Err(auth),
        ),
        StreamSessionDecision::Fatal(error) if error.contains("proxy authentication is unsupported")
    ));

    let unsupported = parse_http_proxy_endpoint("socks5://proxy.test:1080")
        .expect_err("unsupported proxy scheme must fail configuration");
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Auto,
            StreamTransport::WebSocket,
            false,
            Err(unsupported),
        ),
        StreamSessionDecision::TryNext(RunnerTransportError::ProxyConfiguration(error))
            if error.contains("proxy scheme is unsupported")
    ));

    let proxy_auth_required = RunnerTransportError::proxy_configuration(
        "websocket connect failed: proxy CONNECT returned HTTP 407",
    );
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
            StreamTransport::WebSocket,
            false,
            Err(proxy_auth_required),
        ),
        StreamSessionDecision::Fatal(error) if error.contains("HTTP 407")
    ));

    let proxy_auth_required = RunnerTransportError::proxy_configuration(
        "websocket connect failed: proxy CONNECT returned HTTP 407",
    );
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Auto,
            StreamTransport::WebSocket,
            false,
            Err(proxy_auth_required),
        ),
        StreamSessionDecision::TryNext(RunnerTransportError::ProxyConfiguration(error))
            if error.contains("HTTP 407")
    ));

    let network =
        RunnerTransportError::transient("websocket connect failed: proxy TCP connect failed");
    assert!(matches!(
        decide_stream_session(
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
            StreamTransport::WebSocket,
            false,
            Err(network),
        ),
        StreamSessionDecision::Reconnect(Some(RunnerTransportError::Transient(error)))
            if error.contains("proxy TCP connect failed")
    ));
}

#[test]
fn auto_log_lines_are_concise_and_redacted() {
    assert_eq!(
        auto_quic_not_configured_log_line(),
        "webcodex-runner transport auto: quic not configured; skipping"
    );
    assert_eq!(
        auto_trying_log_line(TRANSPORT_WEBSOCKET),
        "webcodex-runner transport auto: websocket trying"
    );
    assert_eq!(
        auto_trying_log_line(TRANSPORT_POLLING),
        "webcodex-runner transport auto: polling trying"
    );

    let token = "DO_NOT_LEAK_THIS_TOKEN";
    let concise = concise_log_error(
        "websocket connect failed: token=DO_NOT_LEAK_THIS_TOKEN\nwhile connecting",
        token,
    );
    assert!(!concise.contains(token), "{concise}");
    assert!(!concise.contains('\n'), "{concise}");
}

#[test]
fn registered_log_includes_actual_transport_without_url_query_or_token() {
    let token = "DO_NOT_LEAK_THIS_TOKEN";
    let mut cfg = test_runner_config(format!(
        "https://webcodex.example.test/agent/path?token={}",
        token
    ));
    cfg.token = token.to_string();
    cfg.transport = Some(TRANSPORT_AUTO.to_string());

    let line = registered_log_line(&cfg, TRANSPORT_POLLING, 11);
    assert!(line.contains("client_id=oe"), "{line}");
    assert!(
        line.contains("server=https://webcodex.example.test"),
        "{line}"
    );
    assert!(line.contains("preferred_transport=auto"), "{line}");
    assert!(line.contains("actual_transport=polling"), "{line}");
    assert!(line.contains("projects=11"), "{line}");
    assert!(!line.contains(token), "{line}");
    assert!(!line.contains("/agent/path"), "{line}");
    assert!(!line.contains("?token="), "{line}");
}

#[test]
fn auto_websocket_failure_falls_back_to_polling() {
    let (server_url, poll_count, server) =
        start_auto_fallback_http_server("502 Bad Gateway", "text/html", "<html>bad gateway</html>");
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_runner_config(server_url);
    cfg.transport = Some(TRANSPORT_AUTO.to_string());
    cfg.project_registry_dir = Some(tmp.path().join("project-registry"));
    cfg.websocket_connect_timeout_secs = 1;

    let runtime = test_runtime(&cfg);
    let err = run_auto_runner(cfg, false, "inst-auto-fallback", &runtime)
        .expect_err("terminal polling 404 should stop after recovering the first 502");
    server.join().unwrap();
    assert_eq!(poll_count.load(Ordering::SeqCst), 2);
    assert!(
        err.contains("poll endpoint missing or incompatible server"),
        "{err}"
    );
}

#[test]
fn polling_502_html_is_transient_and_sanitized() {
    let nginx_html = "<html>\n<head><title>502 Bad Gateway</title></head>\n<body>\n<center><h1>502 Bad Gateway</h1></center>\n<hr><center>nginx/1.31.1</center>\n</body>\n</html>";
    let error = RunnerHttpError::status(
        "/api/shell/agent/poll",
        reqwest::StatusCode::BAD_GATEWAY,
        nginx_html,
    );
    let poll_error = crate::webcodex_runner::transport::poll_dispatch::PollError::from_http(error, "oe");
    assert_eq!(
        poll_error.recovery_action(),
        PollingRecoveryAction::RetryPoll
    );
    let message = poll_error.to_string();
    assert!(
        message.contains(
            "server unavailable while polling /api/shell/agent/poll: HTTP 502 Bad Gateway"
        ),
        "{message}"
    );
    assert!(!message.contains("<html"), "{message}");
    assert!(!message.contains("nginx/1.31.1"), "{message}");
    assert!(!message.contains("<center><h1>502 Bad Gateway</h1></center>"));
}

#[test]
fn polling_503_and_504_are_transient_server_unavailable() {
    for (status, expected) in [
        (
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "server unavailable while polling /api/shell/agent/poll: HTTP 503 Service Unavailable",
        ),
        (
            reqwest::StatusCode::GATEWAY_TIMEOUT,
            "server unavailable while polling /api/shell/agent/poll: HTTP 504 Gateway Timeout",
        ),
    ] {
        let error = RunnerHttpError::status("/api/shell/agent/poll", status, "proxy unavailable");
        let poll_error = crate::webcodex_runner::transport::poll_dispatch::PollError::from_http(error, "oe");
        assert_eq!(
            poll_error.recovery_action(),
            PollingRecoveryAction::RetryPoll
        );
        let message = poll_error.to_string();
        assert!(message.contains(expected), "{message}");
        assert!(!message.contains("proxy unavailable"), "{message}");
    }
}

#[test]
fn polling_401_and_403_are_terminal_auth_errors() {
    for (status, expected) in [
            (
                "401 Unauthorized",
                "authentication failed while polling /api/shell/agent/poll: HTTP 401 Unauthorized; check agent token/config",
            ),
            (
                "403 Forbidden",
                "authentication failed while polling /api/shell/agent/poll: HTTP 403 Forbidden; check agent token/config",
            ),
        ] {
            let (result, poll_count) = run_polling_runner_against_server(
                status,
                "application/json",
                r#"{"error":"unauthorized"}"#,
                false,
            );
            let error = result.expect_err("auth poll response must stop the foreground agent");

            assert_eq!(poll_count, 1);
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("unauthorized\""), "{error}");
        }
}

#[test]
fn polling_once_completes_canonical_inventory_without_extra_business_poll() {
    let (result, poll_count) = run_polling_runner_against_server(
        "200 OK",
        "application/json",
        r#"{"success":true,"request":null,"error":null}"#,
        true,
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(poll_count, 1);
}

#[test]
fn polling_register_and_ordinary_poll_both_omit_inline_projects() {
    let server = start_scripted_runner_server(vec![ScriptStep::Register, ScriptStep::PollEmpty]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("empty polling turn should stop cleanly with scripted shutdown");
    server.handle.join().unwrap();

    let requests = server.requests.lock().unwrap();
    let register: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    let poll: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
    assert!(
        register["projects"].is_null(),
        "generation-2 registration must not carry inline projects"
    );
    assert!(
        poll["projects"].is_null(),
        "ordinary poll must not revive inline project refresh"
    );
}

#[test]
fn polling_graceful_shutdown_sends_instance_scoped_offline_notice() {
    let poll_seen = Arc::new(AtomicBool::new(false));
    let offline_seen = Arc::new(AtomicBool::new(false));
    let seen_poll = Arc::clone(&poll_seen);
    let seen_offline = Arc::clone(&offline_seen);
    let server = start_concurrent_polling_server(Arc::new(move |path, body| match path {
        "/api/shell/agent/register" => register_inventory_support_response(),
        "/api/shell/agent/poll" => {
            seen_poll.store(true, Ordering::SeqCst);
            ConcurrentHttpResponse::json(r#"{"success":true,"request":null,"error":null}"#)
        }
        "/api/shell/agent/offline" => {
            let value: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(value["client_id"], "oe");
            assert_eq!(value["agent_instance_id"], "inst-offline");
            seen_offline.store(true, Ordering::SeqCst);
            ConcurrentHttpResponse::json(r#"{"success":true,"error":null}"#)
        }
        other => panic!("unexpected polling shutdown request: {other}"),
    }));
    let tmp = tempfile::tempdir().unwrap();
    let cfg = polling_runner_config(
        server.server_url.clone(),
        tmp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let shutdown = Arc::new(AtomicBool::new(false));
    let handle = spawn_polling_runner(cfg, runtime, false, "inst-offline", Arc::clone(&shutdown));

    let deadline = Instant::now() + Duration::from_secs(5);
    while !poll_seen.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        poll_seen.load(Ordering::SeqCst),
        "polling Runner never became active"
    );
    shutdown.store(true, Ordering::SeqCst);
    handle
        .finish(Duration::from_secs(5), "polling graceful offline")
        .unwrap();
    assert!(offline_seen.load(Ordering::SeqCst));
    server.finish();
}
