
#[test]
fn websocket_proxy_env_precedence_and_no_proxy_bypass() {
    use std::ffi::OsString;

    let values = std::collections::HashMap::from([
        (
            "HTTPS_PROXY",
            OsString::from("http://https-upper.test:8001"),
        ),
        (
            "https_proxy",
            OsString::from("http://https-lower.test:8002"),
        ),
        ("HTTP_PROXY", OsString::from("http://http-upper.test:8003")),
        ("http_proxy", OsString::from("http://http-lower.test:8004")),
        ("ALL_PROXY", OsString::from("http://all-upper.test:8005")),
        ("all_proxy", OsString::from("http://all-lower.test:8006")),
    ]);
    let wss =
        websocket_proxy_from_env_with("wss://example.test/ws", |name| values.get(name).cloned())
            .unwrap()
            .unwrap();
    assert_eq!(wss.host, "https-upper.test");
    assert_eq!(wss.port, 8001);
    let ws =
        websocket_proxy_from_env_with("ws://example.test/ws", |name| values.get(name).cloned())
            .unwrap()
            .unwrap();
    assert_eq!(ws.host, "http-upper.test");
    assert_eq!(ws.port, 8003);

    let fallback_values = std::collections::HashMap::from([
        (
            "https_proxy",
            OsString::from("http://https-lower-only.test:8101"),
        ),
        ("ALL_PROXY", OsString::from("http://all-fallback.test:8102")),
    ]);
    let wss = websocket_proxy_from_env_with("wss://example.test/ws", |name| {
        fallback_values.get(name).cloned()
    })
    .unwrap()
    .unwrap();
    assert_eq!(wss.host, "https-lower-only.test");
    assert_eq!(wss.port, 8101);

    let all_only = std::collections::HashMap::from([(
        "all_proxy",
        OsString::from("http://all-lower-only.test:8103"),
    )]);
    let ws =
        websocket_proxy_from_env_with("ws://example.test/ws", |name| all_only.get(name).cloned())
            .unwrap()
            .unwrap();
    assert_eq!(ws.host, "all-lower-only.test");
    assert_eq!(ws.port, 8103);

    for (url, no_proxy, bypass) in [
        ("ws://localhost/ws", "localhost", true),
        ("ws://127.0.0.1/ws", "127.0.0.1", true),
        ("wss://api.example.com/ws", "api.example.com", true),
        ("wss://deep.example.com/ws", ".example.com", true),
        (
            "wss://api.example.com:8443/ws",
            "api.example.com:8443",
            true,
        ),
        ("wss://api.example.com/ws", "api.example.com:8443", false),
        ("wss://anything.test/ws", "*", true),
    ] {
        let values = std::collections::HashMap::from([
            ("HTTP_PROXY", OsString::from("http://proxy.test:8080")),
            ("HTTPS_PROXY", OsString::from("http://proxy.test:8080")),
            ("NO_PROXY", OsString::from(no_proxy)),
        ]);
        let selected =
            websocket_proxy_from_env_with(url, |name| values.get(name).cloned()).unwrap();
        assert_eq!(selected.is_none(), bypass, "url={url} no_proxy={no_proxy}");
    }

    let lower_no_proxy = std::collections::HashMap::from([
        ("HTTP_PROXY", OsString::from("http://proxy.test:8080")),
        ("no_proxy", OsString::from("localhost")),
    ]);
    assert!(websocket_proxy_from_env_with("ws://localhost/ws", |name| {
        lower_no_proxy.get(name).cloned()
    })
    .unwrap()
    .is_none());
}

#[test]
fn websocket_proxy_ipv6_hosts_are_canonical() {
    use std::ffi::OsString;

    let proxy = parse_http_proxy_endpoint("http://[::1]:8080").unwrap();
    assert_eq!(proxy.host, "::1");
    assert_eq!(proxy.port, 8080);

    let (target_host, target_port) =
        websocket_target_endpoint("wss://[2001:db8::1]/api/agents/ws").unwrap();
    assert_eq!(target_host, "2001:db8::1");
    assert_eq!(target_port, 443);
    let authority = target_authority(&target_host, target_port);
    assert_eq!(authority, "[2001:db8::1]:443");
    assert!(!authority.contains("[["), "{authority}");

    let values = std::collections::HashMap::from([
        ("HTTP_PROXY", OsString::from("http://proxy.test:8080")),
        ("NO_PROXY", OsString::from("::1")),
    ]);
    assert!(
        websocket_proxy_from_env_with("ws://[::1]/api/agents/ws", |name| {
            values.get(name).cloned()
        })
        .unwrap()
        .is_none()
    );
}

