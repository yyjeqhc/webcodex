//! Long-lived stream transport selection and reconnect supervision.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamSupervisorMode {
    Strict(StreamTransport),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamSupervisorExit {
    Completed,
    PollingFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerSessionExit {
    Completed,
    TransportDisconnected,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StreamSessionDecision {
    Complete { shutdown: bool },
    Reconnect(Option<RunnerTransportError>),
    TryNext(RunnerTransportError),
    Fatal(String),
}

pub(super) fn decide_stream_session(
    mode: StreamSupervisorMode,
    transport: StreamTransport,
    once: bool,
    result: Result<RunnerSessionExit, RunnerTransportError>,
) -> StreamSessionDecision {
    match result {
        Ok(RunnerSessionExit::Shutdown) => StreamSessionDecision::Complete { shutdown: true },
        Ok(RunnerSessionExit::Completed) => StreamSessionDecision::Complete { shutdown: false },
        Ok(RunnerSessionExit::TransportDisconnected) if once => {
            StreamSessionDecision::Complete { shutdown: false }
        }
        Ok(RunnerSessionExit::TransportDisconnected) => StreamSessionDecision::Reconnect(None),
        Err(error) if error.is_proxy_configuration() => {
            if matches!(mode, StreamSupervisorMode::Strict(_)) {
                StreamSessionDecision::Fatal(error.into_message())
            } else {
                StreamSessionDecision::TryNext(error)
            }
        }
        Err(error) => {
            if error.is_fatal()
                || matches!(mode, StreamSupervisorMode::Strict(_)) && once
                || mode == StreamSupervisorMode::Auto
                    && transport == StreamTransport::WebSocket
                    && once
            {
                StreamSessionDecision::Fatal(error.into_message())
            } else if matches!(mode, StreamSupervisorMode::Strict(_)) {
                StreamSessionDecision::Reconnect(Some(error))
            } else {
                StreamSessionDecision::TryNext(error)
            }
        }
    }
}

pub(super) fn stream_transport_plan(
    cfg: &RunnerConfig,
    mode: StreamSupervisorMode,
) -> Vec<StreamTransport> {
    match mode {
        StreamSupervisorMode::Strict(transport) => vec![transport],
        StreamSupervisorMode::Auto => auto_transport_plan(cfg)
            .into_iter()
            .filter_map(|transport| match transport {
                TRANSPORT_QUIC => Some(StreamTransport::Quic),
                TRANSPORT_WEBSOCKET => Some(StreamTransport::WebSocket),
                _ => None,
            })
            .collect(),
    }
}

pub(super) async fn run_stream_session(
    transport: StreamTransport,
    cfg: &RunnerConfig,
    projects: Vec<RunnerProjectSummary>,
    runner_instance_id: &str,
    once: bool,
    runtime: &RunnerRuntimeState,
) -> Result<RunnerSessionExit, RunnerTransportError> {
    match transport {
        StreamTransport::WebSocket => {
            websocket_session_classified(cfg, projects, runner_instance_id, runtime).await
        }
        StreamTransport::Quic => quic_session(cfg, projects, runner_instance_id, once, runtime)
            .await
            .map_err(classify_session_error),
    }
}

pub(super) async fn supervise_stream_transports(
    cfg: &RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
    mode: StreamSupervisorMode,
) -> Result<StreamSupervisorExit, String> {
    let mut project_cache = RunnerProjectCache::default();
    let mut backoff = RetryBackoff::new(&RECONNECT_BACKOFF_STEPS);
    'supervisor: loop {
        if mode == StreamSupervisorMode::Auto && cfg.quic.is_none() {
            eprintln!("{}", auto_quic_not_configured_log_line());
        }
        for transport in stream_transport_plan(cfg, mode) {
            if mode == StreamSupervisorMode::Auto {
                eprintln!("{}", auto_trying_log_line(transport.name()));
            }
            let projects = runtime.project_summaries(&mut project_cache, cfg);
            let session_started = Instant::now();
            let result =
                run_stream_session(transport, cfg, projects, runner_instance_id, once, runtime)
                    .await;
            project_cache.invalidate();
            match decide_stream_session(mode, transport, once, result) {
                StreamSessionDecision::Complete { shutdown } => {
                    if shutdown {
                        runtime.shutdown();
                    }
                    return Ok(StreamSupervisorExit::Completed);
                }
                StreamSessionDecision::Reconnect(error) => {
                    if let Some(error) = error {
                        eprintln!(
                            "webcodex-runner {} error: {}; reconnecting",
                            transport.name(),
                            error
                        );
                        tracing::debug!(
                            transport = transport.name(),
                            error = %error,
                            "webcodex-runner stream transport transient error"
                        );
                    } else {
                        reset_backoff_after_stable_session(&mut backoff, session_started);
                        eprintln!(
                            "webcodex-runner {} connection closed; reconnecting",
                            transport.name()
                        );
                    }
                    let delay = schedule_reconnect(transport.name(), &mut backoff);
                    if async_sleep_or_shutdown(delay, runtime).await {
                        runtime.shutdown();
                        return Ok(StreamSupervisorExit::Completed);
                    }
                    continue 'supervisor;
                }
                StreamSessionDecision::TryNext(error) => {
                    let log_error = concise_log_error(&error.to_string(), &cfg.token);
                    match transport {
                        StreamTransport::Quic => eprintln!(
                            "webcodex-runner transport auto: quic unavailable: {}; trying websocket",
                            log_error
                        ),
                        StreamTransport::WebSocket => eprintln!(
                            "webcodex-runner transport auto: websocket failed: {}; falling back to polling",
                            log_error
                        ),
                    }
                    tracing::debug!(
                        transport = transport.name(),
                        error = %log_error,
                        "webcodex-runner auto transport attempt failed"
                    );
                }
                StreamSessionDecision::Fatal(error) => return Err(error),
            }
        }
        debug_assert_eq!(mode, StreamSupervisorMode::Auto);
        eprintln!("{}", auto_trying_log_line(TRANSPORT_POLLING));
        return Ok(StreamSupervisorExit::PollingFallback);
    }
}

pub(super) fn run_stream_transport_runner(
    cfg: &RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
    mode: StreamSupervisorMode,
) -> Result<StreamSupervisorExit, String> {
    let runtime_for_shutdown = runtime.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    let result = rt.block_on(supervise_stream_transports(
        cfg,
        once,
        runner_instance_id,
        runtime,
        mode,
    ));
    rt.shutdown_timeout(runtime_for_shutdown.transport_runtime_shutdown_timeout());
    result
}

pub(super) fn run_auto_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    match run_stream_transport_runner(
        &cfg,
        once,
        runner_instance_id,
        runtime,
        StreamSupervisorMode::Auto,
    )? {
        StreamSupervisorExit::Completed => Ok(()),
        StreamSupervisorExit::PollingFallback => {
            run_polling_runner(cfg, once, runner_instance_id, runtime)
        }
    }
}
