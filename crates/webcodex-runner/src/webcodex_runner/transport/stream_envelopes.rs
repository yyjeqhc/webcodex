//! Registration acknowledgements and incoming envelope dispatch.

use super::*;

pub(super) fn registered_ack(ack: RunnerEnvelope) -> Result<ShellProjectInventoryStatus, String> {
    match ack {
        RunnerEnvelope::Registered {
            success: true,
            client,
            ..
        } => {
            crate::webcodex_runner::computer_session::set_server_availability_contract(
                crate::webcodex_runner::computer_session::registration_echo_confirms_contract(
                    client
                        .as_ref()
                        .and_then(|view| view.computer_session_availability),
                ),
            );
            client
                .and_then(|client| client.project_inventory)
                .ok_or_else(|| "register acknowledgement missing canonical project_inventory status; Server is incompatible with this 0.4 Runner".to_string())
        }
        RunnerEnvelope::Registered { error, .. } => Err(format!(
            "register rejected by server: {}",
            error.unwrap_or_else(|| "no server error message".to_string())
        )),
        RunnerEnvelope::Error { code, message } => Err(format!(
            "server error during register {}: {}",
            code, message
        )),
        other => Err(format!("expected registered ack, got {}", other.kind())),
    }
}

pub(super) fn handle_stream_envelope(
    transport: StreamTransport,
    envelope: RunnerEnvelope,
    cfg: &RunnerConfig,
    sink: &RunnerSink,
    out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    project_inventory: &mut StreamingProjectInventoryCoordinator,
    runtime: &RunnerRuntimeState,
) -> Option<String> {
    let envelope_kind = envelope.kind();
    observe_runner_stream_incoming_envelope(transport, envelope_kind);
    match envelope {
        RunnerEnvelope::Request { request } => {
            let received_at = Instant::now();
            let sink = sink.clone();
            let config = Arc::clone(&runtime.config);
            let hot = config.snapshot();
            let jobs = runtime.jobs.clone();
            let persistent_shells = runtime.persistent_shells.clone();
            let project_registry_dir = match project_registry_dir(cfg) {
                Ok(dir) => dir,
                Err(error) => return Some(error),
            };
            let lsp = runtime.lsp.clone();
            let browser = runtime.browser.clone();
            let dispatch_guard = runtime.dispatches.enter();
            let runtime = runtime.clone();
            let project_operation =
                webcodex_core::runner_operation::RunnerProjectOperationKind::from_wire(
                    &request.kind,
                )
                .is_some();
            tokio::task::spawn_blocking(move || {
                let _dispatch_guard = dispatch_guard;
                observe_runner_stream_request_dispatch_wait(transport, received_at.elapsed());
                let dispatch_result = dispatch_request_with_outcome(
                    &sink,
                    &hot,
                    &config,
                    &jobs,
                    &persistent_shells,
                    &project_registry_dir,
                    &lsp,
                    &browser,
                    request,
                );
                let refresh_inventory = match dispatch_result {
                    Ok(outcome) => outcome.project_cache_invalidation_required,
                    // The mutation can finish even when its result cannot reach
                    // the old stream. Only reobserve the catalog; never replay it.
                    Err(_) => project_operation,
                };
                if refresh_inventory {
                    runtime.invalidate_project_inventory();
                }
            });
            None
        }
        RunnerEnvelope::Ping { ts } => {
            let _ = try_send_runner_stream_control(transport, out_tx, RunnerEnvelope::Pong { ts });
            None
        }
        RunnerEnvelope::Pong { .. } => None,
        RunnerEnvelope::ProjectInventoryStatus { status } => {
            project_inventory.handle_status(transport, status, cfg, runtime, out_tx);
            None
        }
        RunnerEnvelope::Registered { .. } if transport == StreamTransport::Quic => None,
        RunnerEnvelope::Error { code, message } => {
            Some(format!("server error {}: {}", code, message))
        }
        other => {
            eprintln!(
                "webcodex-runner {} ignoring unexpected envelope: {}",
                transport.name(),
                other.kind()
            );
            None
        }
    }
}
