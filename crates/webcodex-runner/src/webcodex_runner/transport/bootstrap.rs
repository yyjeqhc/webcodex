//! Runner startup and transport selection.

use super::*;

pub(super) fn send_provider_metadata(
    transport: StreamTransport,
    tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    runtime: &ReloadableRunnerConfig,
    expected_generation: Option<u64>,
) {
    runtime.with_active(|config| {
        if expected_generation.is_some_and(|expected| expected != config.generation) {
            // An accepted result may belong to the generation that performed a
            // successful config reload. Treat that result only as a delivery
            // trigger: metadata is always read from the current active
            // generation below, so stale work can never publish stale routing.
            tracing::debug!(
                expected_generation,
                active_generation = config.generation,
                "publishing current Runner metadata after config generation changed"
            );
        }
        let provider_update = config.external_tools.claim_status_update();
        let availability_update = crate::webcodex_runner::computer_session::changed_availability();
        if provider_update.is_none() && availability_update.is_none() {
            return;
        }
        let had_provider_update = provider_update.is_some();
        let (mut status, revision) =
            provider_update.unwrap_or_else(|| config.external_tools.registration_status());
        status.config_reload = config.reload_status();
        if try_send_runner_stream_control(
            transport,
            tx,
            RunnerEnvelope::RuntimeMetadata {
                tool_providers: status,
                mcp_gateway_providers: Some(runtime.mcp_gateway().provider_inventory()),
                computer_session_availability: availability_update,
            },
        ) {
            if had_provider_update {
                config.external_tools.mark_status_reported(revision);
            }
            if let Some(available) = availability_update {
                crate::webcodex_runner::computer_session::mark_availability_reported(available);
            }
        } else {
            if had_provider_update {
                config.external_tools.release_status_update(revision);
            }
        }
    });
}

pub(crate) fn non_empty_token(token: &str) -> Option<String> {
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(crate) fn run_runner(
    cfg: RunnerConfig,
    config_path: PathBuf,
    once: bool,
    stop_on_stdin_eof: bool,
    computer_session_dir: Option<PathBuf>,
    #[cfg(windows)] service_stop: Option<webcodex_environment::service::runtime::ServiceStop>,
) -> Result<(), String> {
    // Generate the per-process agent instance identity once. It is stable for
    // the whole process lifetime, including across WebSocket reconnects, so the
    // server can treat this process as a single active lease for `client_id`.
    // It is not a secret and is never persisted to disk. Windows exit diagnostics
    // use a separate local diagnostic id and therefore preserve that boundary.
    let runner_instance_id = uuid::Uuid::new_v4().to_string();
    if let Some(dir) = computer_session_dir {
        crate::webcodex_runner::computer_session::configure(dir, runner_instance_id.clone())?;
    }
    let transport = cfg
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
        .to_string();
    #[cfg(windows)]
    let exit_diagnostics = {
        let build = crate::runner_build_info();
        match RunnerExitDiagnostics::start(
            &cfg.client_id,
            &cfg.server_url,
            &transport,
            crate::process_started_at(),
            build.version.as_deref(),
            build.git_commit.as_deref(),
            build.git_dirty,
        ) {
            Ok(diagnostics) => {
                diagnostics.install_panic_hook();
                Some(diagnostics)
            }
            Err(error) => {
                tracing::warn!(error = %error, "Windows Runner exit diagnostics unavailable; Runner continues");
                None
            }
        }
    };
    // The LSP supervisor belongs to the Runner process rather than any server
    // transport session and is shared across reconnects.
    let runtime = RunnerRuntimeState::new(&cfg, config_path.clone());
    #[cfg(windows)]
    let runtime = {
        let mut runtime = runtime;
        runtime.exit_diagnostics = exit_diagnostics.clone();
        runtime
    };
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    match DetachedJobStore::default_root_for_runner(&cfg.client_id, &cfg.server_url) {
        Ok(root) => match runtime.jobs.recover_detached_jobs(
            DetachedJobStore::new(root),
            &cfg.client_id,
            &runner_instance_id,
        ) {
            Ok(count) if count > 0 => {
                tracing::info!(count, "recovered detached Jobs before Runner registration");
            }
            Ok(_) => {}
            Err(error) => {
                // Recovery is fail-closed for the detached records but must not
                // brick ordinary Runner service. Omitting an untrusted/corrupt
                // detached record lets normal Server reconciliation mark it lost.
                tracing::error!(error = %error, "detached Job restart recovery failed closed");
            }
        },
        Err(error) => {
            tracing::error!(error = %error, "detached Job state root is unavailable");
        }
    }
    if stop_on_stdin_eof {
        if let Err(error) = install_parent_liveness_listener(runtime.clone()) {
            #[cfg(windows)]
            if let Some(diagnostics) = exit_diagnostics.as_ref() {
                diagnostics.mark_terminal(false, "parent_liveness_listener_install_failed", None);
            }
            return Err(error);
        }
    }
    let shutdown_listener = match install_shutdown_listener(runtime.clone()) {
        Ok(listener) => listener,
        Err(error) => {
            #[cfg(windows)]
            if let Some(diagnostics) = exit_diagnostics.as_ref() {
                diagnostics.mark_terminal(false, "shutdown_listener_install_failed", None);
            }
            return Err(error);
        }
    };
    runtime.register_background_thread(shutdown_listener);
    #[cfg(windows)]
    if let Some(stop) = service_stop {
        match install_service_stop_listener(runtime.clone(), stop) {
            Ok(listener) => runtime.register_background_thread(listener),
            Err(error) => {
                runtime.shutdown();
                return Err(error);
            }
        }
    }
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_running();
    }
    #[cfg(unix)]
    match install_reload_listener(Arc::clone(&runtime.config)) {
        Ok(reload_listener) => runtime.register_reload_thread(reload_listener),
        Err(error) => {
            runtime.shutdown();
            return Err(error);
        }
    }
    let result = match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_runner(cfg, once, &runner_instance_id, &runtime),
        TRANSPORT_QUIC => run_quic_runner(cfg, once, &runner_instance_id, &runtime),
        TRANSPORT_AUTO => run_auto_runner(cfg, once, &runner_instance_id, &runtime),
        _ => run_polling_runner(cfg, once, &runner_instance_id, &runtime),
    };
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_transport_returned(result.is_ok());
    }
    let _shutdown = runtime.shutdown();
    #[cfg(windows)]
    if let Some(diagnostics) = exit_diagnostics.as_ref() {
        diagnostics.mark_terminal(
            result.is_ok(),
            if result.is_ok() {
                "transport_completed"
            } else {
                "transport_returned_error"
            },
            Some(&_shutdown),
        );
    }
    result
}

pub(crate) fn effective_transport(cfg: &RunnerConfig) -> &str {
    cfg.transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
}

pub(crate) fn auto_transport_plan(cfg: &RunnerConfig) -> Vec<&'static str> {
    let mut plan = Vec::new();
    if cfg.quic.is_some() {
        plan.push(TRANSPORT_QUIC);
    }
    plan.push(TRANSPORT_WEBSOCKET);
    plan.push(TRANSPORT_POLLING);
    plan
}