#[test]
fn websocket_proxy_invalid_configuration_is_sanitized() {
    use std::ffi::OsString;

    let proxy_secret = "PROXY_PASSWORD_DO_NOT_LEAK";
    let query_secret = "PROXY_QUERY_DO_NOT_LEAK";
    let raw = format!("http://proxy-user:{proxy_secret}@proxy.test:8080/?token={query_secret}");
    let values = std::collections::HashMap::from([("HTTP_PROXY", OsString::from(raw))]);
    let error =
        websocket_proxy_from_env_with("ws://server.test/ws", |name| values.get(name).cloned())
            .expect_err("credentialed proxy URL is intentionally unsupported");
    assert!(matches!(error, RunnerTransportError::ProxyConfiguration(_)));
    let error = error.to_string();
    assert!(
        error.contains("proxy authentication is unsupported"),
        "{error}"
    );
    assert!(!error.contains("proxy-user"), "{error}");
    assert!(!error.contains(proxy_secret), "{error}");
    assert!(!error.contains(query_secret), "{error}");
}

#[tokio::test]
async fn websocket_http_proxy_connect_tunnels_websocket_handshake() {
    use tokio::io::AsyncWriteExt;

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();
    let seen_auth = Arc::new(Mutex::new(None::<String>));
    let server_seen_auth = Arc::clone(&seen_auth);
    let target = tokio::spawn(async move {
        let (stream, _) = target_listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async(
            stream,
            move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                  response: tokio_tungstenite::tungstenite::handshake::server::Response| {
            *server_seen_auth.lock().unwrap() = request
                .headers()
                .get(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            Ok(response)
        })
        .await
        .unwrap();
        let _ = ws.send(WsMessage::Close(None)).await;
    });

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut downstream, _) = proxy_listener.accept().await.unwrap();
        let connect = read_async_http_headers(&mut downstream).await;
        let expected = format!("CONNECT {target_addr} HTTP/1.1");
        assert!(connect.starts_with(&expected), "{connect}");
        assert!(
            !connect.to_ascii_lowercase().contains("authorization:"),
            "{connect}"
        );
        assert!(!connect.contains("SERVER_TOKEN_DO_NOT_LEAK"), "{connect}");
        let mut upstream = tokio::net::TcpStream::connect(target_addr).await.unwrap();
        downstream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
        connect
    });

    let token = "SERVER_TOKEN_DO_NOT_LEAK";
    let ws_url = format!("ws://{target_addr}/api/agents/ws");
    let request = build_ws_request(&ws_url, token).unwrap();
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: proxy_addr.port(),
    };
    let ws = tokio::time::timeout(
        Duration::from_secs(5),
        connect_websocket_request_with_proxy(request, &ws_url, Some(&endpoint), token),
    )
    .await
    .expect("proxy websocket connect timed out")
    .expect("proxy websocket connect failed");
    drop(ws);

    let connect = tokio::time::timeout(Duration::from_secs(5), proxy)
        .await
        .expect("proxy tunnel task did not finish")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), target)
        .await
        .expect("target websocket task did not finish")
        .unwrap();
    assert!(connect.starts_with(&format!("CONNECT {target_addr} HTTP/1.1")));
    assert_eq!(
        seen_auth.lock().unwrap().as_deref(),
        Some("Bearer SERVER_TOKEN_DO_NOT_LEAK")
    );
}

