//! QUIC configuration, handshake, and stream writer.

use super::*;

// The custom QUIC transport is a QUIC stream, not HTTP/3. It intentionally
// keeps one serialized bidirectional stream today so a future multistream
// implementation can change this adapter without changing the supervisor.
pub(super) fn run_quic_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    run_stream_transport_runner(
        &cfg,
        once,
        runner_instance_id,
        runtime,
        StreamSupervisorMode::Strict(StreamTransport::Quic),
    )
    .map(|_| ())
}

/// Validate the `[quic]` config section. Returns a cloned, resolved config so
/// the session owns a concrete value (defaults applied).
pub(crate) fn resolve_quic_config(cfg: &RunnerConfig) -> Result<QuicClientConfig, String> {
    let quic = cfg.quic.clone().ok_or_else(|| {
        "transport=quic requires a [quic] section in the Runner config".to_string()
    })?;
    validate_quic_config(&quic)?;
    Ok(quic)
}

pub(crate) fn resolve_quic_server_addrs(server_addr: &str) -> Result<Vec<SocketAddr>, String> {
    let addrs = server_addr
        .to_socket_addrs()
        .map_err(|e| {
            format!(
                "failed to resolve [quic] server_addr '{}': {}",
                server_addr, e
            )
        })?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!(
            "[quic] server_addr '{}' resolved to no socket addresses",
            server_addr
        ));
    }
    Ok(addrs)
}

pub(crate) fn quic_client_bind_addr_for(server_addr: SocketAddr) -> SocketAddr {
    if server_addr.is_ipv6() {
        "[::]:0"
            .parse()
            .expect("hard-coded IPv6 client bind address is valid")
    } else {
        "0.0.0.0:0"
            .parse()
            .expect("hard-coded IPv4 client bind address is valid")
    }
}

/// The rustls crypto provider for the QUIC client. The dependency tree pulls
/// both `aws-lc-rs` and `ring`, so rustls cannot auto-select; pin aws-lc-rs
/// explicitly per config via `builder_with_provider` (thread-safe, no global
/// install).
pub(super) fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Build the quinn-wrapped rustls client config for the QUIC transport. The
/// agent validates the server certificate against the Mozilla root store
/// (webpki-roots) using `server_name` as the SNI/verification name — TLS is
/// transport security, not authentication; the agent token still authenticates
/// the agent.
pub(super) fn build_quic_client_crypto(
    quic: &QuicClientConfig,
) -> Result<quinn::crypto::rustls::QuicClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    // `RootCertStore` implements `Extend<TrustAnchor>` (in-place, infallible).
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut client_crypto = rustls::ClientConfig::builder_with_provider(rustls_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("failed to select rustls protocol versions: {}", e))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![quic.alpn.as_bytes().to_vec()];
    quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
        .map_err(|e| format!("failed to build quinn client crypto: {}", e))
}

pub(super) fn build_quic_transport_config(
    quic: &QuicClientConfig,
) -> Result<quinn::TransportConfig, String> {
    let idle_timeout: quinn::IdleTimeout = QUIC_IDLE_TIMEOUT
        .try_into()
        .map_err(|_| "failed to encode QUIC idle timeout".to_string())?;
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(idle_timeout));
    transport.keep_alive_interval(Some(Duration::from_secs(quic.keepalive_interval_secs)));
    Ok(transport)
}

pub(super) fn classify_quic_runner_connect_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("certificate")
        || lower.contains("cert")
        || lower.contains("webpki")
        || lower.contains("notvalidforname")
        || lower.contains("unknownissuer")
    {
        "certificate verify failed; check [quic].server_name and the certificate SAN/issuer"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connect timeout; check UDP firewall/security group/NAT and that the server QUIC listener is enabled"
    } else if lower.contains("alpn") || lower.contains("no application protocol") {
        "handshake failed; check WEBCODEX_QUIC_ENABLED, listener bind, and ALPN"
    } else if lower.contains("applicationclosed")
        || lower.contains("connectionclosed")
        || lower.contains("closed")
    {
        "handshake failed; check WEBCODEX_QUIC_ENABLED, listener bind, and server availability"
    } else {
        "handshake failed"
    }
}

