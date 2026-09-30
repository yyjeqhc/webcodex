//! Shared registered-stream reads, graceful close, and session loop.

use super::*;

pub(super) type RunnerWebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(super) enum StreamRead {
    Envelope(RunnerEnvelope),
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamWriterExit {
    ChannelClosed,
    GracefulClose,
    TransportFailed,
}

pub(super) enum RegisteredStream {
    WebSocket {
        reader: futures_util::stream::SplitStream<RunnerWebSocket>,
    },
    Quic {
        reader: quinn::RecvStream,
        connection: quinn::Connection,
        endpoint: quinn::Endpoint,
    },
    #[cfg(test)]
    Test {
        reader: tokio::sync::mpsc::Receiver<StreamRead>,
    },
}

impl RegisteredStream {
    pub(super) async fn receive(&mut self) -> Result<StreamRead, String> {
        use futures_util::StreamExt;

        match self {
            Self::WebSocket { reader, .. } => loop {
                let message = match reader.next().await {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        tracing::debug!(
                            transport = "websocket",
                            error = ?error,
                            "webcodex-runner websocket read error"
                        );
                        return Ok(StreamRead::Closed);
                    }
                    None => {
                        tracing::debug!(
                            transport = "websocket",
                            "webcodex-runner websocket stream ended"
                        );
                        return Ok(StreamRead::Closed);
                    }
                };
                if let tokio_tungstenite::tungstenite::Message::Close(frame) = message {
                    if let Some(frame) = frame {
                        tracing::debug!(
                            transport = "websocket",
                            close_code = ?frame.code,
                            close_reason = %frame.reason,
                            "webcodex-runner websocket close frame received"
                        );
                    } else {
                        tracing::debug!(
                            transport = "websocket",
                            "webcodex-runner websocket close frame received"
                        );
                    }
                    return Ok(StreamRead::Closed);
                }
                let text = match message.into_text() {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                match RunnerEnvelope::from_slice(text.as_bytes()) {
                    Ok(envelope) => return Ok(StreamRead::Envelope(envelope)),
                    Err(error) => {
                        eprintln!("webcodex-runner websocket malformed envelope: {}", error);
                    }
                }
            },
            Self::Quic { reader, .. } => match read_quic_frame(reader).await {
                Ok(envelope) => Ok(StreamRead::Envelope(envelope)),
                Err(QuicFrameError::EmptyStream) => {
                    tracing::debug!(
                        transport = "quic",
                        "webcodex-runner quic stream closed by peer"
                    );
                    Ok(StreamRead::Closed)
                }
                Err(error) => Err(format!("quic stream read error: {}", error)),
            },
            #[cfg(test)]
            Self::Test { reader } => Ok(reader.recv().await.unwrap_or(StreamRead::Closed)),
        }
    }

    pub(super) async fn finish(
        self,
        graceful: bool,
        writer: Option<tokio::task::JoinHandle<StreamWriterExit>>,
    ) {
        use futures_util::StreamExt;

        match self {
            Self::WebSocket { mut reader } => {
                let Some(mut writer) = writer else {
                    return;
                };
                if !graceful {
                    writer.abort();
                    return;
                }
                // Continue polling the read half while the writer flushes
                // Goodbye and the close frame. One absolute deadline bounds
                // both the writer and peer-close observation.
                let close_deadline = tokio::time::Instant::now() + STREAM_WRITER_CLOSE_TIMEOUT;
                let mut reader_open = true;
                let mut writer_finished = false;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(close_deadline) => {
                            writer.abort();
                            break;
                        }
                        _ = &mut writer => {
                            writer_finished = true;
                            break;
                        }
                        message = reader.next(), if reader_open => {
                            if !matches!(message, Some(Ok(_))) {
                                reader_open = false;
                            }
                        }
                    }
                }
                while writer_finished && reader_open {
                    tokio::select! {
                        _ = tokio::time::sleep_until(close_deadline) => break,
                        message = reader.next() => {
                            if !matches!(message, Some(Ok(message)) if !message.is_close()) {
                                reader_open = false;
                            }
                        }
                    }
                }
            }
            Self::Quic {
                connection,
                endpoint,
                ..
            } => {
                // Graceful order is deliberate: serve_registered_stream has
                // already queued Goodbye and dropped all producers. Let the
                // writer drain it and finish the SendStream first. Quinn's
                // SendStream::finish only queues the FIN; Connection::close is
                // immediate and can discard buffered stream data. Use the same
                // absolute close budget to give the peer a chance to close the
                // connection after receiving Goodbye, then force-close if it
                // does not cooperate. Broken transports skip this grace wait.
                let close_started = tokio::time::Instant::now();
                let writer_graceful =
                    finish_quic_writer(writer, graceful, STREAM_WRITER_CLOSE_TIMEOUT).await;
                if graceful && writer_graceful {
                    let remaining =
                        STREAM_WRITER_CLOSE_TIMEOUT.saturating_sub(close_started.elapsed());
                    wait_for_quic_peer_close(
                        async {
                            let _ = connection.closed().await;
                        },
                        remaining,
                    )
                    .await;
                }
                connection.close(quinn::VarInt::from_u32(0), b"process shutdown");
                endpoint.close(quinn::VarInt::from_u32(0), b"process shutdown");
            }
            #[cfg(test)]
            Self::Test { .. } => {
                if let Some(mut writer) = writer {
                    if graceful {
                        let _ =
                            tokio::time::timeout(STREAM_WRITER_CLOSE_TIMEOUT, &mut writer).await;
                    } else {
                        writer.abort();
                    }
                }
            }
        }
    }
}