#[tokio::test]
async fn websocket_proxy_ipv6_target_connect_authority_has_single_brackets() {
    use tokio::io::AsyncWriteExt;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_async_http_headers(&mut stream).await;
        assert!(
            connect.starts_with("CONNECT [2001:db8::1]:443 HTTP/1.1"),
            "{connect}"
        );
        assert!(!connect.contains("[[2001:db8::1]]"), "{connect}");
        stream
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
        connect
    });

    let ws_url = "wss://[2001:db8::1]/api/agents/ws";
    let request = build_ws_request(ws_url, "SERVER_TOKEN_DO_NOT_LEAK").unwrap();
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let error = connect_websocket_request_with_proxy(
        request,
        ws_url,
        Some(&endpoint),
        "SERVER_TOKEN_DO_NOT_LEAK",
    )
    .await
    .expect_err("synthetic proxy must reject the IPv6 target");
    let connect = proxy.await.unwrap();

    assert!(connect.starts_with("CONNECT [2001:db8::1]:443 HTTP/1.1"));
    assert!(
        matches!(error, RunnerTransportError::Transient(_)),
        "{error}"
    );
}

#[tokio::test]
async fn websocket_wss_proxy_uses_connect_before_target_tls() {
    use tokio::io::AsyncWriteExt;

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let server_token = "SERVER_TOKEN_DO_NOT_LEAK";
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut stream, _) = proxy_listener.accept().await.unwrap();
        let connect = read_async_http_headers(&mut stream).await;
        assert!(
            connect.starts_with("CONNECT server.test:443 HTTP/1.1"),
            "{connect}"
        );
        assert!(
            !connect.to_ascii_lowercase().contains("authorization:"),
            "{connect}"
        );
        assert!(!connect.contains(server_token), "{connect}");
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        connect
    });

    let ws_url = "wss://server.test/api/agents/ws";
    let request = build_ws_request(ws_url, server_token).unwrap();
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: proxy_addr.port(),
    };
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        connect_websocket_request_with_proxy(request, ws_url, Some(&endpoint), server_token),
    )
    .await
    .expect("wss proxy CONNECT test timed out")
    .expect_err("the synthetic tunnel closes before target TLS can complete");
    let connect = proxy.await.unwrap();

    assert!(connect.starts_with("CONNECT server.test:443 HTTP/1.1"));
    let error = error.to_string();
    assert!(error.contains("websocket connect failed"), "{error}");
    assert!(!error.contains(server_token), "{error}");
}

#[tokio::test]
async fn websocket_proxy_network_failure_remains_transient() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
    });
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let error = http_proxy_connect_tunnel(&endpoint, "server.test", 443)
        .await
        .expect_err("closed proxy connection must fail");
    peer.await.unwrap();
    assert!(
        matches!(error, RunnerTransportError::Transient(_)),
        "{error}"
    );
}

#[tokio::test]
async fn websocket_proxy_connect_rejects_non_success_without_leaking_secrets() {
    use tokio::io::AsyncWriteExt;

    let proxy_secret = "PROXY_RESPONSE_SECRET_DO_NOT_LEAK";
    let server_token = "SERVER_TOKEN_DO_NOT_LEAK";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_async_http_headers(&mut stream).await;
        assert!(!connect.contains(server_token), "{connect}");
        let response = format!(
            "HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: {}\r\n\r\n{}",
            proxy_secret.len(),
            proxy_secret
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let ws_url = "ws://127.0.0.1:9/api/agents/ws";
    let request = build_ws_request(ws_url, server_token).unwrap();
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        connect_websocket_request_with_proxy(request, ws_url, Some(&endpoint), server_token),
    )
    .await
    .expect("non-success CONNECT test timed out")
    .expect_err("non-2xx CONNECT status must fail");
    proxy.await.unwrap();
    assert!(
        matches!(&error, RunnerTransportError::ProxyConfiguration(_)),
        "{error}"
    );
    let error = error.to_string();
    assert!(error.contains("HTTP 407"), "{error}");
    assert!(!error.contains(proxy_secret), "{error}");
    assert!(!error.contains(server_token), "{error}");
}