/// One QUIC connection lifecycle: connect, register, dispatch requests until
/// the stream closes or a fatal server error arrives. In `--once` mode,
/// completes one ping/pong after the ack then returns.
pub(super) async fn quic_session(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    once: bool,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, String> {
    let quic = resolve_quic_config(cfg)?;
    let client_crypto = build_quic_client_crypto(&quic)?;
    let mut client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    client_config.transport_config(Arc::new(build_quic_transport_config(&quic)?));
    let server_addrs = resolve_quic_server_addrs(&quic.server_addr)?;
    let mut connect_errors = Vec::new();
    let mut client_endpoint = None;
    let mut conn = None;
    for server_addr in server_addrs {
        if runtime.shutdown_requested() {
            return Ok(RunnerSessionExit::Shutdown);
        }
        let endpoint = match quinn::Endpoint::client(quic_client_bind_addr_for(server_addr)) {
            Ok(endpoint) => endpoint,
            Err(e) => {
                connect_errors.push(format!(
                    "{}: failed to bind quic client endpoint: {}",
                    server_addr, e
                ));
                continue;
            }
        };
        let connect =
            match endpoint.connect_with(client_config.clone(), server_addr, &quic.server_name) {
                Ok(connect) => connect,
                Err(e) => {
                    connect_errors.push(format!(
                        "{}: failed to start quic connect: {}",
                        server_addr, e
                    ));
                    continue;
                }
            };
        let Some(connect_result) = future_or_shutdown(
            tokio::time::timeout(Duration::from_secs(quic.connect_timeout_secs), connect),
            runtime,
        )
        .await
        else {
            return Ok(RunnerSessionExit::Shutdown);
        };
        match connect_result {
            Ok(Ok(connection)) => {
                client_endpoint = Some(endpoint);
                conn = Some(connection);
                break;
            }
            Err(_) => connect_errors.push(format!(
                "{} timed out after {}s; check UDP firewall/security group/NAT and that the server QUIC listener is enabled",
                server_addr, quic.connect_timeout_secs
            )),
            Ok(Err(e)) => {
                let raw = e.to_string();
                connect_errors.push(format!(
                    "{}: {} ({})",
                    server_addr,
                    classify_quic_runner_connect_error(&raw),
                    raw
                ));
            }
        }
    }
    let client_endpoint = client_endpoint.ok_or_else(|| {
        format!(
            "quic connect to {} failed for all resolved addresses: {}",
            quic.server_addr,
            connect_errors.join("; ")
        )
    })?;
    let conn = conn.expect("client endpoint is set only after a successful QUIC connection");

    // ALPN is enforced by quinn during the TLS handshake: a connection only
    // completes when the client and server agree on a matching ALPN. A
    // mismatch fails the handshake (surfaced as the connect error above).

    // Open a single bidirectional stream for register/ack/keepalive.
    let Some(open_result) = future_or_shutdown(conn.open_bi(), runtime).await else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(RunnerSessionExit::Shutdown);
    };
    let (mut send, mut recv) =
        open_result.map_err(|e| format!("failed to open quic bidirectional stream: {}", e))?;

    // Credential ownership stays outside the transport-neutral registration payload.
    // The token is never logged.
    let projects_count = enabled_projects_count(&projects);
    let registered_jobs = runtime.jobs.inventory();
    let (register_payload, provider, provider_revision) =
        build_register_request_with_provider_status(
            cfg,
            &runtime.config,
            runner_instance_id,
            0,
            registered_jobs.clone(),
        );
    let register_frame = QuicRegisterFrame::new(register_payload, non_empty_token(&cfg.token));
    let Some(register_write) = future_or_shutdown(
        write_quic_register_frame(&mut send, &register_frame),
        runtime,
    )
    .await
    else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(RunnerSessionExit::Shutdown);
    };
    register_write.map_err(|e| format!("failed to send quic register: {}", e))?;

    // Wait for the Registered ack.
    let Some(ack_result) = future_or_shutdown(
        tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv)),
        runtime,
    )
    .await
    else {
        conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
        return Ok(RunnerSessionExit::Shutdown);
    };
    let ack = ack_result
        .map_err(|_| "quic register ack timed out".to_string())?
        .map_err(|e| format!("failed to read quic register ack: {}", e))?;
    let _inventory_status = registered_ack(ack)?;
    let mut project_inventory_sync = Some(paged_sync_after_registration(projects));
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_QUIC, projects_count)
    );

    if once {
        // Complete one ping/pong round trip then exit, mirroring the websocket
        // `--once` semantics.
        let ping = RunnerEnvelope::Ping {
            ts: chrono::Utc::now().timestamp(),
        };
        let Some(ping_write) =
            future_or_shutdown(write_quic_frame(&mut send, &ping), runtime).await
        else {
            conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
            client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            return Ok(RunnerSessionExit::Shutdown);
        };
        ping_write.map_err(|e| format!("quic once ping send failed: {}", e))?;
        let Some(pong_result) = future_or_shutdown(
            tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv)),
            runtime,
        )
        .await
        else {
            conn.close(quinn::VarInt::from_u32(0), b"process shutdown");
            client_endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            return Ok(RunnerSessionExit::Shutdown);
        };
        let resp = pong_result
            .map_err(|_| "quic once pong timed out".to_string())?
            .map_err(|e| format!("quic once pong read failed: {}", e))?;
        match resp {
            RunnerEnvelope::Pong { .. } => {}
            other => return Err(format!("expected pong, got {}", other.kind())),
        }
        let goodbye = RunnerEnvelope::Goodbye {
            reason: Some("once complete".to_string()),
        };
        let close_started = tokio::time::Instant::now();
        let goodbye_result = tokio::time::timeout(
            STREAM_WRITER_CLOSE_TIMEOUT,
            write_quic_frame(&mut send, &goodbye),
        )
        .await;
        let goodbye_sent = matches!(&goodbye_result, Ok(Ok(())));
        let finish_result = send.finish();
        if goodbye_sent && finish_result.is_ok() {
            let remaining = STREAM_WRITER_CLOSE_TIMEOUT.saturating_sub(close_started.elapsed());
            wait_for_quic_peer_close(
                async {
                    let _ = conn.closed().await;
                },
                remaining,
            )
            .await;
        }
        conn.close(quinn::VarInt::from_u32(0), b"once complete");
        client_endpoint.close(quinn::VarInt::from_u32(0), b"once complete");
        goodbye_result
            .map_err(|_| "quic once goodbye flush timed out".to_string())?
            .map_err(|e| format!("quic once goodbye send failed: {e}"))?;
        finish_result.map_err(|e| format!("quic once send finish failed: {e}"))?;
        return Ok(RunnerSessionExit::Completed);
    }

    // Outgoing envelopes share one writer so future QUIC multistream work can
    // change the transport adapter without duplicating the session lifecycle.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
    try_queue_project_inventory_page(StreamTransport::Quic, &mut project_inventory_sync, &out_tx);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            let envelope_kind = env.kind();
            let graceful = matches!(env, RunnerEnvelope::Goodbye { .. });
            let send_started = Instant::now();
            if write_quic_frame(&mut send, &env).await.is_err() {
                observe_runner_stream_writer_send(
                    StreamTransport::Quic,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            }
            observe_runner_stream_writer_send(
                StreamTransport::Quic,
                envelope_kind,
                Some(send_started.elapsed()),
                RunnerStreamMetricOutcome::Success,
            );
            if graceful {
                return if send.finish().is_ok() {
                    StreamWriterExit::GracefulClose
                } else {
                    StreamWriterExit::TransportFailed
                };
            }
        }
        if send.finish().is_ok() {
            StreamWriterExit::ChannelClosed
        } else {
            StreamWriterExit::TransportFailed
        }
    });
    serve_registered_stream(
        StreamTransport::Quic,
        cfg,
        runner_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::Quic {
            reader: recv,
            connection: conn,
            endpoint: client_endpoint,
        },
        writer_task,
        project_inventory_sync,
        runtime,
        runtime.wait_for_shutdown(),
    )
    .await
}