pub(super) async fn finish_quic_writer(
    writer: Option<tokio::task::JoinHandle<StreamWriterExit>>,
    graceful: bool,
    timeout: Duration,
) -> bool {
    let Some(mut writer) = writer else {
        return false;
    };
    if !graceful {
        writer.abort();
        return false;
    }
    match tokio::time::timeout(timeout, &mut writer).await {
        Ok(Ok(StreamWriterExit::GracefulClose)) => true,
        Ok(_) => false,
        Err(_) => {
            writer.abort();
            false
        }
    }
}

pub(super) async fn wait_for_quic_peer_close<F>(peer_closed: F, timeout: Duration)
where
    F: std::future::Future<Output = ()>,
{
    if timeout.is_zero() {
        return;
    }
    let _ = tokio::time::timeout(timeout, peer_closed).await;
}

pub(super) async fn serve_registered_stream<F>(
    transport: StreamTransport,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered_jobs: &ShellJobInventory,
    out_tx: tokio::sync::mpsc::Sender<RunnerEnvelope>,
    mut stream: RegisteredStream,
    mut writer_task: tokio::task::JoinHandle<StreamWriterExit>,
    project_inventory_sync: Option<ProjectInventorySync>,
    runtime: &RunnerRuntimeState,
    shutdown: F,
) -> Result<RunnerSessionExit, String>
where
    F: std::future::Future<Output = ()>,
{
    let sink = match transport {
        StreamTransport::WebSocket => RunnerSink::WebSocket {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
        },
        StreamTransport::Quic => RunnerSink::Quic {
            tx: out_tx.clone(),
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
        },
    };
    let jobs = runtime.jobs.clone();
    jobs.install_sink(sink.clone());
    jobs.replay_snapshots_since(registered_jobs);
    let mut ping_interval = tokio::time::interval(transport.ping_interval());
    ping_interval.tick().await;
    let mut project_inventory = StreamingProjectInventoryCoordinator::new(project_inventory_sync);
    let (project_inventory_refresh_tx, mut project_inventory_refresh_rx) =
        tokio::sync::mpsc::channel::<()>(1);
    let mut shutdown = Box::pin(shutdown);
    let mut shutdown_requested = false;
    let mut session_error = None;
    let mut writer_observed = false;

    loop {
        let project_inventory_retry_at = project_inventory.retry_at();
        let project_inventory_retry_deadline =
            project_inventory_retry_at.unwrap_or_else(tokio::time::Instant::now);
        tokio::select! {
            _ = tokio::time::sleep_until(project_inventory_retry_deadline), if project_inventory_retry_at.is_some() => {
                project_inventory.retry_pending_now(transport, &out_tx);
            }
            refresh = project_inventory_refresh_rx.recv() => {
                if refresh.is_some() {
                    project_inventory.refresh_from_current_projects(
                        transport,
                        cfg,
                        runtime,
                        &out_tx,
                        "project_inventory_local_project_mutation",
                    );
                }
            }
            _ = &mut shutdown => {
                runtime.request_shutdown_signal();
                shutdown_requested = true;
                break;
            }
            writer = &mut writer_task => {
                writer_observed = true;
                let reason_code = match writer {
                    Ok(StreamWriterExit::ChannelClosed) => "writer_channel_closed",
                    Ok(StreamWriterExit::GracefulClose) => "writer_graceful_close_unexpected",
                    Ok(StreamWriterExit::TransportFailed) => "writer_transport_failed",
                    Err(error) if error.is_panic() => "writer_task_panicked",
                    Err(_) => "writer_task_cancelled",
                };
                tracing::debug!(
                    transport = transport.name(),
                    reason_code,
                    "webcodex-runner stream writer ended; terminating session"
                );
                break;
            }
            read = stream.receive() => {
                match read {
                    Ok(StreamRead::Envelope(envelope)) => {
                        if let Some(error) = handle_stream_envelope(
                            transport,
                            envelope,
                            cfg,
                            &sink,
                            &out_tx,
                            &mut project_inventory,
                            &project_inventory_refresh_tx,
                            runtime,
                        ) {
                            session_error = Some(error);
                            break;
                        }
                    }
                    Ok(StreamRead::Closed) => break,
                    Err(error) => {
                        session_error = Some(error);
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                tracing::debug!(
                    transport = transport.name(),
                    "webcodex-runner stream keepalive ping"
                );
                send_provider_metadata(transport, &out_tx, &runtime.config, None);
                // An acknowledgement can be dropped if the Server's outbound
                // channel is saturated. Re-sending the exact pending page is
                // idempotent and gives the sync a bounded periodic recovery path.
                // When the Server explicitly reported staging pressure, the
                // dedicated backoff timer owns retry timing so keepalive cannot
                // collapse that bounded delay into an eager resend.
                if project_inventory.retry_at().is_none() {
                    project_inventory.queue_pending(transport, &out_tx);
                }
                let _ = try_send_runner_stream_control(
                    transport,
                    &out_tx,
                    RunnerEnvelope::Ping {
                        ts: chrono::Utc::now().timestamp(),
                    },
                );
            }
        }
    }

    if shutdown_requested {
        let queue_started = Instant::now();
        match tokio::time::timeout(
            TRANSPORT_CONTROL_SEND_TIMEOUT,
            out_tx.send(RunnerEnvelope::Goodbye {
                reason: Some("process shutdown".to_string()),
            }),
        )
        .await
        {
            Ok(Ok(())) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                Some(queue_started.elapsed()),
                false,
                RunnerStreamMetricOutcome::Success,
            ),
            Ok(Err(_)) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                None,
                false,
                RunnerStreamMetricOutcome::Closed,
            ),
            Err(_) => observe_runner_stream_outgoing_channel(
                transport,
                "goodbye",
                None,
                false,
                RunnerStreamMetricOutcome::Timeout,
            ),
        }
    } else if jobs.has_work() {
        tracing::warn!(
            transport = transport.name(),
            "webcodex-runner stream disconnected with active jobs; reconnecting without waiting"
        );
    }
    if !shutdown_requested && transport == StreamTransport::Quic {
        runtime
            .persistent_shells
            .close_all("runner_transport_disconnected");
    }
    drop(sink);
    drop(out_tx);
    let writer = (!writer_observed).then_some(writer_task);
    stream.finish(shutdown_requested, writer).await;
    if !shutdown_requested && transport == StreamTransport::WebSocket {
        runtime
            .persistent_shells
            .close_all("runner_transport_disconnected");
    }
    if !shutdown_requested {
        observe_runner_stream_disconnect(transport);
    }
    if let Some(error) = session_error {
        return Err(error);
    }
    Ok(if shutdown_requested {
        RunnerSessionExit::Shutdown
    } else {
        RunnerSessionExit::TransportDisconnected
    })
}