#[tokio::test]
async fn websocket_proxy_connect_response_header_is_bounded_and_redacted() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let proxy_secret = "PROXY_RESPONSE_SECRET_DO_NOT_LEAK";
    let server_token = "SERVER_TOKEN_DO_NOT_LEAK";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_async_http_headers(&mut stream).await;
        assert!(!connect.contains(server_token), "{connect}");
        let mut response = format!("HTTP/1.1 200 OK\r\nX-Secret: {proxy_secret}\r\n").into_bytes();
        assert!(response.len() < WS_PROXY_CONNECT_HEADER_MAX_BYTES);
        response.resize(WS_PROXY_CONNECT_HEADER_MAX_BYTES, b'x');
        stream.write_all(&response).await.unwrap();
        stream.flush().await.unwrap();
        // Keep the synthetic proxy alive until the client consumes the bounded
        // header prefix and closes. Dropping immediately can surface a Windows
        // connection-reset error before the client observes the size fence.
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte).await;
    });

    let ws_url = "ws://127.0.0.1:9/api/agents/ws";
    let request = build_ws_request(ws_url, server_token).unwrap();
    let endpoint = HttpProxyEndpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        connect_websocket_request_with_proxy(request, ws_url, Some(&endpoint), server_token),
    )
    .await
    .expect("bounded CONNECT response test timed out")
    .expect_err("oversized CONNECT headers must fail");
    proxy.await.unwrap();
    let error = error.to_string();
    assert!(error.contains("response headers exceeded"), "{error}");
    assert!(!error.contains(proxy_secret), "{error}");
    assert!(!error.contains(server_token), "{error}");
}

#[test]
fn quic_transport_config_applies_keepalive_without_changing_application_ping() {
    let quic = QuicClientConfig {
        server_addr: "127.0.0.1:8443".to_string(),
        server_name: "localhost".to_string(),
        alpn: crate::webcodex_runner::default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 17,
    };
    let transport = build_quic_transport_config(&quic).unwrap();
    let rendered = format!("{transport:?}");
    assert!(
        rendered.contains("max_idle_timeout: Some(30000)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("keep_alive_interval: Some(17s)"),
        "{rendered}"
    );
    assert_eq!(QUIC_PING_INTERVAL, Duration::from_secs(30));
}

#[tokio::test]
async fn quic_graceful_writer_waits_for_flush_and_stuck_writer_is_bounded() {
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let (flushed_tx, flushed_rx) = tokio::sync::oneshot::channel::<()>();
    let writer = tokio::spawn(async move {
        let _ = release_rx.await;
        let _ = flushed_tx.send(());
        StreamWriterExit::GracefulClose
    });
    let finish = finish_quic_writer(Some(writer), true, Duration::from_secs(1));
    tokio::pin!(finish);
    tokio::select! {
        _ = &mut finish => panic!("graceful QUIC finish returned before writer flush"),
        _ = tokio::task::yield_now() => {}
    }
    release_tx.send(()).unwrap();
    assert!(
        finish.await,
        "graceful writer must report a flushed Goodbye"
    );
    flushed_rx.await.expect("writer flush marker");

    let stuck = tokio::spawn(async { std::future::pending::<StreamWriterExit>().await });
    let stuck_graceful = tokio::time::timeout(
        Duration::from_millis(100),
        finish_quic_writer(Some(stuck), true, Duration::from_millis(10)),
    )
    .await
    .expect("stuck graceful QUIC writer must be bounded");
    assert!(!stuck_graceful);

    let broken = tokio::spawn(async { std::future::pending::<StreamWriterExit>().await });
    let broken_graceful = tokio::time::timeout(
        Duration::from_millis(100),
        finish_quic_writer(Some(broken), false, Duration::from_secs(1)),
    )
    .await
    .expect("non-graceful QUIC writer teardown must abort promptly");
    assert!(!broken_graceful);
}

#[tokio::test]
async fn quic_graceful_close_waits_for_peer_within_remaining_budget() {
    let (peer_closed_tx, peer_closed_rx) = tokio::sync::oneshot::channel::<()>();
    let wait = wait_for_quic_peer_close(
        async {
            let _ = peer_closed_rx.await;
        },
        Duration::from_secs(1),
    );
    tokio::pin!(wait);

    tokio::select! {
        _ = &mut wait => panic!("graceful QUIC close skipped the peer-close grace period"),
        _ = tokio::task::yield_now() => {}
    }

    peer_closed_tx.send(()).unwrap();
    wait.await;

    tokio::time::timeout(
        Duration::from_millis(100),
        wait_for_quic_peer_close(std::future::pending::<()>(), Duration::from_millis(10)),
    )
    .await
    .expect("non-responsive QUIC peer grace wait must remain bounded");
}

#[tokio::test]
async fn streaming_writer_failure_terminates_pending_reader_for_ws_and_quic() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let cfg = test_runner_config("http://127.0.0.1:9".to_string());
        let runtime = test_runtime(&cfg);
        let registered_jobs = ShellJobInventory::default();
        let (out_tx, _out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
        let (_read_tx, read_rx) = tokio::sync::mpsc::channel::<StreamRead>(1);
        let writer_task = tokio::spawn(async { StreamWriterExit::TransportFailed });

        let exit = tokio::time::timeout(
            Duration::from_millis(250),
            serve_registered_stream(
                transport,
                &cfg,
                "inst-writer-fail",
                &registered_jobs,
                out_tx,
                RegisteredStream::Test { reader: read_rx },
                writer_task,
                None,
                &runtime,
                std::future::pending::<()>(),
            ),
        )
        .await
        .expect("writer failure must not wait for pending reader")
        .expect("writer failure is a transport disconnect, not a session error");
        assert_eq!(
            exit,
            RunnerSessionExit::TransportDisconnected,
            "{transport:?}"
        );
    }
}

