//! WebSocket session handshake and stream writer.

use super::*;

// ============================================================================
// WebSocket agent transport
// ============================================================================
//
// The WebSocket mode keeps one long-lived connection to the server. The server
// pushes `Request` envelopes; the Runner executes them via the same
// `dispatch_request` path the polling loop uses, and sends `Result` /
// `JobUpdate` envelopes back. Polling is unchanged and remains the fallback.

pub(super) fn run_websocket_runner(
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
        StreamSupervisorMode::Strict(StreamTransport::WebSocket),
    )
    .map(|_| ())
}

/// One WebSocket connection lifecycle: connect, register, then serve requests
/// until the socket closes or a fatal server error arrives.
#[cfg(test)]
pub(crate) async fn websocket_session(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, String> {
    websocket_session_classified(cfg, projects, runner_instance_id, runtime)
        .await
        .map_err(RunnerTransportError::into_message)
}

pub(super) async fn websocket_session_classified(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, RunnerTransportError> {
    websocket_session_with_shutdown(
        cfg,
        projects,
        runner_instance_id,
        runtime,
        runtime.wait_for_shutdown(),
    )
    .await
}

pub(super) async fn websocket_session_with_shutdown<F>(
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
    shutdown: F,
) -> Result<RunnerSessionExit, RunnerTransportError>
where
    F: std::future::Future<Output = ()>,
{
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let mut shutdown = Box::pin(shutdown);
    let ws_url = server_url_to_ws(&cfg.server_url, "/api/agents/ws")?;
    let request = build_ws_request(&ws_url, &cfg.token)?;
    let connect = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(cfg.websocket_connect_timeout_secs),
            connect_websocket_request(request, &ws_url, &cfg.token),
        ) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(RunnerSessionExit::Shutdown);
        }
    };
    let mut ws_stream = connect.map_err(|_| {
        format!(
            "websocket connect timed out after {}s",
            cfg.websocket_connect_timeout_secs
        )
    })??;

    // Register over the socket. The prepared-profile cache is empty at
    // registration time (snapshots are prepared lazily on first use), so
    // `prepared_cache_count` is reported as 0 here.
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
    let reg_env = RunnerEnvelope::Register {
        payload: register_payload,
    };
    let reg_json =
        serde_json::to_string(&reg_env).map_err(|e| format!("failed to encode register: {}", e))?;
    tokio::select! {
        result = ws_stream.send(WsMessage::Text(reg_json.into())) => result,
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(RunnerSessionExit::Shutdown);
        }
    }
    .map_err(|e| format!("failed to send register: {}", e))?;

    // Wait for Registered ack.
    let ack_msg = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(10), ws_stream.next()) => {
            result.map_err(|_| "websocket register ack timed out".to_string())?
        }
        _ = &mut shutdown => {
            runtime.request_shutdown_signal();
            return Ok(RunnerSessionExit::Shutdown);
        }
    }
    .ok_or_else(|| "server closed before register ack".to_string())?
    .map_err(|e| format!("failed to read register ack: {}", e))?;
    let ack_text = ack_msg
        .into_text()
        .map_err(|_| "register ack was not text".to_string())?;
    let ack = RunnerEnvelope::from_slice(ack_text.as_bytes())
        .map_err(|e| format!("register ack is not a valid envelope: {}", e))?;
    let _inventory_status = registered_ack(ack)?;
    let mut project_inventory_sync = Some(paged_sync_after_registration(projects));
    provider.mark_status_reported(provider_revision);
    eprintln!(
        "{}",
        registered_log_line(cfg, TRANSPORT_WEBSOCKET, projects_count)
    );

    // Split socket into writer (drains outgoing envelopes) and reader.
    let (mut sink, stream) = ws_stream.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RunnerEnvelope>(WS_OUTGOING_CAPACITY);
    try_queue_project_inventory_page(
        StreamTransport::WebSocket,
        &mut project_inventory_sync,
        &out_tx,
    );
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            let envelope_kind = env.kind();
            let is_goodbye = matches!(env, RunnerEnvelope::Goodbye { .. });
            let send_started = Instant::now();
            let Ok(json) = serde_json::to_string(&env) else {
                observe_runner_stream_writer_send(
                    StreamTransport::WebSocket,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            };
            if sink.send(WsMessage::Text(json.into())).await.is_err() {
                observe_runner_stream_writer_send(
                    StreamTransport::WebSocket,
                    envelope_kind,
                    None,
                    RunnerStreamMetricOutcome::TransportError,
                );
                return StreamWriterExit::TransportFailed;
            }
            observe_runner_stream_writer_send(
                StreamTransport::WebSocket,
                envelope_kind,
                Some(send_started.elapsed()),
                RunnerStreamMetricOutcome::Success,
            );
            if is_goodbye {
                // The session loop continues polling the split read half while
                // awaiting this task, allowing tungstenite's close handshake to
                // progress without turning this into an unbounded wait.
                return if sink.close().await.is_ok() {
                    StreamWriterExit::GracefulClose
                } else {
                    StreamWriterExit::TransportFailed
                };
            }
        }
        StreamWriterExit::ChannelClosed
    });
    serve_registered_stream(
        StreamTransport::WebSocket,
        cfg,
        runner_instance_id,
        &registered_jobs,
        out_tx,
        RegisteredStream::WebSocket { reader: stream },
        writer_task,
        project_inventory_sync,
        runtime,
        shutdown,
    )
    .await
    .map_err(classify_session_error)
}