#[tokio::test]
async fn streaming_graceful_writer_completion_is_not_a_failure_signal() {
    let cfg = test_runner_config("http://127.0.0.1:9".to_string());
    let runtime = test_runtime(&cfg);
    let registered_jobs = ShellJobInventory::default();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
    let (_read_tx, read_rx) = tokio::sync::mpsc::channel::<StreamRead>(1);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            if matches!(env, RunnerEnvelope::Goodbye { .. }) {
                return StreamWriterExit::GracefulClose;
            }
        }
        StreamWriterExit::ChannelClosed
    });

    let exit = serve_registered_stream(
        StreamTransport::WebSocket,
        &cfg,
        "inst-graceful",
        &registered_jobs,
        out_tx,
        RegisteredStream::Test { reader: read_rx },
        writer_task,
        None,
        &runtime,
        async {},
    )
    .await
    .expect("graceful shutdown must not be classified as writer failure");
    assert_eq!(exit, RunnerSessionExit::Shutdown);
}

#[tokio::test]
async fn websocket_close_returns_transport_disconnect_not_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        ws.send(WsMessage::Close(None)).await.unwrap();
        if let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(200), ws.next()).await
        {
            if msg.is_text() {
                let env = RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap();
                assert!(
                    !matches!(env, RunnerEnvelope::Goodbye { .. }),
                    "ordinary transport disconnect must not send Goodbye"
                );
            }
        }
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let exit = tokio::time::timeout(
        Duration::from_secs(5),
        websocket_session(
            &cfg,
            vec![test_project("close-test")],
            "inst-close",
            &test_runtime(&cfg),
        ),
    )
    .await
    .expect("session completed")
    .expect("session should not error");

    assert_eq!(exit, RunnerSessionExit::TransportDisconnected);
    assert_ne!(exit, RunnerSessionExit::Shutdown);
    server.await.unwrap();
}

#[tokio::test]
async fn websocket_disconnect_with_active_job_returns_without_waiting_for_job() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let request = start_job_request(cwd.path(), "sleep 2");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        ws.send(WsMessage::Text(
            RunnerEnvelope::Request { request }
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let msg = ws.next().await.unwrap().unwrap();
                if !msg.is_text() {
                    continue;
                }
                match RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap() {
                    RunnerEnvelope::JobUpdate { payload }
                        if payload.job_id == "job-active" && !payload.finished =>
                    {
                        break;
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("agent did not report active job");

        ws.send(WsMessage::Close(None)).await.unwrap();
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let started = Instant::now();
    let exit = tokio::time::timeout(
        Duration::from_millis(900),
        websocket_session(
            &cfg,
            vec![test_project("active-job-test")],
            "inst-active-job",
            &test_runtime(&cfg),
        ),
    )
    .await
    .expect("session must return promptly after disconnect despite active job")
    .expect("session should not error");

    assert_eq!(exit, RunnerSessionExit::TransportDisconnected);
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "session waited for the active job instead of reconnecting"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn websocket_register_rejected_is_fatal() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_register_rejected_ack(&mut ws).await;
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let error = websocket_session(
        &cfg,
        vec![test_project("reject-test")],
        "inst-reject",
        &test_runtime(&cfg),
    )
    .await
    .expect_err("register rejection must error");
    let classified = classify_session_error(error);
    assert!(classified.is_fatal(), "{classified}");
    server.await.unwrap();
}

#[tokio::test]
async fn strict_websocket_transient_connect_failure_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (first_stream, _) = listener.accept().await.unwrap();
        drop(first_stream);

        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_register_rejected_ack(&mut ws).await;
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let runtime = test_runtime(&cfg);
    let started = Instant::now();
    let runner = tokio::task::spawn_blocking(move || {
        run_websocket_runner(cfg, false, "inst-retry", &runtime)
    });
    let error = tokio::time::timeout(Duration::from_secs(5), runner)
        .await
        .expect("strict websocket retry did not finish after fatal register rejection")
        .unwrap()
        .expect_err("register rejection after reconnect must be fatal");

    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "strict websocket did not wait for reconnect backoff after transient connect failure"
    );
    assert!(error.contains("register rejected"), "{error}");
    server.await.unwrap();
}

#[tokio::test]
async fn strict_websocket_once_stops_after_first_registered_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        ws.send(WsMessage::Close(None)).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1_500), listener.accept())
                .await
                .is_err(),
            "--once must not open a reconnecting websocket session"
        );
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let runtime = test_runtime(&cfg);
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || run_websocket_runner(cfg, true, "inst-once", &runtime)),
    )
    .await
    .expect("websocket --once did not stop after the first disconnect")
    .unwrap()
    .expect("registered websocket --once disconnect should be successful");
    server.await.unwrap();
}

#[tokio::test]
async fn auto_websocket_register_rejected_is_fatal_without_polling_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_register_rejected_ack(&mut ws).await;
    });

    let mut cfg = test_runner_config(format!("http://{}", addr));
    cfg.transport = Some(TRANSPORT_AUTO.to_string());
    let runtime = test_runtime(&cfg);
    let runner = tokio::task::spawn_blocking(move || {
        run_auto_runner(cfg, false, "inst-auto-reject", &runtime)
    });
    let error = tokio::time::timeout(Duration::from_secs(5), runner)
        .await
        .expect("auto websocket register rejection did not return")
        .unwrap()
        .expect_err("fatal register rejection must not fall back to polling");

    assert!(error.contains("register rejected"), "{error}");
    server.await.unwrap();
}

#[tokio::test]
async fn websocket_disconnect_loop_reregisters_identity_generation_and_capabilities() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (reg_tx, mut reg_rx) = mpsc::channel(2);
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let register = read_register(&mut ws).await;
            reg_tx.send(register).await.unwrap();
            send_registered_ack(&mut ws).await;
            ws.send(WsMessage::Close(None)).await.unwrap();
        }
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let projects = vec![test_project("repo-one")];
    for instance in ["inst-reconnect", "inst-reconnect"] {
        let exit = tokio::time::timeout(
            Duration::from_secs(5),
            websocket_session(&cfg, projects.clone(), instance, &test_runtime(&cfg)),
        )
        .await
        .expect("session completed")
        .expect("session should not error");
        assert_eq!(exit, RunnerSessionExit::TransportDisconnected);
    }

    let first = reg_rx.recv().await.expect("first register");
    let second = reg_rx.recv().await.expect("second register");
    for register in [first, second] {
        assert_eq!(register.client_id, "oe");
        assert_eq!(register.runner_instance_id, "inst-reconnect");
        assert_eq!(
            register.runner_protocol_generation,
            RUNNER_PROTOCOL_GENERATION_V2
        );
        let caps = register.capabilities;
        assert!(caps.shell);
        assert!(caps.file_read);
        assert!(caps.file_write);
        assert!(caps.jobs);
        assert!(caps.async_jobs);
        assert!(caps.async_shell_jobs);
        assert!(caps.git);
    }

    server.await.unwrap();
}

#[tokio::test]
async fn websocket_reconnect_backoff_is_interrupted_by_process_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        ws.send(WsMessage::Close(None)).await.unwrap();
        closed_tx.send(()).unwrap();
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(500));
    let runner_runtime = runtime.clone();
    let runner = tokio::task::spawn_blocking(move || {
        run_websocket_runner(cfg, false, "inst-backoff-shutdown", &runner_runtime)
    });
    closed_rx.await.unwrap();
    let started = Instant::now();
    runtime.request_shutdown_signal();
    tokio::time::timeout(Duration::from_secs(2), runner)
        .await
        .expect("websocket reconnect backoff ignored shutdown")
        .unwrap()
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "websocket reconnect shutdown was delayed"
    );
    assert_eq!(runtime.coordinator.run_count(), 1);
    server.await.unwrap();
}

#[test]
fn quic_connect_or_reconnect_wait_is_interrupted_by_process_shutdown() {
    let mut cfg = test_runner_config("https://localhost".to_string());
    cfg.transport = Some(TRANSPORT_QUIC.to_string());
    cfg.quic = Some(QuicClientConfig {
        server_addr: "127.0.0.1:9".to_string(),
        server_name: "localhost".to_string(),
        alpn: crate::webcodex_runner::default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 20,
    });
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(500));
    let trigger_runtime = runtime.clone();
    let trigger = thread::spawn(move || {
        thread::sleep(Duration::from_millis(75));
        trigger_runtime.request_shutdown_signal();
    });
    let started = Instant::now();
    run_quic_runner(cfg, false, "inst-quic-shutdown", &runtime)
        .expect("QUIC process shutdown should be a normal exit");
    trigger.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "QUIC connect/reconnect wait ignored shutdown"
    );
    assert_eq!(runtime.coordinator.run_count(), 1);
}

#[tokio::test]
async fn websocket_process_shutdown_exits_gracefully() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (registered_tx, registered_rx) = oneshot::channel();
    let (goodbye_tx, goodbye_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        ws.send(WsMessage::Text(
            serde_json::to_string(&RunnerEnvelope::Ping { ts: 1 })
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let pong = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("agent did not enter the registered session")
            .expect("stream open")
            .expect("pong message ok");
        assert!(matches!(
            RunnerEnvelope::from_slice(pong.into_text().unwrap().as_bytes()).unwrap(),
            RunnerEnvelope::Pong { ts: 1 }
        ));
        registered_tx.send(()).unwrap();
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("agent did not send shutdown goodbye")
            .expect("stream open")
            .expect("message ok");
        match RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap() {
            RunnerEnvelope::Goodbye { reason } => goodbye_tx.send(reason).unwrap(),
            other => panic!("expected goodbye, got {}", other.kind()),
        }
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let runtime = test_runtime(&cfg);
    let session_runtime = runtime.clone();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let session = tokio::spawn(async move {
        websocket_session_with_shutdown(
            &cfg,
            vec![test_project("shutdown-test")],
            "inst-shutdown",
            &session_runtime,
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await
    });

    registered_rx.await.unwrap();
    shutdown_tx.send(()).unwrap();
    let exit = tokio::time::timeout(Duration::from_secs(5), session)
        .await
        .expect("shutdown completed")
        .unwrap()
        .expect("session should not error");
    assert_eq!(exit, RunnerSessionExit::Shutdown);
    assert_eq!(
        goodbye_rx.await.unwrap().as_deref(),
        Some("process shutdown")
    );
    server.await.unwrap();
    runtime.shutdown();
    runtime.shutdown();
    assert_eq!(runtime.coordinator.run_count(), 1);
}
